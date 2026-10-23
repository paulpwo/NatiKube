use futures::io::AsyncBufRead;
use futures::io::AsyncBufReadExt as _;
use futures::{Stream, StreamExt as _, stream};
use k8s_openapi::api::core::v1::Pod;
use kube::api::LogParams;
use kube::{Api, Client};
use natikube_core::ResourceRef;

use crate::domain::DEFAULT_TAIL_LINES;

/// What to stream. Timestamps are always requested; the UI toggles their
/// visibility without re-streaming.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogRequest {
    pub pod: ResourceRef,
    pub container: Option<String>,
    pub previous: bool,
    pub follow: bool,
    pub tail_lines: Option<i64>,
}

impl LogRequest {
    pub fn new(pod: ResourceRef, container: Option<String>) -> Self {
        Self {
            pod,
            container,
            previous: false,
            follow: true,
            tail_lines: Some(DEFAULT_TAIL_LINES),
        }
    }
}

/// One item of a log stream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LogEvent {
    /// A complete line, as sent by the API (timestamp prefix included).
    Line(String),
    /// The stream ended normally (container finished or `follow` is off).
    Ended,
    Error(String),
}

/// Maps a request to the API parameters. A previous container cannot be
/// followed, so `follow` is dropped in that case.
pub fn log_params(req: &LogRequest) -> LogParams {
    LogParams {
        container: req.container.clone(),
        follow: req.follow && !req.previous,
        previous: req.previous,
        tail_lines: req.tail_lines,
        timestamps: true,
        ..LogParams::default()
    }
}

/// Turns any buffered async reader into line events, ending with
/// [`LogEvent::Ended`] (or [`LogEvent::Error`]).
pub fn lines_from_reader<R>(reader: R) -> impl Stream<Item = LogEvent> + Send
where
    R: AsyncBufRead + Unpin + Send + 'static,
{
    stream::unfold(Some(reader), |state| async move {
        let mut reader = state?;
        let mut buf = Vec::new();
        match reader.read_until(b'\n', &mut buf).await {
            Ok(0) => Some((LogEvent::Ended, None)),
            Ok(_) => {
                if buf.last() == Some(&b'\n') {
                    buf.pop();
                }
                let line = String::from_utf8_lossy(&buf).into_owned();
                Some((LogEvent::Line(line), Some(reader)))
            }
            Err(e) => Some((LogEvent::Error(e.to_string()), None)),
        }
    })
}

/// Opens the log stream of `req` on `client`. Must be polled on the Tokio
/// runtime (use `natikube_kube_client::spawn_stream`).
pub fn stream_logs(client: Client, req: LogRequest) -> impl Stream<Item = LogEvent> + Send {
    let namespace = req.pod.namespace.clone().unwrap_or_default();
    let name = req.pod.name.clone();
    let params = log_params(&req);
    stream::once(async move {
        let api: Api<Pod> = Api::namespaced(client, &namespace);
        match api.log_stream(&name, &params).await {
            Ok(reader) => lines_from_reader(Box::pin(reader)).left_stream(),
            Err(e) => stream::once(async move { LogEvent::Error(e.to_string()) }).right_stream(),
        }
    })
    .flatten()
}

/// Container names of a pod.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PodContainers {
    pub init: Vec<String>,
    pub regular: Vec<String>,
}

impl PodContainers {
    /// Regular containers first, then init containers.
    pub fn all(&self) -> Vec<String> {
        self.regular.iter().chain(&self.init).cloned().collect()
    }

    /// The container the API would pick: the first regular one.
    pub fn default_container(&self) -> Option<&str> {
        self.regular
            .first()
            .or(self.init.first())
            .map(String::as_str)
    }
}

/// Reads the container names of `pod`.
pub async fn pod_containers(client: Client, pod: &ResourceRef) -> Result<PodContainers, String> {
    let api: Api<Pod> = Api::namespaced(client, pod.namespace.as_deref().unwrap_or_default());
    let pod = api.get(&pod.name).await.map_err(|e| e.to_string())?;
    Ok(containers_of(&pod))
}

fn containers_of(pod: &Pod) -> PodContainers {
    let Some(spec) = &pod.spec else {
        return PodContainers::default();
    };
    PodContainers {
        init: spec
            .init_containers
            .iter()
            .flatten()
            .map(|c| c.name.clone())
            .collect(),
        regular: spec.containers.iter().map(|c| c.name.clone()).collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::io::Cursor;
    use natikube_core::{ClusterId, kinds};

    fn req() -> LogRequest {
        LogRequest::new(
            ResourceRef::new(
                ClusterId::new("c"),
                kinds::POD,
                Some("default".into()),
                "api-1",
            ),
            Some("app".into()),
        )
    }

    #[test]
    fn params_follow_with_tail_and_timestamps() {
        let p = log_params(&req());
        assert!(p.follow && p.timestamps && !p.previous);
        assert_eq!(p.tail_lines, Some(1000));
        assert_eq!(p.container.as_deref(), Some("app"));
    }

    #[test]
    fn previous_container_is_not_followed() {
        let mut r = req();
        r.previous = true;
        let p = log_params(&r);
        assert!(p.previous && !p.follow);
    }

    #[test]
    fn splits_reader_into_lines_then_ends() {
        let reader = Cursor::new(b"one\ntwo\r\nlast".to_vec());
        let events = futures::executor::block_on(lines_from_reader(reader).collect::<Vec<_>>());
        assert_eq!(
            events,
            vec![
                LogEvent::Line("one".into()),
                LogEvent::Line("two\r".into()),
                LogEvent::Line("last".into()),
                LogEvent::Ended,
            ]
        );
    }

    #[test]
    fn invalid_utf8_is_replaced_not_fatal() {
        let reader = Cursor::new(vec![b'a', 0xff, b'\n']);
        let events = futures::executor::block_on(lines_from_reader(reader).collect::<Vec<_>>());
        assert_eq!(events[0], LogEvent::Line("a\u{fffd}".into()));
    }

    #[test]
    fn separates_init_and_regular_containers() {
        let pod: Pod = serde_json::from_value(serde_json::json!({
            "spec": {"initContainers": [{"name": "init"}], "containers": [{"name": "app"}, {"name": "side"}]}
        }))
        .unwrap();
        let c = containers_of(&pod);
        assert_eq!(c.init, vec!["init"]);
        assert_eq!(c.all(), vec!["app", "side", "init"]);
        assert_eq!(c.default_container(), Some("app"));
    }
}
