//! The sea plane the generator expects a consumer to supply, plus the river
//! water surface it does generate. Both are blended surfaces reading the opaque
//! depth prepass for the ground under them; the shading itself is in `surface`.

use bevy::prelude::*;
use motu::ISLAND_WORLD_METRES;

use crate::{
    island_gen::{IslandEntity, IslandReady, PreparedMeshes},
    surface::{OceanExtension, OceanMaterial, RiverExtension, RiverMaterial},
};

/// The coastline is constrained exactly onto the sea plane, so the quad sits a
/// few centimetres lower to keep the shared edge out of the depth fight.
const SEA_DEPTH_BIAS: f32 = -0.05;
/// The sea has to reach past the far clip so its own rim never enters a frame.
const OCEAN_EXTENT: f32 = ISLAND_WORLD_METRES * 100.0;

pub struct WaterPlugin;

impl Plugin for WaterPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_sea)
            .add_systems(Update, spawn_rivers.run_if(on_message::<IslandReady>));
    }
}

/// Nothing opaque is laid under the open sea. Where the generated terrain ends,
/// the ray under the water finds nothing at all, and the shader answers that
/// exactly as it answers a bottom too deep to reach: both saturate to the same
/// absorbed colour, so the seabed square has no edge to show. What is left
/// past the island is the atmosphere the water thins out into, which carries
/// the sea to the horizon without a rim.
fn spawn_sea(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<OceanMaterial>>,
) {
    let mesh = meshes.add(Plane3d::default().mesh().size(OCEAN_EXTENT, OCEAN_EXTENT));
    // The extension writes base colour, opacity, roughness and reflectance
    // outright, so what the base material still decides is only how the surface
    // is drawn: blended, from either side, after the sky.
    let material = materials.add(OceanMaterial {
        base: StandardMaterial {
            alpha_mode: AlphaMode::Blend,
            double_sided: true,
            cull_mode: None,
            ..default()
        },
        extension: OceanExtension::default(),
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
    mut materials: ResMut<Assets<RiverMaterial>>,
    mut prepared: ResMut<PreparedMeshes>,
) {
    let Some(mesh) = prepared.river.take() else {
        return;
    };
    let material = materials.add(RiverMaterial {
        base: StandardMaterial {
            alpha_mode: AlphaMode::Blend,
            double_sided: true,
            cull_mode: None,
            ..default()
        },
        extension: RiverExtension::new(ISLAND_WORLD_METRES),
    });
    commands.spawn((
        Name::new("Rivers"),
        IslandEntity,
        Mesh3d(meshes.add(mesh)),
        MeshMaterial3d(material),
        Transform::default(),
    ));
}
