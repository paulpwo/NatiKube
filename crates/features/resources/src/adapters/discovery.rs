use std::time::Duration;

use futures::{Stream, StreamExt as _};
use kube::api::{DynamicObject, ListParams};
use kube::core::request::Request;
use kube::{Api, Client};
use natikube_core::kinds;
use natikube_kube_client::ClusterConnection;

use super::objects::api_resource;
use crate::domain::{CrdInfo, SourceError, crd_from_value, crds_from_list_json};

/// CRDs change rarely and the list is large, so it is re-read every five
/// minutes.
const REFRESH: Duration = Duration::from_secs(300);

const CRD_LIST_PATH: &str = "/apis/apiextensions.k8s.io/v1/customresourcedefinitions";

async fn list_crds(client: Client) -> Result<Vec<CrdInfo>, SourceError> {
    let fail =
        |e: &dyn std::fmt::Display| SourceError::new(format!("custom resource discovery: {e}"));
    let request = Request::new(CRD_LIST_PATH)
        .list(&ListParams::default())
        .map_err(|e| fail(&e))?;
    // The body holds every CRD's schema; read it as plain bytes and extract
    // only names and printer columns (see `crds_from_list_json`).
    let body = client.request_text(request).await.map_err(|e| fail(&e))?;
    let mut crds = crds_from_list_json(body.as_bytes()).map_err(|e| fail(&e))?;
    drop(body);
    crds.sort_by(|a, b| {
        (a.kind.group.as_ref(), a.kind.kind.as_ref())
            .cmp(&(b.kind.group.as_ref(), b.kind.kind.as_ref()))
    });
    Ok(crds)
}

/// Lists the cluster's CRDs now and every five minutes after (read-only). Dropping
/// the stream stops the refresh.
pub fn watch_crds(
    connection: &ClusterConnection,
) -> impl Stream<Item = Result<Vec<CrdInfo>, SourceError>> + Send + 'static {
    let client = connection.client().clone();
    futures::stream::unfold(true, move |first| {
        let client = client.clone();
        async move {
            if !first {
                tokio::time::sleep(REFRESH).await;
            }
            Some((list_crds(client).await, false))
        }
    })
    .boxed()
}

/// Fetches one CRD by its kind (`<plural>.<group>`), for the printer columns.
pub async fn fetch_crd(
    client: Client,
    kind: &natikube_core::ResourceKind,
) -> Result<CrdInfo, SourceError> {
    let api: Api<DynamicObject> =
        Api::all_with(client, &api_resource(&kinds::CUSTOM_RESOURCE_DEFINITION));
    let name = format!("{}.{}", kind.plural, kind.group);
    let object = api
        .get(&name)
        .await
        .map_err(|e| SourceError::new(format!("custom resource definition {name}: {e}")))?;
    serde_json::to_value(&object)
        .ok()
        .and_then(|v| crd_from_value(&v))
        .ok_or_else(|| SourceError::new(format!("{name} is not a valid definition")))
}
