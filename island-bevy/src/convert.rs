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
use motu::{ISLAND_WORLD_METRES, Island};

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
    if source.triangles.is_empty() || source.vertices.is_empty() {
        return None;
    }
    let positions: Vec<[f32; 3]> = source
        .vertices
        .iter()
        .map(|vertex| island_to_world(vertex.x, vertex.y, vertex.z).to_array())
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
        RenderAssetUsages::default(),
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

/// Converts a terrain mesh and bakes the generator's material weights into
/// [`Mesh::ATTRIBUTE_COLOR`].
#[must_use]
pub fn terrain_mesh(island: &Island, source: &motu::Mesh) -> Option<Mesh> {
    let mut mesh = render_mesh(source)?;
    let palette = Palette::new();
    let materials = island.material_values_for(source);
    let max_height = island.options().max_height.max(f32::EPSILON);
    let colours: Vec<[f32; 4]> = source
        .vertices
        .iter()
        .enumerate()
        .map(|(index, vertex)| {
            let normal_z = source.normals.get(index).map_or(1.0, |normal| normal.z);
            let material = materials
                .get(index)
                .copied()
                .unwrap_or(motu::Vec3::new(0.5, 1.0, 0.0));
            palette.shade(vertex.z, normal_z, material, max_height)
        })
        .collect();
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colours);
    Some(mesh)
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

/// Linear-space terrain bands mirroring the generator's own preview palette in
/// `island-rs/src/raster.rs` and the material weighting the Unity terrain
/// shader applies on top of it.
struct Palette {
    deep: Vec3,
    seabed: Vec3,
    sand: Vec3,
    dirt: Vec3,
    grass_low: Vec3,
    grass_high: Vec3,
    rock: Vec3,
    snow: Vec3,
}

impl Palette {
    fn new() -> Self {
        Self {
            deep: linear(0.04, 0.17, 0.30),
            seabed: linear(0.50, 0.48, 0.35),
            sand: linear(0.761, 0.698, 0.463),
            dirt: linear(0.42, 0.34, 0.22),
            grass_low: linear(0.188, 0.463, 0.165),
            grass_high: linear(0.322, 0.365, 0.165),
            rock: linear(0.439, 0.412, 0.357),
            snow: linear(0.922, 0.933, 0.941),
        }
    }

    /// Shades one vertex from its island-space elevation, the up component of
    /// its island-space normal, and the generator's material triple: x is
    /// bedrock hardness (exactly one means forced rock), y is loose cover, and
    /// z is sea proximity.
    fn shade(
        &self,
        elevation: f32,
        normal_z: f32,
        material: motu::Vec3,
        max_height: f32,
    ) -> [f32; 4] {
        let metres = elevation * ISLAND_WORLD_METRES;
        if elevation <= 0.0 {
            let depth = smoothstep(0.0, 6.0, -metres);
            return rgba(self.seabed.lerp(self.deep, depth));
        }

        let hardness = material.x.clamp(0.0, 1.0);
        let cover = material.y.clamp(0.0, 1.0);
        let sea_proximity = material.z.clamp(0.0, 1.0);
        let height = (elevation / max_height).clamp(0.0, 1.0);
        let slope = (1.0 - normal_z).clamp(0.0, 1.0);

        // Thin deposits expose bare dirt; established cover greens with height.
        let grass = self.grass_low.lerp(self.grass_high, height);
        let ground = self.dirt.lerp(grass, smoothstep(0.05, 0.70, cover));

        // Beaches need loose material within a few metres of the open sea.
        let shore = 1.0 - smoothstep(2.0, 6.0, metres);
        let sand_richness = cover * sea_proximity * shore;
        let mut colour = ground.lerp(self.sand, smoothstep(0.08, 0.45, sand_richness));

        // Forced rock is a one-hot maximum; harder bedrock breaks out at
        // shallower angles, and alpine ground turns bare well below the snow.
        let forced_rock = smoothstep(0.97, 1.0, hardness);
        let geology_rock = smoothstep(0.20, 0.60, slope * (1.3 + hardness * 1.7));
        let alpine_rock = smoothstep(0.55, 0.80, height);
        colour = colour.lerp(self.rock, forced_rock.max(geology_rock).max(alpine_rock));

        let snow = smoothstep(0.72, 1.0, height) * (1.0 - slope).clamp(0.0, 1.0);
        rgba(colour.lerp(self.snow, snow))
    }
}

fn linear(red: f32, green: f32, blue: f32) -> Vec3 {
    let colour = LinearRgba::from(Srgba::rgb(red, green, blue));
    Vec3::new(colour.red, colour.green, colour.blue)
}

fn rgba(colour: Vec3) -> [f32; 4] {
    [colour.x, colour.y, colour.z, 1.0]
}

fn smoothstep(low: f32, high: f32, value: f32) -> f32 {
    let interpolation = ((value - low) / (high - low)).clamp(0.0, 1.0);
    interpolation * interpolation * (3.0 - 2.0 * interpolation)
}
