//! Keyboard input to terminal bytes (xterm conventions).

/// Modifier state of a key press.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct KeyMods {
    pub control: bool,
    pub alt: bool,
    pub shift: bool,
    /// Command key on macOS; reserved for app shortcuts, never sent.
    pub platform: bool,
}

impl KeyMods {
    pub const NONE: KeyMods = KeyMods {
        control: false,
        alt: false,
        shift: false,
        platform: false,
    };

    /// xterm modifier parameter (1 = none).
    fn param(self) -> u8 {
        1 + self.shift as u8 + 2 * self.alt as u8 + 4 * self.control as u8
    }

    fn any(self) -> bool {
        self.shift || self.alt || self.control
    }
}

/// Encodes a key press. `key` is GPUI's key name (`"enter"`, `"up"`, `"a"`),
/// `key_char` the text the key would type. Returns `None` for keys that must
/// not reach the terminal (Command shortcuts, bare modifiers, unknown keys).
pub fn encode_key(
    key: &str,
    key_char: Option<&str>,
    mods: KeyMods,
    app_cursor: bool,
) -> Option<Vec<u8>> {
    if mods.platform {
        return None;
    }

    if let Some(bytes) = named_key(key, mods, app_cursor) {
        return Some(bytes);
    }

    // Single character keys.
    let base = single_char(key).or_else(|| key_char.and_then(single_char))?;

    if mods.control {
        if let Some(byte) = control_byte(base) {
            let mut out = Vec::with_capacity(2);
            if mods.alt {
                out.push(0x1b);
            }
            out.push(byte);
            return Some(out);
        }
        return None;
    }

    if mods.alt {
        // Alt as Meta: send ESC plus the base key (not the macOS option glyph).
        let ch = if mods.shift {
            base.to_uppercase().next().unwrap_or(base)
        } else {
            base
        };
        let mut out = vec![0x1b];
        let mut buf = [0u8; 4];
        out.extend_from_slice(ch.encode_utf8(&mut buf).as_bytes());
        return Some(out);
    }

    // Plain typing: prefer the produced text (handles shift, dead keys, IME).
    if let Some(text) = key_char
        && !text.is_empty()
    {
        return Some(text.as_bytes().to_vec());
    }
    if key.chars().count() == 1 {
        return Some(key.as_bytes().to_vec());
    }
    None
}

fn single_char(s: &str) -> Option<char> {
    let mut chars = s.chars();
    let first = chars.next()?;
    chars.next().is_none().then_some(first)
}

fn control_byte(c: char) -> Option<u8> {
    match c {
        'a'..='z' => Some(c as u8 - b'a' + 1),
        'A'..='Z' => Some(c as u8 - b'A' + 1),
        '@' | ' ' | '2' => Some(0x00),
        '[' | '3' => Some(0x1b),
        '\\' | '4' => Some(0x1c),
        ']' | '5' => Some(0x1d),
        '^' | '6' => Some(0x1e),
        '_' | '/' | '7' => Some(0x1f),
        '8' => Some(0x7f),
        _ => None,
    }
}

fn named_key(key: &str, mods: KeyMods, app_cursor: bool) -> Option<Vec<u8>> {
    let csi_mod = |final_byte: char| -> Vec<u8> {
        format!("\x1b[1;{}{}", mods.param(), final_byte).into_bytes()
    };
    let tilde = |code: u8| -> Vec<u8> {
        if mods.any() {
            format!("\x1b[{};{}~", code, mods.param()).into_bytes()
        } else {
            format!("\x1b[{code}~").into_bytes()
        }
    };
    let cursor = |final_byte: char| -> Vec<u8> {
        if mods.any() {
            csi_mod(final_byte)
        } else if app_cursor {
            format!("\x1bO{final_byte}").into_bytes()
        } else {
            format!("\x1b[{final_byte}").into_bytes()
        }
    };

    let bytes = match key {
        "enter" => {
            if mods.alt {
                b"\x1b\r".to_vec()
            } else {
                b"\r".to_vec()
            }
        }
        "tab" => {
            if mods.shift {
                b"\x1b[Z".to_vec()
            } else if mods.alt {
                b"\x1b\t".to_vec()
            } else {
                b"\t".to_vec()
            }
        }
        "escape" => b"\x1b".to_vec(),
        "backspace" => {
            let base: &[u8] = if mods.control { b"\x08" } else { b"\x7f" };
            let mut out = Vec::new();
            if mods.alt {
                out.push(0x1b);
            }
            out.extend_from_slice(base);
            out
        }
        "space" => {
            if mods.control {
                vec![0x00]
            } else if mods.alt {
                b"\x1b ".to_vec()
            } else {
                b" ".to_vec()
            }
        }
        "up" => cursor('A'),
        "down" => cursor('B'),
        "right" => cursor('C'),
        "left" => cursor('D'),
        "home" => cursor('H'),
        "end" => cursor('F'),
        "insert" => tilde(2),
        "delete" => tilde(3),
        "pageup" => tilde(5),
        "pagedown" => tilde(6),
        "f1" | "f2" | "f3" | "f4" => {
            let final_byte = match key {
                "f1" => 'P',
                "f2" => 'Q',
                "f3" => 'R',
                _ => 'S',
            };
            if mods.any() {
                csi_mod(final_byte)
            } else {
                format!("\x1bO{final_byte}").into_bytes()
            }
        }
        "f5" => tilde(15),
        "f6" => tilde(17),
        "f7" => tilde(18),
        "f8" => tilde(19),
        "f9" => tilde(20),
        "f10" => tilde(21),
        "f11" => tilde(23),
        "f12" => tilde(24),
        // Bare modifiers never produce bytes.
        "shift" | "control" | "alt" | "platform" | "function" | "capslock" => return Some(vec![]),
        _ => return None,
    };
    Some(bytes)
}

/// Wraps pasted text for the terminal. With bracketed paste on, the payload is
/// fenced (and any embedded end marker stripped so it cannot break out);
/// otherwise newlines become carriage returns like a typed Enter.
pub fn bracketed_paste(text: &str, bracketed: bool) -> Vec<u8> {
    if bracketed {
        let cleaned = text.replace("\x1b[201~", "");
        let mut out = b"\x1b[200~".to_vec();
        out.extend_from_slice(cleaned.as_bytes());
        out.extend_from_slice(b"\x1b[201~");
        out
    } else {
        text.replace("\r\n", "\r").replace('\n', "\r").into_bytes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NONE: KeyMods = KeyMods::NONE;
    const CTRL: KeyMods = KeyMods {
        control: true,
        ..KeyMods::NONE
    };
    const ALT: KeyMods = KeyMods {
        alt: true,
        ..KeyMods::NONE
    };
    const SHIFT: KeyMods = KeyMods {
        shift: true,
        ..KeyMods::NONE
    };

    fn enc(key: &str, ch: Option<&str>, mods: KeyMods) -> Option<Vec<u8>> {
        encode_key(key, ch, mods, false)
    }

    #[test]
    fn plain_text_uses_key_char() {
        assert_eq!(enc("a", Some("a"), NONE), Some(b"a".to_vec()));
        assert_eq!(enc("a", Some("A"), SHIFT), Some(b"A".to_vec()));
        assert_eq!(enc("é", Some("é"), NONE), Some("é".as_bytes().to_vec()));
    }

    #[test]
    fn control_letters_map_to_c0() {
        assert_eq!(enc("c", Some("c"), CTRL), Some(vec![0x03]));
        assert_eq!(enc("d", None, CTRL), Some(vec![0x04]));
        assert_eq!(enc("l", None, CTRL), Some(vec![0x0c]));
        assert_eq!(enc("[", None, CTRL), Some(vec![0x1b]));
        assert_eq!(enc("space", None, CTRL), Some(vec![0x00]));
    }

    #[test]
    fn alt_sends_escape_prefix_with_base_key() {
        // macOS Option+b types "∫" in key_char; Meta-b must still be ESC b.
        assert_eq!(enc("b", Some("∫"), ALT), Some(b"\x1bb".to_vec()));
        assert_eq!(enc("backspace", None, ALT), Some(b"\x1b\x7f".to_vec()));
        let ctrl_alt = KeyMods {
            control: true,
            alt: true,
            ..NONE
        };
        assert_eq!(enc("c", None, ctrl_alt), Some(vec![0x1b, 0x03]));
    }

    #[test]
    fn arrows_follow_application_cursor_mode() {
        assert_eq!(
            encode_key("up", None, NONE, false),
            Some(b"\x1b[A".to_vec())
        );
        assert_eq!(encode_key("up", None, NONE, true), Some(b"\x1bOA".to_vec()));
        assert_eq!(
            encode_key("home", None, NONE, true),
            Some(b"\x1bOH".to_vec())
        );
    }

    #[test]
    fn modified_arrows_use_xterm_parameters() {
        assert_eq!(enc("right", None, CTRL), Some(b"\x1b[1;5C".to_vec()));
        assert_eq!(enc("left", None, ALT), Some(b"\x1b[1;3D".to_vec()));
        assert_eq!(enc("up", None, SHIFT), Some(b"\x1b[1;2A".to_vec()));
        // Modified arrows ignore application cursor mode.
        assert_eq!(
            encode_key("down", None, CTRL, true),
            Some(b"\x1b[1;5B".to_vec())
        );
    }

    #[test]
    fn editing_and_function_keys() {
        assert_eq!(enc("enter", Some("\n"), NONE), Some(b"\r".to_vec()));
        assert_eq!(enc("tab", None, NONE), Some(b"\t".to_vec()));
        assert_eq!(enc("tab", None, SHIFT), Some(b"\x1b[Z".to_vec()));
        assert_eq!(enc("escape", None, NONE), Some(b"\x1b".to_vec()));
        assert_eq!(enc("backspace", None, NONE), Some(vec![0x7f]));
        assert_eq!(enc("delete", None, NONE), Some(b"\x1b[3~".to_vec()));
        assert_eq!(enc("pageup", None, NONE), Some(b"\x1b[5~".to_vec()));
        assert_eq!(enc("f1", None, NONE), Some(b"\x1bOP".to_vec()));
        assert_eq!(enc("f5", None, NONE), Some(b"\x1b[15~".to_vec()));
        assert_eq!(enc("f12", None, NONE), Some(b"\x1b[24~".to_vec()));
        assert_eq!(enc("space", Some(" "), NONE), Some(b" ".to_vec()));
    }

    #[test]
    fn command_key_and_unknown_keys_are_not_sent() {
        let cmd = KeyMods {
            platform: true,
            ..NONE
        };
        assert_eq!(enc("c", Some("c"), cmd), None);
        assert_eq!(enc("nonexistent", None, NONE), None);
        assert_eq!(enc("shift", None, SHIFT), Some(vec![]));
    }

    #[test]
    fn paste_is_bracketed_or_normalised() {
        assert_eq!(
            bracketed_paste("ls -l", true),
            b"\x1b[200~ls -l\x1b[201~".to_vec()
        );
        assert_eq!(
            bracketed_paste("a\x1b[201~b", true),
            b"\x1b[200~ab\x1b[201~".to_vec()
        );
        assert_eq!(bracketed_paste("a\nb\r\nc", false), b"a\rb\rc".to_vec());
    }
}
