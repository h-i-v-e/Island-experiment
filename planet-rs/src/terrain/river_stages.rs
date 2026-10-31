//! Intermediate river shaping, like Island's `form_deltas` passes. Rebuild the
//! drainage on each terrain stage, excavate without waterfall or mesh detail,
//! then carry that exact volume to alluvial banks and bounded coastal fans.
use super::{
    Drainage, TerrainOptions,
    channels::Footprints,
    drainage, erosion,
    graph::{SurfaceGraph, arc},
    river_widths,
};
use crate::{DVec3, PlanetMesh};

#[derive(Debug, Default)]
pub(super) struct Budget {
    pub excavated: f64,
    pub alluvial: f64,
    pub delta: f64,
    pub exported: f64,
}

/// All scratch belongs to this stage; topology and material arrays are borrowed.
/// No river positions, tessellation, waterfall supports or water meshes change.
#[allow(clippy::too_many_lines)] // One preparation pass followed by one downstream transport pass.
pub(super) fn carve(
    mesh: &PlanetMesh,
    graph: &SurfaceGraph,
    heights: &mut [f64],
    hardness: &[f64],
    soil: &mut [f64],
    settings: &TerrainOptions,
) -> Budget {
    if settings.river_depth == 0.0
        || !heights.iter().any(|&h| h < 0.0)
        || !heights.iter().any(|&h| h > 0.0)
    {
        return Budget::default();
    }
    let mut drainage = drainage::route(&mesh.directions, graph, heights);
    let mut cuts = vec![0.0; heights.len()];
    let ceilings: Vec<_> = heights
        .iter()
        .enumerate()
        .map(|(i, &h)| {
            let spacing = graph.neighbours[i]
                .iter()
                .map(|&j| arc(mesh.directions[i], mesh.directions[j]))
                .fold(f64::INFINITY, f64::min);
            h + (settings.river_depth * 2.0).min(spacing * 0.25)
        })
        .collect();
    // Include saddle excavation in the same sediment ledger as the corridor.
    let (rivers, _) = drainage::repair(
        &mesh.directions,
        graph,
        heights,
        soil,
        settings,
        &mut drainage,
        &mut cuts,
    );
    let mut sediment: Vec<_> = cuts
        .iter()
        .zip(&graph.areas)
        .map(|(&cut, &area)| cut * area)
        .collect();
    let mut protected: Vec<_> = cuts.iter().map(|&cut| cut > 0.0).collect();
    let widths = river_widths::build(mesh, &drainage, &rivers, settings);
    let threshold = settings.river_catchment * std::f64::consts::TAU * 2.0;
    let levels: Vec<_> = heights
        .iter()
        .zip(&drainage.catchment)
        .map(|(&h, &q)| {
            let depth = settings.river_depth * (q / threshold).sqrt().clamp(1.0, 3.0).sqrt();
            (h - depth * 0.35).max(0.0)
        })
        .collect();
    let footprints = Footprints::with_widths(mesh, &rivers, &levels, settings, 0.0, &widths);
    for (i, height) in heights.iter_mut().enumerate() {
        let (target, core) = footprints.bed_target(mesh.directions[i], *height);
        let requested = (*height - target).max(0.0);
        let loose = requested.min(soil[i]);
        let removed = loose + (requested - loose) * erosion::bedrock_rate(hardness[i]);
        *height -= removed;
        soil[i] = (soil[i] - loose).max(0.0);
        sediment[i] += removed * graph.areas[i];
        protected[i] |= core;
    }
    drop(footprints);
    drop(widths);
    cuts.fill(0.0);
    let (rivers, _) = drainage::repair(
        &mesh.directions,
        graph,
        heights,
        soil,
        settings,
        &mut drainage,
        &mut cuts,
    );
    for (i, &cut) in cuts.iter().enumerate() {
        sediment[i] += cut * graph.areas[i];
        protected[i] |= cut > 0.0;
    }
    drop(cuts);
    for river in &rivers {
        protected[river.from] = true;
        protected[river.to] = true;
    }
    let mut budget = Budget {
        excavated: sediment.iter().sum(),
        ..Budget::default()
    };
    let mut outlets: Vec<_> = (0..heights.len()).collect();
    for &i in &drainage.order {
        if let Some(to) = drainage.receivers[i] {
            outlets[i] = outlets[to];
        }
    }
    let mut alluvial = vec![0.0; heights.len()];
    for (i, &volume) in sediment.iter().enumerate() {
        alluvial[outlets[i]] += volume * 0.7;
    }
    for &i in drainage.order.iter().rev() {
        let Some(to) = drainage.receivers[i] else {
            continue;
        };
        if sediment[i] <= 0.0 {
            continue;
        }
        let slope = (heights[i] - heights[to]).max(0.0)
            / arc(mesh.directions[i], mesh.directions[to]).max(1e-12);
        let gentle = (1.0 - slope / 12.0_f64.to_radians().tan()).clamp(0.0, 1.0);
        let available = (sediment[i] * 0.35 * gentle * gentle).min(alluvial[outlets[i]].max(0.0));
        let bank_level = levels[i] + settings.river_depth * 0.5;
        let capacity = |j: usize, height: f64| {
            if protected[j] || height < 0.0 {
                0.0
            } else {
                (ceilings[j].min(bank_level) - height).max(0.0) * graph.areas[j]
            }
        };
        let total: f64 = graph.neighbours[i]
            .iter()
            .map(|&j| capacity(j, heights[j]))
            .sum();
        if total > 0.0 {
            let fraction = (available / total).min(1.0);
            for &j in &graph.neighbours[i] {
                let volume = (capacity(j, heights[j]) * fraction).min(sediment[i]);
                let depth = volume / graph.areas[j];
                heights[j] += depth;
                soil[j] += depth;
                sediment[i] -= volume;
                alluvial[outlets[i]] -= volume;
                budget.alluvial += volume;
            }
        }
        sediment[to] += sediment[i];
        sediment[i] = 0.0;
    }
    drop(outlets);
    drop(alluvial);
    drop(levels);
    let mut fans = Fans::new(mesh, graph, &protected, &ceilings);
    for (i, volume) in sediment.iter_mut().enumerate().filter(|(_, v)| **v > 0.0) {
        let deposited = fans.deposit(i, *volume, &drainage, heights, soil);
        budget.delta += deposited;
        *volume -= deposited;
    }
    budget.exported = sediment.iter().sum();
    debug_assert!(
        (budget.excavated - budget.alluvial - budget.delta - budget.exported).abs() < 1e-12
    );
    budget
}

struct Fans<'a> {
    mesh: &'a PlanetMesh,
    graph: &'a SurfaceGraph,
    protected: &'a [bool],
    ceilings: &'a [f64],
    visited: Vec<usize>,
    frontier: Vec<(usize, f64)>,
    next: Vec<(usize, f64)>,
    fan: Vec<(usize, f64)>,
}

impl<'a> Fans<'a> {
    fn new(
        mesh: &'a PlanetMesh,
        graph: &'a SurfaceGraph,
        protected: &'a [bool],
        ceilings: &'a [f64],
    ) -> Self {
        Self {
            mesh,
            graph,
            protected,
            ceilings,
            visited: vec![usize::MAX; ceilings.len()],
            frontier: Vec::new(),
            next: Vec::new(),
            fan: Vec::new(),
        }
    }

    fn capacity(&self, i: usize, height: f64) -> f64 {
        if self.protected[i] {
            0.0
        } else {
            (self.ceilings[i] - height).max(0.0) * self.graph.areas[i]
        }
    }

    #[allow(clippy::too_many_lines)] // Bounded breadth-first fan with shared scratch and two deposit preferences.
    fn deposit(
        &mut self,
        root: usize,
        volume: f64,
        drainage: &Drainage,
        heights: &mut [f64],
        soil: &mut [f64],
    ) -> f64 {
        let origin = self.mesh.directions[root];
        let incoming = self.graph.neighbours[root]
            .iter()
            .copied()
            .filter(|&j| drainage.receivers[j] == Some(root))
            .max_by(|&a, &b| drainage.catchment[a].total_cmp(&drainage.catchment[b]));
        let forward = incoming.map_or(DVec3::ZERO, |j| {
            let p = self.mesh.directions[j];
            (origin * origin.dot(p) - p).normalize_or_zero()
        });
        let spacing = self.graph.neighbours[root]
            .iter()
            .map(|&j| arc(origin, self.mesh.directions[j]))
            .fold(f64::INFINITY, f64::min);
        let maximum = (spacing * 18.0).clamp(0.03, 0.35);
        let mut radius: f64 = 0.015;
        let mut remaining = volume;
        self.frontier.clear();
        self.frontier.push((root, 0.0));
        self.visited[root] = root;
        while remaining > 0.0 && !self.frontier.is_empty() {
            self.next.clear();
            self.fan.clear();
            let mut cursor = 0;
            while cursor < self.frontier.len() {
                let (i, distance) = self.frontier[cursor];
                cursor += 1;
                if distance > radius {
                    self.next.push((i, distance));
                    continue;
                }
                let p = self.mesh.directions[i];
                let offset = (p - origin * origin.dot(p)).normalize_or_zero();
                let directional = 0.6 + 0.4 * forward.dot(offset);
                let weight = directional * (1.0 - 0.75 * (distance / maximum).min(1.0));
                self.fan.push((i, weight));
                for &j in &self.graph.neighbours[i] {
                    if self.visited[j] != root {
                        self.visited[j] = root;
                        let distance = arc(origin, self.mesh.directions[j]);
                        if distance <= maximum {
                            self.frontier.push((j, distance));
                        }
                    }
                }
            }
            // Prefer the sea floor, then a low alluvial coastal fan. Protected
            // beds/mouths stay open; per-cell ceilings prevent sediment spikes.
            for submerged in [true, false] {
                let capacity: f64 = self
                    .fan
                    .iter()
                    .filter(|&&(i, _)| (heights[i] < 0.0) == submerged)
                    .map(|&(i, weight)| self.capacity(i, heights[i]) * weight)
                    .sum();
                if capacity <= 0.0 || remaining <= 0.0 {
                    continue;
                }
                let fraction = (remaining / capacity).min(1.0);
                for &(i, weight) in &self.fan {
                    if (heights[i] < 0.0) == submerged {
                        let amount =
                            (self.capacity(i, heights[i]) * weight * fraction).min(remaining);
                        let depth = amount / self.graph.areas[i];
                        heights[i] += depth;
                        soil[i] += depth;
                        remaining = (remaining - amount).max(0.0);
                    }
                }
                if fraction < 1.0 {
                    remaining = 0.0;
                }
            }
            std::mem::swap(&mut self.frontier, &mut self.next);
            if radius >= maximum {
                break;
            }
            radius = (radius * 2.0).min(maximum);
        }
        // Material leaving this bounded fan is exported to the unresolved deep
        // ocean, just as Island exports the remainder of its delta budget.
        volume - remaining
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valley() -> (PlanetMesh, SurfaceGraph, Vec<f64>, TerrainOptions) {
        let mut mesh = PlanetMesh {
            directions: Vec::new(),
            triangles: Vec::new(),
            radius: 1000.0,
        };
        let mut heights = Vec::new();
        for row in 0..26 {
            let x = f64::from(row) * 0.002;
            for column in 0..17 {
                let y = f64::from(column - 8) * 0.002;
                mesh.directions.push(DVec3::new(x, y, 1.0).normalize());
                heights.push(0.0015 - x * 0.04 + y.abs() * 0.04);
            }
        }
        for row in 0..25 {
            for column in 0..16 {
                let i = row * 17 + column;
                mesh.triangles
                    .extend([[i, i + 17, i + 1], [i + 1, i + 17, i + 18]]);
            }
        }
        let graph = SurfaceGraph::new(&mesh);
        (
            mesh,
            graph,
            heights,
            TerrainOptions {
                river_depth: 0.0001,
                river_width: 0.0005,
                river_catchment: 0.000_006,
                river_taper: 0.0,
                hydraulic_passes: 0,
                thermal_passes: 0,
                ..TerrainOptions::default()
            },
        )
    }

    fn material(graph: &SurfaceGraph, heights: &[f64]) -> f64 {
        heights
            .iter()
            .zip(&graph.areas)
            .map(|(h, area)| h * area)
            .sum()
    }

    #[test]
    fn repeated_stages_excavate_and_form_alluvium_and_deltas_without_changing_topology() {
        let (mesh, graph, mut heights, settings) = valley();
        let counts = (mesh.directions.len(), mesh.triangles.len());
        let mut soil = vec![0.00002; heights.len()];
        let hardness = vec![0.35; heights.len()];
        for _ in 0..3 {
            let before = material(&graph, &heights);
            let budget = carve(&mesh, &graph, &mut heights, &hardness, &mut soil, &settings);
            assert!(
                budget.excavated > 1e-10,
                "stage skipped excavation: {budget:?}"
            );
            assert!(
                budget.alluvial > 0.0,
                "gentle valley banks need deposition: {budget:?}"
            );
            assert!(
                budget.delta > 0.0,
                "excavation must feed a coastal fan: {budget:?}"
            );
            assert!((before - material(&graph, &heights) - budget.exported).abs() < 1e-14);
            assert!(
                (budget.excavated - budget.alluvial - budget.delta - budget.exported).abs() < 1e-14
            );
            assert!(soil.iter().all(|&s| s.is_finite() && s >= 0.0));
            assert_eq!(
                (mesh.directions.len(), mesh.triangles.len()),
                counts,
                "stages cannot tessellate or move bank positions"
            );
        }
    }

    #[test]
    fn hard_rock_resists_stage_channel_excavation_and_zero_depth_or_no_sea_skips_it() {
        let (mesh, graph, original, settings) = valley();
        let excavation = |hardness| {
            let mut heights = original.clone();
            carve(
                &mesh,
                &graph,
                &mut heights,
                &vec![hardness; original.len()],
                &mut vec![0.0; original.len()],
                &settings,
            )
            .excavated
        };
        assert!(excavation(0.0) > excavation(1.0) * 1.2);
        for (height, depth) in [
            (0.1, settings.river_depth),
            (-0.1, settings.river_depth),
            (0.01, 0.0),
        ] {
            let mut heights = vec![height; original.len()];
            let mut soil = vec![0.002; original.len()];
            let budget = carve(
                &mesh,
                &graph,
                &mut heights,
                &vec![0.5; original.len()],
                &mut soil,
                &TerrainOptions {
                    river_depth: depth,
                    ..settings
                },
            );
            assert_eq!(budget.excavated.to_bits(), 0.0_f64.to_bits());
            assert!(heights.iter().all(|h| h.to_bits() == height.to_bits()));
            assert!(soil.iter().all(|s| s.to_bits() == 0.002_f64.to_bits()));
        }
    }

    #[test]
    fn delta_fans_keep_mouths_open_and_bound_fill_even_on_tiny_cells() {
        let (mesh, mut graph, mut heights, _) = valley();
        let root = 22 * 17 + 8;
        let mut protected = vec![false; heights.len()];
        protected[root] = true;
        graph.areas[root + 1] *= 1e-8;
        let original = heights.clone();
        let ceilings: Vec<_> = heights.iter().map(|h| h + 0.00005).collect();
        let routing = drainage::route(&mesh.directions, &graph, &heights);
        let mut soil = vec![0.0; heights.len()];
        let before = material(&graph, &heights);
        let deposited = Fans::new(&mesh, &graph, &protected, &ceilings).deposit(
            root,
            1e-6,
            &routing,
            &mut heights,
            &mut soil,
        );
        assert!(
            deposited > 0.0 && deposited < 1e-6,
            "bounded fans must account for export"
        );
        assert_eq!(heights[root].to_bits(), original[root].to_bits());
        assert!((material(&graph, &heights) - before - deposited).abs() < 1e-14);
        for (i, &h) in heights.iter().enumerate() {
            assert!(h <= ceilings[i] + 1e-15, "sediment spike in cell {i}");
            assert!((soil[i] - h + original[i]).abs() < 1e-15);
        }
    }

    #[test]
    fn initial_and_each_terrain_refinement_stage_carve_before_final_river_finishing() {
        use super::super::{Formation, impacts, refine_stages};
        use crate::{PlanetOptions, generate};

        let mut mesh = generate(PlanetOptions {
            vertex_count: 256,
            ..PlanetOptions::default()
        })
        .unwrap();
        let mut heights: Vec<_> = mesh.directions.iter().map(|p| 0.05 * p.y).collect();
        let mut hardness = vec![0.4; heights.len()];
        let mut soil = vec![0.002; heights.len()];
        let mut settings = TerrainOptions {
            refinement_passes: 1,
            smoothing_passes: 0,
            hydraulic_passes: 0,
            thermal_passes: 0,
            connect_seas: false,
            ..TerrainOptions::default()
        };
        for layer in &mut settings.noise_layers {
            layer.weight = 0.0;
        }
        let mut formation = Formation {
            seed: 42,
            settings: &settings,
            tectonics: None,
            impacts: impacts::ImpactField::new(42, settings.impacts, heights.len()),
            river_exported: 0.0,
        };
        let mut ocean = Vec::new();
        let before = heights.clone();
        formation.initialize(&mesh, &mut heights, &hardness, &mut soil, &mut ocean);
        assert!(heights.iter().zip(&before).any(|(&h, &old)| h < old - 1e-6));
        for _ in 0..2 {
            let before = heights.clone();
            let counts = refine_stages(
                &mut mesh,
                &mut heights,
                &mut hardness,
                &mut soil,
                &mut formation,
                &mut ocean,
            )
            .unwrap();
            assert_eq!(
                counts.len(),
                2,
                "only the requested terrain subdivision should run"
            );
            assert!(
                heights.iter().zip(&before).any(|(&h, &old)| h < old - 1e-6),
                "old vertices need further excavation, not just interpolation onto new ones"
            );
        }
    }
}
