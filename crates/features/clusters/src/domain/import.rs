//! Outcome of importing kubeconfig paths (from Freelens or an open panel).

use std::path::{Path, PathBuf};

use natikube_core::Settings;

/// What an import did with each candidate path. Paths only, never contents.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ImportSummary {
    /// New paths to add to `extra_kubeconfigs`.
    pub imported: Vec<PathBuf>,
    /// Paths the app already reads (extras, or inside a scanned folder).
    pub already_present: Vec<PathBuf>,
    /// Paths that do not exist (any more).
    pub missing: Vec<PathBuf>,
}

impl ImportSummary {
    pub fn is_empty(&self) -> bool {
        self.imported.is_empty() && self.already_present.is_empty() && self.missing.is_empty()
    }

    /// Appends the imported paths to the settings (no duplicates).
    pub fn apply(&self, settings: &mut Settings) {
        for path in &self.imported {
            if !settings.extra_kubeconfigs.contains(path) {
                settings.extra_kubeconfigs.push(path.clone());
            }
        }
    }

    /// One-line text for a toast or dialog.
    pub fn message(&self) -> String {
        format!(
            "{} imported, {} already present, {} missing",
            self.imported.len(),
            self.already_present.len(),
            self.missing.len()
        )
    }
}

fn same_path(a: &Path, b: &Path) -> bool {
    a == b
        || matches!(
            (std::fs::canonicalize(a), std::fs::canonicalize(b)),
            (Ok(x), Ok(y)) if x == y
        )
}

/// Classifies `candidates`. A path is already present when it is in `known`,
/// or when it sits directly inside one of `covered_dirs` (folders the app
/// scans on its own, such as `~/.kube`).
pub fn classify(
    candidates: Vec<PathBuf>,
    known: &[PathBuf],
    covered_dirs: &[PathBuf],
) -> ImportSummary {
    let mut summary = ImportSummary::default();
    let mut seen: Vec<PathBuf> = Vec::new();
    for path in candidates {
        if seen.iter().any(|s| s == &path) {
            continue;
        }
        seen.push(path.clone());
        let covered = known.iter().any(|k| same_path(k, &path))
            || path
                .parent()
                .is_some_and(|p| covered_dirs.iter().any(|d| same_path(d, p)));
        if covered {
            summary.already_present.push(path);
        } else if !path.exists() {
            summary.missing.push(path);
        } else {
            summary.imported.push(path);
        }
    }
    summary
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn apply_appends_without_duplicates() {
        let mut settings = Settings {
            extra_kubeconfigs: vec![PathBuf::from("/a")],
            ..Settings::default()
        };
        let summary = ImportSummary {
            imported: vec![PathBuf::from("/a"), PathBuf::from("/b")],
            ..ImportSummary::default()
        };
        summary.apply(&mut settings);
        assert_eq!(
            settings.extra_kubeconfigs,
            vec![PathBuf::from("/a"), PathBuf::from("/b")]
        );
    }

    #[test]
    fn message_counts_each_bucket() {
        let summary = ImportSummary {
            imported: vec![PathBuf::from("/a")],
            already_present: vec![],
            missing: vec![PathBuf::from("/m"), PathBuf::from("/n")],
        };
        assert_eq!(
            summary.message(),
            "1 imported, 0 already present, 2 missing"
        );
    }
}
