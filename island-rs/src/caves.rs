//! Island-local, metre-based caves. Original terrain remains the query source.
#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]
mod clearance;
mod field;
mod options;
mod passage;
mod placement;
mod portal;
mod stitch;
mod surface;
#[cfg(test)]
mod tests;
use crate::{ISLAND_WORLD_METRES, Mesh, Vec2, Vec3, terrain::Terrain};
pub use options::CaveOptions;
pub use placement::CaveStats;
use serde::{Deserialize, Serialize};
pub const CAVE_REVISION: u32 = 7;
pub(crate) const MAX_TRIANGLES: usize = 250_000;
pub(crate) const MAX_CHUNKS: usize = 128;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct CaveSet {
    pub options: CaveOptions,
    pub caves: Vec<Cave>,
    pub stats: CaveStats,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Cave {
    pub id: u64,
    /// Floor at the mouth, in Rust XY-horizontal/Z-up island-local metres.
    pub entrance: Vec3,
    pub inward: Vec2,
    pub nodes: Vec<Node>,
    pub minimum: Vec2,
    pub maximum: Vec2,
    pub surface: surface::SurfaceGrid,
    pub chunks: Vec<Mesh>,
    pub portal: portal::Portal,
    pub entrance_collider: Mesh,
}

impl Cave {
    /// Ambient light and passage tag for a meshed point in island-local metres.
    pub(crate) fn surface_attributes(&self, _options: &CaveOptions, p: Vec3) -> Vec2 {
        let cover = (self.surface.height(p.truncate()) - p.z).max(0.0);
        // Begin ambient darkening after the shared throat, avoiding a lighting
        // step where the terrain lip meets the separate passage material.
        let distance = (p - self.portal.throat())
            .truncate()
            .dot(self.inward)
            .max(0.0);
        let ambient = (1.0 - cover / 4.0)
            .clamp(0.08, 1.0)
            .max((1.0 - distance / 15.0).clamp(0.08, 1.0));
        Vec2::new(ambient, 0.0)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Node {
    pub floor: Vec3,
    pub width: f32,
    pub height: f32,
}

impl CaveSet {
    /// # Errors
    /// Returns invalid configuration or bounded meshing failures.
    pub fn generate(
        seed: u64,
        terrain: &Terrain,
        options: CaveOptions,
        hazard: impl Fn(Vec2) -> bool,
    ) -> Result<Self, String> {
        let _timer = crate::profiling::StageTimer::new("caves.generate");
        let options = options.validate()?;
        if options.enabled == 0 || options.maximum_caves == 0 {
            return Ok(Self {
                options,
                ..Self::default()
            });
        }
        placement::generate(seed, terrain, options, hazard)
    }

    /// Exclude mouth/approach objects, preserving vegetation above deep tunnels.
    #[must_use]
    pub fn excludes(&self, point: Vec3, radius: f32) -> bool {
        self.caves.iter().any(|cave| {
            let delta = point - cave.entrance;
            let along = delta.truncate().dot(cave.inward);
            let across = delta.truncate().perp_dot(cave.inward).abs();
            along > -self.options.approach_length - radius
                && along < self.options.transition_length
                && across < self.options.entrance_width * 0.5 + self.options.side_margin + radius
                && delta.z < self.options.entrance_height + radius
                && delta.z > -2.0 - radius
        })
    }

    #[must_use]
    pub fn cut_surface(&self, mesh: Mesh) -> Mesh {
        self.caves.iter().fold(mesh, |mesh, cave| {
            cave.portal.cut(mesh, &cave.entrance_collider.vertices)
        })
    }

    /// # Errors
    /// Rejects corrupt layouts, meshes and excessive resource counts.
    pub fn validate(&self) -> Result<(), String> {
        self.options.validate()?;
        if self.caves.len() > self.options.maximum_caves as usize
            || (self.options.enabled == 0 && !self.caves.is_empty())
        {
            return Err("too many serialized caves".into());
        }
        let mut triangles = 0;
        let mut chunks = 0;
        for cave in &self.caves {
            if !cave.entrance.is_finite()
                || !cave.inward.is_finite()
                || (cave.inward.length() - 1.0).abs() > 0.001
                || cave.nodes.len() < 2
                || cave.nodes.len() > 64
                || cave.nodes.iter().any(|n| {
                    !n.floor.is_finite()
                        || !n.width.is_finite()
                        || !n.height.is_finite()
                        || n.width < 1.5
                        || n.height < 2.0
                })
            {
                return Err("invalid cave layout".into());
            }
            cave.surface.validate()?;
            if !cave.portal.validate() {
                return Err("invalid cave entrance portal".into());
            }
            if cave.minimum != cave.surface.minimum
                || cave.maximum != cave.surface.maximum()
                || !cave.minimum.cmpge(Vec2::ZERO).all()
                || !cave.maximum.cmple(Vec2::splat(ISLAND_WORLD_METRES)).all()
                || cave.chunks.is_empty()
            {
                return Err("invalid cave bounds or missing geometry".into());
            }
            for mesh in cave
                .chunks
                .iter()
                .chain(std::iter::once(&cave.entrance_collider))
            {
                triangles += mesh.triangles.len() / 3;
                chunks += 1;
                if mesh.vertices.len() != mesh.normals.len()
                    || mesh.vertices.len() != mesh.uv.len()
                    || mesh.triangles.len() % 3 != 0
                    || mesh.vertices.iter().any(|p| !p.is_finite())
                    || mesh.normals.iter().any(|p| !p.is_finite())
                    || mesh
                        .triangles
                        .iter()
                        .any(|&i| i as usize >= mesh.vertices.len())
                {
                    return Err("invalid cave mesh".into());
                }
            }
        }
        if triangles > MAX_TRIANGLES || chunks > MAX_CHUNKS {
            return Err("cave geometry exceeds the island budget".into());
        }
        for cave in &self.caves {
            if cave.entrance_collider.triangles.is_empty() {
                return Err("missing entrance collision".into());
            }
        }
        Ok(())
    }
}

pub(crate) fn terrain_height(terrain: &Terrain, p: Vec2) -> f32 {
    terrain.sample(p.x / ISLAND_WORLD_METRES, p.y / ISLAND_WORLD_METRES) * ISLAND_WORLD_METRES
}
