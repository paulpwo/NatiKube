use std::path::{Path, PathBuf};
use std::sync::Mutex;

use natikube_core::{ClusterId, Retention, RuleKind};
use rusqlite::{Connection, params};

use crate::domain::{Alert, AlertStore, Severity, StoreError, StoredAlert, Timestamp};

/// File name inside the NatiKube config dir.
pub const DB_FILE_NAME: &str = "notifications.sqlite";

/// Highest schema version this build knows.
pub const LATEST_SCHEMA_VERSION: StoreSchemaVersion = 1;

pub type StoreSchemaVersion = u32;

/// Ordered migrations; index + 1 is the version they bring the database to.
const MIGRATIONS: &[&str] = &["CREATE TABLE alerts (
        row_id INTEGER PRIMARY KEY AUTOINCREMENT,
        id TEXT NOT NULL,
        cluster TEXT NOT NULL,
        rule TEXT NOT NULL,
        severity TEXT NOT NULL,
        namespace TEXT,
        object_kind TEXT,
        object_name TEXT,
        message TEXT NOT NULL,
        timestamp INTEGER NOT NULL,
        dedup_key TEXT NOT NULL,
        read INTEGER NOT NULL DEFAULT 0
    );
    CREATE INDEX alerts_timestamp ON alerts (timestamp DESC, row_id DESC);
    CREATE INDEX alerts_read ON alerts (read);"];

fn err(error: rusqlite::Error) -> StoreError {
    StoreError(error.to_string())
}

/// The notification history in one SQLite file.
pub struct SqliteAlertStore {
    conn: Mutex<Connection>,
    path: Option<PathBuf>,
}

impl SqliteAlertStore {
    /// Opens (creating folders, file and schema as needed) the database at
    /// `path`.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, StoreError> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| StoreError(e.to_string()))?;
        }
        let conn = Connection::open(path).map_err(err)?;
        Self::init(conn, Some(path.to_path_buf()))
    }

    /// `notifications.sqlite` inside the NatiKube config dir.
    pub fn in_config_dir(dir: impl AsRef<Path>) -> Result<Self, StoreError> {
        Self::open(dir.as_ref().join(DB_FILE_NAME))
    }

    /// A throwaway database (tests, previews).
    pub fn open_in_memory() -> Result<Self, StoreError> {
        Self::init(Connection::open_in_memory().map_err(err)?, None)
    }

    fn init(conn: Connection, path: Option<PathBuf>) -> Result<Self, StoreError> {
        conn.busy_timeout(std::time::Duration::from_secs(2))
            .map_err(err)?;
        migrate(&conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
            path,
        })
    }

    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    /// Version recorded in the migrations table.
    pub fn schema_version(&self) -> Result<StoreSchemaVersion, StoreError> {
        current_version(&*self.lock()?)
    }

    fn lock(&self) -> Result<std::sync::MutexGuard<'_, Connection>, StoreError> {
        self.conn
            .lock()
            .map_err(|_| StoreError("database lock poisoned".into()))
    }
}

fn current_version(conn: &Connection) -> Result<StoreSchemaVersion, StoreError> {
    conn.query_row(
        "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
        [],
        |row| row.get(0),
    )
    .map_err(err)
}

fn migrate(conn: &Connection) -> Result<(), StoreError> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_migrations (
            version INTEGER PRIMARY KEY,
            applied_at INTEGER NOT NULL
        );",
    )
    .map_err(err)?;
    let current = current_version(conn)? as usize;
    if current > MIGRATIONS.len() {
        return Err(StoreError(format!(
            "history was written by a newer NatiKube (schema {current}, this build knows {})",
            MIGRATIONS.len()
        )));
    }
    for (index, sql) in MIGRATIONS.iter().enumerate().skip(current) {
        let tx = conn.unchecked_transaction().map_err(err)?;
        tx.execute_batch(sql).map_err(err)?;
        tx.execute(
            "INSERT INTO schema_migrations (version, applied_at) VALUES (?1, strftime('%s','now'))",
            params![index as i64 + 1],
        )
        .map_err(err)?;
        tx.commit().map_err(err)?;
    }
    Ok(())
}

fn row_to_alert(row: &rusqlite::Row<'_>) -> rusqlite::Result<Option<StoredAlert>> {
    let rule: String = row.get("rule")?;
    let severity: String = row.get("severity")?;
    let (Some(rule), Some(severity)) =
        (RuleKind::from_key(&rule), Severity::from_str_key(&severity))
    else {
        // Written by a newer build with a rule this one does not know.
        return Ok(None);
    };
    Ok(Some(StoredAlert {
        row_id: row.get("row_id")?,
        read: row.get::<_, i64>("read")? != 0,
        alert: Alert {
            id: row.get("id")?,
            cluster: ClusterId::new(row.get::<_, String>("cluster")?),
            rule,
            severity,
            namespace: row.get("namespace")?,
            object_kind: row.get("object_kind")?,
            object_name: row.get("object_name")?,
            message: row.get("message")?,
            timestamp: row.get("timestamp")?,
            dedup_key: row.get("dedup_key")?,
        },
    }))
}

impl AlertStore for SqliteAlertStore {
    fn insert(&self, alert: &Alert) -> Result<i64, StoreError> {
        let conn = self.lock()?;
        conn.execute(
            "INSERT INTO alerts (id, cluster, rule, severity, namespace, object_kind, object_name,
                                 message, timestamp, dedup_key, read)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, 0)",
            params![
                alert.id,
                alert.cluster.as_str(),
                alert.rule.key(),
                alert.severity.as_str(),
                alert.namespace,
                alert.object_kind,
                alert.object_name,
                alert.message,
                alert.timestamp,
                alert.dedup_key,
            ],
        )
        .map_err(err)?;
        Ok(conn.last_insert_rowid())
    }

    fn list(&self, limit: u32, offset: u32) -> Result<Vec<StoredAlert>, StoreError> {
        let conn = self.lock()?;
        let mut statement = conn
            .prepare("SELECT * FROM alerts ORDER BY timestamp DESC, row_id DESC LIMIT ?1 OFFSET ?2")
            .map_err(err)?;
        let rows = statement
            .query_map(params![limit, offset], row_to_alert)
            .map_err(err)?;
        let mut out = Vec::new();
        for row in rows {
            if let Some(alert) = row.map_err(err)? {
                out.push(alert);
            }
        }
        Ok(out)
    }

    fn total_count(&self) -> Result<u32, StoreError> {
        self.lock()?
            .query_row("SELECT COUNT(*) FROM alerts", [], |row| row.get(0))
            .map_err(err)
    }

    fn unread_count(&self) -> Result<u32, StoreError> {
        self.lock()?
            .query_row("SELECT COUNT(*) FROM alerts WHERE read = 0", [], |row| {
                row.get(0)
            })
            .map_err(err)
    }

    fn mark_read(&self, row_id: i64) -> Result<(), StoreError> {
        self.lock()?
            .execute(
                "UPDATE alerts SET read = 1 WHERE row_id = ?1",
                params![row_id],
            )
            .map(|_| ())
            .map_err(err)
    }

    fn mark_all_read(&self) -> Result<(), StoreError> {
        self.lock()?
            .execute("UPDATE alerts SET read = 1 WHERE read = 0", [])
            .map(|_| ())
            .map_err(err)
    }

    fn clear(&self) -> Result<(), StoreError> {
        self.lock()?
            .execute("DELETE FROM alerts", [])
            .map(|_| ())
            .map_err(err)
    }

    fn prune(&self, now: Timestamp, retention: &Retention) -> Result<u32, StoreError> {
        let retention = retention.clone().clamped();
        let cutoff = now - i64::from(retention.max_age_days) * 86_400;
        let conn = self.lock()?;
        let by_age = conn
            .execute("DELETE FROM alerts WHERE timestamp < ?1", params![cutoff])
            .map_err(err)?;
        let by_count = conn
            .execute(
                "DELETE FROM alerts WHERE row_id NOT IN (
                    SELECT row_id FROM alerts ORDER BY timestamp DESC, row_id DESC LIMIT ?1)",
                params![retention.max_rows],
            )
            .map_err(err)?;
        Ok((by_age + by_count) as u32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn alert(n: i64, cluster: &str) -> Alert {
        Alert {
            id: format!("id-{n}"),
            cluster: ClusterId::new(cluster),
            rule: RuleKind::PodFailing,
            severity: Severity::Critical,
            namespace: Some("apps".into()),
            object_kind: Some("Pod".into()),
            object_name: Some(format!("pod-{n}")),
            message: format!("message {n}"),
            timestamp: n,
            dedup_key: format!("k{n}"),
        }
    }

    fn store() -> (tempfile::TempDir, SqliteAlertStore) {
        let dir = tempfile::tempdir().unwrap();
        let store = SqliteAlertStore::in_config_dir(dir.path()).unwrap();
        (dir, store)
    }

    #[test]
    fn creates_the_file_in_the_config_dir_with_the_latest_schema() {
        let (dir, store) = store();
        assert!(dir.path().join("notifications.sqlite").exists());
        assert_eq!(store.path(), Some(dir.path().join(DB_FILE_NAME).as_path()));
        assert_eq!(store.schema_version().unwrap(), LATEST_SCHEMA_VERSION);
    }

    #[test]
    fn creates_missing_parent_folders() {
        let dir = tempfile::tempdir().unwrap();
        let nested = dir.path().join("a").join("b");
        SqliteAlertStore::in_config_dir(&nested).unwrap();
        assert!(nested.join(DB_FILE_NAME).exists());
    }

    #[test]
    fn reopening_keeps_history_and_does_not_rerun_migrations() {
        let (dir, store) = store();
        store.insert(&alert(1, "c")).unwrap();
        drop(store);
        let again = SqliteAlertStore::in_config_dir(dir.path()).unwrap();
        assert_eq!(again.total_count().unwrap(), 1);
        assert_eq!(again.schema_version().unwrap(), LATEST_SCHEMA_VERSION);
    }

    #[test]
    fn a_newer_schema_is_refused_instead_of_damaged() {
        let (dir, store) = store();
        store
            .lock()
            .unwrap()
            .execute(
                "INSERT INTO schema_migrations (version, applied_at) VALUES (99, 0)",
                [],
            )
            .unwrap();
        drop(store);
        assert!(SqliteAlertStore::in_config_dir(dir.path()).is_err());
    }

    #[test]
    fn insert_round_trips_every_field_as_unread() {
        let (_dir, store) = store();
        let original = alert(5, "ctx");
        let row_id = store.insert(&original).unwrap();
        let rows = store.list(10, 0).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].row_id, row_id);
        assert!(!rows[0].read);
        assert_eq!(rows[0].alert, original);
    }

    #[test]
    fn optional_fields_round_trip_as_none() {
        let (_dir, store) = store();
        let mut a = alert(1, "c");
        a.namespace = None;
        a.object_kind = None;
        a.object_name = None;
        store.insert(&a).unwrap();
        assert_eq!(store.list(1, 0).unwrap()[0].alert, a);
    }

    #[test]
    fn list_is_newest_first_and_paged() {
        let (_dir, store) = store();
        for n in 1..=5 {
            store.insert(&alert(n, "c")).unwrap();
        }
        let first: Vec<i64> = store
            .list(2, 0)
            .unwrap()
            .iter()
            .map(|r| r.alert.timestamp)
            .collect();
        let second: Vec<i64> = store
            .list(2, 2)
            .unwrap()
            .iter()
            .map(|r| r.alert.timestamp)
            .collect();
        let last: Vec<i64> = store
            .list(2, 4)
            .unwrap()
            .iter()
            .map(|r| r.alert.timestamp)
            .collect();
        assert_eq!(first, vec![5, 4]);
        assert_eq!(second, vec![3, 2]);
        assert_eq!(last, vec![1]);
        assert!(store.list(2, 10).unwrap().is_empty());
    }

    #[test]
    fn equal_timestamps_keep_insertion_order_newest_first() {
        let (_dir, store) = store();
        let mut a = alert(1, "c");
        a.timestamp = 7;
        let mut b = alert(2, "c");
        b.timestamp = 7;
        store.insert(&a).unwrap();
        store.insert(&b).unwrap();
        let ids: Vec<String> = store
            .list(10, 0)
            .unwrap()
            .into_iter()
            .map(|r| r.alert.id)
            .collect();
        assert_eq!(ids, vec!["id-2", "id-1"]);
    }

    #[test]
    fn unread_count_and_mark_read() {
        let (_dir, store) = store();
        let a = store.insert(&alert(1, "c")).unwrap();
        store.insert(&alert(2, "c")).unwrap();
        store.insert(&alert(3, "c")).unwrap();
        assert_eq!(store.unread_count().unwrap(), 3);
        store.mark_read(a).unwrap();
        assert_eq!(store.unread_count().unwrap(), 2);
        let rows = store.list(10, 0).unwrap();
        assert!(rows.iter().find(|r| r.row_id == a).unwrap().read);
        store.mark_all_read().unwrap();
        assert_eq!(store.unread_count().unwrap(), 0);
        assert_eq!(store.total_count().unwrap(), 3, "marking read keeps rows");
    }

    #[test]
    fn marking_an_unknown_row_is_harmless() {
        let (_dir, store) = store();
        store.mark_read(12345).unwrap();
    }

    #[test]
    fn clear_deletes_everything() {
        let (_dir, store) = store();
        store.insert(&alert(1, "c")).unwrap();
        store.clear().unwrap();
        assert_eq!(store.total_count().unwrap(), 0);
        assert_eq!(store.unread_count().unwrap(), 0);
        assert!(store.list(10, 0).unwrap().is_empty());
    }

    #[test]
    fn prune_removes_rows_older_than_the_retention_age() {
        let (_dir, store) = store();
        let day = 86_400;
        store.insert(&alert(0, "c")).unwrap();
        store.insert(&alert(10 * day, "c")).unwrap();
        store.insert(&alert(39 * day, "c")).unwrap();
        let retention = Retention {
            max_age_days: 30,
            max_rows: 5_000,
        };
        let removed = store.prune(40 * day, &retention).unwrap();
        assert_eq!(removed, 1);
        let kept: Vec<i64> = store
            .list(10, 0)
            .unwrap()
            .iter()
            .map(|r| r.alert.timestamp)
            .collect();
        assert_eq!(kept, vec![39 * day, 10 * day]);
    }

    #[test]
    fn prune_caps_the_row_count_dropping_the_oldest() {
        let (_dir, store) = store();
        for n in 1..=150 {
            store.insert(&alert(n, "c")).unwrap();
        }
        let retention = Retention {
            max_age_days: 365,
            max_rows: 100,
        };
        assert_eq!(store.prune(200, &retention).unwrap(), 50);
        assert_eq!(store.total_count().unwrap(), 100);
        let rows = store.list(1, 99).unwrap();
        assert_eq!(rows[0].alert.timestamp, 51, "the 50 oldest are gone");
    }

    #[test]
    fn prune_with_nothing_to_remove_returns_zero() {
        let (_dir, store) = store();
        store.insert(&alert(100, "c")).unwrap();
        assert_eq!(store.prune(101, &Retention::default()).unwrap(), 0);
    }

    #[test]
    fn rows_of_unknown_rules_are_skipped_not_fatal() {
        let (_dir, store) = store();
        store.insert(&alert(1, "c")).unwrap();
        store
            .lock()
            .unwrap()
            .execute("UPDATE alerts SET rule = 'future_rule'", [])
            .unwrap();
        store.insert(&alert(2, "c")).unwrap();
        let rows = store.list(10, 0).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].alert.timestamp, 2);
    }

    #[test]
    fn in_memory_store_works() {
        let store = SqliteAlertStore::open_in_memory().unwrap();
        store.insert(&alert(1, "c")).unwrap();
        assert_eq!(store.total_count().unwrap(), 1);
        assert!(store.path().is_none());
    }
}
