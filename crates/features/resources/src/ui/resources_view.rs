use futures::StreamExt as _;
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::*;
use natikube_core::{AppCommand, NamespaceFilter, ResourceKind, ResourceRef, ViewId, kinds};
use natikube_kube_client::{ClusterConnection, spawn_stream};
use natikube_ui_kit::empty_state;

use std::time::Duration;

use crate::adapters::{POLL_INTERVAL, UNREACHABLE_PREFIX, fetch_crd, watch_crds};
use crate::domain::{ObjectTable, Printer, printer_for, printer_for_custom};
use crate::ui::backend::Backend;
use crate::ui::feed::pump_rows;
use crate::ui::overview::{ClusterOverviewView, WorkloadsOverviewView};
use crate::ui::resource_list::ResourceList;

/// How long the background feeds wait for the visible list before starting
/// anyway.
const FEEDS_GRACE: std::time::Duration = std::time::Duration::from_secs(5);

enum Content {
    List(Entity<ResourceList>),
    Workloads(Entity<WorkloadsOverviewView>),
    Cluster(Entity<ClusterOverviewView>),
    /// Reading the CRD of a custom resource to learn its columns.
    Loading(SharedString),
    /// A view this backend cannot render.
    Placeholder(SharedString),
}

/// The main content area for one cluster: renders whatever [`ViewId`] is
/// selected (resource tables, workloads overview, cluster overview).
///
/// Contract:
/// * built once per cluster connection with [`ResourcesView::new`];
/// * the shell calls [`set_view`](Self::set_view) on sidebar/tab navigation
///   and [`set_namespaces`](Self::set_namespaces) when the filter changes;
/// * it emits [`AppCommand`]s (`OpenDetail`, `Navigate`, `OpenLogs`, ...)
///   as GPUI events, which the shell subscribes to.
///
/// Every watcher and poller belongs to the visible content and is cancelled
/// when the view changes or this view is dropped. Two cluster-wide background
/// feeds live as long as the view: the namespace names (for the filter
/// dropdown) and the CRD list (emitted as `SetCustomResources`).
pub struct ResourcesView {
    backend: Backend,
    view: ViewId,
    namespaces: NamespaceFilter,
    content: Content,
    known_namespaces: Vec<String>,
    custom_kinds: Vec<ResourceKind>,
    /// The API server answered at least one request (namespace feed).
    reached: bool,
    _forward: Vec<Subscription>,
    _feeds: Vec<Task<()>>,
    /// Whether the background feeds were started (see [`Self::kick_feeds`]).
    feeds_started: bool,
    _feed_fallback: Option<Task<()>>,
    _load: Option<Task<()>>,
}

/// What the visible content says about the connection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoadState {
    Loading,
    Ready,
    Unreachable(String),
}

impl EventEmitter<AppCommand> for ResourcesView {}

impl ResourcesView {
    pub fn new(
        connection: ClusterConnection,
        view: ViewId,
        namespaces: NamespaceFilter,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        Self::with_poll_interval(connection, view, namespaces, POLL_INTERVAL, window, cx)
    }

    /// Like [`new`](Self::new) with an explicit metrics poll interval
    /// (Settings → Data).
    pub fn with_poll_interval(
        connection: ClusterConnection,
        view: ViewId,
        namespaces: NamespaceFilter,
        poll: Duration,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        Self::build(
            Backend::Cluster(connection, poll),
            view,
            namespaces,
            window,
            cx,
        )
    }

    /// Changes the metrics poll interval live: the visible list restarts its
    /// poller with the new period. No-op when unchanged.
    pub fn set_poll_interval(
        &mut self,
        poll: Duration,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.backend.poll_interval() == poll || self.backend.connection().is_none() {
            return;
        }
        self.backend.set_poll_interval(poll);
        let view = self.view.clone();
        self.show(view, window, cx);
        cx.notify();
    }

    /// A view over `count` synthetic pods, for `--fake-rows` and previews.
    pub fn fake(count: usize, view: ViewId, window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self::build(
            Backend::Fake { count },
            view,
            NamespaceFilter::All,
            window,
            cx,
        )
    }

    /// A view over the curated demo cluster (`--demo`): every sidebar view is
    /// populated without a cluster. Nothing in it comes from a real cluster.
    pub fn demo(
        view: ViewId,
        namespaces: NamespaceFilter,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut this = Self::build(Backend::Demo, view, namespaces, window, cx);
        this.known_namespaces = crate::domain::demo_namespaces();
        if let Content::List(list) = &this.content {
            let known = this.known_namespaces.clone();
            list.update(cx, |list, cx| list.set_known_namespaces(known, cx));
        }
        this
    }

    fn build(
        backend: Backend,
        view: ViewId,
        namespaces: NamespaceFilter,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut this = Self {
            backend,
            view: view.clone(),
            namespaces,
            content: Content::Placeholder(SharedString::default()),
            known_namespaces: Vec::new(),
            custom_kinds: Vec::new(),
            reached: false,
            _forward: Vec::new(),
            _feeds: Vec::new(),
            feeds_started: false,
            _feed_fallback: None,
            _load: None,
        };
        this.show(view, window, cx);
        this.kick_feeds(cx);
        // If the visible list never loads (forbidden, unreachable), the feeds
        // still start after a grace period.
        this._feed_fallback = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(FEEDS_GRACE).await;
            this.update(cx, |this, cx| this.start_feeds_now(cx)).ok();
        }));
        this
    }

    pub fn view(&self) -> &ViewId {
        &self.view
    }

    pub fn namespaces(&self) -> &NamespaceFilter {
        &self.namespaces
    }

    /// Switches to another view, cancelling the previous view's watchers.
    pub fn set_view(&mut self, view: ViewId, window: &mut Window, cx: &mut Context<Self>) {
        if self.view == view {
            return;
        }
        self.view = view.clone();
        self.show(view, window, cx);
        cx.notify();
    }

    /// Updates the namespace filter and applies it to the visible content.
    pub fn set_namespaces(&mut self, namespaces: NamespaceFilter, cx: &mut Context<Self>) {
        if self.namespaces == namespaces {
            return;
        }
        self.namespaces = namespaces.clone();
        match &self.content {
            Content::List(list) => {
                list.update(cx, |list, cx| list.set_namespaces(namespaces, cx));
            }
            Content::Workloads(overview) => {
                overview.update(cx, |view, cx| view.set_namespaces(namespaces, cx));
            }
            _ => {}
        }
        cx.notify();
    }

    /// Whether the cluster answers, judged from the visible content: still
    /// loading, loaded, or the API server could not be reached.
    pub fn load_state(&self, cx: &App) -> LoadState {
        let Content::List(list) = &self.content else {
            // Overviews and placeholders have no list to judge: only a real
            // answer from the API server (the namespace feed) counts, so the
            // rail never says Connected for a cluster nobody reached.
            return overview_state(self.backend.connection().is_some(), self.reached);
        };
        let list = list.read(cx);
        match list.error() {
            Some(error) if error.starts_with(UNREACHABLE_PREFIX) => {
                LoadState::Unreachable(error.to_string())
            }
            _ if list.is_loaded() => LoadState::Ready,
            _ => LoadState::Loading,
        }
    }

    /// Rows currently shown by the active table (0 for overviews). Used by
    /// the shell's first-frame measurement.
    pub fn item_count(&self, cx: &App) -> usize {
        match &self.content {
            Content::List(list) => list.read(cx).item_count(cx),
            _ => 0,
        }
    }

    /// References to the objects of the visible list (empty for overviews),
    /// so the shell can offer them in the command palette.
    pub fn loaded_refs(&self, cx: &App) -> Vec<ResourceRef> {
        match &self.content {
            Content::List(list) => list.read(cx).loaded_refs(cx),
            _ => Vec::new(),
        }
    }

    /// Starts the background feeds once the visible content has its first
    /// data. Each new connection can be slow (tunnels, TLS), and the first
    /// request after the probe reuses the pooled connection, so the visible
    /// list goes first and the feeds follow instead of racing it.
    fn kick_feeds(&mut self, cx: &mut Context<Self>) {
        let ready = match &self.content {
            Content::List(list) => list.read(cx).is_loaded(),
            _ => true,
        };
        if ready {
            self.start_feeds_now(cx);
        }
    }

    fn start_feeds_now(&mut self, cx: &mut Context<Self>) {
        if !self.feeds_started {
            self.feeds_started = true;
            self._feeds = Self::start_feeds(&self.backend, cx);
            self._feed_fallback = None;
        }
    }

    /// Background feeds: namespace names and CRD discovery (clusters only).
    fn start_feeds(backend: &Backend, cx: &mut Context<Self>) -> Vec<Task<()>> {
        let Some(conn) = backend.connection() else {
            return Vec::new();
        };
        let printer = printer_for(&kinds::NAMESPACE);
        let source = backend.source(&kinds::NAMESPACE, &printer);
        let mut names = ObjectTable::new(printer.columns().to_vec());
        let namespace_feed = pump_rows(source.as_ref(), cx, move |this: &mut Self, batch, cx| {
            let mut answered = false;
            for item in batch {
                match item {
                    Ok(event) => {
                        answered = true;
                        names.apply(event);
                    }
                    // A refusal (for example no permission to list namespaces)
                    // still proves the API server answered.
                    Err(e) if !e.to_string().starts_with(UNREACHABLE_PREFIX) => answered = true,
                    Err(_) => {}
                }
            }
            if answered && !this.reached {
                this.reached = true;
                cx.notify();
            }
            let list: Vec<String> = names.all_rows().iter().map(|r| r.name.clone()).collect();
            if list != this.known_namespaces {
                this.known_namespaces = list.clone();
                if let Content::List(view) = &this.content {
                    view.update(cx, |view, cx| view.set_known_namespaces(list, cx));
                }
            }
        });

        let mut crds = spawn_stream(watch_crds(conn));
        let crd_feed = cx.spawn(async move |this, cx| {
            while let Some(result) = crds.next().await {
                // Discovery failures (no permission to list CRDs) leave the
                // Custom Resources section as it was.
                let Ok(found) = result else { continue };
                let kinds: Vec<ResourceKind> = found.into_iter().map(|c| c.kind).collect();
                let alive = this.update(cx, |this, cx| {
                    if this.custom_kinds != kinds {
                        this.custom_kinds = kinds.clone();
                        cx.emit(AppCommand::SetCustomResources(kinds));
                    }
                });
                if alive.is_err() {
                    break;
                }
            }
        });
        vec![namespace_feed, crd_feed]
    }

    fn forward<T: EventEmitter<AppCommand> + 'static>(
        &mut self,
        entity: &Entity<T>,
        cx: &mut Context<Self>,
    ) {
        self._forward
            .push(cx.subscribe(entity, |this, _, command: &AppCommand, cx| {
                if let AppCommand::NamespaceFilterChanged(filter) = command {
                    this.namespaces = filter.clone();
                }
                cx.emit(command.clone());
            }));
        // Bubble content changes up so the shell can observe row counts.
        self._forward.push(cx.observe(entity, |this, _, cx| {
            this.kick_feeds(cx);
            cx.notify();
        }));
    }

    fn build_list(
        &mut self,
        kind: ResourceKind,
        printer: Printer,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let backend = self.backend.clone();
        let is_pod = kind == kinds::POD;
        let source = backend.source(&kind, &printer);
        let usage = backend.usage(&kind);
        let cluster = backend.cluster_id();
        let namespaces = self.namespaces.clone();
        let known = self.known_namespaces.clone();
        let list = cx.new(|cx| {
            ResourceList::new(
                cluster, kind, printer, source, usage, namespaces, window, cx,
            )
        });
        list.update(cx, |list, cx| list.set_known_namespaces(known, cx));
        if is_pod {
            // Node capacity feeds the Observatory KPI cards; watched lazily.
            let nodes = backend.source(&kinds::NODE, &printer_for(&kinds::NODE));
            list.update(cx, |list, _| list.set_nodes_source(nodes));
        }
        self._forward.clear();
        self.forward(&list, cx);
        self.content = Content::List(list);
    }

    fn show(&mut self, view: ViewId, window: &mut Window, cx: &mut Context<Self>) {
        self._forward.clear();
        self._load = None;
        match &view {
            ViewId::ClusterOverview => {
                let backend = self.backend.clone();
                let overview = cx.new(|cx| ClusterOverviewView::new(&backend, window, cx));
                self.forward(&overview, cx);
                self.content = Content::Cluster(overview);
            }
            ViewId::WorkloadsOverview => {
                let backend = self.backend.clone();
                let namespaces = self.namespaces.clone();
                let overview =
                    cx.new(|cx| WorkloadsOverviewView::new(&backend, namespaces, window, cx));
                self.forward(&overview, cx);
                self.content = Content::Workloads(overview);
            }
            ViewId::Resource(kind) => self.show_resource(kind.clone(), window, cx),
        }
    }

    fn show_resource(&mut self, kind: ResourceKind, window: &mut Window, cx: &mut Context<Self>) {
        let builtin = kinds::all_builtin().contains(&kind);
        if builtin || matches!(self.backend, Backend::Fake { .. } | Backend::Demo) {
            // Fake mode only renders built-in kinds (synthetic pods, empty rest).
            let printer = if builtin {
                printer_for(&kind)
            } else {
                printer_for_custom(&kind, &[])
            };
            self.build_list(kind, printer, window, cx);
            return;
        }
        let Some(conn) = self.backend.connection().cloned() else {
            return;
        };
        // A custom resource: read its CRD for the printer columns first.
        self.content = Content::Loading(format!("Reading {}…", kind.kind).into());
        let requested = ViewId::Resource(kind.clone());
        self._load = Some(cx.spawn_in(window, async move |this, cx| {
            let client = conn.client().clone();
            let target = kind.clone();
            let fetched = conn
                .run(move |_| async move { fetch_crd(client, &target).await })
                .await;
            let columns = match fetched {
                Ok(Ok(info)) => info.columns,
                // No access to the CRD: Name/Namespace/Age still work.
                _ => Vec::new(),
            };
            this.update_in(cx, |this, window, cx| {
                if this.view == requested {
                    let printer = printer_for_custom(&kind, &columns);
                    this.build_list(kind, printer, window, cx);
                    cx.notify();
                }
            })
            .ok();
        }));
    }
}

impl Render for ResourcesView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let (background, foreground) = (theme.background, theme.foreground);
        let body = match &self.content {
            Content::List(list) => list.clone().into_any_element(),
            Content::Workloads(view) => view.clone().into_any_element(),
            Content::Cluster(view) => view.clone().into_any_element(),
            Content::Loading(message) => {
                empty_state(self.view.title(), message.clone(), cx).into_any_element()
            }
            Content::Placeholder(message) => {
                empty_state(self.view.title(), message.clone(), cx).into_any_element()
            }
        };
        div()
            .size_full()
            .bg(background)
            .text_color(foreground)
            .child(body)
    }
}

/// Health of a view that has no list of its own (overviews, loading
/// placeholders): synthetic backends are always ready, a real connection only
/// once the API server has answered.
pub(crate) fn overview_state(has_connection: bool, reached: bool) -> LoadState {
    if !has_connection || reached {
        LoadState::Ready
    } else {
        LoadState::Loading
    }
}

#[cfg(test)]
mod overview_state_tests {
    use super::{LoadState, overview_state};

    #[test]
    fn overview_is_ready_only_after_the_api_server_answered() {
        assert!(matches!(overview_state(true, false), LoadState::Loading));
        assert!(matches!(overview_state(true, true), LoadState::Ready));
        assert!(matches!(overview_state(false, false), LoadState::Ready));
    }
}
