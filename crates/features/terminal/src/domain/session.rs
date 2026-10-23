//! One emulator instance: bytes in, snapshot and replies out.

use std::sync::{Arc, Mutex};

use alacritty_terminal::event::{Event, EventListener, WindowSize};
use alacritty_terminal::grid::{Dimensions, Scroll};
use alacritty_terminal::index::{Point, Side};
use alacritty_terminal::selection::{Selection, SelectionType};
use alacritty_terminal::term::{Config, Term, TermMode, viewport_to_point};
use alacritty_terminal::vte::ansi::Processor;

use super::palette::Palette;
use super::snapshot::{Snapshot, snapshot};

/// Collects events the emulator wants the host to act on.
#[derive(Clone, Default)]
struct EventSink(Arc<Mutex<Vec<Event>>>);

impl EventListener for EventSink {
    fn send_event(&self, event: Event) {
        if let Ok(mut events) = self.0.lock() {
            events.push(event);
        }
    }
}

/// Wraps `alacritty_terminal::Term` and its ANSI parser.
pub struct TermSession {
    term: Term<EventSink>,
    parser: Processor,
    sink: EventSink,
    cell_size: (u16, u16),
    title: Option<String>,
    /// Bytes the emulator wants written back to the program (cursor position
    /// reports, device attributes, ...).
    replies: Vec<u8>,
    clipboard: Vec<String>,
    bell: bool,
}

impl TermSession {
    pub fn new(cols: usize, rows: usize, scrollback: usize) -> Self {
        let sink = EventSink::default();
        let config = Config {
            scrolling_history: scrollback,
            ..Config::default()
        };
        let size = Size {
            cols: cols.max(2),
            rows: rows.max(1),
        };
        Self {
            term: Term::new(config, &size, sink.clone()),
            parser: Processor::new(),
            sink,
            cell_size: (8, 16),
            title: None,
            replies: Vec::new(),
            clipboard: Vec::new(),
            bell: false,
        }
    }

    /// Feeds bytes from the program into the emulator and processes the
    /// resulting events.
    pub fn feed(&mut self, bytes: &[u8]) {
        self.parser.advance(&mut self.term, bytes);
        self.drain_events();
    }

    fn drain_events(&mut self) {
        let events = match self.sink.0.lock() {
            Ok(mut guard) => std::mem::take(&mut *guard),
            Err(_) => return,
        };
        for event in events {
            match event {
                Event::PtyWrite(text) => self.replies.extend_from_slice(text.as_bytes()),
                Event::Title(title) => self.title = Some(title),
                Event::ResetTitle => self.title = None,
                Event::ClipboardStore(_, text) => self.clipboard.push(text),
                Event::TextAreaSizeRequest(format) => {
                    let size = WindowSize {
                        num_lines: self.term.screen_lines() as u16,
                        num_cols: self.term.columns() as u16,
                        cell_width: self.cell_size.0,
                        cell_height: self.cell_size.1,
                    };
                    self.replies.extend_from_slice(format(size).as_bytes());
                }
                Event::ColorRequest(index, format) => {
                    let palette = Palette::dark();
                    let color = self.term.colors()[index].unwrap_or(match index {
                        256 => palette.foreground,
                        257 => palette.background,
                        _ => palette.ansi[index.min(15)],
                    });
                    self.replies.extend_from_slice(format(color).as_bytes());
                }
                Event::Bell => self.bell = true,
                _ => {}
            }
        }
    }

    /// Bytes to write back to the program.
    pub fn take_replies(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.replies)
    }

    /// Text the program asked to put on the clipboard (OSC 52).
    pub fn take_clipboard_requests(&mut self) -> Vec<String> {
        std::mem::take(&mut self.clipboard)
    }

    pub fn take_bell(&mut self) -> bool {
        std::mem::take(&mut self.bell)
    }

    /// Window title set by the program, if any.
    pub fn title(&self) -> Option<&str> {
        self.title.as_deref()
    }

    pub fn set_cell_size(&mut self, width: u16, height: u16) {
        self.cell_size = (width.max(1), height.max(1));
    }

    pub fn resize(&mut self, cols: usize, rows: usize) {
        let (cols, rows) = (cols.max(2), rows.max(1));
        if cols != self.term.columns() || rows != self.term.screen_lines() {
            self.term.resize(Size { cols, rows });
        }
    }

    pub fn cols(&self) -> usize {
        self.term.columns()
    }

    pub fn rows(&self) -> usize {
        self.term.screen_lines()
    }

    pub fn mode(&self) -> TermMode {
        *self.term.mode()
    }

    pub fn display_offset(&self) -> usize {
        self.term.grid().display_offset()
    }

    /// Scrolls the viewport; positive moves towards older history.
    pub fn scroll(&mut self, lines: i32) {
        self.term.scroll_display(Scroll::Delta(lines));
    }

    pub fn scroll_to_bottom(&mut self) {
        self.term.scroll_display(Scroll::Bottom);
    }

    pub fn snapshot(&self, palette: &Palette) -> Snapshot {
        snapshot(&self.term, palette)
    }

    fn viewport_point(&self, row: usize, col: usize) -> Point {
        viewport_to_point(
            self.display_offset(),
            Point::new(
                row.min(self.rows() - 1),
                alacritty_terminal::index::Column(col.min(self.cols() - 1)),
            ),
        )
    }

    pub fn start_selection(&mut self, row: usize, col: usize, side: Side) {
        let point = self.viewport_point(row, col);
        self.term.selection = Some(Selection::new(SelectionType::Simple, point, side));
    }

    pub fn start_semantic_selection(&mut self, row: usize, col: usize) {
        let point = self.viewport_point(row, col);
        self.term.selection = Some(Selection::new(SelectionType::Semantic, point, Side::Left));
    }

    pub fn start_line_selection(&mut self, row: usize, col: usize) {
        let point = self.viewport_point(row, col);
        self.term.selection = Some(Selection::new(SelectionType::Lines, point, Side::Left));
    }

    pub fn update_selection(&mut self, row: usize, col: usize, side: Side) {
        let point = self.viewport_point(row, col);
        if let Some(selection) = self.term.selection.as_mut() {
            selection.update(point, side);
        }
    }

    pub fn clear_selection(&mut self) {
        self.term.selection = None;
    }

    pub fn has_selection(&self) -> bool {
        self.term
            .selection
            .as_ref()
            .is_some_and(|selection| !selection.is_empty())
    }

    pub fn selection_text(&self) -> Option<String> {
        self.term
            .selection_to_string()
            .filter(|text| !text.is_empty())
    }

    /// Selects the whole buffer (Cmd+A).
    pub fn select_all(&mut self) {
        let top = self.term.topmost_line();
        let bottom = self.term.bottommost_line();
        let last = alacritty_terminal::index::Column(self.cols() - 1);
        let mut selection = Selection::new(
            SelectionType::Simple,
            Point::new(top, alacritty_terminal::index::Column(0)),
            Side::Left,
        );
        selection.update(Point::new(bottom, last), Side::Right);
        self.term.selection = Some(selection);
    }
}

struct Size {
    cols: usize,
    rows: usize,
}

impl Dimensions for Size {
    fn total_lines(&self) -> usize {
        self.rows
    }

    fn screen_lines(&self) -> usize {
        self.rows
    }

    fn columns(&self) -> usize {
        self.cols
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(session: &TermSession) -> String {
        session.snapshot(&Palette::dark()).text()
    }

    #[test]
    fn cursor_position_report_is_answered() {
        let mut s = TermSession::new(20, 5, 10);
        s.feed(b"ab\x1b[6n");
        assert_eq!(s.take_replies(), b"\x1b[1;3R".to_vec());
        assert!(s.take_replies().is_empty());
    }

    #[test]
    fn title_and_osc52_are_surfaced() {
        let mut s = TermSession::new(20, 5, 10);
        s.feed(b"\x1b]0;my title\x07");
        assert_eq!(s.title(), Some("my title"));
        // "hi" base64 = aGk=
        s.feed(b"\x1b]52;c;aGk=\x07");
        assert_eq!(s.take_clipboard_requests(), vec!["hi".to_string()]);
    }

    #[test]
    fn resize_reflows_and_clamps() {
        let mut s = TermSession::new(10, 3, 10);
        s.feed(b"0123456789AB");
        s.resize(20, 3);
        assert_eq!((s.cols(), s.rows()), (20, 3));
        s.resize(0, 0);
        assert_eq!((s.cols(), s.rows()), (2, 1));
    }

    #[test]
    fn scrollback_scrolls_and_returns_to_bottom() {
        let mut s = TermSession::new(10, 3, 100);
        for i in 0..10 {
            s.feed(format!("line{i}\r\n").as_bytes());
        }
        assert_eq!(s.display_offset(), 0);
        s.scroll(4);
        assert_eq!(s.display_offset(), 4);
        let snap = s.snapshot(&Palette::dark());
        assert!(snap.history_size >= 7);
        assert_eq!(snap.display_offset, 4);
        assert!(snap.cursor.is_none(), "cursor is off-screen while scrolled");
        assert_ne!(snap.row_text(0), "line7");
        s.scroll_to_bottom();
        assert_eq!(s.display_offset(), 0);
        assert!(text(&s).contains("line9"));
    }

    #[test]
    fn alt_screen_and_modes_are_reported() {
        let mut s = TermSession::new(10, 3, 10);
        s.feed(b"\x1b[?1h\x1b[?2004h\x1b[?1049h");
        let snap = s.snapshot(&Palette::dark());
        assert!(snap.app_cursor && snap.bracketed_paste && snap.alt_screen);
    }

    #[test]
    fn select_all_copies_buffer_text() {
        let mut s = TermSession::new(10, 3, 10);
        s.feed(b"one\r\ntwo");
        s.select_all();
        let copied = s.selection_text().expect("selection");
        assert!(copied.contains("one") && copied.contains("two"));
        s.clear_selection();
        assert!(!s.has_selection());
    }
}
