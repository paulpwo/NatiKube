//! Runs blocking work on a short-lived thread and hands back a future.

use futures::channel::oneshot;

/// Runs `work` on its own thread and resolves with its result. Works on any
/// executor (GPUI's, Tokio's or `futures::executor`). If the thread cannot be
/// spawned, the work runs inline so the caller still gets an answer.
pub(super) async fn run<T, F>(work: F) -> T
where
    T: Send + 'static,
    F: FnOnce() -> T + Send + 'static,
{
    let (tx, rx) = oneshot::channel();
    let work = std::sync::Arc::new(std::sync::Mutex::new(Some(work)));
    let job = work.clone();
    let spawned = std::thread::Builder::new()
        .name("natikube-clusters-io".into())
        .spawn(move || {
            let taken = job.lock().ok().and_then(|mut slot| slot.take());
            if let Some(f) = taken {
                let _ = tx.send(f());
            }
        });
    if spawned.is_ok()
        && let Ok(value) = rx.await
    {
        return value;
    }
    let taken = work.lock().ok().and_then(|mut slot| slot.take());
    match taken {
        Some(f) => f(),
        None => panic!("background worker panicked"),
    }
}
