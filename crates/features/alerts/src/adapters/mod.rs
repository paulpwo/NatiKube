//! Adapters: SQLite history, kube monitor, OS notifications.

pub mod monitor;
pub mod notify;
pub mod store;

pub use store::{DB_FILE_NAME, LATEST_SCHEMA_VERSION, SqliteAlertStore, StoreSchemaVersion};
