use std::io;

use k8s_openapi::api::core::v1::{Pod, Service};
use kube::api::ListParams;
use kube::{Api, Client};
use natikube_core::PortForwardSpec;

use super::bridge::{AbortOnDrop, Bound, serve_listener};
use crate::domain::{label_selector, pick_pod, target_port};

/// A running forward. Dropping it closes the listener and its connections.
#[derive(Debug)]
pub struct ActiveForward {
    pub local_port: u16,
    _guard: AbortOnDrop,
}

/// Resolves the target (a service becomes one of its pods), binds the local
/// port and starts serving. Errors are user-presentable strings.
///
/// Must run on the Tokio runtime (`natikube_kube_client::spawn`). Nothing
/// is sent to the cluster until a local client connects, except the lookups
/// (get service, list pods) needed to resolve a service target.
pub async fn start_forward(client: Client, spec: PortForwardSpec) -> Result<ActiveForward, String> {
    let namespace = spec.target.namespace.clone().unwrap_or_default();
    let (pod_name, pod_port) = match spec.target.kind.kind.as_ref() {
        "Pod" => (spec.target.name.clone(), spec.remote_port),
        "Service" => resolve_service(&client, &namespace, &spec).await?,
        other => return Err(format!("cannot port-forward to a {other}")),
    };

    let bound = Bound::bind(spec.local_port)
        .await
        .map_err(|e| match spec.local_port {
            Some(port) if e.kind() == io::ErrorKind::AddrInUse => {
                format!("local port {port} is already in use")
            }
            _ => format!("cannot listen on localhost: {e}"),
        })?;
    let local_port = bound.addr.port();

    let pods: Api<Pod> = Api::namespaced(client, &namespace);
    let task = tokio::spawn(serve_listener(bound.listener, move || {
        let pods = pods.clone();
        let pod_name = pod_name.clone();
        async move { open_pod_stream(pods, &pod_name, pod_port).await }
    }));
    Ok(ActiveForward {
        local_port,
        _guard: AbortOnDrop::new(task.abort_handle()),
    })
}

async fn resolve_service(
    client: &Client,
    namespace: &str,
    spec: &PortForwardSpec,
) -> Result<(String, u16), String> {
    let services: Api<Service> = Api::namespaced(client.clone(), namespace);
    let service = services
        .get(&spec.target.name)
        .await
        .map_err(|e| format!("cannot read service {}: {e}", spec.target.name))?;
    let service_spec = service.spec.unwrap_or_default();
    let selector =
        label_selector(&service_spec.selector.unwrap_or_default()).map_err(|e| e.to_string())?;
    let pods: Api<Pod> = Api::namespaced(client.clone(), namespace);
    let list = pods
        .list(&ListParams::default().labels(&selector))
        .await
        .map_err(|e| format!("cannot list pods for service {}: {e}", spec.target.name))?;
    let pod = pick_pod(&list.items).map_err(|e| e.to_string())?;
    let port = target_port(
        &service_spec.ports.unwrap_or_default(),
        spec.remote_port,
        pod,
    )
    .map_err(|e| e.to_string())?;
    Ok((pod.metadata.name.clone().unwrap_or_default(), port))
}

async fn open_pod_stream(
    pods: Api<Pod>,
    pod: &str,
    port: u16,
) -> io::Result<impl tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + use<>> {
    let mut forwarder = pods
        .portforward(pod, &[port])
        .await
        .map_err(io::Error::other)?;
    let stream = forwarder
        .take_stream(port)
        .ok_or_else(|| io::Error::other("port stream unavailable"))?;
    // Keep the websocket pump alive for as long as the stream is in use.
    tokio::spawn(async move {
        let _ = forwarder.join().await;
    });
    Ok(stream)
}
