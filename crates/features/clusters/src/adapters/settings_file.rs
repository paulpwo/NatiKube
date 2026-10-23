use std::path::{Path, PathBuf};

use natikube_core::{Settings, SettingsError, SettingsStore};

/// Settings persisted as pretty JSON in one file.
#[derive(Debug, Clone)]
pub struct FileSettingsStore {
    path: PathBuf,
}

impl FileSettingsStore {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    /// `settings.json` inside the OS config dir: `~/Library/Application
    /// Support/NatiKube` on macOS, `%APPDATA%\NatiKube` on Windows.
    pub fn default_location() -> Option<Self> {
        Self::in_config_dir(dirs::config_dir())
    }

    /// [`default_location`](Self::default_location) for an explicit config
    /// dir (a pure function, so path resolution is testable on any OS).
    pub fn in_config_dir(config_dir: Option<PathBuf>) -> Option<Self> {
        config_dir.map(|dir| Self::new(dir.join("NatiKube").join("settings.json")))
    }

    /// `settings.json.bak` next to the file, or the first free `.bak1`,
    /// `.bak2`, ... when earlier backups hold different content. `None` when
    /// an existing backup already has exactly `content`.
    fn backup_target(&self, content: &str) -> Option<PathBuf> {
        let base = self.path.as_os_str().to_os_string();
        for n in 0..100 {
            let mut name = base.clone();
            name.push(if n == 0 {
                ".bak".to_string()
            } else {
                format!(".bak{n}")
            });
            let candidate = PathBuf::from(name);
            match std::fs::read_to_string(&candidate) {
                Ok(existing) if existing == content => return None,
                Ok(_) => continue,
                Err(_) => return Some(candidate),
            }
        }
        None
    }

    /// Copies a broken settings file aside. Best effort: a failed backup
    /// never hides the original error.
    fn back_up_broken(&self, content: &str) {
        if let Some(target) = self.backup_target(content) {
            let _ = std::fs::write(target, content);
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl SettingsStore for FileSettingsStore {
    fn load(&self) -> Result<Settings, SettingsError> {
        match std::fs::read_to_string(&self.path) {
            Ok(text) => serde_json::from_str(&text).map_err(|e| {
                self.back_up_broken(&text);
                SettingsError::Format(format!(
                    "{} has a syntax error ({e}); using defaults, the file is left untouched \
                     and a .bak copy was kept",
                    self.path.display()
                ))
            }),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Settings::default()),
            Err(e) => Err(SettingsError::Io(e.to_string())),
        }
    }

    fn save(&self, settings: &Settings) -> Result<(), SettingsError> {
        // A file with a syntax error is the user's hand edit in progress (or
        // a damaged file): never replace it. Back it up and refuse.
        if let Ok(existing) = std::fs::read_to_string(&self.path)
            && serde_json::from_str::<serde_json::Value>(&existing).is_err()
        {
            self.back_up_broken(&existing);
            return Err(SettingsError::Format(format!(
                "{} has a syntax error; not overwriting it, changes were not saved (fix the file or delete it)",
                self.path.display()
            )));
        }
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| SettingsError::Io(e.to_string()))?;
        }
        let text = serde_json::to_string_pretty(settings)
            .map_err(|e| SettingsError::Format(e.to_string()))?;
        // Write then rename so a crash never leaves a half-written file.
        let tmp = self.path.with_extension("json.tmp");
        std::fs::write(&tmp, text).map_err(|e| SettingsError::Io(e.to_string()))?;
        std::fs::rename(&tmp, &self.path).map_err(|e| SettingsError::Io(e.to_string()))
    }
}
