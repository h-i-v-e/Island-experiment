//! Paired, independently hinged timber leaves follow the exact masonry arch.
use super::{plan::Wall, windows};
use crate::{
    BuildingMesh, BuildingRecipe, DoorDescriptor, MaterialSlot, OpeningRecipe,
    collision::BoxCollider, geometry::Builder,
};

const SEGMENTS: u16 = 12;
const EDGE_GAP: f32 = 0.006;
const HALF_SEAM: f32 = 0.004;
const FLOOR_GAP: f32 = 0.02;
const HEAD_GAP: f32 = 0.012;
const THICKNESS: f32 = 0.075;

fn profile(opening: &OpeningRecipe) -> [[f32; 2]; 13] {
    let half = opening.width * 0.5;
    std::array::from_fn(|i| {
        let world_x = if i == 0 {
            -half + EDGE_GAP
        } else if i == usize::from(SEGMENTS) {
            -HALF_SEAM
        } else {
            -half + half * i as f32 / f32::from(SEGMENTS)
        };
        let step = opening.width / 24.;
        let cell = ((world_x + half) / step).floor().clamp(0., 23.);
        let x0 = -half + cell * step;
        let t = (world_x - x0) / step;
        let full_height = opening.height + 3_f32.sqrt() * half;
        let z0 = windows::arch(x0, half, 0., full_height);
        let z1 = windows::arch(x0 + step, half, 0., full_height);
        [
            world_x + half - EDGE_GAP,
            z0 + (z1 - z0) * t - FLOOR_GAP - HEAD_GAP,
        ]
    })
}

pub(super) fn near(r: &BuildingRecipe) -> Vec<DoorDescriptor> {
    let opening = &r.openings[0];
    let profile = profile(opening);
    let half = opening.width * 0.5;
    [false, true]
        .into_iter()
        .map(|right| {
            let width = profile[usize::from(SEGMENTS)][0];
            let height = profile[usize::from(SEGMENTS)][1];
            let depth_offset = THICKNESS * if right { 0.5 } else { -0.5 };
            let hinge = opening.facade.point(
                r,
                if right {
                    half - EDGE_GAP
                } else {
                    -half + EDGE_GAP
                },
                FLOOR_GAP,
                r.wall_thickness + 0.002,
            );
            let mesh = leaf_mesh(&profile, depth_offset);
            let mut hardware_mesh = crate::geometry::hardware::door_knobs(width, height, THICKNESS);
            for point in &mut hardware_mesh.positions {
                point[1] += depth_offset;
            }
            let collision_sections = profile
                .windows(2)
                .map(|pair| BoxCollider {
                    min: [pair[0][0], depth_offset - THICKNESS * 0.5, 0.],
                    max: [
                        pair[1][0],
                        depth_offset + THICKNESS * 0.5,
                        pair[0][1].min(pair[1][1]),
                    ],
                })
                .collect();
            DoorDescriptor {
                id: format!("{}-{}", opening.id, if right { "right" } else { "left" }),
                hinge,
                closed_angle: if right { std::f32::consts::PI } else { 0. },
                swing_angle: opening.swing_degrees.to_radians() * if right { -1. } else { 1. },
                width,
                height,
                thickness: THICKNESS,
                depth_offset,
                collision_sections,
                mesh,
                hardware_mesh,
                exterior: true,
            }
        })
        .collect()
}

fn leaf_mesh(profile: &[[f32; 2]; 13], depth: f32) -> BuildingMesh {
    let mut b = Builder::default();
    let front = depth - THICKNESS * 0.5;
    let back = depth + THICKNESS * 0.5;
    for pair in profile.windows(2) {
        let [[a, za], [c, zc]] = [pair[0], pair[1]];
        wood_face(
            &mut b,
            [
                [a, front, 0.],
                [c, front, 0.],
                [c, front, zc],
                [a, front, za],
            ],
        );
        wood_face(
            &mut b,
            [[c, back, 0.], [a, back, 0.], [a, back, za], [c, back, zc]],
        );
        b.face(
            MaterialSlot::Door,
            [[a, front, za], [c, front, zc], [c, back, zc], [a, back, za]],
        );
        // A narrow raised wooden border follows the pointed head on both faces.
        for (outer, inner) in [(front - 0.008, front), (back, back + 0.008)] {
            let wall = Wall {
                a: [0., outer],
                b: [1., outer],
            };
            windows::prism(
                &mut b,
                wall,
                [a, c],
                [za - 0.075, zc - 0.075],
                [za, zc],
                [0., inner - outer],
                MaterialSlot::Door,
            );
        }
    }
    let width = profile[12][0];
    b.face(
        MaterialSlot::Door,
        [
            [width, front, 0.],
            [0., front, 0.],
            [0., back, 0.],
            [width, back, 0.],
        ],
    );
    for (i, reverse) in [(0, false), (12, true)] {
        let [x, z] = profile[i];
        let mut face = [[x, back, 0.], [x, front, 0.], [x, front, z], [x, back, z]];
        if reverse {
            face.reverse();
        }
        b.face(MaterialSlot::Door, face);
    }
    for (x, top) in [
        (0.02, profile[0][1] - 0.075),
        (width - 0.095, profile[12][1] - 0.075),
    ] {
        for (lo, hi) in [(front - 0.008, front), (back, back + 0.008)] {
            b.cube(MaterialSlot::Door, [x, lo, 0.08], [x + 0.075, hi, top]);
        }
    }
    for z in [0.05, 0.4, 0.8].map(|f| profile[0][1] * f) {
        for (lo, hi) in [(front - 0.008, front), (back, back + 0.008)] {
            b.cube(
                MaterialSlot::Door,
                [0.095, lo, z],
                [width - 0.095, hi, z + 0.085],
            );
        }
    }
    b.finish().parts.remove(0).mesh
}
fn wood_face(b: &mut Builder, p: [[f32; 3]; 4]) {
    b.quad(MaterialSlot::Door, p, p.map(|p| [p[0] * 0.5, p[2] * 0.5]));
}

pub(super) fn proxy(b: &mut Builder, r: &BuildingRecipe, wall: Wall) {
    let opening = &r.openings[0];
    let profile = profile(opening);
    let mid = wall.length() * 0.5;
    let half = opening.width * 0.5;
    for right in [false, true] {
        for pair in profile.windows(2) {
            let points = [pair[0], pair[1]].map(|[x, z]| {
                let along = mid
                    + if right {
                        half - EDGE_GAP - x
                    } else {
                        -half + EDGE_GAP + x
                    };
                [along, z + FLOOR_GAP]
            });
            let [[a, za], [c, zc]] = points;
            let mut face = [
                wall.point(a, FLOOR_GAP, -0.018),
                wall.point(c, FLOOR_GAP, -0.018),
                wall.point(c, zc, -0.018),
                wall.point(a, za, -0.018),
            ];
            if right {
                face.reverse();
            }
            b.quad(
                MaterialSlot::Door,
                face,
                face.map(|p| [p[0] * 0.5, p[2] * 0.5]),
            );
        }
    }
}
