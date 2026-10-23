//! Pure bodies and rules for workload actions.

use natikube_core::{ResourceKind, kinds};
use serde_json::{Value, json};

/// Largest replica count the scale box accepts.
pub const MAX_REPLICAS: u32 = 10_000;

/// Annotation `kubectl rollout restart` stamps on the pod template.
pub const RESTARTED_AT: &str = "kubectl.kubernetes.io/restartedAt";

/// Merge-patch body setting the replica count.
pub fn scale_body(replicas: u32) -> Value {
    json!({"spec": {"replicas": replicas}})
}

/// Merge-patch body that triggers a rolling restart.
pub fn restart_body(timestamp: &str) -> Value {
    json!({"spec": {"template": {"metadata": {"annotations": {RESTARTED_AT: timestamp}}}}})
}

pub fn supports_scale(kind: &ResourceKind) -> bool {
    [
        kinds::DEPLOYMENT,
        kinds::STATEFUL_SET,
        kinds::REPLICA_SET,
        kinds::REPLICATION_CONTROLLER,
    ]
    .contains(kind)
}

pub fn supports_restart(kind: &ResourceKind) -> bool {
    [kinds::DEPLOYMENT, kinds::DAEMON_SET, kinds::STATEFUL_SET].contains(kind)
}

/// Parses the number typed in the scale box.
pub fn parse_replicas(input: &str) -> Result<u32, String> {
    let n: u32 = input
        .trim()
        .parse()
        .map_err(|_| "Enter a whole number of replicas (0 or more)".to_string())?;
    if n > MAX_REPLICAS {
        return Err(format!("At most {MAX_REPLICAS} replicas"));
    }
    Ok(n)
}

#[cfg(test)]
mod tests;
