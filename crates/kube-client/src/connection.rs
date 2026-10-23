use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use kube::Client;
use kube::config::{Config, KubeConfigOptions, Kubeconfig};
use natikube_core::{ClusterId, ClusterInfo};
use thiserror::Error;
use tokio::task::JoinHandle;

use crate::runtime::{StreamHandle, spawn, spawn_stream};

/// How long the initial `/version` probe may take before the cluster is
/// reported unreachable.
const PROBE_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Debug, Error)]
pub enum ConnectError {
    /// The kubeconfig could not be read or the context is unusable.
    #[error("kubeconfig: {0}")]
    Config(String),
    /// The client could not be built (TLS, credentials, exec plugin).
    #[error("client: {0}")]
    Client(String),
    /// The API server did not answer.
    #[error("cluster unreachable: {0}")]
    Unreachable(String),
}

/// A live, read-write-capable connection to one cluster. Cheap to clone
/// (the client is reference counted). Features receive one of these and run
/// all their network I/O through it.
#[derive(Clone)]
pub struct ClusterConnection {
    inner: Arc<Inner>,
}

struct Inner {
    info: ClusterInfo,
    client: Client,
}

impl std::fmt::Debug for ClusterConnection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ClusterConnection")
            .field("id", &self.inner.info.id)
            .finish_non_exhaustive()
    }
}

impl ClusterConnection {
    /// Wraps an existing client (tests, alternate transports).
    pub fn from_client(info: ClusterInfo, client: Client) -> Self {
        Self {
            inner: Arc::new(Inner { info, client }),
        }
    }

    /// Builds a client for the context described by `info` and verifies the
    /// API server answers `GET /version`. Awaitable from any executor; the
    /// work runs on the shared Tokio runtime.
    pub async fn connect(info: &ClusterInfo) -> Result<Self, ConnectError> {
        let info = info.clone();
        let task_info = info.clone();
        let client = spawn(async move { build_client(&task_info, true).await })
            .await
            .map_err(|e| ConnectError::Client(format!("task failed: {e}")))??;
        Ok(Self::from_client(info, client))
    }

    /// Like [`connect`](Self::connect) but without the `GET /version` probe:
    /// the kubeconfig is read and the client built, and nothing touches the
    /// network. The probe costs a full round trip (plus the TLS handshake) in
    /// front of the first request, so callers that start watchers right away
    /// can skip it and use the watcher's own error path as the health signal.
    /// Configuration and client errors are still reported here.
    pub async fn connect_unverified(info: &ClusterInfo) -> Result<Self, ConnectError> {
        let info = info.clone();
        let task_info = info.clone();
        let client = spawn(async move { build_client(&task_info, false).await })
            .await
            .map_err(|e| ConnectError::Client(format!("task failed: {e}")))??;
        Ok(Self::from_client(info, client))
    }

    pub fn id(&self) -> &ClusterId {
        &self.inner.info.id
    }

    pub fn info(&self) -> &ClusterInfo {
        &self.inner.info
    }

    /// The kube client. Clone it freely; it is reference counted.
    pub fn client(&self) -> &Client {
        &self.inner.client
    }

    /// Runs `work` with a client on the shared runtime and returns a handle
    /// that can be awaited from GPUI:
    ///
    /// ```ignore
    /// let pods = conn.run(|client| async move { Api::<Pod>::all(client).list(&Default::default()).await });
    /// let result = pods.await;
    /// ```
    pub fn run<F, Fut, T>(&self, work: F) -> JoinHandle<T>
    where
        F: FnOnce(Client) -> Fut,
        Fut: Future<Output = T> + Send + 'static,
        T: Send + 'static,
    {
        spawn(work(self.inner.client.clone()))
    }

    /// Starts a stream built from this connection's client (typically a
    /// kube-runtime watcher) on the shared runtime. Drop the returned handle
    /// to cancel it.
    pub fn watch<F, S, T>(&self, make: F) -> StreamHandle<T>
    where
        F: FnOnce(Client) -> S,
        S: futures::Stream<Item = T> + Send + 'static,
        T: Send + 'static,
    {
        spawn_stream(make(self.inner.client.clone()))
    }
}

/// Prints a timing line on stderr when `NATIKUBE_TIMING` is set.
pub fn timing(label: &str, since: std::time::Instant) {
    if std::env::var_os("NATIKUBE_TIMING").is_some() {
        eprintln!("timing {label}: {} ms", since.elapsed().as_millis());
    }
}

async fn build_client(info: &ClusterInfo, probe: bool) -> Result<Client, ConnectError> {
    let started = std::time::Instant::now();
    let options = KubeConfigOptions {
        context: Some(info.context.clone()),
        ..KubeConfigOptions::default()
    };
    let config = match &info.kubeconfig {
        Some(path) => {
            let kubeconfig = Kubeconfig::read_from(path)
                .map_err(|e| ConnectError::Config(format!("{}: {e}", path.display())))?;
            Config::from_custom_kubeconfig(kubeconfig, &options).await
        }
        None => Config::from_kubeconfig(&options).await,
    }
    .map_err(|e| ConnectError::Config(e.to_string()))?;
    let client = Client::try_from(config).map_err(|e| ConnectError::Client(e.to_string()))?;
    timing("client built", started);
    if !probe {
        return Ok(client);
    }
    let result = tokio::time::timeout(PROBE_TIMEOUT, client.apiserver_version()).await;
    timing("version probe done", started);
    match result {
        Ok(Ok(_)) => Ok(client),
        Ok(Err(e)) => Err(ConnectError::Unreachable(e.to_string())),
        Err(_) => Err(ConnectError::Unreachable("timed out".to_string())),
    }
}
