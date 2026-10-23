//! F7 — Command palette (⌘K), available in every appearance.
//!
//! * `domain`: pure item model, matching and ranking (palette stream).
//! * `ui`: [`PaletteView`], a modal overlay owned by the shell.

pub mod domain;
pub mod ui;

pub use domain::{PaletteContext, PaletteItem};
pub use ui::PaletteView;
