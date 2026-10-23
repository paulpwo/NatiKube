use std::pin::Pin;
use std::sync::OnceLock;
use std::task::{Context, Poll};

use futures::Stream;
use futures::StreamExt as _;
use futures::channel::mpsc::{UnboundedReceiver, unbounded};
use tokio::runtime::{Builder, Handle};
use tokio::task::{AbortHandle, JoinHandle};

/// Handle to the process-wide Tokio runtime. It lives on its own background
/// threads, so GPUI's main thread never blocks on network I/O.
pub fn runtime_handle() -> Handle {
    static HANDLE: OnceLock<Handle> = OnceLock::new();
    HANDLE
        .get_or_init(|| {
            let (tx, rx) = std::sync::mpsc::channel();
            std::thread::Builder::new()
                .name("natikube-tokio".to_string())
                .spawn(move || {
                    let runtime = Builder::new_multi_thread()
                        .worker_threads(2)
                        .thread_name("natikube-io")
                        .enable_all()
                        .build()
                        .expect("failed to build the Tokio runtime");
                    let _ = tx.send(runtime.handle().clone());
                    // Park this thread forever; the runtime keeps serving.
                    runtime.block_on(std::future::pending::<()>());
                })
                .expect("failed to start the Tokio runtime thread");
            rx.recv()
                .expect("the Tokio runtime thread reports its handle")
        })
        .clone()
}

/// Runs a future on the shared runtime. The returned [`JoinHandle`] can be
/// awaited from any executor (GPUI's included); dropping it detaches the task
/// and `abort()` cancels it.
pub fn spawn<F>(future: F) -> JoinHandle<F::Output>
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    runtime_handle().spawn(future)
}

/// Items produced by a stream running on the shared runtime, delivered
/// through a channel the UI can poll (`handle.next().await` in a GPUI task,
/// or `try_recv` to drain a batch). Dropping the handle cancels the producer:
/// this is how a view cancels its watcher on navigation.
pub struct StreamHandle<T> {
    rx: UnboundedReceiver<T>,
    abort: AbortHandle,
}

impl<T> StreamHandle<T> {
    /// Takes the next item if one is already buffered.
    pub fn try_recv(&mut self) -> Option<T> {
        self.rx.try_recv().ok()
    }
}

impl<T> Stream for StreamHandle<T> {
    type Item = T;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<T>> {
        self.rx.poll_next_unpin(cx)
    }
}

impl<T> Drop for StreamHandle<T> {
    fn drop(&mut self) {
        self.abort.abort();
    }
}

/// Drives `stream` on the shared runtime and forwards every item to the
/// returned [`StreamHandle`]. The producer stops when the stream ends or the
/// handle is dropped.
pub fn spawn_stream<S, T>(stream: S) -> StreamHandle<T>
where
    S: Stream<Item = T> + Send + 'static,
    T: Send + 'static,
{
    let (tx, rx) = unbounded();
    let task = spawn(async move {
        let mut stream = Box::pin(stream);
        while let Some(item) = stream.next().await {
            if tx.unbounded_send(item).is_err() {
                break;
            }
        }
    });
    StreamHandle {
        rx,
        abort: task.abort_handle(),
    }
}
