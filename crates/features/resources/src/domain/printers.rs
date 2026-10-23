//! kubectl/Freelens-like column sets and row builders for every built-in kind,
//! plus a generic fallback for custom resources. Pure: input is raw JSON.

use natikube_core::{ResourceKind, parse_cpu_cores, parse_memory_bytes};
use serde_json::Value;

use super::json::{
    array_at, at, bool_at, i64_at, keys_at, labels_text, str_at, time_at, true_conditions,
};
use super::object::{Capacity, Cell, ColumnKind, ColumnSpec, ObjectRow, OwnerRef};
use super::pod::PodRow;
use super::pod_json::{controller, pod_facts_from_value, pod_object_row};
use super::status::StatusTone;

/// An `additionalPrinterColumns` entry of a CRD.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrinterColumn {
    pub name: String,
    pub json_path: String,
    pub type_: String,
}

/// Row-specific values a kind printer produces.
struct Parts {
    cells: Vec<Cell>,
    health: StatusTone,
    capacity: Option<Capacity>,
    owner: Option<OwnerRef>,
    updated_at: Option<i64>,
    note: Option<String>,
}

impl Parts {
    fn new(cells: Vec<Cell>) -> Self {
        Self {
            cells,
            health: StatusTone::Ok,
            capacity: None,
            owner: None,
            updated_at: None,
            note: None,
        }
    }

    fn health(mut self, health: StatusTone) -> Self {
        self.health = health;
        self
    }
}

type BuildFn = fn(&Value) -> Parts;

#[derive(Clone)]
enum Imp {
    Builtin(BuildFn),
    Custom(Vec<String>),
}

/// Columns of a kind and the function turning raw objects into rows.
#[derive(Clone)]
pub struct Printer {
    columns: Vec<ColumnSpec>,
    imp: Imp,
}

impl Printer {
    pub fn columns(&self) -> &[ColumnSpec] {
        &self.columns
    }

    /// Builds the row for a raw object (`metadata` is required, the rest is
    /// read leniently so missing fields never panic).
    pub fn row(&self, object: &Value) -> ObjectRow {
        let parts = match &self.imp {
            Imp::Builtin(build) => build(object),
            Imp::Custom(paths) => Parts::new(
                paths
                    .iter()
                    .map(|p| Cell::text(json_path_text(object, p)))
                    .collect(),
            ),
        };
        let mut row = ObjectRow::new(
            str_at(object, "metadata.namespace").map(str::to_string),
            str_at(object, "metadata.name").unwrap_or_default(),
        );
        row.created_at = time_at(object, "metadata.creationTimestamp");
        row.cells = parts.cells;
        row.health = parts.health;
        row.capacity = parts.capacity;
        row.updated_at = parts.updated_at;
        row.note = parts.note.map(String::into_boxed_str);
        row.owner = parts.owner.or_else(|| controller(object));
        row
    }
}

// ---- column helpers -------------------------------------------------------

fn name_col() -> ColumnSpec {
    ColumnSpec::new("name", "Name", ColumnKind::Name, 320.0)
}

fn ns_col() -> ColumnSpec {
    ColumnSpec::new("namespace", "Namespace", ColumnKind::Namespace, 160.0)
}

fn age_col() -> ColumnSpec {
    ColumnSpec::new("age", "Age", ColumnKind::Age, 80.0).right()
}

fn key_of(title: &str) -> String {
    title.to_ascii_lowercase().replace(' ', "_")
}

fn txt(title: &str, ix: usize, width: f32) -> ColumnSpec {
    ColumnSpec::new(key_of(title), title, ColumnKind::Cell(ix), width)
}

fn num(title: &str, ix: usize, width: f32) -> ColumnSpec {
    txt(title, ix, width).right()
}

fn special(title: &str, kind: ColumnKind, width: f32) -> ColumnSpec {
    ColumnSpec::new(key_of(title), title, kind, width)
}

// ---- value helpers --------------------------------------------------------

fn int(value: &Value, path: &str) -> i64 {
    i64_at(value, path).unwrap_or(0)
}

fn count_cell(n: i64) -> Cell {
    Cell::number(n as f64, n.to_string())
}

fn string_cell(value: &Value, path: &str) -> Cell {
    Cell::text(str_at(value, path).unwrap_or("-"))
}

fn dash(text: String) -> String {
    if text.is_empty() { "-".into() } else { text }
}

/// Renders an int-or-string field.
fn int_or_string(value: &Value, path: &str) -> String {
    match at(value, path) {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Number(n)) => n.to_string(),
        _ => "-".into(),
    }
}

fn ratio(ready: i64, total: i64) -> (Cell, StatusTone) {
    let tone = if total == 0 {
        StatusTone::Done
    } else if ready >= total {
        StatusTone::Ok
    } else {
        StatusTone::Warning
    };
    (Cell::text(format!("{ready}/{total}")), tone)
}

fn port_text(port: &Value) -> String {
    let number = int(port, "port");
    let protocol = str_at(port, "protocol").unwrap_or("TCP");
    match i64_at(port, "nodePort").filter(|n| *n != 0) {
        Some(node) => format!("{number}:{node}/{protocol}"),
        None => format!("{number}/{protocol}"),
    }
}

fn limit_list(items: Vec<String>, max: usize) -> String {
    if items.len() <= max {
        return dash(items.join(", "));
    }
    format!("{}, +{}", items[..max].join(", "), items.len() - max)
}

fn phase_tone(phase: &str) -> StatusTone {
    match phase {
        "Bound" | "Active" | "Available" => StatusTone::Ok,
        "Released" | "Terminating" | "Pending" => StatusTone::Warning,
        "Failed" | "Lost" => StatusTone::Error,
        _ => StatusTone::Done,
    }
}

// ---- built-in kinds -------------------------------------------------------

fn pod_columns() -> Vec<ColumnSpec> {
    vec![
        name_col(),
        special("Warning", ColumnKind::Warning, 36.0).unsortable(),
        ns_col(),
        txt("Containers", 0, 100.0),
        special("CPU", ColumnKind::Cpu, 80.0).right(),
        special("Memory", ColumnKind::Memory, 90.0).right(),
        num("Restarts", 1, 80.0),
        special("Controlled By", ColumnKind::Owner, 140.0),
        age_col(),
        txt("Status", 2, 150.0),
    ]
}

fn pod_parts(pod: &Value) -> Parts {
    let row = PodRow::from_facts(pod_facts_from_value(pod));
    let converted = pod_object_row(&row);
    Parts {
        cells: converted.cells,
        health: converted.health,
        capacity: None,
        owner: None,
        updated_at: None,
        note: None,
    }
}

fn deployment_parts(o: &Value) -> Parts {
    let total = int(o, "spec.replicas");
    let ready = int(o, "status.readyReplicas");
    let (pods, tone) = ratio(ready, total);
    Parts::new(vec![
        pods,
        count_cell(total),
        Cell::text(dash(true_conditions(o, "status.conditions"))),
    ])
    .health(tone)
}

fn daemon_set_parts(o: &Value) -> Parts {
    let desired = int(o, "status.desiredNumberScheduled");
    let ready = int(o, "status.numberReady");
    let (pods, tone) = ratio(ready, desired);
    Parts::new(vec![
        pods,
        Cell::text(labels_text(o, "spec.template.spec.nodeSelector")),
    ])
    .health(tone)
}

fn stateful_set_parts(o: &Value) -> Parts {
    let total = int(o, "spec.replicas");
    let ready = int(o, "status.readyReplicas");
    let (pods, tone) = ratio(ready, total);
    Parts::new(vec![pods, count_cell(total)]).health(tone)
}

fn replica_parts(o: &Value) -> Parts {
    let desired = int(o, "spec.replicas");
    let ready = int(o, "status.readyReplicas");
    let (_, tone) = ratio(ready, desired);
    Parts::new(vec![
        count_cell(desired),
        count_cell(int(o, "status.replicas")),
        count_cell(ready),
    ])
    .health(tone)
}

fn job_parts(o: &Value) -> Parts {
    let completions = i64_at(o, "spec.completions").unwrap_or(1);
    let succeeded = int(o, "status.succeeded");
    let conditions = true_conditions(o, "status.conditions");
    let health = if conditions.split(' ').any(|c| c == "Failed") {
        StatusTone::Error
    } else if conditions.split(' ').any(|c| c == "Complete") {
        StatusTone::Done
    } else {
        StatusTone::Ok
    };
    Parts::new(vec![
        Cell::text(format!("{succeeded}/{completions}")),
        Cell::text(dash(conditions)),
    ])
    .health(health)
}

fn cron_job_parts(o: &Value) -> Parts {
    let suspended = bool_at(o, "spec.suspend").unwrap_or(false);
    let mut parts = Parts::new(vec![
        string_cell(o, "spec.schedule"),
        Cell::text(suspended.to_string()),
        count_cell(array_at(o, "status.active").len() as i64),
    ])
    .health(if suspended {
        StatusTone::Done
    } else {
        StatusTone::Ok
    });
    parts.updated_at = time_at(o, "status.lastScheduleTime");
    parts
}

fn config_map_parts(o: &Value) -> Parts {
    let mut keys = keys_at(o, "data");
    keys.extend(keys_at(o, "binaryData"));
    keys.sort();
    keys.dedup();
    Parts::new(vec![Cell::text(dash(keys.join(", ")))])
}

/// Only key names are read: Secret values never enter a row.
fn secret_parts(o: &Value) -> Parts {
    let keys = keys_at(o, "data").len() as i64;
    Parts::new(vec![string_cell(o, "type"), count_cell(keys)])
}

fn hpa_parts(o: &Value) -> Parts {
    let reference = format!(
        "{}/{}",
        str_at(o, "spec.scaleTargetRef.kind").unwrap_or("-"),
        str_at(o, "spec.scaleTargetRef.name").unwrap_or("-")
    );
    Parts::new(vec![
        Cell::text(reference),
        count_cell(i64_at(o, "spec.minReplicas").unwrap_or(1)),
        count_cell(int(o, "spec.maxReplicas")),
        count_cell(int(o, "status.currentReplicas")),
    ])
}

fn pdb_parts(o: &Value) -> Parts {
    let allowed = int(o, "status.disruptionsAllowed");
    Parts::new(vec![
        Cell::text(int_or_string(o, "spec.minAvailable")),
        Cell::text(int_or_string(o, "spec.maxUnavailable")),
        count_cell(allowed),
    ])
    .health(if allowed == 0 {
        StatusTone::Warning
    } else {
        StatusTone::Ok
    })
}

fn priority_class_parts(o: &Value) -> Parts {
    Parts::new(vec![
        count_cell(int(o, "value")),
        Cell::text(bool_at(o, "globalDefault").unwrap_or(false).to_string()),
    ])
}

fn runtime_class_parts(o: &Value) -> Parts {
    Parts::new(vec![string_cell(o, "handler")])
}

fn lease_parts(o: &Value) -> Parts {
    Parts::new(vec![string_cell(o, "spec.holderIdentity")])
}

fn webhook_parts(o: &Value) -> Parts {
    Parts::new(vec![count_cell(array_at(o, "webhooks").len() as i64)])
}

fn empty_parts(_: &Value) -> Parts {
    Parts::new(Vec::new())
}

fn service_parts(o: &Value) -> Parts {
    let kind = str_at(o, "spec.type").unwrap_or("ClusterIP");
    let ports: Vec<String> = array_at(o, "spec.ports").iter().map(port_text).collect();
    let mut external: Vec<String> = array_at(o, "status.loadBalancer.ingress")
        .iter()
        .filter_map(|i| str_at(i, "ip").or_else(|| str_at(i, "hostname")))
        .map(str::to_string)
        .collect();
    let has_ingress = !external.is_empty();
    external.extend(
        array_at(o, "spec.externalIPs")
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_string),
    );
    let pending = kind == "LoadBalancer" && !has_ingress;
    Parts::new(vec![
        Cell::text(kind),
        Cell::text(match str_at(o, "spec.clusterIP") {
            Some("") | None => "-",
            Some(ip) => ip,
        }),
        Cell::text(dash(ports.join(", "))),
        Cell::text(dash(external.join(", "))),
        Cell::text(labels_text(o, "spec.selector")),
        if pending {
            Cell::toned("Pending", StatusTone::Warning)
        } else {
            Cell::toned("Active", StatusTone::Ok)
        },
    ])
    .health(if pending {
        StatusTone::Warning
    } else {
        StatusTone::Ok
    })
}

fn endpoints_parts(o: &Value) -> Parts {
    let mut items = Vec::new();
    for subset in array_at(o, "subsets") {
        let ports: Vec<i64> = array_at(subset, "ports")
            .iter()
            .map(|p| int(p, "port"))
            .collect();
        for address in array_at(subset, "addresses") {
            let ip = str_at(address, "ip").unwrap_or("?");
            if ports.is_empty() {
                items.push(ip.to_string());
            }
            items.extend(ports.iter().map(|p| format!("{ip}:{p}")));
        }
    }
    Parts::new(vec![Cell::text(limit_list(items, 3))])
}

fn endpoint_slice_parts(o: &Value) -> Parts {
    let ports: Vec<String> = array_at(o, "ports")
        .iter()
        .filter_map(|p| at(p, "port").and_then(Value::as_i64))
        .map(|p| p.to_string())
        .collect();
    let addresses: Vec<String> = array_at(o, "endpoints")
        .iter()
        .flat_map(|e| array_at(e, "addresses"))
        .filter_map(Value::as_str)
        .map(str::to_string)
        .collect();
    Parts::new(vec![
        string_cell(o, "addressType"),
        Cell::text(dash(ports.join(","))),
        Cell::text(limit_list(addresses, 3)),
    ])
}

fn ingress_parts(o: &Value) -> Parts {
    let balancers: Vec<String> = array_at(o, "status.loadBalancer.ingress")
        .iter()
        .filter_map(|i| str_at(i, "ip").or_else(|| str_at(i, "hostname")))
        .map(str::to_string)
        .collect();
    let hosts: Vec<String> = array_at(o, "spec.rules")
        .iter()
        .map(|r| str_at(r, "host").unwrap_or("*").to_string())
        .collect();
    Parts::new(vec![
        Cell::text(dash(balancers.join(", "))),
        Cell::text(limit_list(hosts, 3)),
    ])
}

fn ingress_class_parts(o: &Value) -> Parts {
    Parts::new(vec![
        string_cell(o, "spec.controller"),
        string_cell(o, "spec.parameters.apiGroup"),
        string_cell(o, "spec.parameters.scope"),
    ])
}

fn network_policy_parts(o: &Value) -> Parts {
    let types: Vec<&str> = array_at(o, "spec.policyTypes")
        .iter()
        .filter_map(Value::as_str)
        .collect();
    Parts::new(vec![Cell::text(dash(types.join(", ")))])
}

fn pvc_parts(o: &Value) -> Parts {
    let phase = str_at(o, "status.phase").unwrap_or("Pending");
    let size = str_at(o, "status.capacity.storage")
        .or_else(|| str_at(o, "spec.resources.requests.storage"))
        .unwrap_or("-");
    Parts::new(vec![
        string_cell(o, "spec.storageClassName"),
        Cell::text(size),
        Cell::toned(phase, phase_tone(phase)),
    ])
    .health(phase_tone(phase))
}

fn pv_parts(o: &Value) -> Parts {
    let phase = str_at(o, "status.phase").unwrap_or("Pending");
    let claim = match (
        str_at(o, "spec.claimRef.namespace"),
        str_at(o, "spec.claimRef.name"),
    ) {
        (Some(ns), Some(name)) => format!("{ns}/{name}"),
        _ => "-".into(),
    };
    Parts::new(vec![
        string_cell(o, "spec.storageClassName"),
        string_cell(o, "spec.capacity.storage"),
        Cell::text(claim),
        Cell::toned(phase, phase_tone(phase)),
    ])
    .health(phase_tone(phase))
}

fn storage_class_parts(o: &Value) -> Parts {
    let default = o
        .pointer("/metadata/annotations/storageclass.kubernetes.io~1is-default-class")
        .and_then(Value::as_str)
        == Some("true");
    Parts::new(vec![
        string_cell(o, "provisioner"),
        Cell::text(str_at(o, "reclaimPolicy").unwrap_or("Delete")),
        Cell::text(default.to_string()),
    ])
}

fn namespace_parts(o: &Value) -> Parts {
    let phase = str_at(o, "status.phase").unwrap_or("Active");
    Parts::new(vec![
        Cell::text(labels_text(o, "metadata.labels")),
        Cell::toned(phase, phase_tone(phase)),
    ])
    .health(phase_tone(phase))
}

fn node_parts(o: &Value) -> Parts {
    let mut roles: Vec<String> = at(o, "metadata.labels")
        .and_then(Value::as_object)
        .map(|labels| {
            labels
                .keys()
                .filter_map(|k| k.strip_prefix("node-role.kubernetes.io/"))
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    roles.sort();
    let taints = array_at(o, "spec.taints").len() as i64;

    let conditions = array_at(o, "status.conditions");
    let ready = conditions
        .iter()
        .any(|c| str_at(c, "type") == Some("Ready") && str_at(c, "status") == Some("True"));
    let pressure: Vec<&str> = conditions
        .iter()
        .filter(|c| str_at(c, "type") != Some("Ready") && str_at(c, "status") == Some("True"))
        .filter_map(|c| str_at(c, "type"))
        .collect();
    let (text, health) = if !ready {
        ("NotReady".to_string(), StatusTone::Error)
    } else if pressure.is_empty() {
        ("Ready".to_string(), StatusTone::Ok)
    } else {
        (format!("Ready {}", pressure.join(" ")), StatusTone::Warning)
    };

    let mut parts = Parts::new(vec![
        count_cell(taints),
        Cell::text(dash(roles.join(", "))),
        string_cell(o, "status.nodeInfo.kubeletVersion"),
        Cell::toned(text, health),
    ])
    .health(health);
    let cpu = str_at(o, "status.allocatable.cpu").and_then(parse_cpu_cores);
    let memory = str_at(o, "status.allocatable.memory").and_then(parse_memory_bytes);
    if let (Some(cpu_cores), Some(memory_bytes)) = (cpu, memory) {
        parts.capacity = Some(Capacity {
            cpu_cores,
            memory_bytes,
        });
    }
    parts
}

fn event_parts(o: &Value) -> Parts {
    let kind = str_at(o, "type").unwrap_or("Normal");
    let warning = kind == "Warning";
    let source = str_at(o, "source.component")
        .or_else(|| str_at(o, "reportingComponent"))
        .or_else(|| str_at(o, "reportingController"))
        .unwrap_or("-");
    let count = i64_at(o, "count")
        .or_else(|| i64_at(o, "series.count"))
        .unwrap_or(1);
    let message = str_at(o, "message").unwrap_or_default().to_string();
    let mut parts = Parts::new(vec![
        if warning {
            Cell::toned(kind, StatusTone::Warning)
        } else {
            Cell::text(kind)
        },
        Cell::text(message.clone()),
        Cell::text(source),
        count_cell(count),
    ])
    .health(if warning {
        StatusTone::Warning
    } else {
        StatusTone::Ok
    });
    parts.owner = str_at(o, "involvedObject.kind").map(|k| OwnerRef {
        kind: k.into(),
        name: str_at(o, "involvedObject.name").unwrap_or_default().into(),
    });
    parts.updated_at = time_at(o, "lastTimestamp")
        .or_else(|| time_at(o, "series.lastObservedTime"))
        .or_else(|| time_at(o, "eventTime"))
        .or_else(|| time_at(o, "metadata.creationTimestamp"));
    parts.note = Some(message);
    parts
}

fn binding_parts(o: &Value) -> Parts {
    let subjects: Vec<String> = array_at(o, "subjects")
        .iter()
        .filter_map(|s| str_at(s, "name").map(str::to_string))
        .collect();
    Parts::new(vec![Cell::text(limit_list(subjects, 3))])
}

fn crd_parts(o: &Value) -> Parts {
    let versions = array_at(o, "spec.versions");
    let storage = versions
        .iter()
        .find(|v| bool_at(v, "storage") == Some(true))
        .or_else(|| versions.first())
        .and_then(|v| str_at(v, "name"))
        .unwrap_or("-");
    Parts::new(vec![
        string_cell(o, "spec.group"),
        Cell::text(storage),
        string_cell(o, "spec.scope"),
    ])
}

fn with_age(mut columns: Vec<ColumnSpec>) -> Vec<ColumnSpec> {
    columns.push(age_col());
    columns
}

fn named(namespaced: bool, extra: Vec<ColumnSpec>) -> Vec<ColumnSpec> {
    let mut columns = vec![name_col()];
    if namespaced {
        columns.push(ns_col());
    }
    columns.extend(extra);
    columns
}

/// The printer for a built-in kind. Unknown kinds get the Name/Namespace/Age
/// fallback.
pub fn printer_for(kind: &ResourceKind) -> Printer {
    let ns = kind.namespaced;
    let (columns, build): (Vec<ColumnSpec>, BuildFn) =
        match (kind.group.as_ref(), kind.plural.as_ref()) {
            ("", "pods") => (pod_columns(), pod_parts),
            ("apps", "deployments") => (
                {
                    let mut c = named(ns, vec![txt("Pods", 0, 80.0), num("Replicas", 1, 90.0)]);
                    c.push(age_col());
                    c.push(txt("Conditions", 2, 220.0));
                    c
                },
                deployment_parts,
            ),
            ("apps", "daemonsets") => (
                with_age(named(
                    ns,
                    vec![txt("Pods", 0, 80.0), txt("Node Selector", 1, 260.0)],
                )),
                daemon_set_parts,
            ),
            ("apps", "statefulsets") => (
                with_age(named(
                    ns,
                    vec![txt("Pods", 0, 80.0), num("Replicas", 1, 90.0)],
                )),
                stateful_set_parts,
            ),
            ("apps", "replicasets") | ("", "replicationcontrollers") => (
                with_age(named(
                    ns,
                    vec![
                        num("Desired", 0, 80.0),
                        num("Current", 1, 80.0),
                        num("Ready", 2, 80.0),
                    ],
                )),
                replica_parts,
            ),
            ("batch", "jobs") => (
                with_age(named(ns, vec![txt("Completions", 0, 110.0)]))
                    .into_iter()
                    .chain([txt("Conditions", 1, 160.0)])
                    .collect(),
                job_parts,
            ),
            ("batch", "cronjobs") => (
                with_age(named(
                    ns,
                    vec![
                        txt("Schedule", 0, 140.0),
                        txt("Suspend", 1, 80.0),
                        num("Active", 2, 80.0),
                        special("Last Schedule", ColumnKind::LastSeen, 110.0).right(),
                    ],
                )),
                cron_job_parts,
            ),
            ("", "configmaps") => (
                with_age(named(ns, vec![txt("Keys", 0, 420.0)])),
                config_map_parts,
            ),
            ("", "secrets") => (
                with_age(named(ns, vec![txt("Type", 0, 260.0), num("Keys", 1, 80.0)])),
                secret_parts,
            ),
            ("", "resourcequotas")
            | ("", "limitranges")
            | ("", "serviceaccounts")
            | ("rbac.authorization.k8s.io", "roles")
            | ("rbac.authorization.k8s.io", "clusterroles") => {
                (with_age(named(ns, Vec::new())), empty_parts)
            }
            ("autoscaling", "horizontalpodautoscalers") => (
                with_age(named(
                    ns,
                    vec![
                        txt("Reference", 0, 240.0),
                        num("Min Pods", 1, 80.0),
                        num("Max Pods", 2, 80.0),
                        num("Replicas", 3, 80.0),
                    ],
                )),
                hpa_parts,
            ),
            ("policy", "poddisruptionbudgets") => (
                with_age(named(
                    ns,
                    vec![
                        txt("Min Available", 0, 110.0),
                        txt("Max Unavailable", 1, 120.0),
                        num("Allowed Disruptions", 2, 150.0),
                    ],
                )),
                pdb_parts,
            ),
            ("scheduling.k8s.io", "priorityclasses") => (
                with_age(named(
                    ns,
                    vec![num("Value", 0, 100.0), txt("Global Default", 1, 120.0)],
                )),
                priority_class_parts,
            ),
            ("node.k8s.io", "runtimeclasses") => (
                with_age(named(ns, vec![txt("Handler", 0, 200.0)])),
                runtime_class_parts,
            ),
            ("coordination.k8s.io", "leases") => (
                with_age(named(ns, vec![txt("Holder", 0, 320.0)])),
                lease_parts,
            ),
            ("admissionregistration.k8s.io", _) => (
                with_age(named(ns, vec![num("Webhooks", 0, 100.0)])),
                webhook_parts,
            ),
            ("", "services") => (
                with_age_before_last(named(
                    ns,
                    vec![
                        txt("Type", 0, 110.0),
                        txt("Cluster IP", 1, 120.0),
                        txt("Ports", 2, 200.0),
                        txt("External IP", 3, 140.0),
                        txt("Selector", 4, 220.0),
                        txt("Status", 5, 90.0),
                    ],
                )),
                service_parts,
            ),
            ("", "endpoints") => (
                with_age(named(ns, vec![txt("Endpoints", 0, 420.0)])),
                endpoints_parts,
            ),
            ("discovery.k8s.io", "endpointslices") => (
                with_age(named(
                    ns,
                    vec![
                        txt("Address Type", 0, 110.0),
                        txt("Ports", 1, 100.0),
                        txt("Endpoints", 2, 320.0),
                    ],
                )),
                endpoint_slice_parts,
            ),
            ("networking.k8s.io", "ingresses") => (
                with_age(named(
                    ns,
                    vec![txt("Load Balancers", 0, 180.0), txt("Rules", 1, 320.0)],
                )),
                ingress_parts,
            ),
            ("networking.k8s.io", "ingressclasses") => (
                with_age(named(
                    ns,
                    vec![
                        txt("Controller", 0, 240.0),
                        txt("API Group", 1, 160.0),
                        txt("Scope", 2, 100.0),
                    ],
                )),
                ingress_class_parts,
            ),
            ("networking.k8s.io", "networkpolicies") => (
                with_age(named(ns, vec![txt("Policy Types", 0, 200.0)])),
                network_policy_parts,
            ),
            ("", "persistentvolumeclaims") => (
                with_age_before_last(named(
                    ns,
                    vec![
                        txt("Storage Class", 0, 160.0),
                        txt("Size", 1, 90.0),
                        txt("Status", 2, 100.0),
                    ],
                )),
                pvc_parts,
            ),
            ("", "persistentvolumes") => (
                with_age_before_last(named(
                    ns,
                    vec![
                        txt("Storage Class", 0, 160.0),
                        txt("Capacity", 1, 90.0),
                        txt("Claim", 2, 260.0),
                        txt("Status", 3, 100.0),
                    ],
                )),
                pv_parts,
            ),
            ("storage.k8s.io", "storageclasses") => (
                with_age(named(
                    ns,
                    vec![
                        txt("Provisioner", 0, 260.0),
                        txt("Reclaim Policy", 1, 120.0),
                        txt("Default", 2, 90.0),
                    ],
                )),
                storage_class_parts,
            ),
            ("", "namespaces") => (
                with_age_before_last(named(
                    ns,
                    vec![txt("Labels", 0, 420.0), txt("Status", 1, 100.0)],
                )),
                namespace_parts,
            ),
            ("", "nodes") => (
                vec![
                    name_col(),
                    special("CPU", ColumnKind::Cpu, 90.0).right(),
                    special("Memory", ColumnKind::Memory, 100.0).right(),
                    num("Taints", 0, 80.0),
                    txt("Roles", 1, 180.0),
                    txt("Version", 2, 140.0),
                    age_col(),
                    txt("Conditions", 3, 160.0),
                ],
                node_parts,
            ),
            ("", "events") => (
                vec![
                    txt("Type", 0, 90.0),
                    txt("Message", 1, 420.0),
                    ns_col(),
                    special("Involved Object", ColumnKind::Object, 260.0),
                    txt("Source", 2, 160.0),
                    num("Count", 3, 70.0),
                    age_col(),
                    special("Last Seen", ColumnKind::LastSeen, 90.0).right(),
                ],
                event_parts,
            ),
            ("rbac.authorization.k8s.io", "rolebindings")
            | ("rbac.authorization.k8s.io", "clusterrolebindings") => (
                with_age(named(ns, vec![txt("Bindings", 0, 420.0)])),
                binding_parts,
            ),
            ("apiextensions.k8s.io", "customresourcedefinitions") => (
                with_age_before_last(named(
                    ns,
                    vec![
                        txt("Group", 0, 240.0),
                        txt("Version", 1, 100.0),
                        txt("Scope", 2, 100.0),
                    ],
                )),
                crd_parts,
            ),
            _ => (with_age(named(ns, Vec::new())), empty_parts),
        };
    Printer {
        columns,
        imp: Imp::Builtin(build),
    }
}

// kubectl puts AGE before a trailing status-like column.
fn with_age_before_last(mut columns: Vec<ColumnSpec>) -> Vec<ColumnSpec> {
    let last = columns.pop();
    columns.push(age_col());
    columns.extend(last);
    columns
}

/// Printer for a CRD kind: Name, Namespace (if namespaced), Age and the
/// CRD's `additionalPrinterColumns` (except the ones that repeat Age).
pub fn printer_for_custom(kind: &ResourceKind, extra: &[PrinterColumn]) -> Printer {
    let mut columns = named(kind.namespaced, Vec::new());
    columns.push(age_col());
    let mut paths = Vec::new();
    for column in extra {
        if column.json_path == ".metadata.creationTimestamp" || column.name == "Age" {
            continue;
        }
        let ix = paths.len();
        let right = matches!(column.type_.as_str(), "integer" | "number");
        let mut spec = txt(&column.name, ix, 140.0);
        spec.align_right = right;
        columns.push(spec);
        paths.push(column.json_path.clone());
    }
    Printer {
        columns,
        imp: Imp::Custom(paths),
    }
}

/// Evaluates the simple JSONPath subset CRDs use for printer columns
/// (`.a.b`, `.a[0].b`). Anything fancier renders empty.
fn json_path_text(object: &Value, path: &str) -> String {
    let mut normalized = String::new();
    for ch in path.chars() {
        match ch {
            '[' => normalized.push('.'),
            ']' => {}
            c => normalized.push(c),
        }
    }
    if normalized.contains(['?', '*', '@', '\'', '"', '(']) {
        return String::new();
    }
    match at(object, &normalized) {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Number(n)) => n.to_string(),
        Some(Value::Bool(b)) => b.to_string(),
        Some(Value::Array(items)) => items
            .iter()
            .map(|v| match v {
                Value::String(s) => s.clone(),
                other => other.to_string(),
            })
            .collect::<Vec<_>>()
            .join(", "),
        Some(Value::Null) | None => String::new(),
        Some(other) => other.to_string(),
    }
}
