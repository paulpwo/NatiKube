//! YAML text <-> JSON object conversion for the YAML tab.

use natikube_core::{ResourceKind, ResourceRef};
use serde_json::Value;

/// Removes `metadata.managedFields` (noise that cannot be applied back).
pub fn strip_managed_fields(obj: &mut Value) {
    if let Some(meta) = obj.get_mut("metadata").and_then(Value::as_object_mut) {
        meta.remove("managedFields");
    }
}

/// List responses omit `apiVersion`/`kind` on items; apply needs them.
pub fn ensure_type_meta(obj: &mut Value, kind: &ResourceKind) {
    if let Some(map) = obj.as_object_mut() {
        map.entry("apiVersion")
            .or_insert_with(|| Value::String(kind.api_version()));
        map.entry("kind")
            .or_insert_with(|| Value::String(kind.kind.to_string()));
    }
}

/// Serializes `obj` as YAML, optionally without `managedFields`.
pub fn to_yaml(obj: &Value, strip: bool) -> String {
    let mut copy;
    let source = if strip {
        copy = obj.clone();
        strip_managed_fields(&mut copy);
        &copy
    } else {
        obj
    };
    serde_yaml_ng::to_string(source).unwrap_or_default()
}

/// Parses editor text into a JSON object.
pub fn parse_yaml(text: &str) -> Result<Value, String> {
    let value: Value = serde_yaml_ng::from_str(text).map_err(|e| e.to_string())?;
    if value.is_object() {
        Ok(value)
    } else {
        Err("The document must be a YAML mapping (an object)".to_string())
    }
}

/// An edit must keep pointing at the same object.
pub fn check_identity(target: &ResourceRef, edited: &Value) -> Result<(), String> {
    let text = |v: Option<&Value>| v.and_then(Value::as_str).map(str::to_string);
    let expect = |field: &str, want: &str, got: Option<String>| match got {
        Some(g) if g == want => Ok(()),
        Some(g) => Err(format!(
            "{field} changed from \"{want}\" to \"{g}\"; edit the existing object only"
        )),
        None => Err(format!("{field} is missing")),
    };
    expect(
        "apiVersion",
        &target.kind.api_version(),
        text(edited.get("apiVersion")),
    )?;
    expect("kind", &target.kind.kind, text(edited.get("kind")))?;
    let meta = edited.get("metadata");
    expect(
        "metadata.name",
        &target.name,
        text(meta.and_then(|m| m.get("name"))),
    )?;
    match (
        &target.namespace,
        text(meta.and_then(|m| m.get("namespace"))),
    ) {
        (Some(ns), got) => expect("metadata.namespace", ns, got),
        (None, None) => Ok(()),
        (None, Some(_)) => {
            Err("metadata.namespace must not be set on a cluster-scoped object".into())
        }
    }
}

#[cfg(test)]
mod tests;
