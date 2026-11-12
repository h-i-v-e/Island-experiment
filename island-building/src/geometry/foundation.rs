//! Buried foundations and independent entrance stairs for terrain up to 30 degrees.
use super::Builder;
use crate::{
    BuildingRecipe, Facade, MaterialSlot, OpeningKind, OpeningRecipe, StairRun,
    collision::BoxCollider,
};
use serde::Serialize;

pub const MAX_SLOPE_DEGREES: f32 = 30.;
pub const STEP_RISE: f32 = 0.20;
pub const STEP_RUN: f32 = 0.24;
const LANDING: f32 = 0.8;
// Clear the permanent stone cap without coplanar faces; doors have 6 mm clearance.
const LANDING_HEIGHT: f32 = 0.002;

#[must_use]
pub fn bounds(recipe: &BuildingRecipe) -> BoxCollider {
    let x = recipe.width * 0.5 + recipe.foundation.outset;
    let y = recipe.depth * 0.5 + recipe.foundation.outset;
    let depth = (x * 2.).hypot(y * 2.) * MAX_SLOPE_DEGREES.to_radians().tan()
        + recipe.foundation.burial_depth;
    BoxCollider {
        min: [-x, -y, -depth],
        max: [x, y, 0.],
    }
}

#[derive(Clone, Copy, Debug, Serialize)]
pub struct StairLayout<'a> {
    pub door_id: &'a str,
    pub facade: Facade,
    pub offset: f32,
    pub orientation: StairRun,
    /// Native XY unit vector from the landing down the flight.
    pub direction: [f32; 2],
    /// Centre of the first tread's upper edge, in native XY metres.
    pub flight_origin: [f32; 2],
    pub landing_bounds: BoxCollider,
    pub width: f32,
    pub landing: f32,
    pub landing_height: f32,
    pub steps: u16,
    pub run: f32,
    pub drop: f32,
    pub step_rise: f32,
    pub step_run: f32,
    pub bounds: BoxCollider,
}

pub fn stairs(recipe: &BuildingRecipe) -> impl Iterator<Item = StairLayout<'_>> {
    recipe
        .openings
        .iter()
        .filter(|opening| recipe.foundation.stairs && opening.kind == OpeningKind::Door)
        .map(|opening| stair_layout(recipe, opening))
}

fn stair_layout<'a>(recipe: &BuildingRecipe, opening: &'a OpeningRecipe) -> StairLayout<'a> {
    let width = opening.width + 0.3;
    let orientation = recipe.foundation.stair_run;
    let normal = opening.facade.normal();
    let tangent = opening.facade.tangent();
    // Looking from outside, positive offsets leave more wall on the left.
    // Equal spans consistently use the right-hand side.
    let side = if opening.offset > 0. { -1. } else { 1. };
    let (direction, landing, origin, landing_bounds) = match orientation {
        StairRun::Outward => {
            let landing = recipe.foundation.outset + LANDING;
            let origin = opening.facade.point(recipe, opening.offset, 0., -landing);
            (
                normal,
                landing,
                origin,
                facade_box(recipe, opening, width, 0., landing),
            )
        }
        StairRun::AlongWall => {
            let landing = width * 0.5;
            let origin = opening.facade.point(
                recipe,
                opening.offset + side * landing,
                0.,
                -(recipe.foundation.outset + width * 0.5),
            );
            (
                tangent.map(|v| v * side),
                landing,
                origin,
                facade_box(recipe, opening, width, 0., recipe.foundation.outset + width),
            )
        }
    };
    let mut layout = StairLayout {
        door_id: &opening.id,
        facade: opening.facade,
        offset: opening.offset,
        orientation,
        direction: [direction[0], direction[1]],
        flight_origin: [origin[0], origin[1]],
        landing_bounds,
        width,
        landing,
        landing_height: LANDING_HEIGHT,
        steps: 1,
        run: landing + STEP_RUN,
        drop: STEP_RISE,
        step_rise: STEP_RISE,
        step_run: STEP_RUN,
        bounds: bounds(recipe),
    };
    let foundation = bounds(recipe);
    // A pitch steeper than 30 degrees intersects even ground descending beyond
    // the house. Include every possible high point and both flight toe corners.
    while layout.drop - LANDING_HEIGHT
        < worst_drop(&layout, foundation) + recipe.foundation.burial_depth
    {
        layout.steps += 1;
        layout.run = landing + f32::from(layout.steps) * STEP_RUN;
        layout.drop = f32::from(layout.steps) * STEP_RISE;
    }
    let bottom = -layout.drop - recipe.foundation.burial_depth;
    layout.landing_bounds.min[2] = bottom;
    let flight = flight_box(&layout, 0., layout.run - landing, bottom, LANDING_HEIGHT);
    layout.bounds = BoxCollider {
        min: std::array::from_fn(|i| flight.min[i].min(layout.landing_bounds.min[i])),
        max: std::array::from_fn(|i| flight.max[i].max(layout.landing_bounds.max[i])),
    };
    layout
}

fn facade_box(
    recipe: &BuildingRecipe,
    opening: &OpeningRecipe,
    width: f32,
    start: f32,
    end: f32,
) -> BoxCollider {
    let a = opening
        .facade
        .point(recipe, opening.offset - width * 0.5, 0., -start);
    let b = opening
        .facade
        .point(recipe, opening.offset + width * 0.5, LANDING_HEIGHT, -end);
    BoxCollider {
        min: std::array::from_fn(|i| a[i].min(b[i])),
        max: std::array::from_fn(|i| a[i].max(b[i])),
    }
}

fn worst_drop(stairs: &StairLayout<'_>, foundation: BoxCollider) -> f32 {
    let distance = [-0.5, 0.5]
        .into_iter()
        .flat_map(|side| {
            let toe = flight_point(stairs, side, stairs.run - stairs.landing, 0.);
            [foundation.min[0], foundation.max[0]]
                .into_iter()
                .flat_map(move |x| {
                    [foundation.min[1], foundation.max[1]].map(|y| (toe[0] - x).hypot(toe[1] - y))
                })
        })
        .fold(0., f32::max);
    distance * MAX_SLOPE_DEGREES.to_radians().tan()
}

fn flight_point(stairs: &StairLayout<'_>, side: f32, run: f32, height: f32) -> [f32; 3] {
    let [dx, dy] = stairs.direction;
    [
        stairs.flight_origin[0] + dx * run - dy * side * stairs.width,
        stairs.flight_origin[1] + dy * run + dx * side * stairs.width,
        height,
    ]
}

fn flight_box(
    stairs: &StairLayout<'_>,
    start: f32,
    end: f32,
    bottom: f32,
    top: f32,
) -> BoxCollider {
    let a = flight_point(stairs, -0.5, start, bottom);
    let b = flight_point(stairs, 0.5, end, top);
    BoxCollider {
        min: std::array::from_fn(|i| a[i].min(b[i])),
        max: std::array::from_fn(|i| a[i].max(b[i])),
    }
}

#[must_use]
pub fn stair_colliders(recipe: &BuildingRecipe) -> Vec<BoxCollider> {
    stairs(recipe).flat_map(steps).collect()
}

fn steps(stairs: StairLayout<'_>) -> impl Iterator<Item = BoxCollider> {
    std::iter::once(stairs.landing_bounds).chain((1..=stairs.steps).map(move |step| {
        flight_box(
            &stairs,
            f32::from(step - 1) * STEP_RUN,
            f32::from(step) * STEP_RUN,
            stairs.bounds.min[2],
            LANDING_HEIGHT - f32::from(step) * STEP_RISE,
        )
    }))
}

pub(crate) fn generate(builder: &mut Builder, recipe: &BuildingRecipe, distant: bool) {
    let foundation = bounds(recipe);
    // The top is a narrow stone ring outside the wooden floor, with no
    // overlapping coplanar top face underneath the floorboards.
    let start = builder.mesh(MaterialSlot::Foundation).positions.len();
    builder.cube(MaterialSlot::Foundation, foundation.min, foundation.max);
    let mesh = builder.mesh(MaterialSlot::Foundation);
    mesh.indices.retain(|index| {
        let index = *index as usize;
        index < start || mesh.normals[index][2] < 0.5
    });
    top_ring(builder, recipe);
    for stairs in stairs(recipe) {
        if distant {
            ramp(builder, stairs);
        } else {
            for shape in steps(stairs) {
                builder.cube(MaterialSlot::Stairs, shape.min, shape.max);
            }
        }
    }
}

fn top_ring(builder: &mut Builder, recipe: &BuildingRecipe) {
    let [x, y, _] = bounds(recipe).max;
    let w = recipe.width * 0.5;
    let d = recipe.depth * 0.5;
    for (min, max) in [
        ([-x, -y, 0.], [x, -d, 0.]),
        ([-x, d, 0.], [x, y, 0.]),
        ([-x, -d, 0.], [-w, d, 0.]),
        ([w, -d, 0.], [x, d, 0.]),
    ] {
        builder.face(
            MaterialSlot::Foundation,
            [min, [max[0], min[1], 0.], max, [min[0], max[1], 0.]],
        );
    }
}

fn ramp(builder: &mut Builder, stairs: StairLayout<'_>) {
    builder.cube(
        MaterialSlot::Stairs,
        stairs.landing_bounds.min,
        stairs.landing_bounds.max,
    );
    let run = stairs.run - stairs.landing;
    let top = [
        flight_point(&stairs, -0.5, 0., LANDING_HEIGHT),
        flight_point(&stairs, -0.5, run, LANDING_HEIGHT - stairs.drop),
        flight_point(&stairs, 0.5, run, LANDING_HEIGHT - stairs.drop),
        flight_point(&stairs, 0.5, 0., LANDING_HEIGHT),
    ];
    builder.face(MaterialSlot::Stairs, top);
    let bottom = top.map(|mut point| {
        point[2] = stairs.bounds.min[2];
        point
    });
    builder.face(
        MaterialSlot::Stairs,
        [bottom[3], bottom[2], bottom[1], bottom[0]],
    );
    for i in 0..4 {
        let next = (i + 1) % 4;
        builder.face(
            MaterialSlot::Stairs,
            [bottom[i], bottom[next], top[next], top[i]],
        );
    }
}
