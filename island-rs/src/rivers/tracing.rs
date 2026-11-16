use super::{
    Adjacency, ISLAND_WORLD_METRES, Mesh, Ordering, River, RiverNode, RiverSourceRule, Vec2,
    average_edge_length, noise, projected_vertex_control_areas, river_half_width,
};

use super::HashSet;

const RIVER_MEANDER_NOISE_SEED: u64 = 0x6d65_616e_6465_7273;
const RIVER_MEANDER_NOISE_OCTAVES: u8 = 2;
const RIVER_MEANDER_WAVELENGTH_EDGE_MULTIPLIER: f32 = 10.0;
const RIVER_MEANDER_HEIGHT_EDGE_MULTIPLIER: f32 = 0.025;

#[derive(Clone, Copy, Debug)]
pub(super) struct RiverMeanderScale {
    pub(super) wavelength: f32,
    pub(super) height: f32,
}

impl RiverMeanderScale {
    pub(super) fn from_average_edge_length(edge_length: f32) -> Self {
        let edge_length = edge_length.max(f32::EPSILON);
        Self {
            wavelength: edge_length * RIVER_MEANDER_WAVELENGTH_EDGE_MULTIPLIER,
            height: edge_length * RIVER_MEANDER_HEIGHT_EDGE_MULTIPLIER,
        }
    }

    pub(super) fn from_mesh(mesh: &Mesh, adjacency: &Adjacency) -> Self {
        Self::from_average_edge_length(average_edge_length(mesh, adjacency))
    }
}

pub(crate) fn fix_inland_seas(mesh: &mut Mesh, adjacency: &Adjacency) -> Vec<bool> {
    let mut ocean = vec![false; mesh.vertices.len()];
    let corner = mesh
        .vertices
        .iter()
        .enumerate()
        .min_by(|(_, left), (_, right)| {
            left.x
                .total_cmp(&right.x)
                .then_with(|| left.y.total_cmp(&right.y))
        })
        .map(|(vertex, _)| vertex);
    let mut fringe = Vec::new();
    if let Some(corner) = corner {
        ocean[corner] = true;
        fringe.push(corner);
    }
    while let Some(vertex) = fringe.pop() {
        for &neighbour in &adjacency[vertex] {
            if !ocean[neighbour] && mesh.vertices[neighbour].z <= 0.0 {
                ocean[neighbour] = true;
                fringe.push(neighbour);
            }
        }
    }
    for (index, vertex) in mesh.vertices.iter_mut().enumerate() {
        if vertex.z <= 0.0 && !ocean[index] {
            vertex.z = f32::EPSILON;
        }
    }
    ocean
}

/// Treat every submerged external sample as ocean; never alter a pinned coast edge.
pub(crate) fn fix_coastal_inland_seas(mesh: &mut Mesh, adjacency: &Adjacency) -> Vec<bool> {
    let perimeter = mesh.perimeter_mask();
    let mut ocean = vec![false; mesh.vertices.len()];
    let mut fringe = Vec::new();
    for (i, &edge) in perimeter.iter().enumerate() {
        if edge && mesh.vertices[i].z <= 0.0 {
            ocean[i] = true;
            fringe.push(i);
        }
    }
    while let Some(vertex) = fringe.pop() {
        for &neighbor in &adjacency[vertex] {
            if !ocean[neighbor] && mesh.vertices[neighbor].z <= 0.0 {
                ocean[neighbor] = true;
                fringe.push(neighbor);
            }
        }
    }
    for (index, vertex) in mesh.vertices.iter_mut().enumerate() {
        if vertex.z <= 0.0 && !ocean[index] && !perimeter[index] {
            vertex.z = f32::EPSILON;
        }
    }
    ocean
}

#[cfg(test)]
pub(super) fn map_downstream(mesh: &Mesh, adjacency: &Adjacency) -> Vec<usize> {
    adjacency
        .iter()
        .enumerate()
        .map(|(index, neighbours)| {
            neighbours
                .iter()
                .copied()
                .filter(|&neighbour| mesh.vertices[neighbour].z < mesh.vertices[index].z)
                .min_by(|&left, &right| {
                    let left_slope = downhill_slope(mesh, index, left);
                    let right_slope = downhill_slope(mesh, index, right);
                    left_slope.total_cmp(&right_slope).reverse()
                })
                .unwrap_or(index)
        })
        .collect()
}

#[cfg(test)]
pub(super) fn downhill_slope(mesh: &Mesh, from: usize, to: usize) -> f32 {
    let distance = (mesh.vertices[from].truncate() - mesh.vertices[to].truncate())
        .length()
        .max(f32::EPSILON);
    (mesh.vertices[from].z - mesh.vertices[to].z) / distance
}

pub(super) fn river_effective_height(
    mesh: &Mesh,
    vertex: usize,
    seed: u64,
    meander_scale: RiverMeanderScale,
) -> f32 {
    let position = mesh.vertices[vertex].truncate() / meander_scale.wavelength;
    let meander = noise::fractal(
        seed ^ RIVER_MEANDER_NOISE_SEED,
        position.x,
        position.y,
        RIVER_MEANDER_NOISE_OCTAVES,
    );
    meander.mul_add(meander_scale.height, mesh.vertices[vertex].z)
}

#[cfg(test)]
pub(super) fn river_route_score(
    mesh: &Mesh,
    from: usize,
    to: usize,
    seed: u64,
    meander_scale: RiverMeanderScale,
    from_effective_height: f32,
) -> f32 {
    let distance = (mesh.vertices[from].truncate() - mesh.vertices[to].truncate())
        .length()
        .max(f32::EPSILON);
    (from_effective_height - river_effective_height(mesh, to, seed, meander_scale)) / distance
}

pub(super) fn calculate_flow_and_catchment(
    mesh: &Mesh,
    downstream: &[usize],
) -> (Vec<u32>, Vec<f32>) {
    // Terrain height is not a topological order once basin outlets climb to
    // their spill saddle. Process leaves before receivers on the actual graph.
    let order = drainage_order(downstream);
    let mut flow = vec![1_u32; mesh.vertices.len()];
    let world_area_scale = ISLAND_WORLD_METRES * ISLAND_WORLD_METRES;
    let mut catchment_areas = projected_vertex_control_areas(mesh);
    catchment_areas
        .iter_mut()
        .zip(&mesh.vertices)
        .for_each(|(area, vertex)| {
            *area = if vertex.z > 0.0 {
                *area * world_area_scale
            } else {
                0.0
            };
        });
    for vertex in order {
        let next = downstream[vertex];
        if next != vertex {
            flow[next] = flow[next].saturating_add(flow[vertex]);
            catchment_areas[next] += catchment_areas[vertex];
        }
    }
    (flow, catchment_areas)
}

pub(super) fn find_sources(
    mesh: &Mesh,
    _adjacency: &Adjacency,
    downstream: &[usize],
    catchment_areas: &[f32],
    rule: RiverSourceRule,
) -> Vec<usize> {
    let mut sources: Vec<_> = mesh
        .vertices
        .iter()
        .enumerate()
        .filter_map(|(vertex, position)| {
            (position.z > 0.0
                && catchment_areas[vertex]
                    >= rule.required_catchment(
                        source_grade(mesh, vertex, downstream[vertex]),
                        position.z,
                    ))
            .then_some(vertex)
        })
        .collect();
    sources.sort_unstable_by(|&left, &right| {
        catchment_areas[right]
            .total_cmp(&catchment_areas[left])
            .then_with(|| left.cmp(&right))
    });
    sources
}

/// Earlier stages are continuity hints only. The current threshold and its
/// descending catchment order decide eligibility after every topology change.
/// In particular, an inherited anchor cannot reserve or invent a source.
pub(super) fn prioritize_inherited_sources(
    mesh: &Mesh,
    _adjacency: &Adjacency,
    candidates: &[usize],
    inherited_sources: &[Vec2],
    excluded: &[bool],
    catchment_areas: &[f32],
) -> Vec<usize> {
    let anchors: HashSet<_> = inherited_sources
        .iter()
        .filter(|anchor| anchor.is_finite())
        .map(|anchor| (anchor.x.to_bits(), anchor.y.to_bits()))
        .collect();
    let mut sources: Vec<_> = candidates
        .iter()
        .copied()
        .filter(|&vertex| {
            mesh.vertices[vertex].z > 0.0 && !excluded.get(vertex).copied().unwrap_or(false)
        })
        .collect();
    if !anchors.is_empty() {
        let inherited = |vertex: usize| {
            let position = mesh.vertices[vertex];
            anchors.contains(&(position.x.to_bits(), position.y.to_bits()))
        };
        sources.sort_unstable_by(|&left, &right| {
            catchment_areas[right]
                .total_cmp(&catchment_areas[left])
                .then_with(|| inherited(right).cmp(&inherited(left)))
                .then_with(|| left.cmp(&right))
        });
    }
    sources
}

pub(super) fn drainage_order(downstream: &[usize]) -> Vec<usize> {
    let mut pending = vec![0_usize; downstream.len()];
    for (vertex, &receiver) in downstream.iter().enumerate() {
        if receiver != vertex {
            pending[receiver] += 1;
        }
    }
    let mut order: Vec<_> = pending
        .iter()
        .enumerate()
        .filter_map(|(vertex, &count)| (count == 0).then_some(vertex))
        .collect();
    let mut cursor = 0;
    while cursor < order.len() {
        let vertex = order[cursor];
        let receiver = downstream[vertex];
        if receiver != vertex {
            pending[receiver] -= 1;
            if pending[receiver] == 0 {
                order.push(receiver);
            }
        }
        cursor += 1;
    }
    debug_assert_eq!(
        order.len(),
        downstream.len(),
        "river drainage contains a cycle"
    );
    order
}

pub(super) fn source_grade(mesh: &Mesh, from: usize, to: usize) -> f32 {
    if from == to {
        return 0.0;
    }
    let edge = mesh.vertices[from] - mesh.vertices[to];
    (edge.z.max(0.0) / edge.length().max(f32::EPSILON)).clamp(0.0, 1.0)
}

#[cfg(test)]
pub(super) fn trace_rivers(
    mesh: &mut Mesh,
    adjacency: &Adjacency,
    flow: &[u32],
    sources: &[usize],
    ocean: &[bool],
    seed: u64,
) -> (Vec<River>, Vec<Option<usize>>) {
    trace_rivers_avoiding(mesh, adjacency, flow, sources, ocean, seed, &[])
}

#[cfg(test)]
pub(super) fn trace_rivers_avoiding(
    mesh: &mut Mesh,
    adjacency: &Adjacency,
    flow: &[u32],
    sources: &[usize],
    ocean: &[bool],
    seed: u64,
    excluded: &[bool],
) -> (Vec<River>, Vec<Option<usize>>) {
    let drainage = super::drainage::Drainage::new(mesh, adjacency, ocean, excluded, seed);
    let (_, catchment) = calculate_flow_and_catchment(mesh, &drainage.downstream);
    trace_drainage(mesh, &drainage, flow, &catchment, sources, ocean)
}

/// Cover qualifying paths first so a main stem can follow the largest incoming
/// catchment through short connector spans that are not themselves sources.
pub(super) fn preferred_upstream(
    drainage: &super::drainage::Drainage,
    catchment: &[f32],
    sources: &[usize],
    ocean: &[bool],
    order: &[usize],
) -> Vec<Option<usize>> {
    let mut selected = vec![false; drainage.downstream.len()];
    for &source in sources {
        selected[source] = drainage.reaches_ocean[source] && !ocean[source];
    }
    let mut upstream = vec![None::<usize>; drainage.downstream.len()];
    for &vertex in order {
        if !selected[vertex] || ocean[vertex] {
            continue;
        }
        let receiver = drainage.downstream[vertex];
        if receiver == vertex {
            continue;
        }
        selected[receiver] = true;
        let incoming = &mut upstream[receiver];
        if incoming.is_none_or(|old| {
            catchment[vertex]
                .total_cmp(&catchment[old])
                .then_with(|| old.cmp(&vertex))
                .is_gt()
        }) {
            *incoming = Some(vertex);
        }
    }
    upstream
}

pub(super) fn trace_drainage(
    mesh: &Mesh,
    drainage: &super::drainage::Drainage,
    flow: &[u32],
    catchment: &[f32],
    sources: &[usize],
    ocean: &[bool],
) -> (Vec<River>, Vec<Option<usize>>) {
    let mut rivers = Vec::<River>::new();
    let mut joins = Vec::new();
    let mut owners = vec![None; mesh.vertices.len()];
    let order = drainage_order(&drainage.downstream);
    let main_upstream = preferred_upstream(drainage, catchment, sources, ocean, &order);
    for &source in sources {
        if !drainage.reaches_ocean[source] || ocean[source] || owners[source].is_some() {
            continue;
        }
        let mut path = Vec::new();
        let mut vertex = source;
        // A large catchment usually lies near the mouth. Start its strongest
        // selected headwater first, rather than turning every downstream cell
        // into a separate two-node river. Smaller branches then join this stem.
        while let Some(upstream) = main_upstream[vertex].filter(|&i| owners[i].is_none()) {
            vertex = upstream;
        }
        loop {
            path.push(vertex);
            if ocean[vertex] || owners[vertex].is_some() {
                break;
            }
            vertex = drainage.downstream[vertex];
        }
        let join = owners[vertex];
        let join_vertex = join.map(|_| vertex);
        let river_index = rivers.len();
        let nodes = path
            .into_iter()
            .map(|vertex| {
                if !ocean[vertex] {
                    owners[vertex].get_or_insert(river_index);
                }
                RiverNode {
                    vertex,
                    flow: flow[vertex],
                    surface: mesh.vertices[vertex].z,
                    position: mesh.vertices[vertex],
                }
            })
            .collect();
        rivers.push(River { nodes, join });
        joins.push(join_vertex);
    }
    // Spill connectivity is a hydraulic reference, not permission to excavate
    // every downstream vertex to the floor of an upstream depression.
    let mut levels: Vec<_> = mesh.vertices.iter().map(|vertex| vertex.z).collect();
    for &vertex in order.iter().rev() {
        let receiver = drainage.downstream[vertex];
        if owners[vertex].is_some() {
            levels[vertex] = levels[vertex].max(levels[receiver]);
        }
    }
    for river in &mut rivers {
        for node in &mut river.nodes {
            node.surface = levels[node.vertex];
        }
    }
    (rivers, joins)
}

pub(super) fn update_join_flows(rivers: &mut [River], join_vertices: &[Option<usize>]) {
    for river in rivers.iter_mut() {
        let mut running = 0_u32;
        for node in &mut river.nodes {
            running = running.max(node.flow);
            node.flow = running;
        }
    }
    for tributary in (0..rivers.len()).rev() {
        let Some(join) = rivers[tributary].join else {
            continue;
        };
        let Some(join_vertex) = join_vertices[tributary] else {
            continue;
        };
        let Some(last) = rivers[tributary].nodes.last().copied() else {
            continue;
        };
        let mut joined = false;
        for node in &mut rivers[join].nodes {
            if node.vertex == join_vertex {
                joined = true;
            }
            if joined {
                node.flow = node.flow.max(last.flow);
            }
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(super) struct RouteState {
    pub(super) cost: f32,
    pub(super) vertex: usize,
}

impl PartialEq for RouteState {
    fn eq(&self, other: &Self) -> bool {
        self.vertex == other.vertex && self.cost.to_bits() == other.cost.to_bits()
    }
}

impl Eq for RouteState {}

impl PartialOrd for RouteState {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for RouteState {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .cost
            .total_cmp(&self.cost)
            .then_with(|| other.vertex.cmp(&self.vertex))
    }
}

#[derive(Clone, Copy, Debug)]
pub(super) struct RiverClearanceNode {
    pub(super) river: usize,
    pub(super) position: Vec2,
    pub(super) half_width: f32,
}

#[derive(Debug)]
pub(super) struct WaterfallClearanceIndex {
    pub(super) nodes: Vec<RiverClearanceNode>,
    pub(super) base_width: f32,
    pub(super) max_flow: u32,
}

impl WaterfallClearanceIndex {
    pub(super) fn new(rivers: &[River], mesh: &Mesh, max_flow: u32, base_width: f32) -> Self {
        let node_count = rivers.iter().map(|river| river.nodes.len()).sum();
        let mut nodes = Vec::with_capacity(node_count);
        for (river, path) in rivers.iter().enumerate() {
            nodes.extend(path.nodes.iter().map(|node| RiverClearanceNode {
                river,
                position: mesh.vertices[node.vertex].truncate(),
                half_width: river_half_width(node.flow, max_flow, base_width),
            }));
        }
        Self {
            nodes,
            base_width,
            max_flow,
        }
    }

    pub(super) fn conflicts(
        &self,
        river: usize,
        mesh: &Mesh,
        nodes: &[RiverNode],
        segment: usize,
    ) -> bool {
        const SHELF_MARGIN: f32 = 1.35;
        const CHANNEL_MARGIN_EDGES: f32 = 2.0;

        let [start, end] = [nodes[segment], nodes[segment + 1]];
        let [start_position, end_position] =
            [start, end].map(|node| mesh.vertices[node.vertex].truncate());
        let waterfall_half_width =
            river_half_width(start.flow.max(end.flow), self.max_flow, self.base_width);
        let waterfall_radius = waterfall_half_width * SHELF_MARGIN;
        self.nodes.iter().any(|other| {
            if other.river == river {
                return false;
            }
            let clearance =
                waterfall_radius + other.half_width + self.base_width * CHANNEL_MARGIN_EDGES;
            point_segment_distance_squared(other.position, start_position, end_position)
                < clearance * clearance
        })
    }
}

pub(super) fn point_segment_distance_squared(point: Vec2, start: Vec2, end: Vec2) -> f32 {
    let segment = end - start;
    let length_squared = segment.length_squared();
    if length_squared <= f32::EPSILON {
        return point.distance_squared(start);
    }
    let progress = ((point - start).dot(segment) / length_squared).clamp(0.0, 1.0);
    point.distance_squared(segment.mul_add(Vec2::splat(progress), start))
}
