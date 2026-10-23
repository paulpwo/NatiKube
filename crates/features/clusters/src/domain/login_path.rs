//! Pure helpers for resolving the user's login-shell `PATH`.

/// Extracts the `PATH` value from the output of `$SHELL -l -i -c 'printf %s "$PATH"'`.
///
/// Interactive login shells may print banners or escape sequences around the
/// value, so the last line that looks like a path list wins.
pub fn parse_login_path(output: &str) -> Option<String> {
    output.lines().rev().find_map(|line| {
        let line = strip_escapes(line);
        let line = line.trim();
        let looks_like_path =
            line.starts_with('/') && !line.contains(char::is_whitespace) && !line.contains(": ");
        looks_like_path.then(|| line.to_string())
    })
}

/// Removes ANSI CSI sequences (`ESC [ ... letter`) from a line.
fn strip_escapes(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            if chars.peek() == Some(&'[') {
                chars.next();
                for n in chars.by_ref() {
                    if n.is_ascii_alphabetic() {
                        break;
                    }
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// Joins two `PATH` values: every login entry first, then the entries of the
/// current one that the login path lacks. Empty segments are dropped.
pub fn merge_paths(login: &str, current: &str) -> String {
    let mut seen = std::collections::HashSet::new();
    login
        .split(':')
        .chain(current.split(':'))
        .filter(|s| !s.is_empty() && seen.insert(*s))
        .collect::<Vec<_>>()
        .join(":")
}

/// Whether the current `PATH` looks like the minimal one Finder hands to apps
/// (no Homebrew or `/usr/local` entries).
pub fn needs_login_path(current: &str) -> bool {
    !current
        .split(':')
        .any(|p| p == "/opt/homebrew/bin" || p == "/usr/local/bin")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_plain_path() {
        assert_eq!(
            parse_login_path("/opt/homebrew/bin:/usr/bin:/bin"),
            Some("/opt/homebrew/bin:/usr/bin:/bin".to_string())
        );
    }

    #[test]
    fn ignores_shell_noise_and_takes_last_path_like_line() {
        let out = "Welcome back!\nnvm: loading\n\u{1b}[0m/opt/homebrew/bin:/usr/bin\n";
        assert_eq!(
            parse_login_path(out),
            Some("/opt/homebrew/bin:/usr/bin".to_string())
        );
    }

    #[test]
    fn rejects_empty_or_non_path_output() {
        assert_eq!(parse_login_path(""), None);
        assert_eq!(parse_login_path("command not found: foo\n"), None);
        assert_eq!(parse_login_path("   \n"), None);
    }

    #[test]
    fn merge_keeps_login_order_and_appends_missing_current_entries() {
        let merged = merge_paths("/opt/homebrew/bin:/usr/bin", "/usr/bin:/bin:/usr/sbin");
        assert_eq!(merged, "/opt/homebrew/bin:/usr/bin:/bin:/usr/sbin");
    }

    #[test]
    fn merge_drops_empty_segments() {
        assert_eq!(merge_paths("/a::/b", ":/c:"), "/a:/b:/c");
    }

    #[test]
    fn finder_path_needs_login_resolution() {
        assert!(needs_login_path("/usr/bin:/bin:/usr/sbin:/sbin"));
        assert!(needs_login_path(""));
        assert!(!needs_login_path("/opt/homebrew/bin:/usr/bin:/bin"));
        assert!(!needs_login_path("/usr/local/bin:/usr/bin"));
    }
}
