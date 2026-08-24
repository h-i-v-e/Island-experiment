//! Trees and bushes placed from the generator's decoration points.
//!
//! Bark, canopy and shrub tones all ride in the mesh's vertex colours, so one
//! white material renders every part of every plant and Bevy can batch the
//! instances. A shared material handle cannot carry a per-instance tint, so the
//! per-plant variation is baked into a small set of meshes instead: each class
//! batches once per variant rather than once in total. Everything else is
//! derived from the decoration index, so the scene stays as deterministic as
//! the island it decorates.

use std::f32::consts::TAU;

use bevy::{light::NotShadowCaster, mesh::VertexAttributeValues, prelude::*};

use crate::{
    convert::island_to_world,
    hash::{choice, mix, unit},
    island_gen::{GeneratedIsland, IslandEntity, IslandReady},
};

const TRUNK_RADIUS: f32 = 0.6;
const TRUNK_HEIGHT: f32 = 5.0;
const CANOPY_RADIUS: f32 = 3.6;
const CANOPY_HEIGHT: f32 = 12.0;
const BUSH_RADIUS: f32 = 2.0;

const TREE_SALT: u64 = 0x54c1_9b0e_a3f7_2d41;
const BUSH_SALT: u64 = 0x9f27_3b6d_1c85_ea07;
const PAINT_SALT: u64 = 0x2a7f_e315_c840_9db6;

/// Canopy tones as sRGB, one per tree variant. Restrained against the ground
/// they stand on: a saturated green at this density reads as paint.
const CANOPY_TONES: [[f32; 3]; 4] = [
    [0.22, 0.29, 0.17],
    [0.19, 0.25, 0.15],
    [0.26, 0.32, 0.19],
    [0.16, 0.22, 0.13],
];
/// Undergrowth sits a little warmer and lighter than the canopy over it.
const SHRUB_TONES: [[f32; 3]; 4] = [
    [0.29, 0.34, 0.19],
    [0.25, 0.30, 0.17],
    [0.33, 0.38, 0.22],
    [0.22, 0.27, 0.15],
];
const BARK_TONE: [f32; 3] = [0.26, 0.20, 0.14];
/// How much a bush variant is flattened, which is the only shape variation
/// symbolic geometry can offer.
const BUSH_FLATTENING: [f32; 4] = [0.55, 0.68, 0.50, 0.74];
/// How far a surface pointing straight down is darkened against one pointing
/// up. This is what gives a smooth canopy or shrub any read of volume.
const CANOPY_VOLUME: f32 = 0.58;
const SHRUB_VOLUME: f32 = 0.62;
/// Per-vertex break-up, so a lathe-turned surface does not read as one.
const LEAF_JITTER: f32 = 0.10;

pub struct VegetationPlugin;

impl Plugin for VegetationPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, spawn_vegetation.run_if(on_message::<IslandReady>));
    }
}

fn spawn_vegetation(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    island: Res<GeneratedIsland>,
) {
    let island = &island.0;
    // White, because every tone is in the vertex colours the material
    // multiplies through.
    let plant = materials.add(StandardMaterial {
        base_color: Color::WHITE,
        perceptual_roughness: 0.92,
        reflectance: 0.03,
        ..default()
    });
    let tree_meshes: Vec<Handle<Mesh>> = (0..CANOPY_TONES.len())
        .map(|variant| meshes.add(tree_mesh(variant)))
        .collect();
    let bush_meshes: Vec<Handle<Mesh>> = (0..SHRUB_TONES.len())
        .map(|variant| meshes.add(bush_mesh(variant)))
        .collect();

    // Canopies at this scale never cast more than a pixel of shadow, and the
    // count runs to tens of thousands, so they stay out of the shadow passes.
    let trees: Vec<_> = island
        .trees
        .iter()
        .enumerate()
        .map(|(index, point)| {
            let hash = mix(index as u64, TREE_SALT);
            (
                IslandEntity,
                Mesh3d(tree_meshes[choice(hash, tree_meshes.len())].clone()),
                MeshMaterial3d(plant.clone()),
                placement(hash, *point, TREE_SALT, 0.75, 0.55),
                NotShadowCaster,
            )
        })
        .collect();
    let bushes: Vec<_> = island
        .bushes
        .iter()
        .enumerate()
        .map(|(index, point)| {
            let hash = mix(index as u64, BUSH_SALT);
            (
                IslandEntity,
                Mesh3d(bush_meshes[choice(hash, bush_meshes.len())].clone()),
                MeshMaterial3d(plant.clone()),
                placement(hash, *point, BUSH_SALT, 0.7, 0.7),
                NotShadowCaster,
            )
        })
        .collect();
    commands.spawn_batch(trees);
    commands.spawn_batch(bushes);
}

/// Decoration points are `(u, v, height)` in normalized island space.
fn placement(hash: u64, point: motu::Vec3, salt: u64, minimum: f32, spread: f32) -> Transform {
    let scale = spread.mul_add(unit(hash), minimum);
    Transform::from_translation(island_to_world(point.x, point.y, point.z))
        .with_rotation(Quat::from_rotation_y(unit(mix(hash, salt)) * TAU))
        .with_scale(Vec3::splat(scale))
}

/// Trunk and canopy are baked into one mesh so each tree stays a single entity.
/// They are painted before the merge, which is what leaves the trunk bark
/// coloured under a material that knows nothing about either.
fn tree_mesh(variant: usize) -> Mesh {
    let trunk_centre = Vec3::Y * TRUNK_HEIGHT * 0.5;
    let canopy_centre = Vec3::Y * (TRUNK_HEIGHT + CANOPY_HEIGHT * 0.5);
    let mut mesh =
        Mesh::from(Cylinder::new(TRUNK_RADIUS, TRUNK_HEIGHT)).translated_by(trunk_centre);
    paint(&mut mesh, BARK_TONE, 0.25);
    let mut canopy =
        Mesh::from(Cone::new(CANOPY_RADIUS, CANOPY_HEIGHT)).translated_by(canopy_centre);
    paint(&mut canopy, CANOPY_TONES[variant], CANOPY_VOLUME);
    mesh.merge(&canopy)
        .expect("trunk and canopy share the same vertex layout");
    mesh
}

fn bush_mesh(variant: usize) -> Mesh {
    let mut mesh = Sphere::new(BUSH_RADIUS)
        .mesh()
        .uv(10, 6)
        .scaled_by(Vec3::new(1.0, BUSH_FLATTENING[variant], 1.0))
        .translated_by(Vec3::new(0.0, BUSH_RADIUS * 0.5, 0.0));
    paint(&mut mesh, SHRUB_TONES[variant], SHRUB_VOLUME);
    mesh
}

/// Writes one linear vertex colour per vertex: the tone, darkened towards the
/// underside by `volume` and broken up by a hash of the vertex position.
fn paint(mesh: &mut Mesh, tone: [f32; 3], volume: f32) {
    let linear = LinearRgba::from(Srgba::rgb(tone[0], tone[1], tone[2]));
    let Some(vertices) = mesh
        .attribute(Mesh::ATTRIBUTE_NORMAL)
        .and_then(VertexAttributeValues::as_float3)
        .map(<[[f32; 3]]>::to_vec)
    else {
        return;
    };
    let colours: Vec<[f32; 4]> = vertices
        .iter()
        .enumerate()
        .map(|(index, normal)| {
            let upward = 0.5f32.mul_add(normal[1], 0.5);
            let jitter = LEAF_JITTER.mul_add(unit(mix(index as u64, PAINT_SALT)) - 0.5, 1.0);
            let shade = volume.mul_add(upward - 0.5, 1.0) * jitter;
            [
                linear.red * shade,
                linear.green * shade,
                linear.blue * shade,
                1.0,
            ]
        })
        .collect();
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colours);
}
