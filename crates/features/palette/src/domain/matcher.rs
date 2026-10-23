//! Item building, subsequence scoring and ranking. No GPUI here.

use natikube_core::{AppCommand, Appearance, ViewId, kind_title};

use super::{PaletteContext, PaletteItem};

/// Subsequence score of `query` against `text`, case-insensitive. `None`
/// when the query is not a subsequence. Higher is better: prefix, word-start
/// and consecutive matches are rewarded, gaps and long texts penalised.
/// An empty query matches everything with score 0.
pub fn score(query: &str, text: &str) -> Option<i32> {
    let q: Vec<char> = query.chars().flat_map(char::to_lowercase).collect();
    if q.is_empty() {
        return Some(0);
    }
    let t: Vec<char> = text.chars().collect();
    let lower: Vec<char> = t.iter().flat_map(|c| c.to_lowercase()).collect();
    // Lowercasing can change length for exotic chars; fall back to no match.
    if lower.len() != t.len() {
        return None;
    }
    let mut total = 0i32;
    let mut qi = 0;
    let mut prev: Option<usize> = None;
    for (i, c) in lower.iter().enumerate() {
        if qi < q.len() && *c == q[qi] {
            let mut gain = 10;
            if i == 0 {
                gain += 15;
            } else if !t[i - 1].is_alphanumeric()
                || (t[i - 1].is_lowercase() && t[i].is_uppercase())
            {
                gain += 8;
            }
            match prev {
                Some(p) if p + 1 == i => gain += 12,
                Some(p) => gain -= ((i - p - 1) as i32).min(6),
                None => {}
            }
            total += gain;
            prev = Some(i);
            qi += 1;
        }
    }
    if qi < q.len() {
        return None;
    }
    Some(total - (t.len() as i32) / 8)
}

fn is_resource(item: &PaletteItem) -> bool {
    matches!(item.command, AppCommand::OpenDetail(_))
}

/// Every selectable item for `context`: commands first, then clusters,
/// kinds, and finally loaded resources.
pub fn build_items(context: &PaletteContext) -> Vec<PaletteItem> {
    let mut items = Vec::new();
    for appearance in Appearance::all() {
        items.push(PaletteItem {
            label: format!("Switch appearance: {}", appearance.label()),
            hint: "Appearance".into(),
            command: AppCommand::SetAppearance(appearance),
        });
    }
    items.push(PaletteItem {
        label: "Import from Freelens…".into(),
        hint: "Clusters".into(),
        command: AppCommand::ImportFreelens,
    });
    items.push(PaletteItem {
        label: "Add kubeconfig…".into(),
        hint: "Clusters".into(),
        command: AppCommand::AddKubeconfig,
    });
    items.push(PaletteItem {
        label: "Reload clusters".into(),
        hint: "Clusters".into(),
        command: AppCommand::ReloadClusters,
    });
    for cluster in &context.clusters {
        let hint = cluster
            .source_file_name()
            .unwrap_or_else(|| "Cluster".to_string());
        items.push(PaletteItem {
            label: format!("Switch to cluster {}", cluster.name),
            hint,
            command: AppCommand::SelectCluster(cluster.id.clone()),
        });
    }
    for view in [ViewId::ClusterOverview, ViewId::WorkloadsOverview] {
        items.push(PaletteItem {
            label: format!("Go to {}", view.title()),
            hint: "View".into(),
            command: AppCommand::Navigate(view),
        });
    }
    for kind in &context.kinds {
        items.push(PaletteItem {
            label: format!("Go to {}", kind_title(kind)),
            hint: if kind.is_core() {
                "Kind".into()
            } else {
                format!("Kind · {}", kind.group)
            },
            command: AppCommand::Navigate(ViewId::Resource(kind.clone())),
        });
    }
    for resource in &context.resources {
        let hint = match &resource.namespace {
            Some(ns) => format!("{} · {ns}", resource.kind.kind),
            None => resource.kind.kind.to_string(),
        };
        items.push(PaletteItem {
            label: resource.name.clone(),
            hint,
            command: AppCommand::OpenDetail(resource.clone()),
        });
    }
    items
}

/// Filter and order `items` for `query`, at most `limit` rows. An empty
/// query keeps the original order and hides individual resources (they would
/// drown the commands). Ties keep the original order.
pub fn rank(items: &[PaletteItem], query: &str, limit: usize) -> Vec<PaletteItem> {
    let query = query.trim();
    if query.is_empty() {
        return items
            .iter()
            .filter(|i| !is_resource(i))
            .take(limit)
            .cloned()
            .collect();
    }
    let mut scored: Vec<(i32, usize, &PaletteItem)> = items
        .iter()
        .enumerate()
        .filter_map(|(ix, item)| {
            let s = score(query, &item.label)?;
            // Resource names are the most specific target: nudge them up.
            Some((if is_resource(item) { s + 5 } else { s }, ix, item))
        })
        .collect();
    scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    scored
        .into_iter()
        .take(limit)
        .map(|(_, _, item)| item.clone())
        .collect()
}
