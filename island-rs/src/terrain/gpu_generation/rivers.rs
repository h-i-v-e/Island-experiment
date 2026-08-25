//! Experimental GPU-native river generation.
//!
//! The solver deliberately does not reproduce the CPU river graph. It builds a
//! depression-free spill field on a regular grid, routes one deterministic
//! rainfall packet from every land cell, then derives channel width, depth and
//! a carved terrain in fixed-size compute passes. CPU work is limited to input
//! rasterization and conversion of the final buffers into the public mesh and
//! river-path representation.

use std::{mem::size_of, sync::mpsc, time::Instant};

use bytemuck::{Pod, Zeroable};
use rayon::prelude::*;
use wgpu::util::DeviceExt;

use super::super::{Mesh, SurfaceMaterial, TriangleIndex, Vec2, Vec3, sample_mesh_surface};
use crate::{
    ISLAND_WORLD_METRES, River, RiverNode,
    rivers::{RiverChannelSettings, RiverSourceRule},
};

use super::super::generation::FinalRiverGeneration;

const WORKGROUP_SIZE: u32 = 64;
const DEFAULT_GRID_DIMENSION: usize = 512;
const DEFAULT_CATCHMENT_MULTIPLIER: f32 = 12.0;
const MAX_RIVERS: usize = 96;
const SOURCE_EXCLUSION_METRES: f32 = 25.0;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct GpuRiverParams {
    grid: [u32; 4],
    routing: [u32; 4],
    channel: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct GpuTerrainVertex {
    position: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct GpuRiverVertex {
    position_surface: [f32; 4],
    attributes: [f32; 4],
}

struct GpuRiverContext {
    device: wgpu::Device,
    queue: wgpu::Queue,
    bind_group_layout: wgpu::BindGroupLayout,
    initialize_pipeline: wgpu::ComputePipeline,
    relax_pipeline: wgpu::ComputePipeline,
    route_pipeline: wgpu::ComputePipeline,
    rain_pipeline: wgpu::ComputePipeline,
    field_pipeline: wgpu::ComputePipeline,
    carve_pipeline: wgpu::ComputePipeline,
    adapter_name: String,
}

struct GpuRiverBuffers {
    heights: wgpu::Buffer,
    spill_a: wgpu::Buffer,
    spill_b: wgpu::Buffer,
    flow: wgpu::Buffer,
    downstream: wgpu::Buffer,
    field: wgpu::Buffer,
    vertices: wgpu::Buffer,
    carved: wgpu::Buffer,
    params: wgpu::Buffer,
    flow_readback: wgpu::Buffer,
    downstream_readback: wgpu::Buffer,
    field_readback: wgpu::Buffer,
    carved_readback: wgpu::Buffer,
}

struct GpuRiverReadback {
    flow: Vec<u32>,
    downstream: Vec<u32>,
    field: Vec<[f32; 4]>,
    vertices: Vec<GpuRiverVertex>,
}

pub(in crate::terrain) fn generate_gpu_rivers(
    seed: u64,
    mut lod0: Mesh,
    mut material: SurfaceMaterial,
    source_rule: RiverSourceRule,
    channel_settings: RiverChannelSettings,
) -> Result<FinalRiverGeneration, String> {
    let started = Instant::now();
    let dimension = grid_dimension();
    let index = TriangleIndex::new(&lod0);
    let heights = rasterize_heights(&lod0, &index, dimension);
    let cell_metres = ISLAND_WORLD_METRES / dimension as f32;
    let cell_area = cell_metres * cell_metres;
    // The legacy source rule was calibrated on irregular vertices and then
    // aggressively pruned while constructing its graph. A fixed rainfall
    // field represents every tributary, so use a larger visual catchment to
    // retain the same broad drainage scale without the sequential pruning.
    let source_flow = (source_rule.required_catchment(0.0, 1.0)
        * positive_float_environment(
            "MOTU_GPU_RIVER_CATCHMENT_MULTIPLIER",
            DEFAULT_CATCHMENT_MULTIPLIER,
        )
        / cell_area)
        .ceil()
        .max(8.0) as u32;
    let maximum_flow = ((dimension * dimension) as u32 / 10).max(source_flow + 1);
    let relax_passes = dimension / 2 + 8;
    let params = GpuRiverParams {
        grid: [
            as_u32(dimension),
            as_u32(dimension * dimension),
            as_u32(lod0.vertices.len()),
            as_u32(dimension * 2),
        ],
        routing: [
            source_flow,
            maximum_flow,
            seed as u32 ^ (seed >> 32) as u32,
            as_u32(relax_passes),
        ],
        channel: [
            channel_settings.source_width,
            channel_settings.maximum_width,
            channel_settings.source_depth,
            channel_settings.maximum_depth,
        ],
    };
    let vertices = lod0
        .vertices
        .iter()
        .map(|position| GpuTerrainVertex {
            position: [position.x, position.y, position.z, 0.0],
        })
        .collect::<Vec<_>>();

    let context = GpuRiverContext::new()?;
    let buffers = context.create_buffers(&heights, &vertices, &params);
    context.solve(&buffers, dimension, lod0.vertices.len(), relax_passes)?;
    let result = read_results(&buffers)?;

    let river_bed = apply_carve(&mut lod0, &mut material, &result.vertices);
    let rivers = build_river_paths(&lod0, &index, dimension, source_flow, &heights, &result);
    let river_mesh = build_river_mesh(&lod0, &river_bed, &result.vertices);

    if std::env::var_os("MOTU_GPU_RIVER_STATS").is_some() {
        let wet_cells = result
            .flow
            .iter()
            .filter(|&&flow| flow >= source_flow)
            .count();
        let bed_vertices = river_bed.iter().filter(|&&bed| bed).count();
        eprintln!(
            "gpu-rivers adapter={:?} grid={} relax_passes={} source_flow={} wet_cells={} rivers={} bed_vertices={} water_triangles={} elapsed_ms={:.3}",
            context.adapter_name,
            dimension,
            relax_passes,
            source_flow,
            wet_cells,
            rivers.len(),
            bed_vertices,
            river_mesh.triangles.len() / 3,
            started.elapsed().as_secs_f64() * 1_000.0,
        );
    }

    Ok(FinalRiverGeneration {
        lod0,
        material,
        rivers,
        river_mesh,
        river_bed,
        river_rock_mesh: Mesh::default(),
    })
}

fn rasterize_heights(mesh: &Mesh, index: &TriangleIndex, dimension: usize) -> Vec<f32> {
    (0..dimension * dimension)
        .into_par_iter()
        .map(|cell| {
            let x = cell % dimension;
            let y = cell / dimension;
            let point = Vec2::new(
                (x as f32 + 0.5) / dimension as f32,
                (y as f32 + 0.5) / dimension as f32,
            );
            sample_mesh_surface(mesh, index, point.x, point.y).0
        })
        .collect()
}

fn apply_carve(
    mesh: &mut Mesh,
    material: &mut SurfaceMaterial,
    output: &[GpuRiverVertex],
) -> Vec<bool> {
    debug_assert_eq!(mesh.vertices.len(), output.len());
    let mut river_bed = Vec::with_capacity(output.len());
    for (vertex, gpu) in mesh.vertices.iter_mut().zip(output) {
        vertex.z = gpu.position_surface[2];
        river_bed.push(gpu.attributes[0] >= 0.45);
    }
    material
        .depths_mut()
        .iter_mut()
        .zip(&river_bed)
        .filter(|(_, bed)| **bed)
        .for_each(|(depth, _)| *depth = 0.0);
    mesh.calculate_normals();
    river_bed
}

fn build_river_mesh(terrain: &Mesh, river_bed: &[bool], output: &[GpuRiverVertex]) -> Mesh {
    let mut mapping = vec![u32::MAX; terrain.vertices.len()];
    let mut river = Mesh::default();
    for (index, (vertex, gpu)) in terrain.vertices.iter().zip(output).enumerate() {
        let coverage = gpu.attributes[0];
        let surface = gpu.position_surface[3];
        if coverage <= 0.02 || surface <= 0.0 || !surface.is_finite() {
            continue;
        }
        mapping[index] = river.vertices.len() as u32;
        river.vertices.push(Vec3::new(vertex.x, vertex.y, surface));
        river.uv.push(Vec2::new(0.0, gpu.attributes[1]));
    }
    for triangle in terrain.triangles.as_chunks::<3>().0 {
        let mapped = [
            mapping[triangle[0] as usize],
            mapping[triangle[1] as usize],
            mapping[triangle[2] as usize],
        ];
        if mapped.iter().all(|&vertex| vertex != u32::MAX)
            && triangle.iter().any(|&vertex| river_bed[vertex as usize])
        {
            river.triangles.extend(mapped);
        }
    }
    river.calculate_normals();
    river
}

fn build_river_paths(
    terrain: &Mesh,
    index: &TriangleIndex,
    dimension: usize,
    source_flow: u32,
    heights: &[f32],
    result: &GpuRiverReadback,
) -> Vec<River> {
    let mut candidates = (0..heights.len())
        .filter(|&cell| {
            heights[cell] > 0.0
                && result.flow[cell] >= source_flow
                && !neighbours(cell, dimension).any(|upstream| {
                    result.downstream[upstream] as usize == cell
                        && result.flow[upstream] >= source_flow
                })
        })
        .collect::<Vec<_>>();
    candidates.sort_unstable_by(|&a, &b| {
        heights[b]
            .total_cmp(&heights[a])
            .then_with(|| result.flow[b].cmp(&result.flow[a]))
            .then_with(|| a.cmp(&b))
    });

    let exclusion_cells = (SOURCE_EXCLUSION_METRES * dimension as f32 / ISLAND_WORLD_METRES)
        .ceil()
        .max(1.0) as isize;
    let mut selected = Vec::<usize>::new();
    let mut owner = vec![None::<usize>; heights.len()];
    let mut rivers = Vec::new();

    for source in candidates {
        if rivers.len() >= MAX_RIVERS
            || selected
                .iter()
                .any(|&other| grid_distance(source, other, dimension) < exclusion_cells)
        {
            continue;
        }
        selected.push(source);
        let river_index = rivers.len();
        let mut nodes = Vec::new();
        let mut visited_cells = Vec::new();
        let mut join = None;
        let mut cell = source;
        for step in 0..dimension * 2 {
            if heights[cell] <= 0.0 {
                break;
            }
            if let Some(existing) = owner[cell] {
                join = Some(existing);
                break;
            }
            visited_cells.push(cell);
            if step % 2 == 0 {
                let point = cell_point(cell, dimension);
                let vertex = index.nearest_vertex(terrain, point);
                if nodes
                    .last()
                    .is_none_or(|node: &RiverNode| node.vertex != vertex)
                {
                    let surface = result.field[cell][3];
                    let mut position = terrain.vertices[vertex];
                    position.z = surface;
                    nodes.push(RiverNode {
                        vertex,
                        flow: result.flow[cell],
                        surface,
                        position,
                    });
                }
            }
            let next = result.downstream[cell] as usize;
            if next == cell || next >= heights.len() {
                break;
            }
            cell = next;
        }
        if nodes.len() >= 2 {
            for cell in visited_cells {
                owner[cell] = Some(river_index);
            }
            rivers.push(River { nodes, join });
        }
    }
    rivers
}

fn neighbours(cell: usize, dimension: usize) -> impl Iterator<Item = usize> {
    let x = cell % dimension;
    let y = cell / dimension;
    let min_x = x.saturating_sub(1);
    let max_x = (x + 1).min(dimension - 1);
    let min_y = y.saturating_sub(1);
    let max_y = (y + 1).min(dimension - 1);
    (min_y..=max_y).flat_map(move |ny| {
        (min_x..=max_x)
            .filter(move |&nx| nx != x || ny != y)
            .map(move |nx| ny * dimension + nx)
    })
}

fn cell_point(cell: usize, dimension: usize) -> Vec2 {
    Vec2::new(
        (cell % dimension) as f32 / dimension as f32 + 0.5 / dimension as f32,
        (cell / dimension) as f32 / dimension as f32 + 0.5 / dimension as f32,
    )
}

fn grid_distance(a: usize, b: usize, dimension: usize) -> isize {
    let ax = (a % dimension) as isize;
    let ay = (a / dimension) as isize;
    let bx = (b % dimension) as isize;
    let by = (b / dimension) as isize;
    (ax - bx).abs().max((ay - by).abs())
}

impl GpuRiverContext {
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
            label: Some("motu GPU river device"),
            required_features: wgpu::Features::empty(),
            required_limits: adapter.limits(),
            ..Default::default()
        }))
        .map_err(|error| error.to_string())?;
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("motu GPU river bind group layout"),
            entries: &[
                storage_layout_entry(0, true),
                storage_layout_entry(1, true),
                storage_layout_entry(2, false),
                storage_layout_entry(3, false),
                storage_layout_entry(4, false),
                storage_layout_entry(5, false),
                storage_layout_entry(6, true),
                storage_layout_entry(7, false),
                uniform_layout_entry(8),
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("motu GPU river pipeline layout"),
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("motu GPU river shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("rivers.wgsl").into()),
        });
        Ok(Self {
            initialize_pipeline: create_pipeline(
                &device,
                &pipeline_layout,
                &shader,
                "initialize spill field",
                "initialize_spill",
            ),
            relax_pipeline: create_pipeline(
                &device,
                &pipeline_layout,
                &shader,
                "relax spill field",
                "relax_spill",
            ),
            route_pipeline: create_pipeline(
                &device,
                &pipeline_layout,
                &shader,
                "map drainage",
                "map_drainage",
            ),
            rain_pipeline: create_pipeline(
                &device,
                &pipeline_layout,
                &shader,
                "accumulate rain",
                "accumulate_rain",
            ),
            field_pipeline: create_pipeline(
                &device,
                &pipeline_layout,
                &shader,
                "derive river field",
                "derive_field",
            ),
            carve_pipeline: create_pipeline(
                &device,
                &pipeline_layout,
                &shader,
                "carve terrain",
                "carve_vertices",
            ),
            device,
            queue,
            bind_group_layout,
            adapter_name,
        })
    }

    fn create_buffers(
        &self,
        heights: &[f32],
        vertices: &[GpuTerrainVertex],
        params: &GpuRiverParams,
    ) -> GpuRiverBuffers {
        let heights = self.upload(
            "motu GPU river heights",
            bytemuck::cast_slice(heights),
            wgpu::BufferUsages::STORAGE,
        );
        let spill_size = heights.size();
        let spill_a = self.storage("motu GPU river spill A", spill_size);
        let spill_b = self.storage("motu GPU river spill B", spill_size);
        let flow = self.storage_copy("motu GPU river flow", spill_size);
        let downstream = self.storage_copy("motu GPU river downstream", spill_size);
        let field = self.storage_copy("motu GPU river field", spill_size * 4);
        let vertices = self.upload(
            "motu GPU river vertices",
            bytemuck::cast_slice(vertices),
            wgpu::BufferUsages::STORAGE,
        );
        let carved = self.storage_copy("motu GPU river carved vertices", vertices.size() * 2);
        let params = self.upload(
            "motu GPU river parameters",
            bytemuck::bytes_of(params),
            wgpu::BufferUsages::UNIFORM,
        );
        GpuRiverBuffers {
            flow_readback: self.readback("motu GPU river flow readback", flow.size()),
            downstream_readback: self
                .readback("motu GPU river downstream readback", downstream.size()),
            field_readback: self.readback("motu GPU river field readback", field.size()),
            carved_readback: self.readback("motu GPU river carve readback", carved.size()),
            heights,
            spill_a,
            spill_b,
            flow,
            downstream,
            field,
            vertices,
            carved,
            params,
        }
    }

    fn solve(
        &self,
        buffers: &GpuRiverBuffers,
        dimension: usize,
        vertex_count: usize,
        relax_passes: usize,
    ) -> Result<(), String> {
        let a_to_b = self.bind_group(&buffers.spill_a, &buffers.spill_b, buffers);
        let b_to_a = self.bind_group(&buffers.spill_b, &buffers.spill_a, buffers);
        let cell_groups = workgroups(dimension * dimension);
        let vertex_groups = workgroups(vertex_count);
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("motu GPU river encoder"),
            });
        dispatch(
            &mut encoder,
            &self.initialize_pipeline,
            &a_to_b,
            cell_groups,
        );
        for pass in 0..relax_passes {
            let bind = if pass % 2 == 0 { &b_to_a } else { &a_to_b };
            dispatch(&mut encoder, &self.relax_pipeline, bind, cell_groups);
        }
        let final_bind = if relax_passes.is_multiple_of(2) {
            &b_to_a
        } else {
            &a_to_b
        };
        dispatch(&mut encoder, &self.route_pipeline, final_bind, cell_groups);
        dispatch(&mut encoder, &self.rain_pipeline, final_bind, cell_groups);
        dispatch(&mut encoder, &self.field_pipeline, final_bind, cell_groups);
        dispatch(
            &mut encoder,
            &self.carve_pipeline,
            final_bind,
            vertex_groups,
        );
        encoder.copy_buffer_to_buffer(
            &buffers.flow,
            0,
            &buffers.flow_readback,
            0,
            buffers.flow.size(),
        );
        encoder.copy_buffer_to_buffer(
            &buffers.downstream,
            0,
            &buffers.downstream_readback,
            0,
            buffers.downstream.size(),
        );
        encoder.copy_buffer_to_buffer(
            &buffers.field,
            0,
            &buffers.field_readback,
            0,
            buffers.field.size(),
        );
        encoder.copy_buffer_to_buffer(
            &buffers.carved,
            0,
            &buffers.carved_readback,
            0,
            buffers.carved.size(),
        );
        self.queue.submit([encoder.finish()]);
        map_readbacks(
            &self.device,
            [
                &buffers.flow_readback,
                &buffers.downstream_readback,
                &buffers.field_readback,
                &buffers.carved_readback,
            ],
        )
    }

    fn bind_group(
        &self,
        spill_source: &wgpu::Buffer,
        spill_target: &wgpu::Buffer,
        buffers: &GpuRiverBuffers,
    ) -> wgpu::BindGroup {
        self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("motu GPU river bind group"),
            layout: &self.bind_group_layout,
            entries: &[
                buffer_entry(0, &buffers.heights),
                buffer_entry(1, spill_source),
                buffer_entry(2, spill_target),
                buffer_entry(3, &buffers.flow),
                buffer_entry(4, &buffers.downstream),
                buffer_entry(5, &buffers.field),
                buffer_entry(6, &buffers.vertices),
                buffer_entry(7, &buffers.carved),
                buffer_entry(8, &buffers.params),
            ],
        })
    }

    fn upload(&self, label: &str, contents: &[u8], usage: wgpu::BufferUsages) -> wgpu::Buffer {
        self.device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some(label),
                contents,
                usage,
            })
    }

    fn storage(&self, label: &str, size: u64) -> wgpu::Buffer {
        self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(label),
            size,
            usage: wgpu::BufferUsages::STORAGE,
            mapped_at_creation: false,
        })
    }

    fn storage_copy(&self, label: &str, size: u64) -> wgpu::Buffer {
        self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(label),
            size,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        })
    }

    fn readback(&self, label: &str, size: u64) -> wgpu::Buffer {
        self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(label),
            size,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        })
    }
}

fn read_results(buffers: &GpuRiverBuffers) -> Result<GpuRiverReadback, String> {
    let flow = mapped_vec::<u32>(&buffers.flow_readback);
    let downstream = mapped_vec::<u32>(&buffers.downstream_readback);
    let field = mapped_vec::<[f32; 4]>(&buffers.field_readback);
    let vertices = mapped_vec::<GpuRiverVertex>(&buffers.carved_readback);
    buffers.flow_readback.unmap();
    buffers.downstream_readback.unmap();
    buffers.field_readback.unmap();
    buffers.carved_readback.unmap();
    if field.iter().flatten().any(|value| !value.is_finite())
        || vertices.iter().any(|vertex| {
            vertex
                .position_surface
                .iter()
                .chain(&vertex.attributes)
                .any(|value| !value.is_finite())
        })
    {
        return Err("GPU returned non-finite river state".into());
    }
    Ok(GpuRiverReadback {
        flow,
        downstream,
        field,
        vertices,
    })
}

fn mapped_vec<T: Pod>(buffer: &wgpu::Buffer) -> Vec<T> {
    let mapped = buffer.get_mapped_range(..);
    bytemuck::cast_slice(&mapped).to_vec()
}

fn map_readbacks(device: &wgpu::Device, buffers: [&wgpu::Buffer; 4]) -> Result<(), String> {
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
    pipeline: &wgpu::ComputePipeline,
    bind_group: &wgpu::BindGroup,
    groups: u32,
) {
    let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
        label: Some("motu GPU river compute pass"),
        timestamp_writes: None,
    });
    pass.set_pipeline(pipeline);
    pass.set_bind_group(0, bind_group, &[]);
    pass.dispatch_workgroups(groups, 1, 1);
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

fn grid_dimension() -> usize {
    std::env::var("MOTU_GPU_RIVER_GRID")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(DEFAULT_GRID_DIMENSION)
        .clamp(128, 1_024)
}

fn positive_float_environment(name: &str, default: f32) -> f32 {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse().ok())
        .filter(|value: &f32| value.is_finite() && *value > 0.0)
        .unwrap_or(default)
}

fn workgroups(items: usize) -> u32 {
    as_u32(items).div_ceil(WORKGROUP_SIZE)
}

fn as_u32(value: usize) -> u32 {
    u32::try_from(value).expect("GPU river buffer exceeds u32 indexing")
}

const _: () = assert!(size_of::<GpuRiverParams>().is_multiple_of(16));
const _: () = assert!(size_of::<GpuTerrainVertex>() == 16);
const _: () = assert!(size_of::<GpuRiverVertex>() == 32);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grid_helpers_are_stable_at_edges() {
        assert_eq!(neighbours(0, 4).collect::<Vec<_>>(), vec![1, 4, 5]);
        assert_eq!(grid_distance(0, 15, 4), 3);
        assert_eq!(cell_point(0, 4), Vec2::splat(0.125));
    }
}
