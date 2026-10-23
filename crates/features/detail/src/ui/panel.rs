use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use futures::FutureExt as _;
use futures::StreamExt as _;
use futures::future::BoxFuture;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{EditorState, Input, InputState};
use gpui_kit::component::{ActiveTheme as _, Disableable as _, Sizable as _, h_flex, v_flex};
use gpui_kit::*;
use natikube_core::{AppCommand, ResourceRef, kinds};
use natikube_kube_client::{ClusterConnection, spawn};
use natikube_ui_kit::{empty_state, error_state};
use serde_json::Value;

use crate::adapters::requests::Operation;
use crate::adapters::transport::{KubeTransport, Transport};
use crate::adapters::watch::{ObjectUpdate, watch_object};
use crate::adapters::workflow::{self, ApplyError};
use crate::domain::actions::{parse_replicas, supports_restart, supports_scale};
use crate::domain::model::{DetailModel, EventRow, parse_events};
use crate::domain::secret::mask_secret_data;
use crate::domain::yaml::{
    check_identity, ensure_type_meta, parse_yaml, strip_managed_fields, to_yaml,
};

/// How often the object's events are re-listed.
const EVENTS_REFRESH: Duration = Duration::from_secs(10);
/// How often relative ages are redrawn.
const AGE_TICK: Duration = Duration::from_secs(5);
/// Minimum time between two redraws caused by watcher events.
const BATCH_WINDOW: Duration = Duration::from_millis(100);

pub(super) fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or_default()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Load {
    Idle,
    Loading,
    Ready,
    Missing,
    Failed(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Tab {
    Overview,
    Yaml,
}

pub(super) struct Banner {
    pub ok: bool,
    pub text: String,
}

/// The right-hand detail panel for one cluster.
///
/// Contract:
/// * built once per cluster connection by the shell;
/// * the shell calls [`show`](Self::show) on `AppCommand::OpenDetail`;
/// * it emits `AppCommand::CloseDetail` (close button, after a delete),
///   `OpenLogs`, `OpenExec`, `StartPortForward`, `OpenDetail` (owner and node
///   links) and `ShowError` as GPUI events.
pub struct DetailPanel {
    pub(super) connection: ClusterConnection,
    pub(super) transport: Arc<dyn Transport>,
    pub(super) target: Option<ResourceRef>,
    pub(super) load: Load,
    pub(super) raw: Option<Value>,
    pub(super) model: Option<DetailModel>,
    pub(super) events: Vec<EventRow>,
    pub(super) tab: Tab,
    pub(super) deleted: bool,

    // YAML tab
    pub(super) yaml: Entity<EditorState>,
    pub(super) pending_yaml: Option<String>,
    pub(super) editing: bool,
    pub(super) show_managed: bool,
    pub(super) reveal_yaml: bool,
    pub(super) validation: Option<String>,
    pub(super) server_changed: bool,

    // Secrets (Overview): keys revealed one by one.
    pub(super) revealed: std::collections::HashSet<String>,

    // Actions
    pub(super) busy: Option<String>,
    pub(super) banner: Option<Banner>,
    pub(super) confirm_delete: bool,
    pub(super) scale_open: bool,
    pub(super) scale_input: Entity<InputState>,
    pub(super) pending_scale: Option<String>,

    _watch: Option<Task<()>>,
    _events: Option<Task<()>>,
    _tick: Task<()>,
}

impl EventEmitter<AppCommand> for DetailPanel {}

impl DetailPanel {
    pub fn new(connection: ClusterConnection, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let yaml = cx.new(|cx| {
            EditorState::new(window, cx)
                .language("yaml")
                .line_number(true)
                .searchable(true)
        });
        let scale_input = cx.new(|cx| InputState::new(window, cx).placeholder("Replicas"));
        let _tick = cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(AGE_TICK).await;
                if this.update(cx, |_, cx| cx.notify()).is_err() {
                    break;
                }
            }
        });
        let transport: Arc<dyn Transport> = Arc::new(KubeTransport::new(&connection));
        Self {
            connection,
            transport,
            target: None,
            load: Load::Idle,
            raw: None,
            model: None,
            events: Vec::new(),
            tab: Tab::Overview,
            deleted: false,
            yaml,
            pending_yaml: None,
            editing: false,
            show_managed: false,
            reveal_yaml: false,
            validation: None,
            server_changed: false,
            revealed: Default::default(),
            busy: None,
            banner: None,
            confirm_delete: false,
            scale_open: false,
            scale_input,
            pending_scale: None,
            _watch: None,
            _events: None,
            _tick,
        }
    }

    pub fn connection(&self) -> &ClusterConnection {
        &self.connection
    }

    pub fn target(&self) -> Option<&ResourceRef> {
        self.target.as_ref()
    }

    /// Shows `target` (replacing whatever was shown). Showing the object that
    /// is already open keeps the current state.
    pub fn show(&mut self, target: ResourceRef, cx: &mut Context<Self>) {
        if self.target.as_ref() == Some(&target) {
            return;
        }
        self.reset_state();
        self.target = Some(target.clone());
        self.load = Load::Loading;
        self.start_watch(&target, cx);
        self.start_events(&target, cx);
        cx.notify();
    }

    /// Closes the object: drops the watcher and the events poller and forgets
    /// the target, so nothing keeps running while the panel is hidden.
    pub fn clear(&mut self, cx: &mut Context<Self>) {
        self._watch = None;
        self._events = None;
        self.target = None;
        self.load = Load::Idle;
        self.reset_state();
        cx.notify();
    }

    fn reset_state(&mut self) {
        self.raw = None;
        self.model = None;
        self.events.clear();
        self.tab = Tab::Overview;
        self.deleted = false;
        self.pending_yaml = Some(String::new());
        self.editing = false;
        self.show_managed = false;
        self.reveal_yaml = false;
        self.validation = None;
        self.server_changed = false;
        self.revealed.clear();
        self.busy = None;
        self.banner = None;
        self.confirm_delete = false;
        self.scale_open = false;
    }

    // ---- loading ----------------------------------------------------------

    fn start_watch(&mut self, target: &ResourceRef, cx: &mut Context<Self>) {
        let mut stream = watch_object(&self.connection, target);
        self._watch = Some(cx.spawn(async move |this, cx| {
            while let Some(first) = stream.next().await {
                let mut batch = vec![first];
                while let Some(next) = stream.try_recv() {
                    batch.push(next);
                }
                let alive = this.update(cx, |view, cx| {
                    for update in batch {
                        view.apply_update(update, cx);
                    }
                });
                if alive.is_err() {
                    break;
                }
                cx.background_executor().timer(BATCH_WINDOW).await;
            }
        }));
    }

    fn start_events(&mut self, target: &ResourceRef, cx: &mut Context<Self>) {
        let transport = self.transport.clone();
        let target = target.clone();
        self._events = Some(cx.spawn(async move |this, cx| {
            loop {
                let (t, tg) = (transport.clone(), target.clone());
                let result =
                    spawn(async move { t.execute(Operation::ListEvents { target: tg }).await })
                        .await;
                let rows = match result {
                    Ok(Ok(list)) => Some(parse_events(&list)),
                    _ => None, // keep the previous events on a transient failure
                };
                let alive = this.update(cx, |view, cx| {
                    if let Some(rows) = rows {
                        view.events = rows;
                        cx.notify();
                    }
                });
                if alive.is_err() {
                    break;
                }
                cx.background_executor().timer(EVENTS_REFRESH).await;
            }
        }));
    }

    fn apply_update(&mut self, update: ObjectUpdate, cx: &mut Context<Self>) {
        match update {
            ObjectUpdate::Object(value) => self.set_object(value, cx),
            ObjectUpdate::Deleted(value) => {
                self.deleted = true;
                self.set_object(value, cx);
            }
            ObjectUpdate::NotFound => {
                if self.raw.is_some() {
                    self.deleted = true;
                } else {
                    self.load = Load::Missing;
                }
            }
            ObjectUpdate::Error(message) => {
                if self.raw.is_none() {
                    self.load = Load::Failed(message);
                } else {
                    tracing::warn!("detail watcher: {message}");
                }
            }
        }
        cx.notify();
    }

    /// Installs a new server state of the object.
    pub(super) fn set_object(&mut self, mut value: Value, cx: &mut Context<Self>) {
        let Some(target) = self.target.clone() else {
            return;
        };
        ensure_type_meta(&mut value, &target.kind);
        let changed = self.raw.as_ref().is_some_and(|old| {
            old.pointer("/metadata/resourceVersion") != value.pointer("/metadata/resourceVersion")
        });
        self.model = Some(DetailModel::build(&value, &target.kind, unix_now()));
        self.raw = Some(value);
        self.load = Load::Ready;
        if self.editing {
            self.server_changed |= changed;
        } else {
            self.refresh_yaml();
        }
        cx.notify();
    }

    fn is_secret(&self) -> bool {
        self.target
            .as_ref()
            .is_some_and(|t| t.kind == kinds::SECRET)
    }

    /// True when the YAML shows real values and may be edited.
    pub(super) fn yaml_editable(&self) -> bool {
        !self.is_secret() || self.reveal_yaml
    }

    /// Queues the editor text for the next render (the editor needs a window).
    pub(super) fn refresh_yaml(&mut self) {
        let Some(raw) = &self.raw else { return };
        let text = if self.is_secret() && !self.reveal_yaml {
            to_yaml(&mask_secret_data(raw), !self.show_managed)
        } else {
            to_yaml(raw, !self.show_managed)
        };
        self.pending_yaml = Some(text);
    }

    // ---- actions ----------------------------------------------------------

    pub(super) fn set_banner(&mut self, ok: bool, text: impl Into<String>) {
        self.banner = Some(Banner {
            ok,
            text: text.into(),
        });
    }

    /// Runs a write on the Tokio runtime and reports the outcome in the banner.
    fn run_action(
        &mut self,
        label: &str,
        job: BoxFuture<'static, Result<String, String>>,
        close_on_ok: bool,
        cx: &mut Context<Self>,
    ) {
        if self.busy.is_some() {
            return;
        }
        self.busy = Some(label.to_string());
        self.banner = None;
        let handle = spawn(job);
        cx.spawn(async move |this, cx| {
            let outcome = match handle.await {
                Ok(result) => result,
                Err(e) => Err(e.to_string()),
            };
            let _ = this.update(cx, |view, cx| {
                view.busy = None;
                match outcome {
                    Ok(message) => {
                        view.set_banner(true, message);
                        if close_on_ok {
                            cx.emit(AppCommand::CloseDetail);
                        }
                    }
                    Err(message) => {
                        view.set_banner(false, message.clone());
                        cx.emit(AppCommand::ShowError(message));
                    }
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    pub(super) fn do_delete(&mut self, cx: &mut Context<Self>) {
        let Some(target) = self.target.clone() else {
            return;
        };
        self.confirm_delete = false;
        let transport = self.transport.clone();
        let message = format!("Deleted {target}");
        let job = async move {
            workflow::delete(transport.as_ref(), &target)
                .await
                .map(|_| message)
                .map_err(|e| e.to_string())
        }
        .boxed();
        self.run_action("Deleting", job, true, cx);
    }

    pub(super) fn open_scale(&mut self, cx: &mut Context<Self>) {
        let current = self
            .raw
            .as_ref()
            .and_then(|r| r.pointer("/spec/replicas"))
            .and_then(Value::as_i64)
            .unwrap_or(1);
        self.pending_scale = Some(current.to_string());
        self.scale_open = true;
        cx.notify();
    }

    pub(super) fn do_scale(&mut self, cx: &mut Context<Self>) {
        let Some(target) = self.target.clone() else {
            return;
        };
        if !supports_scale(&target.kind) {
            return;
        }
        let text = self.scale_input.read(cx).value().to_string();
        let replicas = match parse_replicas(&text) {
            Ok(n) => n,
            Err(message) => {
                self.set_banner(false, message);
                cx.notify();
                return;
            }
        };
        self.scale_open = false;
        let transport = self.transport.clone();
        let message = format!("Scaled {} to {replicas} replicas", target.name);
        let job = async move {
            workflow::scale(transport.as_ref(), &target, replicas)
                .await
                .map(|_| message)
                .map_err(|e| e.to_string())
        }
        .boxed();
        self.run_action("Scaling", job, false, cx);
    }

    pub(super) fn do_restart(&mut self, cx: &mut Context<Self>) {
        let Some(target) = self.target.clone() else {
            return;
        };
        if !supports_restart(&target.kind) {
            return;
        }
        let transport = self.transport.clone();
        let stamp = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
        let message = format!("Restarting {}", target.name);
        let job = async move {
            workflow::rollout_restart(transport.as_ref(), &target, &stamp)
                .await
                .map(|_| message)
                .map_err(|e| e.to_string())
        }
        .boxed();
        self.run_action("Restarting", job, false, cx);
    }

    pub(super) fn begin_edit(&mut self, cx: &mut Context<Self>) {
        if !self.yaml_editable() {
            return;
        }
        self.editing = true;
        self.server_changed = false;
        self.validation = None;
        cx.notify();
    }

    pub(super) fn cancel_edit(&mut self, cx: &mut Context<Self>) {
        self.editing = false;
        self.validation = None;
        self.server_changed = false;
        self.refresh_yaml();
        cx.notify();
    }

    /// Validates with a server-side dry run, then applies for real.
    pub(super) fn save_yaml(&mut self, cx: &mut Context<Self>) {
        if self.busy.is_some() {
            return;
        }
        let Some(target) = self.target.clone() else {
            return;
        };
        let text = self.yaml.read(cx).value().to_string();
        let mut body = match parse_yaml(&text) {
            Ok(body) => body,
            Err(e) => {
                self.validation = Some(format!("Invalid YAML: {e}"));
                cx.notify();
                return;
            }
        };
        if let Err(e) = check_identity(&target, &body) {
            self.validation = Some(e);
            cx.notify();
            return;
        }
        strip_managed_fields(&mut body);
        self.validation = None;
        self.busy = Some("Validating and applying".to_string());
        let transport = self.transport.clone();
        let handle =
            spawn(async move { workflow::apply_object(transport.as_ref(), &target, body).await });
        cx.spawn(async move |this, cx| {
            let outcome = match handle.await {
                Ok(result) => result,
                Err(e) => Err(ApplyError::Apply(e.to_string())),
            };
            let _ = this.update(cx, |view, cx| {
                view.busy = None;
                match outcome {
                    Ok(value) => {
                        view.editing = false;
                        view.server_changed = false;
                        view.set_banner(true, "Changes applied");
                        view.set_object(value, cx);
                        view.refresh_yaml();
                    }
                    Err(ApplyError::Validation(m)) => {
                        view.validation = Some(format!(
                            "Validation failed (server dry run); nothing was changed: {m}"
                        ));
                    }
                    Err(e @ (ApplyError::Apply(_) | ApplyError::MaskedSecret)) => {
                        view.validation = Some(format!("Apply failed: {e}"));
                    }
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    pub(super) fn toggle_reveal_yaml(&mut self, cx: &mut Context<Self>) {
        if self.editing {
            return;
        }
        self.reveal_yaml = !self.reveal_yaml;
        self.refresh_yaml();
        cx.notify();
    }

    pub(super) fn toggle_managed(&mut self, cx: &mut Context<Self>) {
        if self.editing {
            return;
        }
        self.show_managed = !self.show_managed;
        self.refresh_yaml();
        cx.notify();
    }

    pub(super) fn is_pod(&self) -> bool {
        self.target.as_ref().is_some_and(|t| t.kind == kinds::POD)
    }
}

impl Render for DetailPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Editor and input texts can only be set with a window; apply queued text here.
        if let Some(text) = self.pending_yaml.take() {
            self.yaml
                .update(cx, |state, cx| state.set_value(text, window, cx));
        }
        if let Some(text) = self.pending_scale.take() {
            self.scale_input
                .update(cx, |state, cx| state.set_value(text, window, cx));
        }

        let theme = cx.theme();
        let (bg, border, muted, fg, red, green, yellow) = (
            theme.sidebar,
            theme.border,
            theme.muted_foreground,
            theme.foreground,
            theme.red,
            theme.green,
            theme.yellow,
        );

        let Some(target) = self.target.clone() else {
            return v_flex()
                .size_full()
                .bg(bg)
                .text_color(fg)
                .child(self.header("Details", None, cx))
                .child(div().flex_1().min_h_0().child(empty_state(
                    "Nothing selected",
                    "Select an object to see its details",
                    cx,
                )));
        };

        let body: AnyElement = match &self.load {
            Load::Idle | Load::Loading => {
                empty_state("Loading", target.to_string(), cx).into_any_element()
            }
            Load::Missing => error_state(
                "Not found",
                format!("{target} does not exist (or you are not allowed to see it)"),
                cx,
            )
            .into_any_element(),
            Load::Failed(message) => {
                error_state("Could not load the object", message.clone(), cx).into_any_element()
            }
            Load::Ready => match self.tab {
                Tab::Overview => self.render_overview(cx).into_any_element(),
                Tab::Yaml => self.render_yaml_tab(cx).into_any_element(),
            },
        };

        let mut panel = v_flex()
            .size_full()
            .bg(bg)
            .text_color(fg)
            .text_sm()
            .child(self.header(&target.kind.kind, Some(&target), cx))
            .child(self.toolbar(&target, cx));

        if self.confirm_delete {
            panel = panel.child(
                h_flex()
                    .w_full()
                    .gap_2()
                    .px_3()
                    .py_2()
                    .items_center()
                    .flex_wrap()
                    .border_b_1()
                    .border_color(border)
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_color(red)
                            .child(format!("Delete {target}? This cannot be undone.")),
                    )
                    .child(
                        Button::new("detail-delete-cancel")
                            .label("Cancel")
                            .small()
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.confirm_delete = false;
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new("detail-delete-confirm")
                            .label("Delete")
                            .small()
                            .danger()
                            .on_click(cx.listener(|this, _, _, cx| this.do_delete(cx))),
                    ),
            );
        }

        if self.scale_open {
            panel = panel.child(
                h_flex()
                    .w_full()
                    .gap_2()
                    .px_3()
                    .py_2()
                    .items_center()
                    .border_b_1()
                    .border_color(border)
                    .child(div().text_color(muted).child("Replicas"))
                    .child(div().w(px(90.)).child(Input::new(&self.scale_input)))
                    .child(
                        Button::new("detail-scale-apply")
                            .label("Scale")
                            .small()
                            .primary()
                            .on_click(cx.listener(|this, _, _, cx| this.do_scale(cx))),
                    )
                    .child(
                        Button::new("detail-scale-cancel")
                            .label("Cancel")
                            .small()
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.scale_open = false;
                                cx.notify();
                            })),
                    ),
            );
        }

        if let Some(busy) = &self.busy {
            panel = panel.child(
                div()
                    .w_full()
                    .px_3()
                    .py_1()
                    .text_xs()
                    .text_color(muted)
                    .child(format!("{busy}...")),
            );
        }

        if self.deleted {
            panel = panel.child(
                div()
                    .w_full()
                    .px_3()
                    .py_1()
                    .text_xs()
                    .text_color(yellow)
                    .child("This object was deleted. Showing its last known state."),
            );
        }

        if let Some(banner) = &self.banner {
            let color = if banner.ok { green } else { red };
            panel = panel.child(
                h_flex()
                    .w_full()
                    .px_3()
                    .py_1()
                    .gap_2()
                    .items_start()
                    .text_xs()
                    .text_color(color)
                    .child(div().flex_1().min_w_0().child(banner.text.clone()))
                    .child(
                        div()
                            .id("detail-banner-close")
                            .cursor_pointer()
                            .text_color(muted)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.banner = None;
                                cx.notify();
                            }))
                            .child("x"),
                    ),
            );
        }

        panel.child(div().flex_1().min_h_0().child(body))
    }
}

impl DetailPanel {
    fn header(&self, kind: &str, target: Option<&ResourceRef>, cx: &mut Context<Self>) -> Div {
        let theme = cx.theme();
        let (border, muted) = (theme.border, theme.muted_foreground);
        let title: SharedString = match target {
            Some(t) => t.name.clone().into(),
            None => "Details".into(),
        };
        let subtitle: Option<SharedString> = target.map(|t| match &t.namespace {
            Some(ns) => format!("{kind} in {ns}").into(),
            None => kind.to_string().into(),
        });
        h_flex()
            .w_full()
            .min_h(px(48.))
            .flex_none()
            .px_3()
            .py_1()
            .gap_2()
            .items_center()
            .justify_between()
            .border_b_1()
            .border_color(border)
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::SEMIBOLD)
                            .overflow_hidden()
                            .text_ellipsis()
                            .child(title),
                    )
                    .children(subtitle.map(|s| div().text_xs().text_color(muted).child(s))),
            )
            .child(
                div()
                    .id("detail-close")
                    .cursor_pointer()
                    .px_2()
                    .text_color(muted)
                    .hover(|s| s.text_color(gpui_kit::white()))
                    .on_click(cx.listener(|_, _, _, cx| cx.emit(AppCommand::CloseDetail)))
                    .child("✕"),
            )
    }

    fn toolbar(&self, target: &ResourceRef, cx: &mut Context<Self>) -> Div {
        let theme = cx.theme();
        let (border, primary, muted) = (theme.border, theme.primary, theme.muted_foreground);
        let tab = self.tab;
        let tab_button =
            |id: &'static str, label: &'static str, which: Tab, cx: &mut Context<Self>| {
                let active = tab == which;
                div()
                    .id(id)
                    .cursor_pointer()
                    .px_2()
                    .py_1()
                    .border_b_2()
                    .border_color(if active {
                        primary
                    } else {
                        gpui_kit::transparent_black()
                    })
                    .text_color(if active { gpui_kit::white() } else { muted })
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.tab = which;
                        cx.notify();
                    }))
                    .child(label)
            };

        let ready = self.load == Load::Ready && !self.deleted;
        let mut actions = h_flex().gap_1().flex_wrap().items_center();
        if self.is_pod() {
            let (logs_ref, shell_ref) = (target.clone(), target.clone());
            actions = actions
                .child(
                    Button::new("detail-logs")
                        .label("Logs")
                        .small()
                        .disabled(!ready)
                        .on_click(cx.listener(move |_, _, _, cx| {
                            cx.emit(AppCommand::OpenLogs {
                                pod: logs_ref.clone(),
                                container: None,
                            })
                        })),
                )
                .child(
                    Button::new("detail-shell")
                        .label("Shell")
                        .small()
                        .disabled(!ready)
                        .on_click(cx.listener(move |_, _, _, cx| {
                            cx.emit(AppCommand::OpenExec {
                                pod: shell_ref.clone(),
                                container: None,
                            })
                        })),
                );
        }
        if supports_scale(&target.kind) {
            actions = actions.child(
                Button::new("detail-scale")
                    .label("Scale")
                    .small()
                    .disabled(!ready || self.busy.is_some())
                    .on_click(cx.listener(|this, _, _, cx| this.open_scale(cx))),
            );
        }
        if supports_restart(&target.kind) {
            actions = actions.child(
                Button::new("detail-restart")
                    .label("Restart")
                    .small()
                    .disabled(!ready || self.busy.is_some())
                    .on_click(cx.listener(|this, _, _, cx| this.do_restart(cx))),
            );
        }
        actions = actions.child(
            Button::new("detail-delete")
                .label("Delete")
                .small()
                .danger()
                .disabled(!ready || self.busy.is_some())
                .on_click(cx.listener(|this, _, _, cx| {
                    this.confirm_delete = true;
                    cx.notify();
                })),
        );

        h_flex()
            .w_full()
            .flex_none()
            .px_2()
            .gap_2()
            .items_center()
            .justify_between()
            .flex_wrap()
            .border_b_1()
            .border_color(border)
            .child(
                h_flex()
                    .child(tab_button(
                        "detail-tab-overview",
                        "Overview",
                        Tab::Overview,
                        cx,
                    ))
                    .child(tab_button("detail-tab-yaml", "YAML", Tab::Yaml, cx)),
            )
            .child(actions.py_1())
    }
}
