#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss
)]

use std::cmp::Ordering;

use crate::{Mesh, Vec3};

const ISLAND_WORLD_METRES: f32 = 2_000.0;
const FLAT_FACE_MAX_SLOPE_DEGREES: f32 = 35.0;
const STEEP_FACE_MIN_SLOPE_DEGREES: f32 = 55.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RiverEmitter {
    pub vertex_index: usize,
    pub position: Vec3,
    pub direction: Vec3,
    pub strength: f32,
}

#[derive(Clone, Copy)]
struct Candidate {
    vertex_index: usize,
    position: Vec3,
    direction: Vec3,
    strength: f32,
}

#[derive(Clone, Copy)]
struct FaceEdge {
    key: u64,
    face: u32,
}

/// Derives deterministic rough-water emitter positions from a final river mesh.
///
/// The mesh is borrowed. The returned vector is the only persistent allocation;
/// candidate and suppression storage is temporary and released before return.
#[must_use]
pub fn extract_river_emitters(
    mesh: &Mesh,
    sharpness_degrees: f32,
    spacing_metres: f32,
) -> Vec<RiverEmitter> {
    let _timer = crate::profiling::StageTimer::new("river_emitters.extract");
    debug_assert_eq!(mesh.vertices.len(), mesh.normals.len());
    if mesh.vertices.len() != mesh.normals.len() {
        return Vec::new();
    }

    let threshold_dot = sharpness_degrees.clamp(0.0, 89.999).to_radians().cos();
    let sharpness = vertex_sharpness(mesh, threshold_dot);
    let mut candidates = qualifying_candidates(mesh, &sharpness);
    candidates.sort_unstable_by(candidate_priority);

    #[cfg(feature = "profiling")]
    let diagnostics = EmitterDiagnostics::new(mesh, &candidates);
    let spacing = (spacing_metres.max(0.0) / ISLAND_WORLD_METRES).max(f32::EPSILON);
    let mut accepted = suppress_candidates(candidates, spacing);
    accepted.sort_unstable_by_key(|emitter| emitter.vertex_index);
    #[cfg(feature = "profiling")]
    diagnostics.log(&accepted);
    accepted
}

fn qualifying_candidates(mesh: &Mesh, sharpness: &[f32]) -> Vec<Candidate> {
    let mut candidates = Vec::new();
    for (vertex_index, ((&position, &normal), &strength)) in mesh
        .vertices
        .iter()
        .zip(&mesh.normals)
        .zip(sharpness)
        .enumerate()
    {
        if strength <= 0.0 {
            continue;
        }
        let Some(direction) = normal.try_normalize() else {
            continue;
        };
        if !position.is_finite() || !direction.is_finite() || direction.z < 0.0 {
            continue;
        }
        candidates.push(Candidate {
            vertex_index,
            position,
            direction,
            strength,
        });
    }
    candidates
}

fn vertex_sharpness(mesh: &Mesh, threshold_dot: f32) -> Vec<f32> {
    let face_count = mesh.triangles.len() / 3;
    let mut face_normals = vec![Vec3::ZERO; face_count];
    let mut edges = Vec::with_capacity(mesh.triangles.len());
    for (face, triangle) in mesh.triangles.chunks_exact(3).enumerate() {
        let [a, b, c] = [triangle[0], triangle[1], triangle[2]];
        let Some((&position_a, &position_b, &position_c)) = mesh
            .vertices
            .get(a as usize)
            .zip(mesh.vertices.get(b as usize))
            .zip(mesh.vertices.get(c as usize))
            .map(|((a, b), c)| (a, b, c))
        else {
            continue;
        };
        let Some(normal) = (position_b - position_a)
            .cross(position_c - position_a)
            .try_normalize()
        else {
            continue;
        };
        face_normals[face] = normal;
        let face = face as u32;
        edges.extend([
            FaceEdge {
                key: edge_key(a, b),
                face,
            },
            FaceEdge {
                key: edge_key(b, c),
                face,
            },
            FaceEdge {
                key: edge_key(c, a),
                face,
            },
        ]);
    }
    edges.sort_unstable_by_key(|edge| (edge.key, edge.face));

    let mut sharpness = vec![0.0_f32; mesh.vertices.len()];
    let mut start = 0;
    while start < edges.len() {
        let mut end = start + 1;
        while end < edges.len() && edges[end].key == edges[start].key {
            end += 1;
        }
        score_shared_edge(
            &edges[start..end],
            &face_normals,
            threshold_dot,
            &mut sharpness,
        );
        start = end;
    }
    sharpness
}

fn score_shared_edge(
    edges: &[FaceEdge],
    face_normals: &[Vec3],
    threshold_dot: f32,
    sharpness: &mut [f32],
) {
    if edges.len() < 2 {
        return;
    }
    let minimum_flat_alignment = FLAT_FACE_MAX_SLOPE_DEGREES.to_radians().cos();
    let maximum_steep_alignment = STEEP_FACE_MIN_SLOPE_DEGREES.to_radians().cos();
    let mut minimum_dot = 1.0_f32;
    for left in 0..edges.len() - 1 {
        let left_normal = face_normals[edges[left].face as usize];
        if left_normal == Vec3::ZERO {
            continue;
        }
        for right in left + 1..edges.len() {
            let right_normal = face_normals[edges[right].face as usize];
            if right_normal == Vec3::ZERO {
                continue;
            }
            let flatter_alignment = left_normal.z.abs().max(right_normal.z.abs());
            let steeper_alignment = left_normal.z.abs().min(right_normal.z.abs());
            if flatter_alignment < minimum_flat_alignment
                || steeper_alignment > maximum_steep_alignment
            {
                continue;
            }
            minimum_dot = minimum_dot.min(left_normal.dot(right_normal).abs().clamp(0.0, 1.0));
        }
    }
    if minimum_dot >= threshold_dot {
        return;
    }
    let strength =
        ((threshold_dot - minimum_dot) / threshold_dot.max(f32::EPSILON)).clamp(0.0, 1.0);
    let a = (edges[0].key >> 32) as usize;
    let b = (edges[0].key & u64::from(u32::MAX)) as usize;
    sharpness[a] = sharpness[a].max(strength);
    sharpness[b] = sharpness[b].max(strength);
}

fn edge_key(a: u32, b: u32) -> u64 {
    let (minimum, maximum) = if a < b { (a, b) } else { (b, a) };
    (u64::from(minimum) << 32) | u64::from(maximum)
}

fn candidate_priority(left: &Candidate, right: &Candidate) -> Ordering {
    right
        .strength
        .partial_cmp(&left.strength)
        .unwrap_or(Ordering::Equal)
        .then_with(|| left.vertex_index.cmp(&right.vertex_index))
}

fn suppress_candidates(candidates: Vec<Candidate>, spacing: f32) -> Vec<RiverEmitter> {
    let spacing_squared = spacing * spacing;
    let dimension = (1.0 / spacing).ceil().clamp(1.0, 4096.0) as usize;
    let mut bin_heads = vec![usize::MAX; dimension * dimension];
    let mut accepted: Vec<RiverEmitter> = Vec::with_capacity(candidates.len());
    let mut next = Vec::with_capacity(candidates.len());

    for candidate in candidates {
        let x = bin_coordinate(candidate.position.x, dimension);
        let y = bin_coordinate(candidate.position.y, dimension);
        let minimum_x = x.saturating_sub(1);
        let maximum_x = (x + 1).min(dimension - 1);
        let minimum_y = y.saturating_sub(1);
        let maximum_y = (y + 1).min(dimension - 1);
        let mut overlaps = false;
        'bins: for nearby_y in minimum_y..=maximum_y {
            for nearby_x in minimum_x..=maximum_x {
                let mut accepted_index = bin_heads[nearby_y * dimension + nearby_x];
                while accepted_index != usize::MAX {
                    if accepted[accepted_index]
                        .position
                        .distance_squared(candidate.position)
                        < spacing_squared
                    {
                        overlaps = true;
                        break 'bins;
                    }
                    accepted_index = next[accepted_index];
                }
            }
        }
        if overlaps {
            continue;
        }

        let bin = y * dimension + x;
        let accepted_index = accepted.len();
        next.push(bin_heads[bin]);
        bin_heads[bin] = accepted_index;
        accepted.push(RiverEmitter {
            vertex_index: candidate.vertex_index,
            position: candidate.position,
            direction: candidate.direction,
            strength: candidate.strength,
        });
    }
    accepted
}

#[cfg(feature = "profiling")]
struct EmitterDiagnostics {
    perimeter: std::collections::BTreeSet<usize>,
    raw_count: usize,
    raw_edges: usize,
    minimum_strength: f32,
    maximum_strength: f32,
}

#[cfg(feature = "profiling")]
impl EmitterDiagnostics {
    fn new(mesh: &Mesh, candidates: &[Candidate]) -> Self {
        let perimeter = mesh.perimeter_vertices();
        Self {
            raw_count: candidates.len(),
            raw_edges: candidates
                .iter()
                .filter(|candidate| perimeter.contains(&candidate.vertex_index))
                .count(),
            minimum_strength: candidates
                .iter()
                .map(|candidate| candidate.strength)
                .reduce(f32::min)
                .unwrap_or_default(),
            maximum_strength: candidates
                .iter()
                .map(|candidate| candidate.strength)
                .reduce(f32::max)
                .unwrap_or_default(),
            perimeter,
        }
    }

    fn log(&self, accepted: &[RiverEmitter]) {
        let accepted_edges = accepted
            .iter()
            .filter(|emitter| self.perimeter.contains(&emitter.vertex_index))
            .count();
        eprintln!(
            "river_emitters,raw={},accepted={},raw_edges={},accepted_edges={accepted_edges},strength_min={:.3},strength_max={:.3}",
            self.raw_count,
            accepted.len(),
            self.raw_edges,
            self.minimum_strength,
            self.maximum_strength,
        );
    }
}

fn bin_coordinate(value: f32, dimension: usize) -> usize {
    ((value.clamp(0.0, 1.0) * dimension as f32).floor() as usize).min(dimension - 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn crease_between(first_degrees: f32, second_degrees: f32) -> Mesh {
        let first = first_degrees.to_radians();
        let second = second_degrees.to_radians();
        let mut mesh = Mesh {
            vertices: vec![
                Vec3::ZERO,
                Vec3::new(0.01, 0.0, 0.0),
                Vec3::new(0.0, first.cos() * 0.01, -first.sin() * 0.01),
                Vec3::new(0.0, second.cos() * 0.01, -second.sin() * 0.01),
            ],
            triangles: vec![0, 1, 2, 0, 1, 3],
            ..Mesh::default()
        };
        mesh.calculate_normals();
        mesh
    }

    #[test]
    fn dihedral_threshold_rejects_gentler_edges() {
        assert!(extract_river_emitters(&crease_between(20.0, 54.9), 35.0, 2.0).is_empty());
        assert_eq!(
            extract_river_emitters(&crease_between(20.0, 55.1), 35.0, 2.0).len(),
            2
        );
    }

    #[test]
    fn coplanar_vertical_faces_do_not_emit_halfway_down_a_fall() {
        let mut mesh = Mesh {
            vertices: vec![
                Vec3::new(0.0, 0.0, 0.0),
                Vec3::new(0.01, 0.0, 0.0),
                Vec3::new(0.0, 0.0, -0.01),
                Vec3::new(0.01, 0.0, -0.01),
            ],
            triangles: vec![0, 1, 2, 1, 3, 2],
            ..Mesh::default()
        };
        mesh.calculate_normals();
        assert!(extract_river_emitters(&mesh, 35.0, 2.0).is_empty());
    }

    #[test]
    fn waterfall_top_and_bottom_edges_are_selected() {
        let mut mesh = Mesh {
            vertices: vec![
                Vec3::new(0.0, 0.0, 0.01),
                Vec3::new(0.01, 0.0, 0.01),
                Vec3::new(0.0, 0.01, 0.01),
                Vec3::new(0.01, 0.01, 0.01),
                Vec3::new(0.0, 0.01, 0.0),
                Vec3::new(0.01, 0.01, 0.0),
                Vec3::new(0.0, 0.02, 0.0),
                Vec3::new(0.01, 0.02, 0.0),
            ],
            triangles: vec![
                0, 1, 2, 1, 3, 2, // upper flat reach
                2, 3, 4, 3, 5, 4, // vertical waterfall face
                4, 5, 6, 5, 7, 6, // lower flat reach
            ],
            ..Mesh::default()
        };
        mesh.calculate_normals();
        let emitters = extract_river_emitters(&mesh, 35.0, 0.1);
        let vertices: Vec<usize> = emitters
            .iter()
            .map(|emitter| emitter.vertex_index)
            .collect();
        assert_eq!(vertices, vec![2, 3, 4, 5]);
        assert!(
            emitters
                .iter()
                .all(|emitter| (emitter.direction.length() - 1.0).abs() < 1.0e-6)
        );
    }

    #[test]
    fn suppression_is_spaced_and_deterministic() {
        let candidates = vec![
            Candidate {
                vertex_index: 0,
                position: Vec3::new(0.1, 0.1, 0.0),
                direction: Vec3::Z,
                strength: 1.0,
            },
            Candidate {
                vertex_index: 1,
                position: Vec3::new(0.100_5, 0.1, 0.0),
                direction: Vec3::Z,
                strength: 0.9,
            },
            Candidate {
                vertex_index: 2,
                position: Vec3::new(0.102, 0.1, 0.0),
                direction: Vec3::Z,
                strength: 0.8,
            },
        ];
        let spacing = 2.0 / ISLAND_WORLD_METRES;
        let first = suppress_candidates(candidates.clone(), spacing);
        let second = suppress_candidates(candidates, spacing);
        assert_eq!(first, second);
        assert_eq!(first.len(), 2);
        assert!(first[0].position.distance(first[1].position) >= 2.0 / ISLAND_WORLD_METRES);
    }

    #[test]
    fn downward_and_non_finite_normals_are_rejected() {
        let mut mesh = crease_between(0.0, 90.0);
        mesh.normals[0] = Vec3::NEG_Z;
        mesh.normals[1] = Vec3::new(f32::NAN, 0.0, 1.0);
        assert!(extract_river_emitters(&mesh, 35.0, 2.0).is_empty());
    }
}
