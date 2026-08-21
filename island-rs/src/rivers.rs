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
    Adjacency, ISLAND_WORLD_METRES, Mesh, Vec2, Vec3,
    terrain::{
        ProjectedFaceAreas, SurfaceMaterial, VertexFaceAdjacency, bedrock_erosion_rate,
        projected_vertex_control_areas,
    },
};

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
const WATERFALL_EDGE_SMOOTHING_PASSES: usize = 6;
const WATERFALL_EDGE_SMOOTHING: f32 = 0.5;
const WATERFALL_EDGE_BLEND_RUN: f32 = 2.0 * WATERFALL_TARGET_EDGE_LENGTH;
const WATERFALL_DEBUG_PLANE_MARGIN: f32 = 2.0 / ISLAND_WORLD_METRES;
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

    fn required_catchment(self, grade: f32, elevation: f32) -> f32 {
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

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct RiverMouth {
    pub(crate) position: Vec2,
    pub(crate) downstream: Vec2,
    pub(crate) flow: u32,
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
    ) -> Self {
        let perimeter = mesh.perimeter_mask();
        let ocean = fix_inland_seas(mesh, adjacency);
        let downstream = map_downstream(mesh, adjacency);
        let (flow, catchment_areas) = calculate_flow_and_catchment(mesh, &downstream);
        let sources = find_sources(mesh, adjacency, &downstream, &catchment_areas, source_rule);
        let (mut rivers, join_vertices) = trace_rivers(mesh, adjacency, &flow, &sources, &ocean);
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
    ) -> (
        Vec<River>,
        Mesh,
        Vec<bool>,
        Vec<RiverMouth>,
        Vec<usize>,
        RiverDebugGeometry,
    ) {
        let (river_mesh, river_bed, failed_waterfalls, debug_geometry) =
            self.build_mesh_with_mask(mesh, material);
        mesh.calculate_normals();
        self.refresh(mesh);
        let mouths = self.river_mouths();
        (
            self.rivers,
            river_mesh,
            river_bed,
            mouths,
            failed_waterfalls,
            debug_geometry,
        )
    }

    fn river_mouths(&self) -> Vec<RiverMouth> {
        let mut mouths = Vec::<RiverMouth>::new();
        for (river_index, river) in self.rivers.iter().enumerate() {
            if river.join.is_some() {
                continue;
            }
            let Some(path_terminal) = river
                .nodes
                .last()
                .filter(|node| self.ocean.get(node.vertex).copied().unwrap_or(false))
            else {
                continue;
            };
            let mouth_index = self.river_mesh_ends[river_index]
                .unwrap_or_else(|| river.nodes.len().saturating_sub(1))
                .min(river.nodes.len().saturating_sub(1));
            let terminal = &river.nodes[mouth_index];
            let position = terminal.position.truncate();
            let downstream = river
                .nodes
                .iter()
                .skip(mouth_index + 1)
                .map(|node| node.position.truncate() - position)
                .find_map(Vec2::try_normalize)
                .or_else(|| {
                    river.nodes[..mouth_index]
                        .iter()
                        .rev()
                        .map(|node| position - node.position.truncate())
                        .find_map(Vec2::try_normalize)
                });
            let Some(downstream) = downstream else {
                continue;
            };
            if !position.is_finite() || !downstream.is_finite() {
                continue;
            }

            if let Some(existing) = mouths
                .iter_mut()
                .find(|mouth| mouth.position.distance_squared(position) <= 1.0e-12)
            {
                if path_terminal.flow > existing.flow {
                    *existing = RiverMouth {
                        position,
                        downstream,
                        flow: path_terminal.flow,
                    };
                }
            } else {
                mouths.push(RiverMouth {
                    position,
                    downstream,
                    flow: path_terminal.flow,
                });
            }
        }
        mouths
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
        let channel_settings = options.channel_settings;
        let base_width = average_edge_length(mesh, adjacency).max(0.000_25);
        let control_areas = projected_vertex_control_areas(mesh);
        self.cross_sections = if options.form_deltas {
            self.rivers.iter().map(|_| Vec::new()).collect()
        } else {
            target_cross_sections(&self.rivers, channel_settings)
        };
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
                break;
            }
            self.remove_invalid_rivers(&invalid);
            if self.rivers.is_empty() {
                return;
            }
        }

        lower_precarve_river_valleys(self, mesh, adjacency);
        raise_precarve_waterfall_shoulders(self, mesh, adjacency);
        self.refresh_after_vertical_displacement(mesh);
        let depth_multiplier = 1.0 / (self.max_flow as f32).sqrt().max(1.0);
        if !options.form_deltas {
            let loose_volume = material.volume(mesh);
            self.form_channel_rings(mesh, adjacency);
            material.rescale_to_volume(mesh, loose_volume);
            let footprint = build_river_footprint(self, mesh, adjacency, false);
            update_achieved_cross_sections(self, mesh, &footprint, channel_settings.maximum_depth);
            self.enforce_gentle_final_profiles(mesh);
        }
        let mut terrain = RiverTerrain {
            mesh,
            adjacency,
            material,
            bedrock_rates,
            control_areas: &control_areas,
        };
        let mut known_surfaces = HashMap::<usize, f32>::new();
        let mut budgets = vec![RiverSedimentBudget::default(); self.rivers.len()];
        self.carve_channels(
            &mut terrain,
            RiverChannelParameters { depth_multiplier },
            &mut known_surfaces,
            &mut budgets,
        );
        let footprint = build_river_footprint(self, terrain.mesh, terrain.adjacency, false);
        let channel_parameters = RiverChannelParameters { depth_multiplier };
        let carve = carve_river_corridor(
            self,
            &mut terrain,
            &footprint,
            channel_parameters,
            &mut budgets,
        );
        lower_river_surroundings(
            self,
            &mut terrain,
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
        carve_confluence_connectors(
            self,
            &mut terrain,
            &footprint,
            channel_parameters,
            &mut budgets,
        );
        transfer_tributary_budgets(&self.rivers, &mut budgets);
        if options.form_deltas {
            self.deposit_deltas(&mut terrain, &mut budgets);
        }
        finalize_river_budgets(&self.rivers, &mut budgets);
        apply_known_surfaces(&mut self.rivers, &known_surfaces);
        if !options.form_deltas {
            self.enforce_gentle_final_profiles(terrain.mesh);
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
        let environment = RiverProfileEnvironment {
            mesh,
            adjacency,
            ocean: &self.ocean,
        };
        let site_environment =
            rejected_waterfall_vertices.map(|rejected| WaterfallSiteEnvironment {
                adjacency,
                coverage: &footprint.coverage,
                ocean: &self.ocean,
                perimeter: &self.perimeter,
                rejected,
            });
        let mut scratch = RiverProfileScratch::default();
        let mut invalid = vec![false; self.rivers.len()];
        for (river_index, failed) in invalid.iter_mut().enumerate() {
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
            let parameters = RiverCarveParameters {
                downstream_surface,
                terminal_ocean,
                max_height: self.max_height,
                max_flow: self.max_flow,
                depth_multiplier: channel_parameters.depth_multiplier,
                cross_sections: &self.cross_sections[river_index],
            };
            let (mouth, waterfalls_valid) = prepare_river_profile(
                environment,
                &mut self.rivers[river_index].nodes,
                &mut self.waterfalls[river_index],
                WaterfallRelocation {
                    clearance: waterfall_clearance,
                    site: site_environment,
                    river: river_index,
                },
                parameters,
                &mut scratch,
            );
            self.river_mesh_ends[river_index] = mouth.map(|mouth| mouth.river_mesh_end);
            *failed = !waterfalls_valid;
        }
        invalid
    }

    fn carve_channels(
        &mut self,
        terrain: &mut RiverTerrain<'_>,
        channel_parameters: RiverChannelParameters,
        known_surfaces: &mut HashMap<usize, f32>,
        budgets: &mut [RiverSedimentBudget],
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
        self.build_mesh_with_mask(terrain, material).0
    }

    #[allow(clippy::too_many_lines)]
    fn build_mesh_with_mask(
        &self,
        terrain: &mut Mesh,
        material: &mut SurfaceMaterial,
    ) -> (Mesh, Vec<bool>, Vec<usize>, RiverDebugGeometry) {
        let footprint = build_river_footprint(self, terrain, &terrain.adjacency(), true);
        let mut coverage = footprint.coverage;
        let RiverMeshAttributes {
            mut surfaces,
            uv: mut river_uv,
            mut owners,
            mut waterfall_lips,
            mut target_half_widths,
            mut target_depths,
        } = river_mesh_attributes(terrain, &footprint.owner);
        refine_river_corridor_mesh(
            terrain,
            material,
            &mut coverage,
            &mut surfaces,
            &mut river_uv,
            &mut owners,
            &mut waterfall_lips,
            &mut target_half_widths,
            &mut target_depths,
        );
        refine_river_terrain(
            terrain,
            material,
            &mut coverage,
            &mut surfaces,
            &mut river_uv,
            &mut waterfall_lips,
        );
        repair_sharp_terrain_points(
            terrain,
            material,
            &mut coverage,
            &mut surfaces,
            &mut river_uv,
            &mut owners,
            &mut waterfall_lips,
            &mut target_half_widths,
            &mut target_depths,
        );
        let waterfall_patches = derive_waterfall_patches(self, terrain);
        refine_waterfall_terrain(
            terrain,
            material,
            &waterfall_patches,
            &mut coverage,
            &mut surfaces,
            &mut river_uv,
            &mut owners,
            &mut waterfall_lips,
            &mut target_half_widths,
            &mut target_depths,
        );
        let waterfall_notches =
            recess_waterfall_notches(terrain, material, &waterfall_patches, &coverage);
        let mut waterfall_constraints = pin_waterfalls_to_terrain(
            terrain,
            material,
            &waterfall_patches,
            &waterfall_notches,
            &mut coverage,
            &mut surfaces,
            &mut waterfall_lips,
        );
        lift_river_banks_to_surface(
            terrain,
            &terrain.adjacency(),
            &coverage,
            &surfaces,
            &target_half_widths,
            RiverBankLiftMasks {
                ocean: &self.ocean,
                perimeter: &terrain.perimeter_mask(),
                protected: &waterfall_constraints.patch,
            },
        );
        enforce_sea_plane_clearance(terrain, &self.ocean);
        ensure_clear_river_channel(
            self,
            terrain,
            &coverage,
            &surfaces,
            &waterfall_lips,
            &waterfall_constraints.pinned,
            &waterfall_constraints.patch,
        );
        relax_refined_river_surface(
            terrain,
            &coverage,
            &mut surfaces,
            &river_uv,
            &target_half_widths,
            &target_depths,
            &waterfall_constraints,
        );
        enforce_sea_plane_clearance(terrain, &self.ocean);
        smooth_final_waterfall_patches(terrain, &mut surfaces, &waterfall_patches, &coverage);
        enforce_final_waterfall_edge_relationships(
            terrain,
            &mut surfaces,
            &waterfall_patches,
            &coverage,
            &owners,
            &mut waterfall_constraints,
        );
        rebuild_final_waterfall_support_mask(
            terrain,
            &waterfall_patches,
            &coverage,
            &owners,
            &mut waterfall_constraints,
        );
        let failed_waterfalls = if ENABLE_FINAL_WATERFALL_REJECTION {
            detect_failed_final_waterfalls(terrain, &waterfall_patches, &coverage)
        } else {
            Vec::new()
        };
        let (river_mesh, river_bed, river_bed_mesh) = finalize_river_geometry(
            terrain,
            &coverage,
            &surfaces,
            &river_uv,
            &waterfall_constraints,
        );
        let waterfall_face_terrain =
            build_waterfall_face_terrain_debug_mesh(terrain, &waterfall_constraints.patch);
        let waterfall_foot_planes =
            build_waterfall_debug_plane_mesh(terrain, &waterfall_patches, &coverage, 0.0);
        let waterfall_lip_planes = build_waterfall_debug_plane_mesh(
            terrain,
            &waterfall_patches,
            &coverage,
            -WaterfallPatch::face_run(),
        );
        (
            river_mesh,
            river_bed,
            failed_waterfalls,
            RiverDebugGeometry {
                river_bed: river_bed_mesh,
                waterfall_face_terrain,
                waterfall_foot_planes,
                waterfall_lip_planes,
            },
        )
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

fn lower_precarve_river_valleys(
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
struct WaterfallShoulderCandidate {
    vertex: usize,
    distance: u8,
    owner: RiverChannelFootprintOwner,
}

/// Builds an upper terrace beside each planned waterfall after the broad
/// valley lowering but before channel movement or carving. The channel itself
/// remains untouched: only dry vertices outside its one-ring footprint are
/// raised, with two full-height bank rings and one blended outer ring.
fn raise_precarve_waterfall_shoulders(
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

fn confluence_connector(
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

fn finalize_river_geometry(
    terrain: &mut Mesh,
    coverage: &[u8],
    surfaces: &[f32],
    river_uv: &[Vec2],
    waterfall_constraints: &WaterfallTerrainConstraints,
) -> (Mesh, Vec<bool>, Mesh) {
    // Keep the tessellated river topology fixed after carving and smoothing.
    // Flipping an edge here can connect vertices from opposite sides of the
    // settled channel profile, creating sharp bed ridges or accidental dams.
    terrain.calculate_normals();
    let river_bed = river_topology_masks(terrain, coverage).0;
    let river_bed_mesh = duplicate_river_bed_topology(terrain, coverage);
    let mut river_mesh =
        duplicate_river_topology(terrain, coverage, surfaces, river_uv, waterfall_constraints)
            .clipped_above(0.0);
    encode_bank_distance_in_uv(&mut river_mesh);
    (river_mesh, river_bed, river_bed_mesh)
}

fn duplicate_river_bed_topology(terrain: &Mesh, coverage: &[u8]) -> Mesh {
    let mut mapping = vec![u32::MAX; terrain.vertices.len()];
    let mut output = Mesh::default();

    for triangle in terrain.triangles.chunks_exact(3) {
        if !is_river_bed_triangle(triangle, coverage) {
            continue;
        }
        for &source in triangle {
            let source = source as usize;
            let mapped = if mapping[source] == u32::MAX {
                let mapped = output.vertices.len() as u32;
                mapping[source] = mapped;
                output.vertices.push(terrain.vertices[source]);
                output.normals.push(terrain.normals[source]);
                output.uv.push(
                    terrain
                        .uv
                        .get(source)
                        .copied()
                        .unwrap_or_else(|| terrain.vertices[source].truncate()),
                );
                mapped
            } else {
                mapping[source]
            };
            output.triangles.push(mapped);
        }
    }
    output
}

fn build_waterfall_face_terrain_debug_mesh(terrain: &Mesh, face_vertices: &[bool]) -> Mesh {
    debug_assert_eq!(terrain.vertices.len(), face_vertices.len());
    let mut mapping = vec![u32::MAX; terrain.vertices.len()];
    let mut output = Mesh::default();

    for triangle in terrain.triangles.chunks_exact(3) {
        if !triangle
            .iter()
            .any(|&vertex| face_vertices[vertex as usize])
        {
            continue;
        }

        for &source in triangle {
            let source = source as usize;
            let mapped = if mapping[source] == u32::MAX {
                let mapped = output.vertices.len() as u32;
                mapping[source] = mapped;
                output.vertices.push(terrain.vertices[source]);
                output.uv.push(
                    terrain
                        .uv
                        .get(source)
                        .copied()
                        .unwrap_or_else(|| terrain.vertices[source].truncate()),
                );
                mapped
            } else {
                mapping[source]
            };
            output.triangles.push(mapped);
        }
    }
    output.calculate_normals();
    output
}

fn build_waterfall_debug_plane_mesh(
    terrain: &Mesh,
    patches: &[WaterfallPatch],
    coverage: &[u8],
    along: f32,
) -> Mesh {
    if patches.is_empty() {
        return Mesh::default();
    }

    let adjacency = terrain.adjacency();
    let perimeter = terrain.perimeter_mask();
    let banks = waterfall_bank_mask(&adjacency, &perimeter, coverage);
    let mut output = Mesh::default();

    for &patch in patches {
        let selected =
            waterfall_side_bank_apron_for_patch(terrain, patch, coverage, &adjacency, &banks);
        let mut lateral_extent = patch.half_width * 1.25;
        let mut minimum_height = patch.lower_floor.min(patch.lower_surface);
        let mut maximum_height = patch.upper_surface;
        for (position, &is_selected) in terrain.vertices.iter().zip(&selected) {
            if !is_selected {
                continue;
            }
            let (_, lateral) = patch.local_coordinates(position.truncate());
            lateral_extent = lateral_extent.max(lateral.abs());
            minimum_height = minimum_height.min(position.z);
            maximum_height = maximum_height.max(position.z);
        }
        lateral_extent += WATERFALL_DEBUG_PLANE_MARGIN;
        minimum_height -= WATERFALL_DEBUG_PLANE_MARGIN;
        maximum_height += WATERFALL_DEBUG_PLANE_MARGIN;

        append_waterfall_debug_plane(
            &mut output,
            patch,
            along,
            lateral_extent,
            minimum_height,
            maximum_height,
        );
    }
    output.calculate_normals();
    output
}

fn append_waterfall_debug_plane(
    output: &mut Mesh,
    patch: WaterfallPatch,
    along: f32,
    lateral_extent: f32,
    minimum_height: f32,
    maximum_height: f32,
) {
    let base = output.vertices.len() as u32;
    output.vertices.extend([
        patch.face_plane_point(along, -lateral_extent, minimum_height),
        patch.face_plane_point(along, lateral_extent, minimum_height),
        patch.face_plane_point(along, -lateral_extent, maximum_height),
        patch.face_plane_point(along, lateral_extent, maximum_height),
    ]);
    output.uv.extend([
        Vec2::new(0.0, 0.0),
        Vec2::new(1.0, 0.0),
        Vec2::new(0.0, 1.0),
        Vec2::new(1.0, 1.0),
    ]);
    output
        .triangles
        .extend([base, base + 2, base + 1, base + 1, base + 2, base + 3]);
}

fn enforce_sea_plane_clearance(terrain: &mut Mesh, ocean: &[bool]) {
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

fn ensure_clear_river_channel(
    network: &RiverNetwork,
    terrain: &mut Mesh,
    coverage: &[u8],
    surfaces: &[f32],
    waterfall_lips: &[bool],
    waterfall_pinned: &[bool],
    waterfall_protected: &[bool],
) -> usize {
    debug_assert_eq!(terrain.vertices.len(), coverage.len());
    debug_assert_eq!(terrain.vertices.len(), surfaces.len());
    debug_assert_eq!(terrain.vertices.len(), waterfall_lips.len());
    debug_assert_eq!(terrain.vertices.len(), waterfall_pinned.len());
    debug_assert_eq!(terrain.vertices.len(), waterfall_protected.len());

    let adjacency = terrain.adjacency();
    let (under_river, banks) = river_topology_masks(terrain, coverage);
    let mut ceilings = vec![f32::INFINITY; terrain.vertices.len()];
    for vertex in 0..terrain.vertices.len() {
        if under_river[vertex]
            && !banks[vertex]
            && !waterfall_protected[vertex]
            && surfaces[vertex].is_finite()
        {
            ceilings[vertex] = surfaces[vertex] - RIVER_SURFACE_OFFSET;
        }
    }

    let mut centre_floors = vec![f32::INFINITY; terrain.vertices.len()];
    for (river_index, river) in network.rivers.iter().enumerate() {
        for (node_index, node) in river.nodes.iter().enumerate() {
            if node.vertex >= terrain.vertices.len()
                || banks[node.vertex]
                || !under_river[node.vertex]
                || waterfall_protected[node.vertex]
            {
                continue;
            }
            let required_depth = network
                .cross_sections
                .get(river_index)
                .and_then(|sections| sections.get(node_index))
                .map_or(RIVER_SURFACE_OFFSET, |section| {
                    section.required_depth.max(RIVER_SURFACE_OFFSET)
                });
            let floor = if waterfall_lips[node.vertex] || waterfall_pinned[node.vertex] {
                node.surface - RIVER_SURFACE_OFFSET
            } else {
                node.surface - required_depth
            };
            centre_floors[node.vertex] = centre_floors[node.vertex].min(floor);
            ceilings[node.vertex] = ceilings[node.vertex].min(floor);
        }
    }

    for (centre, &floor) in centre_floors.iter().enumerate() {
        if !floor.is_finite() {
            continue;
        }
        for &neighbour in &adjacency[centre] {
            if !under_river[neighbour]
                || banks[neighbour]
                || waterfall_lips[neighbour]
                || waterfall_pinned[neighbour]
                || waterfall_protected[neighbour]
                || !surfaces[neighbour].is_finite()
            {
                continue;
            }
            let core_ceiling = floor
                + (surfaces[neighbour] - RIVER_SURFACE_OFFSET - floor) * RIVER_CHANNEL_CORE_BLEND;
            ceilings[neighbour] = ceilings[neighbour].min(core_ceiling);
        }
    }

    let mut targets = terrain
        .vertices
        .iter()
        .enumerate()
        .map(|(vertex, position)| position.z.min(ceilings[vertex]))
        .collect::<Vec<_>>();
    let snapshot = targets.clone();
    for vertex in 0..terrain.vertices.len() {
        if !under_river[vertex]
            || banks[vertex]
            || waterfall_lips[vertex]
            || waterfall_pinned[vertex]
            || waterfall_protected[vertex]
            || !ceilings[vertex].is_finite()
        {
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

    let mut lowered = 0;
    for (position, target) in terrain.vertices.iter_mut().zip(targets) {
        if target < position.z {
            position.z = target;
            lowered += 1;
        }
    }
    lowered
}

#[allow(clippy::too_many_lines)]
fn relax_refined_river_surface(
    terrain: &mut Mesh,
    coverage: &[u8],
    surfaces: &mut [f32],
    river_uv: &[Vec2],
    target_half_widths: &[f32],
    target_depths: &[f32],
    waterfall: &WaterfallTerrainConstraints,
) -> usize {
    debug_assert_eq!(terrain.vertices.len(), coverage.len());
    debug_assert_eq!(terrain.vertices.len(), surfaces.len());
    debug_assert_eq!(terrain.vertices.len(), river_uv.len());
    debug_assert_eq!(terrain.vertices.len(), target_half_widths.len());
    debug_assert_eq!(terrain.vertices.len(), target_depths.len());
    debug_assert_eq!(terrain.vertices.len(), waterfall.patch.len());
    debug_assert_eq!(terrain.vertices.len(), waterfall.pinned.len());

    let adjacency = terrain.adjacency();
    let perimeter = terrain.perimeter_mask();
    let (_, banks) = river_topology_masks(terrain, coverage);
    let patch = river_corridor_apron_mask(&adjacency, coverage, RIVER_REFINEMENT_APRON_RINGS);
    let movable = patch
        .iter()
        .enumerate()
        .map(|(vertex, &selected)| {
            selected
                && !perimeter[vertex]
                && !banks[vertex]
                && !waterfall.patch[vertex]
                && !waterfall.pinned[vertex]
                && !adjacency[vertex].is_empty()
                && adjacency[vertex].iter().all(|&neighbour| patch[neighbour])
        })
        .collect::<Vec<_>>();
    let mut snapshot = terrain
        .vertices
        .iter()
        .map(|vertex| vertex.z)
        .collect::<Vec<_>>();
    let profile_targets = terrain
        .vertices
        .iter()
        .enumerate()
        .map(|(vertex, position)| {
            if waterfall.patch[vertex] {
                return Some(position.z);
            }
            let surface = surfaces[vertex];
            let half_width = target_half_widths[vertex];
            let depth = target_depths[vertex];
            if coverage[vertex] == 0
                || !surface.is_finite()
                || half_width <= f32::EPSILON
                || depth <= RIVER_SURFACE_OFFSET
            {
                return None;
            }
            let lateral = (river_uv[vertex].x.abs() / half_width).clamp(0.0, 1.0);
            let bank_progress = smoothstep(lateral);
            Some(depth.mul_add(bank_progress, surface - depth))
        })
        .collect::<Vec<_>>();
    let mut moved = vec![false; terrain.vertices.len()];

    for _ in 0..FINAL_RIVER_RELAXATION_PASSES {
        for vertex in 0..terrain.vertices.len() {
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
            if coverage[vertex] != 0
                && profile_targets[vertex].is_none()
                && !waterfall.patch[vertex]
            {
                target = target.min(snapshot[vertex]);
            }
            let ceiling = if coverage[vertex] != 0
                && !banks[vertex]
                && !waterfall.support[vertex]
                && surfaces[vertex].is_finite()
            {
                surfaces[vertex] - RIVER_SURFACE_OFFSET
            } else {
                f32::INFINITY
            };
            let bank_floor = if banks[vertex] && surfaces[vertex].is_finite() {
                surfaces[vertex]
            } else {
                f32::NEG_INFINITY
            };
            target = target.min(ceiling).max(bank_floor);
            if (target - terrain.vertices[vertex].z).abs() > f32::EPSILON {
                terrain.vertices[vertex].z = target;
                moved[vertex] = true;
            }
        }
        snapshot
            .iter_mut()
            .zip(&terrain.vertices)
            .for_each(|(height, vertex)| *height = vertex.z);
    }

    let relaxed = moved.into_iter().filter(|&was_moved| was_moved).count();
    let ceiling_restored =
        enforce_waterfall_downstream_ceiling(terrain, &waterfall.terrain_ceiling);
    relaxed
        + ceiling_restored
        + squish_waterfall_downstream_spikes(terrain, surfaces, &waterfall.water_unclamped)
}

#[allow(clippy::too_many_arguments)]
fn refine_river_corridor_mesh(
    terrain: &mut Mesh,
    material: &mut SurfaceMaterial,
    coverage: &mut Vec<u8>,
    surfaces: &mut Vec<f32>,
    river_uv: &mut Vec<Vec2>,
    owners: &mut Vec<Option<RiverOwnerKey>>,
    waterfall_lips: &mut Vec<bool>,
    target_half_widths: &mut Vec<f32>,
    target_depths: &mut Vec<f32>,
) -> usize {
    let mut added_vertices = 0;
    for _ in 0..RIVER_REFINEMENT_PASSES {
        let adjacency = terrain.adjacency();
        let edge_targets = river_refinement_edge_targets(
            &adjacency,
            coverage,
            target_half_widths,
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
        extend_river_attributes_after_tessellation(
            &stencils,
            coverage,
            surfaces,
            river_uv,
            owners,
            waterfall_lips,
            target_half_widths,
            target_depths,
        );
        added_vertices += stencils.len();

        for value in coverage.iter_mut() {
            *value &= !RIVER_BOUNDARY;
        }
        let adjacency = terrain.adjacency();
        let perimeter = terrain.perimeter_mask();
        mark_river_boundary(&adjacency, &perimeter, coverage);
    }
    added_vertices
}

fn river_refinement_edge_targets(
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

#[allow(clippy::too_many_arguments)]
fn extend_river_attributes_after_tessellation(
    stencils: &[crate::mesh::NewVertexStencil],
    coverage: &mut Vec<u8>,
    surfaces: &mut Vec<f32>,
    river_uv: &mut Vec<Vec2>,
    owners: &mut Vec<Option<RiverOwnerKey>>,
    waterfall_lips: &mut Vec<bool>,
    target_half_widths: &mut Vec<f32>,
    target_depths: &mut Vec<f32>,
) {
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

fn interpolated_river_owner(
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

fn river_corridor_apron_mask(adjacency: &Adjacency, coverage: &[u8], apron_rings: u8) -> Vec<bool> {
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
struct RiverBankLiftMasks<'a> {
    ocean: &'a [bool],
    perimeter: &'a [bool],
    protected: &'a [bool],
}

fn lift_river_banks_to_surface(
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
    coverage: &mut [u8],
    surfaces: &mut [f32],
    _river_uv: &mut [Vec2],
    _waterfall_lips: &mut [bool],
) {
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

#[allow(clippy::too_many_arguments)]
fn repair_sharp_terrain_points(
    terrain: &mut Mesh,
    material: &mut SurfaceMaterial,
    coverage: &mut Vec<u8>,
    surfaces: &mut Vec<f32>,
    river_uv: &mut Vec<Vec2>,
    owners: &mut Vec<Option<RiverOwnerKey>>,
    waterfall_lips: &mut Vec<bool>,
    target_half_widths: &mut Vec<f32>,
    target_depths: &mut Vec<f32>,
) {
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

fn sharp_point_mask(terrain: &Mesh, adjacency: &Adjacency, perimeter: &[bool]) -> Vec<bool> {
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

fn smooth_sharp_point_patch(
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

fn river_reaches_ocean(river: &River, ocean: &[bool]) -> bool {
    river.join.is_none() && river.nodes.last().is_some_and(|node| ocean[node.vertex])
}

fn transfer_tributary_budgets(rivers: &[River], budgets: &mut [RiverSedimentBudget]) {
    for tributary in (0..rivers.len()).rev() {
        let Some(join) = rivers[tributary].join else {
            continue;
        };
        let upstream = std::mem::take(&mut budgets[tributary]);
        budgets[join].absorb(upstream);
    }
}

fn finalize_river_budgets(rivers: &[River], budgets: &mut [RiverSedimentBudget]) {
    for (river, budget) in rivers.iter().zip(budgets) {
        if river.join.is_none() {
            budget.export_remaining();
            debug_assert!(budget.is_balanced());
        } else {
            debug_assert_eq!(budget.carried.to_bits(), 0.0_f64.to_bits());
        }
    }
}

fn apply_known_surfaces(rivers: &mut [River], known_surfaces: &HashMap<usize, f32>) {
    for river in rivers {
        for node in &mut river.nodes {
            if let Some(surface) = known_surfaces.get(&node.vertex) {
                node.surface = node.surface.min(*surface);
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct RiverOwnerKey {
    river: u32,
    node: u32,
}

#[derive(Clone, Copy, Debug)]
struct RiverChannelFootprintOwner {
    key: RiverOwnerKey,
    surface: f32,
    floor_override: Option<f32>,
    flow_origin: Vec2,
    flow_direction: Vec2,
    distance_along: f32,
    target_half_width: f32,
    target_depth: f32,
    ring_count: u8,
    waterfall_lip: bool,
}

struct RiverMeshAttributes {
    surfaces: Vec<f32>,
    uv: Vec<Vec2>,
    owners: Vec<Option<RiverOwnerKey>>,
    waterfall_lips: Vec<bool>,
    target_half_widths: Vec<f32>,
    target_depths: Vec<f32>,
}

fn river_mesh_attributes(
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
struct RiverFootprint {
    coverage: Vec<u8>,
    ring_distance: Vec<u8>,
    owner: Vec<Option<RiverChannelFootprintOwner>>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct RiverDebugGeometry {
    pub(crate) river_bed: Mesh,
    pub(crate) waterfall_face_terrain: Mesh,
    pub(crate) waterfall_foot_planes: Mesh,
    pub(crate) waterfall_lip_planes: Mesh,
}

#[derive(Clone, Copy, Debug)]
struct PlungePool {
    centre: Vec2,
    downstream_radius: f32,
    lateral_radius: f32,
    depth: f32,
}

#[derive(Clone, Copy, Debug)]
struct WaterfallPatch {
    river: u32,
    segment: u32,
    upper_vertex: usize,
    upper_centre: Vec2,
    direction: Vec2,
    across: Vec2,
    upper_surface: f32,
    lower_surface: f32,
    lower_floor: f32,
    half_width: f32,
    support_run: f32,
    pool: Option<PlungePool>,
}

#[derive(Debug)]
struct WaterfallTerrainConstraints {
    patch: Vec<bool>,
    pinned: Vec<bool>,
    support: Vec<bool>,
    water_unclamped: Vec<bool>,
    terrain_ceiling: Vec<f32>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WaterfallPlaneZone {
    BeforeLip,
    Face,
    AfterFoot,
}

impl WaterfallPatch {
    fn face_run() -> f32 {
        2.0 * WATERFALL_TARGET_EDGE_LENGTH
    }

    fn signed_distance_to_face_plane(self, point: Vec2) -> f32 {
        (point - self.upper_centre).dot(self.direction)
    }

    fn plane_zone(self, point: Vec2) -> WaterfallPlaneZone {
        let along = self.signed_distance_to_face_plane(point);
        if along < -Self::face_run() - f32::EPSILON {
            WaterfallPlaneZone::BeforeLip
        } else if along > f32::EPSILON {
            WaterfallPlaneZone::AfterFoot
        } else {
            WaterfallPlaneZone::Face
        }
    }

    fn edge_normal_blend(self, point: Vec2) -> f32 {
        let along = self.signed_distance_to_face_plane(point);
        match self.plane_zone(point) {
            WaterfallPlaneZone::BeforeLip => (-Self::face_run() - along) / WATERFALL_EDGE_BLEND_RUN,
            WaterfallPlaneZone::Face => 0.0,
            WaterfallPlaneZone::AfterFoot => 1.0,
        }
        .clamp(0.0, 1.0)
    }

    fn local_coordinates(self, point: Vec2) -> (f32, f32) {
        let offset = point - self.upper_centre;
        (
            self.signed_distance_to_face_plane(point),
            offset.dot(self.across),
        )
    }

    fn face_plane_point(self, along: f32, lateral: f32, height: f32) -> Vec3 {
        (self.upper_centre + self.direction * along + self.across * lateral).extend(height)
    }

    fn downstream_extent(self) -> f32 {
        let landing = self.support_run + self.half_width * WATERFALL_LANDING_LENGTH_MULTIPLIER;
        self.pool.map_or(landing, |pool| {
            landing
                .max((pool.centre - self.upper_centre).dot(self.direction) + pool.downstream_radius)
        })
    }

    fn lateral_extent(self) -> f32 {
        self.pool.map_or(self.half_width * 1.25, |pool| {
            (self.half_width * 1.25).max(pool.lateral_radius)
        })
    }

    fn contains_refinement_point(self, point: Vec2) -> bool {
        let (along, lateral) = self.local_coordinates(point);
        along >= -2.0 * WATERFALL_TARGET_EDGE_LENGTH
            && along <= self.downstream_extent()
            && lateral.abs() <= self.lateral_extent()
    }

    fn contains_face_point(self, point: Vec2) -> bool {
        let (along, lateral) = self.local_coordinates(point);
        (-Self::face_run()..=f32::EPSILON).contains(&along)
            && lateral.abs() <= self.half_width * 1.25
    }

    fn contains_face_smoothing_band(self, point: Vec2) -> bool {
        let (along, _) = self.local_coordinates(point);
        (-3.0 * WATERFALL_TARGET_EDGE_LENGTH..=f32::EPSILON).contains(&along)
    }

    fn contains_face_flow_band(self, point: Vec2) -> bool {
        let (along, _) = self.local_coordinates(point);
        (-3.0 * WATERFALL_TARGET_EDGE_LENGTH..=WATERFALL_TARGET_EDGE_LENGTH).contains(&along)
    }

    fn contains_side_constraint_band(self, point: Vec2) -> bool {
        let (along, _) = self.local_coordinates(point);
        (-3.0 * WATERFALL_TARGET_EDGE_LENGTH..=self.downstream_extent()).contains(&along)
    }

    fn contains_downstream_point(self, point: Vec2) -> bool {
        let (along, lateral) = self.local_coordinates(point);
        along > f32::EPSILON
            && along <= self.downstream_extent()
            && lateral.abs() <= self.lateral_extent()
    }

    fn pool_depth_at(self, point: Vec2) -> f32 {
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
    fn face_surface_at(self, point: Vec2) -> Option<f32> {
        let (along, _) = self.local_coordinates(point);
        if along > f32::EPSILON {
            return None;
        }
        let face_run = Self::face_run();
        let progress = smoothstep((along + face_run) / face_run);
        Some((self.lower_surface - self.upper_surface).mul_add(progress, self.upper_surface))
    }
}

struct WaterfallPatchIndex<'a> {
    patches: &'a [WaterfallPatch],
    cells: HashMap<(i32, i32), Vec<usize>>,
}

impl<'a> WaterfallPatchIndex<'a> {
    const CELL_SIZE: f32 = 16.0 / ISLAND_WORLD_METRES;

    fn new(patches: &'a [WaterfallPatch]) -> Self {
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

    fn candidates(&self, point: Vec2) -> impl Iterator<Item = &'a WaterfallPatch> + '_ {
        self.cells
            .get(&Self::cell(point))
            .into_iter()
            .flatten()
            .map(|&index| &self.patches[index])
    }

    fn cell(point: Vec2) -> (i32, i32) {
        (
            (point.x / Self::CELL_SIZE).floor() as i32,
            (point.y / Self::CELL_SIZE).floor() as i32,
        )
    }
}

fn derive_waterfall_patches(network: &RiverNetwork, terrain: &Mesh) -> Vec<WaterfallPatch> {
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

#[allow(clippy::too_many_arguments)]
fn refine_waterfall_terrain(
    terrain: &mut Mesh,
    material: &mut SurfaceMaterial,
    patches: &[WaterfallPatch],
    coverage: &mut Vec<u8>,
    surfaces: &mut Vec<f32>,
    river_uv: &mut Vec<Vec2>,
    owners: &mut Vec<Option<RiverOwnerKey>>,
    waterfall_lips: &mut Vec<bool>,
    target_half_widths: &mut Vec<f32>,
    target_depths: &mut Vec<f32>,
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
        extend_river_attributes_after_tessellation(
            &stencils,
            coverage,
            surfaces,
            river_uv,
            owners,
            waterfall_lips,
            target_half_widths,
            target_depths,
        );
        added += stencils.len();
    }
    added += tessellate_final_waterfall_faces(
        terrain,
        material,
        patches,
        coverage,
        surfaces,
        river_uv,
        owners,
        waterfall_lips,
        target_half_widths,
        target_depths,
    );
    for value in coverage.iter_mut() {
        *value &= !RIVER_BOUNDARY;
    }
    mark_river_boundary(&terrain.adjacency(), &terrain.perimeter_mask(), coverage);
    added
}

/// Adds one unconditional detail tier to each final waterfall face, expanding
/// laterally through as many river rings as necessary to reach both banks and
/// one topology ring beyond them. The flow-aligned band prevents that
/// traversal from following the river away from the fall; triangles outside
/// the bank apron are only conformingly stitched.
#[allow(clippy::too_many_arguments)]
fn tessellate_final_waterfall_faces(
    terrain: &mut Mesh,
    material: &mut SurfaceMaterial,
    patches: &[WaterfallPatch],
    coverage: &mut Vec<u8>,
    surfaces: &mut Vec<f32>,
    river_uv: &mut Vec<Vec2>,
    owners: &mut Vec<Option<RiverOwnerKey>>,
    waterfall_lips: &mut Vec<bool>,
    target_half_widths: &mut Vec<f32>,
    target_depths: &mut Vec<f32>,
) -> usize {
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
    let mut marked = vec![false; terrain.vertices.len()];
    for patch in patches {
        let face = terrain
            .vertices
            .iter()
            .map(|position| patch.contains_face_point(position.truncate()))
            .collect::<Vec<_>>();
        let flow_band = terrain
            .vertices
            .iter()
            .map(|position| patch.contains_face_flow_band(position.truncate()))
            .collect::<Vec<_>>();
        let eligible = coverage
            .iter()
            .zip(&flow_band)
            .map(|(&remaining, &inside_band)| remaining != 0 && inside_band)
            .collect::<Vec<_>>();
        let face_to_bank_apron =
            expand_vertex_mask_to_banks(&adjacency, &face, &eligible, &flow_band, &banks);
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
    extend_river_attributes_after_tessellation(
        &stencils,
        coverage,
        surfaces,
        river_uv,
        owners,
        waterfall_lips,
        target_half_widths,
        target_depths,
    );
    stencils.len()
}

fn expand_vertex_mask_to_banks(
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

fn expand_vertex_mask_through_river_to_banks(
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

fn smoothstep(progress: f32) -> f32 {
    let progress = progress.clamp(0.0, 1.0);
    progress * progress * (3.0 - 2.0 * progress)
}

/// Recesses a complete bank-to-bank cross-section around the flow-perpendicular
/// waterfall plane. The cut follows covered river topology rather than a fixed
/// lateral radius, so refined or unusually broad channels cannot bypass one
/// side of the fall. Downstream vertices return to ordinary river smoothing.
fn recess_waterfall_notches(
    terrain: &mut Mesh,
    material: &mut SurfaceMaterial,
    patches: &[WaterfallPatch],
    coverage: &[u8],
) -> Vec<Option<usize>> {
    debug_assert_eq!(terrain.vertices.len(), coverage.len());
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
    let mut owners = vec![None::<usize>; terrain.vertices.len()];
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
            .map(|(position, &remaining)| {
                remaining != 0 && patch.contains_face_point(position.truncate())
            })
            .collect::<Vec<_>>();
        seeds[patch.upper_vertex] = true;
        let eligible = terrain
            .vertices
            .iter()
            .zip(coverage)
            .map(|(position, &remaining)| {
                remaining != 0 && patch.contains_face_flow_band(position.truncate())
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
            if owners[vertex].is_none() || target < targets[vertex] {
                owners[vertex] = Some(patch_index);
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
            if owners[vertex].is_none() || target < targets[vertex] {
                owners[vertex] = Some(patch_index);
                targets[vertex] = targets[vertex].min(target);
            }
        }
    }

    for (vertex, owner) in owners.iter().enumerate() {
        if owner.is_none() {
            continue;
        }
        terrain.vertices[vertex].z = targets[vertex];
        material.depths_mut()[vertex] = 0.0;
    }

    owners
}

/// Applies the final unconstrained relaxation to each tessellated waterfall,
/// through the river banks and their first dry-side ring. Terrain XYZ moves
/// toward the complete one-ring average, then covered water is derived from
/// the adjusted terrain with the normal waterfall clearance. Vertices outside
/// the patch anchor the blend. This intentionally runs after every carve, pin,
/// bank lift, and normal river relaxation stage.
fn smooth_final_waterfall_patches(
    terrain: &mut Mesh,
    surfaces: &mut [f32],
    patches: &[WaterfallPatch],
    coverage: &[u8],
) -> usize {
    debug_assert_eq!(terrain.vertices.len(), surfaces.len());
    debug_assert_eq!(terrain.vertices.len(), coverage.len());
    if patches.is_empty() {
        return 0;
    }

    let adjacency = terrain.adjacency();
    let perimeter = terrain.perimeter_mask();
    let selected =
        waterfall_face_bank_apron_mask(terrain, patches, coverage, &adjacency, &perimeter);
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
    let zones = classify_waterfall_vertices(terrain, patches, coverage, &adjacency, &banks);
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
struct WaterfallVertexPlaneZone {
    patch: usize,
    zone: WaterfallPlaneZone,
}

fn classify_waterfall_vertices(
    terrain: &Mesh,
    patches: &[WaterfallPatch],
    coverage: &[u8],
    adjacency: &Adjacency,
    banks: &[bool],
) -> Vec<Option<WaterfallVertexPlaneZone>> {
    let mut zones = vec![None::<WaterfallVertexPlaneZone>; terrain.vertices.len()];
    for (patch_index, &patch) in patches.iter().enumerate() {
        let selected =
            waterfall_side_bank_apron_for_patch(terrain, patch, coverage, adjacency, banks);
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
fn enforce_final_waterfall_edge_relationships(
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
    let zones = classify_waterfall_vertices(terrain, patches, coverage, &adjacency, &banks);
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

#[derive(Clone, Copy, Debug)]
struct WaterfallChannelLevels {
    lip: f32,
    bottom: f32,
}

#[derive(Debug)]
struct WaterfallReachConstraints {
    constrained: Vec<bool>,
    downstream_ceiling: Vec<f32>,
    normal_blend: Vec<f32>,
}

#[derive(Clone, Copy)]
struct WaterfallReachEnvironment<'a> {
    terrain: &'a Mesh,
    patches: &'a [WaterfallPatch],
    levels: &'a [WaterfallChannelLevels],
    coverage: &'a [u8],
    owners: &'a [Option<RiverOwnerKey>],
}

fn final_waterfall_channel_levels(
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

fn enforce_waterfall_reach_surface_levels(
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
            match patch.plane_zone(point) {
                WaterfallPlaneZone::BeforeLip => {
                    next = Some(patch_index);
                    break;
                }
                WaterfallPlaneZone::Face => {
                    on_face = true;
                    break;
                }
                WaterfallPlaneZone::AfterFoot => previous = Some(patch_index),
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

fn waterfall_side_bank_apron_for_patch(
    terrain: &Mesh,
    patch: WaterfallPatch,
    coverage: &[u8],
    adjacency: &Adjacency,
    banks: &[bool],
) -> Vec<bool> {
    let seeds = terrain
        .vertices
        .iter()
        .zip(coverage)
        .map(|(position, &remaining)| {
            remaining != 0 && patch.contains_face_point(position.truncate())
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
        .map(|(&remaining, &inside_band)| remaining != 0 && inside_band)
        .collect::<Vec<_>>();
    expand_vertex_mask_to_banks(adjacency, &seeds, &eligible, &constraint_band, banks)
}

/// Rebuilds waterfall support from the final positions and the exact lip/foot
/// planes. Only the middle zone is terrain-pinned; vertices before the lip or
/// after the foot return to ordinary river-water handling.
fn rebuild_final_waterfall_support_mask(
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
            patch.river == owner.river && patch.plane_zone(point) == WaterfallPlaneZone::Face
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

fn waterfall_face_bank_apron_mask(
    terrain: &Mesh,
    patches: &[WaterfallPatch],
    coverage: &[u8],
    adjacency: &Adjacency,
    perimeter: &[bool],
) -> Vec<bool> {
    let banks = waterfall_bank_mask(adjacency, perimeter, coverage);
    let mut selected = vec![false; terrain.vertices.len()];

    for patch in patches {
        let face_to_bank_apron =
            waterfall_face_bank_apron_for_patch(terrain, *patch, coverage, adjacency, &banks);
        selected
            .iter_mut()
            .zip(face_to_bank_apron)
            .for_each(|(included, candidate)| *included |= candidate);
    }
    selected
}

fn waterfall_bank_mask(adjacency: &Adjacency, perimeter: &[bool], coverage: &[u8]) -> Vec<bool> {
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

fn waterfall_face_bank_apron_for_patch(
    terrain: &Mesh,
    patch: WaterfallPatch,
    coverage: &[u8],
    adjacency: &Adjacency,
    banks: &[bool],
) -> Vec<bool> {
    let face = terrain
        .vertices
        .iter()
        .zip(coverage)
        .map(|(position, &remaining)| {
            remaining != 0 && patch.contains_face_point(position.truncate())
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
        .map(|(&remaining, &inside_band)| remaining != 0 && inside_band)
        .collect::<Vec<_>>();
    expand_vertex_mask_to_banks(adjacency, &face, &eligible, &smoothing_band, banks)
}

/// Detects the characteristic failed final waterfall where smoothing has
/// dragged an upstream bank vertex down toward the lower terrace. This runs
/// after every terrain refinement and reprojection pass, so the caller can
/// reject the exact site and regenerate from an untouched LOD 0 snapshot.
fn detect_failed_final_waterfalls(
    terrain: &Mesh,
    patches: &[WaterfallPatch],
    coverage: &[u8],
) -> Vec<usize> {
    let adjacency = terrain.adjacency();
    let perimeter = terrain.perimeter_mask();
    let banks = waterfall_bank_mask(&adjacency, &perimeter, coverage);
    let mut failed = Vec::new();

    for &patch in patches {
        let drop = patch.upper_surface - patch.lower_surface;
        if drop <= RIVER_SURFACE_OFFSET {
            continue;
        }
        let apron =
            waterfall_face_bank_apron_for_patch(terrain, patch, coverage, &adjacency, &banks);
        let low_bank_ceiling =
            drop.mul_add(-WATERFALL_FINAL_BANK_DROP_FRACTION, patch.upper_surface);
        let minimum_edge_drop = drop * WATERFALL_FINAL_BANK_EDGE_DROP_FRACTION;
        let malformed = terrain
            .vertices
            .iter()
            .enumerate()
            .any(|(vertex, position)| {
                if !banks[vertex] || !apron[vertex] || position.z > low_bank_ceiling {
                    return false;
                }
                let (along, _) = patch.local_coordinates(position.truncate());
                if !(-3.0 * WATERFALL_TARGET_EDGE_LENGTH..=-WATERFALL_TARGET_EDGE_LENGTH * 0.5)
                    .contains(&along)
                {
                    return false;
                }
                adjacency[vertex].iter().any(|&neighbour| {
                    let neighbour_position = terrain.vertices[neighbour];
                    let (neighbour_along, _) =
                        patch.local_coordinates(neighbour_position.truncate());
                    neighbour_along <= WATERFALL_TARGET_EDGE_LENGTH * 0.25
                        && neighbour_position.z - position.z >= minimum_edge_drop
                })
            });
        if malformed {
            failed.push(patch.upper_vertex);
        }
    }
    failed.sort_unstable();
    failed.dedup();
    failed
}

/// Pins the complete bank-to-bank upstream waterfall face. Downstream vertices
/// retain their ordinary river heights and smoothing eligibility; their water
/// is left at the hydraulic surface so local terrain projections pierce it
/// rather than lifting the sheet into spikes.
fn pin_waterfalls_to_terrain(
    terrain: &Mesh,
    material: &mut SurfaceMaterial,
    patches: &[WaterfallPatch],
    notch_owners: &[Option<usize>],
    coverage: &mut [u8],
    surfaces: &mut [f32],
    waterfall_lips: &mut [bool],
) -> WaterfallTerrainConstraints {
    debug_assert_eq!(terrain.vertices.len(), notch_owners.len());
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

    for (vertex, (&remaining, position)) in coverage.iter().zip(&terrain.vertices).enumerate() {
        if remaining == 0 {
            continue;
        }
        let lower_surface = patches
            .iter()
            .filter(|patch| patch.contains_downstream_point(position.truncate()))
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
fn enforce_waterfall_downstream_ceiling(terrain: &mut Mesh, ceilings: &[f32]) -> usize {
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
fn squish_waterfall_downstream_spikes(
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

#[derive(Clone, Copy, Debug)]
struct RiverMeshCandidate {
    remaining: u8,
    distance: u8,
    vertex: usize,
    owner: RiverChannelFootprintOwner,
}

fn build_river_footprint(
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

fn river_footprint_dimensions(
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
fn seed_confluence_footprints(
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

fn river_candidate_wins(
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

fn target_cross_sections(
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

fn shape_channel_ring_vertices(
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

fn update_achieved_cross_sections(
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

fn compensated_channel_depth(
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
struct RiverCorridorCarve {
    original_heights: Vec<f32>,
    lowered: Vec<bool>,
}

fn carve_river_corridor(
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

fn river_floor(
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

fn river_node_floor(
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
struct RiverValleyCandidate {
    vertex: usize,
    distance: u8,
    owner: RiverChannelFootprintOwner,
}

fn lower_river_surroundings(
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

fn smooth_river_corridor(
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
struct ConfluenceCarveTarget {
    height: f32,
    river: usize,
}

/// Cuts the short centreline gap left when two river footprints touch before
/// their traced centreline vertices meet. This runs after ordinary corridor
/// smoothing so the join cannot be rebuilt into a ridge.
fn carve_confluence_connectors(
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

fn record_confluence_carve_target(
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
fn log_cross_section_diagnostics(
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
const fn log_cross_section_diagnostics(
    _unresolved: usize,
    _maximum_applied_depth: f32,
    _mean_applied_depth: f64,
    _maximum_achieved_width: f32,
    _mean_achieved_width: f64,
) {
}

type CrossSectionSample = Option<(f32, f32)>;

fn cross_section_samples(
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

fn local_path_spacing(mesh: &Mesh, nodes: &[RiverNode], index: usize) -> f32 {
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

fn fill_missing_widths(widths: &mut [Option<f32>]) {
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

fn river_centreline_mask(network: &RiverNetwork, vertex_count: usize) -> Vec<bool> {
    let mut centreline = vec![false; vertex_count];
    for node in network.rivers.iter().flat_map(|river| &river.nodes) {
        centreline[node.vertex] = true;
    }
    centreline
}

fn river_node_water_position(terrain: &Mesh, node: &RiverNode) -> Vec3 {
    let position = terrain.vertices[node.vertex];
    Vec3::new(position.x, position.y, node.surface)
}

fn river_node_flow_direction(terrain: &Mesh, nodes: &[RiverNode], index: usize) -> Vec2 {
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

fn river_ring_count(flow: u32, max_flow: u32) -> u8 {
    (river_half_width(flow, max_flow, 1.0).ceil() as u8).min(MAX_RIVER_RINGS)
}

fn river_half_width(flow: u32, max_flow: u32, base_width: f32) -> f32 {
    let normalized_flow = (flow as f32 / max_flow.max(1) as f32).sqrt();
    base_width * normalized_flow.mul_add(1.9, 0.58)
}

fn mark_river_boundary(adjacency: &Adjacency, perimeter: &[bool], coverage: &mut [u8]) {
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

fn is_river_boundary(coverage: u8) -> bool {
    coverage & RIVER_BOUNDARY != 0
}

fn river_topology_masks(terrain: &Mesh, coverage: &[u8]) -> (Vec<bool>, Vec<bool>) {
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

fn is_river_bed_triangle(triangle: &[u32], coverage: &[u8]) -> bool {
    triangle
        .iter()
        .all(|&vertex| coverage[vertex as usize] != 0)
        && !triangle
            .iter()
            .all(|&vertex| is_river_boundary(coverage[vertex as usize]))
}

fn duplicate_river_topology(
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
fn round_waterfall_lips(
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

fn apply_averaged(mesh: &mut Mesh, accumulated: &[Vec3], count: &[u32], perimeter: &[bool]) {
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

fn fix_inland_seas(mesh: &mut Mesh, adjacency: &Adjacency) -> Vec<bool> {
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

fn map_downstream(mesh: &Mesh, adjacency: &Adjacency) -> Vec<usize> {
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

fn downhill_slope(mesh: &Mesh, from: usize, to: usize) -> f32 {
    let distance = (mesh.vertices[from].truncate() - mesh.vertices[to].truncate())
        .length()
        .max(f32::EPSILON);
    (mesh.vertices[from].z - mesh.vertices[to].z) / distance
}

fn calculate_flow_and_catchment(mesh: &Mesh, downstream: &[usize]) -> (Vec<u32>, Vec<f32>) {
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

fn find_sources(
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

fn source_grade(mesh: &Mesh, from: usize, to: usize) -> f32 {
    if from == to {
        return 0.0;
    }
    let edge = mesh.vertices[from] - mesh.vertices[to];
    (edge.z.max(0.0) / edge.length().max(f32::EPSILON)).clamp(0.0, 1.0)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct TracedFootprintOwner {
    river: usize,
    node: usize,
    centre: usize,
    distance: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct RiverFlowMerge {
    river: usize,
    join_vertex: usize,
    incoming_flow: u32,
}

/// Indexes the adjacency rings that will become river surface, not merely the
/// centreline. This makes confluences agree with the topology later duplicated
/// by `build_mesh_with_mask`.
#[derive(Debug)]
struct RiverFootprintIndex {
    owners: Vec<Option<TracedFootprintOwner>>,
    visited: Vec<u32>,
    stamp: u32,
    frontier: Vec<(usize, u8)>,
}

impl RiverFootprintIndex {
    fn new(vertex_count: usize) -> Self {
        Self {
            owners: vec![None; vertex_count],
            visited: vec![0; vertex_count],
            stamp: 0,
            frontier: Vec::new(),
        }
    }

    fn touching(
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

    fn register_river(
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

    fn register_node(&mut self, owner: TracedFootprintOwner, adjacency: &Adjacency, rings: u8) {
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

    fn begin(&mut self, centre: usize) {
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
struct TracedRiverPath {
    vertices: Vec<usize>,
    join: Option<usize>,
    join_vertex: Option<usize>,
}

/// Coarse world-space occupancy used only to reject later river sources near
/// an accepted river. `Vec<bool>` keeps the fixed grid compact and avoids a
/// hash lookup in the source loop.
#[derive(Debug)]
struct RiverSourceExclusionGrid {
    minimum_cell_x: i32,
    minimum_cell_y: i32,
    width: usize,
    height: usize,
    occupied: Vec<bool>,
}

impl RiverSourceExclusionGrid {
    fn new(vertices: &[Vec3]) -> Self {
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

    fn contains(&self, position: Vec3) -> bool {
        let (cell_x, cell_y) = Self::world_cell(position, Self::cell_size());
        self.index(cell_x, cell_y)
            .is_some_and(|index| self.occupied[index])
    }

    fn reserve_path(&mut self, mesh: &Mesh, path: &[usize]) {
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

    const fn cell_size() -> f32 {
        RIVER_SOURCE_EXCLUSION_CELL_METRES / ISLAND_WORLD_METRES
    }

    fn world_cell(position: Vec3, cell_size: f32) -> (i32, i32) {
        (
            (position.x / cell_size).floor() as i32,
            (position.y / cell_size).floor() as i32,
        )
    }

    fn index(&self, cell_x: i32, cell_y: i32) -> Option<usize> {
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

struct RiverPathTracer<'a> {
    mesh: &'a mut Mesh,
    adjacency: &'a Adjacency,
    flow: &'a [u32],
    ocean: &'a [bool],
    occupied: &'a HashMap<usize, usize>,
    footprints: &'a mut RiverFootprintIndex,
    max_flow: u32,
}

/// Detects when a growing centreline returns within its own future channel
/// footprint. Exact vertex repetition is already rejected by `seen`, but that
/// is insufficient once a river spans several adjacency rings.
struct RiverSelfContactIndex {
    node_at: Vec<usize>,
    visited: Vec<u32>,
    stamp: u32,
    frontier: Vec<(usize, u8)>,
}

impl RiverSelfContactIndex {
    fn new(vertex_count: usize) -> Self {
        Self {
            node_at: vec![usize::MAX; vertex_count],
            visited: vec![0; vertex_count],
            stamp: 0,
            frontier: Vec::new(),
        }
    }

    fn register(&mut self, vertex: usize, node: usize) {
        self.node_at[vertex] = node;
    }

    fn touches_earlier(
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
    fn trace(&mut self, source: usize) -> TracedRiverPath {
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

fn trace_rivers(
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

fn merge_flow_into_river(
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

fn update_join_flow_chain(rivers: &mut [River], join_vertices: &[Option<usize>], tributary: usize) {
    let incoming_flow = rivers[tributary].nodes.last().map_or(0, |node| node.flow);
    if let (Some(join), Some(target)) = (rivers[tributary].join, join_vertices[tributary]) {
        update_join_flow_from(rivers, join_vertices, join, target, incoming_flow);
    }
}

fn update_join_flow_from(
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

fn register_join_chain(
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

fn update_join_flows(rivers: &mut [River], join_vertices: &[Option<usize>]) {
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
struct RouteState {
    cost: f32,
    vertex: usize,
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

fn escape_sink(
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
struct RiverClearanceNode {
    river: usize,
    position: Vec2,
    half_width: f32,
}

#[derive(Debug)]
struct WaterfallClearanceIndex {
    nodes: Vec<RiverClearanceNode>,
    base_width: f32,
    max_flow: u32,
}

impl WaterfallClearanceIndex {
    fn new(rivers: &[River], mesh: &Mesh, max_flow: u32, base_width: f32) -> Self {
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

    fn conflicts(&self, river: usize, mesh: &Mesh, nodes: &[RiverNode], segment: usize) -> bool {
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

fn point_segment_distance_squared(point: Vec2, start: Vec2, end: Vec2) -> f32 {
    let segment = end - start;
    let length_squared = segment.length_squared();
    if length_squared <= f32::EPSILON {
        return point.distance_squared(start);
    }
    let progress = ((point - start).dot(segment) / length_squared).clamp(0.0, 1.0);
    point.distance_squared(segment.mul_add(Vec2::splat(progress), start))
}

#[derive(Clone, Copy, Debug)]
struct WaterfallDrop {
    segment: usize,
    height: f32,
    placed: bool,
}

struct RiverTerrain<'a> {
    mesh: &'a mut Mesh,
    adjacency: &'a Adjacency,
    material: &'a mut SurfaceMaterial,
    bedrock_rates: &'a [f32],
    control_areas: &'a [f32],
}

impl RiverTerrain<'_> {
    fn lower_vertex_exactly(
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

    fn carve_vertex(
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

    fn deposit_vertex(
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
struct RiverChannelParameters {
    depth_multiplier: f32,
}

#[derive(Debug)]
struct RiverCarveScratch {
    gradients: Vec<f32>,
}

#[derive(Clone, Copy, Debug)]
struct RiverProfileEnvironment<'a> {
    mesh: &'a Mesh,
    adjacency: &'a Adjacency,
    ocean: &'a [bool],
}

#[derive(Debug, Default)]
struct RiverProfileScratch {
    gradients: Vec<f32>,
    waterfall_drops: Vec<WaterfallDrop>,
}

#[derive(Clone, Copy, Debug)]
struct WaterfallSiteEnvironment<'a> {
    adjacency: &'a Adjacency,
    coverage: &'a [u8],
    ocean: &'a [bool],
    perimeter: &'a [bool],
    rejected: &'a HashSet<usize>,
}

impl RiverCarveScratch {
    fn new(_vertex_count: usize) -> Self {
        Self {
            gradients: Vec::new(),
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct WaterfallRelocation<'a> {
    clearance: &'a WaterfallClearanceIndex,
    site: Option<WaterfallSiteEnvironment<'a>>,
    river: usize,
}

#[derive(Clone, Copy, Debug)]
struct RiverCarveOptions<'a> {
    form_deltas: bool,
    channel_settings: RiverChannelSettings,
    rejected_waterfall_vertices: &'a HashSet<usize>,
}

#[derive(Clone, Copy, Debug)]
struct RiverCarveParameters<'a> {
    downstream_surface: f32,
    terminal_ocean: bool,
    max_height: f32,
    max_flow: u32,
    depth_multiplier: f32,
    cross_sections: &'a [RiverCrossSection],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct RiverMouthTransition {
    waterfall_segment: Option<usize>,
    river_mesh_end: usize,
}

#[derive(Clone, Copy, Debug, Default)]
struct RiverCarveResult {
    budget: RiverSedimentBudget,
    river_mesh_end: Option<usize>,
}

fn shape_and_carve_river(
    terrain: &mut RiverTerrain<'_>,
    nodes: &mut [RiverNode],
    waterfalls: &mut [bool],
    scratch: &mut RiverCarveScratch,
    ocean: &[bool],
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
    if let Some(mouth) = mouth {
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
        river_mesh_end: mouth.map(|mouth| mouth.river_mesh_end),
    }
}

fn prepare_river_profile(
    environment: RiverProfileEnvironment<'_>,
    nodes: &mut [RiverNode],
    waterfalls: &mut [bool],
    waterfall_relocation: WaterfallRelocation<'_>,
    parameters: RiverCarveParameters<'_>,
    scratch: &mut RiverProfileScratch,
) -> (Option<RiverMouthTransition>, bool) {
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
        &mut scratch.gradients,
    );
    let waterfalls_valid = relocate_conflicting_waterfalls(
        environment.mesh,
        nodes,
        waterfalls,
        profile_end,
        waterfall_relocation,
        parameters.cross_sections,
        &mut scratch.waterfall_drops,
    );
    (
        ocean_entry.map(|ocean_entry| river_mouth_transition(ocean_entry, waterfalls)),
        waterfalls_valid,
    )
}

fn river_depth(
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

fn unfitted_river_depth(
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

fn carve_stepped_bed(
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

fn carve_bed_reach(
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

fn form_stepped_profile(
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

fn enforce_gentle_river_profile(
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
fn level_confluence_reach(
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

fn valid_waterfall_site(
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

fn planned_waterfall_patch(
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

fn complete_waterfall_face(
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

fn waterfall_face_has_side_bypass(
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

fn relocate_conflicting_waterfalls(
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

fn river_ocean_entry(nodes: &[RiverNode], ocean: &[bool]) -> Option<usize> {
    nodes
        .iter()
        .position(|node| ocean.get(node.vertex).copied().unwrap_or(false))
}

fn river_mouth_transition(ocean_entry: usize, waterfalls: &[bool]) -> RiverMouthTransition {
    let waterfall_segment = waterfalls[..ocean_entry.min(waterfalls.len())]
        .iter()
        .rposition(|&waterfall| waterfall);
    RiverMouthTransition {
        waterfall_segment,
        river_mesh_end: waterfall_segment.map_or(0, |segment| segment + 1),
    }
}

fn carve_submerged_river_mouth(
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
struct DeltaCandidate {
    priority: f32,
    distance: f32,
    vertex: usize,
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
struct DeltaScratch {
    visited: Vec<u32>,
    channel: Vec<u32>,
    stamp: u32,
    frontier: BinaryHeap<DeltaCandidate>,
}

impl DeltaScratch {
    fn new(vertex_count: usize) -> Self {
        Self {
            visited: vec![0; vertex_count],
            channel: vec![0; vertex_count],
            stamp: 0,
            frontier: BinaryHeap::new(),
        }
    }

    fn begin(&mut self, nodes: &[RiverNode]) {
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

    fn visit(&mut self, vertex: usize) -> bool {
        if self.visited[vertex] == self.stamp {
            return false;
        }
        self.visited[vertex] = self.stamp;
        true
    }

    fn is_channel(&self, vertex: usize) -> bool {
        self.channel[vertex] == self.stamp
    }
}

fn create_delta(
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

fn create_alluvial_valley(
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

fn average_edge_length(mesh: &Mesh, adjacency: &Adjacency) -> f32 {
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

fn local_edge_length(mesh: &Mesh, adjacency: &Adjacency, vertex: usize) -> f32 {
    let neighbours = &adjacency[vertex];
    let total = neighbours
        .iter()
        .map(|&neighbour| {
            (mesh.vertices[vertex].truncate() - mesh.vertices[neighbour].truncate()).length()
        })
        .sum::<f32>();
    total / neighbours.len().max(1) as f32
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Vec2;

    #[test]
    fn waterfall_debug_plane_is_one_quad_on_the_classification_plane() {
        let patch = WaterfallPatch {
            river: 0,
            segment: 0,
            upper_vertex: 0,
            upper_centre: Vec2::new(0.4, 0.6),
            direction: Vec2::X,
            across: Vec2::Y,
            upper_surface: 0.3,
            lower_surface: 0.1,
            lower_floor: 0.08,
            half_width: 0.02,
            support_run: 0.03,
            pool: None,
        };
        let mut mesh = Mesh::default();
        append_waterfall_debug_plane(&mut mesh, patch, 0.0, 0.05, 0.04, 0.34);

        assert_eq!(mesh.vertices.len(), 4);
        assert_eq!(mesh.triangles.len(), 6);
        assert!(
            mesh.vertices
                .iter()
                .all(
                    |vertex| patch.signed_distance_to_face_plane(vertex.truncate()).abs() < 1.0e-6
                )
        );
        assert!(mesh.vertices.iter().any(|vertex| {
            ((vertex.truncate() - patch.upper_centre).dot(patch.across) + 0.05).abs() < 1.0e-6
                && (vertex.z - 0.04).abs() < 1.0e-6
        }));
        assert!(mesh.vertices.iter().any(|vertex| {
            ((vertex.truncate() - patch.upper_centre).dot(patch.across) - 0.05).abs() < 1.0e-6
                && (vertex.z - 0.34).abs() < 1.0e-6
        }));

        let mut lip = Mesh::default();
        append_waterfall_debug_plane(
            &mut lip,
            patch,
            -WaterfallPatch::face_run(),
            0.05,
            0.04,
            0.34,
        );
        assert!(lip.vertices.iter().all(|vertex| {
            (patch.signed_distance_to_face_plane(vertex.truncate()) + WaterfallPatch::face_run())
                .abs()
                < 1.0e-6
        }));
        assert_eq!(
            patch.plane_zone(patch.upper_centre - patch.direction * WaterfallPatch::face_run()),
            WaterfallPlaneZone::Face
        );
        assert_eq!(
            patch.plane_zone(patch.upper_centre),
            WaterfallPlaneZone::Face
        );
        assert_eq!(
            patch.plane_zone(
                patch.upper_centre
                    - patch.direction * (WaterfallPatch::face_run() + WATERFALL_TARGET_EDGE_LENGTH)
            ),
            WaterfallPlaneZone::BeforeLip
        );
        assert_eq!(
            patch.plane_zone(patch.upper_centre + patch.direction * WATERFALL_TARGET_EDGE_LENGTH),
            WaterfallPlaneZone::AfterFoot
        );
    }

    #[test]
    fn river_bed_debug_mesh_contains_only_selected_bed_triangles() {
        let mut terrain = Mesh {
            vertices: vec![
                Vec3::new(0.0, 0.0, 0.0),
                Vec3::new(1.0, 0.0, 0.0),
                Vec3::new(0.0, 1.0, 0.0),
                Vec3::new(1.0, 1.0, 0.0),
            ],
            triangles: vec![0, 1, 2, 1, 3, 2],
            ..Mesh::default()
        };
        terrain.calculate_normals();
        let debug_mesh = duplicate_river_bed_topology(&terrain, &[2, 2, 2, 0]);

        assert_eq!(debug_mesh.vertices.len(), 3);
        assert_eq!(debug_mesh.triangles, vec![0, 1, 2]);
        assert_eq!(debug_mesh.normals.len(), debug_mesh.vertices.len());
    }

    #[test]
    fn waterfall_face_terrain_debug_mesh_uses_final_face_vertex_mask() {
        let run = WaterfallPatch::face_run();
        let terrain = Mesh {
            vertices: vec![
                Vec3::new(-run * 2.0, -1.0, 0.4),
                Vec3::new(run, -1.0, 0.1),
                Vec3::new(run, 1.0, 0.1),
                Vec3::new(-run * 2.0, 1.0, 0.4),
            ],
            triangles: vec![0, 1, 3, 3, 1, 2],
            ..Mesh::default()
        };
        let debug_mesh =
            build_waterfall_face_terrain_debug_mesh(&terrain, &[false, true, false, false]);

        assert!(!debug_mesh.triangles.is_empty());
        assert_eq!(debug_mesh.normals.len(), debug_mesh.vertices.len());
        assert_eq!(debug_mesh.triangles.len(), terrain.triangles.len());
        assert_eq!(debug_mesh.vertices.len(), terrain.vertices.len());
        assert!(
            terrain
                .vertices
                .iter()
                .all(|vertex| debug_mesh.vertices.contains(vertex))
        );
    }

    fn build_test_river_mesh(
        network: &RiverNetwork,
        terrain: &mut Mesh,
        adjacency: &Adjacency,
    ) -> Mesh {
        let mut material = SurfaceMaterial::empty(terrain.vertices.len());
        network.build_mesh(terrain, adjacency, &mut material)
    }

    fn test_river_terrain<'a>(
        mesh: &'a mut Mesh,
        adjacency: &'a Adjacency,
        material: &'a mut SurfaceMaterial,
        bedrock_rates: &'a [f32],
        control_areas: &'a [f32],
    ) -> RiverTerrain<'a> {
        RiverTerrain {
            mesh,
            adjacency,
            material,
            bedrock_rates,
            control_areas,
        }
    }

    #[test]
    fn pre_carve_valley_gently_lowers_several_rings_around_the_course() {
        let points = (0..3)
            .flat_map(|y| (0..=12).map(move |x| Vec2::new(x as f32, y as f32)))
            .collect::<Vec<_>>();
        let mut triangles = Vec::new();
        for y in 0..2 {
            for x in 0..12 {
                let lower_left = (y * 13 + x) as u32;
                let lower_right = lower_left + 1;
                let upper_left = lower_left + 13;
                let upper_right = upper_left + 1;
                triangles.extend([
                    lower_left,
                    lower_right,
                    upper_left,
                    upper_left,
                    lower_right,
                    upper_right,
                ]);
            }
        }
        let mut mesh = Mesh {
            vertices: points.iter().map(|point| point.extend(1.0)).collect(),
            triangles,
            ..Mesh::default()
        };
        let original = mesh.clone();
        let adjacency = mesh.adjacency();
        let vertex_at = |mesh: &Mesh, x: f32, y: f32| {
            mesh.vertices
                .iter()
                .position(|vertex| vertex.truncate() == Vec2::new(x, y))
                .unwrap()
        };
        let nodes = (0..3)
            .map(|y| {
                let vertex = vertex_at(&mesh, 6.0, y as f32);
                RiverNode {
                    vertex,
                    flow: 1,
                    surface: 1.0,
                    position: mesh.vertices[vertex],
                }
            })
            .collect::<Vec<_>>();
        let network = RiverNetwork {
            rivers: vec![River { nodes, join: None }],
            join_vertices: vec![None],
            waterfalls: vec![vec![false; 3]],
            river_mesh_ends: vec![None],
            max_flow: 1,
            max_height: 1.0,
            ocean: vec![false; mesh.vertices.len()],
            perimeter: vec![false; mesh.vertices.len()],
            cross_sections: vec![Vec::new()],
        };

        assert!(lower_precarve_river_valleys(&network, &mut mesh, &adjacency) > 0);

        let depths = (6..=10)
            .map(|x| 1.0 - mesh.vertices[vertex_at(&mesh, x as f32, 1.0)].z)
            .collect::<Vec<_>>();
        assert!((depths[0] - PRECARVE_VALLEY_CENTRE_DEPTH).abs() < f32::EPSILON);
        assert!(depths.windows(2).all(|pair| pair[0] > pair[1]));
        assert!(depths[3] > 0.0);
        assert_eq!(depths[4].to_bits(), 0.0_f32.to_bits());
        assert_eq!(mesh.triangles, original.triangles);
        assert!(
            mesh.vertices
                .iter()
                .zip(original.vertices)
                .all(
                    |(lowered, initial)| lowered.truncate() == initial.truncate()
                        && lowered.z <= initial.z
                )
        );
    }

    #[test]
    fn pre_carve_waterfall_shoulders_raise_low_banks_without_moving_the_channel() {
        let points = (0..=10)
            .flat_map(|y| (0..=8).map(move |x| Vec2::new(x as f32, y as f32)))
            .collect::<Vec<_>>();
        let mut triangles = Vec::new();
        for y in 0..10 {
            for x in 0..8 {
                let lower_left = (y * 9 + x) as u32;
                let lower_right = lower_left + 1;
                let upper_left = lower_left + 9;
                let upper_right = upper_left + 1;
                triangles.extend([
                    lower_left,
                    lower_right,
                    upper_left,
                    upper_left,
                    lower_right,
                    upper_right,
                ]);
            }
        }
        let mut mesh = Mesh {
            vertices: points.iter().map(|point| point.extend(1.0)).collect(),
            triangles,
            ..Mesh::default()
        };
        let vertex_at = |x: f32, y: f32| {
            mesh.vertices
                .iter()
                .position(|vertex| vertex.truncate() == Vec2::new(x, y))
                .unwrap()
        };
        let upper = vertex_at(3.0, 5.0);
        let lower = vertex_at(4.0, 5.0);
        let bank = vertex_at(3.0, 3.0);
        let second_bank_ring = vertex_at(3.0, 2.0);
        let blended_outer_ring = vertex_at(3.0, 1.0);
        let downstream_land = vertex_at(5.0, 3.0);
        for vertex in [bank, second_bank_ring, blended_outer_ring, downstream_land] {
            mesh.vertices[vertex].z = 0.1;
        }
        let adjacency = mesh.adjacency();
        let node = |vertex, surface| RiverNode {
            vertex,
            flow: 1,
            surface,
            position: mesh.vertices[vertex],
        };
        let network = RiverNetwork {
            rivers: vec![River {
                nodes: vec![node(upper, 0.8), node(lower, 0.4)],
                join: None,
            }],
            join_vertices: vec![None],
            waterfalls: vec![vec![true, false]],
            river_mesh_ends: vec![None],
            max_flow: 1,
            max_height: 1.0,
            ocean: vec![false; mesh.vertices.len()],
            perimeter: mesh.perimeter_mask(),
            cross_sections: vec![vec![
                RiverCrossSection {
                    target_half_width: 0.5,
                    ..RiverCrossSection::default()
                };
                2
            ]],
        };

        lower_precarve_river_valleys(&network, &mut mesh, &adjacency);
        let channel_heights = [mesh.vertices[upper].z, mesh.vertices[lower].z];
        let bank_before = mesh.vertices[bank].z;
        let outer_before = mesh.vertices[blended_outer_ring].z;
        let downstream_before = mesh.vertices[downstream_land].z;
        let lip_height = network.rivers[0].nodes[0].surface + mesh.vertices[upper].z
            - network.rivers[0].nodes[0].position.z;

        assert!(raise_precarve_waterfall_shoulders(&network, &mut mesh, &adjacency) > 0);

        assert_eq!(
            [mesh.vertices[upper].z, mesh.vertices[lower].z],
            channel_heights
        );
        assert!((mesh.vertices[bank].z - lip_height).abs() < f32::EPSILON);
        assert!((mesh.vertices[second_bank_ring].z - lip_height).abs() < f32::EPSILON);
        assert!(mesh.vertices[bank].z > bank_before);
        assert!(mesh.vertices[blended_outer_ring].z > outer_before);
        assert!(mesh.vertices[blended_outer_ring].z < lip_height);
        assert_eq!(
            mesh.vertices[downstream_land].z.to_bits(),
            downstream_before.to_bits()
        );
    }

    #[test]
    fn final_one_ring_footprint_bridges_an_early_confluence_gap() {
        let points = (0..3)
            .flat_map(|y| (0..=6).map(move |x| Vec2::new(x as f32, y as f32)))
            .collect::<Vec<_>>();
        let mut triangles = Vec::new();
        for y in 0..2 {
            for x in 0..6 {
                let lower_left = (y * 7 + x) as u32;
                let lower_right = lower_left + 1;
                let upper_left = lower_left + 7;
                let upper_right = upper_left + 1;
                triangles.extend([
                    lower_left,
                    lower_right,
                    upper_left,
                    upper_left,
                    lower_right,
                    upper_right,
                ]);
            }
        }
        let mesh = Mesh {
            vertices: points.iter().map(|point| point.extend(1.0)).collect(),
            triangles,
            ..Mesh::default()
        };
        let adjacency = mesh.adjacency();
        let vertex_at = |x: f32, y: f32| {
            mesh.vertices
                .iter()
                .position(|vertex| vertex.truncate() == Vec2::new(x, y))
                .unwrap()
        };
        let join = vertex_at(5.0, 1.0);
        let terminal = vertex_at(1.0, 1.0);
        let node = |vertex, surface| RiverNode {
            vertex,
            flow: 1,
            surface,
            position: mesh.vertices[vertex],
        };
        let network = RiverNetwork {
            rivers: vec![
                River {
                    nodes: vec![node(join, 0.4), node(vertex_at(6.0, 1.0), 0.3)],
                    join: None,
                },
                River {
                    nodes: vec![node(vertex_at(0.0, 1.0), 0.8), node(terminal, 0.4)],
                    join: Some(0),
                },
            ],
            join_vertices: vec![None, Some(join)],
            waterfalls: vec![vec![false; 2]; 2],
            river_mesh_ends: vec![None; 2],
            max_flow: 1,
            max_height: 1.0,
            ocean: vec![false; mesh.vertices.len()],
            perimeter: mesh.perimeter_mask(),
            cross_sections: vec![
                vec![
                    RiverCrossSection {
                        target_half_width: 0.4,
                        required_depth: 0.2,
                        ..RiverCrossSection::default()
                    };
                    2
                ],
                vec![
                    RiverCrossSection {
                        target_half_width: 0.3,
                        required_depth: 0.1,
                        ..RiverCrossSection::default()
                    };
                    2
                ],
            ],
        };

        let path = confluence_connector(&network, &adjacency, terminal, join);
        assert!(path.len() > 3);
        let footprint = build_river_footprint(&network, &mesh, &adjacency, true);

        assert!(path.iter().all(|&vertex| footprint.coverage[vertex] != 0));
        assert!(
            path.iter()
                .skip(1)
                .take(path.len() - 2)
                .all(|&vertex| footprint.ring_distance[vertex] == 0)
        );
        let middle = footprint.owner[path[path.len() / 2]].unwrap();
        assert!(middle.floor_override.is_some());
        assert!(middle.target_half_width > 0.3 && middle.target_half_width < 0.4);
    }

    #[test]
    fn pre_carve_valley_connects_touching_river_centrelines_without_a_dam() {
        let points = (0..3)
            .flat_map(|y| (0..=6).map(move |x| Vec2::new(x as f32, y as f32)))
            .collect::<Vec<_>>();
        let mut triangles = Vec::new();
        for y in 0..2 {
            for x in 0..6 {
                let lower_left = (y * 7 + x) as u32;
                let lower_right = lower_left + 1;
                let upper_left = lower_left + 7;
                let upper_right = upper_left + 1;
                triangles.extend([
                    lower_left,
                    lower_right,
                    upper_left,
                    upper_left,
                    lower_right,
                    upper_right,
                ]);
            }
        }
        let mut mesh = Mesh {
            vertices: points.iter().map(|point| point.extend(1.0)).collect(),
            triangles,
            ..Mesh::default()
        };
        let adjacency = mesh.adjacency();
        let vertex_at = |x: f32, y: f32| {
            mesh.vertices
                .iter()
                .position(|vertex| vertex.truncate() == Vec2::new(x, y))
                .unwrap()
        };
        let join = vertex_at(5.0, 1.0);
        let tributary_terminal = vertex_at(1.0, 1.0);
        let node = |vertex| RiverNode {
            vertex,
            flow: 1,
            surface: 1.0,
            position: mesh.vertices[vertex],
        };
        let network = RiverNetwork {
            rivers: vec![
                River {
                    nodes: vec![node(join), node(vertex_at(6.0, 1.0))],
                    join: None,
                },
                River {
                    nodes: vec![node(vertex_at(0.0, 1.0)), node(tributary_terminal)],
                    join: Some(0),
                },
            ],
            join_vertices: vec![None, Some(join)],
            waterfalls: vec![vec![false; 2]; 2],
            river_mesh_ends: vec![None; 2],
            max_flow: 1,
            max_height: 1.0,
            ocean: vec![false; mesh.vertices.len()],
            perimeter: vec![false; mesh.vertices.len()],
            cross_sections: vec![
                vec![
                    RiverCrossSection {
                        required_depth: 0.1,
                        ..RiverCrossSection::default()
                    };
                    2
                ];
                2
            ],
        };

        lower_precarve_river_valleys(&network, &mut mesh, &adjacency);

        let centre_height = 1.0 - PRECARVE_VALLEY_CENTRE_DEPTH;
        let mut reached = vec![false; mesh.vertices.len()];
        let mut pending = VecDeque::from([tributary_terminal]);
        reached[tributary_terminal] = true;
        while let Some(vertex) = pending.pop_front() {
            for &neighbour in &adjacency[vertex] {
                if !reached[neighbour] && mesh.vertices[neighbour].z <= centre_height + f32::EPSILON
                {
                    reached[neighbour] = true;
                    pending.push_back(neighbour);
                }
            }
        }
        assert!(reached[join]);

        // Corridor smoothing and the two independently owned river footprints
        // can leave this short connector raised again. The final confluence
        // pass must cut the whole centreline through to the interpolated river
        // floors, not merely lower a broad valley around it.
        let path = confluence_connector(&network, &adjacency, tributary_terminal, join);
        for &vertex in &path {
            mesh.vertices[vertex].z = 1.2;
        }
        let footprint = RiverFootprint {
            coverage: vec![2; mesh.vertices.len()],
            ring_distance: vec![0; mesh.vertices.len()],
            owner: vec![None; mesh.vertices.len()],
        };
        let mut material = SurfaceMaterial::empty(mesh.vertices.len());
        let bedrock_rates = vec![1.0; mesh.vertices.len()];
        let control_areas = projected_vertex_control_areas(&mesh);
        let mut budgets = vec![RiverSedimentBudget::default(); 2];
        let mut terrain = test_river_terrain(
            &mut mesh,
            &adjacency,
            &mut material,
            &bedrock_rates,
            &control_areas,
        );

        assert!(
            carve_confluence_connectors(
                &network,
                &mut terrain,
                &footprint,
                RiverChannelParameters {
                    depth_multiplier: 1.0,
                },
                &mut budgets,
            ) >= path.len()
        );
        assert!(
            path.iter()
                .all(|&vertex| terrain.mesh.vertices[vertex].z <= 0.9 + f32::EPSILON)
        );
    }

    #[test]
    fn river_uv_u_is_shortest_mesh_distance_from_the_bank() {
        let points: Vec<Vec2> = (0..=2)
            .flat_map(|y| (0..=2).map(move |x| Vec2::new(x as f32 * 0.5, y as f32 * 0.5)))
            .collect();
        let mut mesh = Mesh::delaunay(&points);
        mesh.uv = mesh
            .vertices
            .iter()
            .map(|vertex| Vec2::new(-1.0, vertex.y + 3.0))
            .collect();
        let downstream = mesh.uv.iter().map(|uv| uv.y).collect::<Vec<_>>();
        let perimeter = mesh.perimeter_mask();
        let centre = mesh
            .vertices
            .iter()
            .position(|vertex| vertex.truncate() == Vec2::splat(0.5))
            .unwrap();

        encode_bank_distance_in_uv(&mut mesh);

        for (vertex, &is_bank) in perimeter.iter().enumerate() {
            if is_bank {
                assert_eq!(mesh.uv[vertex].x.to_bits(), 0.0_f32.to_bits());
            }
            assert_eq!(mesh.uv[vertex].y.to_bits(), downstream[vertex].to_bits());
        }
        assert!((mesh.uv[centre].x - 0.5).abs() < 1.0e-6);
    }

    #[test]
    fn source_cutoff_is_an_absolute_world_space_area() {
        let rule = RiverSourceRule::new(0.5, 1.0, 0.0, 0.2);

        assert_eq!(
            rule.required_catchment(0.0, 0.0).to_bits(),
            5_000_f32.to_bits()
        );
        assert_eq!(
            rule.required_catchment(1.0, 0.2).to_bits(),
            5_000_f32.to_bits()
        );
    }

    #[test]
    fn catchment_accumulates_projected_land_area_in_square_metres() {
        let mesh = Mesh {
            vertices: vec![
                Vec3::new(0.0, 0.0, 4.0),
                Vec3::new(1.0, 0.0, 3.0),
                Vec3::new(1.0, 1.0, 2.0),
                Vec3::new(0.0, 1.0, 1.0),
            ],
            triangles: vec![0, 1, 2, 0, 2, 3],
            ..Mesh::default()
        };
        let downstream = [1, 2, 3, 3];

        let (flow, catchment) = calculate_flow_and_catchment(&mesh, &downstream);

        assert_eq!(flow, [1, 2, 3, 4]);
        assert!((catchment[3] - ISLAND_WORLD_METRES * ISLAND_WORLD_METRES).abs() < 0.5);
    }

    #[test]
    fn mouths_only_include_main_rivers_reaching_connected_ocean() {
        let node = |vertex, x, flow| RiverNode {
            vertex,
            flow,
            surface: 0.0,
            position: Vec3::new(x, 0.5, 0.0),
        };
        let network = RiverNetwork {
            rivers: vec![
                River {
                    nodes: vec![
                        node(0, 0.2, 10),
                        node(1, 0.4, 20),
                        node(2, 0.5, 30),
                        node(6, 0.6, 40),
                    ],
                    join: None,
                },
                River {
                    nodes: vec![node(3, 0.3, 5), node(1, 0.4, 10)],
                    join: Some(0),
                },
                River {
                    nodes: vec![node(4, 0.7, 10), node(5, 0.8, 20)],
                    join: None,
                },
            ],
            join_vertices: vec![None, Some(1), None],
            waterfalls: vec![vec![false; 4], vec![false; 2], vec![false; 2]],
            river_mesh_ends: vec![Some(2), None, None],
            max_flow: 40,
            max_height: 0.2,
            ocean: vec![false, false, true, false, false, false, true],
            perimeter: vec![false; 7],
            cross_sections: Vec::new(),
        };

        let mouths = network.river_mouths();
        assert_eq!(mouths.len(), 1);
        assert_eq!(mouths[0].position, Vec2::new(0.5, 0.5));
        assert_eq!(mouths[0].downstream, Vec2::X);
        assert_eq!(mouths[0].flow, 40);
    }

    #[test]
    fn source_cutoff_rises_smoothly_with_routing_grade() {
        let rule = RiverSourceRule::new(0.5, 4.0, 0.0, 0.2);

        assert_eq!(
            rule.required_catchment(0.0, 0.2).to_bits(),
            5_000_f32.to_bits()
        );
        assert_eq!(
            rule.required_catchment(0.5, 0.2).to_bits(),
            8_750_f32.to_bits()
        );
        assert_eq!(
            rule.required_catchment(1.0, 0.2).to_bits(),
            20_000_f32.to_bits()
        );
        assert_eq!(
            RiverSourceRule::new(0.5, 1.0, 0.0, 0.2)
                .required_catchment(1.0, 0.2)
                .to_bits(),
            5_000_f32.to_bits()
        );
    }

    #[test]
    fn source_cutoff_falls_smoothly_with_elevation() {
        let rule = RiverSourceRule::new(0.5, 1.0, 9.0, 0.2);

        assert_eq!(
            rule.required_catchment(0.0, 0.0).to_bits(),
            50_000_f32.to_bits()
        );
        assert_eq!(
            rule.required_catchment(0.0, 0.1).to_bits(),
            27_500_f32.to_bits()
        );
        assert_eq!(
            rule.required_catchment(0.0, 0.2).to_bits(),
            5_000_f32.to_bits()
        );
        assert_eq!(
            rule.required_catchment(0.0, 0.4).to_bits(),
            5_000_f32.to_bits()
        );
    }

    #[test]
    fn source_grade_uses_the_routed_edge_and_handles_sinks() {
        let mesh = Mesh {
            vertices: vec![
                Vec3::new(0.0, 0.0, 1.0),
                Vec3::new(1.0, 0.0, 0.0),
                Vec3::new(0.0, 1.0, 2.0),
            ],
            ..Mesh::default()
        };

        assert!((source_grade(&mesh, 0, 1) - 0.5_f32.sqrt()).abs() < 1.0e-6);
        assert_eq!(source_grade(&mesh, 0, 0).to_bits(), 0.0_f32.to_bits());
        assert_eq!(source_grade(&mesh, 0, 2).to_bits(), 0.0_f32.to_bits());
    }

    #[test]
    fn sources_are_the_upstream_boundary_of_local_candidates() {
        let mesh = Mesh {
            vertices: vec![
                Vec3::new(0.0, 0.0, 0.2),
                Vec3::new(1.0, 0.0, 0.2),
                Vec3::new(0.0, 1.0, 0.1),
                Vec3::new(1.0, 1.0, 0.1),
            ],
            triangles: vec![0, 1, 2, 1, 3, 2],
            ..Mesh::default()
        };
        let adjacency = mesh.adjacency();
        let downstream = [2, 3, 2, 3];
        let catchment_areas = [50_000.0, 200_000.0, 300_000.0, 400_000.0];

        let sources = find_sources(
            &mesh,
            &adjacency,
            &downstream,
            &catchment_areas,
            RiverSourceRule::new(1.0, 1.0, 0.0, 0.2),
        );

        assert_eq!(sources, [1, 0]);
    }

    #[test]
    fn low_elevation_sources_require_more_catchment_instead_of_being_excluded() {
        let mesh = Mesh {
            vertices: vec![
                Vec3::new(0.0, 0.0, 0.0),
                Vec3::new(1.0, 0.0, 0.0),
                Vec3::new(0.0, 1.0, 0.2),
            ],
            triangles: vec![0, 1, 2],
            ..Mesh::default()
        };
        let adjacency = mesh.adjacency();
        let downstream = [0, 1, 2];
        let catchment_areas = [49_999.0, 50_000.0, 5_000.0];

        let sources = find_sources(
            &mesh,
            &adjacency,
            &downstream,
            &catchment_areas,
            RiverSourceRule::new(0.5, 1.0, 9.0, 0.2),
        );

        assert_eq!(sources, [1, 2]);
    }

    #[test]
    fn vertices_within_ten_centimetres_of_sea_level_move_away_from_it() {
        let mut terrain = Mesh {
            vertices: vec![
                Vec3::new(0.0, 0.0, -0.000_01),
                Vec3::new(0.0, 0.0, -0.001),
                Vec3::ZERO,
                Vec3::new(0.0, 0.0, 0.000_01),
                Vec3::new(0.0, 0.0, 0.001),
            ],
            ..Mesh::default()
        };

        enforce_sea_plane_clearance(&mut terrain, &[]);

        assert_eq!(
            terrain.vertices[0].z.to_bits(),
            (-SEA_PLANE_CLEARANCE).to_bits()
        );
        assert_eq!(terrain.vertices[1].z.to_bits(), (-0.001_f32).to_bits());
        assert_eq!(
            terrain.vertices[2].z.to_bits(),
            (-SEA_PLANE_CLEARANCE).to_bits()
        );
        assert_eq!(
            terrain.vertices[3].z.to_bits(),
            SEA_PLANE_CLEARANCE.to_bits()
        );
        assert_eq!(terrain.vertices[4].z.to_bits(), 0.001_f32.to_bits());
    }

    #[test]
    fn final_clearance_keeps_flood_filled_ocean_vertices_below_sea() {
        let mut terrain = Mesh {
            vertices: vec![Vec3::new(0.0, 0.0, 0.001), Vec3::new(1.0, 0.0, 0.000_01)],
            ..Mesh::default()
        };

        enforce_sea_plane_clearance(&mut terrain, &[true, false]);

        assert_eq!(
            terrain.vertices[0].z.to_bits(),
            (-SEA_PLANE_CLEARANCE).to_bits()
        );
        assert_eq!(
            terrain.vertices[1].z.to_bits(),
            SEA_PLANE_CLEARANCE.to_bits()
        );
    }

    #[test]
    fn final_sharp_point_repair_refines_and_rounds_an_isolated_spike() {
        let points: Vec<Vec2> = (0..=4)
            .flat_map(|y| (0..=4).map(move |x| Vec2::new(x as f32 / 4.0, y as f32 / 4.0)))
            .collect();
        let mut terrain = Mesh::delaunay(&points);
        let center = points
            .iter()
            .position(|point| *point == Vec2::new(0.5, 0.5))
            .unwrap();
        terrain.vertices[center].z = 0.2;
        let original_vertex_count = terrain.vertices.len();
        let original_height = terrain.vertices[center].z;
        let mut material = SurfaceMaterial::empty(original_vertex_count);
        let mut coverage = vec![0; original_vertex_count];
        let mut surfaces = vec![0.0; original_vertex_count];
        let mut river_uv = vec![Vec2::ZERO; original_vertex_count];
        let mut owners = vec![None; original_vertex_count];
        let mut waterfall_lips = vec![false; original_vertex_count];
        let mut target_half_widths = vec![0.0; original_vertex_count];
        let mut target_depths = vec![0.0; original_vertex_count];

        repair_sharp_terrain_points(
            &mut terrain,
            &mut material,
            &mut coverage,
            &mut surfaces,
            &mut river_uv,
            &mut owners,
            &mut waterfall_lips,
            &mut target_half_widths,
            &mut target_depths,
        );

        assert!(terrain.vertices.len() > original_vertex_count);
        assert!(terrain.vertices[center].z < original_height * 0.6);
        let repaired_adjacency = terrain.adjacency();
        let repaired_perimeter = terrain.perimeter_mask();
        assert!(!sharp_point_mask(&terrain, &repaired_adjacency, &repaired_perimeter)[center]);
        assert_eq!(material.depths().len(), terrain.vertices.len());
        assert_eq!(coverage.len(), terrain.vertices.len());
        assert_eq!(surfaces.len(), terrain.vertices.len());
        assert_eq!(river_uv.len(), terrain.vertices.len());
        assert_eq!(owners.len(), terrain.vertices.len());
        assert_eq!(waterfall_lips.len(), terrain.vertices.len());
        assert_eq!(target_half_widths.len(), terrain.vertices.len());
        assert_eq!(target_depths.len(), terrain.vertices.len());
    }

    #[test]
    fn final_sharp_point_repair_leaves_an_inclined_plane_unchanged() {
        let points: Vec<Vec2> = (0..=4)
            .flat_map(|y| (0..=4).map(move |x| Vec2::new(x as f32 / 4.0, y as f32 / 4.0)))
            .collect();
        let mut terrain = Mesh::delaunay(&points);
        terrain
            .vertices
            .iter_mut()
            .for_each(|vertex| vertex.z = vertex.x.mul_add(0.2, vertex.y * 0.1));
        let original = terrain.clone();
        let vertex_count = terrain.vertices.len();
        let mut material = SurfaceMaterial::empty(vertex_count);
        let mut coverage = vec![0; vertex_count];
        let mut surfaces = vec![0.0; vertex_count];
        let mut river_uv = vec![Vec2::ZERO; vertex_count];
        let mut owners = vec![None; vertex_count];
        let mut waterfall_lips = vec![false; vertex_count];
        let mut target_half_widths = vec![0.0; vertex_count];
        let mut target_depths = vec![0.0; vertex_count];

        repair_sharp_terrain_points(
            &mut terrain,
            &mut material,
            &mut coverage,
            &mut surfaces,
            &mut river_uv,
            &mut owners,
            &mut waterfall_lips,
            &mut target_half_widths,
            &mut target_depths,
        );

        assert_eq!(terrain, original);
    }

    #[test]
    fn ocean_mask_excludes_a_disconnected_subsea_basin() {
        let points: Vec<Vec2> = (0..=4)
            .flat_map(|y| (0..=4).map(move |x| Vec2::new(x as f32 / 4.0, y as f32 / 4.0)))
            .collect();
        let mut mesh = Mesh::delaunay(&points);
        mesh.vertices
            .iter_mut()
            .enumerate()
            .for_each(|(index, vertex)| {
                let (x, y) = (index % 5, index / 5);
                vertex.z = if x == 0 || x == 4 || y == 0 || y == 4 {
                    -0.1
                } else {
                    0.1
                };
            });
        let basin = points
            .iter()
            .position(|point| *point == Vec2::splat(0.5))
            .unwrap();
        mesh.vertices[basin].z = -0.1;
        let adjacency = mesh.adjacency();

        let ocean = fix_inland_seas(&mut mesh, &adjacency);

        assert!(!ocean[basin]);
        assert_eq!(mesh.vertices[basin].z.to_bits(), f32::EPSILON.to_bits());
        assert!(ocean.iter().enumerate().all(|(vertex, &is_ocean)| {
            let (x, y) = (vertex % 5, vertex / 5);
            x != 0 && x != 4 && y != 0 && y != 4 || is_ocean
        }));
    }

    #[test]
    fn river_reaches_sea_only_when_its_terminal_vertex_is_in_the_ocean_mask() {
        let river = River {
            nodes: vec![RiverNode {
                vertex: 1,
                flow: 1,
                surface: -0.1,
                position: Vec3::new(0.5, 0.5, -0.1),
            }],
            join: None,
        };

        assert!(!river_reaches_ocean(&river, &[true, false]));
        assert!(river_reaches_ocean(&river, &[false, true]));
    }

    #[test]
    fn waterfall_height_and_frequency_increase_with_smoothed_gradient() {
        let mut surface = 0.0_f32;
        let mut nodes = Vec::new();
        for index in (0..=20).rev() {
            if index < 20 {
                surface += if index < 10 { 0.012 } else { 0.002 };
            }
            nodes.push(RiverNode {
                vertex: index,
                flow: 10,
                surface,
                position: Vec3::new(index as f32 * 0.02, 0.5, surface),
            });
        }
        nodes.reverse();
        let outlet_surface = nodes[20].surface;
        let mut waterfalls = vec![false; nodes.len()];
        let mut scratch = Vec::new();

        form_stepped_profile(&mut nodes, &mut waterfalls, &[], 20, 0.2, &mut scratch);

        let mut gentle = Vec::new();
        let mut steep = Vec::new();
        for (index, pair) in nodes.windows(2).enumerate() {
            let drop = pair[0].surface - pair[1].surface;
            if waterfalls[index] {
                if index < 10 {
                    steep.push(drop);
                } else {
                    gentle.push(drop);
                }
            } else {
                assert!(drop.abs() < 1.0e-7);
            }
        }
        assert!(!gentle.is_empty());
        assert!(!steep.is_empty());
        assert!(steep.len() > gentle.len());
        assert!(steep.len() >= 8);
        let gentle_average = gentle.iter().sum::<f32>() / gentle.len() as f32;
        let steep_average = steep.iter().sum::<f32>() / steep.len() as f32;
        assert!(steep_average > gentle_average * 1.35);
        assert!(steep.iter().all(|height| *height <= 0.0036 + 1.0e-7));
        assert!((nodes[20].surface - outlet_surface).abs() < f32::EPSILON);
    }

    #[test]
    fn waterfall_spacing_contains_the_full_channel_width_patch() {
        let mut nodes = (0..=30)
            .map(|index| RiverNode {
                vertex: index,
                flow: 10,
                surface: (30 - index) as f32 * 0.005,
                position: Vec3::new(index as f32 * 0.001, 0.5, 0.0),
            })
            .collect::<Vec<_>>();
        let sections = vec![
            RiverCrossSection {
                target_half_width: 0.002,
                ..RiverCrossSection::default()
            };
            nodes.len()
        ];
        let mut waterfalls = vec![false; nodes.len()];
        let mut scratch = Vec::new();

        form_stepped_profile(
            &mut nodes,
            &mut waterfalls,
            &sections,
            30,
            0.2,
            &mut scratch,
        );

        let waterfall_segments = waterfalls
            .iter()
            .enumerate()
            .filter_map(|(segment, &waterfall)| waterfall.then_some(segment))
            .collect::<Vec<_>>();
        let minimum_spacing = WATERFALL_SUPPORT_RUN
            + sections[0].target_half_width * (1.0 + WATERFALL_LANDING_LENGTH_MULTIPLIER);
        assert!(waterfall_segments.len() >= 2);
        assert!(
            waterfall_segments.windows(2).all(|pair| {
                (pair[1] - pair[0]) as f32 * 0.001 + f32::EPSILON >= minimum_spacing
            })
        );
    }

    #[test]
    fn final_profile_limits_bed_grade_without_creating_late_waterfalls() {
        let mesh = Mesh {
            vertices: vec![
                Vec3::new(0.0, 0.0, 1.0),
                Vec3::new(0.1, 0.0, 1.0),
                Vec3::new(0.2, 0.0, 0.9),
            ],
            ..Mesh::default()
        };
        let nodes = vec![
            RiverNode {
                vertex: 0,
                flow: 1,
                surface: 1.0,
                position: mesh.vertices[0],
            },
            RiverNode {
                vertex: 1,
                flow: 2,
                surface: 1.0,
                position: mesh.vertices[1],
            },
            RiverNode {
                vertex: 2,
                flow: 3,
                surface: 0.9,
                position: mesh.vertices[2],
            },
        ];
        let waterfalls = vec![false; nodes.len()];
        let mut sections = vec![
            RiverCrossSection {
                required_depth: 0.10,
                ..RiverCrossSection::default()
            },
            RiverCrossSection {
                required_depth: 0.30,
                ..RiverCrossSection::default()
            },
            RiverCrossSection {
                required_depth: 0.50,
                ..RiverCrossSection::default()
            },
        ];

        let adjusted = enforce_gentle_river_profile(&mesh, &nodes, &waterfalls, &mut sections);

        let maximum_gentle_drop = 0.1 * MAXIMUM_GENTLE_RIVER_GRADE;
        let first_floor_drop = (nodes[0].surface - sections[0].required_depth)
            - (nodes[1].surface - sections[1].required_depth);
        assert!(first_floor_drop <= maximum_gentle_drop + f32::EPSILON);
        assert!(!waterfalls[1]);
        assert_eq!(sections[2].required_depth.to_bits(), 0.50_f32.to_bits());
        assert_eq!(adjusted, 1);
    }

    #[test]
    fn higher_confluence_reach_is_lowered_back_to_the_nearest_waterfall() {
        let initial_surfaces = [0.8, 0.8, 0.5, 0.5, 0.5];
        let mut mesh = Mesh {
            vertices: initial_surfaces
                .map(|surface| Vec3::new(0.0, 0.0, surface))
                .to_vec(),
            ..Mesh::default()
        };
        let adjacency = mesh.adjacency();
        let mut material = SurfaceMaterial::empty(initial_surfaces.len());
        let bedrock_rates = vec![0.0; initial_surfaces.len()];
        let control_areas = vec![1.0; initial_surfaces.len()];
        let mut nodes = initial_surfaces
            .into_iter()
            .enumerate()
            .map(|(vertex, surface)| RiverNode {
                vertex,
                flow: 10,
                surface,
                position: Vec3::ZERO,
            })
            .collect::<Vec<_>>();
        let waterfalls = [false, true, false, false, false];
        let mut budget = RiverSedimentBudget::default();

        level_confluence_reach(
            &mut test_river_terrain(
                &mut mesh,
                &adjacency,
                &mut material,
                &bedrock_rates,
                &control_areas,
            ),
            &mut nodes,
            &waterfalls,
            0.3,
            &mut budget,
        );

        let surfaces = nodes.iter().map(|node| node.surface).collect::<Vec<_>>();
        assert_eq!(surfaces[0].to_bits(), 0.8_f32.to_bits());
        assert_eq!(surfaces[1].to_bits(), 0.8_f32.to_bits());
        assert!((surfaces[2] - 0.3).abs() < 1.0e-6);
        assert!((surfaces[3] - 0.3).abs() < 1.0e-6);
        assert!((surfaces[4] - 0.3).abs() < 1.0e-6);
        assert_eq!(mesh.vertices[0].z.to_bits(), 0.8_f32.to_bits());
        assert_eq!(mesh.vertices[1].z.to_bits(), 0.8_f32.to_bits());
        assert!((mesh.vertices[2].z - 0.3).abs() < 1.0e-6);
        assert!((mesh.vertices[3].z - 0.3).abs() < 1.0e-6);
        assert!((mesh.vertices[4].z - 0.3).abs() < 1.0e-6);
        assert!((budget.bedrock_eroded - 0.6).abs() < 1.0e-6);
    }

    #[test]
    fn nearby_river_pushes_a_waterfall_drop_upstream() {
        let mut mesh = Mesh {
            vertices: vec![
                Vec3::new(0.0, 0.0, 0.10),
                Vec3::new(1.0, 0.0, 0.10),
                Vec3::new(2.0, 0.0, 0.10),
                Vec3::new(3.0, 0.0, 0.05),
                Vec3::new(2.5, 0.02, 0.08),
            ],
            ..Mesh::default()
        };
        mesh.calculate_normals();
        let mut nodes: Vec<RiverNode> = (0..4)
            .map(|vertex| RiverNode {
                vertex,
                flow: 10,
                surface: mesh.vertices[vertex].z,
                position: mesh.vertices[vertex],
            })
            .collect();
        let rivers = vec![
            River {
                nodes: nodes.clone(),
                join: None,
            },
            River {
                nodes: vec![RiverNode {
                    vertex: 4,
                    flow: 10,
                    surface: mesh.vertices[4].z,
                    position: mesh.vertices[4],
                }],
                join: None,
            },
        ];
        let clearance = WaterfallClearanceIndex::new(&rivers, &mesh, 10, 0.01);
        let mut waterfalls = vec![false, false, true, false];
        let mut scratch = Vec::new();

        relocate_conflicting_waterfalls(
            &mesh,
            &mut nodes,
            &mut waterfalls,
            3,
            WaterfallRelocation {
                clearance: &clearance,
                site: None,
                river: 0,
            },
            &[],
            &mut scratch,
        );

        assert_eq!(waterfalls, [false, true, false, false]);
        assert!(!clearance.conflicts(0, &mesh, &nodes, 1));
        assert!((nodes[0].surface - nodes[3].surface - 0.05).abs() < 1.0e-6);
        assert!((nodes[2].surface - nodes[3].surface).abs() < 1.0e-6);
    }

    #[test]
    fn intermediate_lod_keeps_the_original_drop_when_every_site_conflicts() {
        let mesh = Mesh {
            vertices: vec![
                Vec3::new(0.0, 0.0, 0.2),
                Vec3::new(1.0, 0.0, 0.2),
                Vec3::new(2.0, 0.0, 0.2),
                Vec3::new(3.0, 0.0, 0.1),
                Vec3::new(0.5, 0.01, 0.2),
                Vec3::new(1.5, 0.01, 0.2),
                Vec3::new(2.5, 0.01, 0.2),
            ],
            ..Mesh::default()
        };
        let mut nodes = (0..4)
            .map(|vertex| RiverNode {
                vertex,
                flow: 10,
                surface: mesh.vertices[vertex].z,
                position: mesh.vertices[vertex],
            })
            .collect::<Vec<_>>();
        let blockers = (4..7)
            .map(|vertex| RiverNode {
                vertex,
                flow: 10,
                surface: mesh.vertices[vertex].z,
                position: mesh.vertices[vertex],
            })
            .collect::<Vec<_>>();
        let rivers = vec![
            River {
                nodes: nodes.clone(),
                join: None,
            },
            River {
                nodes: blockers,
                join: None,
            },
        ];
        let clearance = WaterfallClearanceIndex::new(&rivers, &mesh, 10, 0.01);
        let mut waterfalls = vec![false, false, true, false];
        let mut scratch = Vec::new();

        assert!(relocate_conflicting_waterfalls(
            &mesh,
            &mut nodes,
            &mut waterfalls,
            3,
            WaterfallRelocation {
                clearance: &clearance,
                site: None,
                river: 0,
            },
            &[],
            &mut scratch,
        ));
        assert_eq!(waterfalls, [false, false, true, false]);
    }

    #[test]
    fn side_bypass_pushes_a_waterfall_to_the_next_complete_cross_channel_cut() {
        let width = 7;
        let mut points = (0..5)
            .flat_map(|y| (0..width).map(move |x| Vec2::new(x as f32, y as f32)))
            .collect::<Vec<_>>();
        let bypass = points.len();
        points.push(Vec2::new(3.0, 4.0));
        let mut triangles = Vec::new();
        for y in 0..4 {
            for x in 0..width - 1 {
                let lower_left = (y * width + x) as u32;
                let lower_right = lower_left + 1;
                let upper_left = lower_left + width as u32;
                let upper_right = upper_left + 1;
                triangles.extend([
                    lower_left,
                    lower_right,
                    upper_left,
                    upper_left,
                    lower_right,
                    upper_right,
                ]);
            }
        }
        let vertex = |x: usize, y: usize| y * width + x;
        // This two-edge route skips the proposed x=3 face and represents the
        // short side flow seen in a failed, partly bypassed waterfall.
        triangles.extend([
            vertex(2, 2) as u32,
            bypass as u32,
            vertex(2, 4) as u32,
            bypass as u32,
            vertex(4, 2) as u32,
            vertex(4, 4) as u32,
        ]);
        let mesh = Mesh {
            vertices: points.iter().map(|point| point.extend(0.2)).collect(),
            triangles,
            ..Mesh::default()
        };
        let adjacency = mesh.adjacency();
        let mut nodes = (0..width)
            .map(|x| RiverNode {
                vertex: vertex(x, 2),
                flow: 10,
                surface: if x <= 3 { 0.2 } else { 0.1 },
                position: mesh.vertices[vertex(x, 2)],
            })
            .collect::<Vec<_>>();
        let rivers = vec![River {
            nodes: nodes.clone(),
            join: None,
        }];
        let clearance = WaterfallClearanceIndex::new(&rivers, &mesh, 10, 0.01);
        let mut coverage = vec![0_u8; mesh.vertices.len()];
        for y in 1..=3 {
            for x in 0..width {
                coverage[vertex(x, y)] = 1;
            }
        }
        coverage[bypass] = 1;
        let rejected = HashSet::new();
        let site = WaterfallSiteEnvironment {
            adjacency: &adjacency,
            coverage: &coverage,
            ocean: &vec![false; mesh.vertices.len()],
            perimeter: &vec![false; mesh.vertices.len()],
            rejected: &rejected,
        };
        let sections = vec![
            RiverCrossSection {
                target_half_width: 1.2,
                ..RiverCrossSection::default()
            };
            nodes.len()
        ];
        let mut waterfalls = vec![false; nodes.len()];
        waterfalls[3] = true;
        let mut scratch = Vec::new();

        assert!(relocate_conflicting_waterfalls(
            &mesh,
            &mut nodes,
            &mut waterfalls,
            width - 1,
            WaterfallRelocation {
                clearance: &clearance,
                site: Some(site),
                river: 0,
            },
            &sections,
            &mut scratch,
        ));
        assert!(waterfalls[2]);
        assert!(!waterfalls[3]);

        let blocked = vec![true; mesh.vertices.len()];
        let blocked_site = WaterfallSiteEnvironment {
            adjacency: &adjacency,
            coverage: &coverage,
            ocean: &blocked,
            perimeter: &blocked,
            rejected: &rejected,
        };
        assert!(!relocate_conflicting_waterfalls(
            &mesh,
            &mut nodes,
            &mut waterfalls,
            width - 1,
            WaterfallRelocation {
                clearance: &clearance,
                site: Some(blocked_site),
                river: 0,
            },
            &sections,
            &mut scratch,
        ));
        assert!(!waterfalls.iter().any(|&waterfall| waterfall));
    }

    #[test]
    fn removing_an_invalid_parent_also_removes_its_dependent_tributaries() {
        let node = |vertex| RiverNode {
            vertex,
            flow: 1,
            surface: 1.0,
            position: Vec3::new(vertex as f32, 0.0, 1.0),
        };
        let mut network = RiverNetwork {
            rivers: vec![
                River {
                    nodes: vec![node(0), node(1)],
                    join: None,
                },
                River {
                    nodes: vec![node(2), node(1)],
                    join: Some(0),
                },
                River {
                    nodes: vec![node(3), node(4)],
                    join: None,
                },
            ],
            join_vertices: vec![None, Some(1), None],
            waterfalls: vec![vec![false; 2]; 3],
            river_mesh_ends: vec![None; 3],
            max_flow: 1,
            max_height: 1.0,
            ocean: vec![false; 5],
            perimeter: vec![false; 5],
            cross_sections: vec![vec![RiverCrossSection::default(); 2]; 3],
        };

        assert_eq!(network.remove_invalid_rivers(&[true, false, false]), 2);
        assert_eq!(network.rivers.len(), 1);
        assert_eq!(network.rivers[0].nodes[0].vertex, 3);
        assert_eq!(network.join_vertices, [None]);
        assert_eq!(network.waterfalls.len(), 1);
        assert_eq!(network.river_mesh_ends.len(), 1);
        assert_eq!(network.cross_sections.len(), 1);
    }

    #[test]
    fn waterfall_terraces_are_carved_into_the_river_bed() {
        let points: Vec<Vec2> = (0..=2)
            .flat_map(|y| (0..=4).map(move |x| Vec2::new(x as f32 * 0.25, y as f32 * 0.5)))
            .collect();
        let mut mesh = Mesh::delaunay(&points);
        mesh.vertices.iter_mut().for_each(|vertex| vertex.z = 0.12);
        let channel: Vec<usize> = (0..4)
            .map(|x| {
                points
                    .iter()
                    .position(|point| *point == Vec2::new(x as f32 * 0.25, 0.5))
                    .unwrap()
            })
            .collect();
        let surfaces = [0.06, 0.06, 0.03, 0.03];
        for (&vertex, &surface) in channel.iter().zip(&surfaces) {
            mesh.vertices[vertex].z = surface;
        }
        let nodes: Vec<RiverNode> = channel
            .iter()
            .zip(surfaces)
            .map(|(&vertex, surface)| RiverNode {
                vertex,
                flow: 10,
                surface,
                position: mesh.vertices[vertex],
            })
            .collect();
        let waterfalls = [false, true, false, false];
        let adjacency = mesh.adjacency();
        let parameters = RiverCarveParameters {
            downstream_surface: 0.0,
            terminal_ocean: false,
            max_height: 0.2,
            max_flow: 10,
            depth_multiplier: 1.0 / 10.0_f32.sqrt(),
            cross_sections: &[],
        };
        let mut targets = Vec::new();
        let mut material = SurfaceMaterial::empty(mesh.vertices.len());
        let bedrock_rates = vec![1.0; mesh.vertices.len()];
        let control_areas = projected_vertex_control_areas(&mesh);
        let mut budget = RiverSedimentBudget::default();

        {
            let mut terrain = test_river_terrain(
                &mut mesh,
                &adjacency,
                &mut material,
                &bedrock_rates,
                &control_areas,
            );
            carve_stepped_bed(
                &mut terrain,
                &nodes,
                &waterfalls,
                3,
                parameters,
                &mut targets,
                &mut budget,
            );
        }

        let beds: Vec<f32> = channel
            .iter()
            .map(|&vertex| mesh.vertices[vertex].z)
            .collect();
        assert!((beds[0] - beds[1]).abs() < f32::EPSILON);
        assert!((beds[2] - beds[3]).abs() < f32::EPSILON);
        assert!(beds[1] > beds[2]);
        assert!(budget.carried > 0.0);
    }

    #[test]
    fn waterfall_lip_refinement_adds_detail_and_rounds_along_the_normal() {
        let points: Vec<Vec2> = (0..=4)
            .flat_map(|y| (0..=4).map(move |x| Vec2::new(x as f32 / 4.0, y as f32 / 4.0)))
            .collect();
        let mut water = Mesh::delaunay(&points);
        for vertex in &mut water.vertices {
            vertex.z = if vertex.y <= 0.5 { 0.1 } else { 0.0 };
        }
        water.uv = water
            .vertices
            .iter()
            .map(|vertex| vertex.truncate())
            .collect();
        let center = points
            .iter()
            .position(|point| *point == Vec2::new(0.5, 0.5))
            .unwrap();
        let original = water.vertices[center];
        let original_vertices = water.vertices.clone();
        let original_vertex_count = water.vertices.len();
        let mut lips = vec![false; water.vertices.len()];
        for (vertex, position) in water.vertices.iter().enumerate() {
            lips[vertex] = (position.y - 0.5).abs() < f32::EPSILON;
        }

        let minimum_heights = water.vertices.iter().map(|vertex| vertex.z - 0.1).collect();
        let rounded = round_waterfall_lips(water, lips, minimum_heights);

        assert!(rounded.vertices.len() > original_vertex_count);
        assert_ne!(rounded.vertices[center], original);
        assert_eq!(rounded.vertices[center].truncate(), original.truncate());
        assert!(rounded.vertices[center].z < original.z);
        assert!(rounded.vertices[center].z > 0.0);
        for (vertex, original) in original_vertices.iter().enumerate() {
            if original.y > 0.5 {
                assert_eq!(rounded.vertices[vertex], *original);
            }
        }
    }

    #[test]
    fn plunge_pool_is_centred_on_the_pulled_down_waterfall_fan() {
        let points: Vec<Vec2> = (0..=8)
            .flat_map(|y| (0..=8).map(move |x| Vec2::new(x as f32 / 8.0, y as f32 / 8.0)))
            .collect();
        let mut terrain = Mesh::delaunay(&points);
        terrain
            .vertices
            .iter_mut()
            .for_each(|vertex| vertex.z = 0.42 - vertex.x * 0.4);
        let original = terrain.vertices.clone();
        let mut material = SurfaceMaterial::empty(terrain.vertices.len());
        material.depths_mut().fill(0.1);
        let mut coverage = vec![0; terrain.vertices.len()];
        let mut surfaces = vec![0.0; terrain.vertices.len()];
        let mut waterfall_lips = vec![false; terrain.vertices.len()];
        let lip = points
            .iter()
            .position(|point| *point == Vec2::new(0.25, 0.5))
            .unwrap();
        let support = points
            .iter()
            .position(|point| *point == Vec2::new(0.375, 0.5))
            .unwrap();
        let non_river_pool = points
            .iter()
            .position(|point| *point == Vec2::new(0.25, 0.625))
            .unwrap();
        coverage[lip] = 2;
        coverage[support] = 2;
        surfaces[support] = 0.35;
        let patch = WaterfallPatch {
            river: 0,
            segment: 0,
            upper_vertex: lip,
            upper_centre: Vec2::new(0.25, 0.5),
            direction: Vec2::X,
            across: Vec2::Y,
            upper_surface: 0.4,
            lower_surface: 0.2,
            lower_floor: 0.15,
            half_width: 0.2,
            support_run: 0.25,
            pool: Some(PlungePool {
                centre: Vec2::new(0.25, 0.5),
                downstream_radius: 0.2,
                lateral_radius: 0.15,
                depth: 0.05,
            }),
        };
        let neighbours = terrain.adjacency()[lip].to_vec();
        let notch_owners =
            recess_waterfall_notches(&mut terrain, &mut material, &[patch], &coverage);

        let constraints = pin_waterfalls_to_terrain(
            &terrain,
            &mut material,
            &[patch],
            &notch_owners,
            &mut coverage,
            &mut surfaces,
            &mut waterfall_lips,
        );

        let outside = points
            .iter()
            .position(|point| *point == Vec2::new(1.0, 1.0))
            .unwrap();
        assert!(constraints.pinned[lip]);
        assert!(!constraints.pinned[support]);
        assert!(!constraints.support[support]);
        assert!(constraints.water_unclamped[support]);
        assert_eq!(
            constraints.terrain_ceiling[support].to_bits(),
            terrain.vertices[support].z.to_bits()
        );
        assert!(!waterfall_lips[support]);
        assert!(constraints.support[lip]);
        assert!(waterfall_lips[lip]);
        assert!((terrain.vertices[lip].z - 0.1).abs() < f32::EPSILON);
        assert!(
            neighbours
                .iter()
                .all(|&vertex| terrain.vertices[vertex].z <= original[vertex].z)
        );
        assert!(terrain.vertices[support].z < original[support].z);
        assert_eq!(terrain.vertices[outside], original[outside]);
        assert!(terrain.vertices[non_river_pool].z < original[non_river_pool].z);
        assert!(constraints.patch[lip]);
        assert!(!constraints.patch[support]);
        assert!(neighbours.iter().all(|&vertex| {
            constraints.patch[vertex]
                == (notch_owners[vertex].is_some()
                    && patch
                        .face_surface_at(terrain.vertices[vertex].truncate())
                        .is_some())
        }));
        assert_eq!(surfaces[lip].to_bits(), patch.lower_surface.to_bits());
        assert_eq!(surfaces[support].to_bits(), patch.lower_surface.to_bits());
        assert_eq!(surfaces[non_river_pool].to_bits(), 0.0_f32.to_bits());
        assert!(
            notch_owners
                .iter()
                .enumerate()
                .filter(|(_, owner)| owner.is_some())
                .all(|(vertex, _)| material.depths()[vertex] <= f32::EPSILON)
        );
        assert_eq!(coverage[outside], 0);
        assert_eq!(coverage[non_river_pool], 0);

        let fan_ceiling = constraints.terrain_ceiling[support];
        terrain.vertices[support].z = patch.upper_surface;
        assert_eq!(
            enforce_waterfall_downstream_ceiling(&mut terrain, &constraints.terrain_ceiling),
            1
        );
        assert_eq!(terrain.vertices[support].z.to_bits(), fan_ceiling.to_bits());
    }

    #[test]
    fn waterfall_face_is_flat_across_the_channel_and_smooth_along_flow() {
        let patch = WaterfallPatch {
            river: 0,
            segment: 0,
            upper_vertex: 0,
            upper_centre: Vec2::ZERO,
            direction: Vec2::X,
            across: Vec2::Y,
            upper_surface: 0.4,
            lower_surface: 0.2,
            lower_floor: 0.15,
            half_width: 0.2,
            support_run: 0.25,
            pool: None,
        };
        let face_run = 2.0 * WATERFALL_TARGET_EDGE_LENGTH;
        let halfway = Vec2::new(-face_run * 0.5, 0.0);
        let across = halfway + Vec2::Y * 0.15;
        let upstream = Vec2::new(-face_run, 0.0);
        let behind = Vec2::new(-face_run - WATERFALL_TARGET_EDGE_LENGTH, 0.0);
        let downstream = Vec2::new(face_run, 0.0);

        let halfway_surface = patch.face_surface_at(halfway).unwrap();
        assert_eq!(
            halfway_surface.to_bits(),
            patch.face_surface_at(across).unwrap().to_bits()
        );
        assert!((halfway_surface - 0.3).abs() < 1.0e-6);
        assert_eq!(
            patch.face_surface_at(upstream).unwrap().to_bits(),
            patch.upper_surface.to_bits()
        );
        assert_eq!(
            patch.face_surface_at(behind).unwrap().to_bits(),
            patch.upper_surface.to_bits()
        );
        assert!(patch.face_surface_at(downstream).is_none());
    }

    #[test]
    fn waterfall_face_reaches_both_banks_when_coverage_is_wider_than_nominal_width() {
        let spacing = WATERFALL_TARGET_EDGE_LENGTH;
        let points = (-2..=2)
            .flat_map(|y| (-2..=2).map(move |x| Vec2::new(x as f32 * spacing, y as f32 * spacing)))
            .collect::<Vec<_>>();
        let mut triangles = Vec::new();
        for y in 0..4 {
            for x in 0..4 {
                let lower_left = (y * 5 + x) as u32;
                let lower_right = lower_left + 1;
                let upper_left = lower_left + 5;
                let upper_right = upper_left + 1;
                triangles.extend([
                    lower_left,
                    lower_right,
                    upper_left,
                    upper_left,
                    lower_right,
                    upper_right,
                ]);
            }
        }
        let mut terrain = Mesh {
            vertices: points.iter().map(|point| point.extend(0.8)).collect(),
            triangles,
            uv: points,
            ..Mesh::default()
        };
        let upper_vertex = terrain
            .vertices
            .iter()
            .position(|position| position.truncate() == Vec2::ZERO)
            .unwrap();
        let patch = WaterfallPatch {
            river: 0,
            segment: 0,
            upper_vertex,
            upper_centre: Vec2::ZERO,
            direction: Vec2::X,
            across: Vec2::Y,
            upper_surface: 0.8,
            lower_surface: 0.2,
            lower_floor: 0.15,
            half_width: spacing * 0.5,
            support_run: spacing,
            pool: None,
        };
        let mut material = SurfaceMaterial::empty(terrain.vertices.len());
        let mut coverage = vec![2; terrain.vertices.len()];
        let mut surfaces = vec![0.8; terrain.vertices.len()];
        let owners = vec![Some(RiverOwnerKey { river: 0, node: 0 }); terrain.vertices.len()];
        let mut waterfall_lips = vec![false; terrain.vertices.len()];

        let notch_owners =
            recess_waterfall_notches(&mut terrain, &mut material, &[patch], &coverage);
        let mut constraints = pin_waterfalls_to_terrain(
            &terrain,
            &mut material,
            &[patch],
            &notch_owners,
            &mut coverage,
            &mut surfaces,
            &mut waterfall_lips,
        );
        constraints.support.fill(false);
        assert!(
            rebuild_final_waterfall_support_mask(
                &terrain,
                &[patch],
                &coverage,
                &owners,
                &mut constraints,
            ) > 0
        );

        for y in -2..=2 {
            let point = Vec2::new(0.0, y as f32 * spacing);
            let vertex = terrain
                .vertices
                .iter()
                .position(|position| position.truncate() == point)
                .unwrap();
            assert!(constraints.pinned[vertex], "unspanned bank row {y}");
            assert!(constraints.support[vertex], "unsupported bank row {y}");
            assert!((surfaces[vertex] - patch.lower_surface).abs() < f32::EPSILON);
        }
    }

    #[test]
    fn waterfall_face_refinement_includes_one_ring_beyond_the_banks() {
        let adjacency = Mesh {
            vertices: vec![Vec3::ZERO; 7],
            triangles: vec![0, 1, 2, 2, 1, 3, 2, 3, 4, 4, 3, 5, 4, 5, 6],
            ..Mesh::default()
        }
        .adjacency();

        assert_eq!(
            expand_vertex_mask_to_banks(
                &adjacency,
                &[true, false, false, false, false, false, false],
                &[true; 7],
                &[true, true, true, true, true, true, false],
                &[false, false, false, true, true, false, false],
            ),
            vec![true, true, true, true, true, true, false]
        );
    }

    #[test]
    fn completed_waterfall_rejects_a_bank_dragged_toward_the_lower_terrace() {
        let spacing = WATERFALL_TARGET_EDGE_LENGTH;
        let width = 7_usize;
        let height = 5_usize;
        let points = (0..height)
            .flat_map(|y| {
                (0..width)
                    .map(move |x| Vec2::new((x as f32 - 4.0) * spacing, (y as f32 - 2.0) * spacing))
            })
            .collect::<Vec<_>>();
        let mut triangles = Vec::new();
        for y in 0..height - 1 {
            for x in 0..width - 1 {
                let lower_left = (y * width + x) as u32;
                let lower_right = lower_left + 1;
                let upper_left = lower_left + width as u32;
                let upper_right = upper_left + 1;
                triangles.extend([
                    lower_left,
                    lower_right,
                    upper_left,
                    upper_left,
                    lower_right,
                    upper_right,
                ]);
            }
        }
        let upper_surface = 0.8;
        let lower_surface = 0.2;
        let mut terrain = Mesh {
            vertices: points
                .iter()
                .map(|point| point.extend(upper_surface))
                .collect(),
            triangles,
            uv: points,
            ..Mesh::default()
        };
        let vertex_at = |x: usize, y: usize| y * width + x;
        let patch = WaterfallPatch {
            river: 0,
            segment: 0,
            upper_vertex: vertex_at(4, 2),
            upper_centre: Vec2::ZERO,
            direction: Vec2::X,
            across: Vec2::Y,
            upper_surface,
            lower_surface,
            lower_floor: lower_surface - 0.05,
            half_width: spacing,
            support_run: spacing,
            pool: None,
        };
        let mut coverage = terrain
            .vertices
            .iter()
            .map(|position| u8::from(position.y.abs() <= spacing * 1.01) * 2)
            .collect::<Vec<_>>();
        mark_river_boundary(
            &terrain.adjacency(),
            &terrain.perimeter_mask(),
            &mut coverage,
        );

        assert!(detect_failed_final_waterfalls(&terrain, &[patch], &coverage).is_empty());

        let collapsed_bank = vertex_at(2, 3);
        terrain.vertices[collapsed_bank].z = lower_surface + 0.05;
        assert_eq!(
            detect_failed_final_waterfalls(&terrain, &[patch], &coverage),
            vec![patch.upper_vertex]
        );
    }

    #[test]
    fn final_waterfall_smoothing_freely_relaxes_face_banks_and_outer_ring() {
        let spacing = WATERFALL_TARGET_EDGE_LENGTH;
        let points = (0..7)
            .flat_map(|y| {
                (0..3)
                    .map(move |x| Vec2::new((x as f32 - 2.0) * spacing, (y as f32 - 3.0) * spacing))
            })
            .collect::<Vec<_>>();
        let mut triangles = Vec::new();
        for y in 0..6 {
            for x in 0..2 {
                let lower_left = (y * 3 + x) as u32;
                let lower_right = lower_left + 1;
                let upper_left = lower_left + 3;
                let upper_right = upper_left + 1;
                triangles.extend([
                    lower_left,
                    lower_right,
                    upper_left,
                    upper_left,
                    lower_right,
                    upper_right,
                ]);
            }
        }
        let mut terrain = Mesh {
            vertices: points.iter().map(|point| point.extend(1.0)).collect(),
            triangles,
            uv: points,
            ..Mesh::default()
        };
        let mut coverage = terrain
            .vertices
            .iter()
            .map(|position| u8::from(position.y.abs() <= spacing * 1.01) * 2)
            .collect::<Vec<_>>();
        mark_river_boundary(
            &terrain.adjacency(),
            &terrain.perimeter_mask(),
            &mut coverage,
        );
        let vertex_at = |point: Vec2| {
            terrain
                .vertices
                .iter()
                .position(|position| position.truncate().distance_squared(point) < 1.0e-12)
                .unwrap()
        };
        let core = vertex_at(Vec2::new(-spacing, 0.0));
        let bank = vertex_at(Vec2::new(-spacing, spacing));
        let apron = vertex_at(Vec2::new(-spacing, spacing * 2.0));
        let outside = vertex_at(Vec2::new(-spacing, spacing * 3.0));
        let patch = WaterfallPatch {
            river: 0,
            segment: 0,
            upper_vertex: vertex_at(Vec2::ZERO),
            upper_centre: Vec2::ZERO,
            direction: Vec2::X,
            across: Vec2::Y,
            upper_surface: 0.8,
            lower_surface: 0.2,
            lower_floor: 0.15,
            half_width: spacing * 1.1,
            support_run: spacing,
            pool: None,
        };
        let mut material = SurfaceMaterial::empty(terrain.vertices.len());
        recess_waterfall_notches(&mut terrain, &mut material, &[patch], &coverage);
        for (position, &remaining) in terrain.vertices.iter_mut().zip(&coverage) {
            if remaining == 0 {
                position.z = 1.0;
            }
        }
        terrain.vertices[core].z -= 0.1;
        let mut surfaces = terrain
            .vertices
            .iter()
            .map(|position| position.z)
            .collect::<Vec<_>>();
        let owners = coverage
            .iter()
            .map(|&remaining| (remaining != 0).then_some(RiverOwnerKey { river: 0, node: 0 }))
            .collect::<Vec<_>>();
        let core_height = terrain.vertices[core].z;
        let bank_height = terrain.vertices[bank].z;
        let bank_surface = surfaces[bank];

        assert!(!terrain.perimeter_mask()[bank]);
        assert_ne!(coverage[bank], 0);
        assert!(patch.contains_face_point(terrain.vertices[bank].truncate()));
        let adjacency = terrain.adjacency();
        assert!(!adjacency[bank].is_empty());
        let perimeter = terrain.perimeter_mask();
        let face = terrain
            .vertices
            .iter()
            .map(|position| patch.contains_face_point(position.truncate()))
            .collect::<Vec<_>>();
        let smoothing_band = terrain
            .vertices
            .iter()
            .map(|position| patch.contains_face_smoothing_band(position.truncate()))
            .collect::<Vec<_>>();
        let eligible = coverage
            .iter()
            .zip(&smoothing_band)
            .map(|(&remaining, &inside_band)| remaining != 0 && inside_band)
            .collect::<Vec<_>>();
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
        let broad_patch = WaterfallPatch {
            half_width: spacing * 4.0,
            ..patch
        };
        let bank_apron = waterfall_face_bank_apron_for_patch(
            &terrain,
            broad_patch,
            &coverage,
            &adjacency,
            &banks,
        );
        assert!(bank_apron[bank]);
        assert!(bank_apron[apron]);
        assert!(!bank_apron[outside]);
        let selected =
            expand_vertex_mask_to_banks(&adjacency, &face, &eligible, &smoothing_band, &banks);
        assert!(selected[bank]);
        let bank_average = adjacency[bank]
            .iter()
            .fold(terrain.vertices[bank], |total, &neighbour| {
                total + terrain.vertices[neighbour]
            })
            / (adjacency[bank].len() + 1) as f32;
        assert!(bank_average.distance_squared(terrain.vertices[bank]) > f32::EPSILON);

        let moved =
            smooth_final_waterfall_patches(&mut terrain, &mut surfaces, &[patch], &coverage);

        assert!(moved > 0);
        assert_ne!(terrain.vertices[core].z.to_bits(), core_height.to_bits());
        assert!(terrain.vertices[bank].z > bank_height);
        assert!(terrain.vertices[bank].z < terrain.vertices[apron].z);
        assert!(terrain.vertices[apron].z < 1.0);
        assert_ne!(surfaces[bank].to_bits(), bank_surface.to_bits());
        for vertex in [core, bank] {
            let expected = terrain.vertices[vertex].z + WATERFALL_WATER_CLEARANCE;
            assert!((surfaces[vertex] - expected).abs() < f32::EPSILON);
        }
        assert_eq!(terrain.vertices[outside].z.to_bits(), 1.0_f32.to_bits());

        let smoothed_positions = terrain.vertices.clone();
        let mut constraints = WaterfallTerrainConstraints {
            patch: vec![false; terrain.vertices.len()],
            pinned: vec![false; terrain.vertices.len()],
            support: vec![false; terrain.vertices.len()],
            water_unclamped: vec![false; terrain.vertices.len()],
            terrain_ceiling: vec![f32::INFINITY; terrain.vertices.len()],
        };
        rebuild_final_waterfall_support_mask(
            &terrain,
            &[patch],
            &coverage,
            &owners,
            &mut constraints,
        );

        assert_eq!(terrain.vertices, smoothed_positions);
        assert!(constraints.patch[core]);
        assert!(constraints.patch[bank]);
        assert!(constraints.support[core]);
        assert!(constraints.support[bank]);
        assert!(!constraints.support[apron]);

        let river_uv = vec![Vec2::ZERO; terrain.vertices.len()];
        let water =
            duplicate_river_topology(&terrain, &coverage, &surfaces, &river_uv, &constraints);
        for vertex in [core, bank] {
            let water_vertex = water
                .vertices
                .iter()
                .find(|position| {
                    position
                        .truncate()
                        .distance_squared(terrain.vertices[vertex].truncate())
                        < 1.0e-12
                })
                .unwrap();
            let expected = terrain.vertices[vertex].z + WATERFALL_WATER_CLEARANCE;
            assert!((water_vertex.z - expected).abs() < f32::EPSILON);
        }
        assert_eq!(terrain.vertices[outside].z.to_bits(), 1.0_f32.to_bits());
    }

    #[test]
    fn final_waterfall_edges_switch_relationship_at_the_lip_and_foot_planes() {
        let spacing = WATERFALL_TARGET_EDGE_LENGTH;
        let points = (-3..=3)
            .flat_map(|y| (-4..=4).map(move |x| Vec2::new(x as f32 * spacing, y as f32 * spacing)))
            .collect::<Vec<_>>();
        let mut triangles = Vec::new();
        for y in 0..6 {
            for x in 0..8 {
                let lower_left = (y * 9 + x) as u32;
                let lower_right = lower_left + 1;
                let upper_left = lower_left + 9;
                let upper_right = upper_left + 1;
                triangles.extend([
                    lower_left,
                    lower_right,
                    upper_left,
                    upper_left,
                    lower_right,
                    upper_right,
                ]);
            }
        }
        let mut terrain = Mesh {
            vertices: points.iter().map(|point| point.extend(0.1)).collect(),
            triangles,
            uv: points,
            ..Mesh::default()
        };
        let mut coverage = terrain
            .vertices
            .iter()
            .map(|position| u8::from(position.y.abs() <= spacing * 1.01) * 2)
            .collect::<Vec<_>>();
        mark_river_boundary(
            &terrain.adjacency(),
            &terrain.perimeter_mask(),
            &mut coverage,
        );
        let vertex_at = |x: i32, y: i32| {
            let target = Vec2::new(x as f32 * spacing, y as f32 * spacing);
            terrain
                .vertices
                .iter()
                .position(|position| position.truncate().distance_squared(target) < 1.0e-12)
                .unwrap()
        };
        let patch = WaterfallPatch {
            river: 0,
            segment: 2,
            upper_vertex: vertex_at(0, 0),
            upper_centre: Vec2::ZERO,
            direction: Vec2::X,
            across: Vec2::Y,
            upper_surface: 0.8,
            lower_surface: 0.2,
            lower_floor: 0.15,
            half_width: spacing * 1.1,
            support_run: spacing,
            pool: None,
        };
        let upstream = [vertex_at(-3, 0), vertex_at(-3, 1), vertex_at(-3, 2)];
        let downstream = [vertex_at(1, 0), vertex_at(1, 1), vertex_at(1, 2)];
        let before_first = [vertex_at(-4, 0), vertex_at(-4, 1), vertex_at(-4, 2)];
        let after_last = [vertex_at(3, 0), vertex_at(3, 1), vertex_at(3, 2)];
        let lip = [vertex_at(-2, 0), vertex_at(-2, 1), vertex_at(-2, 2)];
        let face = [vertex_at(-1, 0), vertex_at(-1, 1), vertex_at(-1, 2)];
        let foot = [vertex_at(0, 0), vertex_at(0, 1), vertex_at(0, 2)];
        let outside = [vertex_at(-3, 3), vertex_at(1, 3)];
        let mut surfaces = vec![0.5; terrain.vertices.len()];
        for &vertex in &upstream {
            surfaces[vertex] = patch.upper_surface;
        }
        for &vertex in &downstream {
            surfaces[vertex] = patch.lower_surface;
        }
        for &vertex in &before_first {
            surfaces[vertex] = 0.1;
        }
        for &vertex in &after_last {
            surfaces[vertex] = 0.9;
        }
        surfaces[lip[0]] = patch.upper_surface;
        surfaces[foot[0]] = patch.lower_surface;
        let mut owners = coverage
            .iter()
            .map(|&remaining| (remaining != 0).then_some(RiverOwnerKey { river: 0, node: 2 }))
            .collect::<Vec<_>>();
        for &vertex in &downstream[..2] {
            owners[vertex] = Some(RiverOwnerKey { river: 0, node: 1 });
        }
        owners[before_first[1]] = Some(RiverOwnerKey { river: 0, node: 3 });
        terrain.vertices[upstream[0]].z = 0.95;
        terrain.vertices[downstream[0]].z = 0.9;
        terrain.vertices[after_last[0]].z = 0.9;
        let mut constraints = WaterfallTerrainConstraints {
            patch: vec![false; terrain.vertices.len()],
            pinned: vec![false; terrain.vertices.len()],
            support: vec![false; terrain.vertices.len()],
            water_unclamped: vec![false; terrain.vertices.len()],
            terrain_ceiling: vec![f32::INFINITY; terrain.vertices.len()],
        };

        assert!(
            enforce_final_waterfall_edge_relationships(
                &mut terrain,
                &mut surfaces,
                &[patch],
                &coverage,
                &owners,
                &mut constraints,
            ) > 0
        );

        assert_eq!(
            terrain.vertices[upstream[0]].z.to_bits(),
            0.95_f32.to_bits()
        );
        let first_step_blend = patch.edge_normal_blend(terrain.vertices[upstream[1]].truncate());
        assert!((first_step_blend - 0.5).abs() < f32::EPSILON);
        let expected_upstream_bank = (patch.upper_surface - 0.1).mul_add(first_step_blend, 0.1);
        assert!((terrain.vertices[upstream[1]].z - expected_upstream_bank).abs() < f32::EPSILON);
        assert_eq!(terrain.vertices[upstream[2]].z.to_bits(), 0.1_f32.to_bits());
        assert!((terrain.vertices[downstream[0]].z - patch.lower_surface).abs() <= f32::EPSILON);
        let downstream_blend = patch.edge_normal_blend(terrain.vertices[downstream[1]].truncate());
        assert!((downstream_blend - 1.0).abs() < f32::EPSILON);
        let expected_downstream_bank = (patch.lower_surface - 0.1).mul_add(downstream_blend, 0.1);
        assert!(
            (terrain.vertices[downstream[1]].z - expected_downstream_bank).abs() < f32::EPSILON
        );
        assert_eq!(
            terrain.vertices[downstream[2]].z.to_bits(),
            0.1_f32.to_bits()
        );
        let outer_plane_blend =
            patch.edge_normal_blend(terrain.vertices[before_first[0]].truncate());
        assert!((outer_plane_blend - 1.0).abs() < f32::EPSILON);
        let expected_before_first_terrain =
            (patch.upper_surface - 0.1).mul_add(outer_plane_blend, 0.1);
        let expected_before_first_surface = (patch.upper_surface
            - (expected_before_first_terrain + WATERFALL_WATER_CLEARANCE))
            .mul_add(
                outer_plane_blend,
                expected_before_first_terrain + WATERFALL_WATER_CLEARANCE,
            );
        assert!((surfaces[before_first[0]] - expected_before_first_surface).abs() < f32::EPSILON);
        assert!(
            (terrain.vertices[before_first[1]].z - expected_before_first_terrain).abs()
                < f32::EPSILON
        );
        assert_eq!(
            surfaces[after_last[0]].to_bits(),
            patch.lower_surface.to_bits()
        );
        assert!((terrain.vertices[after_last[0]].z - patch.lower_surface).abs() <= f32::EPSILON);
        assert!((terrain.vertices[after_last[1]].z - patch.lower_surface).abs() <= f32::EPSILON);
        for vertices in [&lip, &face, &foot] {
            assert_eq!(terrain.vertices[vertices[0]].z.to_bits(), 0.1_f32.to_bits());
            assert_eq!(terrain.vertices[vertices[1]].z.to_bits(), 0.1_f32.to_bits());
            assert_eq!(terrain.vertices[vertices[2]].z.to_bits(), 0.1_f32.to_bits());
            assert!(
                (surfaces[vertices[1]] - (0.1 + WATERFALL_WATER_CLEARANCE)).abs() < f32::EPSILON
            );
        }
        assert!(
            outside
                .iter()
                .all(|&vertex| terrain.vertices[vertex].z.to_bits() == 0.1_f32.to_bits())
        );

        rebuild_final_waterfall_support_mask(
            &terrain,
            &[patch],
            &coverage,
            &owners,
            &mut constraints,
        );
        assert!(constraints.water_unclamped[before_first[0]]);
        assert!(constraints.water_unclamped[after_last[0]]);
        let water = duplicate_river_topology(
            &terrain,
            &coverage,
            &surfaces,
            &vec![Vec2::ZERO; terrain.vertices.len()],
            &constraints,
        );
        let downstream_water = water
            .vertices
            .iter()
            .find(|position| {
                position
                    .truncate()
                    .distance_squared(terrain.vertices[after_last[0]].truncate())
                    < 1.0e-12
            })
            .unwrap();
        let upstream_water = water
            .vertices
            .iter()
            .find(|position| {
                position
                    .truncate()
                    .distance_squared(terrain.vertices[upstream[0]].truncate())
                    < 1.0e-12
            })
            .unwrap();
        let expected_upstream_surface = (patch.upper_surface
            - (terrain.vertices[upstream[0]].z + WATERFALL_WATER_CLEARANCE))
            .mul_add(
                first_step_blend,
                terrain.vertices[upstream[0]].z + WATERFALL_WATER_CLEARANCE,
            );
        assert!(
            (upstream_water.z - (expected_upstream_surface + RIVER_SURFACE_OFFSET)).abs()
                < f32::EPSILON
        );
        assert!(upstream_water.z < terrain.vertices[upstream[0]].z);
        assert!(
            (downstream_water.z - (patch.lower_surface + RIVER_SURFACE_OFFSET)).abs()
                < f32::EPSILON,
            "downstream water {}, expected {}, hydraulic surface {}",
            downstream_water.z,
            patch.lower_surface + RIVER_SURFACE_OFFSET,
            surfaces[after_last[0]],
        );
        assert!(downstream_water.z > terrain.vertices[after_last[0]].z);
    }

    #[test]
    fn river_reach_is_bounded_by_the_next_lip_and_previous_waterfall_bottom() {
        let terrain = Mesh {
            vertices: [-3.0, 1.0, 2.0, 11.0, 2.0]
                .into_iter()
                .map(|x| Vec3::new(x, 0.0, 0.0))
                .collect(),
            ..Mesh::default()
        };
        let first = WaterfallPatch {
            river: 0,
            segment: 1,
            upper_vertex: 0,
            upper_centre: Vec2::ZERO,
            direction: Vec2::X,
            across: Vec2::Y,
            upper_surface: 0.8,
            lower_surface: 0.6,
            lower_floor: 0.55,
            half_width: WATERFALL_TARGET_EDGE_LENGTH,
            support_run: WATERFALL_TARGET_EDGE_LENGTH,
            pool: None,
        };
        let second = WaterfallPatch {
            segment: 4,
            upper_centre: Vec2::splat(10.0),
            upper_surface: 0.4,
            lower_surface: 0.2,
            lower_floor: 0.15,
            ..first
        };
        let coverage = vec![2; terrain.vertices.len()];
        let owners = [
            Some(RiverOwnerKey { river: 0, node: 0 }),
            Some(RiverOwnerKey { river: 0, node: 2 }),
            Some(RiverOwnerKey { river: 0, node: 3 }),
            Some(RiverOwnerKey { river: 0, node: 5 }),
            Some(RiverOwnerKey { river: 1, node: 2 }),
        ];
        let levels = [
            WaterfallChannelLevels {
                lip: first.upper_surface,
                bottom: first.lower_surface,
            },
            WaterfallChannelLevels {
                lip: second.upper_surface,
                bottom: second.lower_surface,
            },
        ];
        let mut surfaces = vec![0.1, 0.9, 0.1, 0.5, 0.7];

        let (adjusted, reaches) = enforce_waterfall_reach_surface_levels(
            &mut surfaces,
            WaterfallReachEnvironment {
                terrain: &terrain,
                patches: &[first, second],
                levels: &levels,
                coverage: &coverage,
                owners: &owners,
            },
        );

        assert_eq!(adjusted, 4);
        assert_eq!(surfaces, vec![0.8, 0.6, 0.4, 0.2, 0.7]);
        assert_eq!(reaches.constrained, vec![true, true, true, true, false]);
        assert_eq!(
            reaches.downstream_ceiling,
            vec![f32::INFINITY, 0.6, 0.6, 0.2, f32::INFINITY]
        );
    }

    #[test]
    fn final_waterfall_face_refinement_is_unconditional_and_reprojects_the_face() {
        let spacing = WATERFALL_TARGET_EDGE_LENGTH * 0.25;
        let points = [
            Vec2::new(-spacing, -spacing),
            Vec2::new(0.0, -spacing),
            Vec2::new(-spacing, spacing),
            Vec2::new(0.0, spacing),
        ];
        let mut terrain = Mesh {
            vertices: points.iter().map(|point| point.extend(0.5)).collect(),
            triangles: vec![0, 1, 2, 2, 1, 3],
            uv: points.to_vec(),
            ..Mesh::default()
        };
        let patch = WaterfallPatch {
            river: 0,
            segment: 0,
            upper_vertex: 1,
            upper_centre: Vec2::ZERO,
            direction: Vec2::X,
            across: Vec2::Y,
            upper_surface: 0.4,
            lower_surface: 0.2,
            lower_floor: 0.15,
            half_width: WATERFALL_TARGET_EDGE_LENGTH,
            support_run: WATERFALL_TARGET_EDGE_LENGTH,
            pool: None,
        };
        let original_vertices = terrain.vertices.len();
        let mut material = SurfaceMaterial::empty(original_vertices);
        let mut coverage = vec![2; original_vertices];
        let mut surfaces = vec![0.3; original_vertices];
        let mut river_uv = vec![Vec2::ZERO; original_vertices];
        let mut owners = vec![Some(RiverOwnerKey { river: 0, node: 0 }); original_vertices];
        let mut waterfall_lips = vec![false; original_vertices];
        let mut target_half_widths = vec![patch.half_width; original_vertices];
        let mut target_depths = vec![0.05; original_vertices];

        let added = tessellate_final_waterfall_faces(
            &mut terrain,
            &mut material,
            &[patch],
            &mut coverage,
            &mut surfaces,
            &mut river_uv,
            &mut owners,
            &mut waterfall_lips,
            &mut target_half_widths,
            &mut target_depths,
        );
        recess_waterfall_notches(&mut terrain, &mut material, &[patch], &coverage);

        assert!(added > 0);
        assert!(terrain.vertices.len() > original_vertices);
        for position in &terrain.vertices {
            if !patch.contains_face_point(position.truncate()) {
                continue;
            }
            let expected =
                patch.face_surface_at(position.truncate()).unwrap() - WATERFALL_WATER_CLEARANCE;
            assert!((position.z - expected).abs() < 1.0e-6);
        }
    }

    #[test]
    fn final_downstream_cleanup_only_squishes_convex_spikes() {
        let points = (0..=4)
            .flat_map(|y| (0..=4).map(move |x| Vec2::new(x as f32 / 4.0, y as f32 / 4.0)))
            .collect::<Vec<_>>();
        let mut terrain = Mesh::delaunay(&points);
        terrain
            .vertices
            .iter_mut()
            .for_each(|position| position.z = 0.1);
        let centre = terrain
            .vertices
            .iter()
            .position(|position| position.truncate() == Vec2::splat(0.5))
            .unwrap();
        let outside = terrain
            .vertices
            .iter()
            .position(|position| position.truncate() == Vec2::ZERO)
            .unwrap();
        terrain.vertices[centre].z = 0.9;
        let mut surfaces = vec![0.2; terrain.vertices.len()];
        surfaces[centre] = 0.8;
        let downstream = vec![true; terrain.vertices.len()];

        let adjusted = squish_waterfall_downstream_spikes(&mut terrain, &mut surfaces, &downstream);

        assert_eq!(adjusted, 1);
        assert!(terrain.vertices[centre].z < 0.2);
        assert!(surfaces[centre] < 0.3);
        assert_eq!(terrain.vertices[outside].z.to_bits(), 0.1_f32.to_bits());
        assert_eq!(surfaces[outside].to_bits(), 0.2_f32.to_bits());
    }

    #[test]
    fn final_refined_surface_relaxation_follows_a_continuous_cross_channel_profile() {
        let points = (0..=6)
            .flat_map(|y| (0..=6).map(move |x| Vec2::new(x as f32 / 6.0, y as f32 / 6.0)))
            .collect::<Vec<_>>();
        let mut terrain = Mesh::delaunay(&points);
        terrain
            .vertices
            .iter_mut()
            .for_each(|vertex| vertex.z = 1.0);
        let centre = terrain
            .vertices
            .iter()
            .position(|vertex| vertex.truncate() == Vec2::splat(0.5))
            .unwrap();
        let adjacency = terrain.adjacency();
        let pinned_neighbour = adjacency[centre][0];
        let outer = terrain
            .vertices
            .iter()
            .position(|vertex| vertex.truncate() == Vec2::ZERO)
            .unwrap();
        let coverage = vec![2; terrain.vertices.len()];
        let mut surfaces = vec![1.0; terrain.vertices.len()];
        let river_uv = terrain
            .vertices
            .iter()
            .map(|vertex| Vec2::new(vertex.x - 0.5, vertex.y))
            .collect::<Vec<_>>();
        let target_half_widths = vec![0.5; terrain.vertices.len()];
        let target_depths = vec![0.25; terrain.vertices.len()];
        let mut pinned = vec![false; terrain.vertices.len()];
        pinned[pinned_neighbour] = true;
        let waterfall = WaterfallTerrainConstraints {
            patch: vec![false; terrain.vertices.len()],
            pinned,
            support: vec![false; terrain.vertices.len()],
            water_unclamped: vec![false; terrain.vertices.len()],
            terrain_ceiling: vec![f32::INFINITY; terrain.vertices.len()],
        };

        let moved = relax_refined_river_surface(
            &mut terrain,
            &coverage,
            &mut surfaces,
            &river_uv,
            &target_half_widths,
            &target_depths,
            &waterfall,
        );

        assert!(moved > 0);
        assert!(terrain.vertices[centre].z < 0.8);
        assert!(terrain.vertices[centre].z >= 0.75);
        assert_eq!(
            terrain.vertices[pinned_neighbour].z.to_bits(),
            1.0_f32.to_bits()
        );
        assert_eq!(terrain.vertices[outer].z.to_bits(), 1.0_f32.to_bits());
    }

    #[test]
    fn final_relaxation_does_not_collapse_waterfall_support_to_lower_water() {
        let points = (0..=6)
            .flat_map(|y| (0..=6).map(move |x| Vec2::new(x as f32 / 6.0, y as f32 / 6.0)))
            .collect::<Vec<_>>();
        let mut terrain = Mesh::delaunay(&points);
        terrain
            .vertices
            .iter_mut()
            .for_each(|vertex| vertex.z = 0.3);
        let centre = terrain
            .vertices
            .iter()
            .position(|vertex| vertex.truncate() == Vec2::splat(0.5))
            .unwrap();
        terrain.vertices[centre].z = 0.35;

        let coverage = vec![2; terrain.vertices.len()];
        let mut surfaces = vec![0.2; terrain.vertices.len()];
        let river_uv = vec![Vec2::ZERO; terrain.vertices.len()];
        let target_half_widths = vec![0.5; terrain.vertices.len()];
        let target_depths = vec![0.0; terrain.vertices.len()];
        let mut waterfall = WaterfallTerrainConstraints {
            patch: vec![false; terrain.vertices.len()],
            pinned: vec![false; terrain.vertices.len()],
            support: vec![false; terrain.vertices.len()],
            water_unclamped: vec![false; terrain.vertices.len()],
            terrain_ceiling: vec![f32::INFINITY; terrain.vertices.len()],
        };
        waterfall.patch[centre] = true;
        waterfall.support[centre] = true;
        let network = RiverNetwork {
            rivers: Vec::new(),
            join_vertices: Vec::new(),
            waterfalls: Vec::new(),
            river_mesh_ends: Vec::new(),
            max_flow: 1,
            max_height: 1.0,
            ocean: vec![false; terrain.vertices.len()],
            perimeter: terrain.perimeter_mask(),
            cross_sections: Vec::new(),
        };
        let waterfall_lips = vec![false; terrain.vertices.len()];

        ensure_clear_river_channel(
            &network,
            &mut terrain,
            &coverage,
            &mut surfaces,
            &waterfall_lips,
            &waterfall.pinned,
            &waterfall.patch,
        );
        assert_eq!(terrain.vertices[centre].z.to_bits(), 0.35_f32.to_bits());

        relax_refined_river_surface(
            &mut terrain,
            &coverage,
            &mut surfaces,
            &river_uv,
            &target_half_widths,
            &target_depths,
            &waterfall,
        );

        assert!(terrain.vertices[centre].z > surfaces[centre]);
        assert!(terrain.vertices[centre].z > 0.3);
    }

    #[test]
    fn waterfall_face_is_pinned_while_downstream_water_allows_terrain_to_pierce() {
        let terrain = Mesh {
            vertices: vec![
                Vec3::new(-0.1, -0.1, 0.3),
                Vec3::new(-0.1, 0.1, 0.3),
                Vec3::new(0.1, -0.1, 0.3),
                Vec3::new(0.1, 0.1, 0.3),
                Vec3::new(0.2, -0.1, 0.3),
                Vec3::new(0.2, 0.1, 0.3),
            ],
            triangles: vec![0, 2, 1, 1, 2, 3, 2, 4, 3, 3, 4, 5],
            ..Mesh::default()
        };
        let mut coverage = vec![2; terrain.vertices.len()];
        coverage[2] |= RIVER_BOUNDARY;
        coverage[3] |= RIVER_BOUNDARY;
        let pinned = 0.3 + WATERFALL_WATER_CLEARANCE;
        let surfaces = vec![0.4, 0.4, 0.9, 0.9, 0.2, 0.2];
        let river_uv = vec![Vec2::ZERO; terrain.vertices.len()];
        let waterfall = WaterfallTerrainConstraints {
            patch: vec![true; terrain.vertices.len()],
            pinned: vec![false; terrain.vertices.len()],
            support: vec![false, false, true, true, false, false],
            water_unclamped: vec![false, false, false, false, true, true],
            terrain_ceiling: vec![f32::INFINITY; terrain.vertices.len()],
        };

        let water = duplicate_river_topology(&terrain, &coverage, &surfaces, &river_uv, &waterfall);

        assert_eq!(water.triangles, terrain.triangles);
        assert_eq!(water.vertices[2].z.to_bits(), pinned.to_bits());
        assert!(water.vertices[2].z > terrain.vertices[2].z);
        assert!(water.vertices[4].z < terrain.vertices[4].z);
        assert_eq!(
            water.vertices[4].z.to_bits(),
            (surfaces[4] + RIVER_SURFACE_OFFSET).to_bits()
        );
    }

    #[cfg(any())]
    #[test]
    fn bank_grading_spreads_the_cut_beyond_the_first_ring() {
        let points: Vec<Vec2> = (0..=6)
            .flat_map(|y| (0..=6).map(move |x| Vec2::new(x as f32 / 6.0, y as f32 / 6.0)))
            .collect();
        let mut mesh = Mesh::delaunay(&points);
        mesh.vertices.iter_mut().for_each(|vertex| vertex.z = 0.18);
        let center = points
            .iter()
            .position(|point| *point == Vec2::new(0.5, 0.5))
            .unwrap();
        mesh.vertices[center].z = 0.02;
        let adjacency = mesh.adjacency();
        let first_ring = adjacency[center].to_vec();
        let node = RiverNode {
            vertex: center,
            flow: 10,
            surface: 0.03,
            position: mesh.vertices[center],
        };
        let mut scratch = BankScratch::new(mesh.vertices.len());
        let mut material = SurfaceMaterial::empty(mesh.vertices.len());
        let bedrock_rates = vec![1.0; mesh.vertices.len()];
        let control_areas = projected_vertex_control_areas(&mesh);
        let mut budget = RiverSedimentBudget::default();
        let base_width = average_edge_length(&mesh, &adjacency);
        let shelf_width = river_half_width(node.flow, 10, base_width);
        let ocean = vec![false; mesh.vertices.len()];

        {
            let mut terrain = test_river_terrain(
                &mut mesh,
                &adjacency,
                &mut material,
                &bedrock_rates,
                &control_areas,
            );
            carve_banks(
                &mut terrain,
                &[node],
                &[true],
                RiverCarveParameters {
                    downstream_surface: 0.0,
                    terminal_ocean: false,
                    max_height: 0.2,
                    max_flow: 10,
                    depth_multiplier: 1.0,
                    base_width,
                    form_waterfall_shelves: true,
                    cross_sections: &[],
                },
                &mut budget,
                &mut scratch,
                &ocean,
            );
        }

        assert!(
            first_ring
                .iter()
                .all(|&bank| (mesh.vertices[bank].z - mesh.vertices[center].z).abs() < 1.0e-6)
        );
        assert!(mesh.vertices.iter().all(|position| {
            let distance = (*position - mesh.vertices[center]).truncate().length();
            distance > shelf_width || (position.z - mesh.vertices[center].z).abs() < 1.0e-6
        }));
        assert!(mesh.vertices.iter().enumerate().any(|(vertex, position)| {
            vertex != center && !first_ring.contains(&vertex) && position.z < 0.18
        }));
        assert!(budget.carried > 0.0);
    }

    #[cfg(any())]
    #[test]
    fn submerged_channel_grades_the_ocean_bed_beyond_its_centreline() {
        let points: Vec<Vec2> = (0..=6)
            .flat_map(|y| (0..=10).map(move |x| Vec2::new(x as f32 * 0.01, y as f32 * 0.01)))
            .collect();
        let mut mesh = Mesh::delaunay(&points);
        mesh.vertices
            .iter_mut()
            .for_each(|vertex| vertex.z = -0.001);
        let vertex_at = |x: usize, y: usize| {
            points
                .iter()
                .position(|point| *point == Vec2::new(x as f32 * 0.01, y as f32 * 0.01))
                .unwrap()
        };
        let centre = vertex_at(5, 3);
        let first_bank = vertex_at(5, 2);
        let second_bank = vertex_at(5, 1);
        let distant = vertex_at(0, 0);
        mesh.vertices[centre].z = -0.02;
        let node = RiverNode {
            vertex: centre,
            flow: 10,
            surface: -0.019,
            position: mesh.vertices[centre],
        };
        let adjacency = mesh.adjacency();
        let base_width = average_edge_length(&mesh, &adjacency);
        let mut scratch = BankScratch::new(mesh.vertices.len());
        let mut material = SurfaceMaterial::empty(mesh.vertices.len());
        let bedrock_rates = vec![1.0; mesh.vertices.len()];
        let control_areas = projected_vertex_control_areas(&mesh);
        let mut budget = RiverSedimentBudget::default();
        let ocean = vec![true; mesh.vertices.len()];

        {
            let mut terrain = test_river_terrain(
                &mut mesh,
                &adjacency,
                &mut material,
                &bedrock_rates,
                &control_areas,
            );
            carve_banks(
                &mut terrain,
                &[node],
                &[false],
                RiverCarveParameters {
                    downstream_surface: f32::NEG_INFINITY,
                    terminal_ocean: true,
                    max_height: 0.2,
                    max_flow: 10,
                    depth_multiplier: 1.0,
                    base_width,
                    form_waterfall_shelves: false,
                    cross_sections: &[],
                },
                &mut budget,
                &mut scratch,
                &ocean,
            );
        }

        assert!(mesh.vertices[first_bank].z < -0.001);
        assert!(mesh.vertices[second_bank].z < -0.001);
        assert_eq!(mesh.vertices[distant].z.to_bits(), (-0.001_f32).to_bits());
        assert!(budget.carried > 0.0);
    }

    #[cfg(any())]
    #[test]
    fn waterfall_banks_follow_the_nearest_terrace_instead_of_the_lowest_one() {
        let points: Vec<Vec2> = (0..=6)
            .flat_map(|y| (0..=6).map(move |x| Vec2::new(x as f32 / 6.0, y as f32 / 6.0)))
            .collect();
        let mut mesh = Mesh::delaunay(&points);
        mesh.vertices.iter_mut().for_each(|vertex| vertex.z = 0.18);
        let vertex_at = |x: usize, y: usize| {
            points
                .iter()
                .position(|point| *point == Vec2::new(x as f32 / 6.0, y as f32 / 6.0))
                .unwrap()
        };
        let upper = vertex_at(2, 3);
        let lower = vertex_at(3, 3);
        let upper_bank = vertex_at(2, 2);
        let lower_bank = vertex_at(3, 2);
        mesh.vertices[upper].z = 0.08;
        mesh.vertices[lower].z = 0.02;
        mesh.vertices[upper_bank].z = 0.01;
        let nodes = [
            RiverNode {
                vertex: upper,
                flow: 10,
                surface: 0.09,
                position: mesh.vertices[upper],
            },
            RiverNode {
                vertex: lower,
                flow: 10,
                surface: 0.03,
                position: mesh.vertices[lower],
            },
        ];
        let adjacency = mesh.adjacency();
        let base_width = average_edge_length(&mesh, &adjacency);
        let mut scratch = BankScratch::new(mesh.vertices.len());
        let mut material = SurfaceMaterial::empty(mesh.vertices.len());
        let bedrock_rates = vec![1.0; mesh.vertices.len()];
        let control_areas = projected_vertex_control_areas(&mesh);
        let mut budget = RiverSedimentBudget::default();
        let ocean = vec![false; mesh.vertices.len()];

        {
            let mut terrain = test_river_terrain(
                &mut mesh,
                &adjacency,
                &mut material,
                &bedrock_rates,
                &control_areas,
            );
            carve_banks(
                &mut terrain,
                &nodes,
                &[true, false],
                RiverCarveParameters {
                    downstream_surface: 0.0,
                    terminal_ocean: false,
                    max_height: 0.2,
                    max_flow: 10,
                    depth_multiplier: 1.0,
                    base_width,
                    form_waterfall_shelves: true,
                    cross_sections: &[],
                },
                &mut budget,
                &mut scratch,
                &ocean,
            );
        }

        assert!((mesh.vertices[upper_bank].z - mesh.vertices[upper].z).abs() < 1.0e-6);
        assert!((mesh.vertices[lower_bank].z - mesh.vertices[lower].z).abs() < 1.0e-6);
        assert!(mesh.vertices[upper_bank].z > mesh.vertices[lower_bank].z + 0.05);
    }

    #[test]
    fn all_channel_footprints_use_one_ring_regardless_of_flow() {
        let points: Vec<Vec2> = (0..=8)
            .flat_map(|y| (0..=8).map(move |x| Vec2::new(x as f32 / 8.0, y as f32 / 8.0)))
            .collect();
        let terrain = Mesh::delaunay(&points);
        let center = points
            .iter()
            .position(|point| *point == Vec2::new(0.5, 0.5))
            .unwrap();
        let network = |flow| RiverNetwork {
            rivers: vec![River {
                nodes: vec![RiverNode {
                    vertex: center,
                    flow,
                    surface: 0.01,
                    position: terrain.vertices[center],
                }],
                join: None,
            }],
            join_vertices: vec![None],
            waterfalls: vec![vec![false]],
            river_mesh_ends: vec![None],
            max_flow: 100,
            max_height: 0.2,
            ocean: vec![false; terrain.vertices.len()],
            perimeter: vec![false; terrain.vertices.len()],
            cross_sections: Vec::new(),
        };
        let adjacency = terrain.adjacency();
        let narrow = build_river_footprint(&network(1), &terrain, &adjacency, false);
        let broad = build_river_footprint(&network(100), &terrain, &adjacency, false);

        for footprint in [&narrow, &broad] {
            assert!(
                footprint
                    .ring_distance
                    .iter()
                    .zip(&footprint.coverage)
                    .all(|(&distance, &coverage)| coverage == 0 || distance <= 1)
            );
            assert!(footprint.ring_distance.contains(&1));
        }
        assert_eq!(narrow.coverage, broad.coverage);
        assert!(
            broad.owner[center].unwrap().target_half_width
                > narrow.owner[center].unwrap().target_half_width
        );
    }

    #[test]
    fn channel_targets_only_widen_and_deepen_downstream() {
        let river = River {
            nodes: [1, 4, 2, 16]
                .into_iter()
                .enumerate()
                .map(|(vertex, flow)| RiverNode {
                    vertex,
                    flow,
                    surface: 0.1,
                    position: Vec3::new(vertex as f32, 0.0, 0.1),
                })
                .collect(),
            join: None,
        };
        let settings = RiverChannelSettings {
            source_width: 0.01,
            maximum_width: 0.08,
            source_depth: 0.001,
            maximum_depth: 0.01,
        };
        let sections = target_cross_sections(&[river], settings);

        assert!(sections[0].windows(2).all(|pair| pair[0].target_half_width
            <= pair[1].target_half_width
            && pair[0].nominal_depth <= pair[1].nominal_depth));
        assert_eq!(
            sections[0][0].target_half_width.to_bits(),
            (settings.source_width * 0.5).to_bits()
        );
        assert_eq!(
            sections[0].last().unwrap().target_half_width.to_bits(),
            (settings.maximum_width * 0.5).to_bits()
        );
    }

    #[test]
    fn width_error_adds_or_removes_a_bounded_depth_compensation() {
        let section = RiverCrossSection {
            target_half_width: 0.02,
            nominal_depth: 0.004,
            achieved_width: 0.0,
            required_depth: 0.0,
        };

        let exact = compensated_channel_depth(section, 0.04, 0.02);
        let broad = compensated_channel_depth(section, 0.06, 0.02);
        let narrow = compensated_channel_depth(section, 0.02, 0.02);

        assert!(broad < exact);
        assert!(narrow > broad);
        assert_eq!(exact.to_bits(), section.nominal_depth.to_bits());
        assert_eq!(
            compensated_channel_depth(section, 1.0e-6, 0.006).to_bits(),
            0.006_f32.to_bits()
        );
    }

    #[test]
    fn channel_nodes_keep_their_individual_floor_targets() {
        let mut mesh = Mesh {
            vertices: vec![Vec3::new(0.0, 0.0, 0.1), Vec3::new(1.0, 0.0, 0.1)],
            ..Mesh::default()
        };
        let adjacency = mesh.adjacency();
        let nodes = [
            RiverNode {
                vertex: 0,
                flow: 1,
                surface: 0.1,
                position: mesh.vertices[0],
            },
            RiverNode {
                vertex: 1,
                flow: 2,
                surface: 0.1,
                position: mesh.vertices[1],
            },
        ];
        let mut material = SurfaceMaterial::empty(mesh.vertices.len());
        let bedrock_rates = vec![1.0; mesh.vertices.len()];
        let control_areas = vec![1.0; mesh.vertices.len()];
        let mut budget = RiverSedimentBudget::default();
        {
            let mut terrain = test_river_terrain(
                &mut mesh,
                &adjacency,
                &mut material,
                &bedrock_rates,
                &control_areas,
            );
            carve_bed_reach(&mut terrain, &nodes, &[0.09, 0.04], false, &mut budget);
        }

        assert!((mesh.vertices[0].z - 0.09).abs() < 1.0e-6);
        assert!((mesh.vertices[1].z - 0.04).abs() < 1.0e-6);
    }

    #[test]
    fn channel_rings_are_shaped_without_changing_topology() {
        let points: Vec<Vec2> = (0..=8)
            .flat_map(|y| (0..=8).map(move |x| Vec2::new(x as f32 / 8.0, y as f32 / 8.0)))
            .collect();
        let mut mesh = Mesh::delaunay(&points);
        mesh.vertices.iter_mut().for_each(|vertex| vertex.z = 0.1);
        let path: Vec<usize> = [2, 4, 6]
            .into_iter()
            .map(|x| {
                points
                    .iter()
                    .position(|point| *point == Vec2::new(x as f32 / 8.0, 0.5))
                    .unwrap()
            })
            .collect();
        let nodes: Vec<RiverNode> = path
            .iter()
            .enumerate()
            .map(|(index, &vertex)| RiverNode {
                vertex,
                flow: (index + 1) as u32,
                surface: 0.11,
                position: mesh.vertices[vertex],
            })
            .collect();
        let adjacency = mesh.adjacency();
        let perimeter = mesh.perimeter_mask();
        let mut network = RiverNetwork {
            rivers: vec![River { nodes, join: None }],
            join_vertices: vec![None],
            waterfalls: vec![vec![false; path.len()]],
            river_mesh_ends: vec![None],
            max_flow: 3,
            max_height: 0.2,
            ocean: vec![false; mesh.vertices.len()],
            perimeter,
            cross_sections: Vec::new(),
        };
        let original_vertices = mesh.vertices.clone();
        let settings = RiverChannelSettings {
            source_width: 0.04,
            maximum_width: 0.20,
            source_depth: 0.004,
            maximum_depth: 0.012,
        };
        network.cross_sections = target_cross_sections(&network.rivers, settings);
        network.form_channel_rings(&mut mesh, &adjacency);
        let footprint = build_river_footprint(&network, &mesh, &adjacency, false);
        update_achieved_cross_sections(&mut network, &mesh, &footprint, settings.maximum_depth);

        assert_eq!(mesh.vertices.len(), original_vertices.len());
        assert!(
            mesh.vertices
                .iter()
                .zip(&original_vertices)
                .all(|(current, original)| current.z.to_bits() == original.z.to_bits())
        );
        assert!(mesh.triangles.chunks_exact(3).all(|triangle| {
            let [a, b, c] = [
                mesh.vertices[triangle[0] as usize].truncate(),
                mesh.vertices[triangle[1] as usize].truncate(),
                mesh.vertices[triangle[2] as usize].truncate(),
            ];
            (b - a).perp_dot(c - a).abs() > 1.0e-9
        }));
        assert!(
            network.cross_sections[0]
                .windows(2)
                .all(|pair| pair[0].target_half_width <= pair[1].target_half_width)
        );
        assert!(network.cross_sections[0].iter().all(|section| {
            section.required_depth >= section.nominal_depth * 0.5
                && section.required_depth <= section.nominal_depth * 1.5 + f32::EPSILON
        }));
    }

    #[test]
    fn corridor_rings_share_the_centre_floor_then_smooth_into_the_banks() {
        let points: Vec<Vec2> = (0..=8)
            .flat_map(|y| (0..=8).map(move |x| Vec2::new(x as f32 / 8.0, y as f32 / 8.0)))
            .collect();
        let mut mesh = Mesh::delaunay(&points);
        mesh.vertices.iter_mut().for_each(|vertex| vertex.z = 0.2);
        let centre = points
            .iter()
            .position(|point| *point == Vec2::splat(0.5))
            .unwrap();
        let adjacency = mesh.adjacency();
        let network = RiverNetwork {
            rivers: vec![River {
                nodes: vec![RiverNode {
                    vertex: centre,
                    flow: 1,
                    surface: 0.1,
                    position: mesh.vertices[centre],
                }],
                join: None,
            }],
            join_vertices: vec![None],
            waterfalls: vec![vec![false]],
            river_mesh_ends: vec![None],
            max_flow: 1,
            max_height: 0.2,
            ocean: vec![false; mesh.vertices.len()],
            perimeter: mesh.perimeter_mask(),
            cross_sections: vec![vec![RiverCrossSection {
                target_half_width: 0.2,
                nominal_depth: 0.02,
                achieved_width: 0.4,
                required_depth: 0.02,
            }]],
        };
        network.form_channel_rings(&mut mesh, &adjacency);
        let footprint = build_river_footprint(&network, &mesh, &adjacency, false);
        let boundary = footprint
            .coverage
            .iter()
            .map(|&coverage| is_river_boundary(coverage))
            .collect::<Vec<_>>();
        let naturally_low = boundary
            .iter()
            .enumerate()
            .find_map(|(vertex, &is_boundary)| is_boundary.then_some(vertex))
            .unwrap();
        let apron_vertex = boundary
            .iter()
            .enumerate()
            .filter(|(_, is_boundary)| **is_boundary)
            .flat_map(|(vertex, _)| adjacency[vertex].iter().copied())
            .find(|&vertex| footprint.coverage[vertex] == 0)
            .unwrap();
        mesh.vertices[naturally_low].z = 0.05;
        let mut material = SurfaceMaterial::empty(mesh.vertices.len());
        let bedrock_rates = vec![1.0; mesh.vertices.len()];
        let control_areas = projected_vertex_control_areas(&mesh);
        let mut budgets = vec![RiverSedimentBudget::default()];
        let parameters = RiverChannelParameters {
            depth_multiplier: 1.0,
        };
        let carve = {
            let mut terrain = test_river_terrain(
                &mut mesh,
                &adjacency,
                &mut material,
                &bedrock_rates,
                &control_areas,
            );
            let carve =
                carve_river_corridor(&network, &mut terrain, &footprint, parameters, &mut budgets);
            lower_river_surroundings(
                &network,
                &mut terrain,
                &footprint,
                parameters,
                &carve,
                &mut budgets,
            );
            carve
        };
        let floor = 0.08;
        assert!(
            mesh.vertices
                .iter()
                .enumerate()
                .filter(|(vertex, _)| footprint.coverage[*vertex] != 0)
                .all(|(vertex, position)| if vertex == naturally_low {
                    (position.z - 0.05).abs() < 1.0e-6
                } else {
                    (position.z - floor).abs() < 1.0e-6
                })
        );

        assert!(mesh.vertices[apron_vertex].z < 0.2);
        smooth_river_corridor(
            &network, &mut mesh, &adjacency, &footprint, parameters, &carve,
        );

        assert_smoothed_corridor(&mesh, &footprint, &boundary, naturally_low, floor);
    }

    fn assert_smoothed_corridor(
        mesh: &Mesh,
        footprint: &RiverFootprint,
        boundary: &[bool],
        naturally_low: usize,
        floor: f32,
    ) {
        assert!(mesh.vertices.iter().enumerate().all(|(vertex, position)| {
            footprint.coverage[vertex] == 0
                || ((if vertex == naturally_low { 0.05 } else { floor })
                    ..=0.1 - RIVER_SURFACE_OFFSET)
                    .contains(&position.z)
        }));
        assert!(mesh.vertices[naturally_low].z <= 0.05);
        assert!(mesh.vertices.iter().enumerate().any(|(vertex, position)| {
            vertex != naturally_low && boundary[vertex] && position.z > floor + f32::EPSILON
        }));
    }

    #[test]
    fn river_mesh_is_hard_clipped_at_sea_level() {
        let points: Vec<Vec2> = (0..=8)
            .flat_map(|y| (0..=8).map(move |x| Vec2::new(x as f32 / 8.0, y as f32 / 8.0)))
            .collect();
        let mut terrain = Mesh::delaunay(&points);
        let path: Vec<usize> = (1..=7)
            .map(|x| {
                points
                    .iter()
                    .position(|point| *point == Vec2::new(x as f32 / 8.0, 0.5))
                    .unwrap()
            })
            .collect();
        let nodes: Vec<RiverNode> = path
            .iter()
            .enumerate()
            .map(|(index, &vertex)| RiverNode {
                vertex,
                flow: 1,
                surface: if index < 3 { 0.01 } else { -0.001 },
                position: terrain.vertices[vertex],
            })
            .collect();
        let omitted_terminal = terrain.vertices[*path.last().unwrap()].truncate();
        let network = RiverNetwork {
            rivers: vec![River { nodes, join: None }],
            join_vertices: vec![None],
            waterfalls: vec![vec![false, false, true, false, false, false, false]],
            river_mesh_ends: vec![Some(3)],
            max_flow: 1,
            max_height: 0.2,
            ocean: vec![false; terrain.vertices.len()],
            perimeter: vec![false; terrain.vertices.len()],
            cross_sections: Vec::new(),
        };
        let adjacency = terrain.adjacency();
        let river_mesh = build_test_river_mesh(&network, &mut terrain, &adjacency);

        assert!(river_mesh.vertices.iter().all(|vertex| vertex.z >= 0.0));
        assert!(river_mesh.vertices.iter().any(|vertex| vertex.z == 0.0));
        assert!(
            river_mesh
                .vertices
                .iter()
                .all(|vertex| vertex.truncate() != omitted_terminal)
        );
    }

    #[test]
    fn low_river_banks_are_lifted_with_a_smooth_outer_falloff() {
        let points: Vec<Vec2> = (0..=16)
            .flat_map(|y| {
                (0..=16).map(move |x| {
                    Vec2::new(
                        x as f32 * 10.0 / ISLAND_WORLD_METRES,
                        y as f32 * 10.0 / ISLAND_WORLD_METRES,
                    )
                })
            })
            .collect();
        let mut terrain = Mesh::delaunay(&points);
        let adjacency = terrain.adjacency();
        let perimeter = terrain.perimeter_mask();
        let mut coverage = terrain
            .vertices
            .iter()
            .map(|vertex| {
                u8::from(
                    (60.0 / ISLAND_WORLD_METRES..=100.0 / ISLAND_WORLD_METRES).contains(&vertex.x)
                        && (50.0 / ISLAND_WORLD_METRES..=110.0 / ISLAND_WORLD_METRES)
                            .contains(&vertex.y),
                )
            })
            .collect::<Vec<_>>();
        mark_river_boundary(&adjacency, &perimeter, &mut coverage);
        let surfaces = vec![0.1; terrain.vertices.len()];
        let target_half_widths = vec![10.0 / ISLAND_WORLD_METRES; terrain.vertices.len()];
        let ocean = vec![false; terrain.vertices.len()];
        let protected = vec![false; terrain.vertices.len()];
        let banks = river_topology_masks(&terrain, &coverage).1;

        let mut protected_terrain = terrain.clone();
        let protected_patch = vec![true; terrain.vertices.len()];
        let protected_original = protected_terrain.vertices.clone();
        let protected_raised = lift_river_banks_to_surface(
            &mut protected_terrain,
            &adjacency,
            &coverage,
            &surfaces,
            &target_half_widths,
            RiverBankLiftMasks {
                ocean: &ocean,
                perimeter: &perimeter,
                protected: &protected_patch,
            },
        );
        assert_eq!(protected_raised, 0);
        assert_eq!(protected_terrain.vertices, protected_original);

        let raised = lift_river_banks_to_surface(
            &mut terrain,
            &adjacency,
            &coverage,
            &surfaces,
            &target_half_widths,
            RiverBankLiftMasks {
                ocean: &ocean,
                perimeter: &perimeter,
                protected: &protected,
            },
        );

        let eligible_banks = banks
            .iter()
            .enumerate()
            .filter(|(vertex, is_bank)| **is_bank && !perimeter[*vertex])
            .count();
        assert!(
            raised > eligible_banks,
            "raised={raised}, banks={}, outer={}",
            eligible_banks,
            terrain
                .vertices
                .iter()
                .enumerate()
                .filter(|(vertex, position)| coverage[*vertex] == 0 && position.z > 0.0)
                .count()
        );
        assert!(
            terrain
                .vertices
                .iter()
                .enumerate()
                .filter(|(vertex, _)| banks[*vertex] && !perimeter[*vertex])
                .all(|(_, vertex)| (vertex.z - 0.1).abs() < 1.0e-6)
        );
        assert!(
            terrain
                .vertices
                .iter()
                .enumerate()
                .any(|(vertex, position)| {
                    coverage[vertex] == 0 && position.z > 0.0 && position.z < 0.1
                })
        );
        let distant = points
            .iter()
            .position(|point| {
                *point == Vec2::new(10.0 / ISLAND_WORLD_METRES, 80.0 / ISLAND_WORLD_METRES)
            })
            .unwrap();
        assert_eq!(terrain.vertices[distant].z.to_bits(), 0.0_f32.to_bits());
    }

    #[test]
    fn river_corridor_refinement_caps_large_faces_in_the_bank_apron() {
        let points: Vec<Vec2> = (0..=6)
            .flat_map(|y| {
                (0..=6).map(move |x| {
                    Vec2::new(
                        x as f32 * 8.0 / ISLAND_WORLD_METRES,
                        y as f32 * 8.0 / ISLAND_WORLD_METRES,
                    )
                })
            })
            .collect();
        let mut terrain = Mesh::delaunay(&points);
        let mut coverage = terrain
            .vertices
            .iter()
            .map(|vertex| {
                u8::from(
                    (16.0 / ISLAND_WORLD_METRES..=32.0 / ISLAND_WORLD_METRES).contains(&vertex.x)
                        && (8.0 / ISLAND_WORLD_METRES..=40.0 / ISLAND_WORLD_METRES)
                            .contains(&vertex.y),
                )
            })
            .collect::<Vec<_>>();
        let adjacency = terrain.adjacency();
        let perimeter = terrain.perimeter_mask();
        mark_river_boundary(&adjacency, &perimeter, &mut coverage);
        let original_vertex_count = terrain.vertices.len();
        let mut material = SurfaceMaterial::empty(original_vertex_count);
        let mut surfaces = vec![0.05; original_vertex_count];
        let mut river_uv = vec![Vec2::ZERO; original_vertex_count];
        let mut owners = vec![None; original_vertex_count];
        let mut waterfall_lips = vec![false; original_vertex_count];
        let mut target_half_widths = vec![2.0 / ISLAND_WORLD_METRES; original_vertex_count];
        let mut target_depths = vec![0.5 / ISLAND_WORLD_METRES; original_vertex_count];

        let added = refine_river_corridor_mesh(
            &mut terrain,
            &mut material,
            &mut coverage,
            &mut surfaces,
            &mut river_uv,
            &mut owners,
            &mut waterfall_lips,
            &mut target_half_widths,
            &mut target_depths,
        );

        assert!(added > 0);
        assert!(terrain.vertices.len() > original_vertex_count);
        assert_eq!(material.depths().len(), terrain.vertices.len());
        assert_eq!(coverage.len(), terrain.vertices.len());
        assert_eq!(surfaces.len(), terrain.vertices.len());
        assert_eq!(river_uv.len(), terrain.vertices.len());
        assert_eq!(owners.len(), terrain.vertices.len());
        assert_eq!(waterfall_lips.len(), terrain.vertices.len());
        assert_eq!(target_half_widths.len(), terrain.vertices.len());
        assert_eq!(target_depths.len(), terrain.vertices.len());

        let adjacency = terrain.adjacency();
        let targets = river_refinement_edge_targets(
            &adjacency,
            &coverage,
            &target_half_widths,
            RIVER_REFINEMENT_APRON_RINGS,
        );
        assert!(terrain.triangles.chunks_exact(3).all(|triangle| {
            let indices = [
                triangle[0] as usize,
                triangle[1] as usize,
                triangle[2] as usize,
            ];
            let target = indices
                .iter()
                .map(|&vertex| targets[vertex])
                .fold(f32::INFINITY, f32::min);
            if !target.is_finite() {
                return true;
            }
            let [a, b, c] = indices.map(|vertex| terrain.vertices[vertex].truncate());
            a.distance(b).max(b.distance(c)).max(c.distance(a)) <= target * 1.001
        }));
    }

    #[test]
    fn final_channel_integrity_lowers_the_core_and_keeps_banks_pinned() {
        let points: Vec<Vec2> = (0..=8)
            .flat_map(|y| (0..=8).map(move |x| Vec2::new(x as f32 / 8.0, y as f32 / 8.0)))
            .collect();
        let mut terrain = Mesh::delaunay(&points);
        terrain
            .vertices
            .iter_mut()
            .for_each(|vertex| vertex.z = 0.12);
        let center = points
            .iter()
            .position(|point| *point == Vec2::new(0.5, 0.5))
            .unwrap();
        let adjacency = terrain.adjacency();
        let perimeter = terrain.perimeter_mask();
        let mut coverage = terrain
            .vertices
            .iter()
            .map(|vertex| {
                u8::from((0.25..=0.75).contains(&vertex.x) && (0.25..=0.75).contains(&vertex.y))
            })
            .collect::<Vec<_>>();
        mark_river_boundary(&adjacency, &perimeter, &mut coverage);
        let surfaces = vec![0.1; terrain.vertices.len()];
        let waterfall_lips = vec![false; terrain.vertices.len()];
        let banks = river_topology_masks(&terrain, &coverage).1;
        let network = RiverNetwork {
            rivers: vec![River {
                nodes: vec![RiverNode {
                    vertex: center,
                    flow: 100,
                    surface: 0.1,
                    position: terrain.vertices[center],
                }],
                join: None,
            }],
            join_vertices: vec![None],
            waterfalls: vec![vec![false]],
            river_mesh_ends: vec![None],
            max_flow: 100,
            max_height: 0.2,
            ocean: vec![false; terrain.vertices.len()],
            perimeter: vec![false; terrain.vertices.len()],
            cross_sections: vec![vec![RiverCrossSection {
                target_half_width: 0.02,
                nominal_depth: 0.02,
                achieved_width: 0.02,
                required_depth: 0.02,
            }]],
        };

        let lowered = ensure_clear_river_channel(
            &network,
            &mut terrain,
            &coverage,
            &surfaces,
            &waterfall_lips,
            &vec![false; coverage.len()],
            &vec![false; coverage.len()],
        );

        assert!(lowered > 0);
        assert!((terrain.vertices[center].z - 0.08).abs() < 1.0e-6);
        assert!(adjacency[center].iter().copied().any(|neighbour| {
            !banks[neighbour] && terrain.vertices[neighbour].z <= 0.09 + 1.0e-6
        }));
        assert!(
            terrain
                .vertices
                .iter()
                .enumerate()
                .filter(|(vertex, _)| banks[*vertex])
                .all(|(_, vertex)| (vertex.z - 0.12).abs() < 1.0e-6)
        );
    }

    #[test]
    fn river_mesh_banks_never_climb_terrain_and_centreline_stays_below_water() {
        let points: Vec<Vec2> = (0..=8)
            .flat_map(|y| (0..=8).map(move |x| Vec2::new(x as f32 / 8.0, y as f32 / 8.0)))
            .collect();
        let mut terrain = Mesh::delaunay(&points);
        for vertex in &mut terrain.vertices {
            vertex.z = vertex.x.mul_add(0.03, vertex.y * 0.02);
        }
        let center = points
            .iter()
            .position(|point| *point == Vec2::new(0.5, 0.5))
            .unwrap();
        let surface = 0.025;
        let network = RiverNetwork {
            rivers: vec![River {
                nodes: vec![RiverNode {
                    vertex: center,
                    flow: 100,
                    surface,
                    position: terrain.vertices[center],
                }],
                join: None,
            }],
            join_vertices: vec![None],
            waterfalls: vec![vec![false]],
            river_mesh_ends: vec![None],
            max_flow: 100,
            max_height: 0.2,
            ocean: vec![false; terrain.vertices.len()],
            perimeter: vec![false; terrain.vertices.len()],
            cross_sections: Vec::new(),
        };
        let adjacency = terrain.adjacency();
        let river_mesh = build_test_river_mesh(&network, &mut terrain, &adjacency);
        assert!(!river_mesh.triangles.is_empty());
        let banks = river_mesh.perimeter_mask();
        for &water_vertex in &river_mesh.triangles {
            let water_vertex = water_vertex as usize;
            let water = river_mesh.vertices[water_vertex];
            let ground = terrain
                .vertices
                .iter()
                .find(|ground| {
                    ground.x.to_bits() == water.x.to_bits()
                        && ground.y.to_bits() == water.y.to_bits()
                })
                .expect("river topology should share its XY vertices with the terrain");
            if banks[water_vertex] {
                assert!(
                    water.z <= ground.z + 1.0e-6,
                    "river bank at {water:?} climbs terrain vertex {ground:?}"
                );
                assert!(
                    (water.z - surface).abs() < 1.0e-6,
                    "river bank at {water:?} was pulled below surface {surface}"
                );
            } else {
                assert!(
                    water.z + 1.0e-6 >= ground.z + RIVER_SURFACE_OFFSET,
                    "interior water at {water:?} is below terrain vertex {ground:?}"
                );
            }
        }
        assert!(terrain.vertices[center].z <= surface - RIVER_SURFACE_OFFSET);
        assert!(
            river_mesh
                .triangles
                .iter()
                .all(|&vertex| (vertex as usize) < river_mesh.vertices.len())
        );
    }

    #[test]
    fn river_mesh_omits_triangles_made_only_from_its_outer_ring() {
        let points: Vec<Vec2> = (0..=8)
            .flat_map(|y| (0..=8).map(move |x| Vec2::new(x as f32 / 8.0, y as f32 / 8.0)))
            .collect();
        let mut terrain = Mesh::delaunay(&points);
        let center = points
            .iter()
            .position(|point| *point == Vec2::new(0.5, 0.5))
            .unwrap();
        let surface = 0.12;
        let network = RiverNetwork {
            rivers: vec![River {
                nodes: vec![RiverNode {
                    vertex: center,
                    flow: 100,
                    surface,
                    position: terrain.vertices[center],
                }],
                join: None,
            }],
            join_vertices: vec![None],
            waterfalls: vec![vec![false]],
            river_mesh_ends: vec![None],
            max_flow: 100,
            max_height: 0.2,
            ocean: vec![false; terrain.vertices.len()],
            perimeter: vec![false; terrain.vertices.len()],
            cross_sections: Vec::new(),
        };
        let adjacency = terrain.adjacency();
        let river_mesh = build_test_river_mesh(&network, &mut terrain, &adjacency);

        assert!(!river_mesh.triangles.is_empty());
        assert!(river_mesh.triangles.chunks_exact(3).all(|triangle| {
            triangle.iter().any(|&vertex| {
                river_mesh.vertices[vertex as usize].z > RIVER_SURFACE_OFFSET + 1.0e-6
            })
        }));
    }

    #[test]
    fn river_mesh_extraction_refines_the_authoritative_terrain_topology() {
        let points: Vec<Vec2> = (0..=8)
            .flat_map(|y| (0..=8).map(move |x| Vec2::new(x as f32 / 8.0, y as f32 / 8.0)))
            .collect();
        let mut terrain = Mesh::delaunay(&points);
        let center = points
            .iter()
            .position(|point| *point == Vec2::new(0.5, 0.5))
            .unwrap();
        let network = RiverNetwork {
            rivers: vec![River {
                nodes: vec![RiverNode {
                    vertex: center,
                    flow: 100,
                    surface: 0.12,
                    position: terrain.vertices[center],
                }],
                join: None,
            }],
            join_vertices: vec![None],
            waterfalls: vec![vec![false]],
            river_mesh_ends: vec![None],
            max_flow: 100,
            max_height: 0.2,
            ocean: vec![false; terrain.vertices.len()],
            perimeter: vec![false; terrain.vertices.len()],
            cross_sections: Vec::new(),
        };
        let original_vertices = terrain.vertices.clone();
        let adjacency = terrain.adjacency();

        let river_mesh = build_test_river_mesh(&network, &mut terrain, &adjacency);
        assert!(terrain.vertices.len() > original_vertices.len());
        assert!(
            terrain.vertices[..original_vertices.len()]
                .iter()
                .zip(&original_vertices)
                .all(|(refined, original)| refined.truncate() == original.truncate())
        );
        assert!(
            river_mesh
                .triangles
                .iter()
                .all(|&vertex| (vertex as usize) < river_mesh.vertices.len())
        );
    }

    #[cfg(any())]
    #[test]
    fn river_refinement_preserves_bank_heights_while_lowering_the_bed() {
        let points: Vec<Vec2> = (0..=4)
            .flat_map(|y| (0..=4).map(move |x| Vec2::new(x as f32 / 4.0, y as f32 / 4.0)))
            .collect();
        let mut terrain = Mesh::delaunay(&points);
        terrain
            .vertices
            .iter_mut()
            .for_each(|vertex| vertex.z = 0.2);
        let under_river: Vec<bool> = terrain
            .vertices
            .iter()
            .map(|vertex| (0.25..=0.75).contains(&vertex.x) && (0.25..=0.75).contains(&vertex.y))
            .collect();
        let bank: Vec<bool> = terrain
            .vertices
            .iter()
            .zip(&under_river)
            .map(|(vertex, &under_river)| {
                under_river
                    && ([0.25_f32.to_bits(), 0.75_f32.to_bits()].contains(&vertex.x.to_bits())
                        || [0.25_f32.to_bits(), 0.75_f32.to_bits()].contains(&vertex.y.to_bits()))
            })
            .collect();
        let center = terrain
            .vertices
            .iter()
            .position(|vertex| {
                vertex
                    .truncate()
                    .abs_diff_eq(Vec2::splat(0.5), f32::EPSILON)
            })
            .unwrap();
        let adjacency = terrain.adjacency();
        let surfaces = vec![0.05; terrain.vertices.len()];

        smooth_river_terrain_vertices(&mut terrain, &adjacency, &under_river, &bank, &surfaces);

        assert!(
            terrain
                .vertices
                .iter()
                .zip(&bank)
                .filter(|(_, is_bank)| **is_bank)
                .all(|(vertex, _)| (vertex.z - 0.2).abs() <= f32::EPSILON)
        );
        assert!(terrain.vertices[center].z <= 0.05 - RIVER_SURFACE_OFFSET);
    }

    #[test]
    fn river_mesh_extraction_repairs_an_isolated_sharp_bed_point() {
        let points: Vec<Vec2> = (0..=8)
            .flat_map(|y| (0..=8).map(move |x| Vec2::new(x as f32 / 8.0, y as f32 / 8.0)))
            .collect();
        let mut terrain = Mesh::delaunay(&points);
        let center = points
            .iter()
            .position(|point| *point == Vec2::new(0.5, 0.5))
            .unwrap();
        terrain.vertices[center].z = -0.1;
        let network = RiverNetwork {
            rivers: vec![River {
                nodes: vec![RiverNode {
                    vertex: center,
                    flow: 100,
                    surface: 0.12,
                    position: terrain.vertices[center],
                }],
                join: None,
            }],
            join_vertices: vec![None],
            waterfalls: vec![vec![false]],
            river_mesh_ends: vec![None],
            max_flow: 100,
            max_height: 0.2,
            ocean: vec![false; terrain.vertices.len()],
            perimeter: vec![false; terrain.vertices.len()],
            cross_sections: Vec::new(),
        };
        let original_vertex_count = terrain.vertices.len();
        let adjacency = terrain.adjacency();

        let river_mesh = build_test_river_mesh(&network, &mut terrain, &adjacency);

        assert!(terrain.vertices.len() > original_vertex_count);
        assert!(terrain.vertices[center].z > -0.1);
        assert!(terrain.vertices[center].z < 0.12);
        assert!(
            river_mesh
                .vertices
                .iter()
                .any(|vertex| vertex.truncate() == terrain.vertices[center].truncate())
        );
    }

    #[test]
    fn river_mouth_reuses_the_last_existing_waterfall_for_its_submerged_channel() {
        let mut mesh = Mesh {
            vertices: (0..12)
                .map(|index| {
                    let height = if index >= 8 {
                        -0.000_01
                    } else {
                        0.08 - index as f32 * 0.008
                    };
                    Vec3::new(index as f32 * 0.01, 0.5, height)
                })
                .collect(),
            ..Mesh::default()
        };
        let mut nodes: Vec<RiverNode> = mesh
            .vertices
            .iter()
            .copied()
            .enumerate()
            .map(|(vertex, position)| RiverNode {
                vertex,
                flow: 10,
                surface: position.z.max(0.0),
                position,
            })
            .collect();
        let original_waterfall_lip = mesh.vertices[6].z;
        let mut material = SurfaceMaterial::empty(mesh.vertices.len());
        let bedrock_rates = vec![1.0; mesh.vertices.len()];
        let control_areas = vec![1.0; mesh.vertices.len()];
        let mut budget = RiverSedimentBudget::default();
        let mut waterfalls = vec![false; nodes.len()];
        waterfalls[3] = true;
        waterfalls[6] = true;
        let ocean: Vec<bool> = (0..nodes.len()).map(|index| index >= 8).collect();
        let ocean_entry = river_ocean_entry(&nodes, &ocean).unwrap();
        let mouth = river_mouth_transition(ocean_entry, &waterfalls);

        assert_eq!(
            mouth,
            RiverMouthTransition {
                waterfall_segment: Some(6),
                river_mesh_end: 7,
            }
        );

        {
            let adjacency = mesh.adjacency();
            let mut terrain = test_river_terrain(
                &mut mesh,
                &adjacency,
                &mut material,
                &bedrock_rates,
                &control_areas,
            );
            carve_submerged_river_mouth(
                &mut terrain,
                &mut nodes,
                &mut waterfalls,
                mouth,
                0.2,
                &[],
                &mut budget,
            );
        }

        assert_eq!(
            mesh.vertices[6].z.to_bits(),
            original_waterfall_lip.to_bits()
        );
        assert!(waterfalls[3]);
        assert!(waterfalls[6]);
        assert!(waterfalls[7..].iter().all(|&waterfall| !waterfall));
        assert!(nodes[7..].iter().all(|node| node.surface < 0.0));
        assert!(
            nodes[7..]
                .windows(2)
                .all(|pair| pair[0].surface + f32::EPSILON >= pair[1].surface)
        );
        let mouth_depth = 0.2 * 0.0025;
        assert!(
            mesh.vertices[7..]
                .iter()
                .zip(&nodes[7..])
                .all(|(vertex, node)| vertex.z <= node.surface - mouth_depth + f32::EPSILON)
        );
        assert!(budget.carried > 0.0);
    }

    #[test]
    fn river_without_a_waterfall_is_carved_entirely_below_the_sea_plane() {
        let mut mesh = Mesh {
            vertices: (0..6)
                .map(|index| Vec3::new(index as f32 * 0.01, 0.5, 0.06 - index as f32 * 0.008))
                .collect(),
            ..Mesh::default()
        };
        let mut nodes: Vec<RiverNode> = mesh
            .vertices
            .iter()
            .copied()
            .enumerate()
            .map(|(vertex, position)| RiverNode {
                vertex,
                flow: 10,
                surface: position.z,
                position,
            })
            .collect();
        let mut waterfalls = vec![false; nodes.len()];
        let mouth = river_mouth_transition(4, &waterfalls);
        assert_eq!(
            mouth,
            RiverMouthTransition {
                waterfall_segment: None,
                river_mesh_end: 0,
            }
        );

        let mut material = SurfaceMaterial::empty(mesh.vertices.len());
        let bedrock_rates = vec![1.0; mesh.vertices.len()];
        let control_areas = vec![1.0; mesh.vertices.len()];
        let mut budget = RiverSedimentBudget::default();
        {
            let adjacency = mesh.adjacency();
            let mut terrain = test_river_terrain(
                &mut mesh,
                &adjacency,
                &mut material,
                &bedrock_rates,
                &control_areas,
            );
            carve_submerged_river_mouth(
                &mut terrain,
                &mut nodes,
                &mut waterfalls,
                mouth,
                0.2,
                &[],
                &mut budget,
            );
        }

        assert!(waterfalls.iter().all(|&waterfall| !waterfall));
        assert!(nodes.iter().all(|node| node.surface < 0.0));
        assert!(mesh.vertices.iter().all(|vertex| vertex.z < 0.0));
        assert!(budget.carried > 0.0);
    }

    #[test]
    fn delta_builds_a_raised_valley_and_spreads_offshore() {
        let mut points = Vec::new();
        for y in 0..5 {
            for x in 0..5 {
                points.push(Vec2::new(x as f32 * 0.25, y as f32 * 0.25));
            }
        }
        let mut mesh = Mesh::delaunay(&points);
        for vertex in &mut mesh.vertices {
            vertex.z = if vertex.x < 0.25 {
                0.04
            } else if vertex.x < 0.5 {
                0.001
            } else {
                -0.02 - (vertex.y - 0.5).abs() * 0.004 - vertex.x * 0.002
            };
        }
        let previous = points
            .iter()
            .position(|point| *point == Vec2::new(0.25, 0.5))
            .unwrap();
        let outlet = points
            .iter()
            .position(|point| *point == Vec2::new(0.5, 0.5))
            .unwrap();
        let nodes = [
            RiverNode {
                vertex: previous,
                flow: 10,
                surface: 0.01,
                position: mesh.vertices[previous],
            },
            RiverNode {
                vertex: outlet,
                flow: 10,
                surface: 0.0,
                position: mesh.vertices[outlet],
            },
        ];
        let adjacency = mesh.adjacency();
        let edge_length = average_edge_length(&mesh, &adjacency);
        let before: Vec<f32> = mesh.vertices.iter().map(|vertex| vertex.z).collect();
        let channel_before = [before[previous], before[outlet]];
        let mut scratch = DeltaScratch::new(mesh.vertices.len());
        let mut material = SurfaceMaterial::empty(mesh.vertices.len());
        let control_areas = projected_vertex_control_areas(&mesh);
        let mut budget = RiverSedimentBudget {
            carried: 1.0,
            bedrock_eroded: 1.0,
            ..RiverSedimentBudget::default()
        };

        {
            let bedrock_rates = vec![1.0; mesh.vertices.len()];
            let mut terrain = test_river_terrain(
                &mut mesh,
                &adjacency,
                &mut material,
                &bedrock_rates,
                &control_areas,
            );
            create_delta(
                &mut terrain,
                &nodes,
                &mut budget,
                0.2,
                edge_length,
                &mut scratch,
            );
        }

        let changed: Vec<usize> = mesh
            .vertices
            .iter()
            .enumerate()
            .filter_map(|(index, vertex)| (vertex.z > before[index]).then_some(index))
            .collect();
        assert!(changed.len() > 3);
        assert!(changed.iter().any(|&index| {
            mesh.vertices[index].x > 0.5 && (mesh.vertices[index].y - 0.5).abs() > 0.1
        }));
        assert!(changed.iter().any(|&index| mesh.vertices[index].z > 0.0));
        assert!((mesh.vertices[previous].z - channel_before[0]).abs() < f32::EPSILON);
        assert!((mesh.vertices[outlet].z - channel_before[1]).abs() < f32::EPSILON);
    }

    #[test]
    fn outer_valley_hardness_preserves_resistant_banks_after_loose_cover() {
        let mut mesh = Mesh {
            vertices: vec![Vec3::new(0.0, 0.0, 0.1), Vec3::new(1.0, 0.0, 0.1)],
            ..Mesh::default()
        };
        let mut material = SurfaceMaterial::empty(mesh.vertices.len());
        material.depths_mut().fill(0.02);
        let bedrock_rates = [0.05, 1.0];
        let control_areas = [1.0, 1.0];
        let mut hard_budget = RiverSedimentBudget::default();
        let mut soft_budget = RiverSedimentBudget::default();
        let adjacency = mesh.adjacency();
        {
            let mut terrain = test_river_terrain(
                &mut mesh,
                &adjacency,
                &mut material,
                &bedrock_rates,
                &control_areas,
            );
            terrain.carve_vertex(0, 0.0, 0.0, &mut hard_budget);
            terrain.carve_vertex(1, 0.0, 0.0, &mut soft_budget);
        }

        assert_eq!(material.depths(), &[0.0, 0.0]);
        assert!(mesh.vertices[0].z > mesh.vertices[1].z + 0.07);
        assert!((hard_budget.loose_eroded - soft_budget.loose_eroded).abs() < 1.0e-7);
        assert!(soft_budget.bedrock_eroded > hard_budget.bedrock_eroded * 10.0);
    }

    #[test]
    fn tributary_budget_transfer_and_outlet_export_are_conservative() {
        let mut tributary = RiverSedimentBudget::default();
        tributary.record_erosion(0.2, 0.3, 2.0);
        let mut main_stem = RiverSedimentBudget::default();
        main_stem.record_erosion(0.1, 0.4, 1.0);

        main_stem.absorb(tributary);
        main_stem.export_remaining();

        assert!(main_stem.is_balanced());
        assert_eq!(main_stem.carried.to_bits(), 0.0_f64.to_bits());
        assert!((main_stem.exported - 1.5).abs() < 1.0e-6);
    }

    #[test]
    fn routed_rivers_have_monotonic_surfaces_and_valid_joins() {
        let points = [
            Vec2::new(0.0, 0.0),
            Vec2::new(1.0, 0.0),
            Vec2::new(1.0, 1.0),
            Vec2::new(0.0, 1.0),
            Vec2::new(0.5, 0.5),
            Vec2::new(0.25, 0.65),
            Vec2::new(0.75, 0.65),
        ];
        let mut mesh = Mesh::delaunay(&points);
        for vertex in &mut mesh.vertices {
            vertex.z = 0.2 - (vertex.y - 0.5).abs() * 0.3;
        }
        mesh.vertices[0].z = -0.02;
        mesh.vertices[1].z = -0.02;
        let adjacency = mesh.adjacency();
        let mut network = RiverNetwork::generate(
            &mut mesh,
            &adjacency,
            RiverSourceRule::new(0.0, 1.0, 0.0, 1.0),
        );
        let mut material = SurfaceMaterial::empty(mesh.vertices.len());
        network.shape(&mut mesh, &adjacency, &mut material, true, true);
        for (index, river) in network.rivers.iter().enumerate() {
            assert!(river.join.is_none_or(|join| join < index));
            let mut outlet = index;
            while let Some(join) = network.rivers[outlet].join {
                outlet = join;
            }
            assert!(
                network.rivers[outlet]
                    .nodes
                    .last()
                    .is_some_and(|node| network.ocean[node.vertex])
            );
            assert!(
                river
                    .nodes
                    .windows(2)
                    .all(|pair| pair[0].surface + 1.0e-6 >= pair[1].surface)
            );
        }
    }

    #[test]
    fn self_contact_index_ignores_the_local_tail_but_detects_a_return() {
        let mesh = Mesh {
            vertices: vec![Vec3::ZERO; 9],
            triangles: vec![0, 6, 7, 6, 7, 8],
            ..Mesh::default()
        };
        let adjacency = mesh.adjacency();
        let mut contact = RiverSelfContactIndex::new(mesh.vertices.len());
        contact.register(0, 0);
        contact.register(6, 6);

        assert!(contact.touches_earlier(&adjacency, 7, 1, 4));
        assert!(!contact.touches_earlier(&adjacency, 8, 1, 4));
    }

    #[test]
    fn rivers_join_when_their_mesh_rings_touch_before_their_centrelines() {
        let main = [0, 1, 2, 3, 4];
        let tributary = [5, 6, 7, 8, 9];
        let shared_bank = 18;
        let mut vertices = vec![Vec3::new(0.0, 0.0, 3.0); 21];
        for (step, &centre) in main.iter().enumerate() {
            vertices[centre] = Vec3::new(step as f32, 0.0, 0.9 - step as f32 * 0.1);
        }
        for (step, &centre) in tributary.iter().enumerate() {
            vertices[centre] = Vec3::new(step as f32, 2.0, 0.9 - step as f32 * 0.1);
        }
        vertices[shared_bank] = Vec3::new(4.0, 1.0, 3.0);
        let mut triangles = Vec::new();
        for (edge, pair) in main.windows(2).chain(tributary.windows(2)).enumerate() {
            triangles.extend([pair[0] as u32, pair[1] as u32, (10 + edge) as u32]);
        }
        triangles.extend([4, shared_bank as u32, 19, 9, shared_bank as u32, 20]);
        let mut mesh = Mesh {
            vertices,
            triangles,
            ..Mesh::default()
        };
        let adjacency = mesh.adjacency();
        let mut flow = vec![1; mesh.vertices.len()];
        for &centre in &tributary {
            flow[centre] = 2;
        }
        flow[10] = 100;
        let mut ocean = vec![false; mesh.vertices.len()];
        ocean[*main.last().unwrap()] = true;
        let (rivers, join_vertices) = trace_rivers(
            &mut mesh,
            &adjacency,
            &flow,
            &[main[0], tributary[0]],
            &ocean,
        );

        assert_eq!(rivers.len(), 2);
        assert_eq!(rivers[1].join, Some(0));
        assert_eq!(
            rivers[1].nodes.last().map(|node| node.vertex),
            Some(tributary[4])
        );
        assert!(!main.contains(&tributary[4]));
        assert_eq!(join_vertices[1], Some(main[4]));
        assert_eq!(rivers[0].nodes.last().map(|node| node.flow), Some(2));

        let mut main_footprint = RiverFootprintIndex::new(mesh.vertices.len());
        main_footprint.register_river(0, &rivers[0], &adjacency, 100);
        assert!(
            main_footprint
                .touching(&mesh, &adjacency, tributary[4], 1)
                .is_some()
        );
        assert!(adjacency[tributary[4]].contains(&shared_bank));
        assert!(adjacency[main[4]].contains(&shared_bank));
    }

    #[test]
    fn later_river_sources_are_rejected_near_an_accepted_river_path() {
        let separation = 20.0 / ISLAND_WORLD_METRES;
        let mut mesh = Mesh {
            vertices: vec![
                Vec3::new(0.0, 0.0, 0.2),
                Vec3::new(0.01, 0.0, 0.1),
                Vec3::new(0.02, 0.0, 0.0),
                Vec3::new(0.01, -0.01, 0.3),
                Vec3::new(0.0, separation, 0.2),
                Vec3::new(0.01, separation, 0.1),
                Vec3::new(0.02, separation, 0.0),
                Vec3::new(0.01, separation + 0.01, 0.3),
            ],
            triangles: vec![0, 1, 3, 1, 2, 3, 4, 5, 7, 5, 6, 7],
            ..Mesh::default()
        };
        let adjacency = mesh.adjacency();
        let flow = [3, 4, 5, 1, 3, 4, 5, 1];
        let mut ocean = [false; 8];
        ocean[2] = true;
        ocean[6] = true;

        let (rivers, join_vertices) = trace_rivers(&mut mesh, &adjacency, &flow, &[0, 4], &ocean);

        assert_eq!(rivers.len(), 1);
        assert_eq!(rivers[0].nodes.first().map(|node| node.vertex), Some(0));
        assert_eq!(join_vertices, [None]);
    }

    #[test]
    fn trace_discards_a_landlocked_path_when_no_ocean_route_exists() {
        let mut mesh = Mesh {
            vertices: vec![
                Vec3::new(0.0, 0.0, 0.3),
                Vec3::new(1.0, 0.0, 0.2),
                Vec3::new(0.5, 1.0, 0.1),
            ],
            triangles: vec![0, 1, 2],
            ..Mesh::default()
        };
        let adjacency = mesh.adjacency();
        let flow = [1, 2, 3];

        let (rivers, join_vertices) = trace_rivers(&mut mesh, &adjacency, &flow, &[0], &[false; 3]);

        assert!(rivers.is_empty());
        assert!(join_vertices.is_empty());
    }
}
