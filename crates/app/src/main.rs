#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]
//! `natikube`: parses flags, loads the cluster catalog and settings, and
//! hands over to the shell.

mod args;
mod demo;

use std::process::ExitCode;
use std::sync::Arc;
use std::time::{Duration, Instant};

use natikube_clusters::{
    BackgroundSettingsStore, FileSettingsStore, current_context, find_cluster, init_environment,
    load_catalog,
};
use natikube_core::{ClusterId, ClusterInfo, MemorySettingsStore, Settings, SettingsStore, kinds};
use natikube_kube_client::{ClusterConnection, spawn};
use natikube_resources::list_names;
use natikube_shell::ShellOptions;

use crate::args::Args;

// Allocator experiments (settings-perf-polish O5). Both features are off by
// default; see docs/architecture.md, Performance.
#[cfg(feature = "mimalloc")]
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

#[cfg(feature = "dhat-heap")]
#[global_allocator]
static PROFILING: dhat::Alloc = dhat::Alloc;

fn main() -> ExitCode {
    #[cfg(feature = "dhat-heap")]
    let _profiler = dhat::Profiler::new_heap();
    let started_at = Instant::now();
    init_environment();
    let args = match Args::parse(std::env::args().skip(1)) {
        Ok(Some(args)) => args,
        Ok(None) => {
            println!("{}", args::USAGE);
            return ExitCode::SUCCESS;
        }
        Err(message) => {
            eprintln!("error: {message}\n\n{}", args::USAGE);
            return ExitCode::from(2);
        }
    };

    // `--demo` never touches the real settings file or any kubeconfig.
    let file_store: Option<Arc<FileSettingsStore>> = if args.demo {
        None
    } else {
        FileSettingsStore::default_location().map(Arc::new)
    };
    let (settings, startup_warning) = match &file_store {
        Some(file) => {
            let report = file.load_reporting();
            (report.settings, report.warning)
        }
        None => (Settings::default(), None),
    };
    let mut catalog = if args.demo {
        demo::demo_catalog()
    } else {
        load_catalog(&settings)
    };

    // `--context` selects a catalog entry, or adds a bare context when the
    // name is not in any known kubeconfig (never in demo mode).
    let wanted = args.context.clone().or_else(|| {
        if args.demo {
            demo::DEMO_CONTEXTS.first().map(|c| c.to_string())
        } else if args.fake_rows.is_none() && settings.last_cluster.is_none() {
            current_context()
        } else {
            None
        }
    });
    let initial_cluster = wanted.and_then(|context| match find_cluster(&catalog, &context) {
        Some(info) => Some(info.id.clone()),
        None if args.demo => None,
        None => {
            let info = ClusterInfo::from_context(context);
            let id = info.id.clone();
            catalog.push(info);
            Some(id)
        }
    });

    if args.dump_clusters {
        return dump_clusters(&catalog);
    }

    if let Some(kind) = &args.dump {
        return dump(kind, initial_cluster, &catalog);
    }

    if args.dump_rows && args.fake_rows.is_none() {
        print_rows(initial_cluster.clone(), &catalog);
    }

    // Settings writes go through a worker thread so a keystroke or a toggle
    // never waits for the disk; the last write is flushed on quit.
    let background = file_store.map(|file| Arc::new(BackgroundSettingsStore::new(file)));
    let store: Arc<dyn SettingsStore> = match &background {
        Some(background) => background.clone(),
        None => Arc::new(MemorySettingsStore::default()),
    };
    let on_quit: Option<Arc<dyn Fn() + Send + Sync>> = background.as_ref().map(|background| {
        let background = background.clone();
        Arc::new(move || background.flush()) as Arc<dyn Fn() + Send + Sync>
    });

    natikube_shell::run(ShellOptions {
        store,
        catalog,
        initial_cluster,
        fake_rows: args.fake_rows,
        started_at,
        exit_after_first_frame: args.exit_after_first_frame,
        rows_timeout: Duration::from_secs(15),
        appearance_override: args.appearance,
        native_dark_override: args.native_dark,
        open_settings: args.open_settings,
        demo: args.demo,
        startup_warning,
        on_quit,
    });
    if let Some(background) = background {
        background.flush();
    }
    ExitCode::SUCCESS
}

fn cluster_for(
    initial: Option<ClusterId>,
    catalog: &[ClusterInfo],
    settings: &Settings,
) -> Option<ClusterInfo> {
    initial
        .or_else(|| settings.last_cluster.clone())
        .and_then(|id| catalog.iter().find(|c| c.id == id))
        .or_else(|| catalog.first())
        .cloned()
}

/// One `--dump-clusters` line: `context<TAB>source file<TAB>status`. Only names
/// and file names, never contents.
pub(crate) fn cluster_line(info: &ClusterInfo) -> String {
    let source = info
        .source_file_name()
        .unwrap_or_else(|| "(default kubeconfig)".to_string());
    let status = if info.also_in.is_empty() && info.aliases.is_empty() {
        "unique".to_string()
    } else {
        let names: Vec<String> = info
            .also_in
            .iter()
            .filter_map(|p| p.file_name())
            .map(|n| n.to_string_lossy().into_owned())
            .collect();
        let mut text = format!("deduped (also in: {})", names.join(", "));
        if !info.aliases.is_empty() {
            text.push_str(&format!("; aliases: {}", info.aliases.join(", ")));
        }
        text
    };
    format!("{}\t{source}\t{status}", info.context)
}

/// `--dump-clusters`: lists the discovered catalog and exits.
fn dump_clusters(catalog: &[ClusterInfo]) -> ExitCode {
    for info in catalog {
        println!("{}", cluster_line(info));
    }
    ExitCode::SUCCESS
}

/// `--dump <kind>`: prints `namespace/name` lines (sorted) and exits.
fn dump(kind: &str, initial: Option<ClusterId>, catalog: &[ClusterInfo]) -> ExitCode {
    let Some(kind) = kinds::find(kind) else {
        eprintln!("error: unknown kind {kind:?}");
        return ExitCode::from(2);
    };
    let settings = Settings::default();
    let Some(info) = cluster_for(initial, catalog, &settings) else {
        eprintln!("error: no cluster found in the kubeconfig");
        return ExitCode::from(2);
    };
    let result = futures::executor::block_on(spawn(async move {
        let connection = ClusterConnection::connect(&info)
            .await
            .map_err(|e| e.to_string())?;
        list_names(&connection, &kind)
            .await
            .map_err(|e| e.to_string())
    }));
    match result {
        Ok(Ok(names)) => {
            for name in names {
                println!("{name}");
            }
            ExitCode::SUCCESS
        }
        Ok(Err(message)) => {
            eprintln!("error: {message}");
            ExitCode::FAILURE
        }
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

/// `--dump-rows` (F0 compatible): before the window opens, prints
/// `rows_count=N` and one `row=namespace/name` line per pod on stderr. Used by
/// `scripts/measure.sh`; it delays startup by one cluster round-trip, so
/// timings taken with this flag are not meaningful.
fn print_rows(initial: Option<ClusterId>, catalog: &[ClusterInfo]) {
    let Some(info) = cluster_for(initial, catalog, &Settings::default()) else {
        return;
    };
    let names = futures::executor::block_on(spawn(async move {
        let connection = ClusterConnection::connect(&info).await.ok()?;
        list_names(&connection, &kinds::POD).await.ok()
    }));
    if let Ok(Some(names)) = names {
        eprintln!("rows_count={}", names.len());
        for name in names {
            eprintln!("row={name}");
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use natikube_core::ClusterInfo;

    use super::cluster_line;

    #[test]
    fn cluster_line_shows_source_and_dedupe_without_secrets() {
        let mut info = ClusterInfo::new("prod-cluster", Some(PathBuf::from("/h/.kube/config.bak")));
        assert_eq!(cluster_line(&info), "prod-cluster\tconfig.bak\tunique");
        info.also_in = vec![PathBuf::from("/h/.kube/config.bak2")];
        assert_eq!(
            cluster_line(&info),
            "prod-cluster\tconfig.bak\tdeduped (also in: config.bak2)"
        );
        assert!(
            cluster_line(&ClusterInfo::from_context("dev-cluster"))
                .contains("(default kubeconfig)")
        );
    }
}
