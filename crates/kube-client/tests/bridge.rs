use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use futures::StreamExt as _;
use futures::executor::block_on;
use natikube_kube_client::{runtime_handle, spawn, spawn_stream};

#[test]
fn spawned_futures_run_on_the_shared_runtime() {
    let value = block_on(spawn(async { 40 + 2 })).unwrap();
    assert_eq!(value, 42);
    let on_tokio = block_on(spawn(async {
        tokio::runtime::Handle::try_current().is_ok()
    }))
    .unwrap();
    assert!(on_tokio);
}

#[test]
fn runtime_handle_is_stable() {
    let a = runtime_handle().id();
    let b = runtime_handle().id();
    assert_eq!(a, b);
}

#[test]
fn spawn_stream_forwards_items_in_order_and_ends() {
    let handle = spawn_stream(futures::stream::iter(1..=4));
    let items: Vec<i32> = block_on(handle.collect());
    assert_eq!(items, [1, 2, 3, 4]);
}

struct SetOnDrop(Arc<AtomicBool>);

impl Drop for SetOnDrop {
    fn drop(&mut self) {
        self.0.store(true, Ordering::SeqCst);
    }
}

#[test]
fn dropping_the_handle_cancels_the_producer() {
    let dropped = Arc::new(AtomicBool::new(false));
    let guard = SetOnDrop(dropped.clone());
    let stream = futures::stream::once(async move {
        let _guard = guard;
        std::future::pending::<()>().await;
        0u8
    });
    let handle = spawn_stream(stream);
    std::thread::sleep(Duration::from_millis(100));
    assert!(!dropped.load(Ordering::SeqCst));
    drop(handle);
    for _ in 0..50 {
        if dropped.load(Ordering::SeqCst) {
            return;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    panic!("producer was not cancelled after the handle was dropped");
}

#[test]
fn consumer_can_poll_with_try_next_semantics() {
    let mut handle = spawn_stream(futures::stream::iter(["a", "b"]));
    assert_eq!(block_on(handle.next()), Some("a"));
    assert_eq!(block_on(handle.next()), Some("b"));
    assert_eq!(block_on(handle.next()), None);
}
