#[cfg(not(windows))]
use std::io::Read;
#[cfg(not(windows))]
use std::path::Path;
#[cfg(not(windows))]
use std::process::{Command, Stdio};
#[cfg(not(windows))]
use std::time::{Duration, Instant};

#[cfg(not(windows))]
use crate::domain::{merge_paths, needs_login_path, parse_login_path};

/// Upper bound on how long startup waits for the login shell.
#[cfg(not(windows))]
const LOGIN_SHELL_TIMEOUT: Duration = Duration::from_secs(3);

/// Prepares the process environment before any cluster connection is made.
///
/// Called once by the binary at startup, before the Tokio runtime or the UI
/// start. When the app is launched from Finder the process has a minimal
/// `PATH`, so kubeconfig exec credential plugins (`aws`, `gke-gcloud-auth-plugin`,
/// ...) cannot be found. When the inherited `PATH` looks minimal, this resolves
/// the user's login-shell `PATH` (bounded by a timeout, so startup never
/// hangs) and merges it into the process environment.
///
/// Windows has no login shell and GUI apps inherit the full user `PATH`, so
/// there only the credential-plugin console flag is set: this is a GUI-subsystem
/// process, so without `KUBE_RS_UNSTABLE_CREATE_NO_WINDOW=1` every exec plugin
/// (`aws`, `kubelogin`, ...) flashes a console window on each token refresh.
#[cfg(windows)]
pub fn init_environment() {
    if let Some(value) = no_window_value(std::env::var(NO_WINDOW_VAR).ok().as_deref()) {
        // SAFETY: called first thing in `main`, before any other thread
        // (Tokio runtime, GPUI) exists, so nothing reads the environment
        // concurrently.
        unsafe { std::env::set_var(NO_WINDOW_VAR, value) };
    }
}

/// Environment variable `kube-client` reads (exactly the value `"1"` enables it)
/// to spawn exec credential plugins with `CREATE_NO_WINDOW` on Windows.
#[cfg_attr(not(windows), allow(dead_code))]
pub const NO_WINDOW_VAR: &str = "KUBE_RS_UNSTABLE_CREATE_NO_WINDOW";

/// Decides what to set `KUBE_RS_UNSTABLE_CREATE_NO_WINDOW` to, given its
/// current value. An explicit user choice (any value already present) wins, so
/// someone running an interactive plugin that needs stderr can opt out with `0`.
#[cfg_attr(not(windows), allow(dead_code))]
pub fn no_window_value(current: Option<&str>) -> Option<&'static str> {
    match current {
        Some(_) => None,
        None => Some("1"),
    }
}

#[cfg(not(windows))]
pub fn init_environment() {
    let current = std::env::var("PATH").unwrap_or_default();
    if !needs_login_path(&current) {
        return;
    }
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_string());
    match resolve_login_path(Path::new(&shell), LOGIN_SHELL_TIMEOUT) {
        Some(login) => {
            let merged = merge_paths(&login, &current);
            // SAFETY: called first thing in `main`, before any other thread
            // (Tokio runtime, GPUI) exists, so nothing reads the environment
            // concurrently.
            unsafe { std::env::set_var("PATH", merged) };
            tracing::info!("resolved PATH from login shell");
        }
        None => tracing::warn!("could not resolve PATH from login shell; keeping inherited PATH"),
    }
}

/// Runs `<shell> -l -i -c 'printf %s "$PATH"'` and parses its output. Returns
/// `None` when the shell fails, prints nothing path-like or exceeds `timeout`
/// (the child is killed in that case).
#[cfg(not(windows))]
pub fn resolve_login_path(shell: &Path, timeout: Duration) -> Option<String> {
    let mut child = Command::new(shell)
        .args(["-l", "-i", "-c", "printf %s \"$PATH\""])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let mut stdout = child.stdout.take()?;
    let reader = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = stdout.read_to_end(&mut buf);
        buf
    });
    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                if !status.success() {
                    return None;
                }
                break;
            }
            Ok(None) if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(20));
            }
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    }
    let bytes = reader.join().ok()?;
    parse_login_path(&String::from_utf8_lossy(&bytes))
}

#[cfg(test)]
mod decision_tests {
    use super::*;

    #[test]
    fn sets_flag_when_unset() {
        assert_eq!(no_window_value(None), Some("1"));
    }

    #[test]
    fn keeps_an_explicit_user_choice() {
        assert_eq!(no_window_value(Some("0")), None);
        assert_eq!(no_window_value(Some("1")), None);
    }

    #[test]
    fn variable_name_matches_kube_client() {
        assert_eq!(NO_WINDOW_VAR, "KUBE_RS_UNSTABLE_CREATE_NO_WINDOW");
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn script(dir: &tempfile::TempDir, body: &str) -> std::path::PathBuf {
        let path = dir.path().join("fakeshell");
        std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        path
    }

    #[test]
    fn reads_path_from_shell_output() {
        let dir = tempfile::tempdir().unwrap();
        let shell = script(&dir, "echo banner; printf %s /opt/homebrew/bin:/usr/bin");
        assert_eq!(
            resolve_login_path(&shell, Duration::from_secs(5)),
            Some("/opt/homebrew/bin:/usr/bin".to_string())
        );
    }

    #[test]
    fn failing_shell_yields_none() {
        let dir = tempfile::tempdir().unwrap();
        let shell = script(&dir, "printf %s /usr/bin; exit 3");
        assert_eq!(resolve_login_path(&shell, Duration::from_secs(5)), None);
    }

    #[test]
    fn slow_shell_times_out_without_blocking() {
        let dir = tempfile::tempdir().unwrap();
        let shell = script(&dir, "exec sleep 30");
        let started = Instant::now();
        assert_eq!(resolve_login_path(&shell, Duration::from_millis(200)), None);
        assert!(started.elapsed() < Duration::from_secs(5));
    }

    #[test]
    fn missing_shell_yields_none() {
        assert_eq!(
            resolve_login_path(Path::new("/nonexistent/shell"), Duration::from_secs(1)),
            None
        );
    }
}
