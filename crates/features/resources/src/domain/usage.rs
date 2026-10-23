use std::collections::HashMap;

use natikube_core::{parse_cpu_cores, parse_memory_bytes};
use serde_json::Value;

use super::json::{array_at, at, str_at};

/// Live resource usage of one pod or node.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Usage {
    pub cpu_cores: f64,
    pub memory_bytes: u64,
}

/// Key of a pod (`namespace/name`) or node (`name`) in a [`UsageMap`].
pub fn usage_key(namespace: Option<&str>, name: &str) -> String {
    match namespace {
        Some(ns) if !ns.is_empty() => format!("{ns}/{name}"),
        _ => name.to_string(),
    }
}

/// Metrics by object key. Empty when metrics-server is absent.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct UsageMap(HashMap<String, Usage>);

impl UsageMap {
    pub fn insert(&mut self, key: String, usage: Usage) {
        self.0.insert(key, usage);
    }

    pub fn get(&self, key: &str) -> Option<&Usage> {
        self.0.get(key)
    }

    pub fn for_object(&self, namespace: Option<&str>, name: &str) -> Option<&Usage> {
        self.0.get(&usage_key(namespace, name))
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn values(&self) -> impl Iterator<Item = &Usage> {
        self.0.values()
    }
}

fn parse_usage(value: &Value) -> (f64, u64) {
    let cpu = str_at(value, "cpu")
        .and_then(parse_cpu_cores)
        .unwrap_or_default();
    let memory = str_at(value, "memory")
        .and_then(parse_memory_bytes)
        .unwrap_or_default();
    (cpu, memory)
}

/// Parses a `PodMetricsList` or `NodeMetricsList` (the `metrics.k8s.io/v1beta1`
/// list responses). Anything else (a 404 `Status`, garbage) yields an empty map.
pub fn parse_metrics(list: &Value) -> UsageMap {
    let mut map = UsageMap::default();
    for item in array_at(list, "items") {
        let Some(name) = str_at(item, "metadata.name") else {
            continue;
        };
        let namespace = str_at(item, "metadata.namespace");
        let (cpu, memory) = if let Some(usage) = at(item, "usage") {
            parse_usage(usage)
        } else {
            array_at(item, "containers")
                .iter()
                .filter_map(|c| at(c, "usage"))
                .map(parse_usage)
                .fold((0.0, 0u64), |(cpu, mem), (c, m)| (cpu + c, mem + m))
        };
        map.insert(
            usage_key(namespace, name),
            Usage {
                cpu_cores: cpu,
                memory_bytes: memory,
            },
        );
    }
    map
}
