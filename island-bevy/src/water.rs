//! The sea plane and ocean floor the generator expects a consumer to supply,
//! plus the river water surface it does generate.

use bevy::{
    light::{NotShadowCaster, NotShadowReceiver},
    prelude::*,
};
use motu::ISLAND_WORLD_METRES;

use crate::{convert, island_gen::GeneratedIsland};

/// The coastline is constrained exactly onto the sea plane, so the quad sits a
/// few centimetres lower to keep the shared edge out of the depth fight.
const SEA_DEPTH_BIAS: f32 = -0.05;
/// The water planes have to run far enough that the fog closes over them long
/// before their own rim does, and that rim then lands within a few pixels of the
/// horizon rather than cutting a visible edge across the sky.
const OCEAN_EXTENT: f32 = ISLAND_WORLD_METRES * 100.0;
/// Clearance between the deepest generated seabed vertex and the opaque floor,
/// enough that the floor never punches through the seabed.
const OCEAN_FLOOR_CLEARANCE: f32 = 20.0;

pub struct WaterPlugin;

impl Plugin for WaterPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_sea).add_systems(
            Update,
            (spawn_ocean_floor, spawn_rivers).run_if(resource_added::<GeneratedIsland>),
        );
    }
}

fn spawn_sea(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let mesh = meshes.add(Plane3d::default().mesh().size(OCEAN_EXTENT, OCEAN_EXTENT));
    let material = materials.add(StandardMaterial {
        base_color: Color::srgba(0.06, 0.30, 0.50, 0.66),
        perceptual_roughness: 0.10,
        reflectance: 0.55,
        alpha_mode: AlphaMode::Blend,
        double_sided: true,
        cull_mode: None,
        ..default()
    });
    commands.spawn((
        Name::new("Sea"),
        Mesh3d(mesh),
        MeshMaterial3d(material),
        Transform::from_xyz(0.0, SEA_DEPTH_BIAS, 0.0),
    ));
}

/// The sea is translucent, so without an opaque floor the sky shows through it
/// past the coast and the terrain's outer rim silhouettes against blue. The
/// floor carries the terrain palette's deep tone.
fn spawn_ocean_floor(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    island: Res<GeneratedIsland>,
) {
    let Some(lod) = island.0.lod(0) else {
        return;
    };
    let seabed = lod
        .vertices
        .iter()
        .map(|vertex| vertex.z * ISLAND_WORLD_METRES)
        .fold(f32::INFINITY, f32::min);
    if !seabed.is_finite() {
        return;
    }
    let mesh = meshes.add(Plane3d::default().mesh().size(OCEAN_EXTENT, OCEAN_EXTENT));
    let material = materials.add(StandardMaterial {
        base_color: Color::srgb(0.04, 0.17, 0.30),
        perceptual_roughness: 1.0,
        reflectance: 0.02,
        ..default()
    });
    // The sea casts into the shadow maps, and the cascades stop well short of
    // the floor's own extent; taking that shadow would draw the last cascade
    // boundary straight across the open water.
    commands.spawn((
        Name::new("Ocean floor"),
        Mesh3d(mesh),
        MeshMaterial3d(material),
        Transform::from_xyz(0.0, seabed - OCEAN_FLOOR_CLEARANCE, 0.0),
        NotShadowCaster,
        NotShadowReceiver,
    ));
}

fn spawn_rivers(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    island: Res<GeneratedIsland>,
) {
    let Some(mesh) = convert::render_mesh(island.0.river_mesh()) else {
        return;
    };
    // Fresh water reads shallower and greener than the open sea.
    let material = materials.add(StandardMaterial {
        base_color: Color::srgba(0.16, 0.44, 0.46, 0.72),
        perceptual_roughness: 0.06,
        reflectance: 0.55,
        alpha_mode: AlphaMode::Blend,
        double_sided: true,
        cull_mode: None,
        ..default()
    });
    commands.spawn((
        Name::new("Rivers"),
        Mesh3d(meshes.add(mesh)),
        MeshMaterial3d(material),
        Transform::default(),
    ));
}
