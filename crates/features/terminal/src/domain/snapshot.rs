//! Converts the emulator grid into paint-ready runs.
//!
//! A [`Snapshot`] is what the GPUI element draws: per visible row, runs of
//! equally styled text positioned by column, plus the cursor. Keeping this
//! free of GPUI makes the rendering rules (inverse, bold-as-bright, wide
//! characters, selection) unit-testable.

use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::term::{Term, TermMode, point_to_viewport};
use alacritty_terminal::vte::ansi::{CursorShape, Rgb};

use super::palette::{Palette, brighten, dim};

/// A run of cells sharing one style, starting at `col`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Run {
    pub col: usize,
    /// Width in cells (wide characters take two).
    pub cells: usize,
    pub text: String,
    pub fg: Rgb,
    /// `None` means the default background (not painted).
    pub bg: Option<Rgb>,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strikeout: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CursorSnapshot {
    pub row: usize,
    pub col: usize,
    pub shape: CursorShape,
    /// Width in cells (2 on a wide character).
    pub cells: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Snapshot {
    pub cols: usize,
    pub rows: usize,
    pub lines: Vec<Vec<Run>>,
    pub cursor: Option<CursorSnapshot>,
    pub display_offset: usize,
    pub history_size: usize,
    pub app_cursor: bool,
    pub bracketed_paste: bool,
    pub alt_screen: bool,
    pub mouse_mode: bool,
    pub sgr_mouse: bool,
    pub alternate_scroll: bool,
}

impl Snapshot {
    /// Plain text of one row (trailing blanks trimmed); handy for tests and
    /// accessibility.
    pub fn row_text(&self, row: usize) -> String {
        let Some(runs) = self.lines.get(row) else {
            return String::new();
        };
        let mut out = String::new();
        let mut col = 0;
        for run in runs {
            while col < run.col {
                out.push(' ');
                col += 1;
            }
            out.push_str(&run.text);
            col = run.col + run.cells;
        }
        out.trim_end().to_string()
    }

    /// The character painted at a cell, if any (used to redraw the glyph
    /// under a block cursor).
    pub fn char_at(&self, row: usize, col: usize) -> Option<char> {
        let runs = self.lines.get(row)?;
        runs.iter()
            .find(|run| col >= run.col && col < run.col + run.cells)
            .and_then(|run| {
                if run.cells == run.text.chars().count() {
                    run.text.chars().nth(col - run.col)
                } else {
                    run.text.chars().next()
                }
            })
    }

    /// All rows joined by newlines with trailing empty rows removed.
    pub fn text(&self) -> String {
        let mut rows: Vec<String> = (0..self.rows).map(|r| self.row_text(r)).collect();
        while rows.last().is_some_and(|r| r.is_empty()) {
            rows.pop();
        }
        rows.join("\n")
    }
}

#[derive(Clone, PartialEq, Eq)]
struct Style {
    fg: Rgb,
    bg: Option<Rgb>,
    bold: bool,
    italic: bool,
    underline: bool,
    strikeout: bool,
}

pub fn snapshot<T: alacritty_terminal::event::EventListener>(
    term: &Term<T>,
    palette: &Palette,
) -> Snapshot {
    let content = term.renderable_content();
    let cols = term.columns();
    let rows = term.screen_lines();
    let offset = content.display_offset;
    let selection = content.selection;
    let overrides = content.colors;

    // Row-major cells: (char, width, style); None for spacer cells.
    let mut grid: Vec<Vec<Option<(char, usize, Style)>>> = vec![vec![None; cols]; rows];

    for indexed in content.display_iter {
        let Some(view) = point_to_viewport(offset, indexed.point) else {
            continue;
        };
        if view.line >= rows || view.column.0 >= cols {
            continue;
        }
        let cell = indexed.cell;
        let flags = cell.flags;
        if flags.intersects(Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER) {
            continue;
        }

        let mut fg_color = cell.fg;
        if flags.contains(Flags::BOLD) && !flags.contains(Flags::DIM) {
            fg_color = brighten(fg_color);
        }
        let mut fg = palette.resolve(fg_color, overrides);
        let mut bg = palette.resolve(cell.bg, overrides);
        let default_bg = matches!(
            cell.bg,
            alacritty_terminal::vte::ansi::Color::Named(
                alacritty_terminal::vte::ansi::NamedColor::Background
            )
        );
        if flags.contains(Flags::DIM) && !flags.contains(Flags::BOLD) {
            fg = dim(fg);
        }
        let mut has_bg = !default_bg;
        if flags.contains(Flags::INVERSE) {
            std::mem::swap(&mut fg, &mut bg);
            has_bg = true;
        }
        if flags.contains(Flags::HIDDEN) {
            fg = bg;
        }
        let selected = selection.is_some_and(|range| range.contains(indexed.point));
        if selected {
            bg = palette.selection;
            has_bg = true;
        }

        let width = if flags.contains(Flags::WIDE_CHAR) {
            2
        } else {
            1
        };
        let mut ch = cell.c;
        if ch == '\0' {
            ch = ' ';
        }
        grid[view.line][view.column.0] = Some((
            ch,
            width,
            Style {
                fg,
                bg: has_bg.then_some(bg),
                bold: flags.contains(Flags::BOLD),
                italic: flags.contains(Flags::ITALIC),
                underline: flags.intersects(Flags::ALL_UNDERLINES),
                strikeout: flags.contains(Flags::STRIKEOUT),
            },
        ));
    }

    let lines = grid.into_iter().map(group_runs).collect();

    let mode = *term.mode();
    let cursor =
        if mode.contains(TermMode::SHOW_CURSOR) && content.cursor.shape != CursorShape::Hidden {
            point_to_viewport(offset, content.cursor.point)
                .filter(|p| p.line < rows && p.column.0 < cols)
                .map(|p| {
                    let on_wide = term.grid()[content.cursor.point]
                        .flags
                        .contains(Flags::WIDE_CHAR);
                    CursorSnapshot {
                        row: p.line,
                        col: p.column.0,
                        shape: content.cursor.shape,
                        cells: if on_wide { 2 } else { 1 },
                    }
                })
        } else {
            None
        };

    Snapshot {
        cols,
        rows,
        lines,
        cursor,
        display_offset: offset,
        history_size: term.history_size(),
        app_cursor: mode.contains(TermMode::APP_CURSOR),
        bracketed_paste: mode.contains(TermMode::BRACKETED_PASTE),
        alt_screen: mode.contains(TermMode::ALT_SCREEN),
        mouse_mode: mode.intersects(TermMode::MOUSE_MODE),
        sgr_mouse: mode.contains(TermMode::SGR_MOUSE),
        alternate_scroll: mode.contains(TermMode::ALTERNATE_SCROLL),
    }
}

/// Groups a row of cells into runs. Blank default cells are skipped (they
/// paint nothing); wide characters become their own run so a font that does
/// not render them two cells wide cannot shift the rest of the row.
fn group_runs(row: Vec<Option<(char, usize, Style)>>) -> Vec<Run> {
    let mut runs: Vec<Run> = Vec::new();
    for (col, cell) in row.into_iter().enumerate() {
        let Some((ch, width, style)) = cell else {
            continue;
        };
        let blank_default = ch == ' ' && style.bg.is_none() && !style.underline && !style.strikeout;
        if blank_default {
            continue;
        }
        if width == 1
            && let Some(last) = runs.last_mut()
            && last.col + last.cells == col
            && last.cells == last.text.chars().count()
            && last.fg == style.fg
            && last.bg == style.bg
            && last.bold == style.bold
            && last.italic == style.italic
            && last.underline == style.underline
            && last.strikeout == style.strikeout
        {
            last.text.push(ch);
            last.cells += 1;
            continue;
        }
        runs.push(Run {
            col,
            cells: width,
            text: ch.to_string(),
            fg: style.fg,
            bg: style.bg,
            bold: style.bold,
            italic: style.italic,
            underline: style.underline,
            strikeout: style.strikeout,
        });
    }
    runs
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::session::TermSession;

    fn snap(input: &str, cols: usize, rows: usize) -> Snapshot {
        let mut session = TermSession::new(cols, rows, 100);
        session.feed(input.as_bytes());
        session.snapshot(&Palette::dark())
    }

    #[test]
    fn plain_text_lands_on_rows() {
        let s = snap("hello\r\nworld", 20, 5);
        assert_eq!(s.row_text(0), "hello");
        assert_eq!(s.row_text(1), "world");
        assert_eq!(s.text(), "hello\nworld");
        let cursor = s.cursor.expect("cursor visible");
        assert_eq!((cursor.row, cursor.col), (1, 5));
    }

    #[test]
    fn sgr_colors_split_runs() {
        let s = snap("a\x1b[31mb\x1b[0mc", 10, 2);
        let p = Palette::dark();
        let runs = &s.lines[0];
        assert_eq!(runs.len(), 3);
        assert_eq!(runs[0].text, "a");
        assert_eq!(runs[1].text, "b");
        assert_eq!(runs[1].fg, p.ansi[1]);
        assert_eq!(runs[2].fg, p.foreground);
        assert_eq!(runs[2].col, 2);
    }

    #[test]
    fn bold_italic_underline_and_truecolor() {
        let s = snap("\x1b[1;3;4;38;2;10;20;30mX", 10, 2);
        let run = &s.lines[0][0];
        assert!(run.bold && run.italic && run.underline);
        assert_eq!(
            run.fg,
            Rgb {
                r: 10,
                g: 20,
                b: 30
            }
        );
    }

    #[test]
    fn bold_brightens_basic_colors() {
        let s = snap("\x1b[1;31mX", 10, 2);
        assert_eq!(s.lines[0][0].fg, Palette::dark().ansi[9]);
    }

    #[test]
    fn inverse_swaps_colors_and_sets_background() {
        let s = snap("\x1b[7mX", 10, 2);
        let p = Palette::dark();
        let run = &s.lines[0][0];
        assert_eq!(run.fg, p.background);
        assert_eq!(run.bg, Some(p.foreground));
    }

    #[test]
    fn wide_characters_take_two_cells_and_own_run() {
        let s = snap("a\u{4e2d}b", 10, 2);
        let runs = &s.lines[0];
        assert_eq!(runs.len(), 3);
        assert_eq!((runs[1].col, runs[1].cells), (1, 2));
        assert_eq!((runs[2].col, runs[2].text.as_str()), (3, "b"));
        assert_eq!(s.row_text(0), "a\u{4e2d}b");
    }

    #[test]
    fn char_at_finds_glyphs() {
        let s = snap("ab\u{4e2d}", 10, 2);
        assert_eq!(s.char_at(0, 1), Some('b'));
        assert_eq!(s.char_at(0, 2), Some('\u{4e2d}'));
        assert_eq!(s.char_at(0, 3), Some('\u{4e2d}'));
        assert_eq!(s.char_at(0, 6), None);
    }

    #[test]
    fn cursor_hidden_by_dectcem() {
        let s = snap("x\x1b[?25l", 10, 2);
        assert!(s.cursor.is_none());
    }

    #[test]
    fn selection_paints_selection_background() {
        let mut session = TermSession::new(10, 2, 100);
        session.feed(b"abcdef");
        session.start_selection(0, 1, alacritty_terminal::index::Side::Left);
        session.update_selection(0, 3, alacritty_terminal::index::Side::Right);
        let s = session.snapshot(&Palette::dark());
        let selected: Vec<_> = s.lines[0]
            .iter()
            .filter(|r| r.bg == Some(Palette::dark().selection))
            .map(|r| r.text.clone())
            .collect();
        assert_eq!(selected, vec!["bcd".to_string()]);
        assert_eq!(session.selection_text().as_deref(), Some("bcd"));
    }
}
