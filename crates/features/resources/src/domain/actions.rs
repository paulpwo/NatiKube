use natikube_core::{AppCommand, ClusterId, ResourceKind, ResourceRef, kinds};

use super::object::ObjectRow;

/// One entry of a row's action menu and the command it emits.
#[derive(Debug, Clone, PartialEq)]
pub struct RowAction {
    pub label: String,
    pub command: AppCommand,
}

/// The reference to the object a row stands for.
pub fn row_ref(cluster: &ClusterId, kind: &ResourceKind, row: &ObjectRow) -> ResourceRef {
    ResourceRef::new(
        cluster.clone(),
        kind.clone(),
        row.namespace.as_deref().map(str::to_string),
        row.name.clone(),
    )
}

/// Actions for a row: details for everything, logs and shell for pods.
pub fn row_actions(cluster: &ClusterId, kind: &ResourceKind, row: &ObjectRow) -> Vec<RowAction> {
    let target = row_ref(cluster, kind, row);
    let mut actions = vec![RowAction {
        label: "Details".into(),
        command: AppCommand::OpenDetail(target.clone()),
    }];
    if *kind == kinds::POD {
        actions.push(RowAction {
            label: "Logs".into(),
            command: AppCommand::OpenLogs {
                pod: target.clone(),
                container: None,
            },
        });
        actions.push(RowAction {
            label: "Shell".into(),
            command: AppCommand::OpenExec {
                pod: target,
                container: None,
            },
        });
    }
    actions
}
