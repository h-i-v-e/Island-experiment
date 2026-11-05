//! Planet forest silhouettes, shared by both engines.
#![allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
use crate::{Mesh, TreeMeshes, Vec3};
use std::f32::consts::TAU;

// At Planet's smallest 0.7 scale, allow for the 8 cm root burial and
// downward needle fringe while keeping the walking space above two metres.
const CROWN_BASE_METRES: f32 = 3.4;
const FOLIAGE_TIERS: usize = 8;
const FOLIAGE_LOOP_VERTICES: usize = 24;

#[must_use]
pub fn generate_conifer(variant: usize) -> TreeMeshes {
    let height = 13.5 + variant as f32 * 1.3;
    let mut wood = Mesh::default();
    whorl(
        &mut wood,
        Vec3::ZERO,
        0.19,
        height,
        6,
        variant as f32,
        false,
    );
    wood.calculate_normals();
    let [foliage, middle, distant_foliage] = conifer_crowns(height, variant);
    wood.uv = vec![super::encode_bark_axis(Vec3::Z); wood.vertices.len()];
    // The branch LOD keeps the plain trunk; only the closest LOD owns boughs.
    let mut fine_wood = wood.clone();
    let tips = append_upper_branches(&mut fine_wood, variant);
    super::tips::append_pointed_tip(
        &mut fine_wood,
        &[12, 13, 14, 15, 16, 17],
        Vec3::Z,
        height * 0.9 / crate::ISLAND_WORLD_METRES,
    );
    fine_wood.calculate_normals();
    let mut distant = Mesh::default();
    for (z, r) in [(0.0, 0.19), (height, 0.19 * 0.04)] {
        for side in 0..4 {
            let angle = side as f32 * TAU / 4.0;
            distant
                .vertices
                .push(Vec3::new(angle.cos() * r, angle.sin() * r, z) / crate::ISLAND_WORLD_METRES);
        }
    }
    for side in 0..4_u32 {
        let next = (side + 1) % 4;
        distant
            .triangles
            .extend_from_slice(&[side, next, side + 4, next, next + 4, side + 4]);
    }
    distant.uv = vec![super::encode_bark_axis(Vec3::Z); 8];
    distant.calculate_normals();
    TreeMeshes {
        lod0_wood: fine_wood,
        lod0_twigs: super::cards::twig_cards_above(
            tips.map(|tip| [tip]),
            // One millimetre covers float rounding at the two-metre floor.
            ((2.001 + 0.08) / 0.7) / crate::ISLAND_WORLD_METRES,
        ),
        lod1_wood: wood,
        lod2_wood: distant,
        lod0_foliage: foliage,
        lod1_foliage: middle,
        lod2_foliage: distant_foliage,
        ..TreeMeshes::default()
    }
}
fn append_upper_branches(mesh: &mut Mesh, variant: usize) -> [super::BranchTip; 6] {
    let mut tips = [super::BranchTip {
        position: Vec3::ZERO,
        direction: Vec3::Z,
        radius: 0.0,
    }; 6];
    let first_bough = u32::try_from(mesh.vertices.len()).expect("conifer mesh fits u32 indices");
    let mut centrelines = [[Vec3::ZERO; 4]; 6];
    for (branch, centreline) in centrelines.iter_mut().enumerate() {
        let phase = variant as f32 * 0.83 + branch as f32 * 2.399_963;
        let radial = Vec3::new(phase.cos(), phase.sin(), 0.0);
        let root = radial * 0.04 + Vec3::Z * (3.1 + branch as f32 * 0.23);
        let reach = 1.35 + 0.25 * (phase * 1.7).sin();
        let centres = [
            root,
            root + radial * 0.55 + Vec3::Z * 0.60,
            root + radial * reach + Vec3::Z * (1.4 + 0.12 * phase.cos()),
        ];
        centreline[..3].copy_from_slice(&centres.map(|centre| centre / crate::ISLAND_WORLD_METRES));
        let offset = mesh.vertices.len() as u32;
        for (ring, &centre) in centres.iter().enumerate() {
            let previous = centres[ring.saturating_sub(1)];
            let next = centres[(ring + 1).min(2)];
            let axis = (next - previous).normalize();
            let u = axis.cross(Vec3::Z).normalize();
            let v = axis.cross(u);
            let radius = 0.065 * (1.0 - 0.85 * ring as f32 / 2.0);
            for side in 0..4 {
                let angle = side as f32 * TAU / 4.0;
                mesh.vertices.push(
                    (centre + radius * (u * angle.cos() + v * angle.sin()))
                        / crate::ISLAND_WORLD_METRES,
                );
                mesh.uv.push(super::encode_bark_axis(axis));
            }
        }
        for ring in 0..2_u32 {
            for side in 0..4_u32 {
                let a = offset + ring * 4 + side;
                let b = offset + ring * 4 + (side + 1) % 4;
                mesh.triangles
                    .extend_from_slice(&[a, b, a + 4, b, b + 4, a + 4]);
            }
        }
    }
    // Append only after all grown boughs so their existing vertex layout stays
    // stable. Fan positions measure the whole branch, including its point.
    for (branch, (tip, centres)) in tips.iter_mut().zip(&mut centrelines).enumerate() {
        let offset = first_bough + u32::try_from(branch * 12 + 8).expect("bough index fits u32");
        let direction = centres[2] - centres[1];
        let apex = super::tips::append_pointed_tip(
            mesh,
            &[offset, offset + 1, offset + 2, offset + 3],
            direction,
            direction.length(),
        );
        centres[3] = mesh.vertices[apex as usize];
        *tip =
            super::cards::branch_support(centres, 0.065 * 0.15 / crate::ISLAND_WORLD_METRES, 0.5);
    }
    tips
}

/// Author the dense loops once, then remove columns without moving retained
/// vertices. Tier edges and wobble survive every LOD instead of being relaxed.
fn conifer_crowns(height: f32, variant: usize) -> [Mesh; 3] {
    let mut crowns = std::array::from_fn(|_| Mesh::default());
    for i in 0..FOLIAGE_TIERS {
        let t = i as f32 / (FOLIAGE_TIERS - 1) as f32;
        let base = CROWN_BASE_METRES + t * (height - 2.4 - CROWN_BASE_METRES);
        let spread = (3.0 + variant as f32 * 0.12) * (1.0 - t).powf(0.85) + 0.16;
        let mut tier = Mesh::default();
        whorl(
            &mut tier,
            Vec3::Z * base,
            spread,
            (height - base + 0.25).min(3.7),
            FOLIAGE_LOOP_VERTICES,
            variant as f32 + i as f32 * 0.73,
            true,
        );
        tier.calculate_normals();
        let mut columns: Vec<_> = (0..FOLIAGE_LOOP_VERTICES).collect();
        let mut protected = [false; FOLIAGE_LOOP_VERTICES];
        // Protect the horizontal silhouette. Every column already shares the
        // same tip height; pinning one arbitrary tip column clusters far-LOD
        // vertices and leaves overly wide gaps elsewhere around the rim.
        for axis in 0..2 {
            let minimum = tier
                .vertices
                .iter()
                .enumerate()
                .min_by(|(_, a), (_, b)| a[axis].total_cmp(&b[axis]))
                .unwrap()
                .0;
            let maximum = tier
                .vertices
                .iter()
                .enumerate()
                .max_by(|(_, a), (_, b)| a[axis].total_cmp(&b[axis]))
                .unwrap()
                .0;
            protected[minimum % FOLIAGE_LOOP_VERTICES] = true;
            protected[maximum % FOLIAGE_LOOP_VERTICES] = true;
        }
        for (lod, target) in [
            FOLIAGE_LOOP_VERTICES,
            FOLIAGE_LOOP_VERTICES / 2,
            FOLIAGE_LOOP_VERTICES / 4,
        ]
        .into_iter()
        .enumerate()
        {
            cull_columns(&tier, &mut columns, &protected, target);
            append_foliage_tier(&mut crowns[lod], &tier, &columns);
        }
    }
    crowns
}

fn cull_columns(source: &Mesh, columns: &mut Vec<usize>, protected: &[bool], target: usize) {
    while columns.len() > target {
        let removable = (0..columns.len()).filter(|&index| !protected[columns[index]]);
        let remove = removable
            .min_by(|&a, &b| {
                column_error(source, columns, a)
                    .total_cmp(&column_error(source, columns, b))
                    .then_with(|| columns[a].cmp(&columns[b]))
            })
            .expect("at most four silhouette extrema are protected");
        columns.remove(remove);
    }
}

fn column_error(source: &Mesh, columns: &[usize], index: usize) -> f32 {
    let previous = columns[(index + columns.len() - 1) % columns.len()];
    let next = columns[(index + 1) % columns.len()];
    (0..3)
        .map(|ring| {
            let offset = ring * FOLIAGE_LOOP_VERTICES;
            let a = source.vertices[offset + previous];
            let b = source.vertices[offset + next];
            let point = source.vertices[offset + columns[index]];
            let edge = b - a;
            let along = ((point - a).dot(edge) / edge.length_squared()).clamp(0.0, 1.0);
            point.distance_squared(a + edge * along)
        })
        .fold(0.0, f32::max)
}

fn append_foliage_tier(destination: &mut Mesh, source: &Mesh, columns: &[usize]) {
    let offset = u32::try_from(destination.vertices.len()).expect("pine foliage fits u32 indices");
    for ring in 0..3 {
        for &column in columns {
            let index = ring * FOLIAGE_LOOP_VERTICES + column;
            destination.vertices.push(source.vertices[index]);
            // Reuse dense normals to avoid a lighting jump at a LOD boundary.
            destination.normals.push(source.normals[index]);
        }
    }
    let sides = u32::try_from(columns.len()).expect("pine loop fits u32");
    for ring in 0..2 {
        for side in 0..sides {
            let a = offset + ring * sides + side;
            let b = offset + ring * sides + (side + 1) % sides;
            destination
                .triangles
                .extend([a, b, a + sides, b, b + sides, a + sides]);
        }
    }
}

#[cfg(test)]
fn foliage_bounds(vertices: &[Vec3]) -> (Vec3, Vec3) {
    vertices.iter().fold(
        (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY)),
        |(minimum, maximum), &point| (minimum.min(point), maximum.max(point)),
    )
}

fn whorl(
    mesh: &mut Mesh,
    centre: Vec3,
    radius: f32,
    height: f32,
    sides: usize,
    phase: f32,
    foliage: bool,
) {
    let offset = mesh.vertices.len() as u32;
    for ring in 0..3 {
        let (z, r) = if foliage {
            [(0.0, 0.72), (0.18, 1.0), (1.0, 0.025)][ring]
        } else {
            [(0.0, 1.2), (0.1, 0.95), (1.0, 0.04)][ring]
        };
        for side in 0..sides {
            let angle = side as f32 / sides as f32 * TAU + phase;
            let ragged = if foliage {
                1.0 + 0.15 * (angle * 2.0 + phase * 0.7).sin()
                    + 0.11 * (angle * 3.0 - phase * 1.3).sin()
                    + 0.065 * (angle * 5.0 + phase * 0.9).sin()
            } else {
                1.0
            };
            let rim_lift = if foliage {
                (1.0 - z)
                    * (radius / 3.0).min(1.0)
                    * (0.11
                        + 0.07 * (angle * 3.0 + phase * 0.8).sin()
                        + 0.04 * (angle * 5.0 - phase * 1.1).cos())
            } else {
                0.0
            };
            let p = centre
                + Vec3::new(
                    angle.cos() * radius * r * ragged,
                    angle.sin() * radius * r * ragged,
                    height * z + rim_lift,
                );
            mesh.vertices.push(p / crate::ISLAND_WORLD_METRES);
        }
    }
    for ring in 0..2 {
        for side in 0..sides {
            let a = offset + (ring * sides + side) as u32;
            let b = offset + (ring * sides + (side + 1) % sides) as u32;
            let c = a + sides as u32;
            let d = b + sides as u32;
            mesh.triangles.extend_from_slice(&[a, b, c, b, d, c]);
        }
    }
}
/// Three overlapping lobes retain a spreading silhouette without carrying the
/// detailed Island crown's tessellation into every distant instance.
#[must_use]
pub fn broadleaf_crown(source: &Mesh) -> Mesh {
    let mut min = Vec3::splat(f32::INFINITY);
    let mut max = Vec3::splat(f32::NEG_INFINITY);
    for &p in &source.vertices {
        min = min.min(p);
        max = max.max(p);
    }
    if source.vertices.is_empty() {
        return Mesh::default();
    }
    let centre = (min + max) * 0.5;
    let extent = (max - min) * 0.5;
    let mut mesh = Mesh::default();
    for (dx, dy, dz, scale) in [
        (-0.27, -0.12, -0.20, 0.84),
        (0.28, 0.12, -0.04, 0.74),
        (0.0, -0.04, 0.32, 0.75),
    ] {
        let start = mesh.vertices.len() as u32;
        for ring in 0..5 {
            let latitude = -std::f32::consts::FRAC_PI_2 + std::f32::consts::PI * ring as f32 / 4.0;
            for side in 0..8 {
                let angle = TAU * side as f32 / 8.0;
                mesh.vertices.push(
                    centre
                        + extent * Vec3::new(dx, dy, dz)
                        + extent
                            * scale
                            * Vec3::new(
                                latitude.cos() * angle.cos(),
                                latitude.cos() * angle.sin(),
                                latitude.sin(),
                            ),
                );
            }
        }
        for ring in 0..4 {
            for side in 0..8 {
                let a = start + ring * 8 + side;
                let b = start + ring * 8 + (side + 1) % 8;
                mesh.triangles
                    .extend_from_slice(&[a, b, a + 8, b, b + 8, a + 8]);
            }
        }
    }
    mesh.calculate_normals();
    mesh
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fine_conifer_tips_close_the_stem_and_boughs_without_capping_their_roots() {
        for variant in 0..8 {
            let tree = generate_conifer(variant);
            let fine = &tree.lod0_wood;
            let grown_vertices = tree.lod1_wood.vertices.len() + 72;
            assert_eq!(fine.vertices.len(), grown_vertices + 7);
            assert_eq!(
                &fine.vertices[..tree.lod1_wood.vertices.len()],
                tree.lod1_wood.vertices
            );
            let mut edges = std::collections::HashMap::new();
            for triangle in fine.triangles.chunks_exact(3) {
                for (a, b) in [
                    (triangle[0], triangle[1]),
                    (triangle[1], triangle[2]),
                    (triangle[2], triangle[0]),
                ] {
                    *edges.entry((a.min(b), a.max(b))).or_insert(0) += 1;
                }
            }
            assert!(edges.values().all(|&uses| matches!(uses, 1 | 2)));
            assert_eq!(
                edges.values().filter(|&&uses| uses == 1).count(),
                6 + 6 * 4,
                "only the original stem and bough root rims stay open"
            );
            for branch in 0..6 {
                let offset = tree.lod1_wood.vertices.len() + branch * 12;
                let lower = fine.vertices[offset + 4..offset + 8].iter().sum::<Vec3>() * 0.25;
                let upper = fine.vertices[offset + 8..offset + 12].iter().sum::<Vec3>() * 0.25;
                let extension = fine.vertices[grown_vertices + branch] - upper;
                assert!(extension.dot(upper - lower) > 0.0);
                assert!(extension.length() * crate::ISLAND_WORLD_METRES <= 0.801);
            }
            let stem_top = fine.vertices[12..18]
                .iter()
                .map(|vertex| vertex.z)
                .fold(f32::NEG_INFINITY, f32::max);
            assert!(fine.vertices[grown_vertices + 6].z > stem_top);
            for triangle in fine.triangles[(24 + 96) * 3..].chunks_exact(3) {
                let [a, b, c] = [triangle[0], triangle[1], triangle[2]]
                    .map(|vertex| fine.vertices[vertex as usize]);
                assert!((b - a).cross(c - a).length_squared() > 0.0);
            }
        }
    }

    #[test]
    fn smallest_pines_keep_two_metres_clear_including_needles_and_boughs() {
        for variant in 0..8 {
            let tree = generate_conifer(variant);
            let needles = crate::tree_shells(&tree.lod0_foliage, true);
            for crown in [
                &tree.lod0_foliage,
                &tree.lod1_foliage,
                &tree.lod2_foliage,
                &needles.mesh,
                &tree.lod0_twigs,
            ] {
                let clearance = crown
                    .vertices
                    .iter()
                    .map(|p| p.z * crate::ISLAND_WORLD_METRES * 0.7 - 0.08)
                    .fold(f32::INFINITY, f32::min);
                assert!(
                    clearance >= 2.0,
                    "variant {variant}: foliage clearance {clearance}"
                );
            }
            let first_bough = tree.lod1_wood.vertices.len();
            let branches = &tree.lod0_wood.vertices[first_bough..first_bough + 72];
            assert!(!branches.is_empty());
            assert!(
                branches
                    .iter()
                    .all(|p| p.z * crate::ISLAND_WORLD_METRES * 0.7 - 0.08 >= 2.0)
            );
            assert_eq!(
                tree.lod1_wood.vertices.len(),
                18,
                "Distant branch geometry stays simple"
            );
            assert_eq!(branches.len(), 6 * 3 * 4, "Six tapered boughs only in LOD0");
            assert_eq!(tree.lod0_twigs.vertices.len(), 6 * 4);
            assert_eq!(tree.lod0_twigs.triangles.len(), 6 * 6);
            for (branch, (bough, card)) in branches
                .chunks_exact(12)
                .zip(tree.lod0_twigs.vertices.chunks_exact(4))
                .enumerate()
            {
                let centres: [Vec3; 3] = std::array::from_fn(|ring| {
                    bough[ring * 4..ring * 4 + 4].iter().sum::<Vec3>() * 0.25
                });
                let first_length = centres[0].distance(centres[1]);
                let second_length = centres[1].distance(centres[2]);
                let apex = tree.lod0_wood.vertices[first_bough + 72 + branch];
                let tip_length = centres[2].distance(apex);
                let branch_direction = (centres[2] - centres[1]).normalize();
                let side = branch_direction.cross(Vec3::Z).normalize();
                let second_fraction = ((first_length + second_length + tip_length) * 0.5
                    - first_length)
                    / second_length;
                assert!((0.0..1.0).contains(&second_fraction));
                let expected_root = centres[1].lerp(centres[2], second_fraction);
                let root = (card[0] + card[1]) * 0.5;
                let end = (card[2] + card[3]) * 0.5;
                assert!(expected_root.distance(root) < 1.0e-8);
                assert!((root.distance(end) * crate::ISLAND_WORLD_METRES - 1.8).abs() < 1.0e-5);
                let width = card[0].distance(card[1]) * crate::ISLAND_WORLD_METRES;
                assert!((width - 2.7).abs() < 1.0e-5);
                let extension = (end - root).normalize();
                assert!(extension.truncate().normalize().dot(side.truncate()) > 0.9999);
                assert!((extension.z - 15.0_f32.to_radians().sin()).abs() < 1.0e-5);
                assert!(card.iter().all(|vertex| vertex.z >= root.z - 1.0e-9));
                assert!((card[1] - card[0]).normalize().dot(extension).abs() < 1.0e-5);
            }
            assert_eq!(
                tree.lod0_wood.triangles.len() / 3 - tree.lod1_wood.triangles.len() / 3,
                96 + 6 * 4 + 6
            );
            assert_eq!(tree.lod0_wood.vertices.len(), tree.lod0_wood.uv.len());
            assert!(
                tree.lod0_wood
                    .normals
                    .iter()
                    .all(|n| n.is_finite() && (n.length() - 1.0).abs() < 0.001)
            );
        }
    }

    #[test]
    fn conifers_have_tiered_tapered_crowns_and_bounded_geometry() {
        for variant in 0..4 {
            let tree = generate_conifer(variant);
            assert_eq!(tree.lod0_foliage.triangles.len() / 3, 768);
            assert_eq!(tree.lod1_foliage.triangles.len() / 3, 384);
            assert_eq!(tree.lod2_foliage.triangles.len() / 3, 192);
            assert!(tree.lod0_foliage.vertices.iter().all(|p| p.is_finite()));
            assert_eq!(tree.lod2_wood.vertices.len(), 8);
            for crown in [&tree.lod0_foliage, &tree.lod1_foliage] {
                let top = crown.vertices.iter().map(|p| p.z).fold(0.0, f32::max);
                assert!(top * crate::ISLAND_WORLD_METRES > 13.5 + variant as f32 * 1.3);
            }
            let bottom = tree
                .lod0_foliage
                .vertices
                .iter()
                .filter(|p| p.z * crate::ISLAND_WORLD_METRES < 5.0)
                .map(|p| p.truncate().length())
                .fold(0.0, f32::max);
            let top = tree
                .lod0_foliage
                .vertices
                .iter()
                .filter(|p| p.z * crate::ISLAND_WORLD_METRES > 12.0)
                .map(|p| p.truncate().length())
                .fold(0.0, f32::max);
            assert!(bottom > top);
        }
    }

    #[test]
    fn reduced_foliage_lods_keep_dense_positions_normals_and_tier_bounds() {
        for variant in 0..8 {
            let tree = generate_conifer(variant);
            let lods = [&tree.lod0_foliage, &tree.lod1_foliage, &tree.lod2_foliage];
            for pair in lods.windows(2) {
                assert!(
                    pair[1]
                        .vertices
                        .iter()
                        .all(|point| pair[0].vertices.contains(point))
                );
            }
            for (lod, sides) in lods.into_iter().zip([24, 12, 6]) {
                assert_eq!(lod.vertices.len(), FOLIAGE_TIERS * 3 * sides);
                assert_eq!(lod.triangles.len() / 3, FOLIAGE_TIERS * 4 * sides);
                for tier in 0..FOLIAGE_TIERS {
                    let dense = &tree.lod0_foliage.vertices[tier * 72..(tier + 1) * 72];
                    let vertices = &lod.vertices[tier * 3 * sides..(tier + 1) * 3 * sides];
                    let (minimum, maximum) = foliage_bounds(dense);
                    let (low, high) = foliage_bounds(vertices);
                    assert!((minimum.truncate() - low.truncate()).length() < 1.0e-8);
                    assert!((maximum - high).length() < 1.0e-8);
                    assert!(low.z >= minimum.z);
                    assert!((low.z - minimum.z) * crate::ISLAND_WORLD_METRES < 0.221);
                    for (index, point) in vertices.iter().enumerate() {
                        let original = dense.iter().position(|p| p == point).unwrap();
                        assert_eq!(
                            lod.normals[tier * 3 * sides + index],
                            tree.lod0_foliage.normals[tier * 72 + original]
                        );
                    }
                    let skirt = &vertices[..sides];
                    let low = skirt.iter().map(|p| p.z).fold(f32::INFINITY, f32::min);
                    let high = skirt.iter().map(|p| p.z).fold(f32::NEG_INFINITY, f32::max);
                    let dense_low = dense[..FOLIAGE_LOOP_VERTICES]
                        .iter()
                        .map(|p| p.z)
                        .fold(f32::INFINITY, f32::min);
                    let dense_high = dense[..FOLIAGE_LOOP_VERTICES]
                        .iter()
                        .map(|p| p.z)
                        .fold(f32::NEG_INFINITY, f32::max);
                    assert!(high - low > (dense_high - dense_low) * 0.1);
                }
                assert!(
                    lod.normals
                        .iter()
                        .all(|n| n.is_finite() && (n.length() - 1.0).abs() < 0.001)
                );
                for triangle in lod.triangles.chunks_exact(3) {
                    let [a, b, c] =
                        [triangle[0], triangle[1], triangle[2]].map(|i| lod.vertices[i as usize]);
                    assert!((b - a).cross(c - a).length_squared() > 0.0);
                }
            }
            assert_eq!(tree, generate_conifer(variant));
        }
    }
}
