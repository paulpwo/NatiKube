//! The OS command that opens a URL in the default browser.

use std::process::Command;

/// Program and arguments that open `url`: `open <url>` on macOS (and Linux
/// via `xdg-open`), `cmd /C start "" <url>` on Windows. The empty string is
/// the window title `start` would otherwise take from a quoted first argument.
pub fn open_url_command(windows: bool, macos: bool, url: &str) -> (&'static str, Vec<String>) {
    if windows {
        (
            "cmd",
            vec![
                "/C".to_string(),
                "start".to_string(),
                String::new(),
                url.to_string(),
            ],
        )
    } else if macos {
        ("open", vec![url.to_string()])
    } else {
        ("xdg-open", vec![url.to_string()])
    }
}

/// Opens `url` in the default browser on the current OS.
pub fn open_url(url: &str) -> std::io::Result<()> {
    let (program, args) = open_url_command(cfg!(windows), cfg!(target_os = "macos"), url);
    let mut command = Command::new(program);
    command.args(args);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // CREATE_NO_WINDOW: do not flash a console for `cmd`.
        command.creation_flags(0x0800_0000);
    }
    command.spawn().map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn macos_uses_open() {
        assert_eq!(
            open_url_command(false, true, "http://localhost:8080"),
            ("open", vec!["http://localhost:8080".to_string()])
        );
    }

    #[test]
    fn windows_uses_cmd_start_with_an_empty_title() {
        let (program, args) = open_url_command(true, false, "http://localhost:8080");
        assert_eq!(program, "cmd");
        assert_eq!(args, vec!["/C", "start", "", "http://localhost:8080"]);
    }

    #[test]
    fn other_unix_uses_xdg_open() {
        assert_eq!(open_url_command(false, false, "u").0, "xdg-open");
    }
}
