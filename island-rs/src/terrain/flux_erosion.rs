use rayon::prelude::*;

use super::erosion::{
    VertexFaceAdjacency, calculate_normals_with_faces, hydraulic_erosion_direction,
    hydraulic_slope_erosion_weight,
};
use super::{Adjacency, IslandOptions, Mesh, SurfaceMaterial, Vec3};

const DEFAULT_ITERATIONS: usize = 48;

#[derive(Default)]
pub(super) struct FluxErosionScratch {
    offsets: Vec<usize>,
    neighbours: Vec<usize>,
    sources: Vec<usize>,
    reverse_edges: Vec<usize>,
    edge_weights: Vec<f32>,
    water_flux: Vec<f32>,
    sediment_flux: Vec<f32>,
    edge_channel_memory: Vec<f32>,
    edge_channel_next: Vec<f32>,
    outflow: Vec<f32>,
    control_area: Vec<f32>,
    water: Vec<f32>,
    water_next: Vec<f32>,
    sediment: Vec<f32>,
    sediment_next: Vec<f32>,
    channel_memory: Vec<f32>,
    channel_next: Vec<f32>,
    position_delta: Vec<Vec3>,
    loose_delta: Vec<f32>,
}

#[derive(Clone, Copy, Debug)]
struct FluxErosionSettings {
    iterations: usize,
    rainfall: f32,
    flow_fraction: f32,
    evaporation: f32,
    channel_decay: f32,
    channel_feedback: f32,
    flow_exponent: f32,
    shift_ratio: f32,
    retained_sediment: f32,
    capacity_scale: f32,
    erosion_strength: f32,
    deposition_strength: f32,
}

impl FluxErosionSettings {
    fn new(stage_strength: f32, options: IslandOptions) -> Self {
        Self {
            iterations: positive_environment("MOTU_FLUX_EROSION_ITERATIONS", DEFAULT_ITERATIONS),
            rainfall: float_environment("MOTU_FLUX_EROSION_RAINFALL", 0.12),
            flow_fraction: 0.72,
            evaporation: 0.08,
            channel_decay: 0.92,
            channel_feedback: float_environment("MOTU_FLUX_EROSION_CHANNEL_FEEDBACK", 14.0),
            flow_exponent: float_environment("MOTU_FLUX_EROSION_FLOW_EXPONENT", 2.5).max(1.0),
            shift_ratio: float_environment("MOTU_FLUX_EROSION_SHIFT_RATIO", 0.22),
            retained_sediment: float_environment("MOTU_FLUX_EROSION_RETAINED_SEDIMENT", 0.15)
                .clamp(0.0, 1.0),
            capacity_scale: float_environment("MOTU_FLUX_EROSION_CAPACITY_SCALE", 250.0),
            erosion_strength: stage_strength * options.hydraulic_erosion_strength,
            deposition_strength: options.hydraulic_deposition_strength,
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn erode_flux_field(
    mesh: &mut Mesh,
    adjacency: &Adjacency,
    material: &mut SurfaceMaterial,
    bedrock_rates: &[f32],
    include_sea: bool,
    stage_strength: f32,
    options: IslandOptions,
    scratch: &mut FluxErosionScratch,
) {
    let settings = FluxErosionSettings::new(stage_strength, options);
    if settings.erosion_strength == 0.0 || mesh.vertices.is_empty() {
        return;
    }

    scratch.prepare(mesh, adjacency, include_sea, settings.rainfall);
    let vertex_faces = VertexFaceAdjacency::new(mesh);
    let mean_edge = mean_edge_length(mesh, &scratch.sources, &scratch.neighbours);
    let iteration_shift = mean_edge * settings.shift_ratio * settings.erosion_strength
        / settings.iterations.max(1) as f32;

    for _ in 0..settings.iterations {
        calculate_normals_with_faces(mesh, &vertex_faces);
        calculate_edge_weights(mesh, include_sea, mean_edge, settings, scratch);
        normalize_fluxes(settings, scratch);
        update_edge_channel_memory(mesh, settings, scratch);
        gather_fluxes(mesh, include_sea, settings, scratch);
        calculate_exchange(
            mesh,
            material,
            bedrock_rates,
            &vertex_faces,
            include_sea,
            iteration_shift,
            settings,
            scratch,
        );
        apply_exchange(mesh, material, scratch);
        std::mem::swap(&mut scratch.water, &mut scratch.water_next);
        std::mem::swap(&mut scratch.sediment, &mut scratch.sediment_next);
        std::mem::swap(&mut scratch.channel_memory, &mut scratch.channel_next);
        std::mem::swap(
            &mut scratch.edge_channel_memory,
            &mut scratch.edge_channel_next,
        );
    }

    deposit_suspended_material(
        mesh,
        material,
        include_sea,
        settings.retained_sediment,
        scratch,
    );
    mesh.calculate_normals();
}

impl FluxErosionScratch {
    fn prepare(&mut self, mesh: &Mesh, adjacency: &Adjacency, include_sea: bool, rainfall: f32) {
        self.rebuild_topology(adjacency);
        let vertex_count = mesh.vertices.len();
        let edge_count = self.neighbours.len();
        for buffer in [
            &mut self.control_area,
            &mut self.water,
            &mut self.water_next,
            &mut self.sediment,
            &mut self.sediment_next,
            &mut self.channel_memory,
            &mut self.channel_next,
            &mut self.loose_delta,
            &mut self.outflow,
        ] {
            buffer.clear();
            buffer.resize(vertex_count, 0.0);
        }
        self.position_delta.clear();
        self.position_delta.resize(vertex_count, Vec3::ZERO);
        for buffer in [
            &mut self.edge_weights,
            &mut self.water_flux,
            &mut self.sediment_flux,
            &mut self.edge_channel_memory,
            &mut self.edge_channel_next,
        ] {
            buffer.clear();
            buffer.resize(edge_count, 0.0);
        }

        for triangle in mesh.triangles.as_chunks::<3>().0 {
            let [a, b, c] = triangle.map(|vertex| mesh.vertices[vertex as usize].truncate());
            let share = (b - a).perp_dot(c - a).abs() / 6.0;
            for vertex in triangle {
                self.control_area[*vertex as usize] += share;
            }
        }
        let mean_area = self
            .control_area
            .iter()
            .zip(&mesh.vertices)
            .filter(|(_, vertex)| include_sea || vertex.z >= 0.0)
            .map(|(area, _)| *area)
            .sum::<f32>()
            / mesh.vertices.len().max(1) as f32;
        self.water
            .par_iter_mut()
            .zip(&self.control_area)
            .zip(&mesh.vertices)
            .for_each(|((water, &area), vertex)| {
                *water = if (include_sea || vertex.z >= 0.0) && mean_area > f32::EPSILON {
                    rainfall * area / mean_area
                } else {
                    0.0
                };
            });
    }

    fn rebuild_topology(&mut self, adjacency: &Adjacency) {
        self.offsets.clear();
        self.offsets.reserve(adjacency.len() + 1);
        self.offsets.push(0);
        self.neighbours.clear();
        self.sources.clear();
        for (source, neighbours) in adjacency.iter().enumerate() {
            self.neighbours.extend(neighbours.iter().copied());
            self.sources
                .extend(std::iter::repeat_n(source, neighbours.len()));
            self.offsets.push(self.neighbours.len());
        }
        self.reverse_edges.clear();
        self.reverse_edges.resize(self.neighbours.len(), usize::MAX);
        for (edge, (&source, &target)) in self.sources.iter().zip(&self.neighbours).enumerate() {
            let reverse_slot = adjacency[target]
                .binary_search(&source)
                .expect("terrain adjacency must be symmetric");
            self.reverse_edges[edge] = self.offsets[target] + reverse_slot;
        }
    }
}

fn calculate_edge_weights(
    mesh: &Mesh,
    include_sea: bool,
    mean_edge: f32,
    settings: FluxErosionSettings,
    scratch: &mut FluxErosionScratch,
) {
    let head_scale = mean_edge * 0.08;
    scratch
        .edge_weights
        .par_iter_mut()
        .enumerate()
        .for_each(|(edge, weight)| {
            let source = scratch.sources[edge];
            let target = scratch.neighbours[edge];
            let source_vertex = mesh.vertices[source];
            let target_vertex = mesh.vertices[target];
            if (!include_sea && source_vertex.z < 0.0) || scratch.water[source] <= 0.0 {
                *weight = 0.0;
                return;
            }
            let source_head = source_vertex.z + scratch.water[source] * head_scale;
            let target_water = if !include_sea && target_vertex.z < 0.0 {
                0.0
            } else {
                scratch.water[target]
            };
            let target_head = target_vertex.z + target_water * head_scale;
            let drop = (source_head - target_head).max(0.0);
            let distance = source_vertex
                .truncate()
                .distance(target_vertex.truncate())
                .max(f32::EPSILON);
            let channel = scratch.edge_channel_memory[edge]
                .max(scratch.channel_memory[source])
                .max(scratch.channel_memory[target]);
            let rill_bias = 0.85 + 0.3 * edge_noise(source, target);
            *weight = (drop / distance * (1.0 + settings.channel_feedback * channel) * rill_bias)
                .powf(settings.flow_exponent);
        });

    scratch
        .outflow
        .par_iter_mut()
        .enumerate()
        .for_each(|(vertex, total)| {
            *total = scratch.edge_weights[scratch.offsets[vertex]..scratch.offsets[vertex + 1]]
                .iter()
                .sum();
        });
}

fn update_edge_channel_memory(
    mesh: &Mesh,
    settings: FluxErosionSettings,
    scratch: &mut FluxErosionScratch,
) {
    scratch
        .edge_channel_next
        .par_iter_mut()
        .enumerate()
        .for_each(|(edge, memory)| {
            let source = scratch.sources[edge];
            let target = scratch.neighbours[edge];
            let source_position = mesh.vertices[source];
            let target_position = mesh.vertices[target];
            let slope = (source_position.z - target_position.z).max(0.0)
                / source_position
                    .truncate()
                    .distance(target_position.truncate())
                    .max(f32::EPSILON);
            let stream_power = scratch.water_flux[edge] * slope;
            let sample = (stream_power / (0.03 + stream_power)).sqrt();
            *memory = (scratch.edge_channel_memory[edge] * settings.channel_decay
                + sample * (1.0 - settings.channel_decay))
                .clamp(0.0, 1.0);
        });
}

fn normalize_fluxes(settings: FluxErosionSettings, scratch: &mut FluxErosionScratch) {
    scratch
        .water_flux
        .par_iter_mut()
        .zip(&mut scratch.sediment_flux)
        .enumerate()
        .for_each(|(edge, (water_flux, sediment_flux))| {
            let source = scratch.sources[edge];
            let total_weight = scratch.outflow[source];
            if total_weight <= f32::EPSILON {
                *water_flux = 0.0;
                *sediment_flux = 0.0;
                return;
            }
            let fraction = scratch.edge_weights[edge] / total_weight;
            *water_flux = scratch.water[source] * settings.flow_fraction * fraction;
            *sediment_flux = scratch.sediment[source] * settings.flow_fraction * fraction;
        });
}

fn gather_fluxes(
    mesh: &Mesh,
    include_sea: bool,
    settings: FluxErosionSettings,
    scratch: &mut FluxErosionScratch,
) {
    scratch
        .water_next
        .par_iter_mut()
        .zip(&mut scratch.sediment_next)
        .zip(&mut scratch.channel_next)
        .enumerate()
        .for_each(|(vertex, ((water_next, sediment_next), channel_next))| {
            if !include_sea && mesh.vertices[vertex].z < 0.0 {
                *water_next = 0.0;
                *sediment_next = 0.0;
                *channel_next = 0.0;
                return;
            }
            let edges = scratch.offsets[vertex]..scratch.offsets[vertex + 1];
            let outgoing_water = scratch.water_flux[edges.clone()].iter().sum::<f32>();
            let outgoing_sediment = scratch.sediment_flux[edges.clone()].iter().sum::<f32>();
            let incoming_water = scratch.reverse_edges[edges.clone()]
                .iter()
                .map(|&reverse| scratch.water_flux[reverse])
                .sum::<f32>();
            let incoming_sediment = scratch.reverse_edges[edges.clone()]
                .iter()
                .map(|&reverse| scratch.sediment_flux[reverse])
                .sum::<f32>();
            *water_next = ((scratch.water[vertex] - outgoing_water).max(0.0)
                + incoming_water
                + settings.rainfall)
                * (1.0 - settings.evaporation);
            *sediment_next =
                (scratch.sediment[vertex] - outgoing_sediment).max(0.0) + incoming_sediment;
            let position = mesh.vertices[vertex];
            let maximum_slope = scratch.neighbours[edges]
                .iter()
                .map(|&neighbour| {
                    let target = mesh.vertices[neighbour];
                    (position.z - target.z).max(0.0)
                        / position
                            .truncate()
                            .distance(target.truncate())
                            .max(f32::EPSILON)
                })
                .fold(0.0_f32, f32::max);
            let throughput = outgoing_water + incoming_water;
            let stream_power = throughput * maximum_slope;
            let memory_sample = (stream_power / (0.1 + stream_power)).sqrt();
            *channel_next = (scratch.channel_memory[vertex] * settings.channel_decay
                + memory_sample * (1.0 - settings.channel_decay))
                .clamp(0.0, 1.0);
        });
}

#[allow(clippy::too_many_arguments)]
fn calculate_exchange(
    mesh: &Mesh,
    material: &SurfaceMaterial,
    bedrock_rates: &[f32],
    _vertex_faces: &VertexFaceAdjacency,
    include_sea: bool,
    iteration_shift: f32,
    settings: FluxErosionSettings,
    scratch: &mut FluxErosionScratch,
) {
    scratch
        .position_delta
        .par_iter_mut()
        .zip(&mut scratch.loose_delta)
        .zip(&mut scratch.sediment_next)
        .enumerate()
        .for_each(|(vertex, ((position_delta, loose_delta), sediment))| {
            let position = mesh.vertices[vertex];
            if (!include_sea && position.z < 0.0) || scratch.water_next[vertex] <= 0.0 {
                *position_delta = Vec3::ZERO;
                *loose_delta = 0.0;
                return;
            }
            let edges = scratch.offsets[vertex]..scratch.offsets[vertex + 1];
            let maximum_slope = scratch.neighbours[edges.clone()]
                .iter()
                .map(|&neighbour| {
                    let target = mesh.vertices[neighbour];
                    (position.z - target.z).max(0.0)
                        / position
                            .truncate()
                            .distance(target.truncate())
                            .max(f32::EPSILON)
                })
                .fold(0.0_f32, f32::max);
            let throughput = scratch.water_flux[edges].iter().sum::<f32>();
            let channel = scratch.channel_next[vertex];
            let stream_power =
                throughput.sqrt() * maximum_slope * (0.04 + 0.96 * channel * channel);
            let capacity = stream_power * iteration_shift * settings.capacity_scale;
            let difference = capacity - *sediment;
            if difference > 0.0 {
                let normal = mesh.normals[vertex];
                let slope_weight = hydraulic_slope_erosion_weight(normal.z);
                let requested = difference.min(iteration_shift) * slope_weight;
                let loose = material.depths()[vertex].min(requested);
                let bedrock = (requested - loose) * bedrock_rates[vertex];
                let removed = loose + bedrock;
                let direction = hydraulic_erosion_direction(normal);
                let sea_limit = if include_sea {
                    removed
                } else if direction.z > f32::EPSILON {
                    removed.min(position.z.max(0.0) / direction.z)
                } else {
                    0.0
                };
                *position_delta = -direction * sea_limit;
                *loose_delta = -loose.min(sea_limit);
                *sediment += sea_limit;
            } else {
                let deposit = (-difference * 0.18 * settings.deposition_strength)
                    .min(iteration_shift)
                    .min(*sediment);
                *position_delta = Vec3::Z * deposit;
                *loose_delta = deposit;
                *sediment -= deposit;
            }
        });
}

fn apply_exchange(mesh: &mut Mesh, material: &mut SurfaceMaterial, scratch: &FluxErosionScratch) {
    mesh.vertices
        .par_iter_mut()
        .zip(material.depths_mut())
        .zip(&scratch.position_delta)
        .zip(&scratch.loose_delta)
        .for_each(|(((vertex, loose_depth), &position_delta), &loose_delta)| {
            *vertex += position_delta;
            *loose_depth = (*loose_depth + loose_delta).max(0.0);
        });
}

fn deposit_suspended_material(
    mesh: &mut Mesh,
    material: &mut SurfaceMaterial,
    include_sea: bool,
    retained_sediment: f32,
    scratch: &FluxErosionScratch,
) {
    mesh.vertices
        .par_iter_mut()
        .zip(material.depths_mut())
        .zip(&scratch.sediment)
        .for_each(|((vertex, loose_depth), &sediment)| {
            if include_sea || vertex.z >= 0.0 {
                let deposit = sediment * retained_sediment;
                vertex.z += deposit;
                *loose_depth += deposit;
            }
        });
}

fn mean_edge_length(mesh: &Mesh, sources: &[usize], neighbours: &[usize]) -> f32 {
    sources
        .par_iter()
        .zip(neighbours)
        .map(|(&source, &target)| {
            mesh.vertices[source]
                .truncate()
                .distance(mesh.vertices[target].truncate())
        })
        .sum::<f32>()
        / sources.len().max(1) as f32
}

fn edge_noise(source: usize, target: usize) -> f32 {
    let mut value = (source as u64)
        .wrapping_mul(0x9E37_79B9_7F4A_7C15)
        .wrapping_add((target as u64).wrapping_mul(0xBF58_476D_1CE4_E5B9));
    value ^= value >> 30;
    value = value.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    value ^= value >> 27;
    (value >> 40) as f32 / ((1_u32 << 24) - 1) as f32
}

fn positive_environment(name: &str, default: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse().ok())
        .filter(|&value| value > 0)
        .unwrap_or(default)
}

fn float_environment(name: &str, default: f32) -> f32 {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse().ok())
        .filter(|value: &f32| value.is_finite() && *value >= 0.0)
        .unwrap_or(default)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Vec2;
    use crate::terrain::erosion::projected_face_area;

    #[test]
    fn edge_noise_is_stable_and_bounded() {
        let forward = edge_noise(3, 7);
        assert_eq!(forward.to_bits(), edge_noise(3, 7).to_bits());
        assert!((0.0..=1.0).contains(&forward));
        assert_ne!(forward.to_bits(), edge_noise(7, 3).to_bits());
    }

    #[test]
    fn flux_field_is_deterministic_finite_and_erodes_a_closed_surface() {
        let points: Vec<Vec2> = (0..=5)
            .flat_map(|y| (0..=5).map(move |x| Vec2::new(x as f32 / 5.0, y as f32 / 5.0)))
            .collect();
        let mut source = Mesh::delaunay(&points);
        source.vertices.iter_mut().for_each(|vertex| {
            vertex.z = 0.03 + 0.12 * vertex.y + 0.02 * (vertex.x * 11.0).sin();
        });
        source.calculate_normals();
        let projected_areas: Vec<f32> = (0..source.triangles.len() / 3)
            .map(|face| projected_face_area(&source, face))
            .collect();
        let adjacency = source.adjacency();
        let material = SurfaceMaterial::empty(source.vertices.len());
        let bedrock_rates = vec![0.65; source.vertices.len()];
        let initial_height = source.vertices.iter().map(|vertex| vertex.z).sum::<f32>();
        let run = || {
            let mut mesh = source.clone();
            let mut material = material.clone();
            let mut scratch = FluxErosionScratch::default();
            erode_flux_field(
                &mut mesh,
                &adjacency,
                &mut material,
                &bedrock_rates,
                true,
                0.8,
                IslandOptions::default(),
                &mut scratch,
            );
            (mesh, material)
        };

        let first = run();
        let second = run();
        assert_eq!(first, second);
        assert!(first.0.vertices.iter().all(|vertex| vertex.is_finite()));
        assert!(first.1.depths().iter().all(|depth| depth.is_finite()));
        for (face, &before) in projected_areas.iter().enumerate() {
            let after = projected_face_area(&first.0, face);
            assert!(before * after > 0.0);
            assert!(after.abs() >= before.abs() * 0.15);
        }
        let final_height = first.0.vertices.iter().map(|vertex| vertex.z).sum::<f32>();
        assert!(final_height < initial_height);
    }
}
