use std::collections::HashMap;
use std::time::{Duration, Instant};

use gpui_kit::component::{ActiveTheme as _, h_flex, resizable::*, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use natikube_clusters::{ClusterHeader, ClusterRail};
use natikube_core::{
    AppCommand, Appearance, ClusterId, ClusterInfo, ConnectionState, SidebarEntry, ViewId,
    kind_title, kinds,
};
use natikube_detail::DetailPanel;
use natikube_kube_client::ClusterConnection;
use natikube_logs::LogsView;
use natikube_palette::PaletteView;
use natikube_portforward::PortForwardsView;
use natikube_resources::{LoadState, ResourcesView};
use natikube_terminal::TerminalView;
use natikube_ui_kit::{ActiveTokens as _, empty_state, error_state};

use crate::app::ShellOptions;
use crate::dock::DockTab;
use crate::state::{DockState, NavState};

/// The state of the active cluster: its views and (unless synthetic) its
/// connection. Dropping a session cancels every watcher it started.
pub(crate) struct Session {
    pub(crate) connection: Option<ClusterConnection>,
    pub(crate) resources: Entity<ResourcesView>,
    pub(crate) detail: Option<Entity<DetailPanel>>,
    _subscriptions: Vec<Subscription>,
}

/// Root view of the window.
pub struct ShellView {
    pub(crate) options: ShellOptions,
    pub(crate) settings: natikube_core::Settings,
    pub(crate) clusters: Vec<ClusterInfo>,
    pub(crate) states: HashMap<ClusterId, ConnectionState>,
    pub(crate) active: Option<ClusterId>,
    pub(crate) rail: Entity<ClusterRail>,
    pub(crate) header: Entity<ClusterHeader>,
    pub(crate) palette: Entity<PaletteView>,
    pub(crate) nav: NavState,
    pub(crate) dock: DockState,
    pub(crate) dock_tabs: Vec<DockTab>,
    pub(crate) port_forwards: Entity<PortForwardsView>,
    pub(crate) session: Option<Session>,
    pub(crate) detail_open: bool,
    pub(crate) error: Option<SharedString>,
    /// One-line informational message (import summary); click to dismiss.
    pub(crate) notice: Option<SharedString>,
    _connecting: Option<Task<()>>,
    pub(crate) _dialog: Option<Task<()>>,
    pub(crate) _import: Option<Task<()>>,
    pub(crate) _reload: Option<Task<()>>,
    pub(crate) settings_window: Option<(AnyWindowHandle, Entity<natikube_settings::SettingsView>)>,
    pub(crate) _settings_subscription: Option<Subscription>,
    _static_subscriptions: Vec<Subscription>,
    first_frame_logged: bool,
    rows_frame_logged: bool,
    _measure_timeout: Option<Task<()>>,
}

impl ShellView {
    pub fn new(options: ShellOptions, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let settings = options.store.load().unwrap_or_default();

        let mut clusters = options.catalog.clone();
        if options.fake_rows.is_some() {
            clusters = vec![ClusterInfo::from_context("synthetic")];
        }

        // Startup cluster: explicit flag, then last used (matched by id, then
        // context, name or alias), then the first one.
        let initial = options
            .initial_cluster
            .clone()
            .filter(|id| clusters.iter().any(|c| &c.id == id))
            .or_else(|| {
                settings.last_cluster.as_ref().and_then(|last| {
                    natikube_clusters::find_cluster(&clusters, last.as_str())
                        .or_else(|| natikube_clusters::find_cluster(&clusters, last.context_name()))
                        .map(|c| c.id.clone())
                })
            })
            .or_else(|| clusters.first().map(|c| c.id.clone()));

        let rail = cx.new(|_| ClusterRail::new(clusters.clone(), initial.clone()));
        let header = cx.new(|_| ClusterHeader::new(None, ConnectionState::Disconnected));
        header.update(cx, |header, cx| header.set_clusters(clusters.clone(), cx));
        let port_forwards = cx.new(|cx| PortForwardsView::new(window, cx));
        let palette = cx.new(|cx| PaletteView::new(window, cx));

        let mut subscriptions = vec![
            cx.subscribe_in(
                &rail,
                window,
                |this, _, command: &AppCommand, window, cx| {
                    this.dispatch(command.clone(), window, cx)
                },
            ),
            cx.subscribe_in(
                &header,
                window,
                |this, _, command: &AppCommand, window, cx| {
                    this.dispatch(command.clone(), window, cx)
                },
            ),
            cx.subscribe_in(
                &port_forwards,
                window,
                |this, _, command: &AppCommand, window, cx| {
                    this.dispatch(command.clone(), window, cx)
                },
            ),
            cx.subscribe_in(
                &palette,
                window,
                |this, _, command: &AppCommand, window, cx| {
                    this.dispatch(command.clone(), window, cx)
                },
            ),
            // The first-rows measurement reads the table size on render.
            cx.observe(&port_forwards, |_, _, cx| cx.notify()),
            // Native follows the macOS light/dark setting live.
            window.observe_window_appearance(|_, cx| natikube_ui_kit::sync_system_appearance(cx)),
        ];
        subscriptions.shrink_to_fit();

        let measure_timeout = options.exit_after_first_frame.then(|| {
            let timeout = options.rows_timeout;
            cx.spawn(async move |_, cx| {
                cx.background_executor().timer(timeout).await;
                eprintln!("first_rows_frame_ms=timeout");
                cx.update(|cx| cx.quit());
            })
        });

        let mut this = Self {
            options,
            settings,
            clusters,
            states: HashMap::new(),
            active: None,
            rail,
            header,
            palette,
            nav: NavState::new(ViewId::Resource(kinds::POD)),
            dock: DockState::default(),
            dock_tabs: Vec::new(),
            port_forwards,
            session: None,
            detail_open: false,
            error: None,
            notice: None,
            _connecting: None,
            _dialog: None,
            _import: None,
            _reload: None,
            settings_window: None,
            _settings_subscription: None,
            _static_subscriptions: subscriptions,
            first_frame_logged: false,
            rows_frame_logged: false,
            _measure_timeout: measure_timeout,
        };
        this.error = this.options.startup_warning.clone().map(SharedString::from);
        if this.options.demo {
            // Every demo cluster shows as connected in the rail: there is
            // nothing to reach.
            for id in this
                .clusters
                .iter()
                .map(|c| c.id.clone())
                .collect::<Vec<_>>()
            {
                this.set_state(id, ConnectionState::Connected, cx);
            }
        }
        if let Some(id) = initial {
            this.select_cluster(id, window, cx);
        }
        this
    }

    // ----- command routing -------------------------------------------------

    /// Routes one [`AppCommand`]. Every feature view reaches the shell through
    /// this method (via GPUI event subscriptions).
    pub fn dispatch(&mut self, command: AppCommand, window: &mut Window, cx: &mut Context<Self>) {
        match command {
            AppCommand::SelectCluster(id) => self.select_cluster(id, window, cx),
            AppCommand::Navigate(view) => self.navigate(view, window, cx),
            AppCommand::OpenDetail(target) => {
                let detail = self.session.as_ref().and_then(|s| s.detail.clone());
                if let Some(detail) = detail {
                    detail.update(cx, |panel, cx| panel.show(target, cx));
                    self.detail_open = true;
                }
                cx.notify();
            }
            AppCommand::CloseDetail => {
                if let Some(detail) = self.session.as_ref().and_then(|s| s.detail.clone()) {
                    detail.update(cx, |panel, cx| panel.clear(cx));
                }
                self.detail_open = false;
                cx.notify();
            }
            AppCommand::OpenLogs { pod, container } => {
                let Some(connection) = self.connection() else {
                    return self.show_error("Connect to a cluster to open logs", cx);
                };
                let prefs = self.settings.logs.clone();
                let view = cx
                    .new(|cx| LogsView::with_prefs(connection, pod, container, &prefs, window, cx));
                self.subscribe_dock(&view, window, cx);
                self.push_dock_tab(DockTab::Logs(view), cx);
            }
            AppCommand::OpenExec { pod, container } => {
                let Some(connection) = self.connection() else {
                    return self.show_error("Connect to a cluster to open a shell", cx);
                };
                let view =
                    cx.new(|cx| TerminalView::pod_exec(connection, pod, container, window, cx));
                self.subscribe_dock(&view, window, cx);
                self.push_dock_tab(DockTab::Terminal(view), cx);
            }
            AppCommand::OpenLocalShell => {
                let shell = self.settings.terminal.shell.clone();
                let view =
                    cx.new(|cx| TerminalView::local_shell_with(shell.as_deref(), window, cx));
                self.subscribe_dock(&view, window, cx);
                self.push_dock_tab(DockTab::Terminal(view), cx);
            }
            AppCommand::StartPortForward(spec) => {
                let Some(connection) = self.connection() else {
                    return self.show_error("Connect to a cluster to forward ports", cx);
                };
                self.port_forwards
                    .update(cx, |view, cx| view.start(connection, spec, cx));
                match self
                    .dock_tabs
                    .iter()
                    .position(|t| matches!(t, DockTab::PortForwards))
                {
                    Some(ix) => {
                        self.dock.activate(ix);
                        if !self.dock.is_open() {
                            self.dock.toggle();
                        }
                        cx.notify();
                    }
                    None => self.push_dock_tab(DockTab::PortForwards, cx),
                }
            }
            AppCommand::ConnectionChanged { cluster, state } => {
                self.set_state(cluster, state, cx);
            }
            AppCommand::NamespaceFilterChanged(filter) => {
                if let Some(id) = self.active.clone() {
                    self.settings.prefs_mut(&id).namespaces = filter.clone();
                    self.persist();
                    if let Some(session) = &self.session {
                        session
                            .resources
                            .update(cx, |view, cx| view.set_namespaces(filter, cx));
                    }
                }
            }
            AppCommand::SetCustomResources(found) => self.set_custom_resources(found, cx),
            AppCommand::ShowError(message) => self.show_error(&message, cx),
            AppCommand::SetAppearance(appearance) => self.set_appearance(appearance, cx),
            AppCommand::OpenPalette => {
                self.refresh_palette(cx);
                self.palette
                    .update(cx, |palette, cx| palette.open(window, cx));
            }
            AppCommand::ClosePalette => self.palette.update(cx, |palette, cx| palette.close(cx)),
            AppCommand::ImportFreelens => self.import_freelens(window, cx),
            AppCommand::AddKubeconfig => self.pick_kubeconfigs(false, window, cx),
            AppCommand::ReloadClusters => self.reload_clusters(window, cx),
            AppCommand::OpenSettings => self.open_settings(window, cx),
            AppCommand::SettingsChanged(prefs) => self.apply_user_prefs(*prefs, window, cx),
            AppCommand::RemoveKubeconfig(path) => self.remove_kubeconfig(path, window, cx),
        }
    }

    fn connection(&self) -> Option<ClusterConnection> {
        self.session.as_ref().and_then(|s| s.connection.clone())
    }

    fn subscribe_dock<T: EventEmitter<AppCommand> + 'static>(
        &mut self,
        view: &Entity<T>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let subscription =
            cx.subscribe_in(view, window, |this, _, command: &AppCommand, window, cx| {
                this.dispatch(command.clone(), window, cx)
            });
        self._static_subscriptions.push(subscription);
    }

    fn push_dock_tab(&mut self, tab: DockTab, cx: &mut Context<Self>) {
        self.dock_tabs.push(tab);
        self.dock.push();
        cx.notify();
    }

    pub(crate) fn close_dock_tab(&mut self, ix: usize, cx: &mut Context<Self>) {
        if ix < self.dock_tabs.len() {
            self.dock_tabs.remove(ix);
            self.dock.close(ix);
            cx.notify();
        }
    }

    pub(crate) fn show_error(&mut self, message: &str, cx: &mut Context<Self>) {
        self.error = Some(message.to_string().into());
        cx.notify();
    }

    pub(crate) fn persist(&self) {
        if let Err(e) = self.options.store.save(&self.settings) {
            eprintln!("settings: {e}");
        }
    }

    // ----- clusters --------------------------------------------------------

    fn set_state(&mut self, id: ClusterId, state: ConnectionState, cx: &mut Context<Self>) {
        self.states.insert(id.clone(), state.clone());
        self.rail
            .update(cx, |rail, cx| rail.set_state(id.clone(), state.clone(), cx));
        if self.active.as_ref() == Some(&id) {
            self.header
                .update(cx, |header, cx| header.set_state(state, cx));
        }
        cx.notify();
    }

    pub(crate) fn select_cluster(
        &mut self,
        id: ClusterId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(info) = self.clusters.iter().find(|c| c.id == id).cloned() else {
            return;
        };
        if self.active.as_ref() == Some(&id)
            && matches!(
                self.states.get(&id),
                Some(ConnectionState::Connected | ConnectionState::Connecting)
            )
        {
            return;
        }
        self.session = None;
        self.detail_open = false;
        self.error = None;
        self.active = Some(id.clone());
        self.rail
            .update(cx, |rail, cx| rail.set_active(Some(id.clone()), cx));
        self.header
            .update(cx, |header, cx| header.set_cluster(Some(info.clone()), cx));
        window.set_window_title(&format!("NatiKube — {}", info.name));

        let prefs = self.settings.prefs(&id);
        if let Some(view) = prefs.last_view.clone() {
            self.nav.navigate(view);
        }

        if let Some(count) = self.options.fake_rows {
            self.set_state(id.clone(), ConnectionState::Connected, cx);
            let view = self.nav.view().clone();
            let resources = cx.new(|cx| ResourcesView::fake(count, view, window, cx));
            self.install_session(None, resources, None, window, cx);
            return;
        }

        if self.options.demo {
            self.set_state(id.clone(), ConnectionState::Connected, cx);
            let view = self.nav.view().clone();
            let namespaces = self.settings.initial_namespaces(&id);
            let resources = cx.new(|cx| ResourcesView::demo(view, namespaces, window, cx));
            self.install_session(None, resources, None, window, cx);
            return;
        }

        self.set_state(id.clone(), ConnectionState::Connecting, cx);
        self._connecting = Some(cx.spawn_in(window, async move |this, cx| {
            let result = ClusterConnection::connect_unverified(&info).await;
            let _ = this.update_in(cx, |this, window, cx| {
                // The user may have picked another cluster while connecting.
                if this.active.as_ref() != Some(&info.id) {
                    return;
                }
                this.finish_connect(info, result, window, cx);
            });
        }));
    }

    fn finish_connect(
        &mut self,
        info: ClusterInfo,
        result: Result<ClusterConnection, natikube_kube_client::ConnectError>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match result {
            Ok(connection) => {
                // The client is built but nothing was sent yet: the rail shows
                // "connecting" until the first list arrives (or fails).
                self.set_state(info.id.clone(), ConnectionState::Connecting, cx);
                self.settings.last_cluster = Some(info.id.clone());
                self.persist();
                let view = self.nav.view().clone();
                let namespaces = self.settings.initial_namespaces(&info.id);
                let poll = Duration::from_secs(u64::from(self.settings.data.metrics_poll_secs));
                let resources = {
                    let connection = connection.clone();
                    cx.new(|cx| {
                        ResourcesView::with_poll_interval(
                            connection, view, namespaces, poll, window, cx,
                        )
                    })
                };
                let detail = {
                    let connection = connection.clone();
                    cx.new(|cx| DetailPanel::new(connection, window, cx))
                };
                self.install_session(Some(connection), resources, Some(detail), window, cx);
            }
            Err(e) => {
                self.set_state(info.id.clone(), ConnectionState::Failed(e.to_string()), cx);
            }
        }
    }

    /// Maps the visible list's health to the connection state: the first list
    /// means connected, an unreachable API server means failed.
    fn sync_connection_state(&mut self, resources: &Entity<ResourcesView>, cx: &mut Context<Self>) {
        let Some(id) = self.active.clone() else {
            return;
        };
        let wanted = match resources.read(cx).load_state(cx) {
            LoadState::Loading => return,
            LoadState::Ready => ConnectionState::Connected,
            LoadState::Unreachable(message) => ConnectionState::Failed(message),
        };
        if self.states.get(&id) != Some(&wanted) {
            self.set_state(id, wanted, cx);
        }
    }

    fn install_session(
        &mut self,
        connection: Option<ClusterConnection>,
        resources: Entity<ResourcesView>,
        detail: Option<Entity<DetailPanel>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let mut subscriptions = vec![
            cx.subscribe_in(
                &resources,
                window,
                |this, _, command: &AppCommand, window, cx| {
                    this.dispatch(command.clone(), window, cx)
                },
            ),
            // Re-render when the table changes so the first-rows frame is seen,
            // and keep the rail dot honest about what the API server said.
            cx.observe(&resources, |this, resources, cx| {
                this.sync_connection_state(&resources, cx);
                cx.notify();
            }),
        ];
        if let Some(detail) = &detail {
            subscriptions.push(cx.subscribe_in(
                detail,
                window,
                |this, _, command: &AppCommand, window, cx| {
                    this.dispatch(command.clone(), window, cx)
                },
            ));
        }
        self.session = Some(Session {
            connection,
            resources,
            detail,
            _subscriptions: subscriptions,
        });
        cx.notify();
    }

    // ----- navigation ------------------------------------------------------

    fn navigate(&mut self, view: ViewId, window: &mut Window, cx: &mut Context<Self>) {
        self.nav.navigate(view.clone());
        if let Some(session) = &self.session {
            let target = view.clone();
            session
                .resources
                .update(cx, |resources, cx| resources.set_view(target, window, cx));
        }
        if let Some(id) = self.active.clone() {
            self.settings.prefs_mut(&id).last_view = Some(view);
            self.persist();
        }
        cx.notify();
    }

    fn set_custom_resources(
        &mut self,
        found: Vec<natikube_core::ResourceKind>,
        cx: &mut Context<Self>,
    ) {
        let mut found = found;
        found.sort_by(|a, b| (&a.group, &a.kind).cmp(&(&b.group, &b.kind)));
        if let Some(section) = self.nav.sections_mut().iter_mut().find(|s| s.dynamic) {
            let definitions = ViewId::Resource(natikube_core::kinds::CUSTOM_RESOURCE_DEFINITION);
            section.entries.retain(|e| e.view == definitions);
            section.entries.extend(
                found
                    .into_iter()
                    .filter(|kind| ViewId::Resource(kind.clone()) != definitions)
                    .map(|kind| SidebarEntry {
                        label: kind_title(&kind),
                        view: ViewId::Resource(kind),
                    }),
            );
        }
        cx.notify();
    }

    // ----- frame measurement ----------------------------------------------

    fn track_frames(&mut self, rows: usize, window: &mut Window) {
        let started_at: Instant = self.options.started_at;
        let exit = self.options.exit_after_first_frame;
        if !self.first_frame_logged {
            self.first_frame_logged = true;
            window.on_next_frame(move |_, _| {
                eprintln!("first_frame_ms={}", started_at.elapsed().as_millis());
            });
        }
        if !self.rows_frame_logged && rows > 0 {
            self.rows_frame_logged = true;
            window.on_next_frame(move |_, cx| {
                eprintln!("first_rows_frame_ms={}", started_at.elapsed().as_millis());
                if exit {
                    cx.quit();
                }
            });
        }
    }

    // ----- rendering -------------------------------------------------------

    fn render_top_tabs(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let tabs = self.nav.tabs();
        if tabs.is_empty() {
            return None;
        }
        let theme = cx.theme();
        let current = self.nav.view().clone();
        let items: Vec<_> = tabs
            .iter()
            .enumerate()
            .map(|(ix, entry)| {
                let selected = entry.view == current;
                let view = entry.view.clone();
                div()
                    .id(("top-tab", ix))
                    .h_full()
                    .px_4()
                    .flex()
                    .items_center()
                    .flex_none()
                    .cursor_pointer()
                    .text_sm()
                    .border_b_2()
                    .border_color(if selected {
                        theme.primary
                    } else {
                        theme.sidebar.opacity(0.0)
                    })
                    .text_color(if selected {
                        theme.foreground
                    } else {
                        theme.muted_foreground
                    })
                    .hover(|s| s.text_color(theme.foreground))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.dispatch(AppCommand::Navigate(view.clone()), window, cx);
                    }))
                    .child(entry.label.clone())
            })
            .collect();
        Some(
            h_flex()
                .id("top-tabs")
                .w_full()
                .h_10()
                .flex_none()
                .overflow_x_scroll()
                .bg(theme.sidebar)
                .border_b_1()
                .border_color(theme.border)
                .children(items)
                .into_any_element(),
        )
    }

    /// The main area. With `inspector` (Native) the detail panel is a fixed
    /// right column that is always present; otherwise it is the resizable
    /// panel that appears on selection.
    pub(crate) fn render_main(&self, inspector: bool, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme();
        let active_state = self
            .active
            .as_ref()
            .and_then(|id| self.states.get(id))
            .cloned();
        match (&self.session, active_state) {
            (Some(session), _) if inspector => {
                let main = session.resources.clone();
                h_flex()
                    .size_full()
                    .child(div().flex_1().min_w_0().h_full().child(main))
                    .child(self.render_inspector(session.detail.clone(), cx))
                    .into_any_element()
            }
            (Some(session), _) => {
                let main = session.resources.clone();
                let detail = session.detail.clone().filter(|_| self.detail_open);
                h_resizable("main-split")
                    .child(resizable_panel().child(main))
                    .child(
                        resizable_panel()
                            .size(px(440.))
                            .size_range(px(320.)..px(900.))
                            .visible(detail.is_some())
                            .when_some(detail, |panel, detail| panel.child(detail)),
                    )
                    .into_any_element()
            }
            (None, Some(ConnectionState::Failed(message))) => div()
                .size_full()
                .bg(theme.background)
                .child(error_state("Could not connect to the cluster", message, cx))
                .into_any_element(),
            (None, Some(ConnectionState::Connecting)) => div()
                .size_full()
                .bg(theme.background)
                .child(empty_state("Connecting…", "Contacting the API server", cx))
                .into_any_element(),
            _ => div()
                .size_full()
                .bg(theme.background)
                .child(empty_state(
                    "No cluster selected",
                    "Add a kubeconfig or pick a cluster in the rail",
                    cx,
                ))
                .into_any_element(),
        }
    }

    pub(crate) fn render_status_bar(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme();
        let tabs = self.dock_tabs.len();
        let dock_open = self.dock.is_open();
        h_flex()
            .w_full()
            .h_6()
            .flex_none()
            .px_3()
            .gap_3()
            .items_center()
            .bg(theme.status_bar)
            .border_t_1()
            .border_color(theme.status_bar_border)
            .text_xs()
            .text_color(theme.muted_foreground)
            .child(
                div()
                    .id("status-dock")
                    .cursor_pointer()
                    .on_click(cx.listener(|this, _, window, cx| {
                        if this.dock_tabs.is_empty() {
                            this.dispatch(AppCommand::OpenLocalShell, window, cx);
                        } else {
                            this.dock.toggle();
                            cx.notify();
                        }
                    }))
                    .child(if tabs == 0 {
                        "Terminal".to_string()
                    } else if dock_open {
                        format!("Hide dock ({tabs})")
                    } else {
                        format!("Show dock ({tabs})")
                    }),
            )
            .child(div().flex_1())
            .when_some(self.notice.clone(), |this, message| {
                this.child(
                    div()
                        .id("status-notice")
                        .cursor_pointer()
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.notice = None;
                            cx.notify();
                        }))
                        .child(message),
                )
            })
            .when_some(self.error.clone(), |this, message| {
                this.child(
                    div()
                        .id("status-error")
                        .cursor_pointer()
                        .text_color(theme.red)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.error = None;
                            cx.notify();
                        }))
                        .child(message),
                )
            })
            .into_any_element()
    }
}

impl ShellView {
    /// Main area plus the bottom dock (resizable split when the dock is open).
    pub(crate) fn render_content(&self, inspector: bool, cx: &mut Context<Self>) -> AnyElement {
        let main = self.render_main(inspector, cx);
        if self.dock.is_open() {
            v_resizable("content-split")
                .child(resizable_panel().child(div().size_full().child(main)))
                .child(
                    resizable_panel()
                        .size(px(280.))
                        .size_range(px(120.)..px(900.))
                        .child(self.render_dock(cx)),
                )
                .into_any_element()
        } else {
            div().size_full().child(main).into_any_element()
        }
    }

    /// Standard appearance: the Freelens-like composition (cluster rail,
    /// sidebar, top tabs, content, status bar). Unchanged by appearances.
    fn render_standard(&self, cx: &mut Context<Self>) -> AnyElement {
        let (background, foreground) = {
            let theme = cx.theme();
            (theme.background, theme.foreground)
        };
        let tabs = self.render_top_tabs(cx);
        let content = self.render_content(false, cx);

        h_flex()
            .size_full()
            .bg(background)
            .text_color(foreground)
            .child(self.rail.clone())
            .child(self.render_sidebar(cx))
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .children(tabs)
                    .child(div().flex_1().min_h_0().child(content))
                    .child(self.render_status_bar(cx)),
            )
            .into_any_element()
    }
}

impl Render for ShellView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let rows = self
            .session
            .as_ref()
            .map(|s| s.resources.read(cx).item_count(cx))
            .unwrap_or(0);
        self.track_frames(rows, window);

        let body = match cx.tokens().appearance {
            Appearance::Standard => self.render_standard(cx),
            Appearance::Command => self.render_command(cx),
            Appearance::Native => self.render_native(cx),
            Appearance::Observatory => self.render_observatory(cx),
        };
        // The palette overlay is the last child so it paints above everything.
        div()
            .relative()
            .size_full()
            .child(body)
            .child(self.palette.clone())
    }
}
