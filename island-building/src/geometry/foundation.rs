//! Buried foundations and independent entrance stairs for terrain up to 30 degrees.
use super::Builder;
use crate::{
    BuildingRecipe, Facade, MaterialSlot, OpeningKind, OpeningRecipe, collision::BoxCollider,
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
    // The landing clears the plinth and the closed door; steps start outside it.
    let landing = recipe.foundation.outset + LANDING;
    let mut layout = StairLayout {
        door_id: &opening.id,
        facade: opening.facade,
        offset: opening.offset,
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
    // Stair pitch is steeper than 30 degrees, so it eventually intersects even
    // ground descending away from the door. Include both toe corners and every
    // possible highest point in the foundation, rather than just its centre.
    while layout.drop - LANDING_HEIGHT
        < worst_drop(recipe, &layout, foundation) + recipe.foundation.burial_depth
    {
        layout.steps += 1;
        layout.run = landing + f32::from(layout.steps) * STEP_RUN;
        layout.drop = f32::from(layout.steps) * STEP_RISE;
    }
    layout.bounds = local_box(
        recipe,
        &layout,
        0.,
        layout.run,
        -layout.drop - recipe.foundation.burial_depth,
        LANDING_HEIGHT,
    );
    layout
}

fn worst_drop(recipe: &BuildingRecipe, stairs: &StairLayout<'_>, foundation: BoxCollider) -> f32 {
    let distance = [-0.5, 0.5]
        .into_iter()
        .flat_map(|side| {
            let toe =
                stairs
                    .facade
                    .point(recipe, stairs.offset + side * stairs.width, 0., -stairs.run);
            [foundation.min[0], foundation.max[0]]
                .into_iter()
                .flat_map(move |x| {
                    [foundation.min[1], foundation.max[1]].map(|y| (toe[0] - x).hypot(toe[1] - y))
                })
        })
        .fold(0., f32::max);
    distance * MAX_SLOPE_DEGREES.to_radians().tan()
}

fn local_box(
    recipe: &BuildingRecipe,
    stairs: &StairLayout<'_>,
    start: f32,
    end: f32,
    bottom: f32,
    top: f32,
) -> BoxCollider {
    let a = stairs
        .facade
        .point(recipe, stairs.offset - stairs.width * 0.5, bottom, -start);
    let b = stairs
        .facade
        .point(recipe, stairs.offset + stairs.width * 0.5, top, -end);
    BoxCollider {
        min: std::array::from_fn(|i| a[i].min(b[i])),
        max: std::array::from_fn(|i| a[i].max(b[i])),
    }
}

#[must_use]
pub fn stair_colliders(recipe: &BuildingRecipe) -> Vec<BoxCollider> {
    stairs(recipe)
        .flat_map(|stairs| steps(recipe, stairs))
        .collect()
}

fn steps<'a>(
    recipe: &'a BuildingRecipe,
    stairs: StairLayout<'a>,
) -> impl Iterator<Item = BoxCollider> + 'a {
    (0..=stairs.steps).map(move |step| {
        let (start, end) = if step == 0 {
            (0., stairs.landing)
        } else {
            (
                stairs.landing + f32::from(step - 1) * STEP_RUN,
                stairs.landing + f32::from(step) * STEP_RUN,
            )
        };
        local_box(
            recipe,
            &stairs,
            start,
            end,
            stairs.bounds.min[2],
            LANDING_HEIGHT - f32::from(step) * STEP_RISE,
        )
    })
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
            ramp(builder, recipe, stairs);
        } else {
            for shape in steps(recipe, stairs) {
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

fn ramp(builder: &mut Builder, recipe: &BuildingRecipe, stairs: StairLayout<'_>) {
    let landing = local_box(
        recipe,
        &stairs,
        0.,
        stairs.landing,
        stairs.bounds.min[2],
        LANDING_HEIGHT,
    );
    builder.cube(MaterialSlot::Stairs, landing.min, landing.max);
    let point = |side: f32, run: f32, height: f32| {
        stairs.facade.point(
            recipe,
            stairs.offset + side * stairs.width * 0.5,
            height,
            -run,
        )
    };
    let top = [
        point(-1., stairs.landing, LANDING_HEIGHT),
        point(1., stairs.landing, LANDING_HEIGHT),
        point(1., stairs.run, LANDING_HEIGHT - stairs.drop),
        point(-1., stairs.run, LANDING_HEIGHT - stairs.drop),
    ];
    let top = [top[0], top[3], top[2], top[1]];
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
