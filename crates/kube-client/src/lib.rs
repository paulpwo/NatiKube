//! Cluster connectivity shared by every feature: a [`ClusterConnection`]
//! (kube client + shared Tokio runtime) and helpers that bridge Tokio streams
//! and tasks into GPUI. No UI code lives here.
//!
//! The runtime runs on background threads. GPUI tasks `await` the
//! [`tokio::task::JoinHandle`]s returned by [`spawn`] and
//! [`ClusterConnection::run`], or poll a [`StreamHandle`].

mod connection;
mod runtime;

pub use connection::{ClusterConnection, ConnectError, timing};
pub use runtime::{StreamHandle, runtime_handle, spawn, spawn_stream};
