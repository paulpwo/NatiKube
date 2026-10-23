//! Color resolution: grid colors (named, indexed, truecolor) to RGB.

use alacritty_terminal::term::color::Colors;
use alacritty_terminal::vte::ansi::{Color, NamedColor, Rgb};

/// Terminal palette. Defaults are a Freelens-like dark scheme.
#[derive(Debug, Clone)]
pub struct Palette {
    pub foreground: Rgb,
    pub background: Rgb,
    pub cursor: Rgb,
    pub selection: Rgb,
    /// The 16 ANSI colors: 8 normal then 8 bright.
    pub ansi: [Rgb; 16],
}

const fn rgb(hex: u32) -> Rgb {
    Rgb {
        r: (hex >> 16) as u8,
        g: (hex >> 8) as u8,
        b: hex as u8,
    }
}

impl Palette {
    pub fn dark() -> Self {
        Self {
            foreground: rgb(0xd4d7da),
            background: rgb(0x1e2124),
            cursor: rgb(0x3d90ce),
            selection: rgb(0x2f5f8a),
            ansi: [
                rgb(0x2b3035),
                rgb(0xe5534b),
                rgb(0x57ab5a),
                rgb(0xc69026),
                rgb(0x3d90ce),
                rgb(0xb083f0),
                rgb(0x00a7a0),
                rgb(0xcdd9e5),
                rgb(0x636e7b),
                rgb(0xff7b72),
                rgb(0x6bc46d),
                rgb(0xdaaa3f),
                rgb(0x6cb6ff),
                rgb(0xd2a8ff),
                rgb(0x39c5cf),
                rgb(0xf0f6fc),
            ],
        }
    }

    /// Replaces the default foreground/background (used to match the app theme).
    pub fn with_defaults(mut self, foreground: Rgb, background: Rgb) -> Self {
        self.foreground = foreground;
        self.background = background;
        self
    }

    /// Resolves a grid color. `overrides` are colors the program set at run
    /// time (OSC 4/10/11) and win over the palette.
    pub fn resolve(&self, color: Color, overrides: &Colors) -> Rgb {
        match color {
            Color::Spec(rgb) => rgb,
            Color::Indexed(index) => {
                if let Some(rgb) = overrides[index as usize] {
                    return rgb;
                }
                self.indexed(index)
            }
            Color::Named(named) => {
                if let Some(rgb) = overrides[named as usize] {
                    return rgb;
                }
                self.named(named)
            }
        }
    }

    fn indexed(&self, index: u8) -> Rgb {
        match index {
            0..=15 => self.ansi[index as usize],
            16..=231 => {
                let i = index - 16;
                let level = |v: u8| if v == 0 { 0 } else { 55 + 40 * v };
                Rgb {
                    r: level(i / 36),
                    g: level((i / 6) % 6),
                    b: level(i % 6),
                }
            }
            232..=255 => {
                let v = 8 + 10 * (index - 232);
                Rgb { r: v, g: v, b: v }
            }
        }
    }

    fn named(&self, named: NamedColor) -> Rgb {
        use NamedColor::*;
        match named {
            Foreground | BrightForeground => self.foreground,
            Background => self.background,
            Cursor => self.cursor,
            DimForeground => dim(self.foreground),
            DimBlack => dim(self.ansi[0]),
            DimRed => dim(self.ansi[1]),
            DimGreen => dim(self.ansi[2]),
            DimYellow => dim(self.ansi[3]),
            DimBlue => dim(self.ansi[4]),
            DimMagenta => dim(self.ansi[5]),
            DimCyan => dim(self.ansi[6]),
            DimWhite => dim(self.ansi[7]),
            other => self.ansi[(other as usize).min(15)],
        }
    }
}

/// Two thirds brightness, used for the SGR "faint" attribute.
pub fn dim(color: Rgb) -> Rgb {
    let scale = |v: u8| (v as u16 * 2 / 3) as u8;
    Rgb {
        r: scale(color.r),
        g: scale(color.g),
        b: scale(color.b),
    }
}

/// Bright variant of a normal ANSI color (bold-as-bright), identity otherwise.
pub fn brighten(color: Color) -> Color {
    match color {
        Color::Named(named) => {
            let index = named as usize;
            if index < 8 {
                Color::Indexed(index as u8 + 8)
            } else {
                color
            }
        }
        Color::Indexed(index) if index < 8 => Color::Indexed(index + 8),
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn none() -> Colors {
        Colors::default()
    }

    #[test]
    fn named_and_indexed_colors_resolve() {
        let p = Palette::dark();
        assert_eq!(
            p.resolve(Color::Named(NamedColor::Foreground), &none()),
            p.foreground
        );
        assert_eq!(p.resolve(Color::Named(NamedColor::Red), &none()), p.ansi[1]);
        assert_eq!(p.resolve(Color::Indexed(9), &none()), p.ansi[9]);
    }

    #[test]
    fn color_cube_and_grayscale() {
        let p = Palette::dark();
        // 196 = 16 + 5*36 -> pure red at level 5 = 255.
        assert_eq!(
            p.resolve(Color::Indexed(196), &none()),
            Rgb { r: 255, g: 0, b: 0 }
        );
        assert_eq!(
            p.resolve(Color::Indexed(232), &none()),
            Rgb { r: 8, g: 8, b: 8 }
        );
        assert_eq!(
            p.resolve(Color::Indexed(255), &none()),
            Rgb {
                r: 238,
                g: 238,
                b: 238
            }
        );
    }

    #[test]
    fn truecolor_passes_through_and_bold_brightens() {
        let p = Palette::dark();
        let spec = Rgb { r: 1, g: 2, b: 3 };
        assert_eq!(p.resolve(Color::Spec(spec), &none()), spec);
        assert_eq!(brighten(Color::Named(NamedColor::Blue)), Color::Indexed(12));
        assert_eq!(brighten(Color::Indexed(3)), Color::Indexed(11));
        assert_eq!(brighten(Color::Indexed(20)), Color::Indexed(20));
    }
}
