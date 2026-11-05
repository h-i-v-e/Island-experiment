//! Open, pinnate palm crowns with real leaflet gaps, shared by both engines.
#![allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]

use std::f32::consts::{PI, TAU};

use crate::{ISLAND_WORLD_METRES, Mesh, TreeMeshes, Vec2, Vec3};

const FRONDS: usize = 14;

#[derive(Clone, Copy)]
struct PalmShape {
    height: f32,
    phase: f32,
    lean: Vec3,
}

impl PalmShape {
    fn new(variant: usize) -> Self {
        let variant = (variant % 8) as f32;
        let phase = variant * 2.399_963;
        Self {
            height: 7.2 + variant * 0.31,
            phase,
            lean: Vec3::new(phase.cos(), phase.sin(), 0.0) * (0.48 + variant * 0.09),
        }
    }

    fn trunk_centre(self, t: f32) -> Vec3 {
        let lateral = Vec3::Z.cross(self.lean).normalize();
        self.lean * t * t + lateral * (PI * t).sin() * 0.12 + Vec3::Z * self.height * t
    }

    fn trunk_radius(t: f32) -> f32 {
        0.19 - t * 0.065 + 0.065 * (-t * 22.0).exp()
    }

    fn frond(self, index: usize) -> Frond {
        let phase = self.phase + index as f32 * 2.399_963;
        let age = index as f32 / (FRONDS - 1) as f32;
        Frond {
            origin: self.trunk_centre(1.0) + Vec3::Z * (0.38 - age * 0.42),
            radial: Vec3::new(phase.cos(), phase.sin(), 0.0),
            length: 2.25 + age * 1.25 + 0.13 * phase.sin(),
            rise: 1.55 - age * 0.85,
            droop: -0.8 + age * 2.65,
            phase,
        }
    }
}

#[derive(Clone, Copy)]
struct Frond {
    origin: Vec3,
    radial: Vec3,
    length: f32,
    rise: f32,
    droop: f32,
    phase: f32,
}

impl Frond {
    fn point(self, t: f32) -> Vec3 {
        self.origin
            + self.radial * self.length * t
            + Vec3::Z * (self.rise * (PI * t).sin() - self.droop * t * t)
            + self.side() * (PI * t).sin() * 0.10 * self.phase.sin()
    }

    fn side(self) -> Vec3 {
        Vec3::Z.cross(self.radial)
    }

    fn sampled_point(self, t: f32, sections: usize) -> Vec3 {
        let section = (t * sections as f32).floor();
        let a = section / sections as f32;
        let b = ((section + 1.0) / sections as f32).min(1.0);
        self.point(a).lerp(self.point(b), (t - a) / (b - a))
    }

    fn leaflet_length(self, t: f32) -> f32 {
        0.88 * (PI * t).sin().max(0.0).powf(0.65) * (0.94 + 0.06 * self.phase.cos())
    }
}

/// Fine palms use modeled leaflets; middle/far share an arched cutout chart.
#[must_use]
pub fn generate_palm(variant: usize) -> TreeMeshes {
    let shape = PalmShape::new(variant);
    TreeMeshes {
        lod0_wood: trunk(shape, 47, 9, true),
        lod1_wood: trunk(shape, 14, 6, false),
        // Four base and four top vertices are also the distant collider contract.
        lod2_wood: trunk(shape, 2, 4, false),
        lod0_foliage: crown(shape, 18, 8),
        lod1_foliage: card_crown(shape, 6, false),
        lod2_foliage: card_crown(shape, 3, true),
        ..TreeMeshes::default()
    }
}

fn trunk(shape: PalmShape, rings: usize, sides: usize, scars: bool) -> Mesh {
    let mut mesh = Mesh::default();
    for ring in 0..rings {
        let t = ring as f32 / (rings - 1) as f32;
        let centre = shape.trunk_centre(t);
        let axis = (shape.trunk_centre((t + 0.01).min(1.0))
            - shape.trunk_centre((t - 0.01).max(0.0)))
        .normalize();
        let scar = if scars && ring % 2 == 1 { 0.009 } else { 0.0 };
        let radius = PalmShape::trunk_radius(t) + scar;
        for side in 0..sides {
            let angle = TAU * side as f32 / sides as f32 + shape.phase;
            // Horizontal rings keep the entire base rim on the burial plane.
            mesh.vertices.push(
                (centre + Vec3::new(angle.cos(), angle.sin(), 0.0) * radius) / ISLAND_WORLD_METRES,
            );
            mesh.uv.push(super::encode_bark_axis(axis));
        }
    }
    for ring in 0..rings - 1 {
        for side in 0..sides {
            let a = (ring * sides + side) as u32;
            let b = (ring * sides + (side + 1) % sides) as u32;
            let step = sides as u32;
            mesh.triangles
                .extend_from_slice(&[a, b, a + step, b, b + step, a + step]);
        }
    }
    // A cap is hidden inside the crown; the eight-vertex distant mesh stays open.
    if rings > 2 {
        let apex = mesh.vertices.len() as u32;
        mesh.vertices
            .push((shape.trunk_centre(1.0) + Vec3::Z * 0.08) / ISLAND_WORLD_METRES);
        mesh.uv.push(super::encode_bark_axis(Vec3::Z));
        let last = ((rings - 1) * sides) as u32;
        for side in 0..sides as u32 {
            mesh.triangles.extend_from_slice(&[
                last + side,
                last + (side + 1) % sides as u32,
                apex,
            ]);
        }
    }
    mesh.calculate_normals();
    mesh
}

fn crown(shape: PalmShape, pairs: usize, spine_sections: usize) -> Mesh {
    let mut mesh = Mesh::default();
    for index in 0..FRONDS {
        let frond = shape.frond(index);
        append_ribbon(&mut mesh, frond, spine_sections);
        for pair in 0..pairs {
            for side in [-1.0, 1.0] {
                // Stagger the two combs, retaining visible light between leaflets.
                let t = 0.12 + 0.83 * (pair as f32 + 0.28 + side * 0.10) / pairs as f32;
                append_leaflet(&mut mesh, frond, t, side, pairs, spine_sections);
            }
        }
    }
    mesh.calculate_normals();
    mesh
}

fn append_leaflet(
    mesh: &mut Mesh,
    frond: Frond,
    t: f32,
    side: f32,
    pairs: usize,
    spine_sections: usize,
) {
    let root = frond.sampled_point(t, spine_sections);
    let length = frond.leaflet_length(t);
    let direction = frond.side() * side + frond.radial * (0.28 + t * 0.32) - Vec3::Z * 0.30;
    let tip = root + direction * length;
    let across = (tip - root).cross(Vec3::Z).normalize();
    let width = (0.058 * 18.0 / pairs as f32) * (PI * t).sin().sqrt();
    let middle = root.lerp(tip, 0.43) - Vec3::Z * 0.035;
    append_two_sided_quad(
        mesh,
        [root, middle + across * width, tip, middle - across * width],
        [
            Vec2::new(0.5, 0.0),
            Vec2::new(0.0, 0.43),
            Vec2::new(0.5, 1.0),
            Vec2::new(1.0, 0.43),
        ],
    );
}

fn append_ribbon(mesh: &mut Mesh, frond: Frond, sections: usize) {
    for section in 0..sections {
        let start = section as f32 / sections as f32;
        let end = (section + 1) as f32 / sections as f32;
        let half_width = |t: f32| 0.024 * (1.0 - t) + 0.004;
        let start_point = frond.point(start);
        let end_point = frond.point(end);
        let start_width = frond.side() * half_width(start);
        let end_width = frond.side() * half_width(end);
        append_two_sided_quad(
            mesh,
            [
                start_point - start_width,
                start_point + start_width,
                end_point + end_width,
                end_point - end_width,
            ],
            [
                Vec2::new(0.0, start),
                Vec2::new(1.0, start),
                Vec2::new(1.0, end),
                Vec2::new(0.0, end),
            ],
        );
    }
}

fn append_two_sided_quad(mesh: &mut Mesh, points: [Vec3; 4], uv: [Vec2; 4]) {
    let offset = mesh.vertices.len() as u32;
    // Separate back vertices preserve underside normals without cancellation.
    for _ in 0..2 {
        mesh.vertices
            .extend(points.map(|point| point / ISLAND_WORLD_METRES));
        mesh.uv.extend(uv);
    }
    for triangle in [[0, 1, 2], [0, 2, 3], [4, 6, 5], [4, 7, 6]] {
        mesh.triangles.extend(triangle.map(|index| offset + index));
    }
}

fn card_crown(shape: PalmShape, sections: usize, double_sided: bool) -> Mesh {
    let mut mesh = Mesh::default();
    for index in 0..FRONDS {
        let frond = shape.frond(index);
        for half in 0..2 {
            append_card_half(&mut mesh, frond, sections, half, double_sided);
        }
    }
    mesh.calculate_normals();
    mesh
}

fn append_card_half(
    mesh: &mut Mesh,
    frond: Frond,
    sections: usize,
    half: usize,
    double_sided: bool,
) {
    let phase_scale = (0.94 + 0.06 * frond.phase.cos()) / 0.94;
    let first_vertex = mesh.vertices.len() as u32;
    let face_vertices = ((sections + 1) * 2) as u32;
    for back in 0..=usize::from(double_sided) {
        for ring in 0..=sections {
            let v = ring as f32 / sections as f32;
            // This fixed chart is shared with scripts/bake-palm-frond.py.
            // Its transparent margins stay full-width, avoiding UV stretching
            // through the first leaflets and through the pointed alpha tip.
            let centre = frond.point(v * 1.03);
            for edge in 0..2 {
                let lateral = (half + edge) as f32 - 1.0;
                let point = centre + frond.side() * lateral * phase_scale
                    - Vec3::Z * (0.30 * lateral.abs() * phase_scale);
                mesh.vertices.push(point / ISLAND_WORLD_METRES);
                mesh.uv.push(Vec2::new(lateral * 0.5 + 0.5, v));
            }
        }
        for section in 0..sections as u32 {
            let start = first_vertex + back as u32 * face_vertices + section * 2;
            let triangles = if back == 0 {
                [start, start + 3, start + 1, start, start + 2, start + 3]
            } else {
                [start, start + 1, start + 3, start, start + 3, start + 2]
            };
            mesh.triangles.extend(triangles);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn palm_variants_are_deterministic_and_keep_the_distant_collider_layout() {
        for variant in 0..8 {
            let tree = generate_palm(variant);
            assert_eq!(tree, generate_palm(variant));
            assert_eq!(tree.lod2_wood.vertices.len(), 8);
            assert!(tree.lod2_wood.vertices[..4].iter().all(|p| p.z == 0.0));
            assert!(tree.lod0_wood.vertices[..9].iter().all(|p| p.z == 0.0));
            assert!(tree.lod0_twigs.vertices.is_empty());
            assert!(tree.wood_lod1_to_lod0.is_empty());
        }
        assert_ne!(generate_palm(0), generate_palm(1));
    }

    #[test]
    fn palm_meshes_have_finite_normals_valid_triangles_and_reducing_budgets() {
        for variant in 0..8 {
            let tree = generate_palm(variant);
            let mut previous = usize::MAX;
            for (wood, leaves, budget) in [
                (&tree.lod0_wood, &tree.lod0_foliage, 4000),
                (&tree.lod1_wood, &tree.lod1_foliage, 1000),
                (&tree.lod2_wood, &tree.lod2_foliage, 400),
            ] {
                let triangles = (wood.triangles.len() + leaves.triangles.len()) / 3;
                assert!(triangles < previous && triangles <= budget);
                previous = triangles;
                for mesh in [wood, leaves] {
                    assert_eq!(mesh.vertices.len(), mesh.normals.len());
                    assert_eq!(mesh.vertices.len(), mesh.uv.len());
                    assert!(mesh.vertices.iter().all(|p| p.is_finite()));
                    assert!(
                        mesh.normals
                            .iter()
                            .all(|n| n.is_finite() && (n.length() - 1.0).abs() < 1.0e-5)
                    );
                    for triangle in mesh.triangles.chunks_exact(3) {
                        let [a, b, c] = [triangle[0], triangle[1], triangle[2]]
                            .map(|i| mesh.vertices[i as usize] * ISLAND_WORLD_METRES);
                        assert!((b - a).cross(c - a).length_squared() > 1.0e-12);
                    }
                }
            }
        }
    }

    #[test]
    fn all_palm_lods_keep_walking_clearance_and_two_sided_fronds() {
        for variant in 0..8 {
            let tree = generate_palm(variant);
            for crown in [&tree.lod0_foliage, &tree.lod1_foliage, &tree.lod2_foliage] {
                assert!(
                    crown
                        .vertices
                        .iter()
                        .all(|p| p.z * ISLAND_WORLD_METRES * 0.7 - 0.08 >= 2.0)
                );
            }
            for (mesh, face_vertices) in [(&tree.lod0_foliage, 4), (&tree.lod2_foliage, 8)] {
                for faces in mesh.normals.chunks_exact(face_vertices * 2) {
                    for index in 0..face_vertices {
                        assert!(faces[index].dot(faces[index + face_vertices]) < -0.999);
                    }
                }
            }
        }
    }

    #[test]
    fn palm_lods_keep_the_crown_envelope_and_leaflet_gaps() {
        for variant in 0..8 {
            let tree = generate_palm(variant);
            let bounds = |mesh: &Mesh| {
                mesh.vertices.iter().fold(
                    (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY)),
                    |(minimum, maximum), &p| (minimum.min(p), maximum.max(p)),
                )
            };
            let (fine_min, fine_max) = bounds(&tree.lod0_foliage);
            for mesh in [&tree.lod1_foliage, &tree.lod2_foliage] {
                let (minimum, maximum) = bounds(mesh);
                assert!(
                    ((fine_min - minimum) * ISLAND_WORLD_METRES)
                        .abs()
                        .max_element()
                        < 0.7
                );
                assert!(
                    ((fine_max - maximum) * ISLAND_WORLD_METRES)
                        .abs()
                        .max_element()
                        < 0.7
                );
            }
            assert_eq!(tree.lod1_foliage.triangles.len() / 3, FRONDS * 24);
            assert_eq!(tree.lod2_foliage.triangles.len() / 3, FRONDS * 24);
            // Cards use the identical full chart at both stages: no stretching
            // broad leaflets into the much narrower authored fine silhouette.
            for mesh in [&tree.lod1_foliage, &tree.lod2_foliage] {
                assert!(
                    mesh.uv
                        .iter()
                        .all(|uv| (0.0..=1.0).contains(&uv.x) && (0.0..=1.0).contains(&uv.y))
                );
                assert!(mesh.uv.contains(&Vec2::ZERO));
                assert!(mesh.uv.contains(&Vec2::ONE));
                assert!(mesh.uv.contains(&Vec2::new(0.5, 0.0)));
                assert!(mesh.uv.contains(&Vec2::new(0.5, 1.0)));
            }
        }
    }
}
