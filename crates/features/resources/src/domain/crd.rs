use natikube_core::ResourceKind;
use serde::Deserialize;
use serde_json::Value;

use super::json::{array_at, bool_at, str_at};
use super::printers::PrinterColumn;

/// A discovered custom resource definition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CrdInfo {
    pub kind: ResourceKind,
    /// `additionalPrinterColumns` of the storage version.
    pub columns: Vec<PrinterColumn>,
}

/// Reads a `CustomResourceDefinition` object. Returns `None` when it lacks a
/// group, kind, plural or any version.
pub fn crd_from_value(crd: &Value) -> Option<CrdInfo> {
    let group = str_at(crd, "spec.group")?;
    let kind = str_at(crd, "spec.names.kind")?;
    let plural = str_at(crd, "spec.names.plural")?;
    let namespaced = str_at(crd, "spec.scope") == Some("Namespaced");
    let versions = array_at(crd, "spec.versions");
    let version = versions
        .iter()
        .find(|v| bool_at(v, "storage") == Some(true))
        .or_else(|| versions.iter().find(|v| bool_at(v, "served") == Some(true)))
        .or_else(|| versions.first())?;
    let columns = array_at(version, "additionalPrinterColumns")
        .iter()
        .filter_map(|c| {
            Some(PrinterColumn {
                name: str_at(c, "name")?.to_string(),
                json_path: str_at(c, "jsonPath")?.to_string(),
                type_: str_at(c, "type").unwrap_or("string").to_string(),
            })
        })
        .collect();
    Some(CrdInfo {
        kind: ResourceKind::custom(group, str_at(version, "name")?, kind, plural, namespaced),
        columns,
    })
}

// ---- list reader that never builds the schema -------------------------------
//
// A `CustomResourceDefinitionList` carries every CRD's full OpenAPI schema
// (megabytes on a cluster with a few operators). The discovery only needs the
// names and printer columns, so the list is read through these minimal types:
// serde skips everything else without allocating it.

#[derive(Deserialize)]
struct RawList {
    #[serde(default)]
    items: Vec<RawCrd>,
}

#[derive(Deserialize)]
struct RawCrd {
    spec: Option<RawSpec>,
}

#[derive(Deserialize)]
struct RawSpec {
    group: Option<String>,
    scope: Option<String>,
    names: Option<RawNames>,
    #[serde(default)]
    versions: Vec<RawVersion>,
}

#[derive(Deserialize)]
struct RawNames {
    kind: Option<String>,
    plural: Option<String>,
}

#[derive(Deserialize)]
struct RawVersion {
    name: Option<String>,
    served: Option<bool>,
    storage: Option<bool>,
    #[serde(default, rename = "additionalPrinterColumns")]
    columns: Vec<RawColumn>,
}

#[derive(Deserialize)]
struct RawColumn {
    name: Option<String>,
    #[serde(rename = "jsonPath")]
    json_path: Option<String>,
    #[serde(rename = "type")]
    type_: Option<String>,
}

impl RawCrd {
    fn into_info(self) -> Option<CrdInfo> {
        let spec = self.spec?;
        let names = spec.names?;
        let (group, kind, plural) = (spec.group?, names.kind?, names.plural?);
        let namespaced = spec.scope.as_deref() == Some("Namespaced");
        let mut versions = spec.versions;
        let ix = versions
            .iter()
            .position(|v| v.storage == Some(true))
            .or_else(|| versions.iter().position(|v| v.served == Some(true)))
            .or(if versions.is_empty() { None } else { Some(0) })?;
        let version = versions.swap_remove(ix);
        let columns = version
            .columns
            .into_iter()
            .filter_map(|c| {
                Some(PrinterColumn {
                    name: c.name?,
                    json_path: c.json_path?,
                    type_: c.type_.unwrap_or_else(|| "string".to_string()),
                })
            })
            .collect();
        Some(CrdInfo {
            kind: ResourceKind::custom(&group, &version.name?, &kind, &plural, namespaced),
            columns,
        })
    }
}

/// Reads a `CustomResourceDefinitionList` body into the discovered kinds,
/// skipping incomplete entries, without materializing the schemas.
pub fn crds_from_list_json(body: &[u8]) -> Result<Vec<CrdInfo>, serde_json::Error> {
    let list: RawList = serde_json::from_slice(body)?;
    Ok(list
        .items
        .into_iter()
        .filter_map(RawCrd::into_info)
        .collect())
}
