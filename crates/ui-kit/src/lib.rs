//! Shared look and feel: the Freelens-like dark theme and small widgets used
//! by several features. Depends on GPUI Kit only.

mod theme;
mod tokens;
mod widgets;

pub use theme::{Palette, Tone, apply as apply_theme, hex};
pub use tokens::{
    ActiveTokens, BASE_FONT_SIZE, COMMAND_FONT, COMPACT_HEADER, COMPACT_ROW, Fonts, NatiTheme,
    Radii, Tokens, force_native_dark, register_fonts, set_appearance, set_ui_prefs,
    sync_system_appearance, ui_scale, watch_system_appearance,
};
pub use widgets::{
    bar_fraction, container_squares, dashed_chip, empty_state, error_state, filter_chip,
    initials_avatar, kbd_hint, kpi_card, link_text, section_header, section_label,
    segmented_control, status_dot, status_pill, status_text, usage_bar, usage_cell,
    zebra_background, zebra_row,
};
