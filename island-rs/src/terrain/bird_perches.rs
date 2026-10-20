//! Sparse wildlife anchors derived from the final terrain and canopy meshes.
//! Recomputed on export so existing cached island snapshots remain compatible.
#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]

use super::{Island, Terrain, TerrainMaterialField};
use crate::{ISLAND_WORLD_METRES, Mesh, Vec2, Vec3, forest::ForestMeshes, rng::Rng};

#[derive(Clone, Copy, Debug, PartialEq)]
#[repr(C)]
pub struct LandBirdPerch {
    pub position: Vec3,
    pub yaw_radians: f32,
    /// 0 = tree canopy, 1 = cliff ledge.
    pub kind: u32,
    /// Row-major LOD0 owner tile, in normalized XY coordinates.
    pub tile: u32,
}

impl Island {
    /// At most one anchor per requested LOD0 tile; unsuitable tiles stay empty.
    #[must_use]
    pub fn land_bird_perches(&self, divisions: u32) -> Vec<LandBirdPerch> {
        generate(
            &self.terrain,
            &self.material,
            &self.forest,
            self.seed,
            divisions,
        )
    }
}

fn generate(
    terrain: &Terrain,
    material: &TerrainMaterialField,
    forest: &ForestMeshes,
    seed: u64,
    divisions: u32,
) -> Vec<LandBirdPerch> {
    if divisions == 0 || divisions > 256 {
        return Vec::new();
    }
    let mut tiles = vec![None; (divisions * divisions) as usize];
    let mut rng = Rng::new(seed ^ 0x6269_7264_7065_7263);
    // Upward-facing canopy skin, rather than trunk height or a guessed tree size.
    for triangle in forest.lod0_foliage.triangles.chunks_exact(3) {
        let [a, b, c] = vertices(&forest.lod0_foliage, triangle);
        let normal = (b - a).cross(c - a).normalize_or_zero();
        if normal.z < 0.85 {
            continue;
        }
        let position = (a + b + c) / 3.0;
        let Some(tile) = tile_index(position, divisions) else {
            continue;
        };
        if tiles[tile].is_some_and(|p: LandBirdPerch| p.position.z >= position.z)
            || position.z - terrain.sample(position.x, position.y) < 3.0 / ISLAND_WORLD_METRES
        {
            continue;
        }
        insert(
            &mut tiles,
            divisions,
            position + Vec3::Z * (0.10 / ISLAND_WORLD_METRES),
            rng.unit() * std::f32::consts::TAU,
            0,
        );
    }
    let mesh = terrain.mesh();
    for triangle in mesh.triangles.chunks_exact(3) {
        let [a, b, c] = vertices(mesh, triangle);
        let normal = (b - a).cross(c - a).normalize_or_zero();
        if normal.z < 0.9 {
            continue;
        }
        let position = (a + b + c) / 3.0;
        let Some(tile) = tile_index(position, divisions) else {
            continue;
        };
        // Do not spend surface queries on a ledge hidden below a selected canopy.
        if position.z < 5.0 / ISLAND_WORLD_METRES
            || tiles[tile].is_some_and(|p: LandBirdPerch| p.position.z >= position.z)
        {
            continue;
        }
        // A level top next to a steep face, not a flat field or beach.
        if !triangle.iter().any(|&i| mesh.normals[i as usize].z < 0.75) {
            continue;
        }
        let surface = material.sample(terrain, position.truncate());
        if surface.z > 0.1 || surface.y > 0.65 {
            continue;
        } // river bed or deep soil
        if let Some(direction) = cliff_direction(terrain, position) {
            insert(
                &mut tiles,
                divisions,
                position + Vec3::Z * (0.06 / ISLAND_WORLD_METRES),
                direction.x.atan2(direction.y),
                1,
            );
        }
    }
    tiles.into_iter().flatten().collect()
}

fn vertices(mesh: &Mesh, triangle: &[u32]) -> [Vec3; 3] {
    [
        mesh.vertices[triangle[0] as usize],
        mesh.vertices[triangle[1] as usize],
        mesh.vertices[triangle[2] as usize],
    ]
}

fn tile_index(position: Vec3, divisions: u32) -> Option<usize> {
    if !position.is_finite()
        || !(0.0..1.0).contains(&position.x)
        || !(0.0..1.0).contains(&position.y)
    {
        return None;
    }
    Some(
        ((position.y * divisions as f32) as u32 * divisions
            + (position.x * divisions as f32) as u32) as usize,
    )
}

fn insert(
    tiles: &mut [Option<LandBirdPerch>],
    divisions: u32,
    position: Vec3,
    yaw: f32,
    kind: u32,
) {
    let Some(tile) = tile_index(position, divisions) else {
        return;
    };
    if tiles[tile].is_none_or(|p| position.z > p.position.z) {
        tiles[tile] = Some(LandBirdPerch {
            position,
            yaw_radians: yaw,
            kind,
            tile: tile as u32,
        });
    }
}

fn cliff_direction(terrain: &Terrain, position: Vec3) -> Option<Vec2> {
    let mut departure = None;
    for i in 0..8 {
        let angle = i as f32 * std::f32::consts::FRAC_PI_4;
        let direction = Vec2::new(angle.cos(), angle.sin());
        // Enough nearly level ground for the feet in every direction.
        let foot = position.truncate() + direction * (0.4 / ISLAND_WORLD_METRES);
        if (terrain.sample(foot.x, foot.y) - position.z).abs() > 0.25 / ISLAND_WORLD_METRES {
            return None;
        }
        let edge = position.truncate() + direction * (8.0 / ISLAND_WORLD_METRES);
        if (0.0..=1.0).contains(&edge.x)
            && (0.0..=1.0).contains(&edge.y)
            && position.z - terrain.sample(edge.x, edge.y) >= 4.0 / ISLAND_WORLD_METRES
        {
            departure = Some(direction);
        }
    }
    departure
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plateau() -> Terrain {
        Terrain::new(Mesh {
            vertices: vec![
                Vec3::new(0., 0., 0.01),
                Vec3::new(1., 0., 0.01),
                Vec3::new(0., 1., 0.01),
                Vec3::new(1., 1., 0.01),
            ],
            normals: vec![Vec3::Z; 4],
            triangles: vec![0, 1, 2, 1, 3, 2],
            ..Mesh::default()
        })
    }
    #[test]
    fn unsuitable_land_and_water_have_no_perches() {
        let terrain = plateau();
        let material = TerrainMaterialField {
            values: vec![crate::Vec4::ZERO; 4],
        };
        assert!(generate(&terrain, &material, &ForestMeshes::default(), 42, 64).is_empty());
        assert!(generate(&terrain, &material, &ForestMeshes::default(), 42, 0).is_empty());
    }
    #[test]
    fn canopy_uses_actual_surface_and_one_highest_anchor_per_tile() {
        let terrain = plateau();
        let material = TerrainMaterialField {
            values: vec![crate::Vec4::ZERO; 4],
        };
        let forest = ForestMeshes {
            lod0_foliage: Mesh {
                vertices: vec![
                    Vec3::new(0.20, 0.20, 0.015),
                    Vec3::new(0.21, 0.20, 0.015),
                    Vec3::new(0.20, 0.21, 0.015),
                    Vec3::new(0.20, 0.20, 0.020),
                    Vec3::new(0.21, 0.20, 0.020),
                    Vec3::new(0.20, 0.21, 0.020),
                ],
                triangles: vec![0, 1, 2, 3, 4, 5],
                ..Mesh::default()
            },
            ..ForestMeshes::default()
        };
        let perches = generate(&terrain, &material, &forest, 42, 64);
        assert_eq!(perches.len(), 1);
        assert_eq!(perches[0].kind, 0);
        assert!((perches[0].position.z - (0.020 + 0.10 / ISLAND_WORLD_METRES)).abs() < 1e-6);
        assert_eq!(perches, generate(&terrain, &material, &forest, 42, 64));
        assert_eq!(perches[0].tile, 13 * 64 + 13);
    }
    #[test]
    fn cliff_requires_level_feet_and_a_real_drop() {
        let mesh = Mesh {
            vertices: vec![
                Vec3::new(0., 0., 0.01),
                Vec3::new(0.5, 0., 0.01),
                Vec3::new(0.505, 0., 0.),
                Vec3::new(1., 0., 0.),
                Vec3::new(0., 1., 0.01),
                Vec3::new(0.5, 1., 0.01),
                Vec3::new(0.505, 1., 0.),
                Vec3::new(1., 1., 0.),
            ],
            normals: vec![Vec3::Z; 8],
            triangles: vec![0, 1, 4, 1, 5, 4, 1, 2, 5, 2, 6, 5, 2, 3, 6, 3, 7, 6],
            ..Mesh::default()
        };
        let terrain = Terrain::new(mesh);
        assert!(cliff_direction(&terrain, Vec3::new(0.499, 0.5, 0.01)).is_some());
        assert!(cliff_direction(&terrain, Vec3::new(0.25, 0.5, 0.01)).is_none());
        assert!(cliff_direction(&terrain, Vec3::new(0.501, 0.5, 0.008)).is_none());
    }
    #[test]
    fn cliff_ledge_is_exported_but_river_and_soil_are_omitted() {
        let mut mesh = Mesh::default();
        for y in [0.0, 1.0] {
            for (index, (x, z)) in [
                (0.0, 0.01),
                (0.496, 0.01),
                (0.5, 0.01),
                (0.505, 0.0),
                (1.0, 0.0),
            ]
            .into_iter()
            .enumerate()
            {
                mesh.vertices.push(Vec3::new(x, y, z));
                mesh.normals.push(if index == 2 {
                    Vec3::new(1.0, 0.0, 0.1).normalize()
                } else {
                    Vec3::Z
                });
            }
        }
        for x in 0..4 {
            mesh.triangles
                .extend([x, x + 1, x + 5, x + 1, x + 6, x + 5]);
        }
        let terrain = Terrain::new(mesh);
        let mut material = TerrainMaterialField {
            values: vec![crate::Vec4::ZERO; 10],
        };
        let forest = ForestMeshes::default();
        let perches = generate(&terrain, &material, &forest, 42, 64);
        assert!(!perches.is_empty());
        assert!(perches.iter().all(|p| p.kind == 1));
        material.values.fill(crate::Vec4::new(1.0, 0.0, 1.0, 0.0));
        assert!(generate(&terrain, &material, &forest, 42, 64).is_empty());
        material.values.fill(crate::Vec4::new(1.0, 1.0, 0.0, 0.0));
        assert!(generate(&terrain, &material, &forest, 42, 64).is_empty());
    }
}
