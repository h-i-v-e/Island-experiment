//! Instance-local masonry fitted to sampled terrain, separate from reusable shells.
//!
//! Coordinates are native Z-up metres. Terrain callbacks receive world XY and
//! return world Z; returned geometry and colliders remain in building-local space.
//! Sampling is explicit: callers must use spacing no larger than their terrain
//! detail and provide coverage across the entrance flights as well as the house.
mod stairs;
mod surface;

use crate::{
    BuildingMesh, BuildingPrototype, BuildingRecipe, MaterialSlot,
    collision::BoxCollider,
    geometry::{Builder, foundation},
};
use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct PlacementPose {
    pub origin: [f32; 3],
    pub yaw: f32,
}

impl PlacementPose {
    #[must_use]
    pub fn world_point(self, point: [f32; 3]) -> [f32; 3] {
        let (s, c) = self.yaw.sin_cos();
        [
            self.origin[0] + c * point[0] - s * point[1],
            self.origin[1] + s * point[0] + c * point[1],
            self.origin[2] + point[2],
        ]
    }
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub struct SupportOptions {
    pub sample_spacing: f32,
    pub max_stair_steps: u16,
}

impl Default for SupportOptions {
    fn default() -> Self {
        Self {
            sample_spacing: 0.5,
            max_stair_steps: 256,
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct FittedStair {
    pub door_id: String,
    pub steps: u16,
    pub step_rise: f32,
    pub step_run: f32,
    pub bounds: BoxCollider,
}

#[derive(Clone, Copy)]
pub(crate) struct StairTarget<'a> {
    pub door_id: &'a str,
    pub steps: u16,
    /// Minimum and maximum finished ground height across the toe, in world metres.
    pub grade: [f32; 2],
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct FittedSupports {
    /// One shared foundation-stone material, including nosings and landing caps.
    pub mesh: BuildingMesh,
    pub foundation_colliders: Vec<BoxCollider>,
    pub stair_colliders: Vec<BoxCollider>,
    pub footprint_bounds: BoxCollider,
    pub clearance_bounds: BoxCollider,
    pub occupied_footprints: Vec<BoxCollider>,
    pub stairs: Vec<FittedStair>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum SupportError {
    InvalidSampling,
    TerrainUnavailable([f32; 2]),
    StairsCannotReachTerrain(String),
}

impl fmt::Display for SupportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidSampling => {
                f.write_str("support pose and sampling options must be finite and bounded")
            }
            Self::TerrainUnavailable(p) => write!(f, "terrain unavailable at {}, {}", p[0], p[1]),
            Self::StairsCannotReachTerrain(id) => write!(
                f,
                "entrance {id} cannot reach terrain within the stair budget"
            ),
        }
    }
}
impl std::error::Error for SupportError {}

/// Actual occupied plinth regions. Church recesses and spaces between extensions
/// stay empty; these boxes are never replaced by one solid enclosing rectangle.
/// Zero-depth regions retain the local bearing height in both Z coordinates.
#[must_use]
pub fn foundation_footprints(recipe: &BuildingRecipe) -> Vec<BoxCollider> {
    if recipe.church.is_some() {
        crate::church::foundation_footprints(recipe)
    } else {
        crate::extensions::footprints(recipe).collect()
    }
}

pub fn sampled_terrain_range(
    recipe: &BuildingRecipe,
    pose: PlacementPose,
    sample: &impl Fn(f32, f32) -> Option<f32>,
    spacing: f32,
) -> Result<[f32; 2], SupportError> {
    validate_sampling(pose, spacing)?;
    foundation_footprints(recipe).into_iter().try_fold(
        [f32::INFINITY, f32::NEG_INFINITY],
        |range, bounds| {
            let local = sample_rectangle(bounds, pose, sample, spacing)?;
            Ok([
                range[0].min(local[0] + pose.origin[2]),
                range[1].max(local[1] + pose.origin[2]),
            ])
        },
    )
}

pub fn fit(
    recipe: &BuildingRecipe,
    pose: PlacementPose,
    sample: &impl Fn(f32, f32) -> Option<f32>,
) -> Result<FittedSupports, SupportError> {
    fit_with_options(recipe, pose, sample, SupportOptions::default())
}

pub fn fit_with_options(
    recipe: &BuildingRecipe,
    pose: PlacementPose,
    sample: &impl Fn(f32, f32) -> Option<f32>,
    options: SupportOptions,
) -> Result<FittedSupports, SupportError> {
    fit_with_stair_target(recipe, pose, sample, options, None)
}

pub(crate) fn fit_with_stair_target(
    recipe: &BuildingRecipe,
    pose: PlacementPose,
    sample: &impl Fn(f32, f32) -> Option<f32>,
    options: SupportOptions,
    target: Option<StairTarget<'_>>,
) -> Result<FittedSupports, SupportError> {
    validate_sampling(pose, options.sample_spacing)?;
    if options.max_stair_steps == 0 {
        return Err(SupportError::InvalidSampling);
    }
    let occupied_footprints = foundation_footprints(recipe);
    let foundation_colliders = occupied_footprints
        .iter()
        .map(|&region| {
            let range = sample_rectangle(region, pose, sample, options.sample_spacing)?;
            let mut fitted = region;
            let top = region.max[2];
            fitted.min[2] = (range[0] - burial(recipe)).min(top - 0.02);
            fitted.max[2] = top;
            Ok(fitted)
        })
        .collect::<Result<Vec<_>, SupportError>>()?;
    let footprint_bounds = enclosing(&foundation_colliders).expect("main foundation");
    let mut builder = Builder::default();
    surface::foundation(&mut builder, recipe, &foundation_colliders);
    let (stair_colliders, stairs) =
        stairs::generate(&mut builder, recipe, pose, sample, options, target)?;
    let clearance_bounds = stair_colliders
        .iter()
        .copied()
        .fold(footprint_bounds, union);
    let mesh = builder.mesh(MaterialSlot::Foundation);
    for ((point, normal), uv) in mesh.positions.iter().zip(&mesh.normals).zip(&mut mesh.uvs) {
        *uv = crate::units::box_uv(clearance_bounds, *point, *normal);
    }
    let mut lod = builder.finish();
    crate::finalize::lod(&mut lod);
    let mesh = lod
        .parts
        .into_iter()
        .next()
        .map_or_else(BuildingMesh::default, |p| p.mesh);
    Ok(FittedSupports {
        mesh,
        foundation_colliders,
        stair_colliders,
        footprint_bounds,
        clearance_bounds,
        occupied_footprints,
        stairs,
    })
}

/// Clone at the reusable-prototype ownership boundary, never for each placement.
#[must_use]
pub fn supportless_shell(prototype: &BuildingPrototype) -> BuildingPrototype {
    let mut shell = prototype.clone();
    remove_supports(&mut shell);
    shell
}

/// Consuming callers can strip their owned prototype without cloning its textures.
pub fn remove_supports(prototype: &mut BuildingPrototype) {
    let old_bottom = foundation::bounds(&prototype.recipe).min[2];
    let base_boxes = prototype
        .colliders
        .iter()
        .filter(|b| (b.min[2] - old_bottom).abs() < 0.001 && b.max[2] <= 0.)
        .copied()
        .collect::<Vec<_>>();
    for lod in &mut prototype.lods {
        lod.parts.retain(|part| !part.material.is_foundation());
        // Church LOD2 bakes its foundations into Atlas. Preserve the pier's own
        // downward-facing underside on the shared footing bearing plane.
        for part in &mut lod.parts {
            if part.material == MaterialSlot::Atlas {
                part.mesh.indices = part
                    .mesh
                    .indices
                    .chunks_exact(3)
                    .filter(|tri| {
                        !base_boxes.iter().any(|b| {
                            let bearing_underside = part.mesh.normals[tri[0] as usize][2] < -0.5
                                && tri.iter().all(|&i| {
                                    part.mesh.positions[i as usize][2].to_bits()
                                        == b.max[2].to_bits()
                                });
                            !bearing_underside
                                && tri
                                    .iter()
                                    .all(|&i| b.contains(part.mesh.positions[i as usize]))
                        })
                    })
                    .flatten()
                    .copied()
                    .collect();
            }
        }
        crate::finalize::lod(lod);
    }
    prototype.colliders.retain(|b| !base_boxes.contains(b));
    prototype.stair_colliders.clear();
    // Recompute bounds from shell vertices: old layouts included the deeply
    // buried universal base and long worst-case entrance flights.
    let bounds = prototype.lods[0]
        .parts
        .iter()
        .flat_map(|p| &p.mesh.positions)
        .copied()
        .fold(None, |bounds, point| {
            Some(match bounds {
                None => BoxCollider {
                    min: point,
                    max: point,
                },
                Some(bounds) => union(
                    bounds,
                    BoxCollider {
                        min: point,
                        max: point,
                    },
                ),
            })
        });
    if let Some(bounds) = bounds {
        prototype.layout.bounds_min = bounds.min;
        prototype.layout.bounds_max = bounds.max;
    }
    for bounds in &mut prototype.layout.occupied_bounds {
        bounds.min[2] = bounds.min[2].max(-prototype.recipe.floor_thickness);
    }
}

fn burial(recipe: &BuildingRecipe) -> f32 {
    recipe
        .foundation
        .burial_depth
        .max(foundation::TREAD_THICKNESS + 0.02)
}

fn validate_sampling(pose: PlacementPose, spacing: f32) -> Result<(), SupportError> {
    if pose.origin.iter().all(|v| v.is_finite())
        && pose.yaw.is_finite()
        && spacing.is_finite()
        && spacing > 0.
    {
        Ok(())
    } else {
        Err(SupportError::InvalidSampling)
    }
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn sample_rectangle(
    bounds: BoxCollider,
    pose: PlacementPose,
    sample: &impl Fn(f32, f32) -> Option<f32>,
    spacing: f32,
) -> Result<[f32; 2], SupportError> {
    let counts =
        [0, 1].map(|i| (((bounds.max[i] - bounds.min[i]) / spacing * 0.5).ceil() * 2.).max(2.));
    if counts.iter().any(|n| !n.is_finite() || *n > 2048.) {
        return Err(SupportError::InvalidSampling);
    }
    let [nx, ny] = counts.map(|n| n as u16);
    let mut range = [f32::INFINITY, f32::NEG_INFINITY];
    for ix in 0..=nx {
        for iy in 0..=ny {
            let point = pose.world_point([
                bounds.min[0] + (bounds.max[0] - bounds.min[0]) * f32::from(ix) / f32::from(nx),
                bounds.min[1] + (bounds.max[1] - bounds.min[1]) * f32::from(iy) / f32::from(ny),
                0.,
            ]);
            let height = sample(point[0], point[1])
                .filter(|h| h.is_finite())
                .ok_or(SupportError::TerrainUnavailable([point[0], point[1]]))?
                - pose.origin[2];
            range = [range[0].min(height), range[1].max(height)];
        }
    }
    Ok(range)
}

fn enclosing(boxes: &[BoxCollider]) -> Option<BoxCollider> {
    boxes.iter().copied().reduce(union)
}

fn union(a: BoxCollider, b: BoxCollider) -> BoxCollider {
    BoxCollider {
        min: std::array::from_fn(|i| a.min[i].min(b.min[i])),
        max: std::array::from_fn(|i| a.max[i].max(b.max[i])),
    }
}
