use futures::{StreamExt, future};
use k8s_openapi::api::core::v1::Pod;
use kube::runtime::{WatchStreamExt, watcher};
use kube::{Api, Client};
use natikube_kube_client::ClusterConnection;

use super::mapping::EventAssembler;
use crate::domain::{PodEventStream, PodSource, SourceError};

/// Read-only pod source backed by one kube-runtime watcher over all
/// namespaces.
#[derive(Clone)]
pub struct KubePodSource {
    client: Client,
}

impl KubePodSource {
    pub fn new(client: Client) -> Self {
        Self { client }
    }

    pub fn from_connection(connection: &ClusterConnection) -> Self {
        Self::new(connection.client().clone())
    }
}

impl PodSource for KubePodSource {
    fn watch(&self) -> PodEventStream {
        let api: Api<Pod> = Api::all(self.client.clone());
        let mut assembler = EventAssembler::default();
        let stream = watcher(api, watcher::Config::default())
            .default_backoff()
            .filter_map(move |item| {
                let out = match item {
                    Ok(event) => assembler.push(event).map(Ok),
                    Err(e) => Some(Err(SourceError::new(format!("watch: {e}")))),
                };
                future::ready(out)
            });
        Box::pin(stream)
    }
}
