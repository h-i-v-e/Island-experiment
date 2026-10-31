//! Worker-owned global weather and one persistent, shared GPU field.
use crate::{Scene, environment::EnvironmentSettings};
use bevy::{
    asset::RenderAssetUsages,
    image::{ImageFilterMode, ImageSampler, ImageSamplerDescriptor},
    prelude::*,
    render::{
        Render, RenderApp, RenderSystems,
        extract_resource::{ExtractResource, ExtractResourcePlugin},
        render_asset::RenderAssets,
        render_resource::{
            Extent3d, TextureDimension, TextureFormat, TextureUsages, TextureViewDescriptor,
            TextureViewDimension,
        },
        renderer::RenderQueue,
        texture::GpuImage,
    },
    shader::load_shader_library,
    tasks::{AsyncComputeTaskPool, Task, block_on, poll_once},
};
use planet_rs::weather::{
    LAYERS, WeatherSettings, WeatherSimulation, WeatherSnapshot, WeatherStats, WeatherSurface,
};
use std::sync::Arc;

#[derive(Resource, Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct WeatherControls {
    pub enabled: bool,
    pub paused: bool,
    pub speed: f64,
    pub warmup_hours: f64,
    pub debug: u32,
    #[serde(skip)]
    pub step: bool,
    #[serde(skip)]
    pub reset: u64,
    pub model: WeatherSettings,
}
impl Default for WeatherControls {
    fn default() -> Self {
        Self {
            enabled: true,
            paused: false,
            speed: 1.0,
            warmup_hours: 0.0,
            debug: 0,
            step: false,
            reset: 0,
            model: WeatherSettings::default(),
        }
    }
}

#[derive(Resource, Default)]
pub struct WeatherFrame {
    pub texture: Handle<Image>,
    pub ready: bool,
    pub pending: bool,
    pub seconds: f64,
    pub previous_seconds: f64,
    pub age: f64,
    pub epoch: u64,
    pub ticks: u64,
    pub stats: Option<WeatherStats>,
    pub error: Option<String>,
    arrows: Vec<(Vec3, Vec3)>,
}

impl WeatherFrame {
    pub fn render_seconds(&self) -> f64 {
        self.previous_seconds
            + (self.seconds - self.previous_seconds) * (self.age / 0.25).clamp(0.0, 1.0)
    }
    #[allow(clippy::cast_possible_truncation)] // Render uniforms use normalized float coordinates.
    pub fn uniform(&self, controls: &WeatherControls) -> Vec4 {
        if !self.ready || !controls.enabled {
            return Vec4::ZERO;
        }
        let blend = (self.age / 0.25).clamp(0.0, 1.0);
        Vec4::new(
            1.0 + blend as f32,
            (self.previous_seconds + (self.seconds - self.previous_seconds) * blend) as f32,
            controls.model.effective_radius as f32,
            f32::from(u8::try_from(controls.debug.min(7)).unwrap()),
        )
    }
}

#[derive(Resource, Clone, Default, ExtractResource)]
struct WeatherUpload {
    texture: Handle<Image>,
    bytes: Arc<[u8]>,
    size: u32,
    revision: u64,
}
#[derive(Resource, Default)]
struct Uploaded(u64);
#[derive(Clone, Copy, PartialEq, Eq)]
struct Identity {
    geometry: usize,
    seed: u64,
    reset: u64,
    solar_revision: u64,
    solar_enabled: bool,
}
struct Batch {
    simulation: WeatherSimulation,
    size: u32,
    seconds: f64,
    revision: u64,
    stats: WeatherStats,
    bytes: Arc<[u8]>,
    arrows: Vec<(Vec3, Vec3)>,
}
#[derive(Resource, Default)]
struct Worker {
    identity: Option<Identity>,
    simulation: Option<WeatherSimulation>,
    task: Option<Task<Result<Batch, String>>>,
    elapsed: f64,
    failed: bool,
    steps: u32,
}

#[derive(SystemSet, Clone, Debug, Hash, PartialEq, Eq)]
pub(crate) struct WeatherUpdate;

pub struct WeatherPlugin;
impl Plugin for WeatherPlugin {
    fn build(&self, app: &mut App) {
        load_shader_library!(app, "common.wgsl");
        load_shader_library!(app, "material.wgsl");
        app.init_resource::<WeatherControls>()
            .init_resource::<WeatherFrame>()
            .init_resource::<WeatherUpload>()
            .init_resource::<Worker>()
            .add_plugins(ExtractResourcePlugin::<WeatherUpload>::default())
            .add_systems(Startup, setup.before(crate::terrain_material::setup))
            .add_systems(
                Update,
                (
                    update
                        .in_set(WeatherUpdate)
                        .before(crate::environment::FrameUpdate),
                    draw_wind.after(crate::environment::FrameUpdate),
                ),
            );
        if let Some(render) = app.get_sub_app_mut(RenderApp) {
            render
                .init_resource::<Uploaded>()
                .add_systems(Render, upload.in_set(RenderSystems::PrepareResources));
        }
    }
}
fn field_image(size: u32) -> Image {
    let mut image = Image::new_fill(
        Extent3d {
            width: size,
            height: size,
            depth_or_array_layers: u32::try_from(LAYERS * 12).unwrap(),
        },
        TextureDimension::D2,
        &[0; 8],
        TextureFormat::Rgba16Float,
        RenderAssetUsages::RENDER_WORLD,
    );
    image.texture_descriptor.usage = TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST;
    image.texture_view_descriptor = Some(TextureViewDescriptor {
        dimension: Some(TextureViewDimension::CubeArray),
        ..default()
    });
    image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        mag_filter: ImageFilterMode::Linear,
        min_filter: ImageFilterMode::Linear,
        ..default()
    });
    image
}
fn setup(
    controls: Res<WeatherControls>,
    mut frame: ResMut<WeatherFrame>,
    mut images: ResMut<Assets<Image>>,
) {
    frame.texture = images.add(field_image(controls.model.face_size));
}

#[allow(
    clippy::too_many_arguments,
    clippy::cast_possible_truncation,
    clippy::too_many_lines
)] // One worker state transition owns reset, completion and the next bounded batch.
fn update(
    scene: Res<Scene>,
    time: Res<Time>,
    environment: Res<EnvironmentSettings>,
    solar: Option<Res<crate::solar::SolarControls>>,
    mut controls: ResMut<WeatherControls>,
    mut frame: ResMut<WeatherFrame>,
    mut world: ResMut<Worker>,
    mut upload: ResMut<WeatherUpload>,
) {
    frame.age += time.delta_secs_f64();
    let identity = Identity {
        geometry: Arc::as_ptr(&scene.terrain) as usize,
        seed: scene.seed,
        reset: controls.reset,
        solar_revision: solar.as_ref().map_or(0, |s| s.revision),
        solar_enabled: solar.as_ref().is_some_and(|s| s.enabled),
    };
    if world.identity != Some(identity) {
        world.identity = Some(identity);
        world.task = None;
        world.simulation = None;
        world.elapsed = 0.0;
        world.steps = 0;
        world.failed = false;
        frame.ready = false;
        frame.seconds = 0.0;
        frame.previous_seconds = 0.0;
        frame.ticks = 0;
        frame.stats = None;
        frame.error = None;
        frame.epoch = frame.epoch.wrapping_add(1);
    }
    if let Some(task) = world.task.as_mut()
        && let Some(result) = block_on(poll_once(task))
    {
        world.task = None;
        match result {
            Ok(batch) => {
                upload.texture = frame.texture.clone();
                upload.size = batch.size;
                upload.bytes = batch.bytes;
                upload.revision = upload.revision.wrapping_add(1);
                frame.previous_seconds = if frame.ready {
                    frame.seconds
                } else {
                    batch.seconds
                };
                frame.age = if frame.ready { 0.0 } else { 0.25 };
                frame.seconds = batch.seconds;
                frame.ticks = batch.revision;
                frame.stats = Some(batch.stats);
                frame.arrows = batch.arrows;
                frame.ready = true;
                world.simulation = Some(batch.simulation);
            }
            Err(error) => {
                error!("Weather initialization failed: {error}");
                frame.error = Some(error);
                world.failed = true;
            }
        }
    }
    if controls.enabled
        && !controls.paused
        && !environment.paused
        && !solar.as_ref().is_some_and(|s| s.enabled && s.paused)
    {
        world.elapsed += time.delta_secs_f64() * controls.speed;
    } else {
        world.elapsed = 0.0;
    }
    if controls.step {
        world.steps = world.steps.saturating_add(1);
        controls.step = false;
    }
    frame.pending = world.task.is_some();
    if world.failed || world.task.is_some() || !controls.enabled {
        return;
    }
    if world.simulation.is_some() && world.elapsed < 0.25 && world.steps == 0 {
        return;
    }
    // Bound catch-up work; overload slows weather rather than blocking rendering.
    let elapsed = if world.steps > 0 {
        world.steps -= 1;
        0.25
    } else {
        world.elapsed.min(0.25)
    };
    world.elapsed = (world.elapsed - elapsed).clamp(0.0, 2.0);
    let previous = world.simulation.take();
    let previous_bytes = upload.bytes.clone();
    let terrain = scene.terrain.clone();
    let mut settings = controls.model;
    let solar_model = solar.as_ref().filter(|s| s.enabled).map(|s| s.model);
    if let Some(solar) = solar_model {
        settings.rotation_period = solar.rotation_period;
    }
    let warmup_hours = controls.warmup_hours;
    let seed = scene.seed;
    let sun = planet_rs::DVec3::from_array(environment.sun_direction().to_array().map(f64::from));
    world.task = Some(AsyncComputeTaskPool::get().spawn(async move {
        run_batch(
            previous,
            &terrain,
            settings,
            seed,
            sun,
            elapsed,
            &previous_bytes,
            warmup_hours,
            solar_model,
        )
    }));
    frame.pending = true;
}

#[allow(clippy::too_many_arguments, clippy::cast_possible_truncation)]
fn run_batch(
    previous: Option<WeatherSimulation>,
    terrain: &planet_rs::PlanetTerrain,
    settings: WeatherSettings,
    seed: u64,
    sun: planet_rs::DVec3,
    elapsed: f64,
    previous_bytes: &[u8],
    warmup_hours: f64,
    solar_model: Option<planet_rs::solar::SolarSettings>,
) -> Result<Batch, String> {
    let initializing = previous.is_none();
    let mut simulation = match previous {
        Some(simulation) => simulation,
        None => WeatherSimulation::new(
            seed,
            settings,
            WeatherSurface::from_terrain(terrain, settings.face_size),
        )?,
    };
    let solar = solar_model
        .map(|s| planet_rs::solar::SolarSystem::new(s, terrain.topology.radius))
        .transpose()?;
    let warmup = if initializing && settings.time_scale > 0.0 {
        warmup_hours * 3600.0 / settings.time_scale
    } else {
        0.0
    };
    if let Some(solar) = &solar {
        simulation.advance_with_solar(warmup + elapsed, solar);
    } else {
        simulation.advance(warmup + elapsed, sun);
    }
    let WeatherSnapshot {
        size,
        seconds,
        revision,
        pixels,
        stats,
    } = simulation.snapshot();
    // Packing and both immutable snapshots stay on the worker. The main
    // thread only swaps an Arc; the render world uploads at simulation rate.
    let mut arrows = Vec::new();
    let stride = (size / 8).max(1) as usize;
    for face in 0..6 {
        for y in (0..size).step_by(stride) {
            for x in (0..size).step_by(stride) {
                let n = planet_rs::ocean_surface::cube_direction(
                    face,
                    (f64::from(x) + 0.5) * 2.0 / f64::from(size) - 1.0,
                    (f64::from(y) + 0.5) * 2.0 / f64::from(size) - 1.0,
                );
                let direction = Vec3::from_array(n.to_array().map(|v| v as f32));
                let i =
                    face * size as usize * size as usize + y as usize * size as usize + x as usize;
                let wind = Vec3::new(pixels[i][0], pixels[i][1], pixels[i][2]);
                arrows.push((
                    direction * 1.12,
                    wind.normalize_or_zero() * (0.035 + wind.length() * 0.0025),
                ));
            }
        }
    }
    let current_len = pixels.len() * 8;
    let mut bytes = Vec::with_capacity(current_len * 2);
    for value in pixels.into_iter().flatten() {
        bytes.extend_from_slice(&half::f16::from_f32(value).to_bits().to_ne_bytes());
    }
    if previous_bytes.len() >= current_len {
        bytes.extend_from_slice(&previous_bytes[..current_len]);
    } else {
        bytes.extend_from_within(..current_len);
    }
    Ok(Batch {
        simulation,
        size,
        seconds,
        revision,
        stats,
        bytes: bytes.into(),
        arrows,
    })
}

fn upload(
    fields: Res<WeatherUpload>,
    images: Res<RenderAssets<GpuImage>>,
    queue: Res<RenderQueue>,
    mut uploaded: ResMut<Uploaded>,
) {
    if fields.revision == uploaded.0 || fields.bytes.is_empty() {
        return;
    }
    let Some(image) = images.get(&fields.texture) else {
        return;
    };
    queue.write_texture(
        image.texture.as_image_copy(),
        &fields.bytes,
        bevy::render::render_resource::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(fields.size * 8),
            rows_per_image: Some(fields.size),
        },
        Extent3d {
            width: fields.size,
            height: fields.size,
            depth_or_array_layers: u32::try_from(LAYERS * 12).unwrap(),
        },
    );
    uploaded.0 = fields.revision;
}

fn draw_wind(
    controls: Res<WeatherControls>,
    frame: Res<WeatherFrame>,
    cameras: Query<&Transform, With<crate::Orbit>>,
    mut gizmos: Gizmos,
) {
    if !controls.enabled || controls.debug != 2 || !frame.ready {
        return;
    }
    let Ok(camera) = cameras.single() else {
        return;
    };
    for &(origin, vector) in &frame.arrows {
        if origin.normalize().dot(camera.translation.normalize()) > 0.5
            && origin.normalize().dot(camera.translation) > 1.05
        {
            gizmos.arrow(origin, origin + vector, Color::srgb(1.0, 0.95, 0.75));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::time::TimeUpdateStrategy;
    use std::time::Duration;

    fn wait_for(app: &mut App, predicate: impl Fn(&WeatherFrame) -> bool) {
        for _ in 0..500 {
            app.update();
            if predicate(app.world().resource::<WeatherFrame>()) {
                return;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        panic!(
            "weather worker did not finish: {:?}",
            app.world().resource::<WeatherFrame>().error
        );
    }

    #[test]
    fn worker_pause_step_and_regeneration_keep_the_gpu_allocation() {
        let controls = WeatherControls {
            paused: true,
            model: WeatherSettings {
                face_size: 4,
                ..WeatherSettings::default()
            },
            ..WeatherControls::default()
        };
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(
                50,
            )))
            .insert_resource(crate::collision::test_scene())
            .insert_resource(controls)
            .init_resource::<EnvironmentSettings>()
            .init_resource::<WeatherFrame>()
            .init_resource::<WeatherUpload>()
            .init_resource::<Worker>()
            .init_resource::<Assets<Image>>()
            .add_systems(Startup, setup)
            .add_systems(Update, update);
        wait_for(&mut app, |f| f.ready);
        let texture = app.world().resource::<WeatherFrame>().texture.clone();
        let epoch = app.world().resource::<WeatherFrame>().epoch;
        assert_eq!(app.world().resource::<WeatherFrame>().ticks, 0);
        for _ in 0..10 {
            app.update();
        }
        assert_eq!(app.world().resource::<WeatherFrame>().ticks, 0);
        app.world_mut().resource_mut::<WeatherControls>().step = true;
        wait_for(&mut app, |f| f.ticks == 1);
        assert_eq!(
            app.world().resource::<WeatherUpload>().bytes.len(),
            4 * 4 * LAYERS * 12 * 8
        );
        for _ in 0..10 {
            app.update();
        }
        assert_eq!(app.world().resource::<WeatherFrame>().ticks, 1);
        app.world_mut().resource_mut::<Scene>().seed += 1;
        wait_for(&mut app, |f| f.ready && f.epoch > epoch);
        assert_eq!(app.world().resource::<WeatherFrame>().ticks, 0);
        assert_eq!(app.world().resource::<WeatherFrame>().texture, texture);
        assert_eq!(app.world().resource::<Assets<Image>>().len(), 1);
    }

    #[test]
    fn solar_pause_and_apply_control_the_worker_without_replacing_its_texture() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(
                50,
            )))
            .insert_resource(crate::collision::test_scene())
            .insert_resource(WeatherControls {
                model: WeatherSettings {
                    face_size: 4,
                    ..WeatherSettings::default()
                },
                ..WeatherControls::default()
            })
            .insert_resource(crate::solar::SolarControls {
                paused: true,
                ..crate::solar::SolarControls::default()
            })
            .init_resource::<EnvironmentSettings>()
            .init_resource::<WeatherFrame>()
            .init_resource::<WeatherUpload>()
            .init_resource::<Worker>()
            .init_resource::<Assets<Image>>()
            .add_systems(Startup, setup)
            .add_systems(Update, update);
        wait_for(&mut app, |f| f.ready);
        let texture = app.world().resource::<WeatherFrame>().texture.clone();
        for _ in 0..20 {
            app.update();
        }
        assert_eq!(app.world().resource::<WeatherFrame>().ticks, 0);
        app.world_mut()
            .resource_mut::<crate::solar::SolarControls>()
            .paused = false;
        wait_for(&mut app, |f| f.ticks >= 1);
        let epoch = app.world().resource::<WeatherFrame>().epoch;
        {
            let mut solar = app
                .world_mut()
                .resource_mut::<crate::solar::SolarControls>();
            solar.paused = true;
            solar.model.rotation_period = 43_200.0;
            solar.revision += 1;
        }
        wait_for(&mut app, |f| f.ready && f.epoch > epoch);
        assert_eq!(app.world().resource::<WeatherFrame>().ticks, 0);
        assert_eq!(app.world().resource::<WeatherFrame>().texture, texture);
    }

    #[test]
    fn shared_fields_have_two_snapshots_of_six_cubes_and_support_in_place_gpu_uploads() {
        let image = field_image(64);
        assert_eq!(image.texture_descriptor.size.depth_or_array_layers, 72);
        assert_eq!(image.texture_descriptor.format, TextureFormat::Rgba16Float);
        assert_eq!(
            image.texture_view_descriptor.unwrap().dimension,
            Some(TextureViewDimension::CubeArray)
        );
        assert!(
            image
                .texture_descriptor
                .usage
                .contains(TextureUsages::COPY_DST)
        );
    }
}
