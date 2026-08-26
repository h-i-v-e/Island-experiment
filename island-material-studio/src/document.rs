//! Engine-neutral document state for Procedural Material Studio.
//!
//! `StudioDocument` is the one owner of the mutable typed recipe on the UI
//! thread.  All edits pass through snapshot transactions, while background
//! preview/bake work can receive an owned `TextureRecipe` clone through
//! [`StudioDocument::recipe_snapshot`].  File parsing is deliberately staged
//! in [`crate::file_io`] before the live document is replaced, so an invalid
//! open or revert cannot destroy current authoring state.

#![allow(clippy::missing_errors_doc, clippy::missing_panics_doc)]

use std::{
    fmt,
    path::{Path, PathBuf},
};

use motu::procedural_textures::{
    LayerMask, MaterialLayer, MaterialModel, RecipeValidationErrors, TextureRecipe,
    recipe::OutputProfile, validate_recipe,
};

use crate::{
    file_io::{self, ConflictResolution, ExternalChangeConflict, FileIoError, LoadedRecipe},
    history::{DEFAULT_HISTORY_LIMIT, History},
};

/// Maximum number of snapshots retained by a new document.
pub const DOCUMENT_HISTORY_LIMIT: usize = DEFAULT_HISTORY_LIMIT;

/// Errors raised while opening, editing or saving a document.
#[derive(Debug)]
pub enum DocumentError {
    /// File parsing, validation, conflict or filesystem failure.
    File(FileIoError),
    /// A source file changed since it was opened or last saved.
    ExternalChange(ExternalChangeConflict),
    /// A caller attempted to construct a document with an invalid recipe.
    Validation(RecipeValidationErrors),
    /// Save was requested before the document had a source path.
    NoSourcePath,
    /// An operation named a layer that does not exist.
    LayerNotFound(String),
    /// A layer index or insertion position was outside the current stack.
    LayerIndex { index: usize, length: usize },
    /// A layer operation would exceed the evaluator's bounded stack.
    LayerLimit,
    /// Reordering would make an earlier-layer mask reference point forward.
    ForwardLayerReference {
        layer_id: String,
        referenced_id: String,
    },
    /// The recipe could not be serialized for a hash or dirty comparison.
    Serialization(serde_json::Error),
}

impl fmt::Display for DocumentError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::File(error) => error.fmt(formatter),
            Self::ExternalChange(error) => error.fmt(formatter),
            Self::Validation(error) => write!(formatter, "invalid recipe: {error}"),
            Self::NoSourcePath => formatter.write_str("document has no source path; use Save As"),
            Self::LayerNotFound(id) => write!(formatter, "layer {id:?} does not exist"),
            Self::LayerIndex { index, length } => {
                write!(
                    formatter,
                    "layer index {index} is outside stack length {length}"
                )
            }
            Self::LayerLimit => formatter.write_str("the layer stack is full"),
            Self::ForwardLayerReference {
                layer_id,
                referenced_id,
            } => write!(
                formatter,
                "moving layer {layer_id:?} would make its mask reference {referenced_id:?} point forward"
            ),
            Self::Serialization(error) => write!(formatter, "recipe serialization failed: {error}"),
        }
    }
}

impl std::error::Error for DocumentError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::File(error) => Some(error),
            Self::ExternalChange(error) => Some(error),
            Self::Validation(error) => Some(error),
            Self::Serialization(error) => Some(error),
            _ => None,
        }
    }
}

impl From<FileIoError> for DocumentError {
    fn from(error: FileIoError) -> Self {
        match error {
            FileIoError::Conflict(conflict) => Self::ExternalChange(conflict),
            other => Self::File(other),
        }
    }
}

/// A typed report from a layer removal.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RemovedLayer {
    /// Stable identifier of the removed layer.
    pub id: String,
    /// Number of masks repaired from a deleted layer reference to `Own`.
    pub repaired_mask_references: usize,
}

/// One mutable authoring document and its lifecycle bookkeeping.
#[derive(Clone, Debug)]
pub struct StudioDocument {
    recipe: TextureRecipe,
    source_path: Option<PathBuf>,
    source_hash: Option<String>,
    saved_canonical_form: Option<Vec<u8>>,
    dirty: bool,
    selected_layer_id: Option<String>,
    revision: u64,
    history: History<TextureRecipe>,
    last_bake_recipe_hash: Option<String>,
}

impl StudioDocument {
    /// Constructs a new unsaved document from a valid typed recipe.
    ///
    /// New documents are dirty by design: they have no source path and must be
    /// explicitly saved before a close action can discard them safely.
    pub fn new(recipe: TextureRecipe) -> Result<Self, DocumentError> {
        validate_recipe(&recipe).map_err(DocumentError::Validation)?;
        Ok(Self {
            recipe,
            source_path: None,
            source_hash: None,
            saved_canonical_form: None,
            dirty: true,
            selected_layer_id: None,
            revision: 0,
            history: History::new(DOCUMENT_HISTORY_LIMIT),
            last_bake_recipe_hash: None,
        })
    }

    /// Alias for callers that use the terminology `from_recipe`.
    pub fn from_recipe(recipe: TextureRecipe) -> Result<Self, DocumentError> {
        Self::new(recipe)
    }

    /// Constructs a valid default layered-noise document.
    #[must_use]
    pub fn new_default() -> Self {
        Self::new(default_texture_recipe()).expect("the built-in default recipe is valid")
    }

    /// Returns the programmatic default used by New.  It is assembled from
    /// `island-rs` defaults rather than maintaining a second JSON document.
    #[must_use]
    pub fn default_recipe() -> TextureRecipe {
        default_texture_recipe()
    }

    /// Opens and validates a recipe without mutating any existing document.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, DocumentError> {
        let path = path.as_ref().to_path_buf();
        let loaded = file_io::read_recipe(&path)?;
        Ok(Self::from_loaded(path, loaded))
    }

    /// Reads a replacement before mutating this document.
    ///
    /// The method is useful for an Open command after the UI has resolved any
    /// dirty-document modal.  Invalid JSON, validation failures and IO errors
    /// leave every field of the current document untouched.
    pub fn open_replace(&mut self, path: impl AsRef<Path>) -> Result<(), DocumentError> {
        let path = path.as_ref().to_path_buf();
        let loaded = file_io::read_recipe(&path)?;
        *self = Self::from_loaded(path, loaded);
        Ok(())
    }

    /// Alias emphasizing that opening is staged before replacement.
    pub fn replace_from_path(&mut self, path: impl AsRef<Path>) -> Result<(), DocumentError> {
        self.open_replace(path)
    }

    fn from_loaded(path: PathBuf, loaded: LoadedRecipe) -> Self {
        let selected_layer_id = None;
        Self {
            recipe: loaded.recipe,
            source_path: Some(path),
            source_hash: Some(loaded.source_hash),
            saved_canonical_form: Some(loaded.canonical_form),
            dirty: false,
            selected_layer_id,
            revision: 0,
            history: History::new(DOCUMENT_HISTORY_LIMIT),
            last_bake_recipe_hash: None,
        }
    }

    /// Borrows the current typed recipe.
    #[must_use]
    pub const fn recipe(&self) -> &TextureRecipe {
        &self.recipe
    }

    /// Clones the current recipe for an independent preview/bake task.
    #[must_use]
    pub fn recipe_snapshot(&self) -> TextureRecipe {
        self.recipe.clone()
    }

    /// Returns the source path, if this document was opened or saved.
    #[must_use]
    pub fn source_path(&self) -> Option<&Path> {
        self.source_path.as_deref()
    }

    /// Returns the exact source-byte hash captured at open or last save.
    #[must_use]
    pub fn source_hash(&self) -> Option<&str> {
        self.source_hash.as_deref()
    }

    /// Alias using the terminology from the plan.
    #[must_use]
    pub fn source_byte_hash(&self) -> Option<&str> {
        self.source_hash()
    }

    /// Returns the canonical saved representation, including its newline.
    #[must_use]
    pub fn saved_canonical_form(&self) -> Option<&[u8]> {
        self.saved_canonical_form.as_deref()
    }

    /// Alias spelling out that this is the last successfully saved form.
    #[must_use]
    pub fn last_saved_canonical_form(&self) -> Option<&[u8]> {
        self.saved_canonical_form()
    }

    /// Canonical serialization of the current recipe.
    pub fn current_canonical_form(&self) -> Result<Vec<u8>, DocumentError> {
        file_io::canonical_json_bytes(&self.recipe).map_err(DocumentError::File)
    }

    /// Returns whether edits differ from the last successful saved form.
    #[must_use]
    pub const fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// Monotonic edit revision for preview stale-result rejection.
    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    /// Selected stable layer identifier, if any.
    #[must_use]
    pub fn selected_layer_id(&self) -> Option<&str> {
        self.selected_layer_id.as_deref()
    }

    /// Alias used by UI code that treats selection as a layer field.
    #[must_use]
    pub fn selected_layer(&self) -> Option<&str> {
        self.selected_layer_id()
    }

    /// Last recipe hash reported by a successful full bake.
    #[must_use]
    pub fn last_bake_recipe_hash(&self) -> Option<&str> {
        self.last_bake_recipe_hash.as_deref()
    }

    /// Alias used by status panels.
    #[must_use]
    pub fn last_successful_bake_hash(&self) -> Option<&str> {
        self.last_bake_recipe_hash()
    }

    /// Alias used by bake status panels.
    #[must_use]
    pub fn last_bake_hash(&self) -> Option<&str> {
        self.last_bake_recipe_hash()
    }

    /// Borrows the bounded snapshot history for command enablement/status.
    #[must_use]
    pub const fn history(&self) -> &History<TextureRecipe> {
        &self.history
    }

    /// Sets selection only when the stable ID exists, or clears it with None.
    pub fn select_layer(&mut self, id: Option<&str>) -> bool {
        let next = id.map(str::to_owned);
        if next
            .as_deref()
            .is_some_and(|candidate| !self.recipe.layers.iter().any(|layer| layer.id == candidate))
        {
            return false;
        }
        if self.selected_layer_id != next {
            self.selected_layer_id = next;
        }
        true
    }

    /// Mutates the recipe and commits one snapshot transaction when it
    /// changed.  During a gesture, repeated calls are coalesced until
    /// [`Self::end_gesture`] is invoked.
    pub fn edit<F>(&mut self, edit: F) -> bool
    where
        F: FnOnce(&mut TextureRecipe),
    {
        let before = self.recipe.clone();
        edit(&mut self.recipe);
        if before == self.recipe {
            return false;
        }
        if !self.history.gesture_active() {
            self.history.record(before, self.recipe.clone());
        }
        self.bump_revision();
        true
    }

    /// Alias for command handlers that call a mutation an applied edit.
    pub fn apply_edit<F>(&mut self, edit: F) -> bool
    where
        F: FnOnce(&mut TextureRecipe),
    {
        self.edit(edit)
    }

    /// Mutates the recipe while allowing a caller-specific error to abort the
    /// transaction.  Partial closure mutations are rolled back on `Err`.
    pub fn try_edit<F, E>(&mut self, edit: F) -> Result<bool, E>
    where
        F: FnOnce(&mut TextureRecipe) -> Result<(), E>,
    {
        let before = self.recipe.clone();
        if let Err(error) = edit(&mut self.recipe) {
            self.recipe = before;
            return Err(error);
        }
        if before == self.recipe {
            return Ok(false);
        }
        if !self.history.gesture_active() {
            self.history.record(before, self.recipe.clone());
        }
        self.bump_revision();
        Ok(true)
    }

    /// Starts a gesture transaction.  Intermediate `edit` calls do not make
    /// separate undo entries.
    pub fn begin_gesture(&mut self) -> bool {
        self.history.begin_gesture(&self.recipe)
    }

    /// Commits the active gesture and returns whether it changed the recipe.
    pub fn end_gesture(&mut self) -> bool {
        if self.history.commit_gesture(&self.recipe).is_some() {
            self.bump_revision();
            true
        } else {
            false
        }
    }

    /// Cancels an active gesture and restores its starting snapshot.
    pub fn cancel_gesture(&mut self) -> bool {
        let Some(before) = self.history.cancel_gesture() else {
            return false;
        };
        if before == self.recipe {
            return false;
        }
        self.recipe = before;
        self.bump_revision();
        true
    }

    /// Undoes one committed transaction.
    pub fn undo(&mut self) -> bool {
        if self.history.gesture_active() {
            return false;
        }
        let Some(recipe) = self.history.undo(&self.recipe) else {
            return false;
        };
        self.recipe = recipe;
        self.bump_revision();
        true
    }

    /// Redoes one previously undone transaction.
    pub fn redo(&mut self) -> bool {
        if self.history.gesture_active() {
            return false;
        }
        let Some(recipe) = self.history.redo(&self.recipe) else {
            return false;
        };
        self.recipe = recipe;
        self.bump_revision();
        true
    }

    /// Computes the compact normalized recipe hash used by island-rs output
    /// metadata.  It is independent of pretty-print whitespace.
    pub fn normalized_recipe_hash(&self) -> Result<String, DocumentError> {
        let bytes = serde_json::to_vec(&self.recipe).map_err(DocumentError::Serialization)?;
        Ok(file_io::sha256_hex(&bytes))
    }

    /// Records a successful bake only after the output writer has completed.
    pub fn record_successful_bake(&mut self, recipe_hash: impl Into<String>) {
        self.last_bake_recipe_hash = Some(recipe_hash.into());
    }

    /// Alias for code that names the field as `mark_*`.
    pub fn mark_bake_successful(&mut self, recipe_hash: impl Into<String>) {
        self.record_successful_bake(recipe_hash);
    }

    /// Adds one default layer with a stable unique ID and selects it.
    pub fn add_layer(&mut self, name: impl Into<String>) -> Result<String, DocumentError> {
        if self.recipe.layers.len() >= motu::procedural_textures::recipe::MAX_LAYERS {
            return Err(DocumentError::LayerLimit);
        }
        let name = name.into();
        let id = unique_layer_id(&self.recipe.layers, &name);
        let mut layer = MaterialLayer::default();
        layer.id.clone_from(&id);
        layer.name = if name.trim().is_empty() {
            "Layer".into()
        } else {
            name
        };
        self.edit(|recipe| recipe.layers.push(layer));
        self.selected_layer_id = Some(id.clone());
        Ok(id)
    }

    /// Duplicates a layer immediately after its current position.
    pub fn duplicate_layer(&mut self, id: &str) -> Result<String, DocumentError> {
        let index = self.layer_index(id)?;
        self.duplicate_layer_at(index)
    }

    /// Duplicates a layer by index, assigning a new stable ID.
    pub fn duplicate_layer_at(&mut self, index: usize) -> Result<String, DocumentError> {
        let layer = self
            .recipe
            .layers
            .get(index)
            .cloned()
            .ok_or(DocumentError::LayerIndex {
                index,
                length: self.recipe.layers.len(),
            })?;
        if self.recipe.layers.len() >= motu::procedural_textures::recipe::MAX_LAYERS {
            return Err(DocumentError::LayerLimit);
        }
        let id = unique_layer_id(&self.recipe.layers, &format!("{}-copy", layer.id));
        let mut duplicate = layer;
        duplicate.id.clone_from(&id);
        duplicate.name.push_str(" Copy");
        self.edit(|recipe| recipe.layers.insert(index + 1, duplicate));
        self.selected_layer_id = Some(id.clone());
        Ok(id)
    }

    /// Renames a layer without changing its stable ID.
    pub fn rename_layer(&mut self, id: &str, name: impl Into<String>) -> Result<(), DocumentError> {
        let index = self.layer_index(id)?;
        let name = name.into();
        self.edit(|recipe| recipe.layers[index].name = name);
        Ok(())
    }

    /// Enables or disables a layer without changing its stable ID.
    pub fn set_layer_enabled(&mut self, id: &str, enabled: bool) -> Result<(), DocumentError> {
        let index = self.layer_index(id)?;
        self.edit(|recipe| recipe.layers[index].enabled = enabled);
        Ok(())
    }

    /// Moves a layer while rejecting an order that creates a forward mask.
    pub fn reorder_layer(&mut self, id: &str, new_index: usize) -> Result<(), DocumentError> {
        let old_index = self.layer_index(id)?;
        if new_index >= self.recipe.layers.len() {
            return Err(DocumentError::LayerIndex {
                index: new_index,
                length: self.recipe.layers.len(),
            });
        }
        if old_index == new_index {
            return Ok(());
        }
        let mut candidate = self.recipe.layers.clone();
        let layer = candidate.remove(old_index);
        candidate.insert(new_index, layer);
        if let Some((layer_id, referenced_id)) = first_forward_reference(&candidate) {
            return Err(DocumentError::ForwardLayerReference {
                layer_id,
                referenced_id,
            });
        }
        self.edit(|recipe| recipe.layers = candidate);
        Ok(())
    }

    /// Removes a layer and repairs masks that referenced it to use their own
    /// remapped scalar.  Selection moves to the nearest remaining layer.
    pub fn remove_layer(&mut self, id: &str) -> Result<RemovedLayer, DocumentError> {
        let index = self.layer_index(id)?;
        let removed_id = self.recipe.layers[index].id.clone();
        let mut repaired_mask_references = 0;
        self.edit(|recipe| {
            recipe.layers.remove(index);
            for layer in &mut recipe.layers {
                if matches!(
                    layer.mask.as_ref(),
                    Some(LayerMask::Layer { layer_id, .. }) if layer_id == &removed_id
                ) {
                    layer.mask = Some(LayerMask::Own);
                    repaired_mask_references += 1;
                }
            }
        });
        self.selected_layer_id = self
            .recipe
            .layers
            .get(index.min(self.recipe.layers.len().saturating_sub(1)))
            .map(|layer| layer.id.clone());
        Ok(RemovedLayer {
            id: removed_id,
            repaired_mask_references,
        })
    }

    /// Returns a layer's current zero-based position.
    pub fn layer_index(&self, id: &str) -> Result<usize, DocumentError> {
        self.recipe
            .layers
            .iter()
            .position(|layer| layer.id == id)
            .ok_or_else(|| DocumentError::LayerNotFound(id.into()))
    }

    /// Saves to the current source after validating and checking its captured
    /// source-byte hash.  A conflict is returned without writing anything.
    pub fn save(&mut self) -> Result<(), DocumentError> {
        let path = self
            .source_path
            .clone()
            .ok_or(DocumentError::NoSourcePath)?;
        let expected_hash = self.source_hash.as_deref();
        file_io::check_external_change(&path, expected_hash)?;
        self.write_and_mark_saved(path)
    }

    /// Intentionally overwrites the current source after validation.
    pub fn save_overwrite(&mut self) -> Result<(), DocumentError> {
        let path = self
            .source_path
            .clone()
            .ok_or(DocumentError::NoSourcePath)?;
        self.write_and_mark_saved(path)
    }

    /// Saves to a selected path, updating source identity only after the
    /// atomic write succeeds.
    pub fn save_as(&mut self, path: impl AsRef<Path>) -> Result<(), DocumentError> {
        self.write_and_mark_saved(path.as_ref().to_path_buf())
    }

    /// Applies an explicit conflict choice. Save As requires a selected path.
    pub fn resolve_conflict(
        &mut self,
        resolution: ConflictResolution,
        save_as_path: Option<&Path>,
    ) -> Result<(), DocumentError> {
        match resolution {
            ConflictResolution::Reload => self.revert(),
            ConflictResolution::SaveAs => {
                let path = save_as_path.ok_or(DocumentError::NoSourcePath)?;
                self.save_as(path)
            }
            ConflictResolution::Overwrite => self.save_overwrite(),
        }
    }

    /// Re-reads the current source and replaces the document only after the
    /// new bytes parse and validate.
    pub fn revert(&mut self) -> Result<(), DocumentError> {
        let path = self
            .source_path
            .clone()
            .ok_or(DocumentError::NoSourcePath)?;
        let loaded = file_io::read_recipe(&path)?;
        *self = Self::from_loaded(path, loaded);
        Ok(())
    }

    fn write_and_mark_saved(&mut self, path: PathBuf) -> Result<(), DocumentError> {
        validate_recipe(&self.recipe).map_err(DocumentError::Validation)?;
        let bytes = file_io::write_recipe_atomic(&path, &self.recipe)?;
        self.source_hash = Some(file_io::sha256_hex(&bytes));
        self.saved_canonical_form = Some(bytes);
        self.source_path = Some(path);
        self.dirty = false;
        Ok(())
    }

    fn bump_revision(&mut self) {
        self.revision = self.revision.wrapping_add(1);
        self.repair_selection();
        self.dirty = self
            .saved_canonical_form
            .as_deref()
            .is_none_or(|saved| self.current_canonical_form().ok().as_deref() != Some(saved));
    }

    fn repair_selection(&mut self) {
        if self
            .selected_layer_id
            .as_deref()
            .is_some_and(|id| !self.recipe.layers.iter().any(|layer| layer.id == id))
        {
            self.selected_layer_id = None;
        }
    }
}

fn unique_layer_id(layers: &[MaterialLayer], name: &str) -> String {
    let mut base = name
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '_' | '-' | '.') {
                character.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>();
    while base.starts_with('-') {
        base.remove(0);
    }
    if base.is_empty() {
        base = "layer".into();
    }
    if matches!(base.as_str(), "." | "..") {
        base = "layer".into();
    }
    let used = |candidate: &str| layers.iter().any(|layer| layer.id == candidate);
    if !used(&base) {
        return base;
    }
    (2..=motu::procedural_textures::recipe::MAX_LAYERS + 2)
        .map(|number| format!("{base}-{number}"))
        .find(|candidate| !used(candidate))
        .expect("the bounded layer stack always has a free numeric suffix")
}

fn first_forward_reference(layers: &[MaterialLayer]) -> Option<(String, String)> {
    for (index, layer) in layers.iter().enumerate() {
        if let Some(LayerMask::Layer { layer_id, .. }) = &layer.mask {
            let referenced_index = layers
                .iter()
                .position(|candidate| candidate.id == *layer_id);
            if referenced_index.is_none_or(|referenced_index| referenced_index >= index) {
                return Some((layer.id.clone(), layer_id.clone()));
            }
        }
    }
    None
}

fn default_texture_recipe() -> TextureRecipe {
    use motu::procedural_textures::{
        AlbedoSettings, DisplacementSettings, OcclusionRecipeSettings,
    };
    TextureRecipe {
        name: "UntitledMaterial".into(),
        seed: 0,
        width: 128,
        height: 128,
        physical_tile_width_m: 1.0,
        physical_tile_height_m: 1.0,
        material: MaterialModel::default(),
        layers: Vec::new(),
        normal_scale: 1.0,
        displacement: DisplacementSettings::default(),
        occlusion: OcclusionRecipeSettings::default(),
        albedo: AlbedoSettings::default(),
        output_profiles: vec![OutputProfile::Separate],
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
        AlbedoSettings, DisplacementSettings, LayerMask, MaterialLayer, MaterialModel,
        OcclusionRecipeSettings, TextureRecipe, recipe::OutputProfile,
    };

    use super::{DocumentError, StudioDocument};
    use crate::file_io::canonical_json_bytes;

    static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

    fn recipe() -> TextureRecipe {
        TextureRecipe {
            name: "document-test".into(),
            seed: 3,
            width: 4,
            height: 4,
            physical_tile_width_m: 1.0,
            physical_tile_height_m: 1.0,
            material: MaterialModel::default(),
            layers: Vec::new(),
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
            "island-material-studio-document-{}-{nanos}-{counter}",
            std::process::id()
        ));
        fs::create_dir_all(&path).expect("temporary directory");
        path
    }

    fn save_recipe(path: &std::path::Path, recipe: &TextureRecipe) {
        fs::write(path, canonical_json_bytes(recipe).expect("serialize")).expect("write recipe");
    }

    #[test]
    fn new_document_is_dirty_and_undo_to_saved_content_is_clean() {
        let directory = temporary_directory();
        let path = directory.join("recipe.json");
        save_recipe(&path, &recipe());
        let mut document = StudioDocument::open(&path).expect("open");
        assert!(!document.is_dirty());
        document.edit(|recipe| recipe.seed = 99);
        assert!(document.is_dirty());
        assert!(document.undo());
        assert!(!document.is_dirty());
        assert!(document.redo());
        assert!(document.is_dirty());
        fs::remove_dir_all(directory).expect("cleanup");
    }

    #[test]
    fn gesture_coalesces_slider_events_into_one_transaction() {
        let mut document = StudioDocument::new(recipe()).expect("valid recipe");
        assert!(document.begin_gesture());
        for seed in 1..=5 {
            document.edit(|recipe| recipe.seed = seed);
        }
        assert!(document.end_gesture());
        assert_eq!(document.history().undo_len(), 1);
        assert!(document.undo());
        assert_eq!(document.recipe().seed, 3);
    }

    #[test]
    fn invalid_open_preserves_current_document() {
        let directory = temporary_directory();
        let valid_path = directory.join("valid.json");
        let invalid_path = directory.join("invalid.json");
        save_recipe(&valid_path, &recipe());
        fs::write(&invalid_path, b"{not-json").expect("write invalid");
        let mut document = StudioDocument::open(&valid_path).expect("open valid");
        document.edit(|recipe| recipe.seed = 44);
        let before = document.recipe_snapshot();
        assert!(document.open_replace(&invalid_path).is_err());
        assert_eq!(document.recipe(), &before);
        assert!(document.is_dirty());
        fs::remove_dir_all(directory).expect("cleanup");
    }

    #[test]
    fn semantically_invalid_open_also_preserves_current_document() {
        let directory = temporary_directory();
        let valid_path = directory.join("valid.json");
        let invalid_path = directory.join("invalid.json");
        let mut invalid_recipe = recipe();
        invalid_recipe.width = 0;
        save_recipe(&valid_path, &recipe());
        save_recipe(&invalid_path, &invalid_recipe);
        let mut document = StudioDocument::open(&valid_path).expect("open valid");
        document.edit(|recipe| recipe.seed = 17);
        let before = document.recipe_snapshot();
        assert!(document.open_replace(&invalid_path).is_err());
        assert_eq!(document.recipe(), &before);
        fs::remove_dir_all(directory).expect("cleanup");
    }

    #[test]
    fn external_change_blocks_save_until_explicit_overwrite() {
        let directory = temporary_directory();
        let path = directory.join("recipe.json");
        save_recipe(&path, &recipe());
        let mut document = StudioDocument::open(&path).expect("open");
        document.edit(|recipe| recipe.seed = 5);
        fs::write(&path, b"external change\n").expect("external write");
        let error = document.save().expect_err("save must be blocked");
        assert!(matches!(error, DocumentError::ExternalChange(_)));
        assert_eq!(
            fs::read_to_string(&path).expect("read external"),
            "external change\n"
        );
        document.save_overwrite().expect("explicit overwrite");
        assert!(!document.is_dirty());
        fs::remove_dir_all(directory).expect("cleanup");
    }

    #[test]
    fn layer_operations_keep_ids_unique_and_repair_deleted_masks() {
        let mut document = StudioDocument::new(recipe()).expect("valid recipe");
        let first = document.add_layer("Detail").expect("add first");
        let second = document.add_layer("Detail").expect("add second");
        assert_ne!(first, second);
        document.edit(|recipe| {
            recipe.layers[1].mask = Some(LayerMask::Layer {
                layer_id: first.clone(),
                remap: motu::procedural_textures::ScalarRemap::default(),
            });
        });
        let duplicate = document.duplicate_layer(&first).expect("duplicate");
        assert!(![first.clone(), second.clone()].contains(&duplicate));
        let removed = document.remove_layer(&first).expect("remove");
        assert_eq!(removed.repaired_mask_references, 1);
        assert!(
            document
                .recipe()
                .layers
                .iter()
                .all(|layer| layer.id != first)
        );
        let ids: std::collections::HashSet<_> = document
            .recipe()
            .layers
            .iter()
            .map(|layer| layer.id.as_str())
            .collect();
        assert_eq!(ids.len(), document.recipe().layers.len());
    }

    #[test]
    fn reorder_rejects_forward_layer_mask() {
        let mut document = StudioDocument::new(recipe()).expect("valid recipe");
        let first = document.add_layer("First").expect("first");
        let second = document.add_layer("Second").expect("second");
        document.edit(|recipe| {
            recipe.layers[1].mask = Some(LayerMask::Layer {
                layer_id: first.clone(),
                remap: motu::procedural_textures::ScalarRemap::default(),
            });
        });
        assert!(matches!(
            document.reorder_layer(&first, 1),
            Err(DocumentError::ForwardLayerReference { .. })
        ));
        assert_eq!(document.recipe().layers[0].id, first);
        assert_eq!(document.recipe().layers[1].id, second);
    }

    #[test]
    fn save_as_roundtrips_and_updates_source_identity_only_on_success() {
        let directory = temporary_directory();
        let first_path = directory.join("first.json");
        let second_path = directory.join("second.json");
        save_recipe(&first_path, &recipe());
        let mut document = StudioDocument::open(&first_path).expect("open");
        document.edit(|recipe| recipe.seed = 8);
        document.save_as(&second_path).expect("save as");
        assert_eq!(document.source_path(), Some(second_path.as_path()));
        assert!(!document.is_dirty());
        let reopened = StudioDocument::open(&second_path).expect("reopen");
        assert_eq!(reopened.recipe(), document.recipe());
        fs::remove_dir_all(directory).expect("cleanup");
    }

    #[test]
    fn default_recipe_is_valid() {
        let document = StudioDocument::new_default();
        assert!(document.is_dirty());
        assert!(document.recipe().layers.is_empty());
    }

    #[test]
    fn committed_recipes_roundtrip_without_semantic_change() {
        let directory = temporary_directory();
        for (index, bytes) in [
            include_bytes!("../../island-rs/texture-recipes/cracked-stone.json").as_slice(),
            include_bytes!("../../island-rs/texture-recipes/rounded-river-stones.json").as_slice(),
        ]
        .into_iter()
        .enumerate()
        {
            let source = directory.join(format!("source-{index}.json"));
            let destination = directory.join(format!("roundtrip-{index}.json"));
            fs::write(&source, bytes).expect("write committed recipe");
            let mut document = StudioDocument::open(&source).expect("open committed recipe");
            assert!(!document.is_dirty());
            let original = document.recipe_snapshot();
            document.save_as(&destination).expect("save roundtrip");
            let reopened = StudioDocument::open(&destination).expect("reopen roundtrip");
            assert_eq!(reopened.recipe(), &original);
        }
        fs::remove_dir_all(directory).expect("cleanup");
    }

    #[test]
    fn mask_references_can_be_inspected_after_operations() {
        let mut document = StudioDocument::new(recipe()).expect("valid recipe");
        let id = document.add_layer("maskable").expect("add");
        document.edit(|recipe| {
            recipe.layers[0].mask = Some(LayerMask::Own);
        });
        assert_eq!(document.selected_layer_id(), Some(id.as_str()));
        assert!(document.layer_index(&id).is_ok());
    }

    #[test]
    fn duplicate_layer_preserves_source_data_but_gets_new_id() {
        let mut document = StudioDocument::new(recipe()).expect("valid recipe");
        let id = document.add_layer("source").expect("add");
        document.edit(|recipe| recipe.layers[0].enabled = false);
        let duplicate = document.duplicate_layer(&id).expect("duplicate");
        assert_ne!(id, duplicate);
        assert!(!document.recipe().layers[1].enabled);
    }

    #[test]
    fn source_hash_is_raw_bytes_while_saved_form_is_canonical() {
        let directory = temporary_directory();
        let path = directory.join("recipe.json");
        let canonical = canonical_json_bytes(&recipe()).expect("serialize");
        let mut bytes = canonical.clone();
        bytes.insert(0, b' ');
        fs::write(&path, &bytes).expect("write");
        let document = StudioDocument::open(&path).expect("open");
        assert_ne!(
            document.source_hash(),
            Some(crate::file_io::sha256_hex(&canonical).as_str())
        );
        assert_eq!(document.saved_canonical_form(), Some(canonical.as_slice()));
        fs::remove_dir_all(directory).expect("cleanup");
    }

    #[allow(dead_code)]
    fn _material_layer_type_is_available(_: MaterialLayer) {}
}
