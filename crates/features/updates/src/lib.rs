//! Update check (vertical slice).
//!
//! * `domain`: pure release model, newest-release selection, semver
//!   comparison, skip rule and the "should we check" rule.
//! * `adapters`: one read-only GitHub Releases request (hyper + rustls, 5 s
//!   timeout) parsed into the domain model.
//! * `ui`: [`UpdateModal`], the dismissible "update available" overlay.
//!
//! Contract: the shell decides with [`should_check`], runs [`fetch_releases`]
//! on the shared runtime, picks the offer with [`available_update`], calls
//! [`UpdateModal::show`] and acts on the [`UpdateEvent`] it emits. Failures
//! are silent: nothing is shown and only a debug line is logged.

pub mod adapters;
pub mod domain;
pub mod ui;

pub use adapters::{FetchError, RELEASES_URL, fetch_releases, parse_releases, user_agent};
pub use domain::{
    CheckContext, ENV_DISABLE, Release, UpdateOffer, available_update, newest_release,
    notes_excerpt, parse_tag, should_check,
};
pub use ui::{UpdateEvent, UpdateModal};
