use super::{
    Adjacency, BinaryHeap, CHANNEL_FOOTPRINT_RINGS, HashMap, MAX_RIVER_RINGS,
    MAXIMUM_RING_MOVE_FRACTION, MAXIMUM_WIDTH_DEPTH_COMPENSATION, Mesh, ProjectedFaceAreas,
    RIVER_BOUNDARY, RIVER_CHANNEL_DRAINAGE_FLOOR, RIVER_CORRIDOR_SMOOTHING, RIVER_SURFACE_OFFSET,
    RIVER_VALLEY_APRON_RINGS, RIVER_VALLEY_BANK_DEPTH_FRACTION, RIVER_VALLEY_DRAINAGE_FLOOR, River,
    RiverChannelFootprintOwner, RiverChannelParameters, RiverChannelSettings, RiverCrossSection,
    RiverFootprint, RiverNetwork, RiverNode, RiverOwnerKey, RiverSedimentBudget, RiverTerrain,
    RouteState, Vec2, Vec3, VertexFaceAdjacency, WATERFALL_WATER_CLEARANCE,
    WaterfallTerrainConstraints, average_edge_length, confluence_connector, unfitted_river_depth,
};

#[cfg(feature = "profiling")]
use super::ISLAND_WORLD_METRES;
#[cfg(test)]
use super::WATERFALL_LIP_SMOOTHING;

#[derive(Clone, Copy, Debug)]
pub(super) struct RiverMeshCandidate {
    pub(super) remaining: u8,
    pub(super) distance: u8,
    pub(super) vertex: usize,
    pub(super) owner: RiverChannelFootprintOwner,
}

pub(super) fn build_river_footprint(
    network: &RiverNetwork,
    terrain: &Mesh,
    adjacency: &Adjacency,
    visible_only: bool,
) -> RiverFootprint {
    let vertex_count = terrain.vertices.len();
    let perimeter = terrain.perimeter_mask();
    let mut coverage = vec![0_u8; vertex_count];
    let mut owner_distance = vec![u8::MAX; vertex_count];
    let mut owner = vec![None; vertex_count];
    let mut frontiers: [Vec<RiverMeshCandidate>; CHANNEL_FOOTPRINT_RINGS as usize + 1] =
        std::array::from_fn(|_| Vec::new());
    let base_width = average_edge_length(terrain, adjacency).max(0.000_25);

    for (river_index, ((river, waterfalls), mesh_end)) in network
        .rivers
        .iter()
        .zip(&network.waterfalls)
        .zip(&network.river_mesh_ends)
        .enumerate()
    {
        let mut distance_along = 0.0;
        let mut previous_position = None;
        let visible_node_count = if visible_only {
            mesh_end
                .map_or(river.nodes.len(), |end| end.saturating_add(1))
                .min(river.nodes.len())
        } else {
            river.nodes.len()
        };
        for (node_index, node) in river.nodes.iter().take(visible_node_count).enumerate() {
            let water_position = river_node_water_position(terrain, node);
            if let Some(previous) = previous_position {
                distance_along += water_position.distance(previous);
            }
            previous_position = Some(water_position);
            let (target_half_width, target_depth) =
                river_footprint_dimensions(network, river_index, node_index, base_width);
            let remaining = CHANNEL_FOOTPRINT_RINGS;
            let footprint_owner = RiverChannelFootprintOwner {
                key: RiverOwnerKey {
                    river: river_index as u32,
                    node: node_index as u32,
                },
                surface: node.surface,
                floor_override: None,
                flow_origin: water_position.truncate(),
                flow_direction: river_node_flow_direction(terrain, &river.nodes, node_index),
                distance_along,
                target_half_width,
                target_depth,
                ring_count: remaining,
                waterfall_lip: waterfalls.get(node_index).copied().unwrap_or(false),
            };
            frontiers[remaining as usize].push(RiverMeshCandidate {
                remaining,
                distance: 0,
                vertex: node.vertex,
                owner: footprint_owner,
            });
        }
    }

    seed_confluence_footprints(
        network,
        terrain,
        adjacency,
        visible_only,
        base_width,
        &mut frontiers[CHANNEL_FOOTPRINT_RINGS as usize],
    );

    for remaining in (0..=CHANNEL_FOOTPRINT_RINGS).rev() {
        while let Some(candidate) = frontiers[remaining as usize].pop() {
            let candidate_coverage = candidate.remaining + 1;
            if !river_candidate_wins(
                candidate,
                candidate_coverage,
                coverage[candidate.vertex],
                owner_distance[candidate.vertex],
                owner[candidate.vertex],
            ) {
                continue;
            }
            coverage[candidate.vertex] = candidate_coverage;
            owner_distance[candidate.vertex] = candidate.distance;
            owner[candidate.vertex] = Some(candidate.owner);
            if candidate.remaining == 0 {
                continue;
            }
            for &neighbour in &adjacency[candidate.vertex] {
                frontiers[candidate.remaining as usize - 1].push(RiverMeshCandidate {
                    remaining: candidate.remaining - 1,
                    distance: candidate.distance.saturating_add(1),
                    vertex: neighbour,
                    owner: candidate.owner,
                });
            }
        }
    }

    mark_river_boundary(adjacency, &perimeter, &mut coverage);
    RiverFootprint {
        coverage,
        ring_distance: owner_distance,
        owner,
    }
}

pub(super) fn river_footprint_dimensions(
    network: &RiverNetwork,
    river: usize,
    node: usize,
    base_width: f32,
) -> (f32, f32) {
    let river_node = network.rivers[river].nodes[node];
    let section = network
        .cross_sections
        .get(river)
        .and_then(|sections| sections.get(node))
        .copied()
        .unwrap_or_default();
    let half_width = if section.target_half_width > 0.0 {
        section.target_half_width
    } else {
        river_half_width(river_node.flow, network.max_flow, base_width)
    };
    (half_width, section.required_depth.max(0.0))
}

/// Extends a tributary's final one-ring footprint across the short topology
/// gap used to detect its join. Without these seeds, the late connector carve
/// cuts only a centreline through dry terrain and the separately shaped river
/// ends form tall, folded faces around it.
pub(super) fn seed_confluence_footprints(
    network: &RiverNetwork,
    terrain: &Mesh,
    adjacency: &Adjacency,
    visible_only: bool,
    base_width: f32,
    frontier: &mut Vec<RiverMeshCandidate>,
) {
    for (river_index, river) in network.rivers.iter().enumerate() {
        let Some(joined_river) = river.join else {
            continue;
        };
        let Some(terminal_index) = river.nodes.len().checked_sub(1) else {
            continue;
        };
        let terminal = river.nodes[terminal_index];
        let Some(join_vertex) = network.join_vertices.get(river_index).copied().flatten() else {
            continue;
        };
        let Some(join_index) = network.rivers[joined_river]
            .nodes
            .iter()
            .position(|node| node.vertex == join_vertex)
        else {
            continue;
        };
        if visible_only
            && (network.river_mesh_ends[river_index].is_some_and(|end| terminal_index > end)
                || network.river_mesh_ends[joined_river].is_some_and(|end| join_index > end))
        {
            continue;
        }
        let path = confluence_connector(network, adjacency, terminal.vertex, join_vertex);
        if path.len() < 3 {
            continue;
        }

        let join_node = network.rivers[joined_river].nodes[join_index];
        let (terminal_width, terminal_depth) =
            river_footprint_dimensions(network, river_index, terminal_index, base_width);
        let (join_width, join_depth) =
            river_footprint_dimensions(network, joined_river, join_index, base_width);
        let terminal_floor = terminal.surface - terminal_depth;
        let join_floor = join_node.surface - join_depth;
        let fallback_direction = (terrain.vertices[join_vertex].truncate()
            - terrain.vertices[terminal.vertex].truncate())
        .normalize_or_zero();
        let mut distance_along = river.nodes.windows(2).fold(0.0, |distance, nodes| {
            distance
                + river_node_water_position(terrain, &nodes[0])
                    .distance(river_node_water_position(terrain, &nodes[1]))
        });
        let final_step = path.len() - 1;
        for step in 1..final_step {
            let vertex = path[step];
            distance_along += terrain.vertices[path[step - 1]]
                .truncate()
                .distance(terrain.vertices[vertex].truncate());
            let progress = step as f32 / final_step as f32;
            let previous = terrain.vertices[path[step - 1]].truncate();
            let next = terrain.vertices[path[step + 1]].truncate();
            let direction = (next - previous)
                .try_normalize()
                .unwrap_or(fallback_direction);
            let surface =
                (join_node.surface - terminal.surface).mul_add(progress, terminal.surface);
            let floor = (join_floor - terminal_floor).mul_add(progress, terminal_floor);
            let target_half_width = (join_width - terminal_width).mul_add(progress, terminal_width);
            frontier.push(RiverMeshCandidate {
                remaining: CHANNEL_FOOTPRINT_RINGS,
                distance: 0,
                vertex,
                owner: RiverChannelFootprintOwner {
                    key: RiverOwnerKey {
                        river: river_index as u32,
                        node: terminal_index as u32,
                    },
                    surface,
                    floor_override: Some(floor),
                    flow_origin: terrain.vertices[vertex].truncate(),
                    flow_direction: direction,
                    distance_along,
                    target_half_width,
                    target_depth: (surface - floor).max(0.0),
                    ring_count: CHANNEL_FOOTPRINT_RINGS,
                    waterfall_lip: false,
                },
            });
        }
    }
}

pub(super) fn river_candidate_wins(
    candidate: RiverMeshCandidate,
    candidate_coverage: u8,
    current_coverage: u8,
    current_distance: u8,
    current_owner: Option<RiverChannelFootprintOwner>,
) -> bool {
    if candidate_coverage != current_coverage {
        return candidate_coverage > current_coverage;
    }
    if candidate.distance != current_distance {
        return candidate.distance < current_distance;
    }
    let Some(current) = current_owner else {
        return true;
    };
    candidate.owner.surface.total_cmp(&current.surface).is_lt()
        || (candidate.owner.surface.to_bits() == current.surface.to_bits()
            && (candidate
                .owner
                .target_half_width
                .total_cmp(&current.target_half_width)
                .is_gt()
                || (candidate.owner.target_half_width.to_bits()
                    == current.target_half_width.to_bits()
                    && candidate.owner.key < current.key)))
}

pub(super) fn target_cross_sections(
    rivers: &[River],
    settings: RiverChannelSettings,
) -> Vec<Vec<RiverCrossSection>> {
    rivers
        .iter()
        .map(|river| {
            let mut downstream_growth = 0.0_f32;
            let source_flow = river
                .nodes
                .first()
                .map_or(0.0, |node| (node.flow as f32).sqrt());
            let terminal_flow = river
                .nodes
                .last()
                .map_or(source_flow, |node| (node.flow as f32).sqrt());
            let flow_span = terminal_flow - source_flow;
            let path_span = river.nodes.len().saturating_sub(1).max(1) as f32;
            river
                .nodes
                .iter()
                .enumerate()
                .map(|(index, node)| {
                    let path_growth = index as f32 / path_span;
                    let flow_growth = if flow_span > f32::EPSILON {
                        ((node.flow as f32).sqrt() - source_flow) / flow_span
                    } else {
                        path_growth
                    };
                    let local_growth = (flow_growth.clamp(0.0, 1.0) + path_growth) * 0.5;
                    downstream_growth = downstream_growth.max(local_growth);
                    let target_half_width = 0.5
                        * (settings.source_width
                            + (settings.maximum_width - settings.source_width) * downstream_growth);
                    let nominal_depth = settings.source_depth
                        + (settings.maximum_depth - settings.source_depth) * downstream_growth;
                    RiverCrossSection {
                        target_half_width,
                        nominal_depth,
                        achieved_width: 0.0,
                        required_depth: nominal_depth,
                    }
                })
                .collect()
        })
        .collect()
}

pub(super) fn shape_channel_ring_vertices(
    network: &RiverNetwork,
    mesh: &mut Mesh,
    footprint: &RiverFootprint,
) {
    let centreline = river_centreline_mask(network, mesh.vertices.len());
    let vertex_faces = VertexFaceAdjacency::new(mesh);
    let mut projected_areas = ProjectedFaceAreas::new(mesh);

    let mut vertices = centreline
        .iter()
        .enumerate()
        .filter_map(|(vertex, &is_centreline)| {
            (!is_centreline
                && !network.perimeter.get(vertex).copied().unwrap_or(false)
                && footprint.coverage[vertex] != 0)
                .then_some(vertex)
        })
        .collect::<Vec<_>>();
    vertices.sort_unstable_by_key(|&vertex| footprint.ring_distance[vertex]);

    for vertex in vertices {
        let Some(owner) = footprint.owner[vertex] else {
            continue;
        };
        let direction = owner.flow_direction;
        if direction == Vec2::ZERO {
            continue;
        }
        let across = Vec2::new(-direction.y, direction.x);
        let offset = mesh.vertices[vertex].truncate() - owner.flow_origin;
        let lateral = offset.dot(across);
        if lateral.abs() <= f32::EPSILON {
            continue;
        }
        let along = offset.dot(direction);
        let ring_fraction =
            f32::from(footprint.ring_distance[vertex]) / f32::from(owner.ring_count.max(1));
        let target_lateral = owner.target_half_width * ring_fraction;
        let target_xy = direction.mul_add(Vec2::splat(along), owner.flow_origin)
            + across * lateral.signum() * target_lateral;
        let current = mesh.vertices[vertex];
        let target = target_xy.extend(current.z);
        let safe = projected_areas
            .safe_move_fraction(mesh, &vertex_faces, vertex, target)
            .min(MAXIMUM_RING_MOVE_FRACTION);
        if safe <= f32::EPSILON {
            continue;
        }
        mesh.vertices[vertex] = current.truncate().lerp(target_xy, safe).extend(current.z);
        projected_areas.update_incident(mesh, &vertex_faces, vertex);
        if !mesh.uv.is_empty() {
            mesh.uv[vertex] = mesh.vertices[vertex].truncate();
        }
    }
}

pub(super) fn update_achieved_cross_sections(
    network: &mut RiverNetwork,
    mesh: &Mesh,
    footprint: &RiverFootprint,
    maximum_nominal_depth: f32,
) {
    let samples = cross_section_samples(network, mesh, footprint);
    let mut unresolved = 0_usize;
    let mut maximum_applied_depth = 0.0_f32;
    let mut total_applied_depth = 0.0_f64;
    let mut maximum_achieved_width = 0.0_f32;
    let mut total_achieved_width = 0.0_f64;
    let mut section_count = 0_usize;
    for (river_index, sections) in network.cross_sections.iter_mut().enumerate() {
        let mut widths: Vec<Option<f32>> = samples[river_index]
            .iter()
            .map(|sides| match (sides[0], sides[1]) {
                (Some(left), Some(right)) => Some(left + right),
                _ => None,
            })
            .collect();
        unresolved += widths.iter().filter(|width| width.is_none()).count();
        fill_missing_widths(&mut widths);
        let original = widths.clone();
        for index in 0..widths.len() {
            let start = index.saturating_sub(1);
            let end = (index + 1).min(widths.len().saturating_sub(1));
            let (total, count) = original[start..=end]
                .iter()
                .flatten()
                .fold((0.0_f32, 0_u32), |(total, count), &width| {
                    (total + width, count + 1)
                });
            if count > 0 {
                widths[index] = Some(total / count as f32);
            }
        }
        for (section, width) in sections.iter_mut().zip(widths) {
            let achieved_width = width.unwrap_or(section.target_half_width * 2.0).max(1.0e-6);
            section.achieved_width = achieved_width;
            section.required_depth =
                compensated_channel_depth(*section, achieved_width, maximum_nominal_depth);
            maximum_applied_depth = maximum_applied_depth.max(section.required_depth);
            total_applied_depth += f64::from(section.required_depth);
            maximum_achieved_width = maximum_achieved_width.max(achieved_width);
            total_achieved_width += f64::from(achieved_width);
            section_count += 1;
        }
    }
    log_cross_section_diagnostics(
        unresolved,
        maximum_applied_depth,
        total_applied_depth / section_count.max(1) as f64,
        maximum_achieved_width,
        total_achieved_width / section_count.max(1) as f64,
    );
}

pub(super) fn compensated_channel_depth(
    section: RiverCrossSection,
    achieved_width: f32,
    maximum_depth: f32,
) -> f32 {
    let target_width = section.target_half_width * 2.0;
    let relative_error = ((target_width - achieved_width) / target_width.max(f32::EPSILON)).clamp(
        -MAXIMUM_WIDTH_DEPTH_COMPENSATION,
        MAXIMUM_WIDTH_DEPTH_COMPENSATION,
    );
    section
        .nominal_depth
        .mul_add(relative_error, section.nominal_depth)
        .clamp(section.nominal_depth * 0.5, maximum_depth)
}

#[derive(Debug)]
pub(super) struct RiverCorridorCarve {
    pub(super) original_heights: Vec<f32>,
    pub(super) lowered: Vec<bool>,
}

pub(super) fn carve_river_corridor(
    network: &RiverNetwork,
    terrain: &mut RiverTerrain<'_>,
    footprint: &RiverFootprint,
    parameters: RiverChannelParameters,
    budgets: &mut [RiverSedimentBudget],
) -> RiverCorridorCarve {
    let original_heights = terrain
        .mesh
        .vertices
        .iter()
        .map(|vertex| vertex.z)
        .collect();
    let mut lowered = vec![false; terrain.mesh.vertices.len()];
    for (vertex, was_lowered) in lowered.iter_mut().enumerate() {
        if footprint.coverage[vertex] == 0 {
            continue;
        }
        let Some(owner) = footprint.owner[vertex] else {
            continue;
        };
        let river = owner.key.river as usize;
        let Some(floor) = river_floor(network, terrain.mesh, terrain.adjacency, owner, parameters)
        else {
            continue;
        };
        *was_lowered = terrain.mesh.vertices[vertex].z > floor + f32::EPSILON;
        terrain.carve_vertex(
            vertex,
            floor,
            RIVER_CHANNEL_DRAINAGE_FLOOR,
            &mut budgets[river],
        );
    }
    RiverCorridorCarve {
        original_heights,
        lowered,
    }
}

pub(super) fn river_floor(
    network: &RiverNetwork,
    mesh: &Mesh,
    adjacency: &Adjacency,
    owner: RiverChannelFootprintOwner,
    parameters: RiverChannelParameters,
) -> Option<f32> {
    if let Some(floor) = owner.floor_override {
        return Some(floor);
    }
    let river = owner.key.river as usize;
    let node_index = owner.key.node as usize;
    river_node_floor(network, mesh, adjacency, river, node_index, parameters)
}

pub(super) fn river_node_floor(
    network: &RiverNetwork,
    mesh: &Mesh,
    adjacency: &Adjacency,
    river: usize,
    node_index: usize,
    parameters: RiverChannelParameters,
) -> Option<f32> {
    let node = *network.rivers.get(river)?.nodes.get(node_index)?;
    let depth = network
        .cross_sections
        .get(river)
        .and_then(|sections| sections.get(node_index))
        .filter(|section| section.required_depth > 0.0)
        .map_or_else(
            || {
                unfitted_river_depth(
                    mesh,
                    adjacency,
                    node,
                    network.max_height,
                    network.max_flow,
                    parameters.depth_multiplier,
                )
            },
            |section| section.required_depth,
        );
    Some(node.surface - depth)
}

#[derive(Clone, Copy, Debug)]
pub(super) struct RiverValleyCandidate {
    pub(super) vertex: usize,
    pub(super) distance: u8,
    pub(super) owner: RiverChannelFootprintOwner,
}

pub(super) fn lower_river_surroundings(
    network: &RiverNetwork,
    terrain: &mut RiverTerrain<'_>,
    footprint: &RiverFootprint,
    parameters: RiverChannelParameters,
    carve: &RiverCorridorCarve,
    budgets: &mut [RiverSedimentBudget],
) {
    let mut distances = vec![u8::MAX; terrain.mesh.vertices.len()];
    let mut owners = vec![None; terrain.mesh.vertices.len()];
    let mut frontiers: [Vec<RiverValleyCandidate>; RIVER_VALLEY_APRON_RINGS as usize + 1] =
        std::array::from_fn(|_| Vec::new());

    for vertex in 0..terrain.mesh.vertices.len() {
        if !is_river_boundary(footprint.coverage[vertex]) {
            continue;
        }
        let Some(owner) = footprint.owner[vertex] else {
            continue;
        };
        for &neighbour in &terrain.adjacency[vertex] {
            if footprint.coverage[neighbour] == 0 {
                frontiers[1].push(RiverValleyCandidate {
                    vertex: neighbour,
                    distance: 1,
                    owner,
                });
            }
        }
    }

    for distance in 1..=RIVER_VALLEY_APRON_RINGS {
        while let Some(candidate) = frontiers[distance as usize].pop() {
            let vertex = candidate.vertex;
            if footprint.coverage[vertex] != 0
                || network.perimeter.get(vertex).copied().unwrap_or(false)
                || network.ocean.get(vertex).copied().unwrap_or(false)
                || candidate.distance > distances[vertex]
            {
                continue;
            }
            if candidate.distance == distances[vertex]
                && owners[vertex].is_some_and(|current: RiverChannelFootprintOwner| {
                    current.surface <= candidate.owner.surface
                })
            {
                continue;
            }
            distances[vertex] = candidate.distance;
            owners[vertex] = Some(candidate.owner);
            if distance == RIVER_VALLEY_APRON_RINGS {
                continue;
            }
            for &neighbour in &terrain.adjacency[vertex] {
                if footprint.coverage[neighbour] == 0 {
                    frontiers[distance as usize + 1].push(RiverValleyCandidate {
                        vertex: neighbour,
                        distance: distance + 1,
                        owner: candidate.owner,
                    });
                }
            }
        }
    }

    for (vertex, owner) in owners.into_iter().enumerate() {
        let Some(owner) = owner else {
            continue;
        };
        let Some(floor) = river_floor(network, terrain.mesh, terrain.adjacency, owner, parameters)
        else {
            continue;
        };
        let bank_height = if owner.waterfall_lip {
            owner.surface
        } else {
            (floor - owner.surface).mul_add(RIVER_VALLEY_BANK_DEPTH_FRACTION, owner.surface)
        };
        let progress = f32::from(distances[vertex]) / f32::from(RIVER_VALLEY_APRON_RINGS + 1);
        let smooth_progress = progress * progress * (3.0 - 2.0 * progress);
        let original_height = carve.original_heights[vertex];
        let target = (original_height - bank_height).mul_add(smooth_progress, bank_height);
        if target < terrain.mesh.vertices[vertex].z {
            let river = owner.key.river as usize;
            terrain.carve_vertex(
                vertex,
                target,
                RIVER_VALLEY_DRAINAGE_FLOOR,
                &mut budgets[river],
            );
        }
    }
}

pub(super) fn smooth_river_corridor(
    network: &RiverNetwork,
    mesh: &mut Mesh,
    adjacency: &Adjacency,
    footprint: &RiverFootprint,
    parameters: RiverChannelParameters,
    carve: &RiverCorridorCarve,
) {
    let original = mesh.vertices.clone();
    let mut adjusted = Vec::with_capacity(
        footprint
            .coverage
            .iter()
            .filter(|&&coverage| coverage != 0)
            .count(),
    );
    for vertex in 0..original.len() {
        if footprint.coverage[vertex] == 0 {
            continue;
        }
        let Some(owner) = footprint.owner[vertex] else {
            continue;
        };
        let include_surrounding_terrain = is_river_boundary(footprint.coverage[vertex]);
        let (sum, count) = adjacency[vertex]
            .iter()
            .copied()
            .filter(|&neighbour| include_surrounding_terrain || footprint.coverage[neighbour] != 0)
            .fold((original[vertex], 1_u32), |(sum, count), neighbour| {
                (sum + original[neighbour], count + 1)
            });
        let average = sum / count as f32;
        let mut target = original[vertex].lerp(average, RIVER_CORRIDOR_SMOOTHING);
        let river = owner.key.river as usize;
        let node = owner.key.node as usize;
        if let Some((river_node, floor)) = network.rivers[river]
            .nodes
            .get(node)
            .zip(river_floor(network, mesh, adjacency, owner, parameters))
        {
            let water_clearance = river_node.surface - RIVER_SURFACE_OFFSET;
            let minimum_height = floor.min(original[vertex].z);
            let maximum_height = if carve.lowered[vertex] {
                carve.original_heights[vertex].min(water_clearance)
            } else {
                original[vertex].z
            }
            .max(minimum_height);
            target.z = target.z.clamp(minimum_height, maximum_height);
        }
        adjusted.push((vertex, target));
    }

    let vertex_faces = VertexFaceAdjacency::new(mesh);
    let mut projected_areas = ProjectedFaceAreas::new(mesh);
    for (vertex, target) in adjusted {
        let current = mesh.vertices[vertex];
        let horizontal_target = target.truncate().extend(current.z);
        let safe =
            projected_areas.safe_move_fraction(mesh, &vertex_faces, vertex, horizontal_target);
        mesh.vertices[vertex] = current
            .truncate()
            .lerp(target.truncate(), safe)
            .extend(target.z);
        projected_areas.update_incident(mesh, &vertex_faces, vertex);
        if !mesh.uv.is_empty() {
            mesh.uv[vertex] = mesh.vertices[vertex].truncate();
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(super) struct ConfluenceCarveTarget {
    pub(super) height: f32,
    pub(super) river: usize,
}

/// Cuts the short centreline gap left when two river footprints touch before
/// their traced centreline vertices meet. This runs after ordinary corridor
/// smoothing so the join cannot be rebuilt into a ridge.
pub(super) fn carve_confluence_connectors(
    network: &RiverNetwork,
    terrain: &mut RiverTerrain<'_>,
    footprint: &RiverFootprint,
    parameters: RiverChannelParameters,
    budgets: &mut [RiverSedimentBudget],
) -> usize {
    let mut targets = vec![None::<ConfluenceCarveTarget>; terrain.mesh.vertices.len()];
    for (river_index, river) in network.rivers.iter().enumerate() {
        let Some(joined_river) = river.join else {
            continue;
        };
        let Some((terminal, join_vertex)) = river
            .nodes
            .last()
            .map(|node| node.vertex)
            .zip(network.join_vertices.get(river_index).copied().flatten())
        else {
            continue;
        };
        let Some(join_node) = network.rivers[joined_river]
            .nodes
            .iter()
            .position(|node| node.vertex == join_vertex)
        else {
            continue;
        };
        let path = confluence_connector(network, terrain.adjacency, terminal, join_vertex);
        if path.len() < 2 {
            continue;
        }
        let Some((terminal_floor, join_floor)) = river_node_floor(
            network,
            terrain.mesh,
            terrain.adjacency,
            river_index,
            river.nodes.len() - 1,
            parameters,
        )
        .zip(river_node_floor(
            network,
            terrain.mesh,
            terrain.adjacency,
            joined_river,
            join_node,
            parameters,
        )) else {
            continue;
        };

        let final_step = path.len() - 1;
        for (step, &vertex) in path.iter().enumerate() {
            let progress = step as f32 / final_step as f32;
            let floor = (join_floor - terminal_floor).mul_add(progress, terminal_floor);
            record_confluence_carve_target(&mut targets, vertex, floor, river_index);
            for &neighbour in &terrain.adjacency[vertex] {
                if footprint.coverage[neighbour] == 0
                    || is_river_boundary(footprint.coverage[neighbour])
                {
                    continue;
                }
                let shoulder = (terrain.mesh.vertices[neighbour].z - floor).mul_add(0.5, floor);
                record_confluence_carve_target(&mut targets, neighbour, shoulder, river_index);
            }
        }
    }

    let mut lowered = 0;
    for (vertex, target) in targets.into_iter().enumerate() {
        let Some(target) = target else {
            continue;
        };
        let depth = terrain.mesh.vertices[vertex].z - target.height;
        if depth > f32::EPSILON {
            terrain.lower_vertex_exactly(vertex, depth, &mut budgets[target.river]);
            lowered += 1;
        }
    }
    lowered
}

pub(super) fn record_confluence_carve_target(
    targets: &mut [Option<ConfluenceCarveTarget>],
    vertex: usize,
    height: f32,
    river: usize,
) {
    if targets[vertex].is_none_or(|target| height < target.height) {
        targets[vertex] = Some(ConfluenceCarveTarget { height, river });
    }
}

#[cfg(feature = "profiling")]
pub(super) fn log_cross_section_diagnostics(
    unresolved: usize,
    maximum_applied_depth: f32,
    mean_applied_depth: f64,
    maximum_achieved_width: f32,
    mean_achieved_width: f64,
) {
    let maximum_applied_depth_metres = maximum_applied_depth * ISLAND_WORLD_METRES;
    let mean_applied_depth_metres = mean_applied_depth * f64::from(ISLAND_WORLD_METRES);
    let maximum_achieved_width_metres = maximum_achieved_width * ISLAND_WORLD_METRES;
    let mean_achieved_width_metres = mean_achieved_width * f64::from(ISLAND_WORLD_METRES);
    eprintln!(
        "river_cross_sections,unresolved={unresolved},mean_width_metres={mean_achieved_width_metres:.3},maximum_width_metres={maximum_achieved_width_metres:.3},mean_depth_metres={mean_applied_depth_metres:.3},maximum_depth_metres={maximum_applied_depth_metres:.3}"
    );
}

#[cfg(not(feature = "profiling"))]
pub(super) const fn log_cross_section_diagnostics(
    _unresolved: usize,
    _maximum_applied_depth: f32,
    _mean_applied_depth: f64,
    _maximum_achieved_width: f32,
    _mean_achieved_width: f64,
) {
}

pub(super) type CrossSectionSample = Option<(f32, f32)>;

pub(super) fn cross_section_samples(
    network: &RiverNetwork,
    mesh: &Mesh,
    footprint: &RiverFootprint,
) -> Vec<Vec<[Option<f32>; 2]>> {
    let mut samples: Vec<Vec<[CrossSectionSample; 2]>> = network
        .rivers
        .iter()
        .map(|river| vec![[None, None]; river.nodes.len()])
        .collect();
    for vertex in 0..mesh.vertices.len() {
        if !is_river_boundary(footprint.coverage[vertex]) {
            continue;
        }
        let Some(owner) = footprint.owner[vertex] else {
            continue;
        };
        if owner.flow_direction == Vec2::ZERO {
            continue;
        }
        let river = owner.key.river as usize;
        let node = owner.key.node as usize;
        let offset = mesh.vertices[vertex].truncate() - owner.flow_origin;
        let across = Vec2::new(-owner.flow_direction.y, owner.flow_direction.x);
        let lateral = offset.dot(across);
        if lateral.abs() <= f32::EPSILON {
            continue;
        }
        let longitudinal = offset.dot(owner.flow_direction).abs();
        let local_spacing = local_path_spacing(mesh, &network.rivers[river].nodes, node);
        if longitudinal > local_spacing * 0.5 {
            continue;
        }
        let side = usize::from(lateral > 0.0);
        let candidate = (longitudinal, lateral.abs());
        let sample = &mut samples[river][node][side];
        if sample.is_none_or(|current| candidate.0 < current.0) {
            *sample = Some(candidate);
        }
    }
    samples
        .into_iter()
        .map(|river| {
            river
                .into_iter()
                .map(|sides| {
                    [
                        sides[0].map(|sample| sample.1),
                        sides[1].map(|sample| sample.1),
                    ]
                })
                .collect()
        })
        .collect()
}

pub(super) fn local_path_spacing(mesh: &Mesh, nodes: &[RiverNode], index: usize) -> f32 {
    let position = mesh.vertices[nodes[index].vertex].truncate();
    let upstream = index
        .checked_sub(1)
        .map(|previous| position.distance(mesh.vertices[nodes[previous].vertex].truncate()));
    let downstream = nodes
        .get(index + 1)
        .map(|next| position.distance(mesh.vertices[next.vertex].truncate()));
    match (upstream, downstream) {
        (Some(upstream), Some(downstream)) => upstream.max(downstream),
        (Some(spacing), None) | (None, Some(spacing)) => spacing,
        (None, None) => f32::INFINITY,
    }
}

pub(super) fn fill_missing_widths(widths: &mut [Option<f32>]) {
    let mut previous = None;
    for width in widths.iter_mut() {
        if width.is_none() {
            *width = previous;
        } else {
            previous = *width;
        }
    }
    let mut next = None;
    for width in widths.iter_mut().rev() {
        if width.is_none() {
            *width = next;
        } else {
            next = *width;
        }
    }
}

pub(super) fn river_centreline_mask(network: &RiverNetwork, vertex_count: usize) -> Vec<bool> {
    let mut centreline = vec![false; vertex_count];
    for node in network.rivers.iter().flat_map(|river| &river.nodes) {
        centreline[node.vertex] = true;
    }
    centreline
}

pub(super) fn river_node_water_position(terrain: &Mesh, node: &RiverNode) -> Vec3 {
    let position = terrain.vertices[node.vertex];
    Vec3::new(position.x, position.y, node.surface)
}

pub(super) fn river_node_flow_direction(terrain: &Mesh, nodes: &[RiverNode], index: usize) -> Vec2 {
    let current = terrain.vertices[nodes[index].vertex].truncate();
    let upstream = index.checked_sub(1).map_or(current, |previous| {
        terrain.vertices[nodes[previous].vertex].truncate()
    });
    let downstream = nodes
        .get(index + 1)
        .map_or(current, |next| terrain.vertices[next.vertex].truncate());
    let central = (downstream - upstream).normalize_or_zero();
    if central == Vec2::ZERO {
        let downstream_direction = (downstream - current).normalize_or_zero();
        if downstream_direction == Vec2::ZERO {
            (current - upstream).normalize_or_zero()
        } else {
            downstream_direction
        }
    } else {
        central
    }
}

pub(super) fn river_ring_count(flow: u32, max_flow: u32) -> u8 {
    (river_half_width(flow, max_flow, 1.0).ceil() as u8).min(MAX_RIVER_RINGS)
}

pub(super) fn river_half_width(flow: u32, max_flow: u32, base_width: f32) -> f32 {
    let normalized_flow = (flow as f32 / max_flow.max(1) as f32).sqrt();
    base_width * normalized_flow.mul_add(1.9, 0.58)
}

pub(super) fn mark_river_boundary(adjacency: &Adjacency, perimeter: &[bool], coverage: &mut [u8]) {
    for vertex in 0..coverage.len() {
        if coverage[vertex] != 0
            && (perimeter[vertex]
                || adjacency[vertex]
                    .iter()
                    .any(|&neighbour| coverage[neighbour] == 0))
        {
            coverage[vertex] |= RIVER_BOUNDARY;
        }
    }
}

pub(super) fn is_river_boundary(coverage: u8) -> bool {
    coverage & RIVER_BOUNDARY != 0
}

pub(super) fn river_topology_masks(terrain: &Mesh, coverage: &[u8]) -> (Vec<bool>, Vec<bool>) {
    let selected_count = coverage.iter().filter(|&&value| value != 0).count();
    let mut edges = Vec::with_capacity(selected_count.saturating_mul(6));
    let mut under_river = vec![false; terrain.vertices.len()];
    for triangle in terrain.triangles.chunks_exact(3) {
        if !is_river_bed_triangle(triangle, coverage) {
            continue;
        }
        for &vertex in triangle {
            under_river[vertex as usize] = true;
        }
        for (a, b) in [
            (triangle[0], triangle[1]),
            (triangle[1], triangle[2]),
            (triangle[2], triangle[0]),
        ] {
            let (a, b) = if a < b { (a, b) } else { (b, a) };
            edges.push((u64::from(a) << 32) | u64::from(b));
        }
    }
    edges.sort_unstable();
    let mut bank = vec![false; terrain.vertices.len()];
    let mut start = 0;
    while start < edges.len() {
        let edge = edges[start];
        let mut end = start + 1;
        while end < edges.len() && edges[end] == edge {
            end += 1;
        }
        if end - start == 1 {
            bank[(edge >> 32) as usize] = true;
            bank[(edge & u64::from(u32::MAX)) as usize] = true;
        }
        start = end;
    }
    (under_river, bank)
}

pub(super) fn is_river_bed_triangle(triangle: &[u32], coverage: &[u8]) -> bool {
    triangle
        .iter()
        .all(|&vertex| coverage[vertex as usize] != 0)
        && !triangle
            .iter()
            .all(|&vertex| is_river_boundary(coverage[vertex as usize]))
}

pub(super) fn duplicate_river_topology(
    terrain: &Mesh,
    coverage: &[u8],
    surfaces: &[f32],
    river_uv: &[Vec2],
    waterfall: &WaterfallTerrainConstraints,
) -> Mesh {
    debug_assert_eq!(terrain.vertices.len(), waterfall.support.len());
    debug_assert_eq!(terrain.vertices.len(), waterfall.water_unclamped.len());
    let selected_count = coverage.iter().filter(|&&remaining| remaining > 0).count();
    let mut mapping = vec![u32::MAX; terrain.vertices.len()];
    let mut xy_mapping = HashMap::<(u32, u32), u32>::with_capacity(selected_count);
    let mut minimum_heights = Vec::<f32>::with_capacity(selected_count);
    let mut out = Mesh {
        vertices: Vec::with_capacity(selected_count),
        normals: Vec::new(),
        triangles: Vec::new(),
        uv: Vec::with_capacity(selected_count),
    };

    for (index, &remaining) in coverage.iter().enumerate() {
        if remaining == 0 {
            continue;
        }
        let mut vertex = terrain.vertices[index];
        let boundary = is_river_boundary(remaining);
        let waterfall_support = waterfall.support[index];
        let water_unclamped = waterfall.water_unclamped[index];
        let minimum_height = if boundary || waterfall_support || water_unclamped {
            f32::NEG_INFINITY
        } else {
            vertex.z + RIVER_SURFACE_OFFSET
        };
        vertex.z = if waterfall_support {
            vertex.z + WATERFALL_WATER_CLEARANCE
        } else if boundary {
            vertex.z.min(surfaces[index])
        } else if water_unclamped {
            surfaces[index] + RIVER_SURFACE_OFFSET
        } else {
            (surfaces[index] + RIVER_SURFACE_OFFSET).max(minimum_height)
        };
        let key = (vertex.x.to_bits(), vertex.y.to_bits());
        if let Some(&mapped) = xy_mapping.get(&key) {
            mapping[index] = mapped;
            let mapped = mapped as usize;
            if boundary || minimum_heights[mapped] == f32::NEG_INFINITY {
                out.vertices[mapped].z = out.vertices[mapped].z.min(vertex.z);
                minimum_heights[mapped] = f32::NEG_INFINITY;
            } else {
                out.vertices[mapped].z = out.vertices[mapped].z.max(vertex.z);
                minimum_heights[mapped] = minimum_heights[mapped].max(minimum_height);
            }
            out.uv[mapped] = (out.uv[mapped] + river_uv[index]) * 0.5;
            continue;
        }
        let mapped = out.vertices.len() as u32;
        mapping[index] = mapped;
        xy_mapping.insert(key, mapped);
        out.vertices.push(vertex);
        minimum_heights.push(minimum_height);
        out.uv.push(river_uv[index]);
    }

    for triangle in terrain.triangles.chunks_exact(3) {
        let mapped = [
            mapping[triangle[0] as usize],
            mapping[triangle[1] as usize],
            mapping[triangle[2] as usize],
        ];
        let boundary_only = triangle
            .iter()
            .all(|&vertex| is_river_boundary(coverage[vertex as usize]));
        let distinct = mapped[0] != mapped[1] && mapped[1] != mapped[2] && mapped[2] != mapped[0];
        if mapped.iter().all(|&vertex| vertex != u32::MAX) && distinct && !boundary_only {
            out.triangles.extend(mapped);
        }
    }
    for (vertex, &minimum_height) in out.vertices.iter_mut().zip(&minimum_heights) {
        vertex.z = vertex.z.max(minimum_height);
    }
    out.calculate_normals();
    out
}

/// Replaces the river mesh's lateral UV coordinate with the shortest
/// horizontal mesh distance to a bank. The downstream coordinate remains in V.
///
/// This runs on the compact, complete river mesh before it is sliced for Unity,
/// so generated tiles interpolate one continuous distance field at their seams.
pub(crate) fn encode_bank_distance_in_uv(mesh: &mut Mesh) {
    debug_assert_eq!(mesh.uv.len(), mesh.vertices.len());
    if mesh.vertices.is_empty() || mesh.uv.len() != mesh.vertices.len() {
        return;
    }

    let adjacency = mesh.adjacency();
    let perimeter = mesh.perimeter_mask();
    let mut distances = vec![f32::INFINITY; mesh.vertices.len()];
    let mut queue = BinaryHeap::new();

    for (vertex, is_bank) in perimeter.into_iter().enumerate() {
        if is_bank {
            distances[vertex] = 0.0;
            queue.push(RouteState { cost: 0.0, vertex });
        }
    }

    while let Some(RouteState { cost, vertex }) = queue.pop() {
        if cost > distances[vertex] {
            continue;
        }
        let position = mesh.vertices[vertex].truncate();
        for &neighbour in &adjacency[vertex] {
            let candidate = cost + position.distance(mesh.vertices[neighbour].truncate());
            if candidate < distances[neighbour] {
                distances[neighbour] = candidate;
                queue.push(RouteState {
                    cost: candidate,
                    vertex: neighbour,
                });
            }
        }
    }

    for (uv, distance) in mesh.uv.iter_mut().zip(distances) {
        uv.x = distance;
    }
}

#[cfg(test)]
pub(super) fn round_waterfall_lips(
    mut mesh: Mesh,
    mut waterfall_lips: Vec<bool>,
    mut minimum_heights: Vec<f32>,
) -> Mesh {
    debug_assert_eq!(mesh.vertices.len(), minimum_heights.len());
    if !waterfall_lips.iter().any(|&is_lip| is_lip) || mesh.triangles.is_empty() {
        for (vertex, &minimum_height) in mesh.vertices.iter_mut().zip(&minimum_heights) {
            vertex.z = vertex.z.max(minimum_height);
        }
        mesh.calculate_normals();
        return mesh;
    }

    let midpoints = mesh.tessellate_incident_to(&waterfall_lips);
    waterfall_lips.reserve(midpoints.len());
    for stencil in midpoints {
        let [a, b] = [stencil.surrounding[0], stencil.surrounding[1]];
        let midpoint = stencil.vertex;
        debug_assert_eq!(midpoint as usize, waterfall_lips.len());
        waterfall_lips.push(waterfall_lips[a as usize] && waterfall_lips[b as usize]);
        minimum_heights.push((minimum_heights[a as usize] + minimum_heights[b as usize]) * 0.5);
    }
    mesh.optimize_surface_triangulation_where_preserving(
        |vertex| waterfall_lips[vertex as usize],
        |a, b| waterfall_lips[a as usize] && waterfall_lips[b as usize],
    );
    let adjacency = mesh.adjacency();
    let perimeter = mesh.perimeter_mask();
    let mut adjusted = Vec::with_capacity(waterfall_lips.iter().filter(|&&is_lip| is_lip).count());

    for (vertex, &is_lip) in waterfall_lips.iter().enumerate() {
        if !is_lip || perimeter[vertex] || adjacency[vertex].is_empty() {
            continue;
        }
        let current = mesh.vertices[vertex];
        let (total, count, minimum, maximum) = adjacency[vertex].iter().copied().fold(
            (current, 1_u32, current.z, current.z),
            |(total, count, minimum, maximum), neighbour| {
                let position = mesh.vertices[neighbour];
                (
                    total + position,
                    count + 1,
                    minimum.min(position.z),
                    maximum.max(position.z),
                )
            },
        );
        let average = total / count as f32;
        let normal = Vec3::Z;
        let displacement = (average - current).dot(normal) * WATERFALL_LIP_SMOOTHING;
        let mut rounded = current + normal * displacement;
        rounded.z = rounded.z.clamp(minimum, maximum);
        rounded.z = rounded.z.max(minimum_heights[vertex]);
        adjusted.push((vertex, rounded));
    }

    for (vertex, position) in adjusted {
        mesh.vertices[vertex] = position;
    }
    for (vertex, &minimum_height) in mesh.vertices.iter_mut().zip(&minimum_heights) {
        vertex.z = vertex.z.max(minimum_height);
    }
    mesh.calculate_normals();
    mesh
}

pub(super) fn apply_averaged(
    mesh: &mut Mesh,
    accumulated: &[Vec3],
    count: &[u32],
    perimeter: &[bool],
) {
    for (index, vertex) in mesh.vertices.iter_mut().enumerate() {
        if count[index] > 0 && !perimeter[index] {
            *vertex = accumulated[index] / count[index] as f32;
        }
    }
    if !mesh.uv.is_empty() {
        mesh.uv = mesh
            .vertices
            .iter()
            .map(|vertex| vertex.truncate())
            .collect();
    }
}
