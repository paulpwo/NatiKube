//! Kube adapters: watch one object, events, server-side dry-run + apply, delete, scale, rollout restart.

pub mod requests;
pub mod transport;
pub mod watch;
pub mod workflow;
