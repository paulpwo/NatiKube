//! Builds the exact HTTP requests the detail feature sends. Kept separate from
//! execution so request shapes (paths, patch types, dry-run flags) are unit
//! tested without a cluster.

use kube::core::Request;
use kube::core::params::{DeleteParams, ListParams, Patch, PatchParams};
use natikube_core::ResourceRef;
use serde_json::Value;

/// Field manager recorded by server-side apply.
pub const FIELD_MANAGER: &str = "natikube";

/// A request against one object (or its events).
#[derive(Debug, Clone, PartialEq)]
pub enum Operation {
    /// Server-side apply; `dry_run` validates without persisting.
    Apply {
        target: ResourceRef,
        body: Value,
        dry_run: bool,
    },
    Delete {
        target: ResourceRef,
    },
    MergePatch {
        target: ResourceRef,
        body: Value,
    },
    ListEvents {
        target: ResourceRef,
    },
}

/// `PatchParams` for apply: forced (the user explicitly edits), owned by [`FIELD_MANAGER`].
pub fn apply_params(dry_run: bool) -> PatchParams {
    let params = PatchParams::apply(FIELD_MANAGER).force();
    if dry_run { params.dry_run() } else { params }
}

/// Collection path of a kind, e.g. `/apis/apps/v1/namespaces/default/deployments`.
pub fn collection_path(target: &ResourceRef) -> String {
    let kind = &target.kind;
    let base = if kind.is_core() {
        format!("/api/{}", kind.version)
    } else {
        format!("/apis/{}/{}", kind.group, kind.version)
    };
    match (&target.namespace, kind.namespaced) {
        (Some(ns), true) => format!("{base}/namespaces/{ns}/{}", kind.plural),
        _ => format!("{base}/{}", kind.plural),
    }
}

fn events_path(target: &ResourceRef) -> String {
    match &target.namespace {
        Some(ns) => format!("/api/v1/namespaces/{ns}/events"),
        None => "/api/v1/events".to_string(),
    }
}

/// Expands to `Result<http::Request<Vec<u8>>, kube::core::request::Error>`.
/// A macro because the `http` crate is not a direct dependency (its type
/// cannot be named in a signature).
macro_rules! build_request {
    ($op:expr) => {{
        match $op {
            Operation::Apply {
                target,
                body,
                dry_run,
            } => Request::new(collection_path(target)).patch(
                &target.name,
                &apply_params(*dry_run),
                &Patch::Apply(body),
            ),
            Operation::MergePatch { target, body } => Request::new(collection_path(target)).patch(
                &target.name,
                &PatchParams::default(),
                &Patch::Merge(body),
            ),
            Operation::Delete { target } => {
                Request::new(collection_path(target)).delete(&target.name, &DeleteParams::default())
            }
            Operation::ListEvents { target } => {
                let mut selector = format!(
                    "involvedObject.name={},involvedObject.kind={}",
                    target.name, target.kind.kind
                );
                if let Some(ns) = &target.namespace {
                    selector.push_str(&format!(",involvedObject.namespace={ns}"));
                }
                Request::new(events_path(target)).list(&ListParams::default().fields(&selector))
            }
        }
    }};
}

/// Sends `op` through `client` and returns the JSON response.
pub(crate) async fn send(client: &kube::Client, op: &Operation) -> Result<Value, kube::Error> {
    let request = build_request!(op).map_err(kube::Error::BuildRequest)?;
    client.request::<Value>(request).await
}

/// What a request looks like on the wire (tests and diagnostics).
#[derive(Debug, Clone, PartialEq)]
pub struct RequestPreview {
    pub method: String,
    pub path: String,
    pub query: String,
    pub content_type: Option<String>,
    pub body: Option<Value>,
}

pub fn preview(op: &Operation) -> Result<RequestPreview, String> {
    let request = build_request!(op).map_err(|e| e.to_string())?;
    let uri = request.uri().to_string();
    let (path, query) = uri.split_once('?').unwrap_or((uri.as_str(), ""));
    let body = request.body();
    Ok(RequestPreview {
        method: request.method().as_str().to_string(),
        path: path.to_string(),
        query: query.to_string(),
        content_type: request
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .map(str::to_string),
        body: serde_json::from_slice(body).ok(),
    })
}

#[cfg(test)]
mod tests;
