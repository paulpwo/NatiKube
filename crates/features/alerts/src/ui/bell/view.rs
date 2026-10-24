use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use natikube_ui_kit::ActiveTokens as _;

use super::view_model::{BellTarget, PAGE_SIZE, badge_text, group_by_day, relative_time, subject};
use crate::domain::{AlertStore, Severity, StoredAlert, Timestamp};

const BELL_SIZE: f32 = 28.0;
const PANEL_WIDTH: f32 = 400.0;
/// A press on the bell while the panel is open first reads as an outside
/// click (closing it); the click that follows must not reopen it.
const REOPEN_GUARD: Duration = Duration::from_millis(250);

/// What the bell asks the shell to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BellEvent {
    /// An alert was clicked: switch to its cluster and open its object.
    OpenObject(BellTarget),
    /// Read state or history changed (mark all read, clear, item opened);
    /// the shell refreshes the other bells.
    Changed,
}

/// Header bell: an icon button with an unread badge that opens the history
/// panel (newest first).
///
/// Contract: the shell creates one per header with
/// `cx.new(|_| AlertBell::new(store))`, subscribes to [`BellEvent`], and calls
/// [`refresh`](Self::refresh) on `AppCommand::AlertsChanged` (and whenever the
/// bell is first shown). Reads go straight to the store; the only writes are
/// `mark_read`, `mark_all_read` and `clear`.
pub struct AlertBell {
    store: Arc<dyn AlertStore>,
    items: Vec<StoredAlert>,
    unread: u32,
    total: u32,
    open: bool,
    confirm_clear: bool,
    error: Option<SharedString>,
    closed_at: Option<Instant>,
    side: PanelSide,
}

/// Where the history panel opens relative to the bell.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PanelSide {
    /// Under the bell, right edges aligned (bells in a header's right end).
    #[default]
    Below,
    /// Beside the bell, top edges aligned (bells in a left rail).
    Right,
}

impl EventEmitter<BellEvent> for AlertBell {}

impl AlertBell {
    pub fn new(store: Arc<dyn AlertStore>) -> Self {
        let mut bell = Self {
            store,
            items: Vec::new(),
            unread: 0,
            total: 0,
            open: false,
            confirm_clear: false,
            error: None,
            closed_at: None,
            side: PanelSide::Below,
        };
        bell.reload();
        bell
    }

    /// A bell whose panel opens to the right (for a left rail).
    pub fn in_rail(store: Arc<dyn AlertStore>) -> Self {
        let mut bell = Self::new(store);
        bell.side = PanelSide::Right;
        bell
    }

    /// Re-reads the unread count and, while the panel is open, the list.
    pub fn refresh(&mut self, cx: &mut Context<Self>) {
        self.reload();
        cx.notify();
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    pub fn unread(&self) -> u32 {
        self.unread
    }

    /// Opens the panel (menu item / palette).
    pub fn open_panel(&mut self, cx: &mut Context<Self>) {
        self.open = true;
        self.confirm_clear = false;
        self.reload();
        cx.notify();
    }

    pub fn close_panel(&mut self, cx: &mut Context<Self>) {
        if self.open {
            self.open = false;
            self.confirm_clear = false;
            self.closed_at = Some(Instant::now());
            cx.notify();
        }
    }

    fn reload(&mut self) {
        let unread = self.store.unread_count();
        let total = self.store.total_count();
        let items = if self.open {
            Some(self.store.list(PAGE_SIZE, 0))
        } else {
            None
        };
        let mut error = None;
        match unread {
            Ok(n) => self.unread = n,
            Err(e) => error = Some(e.to_string()),
        }
        match total {
            Ok(n) => self.total = n,
            Err(e) => error = Some(e.to_string()),
        }
        match items {
            Some(Ok(list)) => self.items = list,
            Some(Err(e)) => error = Some(e.to_string()),
            None => {}
        }
        self.error = error.map(SharedString::from);
    }

    fn toggle(&mut self, cx: &mut Context<Self>) {
        if self.open {
            self.close_panel(cx);
        } else if self.closed_at.is_some_and(|at| at.elapsed() < REOPEN_GUARD) {
            // The press that just closed the panel.
        } else {
            self.open_panel(cx);
        }
    }

    fn open_alert(&mut self, ix: usize, cx: &mut Context<Self>) {
        let Some(item) = self.items.get(ix) else {
            return;
        };
        let target = BellTarget::from(&item.alert);
        if !item.read
            && let Err(e) = self.store.mark_read(item.row_id)
        {
            self.error = Some(e.to_string().into());
        }
        self.close_panel(cx);
        self.reload();
        cx.emit(BellEvent::OpenObject(target));
        cx.emit(BellEvent::Changed);
        cx.notify();
    }

    fn mark_all_read(&mut self, cx: &mut Context<Self>) {
        if let Err(e) = self.store.mark_all_read() {
            self.error = Some(e.to_string().into());
        }
        self.reload();
        cx.emit(BellEvent::Changed);
        cx.notify();
    }

    fn clear(&mut self, cx: &mut Context<Self>) {
        self.confirm_clear = false;
        if let Err(e) = self.store.clear() {
            self.error = Some(e.to_string().into());
        }
        self.reload();
        cx.emit(BellEvent::Changed);
        cx.notify();
    }
}

fn unix_now() -> Timestamp {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as Timestamp)
        .unwrap_or(0)
}

fn local_offset_secs() -> i64 {
    use chrono::Offset as _;
    i64::from(chrono::Local::now().offset().fix().local_minus_utc())
}

/// Paints a bell glyph filling `bounds` (no icon assets are registered, so it
/// is drawn as a path in the given color).
fn paint_bell(bounds: Bounds<Pixels>, color: Hsla, window: &mut Window) {
    let unit = f32::from(bounds.size.width).min(f32::from(bounds.size.height)) / 24.0;
    let at = |x: f32, y: f32| {
        point(
            bounds.origin.x + px(x * unit),
            bounds.origin.y + px(y * unit),
        )
    };
    let arc = |cx: f32, cy: f32, r: f32, from: f32, to: f32| -> Vec<Point<Pixels>> {
        (0..=12)
            .map(|i| {
                let t = from + (to - from) * i as f32 / 12.0;
                at(cx + r * t.cos(), cy + r * t.sin())
            })
            .collect()
    };
    let mut body = arc(12.0, 10.0, 7.0, std::f32::consts::PI, std::f32::consts::TAU);
    body.extend([
        at(19.0, 16.0),
        at(21.0, 17.5),
        at(21.0, 19.0),
        at(3.0, 19.0),
        at(3.0, 17.5),
        at(5.0, 16.0),
    ]);
    let clapper = arc(12.0, 19.5, 2.5, 0.0, std::f32::consts::PI);
    for shape in [body, clapper] {
        let mut builder = PathBuilder::fill();
        builder.add_polygon(&shape, true);
        if let Ok(path) = builder.build() {
            window.paint_path(path, color);
        }
    }
}

impl Render for AlertBell {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = cx.tokens().clone();
        let p = tokens.palette;
        let open = self.open;
        let side = self.side;
        let glyph_color = if open || self.unread > 0 {
            p.text_strong
        } else {
            p.text_muted
        };

        let mut badge = None;
        if let Some(text) = badge_text(self.unread) {
            badge = Some(
                div()
                    .absolute()
                    .top(px(1.))
                    .right(px(0.))
                    .min_w(px(15.))
                    .h(px(15.))
                    .px(px(4.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded_full()
                    .bg(p.error)
                    .text_color(gpui_kit::white())
                    .text_size(px(9.))
                    .font_weight(FontWeight::BOLD)
                    .child(text),
            );
        }

        let button = div()
            .id("alert-bell")
            .relative()
            .flex_none()
            .size(px(BELL_SIZE))
            .flex()
            .items_center()
            .justify_center()
            .rounded(tokens.radii.sm)
            .cursor_pointer()
            .when(open, |d| d.bg(p.control_active))
            .when(!open, |d| d.hover(move |d| d.bg(p.control_hover)))
            .on_click(cx.listener(|this, _, _, cx| this.toggle(cx)))
            .child(
                canvas(
                    |_, _, _| (),
                    move |bounds, _, window, _| paint_bell(bounds, glyph_color, window),
                )
                .size(px(16.)),
            )
            .children(badge);

        let panel = open.then(|| self.render_panel(cx).into_any_element());

        div()
            .relative()
            .flex_none()
            .child(button)
            .children(panel.map(|panel| {
                deferred(
                    anchored()
                        .anchor(match side {
                            PanelSide::Below => Anchor::TopRight,
                            PanelSide::Right => Anchor::TopLeft,
                        })
                        .offset(match side {
                            PanelSide::Below => point(px(BELL_SIZE), px(BELL_SIZE + 4.)),
                            PanelSide::Right => point(px(BELL_SIZE + 8.), px(0.)),
                        })
                        .snap_to_window_with_margin(px(8.))
                        .child(panel),
                )
                .with_priority(10)
            }))
    }
}

impl AlertBell {
    fn render_panel(&mut self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let tokens = cx.tokens().clone();
        let p = tokens.palette;
        let now = unix_now();
        let offset = local_offset_secs();
        let groups = group_by_day(&self.items, now, offset);

        let mut list = v_flex().w_full().pb_1();
        for group in &groups {
            list = list.child(
                div()
                    .w_full()
                    .px_3()
                    .pt_2()
                    .pb_1()
                    .text_xs()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(p.text_muted)
                    .child(SharedString::from(group.bucket.label().to_uppercase())),
            );
            for ix in group.start..group.end {
                list = list.child(self.render_row(ix, now, cx));
            }
        }
        if self.total as usize > self.items.len() && !self.items.is_empty() {
            list = list.child(
                div()
                    .w_full()
                    .px_3()
                    .py_2()
                    .text_xs()
                    .text_color(p.text_muted)
                    .child(format!(
                        "Showing the latest {} of {}",
                        self.items.len(),
                        self.total
                    )),
            );
        }

        let body = if self.items.is_empty() {
            v_flex()
                .w_full()
                .h(px(160.))
                .items_center()
                .justify_center()
                .gap_1()
                .child(
                    div()
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(p.text)
                        .child("No notifications yet"),
                )
                .child(
                    div()
                        .px_6()
                        .text_center()
                        .text_xs()
                        .text_color(p.text_muted)
                        .child("Turn on alerts in Settings and they will show up here."),
                )
                .into_any_element()
        } else {
            div()
                .id("alert-list")
                .w_full()
                .max_h(px(420.))
                .overflow_y_scroll()
                .child(list)
                .into_any_element()
        };

        let mut actions = h_flex().gap_1().items_center();
        if self.confirm_clear {
            actions = actions
                .child(
                    div()
                        .text_xs()
                        .text_color(p.text_muted)
                        .child("Delete all history?"),
                )
                .child(
                    Button::new("alert-clear-yes")
                        .small()
                        .danger()
                        .label("Clear")
                        .on_click(cx.listener(|this, _, _, cx| this.clear(cx))),
                )
                .child(
                    Button::new("alert-clear-no")
                        .small()
                        .ghost()
                        .label("Cancel")
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.confirm_clear = false;
                            cx.notify();
                        })),
                );
        } else {
            if self.unread > 0 {
                actions = actions.child(
                    Button::new("alert-mark-all")
                        .small()
                        .ghost()
                        .label("Mark all read")
                        .on_click(cx.listener(|this, _, _, cx| this.mark_all_read(cx))),
                );
            }
            if self.total > 0 {
                actions = actions.child(
                    Button::new("alert-clear")
                        .small()
                        .ghost()
                        .label("Clear")
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.confirm_clear = true;
                            cx.notify();
                        })),
                );
            }
        }

        let header = h_flex()
            .w_full()
            .flex_none()
            .px_3()
            .py_2()
            .gap_2()
            .items_center()
            .justify_between()
            .border_b_1()
            .border_color(p.border)
            .child(
                h_flex()
                    .gap_2()
                    .items_center()
                    .child(
                        div()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(p.text_strong)
                            .child("Notifications"),
                    )
                    .when(self.unread > 0, |d| {
                        d.child(
                            div()
                                .text_xs()
                                .text_color(p.text_muted)
                                .child(format!("{} unread", self.unread)),
                        )
                    }),
            )
            .child(actions);

        div()
            .id("alert-panel")
            .occlude()
            .w(px(PANEL_WIDTH))
            .flex()
            .flex_col()
            .bg(p.panel)
            .border_1()
            .border_color(p.border)
            .rounded(tokens.radii.lg)
            .shadow_lg()
            .overflow_hidden()
            .font_family(tokens.fonts.ui.clone())
            .text_size(tokens.font_size)
            .text_color(p.text)
            .on_mouse_down_out(cx.listener(|this, _, _, cx| this.close_panel(cx)))
            .child(header)
            .children(self.error.clone().map(|e| {
                div()
                    .w_full()
                    .px_3()
                    .py_1()
                    .text_xs()
                    .text_color(p.error_text)
                    .bg(p.error_soft)
                    .child(e)
            }))
            .child(body)
    }

    fn render_row(&self, ix: usize, now: Timestamp, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = cx.tokens().clone();
        let p = tokens.palette;
        let item = &self.items[ix];
        let alert = &item.alert;
        let unread = !item.read;
        let dot = match alert.severity {
            Severity::Info => p.accent,
            Severity::Warning => p.warning,
            Severity::Critical => p.error,
        };
        let place = {
            let cluster = alert.cluster.context_name().to_string();
            match subject(alert) {
                Some(subject) => format!("{cluster}  \u{b7}  {subject}"),
                None => cluster,
            }
        };

        div()
            .id(("alert-row", ix))
            .w_full()
            .flex()
            .flex_row()
            .items_start()
            .gap_2()
            .px_3()
            .py_2()
            .cursor_pointer()
            .border_b_1()
            .border_color(p.row_divider)
            .when(unread, |d| d.bg(p.accent_soft))
            .hover(move |d| d.bg(p.control_hover))
            .on_click(cx.listener(move |this, _, _, cx| this.open_alert(ix, cx)))
            .child(
                div()
                    .flex_none()
                    .mt(px(5.))
                    .size(px(8.))
                    .rounded_full()
                    .bg(dot),
            )
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .gap_0p5()
                    .child(
                        h_flex()
                            .w_full()
                            .gap_2()
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .truncate()
                                    .font_weight(if unread {
                                        FontWeight::SEMIBOLD
                                    } else {
                                        FontWeight::NORMAL
                                    })
                                    .text_color(if unread { p.text_strong } else { p.text })
                                    .child(alert.rule.title()),
                            )
                            .child(
                                div()
                                    .flex_none()
                                    .text_xs()
                                    .text_color(p.text_muted)
                                    .child(relative_time(now, alert.timestamp)),
                            ),
                    )
                    .child(
                        div()
                            .truncate()
                            .text_xs()
                            .text_color(p.text_muted)
                            .child(place),
                    )
                    .child(
                        div()
                            .truncate()
                            .text_xs()
                            .text_color(if unread { p.text } else { p.text_muted })
                            .child(alert.message.clone()),
                    ),
            )
    }
}
