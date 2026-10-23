//! Read-only startup timing against one context: client build + probe, then
//! time to the first pod listing. Run it with the context to measure:
//!
//! `cargo run --release -p natikube-resources --example timing -- <context> [runs] [lazy]`
//! (`lazy` skips the `/version` probe, see `ClusterConnection::connect_unverified`).

use std::time::Instant;

use futures::StreamExt as _;
use natikube_core::{ClusterInfo, kinds};
use natikube_kube_client::{ClusterConnection, runtime_handle};
use natikube_resources::{KubeObjectSource, ObjectSource, RowEvent, printer_for};

fn main() {
    let mut args = std::env::args().skip(1);
    let context = args.next().expect("context name");
    let runs: usize = args.next().and_then(|n| n.parse().ok()).unwrap_or(3);
    let lazy = args.next().as_deref() == Some("lazy");
    let info = ClusterInfo::from_context(&context);
    let handle = runtime_handle();
    for run in 1..=runs {
        let start = Instant::now();
        let conn = handle
            .block_on(async {
                if lazy {
                    ClusterConnection::connect_unverified(&info).await
                } else {
                    ClusterConnection::connect(&info).await
                }
            })
            .expect("connect");
        let connected = start.elapsed();
        let source = KubeObjectSource::new(&conn, kinds::POD, printer_for(&kinds::POD));
        let mut stream = source.watch();
        let rows = handle.block_on(async {
            while let Some(item) = stream.next().await {
                if let Ok(RowEvent::Restarted(rows)) = item {
                    return rows.len();
                }
            }
            0
        });
        println!(
            "run {run}: connect {:?}, first pods ({rows}) after {:?}, total {:?}",
            connected,
            start.elapsed() - connected,
            start.elapsed()
        );
    }
}
