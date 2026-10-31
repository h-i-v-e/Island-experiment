//! Sun shadows and final-depth ambient occlusion, in physical metres.
mod ao;
use crate::{Orbit, Scene};
pub use ao::{AmbientOcclusionPlugin, AoSettings};
use bevy::{
    light::{CascadeShadowConfig, DirectionalLightShadowMap},
    prelude::*,
};

#[allow(clippy::struct_excessive_bools)] // Independent renderer feature switches.
#[derive(Resource, Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct LightingSettings {
    pub shadows: bool,
    pub shadow_distance: f32,
    pub shadow_bias: f32,
    pub ao: bool,
    pub ao_intensity: f32,
    pub ao_radius: f32,
    pub ao_samples: u32,
    pub half_resolution: bool,
    pub ao_only: bool,
}
impl Default for LightingSettings {
    fn default() -> Self {
        Self {
            shadows: true,
            shadow_distance: 300.0,
            shadow_bias: 0.02,
            ao: true,
            ao_intensity: 0.3,
            ao_radius: 0.05,
            ao_samples: 12,
            half_resolution: false,
            ao_only: false,
        }
    }
}
impl LightingSettings {
    pub fn validate(&self) -> Result<(), String> {
        if !self.shadow_distance.is_finite()
            || !(20.0..=3000.0).contains(&self.shadow_distance)
            || !self.shadow_bias.is_finite()
            || !(0.0..=0.25).contains(&self.shadow_bias)
            || !self.ao_radius.is_finite()
            || !(0.01..=20.0).contains(&self.ao_radius)
            || !self.ao_intensity.is_finite()
            || !(0.0..=2.5).contains(&self.ao_intensity)
            || ![6, 10, 12].contains(&self.ao_samples)
        {
            return Err("shadow distance must be 20..3000 m, shadow bias 0..0.25 m, AO radius 0.01..20 m, intensity 0..2.5 and samples 6, 10 or 12".into());
        }
        Ok(())
    }
}
#[derive(Component)]
pub struct Sun;
pub struct LightingPlugin;
impl Plugin for LightingPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<LightingSettings>()
            .insert_resource(DirectionalLightShadowMap { size: 2048 })
            .insert_resource(GlobalAmbientLight {
                brightness: 300.0,
                ..default()
            })
            .add_plugins(AmbientOcclusionPlugin);
    }
}
#[allow(clippy::cast_possible_truncation)] // Normalized rendering uses f32.
fn cascades(settings: &LightingSettings, radius: f64, eye: Vec3) -> CascadeShadowConfig {
    let scale = radius as f32;
    let altitude = (eye.length() - 1.08).max(0.0);
    // From orbit, include the planet in the shadow volume. At the surface use
    // Unity's 300 m range and four overlapping cascades, all scaled to scene units.
    let distance = if altitude > settings.shadow_distance / scale {
        eye.length() + 1.1
    } else {
        settings.shadow_distance / scale
    };
    CascadeShadowConfig {
        bounds: [0.067, 0.2, 0.467, 1.0].map(|f| f * distance).to_vec(),
        overlap_proportion: 0.2,
        minimum_distance: 0.01 / scale,
    }
}
#[allow(clippy::cast_possible_truncation)]
pub fn update(
    settings: Res<LightingSettings>,
    scene: Res<Scene>,
    mut cameras: Query<(&Transform, &mut AoSettings), With<Orbit>>,
    mut suns: Query<(&mut DirectionalLight, &mut CascadeShadowConfig), With<Sun>>,
) {
    let Ok((camera, mut ao)) = cameras.single_mut() else {
        return;
    };
    let next = AoSettings::new(&settings, scene.terrain.topology.radius as f32);
    if *ao != next {
        *ao = next;
    }
    for (mut light, mut config) in &mut suns {
        if light.shadow_maps_enabled != settings.shadows {
            light.shadow_maps_enabled = settings.shadows;
        }
        let bias = settings.shadow_bias / scene.terrain.topology.radius as f32;
        if light.shadow_depth_bias.to_bits() != bias.to_bits() {
            light.shadow_depth_bias = bias;
        }
        let next = cascades(&settings, scene.terrain.topology.radius, camera.translation);
        if config.bounds != next.bounds
            || config.minimum_distance.to_bits() != next.minimum_distance.to_bits()
        {
            *config = next;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app() -> App {
        let scene = crate::collision::test_scene();
        let orbit = crate::initial_orbit(crate::command::View::Front, &scene.terrain);
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(scene)
            .insert_resource(crate::InitialOrbit(orbit))
            .init_resource::<LightingSettings>()
            .add_systems(Startup, crate::setup)
            .add_systems(Update, update);
        app.update();
        app
    }
    #[test]
    fn metre_scale_and_runtime_toggles_survive_radius_changes() {
        let mut app = app();
        let world = app.world_mut();
        let camera = world
            .query_filtered::<Entity, With<Orbit>>()
            .single(world)
            .unwrap();
        world.get_mut::<Transform>(camera).unwrap().translation = Vec3::Y * 1.01;
        app.update();
        for (radius, expected) in [(1000.0, 0.3_f32), (2000.0, 0.15)] {
            std::sync::Arc::get_mut(&mut app.world_mut().resource_mut::<Scene>().terrain)
                .unwrap()
                .topology
                .radius = radius;
            app.update();
            let world = app.world_mut();
            let (light, config) = world
                .query_filtered::<(&DirectionalLight, &CascadeShadowConfig), With<Sun>>()
                .single(world)
                .unwrap();
            assert!(light.shadow_maps_enabled);
            assert!((config.bounds.last().unwrap() - expected).abs() < 1.0e-6);
            assert!((f64::from(light.shadow_depth_bias) * radius - 0.02).abs() < 1.0e-8);
            let ao = world.get::<AoSettings>(camera).unwrap();
            assert!((f64::from(ao.scale.x) - radius).abs() < 1.0e-6);
            assert!((ao.parameters.x - 0.05).abs() < 1.0e-6);
        }
        {
            let mut options = app.world_mut().resource_mut::<LightingSettings>();
            options.shadows = false;
            options.ao = false;
            options.half_resolution = true;
        }
        app.update();
        let world = app.world_mut();
        assert!(
            !world
                .query_filtered::<&DirectionalLight, With<Sun>>()
                .single(world)
                .unwrap()
                .shadow_maps_enabled
        );
        let ao = world.get::<AoSettings>(camera).unwrap();
        assert!(ao.parameters.y.abs() < f32::EPSILON && (ao.scale.y - 2.0).abs() < f32::EPSILON);
    }
    #[test]
    fn world_and_overlay_cameras_use_the_same_resolve_mode() {
        let mut app = app();
        let world = app.world_mut();
        assert_eq!(
            world
                .query_filtered::<&Msaa, With<Camera3d>>()
                .single(world)
                .unwrap(),
            &Msaa::Off
        );
        assert_eq!(
            world
                .query_filtered::<&Msaa, With<Camera2d>>()
                .single(world)
                .unwrap(),
            &Msaa::Off
        );
        let camera = world
            .query_filtered::<&Camera3d, With<Orbit>>()
            .single(world)
            .unwrap();
        let usages: bevy::render::render_resource::TextureUsages =
            camera.depth_texture_usages.into();
        assert!(usages.contains(bevy::render::render_resource::TextureUsages::TEXTURE_BINDING));
    }
    #[test]
    fn orbital_cascades_cover_the_planet_and_surface_cascades_keep_contact_detail() {
        let options = LightingSettings::default();
        let orbit = cascades(&options, 1000.0, Vec3::Z * 3.5);
        assert!(*orbit.bounds.last().unwrap() > 4.5);
        let surface = cascades(&options, 1000.0, Vec3::Y * 1.001);
        assert!((surface.bounds[0] - 0.0201).abs() < 1.0e-6);
        assert!(surface.bounds.windows(2).all(|pair| pair[0] < pair[1]));
    }
}
