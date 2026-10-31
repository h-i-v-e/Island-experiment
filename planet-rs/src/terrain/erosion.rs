//! Surface transport uses radial height, angular slope, and solid-angle control
//! areas. No sea/land clamping or island boundary masks are applied.
use super::{
    TerrainOptions,
    graph::{SurfaceGraph, arc},
};
use crate::DVec3;

pub(super) fn erode(
    directions: &[DVec3],
    graph: &SurfaceGraph,
    heights: &mut [f64],
    hardness: &[f64],
    soil: &mut [f64],
    options: &TerrainOptions,
) {
    if options.erosion_strength > 0.0 && options.hydraulic_passes > 0 {
        HydraulicPaths::new(directions, graph, heights, hardness, soil, options).run();
    }
    thermal(directions, graph, heights, soil, options.thermal_passes);
}

/// Spherical adaptation of Island's sequential reference paths. Each path sees
/// the changes made by previous paths: converging flows develop actual gullies
/// rather than applying a single simultaneous height delta. Sediment is carried
/// as area-weighted material so changing triangle sizes cannot create material.
struct HydraulicPaths<'a> {
    graph: &'a SurfaceGraph,
    heights: &'a mut [f64],
    hardness: &'a [f64],
    soil: &'a mut [f64],
    edges: Vec<Vec<(usize, f64)>>,
    options: &'a TerrainOptions,
    /// Fixed for the whole stage, including every source and hydraulic sweep.
    /// Restrict added slope as well as height on small, irregular control cells.
    bounds: Vec<(f64, f64)>,
    terminal_sediment: Vec<f64>,
}

impl<'a> HydraulicPaths<'a> {
    fn new(
        directions: &[DVec3],
        graph: &'a SurfaceGraph,
        heights: &'a mut [f64],
        hardness: &'a [f64],
        soil: &'a mut [f64],
        options: &'a TerrainOptions,
    ) -> Self {
        let edges: Vec<Vec<_>> = graph
            .neighbours
            .iter()
            .enumerate()
            .map(|(i, neighbours)| {
                neighbours
                    .iter()
                    .map(|&j| (j, arc(directions[i], directions[j])))
                    .collect()
            })
            .collect();
        // Strength and sweeps are cumulative erosion-time controls. Let their
        // stage budget grow too; otherwise paths immediately saturate the same
        // small allowance and higher settings cannot deepen the terrain.
        let exposure = options.erosion_strength * f64::from(options.hydraulic_passes);
        let max_shift = heights.iter().copied().fold(0.0, f64::max) * 0.012 * exposure;
        let bounds = heights
            .iter()
            .zip(&edges)
            .map(|(&height, edges)| {
                let shortest = edges
                    .iter()
                    .map(|&(_, length)| length)
                    .fold(f64::INFINITY, f64::min);
                let shift = max_shift.min(shortest * 0.25);
                (height - shift, height + shift)
            })
            .collect();
        let terminal_sediment = vec![0.0; heights.len()];
        Self {
            graph,
            heights,
            hardness,
            soil,
            edges,
            options,
            bounds,
            terminal_sediment,
        }
    }

    fn run(&mut self) {
        let mut order: Vec<_> = (0..self.heights.len()).collect();
        let mut visited = vec![0; self.heights.len()];
        let mut path = 0;
        // Each sweep applies another full interval of rain/transport. Splitting
        // strength by the count made the UI's extra sweeps nearly cancel out.
        // All paths still share the same spacing-limited bounds for this stage.
        let strength = self.options.erosion_strength;
        for _ in 0..self.options.hydraulic_passes {
            order.sort_unstable_by(|&a, &b| {
                self.heights[b].total_cmp(&self.heights[a]).then(a.cmp(&b))
            });
            for &source in &order {
                if self.heights[source] <= 0.0 {
                    continue;
                }
                let parcel_area = self.graph.areas[source];
                let mut current = source;
                let mut speed = 0.0;
                let mut sediment = 0.0;
                path += 1;
                // A moving height field can redirect a later step uphill into a
                // former path. Bound travel, and settle any remaining sediment.
                for _ in 0..self.heights.len().min(512) {
                    if visited[current] == path {
                        break;
                    }
                    visited[current] = path;
                    let next = self.edges[current]
                        .iter()
                        .copied()
                        .filter(|&(j, _)| self.heights[j] < self.heights[current])
                        .min_by(|&(a, _), &(b, _)| {
                            self.heights[a].total_cmp(&self.heights[b]).then(a.cmp(&b))
                        });
                    let Some((target, length)) = next else {
                        break;
                    };
                    let drop = self.heights[current] - self.heights[target];
                    let distance = length.hypot(drop);
                    let slope = drop / length;
                    let sin_slope = drop / distance;
                    speed = speed * 0.75 + sin_slope.powi(3) * distance * 0.25;
                    let capacity = speed * parcel_area;
                    let deposition = deposition_weight(slope);
                    if sediment > capacity {
                        let requested = (sediment - capacity) * 0.525 * deposition;
                        sediment -= self.deposit(current, requested);
                    } else {
                        let slope_weight = 2.0 * slope / (1.0 + slope * slope);
                        let requested =
                            ((capacity - sediment) * strength * (1.0 - deposition) * slope_weight)
                                .min(
                                    (self.heights[current] - self.bounds[current].0).max(0.0)
                                        * self.graph.areas[current],
                                )
                                .min(drop * 0.2 * self.graph.areas[current]);
                        let loose = requested.min(self.soil[current] * self.graph.areas[current]);
                        let rock_rate = 0.08 + 0.92 * (1.0 - self.hardness[current]).powi(2);
                        let removed = loose + (requested - loose) * rock_rate;
                        self.heights[current] -= removed / self.graph.areas[current];
                        self.soil[current] =
                            (self.soil[current] - loose / self.graph.areas[current]).max(0.0);
                        sediment += removed;
                    }
                    current = target;
                    if self.heights[current] < 0.0 {
                        break;
                    }
                }
                self.terminal_sediment[current] += sediment;
            }
        }
        self.settle();
    }

    fn deposit(&mut self, i: usize, requested: f64) -> f64 {
        let volume = requested.min(self.deposit_capacity(i));
        let depth = volume / self.graph.areas[i];
        self.heights[i] += depth;
        self.soil[i] += depth;
        volume
    }

    fn deposit_capacity(&self, i: usize) -> f64 {
        (self.bounds[i].1 - self.heights[i]).max(0.0) * self.graph.areas[i]
    }

    fn settle(&mut self) {
        // Batch converging paths before building fans. A physical angular radius
        // keeps a fan from collapsing to one tiny ring as the mesh is refined.
        // Expand the footprint when full, carrying the excess instead of dumping
        // it at the first submerged vertex. Scratch buffers are reused per fan.
        let mut visited = vec![usize::MAX; self.heights.len()];
        let mut frontier = Vec::new();
        let mut next = Vec::new();
        let mut fan = Vec::new();
        for center in 0..self.heights.len() {
            let mut remaining = std::mem::take(&mut self.terminal_sediment[center]);
            if remaining <= 0.0 {
                continue;
            }
            frontier.clear();
            frontier.push((center, 0.0));
            visited[center] = center;
            let mut radius = 0.015;
            while remaining > 0.0 && !frontier.is_empty() {
                fan.clear();
                next.clear();
                let mut cursor = 0;
                while cursor < frontier.len() {
                    let (i, distance) = frontier[cursor];
                    cursor += 1;
                    if distance > radius {
                        next.push((i, distance));
                        continue;
                    }
                    fan.push(i);
                    for &(j, length) in &self.edges[i] {
                        if visited[j] != center {
                            visited[j] = center;
                            frontier.push((j, distance + length));
                        }
                    }
                }
                // Sea floor is preferred, but neither heights nor sea level are
                // clamped: sediment can still form a shallow delta or fill a lake.
                for submerged in [true, false] {
                    if remaining <= 0.0 {
                        break;
                    }
                    let capacity: f64 = fan
                        .iter()
                        .filter(|&&i| (self.heights[i] < 0.0) == submerged)
                        .map(|&i| self.deposit_capacity(i))
                        .sum();
                    if capacity <= 0.0 {
                        continue;
                    }
                    let fraction = (remaining / capacity).min(1.0);
                    let mut deposited = 0.0;
                    for &i in &fan {
                        if (self.heights[i] < 0.0) == submerged {
                            let requested = (self.deposit_capacity(i) * fraction)
                                .min((remaining - deposited).max(0.0));
                            deposited += self.deposit(i, requested);
                        }
                    }
                    // Do not traverse the whole planet for the final few ulps
                    // of a proportional distribution that fits in this fan.
                    remaining = if fraction < 1.0 {
                        0.0
                    } else {
                        (remaining - deposited).max(0.0)
                    };
                }
                std::mem::swap(&mut frontier, &mut next);
                radius *= 2.0;
            }
            // The symmetric stage bounds provide at least as much global fill
            // capacity as excavated material. Only roundoff can remain here.
            debug_assert!(remaining < 1.0e-12, "unsettled sediment: {remaining}");
        }
    }
}

fn deposition_weight(slope: f64) -> f64 {
    let low = 4.0_f64.to_radians().tan();
    let high = 12.0_f64.to_radians().tan();
    let t = ((slope - low) / (high - low)).clamp(0.0, 1.0);
    1.0 - t * t * (3.0 - 2.0 * t)
}

fn thermal(
    directions: &[DVec3],
    graph: &SurfaceGraph,
    heights: &mut [f64],
    soil: &mut [f64],
    passes: u32,
) {
    let mut delta = vec![0.0; heights.len()];
    let mut soil_delta = vec![0.0; heights.len()];
    for _ in 0..passes {
        delta.fill(0.0);
        soil_delta.fill(0.0);
        for i in 0..heights.len() {
            let Some(target) = graph.steepest(directions, heights, i) else {
                continue;
            };
            let excess =
                heights[i] - heights[target] - 0.30 * arc(directions[i], directions[target]);
            if excess <= 0.0 {
                continue;
            }
            let volume = excess
                * 0.12
                * (graph.areas[i] / degree(graph.neighbours[i].len()))
                    .min(graph.areas[target] / degree(graph.neighbours[target].len()));
            delta[i] -= volume / graph.areas[i];
            delta[target] += volume / graph.areas[target];
            soil_delta[i] -= (volume / graph.areas[i]).min(soil[i]);
            soil_delta[target] += volume / graph.areas[target];
        }
        for i in 0..heights.len() {
            heights[i] += delta[i];
            soil[i] = (soil[i] + soil_delta[i]).max(0.0);
        }
    }
}

#[allow(clippy::cast_precision_loss)]
fn degree(value: usize) -> f64 {
    value as f64
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{PlanetOptions, generate};

    #[test]
    fn hydraulic_strength_and_sweeps_have_cumulative_effect_with_bounded_conservative_transport() {
        let mesh = generate(PlanetOptions {
            vertex_count: 1024,
            ..PlanetOptions::default()
        })
        .unwrap();
        let graph = SurfaceGraph::new(&mesh);
        let original: Vec<_> = mesh
            .directions
            .iter()
            .map(|p| 0.18 * p.y + 0.025 * (p.x * 15.0).sin())
            .collect();
        let hardness = vec![0.4; original.len()];
        let material = |h: &[f64]| {
            h.iter()
                .zip(&graph.areas)
                .map(|(height, area)| height * area)
                .sum::<f64>()
        };
        let before = material(&original);
        let excavation = |strength, passes| {
            let mut heights = original.clone();
            let mut soil = vec![0.002; heights.len()];
            erode(
                &mesh.directions,
                &graph,
                &mut heights,
                &hardness,
                &mut soil,
                &TerrainOptions {
                    erosion_strength: strength,
                    hydraulic_passes: passes,
                    thermal_passes: 0,
                    ..TerrainOptions::default()
                },
            );
            assert!(
                (material(&heights) - before).abs() < 1e-12,
                "erosion lost sediment"
            );
            let mut cut = 0.0;
            for (i, (&height, &old)) in heights.iter().zip(&original).enumerate() {
                let shortest = graph.neighbours[i]
                    .iter()
                    .map(|&j| arc(mesh.directions[i], mesh.directions[j]))
                    .fold(f64::INFINITY, f64::min);
                assert!(
                    (height - old).abs() <= shortest * 0.25 + 1e-12,
                    "tiny control cell escaped its spike guard"
                );
                assert!(soil[i].is_finite() && soil[i] >= 0.0);
                cut += (old - height).max(0.0) * graph.areas[i];
            }
            cut
        };
        let normal = excavation(1.0, 1);
        let stronger = excavation(4.0, 1);
        let repeated = excavation(1.0, 8);
        let extreme = excavation(8.0, 1);
        assert!(
            extreme >= stronger,
            "strength eight should retain cumulative erosion while respecting the same guards"
        );
        assert!(normal > 1e-5);
        assert!(
            stronger > normal * 1.5,
            "strength still saturates at the default budget: {normal} -> {stronger}"
        );
        assert!(
            repeated > normal * 1.5,
            "additional sweeps still cancel their own strength: {normal} -> {repeated}"
        );
    }

    #[test]
    fn many_strong_paths_share_stage_limits_and_conserve_material() {
        let mesh = generate(PlanetOptions {
            vertex_count: 512,
            ..PlanetOptions::default()
        })
        .unwrap();
        let graph = SurfaceGraph::new(&mesh);
        let mut heights: Vec<_> = mesh
            .directions
            .iter()
            .map(|p| 0.18 * p.y + 0.025 * (p.x * 15.0).sin())
            .collect();
        let before: f64 = heights
            .iter()
            .zip(&graph.areas)
            .map(|(h, area)| h * area)
            .sum();
        let hardness = vec![0.2; heights.len()];
        let mut soil = vec![0.002; heights.len()];
        let options = TerrainOptions {
            erosion_strength: 4.0,
            hydraulic_passes: 16,
            thermal_passes: 0,
            ..TerrainOptions::default()
        };
        let mut paths = HydraulicPaths::new(
            &mesh.directions,
            &graph,
            &mut heights,
            &hardness,
            &mut soil,
            &options,
        );
        paths.run();
        for (&height, &(low, high)) in paths.heights.iter().zip(&paths.bounds) {
            assert!((low - 1.0e-14..=high + 1.0e-14).contains(&height));
        }
        let after: f64 = paths
            .heights
            .iter()
            .zip(&graph.areas)
            .map(|(h, area)| h * area)
            .sum();
        assert!((before - after).abs() < 1.0e-12);
        assert!(
            paths
                .soil
                .iter()
                .all(|&depth| depth.is_finite() && depth >= 0.0)
        );
    }

    #[test]
    fn converging_sediment_spreads_past_a_tiny_mouth_without_spikes_or_material_loss() {
        let mesh = generate(PlanetOptions {
            vertex_count: 128,
            ..PlanetOptions::default()
        })
        .unwrap();
        let mut graph = SurfaceGraph::new(&mesh);
        // Mimic a very small control cell in an adaptively refined coastline.
        graph.areas[0] = 1.0e-15;
        let mut heights = vec![-0.01; mesh.directions.len()];
        heights[127] = 0.02;
        let before = heights.clone();
        let hardness = vec![0.4; heights.len()];
        let mut soil = vec![0.0; heights.len()];
        let options = TerrainOptions::default();
        let mut paths = HydraulicPaths::new(
            &mesh.directions,
            &graph,
            &mut heights,
            &hardness,
            &mut soil,
            &options,
        );
        let volume = 0.001;
        paths.terminal_sediment[0] = volume;
        paths.settle();
        let mut deposited = 0.0;
        let mut distant_deposits = 0;
        for (i, &height) in paths.heights.iter().enumerate() {
            assert!(height <= paths.bounds[i].1 + 1.0e-14);
            assert!(height >= before[i]);
            deposited += (height - before[i]) * graph.areas[i];
            if i != 0 && !graph.neighbours[0].contains(&i) && height > before[i] {
                distant_deposits += 1;
            }
        }
        assert!((deposited - volume).abs() < 1.0e-12);
        assert!(
            distant_deposits > 10,
            "excess must travel past the first neighbour ring"
        );
        // Later paths cannot reset the per-stage budget at a saturated mouth.
        assert!(paths.deposit(0, 1.0) < 1.0e-25);
    }

    #[test]
    fn hydraulic_paths_excavate_and_deposit_without_thermal_or_river_carving() {
        let mesh = generate(PlanetOptions {
            vertex_count: 512,
            ..PlanetOptions::default()
        })
        .unwrap();
        let graph = SurfaceGraph::new(&mesh);
        let original: Vec<_> = mesh
            .directions
            .iter()
            .map(|p| 0.18 * p.y + 0.025 * (p.x * 15.0).sin())
            .collect();
        let mut heights = original.clone();
        let hardness = vec![0.4; heights.len()];
        let mut soil = vec![0.002; heights.len()];
        erode(
            &mesh.directions,
            &graph,
            &mut heights,
            &hardness,
            &mut soil,
            &TerrainOptions {
                thermal_passes: 0,
                ..TerrainOptions::default()
            },
        );
        let (excavated, deposited) = original.iter().zip(&heights).zip(&graph.areas).fold(
            (0.0, 0.0),
            |(cut, fill), ((before, after), area)| {
                (
                    cut + (before - after).max(0.0) * area,
                    fill + (after - before).max(0.0) * area,
                )
            },
        );
        eprintln!("Hydraulic-only material: excavated={excavated:.8}, deposited={deposited:.8}");
        assert!(
            excavated > 0.00001,
            "hydraulic erosion must visibly change heights independently of channel carving"
        );
        assert!((excavated - deposited).abs() < 1.0e-12);
        let mut untouched = original.clone();
        erode(
            &mesh.directions,
            &graph,
            &mut untouched,
            &hardness,
            &mut vec![0.002; heights.len()],
            &TerrainOptions {
                erosion_strength: 0.0,
                thermal_passes: 0,
                ..TerrainOptions::default()
            },
        );
        assert_eq!(original, untouched);
    }

    #[test]
    fn thermal_and_hydraulic_transport_conserve_area_weighted_material_across_sea() {
        let mesh = generate(PlanetOptions {
            vertex_count: 128,
            ..PlanetOptions::default()
        })
        .unwrap();
        let graph = SurfaceGraph::new(&mesh);
        let mut heights: Vec<_> = mesh.directions.iter().map(|p| 0.15 * p.y).collect();
        let material = |h: &[f64]| {
            h.iter()
                .zip(&graph.areas)
                .map(|(height, area)| height * area)
                .sum::<f64>()
        };
        let before = material(&heights);
        let original_sea: Vec<_> = heights.iter().map(|&h| h < 0.0).collect();
        let hardness = vec![0.5; heights.len()];
        let mut soil = vec![0.002; heights.len()];
        erode(
            &mesh.directions,
            &graph,
            &mut heights,
            &hardness,
            &mut soil,
            &TerrainOptions::default(),
        );
        assert!((before - material(&heights)).abs() < 1.0e-12);
        assert!(
            heights
                .iter()
                .zip(original_sea)
                .any(|(&h, was_sea)| was_sea && h > -0.15 && h < 0.0)
        );
        assert!(soil.iter().all(|v| v.is_finite() && *v >= 0.0));
    }
}
