use std::sync::Arc;
use std::time::Duration;

use futures::StreamExt as _;
use natikube_core::{ClusterId, ResourceKind, kinds};
use natikube_kube_client::ClusterConnection;

use crate::adapters::{KubeObjectSource, KubeUsageSource, MetricsTarget, POLL_INTERVAL};
use crate::domain::{
    DemoSource, DemoUsage, FakePodSource, ObjectSource, PodRows, Printer, RowEvent, RowEventStream,
    UsageSource,
};
use crate::ui::feed::unix_now;

/// Where rows come from: a live cluster or synthetic pods (`--fake-rows`).
#[derive(Clone)]
pub(crate) enum Backend {
    Cluster(ClusterConnection, Duration),
    Fake {
        count: usize,
    },
    /// The curated demo cluster (`--demo`).
    Demo,
}

/// A source with no objects: used for kinds `--fake-rows` does not synthesize.
struct EmptySource;

impl ObjectSource for EmptySource {
    fn watch(&self) -> RowEventStream {
        futures::stream::once(async { Ok(RowEvent::Restarted(Vec::new())) })
            .chain(futures::stream::pending())
            .boxed()
    }
}

impl Backend {
    pub(crate) fn cluster_id(&self) -> ClusterId {
        match self {
            Self::Cluster(conn, _) => conn.id().clone(),
            Self::Fake { .. } => ClusterId::new("fake"),
            Self::Demo => ClusterId::new("demo"),
        }
    }

    /// How often metrics are polled (clusters only).
    pub(crate) fn poll_interval(&self) -> Duration {
        match self {
            Self::Cluster(_, interval) => *interval,
            Self::Fake { .. } | Self::Demo => POLL_INTERVAL,
        }
    }

    pub(crate) fn set_poll_interval(&mut self, interval: Duration) {
        if let Self::Cluster(_, current) = self {
            *current = interval;
        }
    }

    pub(crate) fn connection(&self) -> Option<&ClusterConnection> {
        match self {
            Self::Cluster(conn, _) => Some(conn),
            Self::Fake { .. } | Self::Demo => None,
        }
    }

    pub(crate) fn source(&self, kind: &ResourceKind, printer: &Printer) -> Arc<dyn ObjectSource> {
        match self {
            Self::Cluster(conn, _) => {
                Arc::new(KubeObjectSource::new(conn, kind.clone(), printer.clone()))
            }
            Self::Fake { count } if *kind == kinds::POD => {
                Arc::new(PodRows(FakePodSource::new(*count, unix_now())))
            }
            Self::Demo => Arc::new(DemoSource::new(kind.clone(), unix_now())),
            Self::Fake { .. } => Arc::new(EmptySource),
        }
    }

    /// Metrics poller for pods and nodes; none without a cluster.
    pub(crate) fn usage(&self, kind: &ResourceKind) -> Option<Arc<dyn UsageSource>> {
        if matches!(self, Self::Demo) {
            return (*kind == kinds::POD || *kind == kinds::NODE)
                .then(|| Arc::new(DemoUsage) as Arc<dyn UsageSource>);
        }
        let conn = self.connection()?;
        let target = if *kind == kinds::POD {
            MetricsTarget::Pods
        } else if *kind == kinds::NODE {
            MetricsTarget::Nodes
        } else {
            return None;
        };
        let source = KubeUsageSource::new(conn, target).with_interval(self.poll_interval());
        Some(Arc::new(source))
    }
}
