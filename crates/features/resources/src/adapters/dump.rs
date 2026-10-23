use futures::StreamExt as _;
use kube::Api;
use kube::api::{ApiResource, DynamicObject, GroupVersionKind, ListParams};
use natikube_core::{ResourceKind, kinds};
use natikube_kube_client::ClusterConnection;

use super::source::KubePodSource;
use crate::domain::{PodEvent, PodSource, SourceError};

/// Lists object names for `kind` (read-only), as the engine sees them:
/// `namespace/name` for namespaced kinds, `name` otherwise, sorted. Used by
/// `natikube --dump <kind>` for parity checks against `kubectl get`.
/// Pods go through the same watcher the UI uses.
pub async fn list_names(
    connection: &ClusterConnection,
    kind: &ResourceKind,
) -> Result<Vec<String>, SourceError> {
    let mut names = if *kind == kinds::POD {
        let source = KubePodSource::from_connection(connection);
        let mut stream = source.watch();
        let task = connection.run(|_| async move {
            while let Some(item) = stream.next().await {
                match item {
                    Ok(PodEvent::Restarted(rows)) => {
                        return Ok(rows
                            .into_iter()
                            .map(|r| format!("{}/{}", r.namespace, r.name))
                            .collect::<Vec<_>>());
                    }
                    Ok(_) => {}
                    Err(e) => return Err(e),
                }
            }
            Err(SourceError::new("watch ended before the first listing"))
        });
        task.await
            .map_err(|e| SourceError::new(format!("task: {e}")))??
    } else {
        let namespaced = kind.namespaced;
        let gvk = GroupVersionKind::gvk(&kind.group, &kind.version, &kind.kind);
        let resource = ApiResource::from_gvk_with_plural(&gvk, &kind.plural);
        let task = connection.run(|client| async move {
            let api: Api<DynamicObject> = Api::all_with(client, &resource);
            let list = api
                .list(&ListParams::default())
                .await
                .map_err(|e| SourceError::new(format!("list: {e}")))?;
            Ok::<_, SourceError>(
                list.items
                    .into_iter()
                    .map(|o| {
                        let name = o.metadata.name.unwrap_or_default();
                        match (namespaced, o.metadata.namespace) {
                            (true, Some(ns)) => format!("{ns}/{name}"),
                            _ => name,
                        }
                    })
                    .collect::<Vec<_>>(),
            )
        });
        task.await
            .map_err(|e| SourceError::new(format!("task: {e}")))??
    };
    names.sort();
    names.dedup();
    Ok(names)
}
