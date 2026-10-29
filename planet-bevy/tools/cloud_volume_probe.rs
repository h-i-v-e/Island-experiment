//! Native GPU morphology check using the production density AND production noise.
use std::{borrow::Cow, sync::mpsc, time::Duration};
use wgpu::util::DeviceExt;
#[path = "../src/environment/noise.rs"]
mod cloud_noise;
#[allow(dead_code)]
mod weather_fixture;
fn bytes(v: &[f32]) -> Vec<u8> {
    v.iter().flat_map(|x| x.to_le_bytes()).collect()
}
fn main() {
    let common = std::fs::read_to_string(std::env::args_os().nth(1).unwrap()).unwrap();
    let common = common
        .lines()
        .filter(|l| !l.starts_with('#'))
        .collect::<Vec<_>>()
        .join("\n");
    let source = common
        + r#"
@group(0) @binding(0) var noise:texture_3d<f32>;
@group(0) @binding(1) var field_sampler:sampler;
@group(0) @binding(2) var<uniform> parameters:EnvironmentUniform;
@group(0) @binding(3) var<storage,read> points:array<vec4<f32>>;
@group(0) @binding(4) var<storage,read_write> values:array<f32>;
@group(0) @binding(5) var weather:texture_cube_array<f32>;
@compute @workgroup_size(64) fn probe(@builtin(global_invocation_id) id:vec3<u32>){
 if id.x>=arrayLength(&points){return;}
 let point=points[id.x];
 values[id.x]=environment_density(point.xyz*(1.0+point.w/parameters.shape.x),parameters,noise,field_sampler,weather);
}
"#;
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = bevy_tasks::block_on(instance.request_adapter(&Default::default())).unwrap();
    let (device, queue) =
        bevy_tasks::block_on(adapter.request_device(&Default::default())).unwrap();
    println!("Cloud morphology GPU: {:?}", adapter.get_info());
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("shipping_cloud_morphology"),
        source: wgpu::ShaderSource::Wgsl(Cow::Owned(source)),
    });
    let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: None,
        layout: None,
        module: &module,
        entry_point: Some("probe"),
        compilation_options: Default::default(),
        cache: None,
    });
    let volume = device.create_texture(&wgpu::TextureDescriptor {
        label: None,
        size: wgpu::Extent3d {
            width: 64,
            height: 64,
            depth_or_array_layers: 64,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D3,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    queue.write_texture(
        volume.as_image_copy(),
        &cloud_noise::generate(),
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(256),
            rows_per_image: Some(64),
        },
        volume.size(),
    );
    let noise = volume.create_view(&Default::default());
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        address_mode_u: wgpu::AddressMode::Repeat,
        address_mode_v: wgpu::AddressMode::Repeat,
        address_mode_w: wgpu::AddressMode::Repeat,
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        ..Default::default()
    });
    const WIDTH: usize = 32;
    const HEIGHTS: usize = 51;
    let points: Vec<f32> = (0..WIDTH * WIDTH)
        .flat_map(|i| {
            let x = (i % WIDTH) as f32 * 15.0 - 232.5;
            let z = (i / WIDTH) as f32 * 15.0 - 232.5;
            let radius = (x * x + 1_000_000.0 + z * z).sqrt();
            (0..HEIGHTS).flat_map(move |j| {
                [
                    x / radius,
                    1000.0 / radius,
                    z / radius,
                    95.0 + j as f32 * 3.0,
                ]
            })
        })
        .collect();
    let count = points.len() / 4;
    let inputs = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: None,
        contents: &bytes(&points),
        usage: wgpu::BufferUsages::STORAGE,
    });
    let parameters = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: None,
        contents: &bytes(&[
            1000., 100., 55., 90., 0.42, 1.2, 0.6, 100., 0., 0., 0., 1., 0., 1., 0., 12000., 1.,
            1., 1., 1., 241., 1., 24., 0., 0., 3., 0., 7.7609863, 2., 0., 6_371_000., 0.,
        ]),
        usage: wgpu::BufferUsages::UNIFORM,
    });
    let output = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: count as u64 * 4,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: output.size(),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut widths = Vec::new();
    for (name, coverage, density) in [
        ("cumulus", [0.7, 0.0], [0.9, 0.0]),
        ("cirrus", [0.0, 0.7], [0.0, 0.9]),
    ] {
        let weather = weather_fixture::image_with_clouds(&device, &queue, coverage, density);
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &pipeline.get_bind_group_layout(0),
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&noise),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: parameters.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: inputs.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: output.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: wgpu::BindingResource::TextureView(&weather),
                },
            ],
        });
        let mut commands = device.create_command_encoder(&Default::default());
        {
            let mut pass = commands.begin_compute_pass(&Default::default());
            pass.set_pipeline(&pipeline);
            pass.set_bind_group(0, &group, &[]);
            pass.dispatch_workgroups((count as u32).div_ceil(64), 1, 1);
        }
        commands.copy_buffer_to_buffer(&output, 0, &readback, 0, output.size());
        queue.submit([commands.finish()]);
        let (send, recv) = mpsc::channel();
        readback
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |r| send.send(r).unwrap());
        device
            .poll(wgpu::PollType::Wait {
                submission_index: None,
                timeout: Some(Duration::from_secs(20)),
            })
            .unwrap();
        recv.recv().unwrap().unwrap();
        let mapped = readback.slice(..).get_mapped_range();
        let values: Vec<f32> = mapped
            .chunks_exact(4)
            .map(|c| f32::from_le_bytes(c.try_into().unwrap()))
            .collect();
        drop(mapped);
        readback.unmap();
        assert!(
            values
                .iter()
                .all(|v| v.is_finite() && (0.0..=1.0).contains(v))
        );
        let mut tops = Vec::new();
        let mut equivalent = 0.0;
        for profile in values.chunks_exact(HEIGHTS) {
            assert!(profile[0..2].iter().all(|v| *v == 0.0));
            assert!(
                profile[48..].iter().all(|v| *v == 0.0),
                "cloud escaped conservative volume"
            );
            let peak = profile.iter().copied().fold(0.0_f32, f32::max);
            if peak > 0.03 {
                let top = profile.iter().rposition(|v| *v > peak * 0.20).unwrap();
                tops.push(95.0 + top as f32 * 3.0);
                equivalent += profile.iter().sum::<f32>() * 3.0 / peak;
            }
        }
        assert!(tops.len() > 30, "missing {name} volume");
        let min = tops.iter().copied().fold(f32::INFINITY, f32::min);
        let max = tops.iter().copied().fold(0.0_f32, f32::max);
        let mean_width = equivalent / tops.len() as f32;
        println!(
            "{name}: {} occupied columns, roof {min:.1}..{max:.1} m, equivalent thickness {mean_width:.2} m",
            tops.len()
        );
        if name == "cumulus" {
            assert!(
                max - min > 25.0,
                "low cloud tops collapsed into a flat spherical roof"
            );
        }
        widths.push(mean_width);
    }
    assert!(
        widths[0] > widths[1] * 1.4,
        "both cloud layers have the same vertical profile"
    );
    println!(
        "Cloud morphology GPU PASS: production periodic volume, varied low roofs, distinct low/high thickness and bounded density."
    );
}
