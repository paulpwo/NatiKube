//! Opens the alert history for this run and decides whether the monitor may
//! start.

use std::path::Path;
use std::sync::Arc;

use natikube_alerts::ui::bell::seed_store;
use natikube_alerts::{AlertStore, SqliteAlertStore};
use natikube_clusters::FileSettingsStore;
use natikube_core::{ClusterId, ClusterInfo};
use natikube_shell::AlertsOptions;

use crate::args::Args;

/// What `main` hands the shell, plus a startup warning if the history could
/// not be opened.
pub struct OpenedAlerts {
    pub options: AlertsOptions,
    pub warning: Option<String>,
}

fn in_memory() -> Arc<dyn AlertStore> {
    Arc::new(SqliteAlertStore::open_in_memory().expect("in-memory sqlite store"))
}

/// `--fake-alerts N` seeds sample history into `dir`'s database (and, with
/// `--fake-live-alerts M`, fires M live alerts after startup). It refuses
/// to run without an explicit `NATIKUBE_CONFIG_DIR`, so the real history is
/// never touched, and it never monitors a cluster.
fn open_fake(
    count: usize,
    live_fake: usize,
    dir: Option<&Path>,
    explicit_dir: bool,
    catalog: &[ClusterInfo],
    now: i64,
) -> Result<OpenedAlerts, String> {
    let dir = dir
        .filter(|_| explicit_dir)
        .ok_or("--fake-alerts needs NATIKUBE_CONFIG_DIR set to a scratch folder")?;
    let store = SqliteAlertStore::in_config_dir(dir).map_err(|e| e.to_string())?;
    store.clear().map_err(|e| e.to_string())?;
    let cluster = catalog
        .first()
        .map(|c| c.id.clone())
        .unwrap_or_else(|| ClusterId::new("fake-cluster"));
    seed_store(&store, now, &cluster, count);
    Ok(OpenedAlerts {
        options: AlertsOptions {
            store: Arc::new(store),
            monitor: false,
            live_fake,
        },
        warning: None,
    })
}

/// Chooses the store and the monitor switch for this run.
///
/// * `--fake-alerts`: seeded SQLite in the scratch config dir, no monitor.
/// * `--demo` or `--fake-rows`: an empty in-memory history, no monitor.
/// * Otherwise: `notifications.sqlite` in the config dir with the monitor on;
///   if it cannot be opened the history lives in memory for this session.
pub fn open_alerts(args: &Args, catalog: &[ClusterInfo], now: i64) -> Result<OpenedAlerts, String> {
    let dir = FileSettingsStore::app_config_dir();
    let explicit =
        std::env::var_os(FileSettingsStore::CONFIG_DIR_ENV).is_some_and(|v| !v.is_empty());
    open_with(args, catalog, now, dir.as_deref(), explicit)
}

fn open_with(
    args: &Args,
    catalog: &[ClusterInfo],
    now: i64,
    dir: Option<&Path>,
    explicit_dir: bool,
) -> Result<OpenedAlerts, String> {
    if let Some(count) = args.fake_alerts {
        return open_fake(
            count,
            args.fake_live_alerts.unwrap_or(0),
            dir,
            explicit_dir,
            catalog,
            now,
        );
    }
    if args.demo || args.fake_rows.is_some() {
        return Ok(OpenedAlerts {
            options: AlertsOptions {
                store: in_memory(),
                monitor: false,
                live_fake: 0,
            },
            warning: None,
        });
    }
    let opened = dir
        .ok_or_else(|| "no config folder".to_string())
        .and_then(|dir| SqliteAlertStore::in_config_dir(dir).map_err(|e| e.to_string()));
    Ok(match opened {
        Ok(store) => OpenedAlerts {
            options: AlertsOptions {
                store: Arc::new(store),
                monitor: true,
                live_fake: 0,
            },
            warning: None,
        },
        Err(e) => OpenedAlerts {
            options: AlertsOptions {
                store: in_memory(),
                monitor: true,
                live_fake: 0,
            },
            warning: Some(format!(
                "Notification history unavailable ({e}); alerts are kept in memory this session."
            )),
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(fake_alerts: Option<usize>, demo: bool) -> Args {
        Args {
            fake_alerts,
            demo,
            ..Args::default()
        }
    }

    #[test]
    fn fake_alerts_seed_the_scratch_database_and_never_monitor() {
        let dir = tempfile::tempdir().unwrap();
        let catalog = vec![ClusterInfo::from_context("demo-cluster")];
        let opened = open_with(
            &args(Some(12), true),
            &catalog,
            1_700_000_000,
            Some(dir.path()),
            true,
        )
        .unwrap();
        assert!(!opened.options.monitor);
        assert_eq!(opened.options.store.total_count().unwrap(), 12);
        assert!(dir.path().join("notifications.sqlite").exists());
    }

    #[test]
    fn fake_alerts_refuse_the_real_config_dir() {
        let dir = tempfile::tempdir().unwrap();
        let result = open_with(&args(Some(3), true), &[], 0, Some(dir.path()), false);
        assert!(result.is_err());
        assert!(!dir.path().join("notifications.sqlite").exists());
    }

    #[test]
    fn demo_uses_memory_and_no_monitor() {
        let dir = tempfile::tempdir().unwrap();
        let opened = open_with(&args(None, true), &[], 0, Some(dir.path()), true).unwrap();
        assert!(!opened.options.monitor);
        assert_eq!(opened.options.store.total_count().unwrap(), 0);
        assert!(!dir.path().join("notifications.sqlite").exists());
    }

    #[test]
    fn normal_run_opens_the_database_and_monitors() {
        let dir = tempfile::tempdir().unwrap();
        let opened = open_with(&args(None, false), &[], 0, Some(dir.path()), true).unwrap();
        assert!(opened.options.monitor);
        assert!(opened.warning.is_none());
        assert!(dir.path().join("notifications.sqlite").exists());
    }

    #[test]
    fn unopenable_history_falls_back_to_memory_with_a_warning() {
        let file = tempfile::NamedTempFile::new().unwrap();
        // A file where a folder is expected.
        let opened = open_with(&args(None, false), &[], 0, Some(file.path()), true).unwrap();
        assert!(opened.warning.is_some());
        assert_eq!(opened.options.store.total_count().unwrap(), 0);
    }
}
