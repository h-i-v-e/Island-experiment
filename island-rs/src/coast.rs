#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss
)]

use std::{
    cmp::Ordering,
    collections::{BinaryHeap, HashMap},
    f32::consts::TAU,
};

use crate::{Adjacency, Mesh, Vec2, noise};

const SEA_EPSILON: f32 = 1.0e-7;
const NO_FACE: u32 = u32::MAX;
const WAVE_DIRECTION_COUNT: usize = 16;

#[derive(Clone, Copy, Debug)]
pub(crate) struct TerrainNoiseSample {
    pub continental: f32,
    pub detail: f32,
}

#[must_use]
pub(crate) fn terrain_noise(seed: u64, position: Vec2) -> TerrainNoiseSample {
    TerrainNoiseSample {
        continental: noise::fractal(
            seed ^ 0x243f_6a88_85a3_08d3,
            position.x * 2.2,
            position.y * 2.2,
            5,
        ),
        detail: noise::fractal(
            seed ^ 0x1319_8a2e_0370_7344,
            position.x * 12.0,
            position.y * 12.0,
            4,
        ),
    }
}

impl TerrainNoiseSample {
    #[must_use]
    pub(crate) fn height_component(self) -> f32 {
        self.continental.mul_add(0.78, self.detail * 0.22)
    }

    fn raw_hardness(self) -> f32 {
        self.continental.mul_add(0.8, self.detail * 0.2)
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct GeologyField {
    seed: u64,
    low_raw_hardness: f32,
    inverse_raw_range: f32,
}

impl GeologyField {
    #[must_use]
    pub(crate) fn calibrated(seed: u64, positions: impl Iterator<Item = Vec2>) -> Self {
        let mut samples: Vec<f32> = positions
            .map(|position| terrain_noise(seed, position).raw_hardness())
            .collect();
        samples.sort_unstable_by(f32::total_cmp);
        let last = samples.len().saturating_sub(1);
        let low = samples.get(last * 8 / 100).copied().unwrap_or(-1.0);
        let high = samples.get(last * 92 / 100).copied().unwrap_or(1.0);
        Self {
            seed,
            low_raw_hardness: low,
            inverse_raw_range: 1.0 / (high - low).max(1.0e-5),
        }
    }

    #[must_use]
    fn hardness(self, position: Vec2) -> f32 {
        let normalized = ((terrain_noise(self.seed, position).raw_hardness()
            - self.low_raw_hardness)
            * self.inverse_raw_range)
            .clamp(0.0, 1.0);
        // Applying smoothstep twice expands the coherent hard and soft ends
        // without introducing independent high-frequency geology.
        let hardness = normalized * normalized * (3.0 - 2.0 * normalized);
        hardness * hardness * (3.0 - 2.0 * hardness)
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum CoastScale {
    Coarse,
    Detail,
}

impl CoastScale {
    const fn band_width(self) -> f32 {
        match self {
            Self::Coarse => 0.05,
            Self::Detail => 0.028,
        }
    }

    const fn iterations(self) -> usize {
        match self {
            Self::Coarse => 3,
            Self::Detail => 2,
        }
    }

    const fn erosion_step(self) -> f32 {
        match self {
            Self::Coarse => 0.0024,
            Self::Detail => 0.0011,
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct ShorePoint {
    edge: [u32; 2],
    position: Vec2,
    triangle: u32,
    tangent: Vec2,
    seaward: Vec2,
    curvature: f32,
    exposure: f32,
}

#[derive(Clone, Copy, Debug)]
struct ShoreSegment {
    points: [u32; 2],
}

#[derive(Clone, Debug)]
struct ShoreLoop {
    points: Vec<u32>,
}

#[derive(Clone, Debug, Default)]
struct CoastTopology {
    points: Vec<ShorePoint>,
    segments: Vec<ShoreSegment>,
    loops: Vec<ShoreLoop>,
}

#[derive(Clone, Debug)]
struct CoastalBand {
    distance: Vec<f32>,
    owner: Vec<u32>,
    dual_area: Vec<f32>,
    mean_edge: Vec<f32>,
}

struct CoastScratch {
    deltas: Vec<f32>,
    soft_cover: Vec<f32>,
    sediment: Vec<f64>,
    transported: Vec<f64>,
    deficit: Vec<f64>,
    ratio: Vec<f32>,
}

impl CoastScratch {
    fn new(vertex_count: usize, point_count: usize) -> Self {
        Self {
            deltas: vec![0.0; vertex_count],
            soft_cover: vec![0.0; vertex_count],
            sediment: vec![0.0; point_count],
            transported: vec![0.0; point_count],
            deficit: vec![0.0; point_count],
            ratio: vec![0.0; point_count],
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct DistanceState {
    cost: f32,
    vertex: usize,
}

impl PartialEq for DistanceState {
    fn eq(&self, other: &Self) -> bool {
        self.vertex == other.vertex && self.cost.to_bits() == other.cost.to_bits()
    }
}

impl Eq for DistanceState {}

impl PartialOrd for DistanceState {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for DistanceState {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .cost
            .total_cmp(&self.cost)
            .then_with(|| other.vertex.cmp(&self.vertex))
    }
}

#[derive(Clone, Copy, Debug)]
struct WaveClimate {
    directions: [Vec2; WAVE_DIRECTION_COUNT],
    weights: [f32; WAVE_DIRECTION_COUNT],
    mean_direction: Vec2,
}

impl WaveClimate {
    fn new(seed: u64) -> Self {
        let phase = ((mix(seed ^ 0xd1b5_4a32_d192_ed03) >> 40) as f32 / (1_u32 << 24) as f32) * TAU;
        let prevailing = Vec2::new(phase.cos(), phase.sin());
        let mut directions = [Vec2::ZERO; WAVE_DIRECTION_COUNT];
        let mut weights = [0.0; WAVE_DIRECTION_COUNT];
        let mut total = 0.0;
        let mut mean = Vec2::ZERO;
        for index in 0..WAVE_DIRECTION_COUNT {
            let angle = TAU * index as f32 / WAVE_DIRECTION_COUNT as f32;
            let direction = Vec2::new(angle.cos(), angle.sin());
            let aligned = direction.dot(prevailing).max(0.0);
            let weight = 0.22 + aligned * aligned * 0.78;
            directions[index] = direction;
            weights[index] = weight;
            total += weight;
            mean += direction * weight;
        }
        for weight in &mut weights {
            *weight /= total;
        }
        Self {
            directions,
            weights,
            mean_direction: mean.try_normalize().unwrap_or(prevailing),
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct CoastIterationStats {
    pub eroded: f64,
    pub deposited: f64,
    pub retained: f64,
}

/// Evolves a narrow band around the actual triangle/sea-level contour.
///
/// Existing vertex indices are retained. Selective refinement only appends
/// vertices, which preserves the shared LOD prefixes used by `correct_lods`.
pub(crate) fn evolve(
    mesh: &mut Mesh,
    geology: GeologyField,
    seed: u64,
    erosion_strength: f32,
    beach_strength: f32,
    scale: CoastScale,
) -> CoastIterationStats {
    if erosion_strength == 0.0 {
        return CoastIterationStats::default();
    }

    selectively_refine_coast(mesh, scale.band_width());
    let climate = WaveClimate::new(seed);
    let adjacency = mesh.adjacency();
    let face_adjacency = face_adjacency(mesh);
    let mut topology = CoastTopology::extract(mesh);
    if topology.points.is_empty() {
        return CoastIterationStats::default();
    }
    topology.calculate_geometry(mesh);
    topology.calculate_exposure(mesh, &face_adjacency, climate);
    let band = CoastalBand::new(mesh, &adjacency, &topology, scale.band_width());
    let mut scratch = CoastScratch::new(mesh.vertices.len(), topology.points.len());
    initialize_soft_cover(mesh, &band, scale.band_width(), &mut scratch.soft_cover);
    let mut total = CoastIterationStats::default();
    for _ in 0..scale.iterations() {
        let stats = erode_transport_and_deposit(
            mesh,
            &adjacency,
            &topology,
            &band,
            geology,
            climate,
            erosion_strength,
            beach_strength,
            scale,
            &mut scratch,
        );
        total.eroded += stats.eroded;
        total.deposited += stats.deposited;
        total.retained = stats.retained;
    }
    let exposed_cover = erode_exposed_soft_cover(
        mesh,
        &topology,
        &band,
        erosion_strength,
        scale,
        &mut scratch,
    );
    total.eroded += exposed_cover;
    total.retained = scratch.sediment.iter().sum();
    mesh.uv.clear();
    mesh.uv
        .extend(mesh.vertices.iter().map(|vertex| vertex.truncate()));
    mesh.calculate_normals();
    total
}

fn selectively_refine_coast(mesh: &mut Mesh, _band_width: f32) {
    let mut marked = vec![false; mesh.vertices.len()];
    for triangle in mesh.triangles.chunks_exact(3) {
        let heights = [
            mesh.vertices[triangle[0] as usize].z,
            mesh.vertices[triangle[1] as usize].z,
            mesh.vertices[triangle[2] as usize].z,
        ];
        let crosses = heights.iter().any(|height| *height > SEA_EPSILON)
            && heights.iter().any(|height| *height < -SEA_EPSILON);
        if crosses {
            for &vertex in triangle {
                marked[vertex as usize] = true;
            }
        }
    }
    if marked.iter().any(|&value| value) {
        mesh.tessellate_incident_to(&marked);
        mesh.calculate_normals();
    }
}

impl CoastTopology {
    fn extract(mesh: &Mesh) -> Self {
        let mut topology = Self::default();
        let mut edge_points = HashMap::<u64, u32>::with_capacity(mesh.triangles.len() / 3);
        for (face, triangle) in mesh.triangles.chunks_exact(3).enumerate() {
            let mut crossings = [NO_FACE; 3];
            let mut crossing_count = 0;
            for (a, b) in [
                (triangle[0], triangle[1]),
                (triangle[1], triangle[2]),
                (triangle[2], triangle[0]),
            ] {
                let height_a = mesh.vertices[a as usize].z;
                let height_b = mesh.vertices[b as usize].z;
                let za = signed_height(height_a, a);
                let zb = signed_height(height_b, b);
                if (za > 0.0) == (zb > 0.0) {
                    continue;
                }
                let edge = ordered_edge(a, b);
                let interpolation = if height_a.abs() <= SEA_EPSILON {
                    0.0
                } else if height_b.abs() <= SEA_EPSILON {
                    1.0
                } else {
                    height_a / (height_a - height_b)
                };
                let key = if interpolation <= SEA_EPSILON {
                    (1_u64 << 63) | u64::from(a)
                } else if interpolation >= 1.0 - SEA_EPSILON {
                    (1_u64 << 63) | u64::from(b)
                } else {
                    (u64::from(edge.0) << 32) | u64::from(edge.1)
                };
                let point = *edge_points.entry(key).or_insert_with(|| {
                    let position = mesh.vertices[a as usize]
                        .truncate()
                        .lerp(mesh.vertices[b as usize].truncate(), interpolation);
                    let index = topology.points.len() as u32;
                    topology.points.push(ShorePoint {
                        edge: [edge.0, edge.1],
                        position,
                        triangle: face as u32,
                        tangent: Vec2::ZERO,
                        seaward: Vec2::ZERO,
                        curvature: 0.0,
                        exposure: 0.0,
                    });
                    index
                });
                crossings[crossing_count] = point;
                crossing_count += 1;
            }
            if crossing_count == 2 && crossings[0] != crossings[1] {
                topology.segments.push(ShoreSegment {
                    points: [crossings[0], crossings[1]],
                });
            }
        }
        topology.build_loops();
        topology
    }

    fn build_loops(&mut self) {
        let mut neighbours = vec![[NO_FACE; 2]; self.points.len()];
        let mut degree = vec![0_u8; self.points.len()];
        for segment in &self.segments {
            let [a, b] = segment.points.map(|point| point as usize);
            if degree[a] < 2 {
                neighbours[a][degree[a] as usize] = b as u32;
                degree[a] += 1;
            }
            if degree[b] < 2 {
                neighbours[b][degree[b] as usize] = a as u32;
                degree[b] += 1;
            }
        }
        let mut visited = vec![false; self.points.len()];
        for start in 0..self.points.len() {
            if visited[start] || degree[start] != 2 {
                continue;
            }
            let mut points = Vec::new();
            let mut previous = NO_FACE;
            let mut current = start as u32;
            for _ in 0..=self.points.len() {
                if current == start as u32 && !points.is_empty() {
                    break;
                }
                let index = current as usize;
                if index >= self.points.len() || degree[index] != 2 || visited[index] {
                    points.clear();
                    break;
                }
                visited[index] = true;
                points.push(current);
                let next = if neighbours[index][0] == previous {
                    neighbours[index][1]
                } else {
                    neighbours[index][0]
                };
                previous = current;
                current = next;
            }
            if current == start as u32 && points.len() >= 3 {
                self.loops.push(ShoreLoop { points });
            }
        }
    }

    fn calculate_geometry(&mut self, mesh: &Mesh) {
        let mut seaward_total = vec![Vec2::ZERO; self.points.len()];
        for (point_index, seaward) in seaward_total.iter_mut().enumerate() {
            let point = self.points[point_index];
            let triangle = triangle_at(mesh, point.triangle as usize);
            let a = mesh.vertices[triangle[0] as usize];
            let b = mesh.vertices[triangle[1] as usize];
            let c = mesh.vertices[triangle[2] as usize];
            let ab = b.truncate() - a.truncate();
            let ac = c.truncate() - a.truncate();
            let determinant = ab.perp_dot(ac);
            if determinant.abs() > 1.0e-9 {
                let rise_along_b = b.z - a.z;
                let rise_along_c = c.z - a.z;
                let gradient = Vec2::new(
                    rise_along_b.mul_add(ac.y, -rise_along_c * ab.y),
                    rise_along_c.mul_add(ab.x, -rise_along_b * ac.x),
                ) / determinant;
                *seaward += -gradient.try_normalize().unwrap_or(Vec2::ZERO);
            }
        }
        for shore_loop in &self.loops {
            let length = shore_loop.points.len();
            for index in 0..length {
                let previous = shore_loop.points[(index + length - 1) % length] as usize;
                let current = shore_loop.points[index] as usize;
                let next = shore_loop.points[(index + 1) % length] as usize;
                let incoming = (self.points[current].position - self.points[previous].position)
                    .try_normalize()
                    .unwrap_or(Vec2::X);
                let outgoing = (self.points[next].position - self.points[current].position)
                    .try_normalize()
                    .unwrap_or(incoming);
                self.points[current].tangent =
                    (incoming + outgoing).try_normalize().unwrap_or(outgoing);
                self.points[current].curvature = incoming.perp_dot(outgoing).clamp(-1.0, 1.0);
                self.points[current].seaward = seaward_total[current]
                    .try_normalize()
                    .unwrap_or_else(|| self.points[current].tangent.perp());
            }
        }
    }

    fn calculate_exposure(
        &mut self,
        mesh: &Mesh,
        face_adjacency: &[[u32; 3]],
        climate: WaveClimate,
    ) {
        let maximum_fetch = 0.75;
        let mut maximum = f32::EPSILON;
        for point in &mut self.points {
            let mut exposure = 0.0;
            for index in 0..WAVE_DIRECTION_COUNT {
                let direction = climate.directions[index];
                let incidence = direction.dot(point.seaward).max(0.0);
                if incidence == 0.0 {
                    continue;
                }
                let fetch = trace_fetch(
                    mesh,
                    face_adjacency,
                    point.position,
                    point.triangle,
                    direction,
                    maximum_fetch,
                );
                exposure += climate.weights[index]
                    * (fetch / maximum_fetch).clamp(0.0, 1.0).sqrt()
                    * incidence
                    * incidence;
            }
            let focus = (1.0 + point.curvature * 0.18).clamp(0.82, 1.18);
            point.exposure = exposure * focus;
            maximum = maximum.max(point.exposure);
        }
        for point in &mut self.points {
            point.exposure = (point.exposure / maximum).clamp(0.0, 1.0);
        }
        let mut smoothed = vec![0.0; self.points.len()];
        for _ in 0..2 {
            smoothed.fill(0.0);
            for shore_loop in &self.loops {
                let length = shore_loop.points.len();
                for index in 0..length {
                    let previous = shore_loop.points[(index + length - 1) % length] as usize;
                    let current = shore_loop.points[index] as usize;
                    let next = shore_loop.points[(index + 1) % length] as usize;
                    smoothed[current] = self.points[previous].exposure.mul_add(
                        0.2,
                        self.points[current]
                            .exposure
                            .mul_add(0.6, self.points[next].exposure * 0.2),
                    );
                }
            }
            for (point, value) in self.points.iter_mut().zip(&smoothed) {
                point.exposure = *value;
            }
        }
    }
}

impl CoastalBand {
    fn new(
        mesh: &Mesh,
        adjacency: &Adjacency,
        topology: &CoastTopology,
        maximum_distance: f32,
    ) -> Self {
        let vertex_count = mesh.vertices.len();
        let mut distance = vec![f32::INFINITY; vertex_count];
        let mut owner = vec![NO_FACE; vertex_count];
        let mut queue = BinaryHeap::new();
        for (point_index, point) in topology.points.iter().enumerate() {
            for &vertex in &point.edge {
                let vertex = vertex as usize;
                // A crossing edge may still be longer than the physical band
                // after one bounded refinement. Its endpoints nevertheless
                // bracket the shoreline and must seed the active band.
                let seed_distance = mesh.vertices[vertex]
                    .truncate()
                    .distance(point.position)
                    .min(maximum_distance * 0.9);
                if seed_distance < distance[vertex] {
                    distance[vertex] = seed_distance;
                    owner[vertex] = point_index as u32;
                    queue.push(DistanceState {
                        cost: seed_distance,
                        vertex,
                    });
                }
            }
        }
        while let Some(DistanceState { cost, vertex }) = queue.pop() {
            if cost > distance[vertex] || cost > maximum_distance {
                continue;
            }
            for &neighbour in &adjacency[vertex] {
                let next = cost
                    + mesh.vertices[vertex]
                        .truncate()
                        .distance(mesh.vertices[neighbour].truncate());
                if next < distance[neighbour] && next <= maximum_distance {
                    distance[neighbour] = next;
                    owner[neighbour] = owner[vertex];
                    queue.push(DistanceState {
                        cost: next,
                        vertex: neighbour,
                    });
                }
            }
        }

        let mut dual_area = vec![0.0; vertex_count];
        for triangle in mesh.triangles.chunks_exact(3) {
            let a = mesh.vertices[triangle[0] as usize].truncate();
            let b = mesh.vertices[triangle[1] as usize].truncate();
            let c = mesh.vertices[triangle[2] as usize].truncate();
            let share = (b - a).perp_dot(c - a).abs() / 6.0;
            for &vertex in triangle {
                dual_area[vertex as usize] += share;
            }
        }
        let mean_edge = adjacency
            .iter()
            .enumerate()
            .map(|(vertex, neighbours)| {
                if neighbours.is_empty() {
                    return 0.0;
                }
                neighbours
                    .iter()
                    .map(|&neighbour| {
                        mesh.vertices[vertex]
                            .truncate()
                            .distance(mesh.vertices[neighbour].truncate())
                    })
                    .sum::<f32>()
                    / neighbours.len() as f32
            })
            .collect();
        Self {
            distance,
            owner,
            dual_area,
            mean_edge,
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn erode_transport_and_deposit(
    mesh: &mut Mesh,
    _adjacency: &Adjacency,
    topology: &CoastTopology,
    band: &CoastalBand,
    geology: GeologyField,
    climate: WaveClimate,
    erosion_strength: f32,
    beach_strength: f32,
    scale: CoastScale,
    scratch: &mut CoastScratch,
) -> CoastIterationStats {
    scratch.deltas.fill(0.0);
    let mut eroded_volume = 0.0;
    let band_width = scale.band_width();
    for (index, vertex) in mesh.vertices.iter().enumerate() {
        let owner = band.owner[index];
        if owner == NO_FACE || band.distance[index] > band_width {
            continue;
        }
        let exposure = topology.points[owner as usize].exposure;
        let bedrock_hardness = geology.hardness(vertex.truncate());
        let hardness = effective_surface_hardness(bedrock_hardness, scratch.soft_cover[index]);
        let distance_falloff = smooth_falloff(band.distance[index] / band_width);
        let erosion_profile = coastal_erosion_profile(hardness, vertex.z);
        let attack = scale.erosion_step()
            * erosion_strength.max(0.0)
            * (0.18 + exposure * 0.82)
            * distance_falloff
            * erosion_profile;
        if attack <= 0.0 {
            continue;
        }
        let platform = -0.16 * band.distance[index];
        let desired = if vertex.z < 0.0 {
            (vertex.z - attack).max(platform)
        } else {
            vertex.z - attack
        };
        let cap = (band.mean_edge[index] * 0.18).max(2.5e-5);
        let delta = (desired - vertex.z).clamp(-cap, 0.0);
        scratch.deltas[index] = delta;
        scratch.soft_cover[index] = (scratch.soft_cover[index] + delta).max(0.0);
        let removed = f64::from(-delta) * f64::from(band.dual_area[index].max(0.0));
        scratch.sediment[owner as usize] += removed;
        eroded_volume += removed;
    }
    for (vertex, delta) in mesh.vertices.iter_mut().zip(&scratch.deltas) {
        vertex.z += *delta;
    }

    transport_sediment(
        topology,
        climate.mean_direction,
        &mut scratch.sediment,
        &mut scratch.transported,
    );
    let deposited_volume = if beach_strength > 0.0 {
        deposit_beaches(
            mesh,
            topology,
            band,
            geology,
            beach_strength,
            band_width,
            &mut scratch.sediment,
            &mut scratch.deltas,
            &mut scratch.soft_cover,
            &mut scratch.deficit,
            &mut scratch.ratio,
        )
    } else {
        0.0
    };
    CoastIterationStats {
        eroded: eroded_volume,
        deposited: deposited_volume,
        retained: scratch.sediment.iter().sum(),
    }
}

fn transport_sediment(
    topology: &CoastTopology,
    wave: Vec2,
    sediment: &mut [f64],
    next: &mut [f64],
) {
    next.copy_from_slice(sediment);
    for shore_loop in &topology.loops {
        let length = shore_loop.points.len();
        for index in 0..length {
            let point_index = shore_loop.points[index] as usize;
            let alongshore = wave.dot(topology.points[point_index].tangent);
            let fraction = f64::from(alongshore.abs().min(1.0) * 0.38);
            let moved = sediment[point_index] * fraction;
            let downstream = if alongshore >= 0.0 {
                shore_loop.points[(index + 1) % length]
            } else {
                shore_loop.points[(index + length - 1) % length]
            } as usize;
            next[point_index] -= moved;
            next[downstream] += moved;
        }
    }
    sediment.copy_from_slice(next);
}

#[allow(clippy::too_many_arguments)]
fn deposit_beaches(
    mesh: &mut Mesh,
    topology: &CoastTopology,
    band: &CoastalBand,
    geology: GeologyField,
    strength: f32,
    band_width: f32,
    sediment: &mut [f64],
    deltas: &mut [f32],
    soft_cover: &mut [f32],
    deficit: &mut [f64],
    ratio: &mut [f32],
) -> f64 {
    deltas.fill(0.0);
    deficit.fill(0.0);
    for (index, vertex) in mesh.vertices.iter().enumerate() {
        let owner = band.owner[index];
        if owner == NO_FACE || band.distance[index] > band_width {
            continue;
        }
        let point = topology.points[owner as usize];
        let sheltered = 1.0 - point.exposure;
        let hardness = geology.hardness(vertex.truncate());
        let soft_rock = 1.0 - hardness;
        let eligibility = sheltered * sheltered * soft_rock * soft_rock;
        if eligibility < 0.04 {
            continue;
        }
        let signed = -(vertex.truncate() - point.position).dot(point.seaward);
        let width = band_width * (0.3 + 0.7 * sheltered);
        if signed.abs() > width {
            continue;
        }
        let target = if signed >= 0.0 {
            0.0035 * (signed / width).clamp(0.0, 1.0)
        } else {
            0.012 * (signed / width).clamp(-1.0, 0.0)
        };
        let maximum_raise = 0.0025 * strength.max(0.0) * eligibility;
        let delta = (target - vertex.z).clamp(0.0, maximum_raise);
        if delta == 0.0 {
            continue;
        }
        deltas[index] = delta;
        deficit[owner as usize] += f64::from(delta) * f64::from(band.dual_area[index].max(0.0));
    }
    ratio.fill(0.0);
    for index in 0..sediment.len() {
        ratio[index] = if deficit[index] > 0.0 {
            (sediment[index] / deficit[index]).min(1.0) as f32
        } else {
            0.0
        };
    }
    let mut deposited = 0.0;
    for (index, vertex) in mesh.vertices.iter_mut().enumerate() {
        let owner = band.owner[index];
        if owner == NO_FACE || deltas[index] == 0.0 {
            continue;
        }
        let delta = deltas[index] * ratio[owner as usize];
        vertex.z += delta;
        soft_cover[index] += delta;
        deposited += f64::from(delta) * f64::from(band.dual_area[index].max(0.0));
    }
    for index in 0..sediment.len() {
        let used = deficit[index] * f64::from(ratio[index]);
        sediment[index] = (sediment[index] - used).max(0.0);
    }
    deposited
}

fn initialize_soft_cover(mesh: &Mesh, band: &CoastalBand, band_width: f32, soft_cover: &mut [f32]) {
    for (index, cover) in soft_cover.iter_mut().enumerate() {
        if band.owner[index] == NO_FACE || band.distance[index] > band_width {
            continue;
        }
        let normal_z = mesh.normals.get(index).map_or(0.0, |normal| normal.z);
        *cover = inferred_soft_cover(
            normal_z,
            mesh.vertices[index].z,
            band.distance[index],
            band_width,
        );
    }
}

fn inferred_soft_cover(normal_z: f32, elevation: f32, shore_distance: f32, band_width: f32) -> f32 {
    let gentle = smooth_step(((normal_z - 0.84) / 0.15).clamp(0.0, 1.0));
    let near_sea = smooth_falloff(elevation.abs() / 0.055);
    let near_shore = smooth_falloff(shore_distance / band_width);
    gentle * near_sea * near_shore * 0.012
}

fn effective_surface_hardness(bedrock_hardness: f32, soft_cover: f32) -> f32 {
    let cover_fraction = smooth_step((soft_cover / 0.002).clamp(0.0, 1.0));
    bedrock_hardness * (1.0 - cover_fraction * 0.98)
}

fn erode_exposed_soft_cover(
    mesh: &mut Mesh,
    topology: &CoastTopology,
    band: &CoastalBand,
    erosion_strength: f32,
    scale: CoastScale,
    scratch: &mut CoastScratch,
) -> f64 {
    scratch.deltas.fill(0.0);
    let band_width = scale.band_width();
    let mut eroded = 0.0;
    for (index, vertex) in mesh.vertices.iter().enumerate() {
        let owner = band.owner[index];
        let cover = scratch.soft_cover[index];
        if owner == NO_FACE || cover == 0.0 || band.distance[index] > band_width {
            continue;
        }
        let exposure = topology.points[owner as usize].exposure;
        let exposed = smooth_step(((exposure - 0.12) / 0.88).clamp(0.0, 1.0));
        let distance_falloff = smooth_falloff(band.distance[index] / band_width);
        let requested =
            (scale.erosion_step() * erosion_strength.max(0.0) * 6.0 * exposed * distance_falloff)
                .min(cover);
        let platform = -0.16 * band.distance[index];
        let desired = if vertex.z < 0.0 {
            (vertex.z - requested).max(platform)
        } else {
            vertex.z - requested
        };
        // Loose sediment can be removed much faster than coherent bedrock.
        // This larger edge-relative cap exposes the resistant face instead of
        // leaving a gently sloped apron wrapped around it.
        let cap = (band.mean_edge[index] * 0.75).max(2.5e-5);
        let delta = (desired - vertex.z).clamp(-cap, 0.0);
        let removed = -delta;
        scratch.deltas[index] = delta;
        scratch.soft_cover[index] = (cover - removed).max(0.0);
        let volume = f64::from(removed) * f64::from(band.dual_area[index].max(0.0));
        scratch.sediment[owner as usize] += volume;
        eroded += volume;
    }
    for (vertex, delta) in mesh.vertices.iter_mut().zip(&scratch.deltas) {
        vertex.z += *delta;
    }
    eroded
}

fn smooth_falloff(value: f32) -> f32 {
    let inverse = (1.0 - value).clamp(0.0, 1.0);
    inverse * inverse * (3.0 - 2.0 * inverse)
}

fn smooth_step(value: f32) -> f32 {
    value * value * (3.0 - 2.0 * value)
}

/// Soft material retreats through a broad vertical band, producing bays and
/// beaches. Hard material barely retreats above the shoreline but receives a
/// stronger, narrow wave-toe attack. On a height-field-like surface this
/// steepens the connected faces and preserves a cliff/headland silhouette.
fn coastal_erosion_profile(hardness: f32, elevation: f32) -> f32 {
    let broad = smooth_falloff(elevation.abs() / 0.065);
    let soft_rock = 1.0 - hardness;
    let bulk_retreat = 0.025 + soft_rock.powf(2.8) * 0.8;
    let toe = smooth_falloff((elevation - 0.0025).abs() / 0.014);
    let toe_undercut = hardness.mul_add(hardness * 1.35, 0.08);
    bulk_retreat.mul_add(broad, toe_undercut * toe)
}

fn face_adjacency(mesh: &Mesh) -> Vec<[u32; 3]> {
    let face_count = mesh.triangles.len() / 3;
    let mut adjacency = vec![[NO_FACE; 3]; face_count];
    let mut edges = HashMap::<(u32, u32), (u32, usize)>::with_capacity(mesh.triangles.len());
    for (face, triangle) in mesh.triangles.chunks_exact(3).enumerate() {
        for (side, (a, b)) in [
            (triangle[0], triangle[1]),
            (triangle[1], triangle[2]),
            (triangle[2], triangle[0]),
        ]
        .into_iter()
        .enumerate()
        {
            let key = ordered_edge(a, b);
            if let Some((other_face, other_side)) = edges.remove(&key) {
                adjacency[face][side] = other_face;
                adjacency[other_face as usize][other_side] = face as u32;
            } else {
                edges.insert(key, (face as u32, side));
            }
        }
    }
    adjacency
}

fn trace_fetch(
    mesh: &Mesh,
    face_adjacency: &[[u32; 3]],
    origin: Vec2,
    start_face: u32,
    direction: Vec2,
    maximum: f32,
) -> f32 {
    let mut face = start_face;
    let mut previous = NO_FACE;
    let mut distance = 2.0e-6;
    for _ in 0..face_adjacency.len().min(4096) {
        let triangle = triangle_at(mesh, face as usize);
        let mut exit_distance = f32::INFINITY;
        let mut exit_side = 0;
        for (side, (a, b)) in [
            (triangle[0], triangle[1]),
            (triangle[1], triangle[2]),
            (triangle[2], triangle[0]),
        ]
        .into_iter()
        .enumerate()
        {
            if face_adjacency[face as usize][side] == previous {
                continue;
            }
            let a = mesh.vertices[a as usize].truncate();
            let b = mesh.vertices[b as usize].truncate();
            let edge = b - a;
            let denominator = direction.perp_dot(edge);
            if denominator.abs() < 1.0e-9 {
                continue;
            }
            let offset = a - origin;
            let ray_t = offset.perp_dot(edge) / denominator;
            let edge_t = offset.perp_dot(direction) / denominator;
            if ray_t > distance + 1.0e-6
                && ray_t < exit_distance
                && (-1.0e-5..=1.000_01).contains(&edge_t)
            {
                exit_distance = ray_t;
                exit_side = side;
            }
        }
        if !exit_distance.is_finite() {
            return distance.min(maximum);
        }
        let probe_distance = (exit_distance - 1.0e-5).max(distance);
        if terrain_height_in_face(mesh, triangle, origin + direction * probe_distance) > 0.0 {
            return probe_distance.min(maximum);
        }
        if exit_distance >= maximum {
            return maximum;
        }
        let next = face_adjacency[face as usize][exit_side];
        if next == NO_FACE {
            return maximum;
        }
        previous = face;
        face = next;
        distance = exit_distance;
    }
    distance.min(maximum)
}

fn terrain_height_in_face(mesh: &Mesh, triangle: [u32; 3], point: Vec2) -> f32 {
    let a = mesh.vertices[triangle[0] as usize];
    let b = mesh.vertices[triangle[1] as usize];
    let c = mesh.vertices[triangle[2] as usize];
    let area = (b.truncate() - a.truncate()).perp_dot(c.truncate() - a.truncate());
    if area.abs() < 1.0e-10 {
        return a.z.min(b.z).min(c.z);
    }
    let wa = (b.truncate() - point).perp_dot(c.truncate() - point) / area;
    let wb = (c.truncate() - point).perp_dot(a.truncate() - point) / area;
    let wc = 1.0 - wa - wb;
    wa.mul_add(a.z, wb.mul_add(b.z, wc * c.z))
}

fn triangle_at(mesh: &Mesh, face: usize) -> [u32; 3] {
    let offset = face * 3;
    [
        mesh.triangles[offset],
        mesh.triangles[offset + 1],
        mesh.triangles[offset + 2],
    ]
}

fn signed_height(height: f32, vertex: u32) -> f32 {
    if height.abs() > SEA_EPSILON {
        height
    } else if vertex & 1 == 0 {
        SEA_EPSILON
    } else {
        -SEA_EPSILON
    }
}

fn ordered_edge(a: u32, b: u32) -> (u32, u32) {
    if a < b { (a, b) } else { (b, a) }
}

fn mix(mut value: u64) -> u64 {
    value ^= value >> 30;
    value = value.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value ^= value >> 27;
    value = value.wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn synthetic_island() -> Mesh {
        let mut mesh = Mesh::delaunay(&[
            Vec2::new(0.0, 0.0),
            Vec2::new(0.5, 0.0),
            Vec2::new(1.0, 0.0),
            Vec2::new(1.0, 0.5),
            Vec2::new(1.0, 1.0),
            Vec2::new(0.5, 1.0),
            Vec2::new(0.0, 1.0),
            Vec2::new(0.0, 0.5),
            Vec2::new(0.5, 0.5),
            Vec2::new(0.3, 0.3),
            Vec2::new(0.7, 0.3),
            Vec2::new(0.7, 0.7),
            Vec2::new(0.3, 0.7),
        ]);
        for vertex in &mut mesh.vertices {
            vertex.z = 0.16 - vertex.truncate().distance(Vec2::splat(0.5)) * 0.42;
        }
        mesh.calculate_normals();
        mesh
    }

    #[test]
    fn shared_noise_hardness_is_deterministic_and_correlated() {
        let positions =
            (0..64).map(|index| Vec2::new((index % 8) as f32 / 7.0, (index / 8) as f32 / 7.0));
        let geology = GeologyField::calibrated(42, positions.clone());
        let pairs: Vec<(f32, f32)> = positions
            .map(|position| {
                (
                    terrain_noise(42, position).continental,
                    geology.hardness(position),
                )
            })
            .collect();
        assert_eq!(
            geology.hardness(Vec2::new(0.37, 0.61)).to_bits(),
            geology.hardness(Vec2::new(0.37, 0.61)).to_bits()
        );
        let mean_x = pairs.iter().map(|pair| pair.0).sum::<f32>() / pairs.len() as f32;
        let mean_y = pairs.iter().map(|pair| pair.1).sum::<f32>() / pairs.len() as f32;
        let covariance = pairs
            .iter()
            .map(|pair| (pair.0 - mean_x) * (pair.1 - mean_y))
            .sum::<f32>();
        assert!(covariance > 1.0, "hardness should follow continental noise");
    }

    #[test]
    fn contour_is_closed_and_dual_area_is_conservative() {
        let mesh = synthetic_island();
        let mut topology = CoastTopology::extract(&mesh);
        topology.calculate_geometry(&mesh);
        assert_eq!(topology.loops.len(), 1);
        assert!(topology.loops[0].points.len() >= 3);
        let adjacency = mesh.adjacency();
        let band = CoastalBand::new(&mesh, &adjacency, &topology, 2.0);
        let mesh_area = mesh
            .triangles
            .chunks_exact(3)
            .map(|triangle| {
                let a = mesh.vertices[triangle[0] as usize].truncate();
                let b = mesh.vertices[triangle[1] as usize].truncate();
                let c = mesh.vertices[triangle[2] as usize].truncate();
                (b - a).perp_dot(c - a).abs() * 0.5
            })
            .sum::<f32>();
        assert!((band.dual_area.iter().sum::<f32>() - mesh_area).abs() < 1.0e-5);
    }

    #[test]
    fn transport_on_closed_loop_conserves_sediment() {
        let mesh = synthetic_island();
        let mut topology = CoastTopology::extract(&mesh);
        topology.calculate_geometry(&mesh);
        let mut sediment = vec![0.0; topology.points.len()];
        for (index, value) in sediment.iter_mut().enumerate() {
            *value = (index + 1) as f64;
        }
        let before = sediment.iter().sum::<f64>();
        let mut transported = vec![0.0; sediment.len()];
        transport_sediment(&topology, Vec2::X, &mut sediment, &mut transported);
        assert!((sediment.iter().sum::<f64>() - before).abs() < 1.0e-10);
        assert!(sediment.iter().all(|value| *value >= 0.0));
    }

    #[test]
    fn hard_rock_is_undercut_at_the_toe_without_softening_its_upper_face() {
        let soft_toe = coastal_erosion_profile(0.0, 0.0025);
        let hard_toe = coastal_erosion_profile(1.0, 0.0025);
        let soft_upper = coastal_erosion_profile(0.0, 0.035);
        let hard_upper = coastal_erosion_profile(1.0, 0.035);

        assert!(hard_toe > soft_toe);
        assert!(hard_toe > hard_upper * 20.0);
        assert!(soft_upper > hard_upper * 20.0);
    }

    #[test]
    fn gentle_near_shore_deposits_override_hard_bedrock_until_removed() {
        let cover = inferred_soft_cover(0.995, 0.002, 0.0, 0.05);
        let exposed_hardness = effective_surface_hardness(0.98, cover);
        let uncovered_hardness = effective_surface_hardness(0.98, 0.0);

        assert!(cover > 0.011);
        assert!(exposed_hardness < 0.03);
        assert!(uncovered_hardness > 0.97);
        assert_eq!(
            inferred_soft_cover(0.6, 0.002, 0.0, 0.05).to_bits(),
            0.0_f32.to_bits()
        );
    }

    #[test]
    fn coastal_evolution_is_finite_and_never_reorders_vertices() {
        let mut mesh = synthetic_island();
        let original_vertices = mesh.vertices.clone();
        let geology = GeologyField::calibrated(7, mesh.vertices.iter().map(|v| v.truncate()));
        let stats = evolve(&mut mesh, geology, 7, 1.0, 1.0, CoastScale::Detail);
        assert!(stats.eroded > 0.0);
        assert!(stats.deposited <= stats.eroded + 1.0e-9);
        assert!(mesh.vertices.len() >= original_vertices.len());
        assert!(mesh.vertices.iter().all(|vertex| vertex.is_finite()));
        assert_eq!(
            mesh.vertices
                .iter()
                .take(original_vertices.len())
                .map(|vertex| vertex.truncate())
                .collect::<Vec<_>>(),
            original_vertices
                .iter()
                .map(|vertex| vertex.truncate())
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn zero_strength_preserves_mesh_exactly() {
        let mut mesh = synthetic_island();
        let original = mesh.clone();
        let geology = GeologyField::calibrated(9, mesh.vertices.iter().map(|v| v.truncate()));
        let stats = evolve(&mut mesh, geology, 9, 0.0, 4.0, CoastScale::Coarse);
        assert_eq!(mesh, original);
        assert_eq!(stats.eroded.to_bits(), 0.0_f64.to_bits());
        assert_eq!(stats.deposited.to_bits(), 0.0_f64.to_bits());
    }
}
