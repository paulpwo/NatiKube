//! F4 — Logs (vertical slice).
//!
//! * `domain`: pure log buffer, search and line formatting.
//! * `adapters`: kube pod log streaming.
//! * `ui`: [`LogsView`], one dock tab per (pod, container).

pub mod adapters;
pub mod domain;
pub mod ui;

pub use ui::LogsView;
