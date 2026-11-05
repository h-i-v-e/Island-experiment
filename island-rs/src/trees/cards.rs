//! Shared bare-twig fans following each branch's recursive depth.

use crate::{ISLAND_WORLD_METRES, Mesh, Vec2, Vec3};

/// Support point and local tangent sampled along a whole branch.
/// Position and radius use normalized island coordinates (Z is up).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BranchTip {
    pub position: Vec3,
    pub direction: Vec3,
    pub radius: f32,
}

/// Quads rooted at UV (0.5, 0), alternating between the transverse sides of
/// each branch and rising 15 degrees.
/// Level the transverse width when needed to keep the entire fan above its root,
/// while retaining its 15-degree ascent towards Z.
/// At branch depth d, supply d supports at arc fractions n/(n+1), n=1..d.
/// The trunk has depth zero and no supports.
///
/// # Panics
/// Panics if the generated mesh exceeds the `u32` vertex index range.
#[must_use]
pub fn twig_cards<B: AsRef<[BranchTip]>>(branches: impl IntoIterator<Item = B>) -> Mesh {
    twig_cards_above(branches, f32::NEG_INFINITY)
}

/// Keep fans above their attachment planes, also respecting a minimum height.
/// Branch supports stay fixed; fans retain their full dimensions when flat.
pub(super) fn twig_cards_above<B: AsRef<[BranchTip]>>(
    branches: impl IntoIterator<Item = B>,
    minimum_height: f32,
) -> Mesh {
    let branches = branches.into_iter();
    let (minimum, _) = branches.size_hint();
    let mut mesh = Mesh {
        vertices: Vec::with_capacity(minimum * 4),
        normals: Vec::with_capacity(minimum * 4),
        triangles: Vec::with_capacity(minimum * 6),
        uv: Vec::with_capacity(minimum * 4),
    };
    let (rise, reach) = 15.0_f32.to_radians().sin_cos();
    for branch in branches {
        let supports = branch.as_ref();
        // Share one transverse frame across the whole group. Bending across
        // vertical must not flip the frame between successive fans.
        let branch_direction = supports
            .iter()
            .map(|tip| tip.direction)
            .sum::<Vec3>()
            .normalize_or(Vec3::Z);
        let side = branch_direction.cross(Vec3::Z).normalize_or(Vec3::X);
        for (index, tip) in supports.iter().enumerate() {
            let sign = if index % 2 == 0 { 1.0 } else { -1.0 };
            let direction = tip.direction.normalize_or(Vec3::Z);
            let extension = side * (sign * reach) + Vec3::Z * rise;
            let branch_width = direction.cross(side).normalize_or(Vec3::Z);
            let mut across = (branch_width - extension * branch_width.dot(extension)).normalize();
            let length =
                (tip.radius * 80.0).clamp(1.8 / ISLAND_WORLD_METRES, 3.5 / ISLAND_WORLD_METRES);
            let candidate_width = across * (length * 0.75);
            let candidate_end = extension * length;
            if [
                -candidate_width.z,
                candidate_width.z,
                candidate_end.z - candidate_width.z,
                candidate_end.z + candidate_width.z,
            ]
            .into_iter()
            .any(|height| height < 0.0)
            {
                // Level the width without flattening the upward centreline.
                // This keeps every corner above its attachment and the fan
                // rising by fifteen degrees at its full dimensions.
                across = (side * sign).cross(Vec3::Z);
            }
            let mut width = length * 0.75;
            if across.z.abs() > 1.0e-6 {
                width = width.min(((tip.position.z - minimum_height) / across.z.abs()).max(0.0));
            }
            let half_width = across * width;
            let end = tip.position + extension * length;
            let offset =
                u32::try_from(mesh.vertices.len()).expect("twig card mesh fits u32 indices");
            mesh.vertices.extend([
                tip.position - half_width,
                tip.position + half_width,
                end - half_width,
                end + half_width,
            ]);
            mesh.uv.extend([
                Vec2::new(0.0, 0.0),
                Vec2::new(1.0, 0.0),
                Vec2::new(0.0, 1.0),
                Vec2::new(1.0, 1.0),
            ]);
            mesh.triangles.extend([
                offset,
                offset + 1,
                offset + 2,
                offset + 1,
                offset + 3,
                offset + 2,
            ]);
            mesh.normals
                .extend([across.cross(extension).normalize(); 4]);
        }
    }
    mesh
}

/// Sample the whole branch centerline rather than its last mesh segment.
pub(super) fn branch_support(centres: &[Vec3], radius: f32, fraction: f32) -> BranchTip {
    let total: f32 = centres
        .windows(2)
        .map(|pair| pair[0].distance(pair[1]))
        .sum();
    let mut remaining = total * fraction;
    for pair in centres.windows(2) {
        let segment = pair[1] - pair[0];
        let length = segment.length();
        if remaining <= length && length > 0.0 {
            return BranchTip {
                position: pair[0] + segment * (remaining / length),
                direction: segment / length,
                radius,
            };
        }
        remaining -= length;
    }
    BranchTip {
        position: centres.last().copied().unwrap_or(Vec3::ZERO),
        direction: Vec3::Z,
        radius,
    }
}

pub(super) fn branch_fan_supports(centres: &[Vec3], radius: f32, depth: u8) -> Vec<BranchTip> {
    (1..=depth)
        .map(|index| {
            let index = f32::from(index);
            branch_support(centres, radius, index / (index + 1.0))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paired_fans_are_rooted_on_opposite_sides_and_never_dip_below_their_roots() {
        for directions in [
            [Vec3::Z; 2],
            [Vec3::X; 2],
            [Vec3::new(0.3, 0.4, 0.7); 2],
            [Vec3::new(0.01, 0.0, 1.0), Vec3::new(-0.01, 0.0, 1.0)],
        ] {
            let tips = directions.map(|direction| BranchTip {
                position: Vec3::new(0.01, -0.02, 0.003),
                direction,
                radius: 0.04 / ISLAND_WORLD_METRES,
            });
            let mesh = twig_cards([tips]);
            assert_eq!(mesh.vertices.len(), 8);
            assert_eq!(mesh.triangles.len(), 12);
            let mut extensions = [Vec3::ZERO; 2];
            for (card, vertices) in mesh.vertices.chunks_exact(4).enumerate() {
                let root = (vertices[0] + vertices[1]) * 0.5;
                let end = (vertices[2] + vertices[3]) * 0.5;
                let extension = (end - root).normalize();
                extensions[card] = extension;
                assert!(root.distance(tips[card].position) < 1.0e-8);
                assert!((extension.z - 15.0_f32.to_radians().sin()).abs() < 1.0e-5);
                assert!(vertices.iter().all(|vertex| vertex.z >= root.z - 1.0e-9));
                assert!((vertices[1] - vertices[0]).normalize().dot(extension).abs() < 1.0e-5);
                let normal = (vertices[1] - vertices[0])
                    .cross(vertices[2] - vertices[0])
                    .normalize();
                assert!(normal.dot(mesh.normals[card * 4]) > 0.9999);
                assert_eq!(
                    (mesh.uv[card * 4] + mesh.uv[card * 4 + 1]) * 0.5,
                    Vec2::new(0.5, 0.0)
                );
            }
            assert!(
                extensions[0]
                    .truncate()
                    .normalize()
                    .dot(extensions[1].truncate().normalize())
                    < -0.9999
            );
        }
    }

    #[test]
    fn supports_sample_half_and_two_thirds_of_the_whole_bent_branch() {
        let centres = [Vec3::ZERO, Vec3::X * 3.0, Vec3::X * 3.0 + Vec3::Z * 6.0]
            .map(|point| point / ISLAND_WORLD_METRES);
        let supports = branch_fan_supports(&centres, 0.01 / ISLAND_WORLD_METRES, 2);
        for (support, z) in supports.into_iter().zip([1.5, 3.0]) {
            assert!(
                support
                    .position
                    .distance((Vec3::X * 3.0 + Vec3::Z * z) / ISLAND_WORLD_METRES)
                    < 1.0e-8
            );
            assert_eq!(support.direction, Vec3::Z);
        }
    }

    #[test]
    fn fan_progression_has_no_trunk_cards_and_one_card_per_depth() {
        let centres = [Vec3::ZERO, Vec3::Z * (12.0 / ISLAND_WORLD_METRES)];
        let fractions = [0.5, 2.0 / 3.0, 0.75, 0.8, 5.0 / 6.0];
        for depth in 0..=5 {
            let supports = branch_fan_supports(&centres, 0.01 / ISLAND_WORLD_METRES, depth);
            assert_eq!(supports.len(), usize::from(depth));
            let mesh = twig_cards([supports.as_slice()]);
            assert_eq!(mesh.vertices.len(), usize::from(depth) * 4);
            assert_eq!(mesh.triangles.len(), usize::from(depth) * 6);
            for (support, expected) in supports.iter().zip(fractions) {
                assert!((support.position.z / centres[1].z - expected).abs() < 1.0e-6);
            }
        }
    }

    #[test]
    fn crossing_fans_keep_full_size_and_rise_above_the_attachment_plane() {
        for (radius, expected_length) in [(0.001, 1.8), (0.04, 3.2), (0.1, 3.5)] {
            let tip = BranchTip {
                position: Vec3::Z * (6.0 / ISLAND_WORLD_METRES),
                direction: Vec3::X,
                radius: radius / ISLAND_WORLD_METRES,
            };
            let mesh = twig_cards([[tip; 2]]);
            let length = mesh.vertices[0].distance(mesh.vertices[2]) * ISLAND_WORLD_METRES;
            let width = mesh.vertices[0].distance(mesh.vertices[1]) * ISLAND_WORLD_METRES;
            assert!((length - expected_length).abs() < 1.0e-5);
            assert!((width - 1.5 * expected_length).abs() < 1.0e-5);
            let minimum_height = tip.position.z - 0.1 / ISLAND_WORLD_METRES;
            let rising = twig_cards_above([[tip; 2]], minimum_height);
            for (card, vertices) in rising.vertices.chunks_exact(4).enumerate() {
                let root = (vertices[0] + vertices[1]) * 0.5;
                let end = (vertices[2] + vertices[3]) * 0.5;
                assert!(root.distance(tip.position) < 1.0e-8);
                assert!(
                    (root.distance(end) * ISLAND_WORLD_METRES - expected_length).abs() < 1.0e-5
                );
                assert!(
                    (vertices[0].distance(vertices[1]) * ISLAND_WORLD_METRES - width).abs()
                        < 1.0e-5
                );
                assert!(vertices.iter().all(|vertex| vertex.z >= root.z - 1.0e-9));
                assert!(((end - root).normalize().z - 15.0_f32.to_radians().sin()).abs() < 1.0e-5);
                assert!((vertices[0].z - vertices[1].z).abs() < 1.0e-9);
                assert!(
                    (rising.normals[card * 4].dot(Vec3::Z) - 15.0_f32.to_radians().cos()).abs()
                        < 1.0e-5
                );
            }
        }
    }

    #[test]
    fn fans_already_above_their_attachment_keep_the_upward_tilt() {
        let tip = BranchTip {
            position: Vec3::Z * (6.0 / ISLAND_WORLD_METRES),
            direction: Vec3::Z,
            radius: 0.01 / ISLAND_WORLD_METRES,
        };
        let mesh = twig_cards([[tip; 2]]);
        for vertices in mesh.vertices.chunks_exact(4) {
            let root = (vertices[0] + vertices[1]) * 0.5;
            let end = (vertices[2] + vertices[3]) * 0.5;
            assert!(root.distance(tip.position) < 1.0e-8);
            assert!(((end - root).normalize().z - 15.0_f32.to_radians().sin()).abs() < 1.0e-5);
            assert!(vertices.iter().all(|vertex| vertex.z >= root.z));
        }
    }
}
