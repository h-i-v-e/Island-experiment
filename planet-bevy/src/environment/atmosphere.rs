//! Spherical Bevy scattering around the terrain's true centre. Physical radii
//! live in metres; a single transform maps them into the viewer's unit sphere.
use super::settings::{EnvironmentSettings, effective_layers};
use crate::{Orbit, Scene, lighting::Sun};
use bevy::{
    light::{
        Atmosphere, AtmosphereEnvironmentMapLight, GeneratedEnvironmentMapLight,
        atmosphere::ScatteringMedium,
    },
    pbr::{
        AtmosphereMode, AtmosphereSettings, ExtractedAtmosphere, GpuAtmosphereSettings,
        resources::GpuAtmosphere,
    },
    prelude::*,
    render::{
        Extract, ExtractSchedule, RenderApp, extract_component::DynamicUniformIndex,
        sync_world::RenderEntity,
    },
};

pub struct PlanetAtmospherePlugin;
impl Plugin for PlanetAtmospherePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<AtmosphereWorld>()
            .init_resource::<Assets<ScatteringMedium>>()
            .add_systems(
                Update,
                update
                    .after(crate::explore::update)
                    .after(crate::solar::SolarUpdate),
            );
        if let Some(render) = app.get_sub_app_mut(RenderApp) {
            render.add_systems(
                ExtractSchedule,
                disable_render_atmosphere.after(bevy::pbr::extract_atmosphere),
            );
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
struct Geometry {
    pointer: usize,
    radius: u64,
}
#[derive(Clone, Copy, PartialEq)]
struct CameraConfiguration {
    entity: Entity,
    quality: u32,
    orbital: bool,
    radius: u32,
}
#[derive(Resource, Default)]
pub(super) struct AtmosphereWorld {
    geometry: Option<Geometry>,
    maximum_height: f32,
    entity: Option<Entity>,
    medium: Option<Handle<ScatteringMedium>>,
    medium_key: Option<(u32, u32)>,
    shell_key: Option<(u32, u32)>,
    camera: Option<CameraConfiguration>,
}

fn medium(height: f32, haze: f32) -> ScatteringMedium {
    // Earth coefficients are per metre, with profiles expressed as fractions of
    // a 60 km column. Preserve that vertical optical depth in the small planet.
    ScatteringMedium::earth(128, 128)
        .with_density_multiplier(60_000.0 / height.max(1.0) * haze)
        .with_label("planet atmosphere")
}

#[allow(clippy::cast_possible_truncation)] // Normalized rendering and GPU properties are f32.
fn geometry(scene: &Scene) -> (Geometry, f32) {
    (
        Geometry {
            pointer: scene.terrain.elevations.as_ptr() as usize,
            radius: scene.terrain.topology.radius.to_bits(),
        },
        scene.terrain.topology.radius as f32,
    )
}

fn camera_settings(quality: u32, orbital: bool, radius: f32) -> AtmosphereSettings {
    let (samples, march) = match quality {
        0 => (16, 8),
        2 => (40, 20),
        _ => (24, 12),
    };
    // The built-in atmospheric IBL probe retains its initial LUT views. Keep
    // their extents stable when quality changes, and scale integration work
    // instead so both the visible sky and IBL always use the current textures.
    AtmosphereSettings {
        transmittance_lut_size: UVec2::new(128, 64),
        transmittance_lut_samples: samples,
        multiscattering_lut_size: UVec2::splat(16),
        multiscattering_lut_dirs: if quality == 2 { 32 } else { 16 },
        multiscattering_lut_samples: if quality == 2 { 12 } else { 8 },
        sky_view_lut_size: UVec2::new(224, 112),
        sky_view_lut_samples: march,
        aerial_view_lut_size: UVec3::splat(24),
        aerial_view_lut_samples: if quality == 2 { 8 } else { 4 },
        aerial_view_lut_max_distance: radius * 4.0,
        sky_max_samples: march,
        rendering_method: if orbital {
            AtmosphereMode::Raymarched
        } else {
            AtmosphereMode::LookupTexture
        },
    }
}

#[allow(
    clippy::too_many_arguments,
    clippy::type_complexity,
    clippy::cast_possible_truncation
)]
pub(super) fn update(
    mut commands: Commands,
    scene: Res<Scene>,
    settings: Res<EnvironmentSettings>,
    solar: Option<Res<crate::solar::SolarFrame>>,
    mut world: ResMut<AtmosphereWorld>,
    mut media: ResMut<Assets<ScatteringMedium>>,
    mut cameras: Query<
        (
            Entity,
            &Transform,
            Option<&AtmosphereSettings>,
            Option<&AtmosphereEnvironmentMapLight>,
            Option<&mut GeneratedEnvironmentMapLight>,
        ),
        (With<Orbit>, With<Camera3d>, Without<Sun>),
    >,
    mut suns: Query<&mut Transform, (With<Sun>, Without<Orbit>)>,
) {
    let (geometry, radius) = geometry(&scene);
    if world.geometry != Some(geometry) {
        world.maximum_height = (scene.terrain.elevations.iter().copied().fold(0.0, f64::max)
            * scene.terrain.topology.radius) as f32;
        world.geometry = Some(geometry);
    }
    let (_, height) = effective_layers(&settings, world.maximum_height);
    let medium_key = (height.to_bits(), settings.haze.to_bits());
    if world.medium_key != Some(medium_key) {
        let next = medium(height, settings.haze);
        if let Some(handle) = &world.medium {
            if let Some(mut current) = media.get_mut(handle) {
                *current = next;
            }
        } else {
            world.medium = Some(media.add(next));
        }
        world.medium_key = Some(medium_key);
    }
    configure_shell(
        &mut commands,
        &mut world,
        radius,
        height,
        settings.atmosphere,
    );
    let direction = solar
        .as_ref()
        .filter(|s| s.enabled)
        .and_then(|s| s.state)
        .map_or_else(
            || settings.sun_direction(),
            |s| Vec3::from_array(s.sun_direction.to_array().map(|v| v as f32)),
        );
    let up = if direction.y.abs() > 0.99 {
        Vec3::Z
    } else {
        Vec3::Y
    };
    let rotation = Transform::default().looking_to(-direction, up).rotation;
    for mut sun in &mut suns {
        if sun.rotation != rotation {
            sun.rotation = rotation;
        }
    }
    for (entity, transform, current, environment, generated) in &mut cameras {
        let orbital = transform.translation.length() > 1.0 + height / radius;
        let key = CameraConfiguration {
            entity,
            quality: settings.quality,
            orbital,
            radius: radius.to_bits(),
        };
        if current.is_none() || world.camera != Some(key) {
            commands
                .entity(entity)
                .insert(camera_settings(settings.quality, orbital, radius));
            world.camera = Some(key);
        }
        if environment.is_none() {
            commands
                .entity(entity)
                .insert(AtmosphereEnvironmentMapLight {
                    // Keep this probe small and persistent; the weather renderer has
                    // its own view-dependent cloud data and reflection approximation.
                    intensity: 0.35,
                    size: UVec2::splat(64),
                    ..default()
                });
        }
        if let Some(mut generated) = generated {
            let intensity: f32 = if settings.atmosphere { 0.35 } else { 0.0 };
            if generated.intensity.to_bits() != intensity.to_bits() {
                generated.intensity = intensity;
            }
        }
    }
}

fn configure_shell(
    commands: &mut Commands,
    world: &mut AtmosphereWorld,
    radius: f32,
    height: f32,
    enabled: bool,
) {
    if !enabled {
        if let Some(entity) = world.entity.take() {
            commands.entity(entity).despawn();
        }
        world.shell_key = None;
        return;
    }
    let key = (radius.to_bits(), height.to_bits());
    if world.shell_key == Some(key) {
        return;
    }
    let atmosphere = Atmosphere {
        inner_radius: radius,
        outer_radius: radius + height,
        ground_albedo: Vec3::splat(0.18),
        medium: world
            .medium
            .as_ref()
            .expect("created scattering medium")
            .clone(),
    };
    let transform = GlobalTransform::from_scale(Vec3::splat(radius.recip()));
    if let Some(entity) = world.entity {
        commands.entity(entity).insert((atmosphere, transform));
    } else {
        // The explicit final insertion also handles radius == 1: Bevy's on_add
        // hook otherwise interprets an identity transform as a flat-world sky
        // and relocates the atmosphere centre a radius below the Y origin.
        let entity = commands.spawn((transform, atmosphere)).id();
        commands.entity(entity).insert(transform);
        world.entity = Some(entity);
    }
    world.shell_key = Some(key);
}

#[allow(clippy::type_complexity)] // Main-world camera filters inside extraction.
fn disable_render_atmosphere(
    settings: Extract<Res<EnvironmentSettings>>,
    cameras: Extract<Query<RenderEntity, (With<Orbit>, With<Camera3d>)>>,
    mut commands: Commands,
) {
    if settings.atmosphere {
        return;
    }
    for camera in &cameras {
        // Bevy's extraction removes the logical settings when no atmosphere is
        // selected. Remove derived GPU indices too so a previously configured
        // view cannot continue executing its cached sky pass after disabling it.
        commands.entity(camera).remove::<(
            ExtractedAtmosphere,
            GpuAtmosphereSettings,
            GpuAtmosphere,
            DynamicUniformIndex<GpuAtmosphereSettings>,
            DynamicUniformIndex<GpuAtmosphere>,
        )>();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn app(radius: f64) -> App {
        let mut scene = crate::collision::test_scene();
        Arc::get_mut(&mut scene.terrain).unwrap().topology.radius = radius;
        let orbit = crate::initial_orbit(crate::command::View::Front, &scene.terrain);
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(scene)
            .init_resource::<EnvironmentSettings>()
            .init_resource::<Assets<ScatteringMedium>>()
            .init_resource::<AtmosphereWorld>()
            .add_systems(Update, update);
        app.world_mut().spawn((
            Camera3d::default(),
            orbit,
            Transform::from_xyz(0.0, 0.0, 3.5),
        ));
        app.world_mut().spawn((Sun, Transform::default()));
        app
    }

    #[test]
    fn atmosphere_is_centred_and_maps_physical_radius_to_the_unit_sphere() {
        for radius in [1.0, 1000.0, 2000.0] {
            let mut app = app(radius);
            app.update();
            let world = app.world_mut();
            let (atmosphere, transform) = world
                .query::<(&Atmosphere, &GlobalTransform)>()
                .single(world)
                .unwrap();
            assert_eq!(transform.translation(), Vec3::ZERO);
            for axis in [Vec3::X, Vec3::NEG_Y, Vec3::Z] {
                let surface = transform.transform_point(axis * atmosphere.inner_radius);
                assert!(surface.distance(axis) < 1.0e-6);
            }
        }
    }

    #[test]
    fn camera_motion_reuses_medium_and_switches_between_surface_and_orbit() {
        let mut app = app(1000.0);
        app.update();
        let medium = app
            .world()
            .resource::<AtmosphereWorld>()
            .medium
            .as_ref()
            .unwrap()
            .clone();
        let camera = app
            .world_mut()
            .query_filtered::<Entity, With<Orbit>>()
            .single(app.world())
            .unwrap();
        for up in [Vec3::Y, Vec3::NEG_Y, Vec3::X, Vec3::NEG_Z] {
            app.world_mut()
                .get_mut::<Transform>(camera)
                .unwrap()
                .translation = up * 1.003;
            app.update();
            assert!(matches!(
                app.world()
                    .get::<AtmosphereSettings>(camera)
                    .unwrap()
                    .rendering_method,
                AtmosphereMode::LookupTexture
            ));
            assert_eq!(
                app.world().resource::<AtmosphereWorld>().medium.as_ref(),
                Some(&medium)
            );
            assert_eq!(app.world().resource::<Assets<ScatteringMedium>>().len(), 1);
        }
        app.world_mut()
            .get_mut::<Transform>(camera)
            .unwrap()
            .translation = Vec3::Z * 3.5;
        app.update();
        assert!(matches!(
            app.world()
                .get::<AtmosphereSettings>(camera)
                .unwrap()
                .rendering_method,
            AtmosphereMode::Raymarched
        ));
    }

    #[test]
    fn disabling_removes_shell_and_reenabling_retains_the_medium_handle() {
        let mut app = app(1000.0);
        app.update();
        let handle = app
            .world()
            .resource::<AtmosphereWorld>()
            .medium
            .as_ref()
            .unwrap()
            .clone();
        app.world_mut()
            .resource_mut::<EnvironmentSettings>()
            .atmosphere = false;
        app.update();
        assert_eq!(
            app.world_mut()
                .query::<&Atmosphere>()
                .iter(app.world())
                .count(),
            0
        );
        app.world_mut()
            .resource_mut::<EnvironmentSettings>()
            .atmosphere = true;
        app.update();
        assert_eq!(
            app.world_mut()
                .query::<&Atmosphere>()
                .iter(app.world())
                .count(),
            1
        );
        assert_eq!(
            app.world().resource::<AtmosphereWorld>().medium.as_ref(),
            Some(&handle)
        );
    }

    #[test]
    fn quality_changes_keep_the_ibl_probes_lut_views_valid() {
        let low = camera_settings(0, false, 1000.0);
        let high = camera_settings(2, true, 1000.0);
        assert_eq!(low.transmittance_lut_size, high.transmittance_lut_size);
        assert_eq!(low.multiscattering_lut_size, high.multiscattering_lut_size);
        assert_eq!(low.sky_view_lut_size, high.sky_view_lut_size);
        assert_eq!(low.aerial_view_lut_size, high.aerial_view_lut_size);
        assert!(low.transmittance_lut_samples < high.transmittance_lut_samples);
        assert!(low.sky_max_samples < high.sky_max_samples);
    }

    #[test]
    fn optical_column_is_stable_when_the_atmosphere_height_changes() {
        let thin = medium(100.0, 1.0);
        let thick = medium(400.0, 1.0);
        for (a, b) in thin.terms.iter().zip(&thick.terms) {
            assert!((a.scattering * 100.0 - b.scattering * 400.0).length() < 1.0e-6);
            assert!((a.absorption * 100.0 - b.absorption * 400.0).length() < 1.0e-6);
        }
    }
}
