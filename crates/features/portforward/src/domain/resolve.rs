use std::collections::BTreeMap;

use k8s_openapi::api::core::v1::{Pod, ServicePort};
use k8s_openapi::apimachinery::pkg::util::intstr::IntOrString;
use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ResolveError {
    #[error("service has no selector, so it has no pods to forward to")]
    NoSelector,
    #[error("no running pod backs this service")]
    NoPod,
    #[error("service does not expose port {0}")]
    UnknownServicePort(u16),
    #[error("pod has no container port named `{0}`")]
    UnknownNamedPort(String),
}

/// `key=value,key2=value2` for a label selector query; sorted for stability.
pub fn label_selector(selector: &BTreeMap<String, String>) -> Result<String, ResolveError> {
    if selector.is_empty() {
        return Err(ResolveError::NoSelector);
    }
    Ok(selector
        .iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<_>>()
        .join(","))
}

fn is_ready(pod: &Pod) -> bool {
    let Some(status) = &pod.status else {
        return false;
    };
    status.phase.as_deref() == Some("Running")
        && status
            .conditions
            .iter()
            .flatten()
            .any(|c| c.type_ == "Ready" && c.status == "True")
}

fn is_running(pod: &Pod) -> bool {
    pod.status.as_ref().and_then(|s| s.phase.as_deref()) == Some("Running")
        && pod.metadata.deletion_timestamp.is_none()
}

/// Picks the pod to forward to: ready first, then merely running, never a pod
/// being deleted. Ties break by name so the choice is deterministic.
pub fn pick_pod(pods: &[Pod]) -> Result<&Pod, ResolveError> {
    let name = |p: &&Pod| p.metadata.name.clone().unwrap_or_default();
    let live = || {
        pods.iter()
            .filter(|p| p.metadata.deletion_timestamp.is_none())
    };
    live()
        .filter(|p| is_ready(p))
        .min_by_key(name)
        .or_else(|| live().filter(|p| is_running(p)).min_by_key(name))
        .ok_or(ResolveError::NoPod)
}

/// Resolves the container port on `pod` that service port `service_port`
/// maps to, following `targetPort` (number, name, or absent = same number).
pub fn target_port(
    ports: &[ServicePort],
    service_port: u16,
    pod: &Pod,
) -> Result<u16, ResolveError> {
    let port = ports
        .iter()
        .find(|p| p.port == i32::from(service_port))
        .ok_or(ResolveError::UnknownServicePort(service_port))?;
    match &port.target_port {
        None => Ok(service_port),
        Some(IntOrString::Int(n)) => Ok(*n as u16),
        Some(IntOrString::String(name)) => pod
            .spec
            .iter()
            .flat_map(|s| s.containers.iter())
            .flat_map(|c| c.ports.iter().flatten())
            .find(|p| p.name.as_deref() == Some(name.as_str()))
            .map(|p| p.container_port as u16)
            .ok_or_else(|| ResolveError::UnknownNamedPort(name.clone())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn pod(v: serde_json::Value) -> Pod {
        serde_json::from_value(v).unwrap()
    }

    fn running(name: &str, ready: bool) -> Pod {
        pod(json!({
            "metadata": {"name": name},
            "status": {"phase": "Running",
                "conditions": [{"type": "Ready", "status": if ready {"True"} else {"False"}}]}
        }))
    }

    #[test]
    fn selector_is_sorted_and_joined() {
        let sel = BTreeMap::from([("b".into(), "2".into()), ("a".into(), "1".into())]);
        assert_eq!(label_selector(&sel).unwrap(), "a=1,b=2");
    }

    #[test]
    fn empty_selector_is_an_error() {
        assert_eq!(
            label_selector(&BTreeMap::new()),
            Err(ResolveError::NoSelector)
        );
    }

    #[test]
    fn prefers_ready_pods_then_name_order() {
        let pods = vec![running("z", true), running("a", false), running("m", true)];
        assert_eq!(pick_pod(&pods).unwrap().metadata.name.as_deref(), Some("m"));
    }

    #[test]
    fn falls_back_to_running_not_ready() {
        let pods = vec![running("b", false), running("a", false)];
        assert_eq!(pick_pod(&pods).unwrap().metadata.name.as_deref(), Some("a"));
    }

    #[test]
    fn skips_pending_and_terminating_pods() {
        let pending = pod(json!({"metadata": {"name": "p"}, "status": {"phase": "Pending"}}));
        let dying = pod(json!({
            "metadata": {"name": "d", "deletionTimestamp": "2024-01-01T00:00:00Z"},
            "status": {"phase": "Running", "conditions": [{"type": "Ready", "status": "True"}]}
        }));
        assert_eq!(pick_pod(&[pending, dying]), Err(ResolveError::NoPod));
        assert_eq!(pick_pod(&[]), Err(ResolveError::NoPod));
    }

    fn svc_ports(v: serde_json::Value) -> Vec<ServicePort> {
        serde_json::from_value(v).unwrap()
    }

    #[test]
    fn numeric_and_absent_target_ports() {
        let ports = svc_ports(json!([
            {"port": 80, "targetPort": 8080},
            {"port": 443}
        ]));
        let p = running("a", true);
        assert_eq!(target_port(&ports, 80, &p), Ok(8080));
        assert_eq!(target_port(&ports, 443, &p), Ok(443));
        assert_eq!(
            target_port(&ports, 22, &p),
            Err(ResolveError::UnknownServicePort(22))
        );
    }

    #[test]
    fn named_target_port_is_looked_up_in_the_pod() {
        let ports = svc_ports(json!([{"port": 80, "targetPort": "http"}]));
        let p = pod(json!({
            "metadata": {"name": "a"},
            "spec": {"containers": [{"name": "c", "ports": [{"name": "http", "containerPort": 3000}]}]}
        }));
        assert_eq!(target_port(&ports, 80, &p), Ok(3000));
        let other = running("b", true);
        assert_eq!(
            target_port(&ports, 80, &other),
            Err(ResolveError::UnknownNamedPort("http".into()))
        );
    }
}
