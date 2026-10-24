use natikube_core::Retention;
use thiserror::Error;

use super::model::{Alert, StoredAlert, Timestamp};

#[derive(Debug, Error, Clone, PartialEq, Eq)]
#[error("alert store: {0}")]
pub struct StoreError(pub String);

#[derive(Debug, Error, Clone, PartialEq, Eq)]
#[error("os notification: {0}")]
pub struct NotifyError(pub String);

/// Port: the notification history.
pub trait AlertStore: Send + Sync {
    /// Appends an alert (unread) and returns its row id.
    fn insert(&self, alert: &Alert) -> Result<i64, StoreError>;
    /// Newest first (by timestamp, then insertion), `limit` rows after `offset`.
    fn list(&self, limit: u32, offset: u32) -> Result<Vec<StoredAlert>, StoreError>;
    fn total_count(&self) -> Result<u32, StoreError>;
    fn unread_count(&self) -> Result<u32, StoreError>;
    fn mark_read(&self, row_id: i64) -> Result<(), StoreError>;
    fn mark_all_read(&self) -> Result<(), StoreError>;
    /// Deletes the whole history.
    fn clear(&self) -> Result<(), StoreError>;
    /// Deletes rows older than the retention age and rows beyond the maximum
    /// (oldest first). Returns how many were removed.
    fn prune(&self, now: Timestamp, retention: &Retention) -> Result<u32, StoreError>;
}

/// Port: operating system notifications.
pub trait OsNotifier: Send + Sync {
    fn notify(&self, alert: &Alert) -> Result<(), NotifyError>;
    /// "Send test notification" button.
    fn send_test(&self) -> Result<(), NotifyError>;
}
