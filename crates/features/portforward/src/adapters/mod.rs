//! Kube adapter: listener that bridges local TCP connections to a pod/service port.

mod bridge;
mod kube_forward;

pub use bridge::{AbortOnDrop, Bound, serve_listener};
pub use kube_forward::{ActiveForward, start_forward};
