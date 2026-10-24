//! Per-cluster watcher tasks. One task per cluster with at least one enabled
//! rule runs on the kube-client runtime: it builds its own read-only
//! connection, watches only the kinds the rules need, feeds the cluster's
//! [`Evaluator`] and hands the alerts to the [`AlertDispatcher`].

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use futures::StreamExt;
use futures::stream::{BoxStream, SelectAll, select_all};
use k8s_openapi::api::apps::v1::Deployment;
use k8s_openapi::api::core::v1::{Event as KubeEvent, Node, Pod};
use kube::Api;
use kube::runtime::{WatchStreamExt, watcher};
use natikube_core::{AlertsConfig, ClusterAlerts, ClusterId, ClusterInfo};
use natikube_kube_client::{ClusterConnection, spawn};
use tokio::sync::watch;
use tokio::task::JoinHandle;

use super::health::HealthTracker;
use super::mapping;
use crate::domain::{AlertDispatcher, Evaluator, MonitorEvent, Timestamp, WatchKind};

/// How often the evaluator checks duration-based rules.
const TICK: Duration = Duration::from_secs(15);
/// Reconnect delays while the client cannot even be built.
const CONNECT_BACKOFF: [u64; 4] = [5, 15, 30, 60];

/// Rules and cooldown a running task evaluates with.
type Rules = (ClusterAlerts, u32);

/// One item from a watcher: the events it produced, or its failure text.
enum Signal {
    Events(Vec<MonitorEvent>),
    Failed(String),
}

struct Running {
    info: ClusterInfo,
    watches: Vec<WatchKind>,
    rules: watch::Sender<Rules>,
    task: JoinHandle<()>,
}

/// Starts and stops the per-cluster watcher tasks. Cheap to call repeatedly:
/// [`apply`](Self::apply) is idempotent and never blocks. Everything stops on
/// drop.
pub struct AlertMonitor {
    dispatcher: Arc<AlertDispatcher>,
    running: HashMap<ClusterId, Running>,
}

impl AlertMonitor {
    pub fn new(dispatcher: Arc<AlertDispatcher>) -> Self {
        Self {
            dispatcher,
            running: HashMap::new(),
        }
    }

    /// Brings the running tasks in line with `config`: starts clusters that
    /// now have a rule on, stops the rest, pushes rule changes into running
    /// tasks and restarts a task only when its cluster entry or the set of
    /// watched kinds changed. Clusters absent from `clusters` are stopped.
    pub fn apply(&mut self, config: &AlertsConfig, clusters: &[ClusterInfo]) {
        let wanted: HashMap<&ClusterId, &ClusterInfo> = clusters
            .iter()
            .filter(|info| config.cluster(&info.id).any_enabled())
            .map(|info| (&info.id, info))
            .collect();
        self.running.retain(|id, running| {
            let keep = wanted.contains_key(id);
            if !keep {
                running.task.abort();
            }
            keep
        });
        for (id, info) in wanted {
            let alerts = config.cluster(id);
            let watches = watches_for(&alerts);
            let rules = (alerts, config.cooldown_secs);
            match self.running.get(id) {
                Some(running) if running.info == *info && running.watches == watches => {
                    running.rules.send_if_modified(|current| {
                        let changed = *current != rules;
                        if changed {
                            *current = rules;
                        }
                        changed
                    });
                }
                existing => {
                    if let Some(old) = existing {
                        old.task.abort();
                    }
                    let running = self.start(info.clone(), watches, rules);
                    self.running.insert(id.clone(), running);
                }
            }
        }
    }

    /// Stops every watcher task.
    pub fn stop_all(&mut self) {
        for (_, running) in self.running.drain() {
            running.task.abort();
        }
    }

    /// Clusters currently being monitored.
    pub fn monitored(&self) -> Vec<ClusterId> {
        let mut ids: Vec<_> = self.running.keys().cloned().collect();
        ids.sort();
        ids
    }

    fn start(&self, info: ClusterInfo, watches: Vec<WatchKind>, rules: Rules) -> Running {
        let (tx, rx) = watch::channel(rules);
        let task = spawn(run_cluster(
            info.clone(),
            watches.clone(),
            rx,
            self.dispatcher.clone(),
        ));
        Running {
            info,
            watches,
            rules: tx,
            task,
        }
    }
}

impl Drop for AlertMonitor {
    fn drop(&mut self) {
        self.stop_all();
    }
}

/// The kinds to watch. A cluster whose only rule is "cluster unreachable"
/// needs no object watch, but still needs one watcher as the health signal;
/// nodes are the smallest list.
fn watches_for(alerts: &ClusterAlerts) -> Vec<WatchKind> {
    let needed = Evaluator::needed_watches(alerts);
    if needed.is_empty() {
        vec![WatchKind::Nodes]
    } else {
        needed
    }
}

fn now() -> Timestamp {
    chrono::Utc::now().timestamp()
}

async fn run_cluster(
    info: ClusterInfo,
    watches: Vec<WatchKind>,
    mut rules: watch::Receiver<Rules>,
    dispatcher: Arc<AlertDispatcher>,
) {
    let (alerts, cooldown) = rules.borrow_and_update().clone();
    let mut evaluator = Evaluator::new(info.id.clone(), alerts, cooldown);
    let mut health = HealthTracker::default();

    let connection = connect(&info, &mut evaluator, &dispatcher).await;
    let mut streams = watch_streams(&connection, &watches);
    let mut tick = tokio::time::interval(TICK);
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    loop {
        let fired = tokio::select! {
            signal = streams.next() => match signal {
                Some(Signal::Events(events)) => {
                    let mut fired = Vec::new();
                    let at = now();
                    // Any healthy item proves the API answers again.
                    let mut events = events;
                    if let Some(up) = health.on_ok() {
                        events.insert(0, up);
                    }
                    for event in events {
                        fired.extend(evaluator.handle(event, at));
                    }
                    fired
                }
                Some(Signal::Failed(text)) => health
                    .on_error(&text)
                    .map(|event| evaluator.handle(event, now()))
                    .unwrap_or_default(),
                None => break,
            },
            _ = tick.tick() => evaluator.tick(now()),
            changed = rules.changed() => {
                if changed.is_err() {
                    break;
                }
                let (alerts, cooldown) = rules.borrow_and_update().clone();
                evaluator.set_config(alerts, cooldown);
                Vec::new()
            }
        };
        if !fired.is_empty() {
            dispatcher.dispatch(&fired, now());
        }
    }
}

/// Builds the client, retrying with backoff. While it cannot be built the
/// cluster is reported unreachable (once).
async fn connect(
    info: &ClusterInfo,
    evaluator: &mut Evaluator,
    dispatcher: &AlertDispatcher,
) -> ClusterConnection {
    let mut health = HealthTracker::default();
    let mut attempt = 0;
    loop {
        match ClusterConnection::connect_unverified(info).await {
            Ok(connection) => return connection,
            Err(error) => {
                tracing::warn!(cluster = %info.id, %error, "alert monitor cannot connect");
                if let Some(down) = health.on_error(&error.to_string()) {
                    let fired = evaluator.handle(down, now());
                    dispatcher.dispatch(&fired, now());
                }
                let delay = CONNECT_BACKOFF[attempt.min(CONNECT_BACKOFF.len() - 1)];
                attempt += 1;
                tokio::time::sleep(Duration::from_secs(delay)).await;
            }
        }
    }
}

type Watchers = SelectAll<BoxStream<'static, Signal>>;

/// One watcher per kind, merged. kube-runtime's default backoff retries
/// failures; each failure is surfaced as [`Signal::Failed`].
fn watch_streams(connection: &ClusterConnection, watches: &[WatchKind]) -> Watchers {
    let client = connection.client();
    select_all(watches.iter().map(|kind| match kind {
        WatchKind::Pods => signals(
            Api::<Pod>::all(client.clone()),
            watcher::Config::default(),
            mapping::pod_events,
        ),
        WatchKind::Deployments => signals(
            Api::<Deployment>::all(client.clone()),
            watcher::Config::default(),
            mapping::deployment_events,
        ),
        WatchKind::Nodes => signals(
            Api::<Node>::all(client.clone()),
            watcher::Config::default(),
            mapping::node_events,
        ),
        WatchKind::Events => signals(
            Api::<KubeEvent>::all(client.clone()),
            watcher::Config::default().fields("type=Warning"),
            mapping::kube_event_events,
        ),
    }))
}

fn signals<K>(
    api: Api<K>,
    config: watcher::Config,
    map: fn(watcher::Event<K>) -> Vec<MonitorEvent>,
) -> BoxStream<'static, Signal>
where
    K: kube::Resource + Clone + std::fmt::Debug + serde::de::DeserializeOwned + Send + 'static,
    K::DynamicType: Default,
{
    watcher(api, config)
        .default_backoff()
        .map(move |item| match item {
            Ok(event) => Signal::Events(map(event)),
            Err(error) => Signal::Failed(error.to_string()),
        })
        .boxed()
}

#[cfg(test)]
mod tests {
    use natikube_core::RuleKind;

    use super::*;

    #[test]
    fn unreachable_only_cluster_still_gets_a_health_watcher() {
        let mut alerts = ClusterAlerts::default();
        alerts.set_enabled(RuleKind::ClusterUnreachable, true);
        assert_eq!(watches_for(&alerts), vec![WatchKind::Nodes]);
        alerts.set_enabled(RuleKind::WarningEvents, true);
        assert_eq!(watches_for(&alerts), vec![WatchKind::Events]);
    }

    /// Bounded live smoke test, run by hand against the dev cluster only:
    /// `NATIKUBE_SMOKE_SECS=60 cargo test -p natikube-alerts live_smoke -- --ignored --nocapture`.
    /// Read-only list/watch; alerts go to an in-memory store and no OS
    /// notification is ever sent.
    #[test]
    #[ignore = "talks to the k3s-dev cluster"]
    fn live_smoke() {
        use crate::SqliteAlertStore;
        use crate::domain::{AlertStore, NotifyError, OsNotifier};

        struct Silent;
        impl OsNotifier for Silent {
            fn notify(&self, _: &crate::Alert) -> Result<(), NotifyError> {
                Ok(())
            }
            fn send_test(&self) -> Result<(), NotifyError> {
                Ok(())
            }
        }

        let secs: u64 = std::env::var("NATIKUBE_SMOKE_SECS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(60);
        let info = ClusterInfo::from_context("k3s-dev");
        let mut config = AlertsConfig::default();
        let cluster = config.cluster_mut(&info.id);
        cluster.set_enabled(RuleKind::NewPod, true);
        cluster.set_enabled(RuleKind::WarningEvents, true);
        cluster.set_enabled(RuleKind::ClusterUnreachable, true);

        let store: Arc<dyn AlertStore> = Arc::new(SqliteAlertStore::open_in_memory().unwrap());
        let dispatcher = Arc::new(AlertDispatcher::new(
            store.clone(),
            Arc::new(Silent),
            Arc::new(|| {}),
        ));
        let mut monitor = AlertMonitor::new(dispatcher);
        monitor.apply(&config, std::slice::from_ref(&info));
        println!("monitoring {:?} for {secs}s", monitor.monitored());
        std::thread::sleep(Duration::from_secs(secs));
        monitor.stop_all();
        let rows = store.list(100, 0).unwrap();
        println!("recorded {} alert(s)", rows.len());
        for row in rows {
            println!(
                "{:?} {:?} {}/{} {}",
                row.alert.rule,
                row.alert.severity,
                row.alert.namespace.as_deref().unwrap_or("-"),
                row.alert.object_name.as_deref().unwrap_or("-"),
                row.alert.message
            );
        }
    }
}
