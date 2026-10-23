//! I/O adapters: a local PTY and a kube websocket exec session. Both speak the
//! same [`Backend`] interface so the view does not care which one it drives.

mod local_pty;
mod pod_exec;

use futures::channel::mpsc::UnboundedReceiver;

use crate::domain::{BackendEvent, PtySize};

pub use local_pty::{CommandSpec, LocalPty, login_shell_spec, login_shell_spec_for};
pub use pod_exec::{PodExec, SHELL_CANDIDATES, ShellOutcome, next_shell_step};

/// Receiving end of a session's output.
pub type BackendEvents = UnboundedReceiver<BackendEvent>;

/// Write side of a terminal session. Dropping the backend ends the session
/// (kills the child process or closes the websocket).
pub trait Backend {
    /// Sends keyboard/paste bytes to the program. Never blocks.
    fn write(&self, bytes: Vec<u8>);
    /// Informs the program of the new terminal size.
    fn resize(&self, size: PtySize);
}
