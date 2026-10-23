use super::pod::{ContainerState, ContainerStateKind, PodFacts};

/// Derived, display-ready pod values.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PodSummary {
    pub status: String,
    pub restarts: u32,
    pub containers: Vec<ContainerState>,
}

/// Reason kubelet sets on pods whose node stopped reporting.
const NODE_LOST_REASON: &str = "NodeLost";

/// Derives the STATUS, RESTARTS and READY values exactly like `kubectl get
/// pods` (`printPod` in `k8s.io/kubernetes/pkg/printers/internalversion`).
pub fn summarize(facts: &PodFacts) -> PodSummary {
    let mut reason = facts
        .reason
        .clone()
        .filter(|r| !r.is_empty())
        .or_else(|| facts.phase.clone())
        .unwrap_or_default();
    if facts.scheduling_gated {
        reason = "SchedulingGated".to_string();
    }

    let mut containers: Vec<ContainerState> = facts
        .init_containers
        .iter()
        .filter(|init| init.restartable)
        .map(|init| ContainerState {
            name: init.name.clone(),
            ready: false,
            restart_count: init.status.as_ref().map_or(0, |s| s.restart_count),
            state: init.status.as_ref().and_then(|s| s.state.clone()),
            sidecar: true,
        })
        .collect();

    let mut restarts: u32 = 0;
    let mut sidecar_restarts: u32 = 0;
    let mut initializing = false;
    let init_total = facts.init_containers.len();
    let mut sidecar_ix = 0;
    for (i, init) in facts.init_containers.iter().enumerate() {
        let this_sidecar = init.restartable.then(|| {
            sidecar_ix += 1;
            sidecar_ix - 1
        });
        let Some(status) = &init.status else {
            continue;
        };
        restarts += status.restart_count;
        if init.restartable {
            sidecar_restarts += status.restart_count;
        }
        if let Some(ix) = this_sidecar
            && status.started
        {
            if status.ready {
                containers[ix].ready = true;
            }
            continue;
        }
        match &status.state {
            Some(ContainerStateKind::Terminated { exit_code: 0, .. }) => continue,
            Some(ContainerStateKind::Terminated {
                reason: why,
                exit_code,
                signal,
            }) => {
                reason = match why.as_deref().filter(|r| !r.is_empty()) {
                    Some(r) => format!("Init:{r}"),
                    None if *signal != 0 => format!("Init:Signal:{signal}"),
                    None => format!("Init:ExitCode:{exit_code}"),
                };
            }
            Some(ContainerStateKind::Waiting { reason: Some(r) })
                if !r.is_empty() && r != "PodInitializing" =>
            {
                reason = format!("Init:{r}");
            }
            _ => reason = format!("Init:{i}/{init_total}"),
        }
        initializing = true;
        break;
    }

    let mut regular: Vec<ContainerState> = facts
        .containers
        .iter()
        .map(|c| ContainerState {
            name: c.name.clone(),
            ready: false,
            restart_count: c.status.as_ref().map_or(0, |s| s.restart_count),
            state: c.status.as_ref().and_then(|s| s.state.clone()),
            sidecar: false,
        })
        .collect();

    if !initializing || facts.initialized {
        restarts = sidecar_restarts;
        let mut has_running = false;
        for (ix, container) in facts.containers.iter().enumerate().rev() {
            let Some(status) = &container.status else {
                continue;
            };
            restarts += status.restart_count;
            match &status.state {
                Some(ContainerStateKind::Waiting { reason: Some(r) }) if !r.is_empty() => {
                    reason = r.clone();
                }
                Some(ContainerStateKind::Terminated {
                    reason: Some(r), ..
                }) if !r.is_empty() => {
                    reason = r.clone();
                }
                Some(ContainerStateKind::Terminated {
                    exit_code, signal, ..
                }) => {
                    reason = if *signal != 0 {
                        format!("Signal:{signal}")
                    } else {
                        format!("ExitCode:{exit_code}")
                    };
                }
                Some(ContainerStateKind::Running) if status.ready => {
                    has_running = true;
                    regular[ix].ready = true;
                }
                _ => {}
            }
        }
        if reason == "Completed" && has_running {
            reason = if facts.ready_condition {
                "Running".to_string()
            } else {
                "NotReady".to_string()
            };
        }
    }

    let terminal_phase = matches!(facts.phase.as_deref(), Some("Succeeded" | "Failed"));
    if facts.deleting && facts.reason.as_deref() == Some(NODE_LOST_REASON) {
        reason = "Unknown".to_string();
    } else if facts.deleting && !terminal_phase {
        reason = "Terminating".to_string();
    }

    containers.append(&mut regular);
    PodSummary {
        status: reason,
        restarts,
        containers,
    }
}

/// Visual category of a STATUS value, for coloring.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusTone {
    /// Healthy and running.
    Ok,
    /// Finished successfully.
    Done,
    /// Transitional: pending, creating, initializing, terminating.
    Warning,
    /// Failed, crashing or evicted.
    Error,
}

/// Classifies a derived STATUS string.
pub fn status_tone(status: &str) -> StatusTone {
    let base = status.strip_prefix("Init:").unwrap_or(status);
    match base {
        "Running" => StatusTone::Ok,
        "Completed" | "Succeeded" => StatusTone::Done,
        "Pending" | "Terminating" | "ContainerCreating" | "PodInitializing" | "SchedulingGated"
        | "NotReady" | "Unknown" => StatusTone::Warning,
        _ if status.starts_with("Init:") && base.contains('/') => StatusTone::Warning,
        _ => StatusTone::Error,
    }
}
