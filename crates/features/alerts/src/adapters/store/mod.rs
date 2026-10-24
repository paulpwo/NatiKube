//! SQLite-backed notification history.

mod sqlite;

pub use sqlite::{DB_FILE_NAME, LATEST_SCHEMA_VERSION, SqliteAlertStore, StoreSchemaVersion};
