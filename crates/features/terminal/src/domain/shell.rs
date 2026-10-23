//! Which local shell to start and how. Pure: environment lookups come in as
//! arguments so the choice is testable on any OS.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};

/// Windows shells in order of preference.
pub const WINDOWS_SHELLS: [&str; 3] = ["pwsh.exe", "powershell.exe", "cmd.exe"];

/// Fallback on macOS and Linux when neither Settings nor `$SHELL` name one.
pub const UNIX_FALLBACK_SHELL: &str = "/bin/zsh";

/// The family of platforms the shell choice differs between.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShellFlavor {
    Unix,
    Windows,
}

impl ShellFlavor {
    /// The flavor this binary was built for.
    pub const fn current() -> Self {
        if cfg!(windows) {
            Self::Windows
        } else {
            Self::Unix
        }
    }
}

/// First entry of `$PATH`-style `path_var` that holds `program` according to
/// `exists`.
pub fn find_in_path(
    program: &str,
    path_var: &OsStr,
    exists: impl Fn(&Path) -> bool,
) -> Option<PathBuf> {
    std::env::split_paths(path_var)
        .map(|dir| dir.join(program))
        .find(|candidate| exists(candidate))
}

/// The default Windows shell: `pwsh.exe`, else `powershell.exe`, else
/// `cmd.exe`. `on_path` says whether a program name resolves on `PATH`;
/// `cmd.exe` is always available, so it is the unconditional last resort.
pub fn default_windows_shell(on_path: impl Fn(&str) -> bool) -> &'static str {
    WINDOWS_SHELLS
        .iter()
        .copied()
        .find(|shell| on_path(shell))
        .unwrap_or("cmd.exe")
}

/// The shell program: an explicit non-blank setting wins; otherwise `$SHELL`
/// then zsh on Unix, or the first installed of [`WINDOWS_SHELLS`] on Windows
/// (`$SHELL` is ignored there: it only exists inside Git Bash and friends).
pub fn pick_shell(
    flavor: ShellFlavor,
    explicit: Option<&str>,
    env_shell: Option<&str>,
    on_path: impl Fn(&str) -> bool,
) -> String {
    let explicit = explicit.map(str::trim).filter(|shell| !shell.is_empty());
    if let Some(shell) = explicit {
        return shell.to_string();
    }
    match flavor {
        ShellFlavor::Unix => env_shell
            .filter(|shell| !shell.is_empty())
            .unwrap_or(UNIX_FALLBACK_SHELL)
            .to_string(),
        ShellFlavor::Windows => default_windows_shell(on_path).to_string(),
    }
}

/// Arguments that give an interactive session for `program`: `-l` (login
/// shell) on Unix; on Windows `-NoLogo` for PowerShell and none for `cmd`.
pub fn shell_args(flavor: ShellFlavor, program: &str) -> Vec<String> {
    match flavor {
        ShellFlavor::Unix => vec!["-l".to_string()],
        ShellFlavor::Windows => {
            let name = program
                .rsplit(['\\', '/'])
                .next()
                .unwrap_or(program)
                .to_ascii_lowercase();
            let name = name.strip_suffix(".exe").unwrap_or(&name);
            if name == "pwsh" || name == "powershell" {
                vec!["-NoLogo".to_string()]
            } else {
                Vec::new()
            }
        }
    }
}

/// What the terminal does for a clipboard-style shortcut.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClipboardAction {
    Copy,
    Paste,
    SelectAll,
}

/// Maps a key chord to a terminal clipboard action. macOS uses Cmd+C/V/A.
/// Elsewhere Ctrl+C must reach the shell as an interrupt, so the chord is
/// Ctrl+Shift+C/V/A.
pub fn clipboard_action(
    flavor: ShellFlavor,
    key: &str,
    control: bool,
    shift: bool,
    platform: bool,
) -> Option<ClipboardAction> {
    let chord = match flavor {
        ShellFlavor::Unix => platform,
        ShellFlavor::Windows => control && shift,
    };
    if !chord {
        return None;
    }
    match key.to_ascii_lowercase().as_str() {
        "c" => Some(ClipboardAction::Copy),
        "v" => Some(ClipboardAction::Paste),
        "a" => Some(ClipboardAction::SelectAll),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_prefers_pwsh_then_powershell_then_cmd() {
        let installed = |names: &'static [&'static str]| move |n: &str| names.contains(&n);
        assert_eq!(
            default_windows_shell(installed(&["pwsh.exe", "powershell.exe", "cmd.exe"])),
            "pwsh.exe"
        );
        assert_eq!(
            default_windows_shell(installed(&["powershell.exe", "cmd.exe"])),
            "powershell.exe"
        );
        assert_eq!(default_windows_shell(installed(&["cmd.exe"])), "cmd.exe");
        assert_eq!(default_windows_shell(installed(&[])), "cmd.exe");
    }

    #[test]
    fn explicit_setting_wins_on_both_flavors() {
        for flavor in [ShellFlavor::Unix, ShellFlavor::Windows] {
            assert_eq!(
                pick_shell(flavor, Some(" /opt/fish "), Some("/bin/bash"), |_| true),
                "/opt/fish"
            );
        }
    }

    #[test]
    fn unix_falls_back_to_env_shell_then_zsh() {
        let none = |_: &str| false;
        assert_eq!(
            pick_shell(ShellFlavor::Unix, None, Some("/bin/bash"), none),
            "/bin/bash"
        );
        assert_eq!(
            pick_shell(ShellFlavor::Unix, Some("  "), Some("/bin/bash"), none),
            "/bin/bash"
        );
        assert_eq!(
            pick_shell(ShellFlavor::Unix, None, Some(""), none),
            "/bin/zsh"
        );
        assert_eq!(pick_shell(ShellFlavor::Unix, None, None, none), "/bin/zsh");
    }

    #[test]
    fn windows_ignores_env_shell_and_probes_path() {
        let only_powershell = |n: &str| n == "powershell.exe";
        assert_eq!(
            pick_shell(
                ShellFlavor::Windows,
                None,
                Some("/usr/bin/bash"),
                only_powershell
            ),
            "powershell.exe"
        );
    }

    #[test]
    fn args_are_login_on_unix_and_quiet_powershell_on_windows() {
        assert_eq!(shell_args(ShellFlavor::Unix, "/bin/zsh"), vec!["-l"]);
        assert_eq!(
            shell_args(ShellFlavor::Windows, "pwsh.exe"),
            vec!["-NoLogo"]
        );
        assert_eq!(
            shell_args(
                ShellFlavor::Windows,
                r"C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe"
            ),
            vec!["-NoLogo"]
        );
        assert!(shell_args(ShellFlavor::Windows, "cmd.exe").is_empty());
        assert!(shell_args(ShellFlavor::Windows, r"C:\Program Files\Git\bin\bash.exe").is_empty());
    }

    #[test]
    fn find_in_path_returns_the_first_match() {
        let path = std::env::join_paths(["/a", "/b", "/c"]).unwrap();
        let found = find_in_path("tool", &path, |p| {
            p.starts_with("/b") || p.starts_with("/c")
        });
        assert_eq!(found, Some(PathBuf::from("/b").join("tool")));
        assert_eq!(find_in_path("tool", &path, |_| false), None);
    }

    #[test]
    fn clipboard_chords_per_platform() {
        use ClipboardAction::*;
        assert_eq!(
            clipboard_action(ShellFlavor::Unix, "c", false, false, true),
            Some(Copy)
        );
        assert_eq!(
            clipboard_action(ShellFlavor::Unix, "v", false, false, true),
            Some(Paste)
        );
        assert_eq!(
            clipboard_action(ShellFlavor::Unix, "x", false, false, true),
            None
        );
        // Plain Ctrl+C is an interrupt, never a copy.
        assert_eq!(
            clipboard_action(ShellFlavor::Windows, "c", true, false, false),
            None
        );
        assert_eq!(
            clipboard_action(ShellFlavor::Windows, "c", true, true, false),
            Some(Copy)
        );
        assert_eq!(
            clipboard_action(ShellFlavor::Windows, "A", true, true, false),
            Some(SelectAll)
        );
        assert_eq!(
            clipboard_action(ShellFlavor::Unix, "c", true, true, false),
            None
        );
    }
}
