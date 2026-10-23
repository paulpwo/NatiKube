//! A [`SettingsStore`] that saves off the calling thread.

use std::sync::{Arc, Condvar, Mutex};
use std::thread::JoinHandle;

use natikube_core::{Settings, SettingsError, SettingsStore};

#[derive(Default)]
struct Queue {
    /// Latest snapshot waiting to be written; newer saves replace older ones.
    pending: Option<Settings>,
    /// A write is in flight on the worker.
    writing: bool,
    closed: bool,
}

struct Shared {
    queue: Mutex<Queue>,
    /// Wakes the worker (new snapshot or close) and `flush` waiters (idle).
    changed: Condvar,
}

/// Wraps a store so `save` returns at once: the latest snapshot is written by
/// a worker thread, intermediate ones are coalesced. `load` reads through.
/// [`flush`](Self::flush) (also run on drop) waits until everything queued is
/// on disk, so quitting never loses the last change.
pub struct BackgroundSettingsStore {
    inner: Arc<dyn SettingsStore>,
    shared: Arc<Shared>,
    worker: Mutex<Option<JoinHandle<()>>>,
}

impl BackgroundSettingsStore {
    pub fn new(inner: Arc<dyn SettingsStore>) -> Self {
        let shared = Arc::new(Shared {
            queue: Mutex::new(Queue::default()),
            changed: Condvar::new(),
        });
        let worker = {
            let inner = inner.clone();
            let shared = shared.clone();
            std::thread::Builder::new()
                .name("natikube-settings-io".into())
                .spawn(move || run_worker(inner.as_ref(), &shared))
                .ok()
        };
        Self {
            inner,
            shared,
            worker: Mutex::new(worker),
        }
    }

    /// Blocks until every queued snapshot has been written.
    pub fn flush(&self) {
        let Ok(mut queue) = self.shared.queue.lock() else {
            return;
        };
        while queue.pending.is_some() || queue.writing {
            queue = match self.shared.changed.wait(queue) {
                Ok(queue) => queue,
                Err(_) => return,
            };
        }
    }
}

fn run_worker(inner: &dyn SettingsStore, shared: &Shared) {
    loop {
        let snapshot = {
            let Ok(mut queue) = shared.queue.lock() else {
                return;
            };
            loop {
                if let Some(snapshot) = queue.pending.take() {
                    queue.writing = true;
                    break snapshot;
                }
                if queue.closed {
                    return;
                }
                queue = match shared.changed.wait(queue) {
                    Ok(queue) => queue,
                    Err(_) => return,
                };
            }
        };
        if let Err(error) = inner.save(&snapshot) {
            tracing::warn!("settings: {error}");
        }
        if let Ok(mut queue) = shared.queue.lock() {
            queue.writing = false;
        }
        shared.changed.notify_all();
    }
}

impl SettingsStore for BackgroundSettingsStore {
    fn load(&self) -> Result<Settings, SettingsError> {
        self.flush();
        self.inner.load()
    }

    fn save(&self, settings: &Settings) -> Result<(), SettingsError> {
        let alive = self.worker.lock().is_ok_and(|worker| worker.is_some());
        if !alive {
            // No worker thread could be started: fall back to writing inline.
            return self.inner.save(settings);
        }
        if let Ok(mut queue) = self.shared.queue.lock() {
            queue.pending = Some(settings.clone());
        }
        self.shared.changed.notify_all();
        Ok(())
    }
}

impl Drop for BackgroundSettingsStore {
    fn drop(&mut self) {
        if let Ok(mut queue) = self.shared.queue.lock() {
            queue.closed = true;
        }
        self.shared.changed.notify_all();
        if let Ok(mut worker) = self.worker.lock()
            && let Some(handle) = worker.take()
        {
            let _ = handle.join();
        }
    }
}
