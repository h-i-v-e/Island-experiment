#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss
)]

use std::{
    cmp::Ordering,
    collections::{BinaryHeap, HashMap, HashSet, VecDeque},
};

use crate::{
    Adjacency, ISLAND_WORLD_METRES, Mesh, Vec2, Vec3, noise,
    rng::Rng,
    terrain::{
        ProjectedFaceAreas, SurfaceMaterial, VertexFaceAdjacency, bedrock_erosion_rate,
        projected_vertex_control_areas,
    },
};

mod carving;
mod channel;
mod geometry;
mod rocks;
mod tracing;
mod waterfalls;

use carving::{
    DeltaScratch, RiverCarveOptions, RiverCarveParameters, RiverCarveScratch,
    RiverChannelParameters, RiverProfileEnvironment, RiverProfileScratch, RiverTerrain,
    WaterfallRelocation, WaterfallSiteEnvironment, average_edge_length, create_delta,
    enforce_gentle_river_profile, form_river_profile, lower_profile_reach_through_confluence,
    relocate_conflicting_waterfalls, river_mouth_transition, shape_and_carve_river,
    unfitted_river_depth,
};
pub(crate) use channel::encode_bank_distance_in_uv;
use channel::{
    ConfluenceCarveTarget, apply_averaged, build_river_footprint, carve_confluence_connectors,
    carve_river_corridor, duplicate_river_topology, is_river_bed_triangle, is_river_boundary,
    lower_river_surroundings, mark_river_boundary, record_confluence_carve_target,
    river_half_width, river_ring_count, river_topology_masks, shape_channel_ring_vertices,
    smooth_river_corridor, target_cross_sections, update_achieved_cross_sections,
};
use geometry::{
    BuiltRiverGeometry, RiverChannelFootprintOwner, RiverFootprint, RiverGeometryBuilder,
    RiverMeshBuffers, RiverOwnerKey, confluence_connector, finalize_river_budgets,
    lower_precarve_river_corridors_to_profiles, lower_precarve_river_valleys,
    raise_precarve_waterfall_shoulders, river_reaches_ocean, transfer_tributary_budgets,
};
pub(crate) use rocks::append_settled_rocks;
use rocks::generate_river_rock_mesh;
pub(crate) use tracing::fix_inland_seas;
use tracing::{
    RouteState, WaterfallClearanceIndex, calculate_flow_and_catchment, find_sources,
    map_downstream, trace_rivers, update_join_flows,
};
use waterfalls::{
    WaterfallPatch, WaterfallTerrainConstraints, derive_waterfall_patches,
    detect_failed_final_waterfalls, enforce_final_waterfall_edge_relationships,
    enforce_waterfall_downstream_ceiling, expand_vertex_mask_through_river_to_banks,
    pin_waterfalls_to_terrain, rebuild_final_waterfall_support_mask, recess_waterfall_notches,
    smooth_final_waterfall_patches, smooth_pinned_waterfall_terrain, smoothstep,
    squish_waterfall_downstream_spikes,
};

#[cfg(test)]
mod tests;

pub(crate) struct RiverParts {
    pub(crate) rivers: Vec<River>,
    pub(crate) river_mesh: Mesh,
    pub(crate) river_bed: Vec<bool>,
    pub(crate) river_rock_mesh: Mesh,
    pub(crate) failed_waterfalls: Vec<usize>,
}

pub(crate) const RIVER_SURFACE_OFFSET: f32 = 0.000_01;
// Unity scales the normalized terrain mesh to 2,000 metres across.
const SEA_PLANE_CLEARANCE: f32 = 0.10 / 2_000.0;
const RIVER_SOURCE_EXCLUSION_CELL_METRES: f32 = 25.0;
const SQUARE_METRES_PER_HECTARE: f32 = 10_000.0;
const MAX_RIVER_RINGS: u8 = 3;
const CHANNEL_FOOTPRINT_RINGS: u8 = 1;
const RIVER_BOUNDARY: u8 = 1 << 7;
const CHANNEL_RING_SHAPING_PASSES: usize = 2;
const MAXIMUM_RING_MOVE_FRACTION: f32 = 0.8;
const MAXIMUM_WIDTH_DEPTH_COMPENSATION: f32 = 0.5;
const MAXIMUM_GENTLE_RIVER_GRADE: f32 = 0.04;
const RIVER_CORRIDOR_SMOOTHING: f32 = 0.35;
// Establish a shallow valley on the original terrain before the river corridor
// is moved, tessellated, or carved.
const PRECARVE_VALLEY_OUTER_RINGS: u8 = 3;
const PRECARVE_VALLEY_CENTRE_DEPTH: f32 = 1.25 / ISLAND_WORLD_METRES;
const PRECARVE_CONFLUENCE_CONNECTOR_RINGS: u8 = MAX_RIVER_RINGS * 2 + 1;
const PRECARVE_WATERFALL_BANK_RINGS: u8 = 2;
const PRECARVE_WATERFALL_OUTER_RINGS: u8 = 3;
const RIVER_VALLEY_APRON_RINGS: u8 = 3;
const RIVER_VALLEY_BANK_DEPTH_FRACTION: f32 = 0.25;
const RIVER_REFINEMENT_PASSES: usize = 3;
const RIVER_REFINEMENT_APRON_RINGS: u8 = 2;
const MINIMUM_RIVER_EDGE_LENGTH: f32 = 1.5 / ISLAND_WORLD_METRES;
const MAXIMUM_RIVER_EDGE_LENGTH: f32 = 5.0 / ISLAND_WORLD_METRES;
const RIVER_BANK_BLEND_HALF_WIDTH_MULTIPLIER: f32 = 2.0;
const MINIMUM_RIVER_BANK_BLEND_WIDTH: f32 = 4.0 / ISLAND_WORLD_METRES;
const MAXIMUM_RIVER_BANK_BLEND_WIDTH: f32 = 20.0 / ISLAND_WORLD_METRES;
const RIVER_CHANNEL_CORE_BLEND: f32 = 0.5;
const RIVER_CHANNEL_CLEARANCE_SMOOTHING: f32 = 0.35;
const FINAL_RIVER_RELAXATION_PASSES: usize = 4;
const FINAL_RIVER_RELAXATION: f32 = 0.35;
const FINAL_RIVER_PROFILE_ATTRACTION: f32 = 0.65;
#[cfg(test)]
const WATERFALL_LIP_SMOOTHING: f32 = 0.5;
const WATERFALL_REFINEMENT_PASSES: usize = 1;
const WATERFALL_TARGET_EDGE_LENGTH: f32 = 1.25 / ISLAND_WORLD_METRES;
const WATERFALL_FINAL_SMOOTHING_PASSES: usize = 2;
const WATERFALL_EDGE_SMOOTHING_PASSES: usize = 6;
const WATERFALL_EDGE_SMOOTHING: f32 = 0.5;
const WATERFALL_EDGE_BLEND_RUN: f32 = 2.0 * WATERFALL_TARGET_EDGE_LENGTH;
const WATERFALL_SUPPORT_RUN: f32 = 0.75 / ISLAND_WORLD_METRES;
const WATERFALL_APRON_WIDTH_MULTIPLIER: f32 = 1.75;
const WATERFALL_LANDING_LENGTH_MULTIPLIER: f32 = 1.5;
const WATERFALL_WATER_CLEARANCE: f32 = 0.03 / ISLAND_WORLD_METRES;
const WATERFALL_DOWNSTREAM_SPIKE_PASSES: usize = 4;
const WATERFALL_DOWNSTREAM_SPIKE_ALLOWANCE: f32 = 0.10 / ISLAND_WORLD_METRES;
const WATERFALL_SITE_MINIMUM_BANK_SPAN_FRACTION: f32 = 0.15;
const WATERFALL_SITE_BYPASS_MAX_HOPS: u8 = 4;
const WATERFALL_FINAL_BANK_DROP_FRACTION: f32 = 0.45;
const WATERFALL_FINAL_BANK_EDGE_DROP_FRACTION: f32 = 0.35;
// Diagnostic switch: show every completed waterfall so pre-carve shoulder
// geometry can be evaluated without whole-island rejection retries.
const ENABLE_FINAL_WATERFALL_REJECTION: bool = false;
// Diagnostic switch: retain the plunge-pool implementation while preventing
// generated waterfalls from creating pools.
const ENABLE_WATERFALL_PLUNGE_POOLS: bool = false;
const WATERFALL_POOL_MINIMUM_DROP: f32 = 0.75 / ISLAND_WORLD_METRES;
const WATERFALL_POOL_MAXIMUM_DEPTH: f32 = 0.75 / ISLAND_WORLD_METRES;
const RIVER_CHANNEL_DRAINAGE_FLOOR: f32 = 0.30;
const RIVER_VALLEY_DRAINAGE_FLOOR: f32 = 0.15;
// A strict one-ring extremum must stand this far from its neighbours relative
// to their mean horizontal spacing before the final repair touches it.
const SHARP_POINT_HEIGHT_RATIO: f32 = 0.35;
const SHARP_POINT_SMOOTHING: f32 = 0.6;
const SHARP_POINT_SMOOTHING_PASSES: usize = 2;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct RiverSourceRule {
    catchment_square_metres: f32,
    steep_multiplier: f32,
    elevation_boost: f32,
    inverse_maximum_elevation: f32,
}

impl RiverSourceRule {
    pub(crate) const fn new(
        catchment_hectares: f32,
        steep_multiplier: f32,
        elevation_boost: f32,
        maximum_elevation: f32,
    ) -> Self {
        Self {
            catchment_square_metres: catchment_hectares * SQUARE_METRES_PER_HECTARE,
            steep_multiplier,
            elevation_boost,
            inverse_maximum_elevation: 1.0 / maximum_elevation,
        }
    }

    pub(crate) fn required_catchment(self, grade: f32, elevation: f32) -> f32 {
        let slope_response = grade * grade;
        let slope_multiplier = (self.steep_multiplier - 1.0).mul_add(slope_response, 1.0);
        let elevation_fraction = (elevation * self.inverse_maximum_elevation).clamp(0.0, 1.0);
        let elevation_multiplier = self.elevation_boost.mul_add(1.0 - elevation_fraction, 1.0);
        self.catchment_square_metres * slope_multiplier * elevation_multiplier
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RiverNode {
    pub vertex: usize,
    pub flow: u32,
    pub surface: f32,
    pub position: Vec3,
}

#[derive(Clone, Debug, PartialEq)]
pub struct River {
    pub nodes: Vec<RiverNode>,
    pub join: Option<usize>,
}

impl River {
    /// The target channel half width at every node, using the same monotone
    /// flow-and-path growth rule the terrain carving pass uses.
    ///
    /// The returned widths use the same unit as the two inputs, so callers may
    /// ask in metres or in normalized island coordinates without conversion
    /// inside the rule.
    #[must_use]
    pub fn target_half_widths(&self, source_width: f32, maximum_width: f32) -> Vec<f32> {
        channel::target_half_widths(self, source_width, maximum_width)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct RiverChannelSettings {
    pub(crate) source_width: f32,
    pub(crate) maximum_width: f32,
    pub(crate) source_depth: f32,
    pub(crate) maximum_depth: f32,
}

impl Default for RiverChannelSettings {
    fn default() -> Self {
        Self {
            source_width: 2.0 / ISLAND_WORLD_METRES,
            maximum_width: 14.0 / ISLAND_WORLD_METRES,
            source_depth: 0.35 / ISLAND_WORLD_METRES,
            maximum_depth: 2.0 / ISLAND_WORLD_METRES,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct RiverCrossSection {
    target_half_width: f32,
    nominal_depth: f32,
    achieved_width: f32,
    required_depth: f32,
}

#[derive(Clone, Copy, Debug, Default)]
struct RiverSedimentBudget {
    carried: f64,
    loose_eroded: f64,
    bedrock_eroded: f64,
    deposited: f64,
    exported: f64,
}

impl RiverSedimentBudget {
    fn record_erosion(&mut self, loose_depth: f32, bedrock_depth: f32, area: f32) {
        let loose = f64::from(loose_depth) * f64::from(area.max(0.0));
        let bedrock = f64::from(bedrock_depth) * f64::from(area.max(0.0));
        self.loose_eroded += loose;
        self.bedrock_eroded += bedrock;
        self.carried += loose + bedrock;
    }

    fn export_remaining(&mut self) {
        self.exported += self.carried;
        self.carried = 0.0;
    }

    fn absorb(&mut self, upstream: Self) {
        self.carried += upstream.carried;
        self.loose_eroded += upstream.loose_eroded;
        self.bedrock_eroded += upstream.bedrock_eroded;
        self.deposited += upstream.deposited;
        self.exported += upstream.exported;
    }

    fn is_balanced(self) -> bool {
        let source = self.loose_eroded + self.bedrock_eroded;
        let sink = self.carried + self.deposited + self.exported;
        (source - sink).abs() <= source.max(1.0) * 1.0e-6
    }
}

impl River {
    #[must_use]
    pub fn source_flow(&self) -> u32 {
        self.nodes.first().map_or(0, |node| node.flow)
    }
}

#[derive(Clone, Debug)]
pub(crate) struct RiverNetwork {
    rivers: Vec<River>,
    join_vertices: Vec<Option<usize>>,
    waterfalls: Vec<Vec<bool>>,
    river_mesh_ends: Vec<Option<usize>>,
    max_flow: u32,
    max_height: f32,
    ocean: Vec<bool>,
    perimeter: Vec<bool>,
    cross_sections: Vec<Vec<RiverCrossSection>>,
}

impl RiverNetwork {
    pub(crate) fn generate(
        mesh: &mut Mesh,
        adjacency: &Adjacency,
        source_rule: RiverSourceRule,
        seed: u64,
    ) -> Self {
        let ocean = fix_inland_seas(mesh, adjacency);
        Self::generate_with_ocean(mesh, adjacency, source_rule, ocean, seed)
    }

    pub(crate) fn generate_with_ocean(
        mesh: &mut Mesh,
        adjacency: &Adjacency,
        source_rule: RiverSourceRule,
        ocean: Vec<bool>,
        seed: u64,
    ) -> Self {
        debug_assert_eq!(ocean.len(), mesh.vertices.len());
        let perimeter = mesh.perimeter_mask();
        let downstream = map_downstream(mesh, adjacency);
        let (flow, catchment_areas) = calculate_flow_and_catchment(mesh, &downstream);
        let sources = find_sources(mesh, adjacency, &downstream, &catchment_areas, source_rule);
        let (mut rivers, join_vertices) =
            trace_rivers(mesh, adjacency, &flow, &sources, &ocean, seed);
        update_join_flows(&mut rivers, &join_vertices);
        let max_flow = rivers
            .iter()
            .flat_map(|river| river.nodes.iter())
            .map(|node| node.flow)
            .max()
            .unwrap_or(1);
        let max_height = mesh
            .vertices
            .iter()
            .map(|vertex| vertex.z)
            .fold(f32::EPSILON, f32::max);
        let waterfalls = rivers
            .iter()
            .map(|river| vec![false; river.nodes.len()])
            .collect();
        let river_mesh_ends = vec![None; rivers.len()];
        let cross_sections = rivers
            .iter()
            .map(|river| vec![RiverCrossSection::default(); river.nodes.len()])
            .collect();
        Self {
            rivers,
            join_vertices,
            waterfalls,
            river_mesh_ends,
            max_flow,
            max_height,
            ocean,
            perimeter,
            cross_sections,
        }
    }

    #[cfg(test)]
    pub(crate) fn shape(
        &mut self,
        mesh: &mut Mesh,
        adjacency: &Adjacency,
        material: &mut SurfaceMaterial,
        smooth: bool,
        form_deltas: bool,
    ) {
        self.shape_with_settings(
            mesh,
            adjacency,
            material,
            smooth,
            form_deltas,
            RiverChannelSettings::default(),
        );
    }

    pub(crate) fn shape_with_settings(
        &mut self,
        mesh: &mut Mesh,
        adjacency: &Adjacency,
        material: &mut SurfaceMaterial,
        smooth: bool,
        form_deltas: bool,
        channel_settings: RiverChannelSettings,
    ) {
        self.shape_with_settings_and_waterfall_rejections(
            mesh,
            adjacency,
            material,
            smooth,
            form_deltas,
            channel_settings,
            &HashSet::new(),
        );
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn shape_with_settings_and_waterfall_rejections(
        &mut self,
        mesh: &mut Mesh,
        adjacency: &Adjacency,
        material: &mut SurfaceMaterial,
        smooth: bool,
        form_deltas: bool,
        channel_settings: RiverChannelSettings,
        rejected_waterfall_vertices: &HashSet<usize>,
    ) {
        if self.rivers.is_empty() {
            return;
        }
        let loose_volume = material.volume(mesh);
        self.jiggle(mesh);
        if smooth {
            self.smooth(mesh, adjacency);
        }
        material.rescale_to_volume(mesh, loose_volume);
        let bedrock_rates: Vec<f32> = material
            .hardnesses()
            .iter()
            .map(|&hardness| bedrock_erosion_rate(hardness))
            .collect();
        self.carve(
            mesh,
            adjacency,
            material,
            &bedrock_rates,
            RiverCarveOptions {
                form_deltas,
                channel_settings,
                rejected_waterfall_vertices,
            },
        );
        self.refresh(mesh);
    }

    pub(crate) fn into_parts_with_waterfall_failures(
        mut self,
        mesh: &mut Mesh,
        material: &mut SurfaceMaterial,
        seed: u64,
    ) -> RiverParts {
        let geometry = self.build_mesh_with_mask(mesh, material, seed);
        mesh.calculate_normals();
        self.refresh(mesh);
        RiverParts {
            rivers: self.rivers,
            river_mesh: geometry.river_mesh,
            river_bed: geometry.river_bed,
            river_rock_mesh: geometry.river_rock_mesh,
            failed_waterfalls: geometry.failed_waterfalls,
        }
    }

    fn jiggle(&mut self, mesh: &mut Mesh) {
        let mut river_vertex = vec![false; mesh.vertices.len()];
        for node in self.rivers.iter().flat_map(|river| &river.nodes) {
            river_vertex[node.vertex] = true;
        }
        let original = mesh.vertices.clone();
        let mut best_height = vec![f32::INFINITY; original.len()];
        let mut best_centroid = vec![Vec3::ZERO; original.len()];
        for triangle in mesh.triangles.chunks_exact(3) {
            let centroid = (original[triangle[0] as usize]
                + original[triangle[1] as usize]
                + original[triangle[2] as usize])
                / 3.0;
            for &vertex in triangle {
                let vertex = vertex as usize;
                if river_vertex[vertex] && centroid.z < best_height[vertex] {
                    best_height[vertex] = centroid.z;
                    best_centroid[vertex] = centroid;
                }
            }
        }
        let mut accumulated = vec![Vec3::default(); original.len()];
        let mut count = vec![0_u32; original.len()];
        for river in &self.rivers {
            for node in river
                .nodes
                .iter()
                .skip(1)
                .take(river.nodes.len().saturating_sub(2))
            {
                let current = original[node.vertex];
                let lowest = best_height[node.vertex];
                if lowest.is_finite() {
                    let centroid = best_centroid[node.vertex];
                    let slope =
                        ((current.z - lowest) / self.max_height.max(f32::EPSILON)).clamp(0.0, 1.0);
                    let moved = current * (0.35 + slope * 0.35) + centroid * (0.65 - slope * 0.35);
                    accumulated[node.vertex] += moved;
                    count[node.vertex] += 1;
                }
            }
        }
        apply_averaged(mesh, &accumulated, &count, &self.perimeter);
    }

    fn smooth(&self, mesh: &mut Mesh, adjacency: &Adjacency) {
        let original = mesh.vertices.clone();
        let mut accumulated = vec![Vec3::default(); original.len()];
        let mut count = vec![0_u32; original.len()];
        for river in &self.rivers {
            for window in river.nodes.windows(3) {
                let previous = window[0].vertex;
                let center = window[1].vertex;
                let next = window[2].vertex;
                accumulated[center] +=
                    (original[previous] + original[center] + original[next]) / 3.0;
                count[center] += 1;
                for &bank in &adjacency[center] {
                    if bank == previous || bank == next {
                        continue;
                    }
                    let (sum, total) = adjacency[bank]
                        .iter()
                        .fold((original[bank], 1_u32), |(sum, total), &neighbour| {
                            (sum + original[neighbour], total + 1)
                        });
                    accumulated[bank] += sum / total as f32;
                    count[bank] += 1;
                }
            }
        }
        apply_averaged(mesh, &accumulated, &count, &self.perimeter);
    }

    fn carve(
        &mut self,
        mesh: &mut Mesh,
        adjacency: &Adjacency,
        material: &mut SurfaceMaterial,
        bedrock_rates: &[f32],
        options: RiverCarveOptions<'_>,
    ) {
        debug_assert_eq!(material.depths().len(), mesh.vertices.len());
        debug_assert_eq!(bedrock_rates.len(), mesh.vertices.len());
        let base_width = average_edge_length(mesh, adjacency).max(0.000_25);
        let control_areas = projected_vertex_control_areas(mesh);
        self.cross_sections = if options.form_deltas {
            self.rivers.iter().map(|_| Vec::new()).collect()
        } else {
            target_cross_sections(&self.rivers, options.channel_settings)
        };
        if !self.prepare_valid_channel_profiles(mesh, adjacency, base_width, options) {
            return;
        }
        self.prepare_channel_terrain(mesh, adjacency, material, options);
        let mut terrain = RiverTerrain {
            mesh,
            adjacency,
            material,
            bedrock_rates,
            control_areas: &control_areas,
        };
        self.carve_prepared_channels(&mut terrain, options.form_deltas);
    }

    fn prepare_valid_channel_profiles(
        &mut self,
        mesh: &Mesh,
        adjacency: &Adjacency,
        base_width: f32,
        options: RiverCarveOptions<'_>,
    ) -> bool {
        loop {
            let depth_multiplier = 1.0 / (self.max_flow as f32).sqrt().max(1.0);
            let waterfall_clearance =
                WaterfallClearanceIndex::new(&self.rivers, mesh, self.max_flow, base_width);
            let footprint = build_river_footprint(self, mesh, adjacency, false);
            let invalid = self.prepare_channel_profiles(
                mesh,
                adjacency,
                RiverChannelParameters { depth_multiplier },
                &waterfall_clearance,
                &footprint,
                (!options.form_deltas).then_some(options.rejected_waterfall_vertices),
            );
            if !invalid.iter().any(|&failed| failed) {
                return true;
            }
            self.remove_invalid_rivers(&invalid);
            if self.rivers.is_empty() {
                return false;
            }
        }
    }

    fn prepare_channel_terrain(
        &mut self,
        mesh: &mut Mesh,
        adjacency: &Adjacency,
        material: &mut SurfaceMaterial,
        options: RiverCarveOptions<'_>,
    ) {
        lower_precarve_river_valleys(self, mesh, adjacency);
        raise_precarve_waterfall_shoulders(self, mesh, adjacency);
        self.refresh_after_vertical_displacement(mesh);
        // Valley and waterfall-shoulder shaping can move the incoming and
        // receiving centrelines by different amounts. Reconcile again after
        // those displacements so the detailed corridor, connector, and water
        // topology inherit one continuous downhill profile at every join.
        self.reconcile_confluence_profiles();
        lower_precarve_river_corridors_to_profiles(self, mesh, adjacency);
        self.refresh(mesh);
        if !options.form_deltas {
            let loose_volume = material.volume(mesh);
            self.form_channel_rings(mesh, adjacency);
            material.rescale_to_volume(mesh, loose_volume);
            let footprint = build_river_footprint(self, mesh, adjacency, false);
            update_achieved_cross_sections(
                self,
                mesh,
                &footprint,
                options.channel_settings.maximum_depth,
            );
            self.enforce_gentle_final_profiles(mesh);
        }
    }

    fn carve_prepared_channels(&mut self, terrain: &mut RiverTerrain<'_>, form_deltas: bool) {
        let depth_multiplier = 1.0 / (self.max_flow as f32).sqrt().max(1.0);
        // Intermediate passes establish the broad valley and delta. Only the
        // detailed final pass should force the submerged mouth below sea level.
        let carve_submerged_mouths = !form_deltas;
        let mut known_surfaces = HashMap::<usize, f32>::new();
        let mut budgets = vec![RiverSedimentBudget::default(); self.rivers.len()];
        self.carve_channels(
            terrain,
            RiverChannelParameters { depth_multiplier },
            &mut known_surfaces,
            &mut budgets,
            carve_submerged_mouths,
        );
        let footprint = build_river_footprint(self, terrain.mesh, terrain.adjacency, false);
        let channel_parameters = RiverChannelParameters { depth_multiplier };
        let carve =
            carve_river_corridor(self, terrain, &footprint, channel_parameters, &mut budgets);
        lower_river_surroundings(
            self,
            terrain,
            &footprint,
            channel_parameters,
            &carve,
            &mut budgets,
        );
        smooth_river_corridor(
            self,
            terrain.mesh,
            terrain.adjacency,
            &footprint,
            channel_parameters,
            &carve,
        );
        carve_confluence_connectors(self, terrain, &footprint, channel_parameters, &mut budgets);
        self.reconcile_carved_confluence_profiles(terrain, &known_surfaces, &mut budgets);
        transfer_tributary_budgets(&self.rivers, &mut budgets);
        if form_deltas {
            self.deposit_deltas(terrain, &mut budgets);
        }
        finalize_river_budgets(&self.rivers, &mut budgets);
        if !form_deltas {
            self.enforce_gentle_final_profiles(terrain.mesh);
        }
    }

    /// Reapplies shared-node and confluence constraints after every river has
    /// been carved. Both operations are one-sided lowerings, so the matching
    /// bed vertices can follow the water profile without disturbing an
    /// already lower channel.
    fn reconcile_carved_confluence_profiles(
        &mut self,
        terrain: &mut RiverTerrain<'_>,
        known_surfaces: &HashMap<usize, f32>,
        budgets: &mut [RiverSedimentBudget],
    ) -> usize {
        let previous_surfaces = self
            .rivers
            .iter()
            .flat_map(|river| river.nodes.iter().map(|node| node.surface))
            .collect::<Vec<_>>();
        self.apply_known_surface_profiles(known_surfaces);
        self.reconcile_confluence_profiles();

        let mut targets = vec![None::<ConfluenceCarveTarget>; terrain.mesh.vertices.len()];
        for ((river_index, node), previous_surface) in self
            .rivers
            .iter()
            .enumerate()
            .flat_map(|(river_index, river)| {
                river.nodes.iter().map(move |node| (river_index, node))
            })
            .zip(previous_surfaces)
        {
            let lowering = previous_surface - node.surface;
            if lowering <= f32::EPSILON {
                continue;
            }
            let target = terrain.mesh.vertices[node.vertex].z - lowering;
            record_confluence_carve_target(&mut targets, node.vertex, target, river_index);
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

    fn apply_known_surface_profiles(&mut self, known_surfaces: &HashMap<usize, f32>) {
        for (river, waterfalls) in self.rivers.iter_mut().zip(&mut self.waterfalls) {
            for node_index in 0..river.nodes.len() {
                let vertex = river.nodes[node_index].vertex;
                let Some(&surface) = known_surfaces.get(&vertex) else {
                    continue;
                };
                lower_profile_reach_through_confluence(
                    &mut river.nodes,
                    waterfalls,
                    node_index,
                    surface,
                );
            }
        }
    }

    fn form_channel_rings(&self, mesh: &mut Mesh, adjacency: &Adjacency) {
        for _ in 0..CHANNEL_RING_SHAPING_PASSES {
            let footprint = build_river_footprint(self, mesh, adjacency, false);
            shape_channel_ring_vertices(self, mesh, &footprint);
        }
    }

    fn enforce_gentle_final_profiles(&mut self, mesh: &Mesh) {
        for ((river, waterfalls), sections) in self
            .rivers
            .iter()
            .zip(&mut self.waterfalls)
            .zip(&mut self.cross_sections)
        {
            enforce_gentle_river_profile(mesh, &river.nodes, waterfalls, sections);
        }
    }

    fn prepare_channel_profiles(
        &mut self,
        mesh: &Mesh,
        adjacency: &Adjacency,
        channel_parameters: RiverChannelParameters,
        waterfall_clearance: &WaterfallClearanceIndex,
        footprint: &RiverFootprint,
        rejected_waterfall_vertices: Option<&HashSet<usize>>,
    ) -> Vec<bool> {
        let mut scratch = RiverProfileScratch::default();
        let mut ocean_entries = vec![None; self.rivers.len()];
        for (river_index, ocean_entry) in ocean_entries.iter_mut().enumerate() {
            let environment = RiverProfileEnvironment {
                mesh,
                adjacency,
                ocean: &self.ocean,
            };
            let terminal_ocean = river_reaches_ocean(&self.rivers[river_index], &self.ocean);
            let join_vertex = self.join_vertices[river_index];
            let downstream_surface = self.rivers[river_index]
                .join
                .and_then(|join| self.rivers.get(join))
                .and_then(|river| {
                    river
                        .nodes
                        .iter()
                        .find(|node| Some(node.vertex) == join_vertex)
                })
                .map_or(f32::NEG_INFINITY, |node| node.surface);
            *ocean_entry = form_river_profile(
                environment,
                &mut self.rivers[river_index].nodes,
                &mut self.waterfalls[river_index],
                RiverCarveParameters {
                    downstream_surface,
                    terminal_ocean,
                    max_height: self.max_height,
                    max_flow: self.max_flow,
                    depth_multiplier: channel_parameters.depth_multiplier,
                    cross_sections: &self.cross_sections[river_index],
                },
                &mut scratch.gradients,
            );
        }
        // Join targets always have lower indices than their tributaries. Walk
        // upstream branches first so a later, lower sibling cannot invalidate
        // an already-reconciled confluence on their shared receiver.
        self.reconcile_confluence_profiles();

        let mut invalid = vec![false; self.rivers.len()];
        for (river_index, failed) in invalid.iter_mut().enumerate() {
            let profile_end = ocean_entries[river_index].map_or_else(
                || self.rivers[river_index].nodes.len().saturating_sub(1),
                |ocean_entry| ocean_entry.saturating_sub(1),
            );
            let site_environment =
                rejected_waterfall_vertices.map(|rejected| WaterfallSiteEnvironment {
                    adjacency,
                    coverage: &footprint.coverage,
                    ocean: &self.ocean,
                    perimeter: &self.perimeter,
                    rejected,
                });
            let waterfalls_valid = relocate_conflicting_waterfalls(
                mesh,
                &mut self.rivers[river_index].nodes,
                &mut self.waterfalls[river_index],
                profile_end,
                WaterfallRelocation {
                    clearance: waterfall_clearance,
                    site: site_environment,
                    river: river_index,
                },
                &self.cross_sections[river_index],
                &mut scratch.waterfall_drops,
            );
            self.river_mesh_ends[river_index] = ocean_entries[river_index].map(|ocean_entry| {
                river_mouth_transition(ocean_entry, &self.waterfalls[river_index]).river_mesh_end
            });
            *failed = !waterfalls_valid;
        }
        invalid
    }

    fn reconcile_confluence_profiles(&mut self) {
        for river_index in (0..self.rivers.len()).rev() {
            self.reconcile_confluence_profile(river_index);
        }
    }

    fn reconcile_confluence_profile(&mut self, mut incoming_river: usize) {
        loop {
            let Some(receiver) = self.rivers[incoming_river].join else {
                return;
            };
            let Some(join_vertex) = self.join_vertices[incoming_river] else {
                return;
            };
            let Some(incoming_terminal) = self.rivers[incoming_river].nodes.len().checked_sub(1)
            else {
                return;
            };
            let Some(receiver_join) = self.rivers[receiver]
                .nodes
                .iter()
                .position(|node| node.vertex == join_vertex)
            else {
                return;
            };
            let incoming_surface = self.rivers[incoming_river].nodes[incoming_terminal].surface;
            let receiver_surface = self.rivers[receiver].nodes[receiver_join].surface;
            let target_surface = incoming_surface.min(receiver_surface);
            lower_profile_reach_through_confluence(
                &mut self.rivers[incoming_river].nodes,
                &mut self.waterfalls[incoming_river],
                incoming_terminal,
                target_surface,
            );
            let reached_terminal = lower_profile_reach_through_confluence(
                &mut self.rivers[receiver].nodes,
                &mut self.waterfalls[receiver],
                receiver_join,
                target_surface,
            );
            if !reached_terminal {
                return;
            }
            incoming_river = receiver;
        }
    }

    fn carve_channels(
        &mut self,
        terrain: &mut RiverTerrain<'_>,
        channel_parameters: RiverChannelParameters,
        known_surfaces: &mut HashMap<usize, f32>,
        budgets: &mut [RiverSedimentBudget],
        carve_submerged_mouths: bool,
    ) {
        let mut scratch = RiverCarveScratch::new(terrain.mesh.vertices.len());
        for (river_index, budget_output) in budgets.iter_mut().enumerate() {
            let terminal_ocean = river_reaches_ocean(&self.rivers[river_index], &self.ocean);
            let join_vertex = self.join_vertices[river_index];
            let downstream_surface = self.rivers[river_index]
                .join
                .and_then(|join| self.rivers.get(join))
                .and_then(|river| {
                    river
                        .nodes
                        .iter()
                        .find(|node| Some(node.vertex) == join_vertex)
                })
                .map_or(f32::NEG_INFINITY, |node| node.surface);
            let nodes = &mut self.rivers[river_index].nodes;
            let cross_sections = &self.cross_sections[river_index];
            let result = shape_and_carve_river(
                terrain,
                nodes,
                &mut self.waterfalls[river_index],
                &mut scratch,
                &self.ocean,
                carve_submerged_mouths,
                RiverCarveParameters {
                    downstream_surface,
                    terminal_ocean,
                    max_height: self.max_height,
                    max_flow: self.max_flow,
                    depth_multiplier: channel_parameters.depth_multiplier,
                    cross_sections,
                },
            );
            self.river_mesh_ends[river_index] = result.river_mesh_end;
            for node in nodes {
                known_surfaces
                    .entry(node.vertex)
                    .and_modify(|value| *value = value.min(node.surface))
                    .or_insert(node.surface);
            }
            *budget_output = result.budget;
        }
    }

    fn deposit_deltas(&self, terrain: &mut RiverTerrain<'_>, budgets: &mut [RiverSedimentBudget]) {
        let edge_length = average_edge_length(terrain.mesh, terrain.adjacency).max(0.000_25);
        let mut scratch = DeltaScratch::new(terrain.mesh.vertices.len());
        for (river, budget) in self.rivers.iter().zip(budgets) {
            let terminal_ocean = budget.carried > 0.0 && river_reaches_ocean(river, &self.ocean);
            if terminal_ocean {
                create_delta(
                    terrain,
                    &river.nodes,
                    budget,
                    self.max_height,
                    edge_length,
                    &mut scratch,
                );
            }
        }
    }

    #[cfg(test)]
    fn build_mesh(
        &self,
        terrain: &mut Mesh,
        _adjacency: &Adjacency,
        material: &mut SurfaceMaterial,
    ) -> Mesh {
        self.build_mesh_with_mask(terrain, material, 0).river_mesh
    }

    fn build_mesh_with_mask(
        &self,
        terrain: &mut Mesh,
        material: &mut SurfaceMaterial,
        seed: u64,
    ) -> BuiltRiverGeometry {
        RiverGeometryBuilder::new(self, terrain, material, seed).build()
    }

    fn refresh(&mut self, mesh: &Mesh) {
        for river in &mut self.rivers {
            for node in &mut river.nodes {
                node.position = mesh.vertices[node.vertex];
            }
        }
    }

    fn refresh_after_vertical_displacement(&mut self, mesh: &Mesh) {
        for river in &mut self.rivers {
            for node in &mut river.nodes {
                let position = mesh.vertices[node.vertex];
                node.surface += position.z - node.position.z;
                node.position = position;
            }
        }
    }

    /// Removes rivers whose waterfalls cannot form a complete local barrier.
    /// A tributary cannot survive without its downstream river, so invalidity
    /// propagates upstream before the parallel network arrays are compacted.
    fn remove_invalid_rivers(&mut self, invalid: &[bool]) -> usize {
        debug_assert_eq!(invalid.len(), self.rivers.len());
        let mut removed = invalid.to_vec();
        loop {
            let mut changed = false;
            for (river_index, river) in self.rivers.iter().enumerate() {
                if !removed[river_index]
                    && river
                        .join
                        .is_some_and(|parent| removed.get(parent).copied().unwrap_or(true))
                {
                    removed[river_index] = true;
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }

        let removed_count = removed.iter().filter(|&&value| value).count();
        if removed_count == 0 {
            return 0;
        }
        let mut remap = vec![None; self.rivers.len()];
        let mut next = 0;
        for (old, &is_removed) in removed.iter().enumerate() {
            if !is_removed {
                remap[old] = Some(next);
                next += 1;
            }
        }

        let old_rivers = std::mem::take(&mut self.rivers);
        let old_join_vertices = std::mem::take(&mut self.join_vertices);
        let old_waterfalls = std::mem::take(&mut self.waterfalls);
        let old_mesh_ends = std::mem::take(&mut self.river_mesh_ends);
        let old_sections = std::mem::take(&mut self.cross_sections);
        self.rivers.reserve(next);
        self.join_vertices.reserve(next);
        self.waterfalls.reserve(next);
        self.river_mesh_ends.reserve(next);
        self.cross_sections.reserve(next);
        for (old, ((((mut river, join_vertex), waterfalls), mesh_end), sections)) in old_rivers
            .into_iter()
            .zip(old_join_vertices)
            .zip(old_waterfalls)
            .zip(old_mesh_ends)
            .zip(old_sections)
            .enumerate()
        {
            if removed[old] {
                continue;
            }
            river.join = river.join.and_then(|parent| remap[parent]);
            self.rivers.push(river);
            self.join_vertices.push(join_vertex);
            self.waterfalls.push(waterfalls);
            self.river_mesh_ends.push(mesh_end);
            self.cross_sections.push(sections);
        }
        self.max_flow = self
            .rivers
            .iter()
            .flat_map(|river| &river.nodes)
            .map(|node| node.flow)
            .max()
            .unwrap_or(1);
        removed_count
    }
}
