//! Small persisted preferences for the standalone studio.
//!
//! Settings are intentionally independent of Bevy and egui so startup and
//! tests can load them without constructing a window.  A missing file is a
//! normal first-run state and returns [`StudioSettings::default`].

#![allow(clippy::missing_errors_doc)]

use std::{
    fmt, fs, io,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use crate::file_io::{self, FileIoError};

/// Maximum number of recently opened recipe paths retained on disk.
pub const MAX_RECENT_FILES: usize = 16;

/// Persisted user preferences that do not belong to a recipe document.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct StudioSettings {
    /// Most recently opened/saved recipes, newest first.
    pub recent_files: Vec<PathBuf>,
    /// Last requested window size in physical pixels.
    pub window_size: [u32; 2],
    /// Preview generation dimension selected by the user.
    pub preview_resolution: u32,
    /// Whether committed edits schedule a debounced preview.
    pub auto_preview: bool,
    /// Whether 2D maps use nearest-neighbour filtering.
    pub nearest_filter: bool,
    /// Last selected map tab, kept as a string so UI enum additions do not
    /// invalidate old settings files.
    pub selected_map: String,
}

impl Default for StudioSettings {
    fn default() -> Self {
        Self {
            recent_files: Vec::new(),
            window_size: [1440, 900],
            preview_resolution: 256,
            auto_preview: true,
            nearest_filter: false,
            selected_map: "albedo".into(),
        }
    }
}

/// Errors while loading or persisting settings.
#[derive(Debug)]
pub enum SettingsError {
    /// Filesystem operation failed.
    Io { path: PathBuf, source: io::Error },
    /// JSON could not be decoded.
    Json {
        path: PathBuf,
        source: serde_json::Error,
    },
    /// Canonical serialization or atomic replacement failed.
    File(FileIoError),
}

impl fmt::Display for SettingsError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { path, source } => write!(formatter, "{}: {source}", path.display()),
            Self::Json { path, source } => {
                write!(
                    formatter,
                    "{}: invalid settings JSON: {source}",
                    path.display()
                )
            }
            Self::File(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for SettingsError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            Self::Json { source, .. } => Some(source),
            Self::File(error) => Some(error),
        }
    }
}

impl From<FileIoError> for SettingsError {
    fn from(error: FileIoError) -> Self {
        Self::File(error)
    }
}

/// Alias for applications that call this state `Settings`.
pub type Settings = StudioSettings;

impl StudioSettings {
    /// Loads settings, treating a missing file as a first-run default.
    pub fn load(path: impl AsRef<Path>) -> Result<Self, SettingsError> {
        let path = path.as_ref();
        let bytes = match fs::read(path) {
            Ok(bytes) => bytes,
            Err(source) if source.kind() == io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(source) => {
                return Err(SettingsError::Io {
                    path: path.to_path_buf(),
                    source,
                });
            }
        };
        let mut settings =
            serde_json::from_slice::<Self>(&bytes).map_err(|source| SettingsError::Json {
                path: path.to_path_buf(),
                source,
            })?;
        settings.normalize();
        Ok(settings)
    }

    /// Alias for code that wants to make missing-file fallback explicit.
    pub fn load_or_default(path: impl AsRef<Path>) -> Result<Self, SettingsError> {
        Self::load(path)
    }

    /// Persists settings as pretty JSON with a trailing newline through the
    /// same atomic sibling replacement used for recipes.
    pub fn save(&self, path: impl AsRef<Path>) -> Result<(), SettingsError> {
        let bytes = file_io::canonical_json_bytes(self)?;
        file_io::write_bytes_atomic(path, &bytes)?;
        Ok(())
    }

    /// Adds a path to the front of the recent list, deduplicating it.
    pub fn remember_recent(&mut self, path: impl AsRef<Path>) {
        let path = normalize_recent_path(path.as_ref());
        self.recent_files.retain(|candidate| candidate != &path);
        self.recent_files.insert(0, path);
        self.recent_files.truncate(MAX_RECENT_FILES);
    }

    /// Removes one path from recents, returning whether it was present.
    pub fn forget_recent(&mut self, path: impl AsRef<Path>) -> bool {
        let path = normalize_recent_path(path.as_ref());
        let original_len = self.recent_files.len();
        self.recent_files.retain(|candidate| candidate != &path);
        original_len != self.recent_files.len()
    }

    /// Removes recent paths that no longer exist, returning the number removed.
    pub fn prune_missing_recent(&mut self) -> usize {
        let original_len = self.recent_files.len();
        self.recent_files.retain(|path| path.exists());
        original_len - self.recent_files.len()
    }

    /// Returns a platform-appropriate default settings path, if a user config
    /// directory can be identified.  The app name is reduced to a safe single
    /// path component.
    #[must_use]
    pub fn default_path(app_name: &str) -> Option<PathBuf> {
        let component = safe_component(app_name);
        #[cfg(target_os = "macos")]
        {
            let home = std::env::var_os("HOME")?;
            Some(
                PathBuf::from(home)
                    .join("Library")
                    .join("Application Support")
                    .join(component)
                    .join("settings.json"),
            )
        }
        #[cfg(target_os = "windows")]
        {
            let root = std::env::var_os("APPDATA")?;
            Some(PathBuf::from(root).join(component).join("settings.json"))
        }
        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        {
            let root = std::env::var_os("XDG_CONFIG_HOME")
                .map(PathBuf::from)
                .or_else(|| {
                    std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config"))
                })?;
            Some(root.join(component).join("settings.json"))
        }
    }

    fn normalize(&mut self) {
        let mut unique = Vec::with_capacity(self.recent_files.len().min(MAX_RECENT_FILES));
        for path in self.recent_files.drain(..) {
            let path = normalize_recent_path(&path);
            if !unique.contains(&path) {
                unique.push(path);
            }
            if unique.len() == MAX_RECENT_FILES {
                break;
            }
        }
        self.recent_files = unique;
        if self.preview_resolution == 0 {
            self.preview_resolution = Self::default().preview_resolution;
        }
        if self.window_size.contains(&0) {
            self.window_size = Self::default().window_size;
        }
        if self.selected_map.trim().is_empty() {
            self.selected_map = Self::default().selected_map;
        }
    }
}

fn normalize_recent_path(path: &Path) -> PathBuf {
    fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

fn safe_component(value: &str) -> String {
    let component = value
        .chars()
        .filter_map(|character| {
            character
                .is_ascii_alphanumeric()
                .then_some(character.to_ascii_lowercase())
        })
        .collect::<String>();
    if component.is_empty() {
        "island-material-studio".into()
    } else {
        component
    }
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        path::PathBuf,
        sync::atomic::{AtomicU64, Ordering},
        time::{SystemTime, UNIX_EPOCH},
    };

    use super::{MAX_RECENT_FILES, StudioSettings};

    static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

    fn temporary_directory() -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock after epoch")
            .as_nanos();
        let counter = TEST_COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "island-material-studio-settings-{}-{nanos}-{counter}",
            std::process::id()
        ));
        fs::create_dir_all(&path).expect("temporary directory");
        path
    }

    #[test]
    fn recent_files_are_newest_first_unique_and_bounded() {
        let directory = temporary_directory();
        let mut settings = StudioSettings::default();
        let first = directory.join("first.json");
        let second = directory.join("second.json");
        for index in 0..(MAX_RECENT_FILES + 3) {
            settings.remember_recent(directory.join(format!("{index}.json")));
        }
        settings.remember_recent(&first);
        settings.remember_recent(&second);
        settings.remember_recent(&first);
        assert_eq!(settings.recent_files.first(), Some(&first));
        assert_eq!(settings.recent_files.get(1), Some(&second));
        assert_eq!(settings.recent_files.len(), MAX_RECENT_FILES);
        fs::remove_dir_all(directory).expect("cleanup");
    }

    #[test]
    fn save_load_roundtrip_and_missing_file_default() {
        let directory = temporary_directory();
        let path = directory.join("settings.json");
        let missing = directory.join("missing.json");
        assert_eq!(
            StudioSettings::load(&missing).expect("first run"),
            StudioSettings::default()
        );
        let mut settings = StudioSettings {
            preview_resolution: 512,
            ..StudioSettings::default()
        };
        settings.remember_recent(directory.join("recipe.json"));
        settings.save(&path).expect("save settings");
        let loaded = StudioSettings::load(&path).expect("load settings");
        assert_eq!(loaded, settings);
        fs::remove_dir_all(directory).expect("cleanup");
    }

    #[test]
    fn malformed_settings_are_reported() {
        let directory = temporary_directory();
        let path = directory.join("settings.json");
        fs::write(&path, b"{not-json").expect("write malformed");
        assert!(StudioSettings::load(&path).is_err());
        fs::remove_dir_all(directory).expect("cleanup");
    }

    #[test]
    fn invalid_dimensions_are_normalized_on_load() {
        let directory = temporary_directory();
        let path = directory.join("settings.json");
        fs::write(
            &path,
            br#"{"window_size":[0,0],"preview_resolution":0,"selected_map":""}"#,
        )
        .expect("write settings");
        let loaded = StudioSettings::load(&path).expect("load settings");
        assert_eq!(loaded.window_size, [1440, 900]);
        assert_eq!(loaded.preview_resolution, 256);
        assert_eq!(loaded.selected_map, "albedo");
        fs::remove_dir_all(directory).expect("cleanup");
    }

    #[test]
    fn forget_and_prune_report_changes() {
        let directory = temporary_directory();
        let existing = directory.join("existing.json");
        let missing = directory.join("missing.json");
        fs::write(&existing, b"x").expect("existing file");
        let mut settings = StudioSettings::default();
        settings.remember_recent(&existing);
        settings.remember_recent(&missing);
        assert!(settings.forget_recent(&existing));
        assert_eq!(settings.prune_missing_recent(), 1);
        assert!(settings.recent_files.is_empty());
        fs::remove_dir_all(directory).expect("cleanup");
    }
}
