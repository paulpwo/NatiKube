use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use kube::config::Kubeconfig;
use natikube_core::{ClusterInfo, Settings};

use crate::domain::{
    Found, RANK_DEFAULT_CONFIG, RANK_ENV_BASE, RANK_EXTRA, RANK_KUBE_DIR, dedupe, env_entries,
};

/// The default kubeconfig's `current-context` (honours `$KUBECONFIG`).
pub fn current_context() -> Option<String> {
    Kubeconfig::read().ok().and_then(|k| k.current_context)
}

/// Every context declared in one kubeconfig file. The returned entries are
/// pinned to that file (`kubeconfig: Some(path)`).
pub fn contexts_in_file(path: &Path) -> Result<Vec<ClusterInfo>, String> {
    let kubeconfig = Kubeconfig::read_from(path).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(contexts_of(&kubeconfig, Some(path)))
}

fn contexts_of(kubeconfig: &Kubeconfig, path: Option<&Path>) -> Vec<ClusterInfo> {
    with_users(kubeconfig, path)
        .into_iter()
        .map(|(info, _)| info)
        .collect()
}

/// Contexts with the name of the user each one points at.
fn with_users(kubeconfig: &Kubeconfig, path: Option<&Path>) -> Vec<(ClusterInfo, Option<String>)> {
    kubeconfig
        .contexts
        .iter()
        .map(|named| {
            let mut info = ClusterInfo::new(&named.name, path.map(Path::to_path_buf));
            let mut user = None;
            if let Some(context) = &named.context {
                user = context.user.clone();
                info.namespace = context.namespace.clone();
                info.server = kubeconfig
                    .clusters
                    .iter()
                    .find(|c| c.name == context.cluster)
                    .and_then(|c| c.cluster.as_ref())
                    .and_then(|c| c.server.clone());
            }
            (info, user)
        })
        .collect()
}

/// Files bigger than this are not kubeconfigs.
const MAX_KUBECONFIG_BYTES: u64 = 8 * 1024 * 1024;

fn is_candidate(path: &Path) -> bool {
    let hidden = path
        .file_name()
        .is_some_and(|n| n.to_string_lossy().starts_with('.'));
    !hidden && std::fs::metadata(path).is_ok_and(|m| m.is_file() && m.len() <= MAX_KUBECONFIG_BYTES)
}

/// Top-level regular files of a folder, sorted by name (one level only).
fn files_in(dir: &Path) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .map(|entries| {
            entries
                .flatten()
                .map(|e| e.path())
                .filter(|p| is_candidate(p))
                .collect()
        })
        .unwrap_or_default();
    files.sort();
    files
}

fn is_symlink(path: &Path) -> bool {
    std::fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_symlink())
}

/// One file reachable through one or more source paths.
struct Source {
    canonical: PathBuf,
    /// The path shown to the user: the real file when it was reached by its
    /// own name, otherwise the first path that reached it.
    path: PathBuf,
    rank: usize,
}

/// Folds source paths that resolve to the same file (a symlink and its
/// target). The group keeps its best (lowest) rank and shows the real file
/// name when some source reached it directly.
fn unique_sources(sources: Vec<(PathBuf, usize)>) -> Vec<Source> {
    let mut files: Vec<Source> = Vec::new();
    for (path, rank) in sources {
        if !is_candidate(&path) {
            continue;
        }
        let canonical = std::fs::canonicalize(&path).unwrap_or_else(|_| path.clone());
        match files.iter_mut().find(|f| f.canonical == canonical) {
            Some(file) => {
                file.rank = file.rank.min(rank);
                if is_symlink(&file.path) && !is_symlink(&path) {
                    file.path = path;
                }
            }
            None => files.push(Source {
                canonical,
                path,
                rank,
            }),
        }
    }
    // A lone symlink keeps its own path, so a user's `config` stays `config`.
    files
}

fn read_file(path: &Path, rank: usize) -> Vec<Found> {
    let Ok(kubeconfig) = Kubeconfig::read_from(path) else {
        return Vec::new();
    };
    let modified: Option<SystemTime> = std::fs::metadata(path).and_then(|m| m.modified()).ok();
    with_users(&kubeconfig, Some(path))
        .into_iter()
        .map(|(info, user)| Found {
            info,
            user,
            rank,
            modified,
        })
        .collect()
}

/// Every context of the files that really are kubeconfigs, ranked by source.
/// A file reachable through several sources keeps its best (lowest) rank.
/// Files are parsed in parallel (input order is kept) once there are several.
fn read_sources(sources: Vec<(PathBuf, usize)>) -> Vec<Found> {
    let files = unique_sources(sources);
    if files.len() < 3 {
        return files
            .iter()
            .flat_map(|f| read_file(&f.path, f.rank))
            .collect();
    }
    std::thread::scope(|scope| {
        let handles: Vec<_> = files
            .iter()
            .map(|f| scope.spawn(|| read_file(&f.path, f.rank)))
            .collect();
        handles
            .into_iter()
            .flat_map(|h| h.join().unwrap_or_default())
            .collect()
    })
}

/// Discovery with explicit inputs (the testable core of [`load_catalog`]).
///
/// Sources, in preference order: `<kube_dir>/config`, the `$KUBECONFIG`
/// entries, the other top-level regular files of `kube_dir` (hidden files,
/// sub-folders such as `cache/` and files without contexts are skipped), then
/// the `extras` (files, or folders scanned one level). Identical contexts
/// found in several files are folded together (see [`dedupe`]).
pub fn discover_clusters(
    kube_dir: Option<&Path>,
    kubeconfig_env: Option<&OsStr>,
    extras: &[PathBuf],
) -> Vec<ClusterInfo> {
    let mut sources: Vec<(PathBuf, usize)> = Vec::new();
    if let Some(dir) = kube_dir {
        sources.push((dir.join("config"), RANK_DEFAULT_CONFIG));
    }
    if let Some(value) = kubeconfig_env {
        for (i, path) in env_entries(value).into_iter().enumerate() {
            sources.push((path, RANK_ENV_BASE + i));
        }
    }
    if let Some(dir) = kube_dir {
        sources.extend(files_in(dir).into_iter().map(|p| (p, RANK_KUBE_DIR)));
    }
    for extra in extras {
        if extra.is_dir() {
            sources.extend(files_in(extra).into_iter().map(|p| (p, RANK_EXTRA)));
        } else {
            sources.push((extra.clone(), RANK_EXTRA));
        }
    }
    dedupe(read_sources(sources))
}

/// `~/.kube` (`%USERPROFILE%\.kube` on Windows), the folder scanned
/// automatically.
pub fn default_kube_dir() -> Option<PathBuf> {
    kube_dir_in(dirs::home_dir().as_deref())
}

/// [`default_kube_dir`] for an explicit home directory (a pure function, so
/// path resolution is testable on any OS).
pub fn kube_dir_in(home: Option<&Path>) -> Option<PathBuf> {
    home.map(|home| home.join(".kube"))
}

/// The cluster catalog: every context found in `~/.kube`, `$KUBECONFIG` and
/// the settings' extra files/folders, deduped, each pinned to its source file
/// (`kubeconfig`) with the other declaring files in `also_in`. Unreadable and
/// non-kubeconfig files are skipped. This is also the live-reload entry point
/// for [`natikube_core::AppCommand::ReloadClusters`].
pub fn load_catalog(settings: &Settings) -> Vec<ClusterInfo> {
    let env = std::env::var_os("KUBECONFIG");
    discover_clusters(
        default_kube_dir().as_deref(),
        env.as_deref(),
        &settings.extra_kubeconfigs,
    )
}

/// [`load_catalog`] on a worker thread, so the first frame never waits for the
/// kubeconfig scan. Runtime-agnostic: any executor can poll the future.
pub async fn load_catalog_async(settings: Settings) -> Vec<ClusterInfo> {
    super::background::run(move || load_catalog(&settings)).await
}
