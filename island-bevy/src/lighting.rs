//! A single warm sun plus sky ambient.

use bevy::{
    light::{CascadeShadowConfigBuilder, GlobalAmbientLight, light_consts::lux},
    prelude::*,
};
use motu::ISLAND_WORLD_METRES;

pub struct LightingPlugin;

impl Plugin for LightingPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(GlobalAmbientLight {
            color: Color::srgb(0.62, 0.74, 0.92),
            brightness: 150.0,
            ..default()
        })
        .add_systems(Startup, spawn_sun);
    }
}

fn spawn_sun(mut commands: Commands) {
    commands.spawn((
        Name::new("Sun"),
        DirectionalLight {
            color: Color::srgb(1.0, 0.95, 0.86),
            illuminance: lux::AMBIENT_DAYLIGHT,
            shadow_maps_enabled: true,
            ..default()
        },
        Transform::default().looking_to(Vec3::new(-0.48, -0.62, -0.62), Vec3::Y),
        // The cascades have to reach across the whole two kilometre island
        // while still resolving relief near the camera.
        CascadeShadowConfigBuilder {
            num_cascades: 4,
            minimum_distance: 1.0,
            maximum_distance: ISLAND_WORLD_METRES * 2.5,
            first_cascade_far_bound: 250.0,
            overlap_proportion: 0.2,
        }
        .build(),
    ));
}
