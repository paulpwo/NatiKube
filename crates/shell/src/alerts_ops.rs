//! Shell side of alerts: owns the history store, the dispatcher, the monitor
//! and the header bells, and routes the alert `AppCommand`s.
//!
//! The monitor runs on the kube-client runtime and reports through the
//! dispatcher; `on_change` fires on a monitor thread, so it only posts to a
//! channel that a UI-thread task drains.

use std::sync::Arc;

use futures::StreamExt as _;
use futures::channel::mpsc;
use gpui_kit::*;
use natikube_alerts::adapters::monitor::AlertMonitor;
use natikube_alerts::adapters::notify::{NativeNotifier, NoopNotifier};
use natikube_alerts::ui::bell::{AlertBell, BellEvent};
use natikube_alerts::ui::settings::AlertClusterEntry;
use natikube_alerts::{AlertDispatcher, AlertStore, OsNotifier};
use natikube_core::{AlertTarget, AlertsConfig, AppCommand, ResourceRef, kinds};

use crate::app::{AlertsOptions, ShellOptions};
use crate::shell_view::ShellView;

/// Everything the shell holds for alerts.
pub(crate) struct AlertsRuntime {
    pub(crate) store: Arc<dyn AlertStore>,
    pub(crate) dispatcher: Arc<AlertDispatcher>,
    /// Header bell (Standard, Command, Native).
    pub(crate) bell: Entity<AlertBell>,
    /// Bell for the Observatory rail; its panel opens to the right.
    pub(crate) rail_bell: Entity<AlertBell>,
    /// `None` when nothing may touch the network (demo, fake data).
    monitor: Option<AlertMonitor>,
    _drain: Task<()>,
    _subscriptions: Vec<Subscription>,
}

/// Maps a bell click to the navigation command.
fn open_object_command(target: natikube_alerts::ui::bell::BellTarget) -> AppCommand {
    AppCommand::OpenAlertObject(AlertTarget {
        cluster: target.cluster,
        namespace: target.namespace,
        kind: target.kind,
        name: target.name,
    })
}

/// The object an alert points at, when its kind is a built-in one.
pub(crate) fn alert_resource(target: &AlertTarget) -> Option<ResourceRef> {
    let kind = kinds::find(target.kind.as_deref()?)?;
    let name = target.name.clone()?;
    let namespace = if kind.namespaced {
        target.namespace.clone()
    } else {
        None
    };
    Some(ResourceRef::new(
        target.cluster.clone(),
        kind,
        namespace,
        name,
    ))
}

impl AlertsRuntime {
    pub(crate) fn new(
        options: &ShellOptions,
        window: &mut Window,
        cx: &mut Context<ShellView>,
    ) -> Self {
        let AlertsOptions { store, monitor } = options.alerts.clone();
        let (tx, mut rx) = mpsc::unbounded::<()>();
        let on_change: Arc<dyn Fn() + Send + Sync> = Arc::new(move || {
            let _ = tx.unbounded_send(());
        });
        let notifier: Arc<dyn OsNotifier> = if options.demo || options.fake_rows.is_some() {
            Arc::new(NoopNotifier)
        } else {
            Arc::new(NativeNotifier::new())
        };
        let dispatcher = Arc::new(AlertDispatcher::new(store.clone(), notifier, on_change));
        let monitor = (monitor && !options.demo && options.fake_rows.is_none())
            .then(|| AlertMonitor::new(dispatcher.clone()));

        let bell = cx.new(|_| AlertBell::new(store.clone()));
        let rail_bell = cx.new(|_| AlertBell::in_rail(store.clone()));
        let subscriptions = [&bell, &rail_bell]
            .into_iter()
            .map(|bell| {
                cx.subscribe_in(bell, window, |this, _, event: &BellEvent, window, cx| {
                    let command = match event {
                        BellEvent::OpenObject(target) => open_object_command(target.clone()),
                        BellEvent::Changed => AppCommand::AlertsChanged,
                    };
                    this.dispatch(command, window, cx)
                })
            })
            .collect();

        // Coalesce bursts: one refresh per drained batch.
        let drain = cx.spawn_in(window, async move |this, cx| {
            while rx.next().await.is_some() {
                while rx.try_recv().is_ok() {}
                let alive = this.update_in(cx, |this, window, cx| {
                    this.dispatch(AppCommand::AlertsChanged, window, cx)
                });
                if alive.is_err() {
                    break;
                }
            }
        });

        Self {
            store,
            dispatcher,
            bell,
            rail_bell,
            monitor,
            _drain: drain,
            _subscriptions: subscriptions,
        }
    }
}

impl ShellView {
    /// Pushes `Settings.alerts` and the catalog into the dispatcher and the
    /// monitor. Cheap and idempotent; called at startup, after a settings
    /// change and after a catalog reload.
    pub(crate) fn apply_alerts(&mut self) {
        let config = &self.settings.alerts;
        self.alerts
            .dispatcher
            .set_os_enabled(config.os_notifications);
        self.alerts
            .dispatcher
            .set_retention(config.retention.clone().clamped());
        if let Some(monitor) = self.alerts.monitor.as_mut() {
            monitor.apply(config, &self.clusters);
        }
    }

    pub(crate) fn alert_cluster_entries(&self) -> Vec<AlertClusterEntry> {
        self.clusters
            .iter()
            .map(|c| AlertClusterEntry {
                id: c.id.clone(),
                name: c.name.clone(),
            })
            .collect()
    }

    /// The bell of the active appearance.
    fn active_bell(&self, cx: &App) -> &Entity<AlertBell> {
        use natikube_ui_kit::ActiveTokens as _;
        match cx.tokens().appearance {
            natikube_core::Appearance::Observatory => &self.alerts.rail_bell,
            _ => &self.alerts.bell,
        }
    }

    pub(crate) fn refresh_bells(&mut self, cx: &mut Context<Self>) {
        for bell in [&self.alerts.bell, &self.alerts.rail_bell] {
            bell.update(cx, |bell, cx| bell.refresh(cx));
        }
    }

    pub(crate) fn open_alert_panel(&mut self, cx: &mut Context<Self>) {
        let bell = self.active_bell(cx).clone();
        bell.update(cx, |bell, cx| bell.open_panel(cx));
    }

    /// Applies an edit from Settings → Alerts: clamp, persist, apply live.
    pub(crate) fn apply_alerts_config(&mut self, received: AlertsConfig, cx: &mut Context<Self>) {
        let mut next = received.clone();
        next.clamp_in_place();
        if next == self.settings.alerts {
            return;
        }
        let clamped = next != received;
        self.settings.alerts = next;
        self.persist();
        self.apply_alerts();
        if clamped {
            self.sync_alerts_settings(cx);
        }
    }

    /// Hands the open Settings window the clamped config and the cluster
    /// list. Deferred: this can run while that window is being updated.
    pub(crate) fn sync_alerts_settings(&mut self, cx: &mut Context<Self>) {
        let (Some((handle, _)), Some(section)) = (&self.settings_window, &self.alerts_section)
        else {
            return;
        };
        let (handle, section) = (*handle, section.clone());
        let config = self.settings.alerts.clone();
        let clusters = self.alert_cluster_entries();
        cx.defer(move |cx| {
            let _ = handle.update(cx, move |_, window, cx| {
                section.update(cx, |section, cx| {
                    section.set_config(config, window, cx);
                    section.set_clusters(clusters, window, cx);
                })
            });
        });
    }

    /// "Send test notification": off the UI thread, because the OS call can
    /// take a moment. It ignores the OS switch on purpose.
    pub(crate) fn send_test_notification(&mut self, cx: &mut Context<Self>) {
        if self.options.demo || self.options.fake_rows.is_some() {
            return self.set_notice("Not available in the demo.", cx);
        }
        let dispatcher = self.alerts.dispatcher.clone();
        let task = cx.background_spawn(async move { dispatcher.send_test_notification() });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |this, cx| match result {
                Ok(()) => this.set_notice(
                    "Test notification sent. If it did not appear, check the system notification settings.",
                    cx,
                ),
                Err(e) => this.show_error(&format!("Could not send the notification: {e}"), cx),
            });
        })
        .detach();
    }

    pub(crate) fn clear_alert_history(&mut self, cx: &mut Context<Self>) {
        match self.alerts.store.clear() {
            Ok(()) => self.refresh_bells(cx),
            Err(e) => self.show_error(&format!("Could not clear the history: {e}"), cx),
        }
    }

    /// Switches to the alert's cluster and opens its object once the cluster
    /// is connected. A kind outside the built-in list only switches cluster.
    pub(crate) fn open_alert_object(
        &mut self,
        target: AlertTarget,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.pending_alert_open = alert_resource(&target);
        self.select_cluster(target.cluster.clone(), window, cx);
        // Already connected to that cluster: no new session will be installed.
        if self.active.as_ref() == Some(&target.cluster) && self.session.is_some() {
            self.open_pending_alert_object(window, cx);
        }
    }

    /// Opens the object an alert asked for, if one is waiting for this
    /// cluster's session.
    pub(crate) fn open_pending_alert_object(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.pending_alert_open.as_ref().map(|r| &r.cluster) != self.active.as_ref() {
            return;
        }
        if let Some(target) = self.pending_alert_open.take() {
            self.dispatch(AppCommand::OpenDetail(target), window, cx);
        }
    }
}

#[cfg(test)]
mod tests {
    use natikube_core::{AlertTarget, ClusterId};

    use super::alert_resource;

    fn target(kind: Option<&str>, namespace: Option<&str>, name: Option<&str>) -> AlertTarget {
        AlertTarget {
            cluster: ClusterId::new("/k#dev"),
            namespace: namespace.map(String::from),
            kind: kind.map(String::from),
            name: name.map(String::from),
        }
    }

    #[test]
    fn a_pod_alert_opens_that_pod() {
        let r = alert_resource(&target(Some("Pod"), Some("shop"), Some("web-1"))).unwrap();
        assert_eq!(r.kind.kind, "Pod");
        assert_eq!(r.namespace.as_deref(), Some("shop"));
        assert_eq!(r.name, "web-1");
    }

    #[test]
    fn cluster_scoped_kinds_drop_the_namespace() {
        let r = alert_resource(&target(Some("Node"), Some("ignored"), Some("n1"))).unwrap();
        assert_eq!(r.namespace, None);
    }

    #[test]
    fn missing_or_unknown_objects_only_switch_cluster() {
        assert!(alert_resource(&target(None, None, None)).is_none());
        assert!(alert_resource(&target(Some("Pod"), Some("a"), None)).is_none());
        assert!(alert_resource(&target(Some("Widget"), None, Some("x"))).is_none());
    }
}
