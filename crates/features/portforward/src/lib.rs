//! F6 — Port-forward (vertical slice).
//!
//! * `domain`: pure forward registry (specs, states, ports).
//! * `adapters`: local TCP listener bridged to the kube portforward API.
//! * `ui`: [`PortForwardsView`], the dock tab listing active forwards.

pub mod adapters;
pub mod domain;
pub mod ui;

pub use domain::{ForwardEntry, ForwardRegistry, ForwardState};
pub use ui::PortForwardsView;
