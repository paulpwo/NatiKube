use natikube_core::{AppCommand, Appearance, ClusterId, ClusterInfo, ResourceRef, ViewId, kinds};
use natikube_palette::PaletteContext;
use natikube_palette::domain::{build_items, rank, score};

fn ctx() -> PaletteContext {
    let mut a = ClusterInfo::from_context("dev-cluster");
    a.kubeconfig = Some("/x/dev.yaml".into());
    let b = ClusterInfo::from_context("eks-prod");
    PaletteContext {
        clusters: vec![a, b],
        active_cluster: Some(ClusterId::new("dev-cluster")),
        kinds: vec![kinds::POD, kinds::DEPLOYMENT],
        resources: vec![ResourceRef::new(
            ClusterId::new("dev-cluster"),
            kinds::POD,
            Some("default".into()),
            "api-server-7d9",
        )],
    }
}

#[test]
fn score_requires_subsequence_and_ignores_case() {
    assert!(score("dpl", "Deployments").is_some());
    assert!(score("DEP", "deployments").is_some());
    assert!(score("zzz", "deployments").is_none());
    assert!(score("", "anything").is_some());
}

#[test]
fn prefix_and_consecutive_beat_scattered() {
    let prefix = score("pod", "Pods").unwrap();
    let scattered = score("pod", "Persistent Volume Claims, d").unwrap();
    assert!(prefix > scattered);
    let word_start = score("sw", "Switch Wheel").unwrap();
    let mid = score("sw", "ahsxwy").unwrap();
    assert!(word_start > mid);
}

#[test]
fn builds_every_command_family() {
    let items = build_items(&ctx());
    for a in Appearance::all() {
        assert!(
            items
                .iter()
                .any(|i| i.command == AppCommand::SetAppearance(a))
        );
    }
    assert!(
        items
            .iter()
            .any(|i| i.command == AppCommand::ImportFreelens)
    );
    assert!(items.iter().any(|i| i.command == AppCommand::AddKubeconfig));
    assert!(
        items
            .iter()
            .any(|i| i.command == AppCommand::SelectCluster(ClusterId::new("eks-prod")))
    );
    assert!(
        items
            .iter()
            .any(|i| i.command == AppCommand::Navigate(ViewId::Resource(kinds::POD)))
    );
    assert!(
        items
            .iter()
            .any(|i| matches!(&i.command, AppCommand::OpenDetail(r) if r.name == "api-server-7d9"))
    );
}

#[test]
fn empty_query_hides_resources() {
    let items = rank(&build_items(&ctx()), "", 100);
    assert!(
        !items
            .iter()
            .any(|i| matches!(i.command, AppCommand::OpenDetail(_)))
    );
    assert!(
        items
            .iter()
            .any(|i| i.command == AppCommand::ImportFreelens)
    );
}

#[test]
fn query_finds_resources_and_ranks_best_first() {
    let items = rank(&build_items(&ctx()), "api-srv", 10);
    assert!(matches!(items[0].command, AppCommand::OpenDetail(_)));
    let items = rank(&build_items(&ctx()), "freelens", 10);
    assert_eq!(items[0].command, AppCommand::ImportFreelens);
}

#[test]
fn limit_is_respected_and_ties_keep_order() {
    let items = rank(&build_items(&ctx()), "", 3);
    assert_eq!(items.len(), 3);
    let all = rank(&build_items(&ctx()), "", 100);
    assert_eq!(&all[..3], &items[..]);
}

#[test]
fn no_match_is_empty() {
    assert!(rank(&build_items(&ctx()), "qqqqzz", 10).is_empty());
}
