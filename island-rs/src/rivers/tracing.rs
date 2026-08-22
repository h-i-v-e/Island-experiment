use super::{
    Adjacency, BinaryHeap, HashMap, HashSet, ISLAND_WORLD_METRES, MAX_RIVER_RINGS, Mesh, Ordering,
    RIVER_SOURCE_EXCLUSION_CELL_METRES, River, RiverNode, RiverSourceRule, Vec2, Vec3,
    projected_vertex_control_areas, river_half_width, river_ring_count,
};

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

pub(super) fn downhill_slope(mesh: &Mesh, from: usize, to: usize) -> f32 {
    let distance = (mesh.vertices[from].truncate() - mesh.vertices[to].truncate())
        .length()
        .max(f32::EPSILON);
    (mesh.vertices[from].z - mesh.vertices[to].z) / distance
}

pub(super) fn calculate_flow_and_catchment(
    mesh: &Mesh,
    downstream: &[usize],
) -> (Vec<u32>, Vec<f32>) {
    let mut order: Vec<usize> = (0..mesh.vertices.len()).collect();
    order
        .sort_unstable_by(|&left, &right| mesh.vertices[right].z.total_cmp(&mesh.vertices[left].z));
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
    adjacency: &Adjacency,
    downstream: &[usize],
    catchment_areas: &[f32],
    rule: RiverSourceRule,
) -> Vec<usize> {
    let candidates: Vec<bool> = mesh
        .vertices
        .iter()
        .enumerate()
        .map(|(vertex, position)| {
            catchment_areas[vertex]
                >= rule
                    .required_catchment(source_grade(mesh, vertex, downstream[vertex]), position.z)
        })
        .collect();
    let mut sources: Vec<usize> = (0..mesh.vertices.len())
        .filter(|&vertex| {
            candidates[vertex]
                && !adjacency[vertex]
                    .iter()
                    .any(|&neighbour| downstream[neighbour] == vertex && candidates[neighbour])
        })
        .collect();
    sources.sort_unstable_by(|&left, &right| {
        mesh.vertices[left]
            .z
            .total_cmp(&mesh.vertices[right].z)
            .then_with(|| catchment_areas[right].total_cmp(&catchment_areas[left]))
    });
    sources.truncate(96);
    sources
}

pub(super) fn source_grade(mesh: &Mesh, from: usize, to: usize) -> f32 {
    if from == to {
        return 0.0;
    }
    let edge = mesh.vertices[from] - mesh.vertices[to];
    (edge.z.max(0.0) / edge.length().max(f32::EPSILON)).clamp(0.0, 1.0)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct TracedFootprintOwner {
    pub(super) river: usize,
    pub(super) node: usize,
    pub(super) centre: usize,
    pub(super) distance: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct RiverFlowMerge {
    pub(super) river: usize,
    pub(super) join_vertex: usize,
    pub(super) incoming_flow: u32,
}

/// Indexes the adjacency rings that will become river surface, not merely the
/// centreline. This makes confluences agree with the topology later duplicated
/// by `build_mesh_with_mask`.
#[derive(Debug)]
pub(super) struct RiverFootprintIndex {
    pub(super) owners: Vec<Option<TracedFootprintOwner>>,
    pub(super) visited: Vec<u32>,
    pub(super) stamp: u32,
    pub(super) frontier: Vec<(usize, u8)>,
}

impl RiverFootprintIndex {
    pub(super) fn new(vertex_count: usize) -> Self {
        Self {
            owners: vec![None; vertex_count],
            visited: vec![0; vertex_count],
            stamp: 0,
            frontier: Vec::new(),
        }
    }

    pub(super) fn touching(
        &mut self,
        mesh: &Mesh,
        adjacency: &Adjacency,
        centre: usize,
        rings: u8,
    ) -> Option<TracedFootprintOwner> {
        self.begin(centre);
        let centre_height = mesh.vertices[centre].z;
        let mut best = None::<(TracedFootprintOwner, u8, f32)>;
        while let Some((vertex, distance)) = self.frontier.pop() {
            if self.visited[vertex] == self.stamp {
                continue;
            }
            self.visited[vertex] = self.stamp;
            if let Some(owner) = self.owners[vertex] {
                let owner_height = mesh.vertices[owner.centre].z;
                if owner_height <= centre_height + 1.0e-6 {
                    let separation = distance.saturating_add(owner.distance);
                    let drop = (centre_height - owner_height).max(0.0);
                    let replace = best.is_none_or(|(current, current_separation, current_drop)| {
                        separation < current_separation
                            || (separation == current_separation
                                && (drop < current_drop
                                    || (drop.to_bits() == current_drop.to_bits()
                                        && (owner.river, owner.node)
                                            < (current.river, current.node))))
                    });
                    if replace {
                        best = Some((owner, separation, drop));
                    }
                }
            }
            if distance == rings {
                continue;
            }
            self.frontier.extend(
                adjacency[vertex]
                    .iter()
                    .map(|&neighbour| (neighbour, distance + 1)),
            );
        }
        best.map(|(owner, _, _)| owner)
    }

    pub(super) fn register_river(
        &mut self,
        river_index: usize,
        river: &River,
        adjacency: &Adjacency,
        max_flow: u32,
    ) {
        for (node_index, node) in river.nodes.iter().enumerate() {
            self.register_node(
                TracedFootprintOwner {
                    river: river_index,
                    node: node_index,
                    centre: node.vertex,
                    distance: 0,
                },
                adjacency,
                river_ring_count(node.flow, max_flow),
            );
        }
    }

    pub(super) fn register_node(
        &mut self,
        owner: TracedFootprintOwner,
        adjacency: &Adjacency,
        rings: u8,
    ) {
        self.begin(owner.centre);
        while let Some((vertex, distance)) = self.frontier.pop() {
            if self.visited[vertex] == self.stamp {
                continue;
            }
            self.visited[vertex] = self.stamp;
            let candidate = TracedFootprintOwner { distance, ..owner };
            let replace = self.owners[vertex].is_none_or(|current| {
                current.river == owner.river
                    && (distance < current.distance
                        || (distance == current.distance && owner.node > current.node))
            });
            if replace {
                self.owners[vertex] = Some(candidate);
            }
            if distance == rings {
                continue;
            }
            self.frontier.extend(
                adjacency[vertex]
                    .iter()
                    .map(|&neighbour| (neighbour, distance + 1)),
            );
        }
    }

    pub(super) fn begin(&mut self, centre: usize) {
        self.stamp = self.stamp.wrapping_add(1);
        if self.stamp == 0 {
            self.visited.fill(0);
            self.stamp = 1;
        }
        self.frontier.clear();
        self.frontier.push((centre, 0));
    }
}

#[derive(Debug)]
pub(super) struct TracedRiverPath {
    pub(super) vertices: Vec<usize>,
    pub(super) join: Option<usize>,
    pub(super) join_vertex: Option<usize>,
}

/// Coarse world-space occupancy used only to reject later river sources near
/// an accepted river. `Vec<bool>` keeps the fixed grid compact and avoids a
/// hash lookup in the source loop.
#[derive(Debug)]
pub(super) struct RiverSourceExclusionGrid {
    pub(super) minimum_cell_x: i32,
    pub(super) minimum_cell_y: i32,
    pub(super) width: usize,
    pub(super) height: usize,
    pub(super) occupied: Vec<bool>,
}

impl RiverSourceExclusionGrid {
    pub(super) fn new(vertices: &[Vec3]) -> Self {
        let cell_size = Self::cell_size();
        let mut minimum_cell_x = i32::MAX;
        let mut minimum_cell_y = i32::MAX;
        let mut maximum_cell_x = i32::MIN;
        let mut maximum_cell_y = i32::MIN;
        for vertex in vertices {
            let (cell_x, cell_y) = Self::world_cell(*vertex, cell_size);
            minimum_cell_x = minimum_cell_x.min(cell_x);
            minimum_cell_y = minimum_cell_y.min(cell_y);
            maximum_cell_x = maximum_cell_x.max(cell_x);
            maximum_cell_y = maximum_cell_y.max(cell_y);
        }
        let width = (maximum_cell_x - minimum_cell_x + 1) as usize;
        let height = (maximum_cell_y - minimum_cell_y + 1) as usize;
        Self {
            minimum_cell_x,
            minimum_cell_y,
            width,
            height,
            occupied: vec![false; width * height],
        }
    }

    pub(super) fn contains(&self, position: Vec3) -> bool {
        let (cell_x, cell_y) = Self::world_cell(position, Self::cell_size());
        self.index(cell_x, cell_y)
            .is_some_and(|index| self.occupied[index])
    }

    pub(super) fn reserve_path(&mut self, mesh: &Mesh, path: &[usize]) {
        let cell_size = Self::cell_size();
        for &vertex in path {
            let (centre_x, centre_y) = Self::world_cell(mesh.vertices[vertex], cell_size);
            for cell_y in centre_y - 1..=centre_y + 1 {
                for cell_x in centre_x - 1..=centre_x + 1 {
                    if let Some(index) = self.index(cell_x, cell_y) {
                        self.occupied[index] = true;
                    }
                }
            }
        }
    }

    pub(super) const fn cell_size() -> f32 {
        RIVER_SOURCE_EXCLUSION_CELL_METRES / ISLAND_WORLD_METRES
    }

    pub(super) fn world_cell(position: Vec3, cell_size: f32) -> (i32, i32) {
        (
            (position.x / cell_size).floor() as i32,
            (position.y / cell_size).floor() as i32,
        )
    }

    pub(super) fn index(&self, cell_x: i32, cell_y: i32) -> Option<usize> {
        let Ok(x) = usize::try_from(cell_x - self.minimum_cell_x) else {
            return None;
        };
        let Ok(y) = usize::try_from(cell_y - self.minimum_cell_y) else {
            return None;
        };
        if x < self.width && y < self.height {
            Some(y * self.width + x)
        } else {
            None
        }
    }
}

pub(super) struct RiverPathTracer<'a> {
    pub(super) mesh: &'a mut Mesh,
    pub(super) adjacency: &'a Adjacency,
    pub(super) flow: &'a [u32],
    pub(super) ocean: &'a [bool],
    pub(super) occupied: &'a HashMap<usize, usize>,
    pub(super) footprints: &'a mut RiverFootprintIndex,
    pub(super) max_flow: u32,
}

/// Detects when a growing centreline returns within its own future channel
/// footprint. Exact vertex repetition is already rejected by `seen`, but that
/// is insufficient once a river spans several adjacency rings.
pub(super) struct RiverSelfContactIndex {
    pub(super) node_at: Vec<usize>,
    pub(super) visited: Vec<u32>,
    pub(super) stamp: u32,
    pub(super) frontier: Vec<(usize, u8)>,
}

impl RiverSelfContactIndex {
    pub(super) fn new(vertex_count: usize) -> Self {
        Self {
            node_at: vec![usize::MAX; vertex_count],
            visited: vec![0; vertex_count],
            stamp: 0,
            frontier: Vec::new(),
        }
    }

    pub(super) fn register(&mut self, vertex: usize, node: usize) {
        self.node_at[vertex] = node;
    }

    pub(super) fn touches_earlier(
        &mut self,
        adjacency: &Adjacency,
        centre: usize,
        rings: u8,
        earlier_limit: usize,
    ) -> bool {
        if earlier_limit == 0 {
            return false;
        }
        self.stamp = self.stamp.wrapping_add(1);
        if self.stamp == 0 {
            self.visited.fill(0);
            self.stamp = 1;
        }
        self.frontier.clear();
        self.frontier.push((centre, 0));
        while let Some((vertex, distance)) = self.frontier.pop() {
            if self.visited[vertex] == self.stamp {
                continue;
            }
            self.visited[vertex] = self.stamp;
            if self.node_at[vertex] < earlier_limit {
                return true;
            }
            if distance == rings {
                continue;
            }
            self.frontier.extend(
                adjacency[vertex]
                    .iter()
                    .map(|&neighbour| (neighbour, distance + 1)),
            );
        }
        false
    }
}

impl RiverPathTracer<'_> {
    pub(super) fn trace(&mut self, source: usize) -> TracedRiverPath {
        let mut path = vec![source];
        let mut seen = HashSet::from([source]);
        let mut self_contact = RiverSelfContactIndex::new(self.mesh.vertices.len());
        self_contact.register(source, 0);
        let mut join = None;
        let mut join_vertex = None;
        'trace: loop {
            let current = *path.last().expect("river path is non-empty");
            if self.ocean[current] {
                break;
            }
            if current != source {
                let rings = river_ring_count(self.flow[current], self.max_flow);
                if let Some(owner) =
                    self.footprints
                        .touching(self.mesh, self.adjacency, current, rings)
                {
                    join = Some(owner.river);
                    join_vertex = Some(owner.centre);
                    break;
                }
            }
            let mut next = None;
            for &candidate in &self.adjacency[current] {
                if seen.contains(&candidate)
                    || self.mesh.vertices[candidate].z >= self.mesh.vertices[current].z
                {
                    continue;
                }
                let search_rings = river_ring_count(self.flow[candidate], self.max_flow)
                    .saturating_add(MAX_RIVER_RINGS);
                let earlier_limit = path
                    .len()
                    .saturating_sub(usize::from(search_rings).saturating_add(1));
                if self_contact.touches_earlier(
                    self.adjacency,
                    candidate,
                    search_rings,
                    earlier_limit,
                ) {
                    continue;
                }
                let replace = next.is_none_or(|current_best| {
                    downhill_slope(self.mesh, current, candidate)
                        > downhill_slope(self.mesh, current, current_best)
                });
                if replace {
                    next = Some(candidate);
                }
            }
            if let Some(next) = next {
                path.push(next);
                seen.insert(next);
                self_contact.register(next, path.len() - 1);
                continue;
            }
            let Some(escape) = escape_sink(
                self.mesh,
                self.adjacency,
                current,
                &seen,
                self.occupied,
                self.ocean,
            ) else {
                break;
            };
            let sink_height = self.mesh.vertices[current].z;
            for &vertex in escape.iter().skip(1) {
                let search_rings = river_ring_count(self.flow[vertex], self.max_flow)
                    .saturating_add(MAX_RIVER_RINGS);
                let earlier_limit = path
                    .len()
                    .saturating_sub(usize::from(search_rings).saturating_add(1));
                if self_contact.touches_earlier(self.adjacency, vertex, search_rings, earlier_limit)
                {
                    break 'trace;
                }
                self.mesh.vertices[vertex].z = self.mesh.vertices[vertex].z.min(sink_height);
                if seen.insert(vertex) {
                    path.push(vertex);
                    self_contact.register(vertex, path.len() - 1);
                }
            }
        }
        TracedRiverPath {
            vertices: path,
            join,
            join_vertex,
        }
    }
}

pub(super) fn trace_rivers(
    mesh: &mut Mesh,
    adjacency: &Adjacency,
    flow: &[u32],
    sources: &[usize],
    ocean: &[bool],
) -> (Vec<River>, Vec<Option<usize>>) {
    let mut rivers = Vec::<River>::new();
    let mut join_vertices = Vec::<Option<usize>>::new();
    let mut occupied = HashMap::<usize, usize>::new();
    let max_flow = flow.iter().copied().max().unwrap_or(1);
    let mut footprints = RiverFootprintIndex::new(mesh.vertices.len());
    let mut source_exclusion = RiverSourceExclusionGrid::new(&mesh.vertices);
    for &source in sources {
        if source_exclusion.contains(mesh.vertices[source]) {
            continue;
        }
        let source_rings = river_ring_count(flow[source], max_flow);
        if let Some(owner) = footprints.touching(mesh, adjacency, source, source_rings) {
            merge_flow_into_river(
                &mut footprints,
                &mut rivers,
                &join_vertices,
                adjacency,
                max_flow,
                RiverFlowMerge {
                    river: owner.river,
                    join_vertex: owner.centre,
                    incoming_flow: flow[source],
                },
            );
            continue;
        }
        let TracedRiverPath {
            vertices: path,
            join,
            join_vertex,
        } = RiverPathTracer {
            mesh,
            adjacency,
            flow,
            ocean,
            occupied: &occupied,
            footprints: &mut footprints,
            max_flow,
        }
        .trace(source);
        let reaches_outlet = join.is_some() || path.last().is_some_and(|&terminal| ocean[terminal]);
        if !reaches_outlet {
            continue;
        }
        if path.len() < 3 {
            if let (Some(join), Some(join_vertex)) = (join, join_vertex) {
                let incoming_flow = path.iter().map(|&vertex| flow[vertex]).max().unwrap_or(0);
                merge_flow_into_river(
                    &mut footprints,
                    &mut rivers,
                    &join_vertices,
                    adjacency,
                    max_flow,
                    RiverFlowMerge {
                        river: join,
                        join_vertex,
                        incoming_flow,
                    },
                );
            }
            continue;
        }
        let river_index = rivers.len();
        let mut running_flow = 0_u32;
        let nodes = path
            .iter()
            .map(|&vertex| RiverNode {
                vertex,
                flow: {
                    running_flow = running_flow.max(flow[vertex]);
                    running_flow
                },
                surface: mesh.vertices[vertex].z,
                position: mesh.vertices[vertex],
            })
            .collect::<Vec<_>>();
        for &vertex in path.iter().take(path.len().saturating_sub(1)) {
            occupied.entry(vertex).or_insert(river_index);
        }
        source_exclusion.reserve_path(mesh, &path);
        rivers.push(River { nodes, join });
        join_vertices.push(join_vertex);
        update_join_flow_chain(&mut rivers, &join_vertices, river_index);

        if let Some(join) = join {
            register_join_chain(&mut footprints, &rivers, adjacency, max_flow, join);
        }
        footprints.register_river(river_index, &rivers[river_index], adjacency, max_flow);
    }
    (rivers, join_vertices)
}

pub(super) fn merge_flow_into_river(
    footprints: &mut RiverFootprintIndex,
    rivers: &mut [River],
    join_vertices: &[Option<usize>],
    adjacency: &Adjacency,
    max_flow: u32,
    merge: RiverFlowMerge,
) {
    update_join_flow_from(
        rivers,
        join_vertices,
        merge.river,
        merge.join_vertex,
        merge.incoming_flow,
    );
    register_join_chain(footprints, rivers, adjacency, max_flow, merge.river);
}

pub(super) fn update_join_flow_chain(
    rivers: &mut [River],
    join_vertices: &[Option<usize>],
    tributary: usize,
) {
    let incoming_flow = rivers[tributary].nodes.last().map_or(0, |node| node.flow);
    if let (Some(join), Some(target)) = (rivers[tributary].join, join_vertices[tributary]) {
        update_join_flow_from(rivers, join_vertices, join, target, incoming_flow);
    }
}

pub(super) fn update_join_flow_from(
    rivers: &mut [River],
    join_vertices: &[Option<usize>],
    mut river: usize,
    mut target: usize,
    mut incoming_flow: u32,
) {
    loop {
        let Some(start) = rivers[river]
            .nodes
            .iter()
            .position(|node| node.vertex == target)
        else {
            debug_assert!(
                false,
                "river join target is absent from the joined centreline"
            );
            break;
        };
        for node in &mut rivers[river].nodes[start..] {
            incoming_flow = incoming_flow.max(node.flow);
            node.flow = incoming_flow;
        }
        let (Some(next), Some(next_target)) = (rivers[river].join, join_vertices[river]) else {
            break;
        };
        river = next;
        target = next_target;
    }
}

pub(super) fn register_join_chain(
    footprints: &mut RiverFootprintIndex,
    rivers: &[River],
    adjacency: &Adjacency,
    max_flow: u32,
    mut river: usize,
) {
    loop {
        footprints.register_river(river, &rivers[river], adjacency, max_flow);
        let Some(join) = rivers[river].join else {
            break;
        };
        river = join;
    }
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

pub(super) fn escape_sink(
    mesh: &Mesh,
    adjacency: &Adjacency,
    start: usize,
    river_path: &HashSet<usize>,
    occupied: &HashMap<usize, usize>,
    ocean: &[bool],
) -> Option<Vec<usize>> {
    let mut distances = HashMap::from([(start, 0.0_f32)]);
    let mut previous = HashMap::<usize, usize>::new();
    let mut queue = BinaryHeap::from([RouteState {
        cost: 0.0,
        vertex: start,
    }]);
    let target_height = mesh.vertices[start].z;
    let mut target = None;
    while let Some(RouteState { cost, vertex }) = queue.pop() {
        if cost > *distances.get(&vertex).unwrap_or(&f32::INFINITY) {
            continue;
        }
        if vertex != start
            && (mesh.vertices[vertex].z < target_height - f32::EPSILON
                || ocean[vertex]
                || occupied.contains_key(&vertex))
        {
            target = Some(vertex);
            break;
        }
        for &neighbour in &adjacency[vertex] {
            if river_path.contains(&neighbour) {
                continue;
            }
            let distance =
                (mesh.vertices[vertex].truncate() - mesh.vertices[neighbour].truncate()).length();
            let rise = (mesh.vertices[neighbour].z - mesh.vertices[vertex].z).max(0.0);
            let next = cost + distance + rise * 40.0;
            if next < *distances.get(&neighbour).unwrap_or(&f32::INFINITY) {
                distances.insert(neighbour, next);
                previous.insert(neighbour, vertex);
                queue.push(RouteState {
                    cost: next,
                    vertex: neighbour,
                });
            }
        }
    }
    let mut current = target?;
    let mut path = vec![current];
    while current != start {
        current = *previous.get(&current)?;
        path.push(current);
    }
    path.reverse();
    Some(path)
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
