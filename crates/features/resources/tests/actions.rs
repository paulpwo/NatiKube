use natikube_core::{AppCommand, ClusterId, ResourceRef, kinds};
use natikube_resources::{ObjectRow, row_actions, row_ref};

#[test]
fn pod_rows_offer_details_logs_and_shell() {
    let cluster = ClusterId::new("c1");
    let row = ObjectRow::new(Some("default".into()), "api-1");
    let actions = row_actions(&cluster, &kinds::POD, &row);
    let labels: Vec<_> = actions.iter().map(|a| a.label.as_str()).collect();
    assert_eq!(labels, ["Details", "Logs", "Shell"]);
    let pod = ResourceRef::new(cluster.clone(), kinds::POD, Some("default".into()), "api-1");
    assert_eq!(actions[0].command, AppCommand::OpenDetail(pod.clone()));
    assert_eq!(
        actions[1].command,
        AppCommand::OpenLogs {
            pod: pod.clone(),
            container: None
        }
    );
    assert_eq!(
        actions[2].command,
        AppCommand::OpenExec {
            pod,
            container: None
        }
    );
}

#[test]
fn other_kinds_only_offer_details() {
    let cluster = ClusterId::new("c1");
    let row = ObjectRow::new(None, "n1");
    let actions = row_actions(&cluster, &kinds::NODE, &row);
    assert_eq!(actions.len(), 1);
    assert_eq!(
        actions[0].command,
        AppCommand::OpenDetail(row_ref(&cluster, &kinds::NODE, &row))
    );
}
