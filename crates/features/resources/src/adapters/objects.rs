use std::sync::Arc;

use futures::{StreamExt, future};
use kube::api::{ApiResource, DynamicObject, GroupVersionKind};
use kube::runtime::WatchStreamExt;
use kube::runtime::watcher::{self, Event, watcher};
use kube::{Api, Client};
use natikube_core::ResourceKind;
use natikube_kube_client::{ClusterConnection, timing};

use crate::domain::{
    Interner, ObjectRow, ObjectSource, Printer, RowEvent, RowEventStream, SourceError,
};

pub(crate) fn api_resource(kind: &ResourceKind) -> ApiResource {
    let gvk = GroupVersionKind::gvk(&kind.group, &kind.version, &kind.kind);
    ApiResource::from_gvk_with_plural(&gvk, &kind.plural)
}

/// Turns raw watcher events into row events, collapsing each initial
/// (re)listing into a single [`RowEvent::Restarted`]. Objects are converted to
/// display rows immediately and never retained.
pub struct RowAssembler {
    printer: Arc<Printer>,
    buffer: Vec<ObjectRow>,
    pool: Interner,
}

/// The annotation `kubectl apply` stores a full copy of the manifest in.
const LAST_APPLIED: &str = "kubectl.kubernetes.io/last-applied-configuration";

/// Drops the bulky metadata no column reads (`managedFields` and the
/// last-applied annotation, often larger than the rest of the object) before
/// the object is converted, so it is never copied or kept.
pub fn strip_ingest_noise(object: &mut DynamicObject) {
    object.metadata.managed_fields = None;
    if let Some(annotations) = &mut object.metadata.annotations {
        annotations.remove(LAST_APPLIED);
        if annotations.is_empty() {
            object.metadata.annotations = None;
        }
    }
}

impl RowAssembler {
    pub fn new(printer: Arc<Printer>) -> Self {
        Self {
            printer,
            buffer: Vec::new(),
            pool: Interner::new(),
        }
    }

    fn row(&mut self, mut object: DynamicObject) -> ObjectRow {
        strip_ingest_noise(&mut object);
        let value = serde_json::to_value(&object).unwrap_or_default();
        drop(object);
        let mut row = self.printer.row(&value);
        row.compact(&mut self.pool);
        row
    }

    pub fn push(&mut self, event: Event<DynamicObject>) -> Option<RowEvent> {
        match event {
            Event::Init => {
                self.buffer.clear();
                None
            }
            Event::InitApply(object) => {
                let row = self.row(object);
                self.buffer.push(row);
                None
            }
            Event::InitDone => {
                let mut rows = std::mem::take(&mut self.buffer);
                rows.shrink_to_fit();
                Some(RowEvent::Restarted(rows))
            }
            Event::Apply(object) => Some(RowEvent::Applied(self.row(object))),
            Event::Delete(object) => Some(RowEvent::Deleted {
                namespace: object.metadata.namespace,
                name: object.metadata.name.unwrap_or_default(),
            }),
        }
    }
}

/// Read-only watcher over every object of one kind, across all namespaces.
pub struct KubeObjectSource {
    client: Client,
    kind: ResourceKind,
    printer: Arc<Printer>,
}

impl KubeObjectSource {
    pub fn new(connection: &ClusterConnection, kind: ResourceKind, printer: Printer) -> Self {
        Self {
            client: connection.client().clone(),
            kind,
            printer: Arc::new(printer),
        }
    }
}

impl ObjectSource for KubeObjectSource {
    fn watch(&self) -> RowEventStream {
        let api: Api<DynamicObject> = Api::all_with(self.client.clone(), &api_resource(&self.kind));
        let mut assembler = RowAssembler::new(self.printer.clone());
        let plural = self.kind.plural.clone();
        let started = std::time::Instant::now();
        let mut first = true;
        let stream = watcher(api, watcher::Config::default())
            .default_backoff()
            .filter_map(move |item| {
                let out = match item {
                    Ok(event) => assembler.push(event).map(|row| {
                        if std::mem::take(&mut first) {
                            timing(&format!("{plural} first list"), started);
                        }
                        Ok(row)
                    }),
                    Err(e) => Some(Err(SourceError::new(describe_watch_error(&e)))),
                };
                future::ready(out)
            });
        Box::pin(stream)
    }
}

/// Start of the watcher error for an unreachable API server.
pub const UNREACHABLE_PREFIX: &str = "Cannot reach the cluster";

/// Short, human readable text for a watcher failure (forbidden, unreachable,
/// kind not served).
fn describe_watch_error(error: &watcher::Error) -> String {
    let text = error.to_string();
    if text.contains("403") || text.to_lowercase().contains("forbidden") {
        "Access denied (forbidden): your account cannot list this resource".to_string()
    } else if text.contains("404") || text.to_lowercase().contains("not found") {
        "This resource is not available on this cluster".to_string()
    } else {
        format!("{UNREACHABLE_PREFIX}: {text}")
    }
}
