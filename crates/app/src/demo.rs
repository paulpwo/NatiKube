//! `--demo`: a fictional cluster catalog. Nothing here is read from a
//! kubeconfig or reaches the network; the resources come from the curated demo
//! data in `natikube-resources`.

use natikube_core::ClusterInfo;

/// Context names of the demo clusters, in rail order.
pub const DEMO_CONTEXTS: [&str; 2] = ["demo-cluster", "staging"];

/// The demo catalog: two clusters that both show the demo data.
pub fn demo_catalog() -> Vec<ClusterInfo> {
    DEMO_CONTEXTS
        .iter()
        .map(|context| {
            let mut info = ClusterInfo::from_context(*context);
            info.server = Some(format!("https://{context}.example.com:6443"));
            info
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_has_the_two_demo_clusters_and_no_kubeconfig_file() {
        let catalog = demo_catalog();
        let names: Vec<&str> = catalog.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, ["demo-cluster", "staging"]);
        assert!(catalog.iter().all(|c| c.kubeconfig.is_none()));
        assert!(catalog.iter().all(|c| {
            c.server
                .as_deref()
                .is_some_and(|s| s.contains(".example.com"))
        }));
    }
}
