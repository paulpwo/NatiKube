//! Rule evaluation over normalized watch events, one [`Evaluator`] per
//! cluster. Pure and deterministic: time comes in as an argument.
//!
//! Edge rules (new pod, failing, restarts, node, warning events) ignore the
//! baseline delivered by the initial list. Duration rules (pending too long,
//! deployment unavailable) also look at objects that were already bad at
//! start, because the condition is ongoing.

use std::collections::{HashMap, HashSet, VecDeque};

use natikube_core::{ClusterAlerts, ClusterId, RuleKind};

use super::events::{
    ConnectionHealth, DeploymentSnapshot, MonitorEvent, NodeReady, NodeSnapshot, ObjectEvent,
    PodSnapshot, WarningEvent, WatchKind,
};
use super::model::{Alert, Timestamp, rule_severity};

const FAILING_WAITING: [&str; 4] = [
    "CrashLoopBackOff",
    "ImagePullBackOff",
    "ErrImagePull",
    "CreateContainerConfigError",
];
const FAILING_TERMINATED: [&str; 2] = ["Error", "OOMKilled"];

type ObjectKey = (String, String);

#[derive(Default)]
struct PodTrack {
    pending_since: Option<Timestamp>,
    pending_fired: bool,
    /// (time, total restarts), oldest first.
    restarts: VecDeque<(Timestamp, u32)>,
    /// Last seen restart count per container, to tell a fresh crash from an
    /// old `lastState`.
    container_restarts: HashMap<String, u32>,
}

#[derive(Default)]
struct DeploymentTrack {
    unavailable_since: Option<Timestamp>,
    fired: bool,
}

/// What an alert is about, for [`Evaluator::emit`].
struct Subject<'a> {
    namespace: Option<&'a str>,
    kind: Option<&'a str>,
    name: Option<&'a str>,
}

pub struct Evaluator {
    cluster: ClusterId,
    config: ClusterAlerts,
    cooldown_secs: u32,
    ready: HashSet<WatchKind>,
    /// Keys seen during the list in progress, per kind, to drop deleted ones.
    list_seen: HashMap<WatchKind, HashSet<String>>,
    pods: HashMap<ObjectKey, PodTrack>,
    deployments: HashMap<ObjectKey, DeploymentTrack>,
    nodes: HashMap<String, NodeReady>,
    connection_down: Option<bool>,
    cooldowns: HashMap<String, Timestamp>,
    id_prefix: String,
    seq: u64,
}

impl Evaluator {
    pub fn new(cluster: ClusterId, config: ClusterAlerts, cooldown_secs: u32) -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        Self {
            cluster,
            config,
            cooldown_secs,
            ready: HashSet::new(),
            list_seen: HashMap::new(),
            pods: HashMap::new(),
            deployments: HashMap::new(),
            nodes: HashMap::new(),
            connection_down: None,
            cooldowns: HashMap::new(),
            id_prefix: format!("{nanos:x}"),
            seq: 0,
        }
    }

    /// Applies new settings without losing what was learned about the cluster.
    pub fn set_config(&mut self, config: ClusterAlerts, cooldown_secs: u32) {
        self.config = config;
        self.cooldown_secs = cooldown_secs;
    }

    pub fn cluster(&self) -> &ClusterId {
        &self.cluster
    }

    /// Which watches the current configuration needs; the monitor starts only
    /// these.
    pub fn needed_watches(config: &ClusterAlerts) -> Vec<WatchKind> {
        let on = |kind| config.is_enabled(kind);
        let mut out = Vec::new();
        if on(RuleKind::NewPod)
            || on(RuleKind::PodFailing)
            || on(RuleKind::PodRestarts)
            || on(RuleKind::PodPendingTooLong)
        {
            out.push(WatchKind::Pods);
        }
        if on(RuleKind::DeploymentUnavailable) {
            out.push(WatchKind::Deployments);
        }
        if on(RuleKind::NodeNotReady) {
            out.push(WatchKind::Nodes);
        }
        if on(RuleKind::WarningEvents) {
            out.push(WatchKind::Events);
        }
        out
    }

    /// Feeds one event and returns the alerts it fires (after cooldown).
    pub fn handle(&mut self, event: MonitorEvent, now: Timestamp) -> Vec<Alert> {
        let mut out = Vec::new();
        match event {
            MonitorEvent::ListStarted(kind) => {
                self.ready.remove(&kind);
                self.list_seen.insert(kind, HashSet::new());
            }
            MonitorEvent::ListDone(kind) => {
                self.finish_list(kind);
                self.ready.insert(kind);
            }
            MonitorEvent::Pod(ObjectEvent::Added(pod) | ObjectEvent::Modified(pod)) => {
                self.pod_applied(&pod, now, &mut out);
            }
            MonitorEvent::Pod(ObjectEvent::Deleted(pod)) => {
                self.pods.remove(&(pod.namespace, pod.name));
            }
            MonitorEvent::Deployment(
                ObjectEvent::Added(deployment) | ObjectEvent::Modified(deployment),
            ) => self.deployment_applied(&deployment, now),
            MonitorEvent::Deployment(ObjectEvent::Deleted(deployment)) => {
                self.deployments
                    .remove(&(deployment.namespace, deployment.name));
            }
            MonitorEvent::Node(ObjectEvent::Added(node) | ObjectEvent::Modified(node)) => {
                self.node_applied(&node, now, &mut out);
            }
            MonitorEvent::Node(ObjectEvent::Deleted(node)) => {
                self.nodes.remove(&node.name);
            }
            MonitorEvent::Warning(event) => self.warning(&event, now, &mut out),
            MonitorEvent::Connection(health) => self.connection(health, now, &mut out),
        }
        out
    }

    /// Time-based rules. Call periodically (every 10-30 seconds is plenty).
    pub fn tick(&mut self, now: Timestamp) -> Vec<Alert> {
        let mut out = Vec::new();
        let pending = self
            .config
            .params(RuleKind::PodPendingTooLong)
            .duration_secs;
        let mut due_pods = Vec::new();
        for (key, track) in &mut self.pods {
            if let Some(since) = track.pending_since
                && !track.pending_fired
                && now - since >= i64::from(pending)
            {
                track.pending_fired = true;
                due_pods.push((key.clone(), now - since));
            }
        }
        for ((namespace, name), age) in due_pods {
            let message = format!(
                "Pod {namespace}/{name} has been Pending for {}",
                format_secs(age)
            );
            self.emit(
                RuleKind::PodPendingTooLong,
                Subject {
                    namespace: Some(&namespace),
                    kind: Some("Pod"),
                    name: Some(&name),
                },
                format!("pending|{namespace}|{name}"),
                message,
                now,
                &mut out,
            );
        }

        let limit = self
            .config
            .params(RuleKind::DeploymentUnavailable)
            .duration_secs;
        let mut due = Vec::new();
        for (key, track) in &mut self.deployments {
            if let Some(since) = track.unavailable_since
                && !track.fired
                && now - since >= i64::from(limit)
            {
                track.fired = true;
                due.push((key.clone(), now - since));
            }
        }
        for ((namespace, name), age) in due {
            let message = format!(
                "Deployment {namespace}/{name} has had unavailable replicas for {}",
                format_secs(age)
            );
            self.emit(
                RuleKind::DeploymentUnavailable,
                Subject {
                    namespace: Some(&namespace),
                    kind: Some("Deployment"),
                    name: Some(&name),
                },
                format!("deployment|{namespace}|{name}"),
                message,
                now,
                &mut out,
            );
        }

        let cooldown = i64::from(self.cooldown_secs);
        self.cooldowns.retain(|_, last| now - *last < cooldown);
        out
    }

    fn finish_list(&mut self, kind: WatchKind) {
        let Some(seen) = self.list_seen.remove(&kind) else {
            return;
        };
        match kind {
            WatchKind::Pods => self
                .pods
                .retain(|(ns, name), _| seen.contains(&format!("{ns}/{name}"))),
            WatchKind::Deployments => self
                .deployments
                .retain(|(ns, name), _| seen.contains(&format!("{ns}/{name}"))),
            WatchKind::Nodes => self.nodes.retain(|name, _| seen.contains(name)),
            WatchKind::Events => {}
        }
    }

    fn note_seen(&mut self, kind: WatchKind, key: String) {
        if let Some(seen) = self.list_seen.get_mut(&kind) {
            seen.insert(key);
        }
    }

    fn is_ready(&self, kind: WatchKind) -> bool {
        self.ready.contains(&kind)
    }

    fn pod_applied(&mut self, pod: &PodSnapshot, now: Timestamp, out: &mut Vec<Alert>) {
        self.note_seen(WatchKind::Pods, format!("{}/{}", pod.namespace, pod.name));
        let ready = self.is_ready(WatchKind::Pods);
        let key = (pod.namespace.clone(), pod.name.clone());
        let is_new = !self.pods.contains_key(&key);
        let window = i64::from(self.config.params(RuleKind::PodRestarts).window_secs);
        let threshold = self.config.params(RuleKind::PodRestarts).count;

        let track = self.pods.entry(key).or_default();
        if pod.phase == "Pending" {
            if track.pending_since.is_none() {
                track.pending_since = Some(pod.created_at.unwrap_or(now));
            }
        } else {
            track.pending_since = None;
            track.pending_fired = false;
        }

        let total: u32 = pod.containers.iter().map(|c| c.restart_count).sum();
        track.restarts.push_back((now, total));
        while track.restarts.len() >= 2 && track.restarts[1].0 <= now - window {
            track.restarts.pop_front();
        }
        let grown = total.saturating_sub(track.restarts.front().map_or(total, |s| s.1));
        let restarts_fired = ready && grown >= threshold;
        if restarts_fired {
            track.restarts.clear();
            track.restarts.push_back((now, total));
        }

        // `lastState` lingers after a restart: it is a new failure only when
        // the container restarted since the previous snapshot. An unseen
        // container is baseline.
        let mut fresh_restart: HashSet<&str> = HashSet::new();
        for c in &pod.containers {
            let before = track
                .container_restarts
                .insert(c.name.clone(), c.restart_count);
            if before.is_some_and(|b| c.restart_count > b) {
                fresh_restart.insert(c.name.as_str());
            }
        }

        if !ready {
            return;
        }
        let subject = || Subject {
            namespace: Some(&pod.namespace),
            kind: Some("Pod"),
            name: Some(&pod.name),
        };
        if is_new {
            self.emit(
                RuleKind::NewPod,
                subject(),
                format!("new_pod|{}|{}", pod.namespace, pod.name),
                format!("New pod {}/{}", pod.namespace, pod.name),
                now,
                out,
            );
        }
        for container in &pod.containers {
            let reason = container
                .waiting_reason
                .as_deref()
                .filter(|r| FAILING_WAITING.contains(r))
                .or_else(|| {
                    let last = fresh_restart
                        .contains(container.name.as_str())
                        .then_some(container.last_terminated_reason.as_deref())
                        .flatten();
                    container
                        .terminated_reason
                        .as_deref()
                        .or(last)
                        .filter(|r| FAILING_TERMINATED.contains(r))
                });
            if let Some(reason) = reason {
                self.emit(
                    RuleKind::PodFailing,
                    subject(),
                    format!(
                        "failing|{}|{}|{}|{reason}",
                        pod.namespace, pod.name, container.name
                    ),
                    format!(
                        "Pod {}/{}: container {} is {reason}",
                        pod.namespace, pod.name, container.name
                    ),
                    now,
                    out,
                );
            }
        }
        if restarts_fired {
            self.emit(
                RuleKind::PodRestarts,
                subject(),
                format!("restarts|{}|{}", pod.namespace, pod.name),
                format!(
                    "Pod {}/{} restarted {grown} times within {}",
                    pod.namespace,
                    pod.name,
                    format_secs(window)
                ),
                now,
                out,
            );
        }
    }

    fn deployment_applied(&mut self, deployment: &DeploymentSnapshot, now: Timestamp) {
        self.note_seen(
            WatchKind::Deployments,
            format!("{}/{}", deployment.namespace, deployment.name),
        );
        let track = self
            .deployments
            .entry((deployment.namespace.clone(), deployment.name.clone()))
            .or_default();
        if deployment.unavailable_replicas > 0 {
            track.unavailable_since.get_or_insert(now);
        } else {
            track.unavailable_since = None;
            track.fired = false;
        }
    }

    fn node_applied(&mut self, node: &NodeSnapshot, now: Timestamp, out: &mut Vec<Alert>) {
        self.note_seen(WatchKind::Nodes, node.name.clone());
        let previous = self.nodes.insert(node.name.clone(), node.ready);
        let was_bad = matches!(previous, Some(r) if r != NodeReady::True);
        if self.is_ready(WatchKind::Nodes) && node.ready != NodeReady::True && !was_bad {
            let state = if node.ready == NodeReady::False {
                "False"
            } else {
                "Unknown"
            };
            self.emit(
                RuleKind::NodeNotReady,
                Subject {
                    namespace: None,
                    kind: Some("Node"),
                    name: Some(&node.name),
                },
                format!("node|{}", node.name),
                format!("Node {} is not Ready (Ready={state})", node.name),
                now,
                out,
            );
        }
    }

    fn warning(&mut self, event: &WarningEvent, now: Timestamp, out: &mut Vec<Alert>) {
        if !self.is_ready(WatchKind::Events) {
            return;
        }
        let reasons = self.config.params(RuleKind::WarningEvents).reasons;
        if !reasons.is_empty() && !reasons.contains(&event.reason) {
            return;
        }
        let ns = event.namespace.as_deref().unwrap_or("");
        self.emit(
            RuleKind::WarningEvents,
            Subject {
                namespace: event.namespace.as_deref(),
                kind: Some(&event.object_kind),
                name: Some(&event.object_name),
            },
            format!(
                "warning|{ns}|{}|{}|{}",
                event.object_kind, event.object_name, event.reason
            ),
            format!(
                "{} {}: {} ({})",
                event.object_kind, event.object_name, event.reason, event.message
            ),
            now,
            out,
        );
    }

    /// Connection changes are state transitions, so they skip the cooldown.
    fn connection(&mut self, health: ConnectionHealth, now: Timestamp, out: &mut Vec<Alert>) {
        let (down, message) = match &health {
            ConnectionHealth::Down(reason) => (true, format!("Cluster API unreachable: {reason}")),
            ConnectionHealth::Up => (false, "Cluster API reachable again".to_string()),
        };
        let previous = self.connection_down.replace(down);
        let changed = match previous {
            None => down,
            Some(before) => before != down,
        };
        if !changed {
            return;
        }
        let key = if down { "cluster|down" } else { "cluster|up" };
        self.push(
            RuleKind::ClusterUnreachable,
            Subject {
                namespace: None,
                kind: None,
                name: None,
            },
            key.to_string(),
            message,
            now,
            out,
            !down,
        );
    }

    /// Applies rule enablement, the namespace filter and the cooldown.
    fn emit(
        &mut self,
        rule: RuleKind,
        subject: Subject<'_>,
        key: String,
        message: String,
        now: Timestamp,
        out: &mut Vec<Alert>,
    ) {
        if let Some(namespace) = subject.namespace
            && rule.is_namespaced()
            && !self.config.namespaces.matches(namespace)
        {
            return;
        }
        if !self.config.is_enabled(rule) {
            return;
        }
        if let Some(last) = self.cooldowns.get(&key)
            && now - *last < i64::from(self.cooldown_secs)
        {
            return;
        }
        self.cooldowns.insert(key.clone(), now);
        self.push(rule, subject, key, message, now, out, false);
    }

    #[allow(clippy::too_many_arguments)]
    fn push(
        &mut self,
        rule: RuleKind,
        subject: Subject<'_>,
        key: String,
        message: String,
        now: Timestamp,
        out: &mut Vec<Alert>,
        recovery: bool,
    ) {
        if !self.config.is_enabled(rule) {
            return;
        }
        self.seq += 1;
        let severity = if recovery {
            super::model::Severity::Info
        } else {
            rule_severity(rule)
        };
        out.push(Alert {
            id: format!("{}-{}", self.id_prefix, self.seq),
            cluster: self.cluster.clone(),
            rule,
            severity,
            namespace: subject.namespace.map(str::to_string),
            object_kind: subject.kind.map(str::to_string),
            object_name: subject.name.map(str::to_string),
            message,
            timestamp: now,
            dedup_key: key,
        });
    }
}

/// `90` -> `1m`, `3600` -> `1h`, compact and good enough for messages.
fn format_secs(secs: i64) -> String {
    match secs {
        s if s >= 3600 && s % 3600 == 0 => format!("{}h", s / 3600),
        s if s >= 60 => format!("{}m", s / 60),
        s => format!("{s}s"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::events::ContainerState;
    use crate::domain::model::Severity;
    use natikube_core::{NamespaceFilter, RuleConfig};
    use std::collections::BTreeSet;

    fn config(rules: &[RuleKind]) -> ClusterAlerts {
        let mut config = ClusterAlerts::default();
        for rule in rules {
            config.set_enabled(*rule, true);
        }
        config
    }

    fn evaluator(rules: &[RuleKind]) -> Evaluator {
        Evaluator::new(ClusterId::new("c"), config(rules), 300)
    }

    fn container(restarts: u32, waiting: Option<&str>, terminated: Option<&str>) -> ContainerState {
        ContainerState {
            name: "app".into(),
            restart_count: restarts,
            waiting_reason: waiting.map(str::to_string),
            terminated_reason: terminated.map(str::to_string),
            last_terminated_reason: None,
        }
    }

    fn pod(ns: &str, name: &str, phase: &str, containers: Vec<ContainerState>) -> PodSnapshot {
        PodSnapshot {
            namespace: ns.into(),
            name: name.into(),
            phase: phase.into(),
            created_at: None,
            containers,
        }
    }

    fn running(ns: &str, name: &str) -> PodSnapshot {
        pod(ns, name, "Running", vec![container(0, None, None)])
    }

    fn list(e: &mut Evaluator, kind: WatchKind, events: Vec<MonitorEvent>) -> Vec<Alert> {
        let mut out = e.handle(MonitorEvent::ListStarted(kind), 0);
        for event in events {
            out.extend(e.handle(event, 0));
        }
        out.extend(e.handle(MonitorEvent::ListDone(kind), 0));
        out
    }

    fn add(p: PodSnapshot) -> MonitorEvent {
        MonitorEvent::Pod(ObjectEvent::Added(p))
    }

    fn modify(p: PodSnapshot) -> MonitorEvent {
        MonitorEvent::Pod(ObjectEvent::Modified(p))
    }

    #[test]
    fn initial_list_never_fires_new_pod() {
        let mut e = evaluator(&[RuleKind::NewPod]);
        let fired = list(&mut e, WatchKind::Pods, vec![add(running("a", "one"))]);
        assert!(fired.is_empty());
        // Re-delivering a known pod after the list is not new either.
        assert!(e.handle(modify(running("a", "one")), 10).is_empty());
    }

    #[test]
    fn pod_after_initial_list_fires_once() {
        let mut e = evaluator(&[RuleKind::NewPod]);
        list(&mut e, WatchKind::Pods, vec![]);
        let fired = e.handle(add(running("a", "two")), 100);
        assert_eq!(fired.len(), 1);
        let alert = &fired[0];
        assert_eq!(alert.rule, RuleKind::NewPod);
        assert_eq!(alert.severity, Severity::Info);
        assert_eq!(alert.namespace.as_deref(), Some("a"));
        assert_eq!(alert.object_kind.as_deref(), Some("Pod"));
        assert_eq!(alert.object_name.as_deref(), Some("two"));
        assert_eq!(alert.timestamp, 100);
        assert_eq!(alert.cluster, ClusterId::new("c"));
        assert!(e.handle(modify(running("a", "two")), 101).is_empty());
    }

    #[test]
    fn events_before_the_list_completes_are_silent() {
        let mut e = evaluator(&[RuleKind::NewPod]);
        assert!(e.handle(add(running("a", "x")), 1).is_empty());
    }

    #[test]
    fn disabled_rules_stay_silent() {
        let mut e = evaluator(&[RuleKind::PodFailing]);
        list(&mut e, WatchKind::Pods, vec![]);
        assert!(e.handle(add(running("a", "x")), 1).is_empty());
    }

    #[test]
    fn failing_reasons_fire_with_cooldown_dedup() {
        let mut e = evaluator(&[RuleKind::PodFailing]);
        list(&mut e, WatchKind::Pods, vec![add(running("a", "p"))]);
        let crash = pod(
            "a",
            "p",
            "Running",
            vec![container(1, Some("CrashLoopBackOff"), None)],
        );
        let first = e.handle(modify(crash.clone()), 100);
        assert_eq!(first.len(), 1);
        assert_eq!(first[0].severity, Severity::Critical);
        assert!(first[0].message.contains("CrashLoopBackOff"));
        assert!(
            e.handle(modify(crash.clone()), 200).is_empty(),
            "inside cooldown"
        );
        assert_eq!(e.handle(modify(crash), 401).len(), 1, "cooldown elapsed");
    }

    #[test]
    fn every_failing_reason_is_recognized_and_others_are_not() {
        for reason in [
            "CrashLoopBackOff",
            "ImagePullBackOff",
            "ErrImagePull",
            "CreateContainerConfigError",
        ] {
            let mut e = evaluator(&[RuleKind::PodFailing]);
            list(&mut e, WatchKind::Pods, vec![]);
            let p = pod("a", "p", "Pending", vec![container(0, Some(reason), None)]);
            assert_eq!(e.handle(add(p), 1).len(), 1, "{reason}");
        }
        for reason in ["Error", "OOMKilled"] {
            let mut e = evaluator(&[RuleKind::PodFailing]);
            list(&mut e, WatchKind::Pods, vec![]);
            let p = pod("a", "p", "Running", vec![container(1, None, Some(reason))]);
            assert_eq!(e.handle(add(p), 1).len(), 1, "{reason}");
        }
        let mut e = evaluator(&[RuleKind::PodFailing]);
        list(&mut e, WatchKind::Pods, vec![]);
        let fine = pod(
            "a",
            "p",
            "Pending",
            vec![container(0, Some("ContainerCreating"), Some("Completed"))],
        );
        assert!(e.handle(add(fine), 1).is_empty());
    }

    fn with_last(restarts: u32, waiting: Option<&str>, last: &str) -> ContainerState {
        ContainerState {
            last_terminated_reason: Some(last.into()),
            ..container(restarts, waiting, None)
        }
    }

    #[test]
    fn old_last_state_on_a_healthy_pod_does_not_fire() {
        let mut e = evaluator(&[RuleKind::PodFailing]);
        let old = |n| pod("a", "p", "Running", vec![with_last(n, None, "OOMKilled")]);
        assert!(list(&mut e, WatchKind::Pods, vec![add(old(2))]).is_empty());
        assert!(e.handle(modify(old(2)), 1000).is_empty());
        assert!(e.handle(modify(old(2)), 2000).is_empty(), "past cooldown");
    }

    #[test]
    fn fresh_restart_with_oom_last_state_fires_once() {
        let mut e = evaluator(&[RuleKind::PodFailing]);
        let snap = |n| pod("a", "p", "Running", vec![with_last(n, None, "OOMKilled")]);
        list(&mut e, WatchKind::Pods, vec![add(snap(2))]);
        let fired = e.handle(modify(snap(3)), 100);
        assert_eq!(fired.len(), 1);
        assert!(fired[0].message.contains("OOMKilled"));
        // Later updates without another restart stay quiet, even past cooldown.
        assert!(e.handle(modify(snap(3)), 1000).is_empty());
    }

    #[test]
    fn crash_loop_waiting_reason_is_not_doubled_by_last_state() {
        let mut e = evaluator(&[RuleKind::PodFailing]);
        let snap = |n| {
            pod(
                "a",
                "p",
                "Running",
                vec![with_last(n, Some("CrashLoopBackOff"), "Error")],
            )
        };
        list(&mut e, WatchKind::Pods, vec![add(snap(1))]);
        let fired = e.handle(modify(snap(2)), 100);
        assert_eq!(fired.len(), 1);
        assert!(fired[0].message.contains("CrashLoopBackOff"));
    }

    #[test]
    fn failing_in_the_initial_list_is_baseline() {
        let mut e = evaluator(&[RuleKind::PodFailing]);
        let crash = pod(
            "a",
            "p",
            "Running",
            vec![container(9, Some("CrashLoopBackOff"), None)],
        );
        assert!(list(&mut e, WatchKind::Pods, vec![add(crash)]).is_empty());
    }

    #[test]
    fn restarts_fire_when_growth_reaches_n_inside_the_window() {
        let mut e = evaluator(&[RuleKind::PodRestarts]);
        list(&mut e, WatchKind::Pods, vec![add(running("a", "p"))]);
        let with = |n| pod("a", "p", "Running", vec![container(n, None, None)]);
        assert!(e.handle(modify(with(1)), 100).is_empty());
        assert!(e.handle(modify(with(2)), 200).is_empty());
        let fired = e.handle(modify(with(3)), 300);
        assert_eq!(fired.len(), 1);
        assert_eq!(fired[0].rule, RuleKind::PodRestarts);
        assert_eq!(fired[0].severity, Severity::Warning);
    }

    #[test]
    fn slow_restarts_outside_the_window_do_not_fire() {
        let mut e = evaluator(&[RuleKind::PodRestarts]);
        list(&mut e, WatchKind::Pods, vec![add(running("a", "p"))]);
        let with = |n| pod("a", "p", "Running", vec![container(n, None, None)]);
        assert!(e.handle(modify(with(1)), 1_000).is_empty());
        assert!(e.handle(modify(with(2)), 2_000).is_empty());
        assert!(e.handle(modify(with(3)), 3_000).is_empty());
    }

    #[test]
    fn restart_threshold_and_window_are_configurable() {
        let mut cfg = config(&[]);
        cfg.set_rule(
            RuleKind::PodRestarts,
            RuleConfig {
                enabled: true,
                count: Some(1),
                window_secs: Some(60),
                ..RuleConfig::default()
            },
        );
        let mut e = Evaluator::new(ClusterId::new("c"), cfg, 300);
        list(&mut e, WatchKind::Pods, vec![add(running("a", "p"))]);
        let one = pod("a", "p", "Running", vec![container(1, None, None)]);
        assert_eq!(e.handle(modify(one), 10).len(), 1);
    }

    #[test]
    fn pending_too_long_fires_once_from_tick() {
        let mut e = evaluator(&[RuleKind::PodPendingTooLong]);
        list(&mut e, WatchKind::Pods, vec![]);
        e.handle(add(pod("a", "p", "Pending", vec![])), 100);
        assert!(e.tick(399).is_empty());
        let fired = e.tick(400);
        assert_eq!(fired.len(), 1);
        assert!(fired[0].message.contains("Pending for 5m"));
        assert!(e.tick(1_000).is_empty(), "fires once per pending spell");
    }

    #[test]
    fn pending_clears_when_the_pod_runs() {
        let mut e = evaluator(&[RuleKind::PodPendingTooLong]);
        list(&mut e, WatchKind::Pods, vec![]);
        e.handle(add(pod("a", "p", "Pending", vec![])), 100);
        e.handle(modify(running("a", "p")), 200);
        assert!(e.tick(1_000).is_empty());
    }

    #[test]
    fn pending_uses_creation_time_even_for_pods_present_at_start() {
        let mut e = evaluator(&[RuleKind::PodPendingTooLong]);
        let mut old = pod("a", "p", "Pending", vec![]);
        old.created_at = Some(0);
        list(&mut e, WatchKind::Pods, vec![add(old)]);
        assert_eq!(e.tick(1_000).len(), 1);
    }

    #[test]
    fn deployment_unavailable_after_threshold_and_recovery_resets() {
        let mut e = evaluator(&[RuleKind::DeploymentUnavailable]);
        list(&mut e, WatchKind::Deployments, vec![]);
        let dep = |unavailable| {
            MonitorEvent::Deployment(ObjectEvent::Modified(DeploymentSnapshot {
                namespace: "a".into(),
                name: "web".into(),
                desired_replicas: 2,
                unavailable_replicas: unavailable,
            }))
        };
        e.handle(dep(1), 100);
        assert!(e.tick(219).is_empty());
        let fired = e.tick(220);
        assert_eq!(fired.len(), 1);
        assert_eq!(fired[0].object_kind.as_deref(), Some("Deployment"));
        assert!(e.tick(400).is_empty());
        e.handle(dep(0), 500);
        e.handle(dep(2), 600);
        assert!(e.tick(719).is_empty());
        // The cooldown (300 s) from the first alert has long expired.
        assert_eq!(e.tick(720).len(), 1);
    }

    fn node(name: &str, ready: NodeReady) -> MonitorEvent {
        MonitorEvent::Node(ObjectEvent::Modified(NodeSnapshot {
            name: name.into(),
            ready,
        }))
    }

    #[test]
    fn node_not_ready_fires_on_transition_only() {
        let mut e = evaluator(&[RuleKind::NodeNotReady]);
        list(
            &mut e,
            WatchKind::Nodes,
            vec![node("n1", NodeReady::True), node("n2", NodeReady::False)],
        );
        assert!(
            e.handle(node("n2", NodeReady::False), 1).is_empty(),
            "already bad at start"
        );
        let fired = e.handle(node("n1", NodeReady::Unknown), 10);
        assert_eq!(fired.len(), 1);
        assert_eq!(fired[0].severity, Severity::Critical);
        assert_eq!(fired[0].namespace, None);
        assert!(
            e.handle(node("n1", NodeReady::False), 11).is_empty(),
            "still not ready"
        );
        assert!(
            e.handle(node("n1", NodeReady::True), 12).is_empty(),
            "recovery is silent"
        );
    }

    #[test]
    fn connection_down_and_recovery_skip_cooldown_but_not_repeats() {
        let mut e = evaluator(&[RuleKind::ClusterUnreachable]);
        assert!(
            e.handle(MonitorEvent::Connection(ConnectionHealth::Up), 1)
                .is_empty()
        );
        let down = e.handle(
            MonitorEvent::Connection(ConnectionHealth::Down("timeout".into())),
            2,
        );
        assert_eq!(down.len(), 1);
        assert_eq!(down[0].severity, Severity::Critical);
        assert!(down[0].message.contains("timeout"));
        assert!(
            e.handle(
                MonitorEvent::Connection(ConnectionHealth::Down("timeout".into())),
                3
            )
            .is_empty()
        );
        let up = e.handle(MonitorEvent::Connection(ConnectionHealth::Up), 4);
        assert_eq!(up.len(), 1);
        assert_eq!(up[0].severity, Severity::Info);
        let again = e.handle(
            MonitorEvent::Connection(ConnectionHealth::Down("x".into())),
            5,
        );
        assert_eq!(again.len(), 1, "flapping inside the cooldown still reports");
    }

    #[test]
    fn unreachable_at_start_is_reported() {
        let mut e = evaluator(&[RuleKind::ClusterUnreachable]);
        let down = e.handle(
            MonitorEvent::Connection(ConnectionHealth::Down("refused".into())),
            1,
        );
        assert_eq!(down.len(), 1);
    }

    fn warning(reason: &str) -> MonitorEvent {
        MonitorEvent::Warning(WarningEvent {
            namespace: Some("a".into()),
            reason: reason.into(),
            message: "boom".into(),
            object_kind: "Pod".into(),
            object_name: "p".into(),
        })
    }

    #[test]
    fn warning_events_respect_baseline_reason_filter_and_cooldown() {
        let mut cfg = config(&[]);
        cfg.set_rule(
            RuleKind::WarningEvents,
            RuleConfig {
                enabled: true,
                reasons: BTreeSet::from(["FailedScheduling".to_string()]),
                ..RuleConfig::default()
            },
        );
        let mut e = Evaluator::new(ClusterId::new("c"), cfg, 300);
        assert!(list(&mut e, WatchKind::Events, vec![warning("FailedScheduling")]).is_empty());
        assert!(
            e.handle(warning("BackOff"), 1).is_empty(),
            "reason filtered out"
        );
        let fired = e.handle(warning("FailedScheduling"), 2);
        assert_eq!(fired.len(), 1);
        assert_eq!(fired[0].object_name.as_deref(), Some("p"));
        assert!(e.handle(warning("FailedScheduling"), 3).is_empty());
    }

    #[test]
    fn empty_reason_filter_matches_every_warning() {
        let mut e = evaluator(&[RuleKind::WarningEvents]);
        list(&mut e, WatchKind::Events, vec![]);
        assert_eq!(e.handle(warning("Anything"), 1).len(), 1);
    }

    #[test]
    fn namespace_filter_applies_to_namespaced_rules_only() {
        let mut cfg = config(&[RuleKind::NewPod, RuleKind::NodeNotReady]);
        cfg.namespaces = NamespaceFilter::Only(BTreeSet::from(["prod".to_string()]));
        let mut e = Evaluator::new(ClusterId::new("c"), cfg, 300);
        list(&mut e, WatchKind::Pods, vec![]);
        list(&mut e, WatchKind::Nodes, vec![node("n", NodeReady::True)]);
        assert!(e.handle(add(running("dev", "x")), 1).is_empty());
        assert_eq!(e.handle(add(running("prod", "y")), 2).len(), 1);
        assert_eq!(e.handle(node("n", NodeReady::False), 3).len(), 1);
    }

    #[test]
    fn deleted_pods_are_forgotten_so_a_recreation_is_new_again() {
        let mut e = evaluator(&[RuleKind::NewPod]);
        list(&mut e, WatchKind::Pods, vec![]);
        assert_eq!(e.handle(add(running("a", "p")), 1).len(), 1);
        e.handle(
            MonitorEvent::Pod(ObjectEvent::Deleted(running("a", "p"))),
            2,
        );
        assert_eq!(e.handle(add(running("a", "p")), 400).len(), 1);
    }

    #[test]
    fn relist_drops_pods_that_disappeared_without_a_delete_event() {
        let mut e = evaluator(&[RuleKind::NewPod]);
        list(
            &mut e,
            WatchKind::Pods,
            vec![add(running("a", "gone")), add(running("a", "kept"))],
        );
        list(&mut e, WatchKind::Pods, vec![add(running("a", "kept"))]);
        assert_eq!(e.handle(add(running("a", "gone")), 1).len(), 1);
    }

    #[test]
    fn alert_ids_are_unique() {
        let mut e = evaluator(&[RuleKind::NewPod]);
        list(&mut e, WatchKind::Pods, vec![]);
        let a = e.handle(add(running("a", "1")), 1);
        let b = e.handle(add(running("a", "2")), 1);
        assert_ne!(a[0].id, b[0].id);
    }

    #[test]
    fn needed_watches_follow_enabled_rules() {
        assert!(Evaluator::needed_watches(&ClusterAlerts::default()).is_empty());
        let cfg = config(&[
            RuleKind::NewPod,
            RuleKind::PodRestarts,
            RuleKind::WarningEvents,
        ]);
        assert_eq!(
            Evaluator::needed_watches(&cfg),
            vec![WatchKind::Pods, WatchKind::Events]
        );
        let cfg = config(&[RuleKind::ClusterUnreachable]);
        assert!(
            Evaluator::needed_watches(&cfg).is_empty(),
            "connection health needs no watch"
        );
    }

    #[test]
    fn set_config_keeps_learned_state() {
        let mut e = evaluator(&[]);
        list(&mut e, WatchKind::Pods, vec![add(running("a", "p"))]);
        e.set_config(config(&[RuleKind::NewPod]), 300);
        assert!(e.handle(modify(running("a", "p")), 1).is_empty());
    }
}
