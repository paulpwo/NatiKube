use std::io::Write;

use futures::executor::block_on;
use natikube_core::ClusterInfo;
use natikube_kube_client::{ClusterConnection, ConnectError, spawn};

const KUBECONFIG: &str = r#"
apiVersion: v1
kind: Config
current-context: local
clusters:
- name: local
  cluster:
    server: http://127.0.0.1:1
contexts:
- name: local
  context:
    cluster: local
    user: nobody
users:
- name: nobody
  user: {}
"#;

fn kubeconfig_file() -> tempfile::NamedTempFile {
    let mut file = tempfile::NamedTempFile::new().unwrap();
    file.write_all(KUBECONFIG.as_bytes()).unwrap();
    file
}

#[test]
fn unknown_context_is_a_config_error() {
    let file = kubeconfig_file();
    let info = ClusterInfo::new("does-not-exist", Some(file.path().to_path_buf()));
    let err = block_on(ClusterConnection::connect(&info)).unwrap_err();
    assert!(matches!(err, ConnectError::Config(_)), "got {err:?}");
}

#[test]
fn missing_kubeconfig_file_is_a_config_error() {
    let info = ClusterInfo::new("local", Some("/nonexistent/kubeconfig.yaml".into()));
    let err = block_on(ClusterConnection::connect(&info)).unwrap_err();
    assert!(matches!(err, ConnectError::Config(_)), "got {err:?}");
}

#[test]
fn unreachable_server_is_an_unreachable_error() {
    let file = kubeconfig_file();
    let info = ClusterInfo::new("local", Some(file.path().to_path_buf()));
    let err = block_on(ClusterConnection::connect(&info)).unwrap_err();
    assert!(matches!(err, ConnectError::Unreachable(_)), "got {err:?}");
    assert!(!err.to_string().is_empty());
}

#[test]
fn connection_runs_closures_with_its_client() {
    let client = block_on(spawn(async {
        let config = kube::Config::new("http://127.0.0.1:1".parse().unwrap());
        kube::Client::try_from(config).unwrap()
    }))
    .unwrap();
    let info = ClusterInfo::from_context("fake");
    let conn = ClusterConnection::from_client(info.clone(), client);
    assert_eq!(conn.info(), &info);
    assert_eq!(conn.id(), &info.id);
    let answer = block_on(conn.run(|_client| async { 7 })).unwrap();
    assert_eq!(answer, 7);
    let clone = conn.clone();
    assert_eq!(clone.id(), conn.id());
}

#[test]
fn unverified_connect_skips_the_probe_but_still_reports_config_errors() {
    let file = kubeconfig_file();
    // The server at 127.0.0.1:1 is unreachable, yet no probe means success.
    let info = ClusterInfo::new("local", Some(file.path().to_path_buf()));
    let started = std::time::Instant::now();
    let conn = block_on(ClusterConnection::connect_unverified(&info)).expect("client builds");
    assert_eq!(conn.info().context, "local");
    assert!(started.elapsed() < std::time::Duration::from_secs(5));

    let missing = ClusterInfo::new("does-not-exist", Some(file.path().to_path_buf()));
    let err = block_on(ClusterConnection::connect_unverified(&missing)).unwrap_err();
    assert!(matches!(err, ConnectError::Config(_)), "got {err:?}");
}
