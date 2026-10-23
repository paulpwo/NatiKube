//! F5 — Terminal (vertical slice).
//!
//! * `domain`: pure terminal state helpers (selection, key encoding).
//! * `adapters`: portable-pty local shell and kube websocket exec.
//! * `ui`: [`TerminalView`] with an own alacritty_terminal grid renderer.

pub mod adapters;
pub mod domain;
pub mod ui;

pub use ui::{TerminalView, current_font_size, set_font};
