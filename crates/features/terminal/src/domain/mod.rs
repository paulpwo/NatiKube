//! Pure terminal logic: no GPUI, no I/O. Everything here is unit tested.

pub mod geometry;
pub mod keys;
pub mod mouse;
pub mod palette;
pub mod session;
pub mod shell;
pub mod snapshot;

pub use geometry::{GridMetrics, ScrollAccumulator};
pub use keys::{KeyMods, bracketed_paste, encode_key};
pub use palette::Palette;
pub use session::TermSession;
pub use snapshot::{CursorSnapshot, Run, Snapshot};

/// Terminal size in cells and pixels, as reported to the PTY or remote exec.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PtySize {
    pub cols: u16,
    pub rows: u16,
    pub cell_width: u16,
    pub cell_height: u16,
}

impl PtySize {
    pub const DEFAULT: PtySize = PtySize {
        cols: 80,
        rows: 24,
        cell_width: 8,
        cell_height: 16,
    };
}

/// What a backend (local PTY or pod exec) reports to the view.
#[derive(Debug)]
pub enum BackendEvent {
    /// Bytes produced by the remote side.
    Output(Vec<u8>),
    /// The session ended; the text is a human readable reason.
    Exited(String),
    /// The session could not start or broke; the text is an error to show.
    Failed(String),
}
