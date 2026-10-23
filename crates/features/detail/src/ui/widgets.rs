//! Small layout helpers local to the detail panel.

use gpui_kit::component::{ActiveTheme as _, h_flex, v_flex};
use gpui_kit::*;
use natikube_ui_kit::Tone;

use crate::domain::model::Health;

pub(super) fn tone(health: Health) -> Tone {
    match health {
        Health::Ok => Tone::Ok,
        Health::Done => Tone::Done,
        Health::Warning => Tone::Warning,
        Health::Error => Tone::Error,
        Health::Muted => Tone::Muted,
    }
}

/// A label/value row of the properties list.
pub(super) fn prop_row(label: impl Into<SharedString>, value: impl IntoElement, cx: &App) -> Div {
    h_flex()
        .w_full()
        .gap_3()
        .py_0p5()
        .items_start()
        .child(
            div()
                .w(px(116.))
                .flex_none()
                .text_color(cx.theme().muted_foreground)
                .child(label.into()),
        )
        .child(div().flex_1().min_w_0().child(value))
}

/// A rounded pill (labels, annotations).
pub(super) fn chip(text: impl Into<SharedString>, cx: &App) -> Div {
    let theme = cx.theme();
    div()
        .px_2()
        .py_0p5()
        .rounded(px(4.))
        .bg(theme.secondary)
        .text_xs()
        .text_color(theme.foreground)
        .max_w_full()
        .overflow_hidden()
        .child(text.into())
}

/// A bordered block (container, event, table).
pub(super) fn card(cx: &App) -> Div {
    let theme = cx.theme();
    v_flex()
        .w_full()
        .gap_1()
        .p_2()
        .rounded(px(6.))
        .border_1()
        .border_color(theme.border)
        .bg(theme.background)
}

pub(super) fn wrap_row() -> Div {
    h_flex().w_full().flex_wrap().gap_1()
}

/// Truncates long values for chips so one annotation cannot fill the panel.
pub(super) fn clip(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        text.to_string()
    } else {
        let head: String = text.chars().take(max).collect();
        format!("{head}...")
    }
}

#[cfg(test)]
mod tests {
    use super::clip;

    #[test]
    fn clip_keeps_short_text_and_marks_long_text() {
        assert_eq!(clip("abc", 5), "abc");
        assert_eq!(clip("abcdef", 3), "abc...");
    }
}
