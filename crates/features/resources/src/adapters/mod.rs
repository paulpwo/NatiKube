//! Kubernetes adapters: kube-runtime watchers and `k8s_openapi` mappings.

mod discovery;
mod dump;
mod mapping;
mod metrics;
mod objects;
mod source;

pub use discovery::{fetch_crd, watch_crds};
pub use dump::list_names;
pub use mapping::{EventAssembler, pod_to_facts, pod_to_row};
pub use metrics::{KubeUsageSource, MetricsTarget, POLL_INTERVAL};
pub use objects::{KubeObjectSource, RowAssembler, UNREACHABLE_PREFIX, strip_ingest_noise};
pub use source::KubePodSource;
