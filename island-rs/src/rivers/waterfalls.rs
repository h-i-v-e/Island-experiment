use super::{
    Adjacency, ENABLE_WATERFALL_PLUNGE_POOLS, HashMap, ISLAND_WORLD_METRES, Mesh, RIVER_BOUNDARY,
    RIVER_SURFACE_OFFSET, RiverMeshBuffers, RiverNetwork, RiverOwnerKey, SEA_PLANE_CLEARANCE,
    SurfaceMaterial, Vec2, Vec3, VecDeque, WATERFALL_APRON_WIDTH_MULTIPLIER,
    WATERFALL_DOWNSTREAM_SPIKE_ALLOWANCE, WATERFALL_DOWNSTREAM_SPIKE_PASSES,
    WATERFALL_EDGE_BLEND_RUN, WATERFALL_EDGE_SMOOTHING, WATERFALL_EDGE_SMOOTHING_PASSES,
    WATERFALL_FINAL_BANK_DROP_FRACTION, WATERFALL_FINAL_BANK_EDGE_DROP_FRACTION,
    WATERFALL_LANDING_LENGTH_MULTIPLIER, WATERFALL_POOL_MAXIMUM_DEPTH, WATERFALL_POOL_MINIMUM_DROP,
    WATERFALL_REFINEMENT_PASSES, WATERFALL_TARGET_EDGE_LENGTH, WATERFALL_WATER_CLEARANCE,
    mark_river_boundary,
};

#[derive(Clone, Copy, Debug)]
pub(super) struct PlungePool {
    pub(super) centre: Vec2,
    pub(super) downstream_radius: f32,
    pub(super) lateral_radius: f32,
    pub(super) depth: f32,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct WaterfallPatch {
    pub(super) river: u32,
    pub(super) segment: u32,
    pub(super) upper_vertex: usize,
    pub(super) upper_centre: Vec2,
    pub(super) direction: Vec2,
    pub(super) across: Vec2,
    pub(super) upper_surface: f32,
    pub(super) lower_surface: f32,
    pub(super) lower_floor: f32,
    pub(super) half_width: f32,
    pub(super) support_run: f32,
    pub(super) pool: Option<PlungePool>,
}

#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub(crate) struct WaterfallFoot {
    pub(crate) position: Vec3,
    pub(crate) direction: Vec3,
    pub(crate) half_width: f32,
    pub(crate) drop: f32,
}

#[derive(Debug)]
pub(super) struct WaterfallTerrainConstraints {
    pub(super) patch: Vec<bool>,
    pub(super) pinned: Vec<bool>,
    pub(super) support: Vec<bool>,
    pub(super) water_unclamped: Vec<bool>,
    pub(super) terrain_ceiling: Vec<f32>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum WaterfallPlaneZone {
    BeforeLip,
    Face,
    AfterFoot,
}

impl WaterfallPatch {
    pub(super) fn face_run() -> f32 {
        2.0 * WATERFALL_TARGET_EDGE_LENGTH
    }

    pub(super) fn foot(self) -> WaterfallFoot {
        WaterfallFoot {
            position: self.upper_centre.extend(self.lower_surface),
            direction: self.direction.extend(0.0),
            half_width: self.half_width,
            drop: self.upper_surface - self.lower_surface,
        }
    }

    pub(super) fn signed_distance_to_face_plane(self, point: Vec2) -> f32 {
        (point - self.upper_centre).dot(self.direction)
    }

    pub(super) fn plane_zone(self, point: Vec2) -> WaterfallPlaneZone {
        let along = self.signed_distance_to_face_plane(point);
        if along < -Self::face_run() - f32::EPSILON {
            WaterfallPlaneZone::BeforeLip
        } else if along > f32::EPSILON {
            WaterfallPlaneZone::AfterFoot
        } else {
            WaterfallPlaneZone::Face
        }
    }

    pub(super) fn edge_normal_blend(self, point: Vec2) -> f32 {
        let along = self.signed_distance_to_face_plane(point);
        match self.plane_zone(point) {
            WaterfallPlaneZone::BeforeLip => (-Self::face_run() - along) / WATERFALL_EDGE_BLEND_RUN,
            WaterfallPlaneZone::Face => 0.0,
            WaterfallPlaneZone::AfterFoot => 1.0,
        }
        .clamp(0.0, 1.0)
    }

    /// Returns the complementary smoothing influence along the upstream
    /// pin/lift transition: one at the lip plane and zero at its outer plane.
    pub(super) fn upstream_pin_smoothing_weight(self, point: Vec2) -> f32 {
        let along = self.signed_distance_to_face_plane(point);
        let upstream_distance = -Self::face_run() - along;
        if upstream_distance < -f32::EPSILON {
            return 0.0;
        }
        if upstream_distance <= f32::EPSILON {
            return 1.0;
        }
        if upstream_distance >= WATERFALL_EDGE_BLEND_RUN - f32::EPSILON {
            return 0.0;
        }
        (1.0 - upstream_distance / WATERFALL_EDGE_BLEND_RUN).clamp(0.0, 1.0)
    }

    pub(super) fn contains_upstream_pin_blend(self, point: Vec2) -> bool {
        let upstream_distance = -Self::face_run() - self.signed_distance_to_face_plane(point);
        (-f32::EPSILON..=WATERFALL_EDGE_BLEND_RUN + f32::EPSILON).contains(&upstream_distance)
    }

    pub(super) fn local_coordinates(self, point: Vec2) -> (f32, f32) {
        let offset = point - self.upper_centre;
        (
            self.signed_distance_to_face_plane(point),
            offset.dot(self.across),
        )
    }

    /// Limits geometric waterfall bands to the part of this river that owns
    /// the lip. A remote bend can cross the same world-space band without
    /// becoming part of the waterfall face.
    pub(super) fn owns_local_channel(self, owner: RiverOwnerKey) -> bool {
        owner.river == self.river
            && owner.node.saturating_add(1) >= self.segment
            && owner.node <= self.segment.saturating_add(1)
    }

    pub(super) fn downstream_extent(self) -> f32 {
        let landing = self.support_run + self.half_width * WATERFALL_LANDING_LENGTH_MULTIPLIER;
        self.pool.map_or(landing, |pool| {
            landing
                .max((pool.centre - self.upper_centre).dot(self.direction) + pool.downstream_radius)
        })
    }

    pub(super) fn lateral_extent(self) -> f32 {
        self.pool.map_or(self.half_width * 1.25, |pool| {
            (self.half_width * 1.25).max(pool.lateral_radius)
        })
    }

    pub(super) fn contains_refinement_point(self, point: Vec2) -> bool {
        let (along, lateral) = self.local_coordinates(point);
        along >= -2.0 * WATERFALL_TARGET_EDGE_LENGTH
            && along <= self.downstream_extent()
            && lateral.abs() <= self.lateral_extent()
    }

    pub(super) fn contains_face_point(self, point: Vec2) -> bool {
        let (along, lateral) = self.local_coordinates(point);
        (-Self::face_run()..=f32::EPSILON).contains(&along)
            && lateral.abs() <= self.half_width * 1.25
    }

    pub(super) fn contains_face_smoothing_band(self, point: Vec2) -> bool {
        let (along, _) = self.local_coordinates(point);
        (-3.0 * WATERFALL_TARGET_EDGE_LENGTH..=f32::EPSILON).contains(&along)
    }

    pub(super) fn contains_face_flow_band(self, point: Vec2) -> bool {
        let (along, _) = self.local_coordinates(point);
        (-3.0 * WATERFALL_TARGET_EDGE_LENGTH..=WATERFALL_TARGET_EDGE_LENGTH).contains(&along)
    }

    pub(super) fn contains_side_constraint_band(self, point: Vec2) -> bool {
        let (along, _) = self.local_coordinates(point);
        (-3.0 * WATERFALL_TARGET_EDGE_LENGTH..=self.downstream_extent()).contains(&along)
    }

    pub(super) fn contains_downstream_point(self, point: Vec2) -> bool {
        let (along, lateral) = self.local_coordinates(point);
        along > f32::EPSILON
            && along <= self.downstream_extent()
            && lateral.abs() <= self.lateral_extent()
    }

    pub(super) fn pool_depth_at(self, point: Vec2) -> f32 {
        let Some(pool) = self.pool else {
            return 0.0;
        };
        let offset = point - pool.centre;
        let downstream = offset.dot(self.direction) / pool.downstream_radius;
        let lateral = offset.dot(self.across) / pool.lateral_radius;
        let radius_squared = downstream.mul_add(downstream, lateral * lateral);
        if radius_squared >= 1.0 {
            return 0.0;
        }
        let influence = 1.0 - radius_squared;
        pool.depth * influence * influence
    }

    /// Returns the waterfall-face water height on the upstream side of the
    /// flow-perpendicular plane through `upper_centre`. The profile varies only
    /// along the flow direction, so equal-along vertices form a flat cross-face
    /// even when the underlying triangles are irregular.
    pub(super) fn face_surface_at(self, point: Vec2) -> Option<f32> {
        let (along, _) = self.local_coordinates(point);
        if along > f32::EPSILON {
            return None;
        }
        let face_run = Self::face_run();
        let progress = smoothstep((along + face_run) / face_run);
        Some((self.lower_surface - self.upper_surface).mul_add(progress, self.upper_surface))
    }
}

pub(super) struct WaterfallPatchIndex<'a> {
    pub(super) patches: &'a [WaterfallPatch],
    pub(super) cells: HashMap<(i32, i32), Vec<usize>>,
}

impl<'a> WaterfallPatchIndex<'a> {
    const CELL_SIZE: f32 = 16.0 / ISLAND_WORLD_METRES;

    pub(super) fn new(patches: &'a [WaterfallPatch]) -> Self {
        let mut cells = HashMap::<(i32, i32), Vec<usize>>::new();
        for (index, patch) in patches.iter().enumerate() {
            let radius = patch
                .downstream_extent()
                .max(patch.half_width * WATERFALL_APRON_WIDTH_MULTIPLIER)
                + patch.half_width;
            let minimum = Self::cell(patch.upper_centre - Vec2::splat(radius));
            let maximum = Self::cell(patch.upper_centre + Vec2::splat(radius));
            for y in minimum.1..=maximum.1 {
                for x in minimum.0..=maximum.0 {
                    cells.entry((x, y)).or_default().push(index);
                }
            }
        }
        Self { patches, cells }
    }

    pub(super) fn candidates(&self, point: Vec2) -> impl Iterator<Item = &'a WaterfallPatch> + '_ {
        self.cells
            .get(&Self::cell(point))
            .into_iter()
            .flatten()
            .map(|&index| &self.patches[index])
    }

    pub(super) fn cell(point: Vec2) -> (i32, i32) {
        (
            (point.x / Self::CELL_SIZE).floor() as i32,
            (point.y / Self::CELL_SIZE).floor() as i32,
        )
    }
}

pub(super) fn derive_waterfall_patches(
    network: &RiverNetwork,
    terrain: &Mesh,
) -> Vec<WaterfallPatch> {
    let mut patches = Vec::<WaterfallPatch>::new();
    for (river_index, river) in network.rivers.iter().enumerate() {
        let visible_end = network.river_mesh_ends[river_index]
            .unwrap_or_else(|| river.nodes.len().saturating_sub(1));
        for (segment, &is_waterfall) in network.waterfalls[river_index].iter().enumerate() {
            if !is_waterfall || segment + 1 >= river.nodes.len() || segment + 1 > visible_end {
                continue;
            }
            let upper = river.nodes[segment];
            let lower = river.nodes[segment + 1];
            let upper_centre = terrain.vertices[upper.vertex].truncate();
            let lower_centre = terrain.vertices[lower.vertex].truncate();
            let separation = upper_centre.distance(lower_centre);
            let Some(direction) = (lower_centre - upper_centre).try_normalize() else {
                continue;
            };
            let upper_section = network
                .cross_sections
                .get(river_index)
                .and_then(|sections| sections.get(segment))
                .copied()
                .unwrap_or_default();
            let lower_section = network
                .cross_sections
                .get(river_index)
                .and_then(|sections| sections.get(segment + 1))
                .copied()
                .unwrap_or_default();
            let half_width = upper_section
                .target_half_width
                .max(lower_section.target_half_width)
                .max(upper_section.achieved_width * 0.5)
                .max(lower_section.achieved_width * 0.5)
                .max(WATERFALL_TARGET_EDGE_LENGTH);
            let lower_depth = lower_section.required_depth.max(RIVER_SURFACE_OFFSET);
            let drop = upper.surface - lower.surface;
            if drop <= RIVER_SURFACE_OFFSET {
                continue;
            }
            let support_run = separation.max(WATERFALL_TARGET_EDGE_LENGTH);
            let across = Vec2::new(-direction.y, direction.x);
            let pool_is_safe = ENABLE_WATERFALL_PLUNGE_POOLS
                && drop >= WATERFALL_POOL_MINIMUM_DROP
                && lower.surface > SEA_PLANE_CLEARANCE
                && !network.perimeter.get(lower.vertex).copied().unwrap_or(true)
                && !network.ocean.get(lower.vertex).copied().unwrap_or(true);
            let pool = pool_is_safe.then(|| {
                let downstream_radius =
                    (half_width * 1.5).clamp(2.0 / ISLAND_WORLD_METRES, 12.0 / ISLAND_WORLD_METRES);
                PlungePool {
                    centre: upper_centre,
                    downstream_radius,
                    lateral_radius: (half_width * 1.1).max(1.5 / ISLAND_WORLD_METRES),
                    depth: (drop * 0.15).min(WATERFALL_POOL_MAXIMUM_DEPTH),
                }
            });
            let pool = pool.filter(|candidate| {
                patches.iter().all(|other| {
                    other.pool.is_none_or(|existing| {
                        existing.centre.distance(candidate.centre)
                            > existing.downstream_radius + candidate.downstream_radius
                    })
                })
            });
            patches.push(WaterfallPatch {
                river: river_index as u32,
                segment: segment as u32,
                upper_vertex: upper.vertex,
                upper_centre,
                direction,
                across,
                upper_surface: upper.surface,
                lower_surface: lower.surface,
                lower_floor: lower.surface - lower_depth,
                half_width,
                support_run,
                pool,
            });
        }
    }
    patches
}

impl RiverMeshBuffers {
    pub(super) fn refine_waterfalls(
        &mut self,
        terrain: &mut Mesh,
        material: &mut SurfaceMaterial,
        patches: &[WaterfallPatch],
    ) -> usize {
        refine_waterfall_terrain(terrain, material, patches, self)
    }

    pub(super) fn tessellate_final_waterfall_faces(
        &mut self,
        terrain: &mut Mesh,
        material: &mut SurfaceMaterial,
        patches: &[WaterfallPatch],
    ) -> usize {
        tessellate_final_waterfall_faces(terrain, material, patches, self)
    }
}

fn refine_waterfall_terrain(
    terrain: &mut Mesh,
    material: &mut SurfaceMaterial,
    patches: &[WaterfallPatch],
    buffers: &mut RiverMeshBuffers,
) -> usize {
    if patches.is_empty() {
        return 0;
    }
    let patch_index = WaterfallPatchIndex::new(patches);
    let mut added = 0;
    for _ in 0..WATERFALL_REFINEMENT_PASSES {
        let mut marked = vec![false; terrain.vertices.len()];
        for triangle in terrain.triangles.chunks_exact(3) {
            let indices = [
                triangle[0] as usize,
                triangle[1] as usize,
                triangle[2] as usize,
            ];
            if !indices.iter().any(|&vertex| {
                patch_index
                    .candidates(terrain.vertices[vertex].truncate())
                    .any(|patch| {
                        patch.contains_refinement_point(terrain.vertices[vertex].truncate())
                    })
            }) {
                continue;
            }
            let [a, b, c] = indices.map(|vertex| terrain.vertices[vertex].truncate());
            if a.distance(b).max(b.distance(c)).max(c.distance(a)) > WATERFALL_TARGET_EDGE_LENGTH {
                for vertex in indices {
                    marked[vertex] = true;
                }
            }
        }
        if !marked.iter().any(|&selected| selected) {
            break;
        }
        let loose_volume = material.volume(terrain);
        let stencils = terrain.tessellate_incident_to(&marked);
        material.extend_after_tessellation(loose_volume, terrain, &stencils);
        buffers.extend_after_tessellation(&stencils);
        added += stencils.len();
    }
    added += buffers.tessellate_final_waterfall_faces(terrain, material, patches);
    for value in &mut buffers.coverage {
        *value &= !RIVER_BOUNDARY;
    }
    mark_river_boundary(
        &terrain.adjacency(),
        &terrain.perimeter_mask(),
        &mut buffers.coverage,
    );
    added
}

/// Adds one unconditional detail tier to each final waterfall face, expanding
/// laterally through as many river rings as necessary to reach both banks and
/// one topology ring beyond them. The flow-aligned band prevents that
/// traversal from following the river away from the fall; triangles outside
/// the bank apron are only conformingly stitched.
fn tessellate_final_waterfall_faces(
    terrain: &mut Mesh,
    material: &mut SurfaceMaterial,
    patches: &[WaterfallPatch],
    buffers: &mut RiverMeshBuffers,
) -> usize {
    let adjacency = terrain.adjacency();
    let perimeter = terrain.perimeter_mask();
    let banks = buffers
        .coverage
        .iter()
        .enumerate()
        .map(|(vertex, &remaining)| {
            remaining != 0
                && (perimeter[vertex]
                    || adjacency[vertex]
                        .iter()
                        .any(|&neighbour| buffers.coverage[neighbour] == 0))
        })
        .collect::<Vec<_>>();
    let mut marked = vec![false; terrain.vertices.len()];
    for patch in patches {
        let face = terrain
            .vertices
            .iter()
            .zip(&buffers.coverage)
            .zip(&buffers.owners)
            .map(|((position, &remaining), &owner)| {
                remaining != 0
                    && owner.is_some_and(|owner| patch.owns_local_channel(owner))
                    && patch.contains_face_point(position.truncate())
            })
            .collect::<Vec<_>>();
        let flow_band = terrain
            .vertices
            .iter()
            .map(|position| patch.contains_face_flow_band(position.truncate()))
            .collect::<Vec<_>>();
        let eligible = buffers
            .coverage
            .iter()
            .zip(&flow_band)
            .zip(&buffers.owners)
            .map(|((&remaining, &inside_band), &owner)| {
                remaining != 0
                    && inside_band
                    && owner.is_some_and(|owner| patch.owns_local_channel(owner))
            })
            .collect::<Vec<_>>();
        let apron_eligible = flow_band
            .iter()
            .zip(&buffers.owners)
            .map(|(&inside_band, &owner)| {
                inside_band && owner.is_none_or(|owner| patch.owns_local_channel(owner))
            })
            .collect::<Vec<_>>();
        let face_to_bank_apron =
            expand_vertex_mask_to_banks(&adjacency, &face, &eligible, &apron_eligible, &banks);
        marked
            .iter_mut()
            .zip(face_to_bank_apron)
            .for_each(|(selected, candidate)| *selected |= candidate);
    }
    if !marked.iter().any(|&selected| selected) {
        return 0;
    }

    let loose_volume = material.volume(terrain);
    let stencils = terrain.tessellate_incident_to(&marked);
    material.extend_after_tessellation(loose_volume, terrain, &stencils);
    buffers.extend_after_tessellation(&stencils);
    stencils.len()
}

pub(super) fn expand_vertex_mask_to_banks(
    adjacency: &Adjacency,
    seeds: &[bool],
    eligible: &[bool],
    apron_eligible: &[bool],
    banks: &[bool],
) -> Vec<bool> {
    debug_assert_eq!(adjacency.len(), seeds.len());
    debug_assert_eq!(adjacency.len(), eligible.len());
    debug_assert_eq!(adjacency.len(), apron_eligible.len());
    debug_assert_eq!(adjacency.len(), banks.len());
    let mut expanded = expand_vertex_mask_through_river_to_banks(adjacency, seeds, eligible, banks);
    let reached_banks = expanded
        .iter()
        .zip(banks)
        .map(|(&selected, &is_bank)| selected && is_bank)
        .collect::<Vec<_>>();
    for (bank, &reached) in reached_banks.iter().enumerate() {
        if !reached {
            continue;
        }
        for &neighbour in &adjacency[bank] {
            if apron_eligible[neighbour] {
                expanded[neighbour] = true;
            }
        }
    }
    expanded
}

pub(super) fn expand_vertex_mask_through_river_to_banks(
    adjacency: &Adjacency,
    seeds: &[bool],
    eligible: &[bool],
    banks: &[bool],
) -> Vec<bool> {
    debug_assert_eq!(adjacency.len(), seeds.len());
    debug_assert_eq!(adjacency.len(), eligible.len());
    debug_assert_eq!(adjacency.len(), banks.len());
    let mut expanded = seeds.to_vec();
    let mut frontier = seeds
        .iter()
        .enumerate()
        .filter_map(|(vertex, &selected)| (selected && eligible[vertex]).then_some(vertex))
        .collect::<VecDeque<_>>();
    while let Some(vertex) = frontier.pop_front() {
        if banks[vertex] {
            continue;
        }
        for &neighbour in &adjacency[vertex] {
            if eligible[neighbour] && !expanded[neighbour] {
                expanded[neighbour] = true;
                frontier.push_back(neighbour);
            }
        }
    }
    expanded
}

pub(super) fn smoothstep(progress: f32) -> f32 {
    let progress = progress.clamp(0.0, 1.0);
    progress * progress * (3.0 - 2.0 * progress)
}

/// Recesses a complete bank-to-bank cross-section around the flow-perpendicular
/// waterfall plane. The cut follows covered river topology rather than a fixed
/// lateral radius, so refined or unusually broad channels cannot bypass one
/// side of the fall. Downstream vertices return to ordinary river smoothing.
pub(super) fn recess_waterfall_notches(
    terrain: &mut Mesh,
    material: &mut SurfaceMaterial,
    patches: &[WaterfallPatch],
    coverage: &[u8],
    river_owners: &[Option<RiverOwnerKey>],
) -> Vec<Option<usize>> {
    debug_assert_eq!(terrain.vertices.len(), coverage.len());
    debug_assert_eq!(terrain.vertices.len(), river_owners.len());
    let adjacency = terrain.adjacency();
    let perimeter = terrain.perimeter_mask();
    let banks = coverage
        .iter()
        .enumerate()
        .map(|(vertex, &remaining)| {
            remaining != 0
                && (perimeter[vertex]
                    || adjacency[vertex]
                        .iter()
                        .any(|&neighbour| coverage[neighbour] == 0))
        })
        .collect::<Vec<_>>();
    let mut notch_owners = vec![None::<usize>; terrain.vertices.len()];
    let mut targets = terrain
        .vertices
        .iter()
        .map(|vertex| vertex.z)
        .collect::<Vec<_>>();

    for (patch_index, patch) in patches.iter().enumerate() {
        if patch.upper_vertex >= terrain.vertices.len() || coverage[patch.upper_vertex] == 0 {
            continue;
        }
        let mut seeds = terrain
            .vertices
            .iter()
            .zip(coverage)
            .zip(river_owners)
            .map(|((position, &remaining), &owner)| {
                remaining != 0
                    && owner.is_some_and(|owner| patch.owns_local_channel(owner))
                    && patch.contains_face_point(position.truncate())
            })
            .collect::<Vec<_>>();
        seeds[patch.upper_vertex] = true;
        let eligible = terrain
            .vertices
            .iter()
            .zip(coverage)
            .zip(river_owners)
            .map(|((position, &remaining), &owner)| {
                remaining != 0
                    && owner.is_some_and(|owner| patch.owns_local_channel(owner))
                    && patch.contains_face_flow_band(position.truncate())
            })
            .collect::<Vec<_>>();
        let face = expand_vertex_mask_through_river_to_banks(&adjacency, &seeds, &eligible, &banks);
        for (vertex, &selected) in face.iter().enumerate() {
            if !selected {
                continue;
            }
            let point = terrain.vertices[vertex].truncate();
            let target = if let Some(face_surface) = patch.face_surface_at(point) {
                face_surface - WATERFALL_WATER_CLEARANCE
            } else {
                patch.lower_floor
            };
            if notch_owners[vertex].is_none() || target < targets[vertex] {
                notch_owners[vertex] = Some(patch_index);
                targets[vertex] = targets[vertex].min(target);
            }
        }
    }

    for (patch_index, patch) in patches.iter().enumerate() {
        if patch.pool.is_none() {
            continue;
        }
        for (vertex, position) in terrain.vertices.iter().enumerate() {
            let depth = patch.pool_depth_at(position.truncate());
            if depth <= 0.0 {
                continue;
            }
            let target = patch.lower_floor - depth;
            if notch_owners[vertex].is_none() || target < targets[vertex] {
                notch_owners[vertex] = Some(patch_index);
                targets[vertex] = targets[vertex].min(target);
            }
        }
    }

    for (vertex, owner) in notch_owners.iter().enumerate() {
        if owner.is_none() {
            continue;
        }
        terrain.vertices[vertex].z = targets[vertex];
        material.depths_mut()[vertex] = 0.0;
    }

    notch_owners
}

/// Applies the final unconstrained relaxation to each tessellated waterfall,
/// through the river banks and their first dry-side ring. Terrain XYZ moves
/// toward the complete one-ring average, then covered water is derived from
/// the adjusted terrain with the normal waterfall clearance. Vertices outside
/// the patch anchor the blend. This intentionally runs after every carve, pin,
/// bank lift, and normal river relaxation stage.
pub(super) fn smooth_final_waterfall_patches(
    terrain: &mut Mesh,
    surfaces: &mut [f32],
    patches: &[WaterfallPatch],
    coverage: &[u8],
    owners: &[Option<RiverOwnerKey>],
) -> usize {
    debug_assert_eq!(terrain.vertices.len(), surfaces.len());
    debug_assert_eq!(terrain.vertices.len(), coverage.len());
    debug_assert_eq!(terrain.vertices.len(), owners.len());
    if patches.is_empty() {
        return 0;
    }

    let adjacency = terrain.adjacency();
    let perimeter = terrain.perimeter_mask();
    let selected =
        waterfall_face_bank_apron_mask(terrain, patches, coverage, owners, &adjacency, &perimeter);
    let mut snapshot = terrain.vertices.clone();
    let mut moved = vec![false; terrain.vertices.len()];
    for _ in 0..WATERFALL_EDGE_SMOOTHING_PASSES {
        for vertex in 0..terrain.vertices.len() {
            if !selected[vertex] || perimeter[vertex] || adjacency[vertex].is_empty() {
                continue;
            }
            let (position_total, count) = adjacency[vertex].iter().fold(
                (snapshot[vertex], 1_u32),
                |(position_total, count), &neighbour| {
                    (position_total + snapshot[neighbour], count + 1)
                },
            );
            let inverse_count = 1.0 / count as f32;
            let average = position_total * inverse_count;
            let target = snapshot[vertex].lerp(average, WATERFALL_EDGE_SMOOTHING);
            if target.distance_squared(terrain.vertices[vertex]) > f32::EPSILON {
                terrain.vertices[vertex] = target;
                moved[vertex] = true;
            }
        }
        snapshot.copy_from_slice(&terrain.vertices);
    }

    let banks = waterfall_bank_mask(&adjacency, &perimeter, coverage);
    let zones = classify_waterfall_vertices(terrain, patches, coverage, owners, &adjacency, &banks);
    for (vertex, (&remaining, zone)) in coverage.iter().zip(zones).enumerate() {
        if remaining != 0
            && zone.is_some_and(|classification| classification.zone == WaterfallPlaneZone::Face)
        {
            surfaces[vertex] = terrain.vertices[vertex].z + WATERFALL_WATER_CLEARANCE;
        }
    }

    if !terrain.uv.is_empty() {
        for (vertex, &was_moved) in moved.iter().enumerate() {
            if was_moved {
                terrain.uv[vertex] = terrain.vertices[vertex].truncate();
            }
        }
    }
    moved.into_iter().filter(|&was_moved| was_moved).count()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct WaterfallVertexPlaneZone {
    pub(super) patch: usize,
    pub(super) zone: WaterfallPlaneZone,
}

pub(super) fn classify_waterfall_vertices(
    terrain: &Mesh,
    patches: &[WaterfallPatch],
    coverage: &[u8],
    owners: &[Option<RiverOwnerKey>],
    adjacency: &Adjacency,
    banks: &[bool],
) -> Vec<Option<WaterfallVertexPlaneZone>> {
    debug_assert_eq!(terrain.vertices.len(), owners.len());
    let mut zones = vec![None::<WaterfallVertexPlaneZone>; terrain.vertices.len()];
    for (patch_index, &patch) in patches.iter().enumerate() {
        let selected =
            waterfall_side_bank_apron_for_patch(terrain, patch, coverage, owners, adjacency, banks);
        for (vertex, &is_selected) in selected.iter().enumerate() {
            if !is_selected {
                continue;
            }
            let candidate = WaterfallVertexPlaneZone {
                patch: patch_index,
                zone: patch.plane_zone(terrain.vertices[vertex].truncate()),
            };
            let replace = zones[vertex].is_none_or(|current| {
                candidate.zone == WaterfallPlaneZone::Face
                    && current.zone != WaterfallPlaneZone::Face
            });
            if replace {
                zones[vertex] = Some(candidate);
            }
        }
    }
    zones
}

/// Reconciles the river edge against the two exact waterfall planes. Water
/// edge vertices between (or touching) the lip and foot planes follow their
/// terrain support. Outside that interval the relationship is inverted: low
/// terrain edge vertices are lifted to the hydraulic surface, preventing a
/// second fall from forming immediately before or after the intended face.
pub(super) fn enforce_final_waterfall_edge_relationships(
    terrain: &mut Mesh,
    surfaces: &mut [f32],
    patches: &[WaterfallPatch],
    coverage: &[u8],
    owners: &[Option<RiverOwnerKey>],
    constraints: &mut WaterfallTerrainConstraints,
) -> usize {
    debug_assert_eq!(terrain.vertices.len(), surfaces.len());
    debug_assert_eq!(terrain.vertices.len(), coverage.len());
    debug_assert_eq!(terrain.vertices.len(), owners.len());
    if patches.is_empty() {
        return 0;
    }

    let adjacency = terrain.adjacency();
    let perimeter = terrain.perimeter_mask();
    let banks = waterfall_bank_mask(&adjacency, &perimeter, coverage);
    let zones = classify_waterfall_vertices(terrain, patches, coverage, owners, &adjacency, &banks);
    let levels =
        final_waterfall_channel_levels(terrain, surfaces, patches, coverage, &banks, &zones);
    let (mut adjusted, reaches) = enforce_waterfall_reach_surface_levels(
        surfaces,
        WaterfallReachEnvironment {
            terrain,
            patches,
            levels: &levels,
            coverage,
            owners,
        },
    );
    for (water_unclamped, &is_constrained_reach) in constraints
        .water_unclamped
        .iter_mut()
        .zip(&reaches.constrained)
    {
        *water_unclamped |= is_constrained_reach;
    }
    for (vertex, (position, &ceiling)) in terrain
        .vertices
        .iter_mut()
        .zip(&reaches.downstream_ceiling)
        .enumerate()
    {
        if ceiling.is_finite() && position.z > ceiling {
            let clamp = if banks[vertex] {
                1.0 - reaches.normal_blend[vertex]
            } else {
                1.0
            };
            let target = (ceiling - position.z).mul_add(clamp, position.z);
            if target.to_bits() != position.z.to_bits() {
                position.z = target;
                adjusted += 1;
            }
        }
    }

    for (vertex, zone) in zones.iter().copied().enumerate() {
        if !banks[vertex] || coverage[vertex] == 0 {
            continue;
        }
        match zone.map(|classification| classification.zone) {
            Some(WaterfallPlaneZone::Face) => {
                let target = terrain.vertices[vertex].z + WATERFALL_WATER_CLEARANCE;
                if surfaces[vertex].to_bits() != target.to_bits() {
                    surfaces[vertex] = target;
                    adjusted += 1;
                }
            }
            Some(WaterfallPlaneZone::BeforeLip | WaterfallPlaneZone::AfterFoot) | None
                if reaches.constrained[vertex] =>
            {
                let hydraulic_surface = surfaces[vertex];
                if !hydraulic_surface.is_finite() {
                    continue;
                }
                let lift = reaches.normal_blend[vertex];
                if terrain.vertices[vertex].z < hydraulic_surface {
                    let target = (hydraulic_surface - terrain.vertices[vertex].z)
                        .mul_add(lift, terrain.vertices[vertex].z);
                    terrain.vertices[vertex].z = target;
                    adjusted += 1;
                }
            }
            Some(WaterfallPlaneZone::BeforeLip | WaterfallPlaneZone::AfterFoot) | None => {}
        }
    }

    for vertex in 0..terrain.vertices.len() {
        if coverage[vertex] == 0 || !reaches.constrained[vertex] {
            continue;
        }
        let normal = reaches.normal_blend[vertex];
        if normal >= 1.0 || !surfaces[vertex].is_finite() {
            continue;
        }
        let hydraulic_surface = surfaces[vertex];
        let hugging_surface = terrain.vertices[vertex].z + WATERFALL_WATER_CLEARANCE;
        let target = (hydraulic_surface - hugging_surface).mul_add(normal, hugging_surface);
        if surfaces[vertex].to_bits() != target.to_bits() {
            surfaces[vertex] = target;
            adjusted += 1;
        }
    }
    adjusted
}

/// Smooths the upstream terrain apron after water pinning and bank lifting.
/// The Jacobi relaxation is strongest at the waterfall lip and fades to zero
/// at the upstream end of the pin/lift transition. River-edge water is then
/// pinned back to the moved terrain; the interior hydraulic surface remains
/// unchanged.
pub(super) fn smooth_pinned_waterfall_terrain(
    terrain: &mut Mesh,
    surfaces: &mut [f32],
    patches: &[WaterfallPatch],
    coverage: &[u8],
    owners: &[Option<RiverOwnerKey>],
) -> usize {
    debug_assert_eq!(terrain.vertices.len(), surfaces.len());
    debug_assert_eq!(terrain.vertices.len(), coverage.len());
    debug_assert_eq!(terrain.vertices.len(), owners.len());
    if patches.is_empty() {
        return 0;
    }

    let adjacency = terrain.adjacency();
    let perimeter = terrain.perimeter_mask();
    let banks = waterfall_bank_mask(&adjacency, &perimeter, coverage);
    let mut weights = vec![0.0_f32; terrain.vertices.len()];
    for &patch in patches {
        let blend_band = terrain
            .vertices
            .iter()
            .map(|position| patch.contains_upstream_pin_blend(position.truncate()))
            .collect::<Vec<_>>();
        let eligible = owners
            .iter()
            .zip(coverage)
            .zip(&blend_band)
            .map(|((&owner, &remaining), &inside_band)| {
                remaining != 0
                    && inside_band
                    && owner.is_some_and(|owner| patch.owns_local_channel(owner))
            })
            .collect::<Vec<_>>();
        let apron =
            expand_vertex_mask_to_banks(&adjacency, &eligible, &eligible, &blend_band, &banks);
        for (vertex, &selected) in apron.iter().enumerate() {
            if selected {
                weights[vertex] = weights[vertex]
                    .max(patch.upstream_pin_smoothing_weight(terrain.vertices[vertex].truncate()));
            }
        }
    }

    let mut snapshot = terrain.vertices.clone();
    let mut moved = vec![false; terrain.vertices.len()];
    for _ in 0..WATERFALL_EDGE_SMOOTHING_PASSES {
        for vertex in 0..terrain.vertices.len() {
            let weight = weights[vertex];
            if weight <= 0.0 || perimeter[vertex] || adjacency[vertex].is_empty() {
                continue;
            }
            let (total, count) = adjacency[vertex]
                .iter()
                .copied()
                .fold((snapshot[vertex], 1_u32), |(total, count), neighbour| {
                    (total + snapshot[neighbour], count + 1)
                });
            let average = total / count as f32;
            let target = snapshot[vertex].lerp(average, WATERFALL_EDGE_SMOOTHING * weight);
            if target != terrain.vertices[vertex] {
                terrain.vertices[vertex] = target;
                moved[vertex] = true;
            }
        }
        snapshot.copy_from_slice(&terrain.vertices);
    }

    for vertex in 0..terrain.vertices.len() {
        if weights[vertex] > 0.0 && coverage[vertex] != 0 && banks[vertex] {
            surfaces[vertex] = terrain.vertices[vertex].z + WATERFALL_WATER_CLEARANCE;
        }
    }
    if !terrain.uv.is_empty() {
        for (vertex, &was_moved) in moved.iter().enumerate() {
            if was_moved {
                terrain.uv[vertex] = terrain.vertices[vertex].truncate();
            }
        }
    }
    moved.into_iter().filter(|&was_moved| was_moved).count()
}

#[derive(Clone, Copy, Debug)]
pub(super) struct WaterfallChannelLevels {
    pub(super) lip: f32,
    pub(super) bottom: f32,
}

#[derive(Debug)]
pub(super) struct WaterfallReachConstraints {
    pub(super) constrained: Vec<bool>,
    pub(super) downstream_ceiling: Vec<f32>,
    pub(super) normal_blend: Vec<f32>,
}

#[derive(Clone, Copy)]
pub(super) struct WaterfallReachEnvironment<'a> {
    pub(super) terrain: &'a Mesh,
    pub(super) patches: &'a [WaterfallPatch],
    pub(super) levels: &'a [WaterfallChannelLevels],
    pub(super) coverage: &'a [u8],
    pub(super) owners: &'a [Option<RiverOwnerKey>],
}

pub(super) fn final_waterfall_channel_levels(
    terrain: &Mesh,
    surfaces: &[f32],
    patches: &[WaterfallPatch],
    coverage: &[u8],
    banks: &[bool],
    zones: &[Option<WaterfallVertexPlaneZone>],
) -> Vec<WaterfallChannelLevels> {
    let mut levels = patches
        .iter()
        .map(|patch| WaterfallChannelLevels {
            lip: patch.upper_surface,
            bottom: patch.lower_surface,
        })
        .collect::<Vec<_>>();
    let mut lip_scores = vec![f32::INFINITY; patches.len()];
    let mut bottom_scores = vec![f32::INFINITY; patches.len()];

    for (vertex, classification) in zones.iter().copied().enumerate() {
        let Some(classification) = classification.filter(|classification| {
            classification.zone == WaterfallPlaneZone::Face
                && coverage[vertex] != 0
                && !banks[vertex]
                && surfaces[vertex].is_finite()
        }) else {
            continue;
        };
        let patch = patches[classification.patch];
        let (along, lateral) = patch.local_coordinates(terrain.vertices[vertex].truncate());
        let lip_score = (along + WaterfallPatch::face_run()).abs().hypot(lateral);
        if lip_score < lip_scores[classification.patch] {
            lip_scores[classification.patch] = lip_score;
            levels[classification.patch].lip = surfaces[vertex];
        }
        let bottom_score = along.abs().hypot(lateral);
        if bottom_score < bottom_scores[classification.patch] {
            bottom_scores[classification.patch] = bottom_score;
            levels[classification.patch].bottom = surfaces[vertex];
        }
    }
    levels
}

pub(super) fn enforce_waterfall_reach_surface_levels(
    surfaces: &mut [f32],
    environment: WaterfallReachEnvironment<'_>,
) -> (usize, WaterfallReachConstraints) {
    let WaterfallReachEnvironment {
        terrain,
        patches,
        levels,
        coverage,
        owners,
    } = environment;
    debug_assert_eq!(patches.len(), levels.len());
    let mut by_river = HashMap::<u32, Vec<usize>>::new();
    for (patch_index, patch) in patches.iter().enumerate() {
        by_river.entry(patch.river).or_default().push(patch_index);
    }
    for river_patches in by_river.values_mut() {
        river_patches.sort_unstable_by_key(|&patch| patches[patch].segment);
    }

    let mut reaches = WaterfallReachConstraints {
        constrained: vec![false; terrain.vertices.len()],
        downstream_ceiling: vec![f32::INFINITY; terrain.vertices.len()],
        normal_blend: vec![1.0; terrain.vertices.len()],
    };
    let mut adjusted = 0;
    for vertex in 0..terrain.vertices.len() {
        let Some(owner) = owners[vertex].filter(|_| coverage[vertex] != 0) else {
            continue;
        };
        let Some(river_patches) = by_river.get(&owner.river) else {
            continue;
        };

        let point = terrain.vertices[vertex].truncate();
        let mut previous = None;
        let mut next = None;
        let mut on_face = false;
        for &patch_index in river_patches {
            let patch = patches[patch_index];
            // Refined footprint ownership can lag the geometric face by one
            // node. Only that immediate neighbourhood needs plane geometry;
            // farther reaches must follow river topology so bends and joins
            // cannot be mistaken for the opposite side of a waterfall.
            if owner.node.saturating_add(1) < patch.segment {
                next = Some(patch_index);
                break;
            }
            if owner.node > patch.segment.saturating_add(1) {
                previous = Some(patch_index);
                continue;
            }

            match patch.plane_zone(point) {
                WaterfallPlaneZone::BeforeLip => next = Some(patch_index),
                WaterfallPlaneZone::Face => on_face = true,
                WaterfallPlaneZone::AfterFoot => previous = Some(patch_index),
            }
            if on_face || next.is_some() {
                break;
            }
        }
        if on_face || (previous.is_none() && next.is_none()) {
            continue;
        }

        reaches.constrained[vertex] = true;
        let mut normal_blend = 1.0_f32;
        if let Some(previous) = previous {
            reaches.downstream_ceiling[vertex] = levels[previous].bottom;
            normal_blend = normal_blend.min(patches[previous].edge_normal_blend(point));
        }
        if let Some(next) = next {
            normal_blend = normal_blend.min(patches[next].edge_normal_blend(point));
        }
        reaches.normal_blend[vertex] = normal_blend;
        let mut target = surfaces[vertex];
        if let Some(next) = next {
            target = target.max(levels[next].lip);
        }
        if let Some(previous) = previous {
            target = target.min(levels[previous].bottom);
        }
        if target.to_bits() != surfaces[vertex].to_bits() {
            surfaces[vertex] = target;
            adjusted += 1;
        }
    }
    (adjusted, reaches)
}

pub(super) fn waterfall_side_bank_apron_for_patch(
    terrain: &Mesh,
    patch: WaterfallPatch,
    coverage: &[u8],
    owners: &[Option<RiverOwnerKey>],
    adjacency: &Adjacency,
    banks: &[bool],
) -> Vec<bool> {
    debug_assert_eq!(terrain.vertices.len(), owners.len());
    let seeds = terrain
        .vertices
        .iter()
        .zip(coverage)
        .zip(owners)
        .map(|((position, &remaining), &owner)| {
            remaining != 0
                && owner.is_some_and(|owner| patch.owns_local_channel(owner))
                && patch.contains_face_point(position.truncate())
        })
        .collect::<Vec<_>>();
    let constraint_band = terrain
        .vertices
        .iter()
        .map(|position| patch.contains_side_constraint_band(position.truncate()))
        .collect::<Vec<_>>();
    let eligible = coverage
        .iter()
        .zip(&constraint_band)
        .zip(owners)
        .map(|((&remaining, &inside_band), &owner)| {
            remaining != 0
                && inside_band
                && owner.is_some_and(|owner| patch.owns_local_channel(owner))
        })
        .collect::<Vec<_>>();
    let apron_eligible = constraint_band
        .iter()
        .zip(owners)
        .map(|(&inside_band, &owner)| {
            inside_band && owner.is_none_or(|owner| patch.owns_local_channel(owner))
        })
        .collect::<Vec<_>>();
    expand_vertex_mask_to_banks(adjacency, &seeds, &eligible, &apron_eligible, banks)
}

/// Rebuilds waterfall support from the final positions and the exact lip/foot
/// planes. Only the middle zone is terrain-pinned; vertices before the lip or
/// after the foot return to ordinary river-water handling.
pub(super) fn rebuild_final_waterfall_support_mask(
    terrain: &Mesh,
    patches: &[WaterfallPatch],
    coverage: &[u8],
    owners: &[Option<RiverOwnerKey>],
    constraints: &mut WaterfallTerrainConstraints,
) -> usize {
    debug_assert_eq!(terrain.vertices.len(), coverage.len());
    debug_assert_eq!(terrain.vertices.len(), owners.len());

    constraints.patch.fill(false);
    constraints.pinned.fill(false);
    constraints.support.fill(false);
    let mut supported = 0;
    for vertex in 0..terrain.vertices.len() {
        let Some(owner) = owners[vertex].filter(|_| coverage[vertex] != 0) else {
            continue;
        };
        let point = terrain.vertices[vertex].truncate();
        let is_face = patches.iter().any(|patch| {
            patch.owns_local_channel(owner) && patch.plane_zone(point) == WaterfallPlaneZone::Face
        });
        if !is_face {
            continue;
        }
        constraints.patch[vertex] = true;
        constraints.pinned[vertex] = true;
        constraints.support[vertex] = true;
        constraints.water_unclamped[vertex] = false;
        supported += 1;
    }
    supported
}

pub(super) fn waterfall_face_bank_apron_mask(
    terrain: &Mesh,
    patches: &[WaterfallPatch],
    coverage: &[u8],
    owners: &[Option<RiverOwnerKey>],
    adjacency: &Adjacency,
    perimeter: &[bool],
) -> Vec<bool> {
    debug_assert_eq!(terrain.vertices.len(), owners.len());
    let banks = waterfall_bank_mask(adjacency, perimeter, coverage);
    let mut selected = vec![false; terrain.vertices.len()];

    for patch in patches {
        let face_to_bank_apron = waterfall_face_bank_apron_for_patch(
            terrain, *patch, coverage, owners, adjacency, &banks,
        );
        selected
            .iter_mut()
            .zip(face_to_bank_apron)
            .for_each(|(included, candidate)| *included |= candidate);
    }
    selected
}

pub(super) fn waterfall_bank_mask(
    adjacency: &Adjacency,
    perimeter: &[bool],
    coverage: &[u8],
) -> Vec<bool> {
    coverage
        .iter()
        .enumerate()
        .map(|(vertex, &remaining)| {
            remaining != 0
                && (perimeter[vertex]
                    || adjacency[vertex]
                        .iter()
                        .any(|&neighbour| coverage[neighbour] == 0))
        })
        .collect()
}

pub(super) fn waterfall_face_bank_apron_for_patch(
    terrain: &Mesh,
    patch: WaterfallPatch,
    coverage: &[u8],
    owners: &[Option<RiverOwnerKey>],
    adjacency: &Adjacency,
    banks: &[bool],
) -> Vec<bool> {
    debug_assert_eq!(terrain.vertices.len(), owners.len());
    let face = terrain
        .vertices
        .iter()
        .zip(coverage)
        .zip(owners)
        .map(|((position, &remaining), &owner)| {
            remaining != 0
                && owner.is_some_and(|owner| patch.owns_local_channel(owner))
                && patch.contains_face_point(position.truncate())
        })
        .collect::<Vec<_>>();
    let smoothing_band = terrain
        .vertices
        .iter()
        .map(|position| patch.contains_face_smoothing_band(position.truncate()))
        .collect::<Vec<_>>();
    let eligible = coverage
        .iter()
        .zip(&smoothing_band)
        .zip(owners)
        .map(|((&remaining, &inside_band), &owner)| {
            remaining != 0
                && inside_band
                && owner.is_some_and(|owner| patch.owns_local_channel(owner))
        })
        .collect::<Vec<_>>();
    let apron_eligible = smoothing_band
        .iter()
        .zip(owners)
        .map(|(&inside_band, &owner)| {
            inside_band && owner.is_none_or(|owner| patch.owns_local_channel(owner))
        })
        .collect::<Vec<_>>();
    expand_vertex_mask_to_banks(adjacency, &face, &eligible, &apron_eligible, banks)
}

/// Repairs the rare final-pass failure where a bank immediately behind the
/// lip is averaged down toward a neighbouring cliff. Only banks that satisfy
/// the existing collapse detector are raised, and each is restored to the
/// analytic waterfall profile at its along-flow position.
pub(super) fn repair_collapsed_waterfall_banks(
    terrain: &mut Mesh,
    surfaces: &mut [f32],
    patches: &[WaterfallPatch],
    coverage: &[u8],
    owners: &[Option<RiverOwnerKey>],
) -> usize {
    debug_assert_eq!(terrain.vertices.len(), surfaces.len());
    debug_assert_eq!(terrain.vertices.len(), coverage.len());
    debug_assert_eq!(terrain.vertices.len(), owners.len());
    let adjacency = terrain.adjacency();
    let perimeter = terrain.perimeter_mask();
    let banks = waterfall_bank_mask(&adjacency, &perimeter, coverage);
    let mut targets = vec![f32::NEG_INFINITY; terrain.vertices.len()];

    for &patch in patches {
        let apron = waterfall_face_bank_apron_for_patch(
            terrain, patch, coverage, owners, &adjacency, &banks,
        );
        for vertex in 0..terrain.vertices.len() {
            if !banks[vertex] || !apron[vertex] {
                continue;
            }
            if let Some(target) =
                collapsed_waterfall_bank_target(terrain, &adjacency, patch, vertex)
            {
                targets[vertex] = targets[vertex].max(target);
            }
        }
    }

    let mut repaired = 0;
    for (vertex, target) in targets.into_iter().enumerate() {
        if !target.is_finite() || target <= terrain.vertices[vertex].z {
            continue;
        }
        terrain.vertices[vertex].z = target;
        surfaces[vertex] = surfaces[vertex].max(target + WATERFALL_WATER_CLEARANCE);
        repaired += 1;
    }
    repaired
}

fn collapsed_waterfall_bank_target(
    terrain: &Mesh,
    adjacency: &Adjacency,
    patch: WaterfallPatch,
    vertex: usize,
) -> Option<f32> {
    let drop = patch.upper_surface - patch.lower_surface;
    if drop <= RIVER_SURFACE_OFFSET {
        return None;
    }
    let position = terrain.vertices[vertex];
    let low_bank_ceiling = drop.mul_add(-WATERFALL_FINAL_BANK_DROP_FRACTION, patch.upper_surface);
    if position.z > low_bank_ceiling {
        return None;
    }
    let (along, _) = patch.local_coordinates(position.truncate());
    if !(-3.0 * WATERFALL_TARGET_EDGE_LENGTH..=-WATERFALL_TARGET_EDGE_LENGTH * 0.5).contains(&along)
    {
        return None;
    }
    let minimum_edge_drop = drop * WATERFALL_FINAL_BANK_EDGE_DROP_FRACTION;
    let has_high_neighbour = adjacency[vertex].iter().any(|&neighbour| {
        let neighbour_position = terrain.vertices[neighbour];
        let (neighbour_along, _) = patch.local_coordinates(neighbour_position.truncate());
        neighbour_along <= WATERFALL_TARGET_EDGE_LENGTH * 0.25
            && neighbour_position.z - position.z >= minimum_edge_drop
    });
    if !has_high_neighbour {
        return None;
    }
    let surface = patch
        .face_surface_at(position.truncate())
        .unwrap_or(patch.upper_surface);
    let target = surface - WATERFALL_WATER_CLEARANCE;
    (target > position.z).then_some(target)
}

/// Detects the characteristic failed final waterfall where smoothing has
/// dragged an upstream bank vertex down toward the lower terrace. This runs
/// after every terrain refinement and reprojection pass, so the caller can
/// reject the exact site and regenerate from an untouched LOD 0 snapshot.
pub(super) fn detect_failed_final_waterfalls(
    terrain: &Mesh,
    patches: &[WaterfallPatch],
    coverage: &[u8],
    owners: &[Option<RiverOwnerKey>],
) -> Vec<usize> {
    debug_assert_eq!(terrain.vertices.len(), owners.len());
    let adjacency = terrain.adjacency();
    let perimeter = terrain.perimeter_mask();
    let banks = waterfall_bank_mask(&adjacency, &perimeter, coverage);
    let mut failed = Vec::new();

    for &patch in patches {
        let drop = patch.upper_surface - patch.lower_surface;
        if drop <= RIVER_SURFACE_OFFSET {
            continue;
        }
        let apron = waterfall_face_bank_apron_for_patch(
            terrain, patch, coverage, owners, &adjacency, &banks,
        );
        let malformed = terrain.vertices.iter().enumerate().any(|(vertex, _)| {
            banks[vertex]
                && apron[vertex]
                && collapsed_waterfall_bank_target(terrain, &adjacency, patch, vertex).is_some()
        });
        if malformed {
            failed.push(patch.upper_vertex);
        }
    }
    failed.sort_unstable();
    failed.dedup();
    failed
}

#[derive(Clone, Copy)]
pub(super) struct WaterfallPinEnvironment<'a> {
    pub(super) terrain: &'a Mesh,
    pub(super) patches: &'a [WaterfallPatch],
    pub(super) notch_owners: &'a [Option<usize>],
    pub(super) river_owners: &'a [Option<RiverOwnerKey>],
}

/// Pins the complete bank-to-bank upstream waterfall face. Downstream vertices
/// retain their ordinary river heights and smoothing eligibility; their water
/// is left at the hydraulic surface so local terrain projections pierce it
/// rather than lifting the sheet into spikes.
pub(super) fn pin_waterfalls_to_terrain(
    environment: WaterfallPinEnvironment<'_>,
    material: &mut SurfaceMaterial,
    coverage: &mut [u8],
    surfaces: &mut [f32],
    waterfall_lips: &mut [bool],
) -> WaterfallTerrainConstraints {
    let WaterfallPinEnvironment {
        terrain,
        patches,
        notch_owners,
        river_owners,
    } = environment;
    debug_assert_eq!(terrain.vertices.len(), notch_owners.len());
    debug_assert_eq!(terrain.vertices.len(), river_owners.len());
    let mut constraints = WaterfallTerrainConstraints {
        patch: vec![false; terrain.vertices.len()],
        pinned: vec![false; terrain.vertices.len()],
        support: vec![false; terrain.vertices.len()],
        water_unclamped: vec![false; terrain.vertices.len()],
        terrain_ceiling: vec![f32::INFINITY; terrain.vertices.len()],
    };
    if patches.is_empty() {
        return constraints;
    }

    for (vertex, owner) in notch_owners.iter().copied().enumerate() {
        let Some(patch_index) = owner else {
            continue;
        };
        let patch = patches[patch_index];
        let point = terrain.vertices[vertex].truncate();
        let (along, _) = patch.local_coordinates(point);
        let Some(face_surface) = patch.face_surface_at(point) else {
            constraints.terrain_ceiling[vertex] = terrain.vertices[vertex].z;
            if coverage[vertex] != 0 {
                constraints.water_unclamped[vertex] = true;
                surfaces[vertex] = surfaces[vertex].min(patch.lower_surface);
                waterfall_lips[vertex] = false;
            }
            continue;
        };
        constraints.patch[vertex] = true;
        if coverage[vertex] == 0 {
            continue;
        }
        constraints.pinned[vertex] = true;
        constraints.support[vertex] = true;
        material.depths_mut()[vertex] = 0.0;
        surfaces[vertex] = face_surface;
        waterfall_lips[vertex] = along.abs() <= WATERFALL_TARGET_EDGE_LENGTH;
    }

    for (vertex, ((&remaining, position), &river_owner)) in coverage
        .iter()
        .zip(&terrain.vertices)
        .zip(river_owners)
        .enumerate()
    {
        if remaining == 0 {
            continue;
        }
        let lower_surface = patches
            .iter()
            .filter(|patch| {
                river_owner.is_some_and(|owner| patch.owns_local_channel(owner))
                    && patch.contains_downstream_point(position.truncate())
            })
            .map(|patch| patch.lower_surface)
            .fold(f32::INFINITY, f32::min);
        if !lower_surface.is_finite() {
            continue;
        }
        constraints.water_unclamped[vertex] = true;
        surfaces[vertex] = surfaces[vertex].min(lower_surface);
        waterfall_lips[vertex] = false;
    }

    let adjacency = terrain.adjacency();
    for value in coverage.iter_mut() {
        *value &= !RIVER_BOUNDARY;
    }
    mark_river_boundary(&adjacency, &terrain.perimeter_mask(), coverage);
    constraints
}

/// Restores the carved downstream half of each waterfall fan after ordinary
/// bank lifting and river relaxation. The ceiling is not a smoothing mask: it
/// only prevents those later stages from undoing the lower-terrace cut.
pub(super) fn enforce_waterfall_downstream_ceiling(terrain: &mut Mesh, ceilings: &[f32]) -> usize {
    debug_assert_eq!(terrain.vertices.len(), ceilings.len());
    let mut lowered = 0;
    for (position, &ceiling) in terrain.vertices.iter_mut().zip(ceilings) {
        if ceiling.is_finite() && position.z > ceiling {
            position.z = ceiling;
            lowered += 1;
        }
    }
    lowered
}

/// Removes residual convex fins after all river and bank shaping. The pass is
/// deliberately one-sided: it lowers anomalously high downstream terrain and
/// water vertices, but never fills pools or raises their neighbours.
pub(super) fn squish_waterfall_downstream_spikes(
    terrain: &mut Mesh,
    surfaces: &mut [f32],
    downstream: &[bool],
) -> usize {
    debug_assert_eq!(terrain.vertices.len(), surfaces.len());
    debug_assert_eq!(terrain.vertices.len(), downstream.len());
    if !downstream.iter().any(|&selected| selected) {
        return 0;
    }

    let adjacency = terrain.adjacency();
    let mut terrain_snapshot = terrain
        .vertices
        .iter()
        .map(|position| position.z)
        .collect::<Vec<_>>();
    let mut surface_snapshot = surfaces.to_vec();
    let mut adjusted = vec![false; terrain.vertices.len()];

    for _ in 0..WATERFALL_DOWNSTREAM_SPIKE_PASSES {
        for vertex in 0..terrain.vertices.len() {
            if !downstream[vertex] {
                continue;
            }
            let (terrain_total, surface_total, count) = adjacency[vertex]
                .iter()
                .copied()
                .filter(|&neighbour| downstream[neighbour])
                .fold((0.0, 0.0, 0_u32), |(terrain, surface, count), neighbour| {
                    (
                        terrain + terrain_snapshot[neighbour],
                        surface + surface_snapshot[neighbour],
                        count + 1,
                    )
                });
            if count < 2 {
                continue;
            }
            let inverse_count = 1.0 / count as f32;
            let terrain_ceiling =
                terrain_total.mul_add(inverse_count, WATERFALL_DOWNSTREAM_SPIKE_ALLOWANCE);
            let surface_ceiling =
                surface_total.mul_add(inverse_count, WATERFALL_DOWNSTREAM_SPIKE_ALLOWANCE);
            if terrain.vertices[vertex].z > terrain_ceiling {
                terrain.vertices[vertex].z = terrain_ceiling;
                adjusted[vertex] = true;
            }
            if surfaces[vertex] > surface_ceiling {
                surfaces[vertex] = surface_ceiling;
                adjusted[vertex] = true;
            }
        }
        terrain_snapshot
            .iter_mut()
            .zip(&terrain.vertices)
            .for_each(|(height, position)| *height = position.z);
        surface_snapshot.copy_from_slice(surfaces);
    }

    adjusted.into_iter().filter(|&changed| changed).count()
}
