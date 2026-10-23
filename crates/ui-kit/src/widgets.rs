//! Small shared widgets. Each one is a plain function returning an element, so
//! feature crates can drop them into any layout.

use gpui_kit::component::{ActiveTheme as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use std::rc::Rc;

use crate::theme::Tone;
use crate::tokens::ActiveTokens as _;

/// A filled circle, e.g. connection health or status.
pub fn status_dot(tone: Tone, cx: &App) -> Div {
    div()
        .flex_none()
        .size(px(8.))
        .rounded_full()
        .bg(tone.color(cx))
}

/// Status text colored by tone (`Running` green, `CrashLoopBackOff` red).
pub fn status_text(label: impl Into<SharedString>, tone: Tone, cx: &App) -> Div {
    div().text_color(tone.color(cx)).child(label.into())
}

/// Clickable, link-styled text (Namespace and Controlled By cells).
pub fn link_text(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    cx: &App,
) -> Stateful<Div> {
    let theme = cx.theme();
    let (link, hover) = (theme.link, theme.link_hover);
    div()
        .id(id)
        .cursor_pointer()
        .text_color(link)
        .hover(move |s| s.text_color(hover).underline())
        .on_click(on_click)
        .child(label.into())
}

/// One small square per container, colored by state (the Containers column).
pub fn container_squares(states: &[Tone], cx: &App) -> Div {
    h_flex()
        .gap_0p5()
        .items_center()
        .children(states.iter().map(|tone| {
            div()
                .flex_none()
                .size(px(8.))
                .rounded(px(1.))
                .bg(tone.color(cx))
        }))
}

/// Uppercase caption used above groups of fields in detail panels.
pub fn section_header(title: impl Into<SharedString>, cx: &App) -> Div {
    let theme = cx.theme();
    div()
        .w_full()
        .pt_3()
        .pb_1()
        .text_xs()
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(theme.muted_foreground)
        .child(SharedString::from(title.into().to_uppercase()))
}

fn centered_message(
    title: impl Into<SharedString>,
    message: impl Into<SharedString>,
    title_color: Hsla,
    message_color: Hsla,
) -> Div {
    v_flex()
        .size_full()
        .items_center()
        .justify_center()
        .gap_1()
        .p_6()
        .child(
            div()
                .text_base()
                .font_weight(FontWeight::MEDIUM)
                .text_color(title_color)
                .child(title.into()),
        )
        .child(
            div()
                .max_w(px(520.))
                .text_center()
                .text_sm()
                .text_color(message_color)
                .child(message.into()),
        )
}

/// Neutral placeholder for views with nothing to show yet.
pub fn empty_state(
    title: impl Into<SharedString>,
    message: impl Into<SharedString>,
    cx: &App,
) -> Div {
    let theme = cx.theme();
    centered_message(title, message, theme.foreground, theme.muted_foreground)
}

/// Placeholder for a failed load; the title is red.
pub fn error_state(
    title: impl Into<SharedString>,
    message: impl Into<SharedString>,
    cx: &App,
) -> Div {
    let theme = cx.theme();
    centered_message(title, message, theme.red, theme.muted_foreground)
}

/// Rounded initials avatar for the cluster rail. `active` draws the accent
/// ring; `health` adds a small status dot in the bottom-right corner.
pub fn initials_avatar(
    id: impl Into<ElementId>,
    initials: impl Into<SharedString>,
    active: bool,
    health: Option<Tone>,
    cx: &App,
) -> Stateful<Div> {
    let theme = cx.theme();
    let ring = if active { theme.primary } else { theme.border };
    let bg = if active {
        theme.primary
    } else {
        theme.secondary
    };
    let fg = if active {
        theme.primary_foreground
    } else {
        theme.foreground
    };
    let hover = theme.secondary_hover;
    div()
        .id(id)
        .relative()
        .flex_none()
        .size(px(40.))
        .rounded(px(8.))
        .border_2()
        .border_color(ring)
        .bg(bg)
        .text_color(fg)
        .cursor_pointer()
        .flex()
        .items_center()
        .justify_center()
        .text_sm()
        .font_weight(FontWeight::SEMIBOLD)
        .when(!active, move |this| this.hover(move |s| s.bg(hover)))
        .child(initials.into())
        .when_some(health, |this, tone| {
            this.child(
                status_dot(tone, cx)
                    .absolute()
                    .bottom(px(-3.))
                    .right(px(-3.))
                    .border_2()
                    .border_color(theme.sidebar),
            )
        })
}

fn tone_pill_colors(tone: Tone, cx: &App) -> (Hsla, Hsla) {
    let p = &cx.tokens().palette;
    match tone {
        Tone::Ok => (p.ok_soft, p.ok_text),
        Tone::Warning => (p.warning_soft, p.warning_text),
        Tone::Error => (p.error_soft, p.error_text),
        Tone::Done | Tone::Muted => (p.done_soft, p.text_muted),
    }
}

/// Filled status pill with a leading dot (Command's Status column).
pub fn status_pill(label: impl Into<SharedString>, tone: Tone, cx: &App) -> Div {
    let (bg, fg) = tone_pill_colors(tone, cx);
    h_flex()
        .flex_none()
        .items_center()
        .gap_1p5()
        .px_2()
        .py_0p5()
        .rounded_full()
        .bg(bg)
        .text_xs()
        .text_color(fg)
        .child(
            div()
                .flex_none()
                .size(px(6.))
                .rounded_full()
                .bg(tone.color(cx)),
        )
        .child(label.into())
}

/// Thin horizontal usage bar. `fraction` is clamped to `0.0..=1.0` and a
/// non-zero value never renders narrower than 2% so it stays visible.
pub fn usage_bar(fraction: f32, cx: &App) -> Div {
    let p = &cx.tokens().palette;
    div()
        .w_full()
        .h(px(3.))
        .rounded(px(2.))
        .bg(p.bar_track)
        .child(
            div()
                .h_full()
                .w(relative(bar_fraction(fraction)))
                .rounded(px(2.))
                .bg(p.bar_fill),
        )
}

/// Clamps a usage fraction: NaN and negatives become 0, anything above 1 is 1,
/// and a positive value is lifted to at least 2%.
pub fn bar_fraction(fraction: f32) -> f32 {
    if fraction.is_nan() || fraction <= 0.0 {
        0.0
    } else {
        fraction.clamp(0.02, 1.0)
    }
}

/// A monospace value above a usage bar (Command's CPU / Memory cells).
pub fn usage_cell(value: impl Into<SharedString>, fraction: f32, cx: &App) -> Div {
    let tokens = cx.tokens();
    v_flex()
        .gap(px(5.))
        .w_full()
        .child(
            div()
                .font_family(tokens.fonts.mono.clone())
                .text_xs()
                .child(value.into()),
        )
        .child(usage_bar(fraction, cx))
}

/// Segmented control (Native's All / Running / Problems filter). Calls
/// `on_select` with the clicked segment's index.
pub fn segmented_control(
    id: impl Into<ElementId>,
    options: &[SharedString],
    selected: usize,
    on_select: impl Fn(usize, &mut Window, &mut App) + 'static,
    cx: &App,
) -> Stateful<Div> {
    let tokens = cx.tokens();
    let p = tokens.palette;
    let radii = tokens.radii;
    let on_select = Rc::new(on_select);
    let id: ElementId = id.into();
    h_flex()
        .id(id.clone())
        .flex_none()
        .p(px(2.))
        .gap_0p5()
        .rounded(radii.md)
        .bg(p.control)
        .children(options.iter().cloned().enumerate().map(|(index, label)| {
            let active = index == selected;
            let handler = on_select.clone();
            div()
                .id((id.clone(), SharedString::from(index.to_string())))
                .h(px(24.))
                .px_3()
                .flex()
                .items_center()
                .rounded(radii.sm)
                .cursor_pointer()
                .text_color(p.text)
                .when(active, |s| s.bg(p.surface).shadow_xs())
                .when(!active, |s| s.hover(move |h| h.bg(p.control_hover)))
                .on_click(move |_, window, cx| handler(index, window, cx))
                .child(label)
        }))
}

/// Filter chip (Command's filter pills, Observatory's section chips). The
/// caller attaches `.on_click(..)`.
pub fn filter_chip(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    active: bool,
    cx: &App,
) -> Stateful<Div> {
    let tokens = cx.tokens();
    let p = tokens.palette;
    let (bg, fg, border) = if active {
        (p.accent_soft, p.accent_soft_text, p.accent)
    } else {
        (p.control, p.text, p.control_border)
    };
    div()
        .id(id)
        .h(px(30.))
        .px_3()
        .flex()
        .items_center()
        .rounded(tokens.radii.lg.min(px(15.)))
        .border_1()
        .border_color(border)
        .bg(bg)
        .text_color(fg)
        .cursor_pointer()
        .when(!active, move |s| s.hover(move |h| h.bg(p.control_hover)))
        .child(label.into())
}

/// Dashed "+ Filter" style chip.
pub fn dashed_chip(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    cx: &App,
) -> Stateful<Div> {
    let tokens = cx.tokens();
    let p = tokens.palette;
    div()
        .id(id)
        .h(px(30.))
        .px_3()
        .flex()
        .items_center()
        .rounded(tokens.radii.lg.min(px(15.)))
        .border_1()
        .border_dashed()
        .border_color(p.control_border)
        .text_color(p.text_muted)
        .cursor_pointer()
        .hover(move |s| s.text_color(p.text))
        .child(label.into())
}

/// KPI card: caption, big value with an optional colored suffix, optional
/// usage bar (Observatory's header cards).
pub fn kpi_card(
    label: impl Into<SharedString>,
    value: impl Into<SharedString>,
    suffix: Option<(SharedString, Tone)>,
    bar: Option<f32>,
    cx: &App,
) -> Div {
    let tokens = cx.tokens();
    let p = tokens.palette;
    v_flex()
        .flex_1()
        .min_w_0()
        .gap_1p5()
        .px_4()
        .py_3()
        .rounded(tokens.radii.lg)
        .border_1()
        .border_color(p.border)
        .bg(p.surface)
        .child(div().text_xs().text_color(p.text_muted).child(label.into()))
        .child(
            h_flex()
                .items_baseline()
                .gap_1p5()
                .child(
                    div()
                        .text_size(px(24.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(p.text_strong)
                        .child(value.into()),
                )
                .when_some(suffix, |this, (text, tone)| {
                    let color = if matches!(tone, Tone::Ok) {
                        p.ok_text
                    } else {
                        tone.color(cx)
                    };
                    this.child(div().text_sm().text_color(color).child(text))
                }),
        )
        .when_some(bar, |this, fraction| {
            this.child(
                div()
                    .w_full()
                    .h(px(6.))
                    .rounded(px(3.))
                    .bg(p.bar_track)
                    .child(
                        div()
                            .h_full()
                            .w(relative(bar_fraction(fraction)))
                            .rounded(px(3.))
                            .bg(p.bar_fill),
                    ),
            )
        })
}

/// Keyboard hint chip such as `⌘K`.
pub fn kbd_hint(text: impl Into<SharedString>, cx: &App) -> Div {
    let tokens = cx.tokens();
    let p = tokens.palette;
    div()
        .flex_none()
        .px(px(6.))
        .py(px(1.))
        .rounded(tokens.radii.sm)
        .border_1()
        .border_color(p.kbd_border)
        .font_family(tokens.fonts.mono.clone())
        .text_size(px(11.))
        .text_color(p.text_muted)
        .child(text.into())
}

/// Small uppercase caption above a sidebar group (`CLUSTERS`, `WORKLOADS`).
pub fn section_label(title: impl Into<SharedString>, cx: &App) -> Div {
    div()
        .px_2()
        .pb_1p5()
        .text_size(px(11.))
        .font_weight(FontWeight::MEDIUM)
        .text_color(cx.tokens().palette.text_muted)
        .child(SharedString::from(title.into().to_uppercase()))
}

/// Background for table row `index` under the active tokens: the zebra color
/// on odd rows when `Tokens::zebra` is on, otherwise `None`.
pub fn zebra_background(index: usize, cx: &App) -> Option<Hsla> {
    let tokens = cx.tokens();
    (tokens.zebra && index % 2 == 1).then_some(tokens.palette.zebra)
}

/// A full-width row container of the active row height with zebra striping
/// applied. Append cells with `.child(..)`.
pub fn zebra_row(index: usize, cx: &App) -> Div {
    let tokens = cx.tokens();
    h_flex()
        .w_full()
        .h(tokens.row_height)
        .items_center()
        .when_some(zebra_background(index, cx), |this, bg| this.bg(bg))
}
