//! LOD0 open treads housed between continuous timber stringers.
use super::Builder;
use crate::{
    BuildingRecipe, MaterialSlot, StairwellSide, WallStyle,
    collision::BoxCollider,
    stairs::{FLIGHT_GAP, RAIL_HEIGHT, StaircaseLayout, TREAD_THICKNESS},
};

const STRINGER_WIDTH: f32 = 0.06;
const STRINGER_LIP: f32 = 0.04;
pub(crate) const WALL_GUARD_CLEARANCE: f32 = 0.60;

struct Stringer {
    across: [f32; 2],
    /// Convex profile in along/height coordinates, with a flat top at the landing.
    profile: [[f32; 2]; 5],
}

struct Rail {
    across: [f32; 2],
    along: [f32; 2],
    height: [f32; 2],
}
#[derive(Default)]
struct Assembly {
    solids: Vec<BoxCollider>,
    treads: Vec<BoxCollider>,
    stringers: Vec<Stringer>,
    colliders: Vec<BoxCollider>,
    supports: Vec<BoxCollider>,
    rails: Vec<Rail>,
}

impl Assembly {
    fn solid(&mut self, bounds: BoxCollider, support: bool) {
        self.solids.push(bounds);
        self.colliders.push(bounds);
        if support {
            self.supports.push(bounds);
        }
    }
}

pub(crate) fn collision(recipe: &BuildingRecipe, layout: &StaircaseLayout) -> Vec<BoxCollider> {
    assemble(recipe, layout).colliders
}

pub(crate) fn generate(
    builder: &mut Builder,
    recipe: &BuildingRecipe,
    layout: &StaircaseLayout,
    colliders: &mut Vec<BoxCollider>,
    supports: &mut Vec<BoxCollider>,
) {
    let assembly = assemble(recipe, layout);
    for solid in assembly.solids {
        builder.cube(MaterialSlot::Timber, solid.min, solid.max);
    }
    for tread in assembly.treads {
        tread_mesh(builder, layout, tread);
    }
    for stringer in assembly.stringers {
        stringer_mesh(builder, layout, &stringer);
    }
    for rail in assembly.rails {
        rail_mesh(builder, layout, &rail);
    }
    colliders.extend(assembly.colliders);
    supports.extend(assembly.supports);
}

fn assemble(recipe: &BuildingRecipe, layout: &StaircaseLayout) -> Assembly {
    let mut result = Assembly::default();
    for &landing in layout.landings.iter().skip(1) {
        result.solid(landing, true);
    }
    for (level, &base) in layout
        .floor_heights
        .iter()
        .enumerate()
        .take(layout.floor_heights.len() - 1)
    {
        result.solid(layout.turn_landings[level], true);
        flight(&mut result, layout, base, false);
        flight(&mut result, layout, base, true);
        let middle = layout.turn_landings[level].max[2];
        edge_guards(
            &mut result,
            recipe,
            layout,
            middle,
            layout.landing_depth + layout.run(),
        );
    }
    for (level, &height) in layout.floor_heights.iter().enumerate().skip(1) {
        edge_guards(&mut result, recipe, layout, height, layout.landing_depth);
        if level == layout.floor_heights.len() - 1 {
            guard(
                &mut result,
                layout,
                [0., layout.landing_depth],
                [layout.width, layout.landing_depth],
                height,
            );
        }
    }
    result
}

fn edge_guards(
    result: &mut Assembly,
    recipe: &BuildingRecipe,
    layout: &StaircaseLayout,
    height: f32,
    start: f32,
) {
    let exposed = |side: StairwellSide| {
        layout.stairwell.as_ref().is_none_or(|room| {
            room.enclosure
                .is_some_and(|plan| plan.shared_sides[side.index()])
        }) && wall_gap(recipe, layout, side) > WALL_GUARD_CLEARANCE + 0.00001
    };
    if exposed(StairwellSide::Far) {
        guard(
            result,
            layout,
            [0., layout.length()],
            [layout.span(), layout.length()],
            height,
        );
    }
    for (u, side) in [
        (0., StairwellSide::Left),
        (layout.span(), StairwellSide::Right),
    ] {
        if exposed(side) {
            guard(result, layout, [u, start], [u, layout.length()], height);
        }
    }
}

fn wall_gap(recipe: &BuildingRecipe, layout: &StaircaseLayout, side: StairwellSide) -> f32 {
    let normal = side.outward();
    let origin = layout.point(0., 0., 0.);
    let tip = layout.point(normal[0], normal[1], 0.);
    let axis = usize::from((tip[1] - origin[1]).abs() > 0.5);
    let sign = (tip[axis] - origin[axis]).signum();
    let edge = match side {
        StairwellSide::Near => [layout.span() * 0.5, 0.],
        StairwellSide::Right => [layout.span(), layout.length() * 0.5],
        StairwellSide::Far => [layout.span() * 0.5, layout.length()],
        StairwellSide::Left => [0., layout.length() * 0.5],
    };
    let projection = if recipe.walls.style == WallStyle::WattleAndDaub {
        recipe.walls.frame_projection
    } else {
        0.
    };
    [recipe.width, recipe.depth][axis] * 0.5
        - recipe.wall_thickness
        - projection
        - sign * layout.point(edge[0], edge[1], 0.)[axis]
}

fn flight(result: &mut Assembly, layout: &StaircaseLayout, base: f32, returning: bool) {
    let across = if returning {
        layout.width + FLIGHT_GAP
    } else {
        0.
    };
    let rise = f32::from(layout.risers_per_flight) * layout.riser;
    let bottom = base + if returning { rise } else { 0. };
    let along = |distance: f32| {
        layout.landing_depth
            + if returning {
                layout.run() - distance
            } else {
                distance
            }
    };
    let top = |distance: f32| {
        bottom
            + (distance / layout.going + 1.).min(f32::from(layout.risers_per_flight)) * layout.riser
            + STRINGER_LIP
    };
    for i in 0..layout.risers_per_flight {
        let distance = f32::from(i) * layout.going;
        let ends = [along(distance), along(distance + layout.going)];
        let start = ends[0].min(ends[1]);
        let end = ends[0].max(ends[1]);
        let z = bottom + f32::from(i + 1) * layout.riser;
        let tread = layout.rectangle(
            [across + STRINGER_WIDTH, start, z - TREAD_THICKNESS],
            [across + layout.width - STRINGER_WIDTH, end, z],
        );
        result.treads.push(tread);
        result.colliders.push(tread);
        result.supports.push(tread);
        for u in [
            across + STRINGER_WIDTH * 0.5,
            across + layout.width - STRINGER_WIDTH * 0.5,
        ] {
            let seat = top(distance + layout.going * 0.5);
            result.colliders.push(layout.rectangle(
                [
                    u - STRINGER_WIDTH * 0.5,
                    start,
                    z - TREAD_THICKNESS - layout.riser,
                ],
                [u + STRINGER_WIDTH * 0.5, end, seat + RAIL_HEIGHT],
            ));
            if i % 2 == 0 {
                let v = along(distance + layout.going * 0.5);
                result.solid(
                    layout.rectangle(
                        [u - 0.0175, v - 0.0175, seat],
                        [u + 0.0175, v + 0.0175, seat + RAIL_HEIGHT],
                    ),
                    false,
                );
            }
        }
    }
    flight_sides(result, layout, across, returning, along, top);
}

fn flight_sides(
    result: &mut Assembly,
    layout: &StaircaseLayout,
    across: f32,
    returning: bool,
    along: impl Fn(f32) -> f32,
    top: impl Fn(f32) -> f32,
) {
    let shoulder = layout.run() - layout.going;
    let depth = layout.riser + TREAD_THICKNESS + STRINGER_LIP;
    for u in [across, across + layout.width - STRINGER_WIDTH] {
        let mut profile = [
            [along(0.), top(0.) - depth],
            [along(layout.run()), top(layout.run()) - depth],
            [along(layout.run()), top(layout.run())],
            [along(shoulder), top(shoulder)],
            [along(0.), top(0.)],
        ];
        if returning {
            profile.reverse();
        }
        result.stringers.push(Stringer {
            across: [u, u + STRINGER_WIDTH],
            profile,
        });
        for distances in [[0., shoulder], [shoulder, layout.run()]] {
            result.rails.push(Rail {
                across: [u + STRINGER_WIDTH * 0.5; 2],
                along: distances.map(&along),
                height: distances.map(|v| top(v) + RAIL_HEIGHT),
            });
        }
    }
}

/// The tread ends are housed in the stringers, so their hidden end faces are omitted.
fn tread_mesh(builder: &mut Builder, layout: &StaircaseLayout, tread: BoxCollider) {
    let [a, b, c] = tread.min;
    let [d, e, f] = tread.max;
    builder.face(
        MaterialSlot::Timber,
        [[a, b, f], [d, b, f], [d, e, f], [a, e, f]],
    );
    builder.face(
        MaterialSlot::Timber,
        [[a, e, c], [d, e, c], [d, b, c], [a, b, c]],
    );
    let sides = if layout.axis == crate::StairAxis::X {
        [
            [[d, b, c], [d, e, c], [d, e, f], [d, b, f]],
            [[a, e, c], [a, b, c], [a, b, f], [a, e, f]],
        ]
    } else {
        [
            [[a, b, c], [d, b, c], [d, b, f], [a, b, f]],
            [[d, e, c], [a, e, c], [a, e, f], [d, e, f]],
        ]
    };
    for face in sides {
        builder.face(MaterialSlot::Timber, face);
    }
}

fn stringer_mesh(builder: &mut Builder, layout: &StaircaseLayout, stringer: &Stringer) {
    let point = |u: f32, [v, z]: [f32; 2]| layout.point(u, v, z);
    let [left, right] = stringer.across;
    // A convex five-sided profile keeps the final tread housed beneath a flat landing end.
    for i in 1..4 {
        for (u, indices) in [(left, [0, i + 1, i]), (right, [0, i, i + 1])] {
            let profile = indices.map(|index| stringer.profile[index]);
            builder.triangle(
                MaterialSlot::Timber,
                profile.map(|p| point(u, p)),
                profile.map(|[v, z]| [-z * 0.5, v * 0.5]),
            );
        }
    }
    for i in 0..5 {
        let a = stringer.profile[i];
        let b = stringer.profile[(i + 1) % 5];
        builder.face(
            MaterialSlot::Timber,
            [
                point(left, a),
                point(left, b),
                point(right, b),
                point(right, a),
            ],
        );
    }
}

fn guard(
    result: &mut Assembly,
    layout: &StaircaseLayout,
    start: [f32; 2],
    end: [f32; 2],
    height: f32,
) {
    let width = 0.035;
    let inset = |value: f32, max: f32| value.clamp(width * 0.5, max - width * 0.5);
    let a = [
        inset(start[0], layout.span()),
        inset(start[1], layout.length()),
    ];
    let b = [inset(end[0], layout.span()), inset(end[1], layout.length())];
    result.rails.push(Rail {
        across: [a[0], b[0]],
        along: [a[1], b[1]],
        height: [height + STRINGER_LIP + RAIL_HEIGHT; 2],
    });
    result.colliders.push(layout.rectangle(
        [
            a[0].min(b[0]) - width * 0.5,
            a[1].min(b[1]) - width * 0.5,
            height,
        ],
        [
            a[0].max(b[0]) + width * 0.5,
            a[1].max(b[1]) + width * 0.5,
            height + STRINGER_LIP + RAIL_HEIGHT,
        ],
    ));
    let length = (a[0] - b[0]).hypot(a[1] - b[1]);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let posts = (length / 0.48).ceil().max(1.) as u16;
    for i in 0..=posts {
        let t = f32::from(i) / f32::from(posts);
        let u = a[0] + (b[0] - a[0]) * t;
        let v = a[1] + (b[1] - a[1]) * t;
        result.solid(
            layout.rectangle(
                [u - width * 0.5, v - width * 0.5, height],
                [
                    u + width * 0.5,
                    v + width * 0.5,
                    height + STRINGER_LIP + RAIL_HEIGHT,
                ],
            ),
            false,
        );
    }
}

fn rail_mesh(builder: &mut Builder, layout: &StaircaseLayout, rail: &Rail) {
    let [u0, u1] = rail.across;
    let [v0, v1] = rail.along;
    let [z0, z1] = rail.height;
    let length = (u1 - u0).hypot(v1 - v0);
    let du = -(v1 - v0) / length * 0.025;
    let dv = (u1 - u0) / length * 0.025;
    let points = [
        layout.point(u0 - du, v0 - dv, z0 - 0.025),
        layout.point(u1 - du, v1 - dv, z1 - 0.025),
        layout.point(u1 + du, v1 + dv, z1 - 0.025),
        layout.point(u0 + du, v0 + dv, z0 - 0.025),
        layout.point(u0 - du, v0 - dv, z0 + 0.025),
        layout.point(u1 - du, v1 - dv, z1 + 0.025),
        layout.point(u1 + du, v1 + dv, z1 + 0.025),
        layout.point(u0 + du, v0 + dv, z0 + 0.025),
    ];
    for face in [
        [0, 3, 2, 1],
        [4, 5, 6, 7],
        [0, 1, 5, 4],
        [1, 2, 6, 5],
        [2, 3, 7, 6],
        [3, 0, 4, 7],
    ] {
        builder.face(MaterialSlot::Timber, face.map(|i| points[i]));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::StairAxis;

    #[test]
    fn landing_guards_follow_wall_clearance_on_both_axes_and_reversed_flights() {
        for axis in [StairAxis::X, StairAxis::Y] {
            let mut recipe = BuildingRecipe::procedural(20., 20.);
            recipe.storeys = 3;
            recipe.staircase.axis = axis;
            recipe.staircase.position = Some([0.; 2]);
            recipe.chimney = None;
            recipe.walls.style = WallStyle::WattleAndDaub;
            let mut stairs = crate::stairs::resolve(&recipe).unwrap().unwrap();
            for reversed in [false, true] {
                stairs.reversed = reversed;
                for side in [
                    StairwellSide::Left,
                    StairwellSide::Right,
                    StairwellSide::Far,
                ] {
                    for (gap, guarded) in [(0.3, false), (0.6, false), (0.61, true), (1.2, true)] {
                        stairs.position = [0.; 2];
                        let shift = wall_gap(&recipe, &stairs, side) - gap;
                        let normal = side.outward();
                        let origin = stairs.point(0., 0., 0.);
                        let tip = stairs.point(normal[0], normal[1], 0.);
                        stairs.position = std::array::from_fn(|i| (tip[i] - origin[i]) * shift);
                        let plan = stairs
                            .stairwell
                            .as_mut()
                            .unwrap()
                            .enclosure
                            .as_mut()
                            .unwrap();
                        plan.shared_sides = [false; 4];
                        plan.shared_sides[side.index()] = true;
                        for height in [stairs.floor_heights[2], stairs.turn_landings[0].max[2]] {
                            let mut guards = Assembly::default();
                            edge_guards(
                                &mut guards,
                                &recipe,
                                &stairs,
                                height,
                                stairs.landing_depth,
                            );
                            assert_eq!(
                                guards.rails.len(),
                                usize::from(guarded),
                                "{axis:?} {reversed} {side:?} {gap}"
                            );
                            assert_eq!(guards.colliders.is_empty(), !guarded);
                        }
                    }
                }
            }
            // The same wall rule applies when the enclosure is disabled.
            stairs.stairwell = None;
            assert!(wall_gap(&recipe, &stairs, StairwellSide::Far) > WALL_GUARD_CLEARANCE);
            let mut guards = Assembly::default();
            edge_guards(&mut guards, &recipe, &stairs, 3., stairs.landing_depth);
            assert_eq!(guards.rails.len(), 3);
        }
    }
}
