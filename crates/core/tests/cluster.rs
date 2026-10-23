use std::path::PathBuf;

use natikube_core::{ClusterId, ClusterInfo, ConnectionState, short_context_name};

#[test]
fn initials_use_first_letters_of_the_first_two_words() {
    assert_eq!(ClusterInfo::from_context("dev-cluster").initials(), "DC");
    assert_eq!(ClusterInfo::from_context("prod_eu west").initials(), "PE");
    // An EKS ARN displays as its short cluster name, so initials follow it.
    assert_eq!(
        ClusterInfo::from_context("arn:aws:eks:us-east-1:000000000000:cluster/demo-eu").initials(),
        "DE"
    );
}

#[test]
fn eks_arn_contexts_display_their_short_name_but_keep_the_context() {
    let arn = "arn:aws:eks:us-east-1:000000000000:cluster/staging-east";
    let info = ClusterInfo::from_context(arn);
    assert_eq!(info.name, "staging-east");
    assert_eq!(info.context, arn);
    assert_eq!(info.id.as_str(), arn);
    assert_eq!(short_context_name(arn), "staging-east");
    // Anything that is not a cluster ARN is left alone.
    assert_eq!(short_context_name("minikube"), "minikube");
    assert_eq!(
        short_context_name("arn:aws:eks:us-east-1:0:cluster/"),
        "arn:aws:eks:us-east-1:0:cluster/"
    );
    assert_eq!(
        short_context_name("gke_proj_zone_name"),
        "gke_proj_zone_name"
    );
}

#[test]
fn initials_of_a_single_word_take_two_characters() {
    assert_eq!(ClusterInfo::from_context("minikube").initials(), "MI");
    assert_eq!(ClusterInfo::from_context("x").initials(), "X");
    assert_eq!(ClusterInfo::from_context("").initials(), "?");
}

#[test]
fn id_is_the_context_for_the_default_kubeconfig() {
    let info = ClusterInfo::from_context("dev-cluster");
    assert_eq!(info.id, ClusterId::new("dev-cluster"));
    assert_eq!(info.name, "dev-cluster");
    assert!(info.kubeconfig.is_none());
}

#[test]
fn id_includes_the_file_for_extra_kubeconfigs() {
    let info = ClusterInfo::new("dev", Some(PathBuf::from("/tmp/extra.yaml")));
    assert_eq!(info.id.as_str(), "/tmp/extra.yaml#dev");
    let other = ClusterInfo::new("dev", None);
    assert_ne!(info.id, other.id);
}

#[test]
fn connection_state_labels() {
    assert!(ConnectionState::Connected.is_connected());
    assert!(!ConnectionState::Connecting.is_connected());
    assert_eq!(ConnectionState::Disconnected.label(), "Disconnected");
    assert_eq!(ConnectionState::Connecting.label(), "Connecting");
    assert_eq!(ConnectionState::Connected.label(), "Connected");
    assert_eq!(
        ConnectionState::Failed("timeout".into()).label(),
        "Error: timeout"
    );
}

#[test]
fn also_in_defaults_empty_and_source_file_name() {
    let info = ClusterInfo::new("a", Some(PathBuf::from("/x/config.bak")));
    assert!(info.also_in.is_empty());
    assert_eq!(info.source_file_name().as_deref(), Some("config.bak"));
    assert_eq!(ClusterInfo::from_context("a").source_file_name(), None);
}
