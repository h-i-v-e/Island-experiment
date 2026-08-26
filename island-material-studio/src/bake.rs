//! Background transactional full-resolution baking.

use std::{path::PathBuf, time::Instant};

use bevy::{
    prelude::{App, Plugin, ResMut, Resource, Update},
    tasks::{AsyncComputeTaskPool, Task, futures::check_ready},
};
use motu::procedural_textures::{
    NormalConvention, OutputOptions, OutputProfile, TextureRecipe, generate_texture_set,
    write_texture_set,
};

/// Successful bake information displayed by the status panel.
#[derive(Clone, Debug)]
pub struct BakeSuccess {
    pub recipe_hash: String,
    pub manifest_path: PathBuf,
    pub elapsed_ms: f64,
    pub map_count: usize,
}

#[derive(Clone, Debug)]
struct BakeRequest {
    recipe: TextureRecipe,
    output: PathBuf,
    options: OutputOptions,
}

struct BakeTask(Task<Result<BakeSuccess, String>>);

/// UI-facing final-bake state.
#[derive(Resource)]
pub struct BakeState {
    pub output_directory: String,
    pub profile: OutputProfile,
    pub overwrite: bool,
    pub status: String,
    pub last_success: Option<BakeSuccess>,
    pub error: Option<String>,
    pending: Option<BakeRequest>,
    running: Option<BakeTask>,
}

impl Default for BakeState {
    fn default() -> Self {
        Self {
            output_directory: String::new(),
            profile: OutputProfile::Separate,
            overwrite: false,
            status: "No bake has run".into(),
            last_success: None,
            error: None,
            pending: None,
            running: None,
        }
    }
}

impl BakeState {
    /// Queues one explicit bake; a running or already-pending bake is retained
    /// rather than silently replaced with different full-resolution output.
    ///
    /// # Errors
    ///
    /// Returns an error when another bake is active or no output directory is
    /// selected.
    pub fn request(&mut self, recipe: TextureRecipe) -> Result<(), String> {
        if self.running.is_some() || self.pending.is_some() {
            return Err("A final bake is already running".into());
        }
        if self.output_directory.trim().is_empty() {
            return Err("Choose an output directory before baking".into());
        }
        self.pending = Some(BakeRequest {
            recipe,
            output: PathBuf::from(self.output_directory.trim()),
            options: OutputOptions {
                profile: self.profile,
                force: self.overwrite,
            },
        });
        self.error = None;
        self.status = "Bake queued".into();
        Ok(())
    }

    #[must_use]
    pub const fn is_running(&self) -> bool {
        self.running.is_some() || self.pending.is_some()
    }

    /// Takes a newly completed bake so the document can record its hash.
    pub fn take_success(&mut self) -> Option<BakeSuccess> {
        self.last_success.take()
    }
}

pub struct BakePlugin;

impl Plugin for BakePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<BakeState>()
            .add_systems(Update, drive_bake);
    }
}

fn drive_bake(mut state: ResMut<BakeState>) {
    if let Some(task) = &mut state.running
        && let Some(result) = check_ready(&mut task.0)
    {
        state.running = None;
        match result {
            Ok(success) => {
                state.status = format!(
                    "Baked {} maps in {:.0} ms",
                    success.map_count, success.elapsed_ms
                );
                state.error = None;
                state.last_success = Some(success);
            }
            Err(error) => {
                state.status = "Bake failed; existing generated files were preserved".into();
                state.error = Some(error);
            }
        }
    }
    if state.running.is_some() {
        return;
    }
    let Some(request) = state.pending.take() else {
        return;
    };
    state.status = "Generating final texture set…".into();
    state.running = Some(BakeTask(AsyncComputeTaskPool::get().spawn(async move {
        let started = Instant::now();
        let textures = generate_texture_set(&request.recipe, NormalConvention::OpenGl)
            .map_err(|error| error.to_string())?;
        let manifest = write_texture_set(&textures, &request.output, &request.options)
            .map_err(|error| error.to_string())?;
        Ok(BakeSuccess {
            recipe_hash: manifest.metadata.recipe_hash.clone(),
            manifest_path: request
                .output
                .join(format!("{}.texture-set.json", manifest.name)),
            elapsed_ms: started.elapsed().as_secs_f64() * 1000.0,
            map_count: manifest.maps.len(),
        })
    })));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bake_requires_an_explicit_output_directory() {
        let recipe = serde_json::from_str(include_str!(
            "../../island-rs/texture-recipes/cracked-stone.json"
        ))
        .unwrap();
        assert!(BakeState::default().request(recipe).is_err());
    }
}
