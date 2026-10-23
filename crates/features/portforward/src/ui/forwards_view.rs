use std::collections::HashMap;

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{ActiveTheme as _, h_flex, v_flex};
use gpui_kit::*;
use natikube_core::{AppCommand, PortForwardSpec};
use natikube_kube_client::{ClusterConnection, spawn};
use natikube_ui_kit::{Tone, empty_state, link_text, status_dot};

use crate::adapters::{ActiveForward, start_forward};
use crate::domain::{ForwardEntry, ForwardRegistry, ForwardState, format_local_url, open_url};

/// The port-forwards dock tab. One instance lives for the whole app (forwards
/// survive cluster switches).
///
/// Contract: the shell calls [`start`](Self::start) on
/// `AppCommand::StartPortForward` with the active cluster's connection and
/// shows [`title`](Self::title) on the dock tab. It may emit
/// `AppCommand::ShowError`. Dropping the view stops every forward.
pub struct PortForwardsView {
    registry: ForwardRegistry,
    /// Running listeners; dropping one closes its local port.
    active: HashMap<u64, ActiveForward>,
    tasks: HashMap<u64, Task<()>>,
}

impl EventEmitter<AppCommand> for PortForwardsView {}

impl PortForwardsView {
    pub fn new(_window: &mut Window, _cx: &mut Context<Self>) -> Self {
        Self {
            registry: ForwardRegistry::new(),
            active: HashMap::new(),
            tasks: HashMap::new(),
        }
    }

    /// Registers a forward for `spec` on `connection` and starts it. The row
    /// appears as Pending and turns Active (or Failed) when the listener is up.
    pub fn start(
        &mut self,
        connection: ClusterConnection,
        spec: PortForwardSpec,
        cx: &mut Context<Self>,
    ) {
        let id = self.registry.add(spec.clone());
        let client = connection.client().clone();
        let job = spawn(async move { start_forward(client, spec).await });
        let task = cx.spawn(async move |this, cx| {
            let outcome = match job.await {
                Ok(result) => result,
                Err(e) => Err(format!("forward task failed: {e}")),
            };
            let _ = this.update(cx, |view, cx| view.finish_start(id, outcome, cx));
        });
        self.tasks.insert(id, task);
        cx.notify();
    }

    fn finish_start(
        &mut self,
        id: u64,
        outcome: Result<ActiveForward, String>,
        cx: &mut Context<Self>,
    ) {
        self.tasks.remove(&id);
        match outcome {
            Ok(forward) => {
                let stopped = self
                    .registry
                    .get(id)
                    .is_none_or(|e| e.state == ForwardState::Stopped);
                if !stopped {
                    self.registry.set_state(
                        id,
                        ForwardState::Active {
                            local_port: forward.local_port,
                        },
                    );
                    self.active.insert(id, forward);
                }
                // A forward stopped while starting is dropped right here.
            }
            Err(message) => {
                let target = self
                    .registry
                    .get(id)
                    .map(|e| e.spec.describe())
                    .unwrap_or_default();
                cx.emit(AppCommand::ShowError(format!(
                    "Port forward {target}: {message}"
                )));
                self.registry.set_state(id, ForwardState::Failed(message));
            }
        }
        cx.notify();
    }

    /// Stops a forward: closes the local port and its connections.
    pub fn stop(&mut self, id: u64, cx: &mut Context<Self>) {
        self.active.remove(&id);
        self.tasks.remove(&id);
        self.registry.set_state(id, ForwardState::Stopped);
        cx.notify();
    }

    /// Removes a finished (stopped or failed) row.
    pub fn remove(&mut self, id: u64, cx: &mut Context<Self>) {
        if self.registry.remove(id) {
            cx.notify();
        }
    }

    /// Stops every forward (app exit).
    pub fn stop_all(&mut self, cx: &mut Context<Self>) {
        let ids: Vec<u64> = self.registry.entries().iter().map(|e| e.id).collect();
        for id in ids {
            self.stop(id, cx);
        }
    }

    pub fn entries(&self) -> &[ForwardEntry] {
        self.registry.entries()
    }

    /// Dock tab label, e.g. `Port Forwards (2)`.
    pub fn title(&self) -> SharedString {
        if self.registry.is_empty() {
            "Port Forwards".into()
        } else {
            format!("Port Forwards ({})", self.registry.len()).into()
        }
    }

    fn open_in_browser(&self, port: u16, cx: &mut Context<Self>) {
        if let Err(e) = open_url(&format_local_url(port)) {
            cx.emit(AppCommand::ShowError(format!(
                "Could not open browser: {e}"
            )));
        }
    }

    fn row(&self, entry: &ForwardEntry, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let id = entry.id;
        let (tone, status) = match &entry.state {
            ForwardState::Pending => (Tone::Warning, "Starting…".to_string()),
            ForwardState::Active { .. } => (Tone::Ok, "Active".to_string()),
            ForwardState::Failed(message) => (Tone::Error, format!("Failed: {message}")),
            ForwardState::Stopped => (Tone::Muted, "Stopped".to_string()),
        };
        let target = &entry.spec.target;
        let local = match &entry.state {
            ForwardState::Active { local_port } => {
                let port = *local_port;
                link_text(
                    SharedString::from(format!("pf-link-{id}")),
                    format!("localhost:{port}"),
                    cx.listener(move |this, _, _, cx| this.open_in_browser(port, cx)),
                    cx,
                )
                .into_any_element()
            }
            _ => div()
                .text_color(theme.muted_foreground)
                .child(match entry.spec.local_port {
                    Some(p) => format!("localhost:{p}"),
                    None => "—".to_string(),
                })
                .into_any_element(),
        };
        let live = entry.state.is_live();
        let active_port = match &entry.state {
            ForwardState::Active { local_port } => Some(*local_port),
            _ => None,
        };

        h_flex()
            .w_full()
            .px_3()
            .py_1()
            .gap_3()
            .items_center()
            .border_b_1()
            .border_color(theme.border)
            .child(div().w(px(150.)).child(local))
            .child(
                div()
                    .w(px(90.))
                    .text_color(theme.muted_foreground)
                    .child(format!("{}", target.kind.kind)),
            )
            .child(div().flex_1().min_w_0().truncate().child(format!(
                "{}{} : {}",
                target
                    .namespace
                    .as_deref()
                    .map(|n| format!("{n}/"))
                    .unwrap_or_default(),
                target.name,
                entry.spec.remote_port
            )))
            .child(
                h_flex()
                    .gap_2()
                    .items_center()
                    .w(px(240.))
                    .child(status_dot(tone, cx))
                    .child(div().truncate().child(status)),
            )
            .child(
                h_flex()
                    .gap_1()
                    .children(active_port.map(|port| {
                        Button::new(SharedString::from(format!("pf-open-{id}")))
                            .label("Open")
                            .tooltip("Open in the default browser")
                            .ghost()
                            .compact()
                            .on_click(
                                cx.listener(move |this, _, _, cx| this.open_in_browser(port, cx)),
                            )
                    }))
                    .child(if live {
                        Button::new(SharedString::from(format!("pf-stop-{id}")))
                            .label("Stop")
                            .ghost()
                            .compact()
                            .on_click(cx.listener(move |this, _, _, cx| this.stop(id, cx)))
                    } else {
                        Button::new(SharedString::from(format!("pf-remove-{id}")))
                            .label("Remove")
                            .ghost()
                            .compact()
                            .on_click(cx.listener(move |this, _, _, cx| this.remove(id, cx)))
                    }),
            )
    }
}

impl Render for PortForwardsView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let (bg, fg, muted, border) = (
            theme.background,
            theme.foreground,
            theme.muted_foreground,
            theme.border,
        );
        if self.registry.is_empty() {
            return div()
                .size_full()
                .bg(bg)
                .child(empty_state(
                    "No port forwards",
                    "Start one from a pod or service detail",
                    cx,
                ))
                .into_any_element();
        }
        let rows: Vec<_> = self
            .registry
            .entries()
            .to_vec()
            .iter()
            .map(|entry| self.row(entry, cx).into_any_element())
            .collect();
        v_flex()
            .size_full()
            .bg(bg)
            .text_color(fg)
            .text_sm()
            .child(
                h_flex()
                    .w_full()
                    .px_3()
                    .py_1()
                    .gap_3()
                    .border_b_1()
                    .border_color(border)
                    .text_xs()
                    .text_color(muted)
                    .child(div().w(px(150.)).child("Local"))
                    .child(div().w(px(90.)).child("Kind"))
                    .child(div().flex_1().child("Target"))
                    .child(div().w(px(240.)).child("Status"))
                    .child(div().w(px(120.)).child("")),
            )
            .child(
                div()
                    .id("forwards-scroll")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .children(rows),
            )
            .into_any_element()
    }
}
