//! F3 — Detail and YAML (vertical slice).
//!
//! * `domain`: pure view models (sections, fields, conditions, secret masking).
//! * `adapters`: kube get/apply/delete/scale/restart.
//! * `ui`: [`DetailPanel`], the resizable right panel owned by the shell.

pub mod adapters;
pub mod domain;
pub mod ui;

pub use ui::DetailPanel;
