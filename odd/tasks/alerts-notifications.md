# Alerts, notification history and optional OS notifications

## Objective
Let the user decide what to monitor in their clusters (new pod, failing pod, network or cluster failures and other quick, useful signals), keep every notification in a local history (SQLite) shown behind a bell in the header, and send operating system notifications only when the user turns them on.

## Why
Paul (2026-10-24): "quiero una forma de configurar alertas para monitorear lo que yo decida, y una campanita para ver el histórico de notificaciones, pero que yo decida cuáles, ejemplo nuevo pod, pod fallando, red fallando, cosas útiles rápidas [...] que sean además de locales sqlite o algo así, también push de sistema operativo solo si lo activo."

## Product rules
- Nothing is monitored until the user enables it. Every rule starts off. OS notifications start off.
- The user picks rules per cluster (and optionally a namespace filter). Only clusters with at least one enabled rule are watched, and only while NatiKube is open.
- In-app history always records fired alerts (when the rule is on); OS notifications are an extra channel the user toggles.
- No noise: existing objects at watch start never fire "new" alerts; repeated alerts for the same object and reason are collapsed within a cooldown window.

## Rule catalog (first version)
| Rule | Fires when | Parameters |
| --- | --- | --- |
| New pod | A pod appears after the initial list | namespace filter |
| Pod failing | A container enters CrashLoopBackOff, Error, ImagePullBackOff, ErrImagePull, CreateContainerConfigError, or is OOMKilled | namespace filter |
| Pod restarts | Restart count grows by N within the window | N (default 3), window (default 10 min) |
| Pod pending too long | A pod stays Pending longer than T | T (default 5 min) |
| Deployment unavailable | A deployment has unavailable replicas for longer than T | T (default 2 min) |
| Node not ready | A node's Ready condition becomes False or Unknown | none |
| Network / cluster unreachable | The cluster API stops answering, and again when it recovers | none |
| Warning events | Kubernetes Warning events (FailedScheduling, BackOff, Unhealthy, FailedMount, network sandbox failures, ...) | reason filter |

## Scope
- New vertical slice `crates/features/alerts` (crate `natikube-alerts`): domain (rules, evaluation, dedup/cooldown), adapters (kube watchers per enabled cluster, SQLite store, OS notification sender), ui (bell + history panel, alerts section in Settings).
- History in SQLite next to settings.json (`notifications.sqlite` in the NatiKube config dir), with read/unread, retention (default 30 days and 5,000 rows), "mark all read" and "clear".
- Bell in the header of every appearance with an unread badge; the panel lists alerts newest first with severity, cluster, namespace/object, message and time; clicking an alert switches to that cluster and opens the object.
- Settings: an Alerts section with rule toggles and parameters per cluster, the OS notifications toggle, retention and a "Send test notification" button.
- OS notifications on macOS and Windows through a small native crate; nothing is sent unless the toggle is on.

## Constraints
- Read-only against clusters: list/watch only. No web runtime crates.
- Watchers run off the UI thread on the existing kube-client runtime; the UI receives alerts through channels.
- Memory and CPU stay low: one watcher per resource kind per enabled cluster, shared when the cluster is also open in the main view where practical.
- Works on macOS and Windows; Linux later.

## Tasks
- [x] A1 Domain: rule catalog, per-cluster rule config, evaluation over watcher events (initial list ignored), cooldown/dedup, severity. Tests first.
- [x] A2 Store: SQLite history (schema, insert, list paged, unread count, mark read, clear, retention prune). Tests on a temp database.
- [ ] A3 Monitor (built and unit-tested; live watch and settings-driven start/stop not observed): watchers for enabled clusters only (pods, deployments, nodes, Warning events, connection health), feeding the domain evaluator; start/stop when settings change.
- [ ] A4 Bell + history panel in the header for every appearance (rendering observed in 3 appearances; click-to-object not observed).
- [x] A5 Settings: alerts section (rules per cluster, parameters, OS toggle, retention, test notification); alerts config persisted in settings.json, tolerant load.
- [x] A6 OS notifications adapter (macOS, Windows), off by default.
- [x] A7 Integration in shell/app, docs (README privacy and usage, architecture.md).

## Acceptance
- With all rules off, no watcher starts for alerts and nothing is recorded.
- Enabling "Pod failing" on a cluster records an entry when a pod enters CrashLoopBackOff, once per cooldown, visible in the bell with an unread badge.
- OS notification appears only with the toggle on.
- History survives restarts; retention prunes old rows.
- fmt, clippy -D warnings, workspace tests green on macOS; CI green on Windows.

## Route
Delegated direct with parallel writers in this worktree (branch `feat/alerts-notifications`, path `NatiKube-worktrees/alerts`, requested by Paul to run in parallel with the update check on main): one foundation writer (A1, A2, settings model), then three parallel writers on disjoint folders (A3 monitor, A4 bell/history UI, A5+A6 settings and OS notifications), one integrator (A7), Opus validation, fix, Opus recheck.

## Progress / evidence
- 2026-10-24: doc created; worktree and branch created from origin/main 767e5c6.

## Contracts

Written by the foundation writer (A1, A2, config dir override). Parallel writers
edit only their own folders; manifests are already complete (rusqlite 0.41
bundled, notify-rust 4 without default features, kube, tokio, gpui-kit,
natikube-ui-kit are declared in `crates/features/alerts/Cargo.toml`).

### Config dir override
`FileSettingsStore::app_config_dir() -> Option<PathBuf>` (natikube-clusters): `$NATIKUBE_CONFIG_DIR`
(used as the NatiKube folder itself) or the OS config dir joined with `NatiKube`.
`FileSettingsStore::default_location()` is `app_config_dir()/settings.json`; the history
is `app_config_dir()/notifications.sqlite`. Every QA run sets `NATIKUBE_CONFIG_DIR`.

### Config model (natikube-core, part of `Settings.alerts`, re-exported by natikube-alerts)
- `Settings.alerts: AlertsConfig { clusters: BTreeMap<String, ClusterAlerts>, os_notifications: bool (false), retention: Retention { max_age_days: u32 (30), max_rows: u32 (5000) }, cooldown_secs: u32 (300) }`; tolerant load, all off by default.
  `AlertsConfig::cluster(&ClusterId) -> ClusterAlerts`, `cluster_mut(&ClusterId) -> &mut ClusterAlerts`, `enabled_clusters() -> Vec<ClusterId>`, `clamp_in_place()`.
- `ClusterAlerts { rules: BTreeMap<RuleKind, RuleConfig>, namespaces: NamespaceFilter }`:
  `is_enabled(RuleKind)`, `any_enabled()`, `enabled_rules()`, `params(RuleKind) -> RuleParams`, `set_enabled(RuleKind, bool)`, `set_rule(RuleKind, RuleConfig)`.
- `RuleKind::{NewPod, PodFailing, PodRestarts, PodPendingTooLong, DeploymentUnavailable, NodeNotReady, ClusterUnreachable, WarningEvents}`; `RuleKind::ALL`, `key()`, `from_key()`, `title()`, `description()`, `params() -> &[ParamKind]` (RestartCount, WindowSecs, DurationSecs, Reasons), `is_namespaced()`, `default_params()`.
- `RuleConfig { enabled, count: Option<u32>, window_secs: Option<u32>, duration_secs: Option<u32>, reasons: BTreeSet<String> }` (None = default); `RuleParams { count (3), window_secs (600), duration_secs (pending 300, deployment 120), reasons (empty = all) }`.
- Settings UI (A5) edits a whole `AlertsConfig` and emits it; it must call `clamp_in_place()` only via the shell (shell clamps and persists).

### Domain (natikube-alerts, pure)
- `Alert { id: String, cluster: ClusterId, rule: RuleKind, severity: Severity, namespace: Option<String>, object_kind: Option<String>, object_name: Option<String>, message: String, timestamp: Timestamp (i64 unix secs), dedup_key: String }`; `Severity::{Info, Warning, Critical}` (`as_str`); `StoredAlert { row_id: i64, alert: Alert, read: bool }`. `object_kind` is a Kubernetes kind (`Pod`, `Deployment`, `Node`, or the involved kind of a warning event): resolve with `natikube_core::kinds::find(&kind)`.
- Events the monitor produces (`MonitorEvent`): `ListStarted(WatchKind)`, `ListDone(WatchKind)`, `Pod(ObjectEvent<PodSnapshot>)`, `Deployment(ObjectEvent<DeploymentSnapshot>)`, `Node(ObjectEvent<NodeSnapshot>)`, `Warning(WarningEvent)`, `Connection(ConnectionHealth::{Up, Down(String)})`. `ObjectEvent::{Added, Modified, Deleted}(T)` (Added and Modified are equivalent; kube `Apply` may map to either). `WatchKind::{Pods, Deployments, Nodes, Events}`.
  Snapshots: `PodSnapshot { namespace, name, phase, created_at: Option<Timestamp>, containers: Vec<ContainerState { name, restart_count: u32, waiting_reason: Option<String>, terminated_reason: Option<String> }> }` (init and regular containers; `terminated_reason` = current or last terminated state), `DeploymentSnapshot { namespace, name, desired_replicas: i32, unavailable_replicas: i32 }`, `NodeSnapshot { name, ready: NodeReady::{True, False, Unknown} }`, `WarningEvent { namespace: Option<String>, reason, message, object_kind, object_name }` (only `type == Warning` events are forwarded).
  List protocol: for every (re)list send `ListStarted(kind)`, one `Added` per existing object, `ListDone(kind)`; kube-runtime `Init` -> ListStarted, `InitApply` -> Added, `InitDone` -> ListDone, `Apply` -> Modified, `Delete` -> Deleted. Events outside a completed list fire alerts; the baseline never does. `Warning` events delivered before `ListDone(Events)` are ignored (so list the events watch the same way). `Connection` needs no list.
- `Evaluator::new(cluster: ClusterId, config: ClusterAlerts, cooldown_secs: u32)`, `handle(&mut self, MonitorEvent, now: Timestamp) -> Vec<Alert>`, `tick(&mut self, now) -> Vec<Alert>` (call every 10-30 s for pending/deployment durations; also prunes cooldowns), `set_config(ClusterAlerts, cooldown_secs)` (keeps learned state), `Evaluator::needed_watches(&ClusterAlerts) -> Vec<WatchKind>` (watch only these; `ClusterUnreachable` needs none, only connection health). One evaluator per cluster, owned by that cluster's monitor task (it is `Send`, not shared).
- Ports: `trait AlertStore: Send + Sync { insert(&Alert) -> Result<i64, StoreError>; list(limit: u32, offset: u32) -> Result<Vec<StoredAlert>, StoreError> /* newest first */; total_count(); unread_count(); mark_read(row_id: i64); mark_all_read(); clear(); prune(now: Timestamp, &Retention) -> Result<u32, StoreError> }`; `trait OsNotifier: Send + Sync { notify(&Alert) -> Result<(), NotifyError>; send_test() -> Result<(), NotifyError> }`.
- `AlertDispatcher::new(store: Arc<dyn AlertStore>, notifier: Arc<dyn OsNotifier>, on_change: Arc<dyn Fn() + Send + Sync>)`; `dispatch(&self, &[Alert], now: Timestamp) -> usize` (stores, notifies the OS only while `set_os_enabled(true)`, prunes, then calls `on_change` once if anything was stored; logs failures, never panics), `set_os_enabled(bool)`, `os_enabled()`, `set_retention(Retention)`, `store() -> &Arc<dyn AlertStore>`, `send_test_notification()` (ignores the switch). It is `Send + Sync`: share in an `Arc`.
- Store adapter: `SqliteAlertStore::in_config_dir(dir) / open(path) / open_in_memory()`, `schema_version()`; table `schema_migrations(version, applied_at)`; implements `AlertStore`.

### Channels: monitor -> evaluator -> store -> UI
1. A3 `adapters::monitor` (owner: monitor writer): `AlertMonitor::new(dispatcher: Arc<AlertDispatcher>)` and `AlertMonitor::apply(&mut self, config: &AlertsConfig, clusters: &[ClusterInfo])`: starts/stops per-cluster tasks on the kube-client runtime (`natikube_kube_client::spawn`), builds its own `ClusterConnection::connect(&info)` per enabled cluster (read-only list/watch), reconfigures running tasks with `Evaluator::set_config`, stops tasks of clusters with no enabled rule, `stop_all()`, and `Drop` stops everything. Inside a task: kube watcher events -> `MonitorEvent` -> `evaluator.handle(event, now)` -> `dispatcher.dispatch(&alerts, now)`; a timer calls `tick`. Connection errors/recovery become `MonitorEvent::Connection`. With all rules off nothing is spawned. `apply` is cheap and idempotent (the shell calls it on startup, after every `AlertsChanged`/settings save, and after a catalog reload). It must not block the UI thread.
2. Dispatcher -> store: synchronous inside the monitor task (rusqlite is blocking but quick); never called from the UI thread for inserts.
3. Store -> UI: `on_change` is called from a monitor task thread. The integrator wires it to a futures unbounded channel (or `std::sync::mpsc` polled by a GPUI task) that the shell drains on the UI thread, emitting `AppCommand::AlertsChanged`; the bell reads `unread_count()` and `list()` on that event. The UI reads the store directly through `Arc<dyn AlertStore>` (reads are short) and writes only `mark_read`, `mark_all_read`, `clear`.
4. A6 `adapters::notify`: `NativeNotifier: OsNotifier` (`NativeNotifier::new()`), no state besides the app name.
5. A4 `ui::bell`: `AlertBell` view (constructor takes `Arc<dyn AlertStore>`), renders in every appearance header, `refresh(cx)` re-reads counts; emits `AppCommand::OpenAlertObject(AlertTarget)` when an alert is clicked and `AppCommand::AlertsChanged` after mark-read/clear so other bells refresh. Opens the history panel itself.
6. A5 `ui::settings`: `AlertsSettingsSection` (renders inside the Settings window, edits an `AlertsConfig` copy, emits `AppCommand::AlertsConfigChanged(Box<AlertsConfig>)`; the "Send test notification" button calls `AppCommand::SendTestNotification`, and the "Clear history" button emits `AppCommand::ClearAlertHistory`).

### AppCommand variants the integrator adds (natikube-core `command.rs`; core stays I/O free)
- `AlertsChanged` — history changed (new alert, read state, cleared); bells refresh.
- `OpenAlerts` — open the bell panel (menu item / palette).
- `OpenAlertObject(AlertTarget)` — `AlertTarget { cluster: ClusterId, namespace: Option<String>, kind: Option<String>, name: Option<String> }` (define in core `alerts.rs`); the shell does `SelectCluster(cluster)` then, once connected, `OpenDetail(ResourceRef)` built with `kinds::find(kind)`; when `kind`/`name` are missing it only switches cluster.
- `AlertsConfigChanged(Box<AlertsConfig>)` — settings edit; the shell clamps, stores in `Settings.alerts`, persists, then calls `AlertMonitor::apply`, `dispatcher.set_os_enabled`, `dispatcher.set_retention`.
- `SendTestNotification`, `ClearAlertHistory` — handled by the shell through the dispatcher/store.
The foundation did not edit `command.rs`, `crates/shell` or `crates/app`.

### Wiring checklist for the integrator (A7)
Create `SqliteAlertStore::in_config_dir(FileSettingsStore::app_config_dir()?)` (fall back to `open_in_memory` and show an error when it fails), `NativeNotifier`, the `AlertDispatcher` (apply `Settings.alerts` at startup) and the `AlertMonitor`; GPUI global handlers run with the window leased, so every window update triggered by `on_change` or by commands must be deferred (see `update_window_later` in `crates/shell/src/app.rs`).

## Progress (foundation)
- 2026-10-24: A1 (core `alerts.rs`, evaluator, dispatcher), A2 (SQLite store), `NATIKUBE_CONFIG_DIR` override done with tests; contracts above. New dependencies: rusqlite 0.41 (MIT, bundled SQLite via libsqlite3-sys 0.39 MIT), notify-rust 4.18 (MIT OR Apache-2.0; mac-notification-sys, tauri-winrt-notification), plus rusqlite helpers (hashlink, fallible-iterator, fallible-streaming-iterator; MIT/Apache).

## Progress (integration, A7)
- 2026-10-24: wired in shell/app. Core: `AppCommand` alert variants and `AlertTarget`. Shell (`alerts_ops.rs`): `AlertsRuntime` (store, dispatcher, monitor, two bells), UI-thread drain of `on_change`, config apply on start/settings change/catalog reload, click-to-object (SelectCluster, then OpenDetail once the session exists), test notification off-thread, clear history. App (`alerts.rs`): store selection (SQLite in config dir; memory in demo/fake-rows; memory plus warning on open failure), `--fake-alerts N` (refuses without `NATIKUBE_CONFIG_DIR`, never monitors), `--open-alerts`, `--settings-section`. Menu View > Notifications, shortcut Cmd/Ctrl+Shift+N. Bells: Standard (tab strip right end), Command (top bar), Native (toolbar), Observatory (rail bottom, panel opens to the right via `AlertBell::in_rail`).
- Contract fixes: `BellTarget` mapped to `AlertTarget` in the shell; the Settings window cannot depend on the alerts slice (feature rule), so the shell builds the Alerts page and passes it as `ExtraPage` (`open_settings_window(settings, build_extra, cx)`); added `seed_store` to the bell fake module.
- Evidence (all observed): `cargo fmt --check` exit 0; `cargo clippy --workspace --all-targets -D warnings` clean (only the `block v0.1.6` future-incompat note); `cargo test --workspace` 550 passed, 0 failed; `cargo build --release` finished.
- Screenshots (release, `--demo --fake-alerts 12`, `NATIKUBE_CONFIG_DIR` under the scratchpad) in scratchpad `alerts/shots/`: standard-bell-closed (badge 8), standard-bell-open, command-bell-open, observatory-bell-open, settings-alerts. Panel and badge render correctly in all three appearances checked; Native not captured.
- New tests: app store selection (5), args (2), shell alert_resource (3), menu item, settings section order.

## Gaps
- Never run against a real cluster from the integrated app: monitor + evaluator + bell end to end is only covered by unit tests and the monitor writer's earlier smoke (0 alerts recorded).
- No OS notification was sent by the integrator; the macOS bundle identity path (`dev.natikube.app`) and Windows toasts are untested. The writer's single test call returned Ok but was not seen on screen.
- Click-to-object was not exercised in the GUI (synthetic clicks are blocked here): only the pure mapping is tested. Panel keyboard support (Escape) and Native appearance screenshot missing. Windows build not verified locally.
- RED was not observed first for domain/adapter code (tests and code were written together).
- A first screenshot run showed badge 6 and a closed panel; it did not reproduce (stray synthetic clicks were suspected), so treat it as unexplained until checked once more.

## Next step
Opus validation of the full diff, then a manual pass by Paul on k3s-dev with `NATIKUBE_CONFIG_DIR` set: enable Pod failing and Cluster unreachable, confirm entries and click-to-object, then try the OS toggle once. Commit and PR only when Paul asks.

## Fix evidence

- V1 (fixed): `ContainerState` now separates `terminated_reason` (current state) from `last_terminated_reason` (`lastState`). `PodTrack` tracks per-container restart counts; `lastState.terminated` counts as a failure only when the restart count grew since the previous snapshot (unseen containers are baseline). Tests in `domain/evaluator.rs`: old OOMKilled lastState on a healthy pod stays silent (also past cooldown), fresh OOMKill restart fires once, CrashLoopBackOff is not doubled by lastState. Mapping tests updated.
