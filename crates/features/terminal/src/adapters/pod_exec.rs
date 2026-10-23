//! Interactive shell inside a pod container over the Kubernetes exec
//! websocket (tty + stdin, resize through the terminal size channel).
//!
//! Safety: this only runs when the user opens an exec tab on a pod. It is
//! unit tested through its pure decision functions and never exercised
//! against a real cluster by the test suite.

use futures::SinkExt;
use futures::channel::mpsc::{UnboundedSender, unbounded};
use k8s_openapi::api::core::v1::Pod;
use k8s_openapi::apimachinery::pkg::apis::meta::v1::Status;
use kube::Api;
use kube::api::{AttachParams, TerminalSize};
use natikube_core::ResourceRef;
use natikube_kube_client::ClusterConnection;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::mpsc as tokio_mpsc;

use super::{Backend, BackendEvents};
use crate::domain::{BackendEvent, PtySize};

/// Shells tried in order. The first that starts and stays alive wins.
pub const SHELL_CANDIDATES: [&str; 2] = ["/bin/bash", "/bin/sh"];

/// How one shell attempt ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShellOutcome {
    /// The API refused or the websocket failed before a shell ran.
    StartFailed(String),
    /// The shell ran (or tried to) and the stream closed.
    Ended {
        output_seen: bool,
        success: bool,
        description: String,
    },
}

/// What to do after an attempt: try the next shell or report and stop.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    Retry(usize),
    /// The session ended normally (or by the user's own exit).
    Finish(String),
    /// No shell could be started; the text is an error for the user.
    Fail(String),
}

/// Fallback policy: a shell that never printed anything and failed (binary
/// missing, `executable file not found`) moves on to the next candidate; once
/// the user saw a prompt the session is reported as ended instead.
pub fn next_shell_step(index: usize, outcome: &ShellOutcome) -> Step {
    let has_next = index + 1 < SHELL_CANDIDATES.len();
    match outcome {
        ShellOutcome::StartFailed(error) => {
            if has_next {
                Step::Retry(index + 1)
            } else {
                Step::Fail(format!("Could not start a shell: {error}"))
            }
        }
        ShellOutcome::Ended {
            output_seen,
            success,
            description,
        } => {
            if !output_seen && !success && has_next {
                Step::Retry(index + 1)
            } else if !output_seen && !success {
                Step::Fail(format!(
                    "No shell available in the container ({description})"
                ))
            } else {
                Step::Finish(description.clone())
            }
        }
    }
}

/// Human text for the Status the API server sends when the command ends.
pub fn describe_status(status: Option<&Status>) -> (bool, String) {
    let Some(status) = status else {
        return (true, "Session closed".to_string());
    };
    if status.status.as_deref() == Some("Success") {
        return (true, "Session closed".to_string());
    }
    let exit_code = status
        .details
        .as_ref()
        .and_then(|d| d.causes.as_ref())
        .and_then(|causes| {
            causes
                .iter()
                .find(|c| c.reason.as_deref() == Some("ExitCode"))
                .and_then(|c| c.message.clone())
        });
    let text = match (exit_code, status.message.as_deref()) {
        (Some(code), _) => format!("Shell exited with code {code}"),
        (None, Some(message)) if !message.is_empty() => message.to_string(),
        _ => "Session closed".to_string(),
    };
    (false, text)
}

/// Exec parameters: a TTY with stdin; stderr is merged into the TTY stream.
pub fn attach_params(container: Option<&str>) -> AttachParams {
    let mut params = AttachParams::interactive_tty();
    if let Some(container) = container {
        params = params.container(container);
    }
    params
}

enum Input {
    Data(Vec<u8>),
    Resize(PtySize),
}

/// A running exec session. Dropping it aborts the task, which closes the
/// websocket.
pub struct PodExec {
    input: tokio_mpsc::UnboundedSender<Input>,
    task: tokio::task::JoinHandle<()>,
}

impl PodExec {
    /// Starts an exec session in `pod` (optionally a specific container).
    pub fn start(
        connection: &ClusterConnection,
        pod: &ResourceRef,
        container: Option<String>,
        size: PtySize,
    ) -> (Self, BackendEvents) {
        let client = connection.client().clone();
        let namespace = pod
            .namespace
            .clone()
            .unwrap_or_else(|| "default".to_string());
        let name = pod.name.clone();
        let (events_tx, events_rx) = unbounded();
        let (input_tx, input_rx) = tokio_mpsc::unbounded_channel();

        let task = natikube_kube_client::spawn(async move {
            let api: Api<Pod> = Api::namespaced(client, &namespace);
            run(api, name, container, size, input_rx, events_tx).await;
        });

        (
            Self {
                input: input_tx,
                task,
            },
            events_rx,
        )
    }
}

impl Backend for PodExec {
    fn write(&self, bytes: Vec<u8>) {
        let _ = self.input.send(Input::Data(bytes));
    }

    fn resize(&self, size: PtySize) {
        let _ = self.input.send(Input::Resize(size));
    }
}

impl Drop for PodExec {
    fn drop(&mut self) {
        self.task.abort();
    }
}

async fn run(
    api: Api<Pod>,
    pod: String,
    container: Option<String>,
    mut size: PtySize,
    mut input: tokio_mpsc::UnboundedReceiver<Input>,
    events: UnboundedSender<BackendEvent>,
) {
    let mut index = 0;
    loop {
        let shell = SHELL_CANDIDATES[index];
        let outcome = attempt(
            &api,
            &pod,
            shell,
            container.as_deref(),
            &mut size,
            &mut input,
            &events,
        )
        .await;
        let Some(outcome) = outcome else {
            // Input side closed: the view is gone.
            return;
        };
        match next_shell_step(index, &outcome) {
            Step::Retry(next) => index = next,
            Step::Finish(reason) => {
                let _ = events.unbounded_send(BackendEvent::Exited(reason));
                return;
            }
            Step::Fail(reason) => {
                let _ = events.unbounded_send(BackendEvent::Failed(reason));
                return;
            }
        }
    }
}

/// Runs one shell until its stream ends. `None` means the view went away.
async fn attempt(
    api: &Api<Pod>,
    pod: &str,
    shell: &str,
    container: Option<&str>,
    size: &mut PtySize,
    input: &mut tokio_mpsc::UnboundedReceiver<Input>,
    events: &UnboundedSender<BackendEvent>,
) -> Option<ShellOutcome> {
    let params = attach_params(container);
    let mut attached = match api.exec(pod, vec![shell], &params).await {
        Ok(attached) => attached,
        Err(error) => return Some(ShellOutcome::StartFailed(error.to_string())),
    };
    let (Some(mut stdin), Some(mut stdout)) = (attached.stdin(), attached.stdout()) else {
        return Some(ShellOutcome::StartFailed(
            "the API server did not open stdin/stdout".to_string(),
        ));
    };
    let status = attached.take_status();
    let mut resize_tx = attached.terminal_size();

    let send_size = |size: PtySize| TerminalSize {
        width: size.cols,
        height: size.rows,
    };
    if let Some(tx) = resize_tx.as_mut() {
        let _ = tx.send(send_size(*size)).await;
    }

    let mut buffer = vec![0u8; 16 * 1024];
    let mut output_seen = false;
    loop {
        tokio::select! {
            read = stdout.read(&mut buffer) => match read {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    output_seen = true;
                    if events
                        .unbounded_send(BackendEvent::Output(buffer[..n].to_vec()))
                        .is_err()
                    {
                        attached.abort();
                        return None;
                    }
                }
            },
            message = input.recv() => match message {
                None => {
                    attached.abort();
                    return None;
                }
                Some(Input::Data(bytes)) => {
                    if stdin.write_all(&bytes).await.is_err() || stdin.flush().await.is_err() {
                        break;
                    }
                }
                Some(Input::Resize(new_size)) => {
                    *size = new_size;
                    if let Some(tx) = resize_tx.as_mut() {
                        let _ = tx.send(send_size(new_size)).await;
                    }
                }
            },
        }
    }

    let status = match status {
        Some(future) => future.await,
        None => None,
    };
    let (success, description) = describe_status(status.as_ref());
    Some(ShellOutcome::Ended {
        output_seen,
        success,
        description,
    })
}

#[cfg(test)]
mod tests {
    use k8s_openapi::apimachinery::pkg::apis::meta::v1::{StatusCause, StatusDetails};

    use super::*;

    fn failure(code: &str) -> Status {
        Status {
            status: Some("Failure".into()),
            message: Some("command terminated with non-zero exit code".into()),
            details: Some(StatusDetails {
                causes: Some(vec![StatusCause {
                    reason: Some("ExitCode".into()),
                    message: Some(code.into()),
                    ..Default::default()
                }]),
                ..Default::default()
            }),
            ..Default::default()
        }
    }

    #[test]
    fn candidates_prefer_bash_then_sh() {
        assert_eq!(SHELL_CANDIDATES, ["/bin/bash", "/bin/sh"]);
    }

    #[test]
    fn missing_bash_falls_back_to_sh() {
        let missing = ShellOutcome::Ended {
            output_seen: false,
            success: false,
            description: "executable file not found".into(),
        };
        assert_eq!(next_shell_step(0, &missing), Step::Retry(1));
        assert!(matches!(next_shell_step(1, &missing), Step::Fail(m) if m.contains("No shell")));
    }

    #[test]
    fn start_failure_retries_then_reports() {
        let failed = ShellOutcome::StartFailed("forbidden".into());
        assert_eq!(next_shell_step(0, &failed), Step::Retry(1));
        assert_eq!(
            next_shell_step(1, &failed),
            Step::Fail("Could not start a shell: forbidden".into())
        );
    }

    #[test]
    fn a_shell_that_ran_is_not_retried() {
        let ran = ShellOutcome::Ended {
            output_seen: true,
            success: false,
            description: "Shell exited with code 2".into(),
        };
        assert_eq!(
            next_shell_step(0, &ran),
            Step::Finish("Shell exited with code 2".into())
        );
        let clean = ShellOutcome::Ended {
            output_seen: true,
            success: true,
            description: "Session closed".into(),
        };
        assert_eq!(
            next_shell_step(0, &clean),
            Step::Finish("Session closed".into())
        );
    }

    #[test]
    fn status_description_reads_exit_code() {
        assert_eq!(describe_status(None), (true, "Session closed".into()));
        assert_eq!(
            describe_status(Some(&failure("130"))),
            (false, "Shell exited with code 130".into())
        );
        let ok = Status {
            status: Some("Success".into()),
            ..Default::default()
        };
        assert_eq!(describe_status(Some(&ok)), (true, "Session closed".into()));
    }

    #[test]
    fn attach_params_request_tty_stdin_and_container() {
        let params = attach_params(Some("app"));
        assert!(params.tty && params.stdin && params.stdout && !params.stderr);
        assert_eq!(params.container.as_deref(), Some("app"));
        assert_eq!(attach_params(None).container, None);
    }
}
