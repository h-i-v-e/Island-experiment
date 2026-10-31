//! Radial port of Island's `lift_river_banks_to_surface` and boundary rules in
//! `duplicate_river_topology`. Pin the terrain first, then bind the final water
//! boundary to the actual terrain after drainage repair.
use super::{
    Drainage,
    graph::{SurfaceGraph, arc},
    river_profile::WaterfallBlend,
};
use crate::{DVec3, PlanetMesh};
use std::{
    cmp::Ordering,
    collections::{BTreeMap, BTreeSet, BinaryHeap},
};

pub(super) const FALL_CLEARANCE: f64 = 0.00003;

fn flat_weight(normal: DVec3, up: DVec3) -> f64 {
    let alignment = normal.dot(up);
    // Geometry-free fixtures have no incident faces. Treat them as flat;
    // generated terrain has finite area-weighted unit normals everywhere.
    let flat = if alignment.is_finite() {
        alignment.clamp(0.0, 1.0)
    } else {
        1.0
    };
    // Zero slope at both endpoints avoids leaving a shallow toe dent when a
    // nearly horizontal landing still has a small terrain-normal tilt.
    flat * flat * (3.0 - 2.0 * flat)
}

fn river_flat_weight(normal: DVec3, up: DVec3, flow: DVec3) -> f64 {
    let tangent = (flow - up * flow.dot(up)).normalize_or_zero();
    if tangent == DVec3::ZERO || !normal.is_finite() {
        return flat_weight(normal, up);
    }
    // Only the normal's downstream component describes a falling water face.
    // A steep cross-channel bank must keep the planned water level rather than
    // acquiring a film of water up its side. For a longitudinal ramp this is
    // exactly the original inclination blend, including its gentle flat toe.
    let along = normal.dot(tangent).clamp(-1.0, 1.0);
    let flat = (1.0 - along * along).max(0.0).sqrt();
    flat * flat * (3.0 - 2.0 * flat)
}

pub(super) struct BankPins {
    pub levels: Vec<f64>,
    pub widths: Vec<f64>,
    pub core: Vec<bool>,
    pub boundary: Vec<bool>,
    /// Geometric channel at sea level: covered by the sea, never a dry bank.
    pub sea_openings: Vec<bool>,
    support: Vec<bool>,
    blends: Vec<WaterfallBlend>,
    flows: Vec<DVec3>,
}

#[derive(Clone, Copy, PartialEq)]
struct State {
    cost: f64,
    vertex: usize,
}
impl Eq for State {}
impl Ord for State {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .cost
            .total_cmp(&self.cost)
            .then_with(|| other.vertex.cmp(&self.vertex))
    }
}
impl PartialOrd for State {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl BankPins {
    pub fn new(
        graph: &SurfaceGraph,
        levels: Vec<f64>,
        distances: &[f64],
        widths: Vec<f64>,
        support: Vec<bool>,
    ) -> Self {
        let core: Vec<_> = distances
            .iter()
            .zip(&levels)
            .map(|(&d, &level)| d > 0.0 && level > 0.0)
            .collect();
        let sea_openings: Vec<_> = distances
            .iter()
            .zip(&levels)
            .map(|(&d, &level)| d > 0.0 && level <= 0.0)
            .collect();
        let boundary = graph
            .neighbours
            .iter()
            .enumerate()
            .map(|(i, neighbours)| {
                !core[i] && !sea_openings[i] && neighbours.iter().any(|&j| core[j])
            })
            .collect();
        Self {
            levels,
            widths,
            core,
            boundary,
            sea_openings,
            support,
            blends: Vec::new(),
            flows: Vec::new(),
        }
    }

    pub fn with_blends(mut self, blends: Vec<WaterfallBlend>) -> Self {
        self.blends = blends;
        self
    }

    pub fn with_flows(mut self, flows: Vec<DVec3>) -> Self {
        self.flows = flows;
        self
    }

    fn water_weight(&self, i: usize, normal: DVec3, up: DVec3) -> f64 {
        river_flat_weight(normal, up, self.flows.get(i).copied().unwrap_or_default())
    }

    /// Submerged river mouths remain openings, including beneath a frozen
    /// waterfall footprint. Bank support must not dam them after excavation.
    pub fn preserve_ocean(&mut self, heights: &[f64]) {
        for (i, &height) in heights.iter().enumerate() {
            if height < 0.0 {
                self.sea_openings[i] = true;
                self.core[i] = false;
                self.boundary[i] = false;
                self.support[i] = false;
                self.levels[i] = 0.0;
            }
        }
    }

    fn blend(&self, vertex: usize) -> WaterfallBlend {
        self.blends.get(vertex).copied().unwrap_or_default()
    }

    pub fn lift(
        &self,
        mesh: &PlanetMesh,
        graph: &SurfaceGraph,
        original: &[f64],
        heights: &mut [f64],
        soil: &mut [f64],
    ) {
        // Match Island's protected waterfall face: support follows the planned
        // face, including previously low vertices that carving alone cannot lift.
        for i in 0..heights.len() {
            if self.support[i] && (self.core[i] || self.boundary[i]) && self.levels[i] > 0.0 {
                let target = self.levels[i] - FALL_CLEARANCE;
                soil[i] = (soil[i] + target - heights[i]).max(0.0);
                heights[i] = target;
            }
        }
        let mut targets = vec![f64::NEG_INFINITY; heights.len()];
        let mut distances = vec![f64::INFINITY; heights.len()];
        let mut sources = vec![usize::MAX; heights.len()];
        let mut queue = BinaryHeap::new();
        for i in 0..heights.len() {
            if self.boundary[i]
                && !self.support[i]
                && original[i] >= 0.0
                && self.levels[i] > heights[i]
            {
                targets[i] = heights[i] + (self.levels[i] - heights[i]) * self.blend(i).normal;
                distances[i] = 0.0;
                sources[i] = i;
                queue.push(State {
                    cost: 0.0,
                    vertex: i,
                });
            }
        }
        while let Some(State { cost, vertex }) = queue.pop() {
            if cost > distances[vertex] || cost >= 1.0 {
                continue;
            }
            let source = sources[vertex];
            let blend_width = (self.widths[source] * 2.0).clamp(0.004, 0.02);
            if !self.boundary[vertex] {
                let influence = 1.0 - cost;
                let blend = influence * influence * (3.0 - 2.0 * influence);
                targets[vertex] = targets[vertex]
                    .max(heights[vertex] + (targets[source] - heights[vertex]) * blend);
            }
            for &neighbour in &graph.neighbours[vertex] {
                // No sphere perimeter rule. Every original submerged vertex is
                // an ocean opening, matching Island's ocean mask at the banks.
                if self.core[neighbour]
                    || self.sea_openings[neighbour]
                    || self.boundary[neighbour]
                    || self.support[neighbour]
                    || original[neighbour] < 0.0
                {
                    continue;
                }
                let next =
                    cost + arc(mesh.directions[vertex], mesh.directions[neighbour]) / blend_width;
                if next < distances[neighbour]
                    || (next.total_cmp(&distances[neighbour]).is_eq()
                        && source < sources[neighbour])
                {
                    distances[neighbour] = next;
                    sources[neighbour] = source;
                    queue.push(State {
                        cost: next,
                        vertex: neighbour,
                    });
                }
            }
        }
        for (i, target) in targets.into_iter().enumerate() {
            if target > heights[i] {
                soil[i] += target - heights[i];
                heights[i] = target;
            }
        }
    }

    /// Edges of the wet corridor's outline, shared by smoothing and bank grading.
    pub fn outline(&self, mesh: &PlanetMesh) -> Vec<(usize, usize)> {
        Self::outline_edges(mesh, &self.core, &self.boundary)
    }

    /// Follow the final water binding, including slopes and any newly dry core.
    pub fn final_outline(&self, mesh: &PlanetMesh, heights: &[f64]) -> Vec<(usize, usize)> {
        let normals = super::normals(mesh, heights);
        let wet: Vec<_> = heights
            .iter()
            .enumerate()
            .map(|(i, &height)| {
                let water = self.water_level(
                    i,
                    height,
                    self.water_weight(i, normals[i], mesh.directions[i]),
                );
                water > 0.0 && (1.0 + water) / (1.0 + height) - 1.0 > 1.0e-10
            })
            .collect();
        let mut boundary = vec![false; heights.len()];
        for face in &mesh.triangles {
            if face.iter().any(|&i| wet[i as usize]) {
                for &i in face {
                    boundary[i as usize] |= !wet[i as usize];
                }
            }
        }
        Self::outline_edges(mesh, &wet, &boundary)
    }

    fn outline_edges(mesh: &PlanetMesh, wet: &[bool], boundary: &[bool]) -> Vec<(usize, usize)> {
        let mut edge_counts = BTreeMap::<_, usize>::new();
        for face in &mesh.triangles {
            if !face.iter().any(|&i| wet[i as usize]) {
                continue;
            }
            for (a, b) in [(face[0], face[1]), (face[1], face[2]), (face[2], face[0])] {
                if boundary[a as usize] && boundary[b as usize] {
                    *edge_counts.entry((a.min(b), a.max(b))).or_default() += 1;
                }
            }
        }
        edge_counts
            .into_iter()
            .filter_map(|((a, b), count)| (count == 1).then_some((a as usize, b as usize)))
            .collect()
    }

    /// Island's two simultaneous XY bank passes, expressed in local tangents.
    /// Terrain and water share these vertices, so the pinned outline moves once.
    pub fn smooth_outline(&self, mesh: &mut PlanetMesh, protected: &[bool]) -> usize {
        let mut adjacent = BTreeMap::<usize, Vec<usize>>::new();
        for (a, b) in self.outline(mesh) {
            adjacent.entry(a).or_default().push(b);
            adjacent.entry(b).or_default().push(a);
        }
        let mut fixed = BTreeSet::new();
        for (&i, neighbours) in &adjacent {
            if neighbours.len() != 2 || protected[i] || self.levels[i] <= 0.0 {
                fixed.insert(i);
                fixed.extend(neighbours.iter().copied());
            }
        }
        let mut moved = BTreeSet::new();
        // The snapshot is necessary for a simultaneous pass; sequential writes
        // bias the outline by vertex order. Reuse candidate storage across passes.
        let mut candidates = vec![None; mesh.directions.len()];
        for _ in 0..2 {
            candidates.fill(None);
            for (&i, neighbours) in &adjacent {
                if fixed.contains(&i) {
                    continue;
                }
                let current = mesh.directions[i];
                let previous = mesh.directions[neighbours[0]];
                let next = mesh.directions[neighbours[1]];
                let average = (previous + next) * 0.5 - current;
                let tangent = (average - current * average.dot(current)) * 0.45;
                let shortest = current.distance(previous).min(current.distance(next));
                let maximum = (shortest * 0.35).min(self.widths[i] * 0.18);
                if maximum > 1.0e-12 && tangent.length_squared() > 1.0e-24 {
                    candidates[i] = Some((current + tangent.clamp_length_max(maximum)).normalize());
                }
            }
            // Test all candidates together, as in Island's XY orientation guard.
            // Reject unsafe moves rather than letting one bank fold another face.
            loop {
                let mut rejected = Vec::new();
                for face in &mesh.triangles {
                    if face.iter().all(|&i| candidates[i as usize].is_none()) {
                        continue;
                    }
                    let before = face.map(|i| mesh.directions[i as usize]);
                    let after =
                        face.map(|i| candidates[i as usize].unwrap_or(mesh.directions[i as usize]));
                    let area = before[0].dot(before[1].cross(before[2]));
                    if after[0].dot(after[1].cross(after[2])) <= (area * 0.05).max(1.0e-18) {
                        rejected.extend(
                            face.iter()
                                .copied()
                                .filter(|&i| candidates[i as usize].is_some()),
                        );
                    }
                }
                if rejected.is_empty() {
                    break;
                }
                for i in rejected {
                    candidates[i as usize] = None;
                }
            }
            for (i, candidate) in candidates.iter().enumerate() {
                if let Some(direction) = candidate {
                    mesh.directions[i] = *direction;
                    moved.insert(i);
                }
            }
        }
        moved.len()
    }

    /// Radial Jacobi port of Island's upstream pin/lift apron: six passes at
    /// strength 0.5, fading to zero two target edges upstream of the lip.
    /// Own one height snapshot so every update sees the same preceding pass.
    pub fn smooth_waterfall_approach(
        &self,
        graph: &SurfaceGraph,
        heights: &mut [f64],
        soil: &mut [f64],
    ) {
        let selected: Vec<_> = (0..heights.len())
            .filter(|&i| {
                !self.sea_openings[i]
                    && self.blend(i).smoothing > 0.0
                    && (self.core[i]
                        || self.boundary[i]
                        || graph.neighbours[i].iter().any(|&j| self.boundary[j]))
            })
            .collect();
        if selected.is_empty() {
            return;
        }
        let mut snapshot = heights.to_vec();
        for _ in 0..6 {
            for &i in &selected {
                let weight = self.blend(i).smoothing;
                let count = u32::try_from(graph.neighbours[i].len() + 1).unwrap();
                let average = (snapshot[i]
                    + graph.neighbours[i]
                        .iter()
                        .map(|&j| snapshot[j])
                        .sum::<f64>())
                    / f64::from(count);
                let mut target = snapshot[i] + (average - snapshot[i]) * (0.5 * weight);
                if self.core[i] {
                    target = target.min(self.levels[i] - FALL_CLEARANCE);
                } else if self.boundary[i] && target > self.levels[i] {
                    // Clamp high terrain banks more strongly toward the lip.
                    // Final water binding uses the independent slope weight.
                    target += (self.levels[i] - FALL_CLEARANCE - target) * weight;
                }
                soil[i] = (soil[i] + target - heights[i]).max(0.0);
                heights[i] = target;
            }
            snapshot.copy_from_slice(heights);
        }
    }

    /// Match Island's full two-ring apron, then fade across two extra rings.
    /// Breadth-first visits use the strongest nearby river influence once,
    /// including on narrow land strips between neighbouring channels.
    fn smoothing_weights(&self, graph: &SurfaceGraph) -> Vec<f64> {
        const FULL_RINGS: u32 = 2;
        const BLEND_RINGS: u32 = 2;
        let mut weights: Vec<_> = self
            .core
            .iter()
            .zip(&self.boundary)
            .map(|(&core, &bank)| f64::from(core || bank))
            .collect();
        let mut frontier: Vec<_> = weights
            .iter()
            .enumerate()
            .filter_map(|(i, &weight)| (weight > 0.0).then_some(i))
            .collect();
        let mut start = 0;
        for ring in 1..=FULL_RINGS + BLEND_RINGS {
            let progress = (f64::from(FULL_RINGS + BLEND_RINGS + 1 - ring)
                / f64::from(BLEND_RINGS + 1))
            .clamp(0.0, 1.0);
            let weight = progress * progress * (3.0 - 2.0 * progress);
            let end = frontier.len();
            for index in start..end {
                for &neighbour in &graph.neighbours[frontier[index]] {
                    if weights[neighbour] == 0.0 {
                        weights[neighbour] = weight;
                        frontier.push(neighbour);
                    }
                }
            }
            start = end;
        }
        weights
    }

    /// Apply the waterfall's two groups of six Jacobi passes to the whole river
    /// corridor and feathered bank apron. The support mask controls water
    /// pinning, never the smoothing extent: stopping at the lip/foot leaves
    /// a transverse ridge.
    /// Preserve drainage by pulling up feeding terrain instead of incising it.
    pub fn relax_river_support(
        &self,
        graph: &SurfaceGraph,
        drainage: &Drainage,
        heights: &mut [f64],
        soil: &mut [f64],
    ) {
        let weights = self.smoothing_weights(graph);
        let selected: Vec<_> = (0..heights.len())
            .filter(|&i| !self.sea_openings[i] && heights[i] >= 0.0 && weights[i] > 0.0)
            .collect();
        if selected.is_empty() {
            return;
        }
        // Ordinary banks may round down to their pinned water plane, but not
        // sink through it. Keep their pre-pass floor fixed while each Jacobi
        // snapshot changes. The face/upstream blend remains free to round.
        let bank_floors: Vec<_> = heights
            .iter()
            .enumerate()
            .map(|(i, &height)| {
                if self.boundary[i] && self.blend(i).normal >= 1.0 {
                    height.min(self.levels[i])
                } else {
                    f64::NEG_INFINITY
                }
            })
            .collect();
        // Limit corridor lifts by the hydraulic clearance of every feeding
        // channel. Propagate those ceilings toward the outlet first, so later
        // upstream pulls cannot fill a channel above its fixed water profile.
        let mut ceilings: Vec<_> = (0..heights.len())
            .map(|i| {
                if self.core[i] || (self.boundary[i] && self.support[i]) {
                    (self.levels[i] - FALL_CLEARANCE).max(heights[i])
                } else {
                    f64::INFINITY
                }
            })
            .collect();
        for &i in drainage.order.iter().rev() {
            if ceilings[i].is_finite()
                && let Some(parent) = drainage.receivers[i]
            {
                ceilings[parent] = ceilings[parent].min(ceilings[i].next_down());
            }
        }
        let mut snapshot = heights.to_vec();
        for _ in 0..12 {
            for &i in &selected {
                let count = u32::try_from(graph.neighbours[i].len() + 1).unwrap();
                let average = (snapshot[i]
                    + graph.neighbours[i]
                        .iter()
                        .map(|&j| snapshot[j])
                        .sum::<f64>())
                    / f64::from(count);
                let floor = drainage.receivers[i]
                    .map_or(f64::NEG_INFINITY, |parent| heights[parent].next_up())
                    .max(bank_floors[i]);
                let (floor, ceiling) = if heights[i] < 0.0 {
                    (floor, ceilings[i].min(0.0_f64.next_down()))
                } else {
                    (floor.max(0.0), ceilings[i])
                };
                let target = (snapshot[i] + (average - snapshot[i]) * (0.5 * weights[i]))
                    .clamp(floor, ceiling);
                soil[i] = (soil[i] + target - heights[i]).max(0.0);
                heights[i] = target;
            }
            // Parent-before-child order restores the minimum downhill grade by
            // lifting connected feeders. Sea roots remain submerged; terrain
            // outside the river/apron and its feeders stays unchanged.
            for &i in &drainage.order {
                if let Some(parent) = drainage.receivers[i] {
                    let target = heights[i].max(heights[parent].next_up());
                    soil[i] += target - heights[i];
                    heights[i] = target;
                }
            }
            snapshot.copy_from_slice(heights);
        }
    }

    fn water_level(&self, i: usize, height: f64, flat: f64) -> f64 {
        if !self.core[i] && !self.boundary[i] {
            return height - 0.00001;
        }
        let regular = if self.boundary[i] {
            self.levels[i].min(height)
        } else {
            self.levels[i]
        };
        let hugging = height + FALL_CLEARANCE;
        let water = hugging + (regular - hugging) * flat;
        // On ordinary reaches the planned profile is also a ceiling: terrain
        // above it is dry land, even inside the footprint. Actual waterfall
        // supports must keep their clearance if final bank grading lifts them.
        let water = if self.support[i] {
            water
        } else {
            water.min(regular)
        };
        // A dry bank is the water/terrain contact. Preserve that contact even
        // when its adjacent river face is steep, rather than wetting the rim.
        if self.boundary[i] {
            water.min(height)
        } else {
            water
        }
    }

    pub fn into_water(mut self, mesh: &PlanetMesh, heights: &[f64]) -> Vec<f64> {
        // Use the final supporting terrain, not normals from a water mesh that
        // already contains the old foot step. The same frozen normal field is
        // used for every vertex, independent of patch masks and update order.
        let normals = super::normals(mesh, heights);
        for (i, &height) in heights.iter().enumerate() {
            self.levels[i] = self.water_level(
                i,
                height,
                self.water_weight(i, normals[i], mesh.directions[i]),
            );
        }
        self.levels
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::DVec3;

    #[test]
    fn sideways_banks_stay_dry_while_downstream_faces_keep_their_clearance() {
        let (_, graph, _) = fixture();
        for up in [
            DVec3::X,
            -DVec3::X,
            DVec3::Y,
            -DVec3::Y,
            DVec3::Z,
            -DVec3::Z,
        ] {
            let flow = up.any_orthonormal_vector();
            let side = up.cross(flow);
            let mut pins = BankPins::new(
                &graph,
                vec![0.04; 8],
                &[1.0; 8],
                vec![0.003; 8],
                vec![false; 8],
            )
            .with_flows(vec![flow; 8]);
            for height in [0.035, 0.039, 0.05] {
                for tilt in [0.0_f64, 0.5, 0.99, 1.0] {
                    let normal = up * tilt + side * (1.0 - tilt * tilt).sqrt();
                    let water = pins.water_level(0, height, pins.water_weight(0, normal, up));
                    assert!(
                        (water - 0.04).abs() < 1e-12,
                        "lateral slope must not wet the bank"
                    );
                    let falling = up * tilt + flow * (1.0 - tilt * tilt).sqrt();
                    assert!(
                        (pins.water_weight(0, falling, up) - flat_weight(falling, up)).abs()
                            < 1e-12,
                        "longitudinal ramps must preserve the original smooth waterfall blend"
                    );
                }
                let face = pins.water_level(0, height, pins.water_weight(0, flow, up));
                assert!((face - (height + FALL_CLEARANCE).min(0.04)).abs() < 1e-12);
                pins.support[0] = true;
                let raised_face = pins.water_level(0, height, pins.water_weight(0, flow, up));
                assert!((raised_face - height - FALL_CLEARANCE).abs() < 1e-12);
                pins.support[0] = false;
            }
        }
    }

    #[test]
    fn steep_gully_water_stays_across_the_bed_instead_of_climbing_the_sides() {
        use super::super::water_mesh;

        for south in [false, true] {
            let mut mesh = PlanetMesh {
                directions: Vec::new(),
                triangles: Vec::new(),
                radius: 1000.0,
            };
            let mut heights = Vec::new();
            let mut flows = Vec::new();
            // Close the gully around the equator so there are no artificial
            // wet mesh boundaries at its ends; every water edge is a real bank.
            for row in 0..128 {
                let x = f64::from(row) * std::f64::consts::TAU / 128.0;
                for y in [-0.004_f64, -0.003, -0.001, 0.0, 0.001, 0.003, 0.004] {
                    let p = DVec3::new(x.cos() * y.cos(), x.sin() * y.cos(), y.sin());
                    let flow = DVec3::new(-x.sin(), x.cos(), 0.0);
                    mesh.directions.push(if south { -p } else { p });
                    flows.push(if south { -flow } else { flow });
                    heights.push(if y.abs() > 0.002 { 0.05 } else { 0.035 });
                }
            }
            for row in 0..128 {
                for column in 0..6 {
                    let i = row * 7 + column;
                    let next = ((row + 1) % 128) * 7 + column;
                    let faces = [[i, next, i + 1], [i + 1, next, next + 1]];
                    mesh.triangles
                        .extend(faces.map(|[a, b, c]| if south { [a, c, b] } else { [a, b, c] }));
                }
            }
            let graph = SurfaceGraph::new(&mesh);
            // Deliberately cover the steep sides with the geometric footprint:
            // high interior vertices must be clipped dry, not coated with water.
            let pins = BankPins::new(
                &graph,
                vec![0.04; 896],
                &[1.0; 896],
                vec![0.005; 896],
                vec![false; 896],
            )
            .with_flows(flows);
            let water = pins.into_water(&mesh, &heights);
            for (&water, &height) in water.iter().zip(&heights) {
                assert!((water - 0.04).abs() < 1e-6);
                if height > 0.04 {
                    assert!(water < height, "the high bank must remain dry");
                }
            }
            let surface = water_mesh::build(&mesh, &heights, &water);
            surface.validate(&mesh, &heights).unwrap();
            assert!(!surface.triangles.is_empty());
            for face in &surface.triangles {
                for &id in face {
                    assert!(surface.positions[id as usize].length() < 1.040_001);
                }
                let [a, b, c] = face.map(|id| surface.positions[id as usize]);
                let normal = (b - a).cross(c - a).normalize();
                assert!(
                    normal.dot(a.normalize()) > 0.99,
                    "water must not have sideways walls"
                );
            }
        }
    }

    #[test]
    fn inclination_controls_water_height_globally_at_every_radial_orientation() {
        let (_, graph, _) = fixture();
        let mut pins = BankPins::new(
            &graph,
            vec![0.04; 8],
            &[1.0, 1.0, -1.0, -1.0, -1.0, -1.0, -1.0, -1.0],
            vec![0.003; 8],
            vec![false; 8],
        );
        let height = 0.039;
        for up in [
            DVec3::X,
            -DVec3::X,
            DVec3::Y,
            -DVec3::Y,
            DVec3::Z,
            -DVec3::Z,
        ] {
            let side = up.any_orthonormal_vector();
            assert!(
                (pins.water_level(0, height, flat_weight(side, up)) - height - FALL_CLEARANCE)
                    .abs()
                    < 1e-12
            );
            assert!((pins.water_level(0, height, flat_weight(up, up)) - 0.04).abs() < 1e-12);
            let tilted = up * 0.5 + side * 0.75_f64.sqrt();
            let halfway = pins.water_level(0, height, flat_weight(tilted, up));
            assert!((halfway - (height + FALL_CLEARANCE + 0.04) * 0.5).abs() < 1e-12);
            let almost_flat = up * 0.99 + side * (1.0_f64 - 0.99 * 0.99).sqrt();
            let toe = pins.water_level(0, height, flat_weight(almost_flat, up));
            assert!(
                (toe - 0.04).abs() < 0.001 * (0.04 - height - FALL_CLEARANCE),
                "nearly flat landings should converge gently to the normal water plane"
            );
            for flat in [0.0, 0.25, 0.5, 0.75, 1.0] {
                assert!(
                    pins.water_level(2, height, flat) <= height,
                    "the bank must never float"
                );
                assert!(
                    pins.water_level(6, height, flat) < height,
                    "dry terrain must stay dry"
                );
            }
        }
        // The old face/AfterFoot flags must not create a jump at equal slope.
        pins.support[0] = true;
        pins.blends = vec![
            WaterfallBlend {
                normal: 0.0,
                smoothing: 0.0
            };
            8
        ];
        pins.blends[1] = WaterfallBlend::default();
        for flat in [0.0, 0.5, 0.99, 1.0] {
            assert_eq!(
                pins.water_level(0, height, flat).to_bits(),
                pins.water_level(1, height, flat).to_bits()
            );
        }
    }

    fn fixture() -> (PlanetMesh, SurfaceGraph, Vec<f64>) {
        let mesh = PlanetMesh {
            directions: [
                (0.0, 0.0),
                (0.001, 0.0),
                (0.0, 0.003),
                (0.0, -0.003),
                (0.0, 0.005),
                (0.0, 0.012),
                (0.0, 0.03),
                (0.0, -0.005),
            ]
            .map(|(x, y)| DVec3::new(x, y, 1.0).normalize())
            .to_vec(),
            triangles: Vec::new(),
            radius: 1000.0,
        };
        let graph = SurfaceGraph {
            neighbours: vec![
                vec![1, 2, 3],
                vec![0, 2, 3],
                vec![0, 1, 4],
                vec![0, 1, 7],
                vec![2, 5],
                vec![4, 6],
                vec![5],
                vec![3],
            ],
            areas: vec![1.0; 8],
        };
        (
            mesh,
            graph,
            vec![0.038, 0.038, 0.01, 0.012, 0.01, 0.01, 0.01, -0.01],
        )
    }

    fn outline_fixture(south: bool) -> (PlanetMesh, BankPins) {
        let mut directions = vec![DVec3::Z];
        directions.extend(
            [
                (-0.01, -0.01),
                (0.0, -0.018),
                (0.01, -0.01),
                (0.01, 0.0),
                (0.01, 0.01),
                (0.0, 0.01),
                (-0.01, 0.01),
                (-0.01, 0.0),
            ]
            .map(|(x, y)| DVec3::new(x, y, 1.0).normalize()),
        );
        if south {
            for direction in &mut directions {
                *direction = DVec3::new(direction.x, -direction.y, -direction.z);
            }
        }
        let mesh = PlanetMesh {
            directions,
            triangles: (1..=8).map(|i| [0, i, i % 8 + 1]).collect(),
            radius: 1000.0,
        };
        let graph = SurfaceGraph::new(&mesh);
        let pins = BankPins::new(
            &graph,
            vec![0.04; 9],
            &[1.0, -1.0, -1.0, -1.0, -1.0, -1.0, -1.0, -1.0, -1.0],
            vec![0.03; 9],
            vec![false; 9],
        );
        (mesh, pins)
    }

    #[test]
    fn tangent_bank_passes_round_the_outline_at_both_poles_without_folding_faces() {
        for south in [false, true] {
            let (mut mesh, pins) = outline_fixture(south);
            let centre = mesh.directions[0];
            let roughness = |mesh: &PlanetMesh| {
                (1..=8)
                    .map(|i| {
                        let previous = mesh.directions[(i + 6) % 8 + 1];
                        let next = mesh.directions[i % 8 + 1];
                        (previous + next - mesh.directions[i] * 2.0).length_squared()
                    })
                    .sum::<f64>()
            };
            let before = roughness(&mesh);
            assert!(pins.smooth_outline(&mut mesh, &[false; 9]) > 0);
            assert!(roughness(&mesh) < before);
            assert_eq!(
                mesh.directions[0], centre,
                "the channel centre must stay fixed"
            );
            for &direction in &mesh.directions {
                assert!((direction.length_squared() - 1.0).abs() < 1.0e-12);
            }
            for face in &mesh.triangles {
                let [a, b, c] = face.map(|i| mesh.directions[i as usize]);
                assert!(a.dot(b.cross(c)) > 0.0);
            }
        }
    }

    #[test]
    fn hydraulic_anchors_and_their_neighbouring_bank_vertices_stay_fixed() {
        let (mut mesh, pins) = outline_fixture(false);
        let before = mesh.directions.clone();
        let mut protected = [false; 9];
        protected[2] = true;
        assert!(pins.smooth_outline(&mut mesh, &protected) > 0);
        for i in [0, 1, 2, 3] {
            assert_eq!(mesh.directions[i], before[i]);
        }
    }

    fn waterfall_fixture() -> (PlanetMesh, SurfaceGraph, BankPins) {
        use super::super::river_profile::Waterfall;
        let fall = Waterfall {
            upper_vertex: 0,
            lower_vertex: 1,
            upstream_vertex: 2,
            lip: DVec3::Z,
            direction: DVec3::X,
            half_width: 0.0025,
            upper_surface: 0.04,
            lower_surface: 0.036,
            face_run: 0.0025,
        };
        let mut mesh = PlanetMesh {
            directions: Vec::new(),
            triangles: Vec::new(),
            radius: 1000.0,
        };
        for row in 0..11 {
            let x = -0.007 + f64::from(row) * 0.001;
            for column in 0..9 {
                let y = -0.004 + f64::from(column) * 0.001;
                mesh.directions
                    .push(DVec3::new(x, y, (1.0 - x * x - y * y).sqrt()));
            }
        }
        for row in 0..10 {
            for column in 0..8 {
                let i = row * 9 + column;
                mesh.triangles
                    .extend([[i, i + 9, i + 1], [i + 1, i + 9, i + 10]]);
            }
        }
        let graph = SurfaceGraph::new(&mesh);
        let levels: Vec<_> = mesh
            .directions
            .iter()
            .map(|&p| fall.surface_at(p).unwrap_or(0.04))
            .collect();
        let distances: Vec<_> = mesh
            .directions
            .iter()
            .map(|p| fall.half_width - p.y.abs())
            .collect();
        let support: Vec<_> = mesh.directions.iter().map(|&p| fall.on_face(p)).collect();
        let blends = mesh.directions.iter().map(|&p| fall.blend(p)).collect();
        let pins = BankPins::new(
            &graph,
            levels,
            &distances,
            vec![fall.half_width; 99],
            support,
        )
        .with_blends(blends);
        (mesh, graph, pins)
    }

    #[test]
    fn final_slope_blend_keeps_waterfall_banks_on_the_actual_rim() {
        let (mesh, _, pins) = waterfall_fixture();
        let original = pins.outline(&mesh);
        let final_edges = pins.final_outline(&mesh, &vec![0.02; mesh.directions.len()]);
        assert_eq!(
            final_edges, original,
            "steep banks stay terrain pinned instead of extending water into the outer apron"
        );
    }

    #[test]
    fn upstream_holes_and_high_banks_smooth_into_a_fully_supported_waterfall_face() {
        use super::super::water_mesh;
        let (mesh, graph, pins) = waterfall_fixture();
        let mut heights = vec![0.039; 99];
        let hole = 3 * 9 + 4; // An eroded gap just upstream of the lip.
        let low_bank = 3 * 9 + 1;
        let high_bank = 4 * 9 + 7;
        heights[hole] = 0.01;
        heights[low_bank] = 0.01;
        heights[high_bank] = 0.08;
        let original = heights.clone();
        let mut soil = vec![0.0; 99];
        pins.lift(&mesh, &graph, &original, &mut heights, &mut soil);
        assert!(heights[low_bank] > original[low_bank] && heights[low_bank] < 0.04);
        let before = heights.clone();
        pins.smooth_waterfall_approach(&graph, &mut heights, &mut soil);
        assert!(
            heights[hole] > before[hole],
            "smooth the unsupported upstream gap upward"
        );
        assert!(
            heights[high_bank] < before[high_bank],
            "clamp the high bank down toward the lip"
        );
        for (i, &height) in heights.iter().enumerate() {
            if pins.blend(i).smoothing <= 0.0 {
                assert_eq!(
                    height.to_bits(),
                    before[i].to_bits(),
                    "face and downstream heights must stay fixed"
                );
            }
        }
        let covered: Vec<_> = (0..99)
            .map(|i| pins.support[i] && (pins.core[i] || pins.boundary[i]))
            .collect();
        let banks: [bool; 99] = pins.boundary.as_slice().try_into().unwrap();
        let water = pins.into_water(&mesh, &heights);
        for (i, &selected) in covered.iter().enumerate() {
            if selected {
                if banks[i] {
                    assert!(water[i] <= heights[i]);
                } else {
                    assert!((water[i] - heights[i] - FALL_CLEARANCE).abs() < 1.0e-12);
                }
            }
        }
        let surface = water_mesh::build(&mesh, &heights, &water);
        let mut face_count = 0;
        for (face_id, face) in mesh.triangles.iter().enumerate() {
            if face.iter().all(|&i| covered[i as usize]) && face.iter().any(|&i| !banks[i as usize])
            {
                assert!(
                    surface.terrain_faces.contains(&face_id),
                    "the waterfall face must have no missing water triangles"
                );
                face_count += 1;
            }
        }
        assert!(face_count > 4);
    }

    #[test]
    fn final_smoothing_fades_across_two_extra_dry_rings() {
        const SIDE: usize = 19;
        let directions = (0..SIDE)
            .flat_map(|y| {
                (0..SIDE).map(move |x| {
                    let x = f64::from(u32::try_from(x).unwrap());
                    let y = f64::from(u32::try_from(y).unwrap());
                    DVec3::new(x * 0.001, y * 0.001, 1.0).normalize()
                })
            })
            .collect();
        let triangles = (0..SIDE - 1)
            .flat_map(|y| {
                (0..SIDE - 1).flat_map(move |x| {
                    let a = u32::try_from(y * SIDE + x).unwrap();
                    let b = a + u32::try_from(SIDE).unwrap();
                    [[a, a + 1, b], [b, a + 1, b + 1]]
                })
            })
            .collect();
        let mesh = PlanetMesh {
            directions,
            triangles,
            radius: 1000.0,
        };
        let graph = SurfaceGraph::new(&mesh);
        let count = SIDE * SIDE;
        // The centre row is wet; the next row on each side is its bank.
        let distances: Vec<_> = (0..SIDE)
            .flat_map(|y| std::iter::repeat_n(if y == SIDE / 2 { 1.0 } else { -1.0 }, SIDE))
            .collect();
        let pins = BankPins::new(
            &graph,
            vec![0.008; count],
            &distances,
            vec![0.003; count],
            vec![false; count],
        );
        let mut heights: Vec<_> = (0..count)
            .map(|i| {
                if (i % SIDE).is_multiple_of(2) {
                    0.01
                } else {
                    0.012
                }
            })
            .collect();
        let mut soil = vec![0.0; count];
        // Isolate the smoothing taper; the next test covers feeding drainage.
        let drainage = Drainage {
            receivers: vec![None; count],
            catchment: vec![1.0; count],
            order: (0..count).collect(),
        };
        // This production call contains both groups of six simultaneous passes.
        pins.relax_river_support(&graph, &drainage, &mut heights, &mut soil);
        for side in [-1_isize, 1] {
            let reduction = |ring: usize| {
                let row = (SIDE / 2)
                    .checked_add_signed(side * (1 + isize::try_from(ring).unwrap()))
                    .unwrap();
                0.012 - heights[row * SIDE + SIDE / 2]
            };
            assert!(reduction(2) > reduction(3));
            assert!(reduction(3) > reduction(4));
            assert!(reduction(4) > 0.0, "the outer blend ring participates");
            assert_eq!(
                reduction(5).to_bits(),
                0.0_f64.to_bits(),
                "terrain beyond the taper stays untouched"
            );
            assert!(
                reduction(4) < reduction(2),
                "the outer seam is smaller than a hard cutoff"
            );
        }
    }

    #[test]
    fn whole_river_relaxation_rounds_both_shoulders_with_or_without_waterfalls_and_preserves_drainage()
     {
        use super::super::{TerrainOptions, drainage};
        for has_waterfall in [true, false] {
            let (mesh, graph, mut pins) = waterfall_fixture();
            if !has_waterfall {
                pins.support.fill(false);
                pins.blends.fill(WaterfallBlend::default());
            }
            let mut heights: Vec<_> = mesh.directions.iter().map(|p| 0.04 - 4.0 * p.x).collect();
            let upstream = 9 + 7;
            let downstream = 8 * 9 + 7;
            for i in [upstream, downstream] {
                assert!(
                    !pins.support[i],
                    "both shoulders lie outside waterfall support"
                );
                heights[i] = 0.08;
            }
            heights[6 * 9 + 7] = 0.08;
            heights[6 * 9 + 4] = 0.001;
            heights[90..].fill(-0.01);
            let mut soil = vec![0.0; heights.len()];
            let (drains, _, _) = drainage::carve(
                &mesh.directions,
                &graph,
                &mut heights,
                &mut soil,
                &TerrainOptions {
                    river_depth: 0.0,
                    ..TerrainOptions::default()
                },
            );
            let before = heights.clone();
            pins.relax_river_support(&graph, &drains, &mut heights, &mut soil);
            assert!(
                heights[6 * 9 + 7] < before[6 * 9 + 7],
                "round the sharp bank beside the waterfall"
            );
            for i in [upstream, downstream] {
                assert!(
                    heights[i] < before[i],
                    "smoothing must continue along the river beyond both ends of a fall"
                );
            }
            for (i, &height) in heights.iter().enumerate() {
                if let Some(parent) = drains.receivers[i] {
                    assert!(height > heights[parent]);
                }
                assert_eq!(height < 0.0, before[i] < 0.0, "preserve sea drainage roots");
                if before[i] < 0.0 {
                    assert_eq!(
                        height.to_bits(),
                        before[i].to_bits(),
                        "river smoothing ends at the sea without lifting the seabed"
                    );
                }
            }
            let covered: Vec<_> = (0..heights.len())
                .map(|i| pins.support[i] && (pins.core[i] || pins.boundary[i]))
                .collect();
            let banks: [bool; 99] = pins.boundary.as_slice().try_into().unwrap();
            let planned: [f64; 99] = pins.levels.as_slice().try_into().unwrap();
            let water = pins.into_water(&mesh, &heights);
            for (i, &selected) in covered.iter().enumerate() {
                if selected {
                    if banks[i] {
                        assert!(water[i] <= heights[i]);
                    } else {
                        let hugging = heights[i] + FALL_CLEARANCE;
                        assert!(
                            water[i] >= hugging.min(planned[i]) - 1e-12
                                && water[i] <= hugging.max(planned[i]) + 1e-12
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn low_banks_are_lifted_with_a_blended_apron_and_final_water_stays_terrain_pinned() {
        let (mesh, graph, mut heights) = fixture();
        let original = heights.clone();
        let pins = BankPins::new(
            &graph,
            vec![0.04; 8],
            &[1.0, 1.0, -1.0, -1.0, -1.0, -1.0, -1.0, -1.0],
            vec![0.003; 8],
            vec![false; 8],
        );
        pins.lift(&mesh, &graph, &original, &mut heights, &mut [0.0; 8]);
        assert_eq!(
            &heights[..2],
            &original[..2],
            "bank lift must leave the wet channel clear"
        );
        assert!((heights[2] - 0.04).abs() < 1.0e-12 && (heights[3] - 0.04).abs() < 1.0e-12);
        assert!(heights[4] > original[4] && heights[4] < heights[2]);
        assert_eq!(
            heights[6].to_bits(),
            original[6].to_bits(),
            "the apron must stop outside its blend width"
        );
        assert_eq!(
            heights[7].to_bits(),
            original[7].to_bits(),
            "a submerged opening must remain open"
        );
        heights[2] -= 0.004; // A late drainage cut must not reopen an air gap.
        let water = pins.into_water(&mesh, &heights);
        assert!((water[2] - heights[2]).abs() < 1.0e-12);
        assert!((water[3] - heights[3]).abs() < 1.0e-12);
        assert!(water[0] > heights[0]);
    }

    #[test]
    fn waterfall_support_raises_low_terrain_and_keeps_dry_bank_contact() {
        let (mesh, graph, mut heights) = fixture();
        let original = heights.clone();
        let mut support = vec![false; 8];
        support[0] = true;
        support[2] = true;
        let pins = BankPins::new(
            &graph,
            vec![0.04; 8],
            &[1.0, 1.0, -1.0, -1.0, -1.0, -1.0, -1.0, -1.0],
            vec![0.003; 8],
            support,
        );
        pins.lift(&mesh, &graph, &original, &mut heights, &mut [0.0; 8]);
        assert!(heights[0] > original[0]);
        let water = pins.into_water(&mesh, &heights);
        assert!((water[0] - heights[0] - FALL_CLEARANCE).abs() < 1.0e-12);
        assert!((water[2] - heights[2]).abs() < 1.0e-12);
    }
}
