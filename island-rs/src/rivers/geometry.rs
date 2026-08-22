use super::{
    Adjacency, BinaryHeap, ENABLE_FINAL_WATERFALL_REJECTION, FINAL_RIVER_PROFILE_ATTRACTION,
    FINAL_RIVER_RELAXATION, FINAL_RIVER_RELAXATION_PASSES, HashMap, HashSet,
    MAXIMUM_RIVER_BANK_BLEND_WIDTH, MAXIMUM_RIVER_EDGE_LENGTH, MINIMUM_RIVER_BANK_BLEND_WIDTH,
    MINIMUM_RIVER_EDGE_LENGTH, Mesh, PRECARVE_CONFLUENCE_CONNECTOR_RINGS,
    PRECARVE_VALLEY_CENTRE_DEPTH, PRECARVE_VALLEY_OUTER_RINGS, PRECARVE_WATERFALL_BANK_RINGS,
    PRECARVE_WATERFALL_OUTER_RINGS, RIVER_BANK_BLEND_HALF_WIDTH_MULTIPLIER, RIVER_BOUNDARY,
    RIVER_CHANNEL_CLEARANCE_SMOOTHING, RIVER_CHANNEL_CORE_BLEND, RIVER_REFINEMENT_APRON_RINGS,
    RIVER_REFINEMENT_PASSES, RIVER_SURFACE_OFFSET, River, RiverNetwork, RiverSedimentBudget,
    RouteState, SEA_PLANE_CLEARANCE, SHARP_POINT_HEIGHT_RATIO, SHARP_POINT_SMOOTHING,
    SHARP_POINT_SMOOTHING_PASSES, SurfaceMaterial, Vec2, VecDeque,
    WATERFALL_FINAL_SMOOTHING_PASSES, WATERFALL_TARGET_EDGE_LENGTH, WaterfallPatch,
    WaterfallTerrainConstraints, build_river_footprint, derive_waterfall_patches,
    detect_failed_final_waterfalls, duplicate_river_topology, encode_bank_distance_in_uv,
    enforce_final_waterfall_edge_relationships, enforce_waterfall_downstream_ceiling,
    generate_river_rock_mesh, is_river_boundary, mark_river_boundary, pin_waterfalls_to_terrain,
    rebuild_final_waterfall_support_mask, recess_waterfall_notches, river_topology_masks,
    smooth_final_waterfall_patches, smooth_pinned_waterfall_terrain, smoothstep,
    squish_waterfall_downstream_spikes,
};
use crate::mesh::{EdgeSplitStencil, NewVertexStencil};

const COAST_PROJECTED_AREA_EPSILON: f32 = 1.0e-12;

pub(super) fn lower_precarve_river_valleys(
    network: &RiverNetwork,
    mesh: &mut Mesh,
    adjacency: &Adjacency,
) -> usize {
    debug_assert_eq!(adjacency.len(), mesh.vertices.len());
    let mut distances = vec![u8::MAX; mesh.vertices.len()];
    let mut frontiers: [Vec<usize>; PRECARVE_VALLEY_OUTER_RINGS as usize + 1] =
        std::array::from_fn(|_| Vec::new());

    for river in &network.rivers {
        for node in &river.nodes {
            if !network.perimeter.get(node.vertex).copied().unwrap_or(true)
                && !network.ocean.get(node.vertex).copied().unwrap_or(true)
                && distances[node.vertex] != 0
            {
                distances[node.vertex] = 0;
                frontiers[0].push(node.vertex);
            }
        }
    }

    for (river_index, river) in network.rivers.iter().enumerate() {
        let Some((terminal, join)) = river
            .nodes
            .last()
            .map(|node| node.vertex)
            .zip(network.join_vertices.get(river_index).copied().flatten())
        else {
            continue;
        };
        for vertex in confluence_connector(network, adjacency, terminal, join) {
            if distances[vertex] != 0 {
                distances[vertex] = 0;
                frontiers[0].push(vertex);
            }
        }
    }

    for distance in 0..PRECARVE_VALLEY_OUTER_RINGS {
        while let Some(vertex) = frontiers[distance as usize].pop() {
            if distances[vertex] != distance {
                continue;
            }
            for &neighbour in &adjacency[vertex] {
                if network.perimeter.get(neighbour).copied().unwrap_or(true)
                    || network.ocean.get(neighbour).copied().unwrap_or(true)
                    || distances[neighbour] <= distance + 1
                {
                    continue;
                }
                distances[neighbour] = distance + 1;
                frontiers[distance as usize + 1].push(neighbour);
            }
        }
    }

    let mut lowered = 0;
    for (vertex, distance) in distances.into_iter().enumerate() {
        if distance > PRECARVE_VALLEY_OUTER_RINGS {
            continue;
        }
        let progress = f32::from(distance) / f32::from(PRECARVE_VALLEY_OUTER_RINGS + 1);
        let smooth_progress = progress * progress * (3.0 - 2.0 * progress);
        mesh.vertices[vertex].z -= PRECARVE_VALLEY_CENTRE_DEPTH * (1.0 - smooth_progress);
        lowered += 1;
    }
    lowered
}

#[derive(Clone, Copy)]
pub(super) struct WaterfallShoulderCandidate {
    pub(super) vertex: usize,
    pub(super) distance: u8,
    pub(super) owner: RiverChannelFootprintOwner,
}

/// Builds an upper terrace beside each planned waterfall after the broad
/// valley lowering but before channel movement or carving. The channel itself
/// remains untouched: only dry vertices outside its one-ring footprint are
/// raised, with two full-height bank rings and one blended outer ring.
pub(super) fn raise_precarve_waterfall_shoulders(
    network: &RiverNetwork,
    mesh: &mut Mesh,
    adjacency: &Adjacency,
) -> usize {
    debug_assert_eq!(adjacency.len(), mesh.vertices.len());
    let footprint = build_river_footprint(network, mesh, adjacency, false);
    let mut distances = vec![u8::MAX; mesh.vertices.len()];
    let mut targets = vec![f32::NEG_INFINITY; mesh.vertices.len()];
    let mut frontiers: [Vec<WaterfallShoulderCandidate>;
        PRECARVE_WATERFALL_OUTER_RINGS as usize + 1] = std::array::from_fn(|_| Vec::new());

    for (vertex, &remaining) in footprint.coverage.iter().enumerate() {
        if !is_river_boundary(remaining) {
            continue;
        }
        let Some(mut owner) = footprint.owner[vertex].filter(|owner| owner.waterfall_lip) else {
            continue;
        };
        let river = owner.key.river as usize;
        let node = owner.key.node as usize;
        let Some(river_node) = network
            .rivers
            .get(river)
            .and_then(|river| river.nodes.get(node))
        else {
            continue;
        };
        owner.surface += mesh.vertices[river_node.vertex].z - river_node.position.z;
        for &neighbour in &adjacency[vertex] {
            if footprint.coverage[neighbour] == 0 {
                frontiers[1].push(WaterfallShoulderCandidate {
                    vertex: neighbour,
                    distance: 1,
                    owner,
                });
            }
        }
    }

    for distance in 1..=PRECARVE_WATERFALL_OUTER_RINGS {
        while let Some(candidate) = frontiers[distance as usize].pop() {
            let vertex = candidate.vertex;
            if footprint.coverage[vertex] != 0
                || network.perimeter.get(vertex).copied().unwrap_or(true)
                || network.ocean.get(vertex).copied().unwrap_or(true)
                || candidate.distance > distances[vertex]
            {
                continue;
            }
            if candidate.distance == distances[vertex] && targets[vertex] >= candidate.owner.surface
            {
                continue;
            }
            let offset = mesh.vertices[vertex].truncate() - candidate.owner.flow_origin;
            let downstream_tolerance = candidate
                .owner
                .target_half_width
                .max(WATERFALL_TARGET_EDGE_LENGTH)
                * 0.25;
            if offset.dot(candidate.owner.flow_direction) > downstream_tolerance {
                continue;
            }

            distances[vertex] = candidate.distance;
            targets[vertex] = candidate.owner.surface;

            if distance == PRECARVE_WATERFALL_OUTER_RINGS {
                continue;
            }
            for &neighbour in &adjacency[vertex] {
                if footprint.coverage[neighbour] == 0 {
                    frontiers[distance as usize + 1].push(WaterfallShoulderCandidate {
                        vertex: neighbour,
                        distance: distance + 1,
                        owner: candidate.owner,
                    });
                }
            }
        }
    }

    let mut raised = 0;
    for (vertex, (position, target)) in mesh.vertices.iter_mut().zip(targets).enumerate() {
        if !target.is_finite() {
            continue;
        }
        let distance = distances[vertex];
        let influence = if distance <= PRECARVE_WATERFALL_BANK_RINGS {
            1.0
        } else {
            let blend_rings =
                PRECARVE_WATERFALL_OUTER_RINGS.saturating_sub(PRECARVE_WATERFALL_BANK_RINGS) + 1;
            1.0 - smoothstep(
                f32::from(distance - PRECARVE_WATERFALL_BANK_RINGS) / f32::from(blend_rings),
            )
        };
        let target = (target - position.z)
            .max(0.0)
            .mul_add(influence, position.z);
        if target > position.z + f32::EPSILON {
            position.z = target;
            raised += 1;
        }
    }
    raised
}

pub(super) fn confluence_connector(
    network: &RiverNetwork,
    adjacency: &Adjacency,
    start: usize,
    goal: usize,
) -> Vec<usize> {
    let valid = |vertex: usize| {
        vertex < adjacency.len()
            && !network.perimeter.get(vertex).copied().unwrap_or(true)
            && !network.ocean.get(vertex).copied().unwrap_or(true)
    };
    if !valid(start) || !valid(goal) {
        return Vec::new();
    }

    let mut predecessors = vec![usize::MAX; adjacency.len()];
    let mut pending = VecDeque::from([(start, 0_u8)]);
    predecessors[start] = start;
    while let Some((vertex, distance)) = pending.pop_front() {
        if vertex == goal {
            break;
        }
        if distance >= PRECARVE_CONFLUENCE_CONNECTOR_RINGS {
            continue;
        }
        for &neighbour in &adjacency[vertex] {
            if valid(neighbour) && predecessors[neighbour] == usize::MAX {
                predecessors[neighbour] = vertex;
                pending.push_back((neighbour, distance + 1));
            }
        }
    }
    if predecessors[goal] == usize::MAX {
        return Vec::new();
    }

    let mut path = Vec::with_capacity(usize::from(PRECARVE_CONFLUENCE_CONNECTOR_RINGS) + 1);
    let mut vertex = goal;
    loop {
        path.push(vertex);
        if vertex == start {
            break;
        }
        vertex = predecessors[vertex];
    }
    path.reverse();
    path
}

pub(super) fn finalize_river_geometry(
    terrain: &mut Mesh,
    coverage: &[u8],
    surfaces: &[f32],
    river_uv: &[Vec2],
    waterfall_constraints: &WaterfallTerrainConstraints,
) -> (Mesh, Vec<bool>) {
    // Keep the tessellated river topology fixed after carving and smoothing.
    // Flipping an edge here can connect vertices from opposite sides of the
    // settled channel profile, creating sharp bed ridges or accidental dams.
    terrain.calculate_normals();
    let river_bed = river_topology_masks(terrain, coverage).0;
    let mut river_mesh =
        duplicate_river_topology(terrain, coverage, surfaces, river_uv, waterfall_constraints)
            .clipped_above(0.0);
    encode_bank_distance_in_uv(&mut river_mesh);
    (river_mesh, river_bed)
}

pub(super) fn enforce_sea_plane_clearance(terrain: &mut Mesh, ocean: &[bool]) {
    terrain
        .vertices
        .iter_mut()
        .enumerate()
        .for_each(|(index, vertex)| {
            if ocean.get(index).copied().unwrap_or(false) {
                vertex.z = vertex.z.min(-SEA_PLANE_CLEARANCE);
            } else if vertex.z > -SEA_PLANE_CLEARANCE && vertex.z < SEA_PLANE_CLEARANCE {
                vertex.z = if vertex.z > 0.0 {
                    SEA_PLANE_CLEARANCE
                } else {
                    -SEA_PLANE_CLEARANCE
                };
            }
        });
}

pub(super) fn ensure_clear_river_channel(
    network: &RiverNetwork,
    terrain: &mut Mesh,
    coverage: &[u8],
    surfaces: &[f32],
    waterfall_lips: &[bool],
    waterfall_pinned: &[bool],
    waterfall_protected: &[bool],
) -> usize {
    RiverChannelClearance {
        network,
        terrain,
        coverage,
        surfaces,
        waterfall_lips,
        waterfall_pinned,
        waterfall_protected,
    }
    .apply()
}

struct RiverChannelClearance<'a> {
    network: &'a RiverNetwork,
    terrain: &'a mut Mesh,
    coverage: &'a [u8],
    surfaces: &'a [f32],
    waterfall_lips: &'a [bool],
    waterfall_pinned: &'a [bool],
    waterfall_protected: &'a [bool],
}

impl RiverChannelClearance<'_> {
    fn apply(mut self) -> usize {
        self.validate_lengths();
        let adjacency = self.terrain.adjacency();
        let (under_river, banks) = river_topology_masks(self.terrain, self.coverage);
        let mut ceilings = self.initial_ceilings(&under_river, &banks);
        let centre_floors = self.centre_floors(&under_river, &banks, &mut ceilings);
        self.spread_centre_floors(
            &adjacency,
            &under_river,
            &banks,
            &centre_floors,
            &mut ceilings,
        );
        let targets = self.smoothed_targets(&adjacency, &under_river, &banks, &ceilings);
        self.lower_to_targets(targets)
    }

    fn validate_lengths(&self) {
        let vertices = self.terrain.vertices.len();
        debug_assert_eq!(vertices, self.coverage.len());
        debug_assert_eq!(vertices, self.surfaces.len());
        debug_assert_eq!(vertices, self.waterfall_lips.len());
        debug_assert_eq!(vertices, self.waterfall_pinned.len());
        debug_assert_eq!(vertices, self.waterfall_protected.len());
    }

    fn initial_ceilings(&self, under_river: &[bool], banks: &[bool]) -> Vec<f32> {
        (0..self.terrain.vertices.len())
            .map(|vertex| {
                if under_river[vertex]
                    && !banks[vertex]
                    && !self.waterfall_protected[vertex]
                    && self.surfaces[vertex].is_finite()
                {
                    self.surfaces[vertex] - RIVER_SURFACE_OFFSET
                } else {
                    f32::INFINITY
                }
            })
            .collect()
    }

    fn centre_floors(
        &self,
        under_river: &[bool],
        banks: &[bool],
        ceilings: &mut [f32],
    ) -> Vec<f32> {
        let mut floors = vec![f32::INFINITY; self.terrain.vertices.len()];
        for (river_index, river) in self.network.rivers.iter().enumerate() {
            for (node_index, node) in river.nodes.iter().enumerate() {
                if node.vertex >= self.terrain.vertices.len()
                    || banks[node.vertex]
                    || !under_river[node.vertex]
                    || self.waterfall_protected[node.vertex]
                {
                    continue;
                }
                let required_depth = self
                    .network
                    .cross_sections
                    .get(river_index)
                    .and_then(|sections| sections.get(node_index))
                    .map_or(RIVER_SURFACE_OFFSET, |section| {
                        section.required_depth.max(RIVER_SURFACE_OFFSET)
                    });
                let floor =
                    if self.waterfall_lips[node.vertex] || self.waterfall_pinned[node.vertex] {
                        node.surface - RIVER_SURFACE_OFFSET
                    } else {
                        node.surface - required_depth
                    };
                floors[node.vertex] = floors[node.vertex].min(floor);
                ceilings[node.vertex] = ceilings[node.vertex].min(floor);
            }
        }
        floors
    }

    fn spread_centre_floors(
        &self,
        adjacency: &Adjacency,
        under_river: &[bool],
        banks: &[bool],
        centre_floors: &[f32],
        ceilings: &mut [f32],
    ) {
        for (centre, &floor) in centre_floors.iter().enumerate() {
            if !floor.is_finite() {
                continue;
            }
            for &neighbour in &adjacency[centre] {
                if self.is_fixed(neighbour, under_river, banks)
                    || !self.surfaces[neighbour].is_finite()
                {
                    continue;
                }
                let surface_ceiling = self.surfaces[neighbour] - RIVER_SURFACE_OFFSET;
                let core_ceiling = floor + (surface_ceiling - floor) * RIVER_CHANNEL_CORE_BLEND;
                ceilings[neighbour] = ceilings[neighbour].min(core_ceiling);
            }
        }
    }

    fn smoothed_targets(
        &self,
        adjacency: &Adjacency,
        under_river: &[bool],
        banks: &[bool],
        ceilings: &[f32],
    ) -> Vec<f32> {
        let mut targets = self
            .terrain
            .vertices
            .iter()
            .enumerate()
            .map(|(vertex, position)| position.z.min(ceilings[vertex]))
            .collect::<Vec<_>>();
        let snapshot = targets.clone();
        for vertex in 0..self.terrain.vertices.len() {
            if self.is_fixed(vertex, under_river, banks) || !ceilings[vertex].is_finite() {
                continue;
            }
            let (total, count) = adjacency[vertex]
                .iter()
                .copied()
                .filter(|&neighbour| under_river[neighbour])
                .fold((snapshot[vertex], 1_u32), |(total, count), neighbour| {
                    (total + snapshot[neighbour], count + 1)
                });
            let average = total / count as f32;
            let smoothed =
                snapshot[vertex] + (average - snapshot[vertex]) * RIVER_CHANNEL_CLEARANCE_SMOOTHING;
            targets[vertex] = targets[vertex].min(smoothed).min(ceilings[vertex]);
        }
        targets
    }

    fn is_fixed(&self, vertex: usize, under_river: &[bool], banks: &[bool]) -> bool {
        !under_river[vertex]
            || banks[vertex]
            || self.waterfall_lips[vertex]
            || self.waterfall_pinned[vertex]
            || self.waterfall_protected[vertex]
    }

    fn lower_to_targets(&mut self, targets: Vec<f32>) -> usize {
        let mut lowered = 0;
        for (position, target) in self.terrain.vertices.iter_mut().zip(targets) {
            if target < position.z {
                position.z = target;
                lowered += 1;
            }
        }
        lowered
    }
}

pub(super) fn relax_refined_river_surface(
    terrain: &mut Mesh,
    coverage: &[u8],
    surfaces: &mut [f32],
    river_uv: &[Vec2],
    target_half_widths: &[f32],
    target_depths: &[f32],
    waterfall: &WaterfallTerrainConstraints,
) -> usize {
    RefinedRiverSurfaceRelaxer {
        terrain,
        coverage,
        surfaces,
        river_uv,
        target_half_widths,
        target_depths,
        waterfall,
    }
    .relax()
}

struct RefinedRiverSurfaceRelaxer<'a> {
    terrain: &'a mut Mesh,
    coverage: &'a [u8],
    surfaces: &'a mut [f32],
    river_uv: &'a [Vec2],
    target_half_widths: &'a [f32],
    target_depths: &'a [f32],
    waterfall: &'a WaterfallTerrainConstraints,
}

impl RefinedRiverSurfaceRelaxer<'_> {
    fn relax(mut self) -> usize {
        self.validate_lengths();
        let adjacency = self.terrain.adjacency();
        let perimeter = self.terrain.perimeter_mask();
        let (_, banks) = river_topology_masks(self.terrain, self.coverage);
        let patch =
            river_corridor_apron_mask(&adjacency, self.coverage, RIVER_REFINEMENT_APRON_RINGS);
        let movable = self.movable_mask(&adjacency, &perimeter, &banks, &patch);
        let profile_targets = self.profile_targets();
        let relaxed = self.relax_passes(&adjacency, &banks, &movable, &profile_targets);
        let ceiling_restored =
            enforce_waterfall_downstream_ceiling(self.terrain, &self.waterfall.terrain_ceiling);
        relaxed
            + ceiling_restored
            + squish_waterfall_downstream_spikes(
                self.terrain,
                self.surfaces,
                &self.waterfall.water_unclamped,
            )
    }

    fn validate_lengths(&self) {
        let vertices = self.terrain.vertices.len();
        debug_assert_eq!(vertices, self.coverage.len());
        debug_assert_eq!(vertices, self.surfaces.len());
        debug_assert_eq!(vertices, self.river_uv.len());
        debug_assert_eq!(vertices, self.target_half_widths.len());
        debug_assert_eq!(vertices, self.target_depths.len());
        debug_assert_eq!(vertices, self.waterfall.patch.len());
        debug_assert_eq!(vertices, self.waterfall.pinned.len());
    }

    fn movable_mask(
        &self,
        adjacency: &Adjacency,
        perimeter: &[bool],
        banks: &[bool],
        patch: &[bool],
    ) -> Vec<bool> {
        patch
            .iter()
            .enumerate()
            .map(|(vertex, &selected)| {
                selected
                    && !perimeter[vertex]
                    && !banks[vertex]
                    && !self.waterfall.patch[vertex]
                    && !self.waterfall.pinned[vertex]
                    && !adjacency[vertex].is_empty()
                    && adjacency[vertex].iter().all(|&neighbour| patch[neighbour])
            })
            .collect()
    }

    fn profile_targets(&self) -> Vec<Option<f32>> {
        self.terrain
            .vertices
            .iter()
            .enumerate()
            .map(|(vertex, position)| self.profile_target(vertex, position.z))
            .collect()
    }

    fn profile_target(&self, vertex: usize, terrain_height: f32) -> Option<f32> {
        if self.waterfall.patch[vertex] {
            return Some(terrain_height);
        }
        let surface = self.surfaces[vertex];
        let half_width = self.target_half_widths[vertex];
        let depth = self.target_depths[vertex];
        if self.coverage[vertex] == 0
            || !surface.is_finite()
            || half_width <= f32::EPSILON
            || depth <= RIVER_SURFACE_OFFSET
        {
            return None;
        }
        let lateral = (self.river_uv[vertex].x.abs() / half_width).clamp(0.0, 1.0);
        Some(depth.mul_add(smoothstep(lateral), surface - depth))
    }

    fn relax_passes(
        &mut self,
        adjacency: &Adjacency,
        banks: &[bool],
        movable: &[bool],
        profile_targets: &[Option<f32>],
    ) -> usize {
        let mut snapshot = self
            .terrain
            .vertices
            .iter()
            .map(|vertex| vertex.z)
            .collect::<Vec<_>>();
        let mut moved = vec![false; self.terrain.vertices.len()];
        for _ in 0..FINAL_RIVER_RELAXATION_PASSES {
            self.relax_pass(
                adjacency,
                banks,
                movable,
                profile_targets,
                &snapshot,
                &mut moved,
            );
            snapshot
                .iter_mut()
                .zip(&self.terrain.vertices)
                .for_each(|(height, vertex)| *height = vertex.z);
        }
        moved.into_iter().filter(|&was_moved| was_moved).count()
    }

    fn relax_pass(
        &mut self,
        adjacency: &Adjacency,
        banks: &[bool],
        movable: &[bool],
        profile_targets: &[Option<f32>],
        snapshot: &[f32],
        moved: &mut [bool],
    ) {
        for vertex in 0..self.terrain.vertices.len() {
            if !movable[vertex] {
                continue;
            }
            let average = adjacency[vertex]
                .iter()
                .map(|&neighbour| snapshot[neighbour])
                .sum::<f32>()
                / adjacency[vertex].len() as f32;
            let relaxed =
                (average - snapshot[vertex]).mul_add(FINAL_RIVER_RELAXATION, snapshot[vertex]);
            let mut target = profile_targets[vertex].map_or(relaxed, |profile| {
                (profile - relaxed).mul_add(FINAL_RIVER_PROFILE_ATTRACTION, relaxed)
            });
            if self.coverage[vertex] != 0
                && profile_targets[vertex].is_none()
                && !self.waterfall.patch[vertex]
            {
                target = target.min(snapshot[vertex]);
            }
            let ceiling = if self.coverage[vertex] != 0
                && !banks[vertex]
                && !self.waterfall.support[vertex]
                && self.surfaces[vertex].is_finite()
            {
                self.surfaces[vertex] - RIVER_SURFACE_OFFSET
            } else {
                f32::INFINITY
            };
            let bank_floor = if banks[vertex] && self.surfaces[vertex].is_finite() {
                self.surfaces[vertex]
            } else {
                f32::NEG_INFINITY
            };
            target = target.min(ceiling).max(bank_floor);
            if (target - self.terrain.vertices[vertex].z).abs() > f32::EPSILON {
                self.terrain.vertices[vertex].z = target;
                moved[vertex] = true;
            }
        }
    }
}

fn refine_river_corridor_mesh(
    terrain: &mut Mesh,
    material: &mut SurfaceMaterial,
    buffers: &mut RiverMeshBuffers,
) -> usize {
    let mut added_vertices = 0;
    for _ in 0..RIVER_REFINEMENT_PASSES {
        let adjacency = terrain.adjacency();
        let edge_targets = river_refinement_edge_targets(
            &adjacency,
            &buffers.coverage,
            &buffers.target_half_widths,
            RIVER_REFINEMENT_APRON_RINGS,
        );
        let mut marked = vec![false; terrain.vertices.len()];
        for triangle in terrain.triangles.chunks_exact(3) {
            let indices = [
                triangle[0] as usize,
                triangle[1] as usize,
                triangle[2] as usize,
            ];
            let target = indices
                .iter()
                .map(|&vertex| edge_targets[vertex])
                .fold(f32::INFINITY, f32::min);
            if !target.is_finite() {
                continue;
            }
            let [a, b, c] = indices.map(|vertex| terrain.vertices[vertex].truncate());
            let longest_edge = a.distance(b).max(b.distance(c)).max(c.distance(a));
            if longest_edge > target {
                for vertex in indices {
                    marked[vertex] = true;
                }
            }
        }
        if !marked.iter().any(|&vertex| vertex) {
            break;
        }

        let loose_volume = material.volume(terrain);
        let stencils = terrain.tessellate_incident_to(&marked);
        material.extend_after_tessellation(loose_volume, terrain, &stencils);
        buffers.extend_after_tessellation(&stencils);
        added_vertices += stencils.len();

        for value in &mut buffers.coverage {
            *value &= !RIVER_BOUNDARY;
        }
        let adjacency = terrain.adjacency();
        let perimeter = terrain.perimeter_mask();
        mark_river_boundary(&adjacency, &perimeter, &mut buffers.coverage);
    }
    added_vertices
}

pub(super) fn river_refinement_edge_targets(
    adjacency: &Adjacency,
    coverage: &[u8],
    target_half_widths: &[f32],
    apron_rings: u8,
) -> Vec<f32> {
    let mut targets = vec![f32::INFINITY; coverage.len()];
    let mut frontier = Vec::new();
    for (vertex, (&remaining, &half_width)) in coverage.iter().zip(target_half_widths).enumerate() {
        if remaining == 0 || half_width <= 0.0 {
            continue;
        }
        targets[vertex] = half_width.clamp(MINIMUM_RIVER_EDGE_LENGTH, MAXIMUM_RIVER_EDGE_LENGTH);
        frontier.push(vertex);
    }
    for _ in 0..apron_rings {
        let mut next = Vec::new();
        for vertex in frontier {
            let target = targets[vertex];
            for &neighbour in &adjacency[vertex] {
                if target < targets[neighbour] {
                    targets[neighbour] = target;
                    next.push(neighbour);
                }
            }
        }
        frontier = next;
    }
    targets
}

fn extend_river_attributes_after_tessellation(
    stencils: &[crate::mesh::NewVertexStencil],
    buffers: &mut RiverMeshBuffers,
) {
    let RiverMeshBuffers {
        coverage,
        surfaces,
        river_uv,
        owners,
        waterfall_lips,
        target_half_widths,
        target_depths,
    } = buffers;
    coverage.reserve(stencils.len());
    surfaces.reserve(stencils.len());
    river_uv.reserve(stencils.len());
    owners.reserve(stencils.len());
    waterfall_lips.reserve(stencils.len());
    target_half_widths.reserve(stencils.len());
    target_depths.reserve(stencils.len());
    for stencil in stencils {
        debug_assert_eq!(stencil.vertex as usize, coverage.len());
        let [a, b] = [
            stencil.surrounding[0] as usize,
            stencil.surrounding[1] as usize,
        ];
        let selected = coverage[a] != 0 && coverage[b] != 0;
        let owner = interpolated_river_owner(owners, surfaces, a, b, selected);
        coverage.push(if selected {
            coverage[a].min(coverage[b]) & !RIVER_BOUNDARY
        } else {
            0
        });
        surfaces.push(if selected {
            (surfaces[a] + surfaces[b]) * 0.5
        } else {
            0.0
        });
        river_uv.push(if selected {
            (river_uv[a] + river_uv[b]) * 0.5
        } else {
            Vec2::ZERO
        });
        owners.push(owner);
        waterfall_lips.push(selected && waterfall_lips[a] && waterfall_lips[b]);
        target_half_widths.push(target_half_widths[a].max(target_half_widths[b]));
        target_depths.push(if selected {
            (target_depths[a] + target_depths[b]) * 0.5
        } else {
            0.0
        });
    }
}

pub(super) fn interpolated_river_owner(
    owners: &[Option<RiverOwnerKey>],
    surfaces: &[f32],
    a: usize,
    b: usize,
    selected: bool,
) -> Option<RiverOwnerKey> {
    if !selected {
        return None;
    }
    match (owners[a], owners[b]) {
        (same @ Some(_), other) if same == other => same,
        (Some(left), Some(right)) => match surfaces[a].total_cmp(&surfaces[b]) {
            std::cmp::Ordering::Less => Some(left),
            std::cmp::Ordering::Greater => Some(right),
            std::cmp::Ordering::Equal => Some(left.min(right)),
        },
        (owner @ Some(_), None) | (None, owner @ Some(_)) => owner,
        (None, None) => None,
    }
}

fn lerp_scalar(a: f32, b: f32, interpolation: f32) -> f32 {
    (b - a).mul_add(interpolation, a)
}

fn extend_waterfall_constraints_after_edge_splits(
    constraints: &mut WaterfallTerrainConstraints,
    stencils: &[EdgeSplitStencil],
) {
    for stencil in stencils {
        debug_assert_eq!(stencil.vertex as usize, constraints.patch.len());
        let [a, b] = stencil.edge.map(|vertex| vertex as usize);
        constraints
            .patch
            .push(constraints.patch[a] || constraints.patch[b]);
        constraints
            .pinned
            .push(constraints.pinned[a] || constraints.pinned[b]);
        constraints
            .support
            .push(constraints.support[a] || constraints.support[b]);
        constraints
            .water_unclamped
            .push(constraints.water_unclamped[a] || constraints.water_unclamped[b]);
        constraints.terrain_ceiling.push(interpolate_ceiling(
            constraints.terrain_ceiling[a],
            constraints.terrain_ceiling[b],
            stencil.interpolation,
        ));
    }
}

fn extend_waterfall_constraints_after_tessellation(
    constraints: &mut WaterfallTerrainConstraints,
    stencils: &[NewVertexStencil],
) {
    for stencil in stencils {
        debug_assert_eq!(stencil.vertex as usize, constraints.patch.len());
        let [a, b] = [
            stencil.surrounding[0] as usize,
            stencil.surrounding[1] as usize,
        ];
        constraints
            .patch
            .push(constraints.patch[a] || constraints.patch[b]);
        constraints
            .pinned
            .push(constraints.pinned[a] || constraints.pinned[b]);
        constraints
            .support
            .push(constraints.support[a] || constraints.support[b]);
        constraints
            .water_unclamped
            .push(constraints.water_unclamped[a] || constraints.water_unclamped[b]);
        constraints.terrain_ceiling.push(interpolate_ceiling(
            constraints.terrain_ceiling[a],
            constraints.terrain_ceiling[b],
            0.5,
        ));
    }
}

fn interpolate_ceiling(a: f32, b: f32, interpolation: f32) -> f32 {
    match (a.is_finite(), b.is_finite()) {
        (true, true) => lerp_scalar(a, b, interpolation),
        (true, false) => a,
        (false, true) => b,
        (false, false) => f32::INFINITY,
    }
}

fn coast_edge(a: u32, b: u32) -> (u32, u32) {
    if a < b { (a, b) } else { (b, a) }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct CoastlinePath {
    vertices: Vec<u32>,
    closed: bool,
}

fn coastline_paths(edges: &[[u32; 2]]) -> Result<Vec<CoastlinePath>, String> {
    let mut adjacency = HashMap::<u32, Vec<u32>>::new();
    for &[a, b] in edges {
        if a == b {
            return Err(format!("degenerate coastline edge at vertex {a}"));
        }
        adjacency.entry(a).or_default().push(b);
        adjacency.entry(b).or_default().push(a);
    }
    for neighbours in adjacency.values_mut() {
        neighbours.sort_unstable();
    }
    for (&vertex, neighbours) in &adjacency {
        if !(1..=2).contains(&neighbours.len()) {
            return Err(format!(
                "coastline vertex {vertex} has degree {}, expected 1 or 2",
                neighbours.len()
            ));
        }
    }

    let mut remaining = edges
        .iter()
        .map(|&[a, b]| coast_edge(a, b))
        .collect::<HashSet<_>>();
    let mut paths = Vec::new();
    let mut endpoints = adjacency
        .iter()
        .filter_map(|(&vertex, neighbours)| (neighbours.len() == 1).then_some(vertex))
        .collect::<Vec<_>>();
    endpoints.sort_unstable();
    for start in endpoints {
        let neighbours = &adjacency[&start];
        if neighbours.len() != 1 || !remaining.contains(&coast_edge(start, neighbours[0])) {
            continue;
        }
        let mut vertices = vec![start];
        let (mut previous, mut current) = (start, neighbours[0]);
        remaining.remove(&coast_edge(previous, current));
        loop {
            vertices.push(current);
            let candidates = &adjacency[&current];
            let Some(next) = candidates.iter().copied().find(|&next| next != previous) else {
                break;
            };
            if !remaining.remove(&coast_edge(current, next)) {
                return Err(format!(
                    "coastline path revisited or missed edge {current}-{next}"
                ));
            }
            previous = current;
            current = next;
        }
        paths.push(CoastlinePath {
            vertices,
            closed: false,
        });
    }
    while let Some(&(start, first)) = remaining.iter().min() {
        let mut vertices = vec![start];
        let (mut previous, mut current) = (start, first);
        remaining.remove(&coast_edge(previous, current));
        while current != start {
            vertices.push(current);
            let neighbours = &adjacency[&current];
            let next = if neighbours[0] == previous {
                neighbours[1]
            } else {
                neighbours[0]
            };
            if !remaining.remove(&coast_edge(current, next)) {
                return Err(format!(
                    "coastline loop revisited or missed edge {current}-{next}"
                ));
            }
            previous = current;
            current = next;
        }
        if vertices.len() < 3 {
            return Err("coastline loop has fewer than three vertices".to_owned());
        }
        paths.push(CoastlinePath {
            vertices,
            closed: true,
        });
    }
    Ok(paths)
}

fn refine_coastline_paths(
    paths: &[CoastlinePath],
    stencils: &[NewVertexStencil],
) -> Result<Vec<CoastlinePath>, String> {
    let midpoints = stencils
        .iter()
        .map(|stencil| {
            (
                coast_edge(stencil.surrounding[0], stencil.surrounding[1]),
                stencil.vertex,
            )
        })
        .collect::<HashMap<_, _>>();
    paths
        .iter()
        .map(|path| {
            let edge_count = if path.closed {
                path.vertices.len()
            } else {
                path.vertices.len().saturating_sub(1)
            };
            let mut refined = Vec::with_capacity(path.vertices.len() + edge_count);
            for index in 0..edge_count {
                let a = path.vertices[index];
                let b = path.vertices[(index + 1) % path.vertices.len()];
                refined.push(a);
                let midpoint = midpoints
                    .get(&coast_edge(a, b))
                    .copied()
                    .ok_or_else(|| format!("coastline edge {a}-{b} was not tessellated"))?;
                refined.push(midpoint);
            }
            if !path.closed {
                refined.push(*path.vertices.last().expect("non-empty coastline path"));
            }
            Ok(CoastlinePath {
                vertices: refined,
                closed: path.closed,
            })
        })
        .collect()
}

fn smooth_coastline_paths_xy(terrain: &mut Mesh, paths: &[CoastlinePath], protected: &[bool]) {
    let source = terrain.vertices.clone();
    let perimeter = terrain.perimeter_mask();
    let mut candidates = vec![None; source.len()];
    for path in paths {
        for index in 0..path.vertices.len() {
            let vertex = path.vertices[index] as usize;
            if perimeter[vertex]
                || protected[vertex]
                || (!path.closed && (index == 0 || index + 1 == path.vertices.len()))
            {
                continue;
            }
            let previous = source
                [path.vertices[(index + path.vertices.len() - 1) % path.vertices.len()] as usize];
            let current = source[vertex];
            let next = source[path.vertices[(index + 1) % path.vertices.len()] as usize];
            candidates[vertex] =
                Some((previous.truncate() + current.truncate() + next.truncate()) / 3.0);
        }
    }

    loop {
        let mut rejected = Vec::new();
        for triangle in terrain.triangles.chunks_exact(3) {
            let vertices = [triangle[0], triangle[1], triangle[2]];
            let original = vertices.map(|vertex| source[vertex as usize].truncate());
            let moved = vertices.map(|vertex| {
                candidates[vertex as usize].unwrap_or(source[vertex as usize].truncate())
            });
            let original_area = (original[1] - original[0]).perp_dot(original[2] - original[0]);
            let moved_area = (moved[1] - moved[0]).perp_dot(moved[2] - moved[0]);
            if moved_area.abs() <= COAST_PROJECTED_AREA_EPSILON || original_area * moved_area <= 0.0
            {
                rejected.extend(
                    triangle
                        .iter()
                        .copied()
                        .filter(|&vertex| candidates[vertex as usize].is_some()),
                );
            }
        }
        if rejected.is_empty() {
            break;
        }
        rejected.sort_unstable();
        rejected.dedup();
        for vertex in rejected {
            candidates[vertex as usize] = None;
        }
    }

    for (position, candidate) in terrain.vertices.iter_mut().zip(candidates) {
        if let Some(xy) = candidate {
            position.x = xy.x;
            position.y = xy.y;
        }
    }
    terrain.calculate_normals();
}

pub(super) fn river_corridor_apron_mask(
    adjacency: &Adjacency,
    coverage: &[u8],
    apron_rings: u8,
) -> Vec<bool> {
    let mut patch = coverage
        .iter()
        .map(|&remaining| remaining != 0)
        .collect::<Vec<_>>();
    let mut frontier = patch
        .iter()
        .enumerate()
        .filter_map(|(vertex, &selected)| selected.then_some(vertex))
        .collect::<Vec<_>>();
    for _ in 0..apron_rings {
        let mut next = Vec::new();
        for vertex in frontier {
            for &neighbour in &adjacency[vertex] {
                if !patch[neighbour] {
                    patch[neighbour] = true;
                    next.push(neighbour);
                }
            }
        }
        frontier = next;
    }
    patch
}

#[derive(Clone, Copy)]
pub(super) struct RiverBankLiftMasks<'a> {
    pub(super) ocean: &'a [bool],
    pub(super) perimeter: &'a [bool],
    pub(super) protected: &'a [bool],
}

pub(super) fn lift_river_banks_to_surface(
    terrain: &mut Mesh,
    adjacency: &Adjacency,
    coverage: &[u8],
    surfaces: &[f32],
    target_half_widths: &[f32],
    masks: RiverBankLiftMasks<'_>,
) -> usize {
    let RiverBankLiftMasks {
        ocean,
        perimeter,
        protected,
    } = masks;
    debug_assert_eq!(terrain.vertices.len(), adjacency.len());
    debug_assert_eq!(terrain.vertices.len(), coverage.len());
    debug_assert_eq!(terrain.vertices.len(), surfaces.len());
    debug_assert_eq!(terrain.vertices.len(), target_half_widths.len());
    debug_assert_eq!(terrain.vertices.len(), perimeter.len());
    debug_assert_eq!(terrain.vertices.len(), protected.len());

    let (_, banks) = river_topology_masks(terrain, coverage);
    let mut targets = vec![f32::NEG_INFINITY; terrain.vertices.len()];
    let mut distances = vec![f32::INFINITY; terrain.vertices.len()];
    let mut sources = vec![usize::MAX; terrain.vertices.len()];
    let mut queue = BinaryHeap::new();

    for (vertex, &is_bank) in banks.iter().enumerate() {
        let surface = surfaces[vertex];
        if !is_bank
            || protected[vertex]
            || perimeter[vertex]
            || ocean.get(vertex).copied().unwrap_or(false)
            || !surface.is_finite()
            || surface <= terrain.vertices[vertex].z
        {
            continue;
        }
        targets[vertex] = surface;
        distances[vertex] = 0.0;
        sources[vertex] = vertex;
        queue.push(RouteState { cost: 0.0, vertex });
    }

    while let Some(RouteState { cost, vertex }) = queue.pop() {
        if cost > distances[vertex] || cost >= 1.0 {
            continue;
        }
        let source = sources[vertex];
        debug_assert_ne!(source, usize::MAX);
        if protected[vertex] {
            continue;
        }
        let surface = surfaces[source];
        let blend_width = (target_half_widths[source] * RIVER_BANK_BLEND_HALF_WIDTH_MULTIPLIER)
            .clamp(
                MINIMUM_RIVER_BANK_BLEND_WIDTH,
                MAXIMUM_RIVER_BANK_BLEND_WIDTH,
            );
        if coverage[vertex] == 0 {
            if perimeter[vertex] || ocean.get(vertex).copied().unwrap_or(false) {
                continue;
            }
            let influence = 1.0 - cost;
            let smooth_influence = influence * influence * (3.0 - 2.0 * influence);
            let original_height = terrain.vertices[vertex].z;
            let target = (surface - original_height).mul_add(smooth_influence, original_height);
            targets[vertex] = targets[vertex].max(target);
        }
        for &neighbour in &adjacency[vertex] {
            if coverage[neighbour] != 0
                || protected[neighbour]
                || perimeter[neighbour]
                || ocean.get(neighbour).copied().unwrap_or(false)
            {
                continue;
            }
            let edge = terrain.vertices[vertex]
                .truncate()
                .distance(terrain.vertices[neighbour].truncate());
            let next = cost + edge / blend_width;
            if next < distances[neighbour]
                || (next.to_bits() == distances[neighbour].to_bits() && source < sources[neighbour])
            {
                distances[neighbour] = next;
                sources[neighbour] = source;
                queue.push(RouteState {
                    cost: next,
                    vertex: neighbour,
                });
            }
        }
    }

    let mut raised = 0;
    for (vertex, target) in targets.into_iter().enumerate() {
        if target > terrain.vertices[vertex].z {
            terrain.vertices[vertex].z = target;
            raised += 1;
        }
    }
    raised
}

fn refine_river_terrain(
    terrain: &mut Mesh,
    material: &mut SurfaceMaterial,
    buffers: &mut RiverMeshBuffers,
) {
    let coverage = &buffers.coverage;
    let surfaces = &buffers.surfaces;
    let (under_river, bank) = river_topology_masks(terrain, coverage);
    if !under_river.iter().any(|&is_river| is_river) {
        return;
    }
    let loose_volume = material.volume(terrain);
    for (vertex, position) in terrain.vertices.iter_mut().enumerate() {
        if under_river[vertex] && !bank[vertex] {
            position.z = position.z.min(surfaces[vertex] - RIVER_SURFACE_OFFSET);
        }
    }
    material.rescale_to_volume(terrain, loose_volume);
}

fn repair_sharp_terrain_points(
    terrain: &mut Mesh,
    material: &mut SurfaceMaterial,
    buffers: &mut RiverMeshBuffers,
) {
    let RiverMeshBuffers {
        coverage,
        surfaces,
        river_uv,
        owners,
        waterfall_lips,
        target_half_widths,
        target_depths,
    } = buffers;
    let adjacency = terrain.adjacency();
    let perimeter = terrain.perimeter_mask();
    let sharp = sharp_point_mask(terrain, &adjacency, &perimeter);
    if !sharp.iter().any(|&is_sharp| is_sharp) {
        return;
    }

    let loose_volume = material.volume(terrain);
    let stencils = terrain.tessellate_incident_to(&sharp);
    material.extend_after_tessellation(loose_volume, terrain, &stencils);

    let mut patch = sharp;
    patch.reserve(stencils.len());
    coverage.reserve(stencils.len());
    surfaces.reserve(stencils.len());
    river_uv.reserve(stencils.len());
    owners.reserve(stencils.len());
    waterfall_lips.reserve(stencils.len());
    target_half_widths.reserve(stencils.len());
    target_depths.reserve(stencils.len());
    for stencil in stencils {
        let [a, b] = [
            stencil.surrounding[0] as usize,
            stencil.surrounding[1] as usize,
        ];
        let count = usize::from(stencil.count);
        let selected = coverage[a] != 0 && coverage[b] != 0;
        let owner = interpolated_river_owner(owners, surfaces, a, b, selected);
        patch.push(
            stencil.surrounding[..count]
                .iter()
                .any(|&vertex| patch[vertex as usize]),
        );
        coverage.push(if selected {
            coverage[a].min(coverage[b]) & !RIVER_BOUNDARY
        } else {
            0
        });
        surfaces.push(if selected {
            (surfaces[a] + surfaces[b]) * 0.5
        } else {
            0.0
        });
        river_uv.push(if selected {
            (river_uv[a] + river_uv[b]) * 0.5
        } else {
            Vec2::ZERO
        });
        owners.push(owner);
        waterfall_lips.push(selected && waterfall_lips[a] && waterfall_lips[b]);
        target_half_widths.push(target_half_widths[a].max(target_half_widths[b]));
        target_depths.push(if selected {
            (target_depths[a] + target_depths[b]) * 0.5
        } else {
            0.0
        });
    }

    let adjacency = terrain.adjacency();
    let perimeter = terrain.perimeter_mask();
    let mut height_scratch = vec![0.0; terrain.vertices.len()];
    for _ in 0..SHARP_POINT_SMOOTHING_PASSES {
        smooth_sharp_point_patch(
            terrain,
            &adjacency,
            &perimeter,
            &patch,
            coverage,
            surfaces,
            &mut height_scratch,
        );
    }
    material.rescale_to_volume(terrain, loose_volume);

    for value in coverage.iter_mut() {
        *value &= !RIVER_BOUNDARY;
    }
    mark_river_boundary(&adjacency, &perimeter, coverage);
}

pub(super) fn sharp_point_mask(
    terrain: &Mesh,
    adjacency: &Adjacency,
    perimeter: &[bool],
) -> Vec<bool> {
    terrain
        .vertices
        .iter()
        .enumerate()
        .map(|(vertex, &position)| {
            let neighbours = &adjacency[vertex];
            if perimeter[vertex] || neighbours.len() < 3 {
                return false;
            }
            let (height_total, edge_total, minimum, maximum) = neighbours.iter().copied().fold(
                (0.0_f32, 0.0_f32, f32::INFINITY, f32::NEG_INFINITY),
                |(height_total, edge_total, minimum, maximum), neighbour| {
                    let candidate = terrain.vertices[neighbour];
                    (
                        height_total + candidate.z,
                        edge_total + position.truncate().distance(candidate.truncate()),
                        minimum.min(candidate.z),
                        maximum.max(candidate.z),
                    )
                },
            );
            let inverse_count = 1.0 / neighbours.len() as f32;
            let mean_height = height_total * inverse_count;
            let mean_edge = edge_total * inverse_count;
            let is_extremum = position.z < minimum || position.z > maximum;
            is_extremum && (position.z - mean_height).abs() > mean_edge * SHARP_POINT_HEIGHT_RATIO
        })
        .collect()
}

pub(super) fn smooth_sharp_point_patch(
    terrain: &mut Mesh,
    adjacency: &Adjacency,
    perimeter: &[bool],
    patch: &[bool],
    coverage: &[u8],
    surfaces: &[f32],
    height_scratch: &mut [f32],
) {
    // Heights are read from a snapshot so every vertex in a pass observes the
    // same surface. XY is deliberately preserved: this rounds the spike while
    // making projected face inversion impossible in the repair stage.
    height_scratch
        .iter_mut()
        .zip(&terrain.vertices)
        .for_each(|(height, vertex)| *height = vertex.z);
    for (vertex, position) in terrain.vertices.iter_mut().enumerate() {
        if !patch[vertex]
            || perimeter[vertex]
            || is_river_boundary(coverage[vertex])
            || adjacency[vertex].is_empty()
        {
            continue;
        }
        let neighbour_height = adjacency[vertex]
            .iter()
            .map(|&neighbour| height_scratch[neighbour])
            .sum::<f32>()
            / adjacency[vertex].len() as f32;
        let mut smoothed = height_scratch[vertex]
            + (neighbour_height - height_scratch[vertex]) * SHARP_POINT_SMOOTHING;
        if coverage[vertex] != 0 {
            smoothed = smoothed.min(surfaces[vertex] - RIVER_SURFACE_OFFSET);
        }
        position.z = smoothed;
    }
}

pub(super) fn river_reaches_ocean(river: &River, ocean: &[bool]) -> bool {
    river.join.is_none() && river.nodes.last().is_some_and(|node| ocean[node.vertex])
}

pub(super) fn transfer_tributary_budgets(rivers: &[River], budgets: &mut [RiverSedimentBudget]) {
    for tributary in (0..rivers.len()).rev() {
        let Some(join) = rivers[tributary].join else {
            continue;
        };
        let upstream = std::mem::take(&mut budgets[tributary]);
        budgets[join].absorb(upstream);
    }
}

pub(super) fn finalize_river_budgets(rivers: &[River], budgets: &mut [RiverSedimentBudget]) {
    for (river, budget) in rivers.iter().zip(budgets) {
        if river.join.is_none() {
            budget.export_remaining();
            debug_assert!(budget.is_balanced());
        } else {
            debug_assert_eq!(budget.carried.to_bits(), 0.0_f64.to_bits());
        }
    }
}

pub(super) fn apply_known_surfaces(rivers: &mut [River], known_surfaces: &HashMap<usize, f32>) {
    for river in rivers {
        for node in &mut river.nodes {
            if let Some(surface) = known_surfaces.get(&node.vertex) {
                node.surface = node.surface.min(*surface);
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct RiverOwnerKey {
    pub(super) river: u32,
    pub(super) node: u32,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct RiverChannelFootprintOwner {
    pub(super) key: RiverOwnerKey,
    pub(super) surface: f32,
    pub(super) floor_override: Option<f32>,
    pub(super) flow_origin: Vec2,
    pub(super) flow_direction: Vec2,
    pub(super) distance_along: f32,
    pub(super) target_half_width: f32,
    pub(super) target_depth: f32,
    pub(super) ring_count: u8,
    pub(super) waterfall_lip: bool,
}

pub(super) struct RiverMeshAttributes {
    pub(super) surfaces: Vec<f32>,
    pub(super) uv: Vec<Vec2>,
    pub(super) owners: Vec<Option<RiverOwnerKey>>,
    pub(super) waterfall_lips: Vec<bool>,
    pub(super) target_half_widths: Vec<f32>,
    pub(super) target_depths: Vec<f32>,
}

pub(super) struct RiverMeshBuffers {
    pub(super) coverage: Vec<u8>,
    pub(super) surfaces: Vec<f32>,
    pub(super) river_uv: Vec<Vec2>,
    pub(super) owners: Vec<Option<RiverOwnerKey>>,
    pub(super) waterfall_lips: Vec<bool>,
    pub(super) target_half_widths: Vec<f32>,
    pub(super) target_depths: Vec<f32>,
}

impl RiverMeshBuffers {
    pub(super) fn new(coverage: Vec<u8>, attributes: RiverMeshAttributes) -> Self {
        Self {
            coverage,
            surfaces: attributes.surfaces,
            river_uv: attributes.uv,
            owners: attributes.owners,
            waterfall_lips: attributes.waterfall_lips,
            target_half_widths: attributes.target_half_widths,
            target_depths: attributes.target_depths,
        }
    }

    pub(super) fn refine_channel(&mut self, terrain: &mut Mesh, material: &mut SurfaceMaterial) {
        self.refine_corridor(terrain, material);
        refine_river_terrain(terrain, material, self);
        self.repair_sharp_points(terrain, material);
    }

    pub(super) fn refine_corridor(
        &mut self,
        terrain: &mut Mesh,
        material: &mut SurfaceMaterial,
    ) -> usize {
        refine_river_corridor_mesh(terrain, material, self)
    }

    pub(super) fn repair_sharp_points(
        &mut self,
        terrain: &mut Mesh,
        material: &mut SurfaceMaterial,
    ) {
        repair_sharp_terrain_points(terrain, material, self);
    }

    pub(super) fn extend_after_tessellation(&mut self, stencils: &[crate::mesh::NewVertexStencil]) {
        extend_river_attributes_after_tessellation(stencils, self);
    }

    fn extend_after_edge_splits(&mut self, stencils: &[EdgeSplitStencil]) {
        self.coverage.reserve(stencils.len());
        self.surfaces.reserve(stencils.len());
        self.river_uv.reserve(stencils.len());
        self.owners.reserve(stencils.len());
        self.waterfall_lips.reserve(stencils.len());
        self.target_half_widths.reserve(stencils.len());
        self.target_depths.reserve(stencils.len());
        for stencil in stencils {
            debug_assert_eq!(stencil.vertex as usize, self.coverage.len());
            let [a, b] = stencil.edge.map(|vertex| vertex as usize);
            let selected = self.coverage[a] != 0 && self.coverage[b] != 0;
            let owner = interpolated_river_owner(&self.owners, &self.surfaces, a, b, selected);
            let t = stencil.interpolation;
            self.coverage.push(if selected {
                self.coverage[a].min(self.coverage[b]) & !RIVER_BOUNDARY
            } else {
                0
            });
            self.surfaces.push(if selected {
                lerp_scalar(self.surfaces[a], self.surfaces[b], t)
            } else {
                0.0
            });
            self.river_uv.push(if selected {
                self.river_uv[a].lerp(self.river_uv[b], t)
            } else {
                Vec2::ZERO
            });
            self.owners.push(owner);
            self.waterfall_lips
                .push(selected && self.waterfall_lips[a] && self.waterfall_lips[b]);
            self.target_half_widths
                .push(self.target_half_widths[a].max(self.target_half_widths[b]));
            self.target_depths.push(if selected {
                lerp_scalar(self.target_depths[a], self.target_depths[b], t)
            } else {
                0.0
            });
        }
    }
}

pub(super) fn river_mesh_attributes(
    terrain: &Mesh,
    owners: &[Option<RiverChannelFootprintOwner>],
) -> RiverMeshAttributes {
    let mut attributes = RiverMeshAttributes {
        surfaces: Vec::with_capacity(owners.len()),
        uv: Vec::with_capacity(owners.len()),
        owners: Vec::with_capacity(owners.len()),
        waterfall_lips: Vec::with_capacity(owners.len()),
        target_half_widths: Vec::with_capacity(owners.len()),
        target_depths: Vec::with_capacity(owners.len()),
    };
    for (vertex, owner) in owners.iter().copied().enumerate() {
        let Some(owner) = owner else {
            attributes.surfaces.push(0.0);
            attributes.uv.push(Vec2::ZERO);
            attributes.owners.push(None);
            attributes.waterfall_lips.push(false);
            attributes.target_half_widths.push(0.0);
            attributes.target_depths.push(0.0);
            continue;
        };
        let offset = terrain.vertices[vertex].truncate() - owner.flow_origin;
        let across = Vec2::new(-owner.flow_direction.y, owner.flow_direction.x);
        attributes.surfaces.push(owner.surface);
        attributes.owners.push(Some(owner.key));
        attributes.uv.push(Vec2::new(
            offset.dot(across),
            owner.distance_along + offset.dot(owner.flow_direction),
        ));
        attributes.waterfall_lips.push(owner.waterfall_lip);
        attributes.target_half_widths.push(owner.target_half_width);
        attributes.target_depths.push(owner.target_depth);
    }
    attributes
}

#[derive(Debug)]
pub(super) struct RiverFootprint {
    pub(super) coverage: Vec<u8>,
    pub(super) ring_distance: Vec<u8>,
    pub(super) owner: Vec<Option<RiverChannelFootprintOwner>>,
}

pub(super) struct BuiltRiverGeometry {
    pub(super) river_mesh: Mesh,
    pub(super) river_bed: Vec<bool>,
    pub(super) river_rock_mesh: Mesh,
    pub(super) failed_waterfalls: Vec<usize>,
}

fn constrain_final_sea_plane_topology(
    terrain: &mut Mesh,
    material: &mut SurfaceMaterial,
    buffers: &mut RiverMeshBuffers,
    constraints: &mut WaterfallTerrainConstraints,
) {
    let loose_volume = material.volume(terrain);
    let constrained = terrain.constrain_height_plane(0.0);
    if constrained.edges.is_empty() {
        return;
    }

    material.extend_after_edge_splits(&constrained.splits);
    buffers.extend_after_edge_splits(&constrained.splits);
    extend_waterfall_constraints_after_edge_splits(constraints, &constrained.splits);
    let paths = coastline_paths(&constrained.edges)
        .expect("generated sea-plane contour must consist of manifold paths");
    let perimeter = terrain.perimeter_mask();
    for path in &paths {
        if !path.closed {
            let first = path.vertices[0] as usize;
            let last = *path.vertices.last().expect("non-empty coastline path") as usize;
            assert!(
                perimeter[first] && perimeter[last],
                "an open sea-plane contour may only terminate on the mesh perimeter"
            );
        }
    }

    let mut coast_vertices = vec![false; terrain.vertices.len()];
    for path in &paths {
        for &vertex in &path.vertices {
            coast_vertices[vertex as usize] = true;
        }
    }
    let stencils = terrain.tessellate_incident_to(&coast_vertices);
    material.extend_after_tessellation(loose_volume, terrain, &stencils);
    buffers.extend_after_tessellation(&stencils);
    extend_waterfall_constraints_after_tessellation(constraints, &stencils);
    let paths = refine_coastline_paths(&paths, &stencils)
        .expect("every constrained coastline edge must be tessellated");
    let protected = constraints
        .patch
        .iter()
        .zip(&constraints.pinned)
        .zip(&constraints.support)
        .map(|((&patch, &pinned), &support)| patch || pinned || support)
        .collect::<Vec<_>>();
    smooth_coastline_paths_xy(terrain, &paths, &protected);

    let adjacency = terrain.adjacency();
    let perimeter = terrain.perimeter_mask();
    buffers
        .coverage
        .iter_mut()
        .for_each(|coverage| *coverage &= !RIVER_BOUNDARY);
    mark_river_boundary(&adjacency, &perimeter, &mut buffers.coverage);
}

pub(super) struct RiverGeometryBuilder<'a> {
    network: &'a RiverNetwork,
    terrain: &'a mut Mesh,
    material: &'a mut SurfaceMaterial,
    seed: u64,
    buffers: RiverMeshBuffers,
}

impl<'a> RiverGeometryBuilder<'a> {
    pub(super) fn new(
        network: &'a RiverNetwork,
        terrain: &'a mut Mesh,
        material: &'a mut SurfaceMaterial,
        seed: u64,
    ) -> Self {
        let footprint = build_river_footprint(network, terrain, &terrain.adjacency(), true);
        let attributes = river_mesh_attributes(terrain, &footprint.owner);
        Self {
            network,
            terrain,
            material,
            seed,
            buffers: RiverMeshBuffers::new(footprint.coverage, attributes),
        }
    }

    pub(super) fn build(mut self) -> BuiltRiverGeometry {
        self.refine_channel();
        let patches = derive_waterfall_patches(self.network, self.terrain);
        let mut constraints = self.refine_waterfalls(&patches);
        self.finish_channel(&constraints);
        self.finish_waterfalls(&patches, &mut constraints);
        self.constrain_coastline(&patches, &mut constraints);
        self.assemble(&patches, &constraints)
    }

    fn refine_channel(&mut self) {
        self.buffers.refine_channel(self.terrain, self.material);
    }

    fn refine_waterfalls(&mut self, patches: &[WaterfallPatch]) -> WaterfallTerrainConstraints {
        self.buffers
            .refine_waterfalls(self.terrain, self.material, patches);
        let notches =
            recess_waterfall_notches(self.terrain, self.material, patches, &self.buffers.coverage);
        pin_waterfalls_to_terrain(
            self.terrain,
            self.material,
            patches,
            &notches,
            &mut self.buffers.coverage,
            &mut self.buffers.surfaces,
            &mut self.buffers.waterfall_lips,
        )
    }

    fn finish_channel(&mut self, constraints: &WaterfallTerrainConstraints) {
        lift_river_banks_to_surface(
            self.terrain,
            &self.terrain.adjacency(),
            &self.buffers.coverage,
            &self.buffers.surfaces,
            &self.buffers.target_half_widths,
            RiverBankLiftMasks {
                ocean: &self.network.ocean,
                perimeter: &self.terrain.perimeter_mask(),
                protected: &constraints.patch,
            },
        );
        enforce_sea_plane_clearance(self.terrain, &self.network.ocean);
        ensure_clear_river_channel(
            self.network,
            self.terrain,
            &self.buffers.coverage,
            &self.buffers.surfaces,
            &self.buffers.waterfall_lips,
            &constraints.pinned,
            &constraints.patch,
        );
        relax_refined_river_surface(
            self.terrain,
            &self.buffers.coverage,
            &mut self.buffers.surfaces,
            &self.buffers.river_uv,
            &self.buffers.target_half_widths,
            &self.buffers.target_depths,
            constraints,
        );
        enforce_sea_plane_clearance(self.terrain, &self.network.ocean);
    }

    fn finish_waterfalls(
        &mut self,
        patches: &[WaterfallPatch],
        constraints: &mut WaterfallTerrainConstraints,
    ) {
        for _ in 0..WATERFALL_FINAL_SMOOTHING_PASSES {
            smooth_final_waterfall_patches(
                self.terrain,
                &mut self.buffers.surfaces,
                patches,
                &self.buffers.coverage,
            );
        }
        enforce_final_waterfall_edge_relationships(
            self.terrain,
            &mut self.buffers.surfaces,
            patches,
            &self.buffers.coverage,
            &self.buffers.owners,
            constraints,
        );
        smooth_pinned_waterfall_terrain(
            self.terrain,
            &mut self.buffers.surfaces,
            patches,
            &self.buffers.coverage,
            &self.buffers.owners,
        );
        rebuild_final_waterfall_support_mask(
            self.terrain,
            patches,
            &self.buffers.coverage,
            &self.buffers.owners,
            constraints,
        );
    }

    fn constrain_coastline(
        &mut self,
        patches: &[WaterfallPatch],
        constraints: &mut WaterfallTerrainConstraints,
    ) {
        enforce_sea_plane_clearance(self.terrain, &self.network.ocean);
        constrain_final_sea_plane_topology(
            self.terrain,
            self.material,
            &mut self.buffers,
            constraints,
        );
        rebuild_final_waterfall_support_mask(
            self.terrain,
            patches,
            &self.buffers.coverage,
            &self.buffers.owners,
            constraints,
        );
    }

    fn assemble(
        self,
        patches: &[WaterfallPatch],
        constraints: &WaterfallTerrainConstraints,
    ) -> BuiltRiverGeometry {
        let failed_waterfalls = if ENABLE_FINAL_WATERFALL_REJECTION {
            detect_failed_final_waterfalls(self.terrain, patches, &self.buffers.coverage)
        } else {
            Vec::new()
        };
        let (river_mesh, river_bed) = finalize_river_geometry(
            self.terrain,
            &self.buffers.coverage,
            &self.buffers.surfaces,
            &self.buffers.river_uv,
            constraints,
        );
        let river_rock_mesh = if failed_waterfalls.is_empty() {
            generate_river_rock_mesh(self.seed, self.terrain, &self.buffers.coverage)
        } else {
            Mesh::default()
        };
        BuiltRiverGeometry {
            river_mesh,
            river_bed,
            river_rock_mesh,
            failed_waterfalls,
        }
    }
}

#[cfg(test)]
mod coastline_tests {
    use super::{CoastlinePath, Mesh, Vec2, coastline_paths, smooth_coastline_paths_xy};
    use crate::Vec3;

    #[test]
    fn coastline_edges_form_closed_loops_independent_of_order() {
        let paths = coastline_paths(&[[2, 3], [0, 1], [3, 0], [1, 2]]).unwrap();

        assert_eq!(paths.len(), 1);
        assert!(paths[0].closed);
        assert_eq!(paths[0].vertices.len(), 4);
        assert!(paths[0].vertices.iter().all(|vertex| *vertex < 4));
    }

    #[test]
    fn coastline_edges_may_end_at_the_mesh_perimeter() {
        let paths = coastline_paths(&[[2, 3], [0, 1], [1, 2]]).unwrap();

        assert_eq!(paths.len(), 1);
        assert!(!paths[0].closed);
        assert_eq!(paths[0].vertices.len(), 4);
        assert!(paths[0].vertices.iter().all(|vertex| *vertex < 4));
    }

    #[test]
    fn coastline_xy_smoothing_is_one_simultaneous_three_point_average() {
        let mut mesh = Mesh {
            vertices: vec![
                Vec3::new(-2.0, -2.0, -1.0),
                Vec3::new(2.0, -2.0, -1.0),
                Vec3::new(2.0, 2.0, -1.0),
                Vec3::new(-2.0, 2.0, -1.0),
                Vec3::new(-0.8, -0.6, 0.0),
                Vec3::new(0.7, -0.8, 0.0),
                Vec3::new(0.9, 0.7, 0.0),
                Vec3::new(-0.6, 0.9, 0.0),
                Vec3::new(0.0, 0.0, 1.0),
            ],
            normals: Vec::new(),
            triangles: vec![
                0, 1, 5, 0, 5, 4, 1, 2, 6, 1, 6, 5, 2, 3, 7, 2, 7, 6, 3, 0, 4, 3, 4, 7, 4, 5, 8, 5,
                6, 8, 6, 7, 8, 7, 4, 8,
            ],
            uv: Vec::new(),
        };
        let source = mesh.vertices.clone();
        let coast_loop = vec![4_u32, 5, 6, 7];
        let expected = (0..coast_loop.len())
            .map(|index| {
                let previous = source[coast_loop[(index + coast_loop.len() - 1) % 4] as usize];
                let current = source[coast_loop[index] as usize];
                let next = source[coast_loop[(index + 1) % 4] as usize];
                (previous.truncate() + current.truncate() + next.truncate()) / 3.0
            })
            .collect::<Vec<Vec2>>();

        smooth_coastline_paths_xy(
            &mut mesh,
            &[CoastlinePath {
                vertices: coast_loop.clone(),
                closed: true,
            }],
            &[false; 9],
        );

        assert_eq!(&mesh.vertices[..4], &source[..4]);
        for (&vertex, expected) in coast_loop.iter().zip(expected) {
            assert!(mesh.vertices[vertex as usize].truncate().distance(expected) < 1.0e-6);
            assert!(mesh.vertices[vertex as usize].z.abs() < f32::EPSILON);
        }
    }
}
