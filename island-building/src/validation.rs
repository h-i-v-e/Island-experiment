//! Dimension and structural validation before material evaluation or exterior baking.
use crate::{
    BuildingRecipe, Facade, OpeningKind, OpeningRecipe, PartitionAxis, PartitionRecipe,
    SCHEMA_VERSION,
};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, fmt};

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Diagnostic {
    pub element: String,
    pub message: String,
}
#[derive(Clone, Debug)]
pub struct BuildingError {
    pub diagnostics: Vec<Diagnostic>,
}
impl BuildingError {
    #[must_use]
    pub fn new(element: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            diagnostics: vec![Diagnostic {
                element: element.into(),
                message: message.into(),
            }],
        }
    }
}
impl fmt::Display for BuildingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (i, d) in self.diagnostics.iter().enumerate() {
            if i > 0 {
                f.write_str("; ")?;
            }
            write!(f, "{}: {}", d.element, d.message)?;
        }
        Ok(())
    }
}
impl std::error::Error for BuildingError {}

#[derive(Default)]
struct Checks(Vec<Diagnostic>);
impl Checks {
    fn require(&mut self, condition: bool, element: &str, message: &str) {
        if !condition {
            self.0.push(Diagnostic {
                element: element.into(),
                message: message.into(),
            });
        }
    }
    fn range(&mut self, element: &str, value: f32, min: f32, max: f32) {
        self.require(
            value.is_finite() && (min..=max).contains(&value),
            element,
            &format!("must be finite and between {min} and {max} metres"),
        );
    }
    fn unique<'a>(&mut self, ids: &mut BTreeSet<&'a str>, id: &'a str) {
        self.require(
            !id.trim().is_empty() && id.len() <= 128 && ids.insert(id),
            id,
            "element ID must be unique and contain 1 to 128 characters",
        );
    }
}

pub fn validate(recipe: &BuildingRecipe) -> Result<(), BuildingError> {
    validate_structure(recipe)?;
    let (_, doors, colliders) = crate::geometry::generate(recipe);
    validate_door_swings(&doors, &colliders)
}

pub(crate) fn validate_door_swings(
    doors: &[crate::DoorDescriptor],
    colliders: &[crate::collision::BoxCollider],
) -> Result<(), BuildingError> {
    let diagnostics = doors
        .iter()
        .filter(|door| swing_blocked(door, colliders))
        .map(|door| Diagnostic {
            element: door.id.clone(),
            message: concat!(
                "configured door swing intersects a wall, partition, window, or fireplace; ",
                "move the opening, change its hinge side, or reduce the swing"
            )
            .into(),
        })
        .collect::<Vec<_>>();
    if diagnostics.is_empty() {
        Ok(())
    } else {
        Err(BuildingError { diagnostics })
    }
}

// Validated door dimensions/angles require fewer than 5000 positive sample steps.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn swing_blocked(
    door: &crate::DoorDescriptor,
    colliders: &[crate::collision::BoxCollider],
) -> bool {
    let steps = ((door.swing_angle.abs() * door.width / 0.02).ceil() as u16).max(1);
    (0..=steps).any(|step| {
        let angle = door.swing_angle * f32::from(step) / f32::from(steps);
        let leaf = door.collider_at(angle);
        colliders
            .iter()
            .any(|collider| leaf.intersects_box(collider))
    })
}

pub(crate) fn validate_structure(recipe: &BuildingRecipe) -> Result<(), BuildingError> {
    let mut checks = Checks::default();
    structure(recipe, &mut checks);
    let mut ids = BTreeSet::new();
    for (index, opening) in recipe.openings.iter().enumerate() {
        checks.unique(&mut ids, &opening.id);
        opening_rules(recipe, opening, index, &mut checks);
    }
    for (index, partition) in recipe.partitions.iter().enumerate() {
        checks.unique(&mut ids, &partition.id);
        partition_rules(recipe, partition, index, &mut checks);
    }
    palette(recipe, &mut checks);
    chimney_rules(recipe, &mut checks);
    if checks.0.is_empty() {
        Ok(())
    } else {
        Err(BuildingError {
            diagnostics: checks.0,
        })
    }
}
fn structure(r: &BuildingRecipe, c: &mut Checks) {
    c.require(
        r.schema_version == SCHEMA_VERSION,
        "schema_version",
        "unsupported building recipe version",
    );
    c.require(
        !r.name.trim().is_empty() && r.name.len() <= 128,
        "name",
        "name must contain 1 to 128 characters",
    );
    for (name, value, min, max) in [
        ("width", r.width, 3., 40.),
        ("depth", r.depth, 3., 40.),
        ("wall_height", r.wall_height, 2.4, 8.),
        ("wall_thickness", r.wall_thickness, 0.08, 0.6),
        ("floor_thickness", r.floor_thickness, 0.05, 0.5),
        ("roof.rise", r.roof.rise, 0.1, 6.),
        ("roof.overhang", r.roof.overhang, 0., 1.5),
        ("roof.thickness", r.roof.thickness, 0.04, 0.5),
        ("walls.beam_width", r.walls.beam_width, 0.08, 0.24),
        (
            "walls.frame_projection",
            r.walls.frame_projection,
            0.005,
            0.04,
        ),
        ("walls.bay_width", r.walls.bay_width, 0.75, 3.),
    ] {
        c.range(name, value, min, max);
    }
    c.require(
        [128, 256, 512, 1024, 2048].contains(&r.atlas_size),
        "atlas_size",
        "must be 128, 256, 512, 1024, or 2048",
    );
    c.require(
        r.openings.len() <= 64,
        "openings",
        "at most 64 exterior openings are supported",
    );
    c.require(
        r.partitions.len() <= 8,
        "partitions",
        "at most 8 parallel room partitions are supported",
    );
    c.require(
        r.openings.iter().any(|o| o.kind == OpeningKind::Door),
        "openings",
        "at least one openable exterior door is required",
    );
}
fn opening_rules(r: &BuildingRecipe, o: &OpeningRecipe, index: usize, c: &mut Checks) {
    let finite = [o.offset, o.width, o.height, o.sill, o.swing_degrees]
        .iter()
        .all(|v| v.is_finite());
    c.require(finite, &o.id, "opening dimensions must be finite");
    if !finite {
        return;
    }
    c.require(
        o.width >= 0.35
            && o.height >= 0.35
            && o.sill >= 0.
            && o.sill + o.height <= r.wall_height - 0.12,
        &o.id,
        "opening must fit below the lintel with positive dimensions",
    );
    c.require(
        o.offset.abs() + o.width * 0.5 <= o.facade.length(r) * 0.5 - r.wall_thickness - 0.12,
        &o.id,
        "opening must leave solid wall at corners",
    );
    if o.kind == OpeningKind::Door {
        c.require(
            o.width >= 0.95
                && o.height >= 2.1
                && o.sill.abs() <= 0.001
                && (75.0..=135.0).contains(&o.swing_degrees),
            &o.id,
            "door requires 0.95 m width, 2.1 m height, a flush sill, and a 75–135 degree swing",
        );
    }
    for other in &r.openings[..index] {
        let separate = other.facade != o.facade
            || (other.offset - o.offset).abs() >= (other.width + o.width) * 0.5 + 0.14
            || other.sill >= o.sill + o.height + 0.14
            || o.sill >= other.sill + other.height + 0.14;
        c.require(separate, &o.id, "overlaps another opening or its frame");
    }
}
fn partition_rules(r: &BuildingRecipe, p: &PartitionRecipe, index: usize, c: &mut Checks) {
    let finite = [p.offset, p.door_offset, p.door_width, p.door_height]
        .iter()
        .all(|v| v.is_finite());
    c.require(finite, &p.id, "partition dimensions must be finite");
    if !finite {
        return;
    }
    let (across, along) = match p.axis {
        PartitionAxis::X => (r.depth, r.width),
        PartitionAxis::Y => (r.width, r.depth),
    };
    c.require(
        p.offset.abs() <= across * 0.5 - r.wall_thickness * 1.5 - 1.,
        &p.id,
        "partition must leave at least one metre of room depth",
    );
    c.require(
        p.door_width >= 0.95
            && p.door_height >= 2.1
            && p.door_height <= r.wall_height - 0.12
            && p.door_offset.abs() + p.door_width * 0.5 <= along * 0.5 - r.wall_thickness - 0.1,
        &p.id,
        "internal doorway must fit and provide player clearance",
    );
    for other in &r.partitions[..index] {
        c.require(
            other.axis == p.axis,
            &p.id,
            "crossing partitions are unsupported; use parallel room partitions",
        );
        c.require(
            other.axis != p.axis || (other.offset - p.offset).abs() >= 1. + r.wall_thickness,
            &p.id,
            "parallel partitions must leave one metre of clear room width",
        );
    }
    for o in &r.openings {
        let endpoint = match (p.axis, o.facade) {
            (PartitionAxis::X, Facade::East) | (PartitionAxis::Y, Facade::South) => Some(p.offset),
            (PartitionAxis::X, Facade::West) | (PartitionAxis::Y, Facade::North) => Some(-p.offset),
            _ => None,
        };
        c.require(
            endpoint.is_none_or(|v| {
                (v - o.offset).abs() >= o.width * 0.5 + r.wall_thickness * 0.5 + 0.05
            }),
            &p.id,
            "partition meets an exterior opening; move either element",
        );
    }
}
fn palette(r: &BuildingRecipe, c: &mut Checks) {
    let m = &r.materials;
    for (name, colour) in [
        ("materials.wall", m.wall),
        ("materials.roof", m.roof),
        ("materials.floor", m.floor),
        ("materials.ceiling", m.ceiling),
        ("materials.interior", m.interior),
        ("materials.trim", m.trim),
        ("materials.door", m.door),
        ("materials.chimney", m.chimney),
        ("materials.timber", m.timber),
    ] {
        c.require(
            colour
                .iter()
                .all(|v| v.is_finite() && (0.0..=1.0).contains(v)),
            name,
            "colour components must be finite values between 0 and 1",
        );
    }
    c.require(
        m.glass
            .iter()
            .all(|v| v.is_finite() && (0.0..=1.0).contains(v))
            && m.glass[3] <= 0.5,
        "materials.glass",
        "glass requires colour components in 0..1 and opacity no greater than 0.5",
    );
    for (name, path) in [
        ("materials.wall_recipe", &m.wall_recipe),
        ("materials.roof_recipe", &m.roof_recipe),
        ("materials.door_recipe", &m.door_recipe),
        ("materials.chimney_recipe", &m.chimney_recipe),
        ("materials.timber_recipe", &m.timber_recipe),
        ("materials.floor_recipe", &m.floor_recipe),
        ("materials.ceiling_recipe", &m.ceiling_recipe),
    ] {
        c.require(
            path.as_ref()
                .is_none_or(|p| !p.trim().is_empty() && p.len() <= 4096),
            name,
            "material path must contain 1 to 4096 characters",
        );
    }
}

fn chimney_rules(recipe: &BuildingRecipe, checks: &mut Checks) {
    let Some(chimney) = recipe.chimney.as_ref() else {
        return;
    };
    let dimensions = [
        chimney.offset,
        chimney.width,
        chimney.depth,
        chimney.firebox_width,
        chimney.firebox_height,
        chimney.firebox_depth,
        chimney.hearth_depth,
        chimney.hearth_height,
        chimney.stack_width,
        chimney.stack_depth,
        chimney.height_above_roof,
    ];
    if !dimensions
        .iter()
        .chain([
            &recipe.width,
            &recipe.depth,
            &recipe.wall_height,
            &recipe.wall_thickness,
            &recipe.roof.rise,
        ])
        .all(|v| v.is_finite())
    {
        checks.require(
            false,
            "chimney",
            "all chimney and structure dimensions must be finite",
        );
        return;
    }
    chimney_dimensions(recipe, chimney, checks);
    chimney_clearance(recipe, chimney, checks);
}
fn chimney_dimensions(recipe: &BuildingRecipe, c: &crate::ChimneyRecipe, checks: &mut Checks) {
    for (name, value, min, max) in [
        ("chimney.width", c.width, 1.1, 3.5),
        ("chimney.depth", c.depth, 0.4, 1.2),
        ("chimney.firebox_width", c.firebox_width, 0.5, 2.5),
        ("chimney.firebox_height", c.firebox_height, 0.5, 1.4),
        ("chimney.firebox_depth", c.firebox_depth, 0.25, 1.0),
        ("chimney.hearth_depth", c.hearth_depth, 0.1, 0.6),
        ("chimney.hearth_height", c.hearth_height, 0.06, 0.2),
        ("chimney.stack_width", c.stack_width, 0.4, 1.2),
        ("chimney.stack_depth", c.stack_depth, 0.35, 1.0),
        ("chimney.height_above_roof", c.height_above_roof, 0.35, 2.5),
    ] {
        checks.range(name, value, min, max);
    }
    checks.require(
        c.firebox_width <= c.width - 0.24,
        "chimney.firebox_width",
        "firebox must leave at least 12 cm of masonry at each jamb",
    );
    checks.require(
        c.firebox_depth <= c.depth - 0.12,
        "chimney.firebox_depth",
        "firebox must leave at least 12 cm of backing masonry",
    );
    checks.require(
        c.stack_width <= c.width - 0.2 && c.stack_depth <= c.depth - 0.06,
        "chimney",
        "stack must fit within the masonry hood and remain inside the wall",
    );
    checks.require(
        c.firebox_height + c.hearth_height + 0.78 <= recipe.wall_height - 0.1,
        "chimney.firebox_height",
        "fireplace hood must finish below the ceiling",
    );
    let flue_wall = 0.1_f32.min(c.stack_width.min(c.stack_depth) * 0.2);
    checks.require(
        c.firebox_width + 0.00001 >= c.stack_width - 2. * flue_wall
            && c.firebox_depth - 0.12 + 0.00001 >= c.stack_depth - 2. * flue_wall,
        "chimney",
        "hood opening must accommodate the complete flue",
    );
    checks.require(
        c.offset.abs() + c.width * 0.5 + 0.1
            <= c.facade.length(recipe) * 0.5 - recipe.wall_thickness - 0.2,
        "chimney.offset",
        "hearth must leave solid corner clearance and remain inside the footprint",
    );
    let across = match c.facade {
        Facade::South | Facade::North => recipe.depth,
        Facade::East | Facade::West => recipe.width,
    };
    checks.require(
        c.depth + c.hearth_depth + 0.66 <= across - 2. * recipe.wall_thickness,
        "chimney.depth",
        "fireplace must leave a clear player route in front of the hearth",
    );
}
fn chimney_clearance(recipe: &BuildingRecipe, c: &crate::ChimneyRecipe, checks: &mut Checks) {
    let hearth = crate::geometry::chimney::hearth_bounds(recipe).expect("enabled chimney");
    for opening in &recipe.openings {
        if opening.facade == c.facade {
            checks.require(
                (opening.offset - c.offset).abs() >= opening.width * 0.5 + c.width * 0.5 + 0.2,
                "chimney",
                "fireplace overlaps an exterior opening or its frame",
            );
        }
        if opening.kind == OpeningKind::Door {
            let approach = facade_rectangle(
                recipe,
                opening.facade,
                opening.offset,
                opening.width + 0.56,
                recipe.wall_thickness - 0.1,
                recipe.wall_thickness + 0.8,
            );
            checks.require(
                !xy_overlap(hearth, approach, 0.),
                "chimney",
                "hearth obstructs an exterior doorway approach",
            );
        }
    }
    for partition in &recipe.partitions {
        chimney_partition(recipe, c, hearth, partition, checks);
    }
}
fn chimney_partition(
    recipe: &BuildingRecipe,
    c: &crate::ChimneyRecipe,
    hearth: crate::collision::BoxCollider,
    p: &PartitionRecipe,
    checks: &mut Checks,
) {
    let axis = match p.axis {
        PartitionAxis::X => 1,
        PartitionAxis::Y => 0,
    };
    let along = 1 - axis;
    let mut corridor = crate::collision::BoxCollider {
        min: [0.; 3],
        max: [0.; 3],
    };
    corridor.min[axis] = p.offset - recipe.wall_thickness * 0.5 - 0.66;
    corridor.max[axis] = p.offset + recipe.wall_thickness * 0.5 + 0.66;
    corridor.min[along] = p.door_offset - p.door_width * 0.5 - 0.28;
    corridor.max[along] = p.door_offset + p.door_width * 0.5 + 0.28;
    checks.require(
        !xy_overlap(hearth, corridor, 0.),
        "chimney",
        "hearth obstructs an interior doorway or its player approach",
    );
    let parallel = matches!(
        (c.facade, p.axis),
        (Facade::South | Facade::North, PartitionAxis::X)
            | (Facade::East | Facade::West, PartitionAxis::Y)
    );
    if parallel {
        let distance = match c.facade {
            Facade::South => p.offset + recipe.depth * 0.5 - recipe.wall_thickness,
            Facade::North => recipe.depth * 0.5 - recipe.wall_thickness - p.offset,
            Facade::East => recipe.width * 0.5 - recipe.wall_thickness - p.offset,
            Facade::West => p.offset + recipe.width * 0.5 - recipe.wall_thickness,
        };
        checks.require(
            distance >= c.depth + c.hearth_depth + 0.66,
            "chimney",
            "hearth leaves insufficient room before a parallel partition",
        );
    } else {
        checks.require(
            p.offset < hearth.min[axis] - 0.66 - recipe.wall_thickness * 0.5
                || p.offset > hearth.max[axis] + 0.66 + recipe.wall_thickness * 0.5,
            "chimney",
            "hearth intersects or leaves a narrow route beside a room partition",
        );
    }
}
fn facade_rectangle(
    recipe: &BuildingRecipe,
    facade: Facade,
    offset: f32,
    width: f32,
    back: f32,
    front: f32,
) -> crate::collision::BoxCollider {
    let a = facade.point(recipe, offset - width * 0.5, 0., back);
    let b = facade.point(recipe, offset + width * 0.5, 2., front);
    crate::collision::BoxCollider {
        min: std::array::from_fn(|axis| a[axis].min(b[axis])),
        max: std::array::from_fn(|axis| a[axis].max(b[axis])),
    }
}
fn xy_overlap(
    a: crate::collision::BoxCollider,
    b: crate::collision::BoxCollider,
    margin: f32,
) -> bool {
    (0..2).all(|axis| a.min[axis] < b.max[axis] + margin && a.max[axis] > b.min[axis] - margin)
}
