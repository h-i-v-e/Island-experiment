use std::{num::NonZeroU64, sync::mpsc, time::Instant};

use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

use super::erosion::{HydraulicErosionSettings, VertexFaceAdjacency};
use super::{Adjacency, IslandOptions, Mesh, SurfaceMaterial, Vec3};

const WORKGROUP_SIZE: u32 = 64;
const PARAM_WORDS: usize = 64;
const PARAM_STRIDE: u64 = 256;
const FIXED_POINT_SCALE: f32 = (1_u32 << 22) as f32;
const DEFAULT_BATCHES: usize = 32;
const DEFAULT_MAX_STEPS: usize = 4_096;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct GpuVertexState {
    position: [f32; 4],
    normal: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct GpuMaterialState {
    loose_depth: f32,
    bedrock_rate: f32,
    padding: [f32; 2],
}

#[derive(Default)]
pub(super) struct GpuParticleErosionScratch {
    context: Option<GpuContext>,
    topology: Vec<u32>,
    order: Vec<usize>,
    params: Vec<u32>,
    vertex_upload: Vec<GpuVertexState>,
    material_upload: Vec<GpuMaterialState>,
}

struct GpuContext {
    device: wgpu::Device,
    queue: wgpu::Queue,
    bind_group_layout: wgpu::BindGroupLayout,
    pipeline: wgpu::ComputePipeline,
    adapter_name: String,
}

struct GpuStageBuffers {
    state: wgpu::Buffer,
    material: wgpu::Buffer,
    topology: wgpu::Buffer,
    accumulators: wgpu::Buffer,
    params: wgpu::Buffer,
    state_readback: wgpu::Buffer,
    material_readback: wgpu::Buffer,
}

#[derive(Clone, Copy)]
struct TopologyLayout {
    adjacency_offsets: u32,
    neighbours: u32,
    triangles: u32,
    face_offsets: u32,
    faces: u32,
    order: u32,
}

#[derive(Clone, Copy)]
struct PrototypeSettings {
    batches: usize,
    maximum_steps: usize,
    route_jitter: f32,
    batch_shift_ratio: f32,
    hydraulic: HydraulicErosionSettings,
}

impl PrototypeSettings {
    fn new(stage_strength: f32, options: IslandOptions) -> Self {
        Self {
            batches: positive_environment("MOTU_GPU_PARTICLE_EROSION_BATCHES", DEFAULT_BATCHES),
            maximum_steps: positive_environment(
                "MOTU_GPU_PARTICLE_EROSION_MAX_STEPS",
                DEFAULT_MAX_STEPS,
            ),
            route_jitter: float_environment("MOTU_GPU_PARTICLE_EROSION_ROUTE_JITTER", 0.18)
                .clamp(0.0, 0.8),
            batch_shift_ratio: float_environment("MOTU_GPU_PARTICLE_EROSION_BATCH_SHIFT", 0.045)
                .clamp(0.001, 0.08),
            hydraulic: HydraulicErosionSettings::new(stage_strength, options),
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn erode_particle_batches_gpu(
    mesh: &mut Mesh,
    adjacency: &Adjacency,
    material: &mut SurfaceMaterial,
    bedrock_rates: &[f32],
    stage_strength: f32,
    options: IslandOptions,
    scratch: &mut GpuParticleErosionScratch,
) {
    if mesh.vertices.is_empty() {
        return;
    }
    let prototype = PrototypeSettings::new(stage_strength, options);
    if prototype.hydraulic.erosion_strength == 0.0 {
        return;
    }
    let started = Instant::now();
    if scratch.context.is_none() {
        scratch.context = Some(
            GpuContext::new()
                .unwrap_or_else(|error| panic!("failed to initialize GPU erosion: {error}")),
        );
    }
    scratch.prepare(mesh, adjacency, material, bedrock_rates, prototype);
    let context = scratch
        .context
        .as_ref()
        .expect("GPU context was initialized");
    context
        .erode(mesh, material, prototype, scratch)
        .unwrap_or_else(|error| panic!("GPU particle erosion failed: {error}"));
    if std::env::var_os("MOTU_GPU_EROSION_STATS").is_some() {
        eprintln!(
            "gpu-particle-erosion adapter={:?} vertices={} batches={} elapsed_ms={:.3}",
            context.adapter_name,
            mesh.vertices.len(),
            prototype.batches,
            started.elapsed().as_secs_f64() * 1_000.0,
        );
    }
}

impl GpuParticleErosionScratch {
    fn prepare(
        &mut self,
        mesh: &Mesh,
        adjacency: &Adjacency,
        material: &SurfaceMaterial,
        bedrock_rates: &[f32],
        prototype: PrototypeSettings,
    ) {
        self.order.clear();
        self.order.extend(0..mesh.vertices.len());
        self.order.sort_unstable_by(|&left, &right| {
            mesh.vertices[right].z.total_cmp(&mesh.vertices[left].z)
        });
        let land_sources = self
            .order
            .partition_point(|&source| mesh.vertices[source].z > 0.0);
        let vertex_faces = VertexFaceAdjacency::new(mesh);
        let topology_layout = self.pack_topology(mesh, adjacency, &vertex_faces);

        self.vertex_upload.clear();
        self.vertex_upload
            .extend(
                mesh.vertices
                    .iter()
                    .zip(&mesh.normals)
                    .map(|(&position, &normal)| GpuVertexState {
                        position: vector4(position),
                        normal: vector4(normal),
                    }),
            );
        self.material_upload.clear();
        self.material_upload
            .extend(material.depths().iter().zip(bedrock_rates).map(
                |(&loose_depth, &bedrock_rate)| GpuMaterialState {
                    loose_depth,
                    bedrock_rate,
                    padding: [0.0; 2],
                },
            ));

        let batch_count = prototype.batches.min(land_sources.max(1));
        let global_shift = mesh
            .vertices
            .iter()
            .map(|vertex| vertex.z)
            .fold(0.0_f32, f32::max)
            * 0.012;
        self.params.clear();
        self.params.reserve(batch_count * 4 * PARAM_WORDS);
        for batch in 0..batch_count {
            for mode in 0..4_u32 {
                let mut record = [0_u32; PARAM_WORDS];
                record[0] = mode;
                record[1] = as_u32(mesh.vertices.len());
                record[2] = as_u32(land_sources);
                record[3] = as_u32(batch_count);
                record[4] = as_u32(batch);
                record[5] = topology_layout.adjacency_offsets;
                record[6] = topology_layout.neighbours;
                record[7] = topology_layout.triangles;
                record[8] = topology_layout.face_offsets;
                record[9] = topology_layout.faces;
                record[10] = topology_layout.order;
                record[11] = as_u32(mesh.triangles.len() / 3);
                record[12] = as_u32(prototype.maximum_steps.min(mesh.vertices.len()));
                record[13] = FIXED_POINT_SCALE.to_bits();
                record[14] = global_shift.to_bits();
                record[15] = prototype.hydraulic.erosion_strength.to_bits();
                record[16] = prototype.hydraulic.deposition_strength.to_bits();
                record[17] = prototype.hydraulic.full_deposition_slope.to_bits();
                record[18] = prototype.hydraulic.maximum_deposition_slope.to_bits();
                record[19] = prototype.route_jitter.to_bits();
                record[20] = prototype.batch_shift_ratio.to_bits();
                self.params.extend(record);
            }
        }
    }

    fn pack_topology(
        &mut self,
        mesh: &Mesh,
        adjacency: &Adjacency,
        vertex_faces: &VertexFaceAdjacency,
    ) -> TopologyLayout {
        self.topology.clear();
        let adjacency_offsets = as_u32(self.topology.len());
        self.topology.push(0);
        let mut offset = 0_u32;
        for neighbours in adjacency.iter() {
            offset += as_u32(neighbours.len());
            self.topology.push(offset);
        }
        let neighbours = as_u32(self.topology.len());
        self.topology
            .extend(adjacency.iter().flatten().copied().map(as_u32));
        let triangles = as_u32(self.topology.len());
        self.topology.extend(mesh.triangles.iter().copied());
        let face_offsets = as_u32(self.topology.len());
        self.topology
            .extend(vertex_faces.offsets.iter().copied().map(as_u32));
        let faces = as_u32(self.topology.len());
        self.topology
            .extend(vertex_faces.faces.iter().copied().map(as_u32));
        let order = as_u32(self.topology.len());
        self.topology.extend(self.order.iter().copied().map(as_u32));
        TopologyLayout {
            adjacency_offsets,
            neighbours,
            triangles,
            face_offsets,
            faces,
            order,
        }
    }
}

impl GpuContext {
    fn new() -> Result<Self, String> {
        let instance = wgpu::Instance::default();
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: None,
            force_fallback_adapter: false,
        }))
        .map_err(|error| error.to_string())?;
        let adapter_name = adapter.get_info().name;
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("motu GPU particle erosion device"),
            required_features: wgpu::Features::empty(),
            required_limits: adapter.limits(),
            ..Default::default()
        }))
        .map_err(|error| error.to_string())?;
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("motu GPU particle erosion bind group layout"),
            entries: &[
                storage_layout_entry(0, false, false),
                storage_layout_entry(1, false, false),
                storage_layout_entry(2, true, false),
                storage_layout_entry(3, false, false),
                storage_layout_entry(4, true, true),
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("motu GPU particle erosion pipeline layout"),
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("motu GPU particle erosion shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("particle_erosion.wgsl").into()),
        });
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("motu GPU particle erosion pipeline"),
            layout: Some(&pipeline_layout),
            module: &shader,
            entry_point: Some("main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            cache: None,
        });
        Ok(Self {
            device,
            queue,
            bind_group_layout,
            pipeline,
            adapter_name,
        })
    }

    fn erode(
        &self,
        mesh: &mut Mesh,
        material: &mut SurfaceMaterial,
        prototype: PrototypeSettings,
        scratch: &GpuParticleErosionScratch,
    ) -> Result<(), String> {
        let buffers = self.create_stage_buffers(mesh.vertices.len(), scratch);
        let bind_group = self.create_stage_bind_group(&buffers);
        let command = self.encode_stage(mesh, prototype, scratch, &buffers, &bind_group);
        self.queue.submit([command]);
        map_readbacks(
            &self.device,
            [&buffers.state_readback, &buffers.material_readback],
        )?;
        apply_readbacks(mesh, material, &buffers)?;
        mesh.calculate_normals();
        Ok(())
    }

    fn create_stage_buffers(
        &self,
        vertex_count: usize,
        scratch: &GpuParticleErosionScratch,
    ) -> GpuStageBuffers {
        let state = self.create_upload_buffer(
            "motu GPU erosion vertex state",
            bytemuck::cast_slice(&scratch.vertex_upload),
            wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        );
        let material = self.create_upload_buffer(
            "motu GPU erosion material state",
            bytemuck::cast_slice(&scratch.material_upload),
            wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        );
        let topology = self.create_upload_buffer(
            "motu GPU erosion topology",
            bytemuck::cast_slice(&scratch.topology),
            wgpu::BufferUsages::STORAGE,
        );
        let accumulators = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("motu GPU erosion accumulators"),
            size: (vertex_count * 16) as u64,
            usage: wgpu::BufferUsages::STORAGE,
            mapped_at_creation: false,
        });
        let params = self.create_upload_buffer(
            "motu GPU erosion parameters",
            bytemuck::cast_slice(&scratch.params),
            wgpu::BufferUsages::STORAGE,
        );
        let state_readback = readback_buffer(
            &self.device,
            "motu GPU erosion state readback",
            state.size(),
        );
        let material_readback = readback_buffer(
            &self.device,
            "motu GPU erosion material readback",
            material.size(),
        );
        GpuStageBuffers {
            state,
            material,
            topology,
            accumulators,
            params,
            state_readback,
            material_readback,
        }
    }

    fn create_stage_bind_group(&self, buffers: &GpuStageBuffers) -> wgpu::BindGroup {
        self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("motu GPU particle erosion bind group"),
            layout: &self.bind_group_layout,
            entries: &[
                buffer_entry(0, &buffers.state, None),
                buffer_entry(1, &buffers.material, None),
                buffer_entry(2, &buffers.topology, None),
                buffer_entry(3, &buffers.accumulators, None),
                buffer_entry(4, &buffers.params, NonZeroU64::new(PARAM_STRIDE)),
            ],
        })
    }

    fn encode_stage(
        &self,
        mesh: &Mesh,
        prototype: PrototypeSettings,
        scratch: &GpuParticleErosionScratch,
        buffers: &GpuStageBuffers,
        bind_group: &wgpu::BindGroup,
    ) -> wgpu::CommandBuffer {
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("motu GPU particle erosion encoder"),
            });
        let vertex_groups = workgroups(mesh.vertices.len());
        let land_sources = scratch
            .order
            .partition_point(|&source| mesh.vertices[source].z > 0.0);
        let batch_count = prototype.batches.min(land_sources.max(1));
        let particle_groups = workgroups(land_sources.div_ceil(batch_count));
        let mut param_index = 0_u32;
        for _batch in 0..batch_count {
            for (label, groups) in [
                ("motu GPU erosion clear", vertex_groups),
                ("motu GPU erosion normals", vertex_groups),
                ("motu GPU erosion particles", particle_groups),
                ("motu GPU erosion apply", vertex_groups),
            ] {
                let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                    label: Some(label),
                    timestamp_writes: None,
                });
                pass.set_pipeline(&self.pipeline);
                pass.set_bind_group(0, bind_group, &[param_index * PARAM_STRIDE as u32]);
                pass.dispatch_workgroups(groups, 1, 1);
                param_index += 1;
            }
        }
        encoder.copy_buffer_to_buffer(
            &buffers.state,
            0,
            &buffers.state_readback,
            0,
            buffers.state.size(),
        );
        encoder.copy_buffer_to_buffer(
            &buffers.material,
            0,
            &buffers.material_readback,
            0,
            buffers.material.size(),
        );
        encoder.finish()
    }

    fn create_upload_buffer(
        &self,
        label: &str,
        contents: &[u8],
        usage: wgpu::BufferUsages,
    ) -> wgpu::Buffer {
        self.device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some(label),
                contents,
                usage,
            })
    }
}

fn storage_layout_entry(
    binding: u32,
    read_only: bool,
    dynamic: bool,
) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Storage { read_only },
            has_dynamic_offset: dynamic,
            min_binding_size: None,
        },
        count: None,
    }
}

fn buffer_entry(
    binding: u32,
    buffer: &wgpu::Buffer,
    size: Option<NonZeroU64>,
) -> wgpu::BindGroupEntry<'_> {
    wgpu::BindGroupEntry {
        binding,
        resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
            buffer,
            offset: 0,
            size,
        }),
    }
}

fn apply_readbacks(
    mesh: &mut Mesh,
    material: &mut SurfaceMaterial,
    buffers: &GpuStageBuffers,
) -> Result<(), String> {
    {
        let mapped = buffers.state_readback.get_mapped_range(..);
        let states: &[GpuVertexState] = bytemuck::cast_slice(&mapped);
        mesh.vertices
            .iter_mut()
            .zip(states)
            .for_each(|(vertex, state)| {
                *vertex = Vec3::new(state.position[0], state.position[1], state.position[2]);
            });
    }
    buffers.state_readback.unmap();
    {
        let mapped = buffers.material_readback.get_mapped_range(..);
        let materials: &[GpuMaterialState] = bytemuck::cast_slice(&mapped);
        material
            .depths_mut()
            .iter_mut()
            .zip(materials)
            .for_each(|(depth, state)| *depth = state.loose_depth.max(0.0));
    }
    buffers.material_readback.unmap();
    if mesh.vertices.iter().any(|vertex| !vertex.is_finite())
        || material.depths().iter().any(|depth| !depth.is_finite())
    {
        Err("GPU returned non-finite terrain data".into())
    } else {
        Ok(())
    }
}

fn readback_buffer(device: &wgpu::Device, label: &str, size: u64) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    })
}

fn map_readbacks(device: &wgpu::Device, buffers: [&wgpu::Buffer; 2]) -> Result<(), String> {
    let (sender, receiver) = mpsc::channel();
    for buffer in buffers {
        let sender = sender.clone();
        buffer.map_async(wgpu::MapMode::Read, .., move |result| {
            let _ = sender.send(result.map_err(|error| error.to_string()));
        });
    }
    drop(sender);
    device
        .poll(wgpu::PollType::wait_indefinitely())
        .map_err(|error| error.to_string())?;
    for result in receiver {
        result?;
    }
    Ok(())
}

fn vector4(vector: Vec3) -> [f32; 4] {
    [vector.x, vector.y, vector.z, 0.0]
}

fn workgroups(items: usize) -> u32 {
    as_u32(items).div_ceil(WORKGROUP_SIZE)
}

fn as_u32(value: usize) -> u32 {
    u32::try_from(value).expect("GPU terrain buffer exceeds u32 indexing")
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

    #[test]
    #[ignore = "requires a native GPU adapter"]
    fn native_gpu_particle_erosion_returns_finite_changed_terrain() {
        let points: Vec<Vec2> = (0..=6)
            .flat_map(|y| (0..=6).map(move |x| Vec2::new(x as f32 / 6.0, y as f32 / 6.0)))
            .collect();
        let mut mesh = Mesh::delaunay(&points);
        mesh.vertices.iter_mut().for_each(|vertex| {
            vertex.z = 0.04 + 0.11 * vertex.y + 0.025 * (vertex.x * 13.0).sin();
        });
        mesh.calculate_normals();
        let before = mesh.vertices.clone();
        let adjacency = mesh.adjacency();
        let mut material = SurfaceMaterial::empty(mesh.vertices.len());
        let bedrock_rates = vec![0.65; mesh.vertices.len()];

        erode_particle_batches_gpu(
            &mut mesh,
            &adjacency,
            &mut material,
            &bedrock_rates,
            0.8,
            IslandOptions::default(),
            &mut GpuParticleErosionScratch::default(),
        );

        assert_ne!(mesh.vertices, before);
        assert!(mesh.vertices.iter().all(|vertex| vertex.is_finite()));
        assert!(material.depths().iter().all(|depth| depth.is_finite()));
    }
}
