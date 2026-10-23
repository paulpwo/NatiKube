//! A curated, entirely fictional demo cluster for `--demo`: README
//! screenshots and trying the app without a cluster. Objects are built as raw
//! Kubernetes JSON and rendered through the real [`Printer`]s, so every column
//! behaves exactly like it does against a live cluster. Nothing here comes
//! from a real cluster.

use chrono::{DateTime, SecondsFormat};
use futures::StreamExt as _;
use natikube_core::{ResourceKind, kinds};
use serde_json::{Value, json};

use super::object::{ObjectRow, ObjectSource, RowEvent, RowEventStream, UsageSource, UsageStream};
use super::printers::printer_for;
use super::usage::{Usage, UsageMap, usage_key};

/// Namespaces of the demo cluster.
pub const DEMO_NAMESPACES: [&str; 6] = [
    "auth",
    "ingress",
    "kube-system",
    "monitoring",
    "payments",
    "shop",
];

/// Demo node names and their allocatable CPU cores / memory in MiB.
const NODES: [(&str, u32, u32); 3] = [
    ("demo-node-1", 4, 16384),
    ("demo-node-2", 4, 16384),
    ("demo-node-3", 8, 32768),
];

const DAY: i64 = 86_400;
const HOUR: i64 = 3_600;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Controller {
    Deployment,
    StatefulSet,
    DaemonSet,
}

impl Controller {
    fn kind(self) -> &'static str {
        match self {
            Self::Deployment => "Deployment",
            Self::StatefulSet => "StatefulSet",
            Self::DaemonSet => "DaemonSet",
        }
    }
}

/// One workload and the pods it owns.
struct Workload {
    namespace: &'static str,
    name: &'static str,
    controller: Controller,
    /// Pod-template hash (Deployments only).
    hash: &'static str,
    /// Random pod-name suffixes (Deployments and DaemonSets); StatefulSets use
    /// ordinals and take their replica count from the length.
    suffixes: &'static [&'static str],
    containers: &'static [&'static str],
    cpu_millis: u32,
    memory_mib: u32,
    age_days: i64,
}

#[allow(clippy::too_many_arguments)]
const fn workload(
    namespace: &'static str,
    name: &'static str,
    controller: Controller,
    hash: &'static str,
    suffixes: &'static [&'static str],
    containers: &'static [&'static str],
    usage: (u32, u32),
    age_days: i64,
) -> Workload {
    Workload {
        namespace,
        name,
        controller,
        hash,
        suffixes,
        containers,
        cpu_millis: usage.0,
        memory_mib: usage.1,
        age_days,
    }
}

use Controller::{DaemonSet, Deployment, StatefulSet};

const WORKLOADS: &[Workload] = &[
    workload(
        "shop",
        "checkout-api",
        Deployment,
        "7d9f8c6b5",
        &["x2kqp", "m4v8d", "q9zt7"],
        &["api", "metrics"],
        (120, 310),
        21,
    ),
    workload(
        "shop",
        "orders-worker",
        Deployment,
        "5c8b7d94f",
        &["h6jnw", "t2rlc"],
        &["worker"],
        (80, 220),
        14,
    ),
    workload(
        "shop",
        "storefront-web",
        Deployment,
        "6f7d5b8c9",
        &["8kd2s", "pw5vn"],
        &["web"],
        (60, 180),
        30,
    ),
    workload(
        "shop",
        "catalog-api",
        Deployment,
        "84c6d7f5b",
        &["l7x9c", "z3mfh"],
        &["api"],
        (90, 260),
        30,
    ),
    workload(
        "shop",
        "postgres",
        StatefulSet,
        "",
        &["0"],
        &["postgres"],
        (210, 640),
        45,
    ),
    workload(
        "payments",
        "payments-api",
        Deployment,
        "69b5f8d7c",
        &["r4tkq", "c8wnj"],
        &["api", "metrics"],
        (140, 340),
        18,
    ),
    workload(
        "payments",
        "payments-worker",
        Deployment,
        "7b8c9d6f4",
        &["v2hgx"],
        &["worker"],
        (0, 0),
        1,
    ),
    workload(
        "payments",
        "ledger-db",
        StatefulSet,
        "",
        &["0"],
        &["postgres"],
        (190, 580),
        60,
    ),
    workload(
        "auth",
        "auth-gateway",
        Deployment,
        "5d7c9b8f6",
        &["n6qzd", "j9sbk"],
        &["gateway"],
        (70, 150),
        26,
    ),
    workload(
        "auth",
        "token-service",
        Deployment,
        "66f9c7d84",
        &["b5hrv", "f2xlm"],
        &["token"],
        (50, 120),
        26,
    ),
    workload(
        "auth",
        "auth-redis",
        StatefulSet,
        "",
        &["0"],
        &["redis"],
        (25, 90),
        26,
    ),
    workload(
        "monitoring",
        "prometheus",
        StatefulSet,
        "",
        &["0"],
        &["prometheus", "config-reloader"],
        (320, 1_450),
        40,
    ),
    workload(
        "monitoring",
        "alertmanager",
        StatefulSet,
        "",
        &["0"],
        &["alertmanager"],
        (15, 60),
        40,
    ),
    workload(
        "monitoring",
        "grafana",
        Deployment,
        "7c4d8b6f9",
        &["k8vtp"],
        &["grafana"],
        (45, 210),
        40,
    ),
    workload(
        "monitoring",
        "node-exporter",
        DaemonSet,
        "",
        &["4hxkd", "9wqzt", "mf6cn"],
        &["exporter"],
        (8, 24),
        40,
    ),
    workload(
        "ingress",
        "ingress-nginx-controller",
        Deployment,
        "7f6b9d5c8",
        &["d4wps", "g8nlh"],
        &["controller"],
        (65, 140),
        50,
    ),
    workload(
        "kube-system",
        "coredns",
        Deployment,
        "5dd5756b68",
        &["6xk7j", "ptq9r"],
        &["coredns"],
        (4, 22),
        90,
    ),
    workload(
        "kube-system",
        "kube-proxy",
        DaemonSet,
        "",
        &["b7nzt", "jq5vc", "w9xhd"],
        &["kube-proxy"],
        (3, 18),
        90,
    ),
    workload(
        "kube-system",
        "metrics-server",
        Deployment,
        "6b4f8c7d9",
        &["s5jvw"],
        &["metrics-server"],
        (6, 38),
        90,
    ),
];

impl Workload {
    fn replicas(&self) -> usize {
        self.suffixes.len()
    }

    /// Name of the ReplicaSet (Deployments) or the workload itself.
    fn owner_name(&self) -> String {
        match self.controller {
            Deployment => format!("{}-{}", self.name, self.hash),
            _ => self.name.to_string(),
        }
    }

    fn pod_name(&self, ix: usize) -> String {
        match self.controller {
            Deployment => format!("{}-{}-{}", self.name, self.hash, self.suffixes[ix]),
            StatefulSet => format!("{}-{ix}", self.name),
            DaemonSet => format!("{}-{}", self.name, self.suffixes[ix]),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Fate {
    Running,
    CrashLoop(u32),
    Pending,
}

/// The two unhealthy pods of the demo: the second `orders-worker` crash
/// loops, the only `payments-worker` cannot be scheduled.
fn fate(workload: &Workload, ix: usize) -> Fate {
    match (workload.name, ix) {
        ("orders-worker", 1) => Fate::CrashLoop(14),
        ("payments-worker", 0) => Fate::Pending,
        _ => Fate::Running,
    }
}

/// Restarts of a healthy pod: mostly none, a few have restarted once or twice.
fn healthy_restarts(workload: &Workload, ix: usize) -> u32 {
    match (workload.name.len() + ix * 3) % 7 {
        0 => 2,
        3 => 1,
        _ => 0,
    }
}

/// Deterministic jitter so bars differ between replicas (`0.85..=1.15`).
fn jitter(ix: usize, salt: usize) -> f64 {
    0.85 + ((ix * 7 + salt * 5) % 7) as f64 * 0.05
}

fn timestamp(unix: i64) -> String {
    DateTime::from_timestamp(unix, 0)
        .unwrap_or_default()
        .to_rfc3339_opts(SecondsFormat::Secs, true)
}

fn meta(namespace: Option<&str>, name: &str, now: i64, age: i64) -> Value {
    let mut meta = json!({
        "name": name,
        "creationTimestamp": timestamp(now - age),
    });
    if let Some(namespace) = namespace {
        meta["namespace"] = json!(namespace);
    }
    meta
}

fn pod_object(workload: &Workload, ix: usize, now: i64) -> Value {
    let name = workload.pod_name(ix);
    let fate = fate(workload, ix);
    // Replicas of one workload were created together; the crash-looping and
    // pending pods are younger.
    let age = match fate {
        Fate::Pending => 7 * 60,
        _ => workload.age_days * DAY - (ix as i64 * 37 * 60) % DAY,
    };
    let restarts = match fate {
        Fate::CrashLoop(n) => n,
        _ => healthy_restarts(workload, ix),
    };
    let mut pod = json!({
        "metadata": meta(Some(workload.namespace), &name, now, age),
        "spec": {
            "containers": workload.containers.iter().map(|c| json!({"name": c})).collect::<Vec<_>>(),
        },
    });
    pod["metadata"]["ownerReferences"] = json!([{
        "kind": match workload.controller { Deployment => "ReplicaSet", c => c.kind() },
        "name": workload.owner_name(),
        "controller": true,
    }]);
    pod["status"] = match fate {
        Fate::Pending => json!({
            "phase": "Pending",
            "conditions": [{"type": "PodScheduled", "status": "False", "reason": "Unschedulable"}],
        }),
        Fate::CrashLoop(_) => json!({
            "phase": "Running",
            "conditions": [
                {"type": "Initialized", "status": "True"},
                {"type": "Ready", "status": "False"},
            ],
            "containerStatuses": workload.containers.iter().map(|c| json!({
                "name": c,
                "ready": false,
                "started": false,
                "restartCount": restarts,
                "state": {"waiting": {"reason": "CrashLoopBackOff"}},
            })).collect::<Vec<_>>(),
        }),
        Fate::Running => json!({
            "phase": "Running",
            "conditions": [
                {"type": "Initialized", "status": "True"},
                {"type": "Ready", "status": "True"},
            ],
            "containerStatuses": workload.containers.iter().enumerate().map(|(c, container)| json!({
                "name": container,
                "ready": true,
                "started": true,
                // Restarts are reported on the first container only.
                "restartCount": if c == 0 { restarts } else { 0 },
                "state": {"running": {}},
            })).collect::<Vec<_>>(),
        }),
    };
    pod
}

fn replica_ready(workload: &Workload) -> i64 {
    (0..workload.replicas())
        .filter(|ix| fate(workload, *ix) == Fate::Running)
        .count() as i64
}

fn deployment_object(workload: &Workload, now: i64) -> Value {
    let ready = replica_ready(workload);
    let mut object = json!({
        "metadata": meta(Some(workload.namespace), workload.name, now, workload.age_days * DAY),
        "spec": {"replicas": workload.replicas()},
        "status": {
            "readyReplicas": ready,
            "conditions": [
                {"type": "Available", "status": if ready > 0 { "True" } else { "False" }},
                {"type": "Progressing", "status": "True"},
            ],
        },
    });
    object["metadata"]["labels"] = json!({"app": workload.name});
    object
}

fn replica_set_object(workload: &Workload, now: i64) -> Value {
    let name = workload.owner_name();
    let mut object = json!({
        "metadata": meta(Some(workload.namespace), &name, now, workload.age_days * DAY),
        "spec": {"replicas": workload.replicas()},
        "status": {"replicas": workload.replicas(), "readyReplicas": replica_ready(workload)},
    });
    object["metadata"]["ownerReferences"] =
        json!([{"kind": "Deployment", "name": workload.name, "controller": true}]);
    object
}

fn stateful_set_object(workload: &Workload, now: i64) -> Value {
    json!({
        "metadata": meta(Some(workload.namespace), workload.name, now, workload.age_days * DAY),
        "spec": {"replicas": workload.replicas()},
        "status": {"readyReplicas": replica_ready(workload)},
    })
}

fn daemon_set_object(workload: &Workload, now: i64) -> Value {
    json!({
        "metadata": meta(Some(workload.namespace), workload.name, now, workload.age_days * DAY),
        "spec": {"template": {"spec": {"nodeSelector": {"kubernetes.io/os": "linux"}}}},
        "status": {
            "desiredNumberScheduled": workload.replicas(),
            "numberReady": replica_ready(workload),
        },
    })
}

/// `(namespace, name, type, cluster IP, ports, selector app)`.
type DemoService = (
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    &'static [(u16, u16)],
    &'static str,
);

const SERVICES: &[DemoService] = &[
    (
        "shop",
        "checkout-api",
        "ClusterIP",
        "10.96.12.41",
        &[(80, 0)],
        "checkout-api",
    ),
    (
        "shop",
        "orders-worker",
        "ClusterIP",
        "10.96.12.58",
        &[(9090, 0)],
        "orders-worker",
    ),
    (
        "shop",
        "storefront-web",
        "ClusterIP",
        "10.96.14.7",
        &[(80, 0)],
        "storefront-web",
    ),
    (
        "shop",
        "catalog-api",
        "ClusterIP",
        "10.96.14.93",
        &[(8080, 0)],
        "catalog-api",
    ),
    (
        "shop",
        "postgres",
        "ClusterIP",
        "10.96.15.20",
        &[(5432, 0)],
        "postgres",
    ),
    (
        "payments",
        "payments-api",
        "ClusterIP",
        "10.96.22.64",
        &[(443, 0)],
        "payments-api",
    ),
    (
        "payments",
        "ledger-db",
        "ClusterIP",
        "10.96.22.101",
        &[(5432, 0)],
        "ledger-db",
    ),
    (
        "auth",
        "auth-gateway",
        "ClusterIP",
        "10.96.31.15",
        &[(443, 0)],
        "auth-gateway",
    ),
    (
        "auth",
        "token-service",
        "ClusterIP",
        "10.96.31.88",
        &[(8443, 0)],
        "token-service",
    ),
    (
        "auth",
        "auth-redis",
        "ClusterIP",
        "10.96.31.140",
        &[(6379, 0)],
        "auth-redis",
    ),
    (
        "monitoring",
        "prometheus",
        "ClusterIP",
        "10.96.40.9",
        &[(9090, 0)],
        "prometheus",
    ),
    (
        "monitoring",
        "grafana",
        "ClusterIP",
        "10.96.40.77",
        &[(3000, 0)],
        "grafana",
    ),
    (
        "ingress",
        "ingress-nginx-controller",
        "LoadBalancer",
        "10.96.50.3",
        &[(80, 31080), (443, 31443)],
        "ingress-nginx-controller",
    ),
    (
        "kube-system",
        "kube-dns",
        "ClusterIP",
        "10.96.0.10",
        &[(53, 0)],
        "coredns",
    ),
    (
        "kube-system",
        "metrics-server",
        "ClusterIP",
        "10.96.0.84",
        &[(443, 0)],
        "metrics-server",
    ),
];

fn service_object(entry: &DemoService, now: i64) -> Value {
    let (namespace, name, kind, cluster_ip, ports, app) = *entry;
    let mut object = json!({
        "metadata": meta(Some(namespace), name, now, 30 * DAY),
        "spec": {
            "type": kind,
            "clusterIP": cluster_ip,
            "ports": ports.iter().map(|(port, node)| json!({
                "port": port,
                "nodePort": node,
                "protocol": "TCP",
            })).collect::<Vec<_>>(),
            "selector": {"app": app},
        },
    });
    if kind == "LoadBalancer" {
        object["status"] =
            json!({"loadBalancer": {"ingress": [{"hostname": "demo-lb.example.com"}]}});
    }
    object
}

/// `(namespace, name, host)`.
const INGRESSES: &[(&str, &str, &str)] = &[
    ("shop", "storefront", "shop.example.com"),
    ("payments", "payments-api", "pay.example.com"),
    ("auth", "auth-gateway", "login.example.com"),
    ("monitoring", "grafana", "metrics.example.com"),
];

fn ingress_object(entry: &(&str, &str, &str), now: i64) -> Value {
    json!({
        "metadata": meta(Some(entry.0), entry.1, now, 30 * DAY),
        "spec": {"rules": [{"host": entry.2}]},
        "status": {"loadBalancer": {"ingress": [{"hostname": "demo-lb.example.com"}]}},
    })
}

/// `(namespace, name, keys)`.
const CONFIG_MAPS: &[(&str, &str, &[&str])] = &[
    ("shop", "checkout-config", &["FEATURE_FLAGS", "LOG_LEVEL"]),
    ("shop", "catalog-config", &["CACHE_TTL", "PAGE_SIZE"]),
    ("payments", "payments-config", &["CURRENCY", "RETRY_LIMIT"]),
    ("auth", "gateway-config", &["gateway.yaml"]),
    (
        "monitoring",
        "prometheus-rules",
        &["alerts.yaml", "recording.yaml"],
    ),
    ("kube-system", "coredns", &["Corefile"]),
];

fn config_map_object(entry: &(&str, &str, &[&str]), now: i64) -> Value {
    let data: serde_json::Map<String, Value> = entry
        .2
        .iter()
        .map(|key| (key.to_string(), json!("demo")))
        .collect();
    json!({
        "metadata": meta(Some(entry.0), entry.1, now, 30 * DAY),
        "data": data,
    })
}

/// `(namespace, name, type, keys)`; values are never part of the demo.
const SECRETS: &[(&str, &str, &str, &[&str])] = &[
    (
        "shop",
        "postgres-credentials",
        "Opaque",
        &["password", "username"],
    ),
    ("payments", "ledger-credentials", "Opaque", &["password"]),
    (
        "auth",
        "signing-keys",
        "Opaque",
        &["private.pem", "public.pem"],
    ),
    (
        "ingress",
        "storefront-tls",
        "kubernetes.io/tls",
        &["tls.crt", "tls.key"],
    ),
];

fn secret_object(entry: &(&str, &str, &str, &[&str]), now: i64) -> Value {
    let data: serde_json::Map<String, Value> = entry
        .3
        .iter()
        .map(|key| (key.to_string(), json!("")))
        .collect();
    json!({
        "metadata": meta(Some(entry.0), entry.1, now, 30 * DAY),
        "type": entry.2,
        "data": data,
    })
}

fn node_object(ix: usize, now: i64) -> Value {
    let (name, cpu, memory) = NODES[ix];
    let role = if ix == 0 { "control-plane" } else { "worker" };
    json!({
        "metadata": {
            "name": name,
            "creationTimestamp": timestamp(now - 90 * DAY),
            "labels": {format!("node-role.kubernetes.io/{role}"): ""},
        },
        "spec": {"taints": if ix == 0 { json!([{"key": "node-role.kubernetes.io/control-plane"}]) } else { json!([]) }},
        "status": {
            "allocatable": {"cpu": cpu.to_string(), "memory": format!("{memory}Mi")},
            "nodeInfo": {"kubeletVersion": "v1.31.4"},
            "conditions": [{"type": "Ready", "status": "True"}],
        },
    })
}

fn namespace_object(name: &str, now: i64) -> Value {
    json!({
        "metadata": {
            "name": name,
            "creationTimestamp": timestamp(now - 90 * DAY),
            "labels": {"kubernetes.io/metadata.name": name},
        },
        "status": {"phase": "Active"},
    })
}

/// `(namespace, name, storage class, size)`.
const VOLUME_CLAIMS: &[(&str, &str, &str, &str)] = &[
    ("shop", "data-postgres-0", "standard", "20Gi"),
    ("payments", "data-ledger-db-0", "standard", "50Gi"),
    ("auth", "data-auth-redis-0", "standard", "5Gi"),
    ("monitoring", "data-prometheus-0", "standard", "100Gi"),
];

fn volume_claim_object(entry: &(&str, &str, &str, &str), now: i64) -> Value {
    json!({
        "metadata": meta(Some(entry.0), entry.1, now, 45 * DAY),
        "spec": {"storageClassName": entry.2},
        "status": {"phase": "Bound", "capacity": {"storage": entry.3}},
    })
}

fn cron_job_object(now: i64) -> Value {
    json!({
        "metadata": meta(Some("shop"), "nightly-report", now, 20 * DAY),
        "spec": {"schedule": "0 2 * * *", "suspend": false},
        "status": {"lastScheduleTime": timestamp(now - 7 * HOUR)},
    })
}

fn job_object(now: i64) -> Value {
    json!({
        "metadata": meta(Some("shop"), "nightly-report-29174400", now, 7 * HOUR),
        "spec": {"completions": 1},
        "status": {"succeeded": 1, "conditions": [{"type": "Complete", "status": "True"}]},
    })
}

fn hpa_object(now: i64) -> Value {
    json!({
        "metadata": meta(Some("shop"), "checkout-api", now, 21 * DAY),
        "spec": {
            "scaleTargetRef": {"kind": "Deployment", "name": "checkout-api"},
            "minReplicas": 2,
            "maxReplicas": 6,
        },
        "status": {"currentReplicas": 3},
    })
}

/// `(namespace, minutes ago, type, reason, kind, object, message, source, count)`.
type DemoEvent = (
    &'static str,
    i64,
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    i64,
);

const EVENTS: &[DemoEvent] = &[
    (
        "shop",
        2,
        "Warning",
        "BackOff",
        "Pod",
        "orders-worker-5c8b7d94f-t2rlc",
        "Back-off restarting failed container worker in pod orders-worker-5c8b7d94f-t2rlc",
        "kubelet",
        142,
    ),
    (
        "payments",
        6,
        "Warning",
        "FailedScheduling",
        "Pod",
        "payments-worker-7b8c9d6f4-v2hgx",
        "0/3 nodes are available: 3 Insufficient memory.",
        "default-scheduler",
        4,
    ),
    (
        "shop",
        9,
        "Normal",
        "Pulled",
        "Pod",
        "orders-worker-5c8b7d94f-t2rlc",
        "Container image \"registry.example.com/shop/orders-worker:1.8.2\" already present on machine",
        "kubelet",
        15,
    ),
    (
        "shop",
        14,
        "Normal",
        "ScalingReplicaSet",
        "Deployment",
        "checkout-api",
        "Scaled up replica set checkout-api-7d9f8c6b5 to 3",
        "deployment-controller",
        1,
    ),
    (
        "shop",
        14,
        "Normal",
        "SuccessfulCreate",
        "ReplicaSet",
        "checkout-api-7d9f8c6b5",
        "Created pod: checkout-api-7d9f8c6b5-q9zt7",
        "replicaset-controller",
        1,
    ),
    (
        "shop",
        14,
        "Normal",
        "Scheduled",
        "Pod",
        "checkout-api-7d9f8c6b5-q9zt7",
        "Successfully assigned shop/checkout-api-7d9f8c6b5-q9zt7 to demo-node-2",
        "default-scheduler",
        1,
    ),
    (
        "shop",
        13,
        "Normal",
        "Started",
        "Pod",
        "checkout-api-7d9f8c6b5-q9zt7",
        "Started container api",
        "kubelet",
        1,
    ),
    (
        "monitoring",
        38,
        "Normal",
        "SuccessfulAttachVolume",
        "Pod",
        "prometheus-0",
        "AttachVolume.Attach succeeded for volume \"pvc-demo-prometheus\"",
        "attachdetach-controller",
        1,
    ),
    (
        "ingress",
        55,
        "Normal",
        "Sync",
        "Ingress",
        "storefront",
        "Scheduled for sync",
        "nginx-ingress-controller",
        3,
    ),
    (
        "auth",
        71,
        "Warning",
        "Unhealthy",
        "Pod",
        "auth-gateway-5d7c9b8f6-j9sbk",
        "Readiness probe failed: HTTP probe failed with statuscode: 503",
        "kubelet",
        2,
    ),
];

fn event_object(entry: &DemoEvent, ix: usize, now: i64) -> Value {
    let (namespace, minutes, kind, reason, involved_kind, involved, message, source, count) =
        *entry;
    json!({
        "metadata": meta(Some(namespace), &format!("{involved}.{ix:x}a3f"), now, minutes * 60 + 600),
        "type": kind,
        "reason": reason,
        "message": message,
        "count": count,
        "source": {"component": source},
        "involvedObject": {"kind": involved_kind, "name": involved},
        "lastTimestamp": timestamp(now - minutes * 60),
    })
}

/// The raw demo objects of a built-in kind (empty for kinds the demo does not
/// populate). Timestamps are relative to `now`.
pub fn demo_objects(kind: &ResourceKind, now: i64) -> Vec<Value> {
    let controlled =
        |controller: Controller| WORKLOADS.iter().filter(move |w| w.controller == controller);
    match (kind.group.as_ref(), kind.plural.as_ref()) {
        ("", "pods") => WORKLOADS
            .iter()
            .flat_map(|w| (0..w.replicas()).map(move |ix| pod_object(w, ix, now)))
            .collect(),
        ("apps", "deployments") => controlled(Deployment)
            .map(|w| deployment_object(w, now))
            .collect(),
        ("apps", "replicasets") => controlled(Deployment)
            .map(|w| replica_set_object(w, now))
            .collect(),
        ("apps", "statefulsets") => controlled(StatefulSet)
            .map(|w| stateful_set_object(w, now))
            .collect(),
        ("apps", "daemonsets") => controlled(DaemonSet)
            .map(|w| daemon_set_object(w, now))
            .collect(),
        ("batch", "cronjobs") => vec![cron_job_object(now)],
        ("batch", "jobs") => vec![job_object(now)],
        ("autoscaling", "horizontalpodautoscalers") => vec![hpa_object(now)],
        ("", "services") => SERVICES.iter().map(|s| service_object(s, now)).collect(),
        ("networking.k8s.io", "ingresses") => {
            INGRESSES.iter().map(|i| ingress_object(i, now)).collect()
        }
        ("", "configmaps") => CONFIG_MAPS
            .iter()
            .map(|c| config_map_object(c, now))
            .collect(),
        ("", "secrets") => SECRETS.iter().map(|s| secret_object(s, now)).collect(),
        ("", "persistentvolumeclaims") => VOLUME_CLAIMS
            .iter()
            .map(|v| volume_claim_object(v, now))
            .collect(),
        ("", "nodes") => (0..NODES.len()).map(|ix| node_object(ix, now)).collect(),
        ("", "namespaces") => DEMO_NAMESPACES
            .iter()
            .map(|n| namespace_object(n, now))
            .collect(),
        ("", "events") => EVENTS
            .iter()
            .enumerate()
            .map(|(ix, e)| event_object(e, ix, now))
            .collect(),
        _ => Vec::new(),
    }
}

/// Table rows of a demo kind, built by the kind's real printer.
pub fn demo_rows(kind: &ResourceKind, now: i64) -> Vec<ObjectRow> {
    let printer = printer_for(kind);
    demo_objects(kind, now)
        .iter()
        .map(|object| printer.row(object))
        .collect()
}

/// Namespace names of the demo cluster, for the filter dropdown.
pub fn demo_namespaces() -> Vec<String> {
    DEMO_NAMESPACES.iter().map(|n| n.to_string()).collect()
}

/// Live CPU/memory usage of the demo pods and nodes.
pub fn demo_usage() -> UsageMap {
    let mut map = UsageMap::default();
    let mut per_node = [(0.0f64, 0u64); NODES.len()];
    for workload in WORKLOADS {
        for ix in 0..workload.replicas() {
            if fate(workload, ix) == Fate::Pending {
                continue;
            }
            let cpu_cores = f64::from(workload.cpu_millis) * jitter(ix, 1) / 1000.0;
            let memory_bytes =
                (f64::from(workload.memory_mib) * jitter(ix, 2) * 1024.0 * 1024.0) as u64;
            let node = (workload.name.len() + ix) % NODES.len();
            per_node[node].0 += cpu_cores;
            per_node[node].1 += memory_bytes;
            map.insert(
                usage_key(Some(workload.namespace), &workload.pod_name(ix)),
                Usage {
                    cpu_cores,
                    memory_bytes,
                },
            );
        }
    }
    for (ix, (name, _, _)) in NODES.iter().enumerate() {
        // Node-level usage adds the system daemons on top of the pods.
        map.insert(
            (*name).to_string(),
            Usage {
                cpu_cores: per_node[ix].0 + 0.35,
                memory_bytes: per_node[ix].1 + 1_400 * 1024 * 1024,
            },
        );
    }
    map
}

/// An [`ObjectSource`] over the demo objects of one kind: one listing, then
/// the stream stays open.
pub struct DemoSource {
    kind: ResourceKind,
    now: i64,
}

impl DemoSource {
    pub fn new(kind: ResourceKind, now: i64) -> Self {
        Self { kind, now }
    }
}

impl ObjectSource for DemoSource {
    fn watch(&self) -> RowEventStream {
        let rows = demo_rows(&self.kind, self.now);
        futures::stream::once(async move { Ok(RowEvent::Restarted(rows)) })
            .chain(futures::stream::pending())
            .boxed()
    }
}

/// A [`UsageSource`] that yields the demo metrics once and stays open.
pub struct DemoUsage;

impl UsageSource for DemoUsage {
    fn poll(&self) -> UsageStream {
        futures::stream::once(async { demo_usage() })
            .chain(futures::stream::pending())
            .boxed()
    }
}

/// Whether the demo populates this kind (used to decide which sidebar views
/// are non-empty).
pub fn demo_has(kind: &ResourceKind) -> bool {
    kind == &kinds::POD || !demo_objects(kind, 0).is_empty()
}
