//! Renderer-free authoring state with bounded history and protected atomic saves.

use std::{
    collections::VecDeque,
    fs::{self, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use crate::{BuildingRecipe, validate_recipe};

const HISTORY_LIMIT: usize = 96;
const MAX_RECIPE_BYTES: u64 = 4 * 1024 * 1024;
static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Debug)]
pub enum DocumentError {
    Io(io::Error),
    Json(serde_json::Error),
    Invalid(String),
    Conflict(PathBuf),
    NoPath,
}

impl std::fmt::Display for DocumentError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => e.fmt(f),
            Self::Json(e) => e.fmt(f),
            Self::Invalid(e) => f.write_str(e),
            Self::Conflict(path) => write!(
                f,
                "{} changed externally or already exists; reload or choose a new filename",
                path.display()
            ),
            Self::NoPath => f.write_str("Choose Save As for this new document"),
        }
    }
}
impl std::error::Error for DocumentError {}
impl From<io::Error> for DocumentError {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}
impl From<serde_json::Error> for DocumentError {
    fn from(e: serde_json::Error) -> Self {
        Self::Json(e)
    }
}

/// The UI owns this state; workers receive independent recipe snapshots.
#[derive(Debug)]
pub struct StudioDocument {
    recipe: BuildingRecipe,
    path: Option<PathBuf>,
    source_bytes: Option<Vec<u8>>,
    saved_recipe: Option<Vec<u8>>,
    undo: VecDeque<BuildingRecipe>,
    redo: Vec<BuildingRecipe>,
    gesture: Option<BuildingRecipe>,
    revision: u64,
}

impl StudioDocument {
    #[must_use]
    pub fn new(recipe: BuildingRecipe) -> Self {
        Self {
            recipe,
            path: None,
            source_bytes: None,
            saved_recipe: None,
            undo: VecDeque::new(),
            redo: Vec::new(),
            gesture: None,
            revision: 1,
        }
    }

    pub fn open(path: &Path) -> Result<Self, DocumentError> {
        if fs::metadata(path)?.len() > MAX_RECIPE_BYTES {
            return Err(DocumentError::Invalid(
                "Recipe exceeds the 4 MiB document limit".into(),
            ));
        }
        let source_bytes = fs::read(path)?;
        let recipe: BuildingRecipe = serde_json::from_slice(&source_bytes)?;
        validate_recipe(&recipe).map_err(|e| DocumentError::Invalid(e.to_string()))?;
        validate_dependencies(&recipe, path.parent().unwrap_or(Path::new(".")))?;
        let saved_recipe = Some(serde_json::to_vec(&recipe)?);
        Ok(Self {
            path: Some(fs::canonicalize(path)?),
            source_bytes: Some(source_bytes),
            saved_recipe,
            ..Self::new(recipe)
        })
    }

    #[must_use]
    pub fn recipe(&self) -> &BuildingRecipe {
        &self.recipe
    }
    #[must_use]
    pub fn source_path(&self) -> Option<&Path> {
        self.path.as_deref()
    }
    #[must_use]
    pub fn base_directory(&self) -> Option<&Path> {
        self.path.as_deref().and_then(Path::parent)
    }
    #[must_use]
    pub fn revision(&self) -> u64 {
        self.revision
    }
    #[must_use]
    pub fn is_dirty(&self) -> bool {
        serde_json::to_vec(&self.recipe).ok().as_ref() != self.saved_recipe.as_ref()
    }
    #[must_use]
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty() && self.gesture.is_none()
    }
    #[must_use]
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty() && self.gesture.is_none()
    }

    pub fn begin_gesture(&mut self) {
        if self.gesture.is_none() {
            self.gesture = Some(self.recipe.clone());
        }
    }

    pub fn end_gesture(&mut self) {
        if let Some(before) = self.gesture.take()
            && !equal(&before, &self.recipe)
        {
            self.record(before);
        }
    }

    pub fn edit(&mut self, edit: impl FnOnce(&mut BuildingRecipe)) -> bool {
        let before = self.recipe.clone();
        edit(&mut self.recipe);
        if equal(&before, &self.recipe) {
            return false;
        }
        if self.gesture.is_none() {
            self.record(before);
        }
        self.revision += 1;
        true
    }

    fn record(&mut self, before: BuildingRecipe) {
        if self.undo.len() == HISTORY_LIMIT {
            self.undo.pop_front();
        }
        self.undo.push_back(before);
        self.redo.clear();
    }

    pub fn undo(&mut self) {
        self.end_gesture();
        if let Some(before) = self.undo.pop_back() {
            self.redo.push(std::mem::replace(&mut self.recipe, before));
            self.revision += 1;
        }
    }

    pub fn redo(&mut self) {
        self.end_gesture();
        if let Some(after) = self.redo.pop() {
            self.undo
                .push_back(std::mem::replace(&mut self.recipe, after));
            self.revision += 1;
        }
    }

    /// Reload is staged, so bad JSON cannot destroy a valid working draft.
    pub fn revert(&mut self) -> Result<(), DocumentError> {
        let mut replacement = Self::open(self.path.as_deref().ok_or(DocumentError::NoPath)?)?;
        replacement.revision = self.revision + 1;
        *self = replacement;
        Ok(())
    }

    pub fn save(&mut self) -> Result<(), DocumentError> {
        let path = self.path.clone().ok_or(DocumentError::NoPath)?;
        let expected = self.source_bytes.take();
        let result = self.write(&path, expected.as_deref());
        if result.is_err() {
            self.source_bytes = expected;
        }
        result
    }

    /// Save As creates a new file. Existing destinations are deliberately protected.
    pub fn save_as(&mut self, path: &Path) -> Result<(), DocumentError> {
        if self.path.as_deref().is_some_and(|current| current == path) {
            return self.save();
        }
        // A moved document must keep resolving the same material dependencies.
        let mut relocated = self.recipe.clone();
        {
            let base = self.base_directory().unwrap_or(Path::new("."));
            for reference in [
                &mut relocated.materials.wall_recipe,
                &mut relocated.materials.roof_recipe,
                &mut relocated.materials.door_recipe,
                &mut relocated.materials.chimney_recipe,
                &mut relocated.materials.timber_recipe,
            ]
            .into_iter()
            .flatten()
            {
                if Path::new(&*reference).is_relative() {
                    *reference = fs::canonicalize(base.join(&*reference))?
                        .to_string_lossy()
                        .into_owned();
                }
            }
        }
        let before = std::mem::replace(&mut self.recipe, relocated);
        if let Err(error) = self.write(path, None) {
            self.recipe = before;
            return Err(error);
        }
        if !equal(&before, &self.recipe) {
            self.revision += 1;
        }
        Ok(())
    }

    fn write(&mut self, path: &Path, expected: Option<&[u8]>) -> Result<(), DocumentError> {
        validate_recipe(&self.recipe).map_err(|e| DocumentError::Invalid(e.to_string()))?;
        validate_dependencies(
            &self.recipe,
            self.base_directory().unwrap_or(Path::new(".")),
        )?;
        let mut bytes = serde_json::to_vec_pretty(&self.recipe)?;
        bytes.push(b'\n');
        protected_write(path, &bytes, expected)?;
        self.path = Some(fs::canonicalize(path)?);
        self.source_bytes = Some(bytes);
        self.saved_recipe = Some(serde_json::to_vec(&self.recipe)?);
        Ok(())
    }
}

fn equal(a: &BuildingRecipe, b: &BuildingRecipe) -> bool {
    serde_json::to_vec(a).ok() == serde_json::to_vec(b).ok()
}

fn validate_dependencies(recipe: &BuildingRecipe, base: &Path) -> Result<(), DocumentError> {
    for path in [
        &recipe.materials.wall_recipe,
        &recipe.materials.roof_recipe,
        &recipe.materials.door_recipe,
        &recipe.materials.chimney_recipe,
        &recipe.materials.timber_recipe,
    ]
    .into_iter()
    .flatten()
    {
        let source = base.join(path);
        if fs::metadata(&source)?.len() > MAX_RECIPE_BYTES {
            return Err(DocumentError::Invalid(format!(
                "Material {path} exceeds the 4 MiB recipe limit"
            )));
        }
        let bytes = fs::read(source)?;
        let material: motu::procedural_textures::TextureRecipe = serde_json::from_slice(&bytes)?;
        motu::procedural_textures::validate_recipe(&material)
            .map_err(|e| DocumentError::Invalid(format!("Material {path}: {e}")))?;
    }
    Ok(())
}

/// Write and fsync a sibling temporary, recheck source bytes, then publish atomically.
/// A new destination is published with a hard link, providing create-only semantics.
pub(crate) fn protected_write(
    path: &Path,
    bytes: &[u8],
    expected: Option<&[u8]>,
) -> Result<(), DocumentError> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let temporary = parent.join(format!(
        ".building-{}-{}.tmp",
        std::process::id(),
        TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        let actual = match fs::read(path) {
            Ok(bytes) => Some(bytes),
            Err(e) if e.kind() == io::ErrorKind::NotFound => None,
            Err(e) => return Err(e.into()),
        };
        if actual.as_deref() != expected {
            return Err(DocumentError::Conflict(path.to_path_buf()));
        }
        if expected.is_none() {
            fs::hard_link(&temporary, path).map_err(|e| {
                if e.kind() == io::ErrorKind::AlreadyExists {
                    DocumentError::Conflict(path.to_path_buf())
                } else {
                    e.into()
                }
            })?;
        } else {
            if fs::symlink_metadata(path)?.file_type().is_symlink() {
                return Err(DocumentError::Conflict(path.to_path_buf()));
            }
            fs::rename(&temporary, path)?;
        }
        if let Ok(directory) = fs::File::open(parent) {
            directory.sync_all()?;
        }
        Ok(())
    })();
    let _ = fs::remove_file(&temporary);
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    fn path(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "building-document-{}-{}-{label}.json",
            std::process::id(),
            TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ))
    }

    #[test]
    fn gesture_is_one_undo_transaction_and_new_edit_drops_redo() {
        let mut doc = StudioDocument::new(BuildingRecipe::default());
        let original = doc.recipe().seed;
        doc.begin_gesture();
        for seed in 100..110 {
            doc.edit(|r| r.seed = seed);
        }
        doc.end_gesture();
        doc.undo();
        assert_eq!(doc.recipe().seed, original);
        doc.redo();
        assert_eq!(doc.recipe().seed, 109);
        doc.undo();
        doc.edit(|r| r.seed = 999);
        assert!(!doc.can_redo());
    }

    #[test]
    fn save_detects_external_change_and_failed_revert_keeps_draft() {
        let target = path("external");
        let mut doc = StudioDocument::new(BuildingRecipe::default());
        doc.save_as(&target).unwrap();
        assert!(!doc.is_dirty());
        doc.edit(|r| r.seed = 876);
        fs::write(&target, b"external editor").unwrap();
        assert!(matches!(doc.save(), Err(DocumentError::Conflict(_))));
        assert!(doc.revert().is_err());
        assert_eq!(doc.recipe().seed, 876);
        assert_eq!(fs::read(&target).unwrap(), b"external editor");
        fs::remove_file(target).unwrap();
    }

    #[test]
    fn history_is_bounded_and_save_as_never_overwrites() {
        let target = path("protected");
        fs::write(&target, b"preserve").unwrap();
        let mut doc = StudioDocument::new(BuildingRecipe::default());
        assert!(matches!(
            doc.save_as(&target),
            Err(DocumentError::Conflict(_))
        ));
        for seed in 0..200 {
            doc.edit(|r| r.seed = seed);
        }
        assert_eq!(doc.undo.len(), HISTORY_LIMIT);
        assert_eq!(fs::read(&target).unwrap(), b"preserve");
        fs::remove_file(target).unwrap();
    }

    #[test]
    fn moving_a_building_document_preserves_its_door_material_dependency() {
        let source = path("door-source").with_extension("");
        let destination = path("door-moved").with_extension("");
        fs::create_dir(&source).unwrap();
        fs::create_dir(&destination).unwrap();
        fs::write(
            source.join("wood.json"),
            include_bytes!("../../island-rs/texture-recipes/wooden-planks.json"),
        )
        .unwrap();
        let mut recipe = BuildingRecipe::hut();
        recipe.materials.door_recipe = Some("wood.json".into());
        fs::write(
            source.join("building.json"),
            serde_json::to_vec(&recipe).unwrap(),
        )
        .unwrap();
        let mut document = StudioDocument::open(&source.join("building.json")).unwrap();
        document
            .save_as(&destination.join("building.json"))
            .unwrap();
        let loaded = StudioDocument::open(&destination.join("building.json")).unwrap();
        let reference = loaded.recipe().materials.door_recipe.as_deref().unwrap();
        assert_eq!(
            fs::canonicalize(reference).unwrap(),
            fs::canonicalize(source.join("wood.json")).unwrap()
        );
        fs::remove_dir_all(source).unwrap();
        fs::remove_dir_all(destination).unwrap();
    }

    #[test]
    fn save_as_and_reopen_preserve_chimney_and_timber_materials_and_build_identity() {
        let source = path("chimney-source").with_extension("");
        let destination = path("chimney-moved").with_extension("");
        fs::create_dir(&source).unwrap();
        fs::create_dir(&destination).unwrap();
        fs::write(
            source.join("stone.json"),
            include_bytes!("../../island-rs/texture-recipes/coursed-stone.json"),
        )
        .unwrap();
        let mut recipe = BuildingRecipe::hut();
        recipe.atlas_size = 128;
        recipe.materials.chimney_recipe = Some("stone.json".into());
        recipe.walls.style = crate::WallStyle::WattleAndDaub;
        recipe.materials.timber_recipe = Some("wood.json".into());
        fs::write(
            source.join("wood.json"),
            include_bytes!("../../island-rs/texture-recipes/wooden-planks.json"),
        )
        .unwrap();
        fs::write(
            source.join("building.json"),
            serde_json::to_vec(&recipe).unwrap(),
        )
        .unwrap();
        let mut document = StudioDocument::open(&source.join("building.json")).unwrap();
        let original =
            crate::generate_with_base(document.recipe(), document.base_directory().unwrap())
                .unwrap();
        document
            .save_as(&destination.join("building.json"))
            .unwrap();
        let reopened = StudioDocument::open(&destination.join("building.json")).unwrap();
        assert!(!reopened.is_dirty());
        let reference = reopened
            .recipe()
            .materials
            .chimney_recipe
            .as_deref()
            .unwrap();
        assert_eq!(
            fs::canonicalize(reference).unwrap(),
            fs::canonicalize(source.join("stone.json")).unwrap()
        );
        let regenerated =
            crate::generate_with_base(reopened.recipe(), reopened.base_directory().unwrap())
                .unwrap();
        assert_eq!(
            fs::canonicalize(
                reopened
                    .recipe()
                    .materials
                    .timber_recipe
                    .as_deref()
                    .unwrap()
            )
            .unwrap(),
            fs::canonicalize(source.join("wood.json")).unwrap()
        );
        assert_eq!(regenerated.hash, original.hash);
        assert_eq!(regenerated.atlas, original.atlas);
        fs::remove_dir_all(source).unwrap();
        fs::remove_dir_all(destination).unwrap();
    }
}
