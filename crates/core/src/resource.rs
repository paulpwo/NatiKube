use std::borrow::Cow;
use std::fmt;

use serde::{Deserialize, Serialize};

use crate::cluster::ClusterId;

/// Describes a Kubernetes resource kind (what discovery calls an API resource).
/// Built-in kinds are `const`s in [`kinds`]; CRDs use [`ResourceKind::custom`].
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ResourceKind {
    /// API group; empty for the core group.
    pub group: Cow<'static, str>,
    pub version: Cow<'static, str>,
    pub kind: Cow<'static, str>,
    /// Lowercase plural used in URLs and kubectl, e.g. `pods`.
    pub plural: Cow<'static, str>,
    pub namespaced: bool,
}

impl ResourceKind {
    pub const fn builtin(
        group: &'static str,
        version: &'static str,
        kind: &'static str,
        plural: &'static str,
        namespaced: bool,
    ) -> Self {
        Self {
            group: Cow::Borrowed(group),
            version: Cow::Borrowed(version),
            kind: Cow::Borrowed(kind),
            plural: Cow::Borrowed(plural),
            namespaced,
        }
    }

    pub fn custom(
        group: impl Into<String>,
        version: impl Into<String>,
        kind: impl Into<String>,
        plural: impl Into<String>,
        namespaced: bool,
    ) -> Self {
        Self {
            group: Cow::Owned(group.into()),
            version: Cow::Owned(version.into()),
            kind: Cow::Owned(kind.into()),
            plural: Cow::Owned(plural.into()),
            namespaced,
        }
    }

    pub fn is_core(&self) -> bool {
        self.group.is_empty()
    }

    /// `apiVersion` value, e.g. `v1` or `apps/v1`.
    pub fn api_version(&self) -> String {
        if self.group.is_empty() {
            self.version.to_string()
        } else {
            format!("{}/{}", self.group, self.version)
        }
    }
}

/// Points at one object in one cluster.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ResourceRef {
    pub cluster: ClusterId,
    pub kind: ResourceKind,
    /// `None` for cluster-scoped kinds.
    pub namespace: Option<String>,
    pub name: String,
}

impl ResourceRef {
    pub fn new(
        cluster: ClusterId,
        kind: ResourceKind,
        namespace: Option<String>,
        name: impl Into<String>,
    ) -> Self {
        Self {
            cluster,
            kind,
            namespace,
            name: name.into(),
        }
    }
}

impl fmt::Display for ResourceRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.namespace {
            Some(ns) => write!(f, "{} {ns}/{}", self.kind.kind, self.name),
            None => write!(f, "{} {}", self.kind.kind, self.name),
        }
    }
}

/// Descriptors of the built-in kinds the browser knows about.
pub mod kinds {
    use super::ResourceKind;

    macro_rules! kind {
        ($name:ident, $group:expr, $version:expr, $kind:expr, $plural:expr, $ns:expr) => {
            pub const $name: ResourceKind =
                ResourceKind::builtin($group, $version, $kind, $plural, $ns);
        };
    }

    kind!(NODE, "", "v1", "Node", "nodes", false);
    kind!(NAMESPACE, "", "v1", "Namespace", "namespaces", false);
    kind!(EVENT, "", "v1", "Event", "events", true);

    kind!(POD, "", "v1", "Pod", "pods", true);
    kind!(DEPLOYMENT, "apps", "v1", "Deployment", "deployments", true);
    kind!(DAEMON_SET, "apps", "v1", "DaemonSet", "daemonsets", true);
    kind!(
        STATEFUL_SET,
        "apps",
        "v1",
        "StatefulSet",
        "statefulsets",
        true
    );
    kind!(REPLICA_SET, "apps", "v1", "ReplicaSet", "replicasets", true);
    kind!(
        REPLICATION_CONTROLLER,
        "",
        "v1",
        "ReplicationController",
        "replicationcontrollers",
        true
    );
    kind!(JOB, "batch", "v1", "Job", "jobs", true);
    kind!(CRON_JOB, "batch", "v1", "CronJob", "cronjobs", true);

    kind!(CONFIG_MAP, "", "v1", "ConfigMap", "configmaps", true);
    kind!(SECRET, "", "v1", "Secret", "secrets", true);
    kind!(
        RESOURCE_QUOTA,
        "",
        "v1",
        "ResourceQuota",
        "resourcequotas",
        true
    );
    kind!(LIMIT_RANGE, "", "v1", "LimitRange", "limitranges", true);
    kind!(
        HORIZONTAL_POD_AUTOSCALER,
        "autoscaling",
        "v2",
        "HorizontalPodAutoscaler",
        "horizontalpodautoscalers",
        true
    );
    kind!(
        POD_DISRUPTION_BUDGET,
        "policy",
        "v1",
        "PodDisruptionBudget",
        "poddisruptionbudgets",
        true
    );
    kind!(
        PRIORITY_CLASS,
        "scheduling.k8s.io",
        "v1",
        "PriorityClass",
        "priorityclasses",
        false
    );
    kind!(
        RUNTIME_CLASS,
        "node.k8s.io",
        "v1",
        "RuntimeClass",
        "runtimeclasses",
        false
    );
    kind!(LEASE, "coordination.k8s.io", "v1", "Lease", "leases", true);
    kind!(
        MUTATING_WEBHOOK_CONFIGURATION,
        "admissionregistration.k8s.io",
        "v1",
        "MutatingWebhookConfiguration",
        "mutatingwebhookconfigurations",
        false
    );
    kind!(
        VALIDATING_WEBHOOK_CONFIGURATION,
        "admissionregistration.k8s.io",
        "v1",
        "ValidatingWebhookConfiguration",
        "validatingwebhookconfigurations",
        false
    );

    kind!(SERVICE, "", "v1", "Service", "services", true);
    kind!(ENDPOINTS, "", "v1", "Endpoints", "endpoints", true);
    kind!(
        ENDPOINT_SLICE,
        "discovery.k8s.io",
        "v1",
        "EndpointSlice",
        "endpointslices",
        true
    );
    kind!(
        INGRESS,
        "networking.k8s.io",
        "v1",
        "Ingress",
        "ingresses",
        true
    );
    kind!(
        INGRESS_CLASS,
        "networking.k8s.io",
        "v1",
        "IngressClass",
        "ingressclasses",
        false
    );
    kind!(
        NETWORK_POLICY,
        "networking.k8s.io",
        "v1",
        "NetworkPolicy",
        "networkpolicies",
        true
    );

    kind!(
        PERSISTENT_VOLUME_CLAIM,
        "",
        "v1",
        "PersistentVolumeClaim",
        "persistentvolumeclaims",
        true
    );
    kind!(
        PERSISTENT_VOLUME,
        "",
        "v1",
        "PersistentVolume",
        "persistentvolumes",
        false
    );
    kind!(
        STORAGE_CLASS,
        "storage.k8s.io",
        "v1",
        "StorageClass",
        "storageclasses",
        false
    );

    kind!(
        SERVICE_ACCOUNT,
        "",
        "v1",
        "ServiceAccount",
        "serviceaccounts",
        true
    );
    kind!(
        ROLE,
        "rbac.authorization.k8s.io",
        "v1",
        "Role",
        "roles",
        true
    );
    kind!(
        ROLE_BINDING,
        "rbac.authorization.k8s.io",
        "v1",
        "RoleBinding",
        "rolebindings",
        true
    );
    kind!(
        CLUSTER_ROLE,
        "rbac.authorization.k8s.io",
        "v1",
        "ClusterRole",
        "clusterroles",
        false
    );
    kind!(
        CLUSTER_ROLE_BINDING,
        "rbac.authorization.k8s.io",
        "v1",
        "ClusterRoleBinding",
        "clusterrolebindings",
        false
    );

    kind!(
        CUSTOM_RESOURCE_DEFINITION,
        "apiextensions.k8s.io",
        "v1",
        "CustomResourceDefinition",
        "customresourcedefinitions",
        false
    );

    /// Looks a built-in kind up by plural (`pods`), kind (`Pod`) or lowercase
    /// singular (`pod`), ignoring case.
    pub fn find(name: &str) -> Option<ResourceKind> {
        let name = name.to_ascii_lowercase();
        all_builtin()
            .into_iter()
            .find(|k| k.plural == name || k.kind.to_ascii_lowercase() == name)
    }

    /// Every built-in kind, in sidebar order.
    pub fn all_builtin() -> Vec<ResourceKind> {
        vec![
            NODE,
            NAMESPACE,
            EVENT,
            POD,
            DEPLOYMENT,
            DAEMON_SET,
            STATEFUL_SET,
            REPLICA_SET,
            REPLICATION_CONTROLLER,
            JOB,
            CRON_JOB,
            CONFIG_MAP,
            SECRET,
            RESOURCE_QUOTA,
            LIMIT_RANGE,
            HORIZONTAL_POD_AUTOSCALER,
            POD_DISRUPTION_BUDGET,
            PRIORITY_CLASS,
            RUNTIME_CLASS,
            LEASE,
            MUTATING_WEBHOOK_CONFIGURATION,
            VALIDATING_WEBHOOK_CONFIGURATION,
            SERVICE,
            ENDPOINTS,
            ENDPOINT_SLICE,
            INGRESS,
            INGRESS_CLASS,
            NETWORK_POLICY,
            PERSISTENT_VOLUME_CLAIM,
            PERSISTENT_VOLUME,
            STORAGE_CLASS,
            SERVICE_ACCOUNT,
            ROLE,
            ROLE_BINDING,
            CLUSTER_ROLE,
            CLUSTER_ROLE_BINDING,
            CUSTOM_RESOURCE_DEFINITION,
        ]
    }
}
