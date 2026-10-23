//! Prints `<plural> <row count>` for every built-in kind using the same engine
//! the UI uses (read-only list/watch), to compare with `kubectl get -A`.
//!
//! `cargo run -p natikube-resources --example parity -- <context>`

use futures::StreamExt as _;
use natikube_core::{ClusterInfo, kinds};
use natikube_kube_client::{ClusterConnection, runtime_handle};
use natikube_resources::{KubeObjectSource, ObjectSource, RowEvent, printer_for};

fn main() {
    let context = std::env::args().nth(1).expect("context name");
    let info = ClusterInfo::from_context(&context);
    let handle = runtime_handle();
    let conn = handle
        .block_on(ClusterConnection::connect(&info))
        .expect("connect");
    for kind in kinds::all_builtin() {
        if kind == kinds::SECRET {
            continue; // never read Secret objects just to count them
        }
        let source = KubeObjectSource::new(&conn, kind.clone(), printer_for(&kind));
        let mut stream = source.watch();
        let count = handle.block_on(async {
            while let Some(item) = stream.next().await {
                match item {
                    Ok(RowEvent::Restarted(rows)) => return Ok(rows.len()),
                    Ok(_) => {}
                    Err(e) => return Err(e.to_string()),
                }
            }
            Err("ended".to_string())
        });
        match count {
            Ok(n) => println!("{} {n}", kind.plural),
            Err(e) => println!("{} ERR {e}", kind.plural),
        }
    }
}
