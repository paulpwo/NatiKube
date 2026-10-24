//! Adapters: SQLite history, kube monitor, OS notifications, system sounds.

pub mod monitor;
pub mod notify;
pub mod sound;
pub mod store;

pub use store::{DB_FILE_NAME, LATEST_SCHEMA_VERSION, SqliteAlertStore, StoreSchemaVersion};
