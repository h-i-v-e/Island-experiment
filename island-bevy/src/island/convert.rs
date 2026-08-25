//! Translation from motu's Z-up island space into Bevy's Y-up render space.
//!
//! island-rs stores normalized XY in `[0, 1]` with Z as elevation and sea level
//! at zero, and it pins its own `glam` release. Every value therefore crosses
//! component by component, and each triangle is reversed because swapping Y and
//! Z flips handedness.

use bevy::{
    asset::RenderAssetUsages,
    mesh::{Indices, PrimitiveTopology},
    prelude::*,
};
use motu::ISLAND_WORLD_METRES;

use crate::{
    hash::{lattice_key_3, mix, unit},
    island_gen::DropIndex,
};

/// The material triple stands for a vertex the generator gave no material to:
/// middling bedrock under full cover, away from the sea. `terrain.wgsl` falls
/// back to the same values when the attribute is missing entirely.
const DEFAULT_MATERIAL: motu::Vec3 = motu::Vec3::new(0.5, 1.0, 0.0);

/// Metres of world space one river-rock tint is held constant over. The
/// generator settles stones of six to twenty-two centimetres, so a cell this
/// size usually covers one body and the hashed tint reads as per-rock rather
/// than per-vertex.
const ROCK_TINT_CELL: f32 = 0.2;
const ROCK_TINT_SALT: u64 = 0x6d1b_7c04_9a3e_5f82;
/// The cool and warm ends the rock tint swings between. Their mean is one, so
/// the swing changes the mineral without moving the material's average albedo.
const ROCK_TINT_COOL: Vec3 = Vec3::new(0.94, 0.97, 1.03);
const ROCK_TINT_WARM: Vec3 = Vec3::new(1.08, 1.00, 0.91);

/// Maps a normalized island-space point onto the Bevy world.
#[must_use]
pub fn island_to_world(x: f32, y: f32, z: f32) -> Vec3 {
    Vec3::new(
        (x - 0.5) * ISLAND_WORLD_METRES,
        z * ISLAND_WORLD_METRES,
        (y - 0.5) * ISLAND_WORLD_METRES,
    )
}

/// Converts positions, normals, UVs and winding without any material work.
/// Returns `None` for meshes the generator left empty.
#[must_use]
pub fn render_mesh(source: &motu::Mesh) -> Option<Mesh> {
    render_mesh_at(source, Vec3::ZERO)
}

/// The same, with every position taken relative to a world-space origin the
/// caller will put on the entity's transform.
///
/// Whole-island meshes pass [`Vec3::ZERO`] and stay in world space. A terrain
/// chunk cannot: Bevy reads the level-of-detail crossfade distance off the
/// entity's translation, so a chunk has to stand where it is rather than at the
/// island's centre.
#[must_use]
pub fn render_mesh_at(source: &motu::Mesh, origin: Vec3) -> Option<Mesh> {
    if source.triangles.is_empty() || source.vertices.is_empty() {
        return None;
    }
    let positions: Vec<[f32; 3]> = source
        .vertices
        .iter()
        .map(|vertex| (island_to_world(vertex.x, vertex.y, vertex.z) - origin).to_array())
        .collect();
    let normals: Vec<[f32; 3]> = source
        .normals
        .iter()
        .map(|normal| {
            Vec3::new(normal.x, normal.z, normal.y)
                .try_normalize()
                .unwrap_or(Vec3::Y)
                .to_array()
        })
        .collect();
    let uv: Vec<[f32; 2]> = source.uv.iter().map(|uv| [uv.x, uv.y]).collect();

    let baked_normals = normals.len() == source.vertices.len();
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        // Generated geometry is immutable after insertion. Render-only usage
        // lets Bevy move its large buffers into extraction instead of cloning
        // and retaining a second main-world copy; bounds are calculated before
        // extraction removes the source asset.
        RenderAssetUsages::RENDER_WORLD,
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    if baked_normals {
        mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    }
    if uv.len() == source.vertices.len() {
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uv);
    }
    mesh.insert_indices(Indices::U32(reversed_triangles(&source.triangles)));
    if !baked_normals {
        mesh.compute_normals();
    }
    Some(mesh)
}

/// Converts a terrain mesh and stores the generator's raw material weights in
/// [`Mesh::ATTRIBUTE_COLOR`]: x is bedrock hardness (exactly one means forced
/// rock), y is loose cover and z is sea proximity. Alpha carries the renderer's
/// own river-bank proximity, which is the one channel the generator's triple
/// leaves free. Nothing here decides what the ground looks like;
/// `terrain.wgsl` is the only authority on that.
#[must_use]
pub fn terrain_mesh(
    source: &motu::Mesh,
    materials: &[motu::Vec3],
    river_wetness: &[f32],
    origin: Vec3,
) -> Option<Mesh> {
    let mut mesh = render_mesh_at(source, origin)?;
    let weights: Vec<[f32; 4]> = (0..source.vertices.len())
        .map(|index| {
            let material = materials.get(index).copied().unwrap_or(DEFAULT_MATERIAL);
            // A vertex the wetness pass never reached is dry ground.
            let wetness = river_wetness.get(index).copied().unwrap_or(0.0);
            [material.x, material.y, material.z, wetness]
        })
        .collect();
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, weights);
    Some(mesh)
}

/// Converts the river water surface and stores what each of its vertices knows
/// about the nearest fall in [`Mesh::ATTRIBUTE_COLOR`], which the generator
/// leaves free on this mesh: the approach to a lip, the falling face and how
/// far down it the vertex stands, the plunge below a foot, and the fall's own
/// height. `river.wgsl` is the only authority on what those become.
#[must_use]
pub fn river_mesh(source: &motu::Mesh, drops: &DropIndex) -> Option<Mesh> {
    let mut mesh = render_mesh(source)?;
    let field: Vec<[f32; 4]> = source
        .vertices
        .iter()
        .map(|&vertex| drops.field(vertex).to_array())
        .collect();
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, field);
    Some(mesh)
}

/// Converts the merged river-rock mesh and hashes a deterministic albedo tint
/// into [`Mesh::ATTRIBUTE_COLOR`]. The bodies arrive already merged, so a
/// vertex attribute is the finest per-body signal the renderer can carry.
/// Alpha carries how much spray from the nearest fall stands on the stone,
/// which is the one channel a tint leaves free.
#[must_use]
pub fn rock_mesh(source: &motu::Mesh, drops: &DropIndex) -> Option<Mesh> {
    let mut mesh = render_mesh(source)?;
    let tints: Vec<[f32; 4]> = source
        .vertices
        .iter()
        .map(|&vertex| {
            let cell = island_to_world(vertex.x, vertex.y, vertex.z) / ROCK_TINT_CELL;
            let hash = mix(cell_key(cell), ROCK_TINT_SALT);
            let shade = 0.40f32.mul_add(unit(hash), 0.70);
            let tint = ROCK_TINT_COOL.lerp(ROCK_TINT_WARM, unit(mix(hash, ROCK_TINT_SALT))) * shade;
            [tint.x, tint.y, tint.z, drops.spray(vertex)]
        })
        .collect();
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, tints);
    Some(mesh)
}

/// One integer lattice cell as a hash input. The multipliers are odd so no two
/// axes can cancel.
fn cell_key(point: Vec3) -> u64 {
    let cell = point.floor().as_i64vec3();
    lattice_key_3(
        cell.x.cast_unsigned(),
        cell.y.cast_unsigned(),
        cell.z.cast_unsigned(),
    )
}

/// The source winding is counter-clockwise in the island XY plane, which the
/// Y/Z swap mirrors. Swapping the last two indices restores outward faces.
fn reversed_triangles(triangles: &[u32]) -> Vec<u32> {
    triangles
        .as_chunks::<3>()
        .0
        .iter()
        .flat_map(|triangle| [triangle[0], triangle[2], triangle[1]])
        .collect()
}
