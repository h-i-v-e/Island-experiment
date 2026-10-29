//! Planet-centred air and weather, with shared persistent GPU state.
mod atmosphere;
mod clouds;
mod noise;
pub(crate) mod reflections;
pub mod settings;

use crate::{Orbit, Scene, lighting::Sun};
use bevy::{
    prelude::*,
    render::{extract_resource::ExtractResourcePlugin, storage::ShaderBuffer},
    shader::load_shader_library,
};
pub(crate) use clouds::CloudsPost;
pub use settings::{EnvironmentAssets, EnvironmentFrame, EnvironmentSettings, EnvironmentUniform};

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct FrameUpdate;

pub struct EnvironmentPlugin;
impl Plugin for EnvironmentPlugin {
    fn build(&self, app: &mut App) {
        load_shader_library!(app, "common.wgsl");
        load_shader_library!(app, "air.wgsl");
        load_shader_library!(app, "material.wgsl");
        app.init_resource::<EnvironmentSettings>()
            .init_resource::<EnvironmentAssets>()
            .init_resource::<EnvironmentFrame>()
            .init_resource::<WeatherClock>()
            .add_plugins((
                ExtractResourcePlugin::<EnvironmentFrame>::default(),
                atmosphere::PlanetAtmospherePlugin,
                clouds::CloudPlugin,
                reflections::SkyReflectionPlugin,
            ))
            .add_systems(Startup, setup.before(crate::terrain_material::setup))
            .add_systems(
                Update,
                update_frame
                    .in_set(FrameUpdate)
                    .after(atmosphere::update)
                    .after(crate::explore::update)
                    .after(crate::forest::update),
            );
    }
}

#[derive(Resource, Default)]
struct WeatherClock {
    geometry: Option<(usize, u64, u64)>,
    maximum_height: f32,
    settings: Option<EnvironmentSettings>,
    seconds: f64,
    revision: u64,
    weather: Option<(u64, bool, bool)>,
    solar: Option<u64>,
}
fn setup(
    mut assets: ResMut<EnvironmentAssets>,
    mut buffers: ResMut<Assets<ShaderBuffer>>,
    mut images: ResMut<Assets<Image>>,
) {
    use bevy::{
        asset::RenderAssetUsages,
        image::{ImageAddressMode, ImageFilterMode, ImageSampler, ImageSamplerDescriptor},
        render::render_resource::{Extent3d, TextureDimension, TextureFormat},
    };
    assets.uniform = buffers.add(ShaderBuffer::from(EnvironmentUniform::default()));
    let mut image = Image::new(
        Extent3d {
            width: noise::SIZE,
            height: noise::SIZE,
            depth_or_array_layers: noise::SIZE,
        },
        TextureDimension::D3,
        noise::generate(),
        TextureFormat::Rgba8Unorm,
        RenderAssetUsages::RENDER_WORLD,
    );
    image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::Repeat,
        address_mode_v: ImageAddressMode::Repeat,
        address_mode_w: ImageAddressMode::Repeat,
        mag_filter: ImageFilterMode::Linear,
        min_filter: ImageFilterMode::Linear,
        ..default()
    });
    assets.cloud_noise = images.add(image);
}

#[allow(
    clippy::too_many_arguments,
    clippy::cast_possible_truncation,
    clippy::too_many_lines
)] // Shared frame assembly keeps the climate, lighting and texture revisions together.
fn update_frame(
    scene: Res<Scene>,
    settings: Res<EnvironmentSettings>,
    weather: Option<Res<crate::weather::WeatherFrame>>,
    weather_controls: Option<Res<crate::weather::WeatherControls>>,
    solar: Option<Res<crate::solar::SolarFrame>>,
    time: Res<Time>,
    assets: Res<EnvironmentAssets>,
    mut clock: ResMut<WeatherClock>,
    mut frame: ResMut<EnvironmentFrame>,
    mut buffers: ResMut<Assets<ShaderBuffer>>,
    cameras: Query<&Transform, With<Orbit>>,
    suns: Query<&DirectionalLight, With<Sun>>,
) {
    let Ok(camera) = cameras.single() else {
        return;
    };
    // Balanced retains half-resolution volumetric detail in orbit as on foot.
    let quality = settings.quality;
    let radius = scene.terrain.topology.radius as f32;
    let geometry = (
        scene.terrain.elevations.as_ptr() as usize,
        scene.terrain.topology.radius.to_bits(),
        scene.seed,
    );
    if clock.geometry != Some(geometry) {
        clock.geometry = Some(geometry);
        clock.maximum_height =
            scene.terrain.elevations.iter().copied().fold(0.0, f64::max) as f32 * radius;
        clock.seconds = 0.0;
        clock.revision = clock.revision.wrapping_add(1);
    }
    if clock.settings != Some(*settings) {
        clock.settings = Some(*settings);
        clock.revision = clock.revision.wrapping_add(1);
    }
    let solar_revision = solar.as_ref().map(|s| s.revision);
    if clock.solar != solar_revision {
        clock.solar = solar_revision;
        clock.revision = clock.revision.wrapping_add(1);
    }
    let sun_direction = solar
        .as_ref()
        .filter(|s| s.enabled)
        .and_then(|s| s.state)
        .map_or_else(
            || settings.sun_direction(),
            |s| Vec3::from_array(s.sun_direction.to_array().map(|v| v as f32)),
        );
    let active = weather_controls.as_ref().is_some_and(|c| c.enabled);
    let weather_identity = weather.as_ref().map(|w| (w.epoch, w.ready, active));
    if clock.weather != weather_identity {
        clock.weather = weather_identity;
        clock.revision = clock.revision.wrapping_add(1);
    }
    if !settings.paused
        && !solar.as_ref().is_some_and(|s| s.paused)
        && !weather_controls
            .as_ref()
            .is_some_and(|c| c.enabled && c.paused)
    {
        clock.seconds += time.delta_secs_f64()
            * weather_controls
                .as_ref()
                .filter(|c| c.enabled)
                .map_or(1.0, |c| c.speed);
    }
    let (base, atmosphere_height) = settings::effective_layers(&settings, clock.maximum_height);
    let minimum_base = if active && weather.as_ref().is_some_and(|w| w.ready) {
        settings.cloud_base
    } else {
        base
    };
    let angle = (clock.seconds * f64::from(settings.wind_speed) / f64::from(radius + base))
        .rem_euclid(std::f64::consts::TAU) as f32;
    let inverse_wind = Quat::from_axis_angle(Vec3::new(0.18, 1.0, 0.12).normalize(), -angle);
    let light = suns.single().ok();
    let colour = light.map_or(LinearRgba::WHITE, |sun| sun.color.to_linear());
    let illuminance = light.map_or(10_000.0, |sun| sun.illuminance);
    let seed_offset = u16::try_from(scene.seed.wrapping_mul(0x9e37_79b9) & 0xffff).unwrap();
    frame.uniform = EnvironmentUniform {
        shape: Vec4::new(radius, base, settings.cloud_thickness, settings.cloud_scale),
        weather: Vec4::new(
            settings.coverage,
            settings.density,
            settings.shadow_strength,
            if settings.clouds { minimum_base } else { 0.0 },
        ),
        wind: Vec4::from_array(inverse_wind.to_array()),
        sun: sun_direction.extend(illuminance),
        light: Vec4::new(
            colour.red,
            colour.green,
            colour.blue,
            if settings.atmosphere { 1.0 } else { 0.0 },
        ),
        air: Vec4::new(
            atmosphere_height,
            settings.haze,
            [16.0, 24.0, 40.0][quality.min(2) as usize],
            clock.seconds as f32,
        ),
        viewer: camera.translation.extend(f32::from(seed_offset) / 8192.0),
        simulation: match (weather.as_deref(), weather_controls.as_deref()) {
            (Some(frame), Some(controls)) => frame.uniform(controls),
            _ => Vec4::ZERO,
        },
    };
    if frame.noise != assets.cloud_noise {
        frame.noise = assets.cloud_noise.clone();
    }
    if let Some(weather) = weather {
        frame.weather = weather.texture.clone();
    }
    frame.quality = quality;
    frame.temporal = settings.temporal;
    frame.enabled = settings.atmosphere || settings.clouds;
    frame.revision = clock.revision;
    if let Some(mut buffer) = buffers.get_mut(&assets.uniform) {
        buffer.set_data(frame.uniform);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::time::TimeUpdateStrategy;
    use std::time::Duration;

    #[test]
    fn weather_moves_through_one_persistent_buffer_and_resets_on_regeneration() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(
                100,
            )))
            .insert_resource(crate::collision::test_scene())
            .init_resource::<EnvironmentSettings>()
            .init_resource::<EnvironmentAssets>()
            .init_resource::<EnvironmentFrame>()
            .init_resource::<WeatherClock>()
            .init_resource::<Assets<Image>>()
            .init_resource::<Assets<ShaderBuffer>>()
            .add_systems(Startup, setup)
            .add_systems(Update, update_frame);
        let orbit = crate::initial_orbit(
            crate::command::View::Front,
            &app.world().resource::<Scene>().terrain,
        );
        let camera = app
            .world_mut()
            .spawn((orbit, Transform::from_xyz(0.0, 1.05, 0.0)))
            .id();
        app.update();
        let handle = app.world().resource::<EnvironmentAssets>().uniform.clone();
        let before_wind = app.world().resource::<EnvironmentFrame>().uniform.wind;
        let before_revision = app.world().resource::<EnvironmentFrame>().revision;
        for _ in 0..4 {
            app.update();
        }
        let frame = app.world().resource::<EnvironmentFrame>();
        assert_ne!(before_wind, frame.uniform.wind);
        assert_eq!(before_revision, frame.revision);
        assert_eq!(app.world().resource::<EnvironmentAssets>().uniform, handle);
        assert_eq!(app.world().resource::<Assets<ShaderBuffer>>().len(), 1);
        assert_eq!(app.world().resource::<Assets<Image>>().len(), 1);
        assert_eq!(
            frame.noise,
            app.world().resource::<EnvironmentAssets>().cloud_noise
        );
        assert!((frame.uniform.wind.length() - 1.0).abs() < 1.0e-6);
        let revision = frame.revision;
        app.world_mut().resource_mut::<Scene>().terrain = crate::collision::test_scene().terrain;
        app.update();
        assert!(app.world().resource::<EnvironmentFrame>().revision > revision);
        for (distance, quality) in [(3.0, 1), (1.8, 1), (1.5, 1)] {
            app.world_mut()
                .entity_mut(camera)
                .get_mut::<Transform>()
                .unwrap()
                .translation = Vec3::Y * distance;
            app.update();
            assert_eq!(app.world().resource::<EnvironmentFrame>().quality, quality);
        }
        app.world_mut()
            .resource_mut::<EnvironmentSettings>()
            .quality = 2;
        app.world_mut()
            .entity_mut(camera)
            .get_mut::<Transform>()
            .unwrap()
            .translation = Vec3::Y * 3.0;
        app.update();
        assert_eq!(app.world().resource::<EnvironmentFrame>().quality, 2);
    }
}
