//! The application shell: composes the feature views into the Freelens-like
//! window (cluster rail, sidebar, top tabs, main content, right detail panel,
//! bottom dock) and routes [`AppCommand`](natikube_core::AppCommand)s.
//!
//! Features never depend on each other; they emit `AppCommand`s as GPUI events
//! and the shell reacts. See `docs/architecture.md`.

mod app;
mod chrome;
mod dock;
mod dock_icon;
mod layout_command;
mod layout_native;
mod layout_observatory;
pub mod menus;
pub mod nav_layout;
mod settings_ops;
mod shell_ops;
mod shell_view;
mod sidebar;
pub mod state;
mod update_ops;

pub use app::{ShellOptions, run};
pub use shell_view::ShellView;
pub use state::{DockState, NavState};
