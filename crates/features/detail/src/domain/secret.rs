//! Secret masking. Values never leave this module unmasked unless the caller
//! explicitly asks for a reveal; nothing here logs or prints values.

use k8s_openapi::ByteString;
use serde_json::{Map, Value};

/// What replaces a secret value while it is hidden.
pub const MASK: &str = "••••••••";

/// Annotation kubectl uses to store the full applied manifest, which for a
/// Secret contains every data value.
pub const LAST_APPLIED: &str = "kubectl.kubernetes.io/last-applied-configuration";

/// One key of a Secret.
#[derive(Clone, PartialEq, Eq)]
pub struct SecretEntry {
    pub key: String,
    bytes: Vec<u8>,
}

// Deliberately no value in Debug output.
impl std::fmt::Debug for SecretEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SecretEntry")
            .field("key", &self.key)
            .field("len", &self.bytes.len())
            .finish()
    }
}

impl SecretEntry {
    /// The value as UTF-8 text; `None` for binary data.
    pub fn decoded(&self) -> Option<String> {
        String::from_utf8(self.bytes.clone()).ok()
    }

    pub fn byte_len(&self) -> usize {
        self.bytes.len()
    }
}

/// Lists `data` (base64) and `stringData` (plain) entries sorted by key.
pub fn secret_entries(obj: &Value) -> Vec<SecretEntry> {
    let mut entries: Vec<SecretEntry> = Vec::new();
    if let Some(data) = obj.get("data").and_then(Value::as_object) {
        for (key, value) in data {
            if let Ok(ByteString(bytes)) = serde_json::from_value::<ByteString>(value.clone()) {
                entries.push(SecretEntry {
                    key: key.clone(),
                    bytes,
                });
            }
        }
    }
    if let Some(data) = obj.get("stringData").and_then(Value::as_object) {
        for (key, value) in data {
            if let Some(text) = value.as_str() {
                entries.retain(|e| &e.key != key);
                entries.push(SecretEntry {
                    key: key.clone(),
                    bytes: text.as_bytes().to_vec(),
                });
            }
        }
    }
    entries.sort_by(|a, b| a.key.cmp(&b.key));
    entries
}

/// The text shown for an entry.
pub fn display_value(entry: &SecretEntry, revealed: bool) -> String {
    if !revealed {
        return MASK.to_string();
    }
    match entry.decoded() {
        Some(text) => text,
        None => format!("<{} bytes of binary data>", entry.byte_len()),
    }
}

/// A copy of `obj` with every `data` / `stringData` value replaced by [`MASK`].
pub fn mask_secret_data(obj: &Value) -> Value {
    let mut masked = obj.clone();
    for field in ["data", "stringData"] {
        if let Some(map) = masked.get_mut(field).and_then(Value::as_object_mut) {
            let replaced: Map<String, Value> = map
                .keys()
                .map(|k| (k.clone(), Value::String(MASK.to_string())))
                .collect();
            *map = replaced;
        }
    }
    if let Some(value) = masked
        .get_mut("metadata")
        .and_then(|m| m.get_mut("annotations"))
        .and_then(Value::as_object_mut)
        .and_then(|a| a.get_mut(LAST_APPLIED))
    {
        *value = Value::String(MASK.to_string());
    }
    masked
}

/// True when any secret value is the mask sentinel (saving that would destroy data).
pub fn contains_mask(obj: &Value) -> bool {
    let in_maps = ["data", "stringData"].iter().any(|field| {
        obj.get(field)
            .and_then(Value::as_object)
            .is_some_and(|map| map.values().any(|v| v.as_str() == Some(MASK)))
    });
    let in_annotation = obj
        .get("metadata")
        .and_then(|m| m.get("annotations"))
        .and_then(|a| a.get(LAST_APPLIED))
        .is_some_and(|v| v.as_str() == Some(MASK));
    in_maps || in_annotation
}

#[cfg(test)]
mod tests;
