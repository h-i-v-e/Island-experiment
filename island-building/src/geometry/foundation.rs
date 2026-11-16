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
pub const TREAD_THICKNESS: f32 = 0.045;
pub const TREAD_OVERHANG: f32 = 0.015;
const LANDING: f32 = 0.8;
// Clear the permanent stone cap without coplanar faces; doors have 6 mm clearance.
const LANDING_HEIGHT: f32 = 0.002;

#[must_use]
pub fn bounds(recipe: &BuildingRecipe) -> BoxCollider {
    let x = recipe.width * 0.5 + recipe.foundation.outset;
    let y = recipe.depth * 0.5 + recipe.foundation.outset;
    let footprint = crate::extensions::footprint(recipe);
    let depth = (footprint.max[0] - footprint.min[0]).hypot(footprint.max[1] - footprint.min[1])
        * MAX_SLOPE_DEGREES.to_radians().tan()
        + recipe.foundation.burial_depth;
    BoxCollider {
        min: [-x, -y, -depth],
        max: [x, y, 0.],
    }
}

/// Placement bounds enclose every attached foundation; geometry keeps the actual footprint.
#[must_use]
pub fn enclosing_bounds(recipe: &BuildingRecipe) -> BoxCollider {
    let mut footprint = crate::extensions::footprint(recipe);
    footprint.min[2] = bounds(recipe).min[2];
    footprint
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
    pub tread_thickness: f32,
    pub tread_overhang: f32,
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
        tread_thickness: TREAD_THICKNESS,
        tread_overhang: TREAD_OVERHANG,
        bounds: bounds(recipe),
    };
    let foundation = enclosing_bounds(recipe);
    // A pitch steeper than 30 degrees intersects even ground descending beyond
    // the house. Include every possible high point and both flight toe corners.
    while layout.drop - LANDING_HEIGHT
        < worst_drop(&layout, foundation) + recipe.foundation.burial_depth
    {
        layout.steps += 1;
        layout.run = landing + f32::from(layout.steps) * STEP_RUN;
        layout.drop = f32::from(layout.steps) * STEP_RISE;
    }
    let bottom = -layout.drop - recipe.foundation.burial_depth.max(TREAD_THICKNESS + 0.02);
    layout.landing_bounds.min[2] = bottom;
    let flight = flight_box(&layout, 0., layout.run - landing, bottom, LANDING_HEIGHT);
    layout.bounds = BoxCollider {
        min: std::array::from_fn(|i| flight.min[i].min(layout.landing_bounds.min[i])),
        max: std::array::from_fn(|i| flight.max[i].max(layout.landing_bounds.max[i])),
    };
    for cap in [
        tread_bounds(layout, layout.landing_bounds, true),
        tread_bounds(layout, flight, false),
    ] {
        for axis in 0..2 {
            layout.bounds.min[axis] = layout.bounds.min[axis].min(cap.min[axis]);
            layout.bounds.max[axis] = layout.bounds.max[axis].max(cap.max[axis]);
        }
    }
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
    let toe = tread_bounds(
        *stairs,
        flight_box(
            stairs,
            stairs.run - stairs.landing - STEP_RUN,
            stairs.run - stairs.landing,
            0.,
            0.,
        ),
        false,
    );
    let distance = [toe.min[0], toe.max[0]]
        .into_iter()
        .flat_map(|tx| {
            [toe.min[1], toe.max[1]].into_iter().flat_map(move |ty| {
                [foundation.min[0], foundation.max[0]]
                    .into_iter()
                    .flat_map(move |x| {
                        [foundation.min[1], foundation.max[1]].map(|y| (tx - x).hypot(ty - y))
                    })
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
    stairs(recipe)
        .flat_map(|stairs| steps(stairs).chain(treads(stairs)))
        .collect()
}

fn tread_bounds(stairs: StairLayout<'_>, body: BoxCollider, landing: bool) -> BoxCollider {
    let mut slab = body;
    slab.min[2] = slab.max[2] - TREAD_THICKNESS;
    for axis in 0..2 {
        slab.min[axis] -= TREAD_OVERHANG;
        slab.max[axis] += TREAD_OVERHANG;
        // Fit the landing against the threshold, and side-flight treads against
        // the plinth. Nosing and exposed sides still project beyond the risers.
        if landing || stairs.orientation == StairRun::AlongWall {
            let normal = stairs.facade.normal()[axis];
            if normal > 0.5 {
                slab.min[axis] = body.min[axis];
            }
            if normal < -0.5 {
                slab.max[axis] = body.max[axis];
            }
        }
    }
    slab
}

fn treads(stairs: StairLayout<'_>) -> impl Iterator<Item = BoxCollider> {
    steps(stairs)
        .enumerate()
        .map(move |(index, body)| tread_bounds(stairs, body, index == 0))
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
    project_uvs(builder, MaterialSlot::Foundation, start, foundation);
    generate_stairs(builder, recipe, distant);
}

pub(crate) fn generate_stairs(builder: &mut Builder, recipe: &BuildingRecipe, distant: bool) {
    for stairs in stairs(recipe) {
        let start = builder.mesh(MaterialSlot::Stairs).positions.len();
        if distant {
            ramp(builder, stairs);
        } else {
            let bodies: Vec<_> = steps(stairs)
                .map(|mut shape| {
                    shape.max[2] -= TREAD_THICKNESS;
                    shape
                })
                .collect();
            // One masonry flight is one opaque, inseparable stair component.
            // Its neighbours can cover deep risers, never the foundation or caps.
            super::slabs::joined_boxes(builder, MaterialSlot::Stairs, &bodies);
            for slab in treads(stairs) {
                super::slabs::stone_box(builder, MaterialSlot::StairTreads, slab);
            }
        }
        project_uvs(builder, MaterialSlot::Stairs, start, stairs.bounds);
    }
}

fn project_uvs(builder: &mut Builder, slot: MaterialSlot, start: usize, bounds: BoxCollider) {
    let mesh = builder.mesh(slot);
    for ((point, normal), uv) in mesh.positions[start..]
        .iter()
        .zip(&mesh.normals[start..])
        .zip(&mut mesh.uvs[start..])
    {
        *uv = crate::units::box_uv(bounds, *point, *normal);
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
    let mut landing_top = stairs.landing_bounds.max;
    landing_top[2] -= TREAD_THICKNESS;
    builder.cube(MaterialSlot::Stairs, stairs.landing_bounds.min, landing_top);
    super::slabs::stone_box(
        builder,
        MaterialSlot::StairTreads,
        tread_bounds(stairs, stairs.landing_bounds, true),
    );
    let run = stairs.run - stairs.landing;
    let top = [
        flight_point(&stairs, -0.5, 0., LANDING_HEIGHT - TREAD_THICKNESS),
        flight_point(
            &stairs,
            -0.5,
            run,
            LANDING_HEIGHT - stairs.drop - TREAD_THICKNESS,
        ),
        flight_point(
            &stairs,
            0.5,
            run,
            LANDING_HEIGHT - stairs.drop - TREAD_THICKNESS,
        ),
        flight_point(&stairs, 0.5, 0., LANDING_HEIGHT - TREAD_THICKNESS),
    ];
    let bottom = top.map(|mut point| {
        point[2] = stairs.bounds.min[2];
        point
    });
    prism(builder, MaterialSlot::Stairs, top, bottom);
    let flight = flight_box(&stairs, 0., run, 0., LANDING_HEIGHT);
    let cap = tread_bounds(stairs, flight, false);
    let mut cap_top = top;
    for point in &mut cap_top {
        for (axis, coordinate) in point.iter_mut().enumerate().take(2) {
            *coordinate = if (*coordinate - flight.min[axis]).abs()
                < (*coordinate - flight.max[axis]).abs()
            {
                cap.min[axis]
            } else {
                cap.max[axis]
            };
        }
        point[2] += TREAD_THICKNESS;
    }
    let cap_bottom = cap_top.map(|mut p| {
        p[2] -= TREAD_THICKNESS;
        p
    });
    let start = builder.mesh(MaterialSlot::StairTreads).positions.len();
    prism(builder, MaterialSlot::StairTreads, cap_top, cap_bottom);
    project_uvs(builder, MaterialSlot::StairTreads, start, stairs.bounds);
}

fn prism(builder: &mut Builder, slot: MaterialSlot, top: [[f32; 3]; 4], bottom: [[f32; 3]; 4]) {
    builder.face(slot, top);
    builder.face(slot, [bottom[3], bottom[2], bottom[1], bottom[0]]);
    for i in 0..4 {
        let next = (i + 1) % 4;
        builder.face(slot, [bottom[i], bottom[next], top[next], top[i]]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slabs_cover_treads_project_at_exposed_edges_and_preserve_step_elevations() {
        for facade in Facade::ALL {
            for orientation in [StairRun::Outward, StairRun::AlongWall] {
                for burial in [0., 0.15] {
                    let mut recipe = BuildingRecipe::hut();
                    recipe.foundation.stair_run = orientation;
                    recipe.foundation.burial_depth = burial;
                    recipe.openings[0].facade = facade;
                    let stairs = stairs(&recipe).next().unwrap();
                    for (index, (body, slab)) in steps(stairs).zip(treads(stairs)).enumerate() {
                        assert!((slab.max[2] - body.max[2]).abs() < 0.00001);
                        assert!((slab.max[2] - slab.min[2] - TREAD_THICKNESS).abs() < 0.00001);
                        assert!(
                            slab.min[2] > body.min[2],
                            "body remains non-degenerate even with no burial margin"
                        );
                        for axis in 0..2 {
                            assert!(slab.min[axis] <= body.min[axis]);
                            assert!(slab.max[axis] >= body.max[axis]);
                            assert!(slab.min[axis] >= stairs.bounds.min[axis] - 0.00001);
                            assert!(slab.max[axis] <= stairs.bounds.max[axis] + 0.00001);
                            if index == 0 || orientation == StairRun::AlongWall {
                                let normal = facade.normal()[axis];
                                if normal > 0.5 {
                                    assert!((slab.min[axis] - body.min[axis]).abs() < 0.00001);
                                }
                                if normal < -0.5 {
                                    assert!((slab.max[axis] - body.max[axis]).abs() < 0.00001);
                                }
                            }
                        }
                    }
                    let mut builder = Builder::default();
                    generate(&mut builder, &recipe, false);
                    let geometry = builder.finish();
                    let caps = &geometry
                        .parts
                        .iter()
                        .find(|p| p.material == MaterialSlot::StairTreads)
                        .unwrap()
                        .mesh;
                    assert_eq!(caps.triangle_count(), (usize::from(stairs.steps) + 1) * 12);
                    for (vertices, slab) in caps.positions.chunks_exact(24).zip(treads(stairs)) {
                        for axis in 0..3 {
                            let min = vertices
                                .iter()
                                .map(|p| p[axis])
                                .fold(f32::INFINITY, f32::min);
                            let max = vertices
                                .iter()
                                .map(|p| p[axis])
                                .fold(f32::NEG_INFINITY, f32::max);
                            assert!((min - slab.min[axis]).abs() < 0.00001);
                            assert!((max - slab.max[axis]).abs() < 0.00001);
                        }
                    }
                }
            }
        }
    }
}
