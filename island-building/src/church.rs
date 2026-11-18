//! One connected, unfurnished cruciform church, shared by rendering and collision.
// Validated dimensions, atlas resolution and segment counts bound these mesh conversions.
#![allow(
    clippy::many_single_char_names,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]
mod atlas;
mod buttress;
mod details;
mod doors;
mod eaves;
mod foundations;
mod plan;
mod shell;
mod tracery;
mod windows;

pub(crate) use foundations::footprints as foundation_footprints;

use crate::{
    BuildingError, BuildingLod, BuildingPrototype, BuildingRecipe, Facade, OpeningDimensions,
    OpeningKind, RoofCovering, WallStyle, collision::BoxCollider, geometry::Builder,
    layout::BuildingLayout,
};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ChurchTowerStyle {
    #[default]
    Twin,
    SingleFront,
}

impl ChurchTowerStyle {
    #[allow(clippy::trivially_copy_pass_by_ref)]
    const fn is_twin(&self) -> bool {
        matches!(self, Self::Twin)
    }
}

const fn enabled() -> bool {
    true
}

#[allow(clippy::trivially_copy_pass_by_ref)]
const fn is_enabled(value: &bool) -> bool {
    *value
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct ChurchRecipe {
    pub nave_width: f32,
    pub transept_depth: f32,
    /// Crossing centre along the nave, positive towards the chancel.
    pub crossing_offset: f32,
    #[serde(skip_serializing_if = "ChurchTowerStyle::is_twin")]
    pub tower_style: ChurchTowerStyle,
    pub tower_width: f32,
    pub tower_height: f32,
    #[serde(default = "enabled", skip_serializing_if = "is_enabled")]
    pub flying_buttresses: bool,
    pub buttress_reach: f32,
    pub bay_spacing: f32,
}
impl Default for ChurchRecipe {
    fn default() -> Self {
        Self {
            nave_width: 9.,
            transept_depth: 8.,
            crossing_offset: 5.,
            tower_style: ChurchTowerStyle::Twin,
            tower_width: 4.5,
            tower_height: 20.,
            flying_buttresses: true,
            buttress_reach: 3.,
            bay_spacing: 3.6,
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ChurchLayout {
    /// Counter-clockwise exterior outline at floor level, excluding buttresses.
    pub outline: Vec<[f32; 2]>,
    pub nave: BoxCollider,
    pub transept: BoxCollider,
    pub towers: Vec<BoxCollider>,
}

impl BuildingRecipe {
    #[must_use]
    pub fn gothic_church() -> Self {
        let mut r = Self::hut();
        r.name = "Stone Gothic church".into();
        r.width = 26.;
        r.depth = 38.;
        r.wall_height = 10.;
        r.wall_thickness = 0.55;
        r.floor_thickness = 0.3;
        r.church = Some(ChurchRecipe::default());
        r.chimney = None;
        r.roof.covering = RoofCovering::Slate;
        r.roof.rise = 5.;
        r.roof.overhang = 0.45;
        r.grub_filter.enabled = true;
        r.grub_filter.height_falloff = 3.;
        r.opening_defaults.window = OpeningDimensions {
            width: 2.25,
            height: 6.,
            sill: 1.65,
        };
        r.opening_defaults.door = OpeningDimensions {
            width: 2.4,
            height: 3.6,
            sill: 0.,
        };
        r.openings.truncate(1);
        let door = &mut r.openings[0];
        door.offset = 0.;
        door.use_default_dimensions = true;
        r.materials.wall = [0.58, 0.53, 0.43];
        r.materials.interior = r.materials.wall;
        r.materials.trim = [0.70, 0.64, 0.52];
        r.materials.roof = [0.15, 0.19, 0.23];
        r.materials.floor = [0.48, 0.45, 0.39];
        r.materials.ceiling = [0.65, 0.61, 0.51];
        r.materials.glass = [0.32, 0.57, 0.69, 0.23];
        r
    }

    /// A smaller cathedral-style church with one projecting entrance tower.
    #[must_use]
    pub fn parish_church() -> Self {
        let mut r = Self::gothic_church();
        r.name = "Stone Gothic parish church".into();
        r.width = 16.;
        r.depth = 24.;
        r.wall_height = 6.8;
        r.wall_thickness = 0.4;
        r.floor_thickness = 0.25;
        r.roof.rise = 3.2;
        r.roof.overhang = 0.3;
        r.church = Some(ChurchRecipe {
            nave_width: 6.5,
            transept_depth: 5.5,
            crossing_offset: 3.,
            tower_style: ChurchTowerStyle::SingleFront,
            tower_width: 4.2,
            tower_height: 14.,
            flying_buttresses: false,
            bay_spacing: 3.,
            ..ChurchRecipe::default()
        });
        r.opening_defaults.window = OpeningDimensions {
            width: 1.5,
            height: 4.2,
            sill: 1.1,
        };
        r.opening_defaults.door = OpeningDimensions {
            width: 1.8,
            height: 2.7,
            sill: 0.,
        };
        r
    }
}

pub(crate) fn validate(r: &BuildingRecipe) -> Result<(), BuildingError> {
    let c = r.church.as_ref().expect("church formula");
    let finite = [
        c.nave_width,
        c.transept_depth,
        c.crossing_offset,
        c.tower_width,
        c.tower_height,
        c.buttress_reach,
        c.bay_spacing,
    ]
    .iter()
    .all(|v| v.is_finite());
    let w = r.opening_defaults.window;
    let valid = finite
        && r.storeys == 1
        && r.walls.style == WallStyle::Plain
        && r.chimney.is_none()
        && r.wall_height >= 6.
        && r.roof.rise >= 2.
        && r.wall_height + r.roof.rise
            - 1.2
            - 3_f32.sqrt() * (c.nave_width * 0.5 - r.wall_thickness - 0.12)
            > 2.5
        && r.partitions.is_empty()
        && r.opening_generation.is_none()
        && r.roof.kind == crate::RoofKind::Gable
        && r.roof.covering != RoofCovering::Thatch
        && (5.0..=r.width - 4.).contains(&c.nave_width)
        && (4.0..=r.depth * 0.4).contains(&c.transept_depth)
        && c.crossing_offset.abs() + c.transept_depth * 0.5 + 4. < r.depth * 0.5
        && (2.5..=6.).contains(&c.tower_width)
        && match c.tower_style {
            ChurchTowerStyle::Twin => c.nave_width + 2. * c.tower_width <= r.width,
            ChurchTowerStyle::SingleFront => c.tower_width + 2. * r.wall_thickness < c.nave_width,
        }
        && c.crossing_offset - c.transept_depth * 0.5 > -r.depth * 0.5 + c.tower_width + 2.
        && (r.wall_height + r.roof.rise + 1.0..=30.).contains(&c.tower_height)
        && (!c.flying_buttresses
            || ((1.5..=5.).contains(&c.buttress_reach)
                && c.nave_width * 0.5 + c.buttress_reach + 0.6 < r.width * 0.5))
        && (2.5..=6.).contains(&c.bay_spacing)
        && [w.width, w.height, w.sill].iter().all(|v| v.is_finite())
        && (1.0..=c.bay_spacing - 0.6).contains(&w.width)
        && w.height > w.width * 0.87 + 0.8
        && w.sill >= 0.8
        && w.sill + w.height + 0.5 < r.wall_height
        && r.openings.len() == 1
        && r.openings[0].kind == OpeningKind::Door
        && r.openings[0].facade == Facade::South
        && r.openings[0].offset.abs() < 0.001
        && r.openings[0].width + 1.
            < match c.tower_style {
                ChurchTowerStyle::Twin => c.nave_width,
                ChurchTowerStyle::SingleFront => c.tower_width,
            };
    let entrance_fits = r
        .openings
        .first()
        .is_some_and(|d| d.height + d.width * 0.5 * 3_f32.sqrt() + 0.3 < r.wall_height);
    if valid && entrance_fits {
        Ok(())
    } else {
        Err(BuildingError::new(
            "church",
            "church needs one centred south entrance whose complete pointed arch fits the wall height, one plain-walled storey, no partitions/chimney, a crossing and towers inside the footprint, and windows that fit their bays and wall height",
        ))
    }
}

pub(crate) fn generate(
    r: &BuildingRecipe,
    base: &Path,
) -> Result<BuildingPrototype, BuildingError> {
    validate(r)?;
    let plan = plan::Plan::new(r);
    let mut near = Builder::default();
    let mut colliders = Vec::new();
    let mut supports = Vec::new();
    shell::generate(&mut near, &plan, 0, &mut colliders, &mut supports);
    details::generate(&mut near, &plan, 0, &mut colliders);
    let mut doors = doors::near(r);
    crate::validation::validate_door_swings(&doors, &colliders)?;
    let mut near = near.finish();
    let mut middle = exterior(&plan, 1);
    let mut distant = exterior(&plan, 2);
    for lod in [&mut near, &mut middle, &mut distant] {
        crate::finalize::lod(lod);
    }
    for door in &mut doors {
        crate::finalize::door(door);
    }
    let materials = crate::materials::generate(r, base)?;
    let atlas = atlas::bake(r, &materials);
    let hash = crate::materials::prototype_hash(r, &materials)?;
    let (min, max) = bounds(&near);
    Ok(BuildingPrototype {
        recipe: r.clone(),
        layout: BuildingLayout {
            extensions: Vec::new(),
            occupied_bounds: Vec::new(),
            storeys: 1,
            floor_heights: vec![0.],
            staircase: None,
            church: Some(plan.layout()),
            bounds_min: min,
            bounds_max: max,
            footprint: [r.width, r.depth],
            room_count: 1,
            entrance: [0., -r.depth * 0.5, 0.],
        },
        lods: [near, middle, distant],
        doors,
        colliders,
        support_colliders: supports,
        stair_colliders: crate::geometry::foundation::stair_colliders(r),
        atlas,
        materials,
        hash,
    })
}

fn exterior(plan: &plan::Plan<'_>, level: u8) -> BuildingLod {
    let mut b = Builder::default();
    shell::generate(&mut b, plan, level, &mut Vec::new(), &mut Vec::new());
    details::generate(&mut b, plan, level, &mut Vec::new());
    b.finish()
}

fn bounds(lod: &BuildingLod) -> ([f32; 3], [f32; 3]) {
    lod.parts.iter().flat_map(|p| &p.mesh.positions).fold(
        ([f32::INFINITY; 3], [f32::NEG_INFINITY; 3]),
        |(mut min, mut max), p| {
            for i in 0..3 {
                min[i] = min[i].min(p[i]);
                max[i] = max[i].max(p[i]);
            }
            (min, max)
        },
    )
}
