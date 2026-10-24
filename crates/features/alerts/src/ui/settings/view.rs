use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::{Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use natikube_core::{AlertsConfig, ClusterId, ParamKind, Retention, RuleKind};
use natikube_ui_kit::{ActiveTokens, Palette};

use super::edit::{
    AlertsEdit, COUNT_RANGE, DURATION_SECS_RANGE, WINDOW_SECS_RANGE, apply_edit, duration_step,
    format_duration, format_list, namespaces_text, step,
};
use crate::domain::{PermissionState, rule_severity};
use crate::ui::severity_color;

/// A cluster the user can configure, as plain data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AlertClusterEntry {
    pub id: ClusterId,
    pub name: String,
}

/// What the section asks the shell to do.
#[derive(Debug, Clone, PartialEq)]
pub enum AlertsSettingsEvent {
    /// The configuration changed; the shell clamps, persists and applies it.
    ConfigChanged(Box<AlertsConfig>),
    SendTestNotification,
    /// "Open System Settings" next to a denied permission.
    OpenNotificationSettings,
    /// "Preview" next to the sound picker: play this system sound once.
    PreviewSound(String),
    ClearHistory,
}

/// Rows of rules per cluster, the OS switch, retention and the test button.
pub struct AlertsSettingsSection {
    config: AlertsConfig,
    clusters: Vec<AlertClusterEntry>,
    selected: usize,
    /// What the OS lets NatiKube do; the shell keeps it current.
    permission: PermissionState,
    namespaces: Entity<InputState>,
    reasons: Entity<InputState>,
    /// Bumped on every typed edit so only the last one is applied.
    typing_generation: u64,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<AlertsSettingsEvent> for AlertsSettingsSection {}

const TEXT_DEBOUNCE: std::time::Duration = std::time::Duration::from_millis(600);

impl AlertsSettingsSection {
    pub fn new(
        config: AlertsConfig,
        clusters: Vec<AlertClusterEntry>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let namespaces = cx.new(|cx| InputState::new(window, cx).placeholder("All namespaces"));
        let reasons = cx.new(|cx| InputState::new(window, cx).placeholder("All reasons"));
        let mut subscriptions = Vec::new();
        for input in [&namespaces, &reasons] {
            subscriptions.push(cx.subscribe_in(
                input,
                window,
                |this: &mut Self, _, event: &InputEvent, _, cx| match event {
                    InputEvent::Change => this.apply_typed_later(cx),
                    InputEvent::Blur | InputEvent::PressEnter { .. } => this.apply_typed(cx),
                    InputEvent::Focus => {}
                },
            ));
        }
        let mut this = Self {
            config,
            clusters,
            selected: 0,
            permission: PermissionState::NotApplicable,
            namespaces,
            reasons,
            typing_generation: 0,
            _subscriptions: subscriptions,
        };
        this.load_inputs(window, cx);
        this
    }

    pub fn config(&self) -> &AlertsConfig {
        &self.config
    }

    /// The shell's configuration changed outside this view.
    pub fn set_config(
        &mut self,
        config: AlertsConfig,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.config != config {
            self.config = config;
            self.load_inputs(window, cx);
            cx.notify();
        }
    }

    /// The OS notification permission changed (or was just read).
    pub fn set_permission(&mut self, permission: PermissionState, cx: &mut Context<Self>) {
        if self.permission != permission {
            self.permission = permission;
            cx.notify();
        }
    }

    pub fn permission(&self) -> &PermissionState {
        &self.permission
    }

    /// The cluster catalog changed. The selection stays on the same cluster
    /// when it still exists.
    pub fn set_clusters(
        &mut self,
        clusters: Vec<AlertClusterEntry>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.clusters == clusters {
            return;
        }
        let current = self.current().map(|c| c.id.clone());
        self.clusters = clusters;
        self.selected = current
            .and_then(|id| self.clusters.iter().position(|c| c.id == id))
            .unwrap_or(0);
        self.load_inputs(window, cx);
        cx.notify();
    }

    /// Applies any typed text now; the shell calls it before closing the window.
    pub fn flush_pending(&mut self, cx: &mut Context<Self>) {
        self.apply_typed(cx);
    }

    fn current(&self) -> Option<&AlertClusterEntry> {
        self.clusters.get(self.selected)
    }

    fn load_inputs(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (namespaces, reasons) = match self.current() {
            Some(entry) => {
                let cluster = self.config.cluster(&entry.id);
                (
                    namespaces_text(&cluster.namespaces),
                    format_list(&cluster.params(RuleKind::WarningEvents).reasons),
                )
            }
            None => (String::new(), String::new()),
        };
        self.namespaces
            .update(cx, |input, cx| input.set_value(namespaces, window, cx));
        self.reasons
            .update(cx, |input, cx| input.set_value(reasons, window, cx));
    }

    fn select(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        if index == self.selected || index >= self.clusters.len() {
            return;
        }
        self.apply_typed(cx);
        self.selected = index;
        self.load_inputs(window, cx);
        cx.notify();
    }

    fn edit(&mut self, edit: AlertsEdit, cx: &mut Context<Self>) {
        let next = apply_edit(&self.config, edit);
        if next == self.config {
            return;
        }
        self.config = next.clone();
        cx.emit(AlertsSettingsEvent::ConfigChanged(Box::new(next)));
        cx.notify();
    }

    /// Applies the namespaces and reasons inputs to the selected cluster.
    /// Edits that change nothing emit nothing.
    fn apply_typed(&mut self, cx: &mut Context<Self>) {
        self.typing_generation += 1;
        let Some(id) = self.current().map(|c| c.id.clone()) else {
            return;
        };
        let namespaces = self.namespaces.read(cx).value().to_string();
        let reasons = self.reasons.read(cx).value().to_string();
        self.edit(AlertsEdit::Namespaces(id.clone(), namespaces), cx);
        self.edit(AlertsEdit::Reasons(id, reasons), cx);
    }

    fn apply_typed_later(&mut self, cx: &mut Context<Self>) {
        self.typing_generation += 1;
        let generation = self.typing_generation;
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(TEXT_DEBOUNCE).await;
            this.update(cx, |this, cx| {
                if this.typing_generation == generation {
                    this.apply_typed(cx);
                }
            })
            .ok();
        })
        .detach();
    }
}

// ---- building blocks -------------------------------------------------------

fn button(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    p: Palette,
    radius: Pixels,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Stateful<Div> {
    div()
        .id(id)
        .flex_none()
        .h(px(26.))
        .px_3()
        .flex()
        .items_center()
        .rounded(radius)
        .border_1()
        .border_color(p.control_border)
        .bg(p.control)
        .text_color(p.text)
        .text_sm()
        .cursor_pointer()
        .hover(move |s| s.bg(p.control_hover))
        .active(move |s| s.bg(p.control_active))
        .on_click(on_click)
        .child(label.into())
}

fn switch(
    id: impl Into<ElementId>,
    on: bool,
    p: Palette,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Stateful<Div> {
    div()
        .id(id)
        .flex_none()
        .relative()
        .w(px(36.))
        .h(px(20.))
        .rounded_full()
        .cursor_pointer()
        .bg(if on { p.accent } else { p.control_border })
        .on_click(on_click)
        .child(
            div()
                .absolute()
                .top(px(2.))
                .left(if on { px(18.) } else { px(2.) })
                .size(px(16.))
                .rounded_full()
                .bg(if on { p.on_accent } else { p.surface })
                .shadow_xs(),
        )
}

fn row(
    title: impl IntoElement,
    help: Option<SharedString>,
    control: impl IntoElement,
    p: Palette,
) -> Div {
    h_flex()
        .w_full()
        .min_h(px(40.))
        .py_2()
        .gap_4()
        .items_center()
        .justify_between()
        .border_b_1()
        .border_color(p.row_divider)
        .child(
            v_flex()
                .flex_1()
                .min_w_0()
                .gap_0p5()
                .child(div().text_sm().text_color(p.text_strong).child(title))
                .when_some(help, |d, help| {
                    d.child(div().text_xs().text_color(p.text_muted).child(help))
                }),
        )
        .child(div().flex_none().child(control))
}

fn heading(title: &'static str, p: Palette) -> Div {
    div()
        .pt_4()
        .pb_1()
        .text_sm()
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(p.text_strong)
        .child(title)
}

impl AlertsSettingsSection {
    fn stepper(
        &self,
        id: impl Into<SharedString>,
        value: String,
        dec: AlertsEdit,
        inc: AlertsEdit,
        cx: &mut Context<Self>,
    ) -> Div {
        let tokens = cx.tokens().clone();
        let (p, radius) = (tokens.palette, tokens.radii.sm);
        let id: SharedString = id.into();
        let square =
            |side: usize, label: &'static str, edit: AlertsEdit, cx: &mut Context<Self>| {
                div()
                    .id(SharedString::from(format!("{id}-{side}")))
                    .size(px(26.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(radius)
                    .border_1()
                    .border_color(p.control_border)
                    .bg(p.control)
                    .text_color(p.text)
                    .cursor_pointer()
                    .hover(move |s| s.bg(p.control_hover))
                    .active(move |s| s.bg(p.control_active))
                    .on_click(cx.listener(move |this, _, _, cx| this.edit(edit.clone(), cx)))
                    .child(label)
            };
        h_flex()
            .gap_1p5()
            .items_center()
            .child(square(0, "\u{2212}", dec, cx))
            .child(
                div()
                    .min_w(px(64.))
                    .text_center()
                    .text_sm()
                    .text_color(p.text)
                    .child(value),
            )
            .child(square(1, "+", inc, cx))
    }

    fn render_clusters(&self, cx: &mut Context<Self>) -> Div {
        let tokens = cx.tokens().clone();
        let (p, radius) = (tokens.palette, tokens.radii.sm);
        if self.clusters.is_empty() {
            return div()
                .py_3()
                .text_sm()
                .text_color(p.text_muted)
                .child("No clusters found. Add a kubeconfig in the Clusters section first.");
        }
        h_flex()
            .gap_1()
            .flex_wrap()
            .children(self.clusters.iter().enumerate().map(|(index, entry)| {
                let selected = index == self.selected;
                let watching = self.config.cluster(&entry.id).any_enabled();
                h_flex()
                    .id(("alerts-cluster", index))
                    .h(px(28.))
                    .px_3()
                    .gap_1p5()
                    .items_center()
                    .rounded(radius)
                    .cursor_pointer()
                    .text_sm()
                    .when(selected, |d| {
                        d.bg(p.sidebar_active).text_color(p.sidebar_active_text)
                    })
                    .when(!selected, |d| {
                        d.text_color(p.text).hover(move |s| s.bg(p.control_hover))
                    })
                    .on_click(
                        cx.listener(move |this, _, window, cx| this.select(index, window, cx)),
                    )
                    .when(watching, |d| {
                        d.child(div().size(px(6.)).rounded_full().bg(p.ok))
                    })
                    .child(entry.name.clone())
            }))
    }

    /// The system sounds as selectable chips (clicking one only selects it;
    /// "Preview" plays it).
    fn render_sound_choices(&self, cx: &mut Context<Self>) -> Div {
        let tokens = cx.tokens().clone();
        let (p, radius) = (tokens.palette, tokens.radii.sm);
        h_flex().w_full().gap_1().flex_wrap().py_2().children(
            AlertsConfig::SYSTEM_SOUNDS
                .iter()
                .enumerate()
                .map(|(index, name)| {
                    let selected = self.config.sound_name == *name;
                    div()
                        .id(("alerts-sound-choice", index))
                        .h(px(26.))
                        .px_3()
                        .flex()
                        .items_center()
                        .rounded(radius)
                        .cursor_pointer()
                        .text_sm()
                        .when(selected, |d| {
                            d.bg(p.sidebar_active).text_color(p.sidebar_active_text)
                        })
                        .when(!selected, |d| {
                            d.text_color(p.text).hover(move |s| s.bg(p.control_hover))
                        })
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.edit(AlertsEdit::SoundName((*name).to_string()), cx)
                        }))
                        .child(*name)
                }),
        )
    }

    fn render_rules(&self, entry: &AlertClusterEntry, cx: &mut Context<Self>) -> Div {
        let p = cx.tokens().palette;
        let cluster = self.config.cluster(&entry.id);
        let mut list = v_flex().w_full();
        for (index, kind) in RuleKind::ALL.into_iter().enumerate() {
            let on = cluster.is_enabled(kind);
            let id = entry.id.clone();
            let toggle = switch(
                ("alerts-rule", index),
                on,
                p,
                cx.listener(move |this, _, _, cx| {
                    this.edit(AlertsEdit::RuleEnabled(id.clone(), kind, !on), cx)
                }),
            );
            let title = h_flex()
                .gap_2()
                .items_center()
                .child(
                    div()
                        .size(px(8.))
                        .rounded_full()
                        .bg(severity_color(rule_severity(kind), p)),
                )
                .child(kind.title());
            list = list.child(row(title, Some(kind.description().into()), toggle, p));
            if on {
                for param in kind.params() {
                    list = list.child(self.render_param(&entry.id, kind, *param, cx));
                }
            }
        }
        list
    }

    fn render_param(
        &self,
        cluster: &ClusterId,
        kind: RuleKind,
        param: ParamKind,
        cx: &mut Context<Self>,
    ) -> Div {
        let p = cx.tokens().palette;
        let params = self.config.cluster(cluster).params(kind);
        let indent = |d: Div| d.pl_6();
        match param {
            ParamKind::RestartCount => {
                let value = params.count;
                indent(row(
                    "Restarts",
                    Some("Alert when a container restarts this many times within the window.".into()),
                    self.stepper(
                        format!("alerts-count-{}", cluster),
                        value.to_string(),
                        AlertsEdit::RestartCount(cluster.clone(), step(value, -1, &COUNT_RANGE)),
                        AlertsEdit::RestartCount(cluster.clone(), step(value, 1, &COUNT_RANGE)),
                        cx,
                    ),
                    p,
                ))
            }
            ParamKind::WindowSecs => {
                let value = params.window_secs;
                indent(row(
                    "Window",
                    None,
                    self.stepper(
                        format!("alerts-window-{}", cluster),
                        format_duration(value),
                        AlertsEdit::WindowSecs(cluster.clone(), step(value, -60, &WINDOW_SECS_RANGE)),
                        AlertsEdit::WindowSecs(cluster.clone(), step(value, 60, &WINDOW_SECS_RANGE)),
                        cx,
                    ),
                    p,
                ))
            }
            ParamKind::DurationSecs => {
                let value = params.duration_secs;
                let down = -duration_step(value.saturating_sub(1));
                let up = duration_step(value);
                indent(row(
                    "Lasts longer than",
                    None,
                    self.stepper(
                        format!("alerts-duration-{}-{}", kind.key(), cluster),
                        format_duration(value),
                        AlertsEdit::DurationSecs(cluster.clone(), kind, step(value, down, &DURATION_SECS_RANGE)),
                        AlertsEdit::DurationSecs(cluster.clone(), kind, step(value, up, &DURATION_SECS_RANGE)),
                        cx,
                    ),
                    p,
                ))
            }
            ParamKind::Reasons => indent(row(
                "Only these reasons",
                Some("Comma separated, for example BackOff, FailedMount. Empty matches every Warning event.".into()),
                div().w(px(240.)).child(Input::new(&self.reasons).small()),
                p,
            )),
        }
    }
}

impl Render for AlertsSettingsSection {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = cx.tokens().clone();
        let (p, radius) = (tokens.palette, tokens.radii.sm);

        let mut body = v_flex().w_full().gap_1().child(
            div()
                .pb_2()
                .text_xl()
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(p.text_strong)
                .child("Alerts"),
        );
        body = body.child(div().pb_2().text_sm().text_color(p.text_muted).child(
            "Choose what NatiKube watches. Nothing is monitored until you turn a rule on, \
                     and only while NatiKube is open. Every alert is kept in the bell history.",
        ));
        body = body.child(self.render_clusters(cx));

        if let Some(entry) = self.current().cloned() {
            let cluster = self.config.cluster(&entry.id);
            body = body.child(heading("Rules", p)).child(
                div()
                    .text_xs()
                    .text_color(p.text_muted)
                    .child(format!("Settings for {}.", entry.name)),
            );
            body = body.child(self.render_rules(&entry, cx));
            if RuleKind::ALL
                .into_iter()
                .any(|k| k.is_namespaced() && cluster.is_enabled(k))
            {
                body = body.child(row(
                    "Namespaces",
                    Some("Pod, deployment and event rules only look at these. Empty watches all namespaces.".into()),
                    div().w(px(240.)).child(Input::new(&self.namespaces).small()),
                    p,
                ));
            }
        }

        let os_on = self.config.os_notifications;
        let permission_row = self.permission.status_text().map(|line| {
            let open = self.permission.offers_system_settings().then(|| {
                button(
                    "alerts-open-system-settings",
                    "Open System Settings",
                    p,
                    radius,
                    cx.listener(|_, _, _, cx| {
                        cx.emit(AlertsSettingsEvent::OpenNotificationSettings)
                    }),
                )
            });
            row(
                "System permission",
                Some(line.into()),
                div().when_some(open, |d, button| d.child(button)),
                p,
            )
        });
        let toasts_on = self.config.in_app_toasts;
        let sound_on = self.config.sound;
        let critical_only = self.config.sound_critical_only;
        let sound_name = self.config.sound_name.clone();
        let preview_name = sound_name.clone();
        body = body
            .child(heading("In-app notifications", p))
            .child(row(
                "Show toasts in the window",
                Some(
                    "On by default. A short card in the corner of the window when an alert \
                     fires; click it to open the object. Alerts still need a rule turned on."
                        .into(),
                ),
                switch(
                    "alerts-toasts",
                    toasts_on,
                    p,
                    cx.listener(move |this, _, _, cx| {
                        this.edit(AlertsEdit::InAppToasts(!toasts_on), cx)
                    }),
                ),
                p,
            ))
            .child(heading("Sound", p))
            .child(row(
                "Play a sound",
                Some(
                    "Off by default. Plays once per batch of alerts, and never more than once \
                     every 2 seconds."
                        .into(),
                ),
                switch(
                    "alerts-sound",
                    sound_on,
                    p,
                    cx.listener(move |this, _, _, cx| this.edit(AlertsEdit::Sound(!sound_on), cx)),
                ),
                p,
            ))
            .child(
                div().w_full().when(!sound_on, |d| d.opacity(0.55)).child(
                    v_flex()
                        .w_full()
                        .child(row(
                            "Only critical alerts",
                            Some("Stay silent for information and warnings.".into()),
                            switch(
                                "alerts-sound-critical",
                                critical_only,
                                p,
                                cx.listener(move |this, _, _, cx| {
                                    this.edit(AlertsEdit::SoundCriticalOnly(!critical_only), cx)
                                }),
                            ),
                            p,
                        ))
                        .child(row(
                            "Sound",
                            Some(format!("Selected: {sound_name}").into()),
                            button(
                                "alerts-sound-preview",
                                "Preview",
                                p,
                                radius,
                                cx.listener(move |_, _, _, cx| {
                                    cx.emit(AlertsSettingsEvent::PreviewSound(preview_name.clone()))
                                }),
                            ),
                            p,
                        ))
                        .child(self.render_sound_choices(cx)),
                ),
            )
            .child(heading("Operating system notifications", p))
            .child(row(
                "Show system notifications",
                Some(
                    "Off by default. When on, new alerts also appear as native notifications. \
                     The bell history works either way."
                        .into(),
                ),
                switch(
                    "alerts-os",
                    os_on,
                    p,
                    cx.listener(move |this, _, _, cx| {
                        this.edit(AlertsEdit::OsNotifications(!os_on), cx)
                    }),
                ),
                p,
            ))
            .children(permission_row)
            .child(row(
                "Test notification",
                Some("Sends one system notification now, even while the switch is off.".into()),
                button(
                    "alerts-test",
                    "Send test notification",
                    p,
                    radius,
                    cx.listener(|_, _, _, cx| cx.emit(AlertsSettingsEvent::SendTestNotification)),
                ),
                p,
            ));

        let retention = &self.config.retention;
        let days = retention.max_age_days;
        let rows = retention.max_rows;
        let cooldown = self.config.cooldown_secs;
        let (days_range, rows_range) = (
            Retention::MAX_AGE_DAYS.0..=Retention::MAX_AGE_DAYS.1,
            Retention::MAX_ROWS.0..=Retention::MAX_ROWS.1,
        );
        let cooldown_range = AlertsConfig::COOLDOWN_SECS.0..=AlertsConfig::COOLDOWN_SECS.1;
        body = body
            .child(heading("History", p))
            .child(row(
                "Keep for",
                Some("Older alerts are deleted automatically.".into()),
                self.stepper(
                    "alerts-days",
                    format!("{days} days"),
                    AlertsEdit::RetentionDays(step(days, -1, &days_range)),
                    AlertsEdit::RetentionDays(step(days, 1, &days_range)),
                    cx,
                ),
                p,
            ))
            .child(row(
                "Keep at most",
                Some("When the history grows past this, the oldest alerts go first.".into()),
                self.stepper(
                    "alerts-rows",
                    format!("{rows} alerts"),
                    AlertsEdit::RetentionRows(step(rows, -100, &rows_range)),
                    AlertsEdit::RetentionRows(step(rows, 100, &rows_range)),
                    cx,
                ),
                p,
            ))
            .child(row(
                "Repeat quiet time",
                Some("The same alert for the same object is shown once per this time.".into()),
                self.stepper(
                    "alerts-cooldown",
                    format_duration(cooldown),
                    AlertsEdit::CooldownSecs(step(
                        cooldown,
                        -duration_step(cooldown.saturating_sub(1)),
                        &cooldown_range,
                    )),
                    AlertsEdit::CooldownSecs(step(
                        cooldown,
                        duration_step(cooldown),
                        &cooldown_range,
                    )),
                    cx,
                ),
                p,
            ))
            .child(row(
                "Clear history",
                Some("Deletes every stored alert. This cannot be undone.".into()),
                button(
                    "alerts-clear",
                    "Clear history",
                    p,
                    radius,
                    cx.listener(|_, _, _, cx| cx.emit(AlertsSettingsEvent::ClearHistory)),
                ),
                p,
            ));

        body.text_color(p.text)
    }
}
