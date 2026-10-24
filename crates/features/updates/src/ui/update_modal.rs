use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use natikube_ui_kit::{ActiveTokens, Palette};

use crate::domain::{UpdateOffer, notes_excerpt};

/// Longest notes excerpt shown, in characters and in lines.
const EXCERPT_CHARS: usize = 360;
const EXCERPT_LINES: usize = 6;

/// What the user chose. The shell performs the effect; the modal only reports.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateEvent {
    /// Open this release page in the browser.
    Download(String),
    /// Close; ask again next launch. Escape and a click outside also send it.
    Later,
    /// Stop offering this version (and older ones). `persist` is false for a
    /// simulated offer (`--fake-update`), which must never reach the settings.
    Skip { version: String, persist: bool },
}

/// The dismissible "update available" overlay.
///
/// Contract: the shell creates one instance, renders it as the last child of
/// the window root (it paints nothing while hidden), calls
/// [`show`](Self::show) when a newer release was found and subscribes to its
/// [`UpdateEvent`]s. The modal closes itself before emitting.
pub struct UpdateModal {
    offer: Option<UpdateOffer>,
    excerpt: Vec<String>,
    focus: FocusHandle,
}

impl EventEmitter<UpdateEvent> for UpdateModal {}

impl UpdateModal {
    pub fn new(cx: &mut Context<Self>) -> Self {
        Self {
            offer: None,
            excerpt: Vec::new(),
            focus: cx.focus_handle(),
        }
    }

    /// Opens the modal for `offer` and takes the keyboard focus (Escape).
    pub fn show(&mut self, offer: UpdateOffer, window: &mut Window, cx: &mut Context<Self>) {
        self.excerpt = notes_excerpt(&offer.release.notes, EXCERPT_CHARS)
            .lines()
            .take(EXCERPT_LINES)
            .map(str::to_string)
            .collect();
        self.offer = Some(offer);
        self.focus.focus(window, cx);
        cx.notify();
    }

    pub fn is_open(&self) -> bool {
        self.offer.is_some()
    }

    /// Closes the modal and reports `event` built from the offer it showed.
    fn finish(&mut self, event: impl FnOnce(&UpdateOffer) -> UpdateEvent, cx: &mut Context<Self>) {
        if let Some(offer) = self.offer.take() {
            cx.emit(event(&offer));
        }
        cx.notify();
    }

    fn later(&mut self, cx: &mut Context<Self>) {
        self.finish(|_| UpdateEvent::Later, cx);
    }
}

/// A modal button: `primary` is the solid accent, otherwise a neutral control.
fn button(
    id: &'static str,
    label: &'static str,
    primary: bool,
    p: Palette,
    radius: Pixels,
) -> Stateful<Div> {
    let base = div()
        .id(id)
        .flex_none()
        .px_4()
        .h(px(30.))
        .flex()
        .items_center()
        .justify_center()
        .rounded(radius)
        .cursor_pointer()
        .text_sm();
    if primary {
        base.bg(p.accent)
            .text_color(p.on_accent)
            .font_weight(FontWeight::SEMIBOLD)
            .hover(move |s| s.bg(p.primary_hover))
            .active(move |s| s.bg(p.primary_active))
            .child(label)
    } else {
        base.bg(p.control)
            .border_1()
            .border_color(p.control_border)
            .text_color(p.text)
            .hover(move |s| s.bg(p.control_hover))
            .active(move |s| s.bg(p.control_active))
            .child(label)
    }
}

impl Render for UpdateModal {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(offer) = self.offer.clone() else {
            return div().into_any_element();
        };
        let tokens = cx.tokens().clone();
        let p = tokens.palette;
        let radius = tokens.radii.md;
        let release = &offer.release;
        let version = release.version.to_string();
        // A title that only repeats the tag adds nothing.
        let name = release
            .name
            .clone()
            .filter(|name| name != &release.tag && name != &version);
        let url = release.url.clone();
        let simulated = offer.simulated;

        let download = button("update-download", "Download", true, p, radius).on_click(
            cx.listener(move |this, _, _, cx| {
                let url = url.clone();
                this.finish(move |_| UpdateEvent::Download(url), cx);
            }),
        );
        let later = button("update-later", "Later", false, p, radius)
            .on_click(cx.listener(|this, _, _, cx| this.later(cx)));
        let skip = div()
            .id("update-skip")
            .cursor_pointer()
            .px_2()
            .h(px(30.))
            .flex()
            .items_center()
            .text_sm()
            .text_color(p.text_muted)
            .hover(move |s| s.text_color(p.text))
            .on_click(cx.listener(move |this, _, _, cx| {
                this.finish(
                    |offer| UpdateEvent::Skip {
                        version: offer.release.version.to_string(),
                        persist: !simulated,
                    },
                    cx,
                );
            }))
            .child("Skip this version");

        div()
            .id("update-backdrop")
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .bg(gpui_kit::black().opacity(0.45))
            .occlude()
            .track_focus(&self.focus)
            .capture_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
                if event.keystroke.key == "escape" {
                    this.later(cx);
                    cx.stop_propagation();
                }
            }))
            .on_click(cx.listener(|this, _, _, cx| this.later(cx)))
            .child(
                v_flex()
                    .id("update-panel")
                    .w(px(460.))
                    .gap_3()
                    .p_5()
                    .bg(p.panel)
                    .border_1()
                    .border_color(p.border)
                    .rounded(tokens.radii.lg)
                    .shadow_lg()
                    .text_color(p.text)
                    .font_family(tokens.fonts.ui.clone())
                    .text_size(tokens.font_size)
                    // A click inside the panel must not reach the backdrop.
                    .on_click(|_, _, cx| cx.stop_propagation())
                    .child(
                        div()
                            .flex()
                            .flex_none()
                            .self_start()
                            .px_2()
                            .py_0p5()
                            .rounded_full()
                            .bg(p.accent_soft)
                            .text_color(p.accent_soft_text)
                            .text_xs()
                            .child("Optional update"),
                    )
                    .child(
                        v_flex()
                            .gap_1()
                            .child(
                                div()
                                    .text_size(tokens.font_size * 1.4)
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(p.text_strong)
                                    .child(format!("NatiKube {version} is available")),
                            )
                            .child(
                                div()
                                    .text_color(p.text_muted)
                                    .child(format!("You have {}.", offer.current)),
                            ),
                    )
                    .when_some(name, |d, name| {
                        d.child(
                            div()
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(p.text_strong)
                                .child(name),
                        )
                    })
                    .when(!self.excerpt.is_empty(), |d| {
                        d.child(
                            v_flex()
                                .gap_1()
                                .p_3()
                                .max_h(px(160.))
                                .overflow_hidden()
                                .rounded(tokens.radii.sm)
                                .bg(p.surface)
                                .border_1()
                                .border_color(p.row_divider)
                                .text_sm()
                                .text_color(p.text_muted)
                                .children(
                                    self.excerpt.iter().cloned().map(|line| div().child(line)),
                                ),
                        )
                    })
                    .child(
                        h_flex()
                            .w_full()
                            .pt_1()
                            .items_center()
                            .justify_between()
                            .child(skip)
                            .child(h_flex().gap_2().child(later).child(download)),
                    ),
            )
            .into_any_element()
    }
}
