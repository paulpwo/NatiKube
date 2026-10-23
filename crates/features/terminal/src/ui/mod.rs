//! GPUI views of the terminal feature.

mod paint;
mod terminal_view;

pub use paint::{current_font_size, set_font};
pub use terminal_view::TerminalView;
