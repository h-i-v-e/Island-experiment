//! Recipe parsing, canonical serialization, hashing and atomic file writes.
//!
//! This module intentionally has no UI or Bevy dependencies.  It is the
//! safety boundary used by both the document lifecycle and settings
//! persistence: parse and validate before a caller mutates live state, compare
//! the source bytes immediately before replacement, and write through a
//! sibling temporary file followed by one rename.

#![allow(clippy::missing_errors_doc)]

use std::{
    fmt,
    fs::{self, File, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

use motu::procedural_textures::{RecipeValidationErrors, TextureRecipe, validate_recipe};
use serde::Serialize;
use sha2::{Digest, Sha256};

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

/// A source file changed between opening/last saving and a requested save.
///
/// The caller must choose an explicit policy: reload the external content,
/// save to another path, or intentionally overwrite the external change.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExternalChangeConflict {
    /// File whose bytes no longer match the captured source hash.
    pub path: PathBuf,
    /// Hash captured at open or after the last successful save. `None` means
    /// the target did not exist when the expected state was captured.
    pub expected_hash: Option<String>,
    /// Current file hash, or `None` when the target has been removed.
    pub actual_hash: Option<String>,
}

impl fmt::Display for ExternalChangeConflict {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "recipe file {} changed externally (expected {:?}, found {:?})",
            self.path.display(),
            self.expected_hash,
            self.actual_hash
        )
    }
}

impl std::error::Error for ExternalChangeConflict {}

/// Explicit choices a UI can present after an external-change conflict.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConflictResolution {
    /// Discard the in-memory state and load the file's current bytes.
    Reload,
    /// Keep the in-memory state and write it to a user-selected path.
    SaveAs,
    /// Keep the in-memory state and intentionally replace the changed file.
    Overwrite,
}

/// A successfully parsed and validated recipe plus its source bookkeeping.
#[derive(Clone, Debug, PartialEq)]
pub struct LoadedRecipe {
    /// Typed recipe owned by the document.
    pub recipe: TextureRecipe,
    /// SHA-256 of the exact bytes read from disk.
    pub source_hash: String,
    /// Canonical pretty JSON representation used for dirty comparisons and
    /// subsequent saves.
    pub canonical_form: Vec<u8>,
}

/// Errors at the recipe file boundary.
#[derive(Debug)]
pub enum FileIoError {
    /// Filesystem operation failed, with the affected path.
    Io { path: PathBuf, source: io::Error },
    /// JSON could not be parsed or serialized.
    Json {
        path: Option<PathBuf>,
        source: serde_json::Error,
    },
    /// The target no longer matches the source hash captured by the document.
    Conflict(ExternalChangeConflict),
    /// The JSON parsed, but the typed recipe is not evaluable.
    Validation {
        path: PathBuf,
        source: RecipeValidationErrors,
    },
}

impl fmt::Display for FileIoError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { path, source } => write!(formatter, "{}: {source}", path.display()),
            Self::Json {
                path: Some(path),
                source,
            } => write!(formatter, "{}: invalid JSON: {source}", path.display()),
            Self::Json { path: None, source } => write!(formatter, "invalid JSON: {source}"),
            Self::Conflict(source) => source.fmt(formatter),
            Self::Validation { path, source } => {
                write!(formatter, "{}: invalid recipe: {source}", path.display())
            }
        }
    }
}

impl std::error::Error for FileIoError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            Self::Json { source, .. } => Some(source),
            Self::Conflict(source) => Some(source),
            Self::Validation { source, .. } => Some(source),
        }
    }
}

/// Backwards-friendly alias for callers that call this boundary `RecipeFile`.
pub type RecipeFileError = FileIoError;

/// Canonical JSON used by `StudioDocument` and settings files.
///
/// Serde preserves the recipe struct's declared field order.  The trailing
/// newline makes files pleasant to inspect and matches the existing output
/// writer's text-file convention.
pub fn canonical_json_bytes<T: Serialize>(value: &T) -> Result<Vec<u8>, FileIoError> {
    let mut bytes = serde_json::to_vec_pretty(value)
        .map_err(|source| FileIoError::Json { path: None, source })?;
    bytes.push(b'\n');
    Ok(bytes)
}

/// Canonical JSON as UTF-8 text.
pub fn canonical_json_string<T: Serialize>(value: &T) -> Result<String, FileIoError> {
    let bytes = canonical_json_bytes(value)?;
    // serde_json emits UTF-8 for all serializable values.  Keep a typed error
    // at this boundary in case a future serializer changes that contract.
    String::from_utf8(bytes).map_err(|error| FileIoError::Json {
        path: None,
        source: serde_json::Error::io(io::Error::new(io::ErrorKind::InvalidData, error)),
    })
}

/// Compute the lowercase SHA-256 hash used for source-change detection.
#[must_use]
pub fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest
        .iter()
        .fold(String::with_capacity(64), |mut result, byte| {
            use fmt::Write as _;
            write!(&mut result, "{byte:02x}").expect("writing to String cannot fail");
            result
        })
}

/// Parse and validate recipe bytes without touching a live document.
pub fn parse_recipe_bytes(path: &Path, bytes: &[u8]) -> Result<LoadedRecipe, FileIoError> {
    let recipe =
        serde_json::from_slice::<TextureRecipe>(bytes).map_err(|source| FileIoError::Json {
            path: Some(path.to_path_buf()),
            source,
        })?;
    validate_recipe(&recipe).map_err(|source| FileIoError::Validation {
        path: path.to_path_buf(),
        source,
    })?;
    let canonical_form = canonical_json_bytes(&recipe).map_err(|error| match error {
        FileIoError::Json { source, .. } => FileIoError::Json {
            path: Some(path.to_path_buf()),
            source,
        },
        other => other,
    })?;
    Ok(LoadedRecipe {
        recipe,
        source_hash: sha256_hex(bytes),
        canonical_form,
    })
}

/// Read, parse and validate one recipe from disk.
pub fn read_recipe(path: impl AsRef<Path>) -> Result<LoadedRecipe, FileIoError> {
    let path = path.as_ref();
    let bytes = fs::read(path).map_err(|source| FileIoError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    parse_recipe_bytes(path, &bytes)
}

/// Return the current file hash, or `None` when the path does not exist.
pub fn file_hash(path: impl AsRef<Path>) -> Result<Option<String>, FileIoError> {
    let path = path.as_ref();
    match fs::read(path) {
        Ok(bytes) => Ok(Some(sha256_hex(&bytes))),
        Err(source) if source.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(source) => Err(FileIoError::Io {
            path: path.to_path_buf(),
            source,
        }),
    }
}

/// Compare a file's current bytes with the hash captured by the document.
pub fn check_external_change(
    path: impl AsRef<Path>,
    expected_hash: Option<&str>,
) -> Result<(), FileIoError> {
    let path = path.as_ref();
    let actual_hash = file_hash(path)?;
    if actual_hash.as_deref() != expected_hash {
        return Err(FileIoError::Conflict(ExternalChangeConflict {
            path: path.to_path_buf(),
            expected_hash: expected_hash.map(str::to_owned),
            actual_hash,
        }));
    }
    Ok(())
}

/// Compare a path and return a typed conflict rather than burying it in an IO
/// error.  This is the preferred document-facing helper.
pub fn external_change(
    path: impl AsRef<Path>,
    expected_hash: Option<&str>,
) -> Result<Option<String>, FileIoError> {
    let path = path.as_ref();
    let actual_hash = file_hash(path)?;
    if actual_hash.as_deref() != expected_hash {
        return Err(FileIoError::Conflict(ExternalChangeConflict {
            path: path.to_path_buf(),
            expected_hash: expected_hash.map(str::to_owned),
            actual_hash,
        }));
    }
    Ok(actual_hash)
}

/// Write a canonical recipe through the atomic sibling replacement path.
pub fn write_recipe_atomic(
    path: impl AsRef<Path>,
    recipe: &TextureRecipe,
) -> Result<Vec<u8>, FileIoError> {
    let bytes = canonical_json_bytes(recipe)?;
    write_bytes_atomic(path, &bytes)?;
    Ok(bytes)
}

/// Write arbitrary UTF-8/text bytes atomically beside the target.
///
/// The temporary file is created with `create_new`, written and synced, then
/// renamed over the target.  Any write or rename failure removes the temporary
/// sibling and leaves an existing target untouched.
pub fn write_bytes_atomic(path: impl AsRef<Path>, bytes: &[u8]) -> Result<(), FileIoError> {
    let path = path.as_ref();
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    if !parent.exists() {
        fs::create_dir_all(parent).map_err(|source| FileIoError::Io {
            path: parent.to_path_buf(),
            source,
        })?;
    }
    let temporary = temporary_sibling(path)?;
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|source| FileIoError::Io {
                path: temporary.clone(),
                source,
            })?;
        file.write_all(bytes).map_err(|source| FileIoError::Io {
            path: temporary.clone(),
            source,
        })?;
        file.sync_all().map_err(|source| FileIoError::Io {
            path: temporary.clone(),
            source,
        })?;
        fs::rename(&temporary, path).map_err(|source| FileIoError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        // Directory syncing is not available on every supported platform;
        // the file sync plus atomic rename is still the important contract.
        if let Ok(directory) = File::open(parent) {
            let _ = directory.sync_all();
        }
        Ok::<(), FileIoError>(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn temporary_sibling(path: &Path) -> Result<PathBuf, FileIoError> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let basename = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("recipe");
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    for _ in 0..32 {
        let counter = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        let candidate = parent.join(format!(
            ".{basename}.studio-tmp-{}-{timestamp}-{counter}",
            std::process::id()
        ));
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&candidate)
        {
            Ok(file) => {
                drop(file);
                // The caller opens it again with create_new after this probe;
                // remove the probe so a failure cannot leave a zero-byte temp.
                fs::remove_file(&candidate).map_err(|source| FileIoError::Io {
                    path: candidate.clone(),
                    source,
                })?;
                return Ok(candidate);
            }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(source) => {
                return Err(FileIoError::Io {
                    path: candidate,
                    source,
                });
            }
        }
    }
    Err(FileIoError::Io {
        path: parent.to_path_buf(),
        source: io::Error::new(
            io::ErrorKind::AlreadyExists,
            "could not allocate a unique sibling temporary file",
        ),
    })
}

/// Extract a typed external-change conflict from a file error, if present.
#[must_use]
pub fn conflict_from_error(error: &FileIoError) -> Option<&ExternalChangeConflict> {
    match error {
        FileIoError::Conflict(conflict) => Some(conflict),
        FileIoError::Io { source, .. } => source
            .get_ref()
            .and_then(|error| error.downcast_ref::<ExternalChangeConflict>()),
        _ => None,
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

    use motu::procedural_textures::{
        AlbedoSettings, DisplacementSettings, MaterialModel, OcclusionRecipeSettings,
        TextureRecipe, recipe::OutputProfile,
    };

    use super::{
        ExternalChangeConflict, canonical_json_bytes, conflict_from_error, parse_recipe_bytes,
        read_recipe, sha256_hex, write_bytes_atomic,
    };

    static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

    fn recipe() -> TextureRecipe {
        TextureRecipe {
            name: "file-io-test".into(),
            seed: 9,
            width: 4,
            height: 3,
            physical_tile_width_m: 1.0,
            physical_tile_height_m: 1.0,
            material: MaterialModel::default(),
            layers: Vec::new(),
            normal_convention: motu::procedural_textures::NormalConvention::default(),
            normal_scale: 1.0,
            displacement: DisplacementSettings::default(),
            occlusion: OcclusionRecipeSettings::default(),
            albedo: AlbedoSettings::default(),
            output_profiles: vec![OutputProfile::Separate],
        }
    }

    fn temporary_directory() -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock after epoch")
            .as_nanos();
        let counter = TEST_COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "island-material-studio-file-{}-{nanos}-{counter}",
            std::process::id()
        ));
        fs::create_dir_all(&path).expect("temporary directory");
        path
    }

    #[test]
    fn canonical_roundtrip_and_hash_are_stable() {
        let value = recipe();
        let bytes = canonical_json_bytes(&value).expect("serialize");
        let path = PathBuf::from("canonical.json");
        let loaded = parse_recipe_bytes(&path, &bytes).expect("parse canonical");
        assert_eq!(loaded.recipe, value);
        assert_eq!(loaded.canonical_form, bytes);
        assert_eq!(loaded.source_hash, sha256_hex(&bytes));
    }

    #[test]
    fn invalid_json_is_rejected_before_any_document_mutation() {
        let error = parse_recipe_bytes(PathBuf::from("bad.json").as_path(), b"not-json")
            .expect_err("invalid JSON");
        assert!(conflict_from_error(&error).is_none());
    }

    #[test]
    fn atomic_write_replaces_target_and_leaves_no_temp_files() {
        let directory = temporary_directory();
        let path = directory.join("recipe.json");
        write_bytes_atomic(&path, b"old\n").expect("initial write");
        write_bytes_atomic(&path, b"new\n").expect("replacement write");
        assert_eq!(fs::read(&path).expect("target readable"), b"new\n");
        let leftovers = fs::read_dir(&directory)
            .expect("directory readable")
            .filter_map(Result::ok)
            .filter(|entry| entry.file_name().to_string_lossy().contains("studio-tmp"))
            .count();
        assert_eq!(leftovers, 0);
        fs::remove_dir_all(directory).expect("cleanup");
    }

    #[test]
    fn atomic_write_failure_keeps_previous_target_and_cleans_temp() {
        let directory = temporary_directory();
        let path = directory.join("target");
        fs::create_dir(&path).expect("directory target");
        assert!(write_bytes_atomic(&path, b"cannot replace directory").is_err());
        assert!(path.is_dir());
        let leftovers = fs::read_dir(&directory)
            .expect("directory readable")
            .filter_map(Result::ok)
            .filter(|entry| entry.file_name().to_string_lossy().contains("studio-tmp"))
            .count();
        assert_eq!(leftovers, 0);
        fs::remove_dir_all(directory).expect("cleanup");
    }

    #[test]
    fn conflict_type_is_extractable_for_callers() {
        let conflict = ExternalChangeConflict {
            path: PathBuf::from("x.json"),
            expected_hash: Some("old".into()),
            actual_hash: Some("new".into()),
        };
        let io_error = std::io::Error::other(conflict.clone());
        let error = super::FileIoError::Io {
            path: conflict.path.clone(),
            source: io_error,
        };
        assert_eq!(conflict_from_error(&error), Some(&conflict));
    }

    #[test]
    fn read_recipe_captures_raw_hash() {
        let directory = temporary_directory();
        let path = directory.join("recipe.json");
        let bytes = canonical_json_bytes(&recipe()).expect("serialize");
        fs::write(&path, &bytes).expect("write");
        let loaded = read_recipe(&path).expect("read");
        assert_eq!(loaded.source_hash, sha256_hex(&bytes));
        fs::remove_dir_all(directory).expect("cleanup");
    }
}
