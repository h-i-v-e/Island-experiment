//! Conforming spherical edge splits and conservative radial smoothing.
use super::{TerrainOptions, TerrainSource, graph::SurfaceGraph};
use crate::{DVec3, PlanetError, PlanetMesh, noise};
use std::collections::BTreeMap;

/// Source geometry is borrowed through the terrain stages. Resampling adds
/// only the source interpolation residual, preserving previous erosion edits.
#[derive(Clone, Copy)]
pub(super) struct Displacement<'a> {
    pub seed: u64,
    pub settings: &'a TerrainOptions,
    pub tectonics: Option<&'a super::tectonics::TectonicField>,
    pub impacts: Option<&'a super::impacts::ImpactField>,
    pub broad_noise: bool,
}

impl Displacement<'_> {
    fn midpoint(
        &self,
        edge: [DVec3; 2],
        point: DVec3,
        t: f64,
        mut height: f64,
        hardness: f64,
    ) -> (f64, f64) {
        if let Some(impacts) = self.impacts {
            let a = impacts.sample(edge[0]);
            height += impacts.sample(point) - (a + (impacts.sample(edge[1]) - a) * t);
        }
        if let Some(field) = self.tectonics {
            let a = field.initial_sample(edge[0], self.seed, self.settings);
            let b = field.initial_sample(edge[1], self.seed, self.settings);
            let new = field.initial_sample(point, self.seed, self.settings);
            let residual = new.height - (a.height + (b.height - a.height) * t);
            return (height + self.settings.relief * residual, new.hardness);
        }
        let detail =
            if !self.broad_noise || !self.settings.noise_layers.iter().any(|l| l.weight > 0.0) {
                0.0
            } else if self.settings.source == TerrainSource::Tectonics {
                self.settings.tectonics.detail
            } else {
                1.0
            };
        (
            height
                + self.settings.relief
                    * edge[0].distance(edge[1])
                    * 0.35
                    * detail
                    * noise::fractal(self.seed, point * 32.0, 3),
            hardness,
        )
    }
}

// Angular spacing retains the generator's radius-scaling contract: this is
// 0.2 metres on the default 1 km planet, formerly 2 metres. It is an adaptive
// stopping criterion, not a demand for a uniform global grid at this spacing.
const MIN_ADAPTIVE_EDGE_ANGLE: f64 = 0.0002;

// The ocean's seabed visibility fades from 6 to 9.5 metres. Keep a small
// margin at the default 1 km radius, preserving terrain's radius scaling.
const SEABED_DETAIL_DEPTH: f64 = 0.01;
const SEABED_SMOOTH_FULL_DEPTH: f64 = 0.006;

fn observable_face(face: &[u32; 3], heights: &[f64]) -> bool {
    face.iter()
        .any(|&i| heights[i as usize] >= -SEABED_DETAIL_DEPTH)
}

/// Shallowest-corner selection preserves shore-crossing triangles. Shared edge
/// splits may extend into their deep neighbours to keep the sphere watertight.
pub(super) fn surface_faces(mesh: &PlanetMesh, heights: &[f64]) -> Vec<bool> {
    mesh.triangles
        .iter()
        .map(|face| observable_face(face, heights))
        .collect()
}

pub(super) fn retain_surface_faces(mesh: &PlanetMesh, heights: &[f64], selected: &mut [bool]) {
    for (face, selected) in mesh.triangles.iter().zip(selected) {
        *selected &= observable_face(face, heights);
    }
}

/// Resolve locally visible procedural features even when their initially
/// sampled corners lie in deep water. Unrelated deep seabed remains coarse.
pub(super) fn feature_faces(mesh: &PlanetMesh, features: &[(DVec3, f64)], selected: &mut [bool]) {
    if features.is_empty() {
        return;
    }
    let index = super::feature_index::FeatureIndex::new(features.iter().copied());
    for (face, selected) in mesh.triangles.iter().zip(selected) {
        if *selected {
            continue;
        }
        let points = face.map(|i| mesh.directions[i as usize]);
        let centre = (points[0] + points[1] + points[2]).normalize_or_zero();
        let bound = if points.iter().any(|p| p.dot(centre) <= 0.0) {
            2.0
        } else {
            points
                .iter()
                .map(|p| p.distance(centre))
                .fold(0.0, f64::max)
        };
        *selected = index.any_near(centre, bound, |id| {
            let (p, r) = features[id];
            bound > r * 0.12 && centre.distance(p) <= bound + 2.0 * (r * 0.5).sin()
        });
    }
}

fn smoothing_weight(height: f64) -> f64 {
    let t = ((height + SEABED_DETAIL_DEPTH) / (SEABED_DETAIL_DEPTH - SEABED_SMOOTH_FULL_DEPTH))
        .clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
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
            length > MIN_ADAPTIVE_EDGE_ANGLE
                && (heights[i] - height / count(neighbours.len())).abs() > length * 0.025
        })
        .collect();
    mesh.triangles
        .iter()
        .map(|face| observable_face(face, heights) && face.iter().any(|&i| marked[i as usize]))
        .collect()
}

/// Smooth positions in the local tangent plane as well as radial heights.
/// A per-face orientation bound prevents relaxation folding skinny triangles.
/// Reprojecting directions preserves sea radius instead of shrinking the sphere.
pub(super) fn relax(mesh: &mut PlanetMesh, heights: &mut [f64], soil: &mut [f64], passes: u32) {
    if passes == 0 {
        return;
    }
    let weights: Vec<_> = heights.iter().map(|&h| smoothing_weight(h)).collect();
    if !weights.iter().any(|&w| w > 0.0) {
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
        let soil_mass = active_mass(soil, &graph.areas, &weights);
        for (i, neighbours) in graph.neighbours.iter().enumerate() {
            let direction = mesh.directions[i];
            // No moving neighbour of a frozen site: its incident areas and
            // material budgets must remain unchanged as well as its position.
            if weights[i] == 0.0 || neighbours.iter().any(|&j| weights[j] == 0.0) {
                targets[i] = direction;
                continue;
            }
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
            targets[i] = (direction
                + (tangent * (0.65 * weights[i])).clamp_length_max(shortest * 0.2))
            .normalize();
        }
        for (i, &target) in targets.iter().enumerate() {
            let original = mesh.directions[i];
            if target == original {
                continue;
            }
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
        let active_area: f64 = graph.areas.iter().zip(&weights).map(|(a, w)| a * w).sum();
        let correction = (height_mass - mass(heights, &graph.areas)) / active_area;
        for (height, &weight) in heights.iter_mut().zip(&weights) {
            *height += correction * weight;
        }
        let new_soil_mass = active_mass(soil, &graph.areas, &weights);
        if new_soil_mass > 0.0 {
            for (depth, &weight) in soil.iter_mut().zip(&weights) {
                if weight > 0.0 {
                    *depth *= soil_mass / new_soil_mass;
                }
            }
        }
        smooth(&graph, heights, 1);
    }
}

fn active_mass(values: &[f64], areas: &[f64], weights: &[f64]) -> f64 {
    values
        .iter()
        .zip(areas)
        .zip(weights)
        .filter(|(_, w)| **w > 0.0)
        .map(|((v, a), _)| v * a)
        .sum()
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
    displacement: Option<Displacement<'_>>,
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
    displacement: Option<Displacement<'_>>,
) -> Result<(), PlanetError> {
    let count = mesh
        .directions
        .len()
        .checked_add(fractions.len())
        .ok_or(PlanetError::InvalidOptions("refined vertex count overflow"))?;
    u32::try_from(count)
        .map_err(|_| PlanetError::InvalidOptions("refined mesh exceeds the u32 index format"))?;
    mesh.directions
        .try_reserve(fractions.len())
        .map_err(|_| PlanetError::InvalidOptions("unable to allocate refined directions"))?;
    for field in [&mut *heights, &mut *hardness, &mut *soil] {
        field.try_reserve(fractions.len()).map_err(|_| {
            PlanetError::InvalidOptions("unable to allocate refined terrain fields")
        })?;
    }
    // Each split introduces at most two faces in a manifold mesh. Reserve for
    // the actual selected edges, not four times every face in an adaptive pass.
    let face_capacity = fractions
        .len()
        .checked_mul(2)
        .and_then(|extra| mesh.triangles.len().checked_add(extra))
        .ok_or(PlanetError::InvalidOptions("refined face count overflow"))?;
    let mut triangles = Vec::new();
    triangles
        .try_reserve(face_capacity)
        .map_err(|_| PlanetError::InvalidOptions("unable to allocate refined faces"))?;
    let displacement = displacement.filter(|d| {
        d.tectonics.is_some()
            || d.impacts.is_some()
            || d.settings
                .noise_layers
                .iter()
                .any(|layer| layer.weight > 0.0)
    });
    let mut splits = BTreeMap::new();
    for ((a, b), t) in fractions {
        let midpoint = u32::try_from(mesh.directions.len())
            .map_err(|_| PlanetError::InvalidTopology("too many vertices"))?;
        splits.insert((a, b), midpoint);
        let (a, b) = (a as usize, b as usize);
        let direction = mesh.directions[a].lerp(mesh.directions[b], t).normalize();
        let height = heights[a] + (heights[b] - heights[a]) * t;
        let rock = hardness[a] + (hardness[b] - hardness[a]) * t;
        let (height, rock) = displacement.map_or((height, rock), |d| {
            d.midpoint(
                [mesh.directions[a], mesh.directions[b]],
                direction,
                t,
                height,
                rock,
            )
        });
        mesh.directions.push(direction);
        heights.push(height);
        hardness.push(rock);
        soil.push(soil[a] + (soil[b] - soil[a]) * t);
    }
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
    if passes == 0 {
        return;
    }
    let weights: Vec<_> = heights.iter().map(|&h| smoothing_weight(h)).collect();
    if !weights.iter().any(|&w| w > 0.0) {
        return;
    }
    let mut delta = vec![0.0; heights.len()];
    for _ in 0..passes {
        delta.fill(0.0);
        for (a, neighbours) in graph.neighbours.iter().enumerate() {
            for &b in neighbours.iter().filter(|&&b| b > a) {
                if weights[a] == 0.0 && weights[b] == 0.0 {
                    continue;
                }
                let capacity = (graph.areas[a] / count(neighbours.len()))
                    .min(graph.areas[b] / count(graph.neighbours[b].len()));
                let amount = (heights[b] - heights[a]) * capacity * 0.45;
                // Frozen seabed remains a boundary sample. Removing its
                // contribution makes coastal smoothing one-sided and fills sea.
                delta[a] += amount / graph.areas[a] * weights[a];
                delta[b] -= amount / graph.areas[b] * weights[b];
            }
        }
        // Only the visible sites move, so boundary exchange is not symmetric.
        // Balance cuts and fills by reducing the larger total. This conserves
        // material without raising unrelated sites or exceeding a local target;
        // the depth fade and frozen deep vertices remain intact.
        let (cuts, fills) = delta
            .iter()
            .zip(&graph.areas)
            .fold((0.0, 0.0), |(c, f), (d, a)| {
                (c + (-d).max(0.0) * a, f + d.max(0.0) * a)
            });
        let cut_scale = if cuts > 0.0 {
            (fills / cuts).min(1.0)
        } else {
            0.0
        };
        let fill_scale = if fills > 0.0 {
            (cuts / fills).min(1.0)
        } else {
            0.0
        };
        for (height, delta) in heights.iter_mut().zip(&delta) {
            *height += delta * if *delta < 0.0 { cut_scale } else { fill_scale };
        }
    }
}

#[allow(clippy::cast_precision_loss)]
fn count(value: usize) -> f64 {
    value as f64
}

pub(super) fn validate(mesh: &PlanetMesh) -> Result<(), PlanetError> {
    let count = mesh.directions.len();
    if count < 4
        || u32::try_from(count).is_err()
        || count.checked_mul(2).and_then(|n| n.checked_sub(4)) != Some(mesh.triangles.len())
    {
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

    #[test]
    fn impact_midpoint_residual_preserves_an_existing_erosion_cut() {
        let mut mesh = generate(PlanetOptions {
            vertex_count: 128,
            ..PlanetOptions::default()
        })
        .unwrap();
        let settings = TerrainOptions {
            relief: 0.0,
            ..TerrainOptions::default()
        };
        let mut heights = vec![0.04; mesh.directions.len()];
        let mut field = super::super::impacts::ImpactField::new(
            42,
            super::super::ImpactOptions {
                enabled: true,
                ..Default::default()
            },
            heights.len(),
        );
        field.add_stage(&mesh, &mut heights, 0);
        for height in &mut heights {
            *height -= 0.003;
        }
        let mut hardness = vec![0.5; heights.len()];
        let mut soil = vec![0.002; heights.len()];
        let selected = vec![true; mesh.triangles.len()];
        subdivide(
            &mut mesh,
            &mut heights,
            &mut hardness,
            &mut soil,
            &selected,
            Some(Displacement {
                seed: 42,
                settings: &settings,
                tectonics: None,
                impacts: Some(&field),
                broad_noise: false,
            }),
        )
        .unwrap();
        for (&p, &h) in mesh.directions.iter().zip(&heights) {
            assert!((h - (0.037 + field.sample(p))).abs() < 1.0e-12);
        }
        validate(&mesh).unwrap();
    }

    #[test]
    fn feature_refinement_resolves_emergent_rims_without_refining_other_deep_seabed() {
        let mesh = generate(PlanetOptions {
            vertex_count: 1024,
            ..PlanetOptions::default()
        })
        .unwrap();
        let heights = vec![-0.05; mesh.directions.len()];
        let mut selected = surface_faces(&mesh, &heights);
        let face = mesh.triangles[0].map(|i| mesh.directions[i as usize]);
        let centre = (face[0] + face[1] + face[2]).normalize();
        let radius = 0.01;
        feature_faces(&mesh, &[(centre, radius)], &mut selected);
        assert!(
            selected[0],
            "all corners missed an observable interior feature"
        );
        let count = selected.iter().filter(|&&v| v).count();
        assert!(count > 0 && count < mesh.triangles.len() / 10);
        for (f, &selected) in mesh.triangles.iter().zip(&selected) {
            if f.iter()
                .all(|&i| mesh.directions[i as usize].dot(centre) < 0.0)
            {
                assert!(!selected);
            }
        }
    }

    #[test]
    fn tectonic_refinement_resolves_source_belts_and_preserves_existing_incision() {
        let mut mesh = generate(PlanetOptions {
            vertex_count: 128,
            ..PlanetOptions::default()
        })
        .unwrap();
        let settings = TerrainOptions {
            source: TerrainSource::Tectonics,
            tectonics: super::super::TectonicOptions {
                detail: 0.0,
                ..super::super::TectonicOptions::default()
            },
            ..TerrainOptions::default()
        };
        let field = super::super::tectonics::TectonicField::new(42, settings.tectonics);
        let incision = 0.003;
        let mut heights: Vec<_> = mesh
            .directions
            .iter()
            .map(|&p| {
                settings.land_bias + settings.relief * field.initial_sample(p, 42, &settings).height
                    - incision
            })
            .collect();
        let mut hardness: Vec<_> = mesh
            .directions
            .iter()
            .map(|&p| field.sample(p).hardness)
            .collect();
        let mut soil = vec![0.002; heights.len()];
        let first = heights.len();
        let selected = vec![true; mesh.triangles.len()];
        subdivide(
            &mut mesh,
            &mut heights,
            &mut hardness,
            &mut soil,
            &selected,
            Some(Displacement {
                seed: 42,
                settings: &settings,
                tectonics: Some(&field),
                impacts: None,
                broad_noise: false,
            }),
        )
        .unwrap();
        for i in first..heights.len() {
            let expected = field.initial_sample(mesh.directions[i], 42, &settings);
            assert!(
                (heights[i] - (settings.land_bias + settings.relief * expected.height - incision))
                    .abs()
                    < 1.0e-12
            );
            assert!((hardness[i] - expected.hardness).abs() < 1.0e-12);
        }
        validate(&mesh).unwrap();
    }
    use crate::{PlanetOptions, generate};

    #[test]
    fn all_requested_subdivisions_complete_beyond_the_former_two_million_limit() {
        let mut mesh = generate(PlanetOptions {
            vertex_count: 512,
            ..PlanetOptions::default()
        })
        .unwrap();
        let mut heights = vec![0.01; 512];
        let mut hardness = vec![0.5; 512];
        let mut soil = vec![0.002; 512];
        for _ in 0..6 {
            let selected = vec![true; mesh.triangles.len()];
            subdivide(
                &mut mesh,
                &mut heights,
                &mut hardness,
                &mut soil,
                &selected,
                None,
            )
            .unwrap();
        }
        assert_eq!(mesh.directions.len(), 2_088_962);
        assert_eq!(mesh.triangles.len(), 4_177_920);
        assert_eq!(heights.len(), mesh.directions.len());
        assert!(heights.iter().all(|&h| (h - 0.01).abs() < f64::EPSILON));
        validate(&mesh).unwrap();
    }

    #[test]
    fn adaptive_detail_resolves_sub_two_metre_features_but_stops_below_twenty_centimetres() {
        let patch = |spacing: f64| {
            let directions = (-1..=1)
                .flat_map(|y| {
                    (-1..=1).map(move |x| {
                        DVec3::new(f64::from(x) * spacing, f64::from(y) * spacing, 1.0).normalize()
                    })
                })
                .collect();
            let triangles = (0..2)
                .flat_map(|y| {
                    (0..2).flat_map(move |x| {
                        let a = y * 3 + x;
                        [[a, a + 1, a + 3], [a + 1, a + 4, a + 3]]
                    })
                })
                .collect();
            PlanetMesh {
                directions,
                triangles,
                radius: 1000.0,
            }
        };
        let mut heights = vec![0.02; 9];
        assert!(curved_faces(&patch(0.0005), &heights).iter().all(|&v| !v));
        heights[4] += 0.0001;
        assert!(
            curved_faces(&patch(0.0005), &heights).iter().any(|&v| v),
            "half-metre relief must no longer be discarded by a two-metre cutoff"
        );
        assert!(
            curved_faces(&patch(0.00005), &heights).iter().all(|&v| !v),
            "already sub-20cm edges should not be selected again"
        );
    }

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
    fn fixed_deep_neighbours_still_anchor_a_coastal_slope_instead_of_filling_it() {
        let graph = SurfaceGraph {
            neighbours: vec![vec![1], vec![0, 2], vec![1, 3], vec![2, 4], vec![3]],
            areas: vec![1.0; 5],
        };
        let mut heights = vec![-0.042, -0.022, -0.002, 0.018, 0.038];
        let before = mass(&heights, &graph.areas);
        smooth(&graph, &mut heights, 8);
        assert_eq!(heights[0].to_bits(), (-0.042_f64).to_bits());
        assert_eq!(heights[1].to_bits(), (-0.022_f64).to_bits());
        assert!(
            (heights[2] + 0.002).abs() < 1.0e-12,
            "linear coastal slope was raised: {heights:?}"
        );
        assert!((mass(&heights, &graph.areas) - before).abs() < 1.0e-12);
    }

    #[test]
    fn positional_relaxation_keeps_closed_sphere_and_material_budgets() {
        let mut mesh = generate(PlanetOptions {
            vertex_count: 128,
            ..PlanetOptions::default()
        })
        .unwrap();
        let graph = SurfaceGraph::new(&mesh);
        let mut heights: Vec<_> = mesh.directions.iter().map(|p| 0.06 + 0.05 * p.y).collect();
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

    #[test]
    fn deep_seabed_is_not_selected_but_shore_crossing_faces_stay_conforming() {
        let mut mesh = generate(PlanetOptions {
            vertex_count: 128,
            ..PlanetOptions::default()
        })
        .unwrap();
        let mut heights: Vec<_> = mesh.directions.iter().map(|p| -0.08 + p.y * 0.02).collect();
        assert!(surface_faces(&mesh, &heights).iter().all(|&s| !s));
        assert!(curved_faces(&mesh, &heights).iter().all(|&s| !s));
        // One observable corner must retain its entire triangle, even when its
        // centre and the other two corners are far below the visibility depth.
        heights[0] = -0.005;
        let selected = surface_faces(&mesh, &heights);
        for (face, &selected) in mesh.triangles.iter().zip(&selected) {
            assert_eq!(selected, face.contains(&0));
        }
        let mut corridor = vec![true; mesh.triangles.len()];
        retain_surface_faces(&mesh, &heights, &mut corridor);
        assert_eq!(corridor, selected);
        let original_vertices = mesh.directions.len();
        let mut hardness = vec![0.5; original_vertices];
        let mut soil = vec![0.002; original_vertices];
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
        assert!(mesh.directions.len() > original_vertices);
        assert!(mesh.directions.len() < original_vertices * 2);
    }

    #[test]
    fn deep_seabed_does_not_move_or_smooth_while_surface_material_is_conserved() {
        let mut mesh = generate(PlanetOptions {
            vertex_count: 512,
            ..PlanetOptions::default()
        })
        .unwrap();
        let mut heights: Vec<_> = mesh.directions.iter().map(|p| p.y * 0.05).collect();
        let mut soil: Vec<_> = mesh
            .directions
            .iter()
            .map(|p| 0.002 + p.x * 0.001)
            .collect();
        let before_directions = mesh.directions.clone();
        let before_heights = heights.clone();
        let before_soil = soil.clone();
        let graph = SurfaceGraph::new(&mesh);
        let before_height_mass = mass(&heights, &graph.areas);
        let before_soil_mass = mass(&soil, &graph.areas);
        relax(&mut mesh, &mut heights, &mut soil, 8);
        let graph = SurfaceGraph::new(&mesh);
        smooth(&graph, &mut heights, 8);
        let mut deep_count = 0;
        let mut moved_count = 0;
        for i in 0..heights.len() {
            if before_heights[i] <= -SEABED_DETAIL_DEPTH {
                deep_count += 1;
                assert_eq!(mesh.directions[i], before_directions[i]);
                assert_eq!(heights[i].to_bits(), before_heights[i].to_bits());
                assert_eq!(soil[i].to_bits(), before_soil[i].to_bits());
            } else if mesh.directions[i].distance(before_directions[i]) > 1.0e-6 {
                moved_count += 1;
            }
        }
        assert!(deep_count > 100 && moved_count > 100);
        assert!((mass(&heights, &graph.areas) - before_height_mass).abs() < 1.0e-12);
        assert!((mass(&soil, &graph.areas) - before_soil_mass).abs() < 1.0e-12);
        assert!(soil.iter().all(|&depth| depth >= 0.0));
        validate(&mesh).unwrap();
        assert!(smoothing_weight(-0.01).abs() < f64::EPSILON);
        assert!((smoothing_weight(-0.008) - 0.5).abs() < 1.0e-12);
        assert!((smoothing_weight(-0.006) - 1.0).abs() < f64::EPSILON);
    }
}
