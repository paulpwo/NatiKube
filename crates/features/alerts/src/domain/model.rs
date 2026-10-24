use natikube_core::{ClusterId, RuleKind};

/// Unix time in seconds. Plain integers keep the domain deterministic in tests.
pub type Timestamp = i64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Severity {
    Info,
    Warning,
    Critical,
}

impl Severity {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Info => "info",
            Self::Warning => "warning",
            Self::Critical => "critical",
        }
    }

    pub fn from_str_key(key: &str) -> Option<Self> {
        match key {
            "info" => Some(Self::Info),
            "warning" => Some(Self::Warning),
            "critical" => Some(Self::Critical),
            _ => None,
        }
    }
}

/// Severity each rule reports with.
pub fn rule_severity(rule: RuleKind) -> Severity {
    match rule {
        RuleKind::NewPod => Severity::Info,
        RuleKind::PodFailing | RuleKind::NodeNotReady | RuleKind::ClusterUnreachable => {
            Severity::Critical
        }
        RuleKind::PodRestarts
        | RuleKind::PodPendingTooLong
        | RuleKind::DeploymentUnavailable
        | RuleKind::WarningEvents => Severity::Warning,
    }
}

/// One fired alert, before it is stored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Alert {
    /// Unique per alert (not stable across restarts of the evaluator).
    pub id: String,
    pub cluster: ClusterId,
    pub rule: RuleKind,
    pub severity: Severity,
    pub namespace: Option<String>,
    /// Kubernetes kind of the object (`Pod`, `Deployment`, `Node`, ...);
    /// resolve with `natikube_core::kinds::find`.
    pub object_kind: Option<String>,
    pub object_name: Option<String>,
    pub message: String,
    pub timestamp: Timestamp,
    /// Alerts with the same key inside the cooldown window are collapsed.
    pub dedup_key: String,
}

/// An alert as read back from the history.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredAlert {
    /// Row id in the history (stable, used to mark read).
    pub row_id: i64,
    pub alert: Alert,
    pub read: bool,
}
