use super::{
    atlas,
    plan::{Plan, Wall},
};
use crate::{
    MaterialSlot,
    collision::BoxCollider,
    geometry::{Builder, DOOR_FRAME_FLOOR_CLEARANCE},
};

pub(super) fn surface(b: &mut Builder, slot: MaterialSlot, points: [[f32; 3]; 4]) {
    if !matches!(
        slot,
        MaterialSlot::Wall | MaterialSlot::Interior | MaterialSlot::Trim
    ) {
        b.face(slot, points);
        return;
    }
    let u = std::array::from_fn::<_, 3, _>(|i| points[1][i] - points[0][i]);
    let v = std::array::from_fn::<_, 3, _>(|i| points[3][i] - points[0][i]);
    let normal = [
        u[1] * v[2] - u[2] * v[1],
        u[2] * v[0] - u[0] * v[2],
        u[0] * v[1] - u[1] * v[0],
    ];
    let axis = usize::from(normal[0].abs() > normal[1].abs());
    let horizontal = normal[2].abs() > normal[0].abs().max(normal[1].abs());
    let uv = points.map(|p| {
        if horizontal {
            [p[0] * 0.5, p[1] * 0.5]
        } else {
            [p[axis] * 0.5, p[2] * 0.5]
        }
    });
    b.quad(slot, points, uv);
}

pub(super) fn arch(x: f32, half: f32, sill: f32, height: f32) -> f32 {
    sill + height - 3_f32.sqrt() * half
        + (4. * half * half - (x.abs() + half).powi(2)).max(0.).sqrt()
}
pub(super) fn prism(
    b: &mut Builder,
    wall: Wall,
    range: [f32; 2],
    bottom: [f32; 2],
    top: [f32; 2],
    depth: [f32; 2],
    slot: MaterialSlot,
) {
    prism_strip(
        b,
        wall,
        &[[range[0], bottom[0], top[0]], [range[1], bottom[1], top[1]]],
        depth,
        slot,
    );
}

/// Extrude a continuous (distance, bottom, top) profile. Only the two exposed
/// ends receive caps; adjacent samples share the same cross-section.
pub(super) fn prism_strip(
    b: &mut Builder,
    wall: Wall,
    profile: &[[f32; 3]],
    depth: [f32; 2],
    slot: MaterialSlot,
) {
    for (index, span) in profile.windows(2).enumerate() {
        let face = |inward| {
            [
                wall.point(span[0][0], span[0][1], inward),
                wall.point(span[1][0], span[1][1], inward),
                wall.point(span[1][0], span[1][2], inward),
                wall.point(span[0][0], span[0][2], inward),
            ]
        };
        let p = face(depth[0]);
        let q = face(depth[1]);
        surface(b, slot, p);
        surface(b, slot, [q[1], q[0], q[3], q[2]]);
        for i in [0, 2] {
            let j = i + 1;
            surface(b, slot, [p[j], p[i], q[i], q[j]]);
        }
        if index == 0 {
            surface(b, slot, [p[0], p[3], q[3], q[0]]);
        }
        if index + 2 == profile.len() {
            surface(b, slot, [p[2], p[1], q[1], q[2]]);
        }
    }
}
// The wall span, vertical profile and collision output are independent mesh inputs.
#[allow(clippy::too_many_arguments)]
pub(super) fn solid(
    b: &mut Builder,
    plan: &Plan<'_>,
    wall: Wall,
    range: [f32; 2],
    bottom: [f32; 2],
    top: f32,
    level: u8,
    colliders: &mut Vec<BoxCollider>,
) {
    solid_with_base(
        b,
        plan,
        wall,
        range,
        bottom,
        top,
        level,
        BaseFace::Exposed,
        colliders,
    );
}

#[derive(Clone, Copy)]
enum BaseFace {
    Exposed,
    /// The continuous stone surround owns this aperture boundary.
    Lined,
}

#[allow(clippy::too_many_arguments)]
fn solid_with_base(
    b: &mut Builder,
    plan: &Plan<'_>,
    wall: Wall,
    range: [f32; 2],
    bottom: [f32; 2],
    top: f32,
    level: u8,
    base: BaseFace,
    colliders: &mut Vec<BoxCollider>,
) {
    if range[1] - range[0] < 0.0001 || top <= bottom[0].max(bottom[1]) + 0.0001 {
        return;
    }
    let r = plan.r;
    if level == 0 {
        let [a, c] = range;
        let front = [
            wall.point(a, bottom[0], 0.),
            wall.point(c, bottom[1], 0.),
            wall.point(c, top, 0.),
            wall.point(a, top, 0.),
        ];
        let back = [
            plan.masonry_point(wall, a, bottom[0], r.wall_thickness),
            plan.masonry_point(wall, c, bottom[1], r.wall_thickness),
            plan.masonry_point(wall, c, top, r.wall_thickness),
            plan.masonry_point(wall, a, top, r.wall_thickness),
        ];
        surface(b, MaterialSlot::Wall, front);
        let direction = [wall.b[0] - wall.a[0], wall.b[1] - wall.a[1]];
        let back_width =
            (back[1][0] - back[0][0]) * direction[0] + (back[1][1] - back[0][1]) * direction[1];
        if back_width <= 0.00001 {
            // A valid wide entrance can leave a jamb narrower than the wall
            // thickness. Clip it at the corner miter instead of inverting its
            // interior skin or emitting a zero-area quad.
            let (u, bottom) = if plan.masonry_depth(wall, c) < r.wall_thickness {
                (c, bottom[1])
            } else {
                (a, bottom[0])
            };
            let depth = plan.masonry_depth(wall, u);
            for (_, points) in [
                [front[1], front[0], wall.point(u, bottom, depth)],
                [front[3], front[2], wall.point(u, top, depth)],
            ]
            .into_iter()
            .enumerate()
            .filter(|(index, _)| *index != 0 || matches!(base, BaseFace::Exposed))
            {
                b.triangle(
                    MaterialSlot::Interior,
                    points,
                    points.map(|p| [p[0] * 0.5, p[1] * 0.5]),
                );
            }
        } else {
            surface(
                b,
                MaterialSlot::Interior,
                [back[1], back[0], back[3], back[2]],
            );
            // All vertical ends meet adjacent masonry. Apertures emit only their
            // exposed jambs below, rather than full hidden caps at every division.
            for i in [0, 2]
                .into_iter()
                .filter(|i| *i != 0 || matches!(base, BaseFace::Exposed))
            {
                let j = i + 1;
                surface(
                    b,
                    MaterialSlot::Interior,
                    [front[j], front[i], back[i], back[j]],
                );
            }
        }
        let a = wall.point(range[0], bottom[0].max(bottom[1]), 0.);
        let c = wall.point(range[1], top, r.wall_thickness);
        colliders.push(BoxCollider {
            min: std::array::from_fn(|i| a[i].min(c[i])),
            max: std::array::from_fn(|i| a[i].max(c[i])),
        });
    } else {
        surface(
            b,
            MaterialSlot::Wall,
            [
                wall.point(range[0], bottom[0], 0.),
                wall.point(range[1], bottom[1], 0.),
                wall.point(range[1], top, 0.),
                wall.point(range[0], top, 0.),
            ],
        );
    }
}

#[derive(Clone, Copy)]
struct Aperture {
    wall: Wall,
    size: crate::OpeningDimensions,
    level: u8,
    portal: bool,
}
pub(super) fn bay(
    b: &mut Builder,
    p: &Plan<'_>,
    wall: Wall,
    level: u8,
    portal: bool,
    colliders: &mut Vec<BoxCollider>,
) {
    let r = p.r;
    let size = if portal {
        let d = &r.openings[0];
        crate::OpeningDimensions {
            width: d.width,
            height: d.height + d.width * 0.866_025_4,
            sill: 0.,
        }
    } else {
        p.window
    };
    let aperture = Aperture {
        wall,
        size,
        level,
        portal,
    };
    aperture.masonry(b, p, colliders);
    if level == 2 {
        return;
    }
    if level == 1 && portal {
        super::doors::proxy(b, r, wall);
    }
    aperture.arch_segments(b, p, colliders);
    aperture.joinery(b, p, colliders);
}
impl Aperture {
    fn masonry(&self, b: &mut Builder, p: &Plan<'_>, colliders: &mut Vec<BoxCollider>) {
        let Self {
            wall,
            size,
            level,
            portal,
        } = *self;
        let len = wall.length();
        let mid = len * 0.5;
        let half = size.width * 0.5;
        if level == 2 {
            atlas::face(
                b,
                if portal { 3 } else { p.window_chart },
                [
                    wall.point(0., 0., 0.),
                    wall.point(len, 0., 0.),
                    wall.point(len, p.height, 0.),
                    wall.point(0., p.height, 0.),
                ],
            );
            return;
        }
        if level == 1 {
            surface(
                b,
                MaterialSlot::Wall,
                [
                    wall.point(0., 0., 0.),
                    wall.point(len, 0., 0.),
                    wall.point(len, p.height, 0.),
                    wall.point(0., p.height, 0.),
                ],
            );
        } else {
            solid(
                b,
                p,
                wall,
                [0., mid - half],
                [0.; 2],
                p.height,
                level,
                colliders,
            );
            solid(
                b,
                p,
                wall,
                [mid + half, len],
                [0.; 2],
                p.height,
                level,
                colliders,
            );
            if size.sill > 0. {
                solid(
                    b,
                    p,
                    wall,
                    [mid - half, mid + half],
                    [0.; 2],
                    size.sill,
                    level,
                    colliders,
                );
            }
            // Jamb linings cover the full opening. Only the short exposed strip
            // below the raised door frame still belongs to the masonry.
            for x in [mid - half, mid + half].into_iter().filter(|_| portal) {
                let depth = p.masonry_depth(wall, x);
                let points = [
                    wall.point(x, size.sill, 0.),
                    wall.point(x, size.sill + DOOR_FRAME_FLOOR_CLEARANCE, 0.),
                    wall.point(x, size.sill + DOOR_FRAME_FLOOR_CLEARANCE, depth),
                    wall.point(x, size.sill, depth),
                ];
                surface(
                    b,
                    MaterialSlot::Interior,
                    if x < mid {
                        [points[1], points[0], points[3], points[2]]
                    } else {
                        points
                    },
                );
            }
        }
    }
    fn arch_segments(&self, b: &mut Builder, p: &Plan<'_>, colliders: &mut Vec<BoxCollider>) {
        let Self {
            wall,
            size,
            level,
            portal,
        } = *self;
        let r = p.r;
        let len = wall.length();
        let mid = len * 0.5;
        let half = size.width * 0.5;
        let segments = if level == 0 { 24_u16 } else { 8 };
        for i in 0..segments {
            let x0 = -half + size.width * f32::from(i) / f32::from(segments);
            let x1 = -half + size.width * f32::from(i + 1) / f32::from(segments);
            let lower = [
                arch(x0, half, size.sill, size.height),
                arch(x1, half, size.sill, size.height),
            ];
            if level == 0 {
                solid_with_base(
                    b,
                    p,
                    wall,
                    [mid + x0, mid + x1],
                    lower,
                    p.height,
                    level,
                    BaseFace::Lined,
                    colliders,
                );
            }
            let floor = size.sill;
            if !portal && lower[0].min(lower[1]) > floor + 0.001 {
                let slot = if level == 0 {
                    MaterialSlot::Glass
                } else {
                    MaterialSlot::WindowProxy
                };
                let depth = if level == 0 {
                    r.wall_thickness * 0.48
                } else {
                    -0.012
                };
                let points = [
                    wall.point(mid + x0, floor, depth),
                    wall.point(mid + x1, floor, depth),
                    wall.point(mid + x1, lower[1], depth),
                    wall.point(mid + x0, lower[0], depth),
                ];
                b.face(slot, points);
            }
            let thick = 0.16;
            let outer_half = half + thick;
            let outer = [
                arch(
                    x0 * outer_half / half,
                    outer_half,
                    size.sill,
                    size.height + thick * 3_f32.sqrt(),
                ),
                arch(
                    x1 * outer_half / half,
                    outer_half,
                    size.sill,
                    size.height + thick * 3_f32.sqrt(),
                ),
            ];
            // Ring segments share their exact inner arch edges; no overlapping wedges.
            ring(
                b,
                wall,
                [mid + x0, mid + x1],
                lower,
                [mid + x0 * outer_half / half, mid + x1 * outer_half / half],
                outer,
                [-0.10, r.wall_thickness + 0.04],
            );
        }
    }
    fn joinery(&self, b: &mut Builder, p: &Plan<'_>, colliders: &mut Vec<BoxCollider>) {
        let Self {
            wall,
            size,
            level,
            portal,
        } = *self;
        let r = p.r;
        let len = wall.length();
        let mid = len * 0.5;
        let half = size.width * 0.5;
        let spring = size.sill + size.height - 3_f32.sqrt() * half;
        for x in [mid - half - 0.16, mid + half] {
            prism(
                b,
                wall,
                [x, x + 0.16],
                [size.sill
                    + if portal {
                        DOOR_FRAME_FLOOR_CLEARANCE
                    } else {
                        -0.14
                    }; 2],
                [spring; 2],
                [-0.10, r.wall_thickness + 0.04],
                MaterialSlot::Trim,
            );
        }
        if !portal {
            prism(
                b,
                wall,
                [mid - half - 0.22, mid + half + 0.22],
                [size.sill - 0.18; 2],
                [size.sill - 0.03; 2],
                [-0.24, r.wall_thickness + 0.03],
                MaterialSlot::Trim,
            );
            let depth = if level == 0 {
                [r.wall_thickness * 0.26, r.wall_thickness * 0.72]
            } else {
                [-0.04, -0.015]
            };
            super::tracery::generate(b, wall, size, depth, level);
            if level == 0 {
                let a = wall.point(mid - half, size.sill, r.wall_thickness * 0.48 - 0.015);
                let c = wall.point(mid + half, spring, r.wall_thickness * 0.48 + 0.015);
                colliders.push(BoxCollider {
                    min: std::array::from_fn(|i| a[i].min(c[i])),
                    max: std::array::from_fn(|i| a[i].max(c[i])),
                });
            }
        }
    }
}

fn ring(
    b: &mut Builder,
    w: Wall,
    inner_x: [f32; 2],
    inner_z: [f32; 2],
    outer_x: [f32; 2],
    outer_z: [f32; 2],
    depth: [f32; 2],
) {
    let p = [
        w.point(inner_x[0], inner_z[0], depth[0]),
        w.point(inner_x[1], inner_z[1], depth[0]),
        w.point(outer_x[1], outer_z[1], depth[0]),
        w.point(outer_x[0], outer_z[0], depth[0]),
    ];
    let q = [
        w.point(inner_x[0], inner_z[0], depth[1]),
        w.point(inner_x[1], inner_z[1], depth[1]),
        w.point(outer_x[1], outer_z[1], depth[1]),
        w.point(outer_x[0], outer_z[0], depth[1]),
    ];
    b.face(MaterialSlot::Trim, p);
    b.face(MaterialSlot::Trim, [q[1], q[0], q[3], q[2]]);
    b.face(MaterialSlot::Trim, [p[1], p[0], q[0], q[1]]);
    b.face(MaterialSlot::Trim, [p[3], p[2], q[2], q[3]]);
}

#[cfg(test)]
mod opening_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::BuildingRecipe;
    use std::collections::BTreeMap;

    #[test]
    fn continuous_prism_has_closed_ends_and_no_internal_caps() {
        let wall = Wall {
            a: [0., 0.],
            b: [3., 0.],
        };
        let profile = [[0., 0., 2.], [1., 0.4, 2.6], [2., 0.4, 2.6], [3., 0., 2.]];
        let mut builder = Builder::default();
        prism_strip(
            &mut builder,
            wall,
            &profile,
            [-0.13, 0.13],
            MaterialSlot::Trim,
        );
        let mesh = builder.finish().parts.remove(0).mesh;
        assert_eq!(mesh.indices.len() / 3, 28);
        // Every geometric triangle edge belongs to two faces, including the
        // seams where the curved arch profile changes direction.
        let mut edges = BTreeMap::new();
        for ids in mesh.indices.chunks_exact(3) {
            let points =
                [ids[0], ids[1], ids[2]].map(|i| mesh.positions[i as usize].map(f32::to_bits));
            for (a, b) in [(0, 1), (1, 2), (2, 0)] {
                let edge = if points[a] < points[b] {
                    [points[a], points[b]]
                } else {
                    [points[b], points[a]]
                };
                *edges.entry(edge).or_insert(0) += 1;
            }
            let xs = [ids[0], ids[1], ids[2]].map(|i| mesh.positions[i as usize][0]);
            assert!(!xs.iter().all(|x| (*x - 1.).abs() < 0.0001));
            assert!(!xs.iter().all(|x| (*x - 2.).abs() < 0.0001));
        }
        assert!(edges.values().all(|count| *count == 2));
    }

    #[test]
    fn wall_join_uses_only_exterior_wall_material_and_shared_interior_miters() {
        let recipe = BuildingRecipe::gothic_church();
        let plan = Plan::new(&recipe);
        let wall = plan.walls().nth(8).unwrap();
        let mut builder = Builder::default();
        solid(
            &mut builder,
            &plan,
            wall,
            [0., wall.length()],
            [0.; 2],
            plan.height,
            0,
            &mut Vec::new(),
        );
        let lod = builder.finish();
        let interior = &lod
            .parts
            .iter()
            .find(|p| p.material == MaterialSlot::Interior)
            .unwrap()
            .mesh;
        let y = wall.a[1];
        assert!(
            interior
                .positions
                .iter()
                .zip(&interior.normals)
                .all(|(p, n)| { (p[1] - y).abs() > 0.0001 || n[1].abs() < 0.99 }),
            "a neighbouring end cap must never cover the exterior wall plane"
        );
        let inside = y - recipe.wall_thickness;
        let skin: Vec<_> = interior
            .positions
            .iter()
            .zip(&interior.normals)
            .filter(|(_, n)| n[1].abs() > 0.99)
            .map(|(p, _)| p)
            .collect();
        assert_eq!(skin.len(), 4);
        assert!(skin.iter().all(|p| (p[1] - inside).abs() < 0.0001));
        assert!(
            skin.iter()
                .all(|p| p[0].abs() <= plan.n - recipe.wall_thickness + 0.0001)
        );
        assert_eq!(lod.triangle_count(), 8);
    }

    #[test]
    fn narrow_corner_masonry_clips_to_a_triangle_without_inverting_its_back() {
        let recipe = BuildingRecipe::gothic_church();
        let plan = Plan::new(&recipe);
        let wall = plan.walls().next().unwrap();
        let mut builder = Builder::default();
        solid(
            &mut builder,
            &plan,
            wall,
            [0., 0.3],
            [0.; 2],
            plan.height,
            0,
            &mut Vec::new(),
        );
        let lod = builder.finish();
        assert_eq!(lod.triangle_count(), 4);
        for part in lod.parts {
            assert!(part.mesh.normals.iter().flatten().all(|v| v.is_finite()));
            assert!(
                part.mesh
                    .positions
                    .iter()
                    .all(|p| { p[0] <= wall.a[0] + 0.3001 && p[1] <= wall.a[1] + 0.3001 })
            );
        }
    }
}
