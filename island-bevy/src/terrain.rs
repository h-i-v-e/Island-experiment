//! Solid ground: the LOD 0 terrain surface and the settled river rocks.

use bevy::prelude::*;
use motu::ISLAND_WORLD_METRES;

use crate::{
    convert,
    island_gen::{GeneratedIsland, IslandEntity, IslandReady},
    surface::{RockExtension, RockMaterial, TerrainExtension, TerrainMaterial},
};

pub struct TerrainPlugin;

impl Plugin for TerrainPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, spawn_terrain.run_if(on_message::<IslandReady>));
    }
}

fn spawn_terrain(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut terrains: ResMut<Assets<TerrainMaterial>>,
    mut rocks: ResMut<Assets<RockMaterial>>,
    island: Res<GeneratedIsland>,
) {
    let island = &island.0;

    // The extension writes base colour, roughness and reflectance outright, so
    // what the base material still decides is only how the surface is drawn:
    // opaque, single-sided, shadow casting and receiving.
    let ground = terrains.add(TerrainMaterial {
        base: StandardMaterial::default(),
        extension: TerrainExtension::new(island.options.max_height, ISLAND_WORLD_METRES),
    });
    if let Some(mesh) =
        convert::terrain_mesh(&island.terrain, &island.materials, &island.river_wetness)
    {
        commands.spawn((
            Name::new("Terrain"),
            IslandEntity,
            Mesh3d(meshes.add(mesh)),
            MeshMaterial3d(ground),
            Transform::default(),
        ));
    } else {
        warn!("island has no LOD 0 terrain mesh");
    }

    let stone = rocks.add(RockMaterial {
        base: StandardMaterial::default(),
        extension: RockExtension::default(),
    });
    if let Some(mesh) = convert::rock_mesh(&island.river_rock_mesh) {
        commands.spawn((
            Name::new("River rocks"),
            IslandEntity,
            Mesh3d(meshes.add(mesh)),
            MeshMaterial3d(stone),
            Transform::default(),
        ));
    }
}
