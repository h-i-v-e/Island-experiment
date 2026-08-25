use super::{
    Adjacency, GenerationScratch, HYDRAULIC_EDGE_SHIFT_LIMIT, HYDRAULIC_MIN_PROJECTED_AREA_RATIO,
    IndexedParallelIterator, IntoParallelIterator, IntoParallelRefMutIterator, IslandOptions,
    LOOSE_DEPTH_EPSILON, MINIMUM_BEDROCK_EROSION_RATE, Mesh, ParallelIterator, StageTimer,
    SurfaceMaterial, TriangleIndex, Vec2, Vec3, sample_mesh_triangle,
};
use std::{io, path::Path};

pub(super) fn hydraulic_erode_stage(
    mesh: &mut Mesh,
    adjacency: &Adjacency,
    material: &mut SurfaceMaterial,
    stage_strength: f32,
    options: IslandOptions,
    scratch: &mut GenerationScratch,
) {
    let _timer = StageTimer::new("hydraulic.stage");
    scratch.bedrock_rates.clear();
    scratch.bedrock_rates.extend(
        material
            .hardnesses()
            .iter()
            .map(|&hardness| bedrock_erosion_rate(hardness)),
    );
    #[cfg(feature = "gpu-erosion")]
    if std::env::var_os("MOTU_EXPERIMENTAL_GPU_PARTICLE_EROSION").is_some() {
        super::gpu_particle_erosion::erode_particle_batches_gpu(
            mesh,
            adjacency,
            material,
            &scratch.bedrock_rates,
            stage_strength,
            options,
            &mut scratch.gpu_particle_erosion,
        );
        return;
    }
    #[cfg(not(feature = "gpu-erosion"))]
    if std::env::var_os("MOTU_EXPERIMENTAL_GPU_PARTICLE_EROSION").is_some() {
        panic!("MOTU_EXPERIMENTAL_GPU_PARTICLE_EROSION requires --features gpu-erosion");
    }
    if std::env::var_os("MOTU_EXPERIMENTAL_PARTICLE_EROSION").is_some() {
        super::particle_erosion::erode_particle_batches(
            mesh,
            adjacency,
            material,
            &scratch.bedrock_rates,
            false,
            stage_strength,
            options,
            &mut scratch.particle_erosion,
        );
    } else if std::env::var_os("MOTU_EXPERIMENTAL_FLUX_EROSION").is_some() {
        super::flux_erosion::erode_flux_field(
            mesh,
            adjacency,
            material,
            &scratch.bedrock_rates,
            false,
            stage_strength,
            options,
            &mut scratch.flux_erosion,
        );
    } else if let Some(prototype) = CausalHydraulicSettings::from_environment() {
        let settings = HydraulicErosionSettings::new(stage_strength, options);
        if settings.erosion_strength != 0.0 {
            HydraulicEroder::new(
                mesh,
                adjacency,
                material,
                &scratch.bedrock_rates,
                false,
                settings,
            )
            .erode_causal_paths(&mut scratch.hydraulic, prototype);
        }
    } else if std::env::var_os("MOTU_EXPERIMENTAL_MESH_FLOW").is_some() {
        hydraulic_erode_with_scratch(
            mesh,
            adjacency,
            material,
            &scratch.bedrock_rates,
            false,
            HydraulicErosionSettings::new(stage_strength, options),
            &mut scratch.hydraulic,
        );
    } else {
        hydraulic_erode_reference(
            mesh,
            adjacency,
            material,
            &scratch.bedrock_rates,
            false,
            HydraulicErosionSettings::new(stage_strength, options),
        );
    }
}

/// Proven sequential hydraulic model. Each source path observes the terrain
/// mutations made by earlier paths, which is part of its ridge and drainage
/// formation rather than an implementation detail that can be reordered.
pub(super) fn hydraulic_erode_reference(
    mesh: &mut Mesh,
    adjacency: &Adjacency,
    material: &mut SurfaceMaterial,
    bedrock_rates: &[f32],
    include_sea: bool,
    settings: HydraulicErosionSettings,
) {
    if settings.erosion_strength == 0.0 {
        return;
    }
    HydraulicEroder::new(
        mesh,
        adjacency,
        material,
        bedrock_rates,
        include_sea,
        settings,
    )
    .erode_reference_paths();
}

pub(super) fn surface_normal_at(
    mesh: &Mesh,
    vertex_faces: &VertexFaceAdjacency,
    vertex: usize,
) -> Vec3 {
    vertex_faces
        .faces(vertex)
        .iter()
        .fold(Vec3::ZERO, |normal, &face| {
            let offset = face * 3;
            let a = mesh.vertices[mesh.triangles[offset] as usize];
            let b = mesh.vertices[mesh.triangles[offset + 1] as usize];
            let c = mesh.vertices[mesh.triangles[offset + 2] as usize];
            normal + (b - a).cross(c - a)
        })
        .try_normalize()
        .unwrap_or(Vec3::Z)
}

pub(crate) fn bedrock_erosion_rate(hardness: f32) -> f32 {
    let softness = 1.0 - hardness.clamp(0.0, 1.0);
    MINIMUM_BEDROCK_EROSION_RATE + (1.0 - MINIMUM_BEDROCK_EROSION_RATE) * softness * softness
}

#[derive(Clone, Copy, Debug)]
pub(super) struct HydraulicErosionSettings {
    pub(super) erosion_strength: f32,
    pub(super) deposition_strength: f32,
    pub(super) full_deposition_slope: f32,
    pub(super) maximum_deposition_slope: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct CausalHydraulicSettings {
    pub(super) epochs: usize,
    pub(super) core_source_threshold: u32,
    pub(super) cohort_sources: usize,
    pub(super) causal_frontiers: bool,
    pub(super) frozen_routing: bool,
    pub(super) reorder_components: bool,
}

impl CausalHydraulicSettings {
    const DEFAULT_EPOCHS: usize = 64;
    const DEFAULT_CORE_SOURCE_THRESHOLD: u32 = 64;

    fn from_environment() -> Option<Self> {
        std::env::var_os("MOTU_EXPERIMENTAL_CAUSAL_EROSION").map(|_| Self {
            epochs: positive_usize_environment("MOTU_CAUSAL_EROSION_EPOCHS", Self::DEFAULT_EPOCHS),
            core_source_threshold: positive_u32_environment(
                "MOTU_CAUSAL_EROSION_CORE_SOURCES",
                Self::DEFAULT_CORE_SOURCE_THRESHOLD,
            ),
            cohort_sources: positive_usize_environment(
                "MOTU_CAUSAL_EROSION_COHORT_SOURCES",
                usize::MAX,
            ),
            causal_frontiers: std::env::var_os("MOTU_CAUSAL_EROSION_FRONTIERS").is_some(),
            frozen_routing: std::env::var_os("MOTU_CAUSAL_EROSION_FROZEN_ROUTING").is_some(),
            reorder_components: std::env::var_os("MOTU_CAUSAL_EROSION_REORDER_COMPONENTS")
                .is_some(),
        })
    }
}

fn positive_usize_environment(name: &str, default: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse().ok())
        .filter(|&value| value > 0)
        .unwrap_or(default)
}

fn positive_u32_environment(name: &str, default: u32) -> u32 {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse().ok())
        .filter(|&value| value > 0)
        .unwrap_or(default)
}

#[derive(Clone, Copy, Debug)]
pub(super) struct HydraulicShiftLimits {
    pub(super) deposition: f32,
    pub(super) erosion: f32,
    pub(super) available_material: f32,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct HydraulicExchange {
    pub(super) capacity: f32,
    pub(super) deposition_weight: f32,
    pub(super) slope_erosion_weight: f32,
    pub(super) limits: HydraulicShiftLimits,
    pub(super) loose_available: f32,
    pub(super) bedrock_rate: f32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(super) struct HydraulicTransfer {
    pub(super) normal_retreat: f32,
    pub(super) vertical_deposit: f32,
    pub(super) loose_removed: f32,
    pub(super) bedrock_removed: f32,
}

#[derive(Clone, Copy)]
pub(super) struct ThermalTransfer {
    pub(super) target: usize,
    pub(super) height: f32,
    pub(super) loose: f32,
}

pub(crate) struct VertexFaceAdjacency {
    pub(super) offsets: Vec<usize>,
    pub(super) faces: Vec<usize>,
}

pub(crate) struct ProjectedFaceAreas {
    pub(super) reference: Vec<f32>,
    pub(super) current: Vec<f32>,
}

impl VertexFaceAdjacency {
    pub(crate) fn new(mesh: &Mesh) -> Self {
        let mut offsets = vec![0; mesh.vertices.len() + 1];
        for &vertex in &mesh.triangles {
            offsets[vertex as usize + 1] += 1;
        }
        for index in 1..offsets.len() {
            offsets[index] += offsets[index - 1];
        }

        let mut next = offsets[..mesh.vertices.len()].to_vec();
        let mut faces = vec![0; mesh.triangles.len()];
        for (face, triangle) in mesh.triangles.chunks_exact(3).enumerate() {
            for &vertex in triangle {
                let vertex = vertex as usize;
                faces[next[vertex]] = face;
                next[vertex] += 1;
            }
        }
        Self { offsets, faces }
    }

    pub(super) fn faces(&self, vertex: usize) -> &[usize] {
        &self.faces[self.offsets[vertex]..self.offsets[vertex + 1]]
    }
}

impl ProjectedFaceAreas {
    pub(crate) fn new(mesh: &Mesh) -> Self {
        let reference: Vec<f32> = (0..mesh.triangles.len() / 3)
            .map(|face| projected_face_area(mesh, face))
            .collect();
        let current = reference.clone();
        Self { reference, current }
    }

    pub(super) fn safe_erosion_cap(
        &self,
        mesh: &Mesh,
        vertex_faces: &VertexFaceAdjacency,
        vertex: usize,
        normal: Vec3,
        requested_cap: f32,
    ) -> f32 {
        let candidate = mesh.vertices[vertex] - normal * requested_cap;
        requested_cap * self.safe_move_fraction(mesh, vertex_faces, vertex, candidate)
    }

    pub(crate) fn safe_move_fraction(
        &self,
        mesh: &Mesh,
        vertex_faces: &VertexFaceAdjacency,
        vertex: usize,
        candidate: Vec3,
    ) -> f32 {
        vertex_faces
            .faces(vertex)
            .iter()
            .fold(1.0_f32, |safe_fraction, &face| {
                let reference = self.reference[face];
                if reference.abs() <= f32::EPSILON {
                    return 0.0;
                }
                let orientation = reference.signum();
                let current = self.current[face] * orientation;
                if current <= 0.0 {
                    return 0.0;
                }
                let minimum = (reference.abs() * HYDRAULIC_MIN_PROJECTED_AREA_RATIO).min(current);
                let candidate =
                    projected_face_area_with_vertex(mesh, face, vertex, candidate) * orientation;
                if candidate >= minimum {
                    safe_fraction
                } else {
                    let fraction = ((current - minimum) / (current - candidate)).clamp(0.0, 1.0);
                    safe_fraction.min(fraction)
                }
            })
    }

    pub(crate) fn update_incident(
        &mut self,
        mesh: &Mesh,
        vertex_faces: &VertexFaceAdjacency,
        vertex: usize,
    ) {
        for &face in vertex_faces.faces(vertex) {
            self.current[face] = projected_face_area(mesh, face);
        }
    }
}

struct HydraulicEroder<'a> {
    mesh: &'a mut Mesh,
    adjacency: &'a Adjacency,
    material: &'a mut SurfaceMaterial,
    bedrock_rates: &'a [f32],
    vertex_faces: VertexFaceAdjacency,
    projected_areas: ProjectedFaceAreas,
    include_sea: bool,
    settings: HydraulicErosionSettings,
    max_shift: f32,
}

#[derive(Clone, Copy)]
struct ErosionGeometry {
    direction: Vec3,
    slope_weight: f32,
    cap: f32,
    available_material: f32,
}

impl<'a> HydraulicEroder<'a> {
    fn new(
        mesh: &'a mut Mesh,
        adjacency: &'a Adjacency,
        material: &'a mut SurfaceMaterial,
        bedrock_rates: &'a [f32],
        include_sea: bool,
        settings: HydraulicErosionSettings,
    ) -> Self {
        debug_assert_eq!(material.depths().len(), mesh.vertices.len());
        debug_assert_eq!(bedrock_rates.len(), mesh.vertices.len());
        let vertex_faces = VertexFaceAdjacency::new(mesh);
        let projected_areas = ProjectedFaceAreas::new(mesh);
        let max_shift = mesh
            .vertices
            .iter()
            .map(|vertex| vertex.z)
            .fold(0.0_f32, f32::max)
            * 0.012;
        Self {
            mesh,
            adjacency,
            material,
            bedrock_rates,
            vertex_faces,
            projected_areas,
            include_sea,
            settings,
            max_shift,
        }
    }

    fn erode_reference_paths(&mut self) {
        let mut order = Vec::with_capacity(self.mesh.vertices.len());
        self.prepare_source_order(&mut order);
        for source in order {
            if self.mesh.vertices[source].z <= 0.0 {
                break;
            }
            self.erode_reference_path(source);
        }
    }

    fn prepare_source_order(&self, order: &mut Vec<usize>) {
        order.clear();
        order.extend(0..self.mesh.vertices.len());
        order.sort_unstable_by(|left, right| {
            self.mesh.vertices[*right]
                .z
                .total_cmp(&self.mesh.vertices[*left].z)
        });
    }

    fn erode_causal_paths(
        &mut self,
        scratch: &mut HydraulicScratch,
        prototype: CausalHydraulicSettings,
    ) {
        scratch.resize(self.mesh.vertices.len());
        self.prepare_source_order(&mut scratch.order);
        let land_source_count = scratch
            .order
            .partition_point(|&source| self.mesh.vertices[source].z > 0.0);
        if land_source_count == 0 {
            return;
        }

        let epoch_size = land_source_count.div_ceil(prototype.epochs);
        scratch.causal_stats = CausalHydraulicStats {
            epochs: land_source_count.div_ceil(epoch_size),
            ..CausalHydraulicStats::default()
        };
        for epoch_start in (0..land_source_count).step_by(epoch_size) {
            prepare_causal_routing(
                self.mesh,
                self.adjacency,
                self.include_sea,
                prototype.core_source_threshold,
                scratch,
            );
            record_causal_plan_stats(scratch);

            let epoch_end = (epoch_start + epoch_size).min(land_source_count);
            if prototype.reorder_components {
                if prototype.causal_frontiers {
                    self.erode_causal_epoch_by_frontier(
                        epoch_start,
                        epoch_end,
                        prototype.cohort_sources,
                        scratch,
                        prototype.frozen_routing,
                    );
                } else {
                    let cohort_size = prototype.cohort_sources.min(epoch_end - epoch_start);
                    for cohort_start in (epoch_start..epoch_end).step_by(cohort_size) {
                        let cohort_end = (cohort_start + cohort_size).min(epoch_end);
                        self.erode_causal_cohort_by_component(
                            cohort_start,
                            cohort_end,
                            scratch,
                            prototype.frozen_routing,
                        );
                        record_causal_cohort(scratch, cohort_end - cohort_start);
                    }
                }
            } else {
                for order_index in epoch_start..epoch_end {
                    let source = scratch.order[order_index];
                    let (steps, repairs) = self.erode_causal_path(
                        source,
                        &scratch.downstream,
                        prototype.frozen_routing,
                    );
                    scratch.causal_stats.paths += 1;
                    scratch.causal_stats.steps += steps;
                    scratch.causal_stats.routing_repairs += repairs;
                }
            }
        }

        report_causal_erosion(self.mesh, scratch);
    }

    fn erode_causal_epoch_by_frontier(
        &mut self,
        epoch_start: usize,
        epoch_end: usize,
        maximum_frontier_sources: usize,
        scratch: &mut HydraulicScratch,
        frozen_routing: bool,
    ) {
        let mut frontier_start = epoch_start;
        while frontier_start < epoch_end {
            let frontier_limit = frontier_start
                .saturating_add(maximum_frontier_sources)
                .min(epoch_end);
            let stamp = scratch.begin_component_frontier();
            let mut frontier_end = frontier_start;
            while frontier_end < frontier_limit {
                let source = scratch.order[frontier_end];
                let component = scratch.components[source];
                if component == NO_CAUSAL_COMPONENT
                    || component_conflicts_with_frontier(component, stamp, scratch)
                {
                    break;
                }
                scratch.active_component_stamps[component as usize] = stamp;
                frontier_end += 1;
            }

            if frontier_end == frontier_start {
                frontier_end += 1;
            }
            self.erode_causal_cohort_by_component(
                frontier_start,
                frontier_end,
                scratch,
                frozen_routing,
            );
            record_causal_cohort(scratch, frontier_end - frontier_start);
            frontier_start = frontier_end;
        }
    }

    fn erode_causal_cohort_by_component(
        &mut self,
        cohort_start: usize,
        cohort_end: usize,
        scratch: &mut HydraulicScratch,
        frozen_routing: bool,
    ) {
        scratch.scheduled_sources.clear();
        scratch.packets.clear();
        for rank in cohort_start..cohort_end {
            let source = scratch.order[rank];
            if scratch.components[source] == NO_CAUSAL_COMPONENT {
                scratch.packets.push(CausalPacket {
                    rank,
                    current: source,
                    speed: 0.0,
                    sediment: 0.0,
                });
            } else {
                scratch
                    .scheduled_sources
                    .push(RankedCausalSource { rank, source });
            }
        }
        let components = &scratch.components;
        let colours = &scratch.component_colours;
        scratch.scheduled_sources.sort_unstable_by(|left, right| {
            let left_component = components[left.source];
            let right_component = components[right.source];
            colours[left_component as usize]
                .cmp(&colours[right_component as usize])
                .then_with(|| left_component.cmp(&right_component))
                .then_with(|| left.rank.cmp(&right.rank))
        });

        for scheduled_index in 0..scratch.scheduled_sources.len() {
            let scheduled = scratch.scheduled_sources[scheduled_index];
            let component = scratch.components[scheduled.source];
            let (packet, steps, repairs) = self.erode_causal_component_path(
                scheduled,
                component,
                &scratch.components,
                &scratch.downstream,
                frozen_routing,
            );
            scratch.causal_stats.steps += steps;
            scratch.causal_stats.routing_repairs += repairs;
            if let Some(packet) = packet {
                scratch.packets.push(packet);
                scratch.causal_stats.boundary_packets += 1;
            }
        }

        scratch.packets.sort_unstable_by_key(|packet| packet.rank);
        scratch.causal_stats.core_packets += scratch.packets.len();
        for packet_index in 0..scratch.packets.len() {
            let packet = scratch.packets[packet_index];
            let (steps, repairs) =
                self.erode_causal_packet(packet, &scratch.downstream, frozen_routing);
            scratch.causal_stats.steps += steps;
            scratch.causal_stats.routing_repairs += repairs;
        }
        scratch.causal_stats.paths += cohort_end - cohort_start;
    }

    fn erode_causal_component_path(
        &mut self,
        scheduled: RankedCausalSource,
        component: u32,
        components: &[u32],
        downstream: &[usize],
        frozen_routing: bool,
    ) -> (Option<CausalPacket>, usize, usize) {
        let mut packet = CausalPacket {
            rank: scheduled.rank,
            current: scheduled.source,
            speed: 0.0,
            sediment: 0.0,
        };
        let mut steps = 0;
        let mut repairs = 0;
        for _ in 0..self.mesh.vertices.len() {
            if components[packet.current] != component {
                return (Some(packet), steps, repairs);
            }
            let Some(next) =
                self.causal_next(packet.current, downstream, frozen_routing, &mut repairs)
            else {
                self.deposit_remaining(packet.current, &mut packet.sediment);
                return (None, steps, repairs);
            };
            if self.mesh.vertices[next].z < 0.0 && !self.include_sea {
                self.deposit_remaining(packet.current, &mut packet.sediment);
                return (None, steps, repairs);
            }
            self.erode_reference_step(
                packet.current,
                next,
                &mut packet.speed,
                &mut packet.sediment,
            );
            packet.current = next;
            steps += 1;
        }
        (None, steps, repairs)
    }

    fn erode_causal_packet(
        &mut self,
        mut packet: CausalPacket,
        downstream: &[usize],
        frozen_routing: bool,
    ) -> (usize, usize) {
        let mut steps = 0;
        let mut repairs = 0;
        for _ in 0..self.mesh.vertices.len() {
            let Some(next) =
                self.causal_next(packet.current, downstream, frozen_routing, &mut repairs)
            else {
                self.deposit_remaining(packet.current, &mut packet.sediment);
                break;
            };
            if self.mesh.vertices[next].z < 0.0 && !self.include_sea {
                self.deposit_remaining(packet.current, &mut packet.sediment);
                break;
            }
            self.erode_reference_step(
                packet.current,
                next,
                &mut packet.speed,
                &mut packet.sediment,
            );
            packet.current = next;
            steps += 1;
        }
        (steps, repairs)
    }

    fn causal_next(
        &self,
        current: usize,
        downstream: &[usize],
        frozen_routing: bool,
        repairs: &mut usize,
    ) -> Option<usize> {
        let cached = downstream[current];
        if !frozen_routing {
            return self.reference_downstream(current);
        }
        if cached != NO_DOWNSTREAM && self.mesh.vertices[cached].z < self.mesh.vertices[current].z {
            Some(cached)
        } else {
            *repairs += usize::from(cached != NO_DOWNSTREAM);
            self.reference_downstream(current)
        }
    }

    fn erode_causal_path(
        &mut self,
        source: usize,
        downstream: &[usize],
        frozen_routing: bool,
    ) -> (usize, usize) {
        let mut current = source;
        let mut speed = 0.0_f32;
        let mut sediment = 0.0_f32;
        let mut steps = 0;
        let mut repairs = 0;
        for _ in 0..self.mesh.vertices.len() {
            let next = self.causal_next(current, downstream, frozen_routing, &mut repairs);
            let Some(next) = next else {
                self.deposit_remaining(current, &mut sediment);
                break;
            };
            if self.mesh.vertices[next].z < 0.0 && !self.include_sea {
                self.deposit_remaining(current, &mut sediment);
                break;
            }
            self.erode_reference_step(current, next, &mut speed, &mut sediment);
            current = next;
            steps += 1;
        }
        (steps, repairs)
    }

    fn erode_reference_path(&mut self, source: usize) {
        let mut current = source;
        let mut speed = 0.0_f32;
        let mut sediment = 0.0_f32;
        for _ in 0..self.mesh.vertices.len() {
            let Some(next) = self.reference_downstream(current) else {
                self.deposit_remaining(current, &mut sediment);
                break;
            };
            if self.mesh.vertices[next].z < 0.0 && !self.include_sea {
                self.deposit_remaining(current, &mut sediment);
                break;
            }
            self.erode_reference_step(current, next, &mut speed, &mut sediment);
            current = next;
        }
    }

    fn reference_downstream(&self, current: usize) -> Option<usize> {
        self.adjacency[current]
            .iter()
            .copied()
            .filter(|neighbour| self.mesh.vertices[*neighbour].z < self.mesh.vertices[current].z)
            .min_by(|left, right| {
                self.mesh.vertices[*left]
                    .z
                    .total_cmp(&self.mesh.vertices[*right].z)
            })
    }

    fn erode_reference_step(
        &mut self,
        current: usize,
        next: usize,
        speed: &mut f32,
        sediment: &mut f32,
    ) {
        let direction = self.mesh.vertices[current] - self.mesh.vertices[next];
        let distance = direction.length().max(f32::EPSILON);
        let horizontal_distance = direction.truncate().length().max(f32::EPSILON);
        let slope = direction.z / horizontal_distance;
        let sin_slope = direction.z / distance;
        let acceleration = sin_slope * sin_slope * sin_slope * distance;
        *speed = speed.mul_add(0.75, acceleration * 0.25);
        let deposition_weight = deposition_weight(slope, self.settings);
        let geometry = (*sediment <= *speed).then(|| {
            let normal = surface_normal_at(self.mesh, &self.vertex_faces, current);
            self.erosion_geometry(current, normal)
        });
        self.exchange_at(current, sediment, *speed, deposition_weight, geometry);
    }

    fn erosion_geometry(&self, current: usize, normal: Vec3) -> ErosionGeometry {
        let direction = hydraulic_erosion_direction(normal);
        let edge_cap =
            local_hydraulic_erosion_cap(self.mesh, self.adjacency, current, self.max_shift);
        let cap = self.projected_areas.safe_erosion_cap(
            self.mesh,
            &self.vertex_faces,
            current,
            direction,
            edge_cap,
        );
        let available_material = if self.include_sea {
            f32::INFINITY
        } else if direction.z > 0.0 {
            self.mesh.vertices[current].z.max(0.0) / direction.z
        } else {
            0.0
        };
        ErosionGeometry {
            direction,
            slope_weight: hydraulic_slope_erosion_weight(normal.z),
            cap,
            available_material,
        }
    }

    fn exchange_at(
        &mut self,
        current: usize,
        sediment: &mut f32,
        capacity: f32,
        deposition_weight: f32,
        geometry: Option<ErosionGeometry>,
    ) {
        let geometry = geometry.unwrap_or(ErosionGeometry {
            direction: Vec3::Z,
            slope_weight: 0.0,
            cap: 0.0,
            available_material: f32::INFINITY,
        });
        let transfer = exchange_sediment(
            sediment,
            self.settings,
            HydraulicExchange {
                capacity,
                deposition_weight,
                slope_erosion_weight: geometry.slope_weight,
                limits: HydraulicShiftLimits {
                    deposition: self.max_shift,
                    erosion: geometry.cap,
                    available_material: geometry.available_material,
                },
                loose_available: self.material.depths()[current],
                bedrock_rate: self.bedrock_rates[current],
            },
        );
        self.apply_transfer(current, geometry.direction, transfer);
    }

    fn apply_transfer(
        &mut self,
        current: usize,
        erosion_direction: Vec3,
        transfer: HydraulicTransfer,
    ) {
        apply_hydraulic_transfer(
            &mut self.mesh.vertices[current],
            erosion_direction,
            transfer,
        );
        let loose_depth = &mut self.material.depths_mut()[current];
        *loose_depth = (*loose_depth - transfer.loose_removed).max(0.0) + transfer.vertical_deposit;
        if *loose_depth < LOOSE_DEPTH_EPSILON {
            *loose_depth = 0.0;
        }
        if transfer.normal_retreat > 0.0 {
            self.projected_areas
                .update_incident(self.mesh, &self.vertex_faces, current);
        }
    }

    fn deposit_remaining(&mut self, current: usize, sediment: &mut f32) {
        deposit_sediment_fan(
            self.mesh,
            self.adjacency,
            self.material,
            current,
            sediment,
            self.max_shift,
            self.settings,
        );
    }

    fn erode_flow(&mut self, scratch: &mut HydraulicScratch) {
        scratch.resize(self.mesh.vertices.len());
        for _ in 0..HYDRAULIC_FLOW_ITERATIONS {
            calculate_normals_with_faces(self.mesh, &self.vertex_faces);
            prepare_hydraulic_flow(self.mesh, self.adjacency, self.include_sea, scratch);
            self.apply_flow(scratch);
        }
    }

    fn apply_flow(&mut self, scratch: &mut HydraulicScratch) {
        for order_index in 0..scratch.order.len() {
            let current = scratch.order[order_index];
            if self.mesh.vertices[current].z <= 0.0 {
                continue;
            }
            let next = scratch.downstream[current];
            let mut sediment = scratch.sediment[current];
            if next == NO_DOWNSTREAM {
                self.deposit_remaining(current, &mut sediment);
                continue;
            }

            let direction = self.mesh.vertices[current] - self.mesh.vertices[next];
            let distance = direction.length().max(f32::EPSILON);
            let horizontal_distance = direction.truncate().length().max(f32::EPSILON);
            let slope = direction.z / horizontal_distance;
            let sin_slope = direction.z / distance;
            let acceleration = sin_slope * sin_slope * sin_slope * distance;
            let capacity = acceleration * scratch.water[current].max(1.0);
            let deposition_weight = deposition_weight(slope, self.settings);
            let normal = self.mesh.normals[current];
            let geometry = self.erosion_geometry(current, normal);
            self.exchange_at(
                current,
                &mut sediment,
                capacity,
                deposition_weight,
                Some(geometry),
            );
            scratch.sediment[next] += sediment;
        }
    }
}

impl HydraulicErosionSettings {
    pub(super) fn new(stage_strength: f32, options: IslandOptions) -> Self {
        let maximum_deposition_slope = options
            .hydraulic_deposition_slope_degrees
            .to_radians()
            .tan();
        Self {
            erosion_strength: stage_strength * options.hydraulic_erosion_strength,
            deposition_strength: options.hydraulic_deposition_strength,
            full_deposition_slope: (options.hydraulic_deposition_slope_degrees / 3.0)
                .to_radians()
                .tan(),
            maximum_deposition_slope,
        }
    }
}

#[cfg(test)]
pub(super) fn hydraulic_erode(
    mesh: &mut Mesh,
    adjacency: &Adjacency,
    material: &mut SurfaceMaterial,
    bedrock_rates: &[f32],
    include_sea: bool,
    settings: HydraulicErosionSettings,
) {
    let mut scratch = HydraulicScratch::default();
    hydraulic_erode_with_scratch(
        mesh,
        adjacency,
        material,
        bedrock_rates,
        include_sea,
        settings,
        &mut scratch,
    );
}

pub(super) const HYDRAULIC_FLOW_ITERATIONS: usize = 16;
pub(super) const NO_DOWNSTREAM: usize = usize::MAX;
pub(super) const NO_CAUSAL_COMPONENT: u32 = u32::MAX;

#[derive(Default)]
pub(super) struct HydraulicScratch {
    pub(super) order: Vec<usize>,
    pub(super) routing_order: Vec<usize>,
    pub(super) downstream: Vec<usize>,
    pub(super) control_areas: Vec<f32>,
    pub(super) water: Vec<f32>,
    pub(super) sediment: Vec<f32>,
    pub(super) source_counts: Vec<u32>,
    pub(super) core_seed: Vec<bool>,
    pub(super) core: Vec<bool>,
    pub(super) components: Vec<u32>,
    pub(super) component_roots: Vec<usize>,
    pub(super) component_conflicts: Vec<u64>,
    pub(super) component_offsets: Vec<usize>,
    pub(super) component_neighbours: Vec<u32>,
    pub(super) component_cursor: Vec<usize>,
    pub(super) component_colours: Vec<u32>,
    pub(super) used_colour_stamps: Vec<u32>,
    pub(super) active_component_stamps: Vec<u32>,
    pub(super) active_component_stamp: u32,
    pub(super) component_colour_count: usize,
    pub(super) scheduled_sources: Vec<RankedCausalSource>,
    pub(super) packets: Vec<CausalPacket>,
    pub(super) causal_stats: CausalHydraulicStats,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct CausalHydraulicStats {
    pub(super) epochs: usize,
    pub(super) cohorts: usize,
    pub(super) maximum_cohort_sources: usize,
    pub(super) paths: usize,
    pub(super) steps: usize,
    pub(super) routing_repairs: usize,
    pub(super) maximum_core_vertices: usize,
    pub(super) maximum_upland_components: usize,
    pub(super) maximum_upland_component_sources: u32,
    pub(super) maximum_component_colours: usize,
    pub(super) maximum_component_conflicts: usize,
    pub(super) core_packets: usize,
    pub(super) boundary_packets: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct RankedCausalSource {
    pub(super) rank: usize,
    pub(super) source: usize,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct CausalPacket {
    pub(super) rank: usize,
    pub(super) current: usize,
    pub(super) speed: f32,
    pub(super) sediment: f32,
}

impl HydraulicScratch {
    pub(super) fn resize(&mut self, vertex_count: usize) {
        self.order.clear();
        self.order.extend(0..vertex_count);
        self.routing_order.clear();
        self.routing_order.extend(0..vertex_count);
        self.downstream.resize(vertex_count, NO_DOWNSTREAM);
        self.control_areas.resize(vertex_count, 0.0);
        self.water.resize(vertex_count, 0.0);
        self.sediment.resize(vertex_count, 0.0);
        self.source_counts.resize(vertex_count, 0);
        self.core_seed.resize(vertex_count, false);
        self.core.resize(vertex_count, false);
        self.components.resize(vertex_count, NO_CAUSAL_COMPONENT);
        self.component_roots.clear();
        self.component_conflicts.clear();
        self.component_offsets.clear();
        self.component_neighbours.clear();
        self.component_cursor.clear();
        self.component_colours.clear();
        self.used_colour_stamps.clear();
        self.active_component_stamps.resize(vertex_count, 0);
        self.active_component_stamps.fill(0);
        self.active_component_stamp = 0;
        self.component_colour_count = 0;
        self.scheduled_sources.clear();
        self.packets.clear();
    }

    fn begin_component_frontier(&mut self) -> u32 {
        self.active_component_stamp = self.active_component_stamp.wrapping_add(1);
        if self.active_component_stamp == 0 {
            self.active_component_stamps.fill(0);
            self.active_component_stamp = 1;
        }
        self.active_component_stamp
    }
}

fn record_causal_cohort(scratch: &mut HydraulicScratch, sources: usize) {
    scratch.causal_stats.cohorts += 1;
    scratch.causal_stats.maximum_cohort_sources =
        scratch.causal_stats.maximum_cohort_sources.max(sources);
}

fn record_causal_plan_stats(scratch: &mut HydraulicScratch) {
    let stats = &mut scratch.causal_stats;
    stats.maximum_core_vertices = stats
        .maximum_core_vertices
        .max(scratch.core.iter().filter(|&&is_core| is_core).count());
    stats.maximum_upland_components = stats
        .maximum_upland_components
        .max(scratch.component_roots.len());
    stats.maximum_upland_component_sources = stats.maximum_upland_component_sources.max(
        scratch
            .component_roots
            .iter()
            .map(|&root| scratch.source_counts[root])
            .max()
            .unwrap_or(0),
    );
    stats.maximum_component_colours = stats
        .maximum_component_colours
        .max(scratch.component_colour_count);
    stats.maximum_component_conflicts = stats
        .maximum_component_conflicts
        .max(scratch.component_conflicts.len());
}

fn report_causal_erosion(mesh: &Mesh, scratch: &HydraulicScratch) {
    if std::env::var_os("MOTU_CAUSAL_EROSION_STATS").is_some() {
        let stats = scratch.causal_stats;
        eprintln!(
            "causal_erosion,epochs={},cohorts={},maximum_cohort_sources={},paths={},steps={},routing_repairs={},maximum_core_vertices={},maximum_upland_components={},maximum_upland_component_sources={},maximum_component_colours={},maximum_component_conflicts={},core_packets={},boundary_packets={}",
            stats.epochs,
            stats.cohorts,
            stats.maximum_cohort_sources,
            stats.paths,
            stats.steps,
            stats.routing_repairs,
            stats.maximum_core_vertices,
            stats.maximum_upland_components,
            stats.maximum_upland_component_sources,
            stats.maximum_component_colours,
            stats.maximum_component_conflicts,
            stats.core_packets,
            stats.boundary_packets,
        );
    }
    if let Some(path) = std::env::var_os("MOTU_CAUSAL_EROSION_PLAN_PNG") {
        let resolution = positive_u32_environment("MOTU_CAUSAL_EROSION_PLAN_SIZE", 1024);
        let path = Path::new(&path);
        if let Err(error) = write_causal_partition_png(mesh, scratch, path, resolution) {
            eprintln!(
                "failed to write causal erosion partition {}: {error}",
                path.display()
            );
        }
    }
}

fn component_conflicts_with_frontier(
    component: u32,
    stamp: u32,
    scratch: &HydraulicScratch,
) -> bool {
    let component = component as usize;
    let neighbours = scratch.component_offsets[component]..scratch.component_offsets[component + 1];
    scratch.component_neighbours[neighbours]
        .iter()
        .any(|&neighbour| scratch.active_component_stamps[neighbour as usize] == stamp)
}

pub(super) fn prepare_causal_routing(
    mesh: &Mesh,
    adjacency: &Adjacency,
    include_sea: bool,
    core_source_threshold: u32,
    scratch: &mut HydraulicScratch,
) {
    scratch
        .downstream
        .par_iter_mut()
        .enumerate()
        .for_each(|(vertex, output)| {
            *output = adjacency[vertex]
                .iter()
                .copied()
                .filter(|&candidate| mesh.vertices[candidate].z < mesh.vertices[vertex].z)
                .min_by(|&left, &right| mesh.vertices[left].z.total_cmp(&mesh.vertices[right].z))
                .unwrap_or(NO_DOWNSTREAM);
        });

    scratch.routing_order.sort_unstable_by(|&left, &right| {
        mesh.vertices[right]
            .z
            .total_cmp(&mesh.vertices[left].z)
            .then_with(|| left.cmp(&right))
    });
    scratch
        .source_counts
        .iter_mut()
        .zip(&mesh.vertices)
        .for_each(|(count, vertex)| {
            *count = u32::from(include_sea || vertex.z > 0.0);
        });
    for &vertex in &scratch.routing_order {
        let downstream = scratch.downstream[vertex];
        if downstream != NO_DOWNSTREAM && (include_sea || mesh.vertices[downstream].z >= 0.0) {
            scratch.source_counts[downstream] =
                scratch.source_counts[downstream].saturating_add(scratch.source_counts[vertex]);
        }
    }

    scratch
        .core_seed
        .iter_mut()
        .zip(&scratch.source_counts)
        .zip(&mesh.vertices)
        .for_each(|((is_core, &source_count), vertex)| {
            *is_core = (include_sea || vertex.z >= 0.0) && source_count >= core_source_threshold;
        });
    scratch.core.copy_from_slice(&scratch.core_seed);
    scratch
        .core
        .iter_mut()
        .enumerate()
        .for_each(|(vertex, is_core)| {
            *is_core |= adjacency[vertex]
                .iter()
                .any(|&neighbour| scratch.core_seed[neighbour]);
        });
    assign_causal_components(mesh, include_sea, scratch);
    colour_causal_components(mesh, scratch);
}

fn assign_causal_components(mesh: &Mesh, include_sea: bool, scratch: &mut HydraulicScratch) {
    scratch.components.fill(NO_CAUSAL_COMPONENT);
    scratch.component_roots.clear();
    for &vertex in scratch.routing_order.iter().rev() {
        if scratch.source_counts[vertex] == 0 || scratch.core[vertex] {
            continue;
        }
        let downstream = scratch.downstream[vertex];
        let reaches_boundary = downstream == NO_DOWNSTREAM
            || (!include_sea && mesh.vertices[downstream].z < 0.0)
            || scratch.core[downstream];
        let component = if reaches_boundary {
            let component = u32::try_from(scratch.component_roots.len())
                .expect("terrain has more causal components than u32 can represent");
            scratch.component_roots.push(vertex);
            component
        } else {
            let component = scratch.components[downstream];
            debug_assert_ne!(component, NO_CAUSAL_COMPONENT);
            component
        };
        scratch.components[vertex] = component;
    }
}

fn colour_causal_components(mesh: &Mesh, scratch: &mut HydraulicScratch) {
    scratch.component_conflicts.clear();
    for triangle in mesh.triangles.as_chunks::<3>().0 {
        let components = [triangle[0], triangle[1], triangle[2]]
            .map(|vertex| scratch.components[vertex as usize]);
        for pair in [[0, 1], [1, 2], [2, 0]] {
            let [left, right] = pair.map(|index| components[index]);
            if left != NO_CAUSAL_COMPONENT && right != NO_CAUSAL_COMPONENT && left != right {
                let [lower, upper] = if left < right {
                    [left, right]
                } else {
                    [right, left]
                };
                scratch
                    .component_conflicts
                    .push((u64::from(lower) << u32::BITS) | u64::from(upper));
            }
        }
    }
    scratch.component_conflicts.sort_unstable();
    scratch.component_conflicts.dedup();

    let component_count = scratch.component_roots.len();
    scratch.component_offsets.clear();
    scratch.component_offsets.resize(component_count + 1, 0);
    for &edge in &scratch.component_conflicts {
        let [left, right] = [(edge >> u32::BITS) as usize, edge as u32 as usize];
        scratch.component_offsets[left + 1] += 1;
        scratch.component_offsets[right + 1] += 1;
    }
    for component in 1..scratch.component_offsets.len() {
        scratch.component_offsets[component] += scratch.component_offsets[component - 1];
    }
    scratch.component_cursor.clear();
    scratch
        .component_cursor
        .extend_from_slice(&scratch.component_offsets[..component_count]);
    scratch
        .component_neighbours
        .resize(scratch.component_conflicts.len() * 2, 0);
    for &edge in &scratch.component_conflicts {
        let [left, right] = [(edge >> u32::BITS) as usize, edge as u32 as usize];
        scratch.component_neighbours[scratch.component_cursor[left]] = right as u32;
        scratch.component_cursor[left] += 1;
        scratch.component_neighbours[scratch.component_cursor[right]] = left as u32;
        scratch.component_cursor[right] += 1;
    }

    scratch.component_colours.clear();
    scratch.component_colours.resize(component_count, u32::MAX);
    scratch.used_colour_stamps.clear();
    scratch.used_colour_stamps.resize(component_count.max(1), 0);
    scratch.component_colour_count = 0;
    for component in 0..component_count {
        let stamp = u32::try_from(component + 1).expect("component count exceeds u32");
        for &neighbour in &scratch.component_neighbours
            [scratch.component_offsets[component]..scratch.component_offsets[component + 1]]
        {
            let colour = scratch.component_colours[neighbour as usize];
            if colour != u32::MAX {
                scratch.used_colour_stamps[colour as usize] = stamp;
            }
        }
        let colour = scratch
            .used_colour_stamps
            .iter()
            .position(|&used_at| used_at != stamp)
            .expect("one colour per component is always sufficient");
        scratch.component_colours[component] = colour as u32;
        scratch.component_colour_count = scratch.component_colour_count.max(colour + 1);
    }
}

fn write_causal_partition_png(
    mesh: &Mesh,
    scratch: &HydraulicScratch,
    path: &Path,
    resolution: u32,
) -> io::Result<()> {
    let resolution = resolution.max(2);
    let index = TriangleIndex::new(mesh);
    let mut pixels = vec![0_u8; resolution as usize * resolution as usize * 3];
    let denominator = resolution.saturating_sub(1) as f32;
    for y in 0..resolution {
        for x in 0..resolution {
            let point = Vec2::new(x as f32 / denominator, y as f32 / denominator);
            let colour = sample_mesh_triangle(mesh, &index, point).map_or(
                [7, 17, 27],
                |(triangle, weights)| {
                    let elevation = weights[0].mul_add(
                        mesh.vertices[triangle[0]].z,
                        weights[1].mul_add(
                            mesh.vertices[triangle[1]].z,
                            weights[2] * mesh.vertices[triangle[2]].z,
                        ),
                    );
                    let dominant = weights
                        .iter()
                        .enumerate()
                        .max_by(|left, right| left.1.total_cmp(right.1))
                        .map_or(0, |(index, _)| index);
                    causal_partition_colour(elevation, triangle[dominant], scratch)
                },
            );
            let offset = (y as usize * resolution as usize + x as usize) * 3;
            pixels[offset..offset + 3].copy_from_slice(&colour);
        }
    }
    crate::write_png(path, resolution, resolution, &pixels)
}

fn causal_partition_colour(elevation: f32, vertex: usize, scratch: &HydraulicScratch) -> [u8; 3] {
    const PALETTE: [[u8; 3]; 12] = [
        [110, 190, 128],
        [199, 166, 90],
        [135, 152, 211],
        [191, 121, 171],
        [89, 177, 181],
        [202, 133, 101],
        [151, 190, 91],
        [122, 132, 183],
        [211, 156, 112],
        [116, 170, 148],
        [178, 137, 195],
        [178, 184, 104],
    ];

    if elevation < 0.0 {
        return [8, 29, 47];
    }
    if scratch.core_seed[vertex] {
        return [38, 214, 255];
    }
    if scratch.core[vertex] {
        return [42, 87, 133];
    }
    let component = scratch.components[vertex];
    if component == NO_CAUSAL_COMPONENT {
        return [28, 35, 39];
    }
    let colour = scratch.component_colours[component as usize];
    PALETTE[colour as usize % PALETTE.len()]
}

pub(super) fn hydraulic_erode_with_scratch(
    mesh: &mut Mesh,
    adjacency: &Adjacency,
    material: &mut SurfaceMaterial,
    bedrock_rates: &[f32],
    include_sea: bool,
    settings: HydraulicErosionSettings,
    scratch: &mut HydraulicScratch,
) {
    if settings.erosion_strength == 0.0 {
        return;
    }
    HydraulicEroder::new(
        mesh,
        adjacency,
        material,
        bedrock_rates,
        include_sea,
        settings,
    )
    .erode_flow(scratch);
}

pub(super) fn calculate_normals_with_faces(mesh: &mut Mesh, vertex_faces: &VertexFaceAdjacency) {
    let vertices = &mesh.vertices;
    let triangles = &mesh.triangles;
    mesh.normals = (0..vertices.len())
        .into_par_iter()
        .map(|vertex| {
            vertex_faces
                .faces(vertex)
                .iter()
                .fold(Vec3::ZERO, |normal, &face| {
                    let offset = face * 3;
                    let a = vertices[triangles[offset] as usize];
                    let b = vertices[triangles[offset + 1] as usize];
                    let c = vertices[triangles[offset + 2] as usize];
                    normal + (b - a).cross(c - a)
                })
                .try_normalize()
                .unwrap_or(Vec3::Z)
        })
        .collect();
}

pub(super) fn prepare_hydraulic_flow(
    mesh: &Mesh,
    adjacency: &Adjacency,
    include_sea: bool,
    scratch: &mut HydraulicScratch,
) {
    scratch
        .downstream
        .par_iter_mut()
        .enumerate()
        .for_each(|(vertex, output)| {
            let source = mesh.vertices[vertex];
            *output = adjacency[vertex]
                .iter()
                .copied()
                .filter(|&candidate| {
                    let height = mesh.vertices[candidate].z;
                    height < source.z && (include_sea || height >= 0.0)
                })
                .max_by(|&left, &right| {
                    downhill_gradient(source, mesh.vertices[left])
                        .total_cmp(&downhill_gradient(source, mesh.vertices[right]))
                        .then_with(|| right.cmp(&left))
                })
                .unwrap_or(NO_DOWNSTREAM);
        });

    scratch.control_areas.fill(0.0);
    for triangle in mesh.triangles.as_chunks::<3>().0 {
        let [a, b, c] = [
            mesh.vertices[triangle[0] as usize].truncate(),
            mesh.vertices[triangle[1] as usize].truncate(),
            mesh.vertices[triangle[2] as usize].truncate(),
        ];
        let share = (b - a).perp_dot(c - a).abs() / 6.0;
        for &vertex in triangle {
            scratch.control_areas[vertex as usize] += share;
        }
    }
    let (land_area, land_vertices) = mesh
        .vertices
        .iter()
        .zip(&scratch.control_areas)
        .filter(|(vertex, _)| include_sea || vertex.z > 0.0)
        .fold((0.0_f32, 0_usize), |(area, count), (_, &control_area)| {
            (area + control_area, count + 1)
        });
    let mean_area = land_area / land_vertices.max(1) as f32;
    scratch
        .water
        .par_iter_mut()
        .zip(&scratch.control_areas)
        .zip(&mesh.vertices)
        .for_each(|((water, &area), vertex)| {
            *water = if (include_sea || vertex.z > 0.0) && mean_area > f32::EPSILON {
                area / mean_area
            } else {
                0.0
            };
        });
    scratch.sediment.fill(0.0);
    scratch.order.sort_unstable_by(|&left, &right| {
        mesh.vertices[right]
            .z
            .total_cmp(&mesh.vertices[left].z)
            .then_with(|| left.cmp(&right))
    });
    for &vertex in &scratch.order {
        let downstream = scratch.downstream[vertex];
        if downstream != NO_DOWNSTREAM {
            scratch.water[downstream] += scratch.water[vertex];
        }
    }
}

pub(super) fn downhill_gradient(source: Vec3, target: Vec3) -> f32 {
    (source.z - target.z)
        / source
            .truncate()
            .distance(target.truncate())
            .max(f32::EPSILON)
}

pub(super) fn hydraulic_slope_erosion_weight(normal_z: f32) -> f32 {
    let vertical_alignment = normal_z.clamp(0.0, 1.0);
    let horizontal_alignment = (1.0 - vertical_alignment * vertical_alignment).sqrt();
    2.0 * vertical_alignment * horizontal_alignment
}

pub(super) fn hydraulic_erosion_direction(normal: Vec3) -> Vec3 {
    let vertical_alignment = normal.z.clamp(0.0, 1.0);
    let beyond_forty_five_degrees =
        (1.0 - 2.0 * vertical_alignment * vertical_alignment).clamp(0.0, 1.0);
    let vertical_blend = smooth_unit_interval(beyond_forty_five_degrees);
    normal.lerp(Vec3::Z, vertical_blend).normalize_or_zero()
}

pub(super) fn smooth_unit_interval(value: f32) -> f32 {
    value * value * (3.0 - 2.0 * value)
}

pub(super) fn local_hydraulic_erosion_cap(
    mesh: &Mesh,
    adjacency: &Adjacency,
    vertex: usize,
    global_cap: f32,
) -> f32 {
    let minimum_edge = adjacency[vertex]
        .iter()
        .map(|&neighbour| mesh.vertices[vertex].distance(mesh.vertices[neighbour]))
        .fold(f32::INFINITY, f32::min);
    global_cap.min(minimum_edge * HYDRAULIC_EDGE_SHIFT_LIMIT)
}

pub(super) fn projected_face_area(mesh: &Mesh, face: usize) -> f32 {
    let offset = face * 3;
    let [a, b, c] = [
        mesh.vertices[mesh.triangles[offset] as usize].truncate(),
        mesh.vertices[mesh.triangles[offset + 1] as usize].truncate(),
        mesh.vertices[mesh.triangles[offset + 2] as usize].truncate(),
    ];
    (b - a).perp_dot(c - a)
}

pub(super) fn projected_face_area_with_vertex(
    mesh: &Mesh,
    face: usize,
    moved_vertex: usize,
    candidate: Vec3,
) -> f32 {
    let offset = face * 3;
    let indices = [
        mesh.triangles[offset] as usize,
        mesh.triangles[offset + 1] as usize,
        mesh.triangles[offset + 2] as usize,
    ];
    let points = indices.map(|index| {
        if index == moved_vertex {
            candidate.truncate()
        } else {
            mesh.vertices[index].truncate()
        }
    });
    (points[1] - points[0]).perp_dot(points[2] - points[0])
}

pub(super) fn apply_hydraulic_transfer(
    vertex: &mut Vec3,
    normal: Vec3,
    transfer: HydraulicTransfer,
) {
    *vertex -= normal * transfer.normal_retreat;
    vertex.z += transfer.vertical_deposit;
}

pub(super) fn deposition_weight(slope: f32, settings: HydraulicErosionSettings) -> f32 {
    let width =
        (settings.maximum_deposition_slope - settings.full_deposition_slope).max(f32::EPSILON);
    let normalized = ((slope - settings.full_deposition_slope) / width).clamp(0.0, 1.0);
    1.0 - smooth_unit_interval(normalized)
}

pub(super) fn exchange_sediment(
    sediment: &mut f32,
    settings: HydraulicErosionSettings,
    exchange: HydraulicExchange,
) -> HydraulicTransfer {
    let difference = *sediment - exchange.capacity;
    if difference > 0.0 {
        let rate = (settings.deposition_strength * 0.35 * exchange.deposition_weight).min(1.0);
        let deposited = (difference * rate)
            .min(exchange.limits.deposition)
            .min(*sediment);
        *sediment -= deposited;
        HydraulicTransfer {
            vertical_deposit: deposited,
            ..HydraulicTransfer::default()
        }
    } else {
        let erosion_weight = 1.0 - exchange.deposition_weight;
        let requested = (-difference
            * settings.erosion_strength
            * erosion_weight
            * exchange.slope_erosion_weight)
            .min(exchange.limits.erosion)
            .min(exchange.limits.available_material);
        let loose_removed = requested.min(exchange.loose_available.max(0.0));
        let bedrock_removed =
            (requested - loose_removed).max(0.0) * exchange.bedrock_rate.clamp(0.0, 1.0);
        let normal_retreat = loose_removed + bedrock_removed;
        *sediment += normal_retreat;
        HydraulicTransfer {
            normal_retreat,
            loose_removed,
            bedrock_removed,
            ..HydraulicTransfer::default()
        }
    }
}

pub(super) fn deposit_sediment_fan(
    mesh: &mut Mesh,
    adjacency: &Adjacency,
    material: &mut SurfaceMaterial,
    center: usize,
    sediment: &mut f32,
    max_shift: f32,
    settings: HydraulicErosionSettings,
) {
    let rate = (settings.deposition_strength * 0.35).min(1.0);
    if *sediment <= 0.0 || rate == 0.0 {
        return;
    }

    let center_position = mesh.vertices[center];
    let mut total_weight = 1.0_f32;
    for &neighbour in &adjacency[center] {
        let candidate = mesh.vertices[neighbour];
        if candidate.z < 0.0 {
            continue;
        }
        let horizontal_distance = (candidate - center_position)
            .truncate()
            .length()
            .max(f32::EPSILON);
        let slope = ((candidate.z - center_position.z) / horizontal_distance).abs();
        total_weight += deposition_weight(slope, settings);
    }

    let total_deposit = (*sediment * rate).min(max_shift * total_weight);
    let deposit_per_weight = total_deposit / total_weight;
    mesh.vertices[center].z += deposit_per_weight;
    material.depths_mut()[center] += deposit_per_weight;
    for &neighbour in &adjacency[center] {
        let candidate = mesh.vertices[neighbour];
        if candidate.z < 0.0 {
            continue;
        }
        let horizontal_distance = (candidate - center_position)
            .truncate()
            .length()
            .max(f32::EPSILON);
        let slope = ((candidate.z - center_position.z) / horizontal_distance).abs();
        let deposit = deposit_per_weight * deposition_weight(slope, settings);
        mesh.vertices[neighbour].z += deposit;
        material.depths_mut()[neighbour] += deposit;
    }
    *sediment -= total_deposit;
}

pub(super) fn erode_mesh(
    mesh: &mut Mesh,
    adjacency: &Adjacency,
    material: &mut SurfaceMaterial,
    options: IslandOptions,
    passes: usize,
) {
    let _timer = StageTimer::new("thermal.stage");
    let mut delta = vec![0.0; mesh.vertices.len()];
    let mut loose_delta = vec![0.0; mesh.vertices.len()];
    let talus = options.max_height * 0.006 / options.slope_multiplier.max(0.1);
    for _ in 0..passes {
        delta.fill(0.0);
        loose_delta.fill(0.0);
        let transfers: Vec<Option<ThermalTransfer>> = (0..adjacency.len())
            .into_par_iter()
            .map(|index| {
                let neighbours = &adjacency[index];
                let height = mesh.vertices[index].z;
                if height <= 0.0 {
                    return None;
                }
                let lowest = neighbours
                    .iter()
                    .copied()
                    .filter(|neighbour| mesh.vertices[*neighbour].z > 0.0)
                    .min_by(|left, right| {
                        mesh.vertices[*left].z.total_cmp(&mesh.vertices[*right].z)
                    })?;
                let difference = height - mesh.vertices[lowest].z;
                (difference > talus).then(|| {
                    let transfer = (difference - talus) * 0.18;
                    ThermalTransfer {
                        target: lowest,
                        height: transfer,
                        loose: material.depths()[index].min(transfer),
                    }
                })
            })
            .collect();
        for (index, transfer) in transfers.into_iter().enumerate() {
            let Some(transfer) = transfer else {
                continue;
            };
            delta[index] -= transfer.height;
            delta[transfer.target] += transfer.height;
            loose_delta[index] -= transfer.loose;
            loose_delta[transfer.target] += transfer.height;
        }
        mesh.vertices
            .iter_mut()
            .zip(&delta)
            .for_each(|(vertex, change)| vertex.z += change);
        material
            .depths_mut()
            .iter_mut()
            .zip(&loose_delta)
            .for_each(|(depth, change)| *depth = (*depth + change).max(0.0));
    }
}

pub(super) fn barycentric(point: Vec2, a: Vec2, b: Vec2, c: Vec2) -> Option<[f32; 3]> {
    let denominator = (b.y - c.y).mul_add(a.x - c.x, (c.x - b.x) * (a.y - c.y));
    if denominator.abs() <= f32::EPSILON {
        return None;
    }
    let first = (b.y - c.y).mul_add(point.x - c.x, (c.x - b.x) * (point.y - c.y)) / denominator;
    let second = (c.y - a.y).mul_add(point.x - c.x, (a.x - c.x) * (point.y - c.y)) / denominator;
    let third = 1.0 - first - second;
    (first >= -1.0e-5 && second >= -1.0e-5 && third >= -1.0e-5).then_some([first, second, third])
}

pub(super) fn bin_coordinate(value: f32, dimension: usize) -> usize {
    ((value.clamp(0.0, 1.0) * dimension as f32).floor() as usize).min(dimension - 1)
}

pub(super) fn triangle_bin_bounds(
    mesh: &Mesh,
    triangle: &[u32],
    dimension: usize,
) -> (usize, usize, usize, usize) {
    let points = [
        mesh.vertices[triangle[0] as usize],
        mesh.vertices[triangle[1] as usize],
        mesh.vertices[triangle[2] as usize],
    ];
    let min_x = points.iter().map(|point| point.x).fold(f32::MAX, f32::min);
    let max_x = points.iter().map(|point| point.x).fold(f32::MIN, f32::max);
    let min_y = points.iter().map(|point| point.y).fold(f32::MAX, f32::min);
    let max_y = points.iter().map(|point| point.y).fold(f32::MIN, f32::max);
    (
        bin_coordinate(min_x, dimension),
        bin_coordinate(max_x, dimension),
        bin_coordinate(min_y, dimension),
        bin_coordinate(max_y, dimension),
    )
}

#[cfg(test)]
mod hydraulic_tests {
    use super::super::{
        Terrain, TerrainMaterialField, TriangleIndex, correct_lods, sample_mesh_surface,
        sharp_rock_mask,
    };
    use super::*;

    pub(super) fn settings() -> HydraulicErosionSettings {
        HydraulicErosionSettings {
            erosion_strength: 1.0,
            deposition_strength: 2.0,
            full_deposition_slope: 4.0_f32.to_radians().tan(),
            maximum_deposition_slope: 12.0_f32.to_radians().tan(),
        }
    }

    pub(super) fn shift_limits(maximum: f32, available_material: f32) -> HydraulicShiftLimits {
        HydraulicShiftLimits {
            deposition: maximum,
            erosion: maximum,
            available_material,
        }
    }

    pub(super) fn exchange(
        capacity: f32,
        deposition_weight: f32,
        slope_erosion_weight: f32,
        limits: HydraulicShiftLimits,
        loose_available: f32,
        bedrock_rate: f32,
    ) -> HydraulicExchange {
        HydraulicExchange {
            capacity,
            deposition_weight,
            slope_erosion_weight,
            limits,
            loose_available,
            bedrock_rate,
        }
    }

    fn causal_test_mesh(dimension: usize) -> Mesh {
        let points: Vec<Vec2> = (0..dimension)
            .flat_map(|y| {
                (0..dimension).map(move |x| {
                    Vec2::new(
                        x as f32 / (dimension - 1) as f32,
                        y as f32 / (dimension - 1) as f32,
                    )
                })
            })
            .collect();
        let mut mesh = Mesh::delaunay(&points);
        mesh.vertices.iter_mut().for_each(|vertex| {
            let ridge = (vertex.x * std::f32::consts::PI).sin() * 0.08;
            vertex.z = 1.0 - vertex.x * 0.35 - vertex.y * 0.45 + ridge;
        });
        mesh.calculate_normals();
        mesh
    }

    fn causal_two_basin_mesh(dimension: usize) -> Mesh {
        let mut mesh = causal_test_mesh(dimension);
        mesh.vertices.iter_mut().for_each(|vertex| {
            let ridge = 0.5 - (vertex.x - 0.5).abs();
            vertex.z = 0.3 + ridge * 0.7 + vertex.y * 0.1;
        });
        mesh.calculate_normals();
        mesh
    }

    #[test]
    fn causal_routing_is_downhill_and_marks_a_one_ring_core() {
        let mesh = causal_test_mesh(6);
        let adjacency = mesh.adjacency();
        let mut scratch = HydraulicScratch::default();
        scratch.resize(mesh.vertices.len());

        prepare_causal_routing(&mesh, &adjacency, false, 4, &mut scratch);

        assert!(
            scratch
                .downstream
                .iter()
                .enumerate()
                .all(|(vertex, &downstream)| downstream == NO_DOWNSTREAM
                    || mesh.vertices[downstream].z < mesh.vertices[vertex].z)
        );
        assert!(scratch.core_seed.iter().any(|&is_core| is_core));
        for (vertex, &is_core) in scratch.core_seed.iter().enumerate() {
            if is_core {
                assert!(scratch.core[vertex]);
                assert!(
                    adjacency[vertex]
                        .iter()
                        .all(|&neighbour| scratch.core[neighbour])
                );
            }
        }
    }

    #[test]
    fn causal_component_colours_are_stable_and_conflict_free() {
        let mesh = causal_two_basin_mesh(9);
        let adjacency = mesh.adjacency();
        let mut first = HydraulicScratch::default();
        let mut second = HydraulicScratch::default();
        for scratch in [&mut first, &mut second] {
            scratch.resize(mesh.vertices.len());
            prepare_causal_routing(&mesh, &adjacency, false, u32::MAX, scratch);
        }

        assert_eq!(first.downstream, second.downstream);
        assert_eq!(first.core, second.core);
        assert_eq!(first.components, second.components);
        assert_eq!(first.component_colours, second.component_colours);
        assert!(first.component_roots.len() >= 2);
        assert!(!first.component_conflicts.is_empty());
        for triangle in mesh.triangles.as_chunks::<3>().0 {
            let components = [triangle[0], triangle[1], triangle[2]]
                .map(|vertex| first.components[vertex as usize]);
            for pair in [[0, 1], [1, 2], [2, 0]] {
                let [left, right] = pair.map(|index| components[index]);
                if left != NO_CAUSAL_COMPONENT && right != NO_CAUSAL_COMPONENT && left != right {
                    assert_ne!(
                        first.component_colours[left as usize],
                        first.component_colours[right as usize]
                    );
                }
            }
        }
    }

    #[test]
    fn causal_prototype_is_deterministic_and_preserves_projected_faces() {
        let source = causal_test_mesh(7);
        let adjacency = source.adjacency();
        let source_areas: Vec<f32> = (0..source.triangles.len() / 3)
            .map(|face| projected_face_area(&source, face))
            .collect();
        let run = || {
            let mut mesh = source.clone();
            let mut material = SurfaceMaterial::empty(mesh.vertices.len());
            material
                .depths_mut()
                .iter_mut()
                .enumerate()
                .for_each(|(vertex, depth)| *depth = (vertex % 5) as f32 * 0.001);
            let bedrock_rates: Vec<f32> = (0..mesh.vertices.len())
                .map(|vertex| 0.2 + (vertex % 7) as f32 * 0.1)
                .collect();
            let mut scratch = HydraulicScratch::default();
            HydraulicEroder::new(
                &mut mesh,
                &adjacency,
                &mut material,
                &bedrock_rates,
                true,
                settings(),
            )
            .erode_causal_paths(
                &mut scratch,
                CausalHydraulicSettings {
                    epochs: 8,
                    core_source_threshold: 64,
                    cohort_sources: usize::MAX,
                    causal_frontiers: true,
                    frozen_routing: false,
                    reorder_components: true,
                },
            );
            (mesh, material, scratch.causal_stats)
        };

        let first = run();
        let second = run();

        assert_eq!(first, second);
        assert_eq!(first.2.paths, source.vertices.len());
        assert!(first.2.steps > first.2.paths);
        assert!(first.2.maximum_upland_components > 0);
        assert!(first.2.maximum_upland_component_sources <= 49);
        for (face, &reference) in source_areas.iter().enumerate() {
            let current = projected_face_area(&first.0, face);
            assert!(reference * current > 0.0);
            assert!(current.abs() + 1.0e-6 >= reference.abs() * 0.2);
        }
        assert!(
            first
                .1
                .depths()
                .iter()
                .all(|depth| depth.is_finite() && *depth >= 0.0)
        );
    }

    #[test]
    fn causal_live_routing_matches_the_reference_exactly() {
        let source = causal_test_mesh(7);
        let adjacency = source.adjacency();
        let mut material = SurfaceMaterial::empty(source.vertices.len());
        material
            .depths_mut()
            .iter_mut()
            .enumerate()
            .for_each(|(vertex, depth)| *depth = (vertex % 5) as f32 * 0.001);
        let initial_material = material.clone();
        let bedrock_rates: Vec<f32> = (0..source.vertices.len())
            .map(|vertex| 0.2 + (vertex % 7) as f32 * 0.1)
            .collect();
        let mut reference_mesh = source.clone();
        let mut reference_material = material.clone();
        hydraulic_erode_reference(
            &mut reference_mesh,
            &adjacency,
            &mut reference_material,
            &bedrock_rates,
            true,
            settings(),
        );
        let mut causal_mesh = source.clone();
        let mut causal_scratch = HydraulicScratch::default();
        HydraulicEroder::new(
            &mut causal_mesh,
            &adjacency,
            &mut material,
            &bedrock_rates,
            true,
            settings(),
        )
        .erode_causal_paths(
            &mut causal_scratch,
            CausalHydraulicSettings {
                epochs: 8,
                core_source_threshold: 4,
                cohort_sources: usize::MAX,
                causal_frontiers: true,
                frozen_routing: false,
                reorder_components: false,
            },
        );

        assert_eq!(causal_mesh, reference_mesh);
        assert_eq!(material, reference_material);

        let mut cohort_mesh = source;
        let mut cohort_material = initial_material;
        let mut cohort_scratch = HydraulicScratch::default();
        HydraulicEroder::new(
            &mut cohort_mesh,
            &adjacency,
            &mut cohort_material,
            &bedrock_rates,
            true,
            settings(),
        )
        .erode_causal_paths(
            &mut cohort_scratch,
            CausalHydraulicSettings {
                epochs: 8,
                core_source_threshold: 4,
                cohort_sources: 1,
                causal_frontiers: true,
                frozen_routing: false,
                reorder_components: true,
            },
        );

        assert_eq!(cohort_mesh, reference_mesh);
        assert_eq!(cohort_material, reference_material);
        assert_eq!(
            cohort_scratch.causal_stats.cohorts,
            cohort_mesh.vertices.len()
        );
        assert_eq!(cohort_scratch.causal_stats.maximum_cohort_sources, 1);
    }

    #[test]
    pub(super) fn deposition_fades_smoothly_between_gentle_and_steep_slopes() {
        let settings = settings();
        let gentle = deposition_weight(2.0_f32.to_radians().tan(), settings);
        let transition = deposition_weight(8.0_f32.to_radians().tan(), settings);
        let steep = deposition_weight(20.0_f32.to_radians().tan(), settings);
        assert!((gentle - 1.0).abs() < f32::EPSILON);
        assert!(transition > 0.0 && transition < gentle);
        assert!(steep.abs() < f32::EPSILON);
    }

    #[test]
    pub(super) fn final_sharp_rock_pass_marks_a_protrusion_but_not_an_inclined_plane() {
        let points: Vec<Vec2> = (0..=4)
            .flat_map(|y| (0..=4).map(move |x| Vec2::new(x as f32 / 4.0, y as f32 / 4.0)))
            .collect();
        let center = points
            .iter()
            .position(|point| *point == Vec2::splat(0.5))
            .unwrap();
        let mut mesh = Mesh::delaunay(&points);
        mesh.vertices.iter_mut().for_each(|vertex| vertex.z = 0.05);
        mesh.vertices[center].z = 0.25;
        mesh.calculate_normals();

        let forced_rock = sharp_rock_mask(&mesh);
        assert!(forced_rock[center]);
        let material = SurfaceMaterial::empty(mesh.vertices.len());
        let field = TerrainMaterialField::from_surface(
            &material,
            &vec![false; mesh.vertices.len()],
            &forced_rock,
        );
        assert_eq!(field.values[center].x.to_bits(), 1.0_f32.to_bits());

        mesh.vertices
            .iter_mut()
            .for_each(|vertex| vertex.z = vertex.x.mul_add(0.2, vertex.y * 0.1) + 0.05);
        mesh.calculate_normals();
        assert!(!sharp_rock_mask(&mesh).into_iter().any(|marked| marked));
    }

    #[test]
    pub(super) fn sediment_exchange_conserves_every_applied_transfer() {
        let settings = settings();
        let mut sediment = 1.0;
        let before = sediment;
        let deposited = exchange_sediment(
            &mut sediment,
            settings,
            exchange(0.2, 1.0, 1.0, shift_limits(10.0, f32::INFINITY), 0.0, 1.0),
        );
        assert!(deposited.vertical_deposit > 0.0);
        assert!((sediment + deposited.vertical_deposit - before).abs() < 1.0e-6);

        let mut sediment = 0.0;
        let before = sediment;
        let eroded = exchange_sediment(
            &mut sediment,
            settings,
            exchange(1.0, 0.0, 1.0, shift_limits(0.5, 0.3), 0.0, 1.0),
        );
        assert!(eroded.normal_retreat > 0.0);
        assert!((sediment - eroded.normal_retreat - before).abs() < 1.0e-6);

        let mut sediment = 1.0;
        let steep_deposit = exchange_sediment(
            &mut sediment,
            settings,
            exchange(0.2, 0.0, 1.0, shift_limits(10.0, f32::INFINITY), 0.0, 1.0),
        );
        assert!(steep_deposit.vertical_deposit.abs() < f32::EPSILON);
    }

    #[test]
    pub(super) fn loose_material_is_removed_before_hard_bedrock() {
        let settings = settings();
        let mut sediment = 0.0;
        let transfer = exchange_sediment(
            &mut sediment,
            settings,
            exchange(1.0, 0.0, 1.0, shift_limits(0.5, f32::INFINITY), 0.2, 0.1),
        );

        assert!((transfer.loose_removed - 0.2).abs() < 1.0e-6);
        assert!((transfer.bedrock_removed - 0.03).abs() < 1.0e-6);
        assert!((transfer.normal_retreat - 0.23).abs() < 1.0e-6);
        assert!((sediment - transfer.normal_retreat).abs() < 1.0e-6);
    }

    #[test]
    pub(super) fn material_volume_is_conserved_across_adaptive_tessellation() {
        let mut mesh = Mesh {
            vertices: vec![
                Vec3::new(-1.0, -1.0, 0.0),
                Vec3::new(1.0, -1.0, 0.0),
                Vec3::new(1.0, 1.0, 0.0),
                Vec3::new(-1.0, 1.0, 0.0),
                Vec3::new(0.0, 0.0, 0.8),
            ],
            normals: Vec::new(),
            triangles: vec![0, 1, 4, 1, 2, 4, 2, 3, 4, 3, 0, 4],
            uv: Vec::new(),
        };
        mesh.calculate_normals();
        let mut material = SurfaceMaterial::empty(mesh.vertices.len());
        material.deposited_depth = vec![0.01, 0.02, 0.03, 0.04, 0.08];
        let old_volume = material.volume(&mesh);
        let tessellation = mesh.tessellated_displaced_attributed(0.1);

        let (tessellated, material) = material.into_tessellated(&mesh, tessellation);
        let new_volume = material.volume(&tessellated);
        let relative_error = (new_volume - old_volume).abs() / old_volume;

        assert!(tessellated.vertices.len() > mesh.vertices.len());
        assert_eq!(material.depths().len(), tessellated.vertices.len());
        assert!(relative_error < 1.0e-6, "relative error: {relative_error}");
        assert!(
            material
                .depths()
                .iter()
                .all(|depth| depth.is_finite() && *depth >= 0.0)
        );
    }

    #[test]
    pub(super) fn hydraulic_erosion_peaks_at_forty_five_degrees_and_stops_at_vertical() {
        let flat = hydraulic_slope_erosion_weight(1.0);
        let thirty_degrees = hydraulic_slope_erosion_weight(0.866_025_4);
        let forty_five_degrees = hydraulic_slope_erosion_weight(0.707_106_77);
        let sixty_degrees = hydraulic_slope_erosion_weight(0.5);
        let steep = hydraulic_slope_erosion_weight(0.207_911_69);
        let near_vertical = hydraulic_slope_erosion_weight(0.017_452_406);

        assert_eq!(flat.to_bits(), 0.0_f32.to_bits());
        assert!((forty_five_degrees - 1.0).abs() < 1.0e-6);
        assert!((thirty_degrees - sixty_degrees).abs() < 1.0e-6);
        assert!(forty_five_degrees > thirty_degrees);
        assert!(sixty_degrees > steep);
        assert!(near_vertical < steep);
        assert_eq!(
            hydraulic_slope_erosion_weight(0.0).to_bits(),
            0.0_f32.to_bits()
        );
        assert_eq!(
            hydraulic_slope_erosion_weight(-0.1).to_bits(),
            0.0_f32.to_bits()
        );
    }

    #[test]
    pub(super) fn steep_hydraulic_erosion_blends_from_the_normal_toward_vertical() {
        let thirty_degrees = Vec3::new(0.5, 0.0, 0.866_025_4);
        let forty_five_degrees = Vec3::new(0.707_106_77, 0.0, 0.707_106_77);
        let sixty_degrees = Vec3::new(0.866_025_4, 0.0, 0.5);
        let near_vertical = Vec3::new(0.999_847_7, 0.0, 0.017_452_406);

        assert!(hydraulic_erosion_direction(thirty_degrees).abs_diff_eq(thirty_degrees, 1.0e-6));
        assert!(
            hydraulic_erosion_direction(forty_five_degrees).abs_diff_eq(forty_five_degrees, 1.0e-6)
        );
        let blended_sixty = hydraulic_erosion_direction(sixty_degrees);
        assert!(blended_sixty.z > sixty_degrees.z);
        assert!(blended_sixty.x < sixty_degrees.x);
        assert!(hydraulic_erosion_direction(near_vertical).z > 0.999);
    }

    #[test]
    pub(super) fn hydraulic_erosion_moves_down_normal_but_deposition_stays_vertical() {
        let normal = Vec3::new(0.6, 0.0, 0.8);
        let original = Vec3::new(0.4, 0.5, 0.2);
        let mut eroded = original;
        apply_hydraulic_transfer(
            &mut eroded,
            normal,
            HydraulicTransfer {
                normal_retreat: 0.05,
                ..HydraulicTransfer::default()
            },
        );
        assert!((eroded - (original - normal * 0.05)).length() < 1.0e-6);

        let mut deposited = original;
        apply_hydraulic_transfer(
            &mut deposited,
            normal,
            HydraulicTransfer {
                vertical_deposit: 0.05,
                ..HydraulicTransfer::default()
            },
        );
        assert!((deposited - (original + Vec3::Z * 0.05)).length() < 1.0e-6);
    }

    #[test]
    pub(super) fn vertical_faces_do_not_supply_hydraulic_sediment() {
        let settings = settings();
        let mut sediment = 0.0;
        let eroded = exchange_sediment(
            &mut sediment,
            settings,
            exchange(1.0, 0.0, 0.0, shift_limits(0.5, 0.3), 0.0, 1.0),
        );

        assert!(eroded.normal_retreat.abs() < f32::EPSILON);
        assert!(sediment.abs() < f32::EPSILON);
    }

    #[test]
    pub(super) fn hydraulic_pass_retreats_sloped_mesh_laterally() {
        let mut mesh = Mesh {
            vertices: vec![
                Vec3::new(0.0, 0.0, 1.0),
                Vec3::new(1.0, 0.0, 0.5),
                Vec3::new(0.0, 1.0, 0.5),
            ],
            normals: Vec::new(),
            triangles: vec![0, 1, 2],
            uv: Vec::new(),
        };
        let adjacency = mesh.adjacency();
        let original = mesh.vertices[0];
        let mut material = SurfaceMaterial::empty(mesh.vertices.len());
        let bedrock_rates = vec![1.0; mesh.vertices.len()];

        hydraulic_erode(
            &mut mesh,
            &adjacency,
            &mut material,
            &bedrock_rates,
            true,
            settings(),
        );

        assert!(mesh.vertices[0].x < original.x);
        assert!(mesh.vertices[0].y < original.y);
        assert!(mesh.vertices[0].z < original.z);
    }

    #[test]
    pub(super) fn hydraulic_cap_preserves_projected_triangle_orientation_and_area() {
        let mesh = Mesh {
            vertices: vec![
                Vec3::new(0.0, 0.0, 1.0),
                Vec3::new(1.0, 0.0, 0.0),
                Vec3::new(0.0, 1.0, 0.0),
            ],
            normals: Vec::new(),
            triangles: vec![0, 1, 2],
            uv: Vec::new(),
        };
        let vertex_faces = VertexFaceAdjacency::new(&mesh);
        let mut projected_areas = ProjectedFaceAreas::new(&mesh);
        let normal = Vec3::new(-0.7, -0.1, 0.707_106_77).normalize();
        let requested = 2.0;
        let cap = projected_areas.safe_erosion_cap(&mesh, &vertex_faces, 0, normal, requested);
        let mut moved_mesh = mesh.clone();
        moved_mesh.vertices[0] -= normal * cap;
        projected_areas.update_incident(&moved_mesh, &vertex_faces, 0);
        let second_cap =
            projected_areas.safe_erosion_cap(&moved_mesh, &vertex_faces, 0, normal, requested);
        let before_area = (mesh.vertices[1] - mesh.vertices[0])
            .truncate()
            .perp_dot((mesh.vertices[2] - mesh.vertices[0]).truncate());
        let after_area = (moved_mesh.vertices[1] - moved_mesh.vertices[0])
            .truncate()
            .perp_dot((moved_mesh.vertices[2] - moved_mesh.vertices[0]).truncate());

        assert!(cap < requested);
        assert!(before_area * after_area > 0.0);
        assert!(after_area.abs() >= before_area.abs() * 0.2);
        assert!(second_cap < f32::EPSILON);
    }

    #[test]
    pub(super) fn local_hydraulic_cap_is_edge_relative() {
        let mesh = Mesh {
            vertices: vec![
                Vec3::new(0.0, 0.0, 1.0),
                Vec3::new(0.5, 0.0, 0.5),
                Vec3::new(0.0, 1.0, 0.0),
            ],
            normals: Vec::new(),
            triangles: vec![0, 1, 2],
            uv: Vec::new(),
        };
        let adjacency = mesh.adjacency();
        let shortest_edge = mesh.vertices[0].distance(mesh.vertices[1]);
        let cap = local_hydraulic_erosion_cap(&mesh, &adjacency, 0, 10.0);

        assert!((cap - shortest_edge * 0.08).abs() < 1.0e-6);
    }

    #[test]
    pub(super) fn terrain_material_field_interpolates_at_export_positions() {
        let mut mesh = Mesh {
            vertices: vec![
                Vec3::new(0.0, 0.0, 0.0),
                Vec3::new(1.0, 0.0, 0.0),
                Vec3::new(0.0, 1.0, 0.0),
            ],
            normals: Vec::new(),
            triangles: vec![0, 1, 2],
            uv: vec![Vec2::ZERO, Vec2::X, Vec2::Y],
        };
        mesh.calculate_normals();
        let terrain = Terrain::new(mesh);
        let field = TerrainMaterialField {
            values: vec![Vec3::ZERO, Vec3::X, Vec3::Y],
        };

        let sample = field.sample(&terrain, Vec2::new(0.25, 0.5));

        assert!(sample.abs_diff_eq(Vec3::new(0.25, 0.5, 0.0), 1.0e-6));
    }

    #[test]
    pub(super) fn final_lod_correction_refines_and_pins_both_coarser_meshes() {
        let mut lod2 = Mesh::delaunay(&[Vec2::ZERO, Vec2::X, Vec2::ONE, Vec2::Y]);
        let mut lod1 = lod2.tessellated();
        let mut lod0 = lod1.tessellated().tessellated();
        lod0.vertices.iter_mut().for_each(|vertex| {
            vertex.z = vertex.x.mul_add(vertex.x * 0.2, vertex.y * vertex.y * 0.1);
        });
        lod0.calculate_normals();
        let lod1_vertex_count = lod1.vertices.len();
        let lod2_vertex_count = lod2.vertices.len();
        let lod1_triangle_count = lod1.triangles.len();
        let lod2_triangle_count = lod2.triangles.len();
        let lod1_shared = lod0.vertices[..lod1_vertex_count].to_vec();
        let lod2_shared = lod0.vertices[..lod2_vertex_count].to_vec();

        correct_lods(&mut lod0, &mut lod1, &mut lod2);

        assert_eq!(lod1.triangles.len(), lod1_triangle_count * 4);
        assert_eq!(lod2.triangles.len(), lod2_triangle_count * 4);
        assert_eq!(lod1.vertices[..lod1_vertex_count], lod1_shared);
        assert_eq!(lod2.vertices[..lod2_vertex_count], lod2_shared);
        let index = TriangleIndex::new(&lod0);
        for mesh in [&lod1, &lod2] {
            assert!(mesh.vertices.iter().all(|vertex| {
                let elevation = sample_mesh_surface(&lod0, &index, vertex.x, vertex.y).0;
                (elevation - vertex.z).abs() < 1.0e-6
            }));
        }
    }
}
