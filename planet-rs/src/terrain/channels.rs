//! Spatially indexed channel footprints shared by terrain incision and water.
use super::{
    RiverSegment, TerrainOptions,
    banks::BankPins,
    graph::{SurfaceGraph, arc},
    river_profile::{Waterfall, WaterfallBlend},
};
use crate::{DVec3, PlanetMesh};
use std::collections::HashMap;

#[derive(Clone, Copy)]
struct Reach {
    from: usize,
    to: usize,
    a: DVec3,
    b: DVec3,
    water_a: f64,
    water_b: f64,
    width: f64,
    depth: f64,
    catchment: f64,
    /// Arc distance from the downstream endpoint to this river's sea outlet.
    outlet_distance: f64,
    outlet: usize,
}

pub(super) struct Footprints<'a> {
    reaches: Vec<Reach>,
    cells: HashMap<[i32; 3], Vec<usize>>,
    cell_size: f64,
    waterfalls: &'a [Waterfall],
    reach_waterfalls: Vec<Vec<usize>>,
    fall_stations: Vec<Option<BankStation>>,
}

#[derive(Clone, Copy)]
struct Sample {
    reach: usize,
    distance: f64,
    width: f64,
    depth: f64,
    water: f64,
    pinned: bool,
    protected: bool,
    blend: WaterfallBlend,
    fall: Option<usize>,
    /// Ownership follows the natural reach even where a fall straightens its
    /// cross-sections; a nearby disconnected river must not capture the patch.
    ownership_ratio: f64,
}

#[derive(Clone, Copy)]
pub(super) struct BankStation {
    pub(super) outlet: usize,
    pub(super) distance: f64,
}

impl<'a> Footprints<'a> {
    pub fn new(
        mesh: &PlanetMesh,
        rivers: &[RiverSegment],
        levels: &[f64],
        settings: TerrainOptions,
        padding: f64,
    ) -> Self {
        let mut result = Self {
            reaches: Vec::with_capacity(rivers.len()),
            cells: HashMap::new(),
            cell_size: (settings.river_width * 8.0 + padding * 2.0).max(0.01),
            waterfalls: &[],
            reach_waterfalls: Vec::new(),
            fall_stations: Vec::new(),
        };
        let threshold = settings.river_catchment * 4.0 * std::f64::consts::PI;
        for river in rivers {
            let factor = (river.catchment / threshold).sqrt().clamp(1.0, 3.0);
            let reach = Reach {
                from: river.from,
                to: river.to,
                a: mesh.directions[river.from],
                b: mesh.directions[river.to],
                water_a: levels[river.from],
                water_b: levels[river.to],
                width: settings.river_width * factor,
                depth: settings.river_depth * factor.sqrt(),
                catchment: river.catchment,
                outlet_distance: 0.0,
                outlet: river.to,
            };
            let margin = DVec3::splat(reach.width * 3.0 + padding);
            let min = result.key(reach.a.min(reach.b) - margin);
            let max = result.key(reach.a.max(reach.b) + margin);
            let id = result.reaches.len();
            result.reaches.push(reach);
            for x in min[0]..=max[0] {
                for y in min[1]..=max[1] {
                    for z in min[2]..=max[2] {
                        result.cells.entry([x, y, z]).or_default().push(id);
                    }
                }
            }
        }
        result.measure_rivers();
        result
    }

    fn measure_rivers(&mut self) {
        let downstream: HashMap<_, _> = self
            .reaches
            .iter()
            .enumerate()
            .map(|(id, reach)| (reach.from, id))
            .collect();
        let mut distances = HashMap::<usize, (f64, usize)>::new();
        let mut path = Vec::new();
        for id in 0..self.reaches.len() {
            path.clear();
            let mut vertex = self.reaches[id].from;
            while !distances.contains_key(&vertex) {
                let Some(&next) = downstream.get(&vertex) else {
                    break;
                };
                path.push(next);
                vertex = self.reaches[next].to;
            }
            let (mut distance, outlet) = distances.get(&vertex).copied().unwrap_or((0.0, vertex));
            for &id in path.iter().rev() {
                let reach = &mut self.reaches[id];
                reach.outlet_distance = distance;
                reach.outlet = outlet;
                distance += arc(reach.a, reach.b);
                distances.insert(reach.from, (distance, outlet));
            }
        }
    }

    /// Freeze hydraulic coordinates for final wet-outline bank grading.
    /// Shared outline indices retain river order as heights and the wet outline
    /// settle; the final inclination still uses the actual terrain geometry.
    pub fn bank_stations(&self, mesh: &PlanetMesh, pins: &BankPins) -> Vec<Option<BankStation>> {
        mesh.directions
            .iter()
            .enumerate()
            .map(|(i, &point)| {
                if !pins.core[i] && !pins.boundary[i] {
                    return None;
                }
                self.bank_station_at(point)
            })
            .collect()
    }

    pub(super) fn bank_station_at(&self, point: DVec3) -> Option<BankStation> {
        let sample = self.owner(point)?;
        if let Some(fall_id) = sample.fall
            && let Some(base) = self.fall_stations[fall_id]
        {
            let fall = self.waterfalls[fall_id];
            // Order the straightened face by its actual flow axis, rather than
            // projecting each bank onto a different natural reach at bends.
            let along = point.dot(fall.direction).atan2(point.dot(fall.lip));
            return Some(BankStation {
                outlet: base.outlet,
                distance: base.distance - along,
            });
        }
        let reach = self.reaches[sample.reach];
        let edge = reach.b - reach.a;
        let t = (point - reach.a).dot(edge) / edge.length_squared();
        Some(BankStation {
            outlet: reach.outlet,
            distance: reach.outlet_distance + (1.0 - t) * arc(reach.a, reach.b),
        })
    }

    /// Actual outline edges, directed downstream -> upstream on each bank.
    /// River distance gives an ordering even on flat water terraces. Follow both
    /// the original corridor and the final wet outline, including inserted falls.
    /// A physical outline can span several short reaches or change bank side at
    /// a confluence: shared outlet ownership, not immediate reach IDs or side
    /// signs, determines whether that edge belongs to one connected river.
    pub fn upstream_banks(
        mesh: &PlanetMesh,
        pins: &BankPins,
        original: &[f64],
        heights: &[f64],
        stations: &[Option<BankStation>],
    ) -> Vec<(usize, usize)> {
        let mut outline = pins.outline(mesh);
        outline.extend(pins.final_outline(mesh, heights));
        outline.sort_unstable();
        outline.dedup();
        outline
            .into_iter()
            .filter_map(|(a, b)| {
                if original[a] < 0.0 || original[b] < 0.0 || heights[a] < 0.0 || heights[b] < 0.0 {
                    return None;
                }
                let (first, second) = (stations[a]?, stations[b]?);
                if first.outlet != second.outlet {
                    return None;
                }
                let delta = first.distance - second.distance;
                if delta > 1.0e-12 {
                    Some((b, a))
                } else if delta < -1.0e-12 {
                    Some((a, b))
                } else {
                    None
                }
            })
            .collect()
    }

    pub fn with_waterfalls(mut self, waterfalls: &'a [Waterfall]) -> Self {
        self.waterfalls = waterfalls;
        // Connectivity alone includes sibling feeders and the receiving river.
        // Keep the profile builder's primary upstream path and the fall's own
        // downstream leg, stopping before it can enter a different branch.
        let mut downstream = HashMap::new();
        let mut incoming = HashMap::<usize, usize>::new();
        let mut primary = HashMap::<usize, usize>::new();
        for (id, reach) in self.reaches.iter().enumerate() {
            downstream.insert(reach.from, id);
            *incoming.entry(reach.to).or_default() += 1;
            if primary
                .get(&reach.to)
                .is_none_or(|&old| reach.catchment > self.reaches[old].catchment)
            {
                primary.insert(reach.to, id);
            }
        }
        self.reach_waterfalls = vec![Vec::new(); self.reaches.len()];
        self.fall_stations = waterfalls
            .iter()
            .map(|fall| {
                let reach = self.reaches[*downstream.get(&fall.upper_vertex)?];
                Some(BankStation {
                    outlet: reach.outlet,
                    distance: reach.outlet_distance + arc(reach.a, reach.b),
                })
            })
            .collect();
        for (id, fall) in waterfalls.iter().enumerate() {
            let radius = fall.face_run * 2.0 + fall.half_width * 2.5 + 0.00075;
            let mut vertex = fall.upper_vertex;
            while let Some(&reach_id) = downstream.get(&vertex) {
                // Include the tributary's final edge into a merge, but never
                // continue into the receiving trunk beyond that junction.
                if vertex != fall.upper_vertex && incoming.get(&vertex).copied().unwrap_or(0) > 1 {
                    break;
                }
                let reach = self.reaches[reach_id];
                self.reach_waterfalls[reach_id].push(id);
                if vertex == fall.upper_vertex && reach.to == fall.lower_vertex {
                    self.reaches[reach_id].water_a = fall.lower_surface;
                }
                if arc(fall.lip, reach.b) > radius {
                    break;
                }
                vertex = reach.to;
            }
            // The first upstream edge is explicit: choosing the largest flow
            // here could switch to the other branch at the waterfall's foot.
            let mut vertex = fall.upstream_vertex;
            let mut receiver = fall.upper_vertex;
            while let Some(&reach_id) = downstream.get(&vertex) {
                let reach = self.reaches[reach_id];
                if reach.to != receiver {
                    break;
                }
                self.reach_waterfalls[reach_id].push(id);
                if arc(fall.lip, reach.a) > radius {
                    break;
                }
                receiver = vertex;
                let Some(&upstream) = primary.get(&vertex) else {
                    break;
                };
                vertex = self.reaches[upstream].from;
            }
        }
        self
    }

    #[allow(clippy::cast_possible_truncation)]
    fn key(&self, p: DVec3) -> [i32; 3] {
        (p / self.cell_size).floor().to_array().map(|x| x as i32)
    }

    fn sample(&self, id: usize, point: DVec3) -> Sample {
        let r = self.reaches[id];
        let edge = r.b - r.a;
        let t = ((point - r.a).dot(edge) / edge.length_squared()).clamp(0.0, 1.0);
        let distance = arc(point, r.a.lerp(r.b, t).normalize());
        // Choose one owning patch for all hydraulic/support/blend attributes.
        // Mixing the lowest surface with a different fall's mask opens seams.
        let patch = self
            .reach_waterfalls
            .get(id)
            .into_iter()
            .flatten()
            .filter_map(|&i| {
                let fall = self.waterfalls[i];
                fall.surface_at(point).map(|surface| (i, fall, surface))
            })
            .min_by(|a, b| a.2.total_cmp(&b.2));
        let ownership_ratio = distance / r.width;
        let (distance, width) = patch.map_or((distance, r.width), |(_, fall, _)| {
            fall.corridor_at(point, distance, r.width)
        });
        Sample {
            reach: id,
            distance,
            width,
            depth: r.depth,
            water: patch.map_or(
                r.water_a + (r.water_b - r.water_a) * t,
                |(_, _, surface)| surface,
            ),
            pinned: patch.is_some_and(|(_, fall, _)| fall.on_face(point)),
            protected: patch.is_some(),
            blend: patch.map_or_else(WaterfallBlend::default, |(_, fall, _)| fall.blend(point)),
            fall: patch.map(|(id, _, _)| id),
            ownership_ratio,
        }
    }

    fn samples(&self, point: DVec3) -> impl Iterator<Item = Sample> + '_ {
        self.cells
            .get(&self.key(point))
            .into_iter()
            .flatten()
            .filter_map(move |&id| {
                let sample = self.sample(id, point);
                (sample.ownership_ratio <= 3.0 + self.cell_size / self.reaches[id].width)
                    .then_some(sample)
            })
    }

    fn owner(&self, point: DVec3) -> Option<Sample> {
        self.samples(point).min_by(|a, b| {
            a.ownership_ratio
                .total_cmp(&b.ownership_ratio)
                .then(a.water.total_cmp(&b.water))
                .then(a.reach.cmp(&b.reach))
        })
    }

    /// Island's width-based edge target, two-ring apron and incident tessellation.
    pub fn refinement_faces(
        &self,
        mesh: &PlanetMesh,
        graph: &SurfaceGraph,
        heights: &[f64],
    ) -> Vec<bool> {
        let pins = self.bank_pins(mesh, graph, heights);
        let mut targets: Vec<_> = pins
            .core
            .iter()
            .zip(&pins.boundary)
            .zip(&pins.widths)
            .map(|((&core, &bank), &width)| {
                if core || bank {
                    width.clamp(0.0015, 0.005)
                } else {
                    f64::INFINITY
                }
            })
            .collect();
        let mut frontier: Vec<_> = targets
            .iter()
            .enumerate()
            .filter_map(|(i, t)| t.is_finite().then_some(i))
            .collect();
        for _ in 0..2 {
            let mut next = Vec::new();
            for i in frontier {
                let target = targets[i];
                for &j in &graph.neighbours[i] {
                    if target < targets[j] {
                        targets[j] = target;
                        next.push(j);
                    }
                }
            }
            frontier = next;
        }
        let mut marked = vec![false; heights.len()];
        for face in &mesh.triangles {
            let target = face
                .iter()
                .map(|&i| targets[i as usize])
                .fold(f64::INFINITY, f64::min);
            let [a, b, c] = face.map(|i| mesh.directions[i as usize]);
            if a.distance(b).max(b.distance(c)).max(c.distance(a)) > target {
                for &i in face {
                    marked[i as usize] = true;
                }
            }
        }
        mesh.triangles
            .iter()
            .map(|face| face.iter().any(|&i| marked[i as usize]))
            .collect()
    }

    /// Island adds a 1.25m-target waterfall pass and a final face detail tier.
    /// Expand face selections through the lateral bank apron, then stitch the
    /// neighbouring terrain with the same conforming midpoint topology.
    pub fn waterfall_refinement_faces(&self, mesh: &PlanetMesh, face_detail: bool) -> Vec<bool> {
        let mut marked = vec![false; mesh.directions.len()];
        for face in &mesh.triangles {
            let [a, b, c] = face.map(|i| mesh.directions[i as usize]);
            let selected = if face_detail {
                [a, b, c].iter().any(|&point| {
                    self.owner(point).is_some_and(|s| {
                        self.reach_waterfalls
                            .get(s.reach)
                            .into_iter()
                            .flatten()
                            .any(|&i| {
                                let fall = self.waterfalls[i];
                                let along = (point - fall.lip).dot(fall.direction);
                                (-fall.face_run..=0.0).contains(&along)
                                    && fall.surface_at(point).is_some()
                            })
                    })
                })
            } else {
                a.distance(b).max(b.distance(c)).max(c.distance(a)) > 0.00125
                    && [a, b, c]
                        .iter()
                        .any(|&point| self.owner(point).is_some_and(|s| s.protected))
            };
            if selected {
                for &i in face {
                    marked[i as usize] = true;
                }
            }
        }
        mesh.triangles
            .iter()
            .map(|face| face.iter().any(|&i| marked[i as usize]))
            .collect()
    }

    pub fn hydraulic_protection(&self, mesh: &PlanetMesh) -> Vec<bool> {
        mesh.directions
            .iter()
            .map(|&point| self.owner(point).is_some_and(|s| s.protected))
            .collect()
    }

    pub fn carve(&self, mesh: &PlanetMesh, heights: &mut [f64], soil: &mut [f64]) {
        for i in 0..heights.len() {
            let original = heights[i];
            let mut target = original;
            if let Some(Sample {
                distance,
                width,
                depth,
                water,
                pinned,
                ..
            }) = self.owner(mesh.directions[i])
            {
                let x = distance / width;
                if x < 1.0 {
                    let clearance = if water <= 0.0 {
                        // Below the last fall, retain a submerged channel /
                        // plunge pool, rather than the face's thin 3cm film.
                        depth.max(super::banks::FALL_CLEARANCE) * (1.0 - x.powi(4))
                    } else if pinned {
                        0.00003
                    } else {
                        depth * (1.0 - x.powi(4))
                    };
                    target = target.min(water - clearance);
                } else if x < 2.5 {
                    let t = ((x - 1.0) / 1.5).clamp(0.0, 1.0);
                    let blend = t * t * (3.0 - 2.0 * t);
                    target = target.min(water + (original - water) * blend);
                }
            }
            heights[i] = target;
            soil[i] = (soil[i] - (original - target)).max(0.0);
        }
    }

    fn field_samples(&self, mesh: &PlanetMesh, graph: &SurfaceGraph) -> Vec<Option<Sample>> {
        let owners: Vec<_> = mesh
            .directions
            .iter()
            .map(|&direction| self.owner(direction).map(|sample| sample.reach))
            .collect();
        // Extend the *profile*, rather than bed heights, through every bank
        // triangle. Topological banks are pinned separately: dry vertices never
        // pull the channel core into a cross-channel mound.
        mesh.directions
            .iter()
            .enumerate()
            .map(|(i, &direction)| {
                owners[i]
                    .or_else(|| {
                        graph.neighbours[i]
                            .iter()
                            .filter_map(|&j| owners[j])
                            .min_by(|&a, &b| {
                                self.sample(a, direction)
                                    .distance
                                    .total_cmp(&self.sample(b, direction).distance)
                            })
                    })
                    .map(|id| self.sample(id, direction))
            })
            .collect()
    }

    pub fn bank_pins(&self, mesh: &PlanetMesh, graph: &SurfaceGraph, heights: &[f64]) -> BankPins {
        let samples = self.field_samples(mesh, graph);
        let levels = samples
            .iter()
            .zip(heights)
            .map(|(s, &height)| s.map_or(height - 0.00001, |s| s.water))
            .collect();
        let distances: Vec<_> = samples
            .iter()
            .map(|s| s.map_or(-1.0, |s| s.width - s.distance))
            .collect();
        let widths = samples.iter().map(|s| s.map_or(0.0, |s| s.width)).collect();
        let support = samples
            .iter()
            .map(|s| s.is_some_and(|s| s.pinned))
            .collect();
        let blends = samples
            .iter()
            .map(|s| s.map_or_else(WaterfallBlend::default, |s| s.blend))
            .collect();
        BankPins::new(graph, levels, &distances, widths, support).with_blends(blends)
    }

    #[cfg(test)]
    pub fn water_field(
        &self,
        mesh: &PlanetMesh,
        graph: &SurfaceGraph,
        heights: &[f64],
    ) -> (Vec<f64>, Vec<f64>) {
        let samples = self.field_samples(mesh, graph);
        let levels = samples
            .iter()
            .zip(heights)
            .map(|(sample, &bed)| sample.map_or(bed - 0.00001, |s| s.water))
            .collect();
        let banks = samples
            .iter()
            .map(|sample| sample.map_or(-1.0, |s| s.width - s.distance))
            .collect();
        (levels, banks)
    }

    pub fn relax_bed(
        &self,
        mesh: &PlanetMesh,
        graph: &SurfaceGraph,
        heights: &mut [f64],
        soil: &mut [f64],
        pins: &BankPins,
    ) {
        // Island relaxes the corridor AND two-ring outer apron after bank lift.
        // Pin the outline and the hydraulic patch, allowing outer heights to
        // relax in either direction instead of keeping the old cut-only cap.
        let mut patch: Vec<_> = pins
            .core
            .iter()
            .zip(&pins.boundary)
            .map(|(&a, &b)| a || b)
            .collect();
        let mut frontier: Vec<_> = patch
            .iter()
            .enumerate()
            .filter_map(|(i, &v)| v.then_some(i))
            .collect();
        for _ in 0..2 {
            let mut next = Vec::new();
            for i in frontier {
                for &j in &graph.neighbours[i] {
                    if !patch[j] {
                        patch[j] = true;
                        next.push(j);
                    }
                }
            }
            frontier = next;
        }
        let samples = self.field_samples(mesh, graph);
        let selected: Vec<_> = graph
            .neighbours
            .iter()
            .enumerate()
            .filter_map(|(i, neighbours)| {
                (patch[i]
                    && !pins.sea_openings[i]
                    && !pins.boundary[i]
                    && !samples[i].is_some_and(|s| s.protected)
                    && neighbours.iter().all(|&j| patch[j]))
                .then_some(i)
            })
            .collect();
        let mut snapshot = heights.to_vec();
        for _ in 0..4 {
            for &i in &selected {
                let count = u32::try_from(graph.neighbours[i].len()).unwrap();
                let average = graph.neighbours[i]
                    .iter()
                    .map(|&j| snapshot[j])
                    .sum::<f64>()
                    / f64::from(count);
                let relaxed = snapshot[i] + (average - snapshot[i]) * 0.35;
                let target = if pins.core[i] {
                    let sample = samples[i].expect("wet vertices have a river owner");
                    let t = (sample.distance / sample.width).clamp(0.0, 1.0);
                    let section =
                        sample.water - sample.depth + sample.depth * t * t * (3.0 - 2.0 * t);
                    (relaxed + (section - relaxed) * 0.65).min(sample.water - 0.00001)
                } else {
                    relaxed
                };
                soil[i] = (soil[i] + target - heights[i]).max(0.0);
                heights[i] = target;
            }
            snapshot.copy_from_slice(heights);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sea_outlet_mesh() -> (PlanetMesh, [f64; 81]) {
        let mut mesh = PlanetMesh {
            directions: Vec::new(),
            triangles: Vec::new(),
            radius: 1000.0,
        };
        let mut original = [0.0; 81];
        for row in 0..9 {
            let x = -0.004 + f64::from(row) * 0.001;
            for col in 0..9 {
                let y = -0.004 + f64::from(col) * 0.001;
                mesh.directions.push(DVec3::new(x, y, 1.0).normalize());
                let i = mesh.directions.len() - 1;
                original[i] = if row == 8 {
                    -0.001
                } else {
                    0.002 - x * 0.5 + y.abs() * 0.25
                };
            }
        }
        for row in 0..8 {
            for col in 0..8 {
                let i = row * 9 + col;
                mesh.triangles
                    .extend([[i, i + 9, i + 1], [i + 1, i + 9, i + 10]]);
            }
        }
        (mesh, original)
    }

    #[test]
    fn sea_level_outlet_retains_a_submerged_channel_through_bank_lift_and_all_smoothing() {
        use super::super::drainage;
        let (mut mesh, original) = sea_outlet_mesh();
        let falls = [Waterfall {
            upper_vertex: 4 * 9 + 4,
            lower_vertex: 5 * 9 + 4,
            upstream_vertex: 3 * 9 + 4,
            lip: DVec3::Z,
            direction: DVec3::X,
            half_width: 0.002,
            upper_surface: 0.003,
            lower_surface: 0.0,
            face_run: 0.0025,
        }];
        let settings = TerrainOptions {
            river_width: 0.002,
            ..TerrainOptions::default()
        };
        let catchment = settings.river_catchment * 4.0 * std::f64::consts::PI;
        let rivers: Vec<_> = (0..8)
            .map(|row| RiverSegment {
                from: row * 9 + 4,
                to: (row + 1) * 9 + 4,
                catchment,
            })
            .collect();
        let levels: Vec<_> = mesh
            .directions
            .iter()
            .map(|&p| {
                falls[0]
                    .surface_at(p)
                    .unwrap_or(if p.x < 0.0 { 0.003 } else { 0.0 })
            })
            .collect();
        let footprint =
            Footprints::new(&mesh, &rivers, &levels, settings, 0.0).with_waterfalls(&falls);
        let mut heights = original;
        let mut soil = [0.0; 81];
        footprint.carve(&mesh, &mut heights, &mut soil);
        let carved = heights;
        let mut graph = SurfaceGraph::new(&mesh);
        let pins = footprint.bank_pins(&mesh, &graph, &heights);
        let mouth = 4 * 9 + 4;
        assert!(
            pins.sea_openings[mouth] && !pins.core[mouth] && !pins.boundary[mouth],
            "the sea-covered channel is neither a visible river nor a dry bank"
        );
        assert!(
            heights[mouth] <= -settings.river_depth * 0.99,
            "the last fall needs a submerged bed, not a thin face film at sea level"
        );
        assert!(
            pins.boundary[3 * 9 + 1] || pins.boundary[3 * 9 + 2],
            "the side bank still gets pinned"
        );
        pins.lift(&mesh, &graph, &original, &mut heights, &mut soil);
        footprint.relax_bed(&mesh, &graph, &mut heights, &mut soil, &pins);
        let protected = footprint.hydraulic_protection(&mesh);
        pins.smooth_outline(&mut mesh, &protected);
        graph = SurfaceGraph::new(&mesh);
        pins.smooth_waterfall_approach(&graph, &mut heights, &mut soil);
        let routing = TerrainOptions {
            river_depth: 0.0,
            ..settings
        };
        let (drains, _, _) =
            drainage::carve(&mesh.directions, &graph, &mut heights, &mut soil, routing);
        pins.relax_river_support(&graph, &drains, &mut heights, &mut soil);
        let stations = footprint.bank_stations(&mesh, &pins);
        let edges = Footprints::upstream_banks(&mesh, &pins, &original, &heights, &stations);
        drainage::lift_banks(
            &mesh.directions,
            &graph,
            &mut heights,
            &mut soil,
            &edges,
            routing,
        )
        .unwrap();
        let openings: Vec<_> = pins
            .sea_openings
            .iter()
            .enumerate()
            .filter_map(|(i, &v)| v.then_some(i))
            .collect();
        assert!(openings.len() > 8);
        for &i in &openings {
            assert!(heights[i] < 0.0);
            assert!(
                (heights[i] - carved[i]).abs() < 1.0e-12,
                "smoothing and bank grading must not refill any part of the submerged mouth"
            );
        }
        let water = pins.into_water(&mesh, &heights);
        assert!(
            openings.iter().all(|&i| water[i] < 0.0),
            "the sea supplies the surface at the opening"
        );
    }

    #[test]
    fn segmented_reaches_and_endpoint_caps_cover_both_banks_without_closing_the_sea() {
        for pole in [1.0, -1.0] {
            for segments in [1_u32, 8, 40] {
                let mut mesh = PlanetMesh {
                    directions: Vec::new(),
                    triangles: Vec::new(),
                    radius: 1000.0,
                };
                for row in 0..9 {
                    for column in 0..7 {
                        mesh.directions.push(
                            DVec3::new(f64::from(row) * 0.001, f64::from(column - 3) * 0.001, pole)
                                .normalize(),
                        );
                    }
                }
                for row in 0..8 {
                    for column in 0..6 {
                        let i = row * 7 + column;
                        mesh.triangles
                            .extend([[i, i + 7, i + 1], [i + 1, i + 7, i + 8]]);
                    }
                }
                // The outline is coarser than the many short centreline reaches.
                // Its neighbouring bank vertices can belong to nonadjacent owners.
                let first = mesh.directions.len();
                for i in 0..=segments {
                    // Endpoints lie inside the bank's extent, exercising the caps.
                    let x = 0.001 + 0.006 * f64::from(i) / f64::from(segments);
                    mesh.directions.push(DVec3::new(x, 0.0, pole).normalize());
                }
                let graph = SurfaceGraph::new(&mesh);
                let settings = TerrainOptions {
                    river_width: 0.0015,
                    ..TerrainOptions::default()
                };
                let rivers: Vec<_> = (0..segments as usize)
                    .map(|i| RiverSegment {
                        from: first + i + 1,
                        to: first + i,
                        catchment: settings.river_catchment * 4.0 * std::f64::consts::PI,
                    })
                    .collect();
                let footprint = Footprints::new(
                    &mesh,
                    &rivers,
                    &vec![0.03; mesh.directions.len()],
                    settings,
                    0.0,
                );
                let mut heights = vec![0.02; mesh.directions.len()];
                heights[..7].fill(-0.01);
                let pins = footprint.bank_pins(&mesh, &graph, &heights);
                let stations = footprint.bank_stations(&mesh, &pins);
                let edges = Footprints::upstream_banks(&mesh, &pins, &heights, &heights, &stations);
                assert_eq!(
                    edges.len(),
                    14,
                    "both banks, owner transitions and endpoint caps must be represented"
                );
                let mut left = 0;
                let mut right = 0;
                for (down, up) in edges {
                    assert!(down >= 7 && up >= 7, "the submerged outlet stays open");
                    assert!(
                        up / 7 > down / 7,
                        "order follows river distance, not the flat water level"
                    );
                    assert_eq!(down % 7, up % 7, "opposite banks must remain independent");
                    if up % 7 < 3 {
                        left += 1;
                    } else {
                        right += 1;
                    }
                }
                assert!(left >= 6 && right >= 6);
            }
        }
    }

    #[test]
    fn bent_reaches_share_one_lateral_span_across_a_sloping_waterfall() {
        let point = |x: f64, y: f64| {
            let y = y.sin();
            DVec3::new(x, y, (1.0 - x * x - y * y).sqrt())
        };
        let mesh = PlanetMesh {
            directions: vec![
                point(-0.004, 0.0018),
                point(-0.0008, 0.0),
                point(0.0, 0.0),
                point(0.004, 0.0015),
            ],
            triangles: Vec::new(),
            radius: 1000.0,
        };
        let settings = TerrainOptions {
            river_width: 0.002,
            ..TerrainOptions::default()
        };
        let threshold = settings.river_catchment * 4.0 * std::f64::consts::PI;
        let rivers: Vec<_> = [1.0, 1.21, 2.25]
            .into_iter()
            .enumerate()
            .map(|(from, factor)| RiverSegment {
                from,
                to: from + 1,
                catchment: threshold * factor,
            })
            .collect();
        let falls = [Waterfall {
            upper_vertex: 2,
            lower_vertex: 3,
            upstream_vertex: 1,
            lip: DVec3::Z,
            direction: DVec3::X,
            half_width: 0.003,
            upper_surface: 0.04,
            lower_surface: 0.036,
            face_run: 0.0025,
        }];
        let footprint = Footprints::new(&mesh, &rivers, &[0.04, 0.04, 0.04, 0.036], settings, 0.0)
            .with_waterfalls(&falls);
        for x in [-0.0024, -0.00125, -0.0001] {
            for lateral in [0.0_f64, 0.0029, 0.0031] {
                let left = footprint.owner(point(x, lateral)).unwrap();
                let right = footprint.owner(point(x, -lateral)).unwrap();
                assert!((left.distance - lateral).abs() < 1.0e-12);
                assert!((right.distance - lateral).abs() < 1.0e-12);
                assert!((left.width - 0.003).abs() < 1.0e-12);
                assert!((right.width - 0.003).abs() < 1.0e-12);
                assert_eq!(left.distance < left.width, lateral < 0.003);
                assert_eq!(right.distance < right.width, lateral < 0.003);
                assert!((left.water - right.water).abs() < 1.0e-12);
                assert!(left.pinned && right.pinned);
                let left_station = footprint.bank_station_at(point(x, lateral)).unwrap();
                let right_station = footprint.bank_station_at(point(x, -lateral)).unwrap();
                assert!(
                    (left_station.distance - right_station.distance).abs() < 1.0e-12,
                    "straightened waterfall banks must use one flow coordinate despite bent source reaches"
                );
            }
        }
        let fall = falls[0];
        for x in [-fall.face_run * 2.0, fall.half_width * 1.5 + 0.00075] {
            let (distance, width) = fall.corridor_at(point(x, 0.001), 0.002, 0.004);
            assert!(
                (distance - 0.002).abs() < 1.0e-12 && (width - 0.004).abs() < 1.0e-12,
                "the natural river width and centreline resume at both shoulders"
            );
        }
    }

    #[test]
    fn upstream_blending_does_not_capture_a_nearby_disconnected_channel() {
        let mesh = PlanetMesh {
            directions: [(-0.004, 0.0), (0.002, 0.0), (-0.004, 0.001), (0.002, 0.001)]
                .map(|(x, y)| DVec3::new(x, y, (1.0_f64 - x * x - y * y).sqrt()))
                .to_vec(),
            triangles: Vec::new(),
            radius: 1000.0,
        };
        let settings = TerrainOptions::default();
        let catchment = settings.river_catchment * 4.0 * std::f64::consts::PI;
        let falls = [Waterfall {
            upper_vertex: 0,
            lower_vertex: 1,
            upstream_vertex: 0,
            lip: DVec3::Z,
            direction: DVec3::X,
            half_width: 0.003,
            upper_surface: 0.04,
            lower_surface: 0.036,
            face_run: 0.0025,
        }];
        let footprints = Footprints::new(
            &mesh,
            &[
                RiverSegment {
                    from: 0,
                    to: 1,
                    catchment,
                },
                RiverSegment {
                    from: 2,
                    to: 3,
                    catchment,
                },
            ],
            &[0.04, 0.036, 0.02, 0.02],
            settings,
            0.0,
        )
        .with_waterfalls(&falls);
        let point = DVec3::new(
            -0.00375,
            0.001,
            (1.0_f64 - 0.00375 * 0.00375 - 0.001 * 0.001).sqrt(),
        );
        let own = footprints.owner(point).unwrap();
        assert!((own.water - 0.02).abs() < 1.0e-12);
        assert!((own.blend.normal - 1.0).abs() < 1.0e-12 && own.blend.smoothing.abs() < 1.0e-12);
        assert!(!own.pinned && !own.protected);
    }

    #[test]
    fn waterfall_face_does_not_lift_other_branches_of_the_same_river() {
        for pole in [1.0, -1.0] {
            let mesh = PlanetMesh {
                directions: [
                    (-0.004, 0.0), // Waterfall's primary upstream path.
                    (-0.0015, 0.0),
                    (0.0, 0.0),
                    (0.003, 0.0),      // Merge into the receiving river.
                    (-0.0015, 0.0015), // Receiving river upstream, inside face band.
                    (-0.004, 0.002),
                    (-0.002, -0.0015), // Another feeder above the waterfall.
                    (-0.001, -0.0015), // Receiving river bends back beside the face.
                    (0.004, -0.004),
                ]
                .map(|(x, y)| DVec3::new(x, y, pole).normalize())
                .to_vec(),
                triangles: Vec::new(),
                radius: 1000.0,
            };
            let settings = TerrainOptions {
                river_width: 0.002,
                ..TerrainOptions::default()
            };
            let threshold = settings.river_catchment * 4.0 * std::f64::consts::PI;
            let rivers: Vec<_> = [
                (6, 1, 9.0), // Both feeders reach the width cap; flow still picks the primary.
                (0, 1, 16.0),
                (1, 2, 25.0),
                (2, 3, 25.0),
                (5, 4, 36.0),
                (4, 3, 36.0),
                (3, 7, 61.0),
                (7, 8, 61.0),
            ]
            .map(|(from, to, flow)| RiverSegment {
                from,
                to,
                catchment: flow * threshold,
            })
            .to_vec();
            let levels = [0.04, 0.04, 0.04, 0.02, 0.02, 0.02, 0.04, 0.018, 0.018];
            let falls = [Waterfall {
                upper_vertex: 2,
                lower_vertex: 3,
                upstream_vertex: 1,
                lip: DVec3::Z * pole,
                direction: DVec3::X,
                half_width: 0.002,
                upper_surface: 0.04,
                lower_surface: 0.02,
                face_run: 0.0025,
            }];
            let footprints =
                Footprints::new(&mesh, &rivers, &levels, settings, 0.0).with_waterfalls(&falls);
            for vertex in [4, 6, 7] {
                assert!(falls[0].surface_at(mesh.directions[vertex]).is_some());
                let sample = footprints.owner(mesh.directions[vertex]).unwrap();
                assert!(
                    (sample.water - levels[vertex]).abs() < 1.0e-12,
                    "a waterfall must not replace another branch's hydraulic profile"
                );
                assert!(
                    !sample.pinned && !sample.protected,
                    "connected neighbouring branches must not inherit waterfall face support"
                );
                assert!(
                    (sample.blend.normal - 1.0).abs() < 1.0e-12
                        && sample.blend.smoothing.abs() < 1.0e-12
                );
            }
            for vertex in [0, 1] {
                assert!(
                    footprints.owner(mesh.directions[vertex]).unwrap().protected,
                    "keep the waterfall's own approach covered across short reaches"
                );
            }
            let graph = SurfaceGraph::new(&mesh);
            let original = levels.map(|level| level - settings.river_depth);
            let mut heights = original;
            let mut soil = [0.0; 9];
            footprints.carve(&mesh, &mut heights, &mut soil);
            let carved = heights;
            let pins = footprints.bank_pins(&mesh, &graph, &heights);
            pins.lift(&mesh, &graph, &original, &mut heights, &mut soil);
            for vertex in [4, 6, 7] {
                assert!(
                    (heights[vertex] - carved[vertex]).abs() < 1.0e-12,
                    "face raising must leave neighbouring channel beds untouched"
                );
            }
            assert!(
                (heights[1] - (pins.levels[1] - super::super::banks::FALL_CLEARANCE)).abs()
                    < 1.0e-12,
                "the real waterfall still receives its thin supported face"
            );
        }
    }

    #[test]
    fn corridor_refinement_stops_at_the_width_target_instead_of_splitting_every_pass() {
        let mesh = PlanetMesh {
            directions: [(0.0, 0.0), (0.0004, 0.0), (0.0, 0.0004)]
                .map(|(x, y)| DVec3::new(x, y, 1.0).normalize())
                .to_vec(),
            triangles: vec![[0, 1, 2]],
            radius: 1000.0,
        };
        let graph = SurfaceGraph::new(&mesh);
        let settings = TerrainOptions::default();
        let river = RiverSegment {
            from: 0,
            to: 1,
            catchment: settings.river_catchment * 4.0 * std::f64::consts::PI,
        };
        let footprint = Footprints::new(&mesh, &[river], &[0.04; 3], settings, 0.0);
        assert!(
            footprint
                .refinement_faces(&mesh, &graph, &[0.06; 3])
                .iter()
                .all(|&v| !v)
        );
        let coarse = PlanetMesh {
            directions: mesh
                .directions
                .iter()
                .map(|p| DVec3::new(p.x * 100.0, p.y * 100.0, p.z).normalize())
                .collect(),
            ..mesh
        };
        let footprint = Footprints::new(&coarse, &[river], &[0.04; 3], settings, 0.0);
        assert!(
            footprint
                .refinement_faces(&coarse, &graph, &[0.06; 3])
                .iter()
                .all(|&v| v)
        );
    }

    #[test]
    fn fixed_profile_stays_flat_across_banks_after_carving_and_joins_choose_lower_water() {
        let mesh = PlanetMesh {
            directions: [
                (-0.02, 0.0),
                (0.02, 0.0),
                (0.0, 0.0015),
                (0.0, -0.0015),
                (0.0, 0.008),
            ]
            .map(|(x, y)| DVec3::new(x, y, 1.0).normalize())
            .to_vec(),
            triangles: vec![[0, 1, 2], [1, 0, 3], [0, 2, 4]],
            radius: 1000.0,
        };
        let graph = SurfaceGraph::new(&mesh);
        let settings = TerrainOptions::default();
        let threshold = settings.river_catchment * 4.0 * std::f64::consts::PI;
        let river = RiverSegment {
            from: 0,
            to: 1,
            catchment: threshold,
        };
        let footprint = Footprints::new(&mesh, &[river], &[0.04; 5], settings, 0.0);
        let mut heights = vec![0.06, 0.06, 0.045, 0.08, 0.07];
        let (before, banks) = footprint.water_field(&mesh, &graph, &heights);
        footprint.carve(&mesh, &mut heights, &mut [0.002; 5]);
        let (after, _) = footprint.water_field(&mesh, &graph, &heights);
        assert_eq!(
            before, after,
            "the carved bed must not redefine water levels"
        );
        assert!(
            (after[2] - after[3]).abs() < 1.0e-12,
            "opposite banks share one profile"
        );
        assert!(
            (after[4] - 0.04).abs() < 1.0e-12,
            "dry bank vertices must extend the water plane"
        );
        assert!(banks[2] > 0.0 && banks[4] < 0.0);
        let pins = footprint.bank_pins(&mesh, &graph, &heights);
        pins.lift(
            &mesh,
            &graph,
            &[0.06, 0.06, 0.045, 0.08, 0.07],
            &mut heights,
            &mut [0.002; 5],
        );
        let water = pins.into_water(&mesh, &heights);
        let river_surface = super::super::water_mesh::build(&mesh, &heights, &water);
        assert!(!river_surface.triangles.is_empty());
        // This open terrain patch intentionally has wet inlet/outlet edges.
        // Closed-world bank validation is covered by the terrain pipeline tests.
        let mut joined = footprint;
        let mut second = joined.reaches[0];
        second.water_a = 0.02;
        second.water_b = 0.02;
        joined.reaches.push(second);
        for reaches in joined.cells.values_mut() {
            reaches.push(1);
        }
        assert!(
            (joined.owner(DVec3::Z).unwrap().water - 0.02).abs() < 1.0e-12,
            "a confluence must not choose the highest overlapping water surface"
        );
    }
}
