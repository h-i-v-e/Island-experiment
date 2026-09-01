//! A single sun under a physical atmosphere.
//!
//! The atmosphere is what draws the sky, tints the sun on its way down and
//! fills the ground with sky light, so there is no clear colour, no distance
//! fog and no uniform ambient term to keep in step with each other. The camera
//! side of the pairing is `AtmosphereSettings` in `camera`.
//!
//! Where the sun stands, how much of it arrives and what the air between is
//! made of all come from the named look in `weather`; this module only builds
//! the two entities they are written onto.

use bevy::{
    light::{
        Atmosphere, CascadeShadowConfigBuilder, GlobalAmbientLight, atmosphere::ScatteringMedium,
    },
    prelude::*,
};
use motu::ISLAND_WORLD_METRES;

use crate::weather::Weather;

/// Sampling resolution of the scattering medium's falloff and phase curves.
pub const MEDIUM_RESOLUTION: u32 = 256;
/// The planet under this island is open ocean, not the average land the earth
/// preset assumes. The albedo sets what a downward ray comes back with, which
/// is what the sea is seen against past the generated seabed, and it feeds the
/// multiple-scattering term the whole sky is built on.
const OCEAN_ALBEDO: f32 = 0.06;

/// Tags the one directional light. `weather` aims it and sets what it carries,
/// `clouds` hangs the cloud shadow off it, and both need to find it.
#[derive(Component)]
pub struct Sun;

pub struct LightingPlugin;

impl Plugin for LightingPlugin {
    fn build(&self, app: &mut App) {
        // The atmosphere's environment map is the only ambient source now; a
        // second uniform one would flatten exactly what it fills.
        app.insert_resource(GlobalAmbientLight::NONE)
            .add_systems(Startup, (spawn_sky, spawn_sun));
    }
}

fn spawn_sky(
    mut commands: Commands,
    weather: Res<Weather>,
    mut mediums: ResMut<Assets<ScatteringMedium>>,
) {
    let medium = mediums.add(weather.look().medium());
    // Left without a transform on purpose: the planet then centres itself one
    // earth radius below the origin, which puts sea level at y == 0.
    commands.spawn((
        Name::new("Atmosphere"),
        Atmosphere {
            ground_albedo: Vec3::splat(OCEAN_ALBEDO),
            ..Atmosphere::earth(medium)
        },
    ));
}

fn spawn_sun(mut commands: Commands, weather: Res<Weather>) {
    let look = weather.look();
    commands.spawn((
        Name::new("Sun"),
        Sun,
        DirectionalLight {
            // Raw sunlight, white: the atmosphere applies both the warm tint
            // and the loss along the path, so pre-warming would count it twice.
            color: Color::WHITE,
            illuminance: look.illuminance(),
            shadow_maps_enabled: true,
            // Cascades at island scale cannot resolve where a rock or a trunk
            // meets the ground; the screen-space pass can.
            contact_shadows_enabled: true,
            ..default()
        },
        // A directional light takes only its rotation from this transform. The
        // other two components are the cloud layer's: `clouds` scales it to the
        // cloud field's own tile, and `weather` drifts the translation downwind.
        Transform::default().looking_to(look.sun_direction, Vec3::Y),
        // The cascades reach across the whole two kilometre island while still
        // resolving relief near the camera.
        CascadeShadowConfigBuilder {
            num_cascades: 4,
            minimum_distance: 1.0,
            maximum_distance: ISLAND_WORLD_METRES,
            first_cascade_far_bound: 250.0,
            overlap_proportion: 0.2,
        }
        .build(),
    ));
}
