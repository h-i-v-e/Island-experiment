//! Debounced background preview scheduling and atomic GPU upload.

use std::{
    collections::VecDeque,
    sync::Arc,
    time::{Duration, Instant},
};

use bevy::{
    asset::Assets,
    prelude::{App, Handle, Image, IntoScheduleConfigs, Plugin, ResMut, Resource, Update},
    tasks::{AsyncComputeTaskPool, Task, futures::check_ready},
};
use bevy_egui::{EguiTextureHandle, EguiUserTextures, egui};
use motu::procedural_textures::{
    NormalConvention, PreviewMaps, PreviewSettings, TextureRecipe, editor_protocol,
    generate_preview, validate_recipe,
};

use crate::preview_images::{LayerImageSet, convert_preview_maps};

const PREVIEW_DEBOUNCE: Duration = Duration::from_millis(300);
const CACHE_CAPACITY: usize = 8;

/// Resolution used by the interactive preview.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum PreviewResolution {
    Small,
    #[default]
    Medium,
    Large,
}

impl PreviewResolution {
    #[must_use]
    pub const fn pixels(self) -> u32 {
        match self {
            Self::Small => 128,
            Self::Medium => 256,
            Self::Large => 512,
        }
    }
}

/// Identity for one complete coherent preview result.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct PreviewKey {
    pub recipe_hash: String,
    pub dimensions: [u32; 2],
    pub selected_layer_id: Option<String>,
}

#[derive(Clone)]
struct PreviewRequest {
    revision: u64,
    key: PreviewKey,
    recipe: TextureRecipe,
    manual: bool,
    queued_at: Instant,
}

struct PreviewResponse {
    revision: u64,
    key: PreviewKey,
    result: Result<PreviewMaps, String>,
}

struct RunningPreview(Task<PreviewResponse>);

#[derive(Clone)]
struct CacheEntry {
    key: PreviewKey,
    maps: Arc<PreviewMaps>,
}

/// UI-facing preview scheduler state.
#[derive(Resource)]
pub struct PreviewState {
    pub auto_preview: bool,
    pub resolution: PreviewResolution,
    pub nearest_filtering: bool,
    pub status: String,
    pub error: Option<String>,
    desired_revision: u64,
    desired_key: Option<PreviewKey>,
    pending: Option<PreviewRequest>,
    running: Option<RunningPreview>,
    ready: Option<(u64, PreviewKey, Arc<PreviewMaps>)>,
    cache: VecDeque<CacheEntry>,
}

impl Default for PreviewState {
    fn default() -> Self {
        Self {
            auto_preview: true,
            resolution: PreviewResolution::default(),
            nearest_filtering: false,
            status: "Preview has not been generated".into(),
            error: None,
            desired_revision: 0,
            desired_key: None,
            pending: None,
            running: None,
            ready: None,
            cache: VecDeque::new(),
        }
    }
}

impl PreviewState {
    /// Queues the newest document snapshot. One running request is retained;
    /// every additional edit replaces the single pending request.
    pub fn request(
        &mut self,
        mut recipe: TextureRecipe,
        revision: u64,
        selected_layer_id: Option<String>,
        manual: bool,
    ) {
        if !manual && !self.auto_preview {
            return;
        }
        let pixels = self.resolution.pixels();
        recipe.width = pixels;
        recipe.height = pixels;
        // A newer edit supersedes any pending/running work immediately. A
        // running task cannot be cancelled here, but its response will fail
        // the revision/key check below and therefore cannot replace the last
        // valid preview.
        self.desired_revision = revision;
        self.desired_key = None;
        self.pending = None;
        if let Err(errors) = validate_recipe(&recipe) {
            self.error = Some(errors.to_string());
            self.status = "Recipe is invalid; showing the last valid preview".into();
            return;
        }
        let recipe_hash = match editor_protocol::recipe_hash(&recipe) {
            Ok(hash) => hash,
            Err(error) => {
                self.error = Some(error);
                return;
            }
        };
        let key = PreviewKey {
            recipe_hash,
            dimensions: [pixels, pixels],
            selected_layer_id,
        };
        self.desired_key = Some(key.clone());
        self.error = None;

        if let Some(maps) = self.cache_get(&key) {
            self.pending = None;
            self.ready = Some((revision, key, maps));
            self.status = "Preview restored from cache".into();
            return;
        }
        self.pending = Some(PreviewRequest {
            revision,
            key,
            recipe,
            manual,
            queued_at: Instant::now(),
        });
        self.status = "Preview queued".into();
    }

    #[must_use]
    pub const fn is_running(&self) -> bool {
        self.running.is_some()
    }

    #[must_use]
    pub fn cached_entries(&self) -> usize {
        self.cache.len()
    }

    fn cache_get(&mut self, key: &PreviewKey) -> Option<Arc<PreviewMaps>> {
        let index = self.cache.iter().position(|entry| &entry.key == key)?;
        let entry = self.cache.remove(index)?;
        let maps = Arc::clone(&entry.maps);
        self.cache.push_back(entry);
        Some(maps)
    }

    fn cache_insert(&mut self, key: PreviewKey, maps: Arc<PreviewMaps>) {
        self.cache.retain(|entry| entry.key != key);
        self.cache.push_back(CacheEntry { key, maps });
        while self.cache.len() > CACHE_CAPACITY {
            self.cache.pop_front();
        }
    }
}

/// One uploaded image and its egui registration.
#[derive(Clone, Debug)]
pub struct RegisteredImage {
    pub handle: Handle<Image>,
    pub texture_id: egui::TextureId,
}

/// Complete displayed map set. Replaced only after all image uploads succeed.
#[derive(Resource, Default)]
pub struct PreviewAssets {
    pub revision: u64,
    pub maps: Option<Arc<PreviewMaps>>,
    pub albedo: Option<RegisteredImage>,
    pub height: Option<RegisteredImage>,
    pub normal: Option<RegisteredImage>,
    pub occlusion: Option<RegisteredImage>,
    pub packed_mask: Option<RegisteredImage>,
    pub depth: Option<Handle<Image>>,
    pub layer_raw: Option<RegisteredImage>,
    pub layer_remapped: Option<RegisteredImage>,
    pub layer_mask: Option<RegisteredImage>,
}

pub struct PreviewPlugin;

impl Plugin for PreviewPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PreviewState>()
            .init_resource::<PreviewAssets>()
            .add_systems(Update, (drive_preview_tasks, upload_ready_preview).chain());
    }
}

fn drive_preview_tasks(mut state: ResMut<PreviewState>) {
    if let Some(running) = &mut state.running
        && let Some(response) = check_ready(&mut running.0)
    {
        state.running = None;
        match response.result {
            Ok(maps) => {
                let maps = Arc::new(maps);
                state.cache_insert(response.key.clone(), Arc::clone(&maps));
                if response.revision == state.desired_revision
                    && state.desired_key.as_ref() == Some(&response.key)
                {
                    state.ready = Some((response.revision, response.key, maps));
                    state.status = "Preview ready".into();
                    state.error = None;
                }
            }
            Err(error) => {
                if response.revision == state.desired_revision {
                    state.error = Some(error);
                    state.status = "Preview failed; showing the last valid result".into();
                }
            }
        }
    }
    if state.running.is_some() {
        return;
    }
    let should_start = state
        .pending
        .as_ref()
        .is_some_and(|request| request.manual || request.queued_at.elapsed() >= PREVIEW_DEBOUNCE);
    if !should_start {
        return;
    }
    let request = state
        .pending
        .take()
        .expect("start requires a pending request");
    state.status = "Generating preview…".into();
    state.running = Some(RunningPreview(AsyncComputeTaskPool::get().spawn(
        async move {
            let settings = PreviewSettings {
                normal_convention: NormalConvention::OpenGl,
                selected_layer_id: request.key.selected_layer_id.clone(),
            };
            PreviewResponse {
                revision: request.revision,
                key: request.key,
                result: generate_preview(&request.recipe, &settings)
                    .map_err(|error| error.to_string()),
            }
        },
    )));
}

fn upload_ready_preview(
    mut state: ResMut<PreviewState>,
    mut assets: ResMut<PreviewAssets>,
    mut images: ResMut<Assets<Image>>,
    mut egui_textures: ResMut<EguiUserTextures>,
) {
    let Some((revision, _key, maps)) = state.ready.take() else {
        return;
    };
    let selected = maps.selected_layer.as_ref().map(|layer| LayerImageSet {
        raw: &layer.raw,
        remapped: &layer.remapped,
        mask: &layer.mask,
    });
    let converted = convert_preview_maps(&maps, selected, state.nearest_filtering);
    let replacement = PreviewAssets {
        revision,
        maps: Some(maps),
        albedo: Some(register(converted.albedo, &mut images, &mut egui_textures)),
        height: Some(register(converted.height, &mut images, &mut egui_textures)),
        normal: Some(register(converted.normal, &mut images, &mut egui_textures)),
        occlusion: Some(register(
            converted.occlusion,
            &mut images,
            &mut egui_textures,
        )),
        packed_mask: Some(register(
            converted.packed_mask,
            &mut images,
            &mut egui_textures,
        )),
        depth: Some(images.add(converted.depth)),
        layer_raw: converted
            .layer_raw
            .map(|image| register(image, &mut images, &mut egui_textures)),
        layer_remapped: converted
            .layer_remapped
            .map(|image| register(image, &mut images, &mut egui_textures)),
        layer_mask: converted
            .layer_mask
            .map(|image| register(image, &mut images, &mut egui_textures)),
    };
    release_assets(&assets, &mut images, &mut egui_textures);
    *assets = replacement;
}

fn register(
    image: Image,
    images: &mut Assets<Image>,
    egui_textures: &mut EguiUserTextures,
) -> RegisteredImage {
    let handle = images.add(image);
    let texture_id = egui_textures.add_image(EguiTextureHandle::Strong(handle.clone()));
    RegisteredImage { handle, texture_id }
}

fn release_assets(
    assets: &PreviewAssets,
    images: &mut Assets<Image>,
    egui_textures: &mut EguiUserTextures,
) {
    for registered in [
        assets.albedo.as_ref(),
        assets.height.as_ref(),
        assets.normal.as_ref(),
        assets.occlusion.as_ref(),
        assets.packed_mask.as_ref(),
        assets.layer_raw.as_ref(),
        assets.layer_remapped.as_ref(),
        assets.layer_mask.as_ref(),
    ]
    .into_iter()
    .flatten()
    {
        egui_textures.remove_image(registered.handle.id());
        images.remove(registered.handle.id());
    }
    if let Some(depth) = &assets.depth {
        images.remove(depth.id());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn recipe() -> TextureRecipe {
        serde_json::from_str(include_str!(
            "../../island-rs/texture-recipes/cracked-stone.json"
        ))
        .unwrap()
    }

    #[test]
    fn newest_pending_request_replaces_older_edits() {
        let mut state = PreviewState::default();
        state.request(recipe(), 1, None, false);
        state.request(recipe(), 2, Some("detail".into()), false);
        assert_eq!(
            state.pending.as_ref().map(|request| request.revision),
            Some(2)
        );
        assert_eq!(state.desired_revision, 2);
    }

    #[test]
    fn invalid_newer_edit_supersedes_older_pending_work() {
        let mut state = PreviewState::default();
        state.request(recipe(), 1, None, false);
        let mut invalid = recipe();
        invalid.name.clear();
        state.request(invalid, 2, None, false);
        assert_eq!(state.desired_revision, 2);
        assert!(state.desired_key.is_none());
        assert!(state.pending.is_none());
        assert!(state.error.is_some());
    }

    #[test]
    fn cache_is_bounded_and_keys_include_selected_layer() {
        let maps = Arc::new(
            generate_preview(
                &recipe(),
                &PreviewSettings {
                    normal_convention: NormalConvention::OpenGl,
                    selected_layer_id: None,
                },
            )
            .expect("preview fixture"),
        );
        let mut state = PreviewState::default();
        for index in 0..10 {
            state.cache_insert(
                PreviewKey {
                    recipe_hash: index.to_string(),
                    dimensions: [128, 128],
                    selected_layer_id: (index == 9).then(|| "selected".into()),
                },
                Arc::clone(&maps),
            );
        }
        assert_eq!(state.cached_entries(), CACHE_CAPACITY);
        assert!(
            state
                .cache
                .iter()
                .any(|entry| { entry.key.selected_layer_id.as_deref() == Some("selected") })
        );
    }

    #[test]
    fn cache_hit_clears_an_older_pending_request() {
        let mut recipe = recipe();
        recipe.width = PreviewResolution::default().pixels();
        recipe.height = PreviewResolution::default().pixels();
        let maps = Arc::new(
            generate_preview(
                &recipe,
                &PreviewSettings {
                    normal_convention: NormalConvention::OpenGl,
                    selected_layer_id: None,
                },
            )
            .expect("preview fixture"),
        );
        let mut state = PreviewState::default();
        state.request(recipe.clone(), 1, None, false);
        let cached_key = state
            .pending
            .as_ref()
            .expect("first preview is pending")
            .key
            .clone();
        state.cache_insert(cached_key, Arc::clone(&maps));
        state.request(recipe, 2, None, false);
        assert!(state.pending.is_none());
        assert_eq!(state.ready.as_ref().map(|ready| ready.0), Some(2));
    }
}
