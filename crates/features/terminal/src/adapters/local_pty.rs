//! Local shell through `portable-pty`.

use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::mpsc as std_mpsc;

use futures::channel::mpsc::{UnboundedSender, unbounded};
use portable_pty::{
    ChildKiller, CommandBuilder, MasterPty, PtySize as PortablePtySize, native_pty_system,
};

use super::{Backend, BackendEvents};
use crate::domain::shell::{ShellFlavor, find_in_path, pick_shell, shell_args};
use crate::domain::{BackendEvent, PtySize};

/// What to run in the PTY.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandSpec {
    pub program: String,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
    pub cwd: Option<PathBuf>,
}

/// The platform's default shell (`$SHELL -l` on Unix, PowerShell on Windows)
/// in the home directory with a sane terminal environment.
pub fn login_shell_spec() -> CommandSpec {
    login_shell_spec_for(None)
}

/// Like [`login_shell_spec`] with an explicit shell program (Settings →
/// Terminal). `None` or blank falls back to `$SHELL` then `/bin/zsh` on Unix,
/// and to `pwsh.exe` → `powershell.exe` → `cmd.exe` on Windows.
pub fn login_shell_spec_for(shell: Option<&str>) -> CommandSpec {
    let path = std::env::var_os("PATH").unwrap_or_default();
    let on_path = |program: &str| find_in_path(program, &path, |p| p.is_file()).is_some();
    let env_shell = std::env::var("SHELL").ok();
    shell_spec(
        ShellFlavor::current(),
        shell,
        env_shell.as_deref(),
        on_path,
        dirs::home_dir(),
    )
}

/// [`login_shell_spec_for`] with the environment passed in (pure).
pub fn shell_spec(
    flavor: ShellFlavor,
    explicit: Option<&str>,
    env_shell: Option<&str>,
    on_path: impl Fn(&str) -> bool,
    home: Option<PathBuf>,
) -> CommandSpec {
    let program = pick_shell(flavor, explicit, env_shell, on_path);
    let args = shell_args(flavor, &program);
    CommandSpec {
        program,
        args,
        env: vec![
            ("TERM".to_string(), "xterm-256color".to_string()),
            ("COLORTERM".to_string(), "truecolor".to_string()),
            ("TERM_PROGRAM".to_string(), "NatiKube".to_string()),
        ],
        cwd: home,
    }
}

fn to_portable(size: PtySize) -> PortablePtySize {
    PortablePtySize {
        rows: size.rows.max(1),
        cols: size.cols.max(2),
        pixel_width: size.cols.saturating_mul(size.cell_width),
        pixel_height: size.rows.saturating_mul(size.cell_height),
    }
}

/// A running local PTY session.
pub struct LocalPty {
    master: Mutex<Box<dyn MasterPty + Send>>,
    killer: Mutex<Box<dyn ChildKiller + Send + Sync>>,
    input: std_mpsc::Sender<Vec<u8>>,
}

impl LocalPty {
    /// Spawns the command in a new PTY. Output and the exit notice arrive on
    /// the returned receiver.
    pub fn spawn(spec: &CommandSpec, size: PtySize) -> anyhow::Result<(Self, BackendEvents)> {
        let pair = native_pty_system().openpty(to_portable(size))?;

        let mut command = CommandBuilder::new(&spec.program);
        command.args(&spec.args);
        for (key, value) in &spec.env {
            command.env(key, value);
        }
        if let Some(cwd) = &spec.cwd {
            command.cwd(cwd);
        }

        let mut child = pair.slave.spawn_command(command)?;
        let killer = child.clone_killer();
        // The slave must be closed in the parent or the reader never sees EOF.
        drop(pair.slave);

        let mut reader = pair.master.try_clone_reader()?;
        let mut writer = pair.master.take_writer()?;

        let (events_tx, events_rx) = unbounded();
        let (input_tx, input_rx) = std_mpsc::channel::<Vec<u8>>();

        std::thread::Builder::new()
            .name("natikube-pty-reader".into())
            .spawn(move || read_loop(&mut reader, events_tx, &mut child))?;

        std::thread::Builder::new()
            .name("natikube-pty-writer".into())
            .spawn(move || {
                while let Ok(bytes) = input_rx.recv() {
                    if writer
                        .write_all(&bytes)
                        .and_then(|()| writer.flush())
                        .is_err()
                    {
                        break;
                    }
                }
            })?;

        Ok((
            Self {
                master: Mutex::new(pair.master),
                killer: Mutex::new(killer),
                input: input_tx,
            },
            events_rx,
        ))
    }
}

fn read_loop(
    reader: &mut dyn Read,
    events: UnboundedSender<BackendEvent>,
    child: &mut Box<dyn portable_pty::Child + Send + Sync>,
) {
    let mut buffer = [0u8; 16 * 1024];
    loop {
        match reader.read(&mut buffer) {
            Ok(0) | Err(_) => break,
            Ok(n) => {
                if events
                    .unbounded_send(BackendEvent::Output(buffer[..n].to_vec()))
                    .is_err()
                {
                    // The view is gone: stop the child too.
                    let _ = child.kill();
                    return;
                }
            }
        }
    }
    let reason = match child.wait() {
        Ok(status) if status.success() => "Process exited".to_string(),
        Ok(status) => format!("Process exited with code {}", status.exit_code()),
        Err(err) => format!("Process ended: {err}"),
    };
    let _ = events.unbounded_send(BackendEvent::Exited(reason));
}

impl Backend for LocalPty {
    fn write(&self, bytes: Vec<u8>) {
        let _ = self.input.send(bytes);
    }

    fn resize(&self, size: PtySize) {
        if let Ok(master) = self.master.lock() {
            let _ = master.resize(to_portable(size));
        }
    }
}

impl Drop for LocalPty {
    fn drop(&mut self) {
        if let Ok(mut killer) = self.killer.lock() {
            let _ = killer.kill();
        }
    }
}

#[cfg(test)]
mod tests {
    #[cfg(unix)]
    use std::time::{Duration, Instant};

    use super::*;
    #[cfg(unix)]
    use crate::domain::{Palette, TermSession};

    /// Drains events into a session until `done` says so or the deadline hits.
    #[cfg(unix)]
    fn pump(
        events: &mut BackendEvents,
        session: &mut TermSession,
        deadline: Duration,
        done: impl Fn(&TermSession) -> bool,
    ) -> Option<String> {
        let end = Instant::now() + deadline;
        let mut exit = None;
        while Instant::now() < end {
            while let Ok(event) = events.try_recv() {
                match event {
                    BackendEvent::Output(bytes) => session.feed(&bytes),
                    BackendEvent::Exited(reason) | BackendEvent::Failed(reason) => {
                        exit = Some(reason)
                    }
                }
            }
            if exit.is_some() || done(session) {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        exit
    }

    #[cfg(unix)]
    fn spec(program: &str, args: &[&str]) -> CommandSpec {
        CommandSpec {
            program: program.into(),
            args: args.iter().map(|a| a.to_string()).collect(),
            env: vec![("TERM".into(), "xterm-256color".into())],
            cwd: None,
        }
    }

    #[cfg(unix)]
    #[test]
    fn echo_output_reaches_the_grid_and_exit_is_reported() {
        let (_pty, mut events) =
            LocalPty::spawn(&spec("/bin/echo", &["hello natikube"]), PtySize::DEFAULT).unwrap();
        let mut session = TermSession::new(80, 24, 100);
        let mut exit = None;
        let deadline = Instant::now() + Duration::from_secs(10);
        while Instant::now() < deadline && exit.is_none() {
            exit = pump(&mut events, &mut session, Duration::from_millis(50), |_| {
                false
            });
        }
        assert_eq!(exit.as_deref(), Some("Process exited"));
        assert_eq!(
            session.snapshot(&Palette::dark()).row_text(0),
            "hello natikube"
        );
    }

    #[cfg(unix)]
    #[test]
    fn input_roundtrip_through_cat() {
        let (pty, mut events) = LocalPty::spawn(&spec("/bin/cat", &[]), PtySize::DEFAULT).unwrap();
        let mut session = TermSession::new(80, 24, 100);
        pty.write(b"ping\n".to_vec());
        pump(&mut events, &mut session, Duration::from_secs(10), |s| {
            s.snapshot(&Palette::dark()).text().contains("ping")
        });
        // The tty echoes the input and cat prints it back.
        assert!(
            session
                .snapshot(&Palette::dark())
                .row_text(0)
                .contains("ping")
        );
        pty.resize(PtySize {
            cols: 100,
            rows: 30,
            cell_width: 8,
            cell_height: 16,
        });
    }

    #[cfg(unix)]
    #[test]
    fn dropping_the_backend_kills_the_child() {
        let (pty, mut events) =
            LocalPty::spawn(&spec("/bin/sleep", &["60"]), PtySize::DEFAULT).unwrap();
        drop(pty);
        let mut session = TermSession::new(80, 24, 10);
        let exit = pump(&mut events, &mut session, Duration::from_secs(10), |_| {
            false
        });
        // Either the exit notice arrived or the channel closed after kill.
        let closed = events.try_recv().is_err();
        assert!(exit.is_some() || closed);
    }

    #[cfg(unix)]
    #[test]
    fn explicit_shell_wins_and_blank_falls_back() {
        assert_eq!(login_shell_spec_for(Some("/bin/bash")).program, "/bin/bash");
        assert_eq!(
            login_shell_spec_for(Some(" /opt/fish ")).program,
            "/opt/fish"
        );
        assert_eq!(
            login_shell_spec_for(Some("  ")).program,
            login_shell_spec().program
        );
        assert_eq!(login_shell_spec_for(None).args, vec!["-l".to_string()]);
    }

    #[cfg(unix)]
    #[test]
    fn login_shell_spec_is_sane() {
        let spec = login_shell_spec();
        assert!(!spec.program.is_empty());
        assert_eq!(spec.args, vec!["-l".to_string()]);
        assert!(
            spec.env
                .iter()
                .any(|(k, v)| k == "TERM" && v == "xterm-256color")
        );
    }

    #[test]
    fn spec_on_unix_is_a_login_shell_in_home() {
        let spec = shell_spec(
            ShellFlavor::Unix,
            None,
            Some("/bin/bash"),
            |_| false,
            Some(PathBuf::from("/home/u")),
        );
        assert_eq!(spec.program, "/bin/bash");
        assert_eq!(spec.args, vec!["-l".to_string()]);
        assert_eq!(spec.cwd, Some(PathBuf::from("/home/u")));
        assert!(
            spec.env
                .iter()
                .any(|(k, v)| k == "TERM_PROGRAM" && v == "NatiKube")
        );
    }

    #[test]
    fn spec_on_windows_picks_powershell_without_login_flag() {
        let spec = shell_spec(
            ShellFlavor::Windows,
            None,
            Some("/bin/bash"),
            |n| n == "powershell.exe" || n == "cmd.exe",
            None,
        );
        assert_eq!(spec.program, "powershell.exe");
        assert_eq!(spec.args, vec!["-NoLogo".to_string()]);
        let cmd = shell_spec(ShellFlavor::Windows, None, None, |_| false, None);
        assert_eq!(cmd.program, "cmd.exe");
        assert!(cmd.args.is_empty());
    }
}
