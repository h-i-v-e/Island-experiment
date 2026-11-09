//! Solid structural geometry. Every opening is removed from both render and collision surfaces.
// Geometric corner and axis names follow the local mathematical notation.
#![allow(clippy::many_single_char_names)]
use crate::{
    BuildingLod, BuildingMesh, BuildingRecipe, DoorDescriptor, Facade, HingeSide, MaterialSlot,
    MeshPart, OpeningKind, PartitionAxis, RoofKind, collision::BoxCollider,
};
mod hardware;

use std::collections::BTreeMap;

const DOOR_THICKNESS: f32 = 0.055;
const DOOR_EDGE_CLEARANCE: f32 = 0.003;
const DOOR_FLOOR_CLEARANCE: f32 = 0.006;
const DOOR_JAMB_CLEARANCE: f32 = 0.001;

#[derive(Default)]
pub(crate) struct Builder {
    parts: BTreeMap<MaterialSlot, BuildingMesh>,
}
impl Builder {
    pub(crate) fn mesh(&mut self, slot: MaterialSlot) -> &mut BuildingMesh {
        self.parts.entry(slot).or_default()
    }
    pub(crate) fn quad(&mut self, slot: MaterialSlot, p: [[f32; 3]; 4], uv: [[f32; 2]; 4]) {
        quad(self.mesh(slot), p, uv);
    }
    pub(crate) fn face(&mut self, slot: MaterialSlot, p: [[f32; 3]; 4]) {
        let width = distance(p[0], p[1]);
        let height = distance(p[1], p[2]);
        self.quad(
            slot,
            p,
            [
                [0., 0.],
                [width * 0.5, 0.],
                [width * 0.5, height * 0.5],
                [0., height * 0.5],
            ],
        );
    }
    pub(crate) fn triangle(&mut self, slot: MaterialSlot, p: [[f32; 3]; 3], uv: [[f32; 2]; 3]) {
        let mesh = self.mesh(slot);
        let start = u32::try_from(mesh.positions.len()).expect("validated mesh budget fits u32");
        let n = normal(p[0], p[1], p[2]);
        mesh.positions.extend(p);
        mesh.normals.extend([n; 3]);
        mesh.uvs.extend(uv);
        mesh.indices.extend([start, start + 1, start + 2]);
    }
    pub(crate) fn cube(&mut self, slot: MaterialSlot, min: [f32; 3], max: [f32; 3]) {
        let [a, b, c] = min;
        let [d, e, f] = max;
        for p in [
            [[a, b, c], [d, b, c], [d, b, f], [a, b, f]],
            [[d, e, c], [a, e, c], [a, e, f], [d, e, f]],
            [[d, b, c], [d, e, c], [d, e, f], [d, b, f]],
            [[a, e, c], [a, b, c], [a, b, f], [a, e, f]],
            [[a, b, f], [d, b, f], [d, e, f], [a, e, f]],
            [[a, e, c], [d, e, c], [d, b, c], [a, b, c]],
        ] {
            self.face(slot, p);
        }
    }
    pub(crate) fn finish(self) -> BuildingLod {
        BuildingLod {
            parts: self
                .parts
                .into_iter()
                .filter(|(_, m)| !m.indices.is_empty())
                .map(|(material, mesh)| MeshPart { material, mesh })
                .collect(),
        }
    }
}
fn distance(a: [f32; 3], b: [f32; 3]) -> f32 {
    (0..3).map(|i| (b[i] - a[i]).powi(2)).sum::<f32>().sqrt()
}
fn normal(a: [f32; 3], b: [f32; 3], c: [f32; 3]) -> [f32; 3] {
    let u = std::array::from_fn::<_, 3, _>(|i| b[i] - a[i]);
    let v = std::array::from_fn::<_, 3, _>(|i| c[i] - a[i]);
    let n = [
        u[1] * v[2] - u[2] * v[1],
        u[2] * v[0] - u[0] * v[2],
        u[0] * v[1] - u[1] * v[0],
    ];
    let len = n.iter().map(|v| v * v).sum::<f32>().sqrt();
    n.map(|v| v / len)
}
pub(crate) fn quad(mesh: &mut BuildingMesh, p: [[f32; 3]; 4], uv: [[f32; 2]; 4]) {
    let start = u32::try_from(mesh.positions.len()).expect("validated mesh budget fits u32");
    let n = normal(p[0], p[1], p[2]);
    mesh.positions.extend(p);
    mesh.normals.extend([n; 4]);
    mesh.uvs.extend(uv);
    mesh.indices
        .extend([start, start + 1, start + 2, start, start + 2, start + 3]);
}

pub(crate) fn generate(r: &BuildingRecipe) -> (BuildingLod, Vec<DoorDescriptor>, Vec<BoxCollider>) {
    let mut b = Builder::default();
    let mut colliders = Vec::new();
    let mut doors = Vec::new();
    slabs(&mut b, r, &mut colliders);
    for facade in Facade::ALL {
        wall(&mut b, r, facade, true, &mut colliders);
    }
    for opening in &r.openings {
        frame(&mut b, r, opening, opening.kind == OpeningKind::Window);
        match opening.kind {
            OpeningKind::Window => window(&mut b, r, opening, &mut colliders),
            OpeningKind::Door => doors.push(door_leaf(r, opening)),
        }
    }
    for p in &r.partitions {
        partition(&mut b, r, p, &mut colliders);
    }
    roof(&mut b, r, true);
    (b.finish(), doors, colliders)
}

fn slabs(b: &mut Builder, r: &BuildingRecipe, colliders: &mut Vec<BoxCollider>) {
    let floor = BoxCollider {
        min: [-r.width * 0.5, -r.depth * 0.5, -r.floor_thickness],
        max: [r.width * 0.5, r.depth * 0.5, 0.],
    };
    b.cube(MaterialSlot::Floor, floor.min, floor.max);
    colliders.push(floor);
    let ceiling = BoxCollider {
        min: [
            -r.width * 0.5 + r.wall_thickness,
            -r.depth * 0.5 + r.wall_thickness,
            r.wall_height - 0.08,
        ],
        max: [
            r.width * 0.5 - r.wall_thickness,
            r.depth * 0.5 - r.wall_thickness,
            r.wall_height,
        ],
    };
    b.cube(MaterialSlot::Interior, ceiling.min, ceiling.max);
    colliders.push(ceiling);
}

fn window(
    b: &mut Builder,
    r: &BuildingRecipe,
    opening: &crate::OpeningRecipe,
    colliders: &mut Vec<BoxCollider>,
) {
    let f = opening.facade;
    let a = opening.offset - opening.width * 0.5;
    let z = opening.sill;
    let p = [
        f.point(r, a, z, r.wall_thickness * 0.48),
        f.point(r, a + opening.width, z, r.wall_thickness * 0.48),
        f.point(
            r,
            a + opening.width,
            z + opening.height,
            r.wall_thickness * 0.48,
        ),
        f.point(r, a, z + opening.height, r.wall_thickness * 0.48),
    ];
    b.face(MaterialSlot::Glass, p);
    let min = std::array::from_fn(|i| p[0][i].min(p[2][i]) - 0.006);
    let max = std::array::from_fn(|i| p[0][i].max(p[2][i]) + 0.006);
    colliders.push(BoxCollider { min, max });
}

fn door_leaf(r: &BuildingRecipe, opening: &crate::OpeningRecipe) -> DoorDescriptor {
    let right = opening.hinge == HingeSide::Right;
    let tangent = opening.facade.tangent();
    let angle = tangent[1].atan2(tangent[0]) + if right { std::f32::consts::PI } else { 0. };
    let hinge = opening.facade.point(
        r,
        opening.offset
            + if right {
                opening.width * 0.5 - DOOR_EDGE_CLEARANCE
            } else {
                -opening.width * 0.5 + DOOR_EDGE_CLEARANCE
            },
        DOOR_FLOOR_CLEARANCE,
        // The outside leaf face seats against the full-depth jamb. Keeping its
        // hinge beyond that plane lets the slab swing inward without clipping it.
        r.wall_thickness + DOOR_THICKNESS * 0.5 + DOOR_JAMB_CLEARANCE,
    );
    let width = opening.width - DOOR_EDGE_CLEARANCE * 2.;
    let height = opening.height - DOOR_EDGE_CLEARANCE - DOOR_FLOOR_CLEARANCE;
    let thickness = DOOR_THICKNESS;
    let mut leaf = Builder::default();
    leaf.cube(
        MaterialSlot::Door,
        [0., -thickness * 0.5, 0.],
        [width, thickness * 0.5, height],
    );
    let mesh = leaf.finish().parts.remove(0).mesh;
    DoorDescriptor {
        id: opening.id.clone(),
        hinge,
        closed_angle: angle,
        swing_angle: opening.swing_degrees.to_radians() * if right { -1. } else { 1. },
        width,
        height,
        thickness,
        mesh,
        hardware_mesh: hardware::door_knobs(width, height, thickness),
        exterior: true,
    }
}

pub(crate) fn wall(
    b: &mut Builder,
    r: &BuildingRecipe,
    f: Facade,
    interior: bool,
    colliders: &mut Vec<BoxCollider>,
) {
    let half = f.length(r) * 0.5;
    let mut xs = vec![-half, half];
    let mut zs = vec![0., r.wall_height];
    for o in r.openings.iter().filter(|o| o.facade == f) {
        xs.extend([o.offset - o.width * 0.5, o.offset + o.width * 0.5]);
        zs.extend([o.sill, o.sill + o.height]);
    }
    xs.sort_by(f32::total_cmp);
    xs.dedup();
    zs.sort_by(f32::total_cmp);
    zs.dedup();
    for x in xs.windows(2) {
        for z in zs.windows(2) {
            let centre = [(x[0] + x[1]) * 0.5, (z[0] + z[1]) * 0.5];
            if r.openings.iter().any(|o| {
                o.facade == f
                    && (centre[0] - o.offset).abs() < o.width * 0.5
                    && centre[1] > o.sill
                    && centre[1] < o.sill + o.height
            }) {
                continue;
            }
            let front = [
                f.point(r, x[0], z[0], 0.),
                f.point(r, x[1], z[0], 0.),
                f.point(r, x[1], z[1], 0.),
                f.point(r, x[0], z[1], 0.),
            ];
            b.quad(
                MaterialSlot::Wall,
                front,
                [
                    [x[0] * 0.5, z[0] * 0.5],
                    [x[1] * 0.5, z[0] * 0.5],
                    [x[1] * 0.5, z[1] * 0.5],
                    [x[0] * 0.5, z[1] * 0.5],
                ],
            );
            if interior {
                let back = [
                    f.point(r, x[1], z[0], r.wall_thickness),
                    f.point(r, x[0], z[0], r.wall_thickness),
                    f.point(r, x[0], z[1], r.wall_thickness),
                    f.point(r, x[1], z[1], r.wall_thickness),
                ];
                b.face(MaterialSlot::Interior, back);
                let p0 = f.point(r, x[0], z[0], 0.);
                let p1 = f.point(r, x[1], z[1], r.wall_thickness);
                let min = std::array::from_fn(|i| p0[i].min(p1[i]));
                let max = std::array::from_fn(|i| p0[i].max(p1[i]));
                colliders.push(BoxCollider { min, max });
            }
        }
    }
    if interior {
        // Door frame boxes below supply complete lining faces. Emitting these
        // reveals too would place duplicate coplanar surfaces inside the jamb.
        for o in r
            .openings
            .iter()
            .filter(|o| o.facade == f && o.kind == OpeningKind::Window)
        {
            let x0 = o.offset - o.width * 0.5;
            let x1 = o.offset + o.width * 0.5;
            let z0 = o.sill;
            let z1 = o.sill + o.height;
            for (a, c) in [
                ([x0, z0], [x0, z1]),
                ([x1, z1], [x1, z0]),
                ([x1, z0], [x0, z0]),
                ([x0, z1], [x1, z1]),
            ] {
                b.face(
                    MaterialSlot::Trim,
                    [
                        f.point(r, c[0], c[1], 0.),
                        f.point(r, a[0], a[1], 0.),
                        f.point(r, a[0], a[1], r.wall_thickness),
                        f.point(r, c[0], c[1], r.wall_thickness),
                    ],
                );
            }
        }
    }
    gable(b, r, f);
}
pub(crate) fn frame(b: &mut Builder, r: &BuildingRecipe, o: &crate::OpeningRecipe, mullions: bool) {
    let f = o.facade;
    let a = o.offset - o.width * 0.5;
    let c = o.offset + o.width * 0.5;
    let z = o.sill;
    let top = z + o.height;
    let mut bars = vec![
        (a - 0.07, a, z - 0.05, top + 0.07),
        (c, c + 0.07, z - 0.05, top + 0.07),
        (a, c, top, top + 0.07),
    ];
    if o.kind == OpeningKind::Window {
        bars.push((a, c, z - 0.07, z));
    }
    if mullions {
        bars.push((o.offset - 0.022, o.offset + 0.022, z, top));
        bars.push((
            a,
            c,
            z + o.height * 0.52 - 0.018,
            z + o.height * 0.52 + 0.018,
        ));
    }
    let lining_depth = if o.kind == OpeningKind::Door {
        r.wall_thickness + DOOR_JAMB_CLEARANCE
    } else {
        0.015
    };
    for (x0, x1, z0, z1) in bars {
        let p0 = f.point(r, x0, z0, -0.045);
        let p1 = f.point(r, x1, z1, lining_depth);
        let min = std::array::from_fn(|i| p0[i].min(p1[i]));
        let max = std::array::from_fn(|i| p0[i].max(p1[i]));
        b.cube(MaterialSlot::Trim, min, max);
    }
}
fn partition(
    b: &mut Builder,
    r: &BuildingRecipe,
    p: &crate::PartitionRecipe,
    colliders: &mut Vec<BoxCollider>,
) {
    let half = match p.axis {
        PartitionAxis::X => r.width * 0.5 - r.wall_thickness,
        PartitionAxis::Y => r.depth * 0.5 - r.wall_thickness,
    };
    for (a, c, z0, z1) in [
        (-half, p.door_offset - p.door_width * 0.5, 0., r.wall_height),
        (p.door_offset + p.door_width * 0.5, half, 0., r.wall_height),
        (
            p.door_offset - p.door_width * 0.5,
            p.door_offset + p.door_width * 0.5,
            p.door_height,
            r.wall_height,
        ),
    ] {
        let (min, max) = match p.axis {
            PartitionAxis::X => (
                [a, p.offset - r.wall_thickness * 0.5, z0],
                [c, p.offset + r.wall_thickness * 0.5, z1],
            ),
            PartitionAxis::Y => (
                [p.offset - r.wall_thickness * 0.5, a, z0],
                [p.offset + r.wall_thickness * 0.5, c, z1],
            ),
        };
        b.cube(MaterialSlot::Interior, min, max);
        colliders.push(BoxCollider { min, max });
    }
}
fn gable(b: &mut Builder, r: &BuildingRecipe, f: Facade) {
    let w = r.width * 0.5;
    let h = r.wall_height;
    match r.roof.kind {
        RoofKind::Gable if matches!(f, Facade::South | Facade::North) => {
            let half = f.length(r) * 0.5;
            b.triangle(
                MaterialSlot::Wall,
                [
                    f.point(r, -half, h, 0.),
                    f.point(r, half, h, 0.),
                    f.point(r, 0., h + r.roof.rise, 0.),
                ],
                [
                    [-half * 0.5, h * 0.5],
                    [half * 0.5, h * 0.5],
                    [0., (h + r.roof.rise) * 0.5],
                ],
            );
        }
        RoofKind::Shed => {
            let roofheight = |x: f32| h + (x / w + 1.) * 0.5 * r.roof.rise;
            let len = f.length(r) * 0.5;
            let p0 = f.point(r, -len, h, 0.);
            let p1 = f.point(r, len, h, 0.);
            let z0 = roofheight(p0[0]);
            let z1 = roofheight(p1[0]);
            if (z0 - h).abs() < 0.001 && (z1 - h).abs() < 0.001 {
                return;
            }
            if (z0 - h).abs() < 0.001 {
                b.triangle(
                    MaterialSlot::Wall,
                    [p0, p1, [p1[0], p1[1], z1]],
                    [
                        [-len * 0.5, h * 0.5],
                        [len * 0.5, h * 0.5],
                        [len * 0.5, z1 * 0.5],
                    ],
                );
            } else if (z1 - h).abs() < 0.001 {
                b.triangle(
                    MaterialSlot::Wall,
                    [p0, p1, [p0[0], p0[1], z0]],
                    [
                        [-len * 0.5, h * 0.5],
                        [len * 0.5, h * 0.5],
                        [-len * 0.5, z0 * 0.5],
                    ],
                );
            } else {
                b.quad(
                    MaterialSlot::Wall,
                    [p0, p1, [p1[0], p1[1], z1], [p0[0], p0[1], z0]],
                    [
                        [-len * 0.5, h * 0.5],
                        [len * 0.5, h * 0.5],
                        [len * 0.5, z1 * 0.5],
                        [-len * 0.5, z0 * 0.5],
                    ],
                );
            }
        }
        RoofKind::Gable => {}
    }
}
pub(crate) fn roof(b: &mut Builder, r: &BuildingRecipe, detailed: bool) {
    let w = r.width * 0.5 + r.roof.overhang;
    let d = r.depth * 0.5 + r.roof.overhang;
    let h = r.wall_height;
    let rise = r.roof.rise;
    let thickness = r.roof.thickness;
    let spans: Vec<(f32, f32, f32, f32)> = match r.roof.kind {
        RoofKind::Gable => vec![
            (
                -w,
                0.,
                h - rise * r.roof.overhang / (r.width * 0.5),
                h + rise,
            ),
            (
                0.,
                w,
                h + rise,
                h - rise * r.roof.overhang / (r.width * 0.5),
            ),
        ],
        RoofKind::Shed => vec![(
            -w,
            w,
            h - rise * r.roof.overhang / r.width,
            h + rise + rise * r.roof.overhang / r.width,
        )],
    };
    for (x0, x1, z0, z1) in spans {
        b.face(
            MaterialSlot::Roof,
            [
                [x0, -d, z0 + thickness],
                [x1, -d, z1 + thickness],
                [x1, d, z1 + thickness],
                [x0, d, z0 + thickness],
            ],
        );
        if detailed {
            b.face(
                MaterialSlot::Interior,
                [[x0, d, z0], [x1, d, z1], [x1, -d, z1], [x0, -d, z0]],
            );
        }
        for y in [-d, d] {
            let p = if y < 0. {
                [
                    [x0, y, z0],
                    [x1, y, z1],
                    [x1, y, z1 + thickness],
                    [x0, y, z0 + thickness],
                ]
            } else {
                [
                    [x1, y, z1],
                    [x0, y, z0],
                    [x0, y, z0 + thickness],
                    [x1, y, z1 + thickness],
                ]
            };
            b.face(MaterialSlot::Trim, p);
        }
        for (x, z, flip) in [(x0, z0, true), (x1, z1, false)] {
            let mut p = [
                [x, -d, z],
                [x, d, z],
                [x, d, z + thickness],
                [x, -d, z + thickness],
            ];
            if flip {
                p.reverse();
            }
            b.face(MaterialSlot::Trim, p);
        }
        if detailed {
            roof_seams(b, x0, x1, z0, z1, thickness, d);
        }
    }
}

// Validated roofs are <=43 metres wide, so this positive count is below 124.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn roof_seams(b: &mut Builder, x0: f32, x1: f32, z0: f32, z1: f32, thickness: f32, depth: f32) {
    let count = ((x1 - x0) / 0.35).ceil() as u16;
    for i in 1..count {
        let t = f32::from(i) / f32::from(count);
        let x = x0 + (x1 - x0) * t;
        let z = z0 + (z1 - z0) * t + thickness;
        b.cube(
            MaterialSlot::Roof,
            [x - 0.012, -depth, z - 0.006],
            [x + 0.012, depth, z + 0.018],
        );
    }
}
