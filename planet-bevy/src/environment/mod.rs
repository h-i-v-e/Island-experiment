//! Planet-centred air and weather, with shared persistent GPU state.
mod atmosphere;
mod clouds;
pub(crate) mod reflections;
pub mod settings;

use crate::{Orbit, Scene, lighting::Sun, terrain_material::TerrainTextures};
use bevy::{
    prelude::*,
    render::{extract_resource::ExtractResourcePlugin, storage::ShaderBuffer},
    shader::load_shader_library,
};
pub(crate) use clouds::CloudsPost;
pub use settings::{EnvironmentAssets, EnvironmentFrame, EnvironmentSettings, EnvironmentUniform};

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
}
fn setup(mut assets: ResMut<EnvironmentAssets>, mut buffers: ResMut<Assets<ShaderBuffer>>) {
    assets.uniform = buffers.add(ShaderBuffer::from(EnvironmentUniform::default()));
}

#[allow(clippy::too_many_arguments, clippy::cast_possible_truncation)]
fn update_frame(
    scene: Res<Scene>,
    settings: Res<EnvironmentSettings>,
    time: Res<Time>,
    textures: Res<TerrainTextures>,
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
    if !settings.paused {
        clock.seconds += time.delta_secs_f64();
    }
    let (base, atmosphere_height) = settings::effective_layers(&settings, clock.maximum_height);
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
            if settings.clouds { 1.0 } else { 0.0 },
        ),
        wind: Vec4::from_array(inverse_wind.to_array()),
        sun: settings.sun_direction().extend(illuminance),
        light: Vec4::new(
            colour.red,
            colour.green,
            colour.blue,
            if settings.atmosphere { 1.0 } else { 0.0 },
        ),
        air: Vec4::new(
            atmosphere_height,
            settings.haze,
            [16.0, 24.0, 40.0][settings.quality.min(2) as usize],
            clock.seconds as f32,
        ),
        viewer: camera.translation.extend(f32::from(seed_offset) / 8192.0),
    };
    if frame.noise != textures.noise {
        frame.noise = textures.noise.clone();
    }
    frame.quality = settings.quality;
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
            .init_resource::<TerrainTextures>()
            .init_resource::<Assets<ShaderBuffer>>()
            .add_systems(Startup, setup)
            .add_systems(Update, update_frame);
        let orbit = crate::initial_orbit(
            crate::command::View::Front,
            &app.world().resource::<Scene>().terrain,
        );
        app.world_mut()
            .spawn((orbit, Transform::from_xyz(0.0, 1.05, 0.0)));
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
        assert!((frame.uniform.wind.length() - 1.0).abs() < 1.0e-6);
        let revision = frame.revision;
        app.world_mut().resource_mut::<Scene>().terrain = crate::collision::test_scene().terrain;
        app.update();
        assert!(app.world().resource::<EnvironmentFrame>().revision > revision);
    }
}
