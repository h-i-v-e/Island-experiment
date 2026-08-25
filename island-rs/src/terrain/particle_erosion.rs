use std::{
    sync::atomic::{AtomicI32, Ordering},
    time::Instant,
};

use rayon::prelude::*;

use super::erosion::{
    HydraulicErosionSettings, HydraulicExchange, HydraulicShiftLimits, VertexFaceAdjacency,
    calculate_normals_with_faces, deposition_weight, exchange_sediment,
    hydraulic_erosion_direction, hydraulic_slope_erosion_weight, local_hydraulic_erosion_cap,
};
use super::{Adjacency, IslandOptions, LOOSE_DEPTH_EPSILON, Mesh, SurfaceMaterial, Vec3};

// A batch deliberately contains many independent droplets. On the GPU each droplet is one
// invocation and these four arrays become atomic<i32> storage buffers.
const DEFAULT_BATCHES: usize = 32;
const FIXED_POINT_SCALE: f32 = (1_u32 << 22) as f32;
const FIXED_POINT_INVERSE: f32 = 1.0 / FIXED_POINT_SCALE;

#[derive(Default)]
pub(super) struct ParticleErosionScratch {
    order: Vec<usize>,
    active_sources: Vec<usize>,
    position_x: Vec<AtomicI32>,
    position_y: Vec<AtomicI32>,
    position_z: Vec<AtomicI32>,
    loose_delta: Vec<AtomicI32>,
    move_limits: Vec<f32>,
}

#[derive(Clone, Copy, Debug)]
struct ParticleErosionSettings {
    batches: usize,
    route_jitter: f32,
    batch_shift_ratio: f32,
    settings: HydraulicErosionSettings,
}

impl ParticleErosionSettings {
    fn new(stage_strength: f32, options: IslandOptions) -> Self {
        Self {
            batches: positive_environment("MOTU_PARTICLE_EROSION_BATCHES", DEFAULT_BATCHES),
            route_jitter: float_environment("MOTU_PARTICLE_EROSION_ROUTE_JITTER", 0.18)
                .clamp(0.0, 0.8),
            batch_shift_ratio: float_environment("MOTU_PARTICLE_EROSION_BATCH_SHIFT", 0.045)
                .clamp(0.001, 0.08),
            settings: HydraulicErosionSettings::new(stage_strength, options),
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn erode_particle_batches(
    mesh: &mut Mesh,
    adjacency: &Adjacency,
    material: &mut SurfaceMaterial,
    bedrock_rates: &[f32],
    include_sea: bool,
    stage_strength: f32,
    options: IslandOptions,
    scratch: &mut ParticleErosionScratch,
) {
    let started = Instant::now();
    let prototype = ParticleErosionSettings::new(stage_strength, options);
    if prototype.settings.erosion_strength == 0.0 || mesh.vertices.is_empty() {
        return;
    }

    scratch.prepare(mesh);
    let vertex_faces = VertexFaceAdjacency::new(mesh);
    let global_shift = mesh
        .vertices
        .iter()
        .map(|vertex| vertex.z)
        .fold(0.0_f32, f32::max)
        * 0.012;
    let land_sources = scratch
        .order
        .partition_point(|&source| mesh.vertices[source].z > 0.0);
    let batch_count = prototype.batches.min(land_sources.max(1));

    for batch in 0..batch_count {
        calculate_normals_with_faces(mesh, &vertex_faces);
        scratch.clear_accumulators();
        scratch.active_sources.clear();
        scratch.active_sources.extend(
            (batch..land_sources)
                .step_by(batch_count)
                .map(|rank| scratch.order[rank]),
        );

        // Nothing inside this dispatch mutates terrain. Atomic integer addition is associative,
        // so workgroup scheduling cannot alter the accumulated result.
        scratch.active_sources.par_iter().for_each(|&source| {
            trace_particle(
                source,
                mesh,
                adjacency,
                material,
                bedrock_rates,
                include_sea,
                global_shift,
                prototype,
                scratch,
            );
        });

        apply_batch(mesh, adjacency, material, global_shift, prototype, scratch);
    }

    mesh.calculate_normals();
    if std::env::var_os("MOTU_PARTICLE_EROSION_STATS").is_some() {
        eprintln!(
            "cpu-particle-erosion vertices={} batches={} elapsed_ms={:.3}",
            mesh.vertices.len(),
            batch_count,
            started.elapsed().as_secs_f64() * 1_000.0,
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn trace_particle(
    source: usize,
    mesh: &Mesh,
    adjacency: &Adjacency,
    material: &SurfaceMaterial,
    bedrock_rates: &[f32],
    include_sea: bool,
    global_shift: f32,
    prototype: ParticleErosionSettings,
    scratch: &ParticleErosionScratch,
) {
    let mut current = source;
    let mut speed = 0.0_f32;
    let mut sediment = 0.0_f32;

    for step in 0..mesh.vertices.len() {
        let Some(next) = choose_downstream(
            current,
            source,
            step,
            mesh,
            adjacency,
            prototype.route_jitter,
        ) else {
            accumulate_deposit(current, &mut sediment, global_shift, prototype, scratch);
            break;
        };
        if mesh.vertices[next].z < 0.0 && !include_sea {
            accumulate_deposit(current, &mut sediment, global_shift, prototype, scratch);
            break;
        }

        let fall = mesh.vertices[current] - mesh.vertices[next];
        let distance = fall.length().max(f32::EPSILON);
        let horizontal_distance = fall.truncate().length().max(f32::EPSILON);
        let slope = fall.z / horizontal_distance;
        let sin_slope = fall.z / distance;
        let acceleration = sin_slope * sin_slope * sin_slope * distance;
        speed = speed.mul_add(0.75, acceleration * 0.25);

        let normal = mesh.normals[current];
        let erosion_direction = hydraulic_erosion_direction(normal);
        let edge_cap = local_hydraulic_erosion_cap(mesh, adjacency, current, global_shift);
        let available_material = if include_sea {
            f32::INFINITY
        } else if erosion_direction.z > f32::EPSILON {
            mesh.vertices[current].z.max(0.0) / erosion_direction.z
        } else {
            0.0
        };
        let transfer = exchange_sediment(
            &mut sediment,
            prototype.settings,
            HydraulicExchange {
                capacity: speed,
                deposition_weight: deposition_weight(slope, prototype.settings),
                slope_erosion_weight: hydraulic_slope_erosion_weight(normal.z),
                limits: HydraulicShiftLimits {
                    deposition: global_shift,
                    erosion: edge_cap,
                    available_material,
                },
                loose_available: material.depths()[current],
                bedrock_rate: bedrock_rates[current],
            },
        );
        let position_delta =
            -erosion_direction * transfer.normal_retreat + Vec3::Z * transfer.vertical_deposit;
        accumulate_vec3(current, position_delta, scratch);
        atomic_add_f32(
            &scratch.loose_delta[current],
            transfer.vertical_deposit - transfer.loose_removed,
        );
        current = next;
    }
}

fn choose_downstream(
    current: usize,
    source: usize,
    step: usize,
    mesh: &Mesh,
    adjacency: &Adjacency,
    route_jitter: f32,
) -> Option<usize> {
    let position = mesh.vertices[current];
    adjacency[current]
        .iter()
        .copied()
        .filter(|&candidate| mesh.vertices[candidate].z < position.z)
        .max_by(|&left, &right| {
            route_score(current, left, source, step, mesh, route_jitter).total_cmp(&route_score(
                current,
                right,
                source,
                step,
                mesh,
                route_jitter,
            ))
        })
}

fn route_score(
    current: usize,
    target: usize,
    source: usize,
    step: usize,
    mesh: &Mesh,
    jitter: f32,
) -> f32 {
    let drop = mesh.vertices[current].z - mesh.vertices[target].z;
    // Most of the bias belongs to the edge, so droplets tend to agree and form channels.
    // A much smaller per-particle term prevents a perfectly static drainage tree.
    let edge_bias = unit_hash(current as u64, target as u64);
    let particle_bias = unit_hash(
        source as u64 ^ (step as u64).wrapping_mul(0x9E37_79B9),
        target as u64,
    );
    drop * (1.0 + jitter * ((edge_bias - 0.5) + 0.2 * (particle_bias - 0.5)))
}

fn accumulate_deposit(
    current: usize,
    sediment: &mut f32,
    global_shift: f32,
    prototype: ParticleErosionSettings,
    scratch: &ParticleErosionScratch,
) {
    let deposited = (*sediment * prototype.settings.deposition_strength * 0.35)
        .min(global_shift)
        .min(*sediment);
    *sediment -= deposited;
    atomic_add_f32(&scratch.position_z[current], deposited);
    atomic_add_f32(&scratch.loose_delta[current], deposited);
}

fn apply_batch(
    mesh: &mut Mesh,
    adjacency: &Adjacency,
    material: &mut SurfaceMaterial,
    global_shift: f32,
    prototype: ParticleErosionSettings,
    scratch: &mut ParticleErosionScratch,
) {
    let vertices = &mesh.vertices;
    scratch
        .move_limits
        .par_iter_mut()
        .enumerate()
        .for_each(|(index, limit)| {
            *limit = adjacency[index]
                .iter()
                .map(|&neighbour| vertices[index].distance(vertices[neighbour]))
                .fold(f32::INFINITY, f32::min)
                * prototype.batch_shift_ratio;
        });
    mesh.vertices
        .par_iter_mut()
        .zip(material.depths_mut())
        .enumerate()
        .for_each(|(index, (vertex, loose_depth))| {
            let mut delta = Vec3::new(
                decode(&scratch.position_x[index]),
                decode(&scratch.position_y[index]),
                decode(&scratch.position_z[index]),
            );
            let limit = scratch.move_limits[index].min(global_shift);
            let length = delta.length();
            if length > limit && length > f32::EPSILON {
                delta *= limit / length;
            }
            *vertex += delta;
            let requested_loose = decode(&scratch.loose_delta[index]);
            *loose_depth = (*loose_depth + requested_loose).max(0.0);
            if *loose_depth < LOOSE_DEPTH_EPSILON {
                *loose_depth = 0.0;
            }
        });
}

impl ParticleErosionScratch {
    fn prepare(&mut self, mesh: &Mesh) {
        self.order.clear();
        self.order.extend(0..mesh.vertices.len());
        self.order.sort_unstable_by(|&left, &right| {
            mesh.vertices[right].z.total_cmp(&mesh.vertices[left].z)
        });
        resize_atomics(&mut self.position_x, mesh.vertices.len());
        resize_atomics(&mut self.position_y, mesh.vertices.len());
        resize_atomics(&mut self.position_z, mesh.vertices.len());
        resize_atomics(&mut self.loose_delta, mesh.vertices.len());
        self.move_limits.clear();
        self.move_limits.resize(mesh.vertices.len(), 0.0);
    }

    fn clear_accumulators(&self) {
        [
            self.position_x.as_slice(),
            self.position_y.as_slice(),
            self.position_z.as_slice(),
            self.loose_delta.as_slice(),
        ]
        .into_par_iter()
        .flatten()
        .for_each(|value| value.store(0, Ordering::Relaxed));
    }
}

fn resize_atomics(values: &mut Vec<AtomicI32>, len: usize) {
    values.clear();
    values.resize_with(len, AtomicI32::default);
}

fn accumulate_vec3(vertex: usize, delta: Vec3, scratch: &ParticleErosionScratch) {
    atomic_add_f32(&scratch.position_x[vertex], delta.x);
    atomic_add_f32(&scratch.position_y[vertex], delta.y);
    atomic_add_f32(&scratch.position_z[vertex], delta.z);
}

fn atomic_add_f32(target: &AtomicI32, value: f32) {
    target.fetch_add(quantize(value), Ordering::Relaxed);
}

fn quantize(value: f32) -> i32 {
    (value * FIXED_POINT_SCALE)
        .round()
        .clamp(i32::MIN as f32, i32::MAX as f32) as i32
}

fn decode(value: &AtomicI32) -> f32 {
    value.load(Ordering::Relaxed) as f32 * FIXED_POINT_INVERSE
}

fn unit_hash(left: u64, right: u64) -> f32 {
    let mut value = left
        .wrapping_mul(0x9E37_79B9_7F4A_7C15)
        .wrapping_add(right.wrapping_mul(0xBF58_476D_1CE4_E5B9));
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
    fn quantized_accumulation_is_order_independent() {
        let accumulator = AtomicI32::new(0);
        for value in [0.003, -0.001, 0.004, -0.002] {
            atomic_add_f32(&accumulator, value);
        }
        let forward = accumulator.load(Ordering::Relaxed);
        accumulator.store(0, Ordering::Relaxed);
        for value in [0.004, -0.002, 0.003, -0.001] {
            atomic_add_f32(&accumulator, value);
        }
        assert_eq!(forward, accumulator.load(Ordering::Relaxed));
    }

    #[test]
    fn route_hash_is_stable_and_bounded() {
        let forward = unit_hash(17, 29);
        assert_eq!(forward.to_bits(), unit_hash(17, 29).to_bits());
        assert!((0.0..=1.0).contains(&forward));
        assert_ne!(forward.to_bits(), unit_hash(29, 17).to_bits());
    }

    #[test]
    fn particle_batches_are_deterministic_finite_and_preserve_face_orientation() {
        let points: Vec<Vec2> = (0..=6)
            .flat_map(|y| (0..=6).map(move |x| Vec2::new(x as f32 / 6.0, y as f32 / 6.0)))
            .collect();
        let mut source = Mesh::delaunay(&points);
        source.vertices.iter_mut().for_each(|vertex| {
            vertex.z = 0.04 + 0.11 * vertex.y + 0.025 * (vertex.x * 13.0).sin();
        });
        source.calculate_normals();
        let initial_height = source.vertices.iter().map(|vertex| vertex.z).sum::<f32>();
        let initial_areas: Vec<f32> = (0..source.triangles.len() / 3)
            .map(|face| projected_face_area(&source, face))
            .collect();
        let adjacency = source.adjacency();
        let original_material = SurfaceMaterial::empty(source.vertices.len());
        let bedrock_rates = vec![0.65; source.vertices.len()];
        let run = || {
            let mut mesh = source.clone();
            let mut material = original_material.clone();
            erode_particle_batches(
                &mut mesh,
                &adjacency,
                &mut material,
                &bedrock_rates,
                true,
                0.8,
                IslandOptions::default(),
                &mut ParticleErosionScratch::default(),
            );
            (mesh, material)
        };

        let first = run();
        let second = run();
        assert_eq!(first, second);
        assert!(first.0.vertices.iter().all(|vertex| vertex.is_finite()));
        assert!(first.1.depths().iter().all(|depth| depth.is_finite()));
        assert!(
            first.0.vertices.iter().map(|vertex| vertex.z).sum::<f32>() < initial_height,
            "the synthetic hill should lose net height"
        );
        for (face, &before) in initial_areas.iter().enumerate() {
            let after = projected_face_area(&first.0, face);
            assert!(before * after > 0.0, "face {face} changed orientation");
        }
    }
}
