//! Offscreen rendering follows the inherited island viewer's capture path.
use bevy::{
    camera::RenderTarget,
    prelude::*,
    render::{
        render_resource::TextureFormat,
        view::screenshot::{Screenshot, ScreenshotCaptured, save_to_disk},
    },
};
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicU8, Ordering},
    },
    time::{Duration, Instant},
};

pub struct CapturePlugin {
    pub path: PathBuf,
}

#[derive(Resource)]
pub struct CaptureTarget(Handle<Image>);

impl CaptureTarget {
    pub fn render_target(&self) -> RenderTarget {
        RenderTarget::Image(self.0.clone().into())
    }
}

#[derive(Resource)]
struct CaptureProgress {
    path: PathBuf,
    started: Instant,
    frames: usize,
    settled_frames: usize,
    requested: bool,
    saved: bool,
    completed_frames: usize,
}
#[derive(Resource, Clone)]
struct GpuReady(Arc<AtomicU8>);

impl Plugin for CapturePlugin {
    fn build(&self, app: &mut App) {
        let ready = GpuReady(Arc::new(AtomicU8::new(0)));
        app.insert_resource(ready.clone());
        if let Some(render) = app.get_sub_app_mut(bevy::render::RenderApp) {
            render.insert_resource(ready).add_systems(
                bevy::render::Render,
                check_pipelines.in_set(bevy::render::RenderSystems::Cleanup),
            );
        }
        if std::env::var_os("PLANET_LIVE_WATER_SHADERS").is_some() {
            app.add_systems(Update, refresh_shader_sources);
        }
        let image = Image::new_target_texture(1600, 1000, TextureFormat::Rgba8UnormSrgb, None);
        let handle = app.world_mut().resource_mut::<Assets<Image>>().add(image);
        app.insert_resource(CaptureTarget(handle))
            .insert_resource(CaptureProgress {
                path: self.path.clone(),
                started: Instant::now(),
                frames: 0,
                settled_frames: 0,
                requested: false,
                saved: false,
                completed_frames: 0,
            })
            .add_systems(Update, capture);
    }
}

#[allow(clippy::too_many_arguments)] // Bevy system resources.
fn capture(
    mut commands: Commands,
    target: Res<CaptureTarget>,
    mut progress: ResMut<CaptureProgress>,
    mut exit: MessageWriter<AppExit>,
    chunks: Res<crate::streaming::ChunkWorld>,
    stats: Res<crate::streaming::StreamingStats>,
    settings: Res<crate::streaming::StreamingSettings>,
    details: Res<crate::details::Details>,
    ocean: Res<crate::ocean::OceanWorld>,
    lighting: Res<crate::lighting::LightingSettings>,
    gpu: Res<GpuReady>,
) {
    if gpu.0.load(Ordering::Acquire) == 2 {
        error!("Capture stopped: GPU shader compilation failed");
        exit.write(AppExit::error());
        return;
    }
    progress.frames += 1;
    progress.settled_frames = if chunks.settled()
        && details.settled()
        && ocean.settled()
        && gpu.0.load(Ordering::Acquire) == 1
    {
        progress.settled_frames + 1
    } else {
        0
    };
    if progress.saved {
        progress.completed_frames += 1;
        if progress.completed_frames >= 4 {
            if std::fs::metadata(&progress.path).is_ok_and(|metadata| metadata.len() > 0) {
                info!("Saved {}", progress.path.display());
                exit.write(AppExit::Success);
            } else {
                error!("Screenshot write failed: {}", progress.path.display());
                exit.write(AppExit::error());
            }
        }
    } else if progress.started.elapsed() > Duration::from_mins(5) {
        error!("Timed out waiting for screenshot");
        exit.write(AppExit::error());
    } else if progress.frames >= 100 && progress.settled_frames >= 60 && !progress.requested {
        progress.requested = true;
        let summary = format!(
            "chunk_depth: {}\nhorizon_culling: {}\nforced_lod: {:?}\nvisible_patches: {}\ntotal_patches: {}\nhorizon_hidden: {}\nfrustum_hidden: {}\nvisible_triangles: {}\nlod_0_1_2_3: {:?}\nresident_bytes: {}\npending_loads: {}\ncoarse_bytes: {}\ndetail_budget_bytes: {}\n",
            settings.level,
            settings.horizon,
            settings.forced_lod,
            stats.visible,
            stats.total,
            stats.hidden_horizon,
            stats.hidden_frustum,
            stats.triangles,
            stats.lods,
            stats.resident_bytes,
            stats.loading,
            stats.coarse_bytes,
            settings.budget_mb * 1024 * 1024,
        );
        let summary = format!(
            "{summary}lod_surface_maps: {}\nsurface_view: {}\nmapped_patches: {}\n",
            settings.surface_maps, settings.surface_debug, stats.mapped
        );
        let summary = format!(
            "{summary}river_mesh_patches: {}\nriver_texture_patches: {}\n",
            stats.river_meshes, stats.river_textures
        );
        let summary = format!(
            "{summary}grass_patches: {}\nstones: {}\nboulders: {}\n",
            details.grass_patches, details.stones, details.boulders
        );
        let summary = format!(
            "{summary}sun_shadows: {}\nshadow_distance_m: {}\nshadow_bias_m: {}\ndepth_ao: {}\nao_radius_m: {}\nao_intensity: {}\nao_samples: {}\nao_half_resolution: {}\nao_only: {}\n",
            lighting.shadows,
            lighting.shadow_distance,
            lighting.shadow_bias,
            lighting.ao,
            lighting.ao_radius,
            lighting.ao_intensity,
            lighting.ao_samples,
            lighting.half_resolution,
            lighting.ao_only
        );
        let summary = format!(
            "{summary}ocean_vertices: {}\nocean_triangles: {}\nocean_rings: {}\nocean_map_size: {}\n",
            ocean.vertices, ocean.triangles, ocean.rings, ocean.map_size
        );
        println!("{summary}");
        if let Err(error) = std::fs::write(progress.path.with_extension("png.txt"), summary) {
            warn!("Could not save capture statistics: {error}");
        }
        commands
            .spawn(Screenshot::image(target.0.clone()))
            .observe(save_to_disk(progress.path.clone()))
            .observe(
                |_: On<ScreenshotCaptured>, mut progress: ResMut<CaptureProgress>| {
                    progress.saved = true;
                },
            );
    }
}

fn check_pipelines(cache: Res<bevy::render::render_resource::PipelineCache>, ready: Res<GpuReady>) {
    use bevy::{render::render_resource::CachedPipelineState, shader::ShaderCacheError};
    let failed = cache.pipelines().any(|p| {
        matches!(
            &p.state,
            CachedPipelineState::Err(
                ShaderCacheError::ProcessShaderError(_) | ShaderCacheError::CreateShaderModule(_)
            )
        )
    });
    ready.0.store(
        if failed {
            2
        } else {
            u8::from(cache.waiting_pipelines().next().is_none())
        },
        Ordering::Release,
    );
}

// Explicit opt-in for local GPU iteration. Packaged viewers always use embedded shaders.
fn refresh_shader_sources(
    mut shaders: ResMut<Assets<Shader>>,
    mut progress: ResMut<CaptureProgress>,
) {
    let changes: Vec<_> = shaders
        .iter()
        .filter_map(|(id, shader)| {
            let relative = shader.path.split("planet_bevy/").nth(1)?;
            let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("src")
                .join(relative);
            let text = std::fs::read_to_string(path).ok()?;
            (text != shader.source.as_str())
                .then(|| (id, Shader::from_wgsl(text, shader.path.clone())))
        })
        .collect();
    if !changes.is_empty() {
        progress.settled_frames = 0;
    }
    for (id, mut replacement) in changes {
        if let Some(mut shader) = shaders.get_mut(id) {
            replacement.shader_defs = std::mem::take(&mut shader.shader_defs);
            replacement.file_dependencies = std::mem::take(&mut shader.file_dependencies);
            replacement.additional_imports = std::mem::take(&mut shader.additional_imports);
            *shader = replacement;
        }
    }
}
