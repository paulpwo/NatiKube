//! Header bell with unread badge and the history panel (task A4).
//!
//! * `view_model`: pure helpers (badge text, relative time, day grouping,
//!   navigation target); unit-tested.
//! * `view`: the [`AlertBell`] GPUI entity.
//! * `fake`: dev-only sample history for `--fake-alerts`.

mod fake;
mod view;
mod view_model;

pub use fake::{fake_alerts, fake_store, seed_store};
pub use view::{AlertBell, BellEvent, PanelSide};
pub use view_model::{
    BellTarget, Bucket, Group, PAGE_SIZE, badge_text, bucket, group_by_day, relative_time, subject,
};
