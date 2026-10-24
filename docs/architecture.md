# NatiKube architecture

100% native macOS arm64 Kubernetes desktop app: Rust + GPUI Kit, no webview, no
Electron. Screaming + hexagonal layout: each feature is a vertical slice with
`domain/` (pure, tested), `adapters/` (kube) and `ui/` (GPUI views).

## Crates

| Crate | Path | Role |
| --- | --- | --- |
| `natikube-core` | `crates/core` | Shared kernel. No I/O, no gpui, no kube. IDs, `ResourceKind`/`ResourceRef`, `ViewId` + sidebar model, `AppCommand`, `Settings` + `SettingsStore` port, formatting and quantity parsing. |
| `natikube-kube-client` | `crates/kube-client` | `ClusterConnection` (kube client for one kubeconfig context) and the shared Tokio runtime on background threads, with helpers that bridge tasks and streams into GPUI. No UI. |
| `natikube-ui-kit` | `crates/ui-kit` | Appearance tokens (`Tokens`, `set_appearance`), Freelens-like dark theme and shared widgets. Depends on `core` (for `Appearance`) and `gpui-kit`. |
| `natikube-clusters` | `crates/features/clusters` | F1: kubeconfig catalog, cluster rail, header, file settings store. |
| `natikube-resources` | `crates/features/resources` | F2: resource browser (owns the F0 pods logic). |
| `natikube-detail` | `crates/features/detail` | F3: right detail panel, YAML, apply/delete/scale. |
| `natikube-logs` | `crates/features/logs` | F4: log tabs. |
| `natikube-portforward` | `crates/features/portforward` | F6: port-forward list and listeners. |
| `natikube-terminal` | `crates/features/terminal` | F5: local shell and pod exec terminals. |
| `natikube-palette` | `crates/features/palette` | F7: command palette (⌘K) overlay. |
| `natikube-settings` | `crates/features/settings` | F8: Settings window (⌘,). |
| `natikube-alerts` | `crates/features/alerts` | F9: alert rules and evaluation, SQLite history, kube monitor, OS notifications, header bell and the Settings > Alerts page. |
| `natikube-updates` | `crates/features/updates` | Startup update check: GitHub Releases fetch (hyper + rustls), semver selection, the dismissible "update available" modal. |
| `natikube-shell` | `crates/shell` | Window layout and `AppCommand` routing. |
| `natikube-app` | `crates/app` | Binary `natikube`: CLI flags, catalog and settings loading. |

### Dependency rules

```
core  <-  kube-client  <-  features/*  <-  shell  <-  app
core  <-  ui-kit(core + gpui-kit only)  <-  features/*
```

* `core` depends on nothing of ours (serde, thiserror only).
* `ui-kit` depends on `core` and `gpui-kit` only.
* Feature crates depend on `core`, `kube-client`, `ui-kit` and `gpui-kit`
  (plus external crates already declared). **They never depend on each other.**
  Cross-feature interaction goes through `AppCommand`.
* `shell` depends on every feature; `app` depends on `shell` and the pieces it
  wires (`clusters` for catalog/store, `resources` for `--dump`).
* Never add a web runtime: `wry`, `tauri`, `webview`, `gpui-webview`,
  `gpui-pre-web`, `wasm-bindgen` are forbidden. Verified absent from
  `cargo tree --target aarch64-apple-darwin`.

## Working in parallel (read before touching anything)

* Every dependency any stream needs is **already declared** in the root
  `[workspace.dependencies]` and in every crate's `Cargo.toml`
  (`kube`, `kube-runtime`, `k8s-openapi`, `tokio`, `futures`, `serde`,
  `serde_json`, `serde_yaml_ng`, `chrono`, `dirs`, `thiserror`, `anyhow`,
  `tracing`, `gpui-kit`; plus `json-patch` and `rfd` in detail, logs, clusters and shell (open panels), `natikube-palette` and `natikube-settings` in shell,
  `alacritty_terminal` and `portable-pty` in terminal; dev: `tempfile`).
  **Do not edit `Cargo.toml` or `Cargo.lock`.** If something is truly missing,
  report it to the integrator instead.
* YAML highlighting needs no extra crate: the detail crate enables
  `gpui-kit`'s `tree-sitter-yaml` feature (GPUI Kit's editor already provides
  Tree-sitter YAML). `tree-sitter` itself is pulled in transitively.
* Each writer owns exactly one `crates/features/<name>/**` directory. Do not
  edit other crates, `core`, `ui-kit`, `shell` or `app`. Need an `AppCommand`
  variant or a ui-kit widget? Ask the integrator.
* All crates share one `target/` directory. Run cargo with an absolute
  manifest path and scope to your crate so builds stay short:
  `cargo test --manifest-path <repo>/Cargo.toml -p natikube-<crate>`.
  Cargo serializes concurrent builds with a lock; waiting is normal. Never run
  `cargo clean`, `cargo update` or touch `Cargo.lock`.
* Public items re-exported from your crate root are your contract. Keep the
  signatures below stable; add, do not rename.

## Performance experiments (outcome)

Workspace deps `mimalloc` (default-features off), `dhat` and `smallvec` were declared up front. `crates/app` has optional
features `mimalloc` and `dhat-heap` (off by default) with the `#[global_allocator]` / profiler hooks in `main.rs`.
Measured with `cargo build --release -p natikube-app --features mimalloc`: no gain (+218 KB, +3.5 MB RSS, same footprint),
so the default allocator stays. `opt-level = "s"` for dependencies saves 2.9 MB of disk but slows the first frame by
25-50 ms and was not adopted. The full table is in `odd/tasks/settings-perf-polish.md`. `smallvec` is a dependency of
`natikube-resources` and `natikube-kube-client`.

Design notes that came out of the pass:

* Time to first rows: `ClusterConnection::connect_unverified` skips the `GET /version` probe, the visible list starts
  first and the namespace/CRD feeds and the metrics poller start after its first load (an SSH-tunnelled API server pays
  0.3-5 s per extra TCP connection). The shell derives the rail dot from `ResourcesView::load_state`.
* Idle CPU: relative ages tick at the finest unit that is on screen (1 s, 60 s, 1 h); batches that change nothing do not
  notify.
* GPU memory dominates the footprint (3 drawables x window pixels, hard-coded in gpui); only the window size moves it.
* UI scale: `Tokens::scale = font_size / 13 * zoom` drives the GPUI rem size, row/header heights, table column widths and
  sidebar widths; `ui_kit::set_ui_prefs` re-applies the active appearance live.

## Threading model

* GPUI runs on the main thread. All network I/O runs on the shared Tokio
  runtime owned by `natikube-kube-client` (`runtime_handle()`).
* From a GPUI task, `await` the `JoinHandle` returned by
  `ClusterConnection::run(|client| async move { ... })` or `spawn(...)`.
* For watchers and streams use `ClusterConnection::watch(|client| stream)` or
  `spawn_stream(stream)`. They return a `StreamHandle<T>` (a `Stream` with
  `try_recv`). **Dropping the handle cancels the producer.** Move the handle
  into a GPUI `Task` owned by the view, so dropping the view (navigation)
  cancels the watcher. See `PodsList` for the reference pattern (batch with
  `try_recv`, redraw at most every ~80 ms).
* One watcher per visible view; cancel on navigation.

## `AppCommand` (core)

```rust
pub enum AppCommand {
    SelectCluster(ClusterId),
    Navigate(ViewId),
    OpenDetail(ResourceRef),
    CloseDetail,
    OpenLogs { pod: ResourceRef, container: Option<String> },
    OpenExec { pod: ResourceRef, container: Option<String> },
    OpenLocalShell,
    StartPortForward(PortForwardSpec),
    ConnectionChanged { cluster: ClusterId, state: ConnectionState },
    NamespaceFilterChanged(NamespaceFilter),
    SetCustomResources(Vec<ResourceKind>),
    ShowError(String),
    SetAppearance(Appearance),
    OpenPalette,
    ClosePalette,
    ImportFreelens,
    AddKubeconfig,
    ReloadClusters,
    OpenSettings,
    SettingsChanged(Box<UserPrefs>),
    RemoveKubeconfig(PathBuf),
    AlertsChanged,
    OpenAlerts,
    OpenAlertObject(AlertTarget),
    AlertsConfigChanged(Box<AlertsConfig>),
    SendTestNotification,
    ClearAlertHistory,
}
pub struct PortForwardSpec { pub target: ResourceRef /* pod or service */, pub remote_port: u16, pub local_port: Option<u16> }
```

Views emit commands as GPUI events: `impl EventEmitter<AppCommand> for MyView {}`
and `cx.emit(AppCommand::...)`. The shell subscribes (`cx.subscribe_in`) to
every feature view and routes in `ShellView::dispatch`:

| Command | Shell behavior |
| --- | --- |
| `SelectCluster` | connects (`ClusterConnection::connect`), swaps the session (drops old watchers), updates rail/header, persists last cluster |
| `Navigate` | `ResourcesView::set_view`, sidebar/tab highlight, persists last view per cluster |
| `OpenDetail` / `CloseDetail` | `DetailPanel::show`, opens/closes the right panel |
| `OpenLogs` / `OpenExec` / `OpenLocalShell` | creates `LogsView` / `TerminalView`, adds a dock tab |
| `StartPortForward` | `PortForwardsView::start(conn, spec)`, reveals the tab |
| `ConnectionChanged` | updates rail/header state |
| `NamespaceFilterChanged` | persists per cluster and calls `ResourcesView::set_namespaces` |
| `SetCustomResources` | fills the sidebar's Custom Resources section |
| `ShowError` | status-bar message (click to dismiss) |
| `OpenSettings` | `open_settings` (shell_ops: `settings_ops.rs`): opens the Settings window or focuses the existing one; subscribes to its commands |
| `SettingsChanged(prefs)` | `apply_user_prefs`: clamps, no-op if equal, persists, applies appearance live via `set_appearance`, pushes the new `Settings` back with `SettingsView::set_settings`. Fonts/density/logs/poll consumers read `ShellView.settings` (wired by their owners) |
| `RemoveKubeconfig(path)` | drops the path from `extra_kubeconfigs`, persists, reloads the catalog |
| `SetAppearance` | stores `settings.appearance`, persists, calls `natikube_ui_kit::set_appearance` (live re-render) |
| `OpenPalette` / `ClosePalette` | shows/hides the `PaletteView` overlay (⌘K) |
| `ImportFreelens` | runs the clusters Freelens import, appends to `extra_kubeconfigs`, reloads the catalog, shows a summary |
| `AddKubeconfig` | native open panel (`rfd`), appends file/folder to `extra_kubeconfigs`, reloads the catalog |
| `ReloadClusters` | rescans kubeconfig files (`load_catalog`) and refreshes rail/header/palette |

| `AlertsChanged` | refreshes every bell (badge and, when open, the list) |
| `OpenAlerts` | opens the bell panel (View > Notifications, `Cmd/Ctrl+Shift+N`) |
| `OpenAlertObject(target)` | `SelectCluster`, then opens the object with `OpenDetail` once that cluster's session exists (only switches cluster when the kind is not built in) |
| `AlertsConfigChanged(config)` | clamps, stores in `Settings.alerts`, persists, then `AlertMonitor::apply`, `set_os_enabled`, `set_retention` |
| `SendTestNotification` | `AlertDispatcher::send_test_notification` off the UI thread (ignores the OS switch) |
| `ClearAlertHistory` | `AlertStore::clear`, then refreshes the bells |

## Core types (what features rely on)

* `Appearance { Standard, Command, Native, Observatory }` (`#[default] Standard`, serde as lowercase strings,
  `label()` = menu text (`macOS Native`), `slug()` = `standard|command|native|observatory`, `all()` in menu order,
  `from_slug(&str)` case-insensitive, `macos` aliases `native`).
* `ClusterId(String)`, `ClusterInfo { id, context, name, kubeconfig: Option<PathBuf>, server, namespace, also_in: Vec<PathBuf> }`
  (`also_in` = other files with the identical context folded in by dedupe; `source_file_name() -> Option<String>`)
  (`ClusterInfo::from_context(name)`, `ClusterInfo::new(context, kubeconfig)`, `.initials()` e.g. `KD`),
  `ConnectionState { Disconnected, Connecting, Connected, Failed(String) }`.
* `ResourceKind { group, version, kind, plural, namespaced }` with `Cow<'static, str>`
  fields; built-ins are `const`s in `natikube_core::kinds` (`kinds::POD`, `DEPLOYMENT`, ...,
  `kinds::all_builtin()`, `kinds::find("pods")`); CRDs use `ResourceKind::custom(...)`.
  `api_version()`, `is_core()`.
* `ResourceRef { cluster, kind, namespace: Option<String>, name }`, `Display` = `Pod default/api-1`.
* `ViewId { ClusterOverview, WorkloadsOverview, Resource(ResourceKind) }`, `.title()`;
  `sidebar()` returns the Freelens-ordered `Vec<SidebarSection>`; `kind_title(&kind)`.
* `Settings { extra_kubeconfigs, last_cluster, appearance, ui: UiPrefs, terminal: TerminalPrefs, logs: LogPrefs, data: DataPrefs, updates: UpdatePrefs, clusters: BTreeMap<String, ClusterPrefs{ namespaces, last_view }> }`.
  Parsing is tolerant per field (wrong type or unknown appearance -> that field's default; the file is never discarded)
  and numbers are clamped on load and in `apply_user_prefs`. Defaults and ranges (all `pub const` ranges on the structs):
  `UiPrefs { font_size: u8 = 13 (10..=20), zoom_percent: u16 = 100 (75..=150), density: Density = Comfortable (Comfortable|Compact, serde lowercase),
  mono_font: Option<String> = None (system mono), mono_font_size: u8 = 12 (8..=20) }`,
  `TerminalPrefs { font_size: u8 = 12 (8..=24), shell: Option<String> = None ($SHELL, else /bin/zsh) }`,
  `LogPrefs { tail_lines: u32 = 1000 (10..=100000), timestamps: bool = false, wrap: bool = false }` (defaults for new log tabs),
  `DataPrefs { metrics_poll_secs: u16 = 15 (5..=300), default_namespace: Option<String> = None (all) }`,
  `UpdatePrefs { check_on_startup: bool = true, skipped_version: Option<String> = None }`. Blank strings normalize to `None`.
  `Settings::user_prefs() -> UserPrefs { appearance, ui, terminal, logs, data, updates }` and `apply_user_prefs(UserPrefs)` (clamps; keeps clusters/last_cluster/extra_kubeconfigs).
  `SettingsStore` port (`load`/`save`), `MemorySettingsStore`. `NamespaceFilter { All, Only(BTreeSet<String>) }`
  with `matches(ns)` and `label()`.
* Formatting: `format_age`/`age_since` (kubectl style), `format_cpu(cores)` (`0.007`),
  `format_memory(bytes)` (`34.6Mi`), `parse_quantity`, `parse_cpu_cores`, `parse_memory_bytes`.

## `kube-client` API

```rust
ClusterConnection::connect(&ClusterInfo) -> Result<ClusterConnection, ConnectError>  // async, any executor
ClusterConnection::from_client(ClusterInfo, kube::Client) -> ClusterConnection
conn.id() / conn.info() / conn.client()        // cheap Clone (Arc inside)
conn.run(|client| async move { .. }) -> tokio::task::JoinHandle<T>
conn.watch(|client| stream) -> StreamHandle<T>
runtime_handle(), spawn(fut) -> JoinHandle, spawn_stream(stream) -> StreamHandle<T>
```

## `ui-kit` API

`apply_theme(cx)` (= `set_appearance(Standard)`; the shell calls
`set_appearance(saved or --appearance)` once after `gpui_kit::init`), `Palette::freelens_dark()`,
`hex(0xRRGGBB)`, `Tone { Ok, Done, Warning, Error, Muted }` with `.color(cx)`.
Widgets: `status_dot(tone, cx)`, `status_text(label, tone, cx)`,
`link_text(id, label, on_click, cx)`, `container_squares(&[Tone], cx)`,
`section_header(title, cx)`, `empty_state(title, msg, cx)`,
`error_state(title, msg, cx)`, `initials_avatar(id, initials, active, health, cx)`.
Read colors from `cx.theme()` (`gpui_kit::component::ActiveTheme`), never hard-code hex in features.

### Appearance tokens (ui-kit contract)

```rust
set_appearance(Appearance, &mut App)   // live: swaps tokens, reinstalls GPUI Kit theme, cx.refresh_windows()
sync_system_appearance(&mut App)       // call on macOS light/dark change; only acts when Native is active
cx.tokens() -> &Tokens                 // trait ActiveTokens, implemented for App (Context<T> derefs to it)
Tokens::for_appearance(Appearance, dark: bool) -> Tokens ; Tokens::standard()
Tokens { appearance, dark, palette: Palette, radii: Radii{sm,md,lg: Pixels}, fonts: Fonts{ui,mono: SharedString},
         font_size, row_height, header_height: Pixels,
         stacked_names, zebra, usage_bars, group_by_namespace, status_pills, inspector_detail: bool }
```

* `Standard` tokens equal the pre-appearance UI and never change. Command sets
  `stacked_names + usage_bars + status_pills`; Native sets `zebra + inspector_detail`
  and follows macOS light/dark (`dark`); Observatory sets `group_by_namespace`.
* Palette/radii/fonts/heights of the three new appearances are filled by the
  ui-kit stream from the canvas files. Additive changes only (new fields get a
  Standard default); never rename.
* Fonts: GPUI resolves families from the system. Tokens use `.SystemUIFont` /
  `Menlo` for Standard and may name SF Pro / SF Mono for the others; embedding
  Instrument Sans / IBM Plex Sans / JetBrains Mono (OFL) is optional and never
  blocks.
* Feature code reads `cx.tokens()` to choose layout (flags) and values; the
  GPUI Kit `cx.theme()` colors keep working in every appearance.

## Feature contracts

All views are GPUI entities that implement `Render` and
`EventEmitter<AppCommand>`. Constructors take the `ClusterConnection` (when
they need one) plus `window, cx` and are created by the shell with
`cx.new(|cx| View::new(..., window, cx))`. Dropping a view must stop its
tasks and streams. Each feature crate has `domain/` (pure + tests),
`adapters/` (kube) and `ui/` (views). Write domain/adapter tests first.

### clusters (F1), `natikube_clusters`

Exposes (already stubbed or working; keep signatures):

* `load_catalog(&Settings) -> Vec<ClusterInfo>` (K1/K2 stream turns it into full discovery with dedupe, filling `also_in`; signature stays), `contexts_in_file(&Path)`, `current_context()`.
* `FileSettingsStore::new(path)`, `FileSettingsStore::default_location()`; implements `SettingsStore`.
* `init_environment()`: called first thing by `main`; resolves the login-shell `PATH` on macOS/Linux when the inherited one is minimal (F1.3). On Windows it sets `KUBE_RS_UNSTABLE_CREATE_NO_WINDOW=1` (the exact variable `kube-client` 4.2 reads, value `1`) before any thread exists, so exec credential plugins (`aws`, `kubelogin`) do not flash a console window from the GUI-subsystem process; a value the user already set is respected. The decision is the pure `no_window_value`.
* `ClusterRail::new(Vec<ClusterInfo>, Option<ClusterId>)` with `set_clusters`, `set_active`, `set_state`; emits `SelectCluster`.
* `ClusterHeader::new(Option<ClusterInfo>, ConnectionState)` with `set_cluster`, `set_state`; emits `SelectCluster` when a cluster is picked from its dropdown.
* `ClusterList` pure state, `RAIL_WIDTH`.

To implement: F1.1 hardening (`$KUBECONFIG` lists, folders, dedup), F1.2 polish
(connection-state tooltips, errors), header dropdown, F1.3 `init_environment`,
F1.4 settings persistence details (atomic write already done).

### resources (F2), `natikube_resources`

Exposes:

* `ResourcesView::new(conn: ClusterConnection, view: ViewId, namespaces: NamespaceFilter, window, cx)`,
  `ResourcesView::fake(count, view, window, cx)` (synthetic pods for `--fake-rows`),
  `set_view(ViewId, window, cx)` (must cancel previous watchers), `set_namespaces(NamespaceFilter, cx)`,
  `item_count(&App) -> usize` (rows of the active table; used by the shell's first-rows timing and must stay cheap),
  `view()`, `namespaces()`.
* Emits: `OpenDetail(ResourceRef)` on row select, `Navigate(ViewId)` (owner/namespace links),
  `OpenLogs`/`OpenExec`/`StartPortForward` from row menus, `NamespaceFilterChanged`,
  `SetCustomResources(kinds)` after CRD discovery, `ShowError`.
* `list_names(&ClusterConnection, &ResourceKind) -> Result<Vec<String>, SourceError>`:
  backs `natikube --dump <kind>` (`namespace/name` or `name` lines, sorted). Must
  work for every sidebar kind (parity with `kubectl --context dev-cluster get <kind> -A`).
* Domain re-exports from F0: `PodRow`, `PodTable`, `PodEvent`, `PodSource`, status/age derivations, `synthetic_rows`.
* `PodsList` is the reference table view; `KubePodSource` the reference adapter.

To implement: F2.1 sidebar kinds (the sidebar itself lives in the shell and is
driven by `core::sidebar()`), F2.2 generic table engine over discovery and
dynamic objects with kubectl-like columns, F2.3 namespace dropdown, search box,
sorting, "N items", F2.4 pod CPU/memory from `metrics.k8s.io`, F2.5 Workloads
and Cluster overviews. Today only `ViewId::Resource(kinds::POD)` renders; other
views show an empty-state placeholder. Namespace filtering is stored but not yet
applied to rows.

### detail (F3), `natikube_detail`

* `DetailPanel::new(conn: ClusterConnection, window, cx)`, `show(&mut self, ResourceRef, cx)`,
  `target()`, `connection()`.
* Emits `CloseDetail` (close button), `OpenDetail` (owner links), `OpenLogs`,
  `OpenExec`, `StartPortForward`, `ShowError`.
* The panel is a resizable column owned by the shell; it should fill its bounds
  and render its own header with a close button (the stub shows the pattern).

To implement: F3.1 per-kind detail sections, F3.2 YAML viewer/editor (GPUI Kit
editor + Tree-sitter YAML; `rfd` for save dialogs), F3.3 apply with server-side
dry-run first, delete with confirmation, scale, rollout restart, Secret
masking with reveal toggle. Write operations are unit-tested only; agents never
run them against a real cluster.

### logs (F4), `natikube_logs`

* `LogsView::new(conn: ClusterConnection, pod: ResourceRef, container: Option<String>, window, cx)`,
  `title() -> SharedString` (dock tab label), `connection()`.
* Emits `ShowError`.

To implement: F4.1 follow stream, container selector, previous container,
timestamps toggle, search, save to file (`rfd`). Cancel the stream on drop.

### terminal (F5), `natikube_terminal`

* `TerminalView::local_shell(window, cx)`,
  `TerminalView::pod_exec(conn, pod: ResourceRef, container: Option<String>, window, cx)`,
  `title()`, `connection() -> Option<&ClusterConnection>`.
* Emits `ShowError`.

To implement: own grid renderer on `alacritty_terminal` (do not copy Zed's GPL
terminal code), `portable-pty` local shell, kube websocket exec, resize,
copy/paste. Exec is unit-tested only; agents never exec against a real cluster.

### palette (F7), `natikube_palette`

* `PaletteView::new(window, cx)`, `set_context(PaletteContext, cx)`, `open(window, cx)`,
  `close(cx)`, `is_open()`, `context()`.
* `PaletteContext { clusters, active_cluster, kinds, resources }`, `PaletteItem { label, hint, command: AppCommand }`.
* Emits `SelectCluster`, `Navigate`, `OpenDetail`, `SetAppearance`, `ImportFreelens`,
  `AddKubeconfig`, then `ClosePalette`. Paints nothing while closed. No fuzzy-matcher
  crate: write a small subsequence scorer in `domain/` (tests first).

### settings (F8), `natikube_settings`

* `open_settings_window(settings: Settings, build_extra: impl FnOnce(&mut Window, &mut App) -> Option<ExtraPage>, cx: &mut App) -> anyhow::Result<(AnyWindowHandle, Entity<SettingsView>)>` (760x560, title "Settings").
* `SettingsView::new(settings, extra, window, cx)`, `set_settings(Settings, cx)` (shell pushes external changes), `settings()`, `section()`, `select_section(SettingsSection, cx)`.
  `SettingsSection { Appearance, Clusters, Terminal, Logs, Data, Alerts }` with `label()` and `all()`.
* `open_settings_window(settings: Settings, cx: &mut App) -> anyhow::Result<(AnyWindowHandle, Entity<SettingsView>)>` (760x560, title "Settings").
* `SettingsView::new(settings, window, cx)`, `set_settings(Settings, cx)` (shell pushes external changes), `settings()`, `section()`, `select_section(SettingsSection, cx)`.
  `SettingsSection { Appearance, Clusters, Terminal, Logs, Data, Updates }` with `label()` and `all()`.
* Emits `SettingsChanged(Box<UserPrefs>)` (full new preference set, on every edit), `AddKubeconfig`, `ImportFreelens`, `ReloadClusters`, `RemoveKubeconfig(PathBuf)`.
  It never writes settings.json itself; the shell persists. Only the main window quits the app when closed.
* ⌘, and NatiKube -> Settings… route to `AppCommand::OpenSettings`.

### alerts (F9), `natikube_alerts`

* `domain/`: pure. `Evaluator` (one per cluster) turns `MonitorEvent`s into `Alert`s, ignoring the initial list for edge rules and collapsing repeats by `dedup_key` within a cooldown; the ports `AlertStore` and `OsNotifier`; `AlertDispatcher` (store, then OS only while `set_os_enabled(true)`, then `on_change`).
* The configuration model (`AlertsConfig`, `RuleKind`, `AlertTarget`) lives in `natikube-core` because it is part of `Settings`. Everything defaults to off and loads tolerantly.
* `adapters/store`: `SqliteAlertStore` (rusqlite, bundled SQLite, versioned `schema_migrations`), file `notifications.sqlite` in the config dir.
* `adapters/monitor`: `AlertMonitor::apply(&AlertsConfig, &[ClusterInfo])` spawns one task per cluster with an enabled rule on the kube-client runtime (own `ClusterConnection`, list/watch only: pods, deployments, nodes, Warning events). Idempotent and non-blocking. Alerts go straight to the dispatcher on that thread.
* `adapters/notify`: `NativeNotifier` (notify-rust; macOS and Windows) and `NoopNotifier`.
* `ui/bell`: `AlertBell` entity and history panel; emits `BellEvent`. `ui/settings`: `AlertsSettingsSection`; emits `AlertsSettingsEvent`.
* Wiring (shell, `alerts_ops.rs`): `AlertsRuntime` owns the store, dispatcher, monitor and bells. `on_change` runs on a monitor thread, so it only posts to a channel that a UI task drains into `AlertsChanged`. The Alerts page is built by the shell and handed to the Settings window as an `ExtraPage`, so `natikube-settings` does not depend on this crate. Nothing starts in `--demo` or with `--fake-rows` (no monitor, in-memory history).
* `NATIKUBE_CONFIG_DIR` points the app at another NatiKube config folder (settings and history); use it for every QA run.

### portforward (F6), `natikube_portforward`

* `PortForwardsView::new(window, cx)` (one instance for the whole app),
  `start(&mut self, conn: ClusterConnection, spec: PortForwardSpec, cx)`,
  `entries() -> &[ForwardEntry]`, `title()` (e.g. `Port Forwards (2)`).
* `ForwardEntry { id, spec, state: ForwardState }`, `ForwardState { Pending, Active { local_port }, Failed(String), Stopped }`.
* Emits `ShowError`.

To implement: local TCP listener bridged to the kube portforward API, stop
button, open-in-browser, list refresh. Listeners are unit-tested only; agents
never port-forward against a real cluster.

### updates, `natikube_updates`

* `domain` (pure, unit-tested): `Release { tag, version: semver::Version, name, notes, url }`, `parse_tag` (`v0.2.0` or `0.2.0`),
  `newest_release` (highest semver precedence; pre-releases count, since every release is one during the alpha),
  `available_update(current, releases, skipped) -> Option<Release>` (strictly newer than `current`, and not at or below the skipped version),
  `notes_excerpt` (Markdown to a short plain-text excerpt) and `should_check(CheckContext { setting_enabled, demo, exit_after_first_frame, env_disable })`.
* `adapters`: `fetch_releases(version)` is one `GET https://api.github.com/repos/paulpwo/NatiKube/releases?per_page=10`
  over hyper + hyper-rustls (ring provider, native roots; the stack kube already pulls in, no reqwest), 5 s timeout, no redirects,
  1 MiB body cap, `User-Agent: NatiKube/<version>`, `Accept: application/vnd.github+json`. `parse_releases` drops drafts, tags that are not
  versions and entries of the wrong shape one by one, and only keeps a `html_url` under `github.com/paulpwo/NatiKube/releases/`.
* `ui`: `UpdateModal` (`show(UpdateOffer, window, cx)`) emits `UpdateEvent { Download(url), Later, Skip { version, persist } }` and closes itself first.
  Escape and a click outside send `Later`. It is styled with `cx.tokens().palette`, so it follows every appearance.
* Wiring (shell `update_ops.rs`): the first render starts the check, so it never delays the first frame. The request runs on the shared Tokio
  runtime (`natikube_kube_client::spawn`) and a GPUI task awaits it; any failure is a `tracing::debug!` line and no UI. The modal is the last
  child of the window root, like the palette. `Download` reuses `natikube_portforward::domain::open_url`; `Skip` stores
  `updates.skipped_version` and `Later` stores nothing. `Settings.updates { check_on_startup = true, skipped_version = None }`, tolerant like the rest
  of the file, edited in Settings > Updates. `NATIKUBE_NO_UPDATE_CHECK=1`, `--demo` and `--exit-after-first-frame` turn the check off.
* `--fake-update <version>` (hidden, not in `--help`) injects a simulated release with no network. It overrides `--demo` and the setting, and its
  "Skip" is never persisted.

## Shell (`natikube-shell`)

Layout: cluster rail | sidebar (cluster header + sections from `core::sidebar()`)
| column of top tabs (entries of the active group), main content with an
optional resizable right detail panel, optional resizable bottom dock, status
bar. `ShellOptions { store, catalog, initial_cluster, fake_rows, started_at,
exit_after_first_frame, rows_timeout, app_version, fake_update, .. }` and `run(options)`. Pure state lives in
`state.rs` (`NavState`, `DockState`, unit-tested). Frame timing
(`first_frame_ms`, `first_rows_frame_ms`) is printed on stderr by the shell.

## Binary flags (`natikube`)

| Flag | Behavior |
| --- | --- |
| `--context <name>` | kubeconfig context (default: last used, else `current-context`) |
| `--fake-rows <N>` | N synthetic pods, no cluster |
| `--exit-after-first-frame` | print `first_frame_ms` / `first_rows_frame_ms` on stderr and exit |
| `--dump <kind>` | print the engine's objects for `<kind>` on stdout (`namespace/name`, sorted) and exit; no window |
| `--appearance <name>` | `standard\|command\|native\|observatory`; overrides the saved appearance for this run only (never persisted); `ShellOptions.appearance_override` |
| `--dump-clusters` | prints `context<TAB>source file name<TAB>unique\|deduped (also in: ...)` per discovered context and exits; no window, no secrets. Uses `load_catalog` + `ClusterInfo::also_in` |
| `--fake-alerts <N>` | seeds N sample notifications into `notifications.sqlite`, never watches a cluster; refuses to run unless `NATIKUBE_CONFIG_DIR` is set (use with `--demo`) |
| `--fake-update <version>` | hidden dev flag: shows the update modal for that version without any request (overrides `--demo`); never persisted |
| `--dump-rows` | F0 compatibility: prints `rows_count=` and `row=ns/name` for pods on stderr before the window opens (used by `scripts/measure.sh`) |

## Testing

* Domain and adapter logic: tests first (RED then GREEN).
* UI has no meaningful RED; verify by running (`--fake-rows`, `--context dev-cluster`).
  Note: window frames are not produced while the macOS screen is locked, so
  `first_rows_frame_ms` times out in that state.
* Agents access clusters read-only (discovery, list, watch, get, logs,
  `metrics.k8s.io`) on context `dev-cluster`. Never create/update/patch/delete/apply
  (including server dry-run), exec, attach or port-forward against a cluster.
* Checks: `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`,
  `cargo test --workspace`, `cargo build --release`.
