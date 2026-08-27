use super::{
    Adjacency, BinaryHeap, HashSet, MAXIMUM_GENTLE_RIVER_GRADE, Mesh, Ordering,
    RIVER_CHANNEL_DRAINAGE_FLOOR, RIVER_SURFACE_OFFSET, RiverChannelSettings, RiverCrossSection,
    RiverNode, RiverSedimentBudget, SEA_PLANE_CLEARANCE, SurfaceMaterial, Vec2, VecDeque,
    WATERFALL_LANDING_LENGTH_MULTIPLIER, WATERFALL_SITE_BYPASS_MAX_HOPS,
    WATERFALL_SITE_MINIMUM_BANK_SPAN_FRACTION, WATERFALL_SUPPORT_RUN, WATERFALL_TARGET_EDGE_LENGTH,
    WaterfallClearanceIndex, WaterfallPatch, expand_vertex_mask_through_river_to_banks,
};

#[derive(Clone, Copy, Debug)]
pub(super) struct WaterfallDrop {
    pub(super) segment: usize,
    pub(super) height: f32,
    pub(super) placed: bool,
}

pub(super) struct RiverTerrain<'a> {
    pub(super) mesh: &'a mut Mesh,
    pub(super) adjacency: &'a Adjacency,
    pub(super) material: &'a mut SurfaceMaterial,
    pub(super) bedrock_rates: &'a [f32],
    pub(super) control_areas: &'a [f32],
}

impl RiverTerrain<'_> {
    pub(super) fn lower_vertex_exactly(
        &mut self,
        vertex: usize,
        depth: f32,
        budget: &mut RiverSedimentBudget,
    ) {
        if depth <= 0.0 {
            return;
        }
        let loose_removed = depth.min(self.material.depths()[vertex]);
        let bedrock_removed = depth - loose_removed;
        self.mesh.vertices[vertex].z -= depth;
        let loose_depth = &mut self.material.depths_mut()[vertex];
        *loose_depth = (*loose_depth - loose_removed).max(0.0);
        if *loose_depth < crate::terrain::LOOSE_DEPTH_EPSILON {
            *loose_depth = 0.0;
        }
        budget.record_erosion(loose_removed, bedrock_removed, self.control_areas[vertex]);
    }

    pub(super) fn carve_vertex(
        &mut self,
        vertex: usize,
        target: f32,
        drainage_floor: f32,
        budget: &mut RiverSedimentBudget,
    ) {
        let requested = (self.mesh.vertices[vertex].z - target).max(0.0);
        if requested == 0.0 {
            return;
        }
        let loose_removed = requested.min(self.material.depths()[vertex]);
        let bedrock_rate = self.bedrock_rates[vertex]
            .max(drainage_floor)
            .clamp(0.0, 1.0);
        let bedrock_removed = (requested - loose_removed) * bedrock_rate;
        self.mesh.vertices[vertex].z -= loose_removed + bedrock_removed;
        let loose_depth = &mut self.material.depths_mut()[vertex];
        *loose_depth = (*loose_depth - loose_removed).max(0.0);
        if *loose_depth < crate::terrain::LOOSE_DEPTH_EPSILON {
            *loose_depth = 0.0;
        }
        budget.record_erosion(loose_removed, bedrock_removed, self.control_areas[vertex]);
    }

    pub(super) fn deposit_vertex(
        &mut self,
        vertex: usize,
        requested_depth: f32,
        available_volume: &mut f64,
    ) -> f64 {
        let area = f64::from(self.control_areas[vertex].max(0.0));
        if requested_depth <= 0.0 || area <= f64::EPSILON || *available_volume <= 0.0 {
            return 0.0;
        }
        let volume = (f64::from(requested_depth) * area).min(*available_volume);
        let depth = (volume / area) as f32;
        self.mesh.vertices[vertex].z += depth;
        self.material.depths_mut()[vertex] += depth;
        *available_volume -= volume;
        volume
    }
}

#[derive(Clone, Copy, Debug)]
pub(super) struct RiverChannelParameters {
    pub(super) depth_multiplier: f32,
}

#[derive(Debug)]
pub(super) struct RiverCarveScratch {
    pub(super) gradients: Vec<f32>,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct RiverProfileEnvironment<'a> {
    pub(super) mesh: &'a Mesh,
    pub(super) adjacency: &'a Adjacency,
    pub(super) ocean: &'a [bool],
}

#[derive(Debug, Default)]
pub(super) struct RiverProfileScratch {
    pub(super) gradients: Vec<f32>,
    pub(super) waterfall_drops: Vec<WaterfallDrop>,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct WaterfallSiteEnvironment<'a> {
    pub(super) adjacency: &'a Adjacency,
    pub(super) coverage: &'a [u8],
    pub(super) ocean: &'a [bool],
    pub(super) perimeter: &'a [bool],
    pub(super) rejected: &'a HashSet<usize>,
}

impl RiverCarveScratch {
    pub(super) fn new(_vertex_count: usize) -> Self {
        Self {
            gradients: Vec::new(),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(super) struct WaterfallRelocation<'a> {
    pub(super) clearance: &'a WaterfallClearanceIndex,
    pub(super) site: Option<WaterfallSiteEnvironment<'a>>,
    pub(super) river: usize,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct RiverCarveOptions<'a> {
    pub(super) form_deltas: bool,
    pub(super) channel_settings: RiverChannelSettings,
    pub(super) rejected_waterfall_vertices: &'a HashSet<usize>,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct RiverCarveParameters<'a> {
    pub(super) downstream_surface: f32,
    pub(super) terminal_ocean: bool,
    pub(super) max_height: f32,
    pub(super) max_flow: u32,
    pub(super) depth_multiplier: f32,
    pub(super) cross_sections: &'a [RiverCrossSection],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct RiverMouthTransition {
    pub(super) waterfall_segment: Option<usize>,
    pub(super) river_mesh_end: usize,
}

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct RiverCarveResult {
    pub(super) budget: RiverSedimentBudget,
    pub(super) river_mesh_end: Option<usize>,
}

pub(super) fn shape_and_carve_river(
    terrain: &mut RiverTerrain<'_>,
    nodes: &mut [RiverNode],
    waterfalls: &mut [bool],
    scratch: &mut RiverCarveScratch,
    ocean: &[bool],
    carve_submerged_mouth: bool,
    parameters: RiverCarveParameters<'_>,
) -> RiverCarveResult {
    let ocean_entry = parameters
        .terminal_ocean
        .then(|| river_ocean_entry(nodes, ocean))
        .flatten();
    let mouth = ocean_entry.map(|ocean_entry| river_mouth_transition(ocean_entry, waterfalls));
    let mut budget = RiverSedimentBudget::default();
    level_confluence_reach(
        terrain,
        nodes,
        waterfalls,
        parameters.downstream_surface,
        &mut budget,
    );
    let bed_end = mouth.map_or_else(
        || nodes.len().checked_sub(1),
        |mouth| mouth.waterfall_segment,
    );
    if let Some(bed_end) = bed_end {
        carve_stepped_bed(
            terrain,
            nodes,
            waterfalls,
            bed_end,
            parameters,
            &mut scratch.gradients,
            &mut budget,
        );
    }
    if let Some(mouth) = mouth.filter(|_| carve_submerged_mouth) {
        carve_submerged_river_mouth(
            terrain,
            nodes,
            waterfalls,
            mouth,
            parameters.max_height,
            parameters.cross_sections,
            &mut budget,
        );
    }
    RiverCarveResult {
        budget,
        river_mesh_end: mouth
            .filter(|_| carve_submerged_mouth)
            .map(|mouth| mouth.river_mesh_end),
    }
}

pub(super) fn form_river_profile(
    environment: RiverProfileEnvironment<'_>,
    nodes: &mut [RiverNode],
    waterfalls: &mut [bool],
    parameters: RiverCarveParameters<'_>,
    gradient_scratch: &mut Vec<f32>,
) -> Option<usize> {
    let mut surface = parameters.downstream_surface;
    let mut water_surface = parameters.downstream_surface;
    for (index, node) in nodes.iter_mut().enumerate().rev() {
        let vertex = environment.mesh.vertices[node.vertex];
        surface = surface.max(vertex.z).max(0.0);
        let depth = river_depth(
            environment.mesh,
            environment.adjacency,
            *node,
            parameters,
            parameters.cross_sections.get(index).copied(),
        );
        water_surface = water_surface.max(surface - depth * 0.35);
        node.surface = water_surface;
    }

    let ocean_entry = parameters
        .terminal_ocean
        .then(|| river_ocean_entry(nodes, environment.ocean))
        .flatten();
    let profile_end = ocean_entry.map_or_else(
        || nodes.len().saturating_sub(1),
        |ocean_entry| ocean_entry.saturating_sub(1),
    );
    form_stepped_profile(
        nodes,
        waterfalls,
        parameters.cross_sections,
        profile_end,
        parameters.max_height,
        gradient_scratch,
    );
    ocean_entry
}

pub(super) fn river_depth(
    mesh: &Mesh,
    adjacency: &Adjacency,
    node: RiverNode,
    parameters: RiverCarveParameters<'_>,
    cross_section: Option<RiverCrossSection>,
) -> f32 {
    if let Some(section) = cross_section.filter(|section| section.required_depth > 0.0) {
        return section.required_depth;
    }
    unfitted_river_depth(
        mesh,
        adjacency,
        node,
        parameters.max_height,
        parameters.max_flow,
        parameters.depth_multiplier,
    )
}

pub(super) fn unfitted_river_depth(
    mesh: &Mesh,
    adjacency: &Adjacency,
    node: RiverNode,
    max_height: f32,
    max_flow: u32,
    depth_multiplier: f32,
) -> f32 {
    let vertex = mesh.vertices[node.vertex];
    let altitude = ((max_height - vertex.z) / max_height.max(f32::EPSILON)).clamp(0.0, 1.0);
    let normalized_flow = (node.flow as f32 / max_flow.max(1) as f32).sqrt();
    let unconstrained = altitude
        * altitude
        * vertex.z.max(0.0)
        * 0.24
        * (node.flow as f32).sqrt()
        * depth_multiplier;
    let edge_limited =
        local_edge_length(mesh, adjacency, node.vertex) * normalized_flow.mul_add(0.18, 0.18);
    unconstrained.min(edge_limited)
}

pub(super) fn carve_stepped_bed(
    terrain: &mut RiverTerrain<'_>,
    nodes: &[RiverNode],
    waterfalls: &[bool],
    end: usize,
    parameters: RiverCarveParameters<'_>,
    bed_targets: &mut Vec<f32>,
    budget: &mut RiverSedimentBudget,
) {
    let end = end.min(nodes.len().saturating_sub(1));
    bed_targets.clear();
    bed_targets.extend(nodes[..=end].iter().enumerate().map(|(index, &node)| {
        node.surface
            - river_depth(
                terrain.mesh,
                terrain.adjacency,
                node,
                parameters,
                parameters.cross_sections.get(index).copied(),
            )
    }));

    let mut reach_start = 0;
    for segment in 0..end {
        if waterfalls[segment] {
            carve_bed_reach(
                terrain,
                &nodes[reach_start..=segment],
                &bed_targets[reach_start..=segment],
                parameters.cross_sections.is_empty(),
                budget,
            );
            reach_start = segment + 1;
        }
    }
    carve_bed_reach(
        terrain,
        &nodes[reach_start..=end],
        &bed_targets[reach_start..=end],
        parameters.cross_sections.is_empty(),
        budget,
    );
}

pub(super) fn carve_bed_reach(
    terrain: &mut RiverTerrain<'_>,
    nodes: &[RiverNode],
    targets: &[f32],
    flatten: bool,
    budget: &mut RiverSedimentBudget,
) {
    if flatten {
        let floor = targets.iter().copied().fold(f32::INFINITY, f32::min);
        for node in nodes {
            terrain.carve_vertex(node.vertex, floor, RIVER_CHANNEL_DRAINAGE_FLOOR, budget);
        }
        return;
    }
    for (node, &target) in nodes.iter().zip(targets) {
        terrain.carve_vertex(node.vertex, target, RIVER_CHANNEL_DRAINAGE_FLOOR, budget);
    }
}

pub(super) fn form_stepped_profile(
    nodes: &mut [RiverNode],
    waterfalls: &mut [bool],
    cross_sections: &[RiverCrossSection],
    end: usize,
    max_height: f32,
    gradient_scratch: &mut Vec<f32>,
) {
    waterfalls.fill(false);
    let end = end.min(nodes.len().saturating_sub(1));
    if end == 0 {
        return;
    }

    gradient_scratch.clear();
    gradient_scratch.reserve(end);
    for segment in 0..end {
        let start = segment.saturating_sub(2);
        let finish = (segment + 2).min(end - 1);
        let mut weighted_gradient = 0.0_f32;
        let mut total_weight = 0.0_f32;
        for neighbour in start..=finish {
            let distance = (nodes[neighbour].position.truncate()
                - nodes[neighbour + 1].position.truncate())
            .length()
            .max(f32::EPSILON);
            let drop = (nodes[neighbour].surface - nodes[neighbour + 1].surface).max(0.0);
            let separation = segment.abs_diff(neighbour) as f32;
            let weight = 3.0 - separation;
            weighted_gradient += drop / distance * weight;
            total_weight += weight;
        }
        gradient_scratch.push(weighted_gradient / total_weight.max(1.0));
    }

    let minimum_fall = max_height * 0.0075;
    let maximum_fall = max_height * 0.018;
    let gentle_reach_length = max_height * 0.006;
    let steep_gradient = (max_height * 2.25).max(f32::EPSILON);
    let mut level = nodes[end].surface;
    let mut reach_length = 0.0_f32;
    let mut reach_half_width = 0.0_f32;
    for index in (0..end).rev() {
        let segment_length =
            (nodes[index].position.truncate() - nodes[index + 1].position.truncate()).length();
        reach_length += segment_length;
        reach_half_width = reach_half_width.max(
            cross_sections
                .get(index)
                .map_or(0.0, |section| section.target_half_width),
        );
        let natural_surface = nodes[index].surface.max(level);
        let available_rise = natural_surface - level;
        let gradient_response = (gradient_scratch[index] / steep_gradient)
            .clamp(0.0, 1.0)
            .sqrt();
        let target_fall = (maximum_fall - minimum_fall).mul_add(gradient_response, minimum_fall);
        let profile_reach_length = gentle_reach_length * (1.0 - gradient_response * 0.9);
        let patch_reach_length =
            WATERFALL_SUPPORT_RUN + reach_half_width * (1.0 + WATERFALL_LANDING_LENGTH_MULTIPLIER);
        let required_reach_length = profile_reach_length.max(patch_reach_length);
        if available_rise >= target_fall && reach_length >= required_reach_length {
            level += target_fall;
            waterfalls[index] = true;
            reach_length = 0.0;
            reach_half_width = 0.0;
        }
        nodes[index].surface = level.min(natural_surface);
    }
}

/// Lowers the terrace containing `anchor` back to its preceding waterfall or
/// source. If the new level would make the receiver rise downstream, the
/// correction crosses successive terraces until the original profile is no
/// higher than `target_surface` again.
///
/// Returns whether the correction reached the river's terminal node.
pub(super) fn lower_profile_reach_through_confluence(
    nodes: &mut [RiverNode],
    waterfalls: &mut [bool],
    anchor: usize,
    target_surface: f32,
) -> bool {
    let Some(anchor_surface) = nodes.get(anchor).map(|node| node.surface) else {
        return false;
    };
    if !target_surface.is_finite() || anchor_surface <= target_surface + f32::EPSILON {
        return false;
    }

    let reach_start = waterfalls[..anchor.min(waterfalls.len())]
        .iter()
        .rposition(|&waterfall| waterfall)
        .map_or(0, |waterfall| waterfall + 1);
    let mut reach_end = anchor;
    while nodes
        .get(reach_end + 1)
        .is_some_and(|node| node.surface > target_surface + f32::EPSILON)
    {
        reach_end += 1;
    }

    for node in &mut nodes[reach_start..=reach_end] {
        node.surface = node.surface.min(target_surface);
    }

    let affected_segment_start = reach_start.saturating_sub(1);
    let affected_segment_end = (reach_end + 1)
        .min(nodes.len().saturating_sub(1))
        .min(waterfalls.len());
    for segment in affected_segment_start..affected_segment_end {
        if waterfalls[segment]
            && nodes[segment].surface <= nodes[segment + 1].surface + f32::EPSILON
        {
            waterfalls[segment] = false;
        }
    }

    reach_end + 1 == nodes.len()
}

pub(super) fn enforce_gentle_river_profile(
    mesh: &Mesh,
    nodes: &[RiverNode],
    waterfalls: &[bool],
    sections: &mut [RiverCrossSection],
) -> usize {
    let segment_count = nodes
        .len()
        .saturating_sub(1)
        .min(waterfalls.len())
        .min(sections.len().saturating_sub(1));
    let mut adjusted = 0;
    for segment in 0..segment_count {
        let distance = mesh.vertices[nodes[segment].vertex]
            .truncate()
            .distance(mesh.vertices[nodes[segment + 1].vertex].truncate())
            .max(f32::EPSILON);
        let maximum_gentle_drop = distance * MAXIMUM_GENTLE_RIVER_GRADE;
        let surface_drop = (nodes[segment].surface - nodes[segment + 1].surface).max(0.0);
        if surface_drop > maximum_gentle_drop + f32::EPSILON {
            // Waterfall selection belongs to `form_stepped_profile`, before
            // conflict relocation and terrace construction. Turning an
            // already-carved slope into a waterfall here creates a curtain
            // without a matching upper terrace or lower landing.
            continue;
        }
        if waterfalls[segment] {
            continue;
        }

        let maximum_downstream_depth = (sections[segment].required_depth + maximum_gentle_drop
            - surface_drop)
            .max(RIVER_SURFACE_OFFSET);
        if sections[segment + 1].required_depth > maximum_downstream_depth {
            sections[segment + 1].required_depth = maximum_downstream_depth;
            adjusted += 1;
        }
    }
    adjusted
}

/// Makes the incoming flat reach meet the joined river without a raised lip.
/// The nearest upstream waterfall absorbs the correction as additional fall,
/// leaving every earlier terrace unchanged. The later bed and bank passes use
/// these shifted surfaces, so the same correction is carved into the terrain.
pub(super) fn level_confluence_reach(
    terrain: &mut RiverTerrain<'_>,
    nodes: &mut [RiverNode],
    waterfalls: &[bool],
    downstream_surface: f32,
    budget: &mut RiverSedimentBudget,
) {
    let Some(terminal_surface) = nodes.last().map(|node| node.surface) else {
        return;
    };
    if !downstream_surface.is_finite() {
        return;
    }

    let correction = terminal_surface - downstream_surface;
    if correction <= f32::EPSILON {
        return;
    }

    let terminal = nodes.len() - 1;
    let reach_start = waterfalls[..terminal]
        .iter()
        .rposition(|&waterfall| waterfall)
        .map_or(0, |waterfall| waterfall + 1);
    for node in &mut nodes[reach_start..] {
        node.surface -= correction;
        terrain.lower_vertex_exactly(node.vertex, correction, budget);
    }
}

pub(super) fn valid_waterfall_site(
    mesh: &Mesh,
    nodes: &[RiverNode],
    segment: usize,
    drop: f32,
    cross_sections: &[RiverCrossSection],
    environment: WaterfallSiteEnvironment<'_>,
) -> bool {
    planned_waterfall_patch(mesh, nodes, segment, drop, cross_sections, environment)
        .and_then(|patch| {
            complete_waterfall_face(mesh, patch, environment).map(|face| (patch, face))
        })
        .is_some_and(|(patch, face)| {
            !waterfall_face_has_side_bypass(mesh, patch, &face, environment)
        })
}

pub(super) fn planned_waterfall_patch(
    mesh: &Mesh,
    nodes: &[RiverNode],
    segment: usize,
    drop: f32,
    cross_sections: &[RiverCrossSection],
    environment: WaterfallSiteEnvironment<'_>,
) -> Option<WaterfallPatch> {
    let (&upper, &lower) = nodes.get(segment).zip(nodes.get(segment + 1))?;
    if drop <= RIVER_SURFACE_OFFSET
        || environment.rejected.contains(&upper.vertex)
        || environment
            .perimeter
            .get(upper.vertex)
            .copied()
            .unwrap_or(true)
        || environment
            .perimeter
            .get(lower.vertex)
            .copied()
            .unwrap_or(true)
        || environment.ocean.get(upper.vertex).copied().unwrap_or(true)
        || environment.ocean.get(lower.vertex).copied().unwrap_or(true)
        || environment.coverage.get(upper.vertex).copied().unwrap_or(0) == 0
        || environment.coverage.get(lower.vertex).copied().unwrap_or(0) == 0
    {
        return None;
    }

    let upper_position = mesh.vertices[upper.vertex].truncate();
    let lower_position = mesh.vertices[lower.vertex].truncate();
    let direction = (lower_position - upper_position).try_normalize()?;
    let follows_upstream = segment.checked_sub(1).is_none_or(|previous| {
        (upper_position - mesh.vertices[nodes[previous].vertex].truncate())
            .try_normalize()
            .is_some_and(|incoming| incoming.dot(direction) >= 0.0)
    });
    let follows_downstream = nodes.get(segment + 2).is_none_or(|next| {
        (mesh.vertices[next.vertex].truncate() - lower_position)
            .try_normalize()
            .is_some_and(|outgoing| outgoing.dot(direction) >= 0.0)
    });
    if !follows_upstream || !follows_downstream {
        return None;
    }

    let upper_section = cross_sections.get(segment).copied().unwrap_or_default();
    let lower_section = cross_sections.get(segment + 1).copied().unwrap_or_default();
    let half_width = upper_section
        .target_half_width
        .max(lower_section.target_half_width)
        .max(upper_section.achieved_width * 0.5)
        .max(lower_section.achieved_width * 0.5)
        .max(WATERFALL_TARGET_EDGE_LENGTH);
    Some(WaterfallPatch {
        river: 0,
        segment: segment as u32,
        upper_vertex: upper.vertex,
        upper_centre: upper_position,
        direction,
        across: Vec2::new(-direction.y, direction.x),
        upper_surface: drop,
        lower_surface: 0.0,
        lower_floor: 0.0,
        half_width,
        support_run: upper_position
            .distance(lower_position)
            .max(WATERFALL_TARGET_EDGE_LENGTH),
        pool: None,
    })
}

pub(super) fn complete_waterfall_face(
    mesh: &Mesh,
    patch: WaterfallPatch,
    environment: WaterfallSiteEnvironment<'_>,
) -> Option<Vec<bool>> {
    let banks = environment
        .coverage
        .iter()
        .enumerate()
        .map(|(vertex, &remaining)| {
            remaining != 0
                && (environment.perimeter.get(vertex).copied().unwrap_or(true)
                    || environment.adjacency[vertex]
                        .iter()
                        .any(|&neighbour| environment.coverage[neighbour] == 0))
        })
        .collect::<Vec<_>>();
    let mut seeds = mesh
        .vertices
        .iter()
        .zip(environment.coverage)
        .map(|(position, &remaining)| {
            remaining != 0 && patch.contains_face_point(position.truncate())
        })
        .collect::<Vec<_>>();
    seeds[patch.upper_vertex] = true;
    let eligible = mesh
        .vertices
        .iter()
        .zip(environment.coverage)
        .map(|(position, &remaining)| {
            remaining != 0 && patch.contains_face_flow_band(position.truncate())
        })
        .collect::<Vec<_>>();
    let face =
        expand_vertex_mask_through_river_to_banks(environment.adjacency, &seeds, &eligible, &banks);
    if face.iter().enumerate().any(|(vertex, &selected)| {
        selected
            && (environment.perimeter.get(vertex).copied().unwrap_or(true)
                || environment.ocean.get(vertex).copied().unwrap_or(true))
    }) {
        return None;
    }

    let (left_bank, right_bank) = face
        .iter()
        .zip(&banks)
        .enumerate()
        .filter(|(_, selected_bank)| *selected_bank.0 && *selected_bank.1)
        .map(|(vertex, _)| patch.local_coordinates(mesh.vertices[vertex].truncate()).1)
        .fold(
            (f32::INFINITY, f32::NEG_INFINITY),
            |(left, right), lateral| (left.min(lateral), right.max(lateral)),
        );
    let minimum_bank_span = (patch.half_width * WATERFALL_SITE_MINIMUM_BANK_SPAN_FRACTION)
        .min(local_edge_length(mesh, environment.adjacency, patch.upper_vertex) * 0.25);
    if left_bank > -minimum_bank_span || right_bank < minimum_bank_span {
        return None;
    }

    Some(face)
}

pub(super) fn waterfall_face_has_side_bypass(
    mesh: &Mesh,
    patch: WaterfallPatch,
    face: &[bool],
    environment: WaterfallSiteEnvironment<'_>,
) -> bool {
    // Search only the immediate waterfall neighbourhood and cap the path
    // length. This catches a short side route around the face without treating
    // a distant confluence or irregular global river topology as a bypass.
    let lateral_limit = patch.lateral_extent() * 1.5;
    let upstream_extent = patch.support_run + 2.0 * WATERFALL_TARGET_EDGE_LENGTH;
    let downstream_extent = patch
        .downstream_extent()
        .min(patch.support_run + patch.half_width);
    let mut local = vec![false; mesh.vertices.len()];
    let mut downstream = vec![false; mesh.vertices.len()];
    let mut distance = vec![u8::MAX; mesh.vertices.len()];
    let mut pending = VecDeque::new();

    for (vertex, (position, &remaining)) in
        mesh.vertices.iter().zip(environment.coverage).enumerate()
    {
        if remaining == 0 || face[vertex] {
            continue;
        }
        let (along, lateral) = patch.local_coordinates(position.truncate());
        if lateral.abs() > lateral_limit || along < -upstream_extent || along > downstream_extent {
            continue;
        }
        local[vertex] = true;
        if along < -f32::EPSILON {
            distance[vertex] = 0;
            pending.push_back(vertex);
        } else if along > f32::EPSILON {
            downstream[vertex] = true;
        }
    }

    while let Some(vertex) = pending.pop_front() {
        if downstream[vertex] {
            return true;
        }
        let next_distance = distance[vertex] + 1;
        if next_distance > WATERFALL_SITE_BYPASS_MAX_HOPS {
            continue;
        }
        for &neighbour in &environment.adjacency[vertex] {
            if local[neighbour] && next_distance < distance[neighbour] {
                distance[neighbour] = next_distance;
                pending.push_back(neighbour);
            }
        }
    }
    false
}

pub(super) fn relocate_conflicting_waterfalls(
    mesh: &Mesh,
    nodes: &mut [RiverNode],
    waterfalls: &mut [bool],
    end: usize,
    relocation: WaterfallRelocation<'_>,
    cross_sections: &[RiverCrossSection],
    scratch: &mut Vec<WaterfallDrop>,
) -> bool {
    let end = end.min(nodes.len().saturating_sub(1));
    scratch.clear();
    scratch.extend(
        waterfalls[..end]
            .iter()
            .enumerate()
            .filter(|(_, waterfall)| **waterfall)
            .map(|(segment, _)| WaterfallDrop {
                segment,
                height: (nodes[segment].surface - nodes[segment + 1].surface).max(0.0),
                placed: true,
            }),
    );
    if scratch.is_empty() {
        return true;
    }

    waterfalls[..end].fill(false);
    let mut upstream_limit = end.saturating_sub(1);
    let mut all_placed = true;
    for drop in scratch.iter_mut().rev() {
        let original = drop.segment.min(upstream_limit);
        let selected = (0..=original)
            .rev()
            .find(|&segment| {
                !relocation
                    .clearance
                    .conflicts(relocation.river, mesh, nodes, segment)
                    && relocation.site.is_none_or(|environment| {
                        valid_waterfall_site(
                            mesh,
                            nodes,
                            segment,
                            drop.height,
                            cross_sections,
                            environment,
                        )
                    })
            })
            // Intermediate erosion LODs retain the historical best-effort
            // behavior. Only the final LOD 0 pass may reject a whole river.
            .or_else(|| relocation.site.is_none().then_some(original));
        if let Some(segment) = selected {
            drop.segment = segment;
            drop.placed = true;
            waterfalls[segment] = true;
            upstream_limit = segment.saturating_sub(1);
        } else {
            drop.placed = false;
            all_placed = false;
        }
    }

    let mut level = nodes[end].surface;
    for segment in (0..end).rev() {
        for drop in scratch
            .iter()
            .filter(|drop| drop.placed && drop.segment == segment)
        {
            level += drop.height;
        }
        nodes[segment].surface = level.min(nodes[segment].surface);
    }
    all_placed
}

pub(super) fn river_ocean_entry(nodes: &[RiverNode], ocean: &[bool]) -> Option<usize> {
    nodes
        .iter()
        .position(|node| ocean.get(node.vertex).copied().unwrap_or(false))
}

pub(super) fn river_mouth_transition(
    ocean_entry: usize,
    waterfalls: &[bool],
) -> RiverMouthTransition {
    let waterfall_segment = waterfalls[..ocean_entry.min(waterfalls.len())]
        .iter()
        .rposition(|&waterfall| waterfall);
    RiverMouthTransition {
        waterfall_segment,
        river_mesh_end: waterfall_segment.map_or(0, |segment| segment + 1),
    }
}

pub(super) fn carve_submerged_river_mouth(
    terrain: &mut RiverTerrain<'_>,
    nodes: &mut [RiverNode],
    waterfalls: &mut [bool],
    mouth: RiverMouthTransition,
    max_height: f32,
    cross_sections: &[RiverCrossSection],
    budget: &mut RiverSedimentBudget,
) {
    let mouth_depth = (max_height * 0.0025).max(0.000_02);
    let first_submerged_surface = -mouth_depth.max(SEA_PLANE_CLEARANCE * 2.0);
    let submerged_nodes = nodes.len() - mouth.river_mesh_end;
    let span = submerged_nodes.saturating_sub(1).max(1) as f32;
    if let Some(waterfall_segment) = mouth.waterfall_segment {
        waterfalls[waterfall_segment] = true;
    }
    waterfalls[mouth.river_mesh_end..].fill(false);

    for (offset, node) in nodes[mouth.river_mesh_end..].iter_mut().enumerate() {
        let progress = offset as f32 / span;
        node.surface = first_submerged_surface - mouth_depth * 0.5 * progress;
        let node_index = mouth.river_mesh_end + offset;
        let channel_depth = cross_sections
            .get(node_index)
            .map_or(mouth_depth, |section| {
                section.required_depth.max(mouth_depth)
            });
        let target_bed = node.surface - channel_depth;
        let depth = (terrain.mesh.vertices[node.vertex].z - target_bed).max(0.0);
        terrain.lower_vertex_exactly(node.vertex, depth, budget);
    }
}

#[derive(Clone, Copy, Debug)]
pub(super) struct DeltaCandidate {
    pub(super) priority: f32,
    pub(super) distance: f32,
    pub(super) vertex: usize,
}

impl PartialEq for DeltaCandidate {
    fn eq(&self, other: &Self) -> bool {
        self.vertex == other.vertex && self.priority.to_bits() == other.priority.to_bits()
    }
}

impl Eq for DeltaCandidate {}

impl PartialOrd for DeltaCandidate {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for DeltaCandidate {
    fn cmp(&self, other: &Self) -> Ordering {
        self.priority
            .total_cmp(&other.priority)
            .then_with(|| other.vertex.cmp(&self.vertex))
    }
}

#[derive(Debug)]
pub(super) struct DeltaScratch {
    pub(super) visited: Vec<u32>,
    pub(super) channel: Vec<u32>,
    pub(super) stamp: u32,
    pub(super) frontier: BinaryHeap<DeltaCandidate>,
}

impl DeltaScratch {
    pub(super) fn new(vertex_count: usize) -> Self {
        Self {
            visited: vec![0; vertex_count],
            channel: vec![0; vertex_count],
            stamp: 0,
            frontier: BinaryHeap::new(),
        }
    }

    pub(super) fn begin(&mut self, nodes: &[RiverNode]) {
        self.frontier.clear();
        self.stamp = self.stamp.wrapping_add(1);
        if self.stamp == 0 {
            self.visited.fill(0);
            self.channel.fill(0);
            self.stamp = 1;
        }
        for node in nodes {
            self.channel[node.vertex] = self.stamp;
        }
    }

    pub(super) fn visit(&mut self, vertex: usize) -> bool {
        if self.visited[vertex] == self.stamp {
            return false;
        }
        self.visited[vertex] = self.stamp;
        true
    }

    pub(super) fn is_channel(&self, vertex: usize) -> bool {
        self.channel[vertex] == self.stamp
    }
}

pub(super) fn create_delta(
    terrain: &mut RiverTerrain<'_>,
    nodes: &[RiverNode],
    budget: &mut RiverSedimentBudget,
    max_height: f32,
    edge_length: f32,
    scratch: &mut DeltaScratch,
) {
    let Some(outlet) = nodes.last().map(|node| node.vertex) else {
        return;
    };
    if terrain.mesh.vertices[outlet].z > 0.0 {
        return;
    }

    let valley_budget = budget.carried * 0.7;
    let mut valley_sediment = valley_budget;
    create_alluvial_valley(
        terrain,
        nodes,
        &mut valley_sediment,
        max_height,
        edge_length,
        scratch,
    );
    let valley_deposited = valley_budget - valley_sediment;
    budget.carried -= valley_deposited;
    budget.deposited += valley_deposited;

    let outlet_position = terrain.mesh.vertices[outlet].truncate();
    let approach = nodes
        .get(nodes.len().saturating_sub(8))
        .map_or(outlet_position, |node| {
            terrain.mesh.vertices[node.vertex].truncate()
        });
    let downstream = (outlet_position - approach).normalize_or_zero();
    let radius = edge_length * 18.0;
    let mean_area =
        terrain.control_areas.iter().sum::<f32>() / terrain.control_areas.len().max(1) as f32;
    let allowance = (budget.carried / (96.0 * f64::from(mean_area.max(f32::EPSILON))))
        .max(f64::from(max_height * 0.000_01)) as f32;
    let mouth_height = max_height * 0.0035;
    scratch.begin(nodes);
    scratch.visit(outlet);
    scratch.frontier.push(DeltaCandidate {
        priority: mouth_height,
        distance: 0.0,
        vertex: outlet,
    });

    for _ in 0..384 {
        let Some(candidate) = scratch.frontier.pop() else {
            break;
        };
        let normalized_distance = (candidate.distance / radius).clamp(0.0, 1.0);
        let target_height = mouth_height * (1.0 - normalized_distance)
            - max_height * normalized_distance * normalized_distance * 0.035;
        if !scratch.is_channel(candidate.vertex)
            && terrain.mesh.vertices[candidate.vertex].z < target_height
        {
            let requested =
                (target_height - terrain.mesh.vertices[candidate.vertex].z).min(allowance);
            let deposited =
                terrain.deposit_vertex(candidate.vertex, requested, &mut budget.carried);
            budget.deposited += deposited;
        }
        if budget.carried <= f64::EPSILON {
            break;
        }

        for &neighbour in &terrain.adjacency[candidate.vertex] {
            if !scratch.visit(neighbour) {
                continue;
            }
            let offset = terrain.mesh.vertices[neighbour].truncate() - outlet_position;
            let distance = offset.length();
            if distance > radius {
                continue;
            }
            let alignment = if distance <= f32::EPSILON {
                1.0
            } else {
                offset.dot(downstream) / distance
            };
            if alignment < -0.2 {
                continue;
            }
            let priority = terrain.mesh.vertices[neighbour].z - distance * 0.04
                + alignment.max(0.0) * edge_length;
            scratch.frontier.push(DeltaCandidate {
                priority,
                distance,
                vertex: neighbour,
            });
        }
    }
}

pub(super) fn create_alluvial_valley(
    terrain: &mut RiverTerrain<'_>,
    nodes: &[RiverNode],
    sediment: &mut f64,
    max_height: f32,
    edge_length: f32,
    scratch: &mut DeltaScratch,
) {
    const VALLEY_REACHES: usize = 14;

    let start = nodes.len().saturating_sub(VALLEY_REACHES);
    let valley_width = edge_length * 7.0;
    let bank_freeboard = max_height * 0.0025;
    let lateral_relief = max_height * 0.012;
    let mean_area =
        terrain.control_areas.iter().sum::<f32>() / terrain.control_areas.len().max(1) as f32;
    let allowance = (*sediment / (128.0 * f64::from(mean_area.max(f32::EPSILON))))
        .max(f64::from(max_height * 0.000_01)) as f32;
    scratch.begin(nodes);
    for node in &nodes[start..] {
        if scratch.visit(node.vertex) {
            let target_height = node.surface.max(0.0) + bank_freeboard;
            scratch.frontier.push(DeltaCandidate {
                priority: target_height,
                distance: 0.0,
                vertex: node.vertex,
            });
        }
    }

    for _ in 0..768 {
        let Some(candidate) = scratch.frontier.pop() else {
            break;
        };
        let target_height = candidate.priority;
        if !scratch.is_channel(candidate.vertex)
            && terrain.mesh.vertices[candidate.vertex].z < target_height
        {
            let requested =
                (target_height - terrain.mesh.vertices[candidate.vertex].z).min(allowance);
            terrain.deposit_vertex(candidate.vertex, requested, sediment);
        }
        if *sediment <= f64::EPSILON {
            break;
        }

        for &neighbour in &terrain.adjacency[candidate.vertex] {
            if !scratch.visit(neighbour) {
                continue;
            }
            let step = (terrain.mesh.vertices[candidate.vertex].truncate()
                - terrain.mesh.vertices[neighbour].truncate())
            .length();
            let distance = candidate.distance + step;
            if distance > valley_width {
                continue;
            }
            let previous_distance = candidate.distance / valley_width;
            let normalized_distance = distance / valley_width;
            let additional_relief = lateral_relief
                * (normalized_distance * normalized_distance
                    - previous_distance * previous_distance);
            let priority = candidate.priority - additional_relief;
            scratch.frontier.push(DeltaCandidate {
                priority,
                distance,
                vertex: neighbour,
            });
        }
    }
}

pub(super) fn average_edge_length(mesh: &Mesh, adjacency: &Adjacency) -> f32 {
    let mut total = 0.0_f32;
    let mut count = 0_u32;
    for (vertex, neighbours) in adjacency.iter().enumerate() {
        for &neighbour in neighbours.iter().filter(|&&neighbour| neighbour > vertex) {
            total +=
                (mesh.vertices[vertex].truncate() - mesh.vertices[neighbour].truncate()).length();
            count += 1;
        }
    }
    total / count.max(1) as f32
}

pub(super) fn local_edge_length(mesh: &Mesh, adjacency: &Adjacency, vertex: usize) -> f32 {
    let neighbours = &adjacency[vertex];
    let total = neighbours
        .iter()
        .map(|&neighbour| {
            (mesh.vertices[vertex].truncate() - mesh.vertices[neighbour].truncate()).length()
        })
        .sum::<f32>();
    total / neighbours.len().max(1) as f32
}
