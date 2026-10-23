/// One log line. `timestamp` is the RFC 3339 prefix the API adds when
/// `timestamps=true`, split from the message so the UI can toggle it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogLine {
    /// Monotonic sequence number; never reused, survives ring eviction.
    pub seq: u64,
    pub timestamp: Option<String>,
    pub text: String,
}

impl LogLine {
    /// The text as displayed: with or without the timestamp prefix.
    pub fn display(&self, show_timestamps: bool) -> String {
        match (&self.timestamp, show_timestamps) {
            (Some(ts), true) => format!("{ts} {}", self.text),
            _ => self.text.clone(),
        }
    }
}

/// Splits `"2024-05-01T10:00:00.123456789Z message"` into timestamp and text.
/// Lines without a valid leading timestamp are returned untouched.
pub fn split_timestamp(raw: &str) -> (Option<&str>, &str) {
    let Some((head, rest)) = raw.split_once(' ') else {
        // A timestamp-only line (empty message).
        return if looks_like_timestamp(raw) {
            (Some(raw), "")
        } else {
            (None, raw)
        };
    };
    if looks_like_timestamp(head) {
        (Some(head), rest)
    } else {
        (None, raw)
    }
}

fn looks_like_timestamp(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() >= 20
        && b[4] == b'-'
        && b[7] == b'-'
        && b[10] == b'T'
        && b[13] == b':'
        && b[16] == b':'
        && b[..4].iter().all(u8::is_ascii_digit)
        && (s.ends_with('Z') || s[19..].contains(['+', '-']))
}

/// Removes ANSI escape sequences (colors, cursor moves) so they do not render
/// as garbage in the viewer.
pub fn strip_ansi(input: &str) -> String {
    if !input.contains('\u{1b}') {
        return input.to_string();
    }
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\u{1b}' {
            out.push(c);
            continue;
        }
        if chars.peek() == Some(&'[') {
            chars.next();
            // CSI: parameters then a final byte in 0x40..=0x7e.
            for next in chars.by_ref() {
                if ('\u{40}'..='\u{7e}').contains(&next) {
                    break;
                }
            }
        }
        // A lone ESC is dropped.
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_a_timestamp_prefix() {
        let (ts, text) = split_timestamp("2024-05-01T10:00:00.123456789Z hello world");
        assert_eq!(ts, Some("2024-05-01T10:00:00.123456789Z"));
        assert_eq!(text, "hello world");
    }

    #[test]
    fn leaves_plain_lines_alone() {
        assert_eq!(split_timestamp("just text"), (None, "just text"));
        assert_eq!(split_timestamp("2024 not a ts"), (None, "2024 not a ts"));
    }

    #[test]
    fn timestamp_only_line_has_empty_text() {
        let (ts, text) = split_timestamp("2024-05-01T10:00:00Z");
        assert_eq!(ts, Some("2024-05-01T10:00:00Z"));
        assert_eq!(text, "");
    }

    #[test]
    fn display_toggles_timestamp() {
        let line = LogLine {
            seq: 0,
            timestamp: Some("2024-05-01T10:00:00Z".into()),
            text: "hi".into(),
        };
        assert_eq!(line.display(true), "2024-05-01T10:00:00Z hi");
        assert_eq!(line.display(false), "hi");
    }

    #[test]
    fn strips_ansi_colors() {
        assert_eq!(strip_ansi("\u{1b}[31merror\u{1b}[0m: boom"), "error: boom");
        assert_eq!(strip_ansi("plain"), "plain");
    }
}
