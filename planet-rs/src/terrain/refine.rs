//! Conforming spherical edge splits and conservative radial smoothing.
use super::{TerrainOptions, graph::SurfaceGraph};
use crate::{DVec3, PlanetError, PlanetMesh, noise};
use std::collections::{BTreeMap, HashSet};

pub(super) const MAX_REFINED_VERTICES: usize = 2_000_000;

pub(super) fn subdivision_fits(mesh: &PlanetMesh, selected: &[bool], limit: usize) -> bool {
    let remaining = limit.saturating_sub(mesh.directions.len());
    let mut edges = HashSet::new();
    for (&[a, b, c], &selected) in mesh.triangles.iter().zip(selected) {
        if selected {
            for (a, b) in [(a, b), (b, c), (c, a)] {
                edges.insert((a.min(b), a.max(b)));
                if edges.len() > remaining {
                    return false;
                }
            }
        }
    }
    true
}

/// Island's displacement/mean-edge criterion in a radial frame. Subtract the
/// undeformed sphere's curvature, so a constant-height sphere needs no detail.
pub(super) fn curved_faces(mesh: &PlanetMesh, heights: &[f64]) -> Vec<bool> {
    let graph = SurfaceGraph::new(mesh);
    let marked: Vec<_> = graph
        .neighbours
        .iter()
        .enumerate()
        .map(|(i, neighbours)| {
            let (height, length) = neighbours.iter().fold((0.0, 0.0), |(h, l), &j| {
                (
                    h + heights[j],
                    l + mesh.directions[i].distance(mesh.directions[j]),
                )
            });
            let length = length / count(neighbours.len());
            length > 0.002 && (heights[i] - height / count(neighbours.len())).abs() > length * 0.025
        })
        .collect();
    mesh.triangles
        .iter()
        .map(|face| face.iter().any(|&i| marked[i as usize]))
        .collect()
}

/// Smooth positions in the local tangent plane as well as radial heights.
/// A per-face orientation bound prevents relaxation folding skinny triangles.
/// Reprojecting directions preserves sea radius instead of shrinking the sphere.
pub(super) fn relax(mesh: &mut PlanetMesh, heights: &mut [f64], soil: &mut [f64], passes: u32) {
    if passes == 0 {
        return;
    }
    let mut graph = SurfaceGraph::new(mesh);
    let mut incident = vec![Vec::new(); mesh.directions.len()];
    for (face, indices) in mesh.triangles.iter().enumerate() {
        for &i in indices {
            incident[i as usize].push(face);
        }
    }
    let mut targets = vec![DVec3::ZERO; mesh.directions.len()];
    for _ in 0..passes {
        let height_mass = mass(heights, &graph.areas);
        let soil_mass = mass(soil, &graph.areas);
        for (i, neighbours) in graph.neighbours.iter().enumerate() {
            let direction = mesh.directions[i];
            let average = neighbours
                .iter()
                .map(|&j| mesh.directions[j])
                .sum::<DVec3>()
                / count(neighbours.len());
            let tangent = average - direction * average.dot(direction);
            let shortest = neighbours
                .iter()
                .map(|&j| direction.distance(mesh.directions[j]))
                .fold(f64::INFINITY, f64::min);
            targets[i] =
                (direction + (tangent * 0.65).clamp_length_max(shortest * 0.2)).normalize();
        }
        for (i, &target) in targets.iter().enumerate() {
            let original = mesh.directions[i];
            let mut fraction: f64 = 1.0;
            for &face in &incident[i] {
                let indices = mesh.triangles[face];
                let before = indices.map(|j| mesh.directions[j as usize]);
                let after = indices.map(|j| {
                    if j as usize == i {
                        target
                    } else {
                        mesh.directions[j as usize]
                    }
                });
                let area = before[0].dot(before[1].cross(before[2]));
                let candidate = after[0].dot(after[1].cross(after[2]));
                if candidate < area * 0.25 {
                    fraction = fraction.min((area * 0.75 / (area - candidate)).clamp(0.0, 1.0));
                }
            }
            mesh.directions[i] = original.lerp(target, fraction).normalize();
        }
        graph = SurfaceGraph::new(mesh);
        let correction =
            (height_mass - mass(heights, &graph.areas)) / graph.areas.iter().sum::<f64>();
        for height in heights.iter_mut() {
            *height += correction;
        }
        let new_soil_mass = mass(soil, &graph.areas);
        if new_soil_mass > 0.0 {
            for depth in soil.iter_mut() {
                *depth *= soil_mass / new_soil_mass;
            }
        }
        smooth(&graph, heights, 1);
    }
}

fn mass(values: &[f64], areas: &[f64]) -> f64 {
    values
        .iter()
        .zip(areas)
        .map(|(value, area)| value * area)
        .sum()
}

pub(super) fn subdivide(
    mesh: &mut PlanetMesh,
    heights: &mut Vec<f64>,
    hardness: &mut Vec<f64>,
    soil: &mut Vec<f64>,
    selected: &[bool],
    displacement: Option<(u64, TerrainOptions)>,
) -> Result<(), PlanetError> {
    let mut splits = BTreeMap::new();
    for (face, &selected) in mesh.triangles.iter().zip(selected) {
        if selected {
            let [a, b, c] = *face;
            for (a, b) in [(a, b), (b, c), (c, a)] {
                splits.entry((a.min(b), a.max(b))).or_insert(0.5);
            }
        }
    }
    split_edges(mesh, heights, hardness, soil, splits, displacement)
}

/// Split each shared edge once, including non-midpoint intersections used to
/// align waterfall crest/foot rows. All incident faces use the same new index.
pub(super) fn split_edges(
    mesh: &mut PlanetMesh,
    heights: &mut Vec<f64>,
    hardness: &mut Vec<f64>,
    soil: &mut Vec<f64>,
    fractions: BTreeMap<(u32, u32), f64>,
    displacement: Option<(u64, TerrainOptions)>,
) -> Result<(), PlanetError> {
    if mesh.directions.len() + fractions.len() > MAX_REFINED_VERTICES {
        return Err(PlanetError::InvalidOptions(
            "refined planet exceeds the two-million-vertex budget",
        ));
    }
    let mut splits = BTreeMap::new();
    for ((a, b), t) in fractions {
        let midpoint = u32::try_from(mesh.directions.len())
            .map_err(|_| PlanetError::InvalidTopology("too many vertices"))?;
        splits.insert((a, b), midpoint);
        let (a, b) = (a as usize, b as usize);
        let direction = mesh.directions[a].lerp(mesh.directions[b], t).normalize();
        let mut height = heights[a] + (heights[b] - heights[a]) * t;
        if let Some((seed, settings)) = displacement {
            let length = mesh.directions[a].distance(mesh.directions[b]);
            height += settings.relief * length * 0.35 * noise::fractal(seed, direction * 32.0, 3);
        }
        mesh.directions.push(direction);
        heights.push(height);
        hardness.push(hardness[a] + (hardness[b] - hardness[a]) * t);
        soil.push(soil[a] + (soil[b] - soil[a]) * t);
    }
    let mut triangles = Vec::with_capacity(mesh.triangles.len() * 4);
    for &[a, b, c] in &mesh.triangles {
        let ab = splits.get(&(a.min(b), a.max(b))).copied();
        let bc = splits.get(&(b.min(c), b.max(c))).copied();
        let ca = splits.get(&(c.min(a), c.max(a))).copied();
        match (ab, bc, ca) {
            (None, None, None) => triangles.push([a, b, c]),
            (Some(m), None, None) => triangles.extend([[a, m, c], [m, b, c]]),
            (None, Some(m), None) => triangles.extend([[b, m, a], [m, c, a]]),
            (None, None, Some(m)) => triangles.extend([[c, m, b], [m, a, b]]),
            (Some(m), Some(n), None) => triangles.extend([[b, n, m], [a, m, c], [m, n, c]]),
            (None, Some(m), Some(n)) => triangles.extend([[c, n, m], [b, m, a], [m, n, a]]),
            (Some(n), None, Some(m)) => triangles.extend([[a, n, m], [c, m, b], [m, n, b]]),
            (Some(m), Some(n), Some(p)) => {
                triangles.extend([[a, m, p], [m, b, n], [p, n, c], [m, n, p]]);
            }
        }
    }
    mesh.triangles = triangles;
    Ok(())
}

pub(super) fn smooth(graph: &SurfaceGraph, heights: &mut [f64], passes: u32) {
    let mut delta = vec![0.0; heights.len()];
    for _ in 0..passes {
        delta.fill(0.0);
        for (a, neighbours) in graph.neighbours.iter().enumerate() {
            for &b in neighbours.iter().filter(|&&b| b > a) {
                let capacity = (graph.areas[a] / count(neighbours.len()))
                    .min(graph.areas[b] / count(graph.neighbours[b].len()));
                let amount = (heights[b] - heights[a]) * capacity * 0.45;
                delta[a] += amount / graph.areas[a];
                delta[b] -= amount / graph.areas[b];
            }
        }
        for (height, delta) in heights.iter_mut().zip(&delta) {
            *height += delta;
        }
    }
}

#[allow(clippy::cast_precision_loss)]
fn count(value: usize) -> f64 {
    value as f64
}

pub(super) fn validate(mesh: &PlanetMesh) -> Result<(), PlanetError> {
    let count = mesh.directions.len();
    if !(4..=MAX_REFINED_VERTICES).contains(&count) || mesh.triangles.len() != 2 * count - 4 {
        return Err(PlanetError::InvalidTopology(
            "invalid refined sphere counts",
        ));
    }
    let mut edges: BTreeMap<(u32, u32), (usize, isize)> = BTreeMap::new();
    for &[a, b, c] in &mesh.triangles {
        if [a, b, c].iter().any(|&i| i as usize >= count) {
            return Err(PlanetError::InvalidTopology("refined triangle index"));
        }
        let [pa, pb, pc] = [a, b, c].map(|i| mesh.directions[i as usize]);
        if (pb - pa).cross(pc - pa).dot(pa + pb + pc) <= 0.0 {
            return Err(PlanetError::InvalidTopology("folded spherical face"));
        }
        for (a, b) in [(a, b), (b, c), (c, a)] {
            let entry = edges.entry((a.min(b), a.max(b))).or_default();
            entry.0 += 1;
            entry.1 += if a < b { 1 } else { -1 };
        }
    }
    if edges.len() != 3 * count - 6
        || edges.values().any(|&(n, w)| n != 2 || w != 0)
        || mesh
            .directions
            .iter()
            .any(|p| !p.is_finite() || (p.length_squared() - 1.0).abs() > 1.0e-10)
    {
        return Err(PlanetError::InvalidTopology(
            "refined mesh is not a closed oriented sphere",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{PlanetOptions, generate};

    #[test]
    fn selective_edge_splits_remain_closed_and_preserve_constant_fields() {
        for all in [false, true] {
            let mut mesh = generate(PlanetOptions {
                vertex_count: 32,
                ..PlanetOptions::default()
            })
            .unwrap();
            let mut heights = vec![0.1; 32];
            let mut hardness = vec![0.5; 32];
            let mut soil = vec![0.002; 32];
            let mut selected = vec![all; mesh.triangles.len()];
            selected[0] = true;
            let required = if all { 122 } else { 35 };
            assert!(subdivision_fits(&mesh, &selected, required));
            assert!(!subdivision_fits(&mesh, &selected, required - 1));
            subdivide(
                &mut mesh,
                &mut heights,
                &mut hardness,
                &mut soil,
                &selected,
                None,
            )
            .unwrap();
            validate(&mesh).unwrap();
            assert_eq!(mesh.directions.len(), if all { 122 } else { 35 });
            assert!(heights.iter().all(|h| (*h - 0.1).abs() < f64::EPSILON));
        }
    }

    #[test]
    fn non_midpoint_crest_cuts_stay_closed_and_interpolate_radial_fields() {
        let mut mesh = generate(PlanetOptions {
            vertex_count: 32,
            ..PlanetOptions::default()
        })
        .unwrap();
        let [a, b, c] = mesh.triangles[0];
        let cuts: BTreeMap<_, _> = [(a, b, 0.2), (b, c, 0.65), (c, a, 0.85)]
            .into_iter()
            .map(|(a, b, t)| {
                if a < b {
                    ((a, b), t)
                } else {
                    ((b, a), 1.0 - t)
                }
            })
            .collect();
        let mut heights: Vec<_> = (0..32).map(|i| f64::from(i) * 0.001).collect();
        let mut hardness = vec![0.5; 32];
        let mut soil = vec![0.002; 32];
        let expected: Vec<_> = cuts
            .iter()
            .map(|(&(a, b), &t)| {
                let (a, b) = (a as usize, b as usize);
                (
                    mesh.directions[a].lerp(mesh.directions[b], t).normalize(),
                    heights[a] + (heights[b] - heights[a]) * t,
                )
            })
            .collect();
        split_edges(
            &mut mesh,
            &mut heights,
            &mut hardness,
            &mut soil,
            cuts,
            None,
        )
        .unwrap();
        validate(&mesh).unwrap();
        assert_eq!(mesh.directions.len(), 35);
        for (i, (direction, height)) in expected.into_iter().enumerate() {
            assert!(mesh.directions[32 + i].distance(direction) < 1.0e-12);
            assert!((heights[32 + i] - height).abs() < 1.0e-12);
        }
        assert!(hardness.iter().all(|&h| (h - 0.5).abs() < f64::EPSILON));
        assert!(soil.iter().all(|&s| (s - 0.002).abs() < f64::EPSILON));
    }

    #[test]
    fn smoothing_conserves_control_area_weighted_height_and_cannot_overshoot() {
        let mesh = generate(PlanetOptions {
            vertex_count: 128,
            ..PlanetOptions::default()
        })
        .unwrap();
        let graph = SurfaceGraph::new(&mesh);
        let mut heights: Vec<_> = mesh.directions.iter().map(|p| p.y * 0.1).collect();
        let mass = |h: &[f64]| h.iter().zip(&graph.areas).map(|(h, a)| h * a).sum::<f64>();
        let before = mass(&heights);
        smooth(&graph, &mut heights, 8);
        assert!((mass(&heights) - before).abs() < 1.0e-12);
        assert!(heights.iter().all(|h| h.abs() <= 0.1));
    }

    #[test]
    fn positional_relaxation_keeps_closed_sphere_and_material_budgets() {
        let mut mesh = generate(PlanetOptions {
            vertex_count: 128,
            ..PlanetOptions::default()
        })
        .unwrap();
        let graph = SurfaceGraph::new(&mesh);
        let mut heights: Vec<_> = mesh.directions.iter().map(|p| 0.05 * p.y).collect();
        let mut soil = vec![0.002; heights.len()];
        let before_height = mass(&heights, &graph.areas);
        let before_soil = mass(&soil, &graph.areas);
        let first = mesh.directions[0];
        relax(&mut mesh, &mut heights, &mut soil, 8);
        validate(&mesh).unwrap();
        let graph = SurfaceGraph::new(&mesh);
        assert!(mesh.directions[0].distance(first) > 0.001);
        assert!((mass(&heights, &graph.areas) - before_height).abs() < 1.0e-12);
        assert!((mass(&soil, &graph.areas) - before_soil).abs() < 1.0e-12);
        assert!(soil.iter().all(|&depth| depth >= 0.0));
    }

    #[test]
    fn adaptive_detail_selects_relief_but_not_the_curvature_of_the_sphere() {
        let mesh = generate(PlanetOptions {
            vertex_count: 128,
            ..PlanetOptions::default()
        })
        .unwrap();
        let mut heights = vec![0.02; mesh.directions.len()];
        assert!(
            curved_faces(&mesh, &heights)
                .iter()
                .all(|&selected| !selected)
        );
        heights[0] += 0.1;
        let selected = curved_faces(&mesh, &heights);
        assert!(selected.iter().any(|&selected| selected));
        assert!(selected.iter().any(|&selected| !selected));
    }
}
