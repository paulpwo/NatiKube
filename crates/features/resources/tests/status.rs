use natikube_resources::{
    ContainerFacts, ContainerStateKind, ContainerStatusFacts, InitContainerFacts, PodFacts, PodRow,
    StatusTone, status_tone, summarize,
};

fn running(ready: bool, restarts: u32) -> Option<ContainerStatusFacts> {
    Some(ContainerStatusFacts {
        ready,
        started: true,
        restart_count: restarts,
        state: Some(ContainerStateKind::Running),
    })
}

fn waiting(reason: &str, restarts: u32) -> Option<ContainerStatusFacts> {
    Some(ContainerStatusFacts {
        ready: false,
        started: false,
        restart_count: restarts,
        state: Some(ContainerStateKind::Waiting {
            reason: Some(reason.to_string()),
        }),
    })
}

fn terminated(reason: Option<&str>, exit_code: i32, signal: i32) -> Option<ContainerStatusFacts> {
    Some(ContainerStatusFacts {
        ready: false,
        started: false,
        restart_count: 0,
        state: Some(ContainerStateKind::Terminated {
            reason: reason.map(str::to_string),
            exit_code,
            signal,
        }),
    })
}

fn container(name: &str, status: Option<ContainerStatusFacts>) -> ContainerFacts {
    ContainerFacts {
        name: name.to_string(),
        status,
    }
}

fn pod(phase: &str, containers: Vec<ContainerFacts>) -> PodFacts {
    PodFacts {
        name: "web-1".to_string(),
        namespace: "default".to_string(),
        phase: Some(phase.to_string()),
        initialized: true,
        ready_condition: true,
        containers,
        ..PodFacts::default()
    }
}

#[test]
fn running_pod_counts_ready_containers_and_restarts() {
    let facts = pod(
        "Running",
        vec![
            container("a", running(true, 2)),
            container("b", running(false, 1)),
        ],
    );
    let summary = summarize(&facts);
    assert_eq!(summary.status, "Running");
    assert_eq!(summary.restarts, 3);
    assert_eq!(summary.containers.len(), 2);
    assert_eq!(summary.containers.iter().filter(|c| c.ready).count(), 1);
}

#[test]
fn pending_pod_without_statuses_keeps_phase_and_spec_total() {
    let facts = pod("Pending", vec![container("a", None), container("b", None)]);
    let row = PodRow::from_facts(facts);
    assert_eq!(row.status, "Pending");
    assert_eq!(row.containers_label(), "0/2");
    assert_eq!(row.restarts, 0);
}

#[test]
fn crash_loop_waiting_reason_wins_over_phase() {
    let facts = pod(
        "Running",
        vec![container("a", waiting("CrashLoopBackOff", 7))],
    );
    let summary = summarize(&facts);
    assert_eq!(summary.status, "CrashLoopBackOff");
    assert_eq!(summary.restarts, 7);
}

#[test]
fn completed_pod_uses_terminated_reason() {
    let facts = pod(
        "Succeeded",
        vec![container("a", terminated(Some("Completed"), 0, 0))],
    );
    assert_eq!(summarize(&facts).status, "Completed");
}

#[test]
fn terminated_without_reason_reports_exit_code_or_signal() {
    let exit = pod("Failed", vec![container("a", terminated(None, 137, 0))]);
    assert_eq!(summarize(&exit).status, "ExitCode:137");
    let signal = pod("Failed", vec![container("a", terminated(None, 0, 9))]);
    assert_eq!(summarize(&signal).status, "Signal:9");
}

#[test]
fn completed_with_running_container_falls_back_to_running_or_not_ready() {
    let mut facts = pod(
        "Running",
        vec![
            container("a", terminated(Some("Completed"), 0, 0)),
            container("b", running(true, 0)),
        ],
    );
    // kubectl walks containers from last to first, so the first container's
    // reason is the one left standing.
    assert_eq!(summarize(&facts).status, "Running");
    facts.ready_condition = false;
    assert_eq!(summarize(&facts).status, "NotReady");
}

#[test]
fn pod_reason_overrides_phase() {
    let mut facts = pod("Failed", vec![container("a", None)]);
    facts.reason = Some("Evicted".to_string());
    assert_eq!(summarize(&facts).status, "Evicted");
}

#[test]
fn scheduling_gated_pod() {
    let mut facts = pod("Pending", vec![container("a", None)]);
    facts.scheduling_gated = true;
    assert_eq!(summarize(&facts).status, "SchedulingGated");
}

#[test]
fn deleting_pod_is_terminating_unless_terminal_phase() {
    let mut facts = pod("Running", vec![container("a", running(true, 0))]);
    facts.deleting = true;
    assert_eq!(summarize(&facts).status, "Terminating");

    let mut done = pod(
        "Succeeded",
        vec![container("a", terminated(Some("Completed"), 0, 0))],
    );
    done.deleting = true;
    assert_eq!(summarize(&done).status, "Completed");
}

#[test]
fn deleting_pod_on_lost_node_is_unknown() {
    let mut facts = pod("Running", vec![container("a", running(true, 0))]);
    facts.deleting = true;
    facts.reason = Some("NodeLost".to_string());
    assert_eq!(summarize(&facts).status, "Unknown");
}

#[test]
fn init_container_progress_is_reported() {
    let mut facts = pod("Pending", vec![container("app", None)]);
    facts.initialized = false;
    facts.init_containers = vec![
        InitContainerFacts {
            name: "migrate".to_string(),
            restartable: false,
            status: terminated(Some("Completed"), 0, 0),
        },
        InitContainerFacts {
            name: "seed".to_string(),
            restartable: false,
            status: running(false, 1),
        },
    ];
    let summary = summarize(&facts);
    assert_eq!(summary.status, "Init:1/2");
    assert_eq!(summary.restarts, 1);
}

#[test]
fn failing_init_container_reports_init_reason() {
    let mut facts = pod("Pending", vec![container("app", None)]);
    facts.initialized = false;
    facts.init_containers = vec![InitContainerFacts {
        name: "migrate".to_string(),
        restartable: false,
        status: waiting("CrashLoopBackOff", 4),
    }];
    assert_eq!(summarize(&facts).status, "Init:CrashLoopBackOff");

    facts.init_containers[0].status = terminated(None, 2, 0);
    assert_eq!(summarize(&facts).status, "Init:ExitCode:2");
}

#[test]
fn restartable_sidecar_counts_in_total_and_ready() {
    let mut facts = pod("Running", vec![container("app", running(true, 1))]);
    facts.init_containers = vec![InitContainerFacts {
        name: "proxy".to_string(),
        restartable: true,
        status: running(true, 2),
    }];
    let row = PodRow::from_facts(facts);
    assert_eq!(row.status, "Running");
    assert_eq!(row.containers_label(), "2/2");
    assert_eq!(row.restarts, 3);
}

#[test]
fn from_facts_keeps_identity_owner_and_creation() {
    let mut facts = pod("Running", vec![container("a", running(true, 0))]);
    facts.controlled_by = Some("DaemonSet".to_string());
    facts.created_at = Some(42);
    let row = PodRow::from_facts(facts);
    assert_eq!(row.name, "web-1");
    assert_eq!(row.namespace, "default");
    assert_eq!(row.controlled_by.as_deref(), Some("DaemonSet"));
    assert_eq!(row.created_at, Some(42));
    assert_eq!(row.age(42 + 3 * 86_400), "3d");
}

#[test]
fn status_tone_classifies_for_display() {
    assert_eq!(status_tone("Running"), StatusTone::Ok);
    assert_eq!(status_tone("Completed"), StatusTone::Done);
    assert_eq!(status_tone("Succeeded"), StatusTone::Done);
    assert_eq!(status_tone("Pending"), StatusTone::Warning);
    assert_eq!(status_tone("Terminating"), StatusTone::Warning);
    assert_eq!(status_tone("Init:0/1"), StatusTone::Warning);
    assert_eq!(status_tone("ContainerCreating"), StatusTone::Warning);
    assert_eq!(status_tone("CrashLoopBackOff"), StatusTone::Error);
    assert_eq!(status_tone("Init:CrashLoopBackOff"), StatusTone::Error);
    assert_eq!(status_tone("Failed"), StatusTone::Error);
    assert_eq!(status_tone("ExitCode:1"), StatusTone::Error);
    assert_eq!(status_tone("Evicted"), StatusTone::Error);
}
