//! Solid ground: the LOD 0 terrain surface and the settled river rocks.

use bevy::prelude::*;

use crate::{convert, island_gen::GeneratedIsland};

pub struct TerrainPlugin;

impl Plugin for TerrainPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            spawn_terrain.run_if(resource_added::<GeneratedIsland>),
        );
    }
}

fn spawn_terrain(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    island: Res<GeneratedIsland>,
) {
    let island = &island.0;

    // Vertex colour carries the whole palette, so the base colour stays white
    // and the surface reads as dry ground rather than polished stone.
    let ground = materials.add(StandardMaterial {
        base_color: Color::WHITE,
        perceptual_roughness: 0.95,
        reflectance: 0.05,
        ..default()
    });
    if let Some(mesh) = island
        .lod(0)
        .and_then(|lod| convert::terrain_mesh(island, lod))
    {
        commands.spawn((
            Name::new("Terrain"),
            Mesh3d(meshes.add(mesh)),
            MeshMaterial3d(ground),
            Transform::default(),
        ));
    } else {
        warn!("island has no LOD 0 terrain mesh");
    }

    let stone = materials.add(StandardMaterial {
        base_color: Color::srgb(0.40, 0.39, 0.37),
        perceptual_roughness: 0.9,
        reflectance: 0.08,
        ..default()
    });
    if let Some(mesh) = convert::render_mesh(island.river_rock_mesh()) {
        commands.spawn((
            Name::new("River rocks"),
            Mesh3d(meshes.add(mesh)),
            MeshMaterial3d(stone),
            Transform::default(),
        ));
    }
}
