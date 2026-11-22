use super::{
    atlas, foundations,
    plan::{Plan, Wall},
    windows,
};
use crate::{ChurchTowerStyle, MaterialSlot, collision::BoxCollider, geometry::Builder};

pub(super) const CEILING_DROP: f32 = 0.15;

pub(super) fn nave_ceiling_height(p: &Plan<'_>, x: f32) -> f32 {
    p.r.wall_height + p.r.roof.rise * (1. - x.abs() / p.n) - CEILING_DROP
}

pub(super) fn generate(
    b: &mut Builder,
    p: &Plan<'_>,
    level: u8,
    colliders: &mut Vec<BoxCollider>,
    supports: &mut Vec<BoxCollider>,
) {
    slabs(b, p, level, colliders, supports);
    let bearing = foundations::floor_bearing_height(p.r);
    for (index, wall) in p.walls().enumerate() {
        floor_edge(b, wall, bearing, level);
        if index == 0 && p.c.tower_style == ChurchTowerStyle::SingleFront {
            windows::bay(b, p, wall, level, true, colliders);
        } else if index == 0 {
            let q = p.c.tower_width;
            for (a, c, portal) in [
                (0., q, false),
                (q, q + p.c.nave_width, true),
                (q + p.c.nave_width, wall.length(), false),
            ] {
                let bay = wall.slice(a, c);
                if portal {
                    windows::bay(b, p, bay, level, true, colliders);
                } else {
                    bays(b, p, bay, level, colliders);
                }
            }
        } else {
            bays(b, p, wall, level, colliders);
        }
    }
    if level == 0 {
        colliders.extend(p.concave_corners());
    }
    roofs(b, p, level);
    super::eaves::generate(b, p, level);
    crate::geometry::foundation::generate_stairs(b, p.r, level == 2);
    // Only entrance steps use the shared terrain-aware stair recipe. Church
    // foundations follow the cruciform floors and each structural buttress pier.
}

fn floor_edge(b: &mut Builder, wall: Wall, bearing: f32, level: u8) {
    let len = wall.length();
    let points = [
        wall.point(0., bearing, 0.),
        wall.point(len, bearing, 0.),
        wall.point(len, 0., 0.),
        wall.point(0., 0., 0.),
    ];
    if level == 2 {
        atlas::face(b, 0, points);
    } else {
        windows::surface(b, MaterialSlot::Wall, points);
    }
}

pub(super) fn bays(
    b: &mut Builder,
    p: &Plan<'_>,
    wall: Wall,
    level: u8,
    colliders: &mut Vec<BoxCollider>,
) {
    let len = wall.length();
    let minimum_bay = p.window.width + 2. * p.r.wall_thickness + 0.1;
    if len < minimum_bay {
        solid_wall(b, p, wall, level, colliders);
        return;
    }
    let spacing = p.c.bay_spacing.max(minimum_bay);
    let count = ((len / spacing).floor().max(1.) as u16).min(16);
    for i in 0..count {
        windows::bay(
            b,
            p,
            wall.slice(
                len * f32::from(i) / f32::from(count),
                len * f32::from(i + 1) / f32::from(count),
            ),
            level,
            false,
            colliders,
        );
    }
}

pub(super) fn solid_wall(
    b: &mut Builder,
    p: &Plan<'_>,
    wall: Wall,
    level: u8,
    colliders: &mut Vec<BoxCollider>,
) {
    let len = wall.length();
    if level == 2 {
        atlas::face(
            b,
            0,
            [
                wall.point(0., 0., 0.),
                wall.point(len, 0., 0.),
                wall.point(len, p.height, 0.),
                wall.point(0., p.height, 0.),
            ],
        );
    } else {
        windows::solid(b, p, wall, [0., len], [0.; 2], p.height, level, colliders);
    }
}

fn slabs(
    b: &mut Builder,
    p: &Plan<'_>,
    level: u8,
    colliders: &mut Vec<BoxCollider>,
    supports: &mut Vec<BoxCollider>,
) {
    let r = p.r;
    let buried = (r.width + 2. * r.foundation.outset).hypot(r.depth + 2. * r.foundation.outset)
        * 30_f32.to_radians().tan()
        + r.foundation.burial_depth;
    let floors: Vec<_> = p
        .floors()
        .map(|rect| rect.bounds(-r.floor_thickness, 0.))
        .collect();
    let foundations: Vec<_> = super::foundation_footprints(r)
        .into_iter()
        .map(|mut bounds| {
            bounds.min[2] = -buried;
            bounds
        })
        .collect();
    if level < 2 {
        crate::geometry::slabs::joined_boxes(b, MaterialSlot::Foundation, &foundations);
    } else {
        for foundation in &foundations {
            atlas::cube(b, 6, foundation.min, foundation.max);
        }
    }
    if level == 0 {
        crate::geometry::slabs::joined_boxes(b, MaterialSlot::Floor, &floors);
        // The exterior shell owns the visible slab edges at every LOD.
        let floor = b.mesh(MaterialSlot::Floor);
        floor
            .indices
            .retain(|&index| floor.normals[index as usize][2].abs() > 0.5);
        colliders.extend(foundations);
        colliders.extend_from_slice(&floors);
        supports.extend(floors);
    }
}

fn roofs(b: &mut Builder, p: &Plan<'_>, level: u8) {
    let z = p.r.wall_height;
    let peak = z + p.r.roof.rise;
    let y0 = p.y - p.t;
    let y1 = p.y + p.t;
    for (a, c) in [(p.nave_front(), y0), (y1, p.d)] {
        roof(
            b,
            level,
            [[-p.n, a, z], [0., a, peak], [0., c, peak], [-p.n, c, z]],
        );
        roof(
            b,
            level,
            [[0., a, peak], [p.n, a, z], [p.n, c, z], [0., c, peak]],
        );
    }
    for (a, c) in [(-p.w, -p.n), (p.n, p.w)] {
        roof(
            b,
            level,
            [[a, y0, z], [c, y0, z], [c, p.y, peak], [a, p.y, peak]],
        );
        roof(
            b,
            level,
            [[a, p.y, peak], [c, p.y, peak], [c, y1, z], [a, y1, z]],
        );
    }
    // The crossing uses the upper envelope of the two gables, with diagonal
    // valleys and uninterrupted ridges, rather than intersecting roof sheets.
    for sx in [-1., 1.] {
        for sy in [-1., 1.] {
            let centre = [0., p.y, peak];
            let a = [sx * p.n, p.y, peak];
            let c = [sx * p.n, p.y + sy * p.t, z];
            let d = [0., p.y + sy * p.t, peak];
            for mut tri in [[centre, a, c], [centre, c, d]] {
                if sx * sy < 0. {
                    tri.swap(1, 2);
                }
                roof_triangle(b, level, tri);
            }
        }
    }
    for (index, wall) in [
        Wall {
            a: [-p.n, p.nave_front()],
            b: [p.n, p.nave_front()],
        },
        Wall {
            a: [p.n, p.d],
            b: [-p.n, p.d],
        },
        Wall {
            a: [p.w, y0],
            b: [p.w, y1],
        },
        Wall {
            a: [-p.w, y1],
            b: [-p.w, y0],
        },
    ]
    .into_iter()
    .enumerate()
    {
        if p.c.tower_style == ChurchTowerStyle::SingleFront && index == 0 {
            front_gable_shoulders(b, p, level);
            continue;
        }
        let a = wall.point(0., z, 0.);
        let c = wall.point(wall.length(), z, 0.);
        let top = wall.point(wall.length() * 0.5, peak, 0.);
        if level == 2 {
            atlas::triangle(b, 0, [a, c, top]);
        } else {
            b.triangle(
                MaterialSlot::Wall,
                [a, c, top],
                [a, c, top].map(|p| {
                    [
                        if (wall.a[0] - wall.b[0]).abs() < 0.001 {
                            p[1] * 0.5
                        } else {
                            p[0] * 0.5
                        },
                        p[2] * 0.5,
                    ]
                }),
            );
            if level == 0 {
                let q = [
                    wall.point(0., z, p.r.wall_thickness),
                    wall.point(wall.length() * 0.5, peak, p.r.wall_thickness),
                    wall.point(wall.length(), z, p.r.wall_thickness),
                ];
                b.triangle(MaterialSlot::Interior, q, [[0., 0.], [0.5, 1.], [1., 0.]]);
            }
        }
    }
}
fn front_gable_shoulders(b: &mut Builder, p: &Plan<'_>, level: u8) {
    let z = p.r.wall_height;
    // The tower's rear wall owns the centre of the south gable.
    // Keep only the two shoulders so coplanar masonry cannot flicker.
    let half = p.c.tower_width * 0.5;
    for side in [-1., 1.] {
        let tip = [side * p.n, p.nave_front(), z];
        let base = [side * half, p.nave_front(), z];
        let top = [
            side * half,
            p.nave_front(),
            nave_ceiling_height(p, half) + CEILING_DROP,
        ];
        let points = if side < 0. {
            [tip, base, top]
        } else {
            [base, tip, top]
        };
        if level == 2 {
            atlas::triangle(b, 0, points);
        } else {
            b.triangle(
                MaterialSlot::Wall,
                points,
                points.map(|v| [v[0] * 0.5, v[2] * 0.5]),
            );
            if level == 0 {
                let mut inner = points.map(|mut v| {
                    v[1] += p.r.wall_thickness;
                    v
                });
                inner.swap(1, 2);
                b.triangle(
                    MaterialSlot::Interior,
                    inner,
                    inner.map(|v| [v[0] * 0.5, v[2] * 0.5]),
                );
            }
        }
    }
}

fn roof(b: &mut Builder, level: u8, points: [[f32; 3]; 4]) {
    if level == 2 {
        atlas::face(b, 1, points);
    } else {
        b.face(MaterialSlot::Roof, points);
        if level == 0 {
            let p = points.map(|mut p| {
                p[2] -= CEILING_DROP;
                p
            });
            b.face(MaterialSlot::Ceiling, [p[1], p[0], p[3], p[2]]);
        }
    }
}
fn roof_triangle(b: &mut Builder, level: u8, p: [[f32; 3]; 3]) {
    if level == 2 {
        atlas::triangle(b, 1, p);
    } else {
        b.triangle(MaterialSlot::Roof, p, p.map(|p| [p[0] * 0.5, p[1] * 0.5]));
        if level == 0 {
            let mut q = p.map(|mut p| {
                p[2] -= CEILING_DROP;
                p
            });
            q.swap(1, 2);
            b.triangle(
                MaterialSlot::Ceiling,
                q,
                q.map(|p| [p[0] * 0.5, p[1] * 0.5]),
            );
        }
    }
}
