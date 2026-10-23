use std::fmt;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Stable identity of a cluster entry in the catalog.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ClusterId(String);

impl ClusterId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Human-friendly label: the context name without the `path#` prefix.
    pub fn context_name(&self) -> &str {
        self.0.rsplit_once('#').map_or(self.0.as_str(), |(_, c)| c)
    }
}

impl fmt::Display for ClusterId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// One kubeconfig context as listed in the cluster rail.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClusterInfo {
    pub id: ClusterId,
    /// Kubeconfig context name, used to build the client.
    pub context: String,
    /// Display name (defaults to the context name).
    pub name: String,
    /// Kubeconfig file the context comes from; `None` means the default
    /// resolution (`$KUBECONFIG` or `~/.kube/config`).
    pub kubeconfig: Option<PathBuf>,
    /// API server URL when known.
    pub server: Option<String>,
    /// Default namespace declared by the context.
    pub namespace: Option<String>,
    /// Other kubeconfig files declaring the identical context (same name,
    /// server and user) that discovery folded into this entry.
    #[serde(default)]
    pub also_in: Vec<PathBuf>,
    /// Other context names that point at the same server, user and namespace
    /// (for example the ARN twin of an EKS alias) and were folded into this
    /// entry by discovery.
    #[serde(default)]
    pub aliases: Vec<String>,
}

/// The name to display for a kubeconfig context: an EKS cluster ARN
/// (`arn:aws:eks:<region>:<account>:cluster/<name>`) shows as `<name>`;
/// every other context is shown as is. The context itself is never changed.
pub fn short_context_name(context: &str) -> &str {
    context
        .strip_prefix("arn:aws:eks:")
        .and_then(|rest| rest.split_once(":cluster/"))
        .map(|(_, name)| name)
        .filter(|name| !name.is_empty() && !name.contains('/'))
        .unwrap_or(context)
}

impl ClusterInfo {
    /// A context from the default kubeconfig: the id is the context name.
    pub fn from_context(context: impl Into<String>) -> Self {
        Self::new(context, None)
    }

    /// A context optionally pinned to one kubeconfig file. Contexts from
    /// extra files get the file path in their id so equal names never clash.
    pub fn new(context: impl Into<String>, kubeconfig: Option<PathBuf>) -> Self {
        let context = context.into();
        let id = match &kubeconfig {
            Some(path) => format!("{}#{context}", path.display()),
            None => context.clone(),
        };
        Self {
            id: ClusterId::new(id),
            name: short_context_name(&context).to_string(),
            context,
            kubeconfig,
            server: None,
            namespace: None,
            also_in: Vec::new(),
            aliases: Vec::new(),
        }
    }

    /// Source file name for display (`config.bak`), or `None` for the default
    /// kubeconfig resolution.
    pub fn source_file_name(&self) -> Option<String> {
        self.kubeconfig
            .as_ref()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().into_owned())
    }

    /// Avatar initials: first letters of the first two words (`dev-cluster` is
    /// `DC`); a single word contributes its first two characters.
    pub fn initials(&self) -> String {
        let words: Vec<&str> = self
            .name
            .split(|c: char| !c.is_alphanumeric())
            .filter(|w| !w.is_empty())
            .collect();
        let text: String = match words.as_slice() {
            [] => return "?".to_string(),
            [only] => only.chars().take(2).collect(),
            [first, second, ..] => first
                .chars()
                .take(1)
                .chain(second.chars().take(1))
                .collect(),
        };
        text.to_uppercase()
    }
}

/// Connection lifecycle of a cluster, shown in the rail and header.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum ConnectionState {
    #[default]
    Disconnected,
    Connecting,
    Connected,
    Failed(String),
}

impl ConnectionState {
    pub fn is_connected(&self) -> bool {
        matches!(self, Self::Connected)
    }

    pub fn label(&self) -> String {
        match self {
            Self::Disconnected => "Disconnected".to_string(),
            Self::Connecting => "Connecting".to_string(),
            Self::Connected => "Connected".to_string(),
            Self::Failed(message) => format!("Error: {message}"),
        }
    }
}
