use std::collections::HashMap;

use super::{ISLAND_WORLD_METRES, Mesh, Vec2};
use crate::rivers::{CoastlinePath, coastline_paths};

const LAND_LIFT: f32 = 2.0 / ISLAND_WORLD_METRES;
const BEACH_CREST: f32 = 0.25 / ISLAND_WORLD_METRES;
const BEACH_TOE: f32 = 0.01 / ISLAND_WORLD_METRES;
const COAST_SMOOTHING_PASSES: usize = 2;
const COAST_SMOOTHING_STRENGTH: f32 = 0.5;
const DISTANCE_EPSILON: f32 = 1.0e-12;

/// Temporary geometry used by the coastal cliff experiment.
///
/// The broad LOD0 mesh owns the actual height mutation. This value retains
/// only the original and smoothed coastline loops needed to project beaches
/// after final tessellation.
pub(super) struct CoastalCliffExperiment {
    original: Vec<Vec<Vec2>>,
    smoothed: Vec<Vec<Vec2>>,
}

impl CoastalCliffExperiment {
    pub(super) fn prepare(terrain: &mut Mesh) -> Result<Self, String> {
        let (points, paths) = sea_level_paths(terrain)?;
        let original = paths
            .into_iter()
            .filter(|path| path.closed)
            .map(|path| {
                path.vertices
                    .into_iter()
                    .map(|vertex| points[vertex as usize])
                    .collect::<Vec<_>>()
            })
            .filter(|path| path.len() >= 3)
            .collect::<Vec<_>>();
        let smoothed = original
            .iter()
            .map(|path| smooth_closed_path(&resample_closed_path(path)))
            .collect();

        terrain
            .vertices
            .iter_mut()
            .filter(|vertex| vertex.z > 0.0)
            .for_each(|vertex| vertex.z += LAND_LIFT);
        terrain.calculate_normals();

        Ok(Self { original, smoothed })
    }

    pub(super) fn raise_beaches(&self, terrain: &mut Mesh) -> usize {
        let mut raised = 0;
        for vertex in &mut terrain.vertices {
            let point = vertex.truncate();
            if contains_any(&self.original, point) || !contains_any(&self.smoothed, point) {
                continue;
            }

            let distance_from_land = distance_to_paths(&self.original, point);
            let distance_from_toe = distance_to_paths(&self.smoothed, point);
            let total_distance = distance_from_land + distance_from_toe;
            if !total_distance.is_finite() || total_distance <= DISTANCE_EPSILON {
                continue;
            }
            let landward = (distance_from_toe / total_distance).clamp(0.0, 1.0);
            let landward = landward * landward * (3.0 - 2.0 * landward);
            let target = (BEACH_CREST - BEACH_TOE).mul_add(landward, BEACH_TOE);
            if vertex.z < target {
                vertex.z = target;
                raised += 1;
            }
        }
        raised
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum ContourPoint {
    Vertex(u32),
    Edge(u32, u32),
}

fn sea_level_paths(terrain: &Mesh) -> Result<(Vec<Vec2>, Vec<CoastlinePath>), String> {
    let adjacency = terrain.adjacency();
    let ocean = ocean_mask(terrain, &adjacency);
    let mut indices = HashMap::<ContourPoint, u32>::new();
    let mut points = Vec::new();
    let mut edges = Vec::new();

    for (face, triangle) in terrain.triangles.chunks_exact(3).enumerate() {
        let vertices = [triangle[0], triangle[1], triangle[2]];
        let has_land = vertices
            .iter()
            .any(|&vertex| terrain.vertices[vertex as usize].z > 0.0);
        let has_ocean = vertices
            .iter()
            .any(|&vertex| ocean[vertex as usize] && terrain.vertices[vertex as usize].z <= 0.0);
        if !has_land || !has_ocean {
            continue;
        }

        let mut intersections = [None; 3];
        let mut count = 0;
        for [a, b] in [
            [vertices[0], vertices[1]],
            [vertices[1], vertices[2]],
            [vertices[2], vertices[0]],
        ] {
            let Some((key, point)) = sea_level_intersection(terrain, a, b) else {
                continue;
            };
            let index = *indices.entry(key).or_insert_with(|| {
                let index = points.len() as u32;
                points.push(point);
                index
            });
            if !intersections[..count].contains(&Some(index)) {
                intersections[count] = Some(index);
                count += 1;
            }
        }
        match intersections[..count] {
            [Some(a), Some(b)] if a != b => edges.push([a, b]),
            [] | [Some(_)] => {}
            _ => {
                return Err(format!(
                    "sea-level contour intersects terrain face {face} more than twice"
                ));
            }
        }
    }

    let paths = coastline_paths(&edges)?;
    Ok((points, paths))
}

#[allow(clippy::float_cmp)] // Exact zero identifies an existing sea-level endpoint.
fn sea_level_intersection(terrain: &Mesh, a: u32, b: u32) -> Option<(ContourPoint, Vec2)> {
    let a_position = terrain.vertices[a as usize];
    let b_position = terrain.vertices[b as usize];
    if a_position.z == 0.0 {
        return Some((ContourPoint::Vertex(a), a_position.truncate()));
    }
    if b_position.z == 0.0 {
        return Some((ContourPoint::Vertex(b), b_position.truncate()));
    }
    if (a_position.z > 0.0) == (b_position.z > 0.0) {
        return None;
    }
    let edge = if a < b { (a, b) } else { (b, a) };
    let interpolation = -a_position.z / (b_position.z - a_position.z);
    Some((
        ContourPoint::Edge(edge.0, edge.1),
        a_position
            .truncate()
            .lerp(b_position.truncate(), interpolation),
    ))
}

fn ocean_mask(terrain: &Mesh, adjacency: &super::Adjacency) -> Vec<bool> {
    let perimeter = terrain.perimeter_mask();
    let mut ocean = vec![false; terrain.vertices.len()];
    let mut fringe = perimeter
        .into_iter()
        .enumerate()
        .filter_map(|(vertex, perimeter)| {
            (perimeter && terrain.vertices[vertex].z <= 0.0).then_some(vertex)
        })
        .collect::<Vec<_>>();
    for &vertex in &fringe {
        ocean[vertex] = true;
    }
    while let Some(vertex) = fringe.pop() {
        for &neighbour in &adjacency[vertex] {
            if !ocean[neighbour] && terrain.vertices[neighbour].z <= 0.0 {
                ocean[neighbour] = true;
                fringe.push(neighbour);
            }
        }
    }
    ocean
}

fn resample_closed_path(path: &[Vec2]) -> Vec<Vec2> {
    if path.len() < 3 {
        return path.to_vec();
    }
    let lengths = (0..path.len())
        .map(|index| path[index].distance(path[(index + 1) % path.len()]))
        .collect::<Vec<_>>();
    let perimeter = lengths.iter().sum::<f32>();
    if !perimeter.is_finite() || perimeter <= f32::EPSILON {
        return path.to_vec();
    }

    let spacing = perimeter / path.len() as f32;
    let mut samples = Vec::with_capacity(path.len());
    let mut edge = 0;
    let mut edge_start_distance = 0.0;
    for sample in 0..path.len() {
        let target = sample as f32 * spacing;
        while edge + 1 < path.len() && edge_start_distance + lengths[edge] < target {
            edge_start_distance += lengths[edge];
            edge += 1;
        }
        let length = lengths[edge];
        let interpolation = if length > f32::EPSILON {
            ((target - edge_start_distance) / length).clamp(0.0, 1.0)
        } else {
            0.0
        };
        samples.push(path[edge].lerp(path[(edge + 1) % path.len()], interpolation));
    }
    samples
}

fn smooth_closed_path(path: &[Vec2]) -> Vec<Vec2> {
    let mut current = path.to_vec();
    for _ in 0..COAST_SMOOTHING_PASSES {
        current = (0..current.len())
            .map(|index| {
                let previous = current[(index + current.len() - 1) % current.len()];
                let next = current[(index + 1) % current.len()];
                current[index].lerp((previous + next) * 0.5, COAST_SMOOTHING_STRENGTH)
            })
            .collect();
    }
    current
}

fn contains_any(paths: &[Vec<Vec2>], point: Vec2) -> bool {
    paths.iter().any(|path| polygon_contains(path, point))
}

fn polygon_contains(polygon: &[Vec2], point: Vec2) -> bool {
    let mut inside = false;
    for index in 0..polygon.len() {
        let a = polygon[index];
        let b = polygon[(index + 1) % polygon.len()];
        if point_segment_distance_squared(point, a, b) <= DISTANCE_EPSILON {
            return true;
        }
        if (a.y > point.y) != (b.y > point.y) {
            let crossing = (b.x - a.x).mul_add((point.y - a.y) / (b.y - a.y), a.x);
            if point.x < crossing {
                inside = !inside;
            }
        }
    }
    inside
}

fn distance_to_paths(paths: &[Vec<Vec2>], point: Vec2) -> f32 {
    paths
        .iter()
        .flat_map(|path| {
            (0..path.len()).map(move |index| {
                point_segment_distance_squared(point, path[index], path[(index + 1) % path.len()])
            })
        })
        .fold(f32::INFINITY, f32::min)
        .sqrt()
}

fn point_segment_distance_squared(point: Vec2, start: Vec2, end: Vec2) -> f32 {
    let segment = end - start;
    let length_squared = segment.length_squared();
    if length_squared <= f32::EPSILON {
        return point.distance_squared(start);
    }
    let interpolation = ((point - start).dot(segment) / length_squared).clamp(0.0, 1.0);
    point.distance_squared(segment.mul_add(Vec2::splat(interpolation), start))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Vec3;

    #[test]
    fn smoothing_pulls_headlands_in_and_pushes_bays_out() {
        let contour = vec![
            Vec2::new(-2.0, -2.0),
            Vec2::new(2.0, -2.0),
            Vec2::new(2.0, 0.0),
            Vec2::new(3.0, 1.0),
            Vec2::new(2.0, 2.0),
            Vec2::new(0.0, 1.0),
            Vec2::new(-2.0, 2.0),
        ];
        let smoothed = smooth_closed_path(&contour);

        assert!(smoothed[3].x < contour[3].x, "headland should retreat");
        assert!(smoothed[5].y > contour[5].y, "bay should expand");
        let headland = Vec2::new(2.7, 1.0);
        assert!(polygon_contains(&contour, headland));
        assert!(!polygon_contains(&smoothed, headland));
        let bay = Vec2::new(0.0, 1.1);
        assert!(!polygon_contains(&contour, bay));
        assert!(polygon_contains(&smoothed, bay));
    }

    #[test]
    fn beach_projection_only_raises_smoothed_seaward_additions() {
        let experiment = CoastalCliffExperiment {
            original: vec![vec![
                Vec2::new(-1.0, -1.0),
                Vec2::new(1.0, -1.0),
                Vec2::new(1.0, 1.0),
                Vec2::new(-1.0, 1.0),
            ]],
            smoothed: vec![vec![
                Vec2::new(-2.0, -2.0),
                Vec2::new(2.0, -2.0),
                Vec2::new(2.0, 2.0),
                Vec2::new(-2.0, 2.0),
            ]],
        };
        let mut terrain = Mesh {
            vertices: vec![
                Vec3::new(0.0, 0.0, -1.0),
                Vec3::new(1.5, 0.0, -1.0),
                Vec3::new(2.5, 0.0, -1.0),
            ],
            ..Mesh::default()
        };

        assert_eq!(experiment.raise_beaches(&mut terrain), 1);
        assert!((terrain.vertices[0].z + 1.0).abs() <= f32::EPSILON);
        assert!(terrain.vertices[1].z >= BEACH_TOE);
        assert!((terrain.vertices[2].z + 1.0).abs() <= f32::EPSILON);
    }

    #[test]
    fn prepare_lifts_land_and_keeps_the_seabed_and_coast_at_their_heights() {
        let mut terrain = Mesh {
            vertices: vec![
                Vec3::new(-1.0, -1.0, -1.0),
                Vec3::new(1.0, -1.0, -1.0),
                Vec3::new(1.0, 1.0, -1.0),
                Vec3::new(-1.0, 1.0, -1.0),
                Vec3::new(0.0, 0.0, 1.0),
            ],
            triangles: vec![0, 1, 4, 1, 2, 4, 2, 3, 4, 3, 0, 4],
            ..Mesh::default()
        };
        let original_vertex_count = terrain.vertices.len();
        let coast = CoastalCliffExperiment::prepare(&mut terrain).unwrap();

        assert_eq!(coast.original.len(), 1);
        assert!((terrain.vertices[0].z + 1.0).abs() <= f32::EPSILON);
        assert!((terrain.vertices[4].z - (1.0 + LAND_LIFT)).abs() <= f32::EPSILON);
        assert_eq!(terrain.vertices.len(), original_vertex_count);
        assert!(terrain.vertices.iter().all(|vertex| vertex.is_finite()));
    }
}
