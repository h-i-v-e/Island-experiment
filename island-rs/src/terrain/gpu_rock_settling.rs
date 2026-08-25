use std::{sync::mpsc, time::Instant};

use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

use super::{Terrain, decorations::RockBody};
use crate::Vec3;

const WORKGROUP_SIZE: u32 = 64;
const GRID_DIMENSION: u32 = 512;
const GRID_CAPACITY: u32 = 32;
const DEFAULT_STEPS: usize = 180;
const DEFAULT_TIME_STEP: f32 = 1.0 / 30.0;
const POSITION_RELAXATION: f32 = 0.92;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct GpuRockState {
    position_radius: [f32; 4],
    velocity_inverse_mass: [f32; 4],
    previous_position: [f32; 4],
    metadata: [u32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct GpuTerrainVertex {
    position: [f32; 4],
    normal: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct GpuRockParams {
    counts: [u32; 4],
    offsets: [u32; 4],
    physics: [f32; 4],
    contact: [f32; 4],
}

struct GpuRockContext {
    device: wgpu::Device,
    queue: wgpu::Queue,
    bind_group_layout: wgpu::BindGroupLayout,
    integrate_pipeline: wgpu::ComputePipeline,
    clear_grid_pipeline: wgpu::ComputePipeline,
    scatter_grid_pipeline: wgpu::ComputePipeline,
    sort_grid_pipeline: wgpu::ComputePipeline,
    solve_pipeline: wgpu::ComputePipeline,
    adapter_name: String,
}

struct GpuRockBuffers {
    state_a: wgpu::Buffer,
    state_b: wgpu::Buffer,
    terrain_vertices: wgpu::Buffer,
    terrain_topology: wgpu::Buffer,
    grid_counts: wgpu::Buffer,
    grid_indices: wgpu::Buffer,
    params: wgpu::Buffer,
    readback: wgpu::Buffer,
    grid_readback: wgpu::Buffer,
}

#[derive(Clone, Copy)]
struct TopologyLayout {
    triangles: u32,
    index_offsets: u32,
    index_faces: u32,
}

pub(super) fn simulate_rock_bodies_gpu(
    terrain: &Terrain,
    bodies: &mut [RockBody],
) -> Result<(), String> {
    if bodies.is_empty() {
        return Ok(());
    }
    let started = Instant::now();
    let steps = positive_environment("MOTU_GPU_ROCK_STEPS", DEFAULT_STEPS);
    let time_step = float_environment("MOTU_GPU_ROCK_TIME_STEP", DEFAULT_TIME_STEP)
        .clamp(1.0 / 240.0, 1.0 / 15.0);
    let context = GpuRockContext::new()?;
    let (states, terrain_vertices, topology, params) = prepare_inputs(terrain, bodies, time_step);
    let buffers = context.create_buffers(&states, &terrain_vertices, &topology, &params);
    context.solve(&buffers, bodies.len(), steps)?;
    let grid_stats = apply_readbacks(&buffers, bodies)?;

    if std::env::var_os("MOTU_GPU_ROCK_STATS").is_some() {
        let supported = bodies.iter().filter(|body| body.supported).count();
        let stable = bodies.iter().filter(|body| body.stable_support).count();
        let state_hash = body_state_hash(bodies);
        eprintln!(
            "gpu-rock-settling adapter={:?} bodies={} supported={} stable={} steps={} maximum_bucket={} overflow_buckets={} state_hash={} elapsed_ms={:.3}",
            context.adapter_name,
            bodies.len(),
            supported,
            stable,
            steps,
            grid_stats.maximum_bucket,
            grid_stats.overflow_buckets,
            state_hash,
            started.elapsed().as_secs_f64() * 1_000.0,
        );
    }
    Ok(())
}

fn prepare_inputs(
    terrain: &Terrain,
    bodies: &[RockBody],
    time_step: f32,
) -> (
    Vec<GpuRockState>,
    Vec<GpuTerrainVertex>,
    Vec<u32>,
    GpuRockParams,
) {
    let states = bodies
        .iter()
        .map(|body| GpuRockState {
            position_radius: vector4(body.centre, body.radius),
            velocity_inverse_mass: vector4(body.velocity, 1.0 / body.radius.powi(3)),
            previous_position: vector4(body.centre, 0.0),
            metadata: [body.appearance_id, 0, 0, 1],
        })
        .collect();
    let terrain_vertices = terrain
        .mesh
        .vertices
        .iter()
        .zip(&terrain.mesh.normals)
        .map(|(&position, &normal)| GpuTerrainVertex {
            position: vector4(position, 0.0),
            normal: vector4(normal, 0.0),
        })
        .collect();
    let (topology, layout) = pack_topology(terrain);
    let params = GpuRockParams {
        counts: [
            as_u32(bodies.len()),
            as_u32(terrain.mesh.triangles.len() / 3),
            as_u32(terrain.triangle_index.dimension),
            GRID_DIMENSION,
        ],
        offsets: [
            GRID_CAPACITY,
            layout.triangles,
            layout.index_offsets,
            layout.index_faces,
        ],
        physics: [
            time_step,
            super::decorations::ROCK_GRAVITY,
            0.998,
            super::decorations::ROCK_CONTACT_DAMPING,
        ],
        contact: [
            super::decorations::ROCK_RESTITUTION,
            POSITION_RELAXATION,
            super::decorations::ROCK_MINIMUM_SETTLED_NORMAL_Z,
            0.0,
        ],
    };
    (states, terrain_vertices, topology, params)
}

fn pack_topology(terrain: &Terrain) -> (Vec<u32>, TopologyLayout) {
    let mut topology = Vec::with_capacity(
        terrain.mesh.triangles.len()
            + terrain.triangle_index.offsets.len()
            + terrain.triangle_index.faces.len(),
    );
    let triangles = 0;
    topology.extend_from_slice(&terrain.mesh.triangles);
    let index_offsets = as_u32(topology.len());
    topology.extend(terrain.triangle_index.offsets.iter().copied().map(as_u32));
    let index_faces = as_u32(topology.len());
    topology.extend_from_slice(&terrain.triangle_index.faces);
    (
        topology,
        TopologyLayout {
            triangles,
            index_offsets,
            index_faces,
        },
    )
}

impl GpuRockContext {
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
            label: Some("motu GPU rock settling device"),
            required_features: wgpu::Features::empty(),
            required_limits: adapter.limits(),
            ..Default::default()
        }))
        .map_err(|error| error.to_string())?;
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("motu GPU rock settling bind group layout"),
            entries: &[
                storage_layout_entry(0, true),
                storage_layout_entry(1, false),
                storage_layout_entry(2, true),
                storage_layout_entry(3, true),
                storage_layout_entry(4, false),
                storage_layout_entry(5, false),
                uniform_layout_entry(6),
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("motu GPU rock settling pipeline layout"),
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("motu GPU rock settling shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("rock_settling.wgsl").into()),
        });
        let integrate_pipeline =
            create_pipeline(&device, &pipeline_layout, &shader, "integrate", "integrate");
        let clear_grid_pipeline = create_pipeline(
            &device,
            &pipeline_layout,
            &shader,
            "clear grid",
            "clear_grid",
        );
        let scatter_grid_pipeline = create_pipeline(
            &device,
            &pipeline_layout,
            &shader,
            "scatter grid",
            "scatter_grid",
        );
        let sort_grid_pipeline =
            create_pipeline(&device, &pipeline_layout, &shader, "sort grid", "sort_grid");
        let solve_pipeline = create_pipeline(
            &device,
            &pipeline_layout,
            &shader,
            "solve",
            "solve_contacts",
        );
        Ok(Self {
            device,
            queue,
            bind_group_layout,
            integrate_pipeline,
            clear_grid_pipeline,
            scatter_grid_pipeline,
            sort_grid_pipeline,
            solve_pipeline,
            adapter_name,
        })
    }

    fn create_buffers(
        &self,
        states: &[GpuRockState],
        terrain_vertices: &[GpuTerrainVertex],
        topology: &[u32],
        params: &GpuRockParams,
    ) -> GpuRockBuffers {
        let state_a = self.create_upload_buffer(
            "motu GPU rock state A",
            bytemuck::cast_slice(states),
            wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        );
        let state_b = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("motu GPU rock state B"),
            size: state_a.size(),
            usage: wgpu::BufferUsages::STORAGE,
            mapped_at_creation: false,
        });
        let terrain_vertices = self.create_upload_buffer(
            "motu GPU rock terrain vertices",
            bytemuck::cast_slice(terrain_vertices),
            wgpu::BufferUsages::STORAGE,
        );
        let terrain_topology = self.create_upload_buffer(
            "motu GPU rock terrain topology",
            bytemuck::cast_slice(topology),
            wgpu::BufferUsages::STORAGE,
        );
        let grid_cells = usize::try_from(GRID_DIMENSION * GRID_DIMENSION)
            .expect("GPU rock grid dimension exceeds usize");
        let grid_counts = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("motu GPU rock grid counts"),
            size: (grid_cells * size_of::<u32>()) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let grid_indices = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("motu GPU rock grid indices"),
            size: (grid_cells * GRID_CAPACITY as usize * size_of::<u32>()) as u64,
            usage: wgpu::BufferUsages::STORAGE,
            mapped_at_creation: false,
        });
        let params = self.create_upload_buffer(
            "motu GPU rock parameters",
            bytemuck::bytes_of(params),
            wgpu::BufferUsages::UNIFORM,
        );
        let readback = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("motu GPU rock state readback"),
            size: state_a.size(),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let grid_readback = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("motu GPU rock grid readback"),
            size: grid_counts.size(),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        GpuRockBuffers {
            state_a,
            state_b,
            terrain_vertices,
            terrain_topology,
            grid_counts,
            grid_indices,
            params,
            readback,
            grid_readback,
        }
    }

    fn solve(
        &self,
        buffers: &GpuRockBuffers,
        body_count: usize,
        steps: usize,
    ) -> Result<(), String> {
        let bind_a_to_b = self.create_bind_group(&buffers.state_a, &buffers.state_b, buffers);
        let bind_b_to_a = self.create_bind_group(&buffers.state_b, &buffers.state_a, buffers);
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("motu GPU rock settling encoder"),
            });
        let body_groups = workgroups(body_count);
        let cell_groups = workgroups((GRID_DIMENSION * GRID_DIMENSION) as usize);
        for _ in 0..steps {
            dispatch(
                &mut encoder,
                "motu GPU rock integrate",
                &self.integrate_pipeline,
                &bind_a_to_b,
                body_groups,
            );
            dispatch(
                &mut encoder,
                "motu GPU rock clear grid",
                &self.clear_grid_pipeline,
                &bind_b_to_a,
                cell_groups,
            );
            dispatch(
                &mut encoder,
                "motu GPU rock scatter grid",
                &self.scatter_grid_pipeline,
                &bind_b_to_a,
                body_groups,
            );
            dispatch(
                &mut encoder,
                "motu GPU rock sort grid",
                &self.sort_grid_pipeline,
                &bind_b_to_a,
                cell_groups,
            );
            dispatch(
                &mut encoder,
                "motu GPU rock solve contacts",
                &self.solve_pipeline,
                &bind_b_to_a,
                body_groups,
            );
        }
        encoder.copy_buffer_to_buffer(
            &buffers.state_a,
            0,
            &buffers.readback,
            0,
            buffers.state_a.size(),
        );
        encoder.copy_buffer_to_buffer(
            &buffers.grid_counts,
            0,
            &buffers.grid_readback,
            0,
            buffers.grid_counts.size(),
        );
        self.queue.submit([encoder.finish()]);
        map_readbacks(&self.device, [&buffers.readback, &buffers.grid_readback])
    }

    fn create_bind_group(
        &self,
        source: &wgpu::Buffer,
        target: &wgpu::Buffer,
        buffers: &GpuRockBuffers,
    ) -> wgpu::BindGroup {
        self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("motu GPU rock settling bind group"),
            layout: &self.bind_group_layout,
            entries: &[
                buffer_entry(0, source),
                buffer_entry(1, target),
                buffer_entry(2, &buffers.terrain_vertices),
                buffer_entry(3, &buffers.terrain_topology),
                buffer_entry(4, &buffers.grid_counts),
                buffer_entry(5, &buffers.grid_indices),
                buffer_entry(6, &buffers.params),
            ],
        })
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

fn create_pipeline(
    device: &wgpu::Device,
    layout: &wgpu::PipelineLayout,
    shader: &wgpu::ShaderModule,
    label: &str,
    entry_point: &str,
) -> wgpu::ComputePipeline {
    device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some(label),
        layout: Some(layout),
        module: shader,
        entry_point: Some(entry_point),
        compilation_options: wgpu::PipelineCompilationOptions::default(),
        cache: None,
    })
}

fn dispatch(
    encoder: &mut wgpu::CommandEncoder,
    label: &str,
    pipeline: &wgpu::ComputePipeline,
    bind_group: &wgpu::BindGroup,
    groups: u32,
) {
    let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
        label: Some(label),
        timestamp_writes: None,
    });
    pass.set_pipeline(pipeline);
    pass.set_bind_group(0, bind_group, &[]);
    pass.dispatch_workgroups(groups, 1, 1);
}

#[derive(Clone, Copy)]
struct GridStats {
    maximum_bucket: u32,
    overflow_buckets: usize,
}

fn apply_readbacks(buffers: &GpuRockBuffers, bodies: &mut [RockBody]) -> Result<GridStats, String> {
    {
        let mapped = buffers.readback.get_mapped_range(..);
        let states: &[GpuRockState] = bytemuck::cast_slice(&mapped);
        for (body, state) in bodies.iter_mut().zip(states) {
            body.centre = Vec3::new(
                state.position_radius[0],
                state.position_radius[1],
                state.position_radius[2],
            );
            body.velocity = Vec3::new(
                state.velocity_inverse_mass[0],
                state.velocity_inverse_mass[1],
                state.velocity_inverse_mass[2],
            );
            body.supported = state.metadata[1] != 0;
            body.stable_support = state.metadata[2] != 0;
            body.sleeping = body.stable_support;
            body.quiet_steps = if body.stable_support { u8::MAX } else { 0 };
        }
    }
    buffers.readback.unmap();
    let grid_stats = {
        let mapped = buffers.grid_readback.get_mapped_range(..);
        let counts: &[u32] = bytemuck::cast_slice(&mapped);
        GridStats {
            maximum_bucket: counts.iter().copied().max().unwrap_or_default(),
            overflow_buckets: counts
                .iter()
                .filter(|&&count| count > GRID_CAPACITY)
                .count(),
        }
    };
    buffers.grid_readback.unmap();
    if bodies
        .iter()
        .any(|body| !body.centre.is_finite() || !body.velocity.is_finite())
    {
        Err("GPU returned non-finite rock state".into())
    } else {
        Ok(grid_stats)
    }
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

fn body_state_hash(bodies: &[RockBody]) -> u64 {
    bodies
        .iter()
        .flat_map(|body| {
            body.centre
                .to_array()
                .into_iter()
                .chain(body.velocity.to_array())
                .map(f32::to_bits)
                .chain([
                    body.radius.to_bits(),
                    body.appearance_id,
                    u32::from(body.supported),
                    u32::from(body.stable_support),
                ])
        })
        .fold(0xcbf2_9ce4_8422_2325, |hash, value| {
            (hash ^ u64::from(value)).wrapping_mul(0x0000_0100_0000_01b3)
        })
}

fn storage_layout_entry(binding: u32, read_only: bool) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Storage { read_only },
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

fn uniform_layout_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

fn buffer_entry(binding: u32, buffer: &wgpu::Buffer) -> wgpu::BindGroupEntry<'_> {
    wgpu::BindGroupEntry {
        binding,
        resource: buffer.as_entire_binding(),
    }
}

fn vector4(vector: Vec3, fourth: f32) -> [f32; 4] {
    [vector.x, vector.y, vector.z, fourth]
}

fn workgroups(items: usize) -> u32 {
    as_u32(items).div_ceil(WORKGROUP_SIZE)
}

fn as_u32(value: usize) -> u32 {
    u32::try_from(value).expect("GPU rock buffer exceeds u32 indexing")
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
        .filter(|value: &f32| value.is_finite() && *value > 0.0)
        .unwrap_or(default)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Mesh, Vec2};

    #[test]
    #[ignore = "requires a native GPU adapter"]
    fn native_gpu_rock_solver_settles_finite_bodies_on_terrain() {
        let points: Vec<Vec2> = (0..=8)
            .flat_map(|y| (0..=8).map(move |x| Vec2::new(x as f32 / 8.0, y as f32 / 8.0)))
            .collect();
        let mut mesh = Mesh::delaunay(&points);
        mesh.vertices.iter_mut().for_each(|vertex| vertex.z = 0.04);
        mesh.calculate_normals();
        let terrain = Terrain::new(mesh);
        let mut bodies = (0..3_u32)
            .map(|appearance_id| RockBody {
                centre: Vec3::new(
                    0.5 + appearance_id as f32 * 0.000_2,
                    0.5,
                    0.055 + appearance_id as f32 * 0.001,
                ),
                velocity: Vec3::ZERO,
                radius: 0.000_25,
                appearance_id,
                supported: false,
                stable_support: false,
                quiet_steps: 0,
                sleeping: false,
            })
            .collect::<Vec<_>>();

        simulate_rock_bodies_gpu(&terrain, &mut bodies).unwrap();

        assert!(bodies.iter().all(|body| body.centre.is_finite()));
        assert!(bodies.iter().all(|body| body.supported));
        assert!(bodies.iter().all(|body| body.centre.z >= 0.04));
    }
}
