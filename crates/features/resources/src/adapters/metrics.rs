use std::time::Duration;

use futures::StreamExt as _;
use kube::api::{ApiResource, DynamicObject, ListParams};
use kube::{Api, Client};
use natikube_kube_client::ClusterConnection;

use crate::domain::{UsageMap, UsageSource, UsageStream, parse_metrics};

/// How often metrics are polled while a table is visible.
pub const POLL_INTERVAL: Duration = Duration::from_secs(15);

/// Which `metrics.k8s.io/v1beta1` list to poll.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetricsTarget {
    Pods,
    Nodes,
}

/// Polls `metrics.k8s.io/v1beta1` (read-only list). Yields an empty map when
/// metrics-server is absent or not readable. The poller lives exactly as long
/// as the stream: dropping it stops the polling.
pub struct KubeUsageSource {
    client: Client,
    target: MetricsTarget,
    interval: Duration,
}

impl KubeUsageSource {
    pub fn new(connection: &ClusterConnection, target: MetricsTarget) -> Self {
        Self {
            client: connection.client().clone(),
            target,
            interval: POLL_INTERVAL,
        }
    }
}

fn resource(target: MetricsTarget) -> ApiResource {
    let (kind, plural) = match target {
        MetricsTarget::Pods => ("PodMetrics", "pods"),
        MetricsTarget::Nodes => ("NodeMetrics", "nodes"),
    };
    ApiResource {
        group: "metrics.k8s.io".into(),
        version: "v1beta1".into(),
        api_version: "metrics.k8s.io/v1beta1".into(),
        kind: kind.into(),
        plural: plural.into(),
    }
}

async fn fetch(client: Client, target: MetricsTarget) -> UsageMap {
    let api: Api<DynamicObject> = Api::all_with(client, &resource(target));
    match api.list(&ListParams::default()).await {
        Ok(list) => parse_metrics(&serde_json::to_value(&list).unwrap_or_default()),
        Err(error) => {
            tracing::debug!(%error, "metrics unavailable");
            UsageMap::default()
        }
    }
}

impl KubeUsageSource {
    /// Polls every `interval` instead of [`POLL_INTERVAL`] (never faster than
    /// five seconds).
    pub fn with_interval(mut self, interval: Duration) -> Self {
        self.interval = interval.max(Duration::from_secs(5));
        self
    }
}

impl UsageSource for KubeUsageSource {
    fn poll(&self) -> UsageStream {
        let client = self.client.clone();
        let target = self.target;
        let interval = self.interval;
        futures::stream::unfold(true, move |first| {
            let client = client.clone();
            async move {
                if !first {
                    tokio::time::sleep(interval).await;
                }
                Some((fetch(client, target).await, false))
            }
        })
        .boxed()
    }
}
