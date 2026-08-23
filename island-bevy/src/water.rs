//! The sea plane the generator expects a consumer to supply, plus the river
//! water surface it does generate.

use bevy::prelude::*;
use motu::ISLAND_WORLD_METRES;

use crate::{convert, island_gen::GeneratedIsland};

/// The coastline is constrained exactly onto the sea plane, so the quad sits a
/// few centimetres lower to keep the shared edge out of the depth fight.
const SEA_DEPTH_BIAS: f32 = -0.05;

pub struct WaterPlugin;

impl Plugin for WaterPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_sea).add_systems(
            Update,
            spawn_rivers.run_if(resource_added::<GeneratedIsland>),
        );
    }
}

fn spawn_sea(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let mesh = meshes.add(
        Plane3d::default()
            .mesh()
            .size(ISLAND_WORLD_METRES, ISLAND_WORLD_METRES),
    );
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
