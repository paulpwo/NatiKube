//! Kube adapter: follow/previous log streams for one pod container.

mod stream;

pub use stream::{
    LogEvent, LogRequest, PodContainers, lines_from_reader, log_params, pod_containers, stream_logs,
};
