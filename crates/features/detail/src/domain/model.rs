//! View model of one object: everything the detail panel shows, derived from
//! the raw JSON so it works for built-in kinds and CRDs alike.

use chrono::DateTime;
use natikube_core::{ResourceKind, age_since, kinds};
use serde_json::Value;

use super::secret::{LAST_APPLIED, MASK, SecretEntry, secret_entries};

/// Semantic color of a value; the UI maps it to a theme tone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Health {
    Ok,
    Done,
    Warning,
    Error,
    Muted,
}

/// A reference to another object (owner, node) that the UI renders as a link.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjectLink {
    pub kind: ResourceKind,
    pub name: String,
}

/// One label/value row.
#[derive(Debug, Clone, PartialEq)]
pub struct Field {
    pub label: String,
    pub value: String,
    pub health: Option<Health>,
    pub link: Option<ObjectLink>,
}

impl Field {
    fn new(label: &str, value: impl Into<String>) -> Self {
        Self {
            label: label.to_string(),
            value: value.into(),
            health: None,
            link: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ConditionRow {
    pub kind: String,
    pub status: String,
    pub reason: String,
    pub message: String,
    pub last_transition: Option<i64>,
    pub health: Health,
}

impl ConditionRow {
    pub fn age(&self, now: i64) -> String {
        age_since(self.last_transition, now)
    }
}

/// A port that can be forwarded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContainerPort {
    pub label: String,
    pub number: u16,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ContainerInfo {
    pub name: String,
    pub image: String,
    pub init: bool,
    pub ports: Vec<ContainerPort>,
    pub env: Vec<String>,
    pub mounts: Vec<String>,
    /// (kind, description), e.g. ("Liveness", "http-get :8080/healthz ...").
    pub probes: Vec<(String, String)>,
    /// (kind, "cpu 100m, memory 64Mi") for Requests and Limits.
    pub resources: Vec<(String, String)>,
    /// Runtime state label and color; `None` for workload templates.
    pub state: Option<(String, Health)>,
    pub restarts: Option<i64>,
    pub ready: Option<bool>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EventRow {
    pub health: Health,
    pub kind: String,
    pub reason: String,
    pub message: String,
    pub count: i64,
    pub last_seen: Option<i64>,
    pub source: String,
}

impl EventRow {
    pub fn age(&self, now: i64) -> String {
        age_since(self.last_seen, now)
    }
}

#[derive(Debug, Clone)]
pub struct DetailModel {
    pub kind: ResourceKind,
    pub name: String,
    pub namespace: Option<String>,
    pub created: Option<i64>,
    pub deleting: bool,
    pub metadata: Vec<Field>,
    pub labels: Vec<(String, String)>,
    pub annotations: Vec<(String, String)>,
    pub owners: Vec<ObjectLink>,
    pub status: Vec<Field>,
    pub conditions: Vec<ConditionRow>,
    pub containers: Vec<ContainerInfo>,
    /// Service ports (forwardable).
    pub ports: Vec<ContainerPort>,
    /// ConfigMap entries.
    pub data: Vec<(String, String)>,
    /// Secret entries (values stay hidden until revealed).
    pub secret: Vec<SecretEntry>,
}

impl DetailModel {
    pub fn age(&self, now: i64) -> String {
        age_since(self.created, now)
    }

    /// Builds the model of `obj` (a full object as returned by the API).
    pub fn build(obj: &Value, kind: &ResourceKind, _now: i64) -> Self {
        let meta = obj.get("metadata").cloned().unwrap_or(Value::Null);
        let name = str_at(&meta, "name").unwrap_or_default();
        let namespace = str_at(&meta, "namespace");
        let created = str_at(&meta, "creationTimestamp").and_then(|t| parse_time(&t));
        let deleting = meta.get("deletionTimestamp").is_some();

        let mut metadata = vec![Field::new("Name", name.clone())];
        if let Some(ns) = &namespace {
            metadata.push(Field::new("Namespace", ns.clone()));
        }
        if let Some(ts) = str_at(&meta, "creationTimestamp") {
            metadata.push(Field::new("Created", ts));
        }
        if let Some(uid) = str_at(&meta, "uid") {
            metadata.push(Field::new("UID", uid));
        }
        if let Some(rv) = str_at(&meta, "resourceVersion") {
            metadata.push(Field::new("Resource version", rv));
        }
        if let Some(finalizers) = meta.get("finalizers").and_then(Value::as_array) {
            let names: Vec<&str> = finalizers.iter().filter_map(Value::as_str).collect();
            if !names.is_empty() {
                metadata.push(Field::new("Finalizers", names.join(", ")));
            }
        }
        if let Some(ts) = str_at(&meta, "deletionTimestamp") {
            let mut field = Field::new("Deleting since", ts);
            field.health = Some(Health::Warning);
            metadata.push(field);
        }

        let (containers, ports) = (containers(obj, kind), service_ports(obj, kind));

        let is_secret = *kind == kinds::SECRET;
        Self {
            kind: kind.clone(),
            name,
            namespace,
            created,
            deleting,
            metadata,
            labels: string_map(meta.get("labels")),
            annotations: {
                let mut annotations = string_map(meta.get("annotations"));
                if is_secret {
                    // The applied manifest of a Secret embeds its data values.
                    for (key, value) in &mut annotations {
                        if key == LAST_APPLIED {
                            *value = MASK.to_string();
                        }
                    }
                }
                annotations
            },
            owners: owners(&meta),
            status: status_fields(obj, kind),
            conditions: conditions(obj),
            containers,
            ports,
            data: if *kind == kinds::CONFIG_MAP {
                string_map(obj.get("data"))
            } else {
                Vec::new()
            },
            secret: if is_secret {
                secret_entries(obj)
            } else {
                Vec::new()
            },
        }
    }
}

fn str_at(v: &Value, key: &str) -> Option<String> {
    v.get(key).and_then(Value::as_str).map(str::to_string)
}

fn parse_time(text: &str) -> Option<i64> {
    DateTime::parse_from_rfc3339(text)
        .ok()
        .map(|d| d.timestamp())
}

fn string_map(v: Option<&Value>) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = v
        .and_then(Value::as_object)
        .map(|m| {
            m.iter()
                .map(|(k, v)| {
                    let text = match v {
                        Value::String(s) => s.clone(),
                        other => other.to_string(),
                    };
                    (k.clone(), text)
                })
                .collect()
        })
        .unwrap_or_default();
    out.sort();
    out
}

fn scalar(v: &Value) -> Option<String> {
    match v {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(b.to_string()),
        _ => None,
    }
}

fn int_at(v: &Value, path: &[&str]) -> Option<i64> {
    let mut cur = v;
    for key in path {
        cur = cur.get(key)?;
    }
    cur.as_i64()
}

fn get_path<'a>(v: &'a Value, path: &[&str]) -> Option<&'a Value> {
    let mut cur = v;
    for key in path {
        cur = cur.get(key)?;
    }
    Some(cur)
}

// ---- owners -------------------------------------------------------------

fn owners(meta: &Value) -> Vec<ObjectLink> {
    meta.get("ownerReferences")
        .and_then(Value::as_array)
        .map(|refs| {
            refs.iter()
                .filter_map(|r| {
                    let api_version = str_at(r, "apiVersion")?;
                    let kind = str_at(r, "kind")?;
                    let name = str_at(r, "name")?;
                    Some(ObjectLink {
                        kind: kind_from_api(&api_version, &kind),
                        name,
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

fn kind_from_api(api_version: &str, kind: &str) -> ResourceKind {
    let (group, version) = api_version.split_once('/').unwrap_or(("", api_version));
    if let Some(builtin) = kinds::all_builtin()
        .into_iter()
        .find(|k| k.group == group && k.kind == kind)
    {
        return builtin;
    }
    ResourceKind::custom(
        group,
        version,
        kind,
        format!("{}s", kind.to_lowercase()),
        true,
    )
}

// ---- conditions ---------------------------------------------------------

/// Condition types where `True` means something is wrong.
fn inverted_condition(kind: &str) -> bool {
    kind.ends_with("Pressure")
        || matches!(
            kind,
            "NetworkUnavailable" | "Failed" | "Failure" | "ReplicaFailure" | "Stalled" | "Degraded"
        )
}

fn conditions(obj: &Value) -> Vec<ConditionRow> {
    obj.get("status")
        .and_then(|s| s.get("conditions"))
        .and_then(Value::as_array)
        .map(|list| {
            list.iter()
                .map(|c| {
                    let kind = str_at(c, "type").unwrap_or_default();
                    let status = str_at(c, "status").unwrap_or_default();
                    let health = match (status.as_str(), inverted_condition(&kind)) {
                        ("True", false) | ("False", true) => Health::Ok,
                        ("False", false) | ("True", true) => Health::Error,
                        _ => Health::Warning,
                    };
                    ConditionRow {
                        reason: str_at(c, "reason").unwrap_or_default(),
                        message: str_at(c, "message").unwrap_or_default(),
                        last_transition: str_at(c, "lastTransitionTime")
                            .or_else(|| str_at(c, "lastUpdateTime"))
                            .and_then(|t| parse_time(&t)),
                        kind,
                        status,
                        health,
                    }
                })
                .collect()
        })
        .unwrap_or_default()
}

// ---- containers ---------------------------------------------------------

fn pod_spec<'a>(obj: &'a Value, kind: &ResourceKind) -> Option<&'a Value> {
    if *kind == kinds::POD {
        return obj.get("spec");
    }
    get_path(obj, &["spec", "template", "spec"])
        .or_else(|| get_path(obj, &["spec", "jobTemplate", "spec", "template", "spec"]))
}

fn containers(obj: &Value, kind: &ResourceKind) -> Vec<ContainerInfo> {
    let Some(spec) = pod_spec(obj, kind) else {
        return Vec::new();
    };
    let is_pod = *kind == kinds::POD;
    let statuses = |key: &str| -> Vec<Value> {
        if !is_pod {
            return Vec::new();
        }
        obj.get("status")
            .and_then(|s| s.get(key))
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
    };
    let init_statuses = statuses("initContainerStatuses");
    let main_statuses = statuses("containerStatuses");
    let mut out = Vec::new();
    for (key, init, list) in [
        ("initContainers", true, &init_statuses),
        ("containers", false, &main_statuses),
    ] {
        let Some(specs) = spec.get(key).and_then(Value::as_array) else {
            continue;
        };
        for c in specs {
            let name = str_at(c, "name").unwrap_or_default();
            let status = list
                .iter()
                .find(|s| str_at(s, "name").as_deref() == Some(&name));
            out.push(container(c, init, status));
        }
    }
    out
}

fn container(c: &Value, init: bool, status: Option<&Value>) -> ContainerInfo {
    let ports = c
        .get("ports")
        .and_then(Value::as_array)
        .map(|ports| {
            ports
                .iter()
                .filter_map(|p| {
                    let number = p.get("containerPort")?.as_u64()? as u16;
                    let proto = str_at(p, "protocol").unwrap_or_else(|| "TCP".into());
                    let label = match str_at(p, "name") {
                        Some(n) => format!("{n} {number}/{proto}"),
                        None => format!("{number}/{proto}"),
                    };
                    Some(ContainerPort { label, number })
                })
                .collect()
        })
        .unwrap_or_default();
    let env = c
        .get("env")
        .and_then(Value::as_array)
        .map(|e| e.iter().filter_map(|v| str_at(v, "name")).collect())
        .unwrap_or_default();
    let mounts = c
        .get("volumeMounts")
        .and_then(Value::as_array)
        .map(|m| {
            m.iter()
                .map(|v| {
                    let ro = if v.get("readOnly").and_then(Value::as_bool) == Some(true) {
                        " (ro)"
                    } else {
                        ""
                    };
                    format!(
                        "{} -> {}{ro}",
                        str_at(v, "name").unwrap_or_default(),
                        str_at(v, "mountPath").unwrap_or_default()
                    )
                })
                .collect()
        })
        .unwrap_or_default();
    let probes = [
        ("Liveness", "livenessProbe"),
        ("Readiness", "readinessProbe"),
        ("Startup", "startupProbe"),
    ]
    .into_iter()
    .filter_map(|(label, key)| Some((label.to_string(), probe(c.get(key)?))))
    .collect();
    let resources = ["requests", "limits"]
        .into_iter()
        .filter_map(|key| {
            let map = c.get("resources")?.get(key)?.as_object()?;
            let mut items: Vec<String> = map
                .iter()
                .filter_map(|(k, v)| Some(format!("{k} {}", scalar(v)?)))
                .collect();
            items.sort();
            (!items.is_empty()).then(|| {
                let label = if key == "requests" {
                    "Requests"
                } else {
                    "Limits"
                };
                (label.to_string(), items.join(", "))
            })
        })
        .collect();
    ContainerInfo {
        name: str_at(c, "name").unwrap_or_default(),
        image: str_at(c, "image").unwrap_or_default(),
        init,
        ports,
        env,
        mounts,
        probes,
        resources,
        state: status.and_then(|s| container_state(s.get("state")?)),
        restarts: status.and_then(|s| s.get("restartCount")?.as_i64()),
        ready: status.and_then(|s| s.get("ready")?.as_bool()),
    }
}

fn probe(p: &Value) -> String {
    let port = |v: &Value| v.get("port").and_then(scalar).unwrap_or_default();
    let handler = if let Some(h) = p.get("httpGet") {
        format!(
            "http-get :{}{}",
            port(h),
            str_at(h, "path").unwrap_or_default()
        )
    } else if let Some(h) = p.get("tcpSocket") {
        format!("tcp-socket :{}", port(h))
    } else if let Some(h) = p.get("grpc") {
        format!("grpc :{}", port(h))
    } else if let Some(cmd) = get_path(p, &["exec", "command"]).and_then(Value::as_array) {
        let parts: Vec<&str> = cmd.iter().filter_map(Value::as_str).collect();
        format!("exec [{}]", parts.join(" "))
    } else {
        "unknown handler".to_string()
    };
    let n = |key: &str, default: i64| p.get(key).and_then(Value::as_i64).unwrap_or(default);
    format!(
        "{handler} delay={}s timeout={}s period={}s #success={} #failure={}",
        n("initialDelaySeconds", 0),
        n("timeoutSeconds", 1),
        n("periodSeconds", 10),
        n("successThreshold", 1),
        n("failureThreshold", 3),
    )
}

fn container_state(state: &Value) -> Option<(String, Health)> {
    if state.get("running").is_some() {
        return Some(("Running".into(), Health::Ok));
    }
    if let Some(t) = state.get("terminated") {
        let reason = str_at(t, "reason").unwrap_or_else(|| "Terminated".into());
        let health = if t.get("exitCode").and_then(Value::as_i64) == Some(0) {
            Health::Done
        } else {
            Health::Error
        };
        return Some((reason, health));
    }
    if let Some(w) = state.get("waiting") {
        let reason = str_at(w, "reason").unwrap_or_else(|| "Waiting".into());
        let health = match reason.as_str() {
            "ContainerCreating" | "PodInitializing" | "Waiting" => Health::Warning,
            _ => Health::Error,
        };
        return Some((reason, health));
    }
    None
}

// ---- service ports ------------------------------------------------------

fn service_ports(obj: &Value, kind: &ResourceKind) -> Vec<ContainerPort> {
    if *kind != kinds::SERVICE {
        return Vec::new();
    }
    obj.get("spec")
        .and_then(|s| s.get("ports"))
        .and_then(Value::as_array)
        .map(|ports| {
            ports
                .iter()
                .filter_map(|p| {
                    let number = p.get("port")?.as_u64()? as u16;
                    let proto = str_at(p, "protocol").unwrap_or_else(|| "TCP".into());
                    let mut label = match str_at(p, "name") {
                        Some(n) => format!("{n} {number}/{proto}"),
                        None => format!("{number}/{proto}"),
                    };
                    if let Some(target) = p.get("targetPort").and_then(scalar)
                        && target != number.to_string()
                    {
                        label.push_str(&format!(" -> {target}"));
                    }
                    Some(ContainerPort { label, number })
                })
                .collect()
        })
        .unwrap_or_default()
}

// ---- status fields ------------------------------------------------------

struct Fields(Vec<Field>);

impl Fields {
    fn add(&mut self, label: &str, value: Option<String>) {
        if let Some(v) = value.filter(|v| !v.is_empty()) {
            self.0.push(Field::new(label, v));
        }
    }
    fn add_health(&mut self, label: &str, value: String, health: Health) {
        let mut field = Field::new(label, value);
        field.health = Some(health);
        self.0.push(field);
    }
    fn add_link(&mut self, label: &str, kind: ResourceKind, name: Option<String>) {
        if let Some(name) = name.filter(|n| !n.is_empty()) {
            let mut field = Field::new(label, name.clone());
            field.link = Some(ObjectLink { kind, name });
            self.0.push(field);
        }
    }
}

fn selector(v: Option<&Value>) -> Option<String> {
    let map = v?.as_object()?;
    let mut items: Vec<String> = map
        .iter()
        .filter_map(|(k, v)| Some(format!("{k}={}", scalar(v)?)))
        .collect();
    items.sort();
    Some(items.join(","))
}

fn quantities(v: Option<&Value>) -> Option<String> {
    let map = v?.as_object()?;
    let mut items: Vec<String> = map
        .iter()
        .filter_map(|(k, v)| Some(format!("{k} {}", scalar(v)?)))
        .collect();
    items.sort();
    Some(items.join(", "))
}

fn joined(v: Option<&Value>) -> Option<String> {
    let list = v?.as_array()?;
    let items: Vec<String> = list.iter().filter_map(scalar).collect();
    Some(items.join(", "))
}

fn status_fields(obj: &Value, kind: &ResourceKind) -> Vec<Field> {
    let spec = obj.get("spec").cloned().unwrap_or(Value::Null);
    let status = obj.get("status").cloned().unwrap_or(Value::Null);
    let st = |key: &str| status.get(key).and_then(scalar);
    let sp = |key: &str| spec.get(key).and_then(scalar);
    let n = |path: &[&str]| int_at(&status, path).unwrap_or(0);
    let mut f = Fields(Vec::new());

    if *kind == kinds::POD {
        let (label, health) = pod_status(obj);
        f.add_health("Status", label, health);
        f.add_link("Node", kinds::NODE, sp("nodeName"));
        f.add("Pod IP", st("podIP"));
        f.add("Host IP", st("hostIP"));
        f.add("QoS class", st("qosClass"));
        f.add("Service account", sp("serviceAccountName"));
        f.add("Restart policy", sp("restartPolicy"));
    } else if *kind == kinds::DEPLOYMENT {
        let total = n(&["replicas"]);
        let available = n(&["availableReplicas"]);
        let unavailable =
            int_at(&status, &["unavailableReplicas"]).unwrap_or((total - available).max(0));
        f.add(
            "Replicas",
            Some(format!(
                "{} desired, {} updated, {total} total, {available} available, {unavailable} unavailable",
                int_at(&spec, &["replicas"]).unwrap_or(1),
                n(&["updatedReplicas"]),
            )),
        );
        f.add(
            "Strategy",
            get_path(&spec, &["strategy", "type"]).and_then(scalar),
        );
        f.add(
            "Selector",
            selector(get_path(&spec, &["selector", "matchLabels"])),
        );
    } else if *kind == kinds::STATEFUL_SET {
        f.add(
            "Replicas",
            Some(format!(
                "{} desired, {} ready, {} updated",
                int_at(&spec, &["replicas"]).unwrap_or(1),
                n(&["readyReplicas"]),
                n(&["updatedReplicas"]),
            )),
        );
        f.add("Service name", sp("serviceName"));
        f.add(
            "Update strategy",
            get_path(&spec, &["updateStrategy", "type"]).and_then(scalar),
        );
        f.add(
            "Selector",
            selector(get_path(&spec, &["selector", "matchLabels"])),
        );
    } else if *kind == kinds::DAEMON_SET {
        f.add(
            "Pods",
            Some(format!(
                "{} desired, {} current, {} ready, {} up-to-date, {} available",
                n(&["desiredNumberScheduled"]),
                n(&["currentNumberScheduled"]),
                n(&["numberReady"]),
                n(&["updatedNumberScheduled"]),
                n(&["numberAvailable"]),
            )),
        );
        f.add(
            "Selector",
            selector(get_path(&spec, &["selector", "matchLabels"])),
        );
        f.add(
            "Node selector",
            selector(get_path(&spec, &["template", "spec", "nodeSelector"])),
        );
    } else if *kind == kinds::REPLICA_SET || *kind == kinds::REPLICATION_CONTROLLER {
        f.add(
            "Replicas",
            Some(format!(
                "{} desired, {} ready",
                int_at(&spec, &["replicas"]).unwrap_or(1),
                n(&["readyReplicas"]),
            )),
        );
        let sel = get_path(&spec, &["selector", "matchLabels"]).or_else(|| spec.get("selector"));
        f.add("Selector", selector(sel));
    } else if *kind == kinds::JOB {
        f.add("Completions", sp("completions"));
        f.add("Parallelism", sp("parallelism"));
        f.add(
            "Pods",
            Some(format!(
                "{} succeeded, {} failed, {} active",
                n(&["succeeded"]),
                n(&["failed"]),
                n(&["active"]),
            )),
        );
        f.add("Start time", st("startTime"));
        f.add("Completion time", st("completionTime"));
    } else if *kind == kinds::CRON_JOB {
        f.add("Schedule", sp("schedule"));
        f.add("Suspend", sp("suspend"));
        f.add("Last schedule", st("lastScheduleTime"));
        f.add(
            "Active jobs",
            status
                .get("active")
                .and_then(Value::as_array)
                .map(|a| a.len().to_string()),
        );
    } else if *kind == kinds::SERVICE {
        f.add("Type", sp("type"));
        f.add("Cluster IP", sp("clusterIP"));
        f.add("External IPs", joined(spec.get("externalIPs")));
        let lb: Vec<String> = get_path(&status, &["loadBalancer", "ingress"])
            .and_then(Value::as_array)
            .map(|l| {
                l.iter()
                    .filter_map(|i| str_at(i, "ip").or_else(|| str_at(i, "hostname")))
                    .collect()
            })
            .unwrap_or_default();
        f.add("Load balancer", Some(lb.join(", ")));
        f.add("Selector", selector(spec.get("selector")));
        f.add("Session affinity", sp("sessionAffinity"));
    } else if *kind == kinds::NODE {
        let info = status.get("nodeInfo").cloned().unwrap_or(Value::Null);
        let ni = |key: &str| info.get(key).and_then(scalar);
        f.add("Kubelet version", ni("kubeletVersion"));
        f.add("OS image", ni("osImage"));
        f.add("Kernel version", ni("kernelVersion"));
        f.add("Container runtime", ni("containerRuntimeVersion"));
        f.add("Architecture", ni("architecture"));
        if let Some(addrs) = status.get("addresses").and_then(Value::as_array) {
            for a in addrs {
                let label = match str_at(a, "type").as_deref() {
                    Some("InternalIP") => "Internal IP".to_string(),
                    Some("ExternalIP") => "External IP".to_string(),
                    Some("Hostname") => "Hostname".to_string(),
                    Some(other) => other.to_string(),
                    None => continue,
                };
                f.add(&label, str_at(a, "address"));
            }
        }
        f.add("Capacity", quantities(status.get("capacity")));
        f.add("Allocatable", quantities(status.get("allocatable")));
        let taints: Vec<String> = spec
            .get("taints")
            .and_then(Value::as_array)
            .map(|t| {
                t.iter()
                    .map(|t| {
                        let key = str_at(t, "key").unwrap_or_default();
                        let effect = str_at(t, "effect").unwrap_or_default();
                        match str_at(t, "value") {
                            Some(v) if !v.is_empty() => format!("{key}={v}:{effect}"),
                            _ => format!("{key}:{effect}"),
                        }
                    })
                    .collect()
            })
            .unwrap_or_default();
        f.add("Taints", Some(taints.join(", ")));
        if spec.get("unschedulable").and_then(Value::as_bool) == Some(true) {
            f.add_health("Scheduling", "Cordoned".into(), Health::Warning);
        }
    } else if *kind == kinds::NAMESPACE {
        f.add("Phase", st("phase"));
    } else if *kind == kinds::PERSISTENT_VOLUME_CLAIM {
        let phase = st("phase").unwrap_or_default();
        let health = if phase == "Bound" {
            Health::Ok
        } else {
            Health::Warning
        };
        f.add_health("Status", phase, health);
        f.add("Volume", sp("volumeName"));
        f.add("Capacity", quantities(status.get("capacity")));
        f.add("Storage class", sp("storageClassName"));
        f.add("Access modes", joined(spec.get("accessModes")));
    } else if *kind == kinds::PERSISTENT_VOLUME {
        let phase = st("phase").unwrap_or_default();
        let health = if phase == "Bound" || phase == "Available" {
            Health::Ok
        } else {
            Health::Warning
        };
        f.add_health("Status", phase, health);
        f.add("Capacity", quantities(spec.get("capacity")));
        f.add("Storage class", sp("storageClassName"));
        f.add("Reclaim policy", sp("persistentVolumeReclaimPolicy"));
        let claim = get_path(&spec, &["claimRef", "name"]).and_then(scalar);
        f.add("Claim", claim);
    } else if *kind == kinds::SECRET {
        f.add("Type", obj.get("type").and_then(scalar));
    } else if *kind == kinds::INGRESS {
        f.add("Class", sp("ingressClassName"));
        let hosts: Vec<String> = spec
            .get("rules")
            .and_then(Value::as_array)
            .map(|r| r.iter().filter_map(|r| str_at(r, "host")).collect())
            .unwrap_or_default();
        f.add("Hosts", Some(hosts.join(", ")));
    } else if let Some(map) = status.as_object() {
        for (key, value) in map.iter().filter(|(_, v)| scalar(v).is_some()).take(12) {
            f.add(&humanize(key), scalar(value));
        }
    }
    f.0
}

/// `readyReplicas` -> `Ready replicas`.
fn humanize(key: &str) -> String {
    let mut out = String::new();
    for (i, ch) in key.chars().enumerate() {
        if ch.is_uppercase() && i > 0 {
            out.push(' ');
            out.extend(ch.to_lowercase());
        } else if i == 0 {
            out.extend(ch.to_uppercase());
        } else {
            out.push(ch);
        }
    }
    out
}

/// Pod status the way the list shows it: waiting/terminated reasons win over the phase.
fn pod_status(obj: &Value) -> (String, Health) {
    if obj
        .get("metadata")
        .and_then(|m| m.get("deletionTimestamp"))
        .is_some()
    {
        return ("Terminating".into(), Health::Warning);
    }
    let status = obj.get("status").cloned().unwrap_or(Value::Null);
    for key in ["initContainerStatuses", "containerStatuses"] {
        for cs in status
            .get(key)
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            if let Some(w) = get_path(cs, &["state", "waiting", "reason"]).and_then(scalar) {
                let health = match w.as_str() {
                    "ContainerCreating" | "PodInitializing" => Health::Warning,
                    _ => Health::Error,
                };
                return (w, health);
            }
        }
    }
    let phase = str_at(&status, "phase").unwrap_or_else(|| "Unknown".into());
    let health = match phase.as_str() {
        "Running" => Health::Ok,
        "Succeeded" => Health::Done,
        "Pending" => Health::Warning,
        "Failed" => Health::Error,
        _ => Health::Muted,
    };
    (phase, health)
}

// ---- events -------------------------------------------------------------

/// Parses a `v1/EventList`, newest first.
pub fn parse_events(list: &Value) -> Vec<EventRow> {
    let mut rows: Vec<EventRow> = list
        .get("items")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .map(|e| {
                    let kind = str_at(e, "type").unwrap_or_else(|| "Normal".into());
                    let last_seen = str_at(e, "lastTimestamp")
                        .or_else(|| str_at(e, "eventTime"))
                        .or_else(|| get_path(e, &["series", "lastObservedTime"]).and_then(scalar))
                        .or_else(|| {
                            get_path(e, &["metadata", "creationTimestamp"]).and_then(scalar)
                        })
                        .and_then(|t| parse_time(&t));
                    let source = get_path(e, &["source", "component"])
                        .and_then(scalar)
                        .or_else(|| str_at(e, "reportingComponent"))
                        .unwrap_or_default();
                    EventRow {
                        health: if kind == "Warning" {
                            Health::Warning
                        } else {
                            Health::Muted
                        },
                        reason: str_at(e, "reason").unwrap_or_default(),
                        message: str_at(e, "message")
                            .or_else(|| str_at(e, "note"))
                            .unwrap_or_default(),
                        count: e
                            .get("count")
                            .and_then(Value::as_i64)
                            .or_else(|| get_path(e, &["series", "count"]).and_then(Value::as_i64))
                            .unwrap_or(1),
                        kind,
                        last_seen,
                        source,
                    }
                })
                .collect()
        })
        .unwrap_or_default();
    rows.sort_by_key(|r| std::cmp::Reverse(r.last_seen));
    rows
}

#[cfg(test)]
mod tests;
