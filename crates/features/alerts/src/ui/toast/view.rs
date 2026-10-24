use std::sync::Arc;
use std::time::{Duration, Instant};

use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use natikube_ui_kit::ActiveTokens as _;

use crate::domain::{AlertStore, StoredAlert, ToastEntry, ToastQueue};
use crate::ui::bell::{BellEvent, BellTarget, subject};
use crate::ui::severity_color;

const TOAST_WIDTH: f32 = 340.0;
/// Distance from the window edge; the bottom one clears the status bar.
const EDGE_RIGHT: f32 = 16.0;
const EDGE_BOTTOM: f32 = 40.0;

/// Stack of toasts over the bottom right corner of the window.
///
/// Contract: the shell creates one with `cx.new(|_| ToastHost::new(store))`,
/// puts it as the last-but-overlay child of the window root, feeds it with
/// [`push`](Self::push) from the UI thread and subscribes to [`BellEvent`]
/// (a click opens the alert's object exactly like the bell does).
pub struct ToastHost {
    store: Arc<dyn AlertStore>,
    queue: ToastQueue,
    started: Instant,
    /// Wakes the host when the next toast expires; replaced on every change.
    _timer: Option<Task<()>>,
}

impl EventEmitter<BellEvent> for ToastHost {}

impl ToastHost {
    pub fn new(store: Arc<dyn AlertStore>) -> Self {
        Self {
            store,
            queue: ToastQueue::new(),
            started: Instant::now(),
            _timer: None,
        }
    }

    fn now_ms(&self) -> u64 {
        self.started.elapsed().as_millis() as u64
    }

    /// Shows a batch of fired alerts (oldest first).
    pub fn push(&mut self, alerts: &[StoredAlert], cx: &mut Context<Self>) {
        if alerts.is_empty() {
            return;
        }
        let now = self.now_ms();
        self.queue.push(alerts, now);
        self.arm_timer(cx);
        cx.notify();
    }

    /// Toasts on screen (tests and the shell's checks).
    pub fn visible(&self) -> usize {
        self.queue.items().len()
    }

    /// Sleeps until the next toast is due, then drops what expired. A newer
    /// call replaces (cancels) the previous sleep.
    fn arm_timer(&mut self, cx: &mut Context<Self>) {
        let Some(deadline) = self.queue.next_deadline() else {
            self._timer = None;
            return;
        };
        let wait = Duration::from_millis(deadline.saturating_sub(self.now_ms()));
        self._timer = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(wait).await;
            this.update(cx, |this, cx| {
                let now = this.now_ms();
                if this.queue.expire(now) {
                    cx.notify();
                }
                this.arm_timer(cx);
            })
            .ok();
        }));
    }

    fn dismiss(&mut self, id: u64, cx: &mut Context<Self>) {
        if self.queue.dismiss(id) {
            self.arm_timer(cx);
            cx.notify();
        }
    }

    fn set_hovered(&mut self, id: u64, hovered: bool, cx: &mut Context<Self>) {
        let now = self.now_ms();
        self.queue.set_hovered(id, hovered, now);
        self.arm_timer(cx);
    }

    /// A click on the toast: mark the alert read and open its object, like a
    /// click on the bell's history row.
    fn open(&mut self, id: u64, cx: &mut Context<Self>) {
        let Some(entry) = self.queue.items().iter().find(|t| t.id == id) else {
            return;
        };
        let target = BellTarget::from(&entry.alert);
        if let Err(error) = self.store.mark_read(entry.row_id) {
            tracing::warn!(%error, "could not mark the alert read");
        }
        self.dismiss(id, cx);
        cx.emit(BellEvent::OpenObject(target));
        cx.emit(BellEvent::Changed);
    }

    fn render_toast(&self, entry: &ToastEntry, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = cx.tokens().clone();
        let p = tokens.palette;
        let id = entry.id;
        let alert = &entry.alert;
        let color = severity_color(alert.severity, p);
        let place = {
            let cluster = alert.cluster.context_name().to_string();
            match subject(alert) {
                Some(subject) => format!("{cluster}  \u{b7}  {subject}"),
                None => cluster,
            }
        };

        h_flex()
            .id(("alert-toast", id))
            .occlude()
            .w_full()
            .items_stretch()
            .bg(p.panel)
            .border_1()
            .border_color(p.border)
            .rounded(tokens.radii.lg)
            .shadow_lg()
            .overflow_hidden()
            .cursor_pointer()
            .hover(move |d| d.bg(p.control_hover))
            .on_hover(
                cx.listener(move |this, hovered: &bool, _, cx| this.set_hovered(id, *hovered, cx)),
            )
            .on_click(cx.listener(move |this, _, _, cx| this.open(id, cx)))
            .child(div().flex_none().w(px(4.)).bg(color))
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .gap_0p5()
                    .px_3()
                    .py_2()
                    .child(
                        h_flex()
                            .w_full()
                            .gap_2()
                            .items_center()
                            .child(
                                div()
                                    .flex_none()
                                    .text_size(px(10.))
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(color)
                                    .child(alert.severity.as_str().to_uppercase()),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .truncate()
                                    .text_sm()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(p.text_strong)
                                    .child(alert.rule.title()),
                            )
                            .child(
                                div()
                                    .id(("alert-toast-close", id))
                                    .flex_none()
                                    .size(px(18.))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .rounded(tokens.radii.sm)
                                    .text_color(p.text_muted)
                                    .hover(move |d| d.bg(p.control_active).text_color(p.text))
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        cx.stop_propagation();
                                        this.dismiss(id, cx);
                                    }))
                                    .child("\u{d7}"),
                            ),
                    )
                    .child(div().truncate().text_xs().text_color(p.text).child(place))
                    .child(
                        div()
                            .truncate()
                            .text_xs()
                            .text_color(p.text_muted)
                            .child(alert.message.clone()),
                    ),
            )
    }
}

impl Render for ToastHost {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = cx.tokens().clone();
        let entries: Vec<ToastEntry> = self.queue.items().to_vec();
        div()
            .absolute()
            .right(px(EDGE_RIGHT))
            .bottom(px(EDGE_BOTTOM))
            .w(px(TOAST_WIDTH))
            .flex()
            .flex_col()
            .gap_2()
            .font_family(tokens.fonts.ui.clone())
            .text_size(tokens.font_size)
            .text_color(tokens.palette.text)
            .when(entries.is_empty(), |d| d.hidden())
            .children(
                entries
                    .iter()
                    .map(|entry| self.render_toast(entry, cx).into_any_element()),
            )
    }
}
