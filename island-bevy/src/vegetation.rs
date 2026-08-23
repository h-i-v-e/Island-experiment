//! Trees and bushes placed from the generator's decoration points.
//!
//! Both kinds share one mesh and one material handle so Bevy can batch the
//! instances, and every variation is derived from the decoration index so the
//! scene stays as deterministic as the island it decorates.

use std::f32::consts::TAU;

use bevy::{light::NotShadowCaster, prelude::*};

use crate::{convert::island_to_world, island_gen::GeneratedIsland};

const TRUNK_RADIUS: f32 = 0.6;
const TRUNK_HEIGHT: f32 = 5.0;
const CANOPY_RADIUS: f32 = 3.6;
const CANOPY_HEIGHT: f32 = 12.0;
const BUSH_RADIUS: f32 = 2.0;

pub struct VegetationPlugin;

impl Plugin for VegetationPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            spawn_vegetation.run_if(resource_added::<GeneratedIsland>),
        );
    }
}

fn spawn_vegetation(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    island: Res<GeneratedIsland>,
) {
    let decorations = island.0.decorations();
    let tree = meshes.add(tree_mesh());
    let bush = meshes.add(bush_mesh());
    let foliage = materials.add(StandardMaterial {
        base_color: Color::srgb(0.16, 0.33, 0.13),
        perceptual_roughness: 0.92,
        reflectance: 0.03,
        ..default()
    });
    let undergrowth = materials.add(StandardMaterial {
        base_color: Color::srgb(0.21, 0.36, 0.15),
        perceptual_roughness: 0.94,
        reflectance: 0.03,
        ..default()
    });

    // Canopies at this scale never cast more than a pixel of shadow, and the
    // count runs to tens of thousands, so they stay out of the shadow passes.
    let trees: Vec<_> = decorations
        .trees()
        .iter()
        .enumerate()
        .map(|(index, point)| {
            (
                Mesh3d(tree.clone()),
                MeshMaterial3d(foliage.clone()),
                placement(index, *point, 0x54c1_9b0e_a3f7_2d41, 0.75, 0.55),
                NotShadowCaster,
            )
        })
        .collect();
    let bushes: Vec<_> = decorations
        .bushes()
        .iter()
        .enumerate()
        .map(|(index, point)| {
            (
                Mesh3d(bush.clone()),
                MeshMaterial3d(undergrowth.clone()),
                placement(index, *point, 0x9f27_3b6d_1c85_ea07, 0.7, 0.7),
                NotShadowCaster,
            )
        })
        .collect();
    commands.spawn_batch(trees);
    commands.spawn_batch(bushes);
}

/// Decoration points are `(u, v, height)` in normalized island space.
fn placement(index: usize, point: motu::Vec3, salt: u64, minimum: f32, spread: f32) -> Transform {
    let hash = mix(index as u64, salt);
    let scale = spread.mul_add(unit(hash), minimum);
    Transform::from_translation(island_to_world(point.x, point.y, point.z))
        .with_rotation(Quat::from_rotation_y(unit(mix(hash, salt)) * TAU))
        .with_scale(Vec3::splat(scale))
}

/// Trunk and canopy are baked into one mesh so each tree stays a single entity.
fn tree_mesh() -> Mesh {
    let trunk_centre = Vec3::Y * TRUNK_HEIGHT * 0.5;
    let canopy_centre = Vec3::Y * (TRUNK_HEIGHT + CANOPY_HEIGHT * 0.5);
    let mut mesh =
        Mesh::from(Cylinder::new(TRUNK_RADIUS, TRUNK_HEIGHT)).translated_by(trunk_centre);
    let canopy = Mesh::from(Cone::new(CANOPY_RADIUS, CANOPY_HEIGHT)).translated_by(canopy_centre);
    mesh.merge(&canopy)
        .expect("trunk and canopy share the same vertex layout");
    mesh
}

fn bush_mesh() -> Mesh {
    Sphere::new(BUSH_RADIUS)
        .mesh()
        .uv(10, 6)
        .scaled_by(Vec3::new(1.0, 0.6, 1.0))
        .translated_by(Vec3::new(0.0, BUSH_RADIUS * 0.5, 0.0))
}

/// `SplitMix64` finalizer, used instead of a random source so repeated runs of
/// the same seed place identical vegetation.
fn mix(value: u64, salt: u64) -> u64 {
    let mut state = value.wrapping_add(salt).wrapping_add(0x9e37_79b9_7f4a_7c15);
    state = (state ^ (state >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    state = (state ^ (state >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    state ^ (state >> 31)
}

fn unit(hash: u64) -> f32 {
    #[allow(clippy::cast_precision_loss)]
    {
        (hash >> 40) as f32 / 16_777_216.0
    }
}
