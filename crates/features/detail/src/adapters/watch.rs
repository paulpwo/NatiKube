//! Live view of one object: a field-selector watcher on the shared runtime.

use futures::StreamExt as _;
use kube::api::{Api, ApiResource, DynamicObject};
use kube::core::GroupVersionKind;
use kube_runtime::{WatchStreamExt as _, watcher};
use natikube_core::{ResourceKind, ResourceRef};
use natikube_kube_client::{ClusterConnection, StreamHandle};
use serde_json::Value;

/// What the watcher tells the panel.
#[derive(Debug, Clone)]
pub enum ObjectUpdate {
    /// The current state of the object.
    Object(Value),
    /// The object was deleted.
    Deleted(Value),
    /// The initial list came back empty.
    NotFound,
    /// The watcher failed (it keeps retrying with backoff).
    Error(String),
}

pub fn api_resource(kind: &ResourceKind) -> ApiResource {
    let gvk = GroupVersionKind::gvk(&kind.group, &kind.version, &kind.kind);
    ApiResource::from_gvk_with_plural(&gvk, &kind.plural)
}

/// Starts watching `target`. Dropping the handle cancels the watcher.
pub fn watch_object(conn: &ClusterConnection, target: &ResourceRef) -> StreamHandle<ObjectUpdate> {
    let resource = api_resource(&target.kind);
    let namespace = target.namespace.clone().filter(|_| target.kind.namespaced);
    let config = watcher::Config::default().fields(&format!("metadata.name={}", target.name));
    conn.watch(move |client| {
        let api: Api<DynamicObject> = match &namespace {
            Some(ns) => Api::namespaced_with(client, ns, &resource),
            None => Api::all_with(client, &resource),
        };
        watcher(api, config)
            .default_backoff()
            .scan(false, |seen, event| {
                let update = match event {
                    Ok(watcher::Event::Init) => {
                        *seen = false;
                        None
                    }
                    Ok(watcher::Event::InitApply(obj) | watcher::Event::Apply(obj)) => {
                        *seen = true;
                        serde_json::to_value(obj).ok().map(ObjectUpdate::Object)
                    }
                    Ok(watcher::Event::InitDone) => (!*seen).then_some(ObjectUpdate::NotFound),
                    Ok(watcher::Event::Delete(obj)) => {
                        serde_json::to_value(obj).ok().map(ObjectUpdate::Deleted)
                    }
                    Err(e) => Some(ObjectUpdate::Error(e.to_string())),
                };
                futures::future::ready(Some(update))
            })
            .filter_map(futures::future::ready)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use natikube_core::kinds;

    #[test]
    fn api_resource_matches_the_kind() {
        let ar = api_resource(&kinds::DEPLOYMENT);
        assert_eq!(ar.group, "apps");
        assert_eq!(ar.version, "v1");
        assert_eq!(ar.api_version, "apps/v1");
        assert_eq!(ar.plural, "deployments");
        let core = api_resource(&kinds::POD);
        assert_eq!(core.api_version, "v1");
    }
}
