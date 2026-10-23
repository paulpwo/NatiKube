use serde::{Deserialize, Serialize};

use crate::resource::{ResourceKind, kinds};

/// A destination in the main content area.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ViewId {
    ClusterOverview,
    WorkloadsOverview,
    /// A resource table for a built-in kind or a CRD.
    Resource(ResourceKind),
}

impl ViewId {
    /// Label used for the sidebar entry, the top tab and the table title.
    pub fn title(&self) -> String {
        match self {
            Self::ClusterOverview => "Cluster".to_string(),
            Self::WorkloadsOverview => "Overview".to_string(),
            Self::Resource(kind) => kind_title(kind),
        }
    }
}

/// Plural, human readable name of a kind (`DaemonSet` becomes `Daemon Sets`).
pub fn kind_title(kind: &ResourceKind) -> String {
    let known: Option<&str> = match kind.plural.as_ref() {
        "ingresses" => Some("Ingresses"),
        "ingressclasses" => Some("Ingress Classes"),
        "endpoints" => Some("Endpoints"),
        "networkpolicies" => Some("Network Policies"),
        "priorityclasses" => Some("Priority Classes"),
        "storageclasses" => Some("Storage Classes"),
        "runtimeclasses" => Some("Runtime Classes"),
        _ => None,
    };
    if let Some(title) = known {
        return title.to_string();
    }
    let mut out = String::new();
    let chars: Vec<char> = kind.kind.chars().collect();
    for (i, c) in chars.iter().enumerate() {
        if i > 0 && c.is_uppercase() && !chars[i - 1].is_uppercase() {
            out.push(' ');
        }
        out.push(*c);
    }
    format!("{out}s")
}

/// One clickable entry inside a sidebar section.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SidebarEntry {
    /// Dynamic for Custom Resources entries (discovered CRDs).
    pub label: String,
    pub view: ViewId,
}

/// A sidebar section. Sections with more than one entry are groups whose
/// entries are also shown as top tabs; single-entry sections are plain links.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SidebarSection {
    pub label: &'static str,
    pub entries: Vec<SidebarEntry>,
    /// Entries are discovered at runtime (CRDs grouped by API group).
    pub dynamic: bool,
}

impl SidebarSection {
    pub fn is_group(&self) -> bool {
        self.entries.len() > 1
    }

    /// The view opened when the section is clicked.
    pub fn landing(&self) -> Option<ViewId> {
        self.entries.first().map(|e| e.view.clone())
    }

    pub fn contains(&self, view: &ViewId) -> bool {
        self.entries.iter().any(|e| &e.view == view)
    }
}

fn entry(label: &'static str, kind: ResourceKind) -> SidebarEntry {
    SidebarEntry {
        label: label.to_string(),
        view: ViewId::Resource(kind),
    }
}

fn section(label: &'static str, entries: Vec<SidebarEntry>) -> SidebarSection {
    SidebarSection {
        label,
        entries,
        dynamic: false,
    }
}

/// The sidebar in Freelens order.
pub fn sidebar() -> Vec<SidebarSection> {
    vec![
        section(
            "Cluster",
            vec![SidebarEntry {
                label: "Cluster".to_string(),
                view: ViewId::ClusterOverview,
            }],
        ),
        section("Nodes", vec![entry("Nodes", kinds::NODE)]),
        section(
            "Workloads",
            vec![
                SidebarEntry {
                    label: "Overview".to_string(),
                    view: ViewId::WorkloadsOverview,
                },
                entry("Pods", kinds::POD),
                entry("Deployments", kinds::DEPLOYMENT),
                entry("Daemon Sets", kinds::DAEMON_SET),
                entry("Stateful Sets", kinds::STATEFUL_SET),
                entry("Replica Sets", kinds::REPLICA_SET),
                entry("Replication Controllers", kinds::REPLICATION_CONTROLLER),
                entry("Jobs", kinds::JOB),
                entry("Cron Jobs", kinds::CRON_JOB),
            ],
        ),
        section(
            "Config",
            vec![
                entry("Config Maps", kinds::CONFIG_MAP),
                entry("Secrets", kinds::SECRET),
                entry("Resource Quotas", kinds::RESOURCE_QUOTA),
                entry("Limit Ranges", kinds::LIMIT_RANGE),
                entry(
                    "Horizontal Pod Autoscalers",
                    kinds::HORIZONTAL_POD_AUTOSCALER,
                ),
                entry("Pod Disruption Budgets", kinds::POD_DISRUPTION_BUDGET),
                entry("Priority Classes", kinds::PRIORITY_CLASS),
                entry("Runtime Classes", kinds::RUNTIME_CLASS),
                entry("Leases", kinds::LEASE),
                entry(
                    "Mutating Webhook Configurations",
                    kinds::MUTATING_WEBHOOK_CONFIGURATION,
                ),
                entry(
                    "Validating Webhook Configurations",
                    kinds::VALIDATING_WEBHOOK_CONFIGURATION,
                ),
            ],
        ),
        section(
            "Network",
            vec![
                entry("Services", kinds::SERVICE),
                entry("Endpoints", kinds::ENDPOINTS),
                entry("Endpoint Slices", kinds::ENDPOINT_SLICE),
                entry("Ingresses", kinds::INGRESS),
                entry("Ingress Classes", kinds::INGRESS_CLASS),
                entry("Network Policies", kinds::NETWORK_POLICY),
            ],
        ),
        section(
            "Storage",
            vec![
                entry("Persistent Volume Claims", kinds::PERSISTENT_VOLUME_CLAIM),
                entry("Persistent Volumes", kinds::PERSISTENT_VOLUME),
                entry("Storage Classes", kinds::STORAGE_CLASS),
            ],
        ),
        section("Namespaces", vec![entry("Namespaces", kinds::NAMESPACE)]),
        section("Events", vec![entry("Events", kinds::EVENT)]),
        section(
            "Access Control",
            vec![
                entry("Service Accounts", kinds::SERVICE_ACCOUNT),
                entry("Roles", kinds::ROLE),
                entry("Role Bindings", kinds::ROLE_BINDING),
                entry("Cluster Roles", kinds::CLUSTER_ROLE),
                entry("Cluster Role Bindings", kinds::CLUSTER_ROLE_BINDING),
            ],
        ),
        SidebarSection {
            label: "Custom Resources",
            // Definitions is always reachable; discovered CRD kinds follow.
            entries: vec![entry("Definitions", kinds::CUSTOM_RESOURCE_DEFINITION)],
            dynamic: true,
        },
    ]
}

/// Index of the section that owns `view`. Resource views that no static
/// entry claims (CRDs) belong to the dynamic Custom Resources section.
pub fn section_index(sections: &[SidebarSection], view: &ViewId) -> Option<usize> {
    sections
        .iter()
        .position(|s| s.contains(view))
        .or_else(|| match view {
            ViewId::Resource(_) => sections.iter().position(|s| s.dynamic),
            _ => None,
        })
}
