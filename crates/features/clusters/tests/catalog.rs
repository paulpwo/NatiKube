use std::io::Write;

use natikube_clusters::{contexts_in_file, discover_clusters};
use natikube_core::ClusterId;

const KUBECONFIG: &str = r#"
apiVersion: v1
kind: Config
current-context: alpha
clusters:
- name: alpha-cluster
  cluster:
    server: https://alpha.example:6443
- name: beta-cluster
  cluster:
    server: https://beta.example:6443
contexts:
- name: alpha
  context:
    cluster: alpha-cluster
    user: u
    namespace: team-a
- name: beta
  context:
    cluster: beta-cluster
    user: u
users:
- name: u
  user: {}
"#;

fn write(dir: &tempfile::TempDir, name: &str, body: &str) -> std::path::PathBuf {
    let path = dir.path().join(name);
    std::fs::File::create(&path)
        .unwrap()
        .write_all(body.as_bytes())
        .unwrap();
    path
}

#[test]
fn lists_every_context_with_server_and_namespace() {
    let dir = tempfile::tempdir().unwrap();
    let path = write(&dir, "config", KUBECONFIG);
    let found = contexts_in_file(&path).unwrap();
    assert_eq!(found.len(), 2);
    assert_eq!(found[0].context, "alpha");
    assert_eq!(
        found[0].server.as_deref(),
        Some("https://alpha.example:6443")
    );
    assert_eq!(found[0].namespace.as_deref(), Some("team-a"));
    assert_eq!(found[1].namespace, None);
    assert_eq!(found[0].kubeconfig.as_deref(), Some(path.as_path()));
}

#[test]
fn unreadable_file_is_an_error() {
    let dir = tempfile::tempdir().unwrap();
    let path = write(&dir, "bad", "not: [valid");
    assert!(contexts_in_file(&path).is_err());
    assert!(contexts_in_file(&dir.path().join("missing")).is_err());
}

#[test]
fn catalog_includes_extra_files_and_folders_deduped() {
    let dir = tempfile::tempdir().unwrap();
    let file = write(&dir, "one.yaml", KUBECONFIG);
    let folder = tempfile::tempdir().unwrap();
    write(&folder, "two.yaml", KUBECONFIG);
    write(&folder, "garbage.txt", "nope");
    let extras = [file.clone(), folder.path().to_path_buf()];
    let catalog = discover_clusters(None, None, &extras);
    // Both files declare the identical two contexts: folded into two entries.
    assert_eq!(catalog.len(), 2);
    assert!(catalog.iter().all(|c| c.kubeconfig.is_some()));
    assert!(catalog.iter().all(|c| c.also_in.len() == 1));
    let ids: std::collections::HashSet<&ClusterId> = catalog.iter().map(|c| &c.id).collect();
    assert_eq!(ids.len(), 2);
}
