#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss
)]

use std::{
    cmp::Ordering,
    collections::{BinaryHeap, HashMap, HashSet},
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
const MAX_RIVER_RINGS: u8 = 3;
const RIVER_BOUNDARY: u8 = 1 << 7;
const WATERFALL_LIP_SMOOTHING: f32 = 0.5;
const RIVER_CHANNEL_DRAINAGE_FLOOR: f32 = 0.30;
const RIVER_BANK_DRAINAGE_FLOOR: f32 = 0.15;
// A strict one-ring extremum must stand this far from its neighbours relative
// to their mean horizontal spacing before the final repair touches it.
const SHARP_POINT_HEIGHT_RATIO: f32 = 0.35;
const SHARP_POINT_SMOOTHING: f32 = 0.6;
const SHARP_POINT_SMOOTHING_PASSES: usize = 2;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct RiverSourceRule {
    catchment_fraction: f32,
    steep_multiplier: f32,
    minimum_elevation: f32,
}

impl RiverSourceRule {
    pub(crate) const fn new(
        catchment_fraction: f32,
        steep_multiplier: f32,
        minimum_elevation_metres: f32,
    ) -> Self {
        Self {
            catchment_fraction,
            steep_multiplier,
            minimum_elevation: minimum_elevation_metres / ISLAND_WORLD_METRES,
        }
    }

    fn base_flow(self, land_vertex_count: usize) -> u32 {
        (land_vertex_count as f32 * self.catchment_fraction)
            .ceil()
            .max(1.0) as u32
    }

    fn required_flow(self, base_flow: u32, grade: f32) -> u32 {
        let slope_response = grade * grade;
        let multiplier = (self.steep_multiplier - 1.0).mul_add(slope_response, 1.0);
        (base_flow as f32 * multiplier).ceil() as u32
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
    max_flow: u32,
    max_height: f32,
    ocean: Vec<bool>,
    perimeter: Vec<bool>,
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
        let flow = calculate_flow(mesh, &downstream);
        let sources = find_sources(mesh, adjacency, &downstream, &flow, source_rule);
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
        Self {
            rivers,
            join_vertices,
            waterfalls,
            max_flow,
            max_height,
            ocean,
            perimeter,
        }
    }

    pub(crate) fn shape(
        &mut self,
        mesh: &mut Mesh,
        adjacency: &Adjacency,
        material: &mut SurfaceMaterial,
        smooth: bool,
        form_deltas: bool,
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
        self.carve(mesh, adjacency, material, &bedrock_rates, form_deltas);
        self.refresh(mesh);
    }

    pub(crate) fn into_parts(
        mut self,
        mesh: &mut Mesh,
        adjacency: &Adjacency,
        material: &mut SurfaceMaterial,
    ) -> (Vec<River>, Mesh, Vec<bool>, Vec<RiverMouth>) {
        let (river_mesh, river_bed) = self.build_mesh_with_mask(mesh, adjacency, material);
        mesh.calculate_normals();
        self.refresh(mesh);
        let mouths = self.river_mouths();
        (self.rivers, river_mesh, river_bed, mouths)
    }

    fn river_mouths(&self) -> Vec<RiverMouth> {
        let mut mouths = Vec::<RiverMouth>::new();
        for river in &self.rivers {
            if river.join.is_some() {
                continue;
            }
            let Some(terminal) = river
                .nodes
                .last()
                .filter(|node| self.ocean.get(node.vertex).copied().unwrap_or(false))
            else {
                continue;
            };
            let position = terminal.position.truncate();
            let Some(downstream) = river
                .nodes
                .iter()
                .rev()
                .skip(1)
                .map(|node| position - node.position.truncate())
                .find_map(Vec2::try_normalize)
            else {
                continue;
            };
            if !position.is_finite() || !downstream.is_finite() {
                continue;
            }

            if let Some(existing) = mouths
                .iter_mut()
                .find(|mouth| mouth.position.distance_squared(position) <= 1.0e-12)
            {
                if terminal.flow > existing.flow {
                    *existing = RiverMouth {
                        position,
                        downstream,
                        flow: terminal.flow,
                    };
                }
            } else {
                mouths.push(RiverMouth {
                    position,
                    downstream,
                    flow: terminal.flow,
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
        form_deltas: bool,
    ) {
        debug_assert_eq!(material.depths().len(), mesh.vertices.len());
        debug_assert_eq!(bedrock_rates.len(), mesh.vertices.len());
        let depth_multiplier = 1.0 / (self.max_flow as f32).sqrt().max(1.0);
        let base_width = average_edge_length(mesh, adjacency).max(0.000_25);
        let control_areas = projected_vertex_control_areas(mesh);
        let waterfall_clearance =
            WaterfallClearanceIndex::new(&self.rivers, mesh, self.max_flow, base_width);
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
            RiverChannelParameters {
                depth_multiplier,
                base_width,
                form_deltas,
            },
            &waterfall_clearance,
            &mut known_surfaces,
            &mut budgets,
        );
        transfer_tributary_budgets(&self.rivers, &mut budgets);
        if form_deltas {
            self.deposit_deltas(&mut terrain, &mut budgets);
        }
        finalize_river_budgets(&self.rivers, &mut budgets);
        apply_known_surfaces(&mut self.rivers, &known_surfaces);
    }

    fn carve_channels(
        &mut self,
        terrain: &mut RiverTerrain<'_>,
        channel_parameters: RiverChannelParameters,
        waterfall_clearance: &WaterfallClearanceIndex,
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
            let budget = shape_and_carve_river(
                terrain,
                nodes,
                &mut self.waterfalls[river_index],
                &mut scratch,
                &self.ocean,
                WaterfallRelocation {
                    clearance: waterfall_clearance,
                    river: river_index,
                },
                RiverCarveParameters {
                    downstream_surface,
                    terminal_ocean,
                    max_height: self.max_height,
                    max_flow: self.max_flow,
                    depth_multiplier: channel_parameters.depth_multiplier,
                    base_width: channel_parameters.base_width,
                    form_waterfall_shelves: !channel_parameters.form_deltas,
                },
            );
            for node in nodes {
                known_surfaces
                    .entry(node.vertex)
                    .and_modify(|value| *value = value.min(node.surface))
                    .or_insert(node.surface);
            }
            *budget_output = budget;
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
        adjacency: &Adjacency,
        material: &mut SurfaceMaterial,
    ) -> Mesh {
        self.build_mesh_with_mask(terrain, adjacency, material).0
    }

    fn build_mesh_with_mask(
        &self,
        terrain: &mut Mesh,
        adjacency: &Adjacency,
        material: &mut SurfaceMaterial,
    ) -> (Mesh, Vec<bool>) {
        let vertex_count = terrain.vertices.len();
        let perimeter = terrain.perimeter_mask();
        let mut coverage = vec![0_u8; vertex_count];
        let mut owner_distance = vec![u8::MAX; vertex_count];
        let mut surfaces = vec![0.0_f32; vertex_count];
        let mut river_uv = vec![Vec2::ZERO; vertex_count];
        let mut waterfall_lips = vec![false; vertex_count];
        let mut frontiers: [Vec<RiverMeshCandidate>; MAX_RIVER_RINGS as usize + 1] =
            std::array::from_fn(|_| Vec::new());

        for (river, waterfalls) in self.rivers.iter().zip(&self.waterfalls) {
            let mut distance_along = 0.0;
            let mut previous_position = None;
            for (index, node) in river.nodes.iter().enumerate() {
                let water_position = river_node_water_position(terrain, node);
                if let Some(previous) = previous_position {
                    distance_along += water_position.distance(previous);
                }
                previous_position = Some(water_position);
                let remaining = river_ring_count(node.flow, self.max_flow);
                frontiers[remaining as usize].push(RiverMeshCandidate {
                    remaining,
                    distance: 0,
                    surface: node.surface,
                    vertex: node.vertex,
                    flow_origin: water_position.truncate(),
                    flow_direction: river_node_flow_direction(terrain, &river.nodes, index),
                    distance_along,
                    waterfall_lip: waterfalls.get(index).copied().unwrap_or(false),
                });
            }
        }

        for remaining in (0..=MAX_RIVER_RINGS).rev() {
            while let Some(candidate) = frontiers[remaining as usize].pop() {
                let current_coverage = coverage[candidate.vertex];
                let candidate_coverage = candidate.remaining + 1;
                if current_coverage > candidate_coverage
                    || (current_coverage == candidate_coverage
                        && (owner_distance[candidate.vertex] < candidate.distance
                            || (owner_distance[candidate.vertex] == candidate.distance
                                && surfaces[candidate.vertex] <= candidate.surface)))
                {
                    continue;
                }
                coverage[candidate.vertex] = candidate_coverage;
                owner_distance[candidate.vertex] = candidate.distance;
                surfaces[candidate.vertex] = candidate.surface;
                river_uv[candidate.vertex] = candidate.uv_at(terrain.vertices[candidate.vertex]);
                waterfall_lips[candidate.vertex] = candidate.waterfall_lip;
                if candidate.remaining == 0 {
                    continue;
                }
                for &neighbour in &adjacency[candidate.vertex] {
                    frontiers[candidate.remaining as usize - 1].push(RiverMeshCandidate {
                        remaining: candidate.remaining - 1,
                        distance: candidate.distance + 1,
                        surface: candidate.surface,
                        vertex: neighbour,
                        flow_origin: candidate.flow_origin,
                        flow_direction: candidate.flow_direction,
                        distance_along: candidate.distance_along,
                        waterfall_lip: candidate.waterfall_lip,
                    });
                }
            }
        }

        mark_river_boundary(adjacency, &perimeter, &mut coverage);
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
            &mut waterfall_lips,
        );
        enforce_sea_plane_clearance(terrain, &self.ocean);
        self.keep_centrelines_below_water(terrain);
        let river_bed = river_topology_masks(terrain, &coverage).0;
        let river_mesh =
            duplicate_river_topology(terrain, &coverage, &surfaces, &river_uv, &waterfall_lips);
        (river_mesh, river_bed)
    }

    fn keep_centrelines_below_water(&self, terrain: &mut Mesh) {
        for node in self.rivers.iter().flat_map(|river| &river.nodes) {
            let bed_height = node.surface - RIVER_SURFACE_OFFSET;
            let bed_height =
                if bed_height > -SEA_PLANE_CLEARANCE && bed_height < SEA_PLANE_CLEARANCE {
                    -SEA_PLANE_CLEARANCE
                } else {
                    bed_height
                };
            terrain.vertices[node.vertex].z = terrain.vertices[node.vertex].z.min(bed_height);
        }
    }

    fn refresh(&mut self, mesh: &Mesh) {
        for river in &mut self.rivers {
            for node in &mut river.nodes {
                node.position = mesh.vertices[node.vertex];
            }
        }
    }
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

fn refine_river_terrain(
    terrain: &mut Mesh,
    material: &mut SurfaceMaterial,
    coverage: &mut Vec<u8>,
    surfaces: &mut Vec<f32>,
    river_uv: &mut Vec<Vec2>,
    waterfall_lips: &mut Vec<bool>,
) {
    let (under_river, _) = river_topology_masks(terrain, coverage);
    if !under_river.iter().any(|&is_river| is_river) {
        return;
    }

    let old_volume = material.volume(terrain);
    let stencils = terrain.tessellate_incident_to(&under_river);
    material.extend_after_tessellation(old_volume, terrain, &stencils);
    coverage.reserve(stencils.len());
    surfaces.reserve(stencils.len());
    river_uv.reserve(stencils.len());
    waterfall_lips.reserve(stencils.len());
    for stencil in stencils {
        let [a, b] = [stencil.surrounding[0], stencil.surrounding[1]];
        debug_assert_eq!(stencil.vertex as usize, coverage.len());
        let [a, b] = [a as usize, b as usize];
        let selected = coverage[a] != 0 && coverage[b] != 0;
        coverage.push(if selected {
            coverage[a].min(coverage[b])
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
        waterfall_lips.push(selected && waterfall_lips[a] && waterfall_lips[b]);
    }
    for value in coverage.iter_mut() {
        *value &= !RIVER_BOUNDARY;
    }
    let adjacency = terrain.adjacency();
    let perimeter = terrain.perimeter_mask();
    mark_river_boundary(&adjacency, &perimeter, coverage);
    let (under_river, bank) = river_topology_masks(terrain, coverage);
    let loose_volume = material.volume(terrain);
    smooth_river_terrain_vertices(terrain, &adjacency, &under_river, &bank, surfaces);
    material.rescale_to_volume(terrain, loose_volume);
}

fn repair_sharp_terrain_points(
    terrain: &mut Mesh,
    material: &mut SurfaceMaterial,
    coverage: &mut Vec<u8>,
    surfaces: &mut Vec<f32>,
    river_uv: &mut Vec<Vec2>,
    waterfall_lips: &mut Vec<bool>,
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
    waterfall_lips.reserve(stencils.len());
    for stencil in stencils {
        let [a, b] = [
            stencil.surrounding[0] as usize,
            stencil.surrounding[1] as usize,
        ];
        let count = usize::from(stencil.count);
        let selected = coverage[a] != 0 && coverage[b] != 0;
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
        waterfall_lips.push(selected && waterfall_lips[a] && waterfall_lips[b]);
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

#[derive(Clone, Copy, Debug)]
struct RiverMeshCandidate {
    remaining: u8,
    distance: u8,
    surface: f32,
    vertex: usize,
    flow_origin: Vec2,
    flow_direction: Vec2,
    distance_along: f32,
    waterfall_lip: bool,
}

impl RiverMeshCandidate {
    fn uv_at(self, position: Vec3) -> Vec2 {
        let offset = position.truncate() - self.flow_origin;
        let across = Vec2::new(-self.flow_direction.y, self.flow_direction.x);
        Vec2::new(
            offset.dot(across),
            self.distance_along + offset.dot(self.flow_direction),
        )
    }
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
        let selected = triangle
            .iter()
            .all(|&vertex| coverage[vertex as usize] != 0);
        let boundary_only = triangle
            .iter()
            .all(|&vertex| is_river_boundary(coverage[vertex as usize]));
        if !selected || boundary_only {
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

fn smooth_river_terrain_vertices(
    terrain: &mut Mesh,
    adjacency: &Adjacency,
    under_river: &[bool],
    bank: &[bool],
    surfaces: &[f32],
) {
    let mut adjusted = Vec::with_capacity(
        under_river
            .iter()
            .filter(|&&is_under_river| is_under_river)
            .count(),
    );
    for (vertex, &is_under_river) in under_river.iter().enumerate() {
        if !is_under_river {
            continue;
        }
        let current = terrain.vertices[vertex];
        let mut smoothed = if bank[vertex] {
            let mut along_bank = adjacency[vertex]
                .iter()
                .copied()
                .filter(|&neighbour| bank[neighbour]);
            match (along_bank.next(), along_bank.next(), along_bank.next()) {
                (Some(previous), Some(next), None) => {
                    let mut smoothed =
                        (current + terrain.vertices[previous] + terrain.vertices[next]) / 3.0;
                    smoothed.z = current.z;
                    smoothed
                }
                _ => current,
            }
        } else {
            let (total, count) = adjacency[vertex]
                .iter()
                .copied()
                .filter(|&neighbour| under_river[neighbour])
                .fold((current, 1_u32), |(total, count), neighbour| {
                    (total + terrain.vertices[neighbour], count + 1)
                });
            total / count as f32
        };
        if !bank[vertex] {
            smoothed.z = smoothed.z.min(surfaces[vertex] - RIVER_SURFACE_OFFSET);
        }
        adjusted.push((vertex, smoothed));
    }
    let vertex_faces = VertexFaceAdjacency::new(terrain);
    let mut projected_areas = ProjectedFaceAreas::new(terrain);
    for (vertex, position) in adjusted {
        let current = terrain.vertices[vertex];
        let horizontal_target = position.truncate().extend(current.z);
        let safe_fraction =
            projected_areas.safe_move_fraction(terrain, &vertex_faces, vertex, horizontal_target);
        terrain.vertices[vertex] = current
            .truncate()
            .lerp(position.truncate(), safe_fraction)
            .extend(position.z);
        projected_areas.update_incident(terrain, &vertex_faces, vertex);
        if !terrain.uv.is_empty() {
            terrain.uv[vertex] = terrain.vertices[vertex].truncate();
        }
    }
}

fn duplicate_river_topology(
    terrain: &Mesh,
    coverage: &[u8],
    surfaces: &[f32],
    river_uv: &[Vec2],
    waterfall_lips: &[bool],
) -> Mesh {
    let selected_count = coverage.iter().filter(|&&remaining| remaining > 0).count();
    let mut mapping = vec![u32::MAX; terrain.vertices.len()];
    let mut xy_mapping = HashMap::<(u32, u32), u32>::with_capacity(selected_count);
    let mut out_waterfall_lips = Vec::with_capacity(selected_count);
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
        let minimum_height = if boundary {
            f32::NEG_INFINITY
        } else {
            vertex.z + RIVER_SURFACE_OFFSET
        };
        vertex.z = if boundary {
            vertex.z.min(surfaces[index])
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
            out_waterfall_lips[mapped] |= waterfall_lips[index] && !boundary;
            continue;
        }
        let mapped = out.vertices.len() as u32;
        mapping[index] = mapped;
        xy_mapping.insert(key, mapped);
        out.vertices.push(vertex);
        minimum_heights.push(minimum_height);
        out_waterfall_lips.push(waterfall_lips[index] && !boundary);
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
    round_waterfall_lips(out, out_waterfall_lips, minimum_heights)
}

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

fn calculate_flow(mesh: &Mesh, downstream: &[usize]) -> Vec<u32> {
    let mut order: Vec<usize> = (0..mesh.vertices.len()).collect();
    order
        .sort_unstable_by(|&left, &right| mesh.vertices[right].z.total_cmp(&mesh.vertices[left].z));
    let mut flow = vec![1_u32; mesh.vertices.len()];
    for vertex in order {
        let next = downstream[vertex];
        if next != vertex {
            flow[next] = flow[next].saturating_add(flow[vertex]);
        }
    }
    flow
}

fn find_sources(
    mesh: &Mesh,
    adjacency: &Adjacency,
    downstream: &[usize],
    flow: &[u32],
    rule: RiverSourceRule,
) -> Vec<usize> {
    let land_vertex_count = mesh.vertices.iter().filter(|vertex| vertex.z > 0.0).count();
    let base_flow = rule.base_flow(land_vertex_count);
    let candidates: Vec<bool> = mesh
        .vertices
        .iter()
        .enumerate()
        .map(|(vertex, position)| {
            position.z >= rule.minimum_elevation
                && flow[vertex]
                    >= rule.required_flow(base_flow, source_grade(mesh, vertex, downstream[vertex]))
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
            .then_with(|| flow[right].cmp(&flow[left]))
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
struct RiverFootprintOwner {
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
    owners: Vec<Option<RiverFootprintOwner>>,
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
    ) -> Option<RiverFootprintOwner> {
        self.begin(centre);
        let centre_height = mesh.vertices[centre].z;
        let mut best = None::<(RiverFootprintOwner, u8, f32)>;
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
                RiverFootprintOwner {
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

    fn register_node(&mut self, owner: RiverFootprintOwner, adjacency: &Adjacency, rings: u8) {
        self.begin(owner.centre);
        while let Some((vertex, distance)) = self.frontier.pop() {
            if self.visited[vertex] == self.stamp {
                continue;
            }
            self.visited[vertex] = self.stamp;
            let candidate = RiverFootprintOwner { distance, ..owner };
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

impl RiverPathTracer<'_> {
    fn trace(&mut self, source: usize) -> TracedRiverPath {
        let mut path = vec![source];
        let mut seen = HashSet::from([source]);
        let mut join = None;
        let mut join_vertex = None;
        loop {
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
            let next = self.adjacency[current]
                .iter()
                .copied()
                .filter(|vertex| !seen.contains(vertex))
                .filter(|&vertex| self.mesh.vertices[vertex].z < self.mesh.vertices[current].z)
                .max_by(|&left, &right| {
                    downhill_slope(self.mesh, current, left)
                        .total_cmp(&downhill_slope(self.mesh, current, right))
                });
            if let Some(next) = next {
                path.push(next);
                seen.insert(next);
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
                self.mesh.vertices[vertex].z = self.mesh.vertices[vertex].z.min(sink_height);
                if seen.insert(vertex) {
                    path.push(vertex);
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
struct BankCandidate {
    target_height: f32,
    distance: f32,
    score: f32,
    shelf_radius: f32,
    radius: f32,
    vertex: usize,
}

impl PartialEq for BankCandidate {
    fn eq(&self, other: &Self) -> bool {
        self.vertex == other.vertex
            && self.target_height.to_bits() == other.target_height.to_bits()
            && self.distance.to_bits() == other.distance.to_bits()
            && self.score.to_bits() == other.score.to_bits()
            && self.shelf_radius.to_bits() == other.shelf_radius.to_bits()
            && self.radius.to_bits() == other.radius.to_bits()
    }
}

impl Eq for BankCandidate {}

impl PartialOrd for BankCandidate {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for BankCandidate {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .score
            .total_cmp(&self.score)
            .then_with(|| other.target_height.total_cmp(&self.target_height))
            .then_with(|| other.vertex.cmp(&self.vertex))
    }
}

#[derive(Debug)]
struct BankScratch {
    targets: Vec<f32>,
    scores: Vec<f32>,
    shelf: Vec<bool>,
    drainage_floor: Vec<f32>,
    touched: Vec<usize>,
    channel: Vec<u32>,
    stamp: u32,
    frontier: BinaryHeap<BankCandidate>,
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
}

impl BankScratch {
    fn new(vertex_count: usize) -> Self {
        Self {
            targets: vec![f32::INFINITY; vertex_count],
            scores: vec![f32::INFINITY; vertex_count],
            shelf: vec![false; vertex_count],
            drainage_floor: vec![0.0; vertex_count],
            touched: Vec::new(),
            channel: vec![0; vertex_count],
            stamp: 0,
            frontier: BinaryHeap::new(),
        }
    }

    fn begin(&mut self, nodes: &[RiverNode]) {
        self.frontier.clear();
        self.touched.clear();
        self.stamp = self.stamp.wrapping_add(1);
        if self.stamp == 0 {
            self.channel.fill(0);
            self.stamp = 1;
        }
        for node in nodes {
            self.channel[node.vertex] = self.stamp;
        }
    }

    fn set_target(&mut self, candidate: BankCandidate) {
        let vertex = candidate.vertex;
        let current_score = self.scores[vertex];
        let score_order = candidate.score.total_cmp(&current_score);
        if score_order.is_gt()
            || (score_order.is_eq() && candidate.target_height >= self.targets[vertex])
        {
            return;
        }
        if !current_score.is_finite() {
            self.touched.push(vertex);
        }
        self.targets[vertex] = candidate.target_height;
        self.scores[vertex] = candidate.score;
        self.shelf[vertex] = candidate.distance <= candidate.shelf_radius;
        let proximity =
            (1.0 - candidate.distance / candidate.radius.max(f32::EPSILON)).clamp(0.0, 1.0);
        let proximity = proximity * proximity * (3.0 - 2.0 * proximity);
        self.drainage_floor[vertex] = RIVER_BANK_DRAINAGE_FLOOR * proximity;
        self.frontier.push(candidate);
    }

    fn is_channel(&self, vertex: usize) -> bool {
        self.channel[vertex] == self.stamp
    }

    fn reset_targets(&mut self) {
        for &vertex in &self.touched {
            self.targets[vertex] = f32::INFINITY;
            self.scores[vertex] = f32::INFINITY;
            self.shelf[vertex] = false;
            self.drainage_floor[vertex] = 0.0;
        }
    }
}

struct RiverTerrain<'a> {
    mesh: &'a mut Mesh,
    adjacency: &'a Adjacency,
    material: &'a mut SurfaceMaterial,
    bedrock_rates: &'a [f32],
    control_areas: &'a [f32],
}

impl RiverTerrain<'_> {
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

fn carve_banks(
    terrain: &mut RiverTerrain<'_>,
    nodes: &[RiverNode],
    waterfalls: &[bool],
    parameters: RiverCarveParameters,
    budget: &mut RiverSedimentBudget,
    scratch: &mut BankScratch,
    ocean: &[bool],
) {
    seed_bank_targets(terrain, nodes, waterfalls, parameters, scratch);
    propagate_bank_targets(terrain, parameters, scratch, ocean);
    apply_bank_targets(terrain, budget, scratch);
    scratch.reset_targets();
}

fn seed_bank_targets(
    terrain: &RiverTerrain<'_>,
    nodes: &[RiverNode],
    waterfalls: &[bool],
    parameters: RiverCarveParameters,
    scratch: &mut BankScratch,
) {
    const SHELF_MARGIN: f32 = 1.35;
    scratch.begin(nodes);
    for (index, node) in nodes.iter().enumerate() {
        let normalized_flow = (node.flow as f32 / parameters.max_flow.max(1) as f32).sqrt();
        let edge_length = local_edge_length(terrain.mesh, terrain.adjacency, node.vertex);
        let supports_waterfall = waterfalls.get(index).copied().unwrap_or(false)
            || index
                .checked_sub(1)
                .and_then(|previous| waterfalls.get(previous))
                .copied()
                .unwrap_or(false);
        let shelf_radius = if parameters.form_waterfall_shelves && supports_waterfall {
            river_half_width(node.flow, parameters.max_flow, parameters.base_width) * SHELF_MARGIN
        } else {
            0.0
        };
        let radius = shelf_radius + edge_length * normalized_flow.mul_add(2.0, 3.0);
        let target_height = terrain.mesh.vertices[node.vertex].z;
        scratch.set_target(BankCandidate {
            target_height,
            distance: 0.0,
            score: if parameters.form_waterfall_shelves {
                0.0
            } else {
                target_height
            },
            shelf_radius,
            radius,
            vertex: node.vertex,
        });
    }
}

fn propagate_bank_targets(
    terrain: &RiverTerrain<'_>,
    parameters: RiverCarveParameters,
    scratch: &mut BankScratch,
    ocean: &[bool],
) {
    const MAXIMUM_BANK_SLOPE: f32 = 0.28;
    while let Some(candidate) = scratch.frontier.pop() {
        let score_order = candidate.score.total_cmp(&scratch.scores[candidate.vertex]);
        if score_order.is_gt()
            || (score_order.is_eq() && candidate.target_height > scratch.targets[candidate.vertex])
        {
            continue;
        }
        for &neighbour in &terrain.adjacency[candidate.vertex] {
            if scratch.is_channel(neighbour) || ocean[neighbour] {
                continue;
            }
            let step = (terrain.mesh.vertices[candidate.vertex].truncate()
                - terrain.mesh.vertices[neighbour].truncate())
            .length();
            let distance = candidate.distance + step;
            if distance > candidate.radius {
                continue;
            }
            let previous_bank_distance = (candidate.distance - candidate.shelf_radius).max(0.0);
            let bank_distance = (distance - candidate.shelf_radius).max(0.0);
            let target_height = candidate.target_height
                + (bank_distance - previous_bank_distance) * MAXIMUM_BANK_SLOPE;
            scratch.set_target(BankCandidate {
                target_height,
                distance,
                score: if parameters.form_waterfall_shelves {
                    distance
                } else {
                    target_height
                },
                shelf_radius: candidate.shelf_radius,
                radius: candidate.radius,
                vertex: neighbour,
            });
        }
    }
}

fn apply_bank_targets(
    terrain: &mut RiverTerrain<'_>,
    budget: &mut RiverSedimentBudget,
    scratch: &BankScratch,
) {
    for &vertex in &scratch.touched {
        if scratch.is_channel(vertex) {
            continue;
        }
        let target = scratch.targets[vertex];
        if terrain.mesh.vertices[vertex].z > target {
            terrain.carve_vertex(vertex, target, scratch.drainage_floor[vertex], budget);
        }
    }
    for &vertex in &scratch.touched {
        if scratch.is_channel(vertex) || !scratch.shelf[vertex] {
            continue;
        }
        let target = scratch.targets[vertex];
        if terrain.mesh.vertices[vertex].z < target {
            let requested = target - terrain.mesh.vertices[vertex].z;
            let deposited = terrain.deposit_vertex(vertex, requested, &mut budget.carried);
            budget.deposited += deposited;
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct RiverChannelParameters {
    depth_multiplier: f32,
    base_width: f32,
    form_deltas: bool,
}

#[derive(Debug)]
struct RiverCarveScratch {
    gradients: Vec<f32>,
    waterfall_drops: Vec<WaterfallDrop>,
    banks: BankScratch,
}

impl RiverCarveScratch {
    fn new(vertex_count: usize) -> Self {
        Self {
            gradients: Vec::new(),
            waterfall_drops: Vec::new(),
            banks: BankScratch::new(vertex_count),
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct WaterfallRelocation<'a> {
    clearance: &'a WaterfallClearanceIndex,
    river: usize,
}

#[derive(Clone, Copy, Debug)]
struct RiverCarveParameters {
    downstream_surface: f32,
    terminal_ocean: bool,
    max_height: f32,
    max_flow: u32,
    depth_multiplier: f32,
    base_width: f32,
    form_waterfall_shelves: bool,
}

fn shape_and_carve_river(
    terrain: &mut RiverTerrain<'_>,
    nodes: &mut [RiverNode],
    waterfalls: &mut [bool],
    scratch: &mut RiverCarveScratch,
    ocean: &[bool],
    waterfall_relocation: WaterfallRelocation<'_>,
    parameters: RiverCarveParameters,
) -> RiverSedimentBudget {
    let mut surface = parameters.downstream_surface;
    let mut water_surface = parameters.downstream_surface;
    for node in nodes.iter_mut().rev() {
        let vertex = terrain.mesh.vertices[node.vertex];
        surface = surface.max(vertex.z).max(0.0);
        let depth = river_depth(terrain.mesh, terrain.adjacency, *node, parameters);
        water_surface = water_surface.max(surface - depth * 0.35);
        node.surface = water_surface;
    }

    let mouth_start = parameters
        .terminal_ocean
        .then(|| river_mouth_grade_start(terrain.mesh, nodes))
        .flatten();
    let stepped_end = mouth_start.unwrap_or_else(|| nodes.len().saturating_sub(1));
    form_stepped_profile(
        nodes,
        waterfalls,
        stepped_end,
        parameters.max_height,
        &mut scratch.gradients,
    );
    relocate_conflicting_waterfalls(
        terrain.mesh,
        nodes,
        waterfalls,
        stepped_end,
        waterfall_relocation.river,
        waterfall_relocation.clearance,
        &mut scratch.waterfall_drops,
    );

    let mut budget = RiverSedimentBudget::default();
    if parameters.terminal_ocean {
        grade_river_mouth(
            terrain,
            nodes,
            waterfalls,
            parameters.max_height,
            &mut budget,
        );
    }
    let bed_end = mouth_start.map_or(stepped_end, |start| start.saturating_sub(1));
    if mouth_start != Some(0) {
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
    carve_banks(
        terrain,
        nodes,
        waterfalls,
        parameters,
        &mut budget,
        &mut scratch.banks,
        ocean,
    );
    budget
}

fn river_depth(
    mesh: &Mesh,
    adjacency: &Adjacency,
    node: RiverNode,
    parameters: RiverCarveParameters,
) -> f32 {
    let vertex = mesh.vertices[node.vertex];
    let altitude = ((parameters.max_height - vertex.z) / parameters.max_height.max(f32::EPSILON))
        .clamp(0.0, 1.0);
    let normalized_flow = (node.flow as f32 / parameters.max_flow.max(1) as f32).sqrt();
    let unconstrained = altitude
        * altitude
        * vertex.z.max(0.0)
        * 0.24
        * (node.flow as f32).sqrt()
        * parameters.depth_multiplier;
    let edge_limited =
        local_edge_length(mesh, adjacency, node.vertex) * normalized_flow.mul_add(0.18, 0.18);
    unconstrained.min(edge_limited)
}

fn carve_stepped_bed(
    terrain: &mut RiverTerrain<'_>,
    nodes: &[RiverNode],
    waterfalls: &[bool],
    end: usize,
    parameters: RiverCarveParameters,
    bed_targets: &mut Vec<f32>,
    budget: &mut RiverSedimentBudget,
) {
    let end = end.min(nodes.len().saturating_sub(1));
    bed_targets.clear();
    bed_targets.extend(nodes[..=end].iter().map(|&node| {
        node.surface - river_depth(terrain.mesh, terrain.adjacency, node, parameters)
    }));

    let mut reach_start = 0;
    for segment in 0..end {
        if waterfalls[segment] {
            carve_flat_bed_reach(
                terrain,
                &nodes[reach_start..=segment],
                &bed_targets[reach_start..=segment],
                budget,
            );
            reach_start = segment + 1;
        }
    }
    carve_flat_bed_reach(
        terrain,
        &nodes[reach_start..=end],
        &bed_targets[reach_start..=end],
        budget,
    );
}

fn carve_flat_bed_reach(
    terrain: &mut RiverTerrain<'_>,
    nodes: &[RiverNode],
    targets: &[f32],
    budget: &mut RiverSedimentBudget,
) {
    let floor = targets.iter().copied().fold(f32::INFINITY, f32::min);
    for node in nodes {
        terrain.carve_vertex(node.vertex, floor, RIVER_CHANNEL_DRAINAGE_FLOOR, budget);
    }
}

fn form_stepped_profile(
    nodes: &mut [RiverNode],
    waterfalls: &mut [bool],
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
    for index in (0..end).rev() {
        let segment_length =
            (nodes[index].position.truncate() - nodes[index + 1].position.truncate()).length();
        reach_length += segment_length;
        let natural_surface = nodes[index].surface.max(level);
        let available_rise = natural_surface - level;
        let gradient_response = (gradient_scratch[index] / steep_gradient)
            .clamp(0.0, 1.0)
            .sqrt();
        let target_fall = (maximum_fall - minimum_fall).mul_add(gradient_response, minimum_fall);
        let required_reach_length = gentle_reach_length * (1.0 - gradient_response * 0.9);
        if available_rise >= target_fall && reach_length >= required_reach_length {
            level += target_fall;
            waterfalls[index] = true;
            reach_length = 0.0;
        }
        nodes[index].surface = level.min(natural_surface);
    }
}

fn relocate_conflicting_waterfalls(
    mesh: &Mesh,
    nodes: &mut [RiverNode],
    waterfalls: &mut [bool],
    end: usize,
    river: usize,
    clearance: &WaterfallClearanceIndex,
    scratch: &mut Vec<WaterfallDrop>,
) {
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
            }),
    );
    if scratch.is_empty() {
        return;
    }

    waterfalls[..end].fill(false);
    let mut upstream_limit = end.saturating_sub(1);
    for drop in scratch.iter_mut().rev() {
        let original = drop.segment.min(upstream_limit);
        drop.segment = (0..=original)
            .rev()
            .find(|&segment| !clearance.conflicts(river, mesh, nodes, segment))
            .unwrap_or(original);
        waterfalls[drop.segment] = true;
        upstream_limit = drop.segment.saturating_sub(1);
    }

    let mut level = nodes[end].surface;
    let mut drop_index = scratch.len();
    for segment in (0..end).rev() {
        while drop_index > 0 && scratch[drop_index - 1].segment == segment {
            level += scratch[drop_index - 1].height;
            drop_index -= 1;
        }
        nodes[segment].surface = level.min(nodes[segment].surface);
    }
}

fn river_mouth_grade_start(mesh: &Mesh, nodes: &[RiverNode]) -> Option<usize> {
    const GRADE_SEGMENTS: usize = 10;

    let last_land = nodes
        .iter()
        .rposition(|node| mesh.vertices[node.vertex].z > 0.0)?;
    let end = nodes.len().saturating_sub(1);
    (last_land < end).then_some(last_land.saturating_sub(GRADE_SEGMENTS))
}

fn grade_river_mouth(
    terrain: &mut RiverTerrain<'_>,
    nodes: &mut [RiverNode],
    waterfalls: &mut [bool],
    max_height: f32,
    budget: &mut RiverSedimentBudget,
) {
    let Some(start) = river_mouth_grade_start(terrain.mesh, nodes) else {
        return;
    };
    let end = nodes.len().saturating_sub(1);
    let span = (end - start) as f32;
    let start_surface = nodes[start].surface.max(0.0);
    let mouth_depth = (max_height * 0.0025).max(0.000_02);
    waterfalls[start..end].fill(false);
    for (offset, node) in nodes[start..=end].iter_mut().enumerate() {
        let progress = offset as f32 / span;
        let surface = start_surface * (1.0 - progress);
        node.surface = surface;
        let target_bed = surface - mouth_depth * progress.mul_add(0.5, 0.5);
        terrain.carve_vertex(
            node.vertex,
            target_bed,
            RIVER_CHANNEL_DRAINAGE_FLOOR,
            budget,
        );
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
    fn source_cutoff_scales_with_land_vertex_density() {
        let rule = RiverSourceRule::new(0.005, 1.0, 5.0);

        assert_eq!(rule.base_flow(200), 1);
        assert_eq!(rule.base_flow(2_000), 10);
        assert_eq!(rule.base_flow(20_000), 100);
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
                    nodes: vec![node(0, 0.2, 10), node(1, 0.4, 20), node(2, 0.5, 30)],
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
            waterfalls: vec![vec![false; 3], vec![false; 2], vec![false; 2]],
            max_flow: 30,
            max_height: 0.2,
            ocean: vec![false, false, true, false, false, false],
            perimeter: vec![false; 6],
        };

        let mouths = network.river_mouths();
        assert_eq!(mouths.len(), 1);
        assert_eq!(mouths[0].position, Vec2::new(0.5, 0.5));
        assert_eq!(mouths[0].downstream, Vec2::X);
        assert_eq!(mouths[0].flow, 30);
    }

    #[test]
    fn source_cutoff_rises_smoothly_with_routing_grade() {
        let rule = RiverSourceRule::new(0.005, 4.0, 5.0);
        let base_flow = 100;

        assert_eq!(rule.required_flow(base_flow, 0.0), 100);
        assert_eq!(rule.required_flow(base_flow, 0.5), 175);
        assert_eq!(rule.required_flow(base_flow, 1.0), 400);
        assert_eq!(
            RiverSourceRule::new(0.005, 1.0, 5.0).required_flow(base_flow, 1.0),
            100
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
        let flow = [5, 20, 30, 40];

        let sources = find_sources(
            &mesh,
            &adjacency,
            &downstream,
            &flow,
            RiverSourceRule::new(1.0, 1.0, 0.0),
        );

        assert_eq!(sources, [1, 0]);
    }

    #[test]
    fn sources_below_the_minimum_world_elevation_are_excluded() {
        let mesh = Mesh {
            vertices: vec![
                Vec3::new(0.0, 0.0, 4.99 / ISLAND_WORLD_METRES),
                Vec3::new(1.0, 0.0, 5.0 / ISLAND_WORLD_METRES),
                Vec3::new(0.0, 1.0, 10.0 / ISLAND_WORLD_METRES),
            ],
            triangles: vec![0, 1, 2],
            ..Mesh::default()
        };
        let adjacency = mesh.adjacency();
        let downstream = [0, 1, 2];
        let flow = [10, 10, 10];

        let sources = find_sources(
            &mesh,
            &adjacency,
            &downstream,
            &flow,
            RiverSourceRule::new(1.0, 1.0, 5.0),
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
        let mut waterfall_lips = vec![false; original_vertex_count];

        repair_sharp_terrain_points(
            &mut terrain,
            &mut material,
            &mut coverage,
            &mut surfaces,
            &mut river_uv,
            &mut waterfall_lips,
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
        assert_eq!(waterfall_lips.len(), terrain.vertices.len());
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
        let mut waterfall_lips = vec![false; vertex_count];

        repair_sharp_terrain_points(
            &mut terrain,
            &mut material,
            &mut coverage,
            &mut surfaces,
            &mut river_uv,
            &mut waterfall_lips,
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

        form_stepped_profile(&mut nodes, &mut waterfalls, 20, 0.2, &mut scratch);

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
            0,
            &clearance,
            &mut scratch,
        );

        assert_eq!(waterfalls, [false, true, false, false]);
        assert!(!clearance.conflicts(0, &mesh, &nodes, 1));
        assert!((nodes[0].surface - nodes[3].surface - 0.05).abs() < 1.0e-6);
        assert!((nodes[2].surface - nodes[3].surface).abs() < 1.0e-6);
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
            base_width: average_edge_length(&mesh, &adjacency),
            form_waterfall_shelves: true,
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
    fn greater_flow_selects_more_terrain_rings() {
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
            max_flow: 100,
            max_height: 0.2,
            ocean: vec![false; terrain.vertices.len()],
            perimeter: vec![false; terrain.vertices.len()],
        };
        let adjacency = terrain.adjacency();
        let mut narrow_terrain = terrain.clone();
        let narrow = build_test_river_mesh(&network(1), &mut narrow_terrain, &adjacency);
        let mut broad_terrain = terrain.clone();
        let broad = build_test_river_mesh(&network(100), &mut broad_terrain, &adjacency);

        assert_eq!(river_ring_count(1, 100), 1);
        assert_eq!(river_ring_count(100, 100), 3);
        assert!(broad.vertices.len() > narrow.vertices.len() * 2);
        assert!(broad.triangles.len() > narrow.triangles.len() * 2);
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
            max_flow: 100,
            max_height: 0.2,
            ocean: vec![false; terrain.vertices.len()],
            perimeter: vec![false; terrain.vertices.len()],
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
            max_flow: 100,
            max_height: 0.2,
            ocean: vec![false; terrain.vertices.len()],
            perimeter: vec![false; terrain.vertices.len()],
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
    fn river_bank_refinement_updates_shared_terrain_topology() {
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
            max_flow: 100,
            max_height: 0.2,
            ocean: vec![false; terrain.vertices.len()],
            perimeter: vec![false; terrain.vertices.len()],
        };
        let original_vertices = terrain.vertices.clone();
        let adjacency = terrain.adjacency();

        let river_mesh = build_test_river_mesh(&network, &mut terrain, &adjacency);
        let terrain_xy: HashSet<(u32, u32)> = terrain
            .vertices
            .iter()
            .map(|vertex| (vertex.x.to_bits(), vertex.y.to_bits()))
            .collect();

        assert!(terrain.vertices.len() > original_vertices.len());
        assert!(
            terrain.vertices[..original_vertices.len()]
                .iter()
                .zip(&original_vertices)
                .any(|(refined, original)| refined.truncate() != original.truncate())
        );
        assert!(
            river_mesh
                .vertices
                .iter()
                .all(|vertex| terrain_xy.contains(&(vertex.x.to_bits(), vertex.y.to_bits())))
        );
    }

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
    fn river_footprint_refinement_smooths_the_shared_terrain_bed() {
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
            max_flow: 100,
            max_height: 0.2,
            ocean: vec![false; terrain.vertices.len()],
            perimeter: vec![false; terrain.vertices.len()],
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
    fn river_mouth_is_graded_across_multiple_segments() {
        let mut mesh = Mesh {
            vertices: (0..12)
                .map(|index| {
                    let height = if index == 11 {
                        -0.01
                    } else {
                        0.12 - index as f32 * 0.008
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
        let original_last_land = mesh.vertices[10].z;
        let mut material = SurfaceMaterial::empty(mesh.vertices.len());
        let bedrock_rates = vec![1.0; mesh.vertices.len()];
        let control_areas = vec![1.0; mesh.vertices.len()];
        let mut budget = RiverSedimentBudget::default();
        let mut waterfalls = vec![true; nodes.len()];

        {
            let adjacency = mesh.adjacency();
            let mut terrain = test_river_terrain(
                &mut mesh,
                &adjacency,
                &mut material,
                &bedrock_rates,
                &control_areas,
            );
            grade_river_mouth(&mut terrain, &mut nodes, &mut waterfalls, 0.2, &mut budget);
        }

        assert!(nodes.last().unwrap().surface.abs() < f32::EPSILON);
        assert!(
            nodes
                .windows(2)
                .all(|pair| pair[0].surface + f32::EPSILON >= pair[1].surface)
        );
        let drops: Vec<f32> = nodes
            .windows(2)
            .map(|pair| pair[0].surface - pair[1].surface)
            .collect();
        assert!(drops.iter().all(|drop| (*drop - drops[0]).abs() < 1.0e-6));
        assert!(mesh.vertices[10].z < original_last_land);
        assert!(budget.carried > 0.0);
        assert!(waterfalls[..11].iter().all(|waterfall| !waterfall));
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
        let mut network =
            RiverNetwork::generate(&mut mesh, &adjacency, RiverSourceRule::new(0.0, 1.0, 0.0));
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
