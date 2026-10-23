//! Small helpers to read dotted paths out of `serde_json::Value`.

use chrono::DateTime;
use serde_json::Value;

pub fn at<'a>(value: &'a Value, path: &str) -> Option<&'a Value> {
    let mut current = value;
    for part in path.split('.').filter(|p| !p.is_empty()) {
        current = match current {
            Value::Object(map) => map.get(part)?,
            Value::Array(items) => items.get(part.parse::<usize>().ok()?)?,
            _ => return None,
        };
    }
    Some(current)
}

pub fn str_at<'a>(value: &'a Value, path: &str) -> Option<&'a str> {
    at(value, path)?.as_str()
}

pub fn i64_at(value: &Value, path: &str) -> Option<i64> {
    let v = at(value, path)?;
    v.as_i64().or_else(|| v.as_f64().map(|f| f as i64))
}

pub fn bool_at(value: &Value, path: &str) -> Option<bool> {
    at(value, path)?.as_bool()
}

pub fn array_at<'a>(value: &'a Value, path: &str) -> &'a [Value] {
    at(value, path)
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default()
}

/// Keys of an object at `path`, sorted.
pub fn keys_at(value: &Value, path: &str) -> Vec<String> {
    let mut keys: Vec<String> = at(value, path)
        .and_then(Value::as_object)
        .map(|m| m.keys().cloned().collect())
        .unwrap_or_default();
    keys.sort();
    keys
}

/// Parses an RFC 3339 timestamp into unix seconds.
pub fn parse_time(text: &str) -> Option<i64> {
    DateTime::parse_from_rfc3339(text)
        .ok()
        .map(|t| t.timestamp())
}

pub fn time_at(value: &Value, path: &str) -> Option<i64> {
    parse_time(str_at(value, path)?)
}

/// `key=value, key2=value2` sorted by key; `-` when empty.
pub fn labels_text(value: &Value, path: &str) -> String {
    let Some(map) = at(value, path).and_then(Value::as_object) else {
        return "-".to_string();
    };
    if map.is_empty() {
        return "-".to_string();
    }
    let mut pairs: Vec<String> = map
        .iter()
        .map(|(k, v)| format!("{k}={}", v.as_str().unwrap_or_default()))
        .collect();
    pairs.sort();
    pairs.join(", ")
}

/// Names of the conditions whose status is `True`, space separated.
pub fn true_conditions(value: &Value, path: &str) -> String {
    array_at(value, path)
        .iter()
        .filter(|c| str_at(c, "status") == Some("True"))
        .filter_map(|c| str_at(c, "type"))
        .collect::<Vec<_>>()
        .join(" ")
}
