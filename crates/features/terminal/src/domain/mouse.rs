//! Mouse reports for applications that enable mouse tracking (vim, tmux, htop).

/// SGR (1006) mouse report. `button` is the xterm button code (0 left,
/// 1 middle, 2 right, 32 added for motion, 64/65 wheel up/down). Coordinates
/// are 0-based and sent 1-based.
pub fn sgr_report(button: u8, col: usize, row: usize, pressed: bool) -> Vec<u8> {
    let final_byte = if pressed { 'M' } else { 'm' };
    format!("\x1b[<{};{};{}{}", button, col + 1, row + 1, final_byte).into_bytes()
}

/// Wheel button code for one notch (up = towards history).
pub fn wheel_button(up: bool) -> u8 {
    if up { 64 } else { 65 }
}

/// Arrow key bytes sent for wheel scrolling on the alternate screen when the
/// application did not request mouse tracking (less, man, vim without mouse).
pub fn wheel_as_arrows(up: bool, lines: usize, app_cursor: bool) -> Vec<u8> {
    let seq: &[u8] = match (up, app_cursor) {
        (true, true) => b"\x1bOA",
        (true, false) => b"\x1b[A",
        (false, true) => b"\x1bOB",
        (false, false) => b"\x1b[B",
    };
    seq.repeat(lines)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sgr_reports_are_one_based() {
        assert_eq!(sgr_report(0, 0, 0, true), b"\x1b[<0;1;1M".to_vec());
        assert_eq!(sgr_report(0, 9, 4, false), b"\x1b[<0;10;5m".to_vec());
        assert_eq!(
            sgr_report(wheel_button(true), 2, 3, true),
            b"\x1b[<64;3;4M".to_vec()
        );
    }

    #[test]
    fn wheel_arrows_repeat() {
        assert_eq!(wheel_as_arrows(true, 2, false), b"\x1b[A\x1b[A".to_vec());
        assert_eq!(wheel_as_arrows(false, 1, true), b"\x1bOB".to_vec());
    }
}
