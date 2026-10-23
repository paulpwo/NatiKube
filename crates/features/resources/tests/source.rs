use std::pin::Pin;
use std::task::{Context, Poll, Waker};

use futures::Stream;
use natikube_resources::{PodEvent, PodEventStream, PodSource, SourceError};

struct OneShotSource;

struct Once(Option<Result<PodEvent, SourceError>>);

impl Stream for Once {
    type Item = Result<PodEvent, SourceError>;
    fn poll_next(mut self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        Poll::Ready(self.0.take())
    }
}

impl PodSource for OneShotSource {
    fn watch(&self) -> PodEventStream {
        Box::pin(Once(Some(Ok(PodEvent::Restarted(Vec::new())))))
    }
}

#[test]
fn pod_source_is_object_safe_and_yields_events() {
    let source: Box<dyn PodSource> = Box::new(OneShotSource);
    let mut stream = source.watch();
    let mut cx = Context::from_waker(Waker::noop());
    match stream.as_mut().poll_next(&mut cx) {
        Poll::Ready(Some(Ok(PodEvent::Restarted(rows)))) => assert!(rows.is_empty()),
        _ => panic!("expected a Restarted event"),
    }
    assert!(matches!(
        stream.as_mut().poll_next(&mut cx),
        Poll::Ready(None)
    ));
}

#[test]
fn source_error_displays_its_message() {
    let err = SourceError::new("watch failed");
    assert_eq!(err.to_string(), "watch failed");
}
