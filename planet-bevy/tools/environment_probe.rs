//! Native GPU checks of the shipping spherical weather-density/light helpers.
use std::{borrow::Cow, sync::mpsc};
use wgpu::util::DeviceExt;

fn main() {
    bevy_tasks::block_on(run());
}
fn bytes(values: &[f32]) -> Vec<u8> {
    values.iter().flat_map(|v| v.to_ne_bytes()).collect()
}
async fn run() {
    let source =
        std::fs::read_to_string(std::env::args_os().nth(1).expect("common shader path")).unwrap();
    let common = source
        .lines()
        .filter(|line| !line.starts_with('#'))
        .collect::<Vec<_>>()
        .join("\n");
    let shader = format!(
        r#"{common}
@group(0) @binding(0) var noise:texture_3d<f32>;
@group(0) @binding(1) var noise_sampler:sampler;
@group(0) @binding(2) var<uniform> parameters:EnvironmentUniform;
@group(0) @binding(3) var<storage,read> points:array<vec4<f32>>;
@group(0) @binding(4) var<storage,read_write> results:array<vec4<f32>>;
@compute @workgroup_size(32)
fn test(@builtin(global_invocation_id) id:vec3<u32>) {{
 if id.x>=arrayLength(&points) {{return;}}
 let p=points[id.x].xyz;
 let density=environment_density(p,parameters,noise,noise_sampler);
 let shadow=environment_shadow(p,parameters,noise,noise_sampler);
 let visibility=environment_sun_visibility(p,parameters);
 let illumination=environment_cloud_light(p,normalize(p),parameters,noise,noise_sampler);
 results[id.x]=vec4<f32>(density,shadow,visibility,length(illumination));
}}
"#
    );
    let mut descriptor = wgpu::InstanceDescriptor::new_without_display_handle();
    descriptor.backends = wgpu::Backends::METAL;
    let instance = wgpu::Instance::new(descriptor);
    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions::default())
        .await
        .unwrap();
    let (device, queue) = adapter
        .request_device(&wgpu::DeviceDescriptor::default())
        .await
        .unwrap();
    println!("Environment GPU: {:?}", adapter.get_info());
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("shipping_environment_helpers"),
        source: wgpu::ShaderSource::Wgsl(Cow::Owned(shader)),
    });
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("smooth_periodic_weather_fixture"),
        size: wgpu::Extent3d {
            width: 16,
            height: 16,
            depth_or_array_layers: 16,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D3,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    let mut pixels = Vec::new();
    for z in 0..16 {
        for y in 0..16 {
            for x in 0..16 {
                let wave = |v: i32| (v as f32 * std::f32::consts::TAU / 16.0).sin();
                pixels.extend([
                    ((0.63 + 0.16 * wave(x) + 0.1 * wave(y)) * 255.0) as u8,
                    ((0.6 + 0.25 * wave(z)) * 255.0) as u8,
                    ((0.55 + 0.2 * wave(x + y)) * 255.0) as u8,
                    255,
                ]);
            }
        }
    }
    queue.write_texture(
        texture.as_image_copy(),
        &pixels,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(16 * 4),
            rows_per_image: Some(16),
        },
        wgpu::Extent3d {
            width: 16,
            height: 16,
            depth_or_array_layers: 16,
        },
    );
    let view = texture.create_view(&Default::default());
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        address_mode_u: wgpu::AddressMode::Repeat,
        address_mode_v: wgpu::AddressMode::Repeat,
        address_mode_w: wgpu::AddressMode::Repeat,
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        ..Default::default()
    });
    let points: Vec<f32> = [
        [0.0, 1.10, 0.0, 0.0],  // lower radial bound
        [0.0, 1.16, 0.0, 0.0],  // upper radial bound
        [0.0, 1.13, 0.0, 0.0],  // inside the sun-facing cloud
        [0.0, -1.13, 0.0, 0.0], // inside the planet-shadowed cloud
        [0.0, 1.0, 0.0, 0.0],   // surface shadow
        [1.13, 0.0, 0.0, 0.0],
        [0.0, 0.0, -1.13, 0.0],
        [0.0, 0.0, 1.13, 0.0],
    ]
    .into_iter()
    .flatten()
    .collect();
    let input = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: None,
        contents: &bytes(&points),
        usage: wgpu::BufferUsages::STORAGE,
    });
    let output = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: points.len() as u64 * 4,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: output.size(),
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: None,
        layout: None,
        module: &module,
        entry_point: Some("test"),
        compilation_options: Default::default(),
        cache: None,
    });
    let base = [
        1000.0, 100.0, 60.0, 140.0, 0.65, 1.8, 0.7, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 1.0, 0.0,
        12000.0, 1.0, 1.0, 1.0, 1.0, 200.0, 1.0, 24.0, 0.0, 0.0, 1.0017, 0.0, 0.37,
    ];
    let mut runs = Vec::new();
    for mode in 0..5 {
        let mut parameters = base;
        if mode == 1 {
            parameters[7] = 0.0;
        }
        if mode == 2 {
            parameters[4] = 0.0;
        }
        if mode == 3 {
            parameters[6] = 0.0;
        }
        if mode == 4 {
            parameters[9] = std::f32::consts::FRAC_1_SQRT_2;
            parameters[11] = std::f32::consts::FRAC_1_SQRT_2;
        }
        let settings = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: None,
            contents: &bytes(&parameters),
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &pipeline.get_bind_group_layout(0),
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: settings.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: input.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: output.as_entire_binding(),
                },
            ],
        });
        let mut encoder = device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_compute_pass(&Default::default());
            pass.set_pipeline(&pipeline);
            pass.set_bind_group(0, &group, &[]);
            pass.dispatch_workgroups(1, 1, 1);
        }
        encoder.copy_buffer_to_buffer(&output, 0, &readback, 0, output.size());
        queue.submit([encoder.finish()]);
        let (tx, rx) = mpsc::channel();
        readback
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |result| tx.send(result).unwrap());
        device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
        rx.recv().unwrap().unwrap();
        let mapped = readback.slice(..).get_mapped_range();
        let values: Vec<[f32; 4]> = mapped
            .chunks_exact(16)
            .map(|v| {
                std::array::from_fn(|i| f32::from_ne_bytes(v[i * 4..i * 4 + 4].try_into().unwrap()))
            })
            .collect();
        assert!(
            values.iter().flatten().all(|v| v.is_finite()),
            "nonfinite weather result: {values:?}"
        );
        assert!(
            values
                .iter()
                .all(|v| (0.0..=1.0).contains(&v[0]) && (0.0..=1.0).contains(&v[1]))
        );
        if mode == 0 {
            assert!(
                values[0][0] < 1e-5 && values[1][0] < 1e-5,
                "cloud escaped radial shell: {values:?}"
            );
            assert!(values[2][0] > 0.01, "weather fixture contains no cloud");
            assert_eq!(values[2][2], 1.0);
            assert_eq!(values[3][2], 0.0, "planet must shadow far-side cloud");
            assert!(
                values[4][1] < 0.99,
                "dense overhead cloud should cast a surface shadow"
            );
        }
        if mode == 1 || mode == 2 {
            assert!(
                values.iter().all(|v| v[0] == 0.0 && v[1] == 1.0),
                "disabled/clear weather should have no cloud or shadow"
            );
        }
        if mode == 3 {
            assert!(
                values.iter().all(|v| v[1] == 1.0),
                "zero shadow strength must preserve lighting"
            );
        }
        drop(mapped);
        readback.unmap();
        runs.push(values);
    }
    assert!(
        (runs[4][5][0] - runs[0][6][0]).abs() < 1e-5,
        "wind must rotate the global volume coherently"
    );
    check_reflection_cube(&device, &queue, &view, &sampler, &common, &base);
    check_air(&device, &queue, &common, &base);
    println!(
        "Environment GPU: radial bounds, density/lighting finite, wind covariance, disabled/clear coverage, planet night shadow and surface shadow passed"
    );
}

fn check_reflection_cube(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    noise: &wgpu::TextureView,
    noise_sampler: &wgpu::Sampler,
    common: &str,
    base: &[f32; 28],
) {
    let folder = std::path::PathBuf::from(std::env::args_os().nth(1).unwrap());
    let source = std::fs::read_to_string(folder.with_file_name("reflections.wgsl")).unwrap();
    let reflection = source
        .lines()
        .filter(|line| !line.starts_with('#'))
        .collect::<Vec<_>>()
        .join("\n");
    let air = std::fs::read_to_string(folder.with_file_name("air.wgsl")).unwrap();
    let air = air
        .lines()
        .filter(|line| !line.starts_with('#'))
        .collect::<Vec<_>>()
        .join("\n");
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("shipping_reflection_cube"),
        source: wgpu::ShaderSource::Wgsl(Cow::Owned(format!("{common}\n{air}\n{reflection}"))),
    });
    let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: None,
        layout: None,
        module: &shader,
        entry_point: Some("update"),
        compilation_options: Default::default(),
        cache: None,
    });
    let cube = device.create_texture(&wgpu::TextureDescriptor {
        label: None,
        size: wgpu::Extent3d {
            width: 64,
            height: 64,
            depth_or_array_layers: 6,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba16Float,
        usage: wgpu::TextureUsages::STORAGE_BINDING
            | wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let storage = cube.create_view(&wgpu::TextureViewDescriptor {
        dimension: Some(wgpu::TextureViewDimension::D2Array),
        ..Default::default()
    });
    let mut data = bytes(base);
    data.extend([0u32, 6, 0, 0].into_iter().flat_map(u32::to_ne_bytes));
    let settings = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: None,
        contents: &data,
        usage: wgpu::BufferUsages::UNIFORM,
    });
    let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &pipeline.get_bind_group_layout(0),
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(noise),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(noise_sampler),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::TextureView(&storage),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: settings.as_entire_binding(),
            },
        ],
    });
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: 64 * 64 * 6 * 8,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&Default::default());
    {
        let mut pass = encoder.begin_compute_pass(&Default::default());
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &group, &[]);
        pass.dispatch_workgroups(8, 8, 6);
    }
    encoder.copy_texture_to_buffer(
        cube.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &readback,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(64 * 8),
                rows_per_image: Some(64),
            },
        },
        wgpu::Extent3d {
            width: 64,
            height: 64,
            depth_or_array_layers: 6,
        },
    );
    queue.submit([encoder.finish()]);
    let (tx, rx) = mpsc::channel();
    readback
        .slice(..)
        .map_async(wgpu::MapMode::Read, move |result| tx.send(result).unwrap());
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    rx.recv().unwrap().unwrap();
    let mapped = readback.slice(..).get_mapped_range();
    let mut populated = 0;
    for pixel in mapped.chunks_exact(8) {
        for channel in pixel.chunks_exact(2) {
            let half = u16::from_ne_bytes(channel.try_into().unwrap());
            assert!(half & 0x7c00 != 0x7c00, "nonfinite reflection cube");
            assert_eq!(half & 0x8000, 0, "negative reflection radiance or density");
        }
        let alpha = u16::from_ne_bytes(pixel[6..8].try_into().unwrap());
        assert!(alpha <= 0x3c00, "cloud-column cache outside 0..1");
        populated += usize::from(alpha > 0);
    }
    assert!(populated > 0, "cloud-column cube was not written");
    drop(mapped);
    readback.unmap();

    let material = std::fs::read_to_string(folder.with_file_name("material.wgsl")).unwrap();
    let start = material.find("fn material_cloud_shadow").unwrap();
    let end = material.find("fn material_sky").unwrap();
    let helpers = &material[start..end];
    let source = format!(
        r#"{common}
@group(0) @binding(0) var environment_sky:texture_cube<f32>;
@group(0) @binding(1) var environment_sky_sampler:sampler;
@group(0) @binding(2) var<uniform> environment:EnvironmentUniform;
@group(0) @binding(3) var<storage,read> points:array<vec4<f32>>;
@group(0) @binding(4) var<storage,read_write> results:array<vec4<f32>>;
{helpers}
@compute @workgroup_size(8)
fn probe(@builtin(global_invocation_id) id:vec3<u32>) {{
 if id.x>=arrayLength(&points) {{return;}}
 let p=points[id.x].xyz;
 results[id.x]=vec4<f32>(material_cloud_shadow(p),material_cloud_light(p),0.0,0.0);
}}
"#
    );
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("shipping_cached_material_shadows"),
        source: wgpu::ShaderSource::Wgsl(Cow::Owned(source)),
    });
    let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: None,
        layout: None,
        module: &shader,
        entry_point: Some("probe"),
        compilation_options: Default::default(),
        cache: None,
    });
    let positions = bytes(&[0.0, 1.0, 0.0, 0.0, 0.0, -1.0, 0.0, 0.0, 0.0, 1.2, 0.0, 0.0]);
    let positions = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: None,
        contents: &positions,
        usage: wgpu::BufferUsages::STORAGE,
    });
    let result = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: 48,
        usage: wgpu::BufferUsages::COPY_SRC | wgpu::BufferUsages::STORAGE,
        mapped_at_creation: false,
    });
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: 48,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let view = cube.create_view(&wgpu::TextureViewDescriptor {
        dimension: Some(wgpu::TextureViewDimension::Cube),
        ..Default::default()
    });
    for mode in 0..3 {
        let mut parameters = *base;
        if mode == 1 {
            parameters[7] = 0.0;
        }
        if mode == 2 {
            parameters[6] = 0.0;
        }
        let settings = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: None,
            contents: &bytes(&parameters),
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &pipeline.get_bind_group_layout(0),
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(noise_sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: settings.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: positions.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: result.as_entire_binding(),
                },
            ],
        });
        let mut encoder = device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_compute_pass(&Default::default());
            pass.set_pipeline(&pipeline);
            pass.set_bind_group(0, &group, &[]);
            pass.dispatch_workgroups(1, 1, 1);
        }
        encoder.copy_buffer_to_buffer(&result, 0, &readback, 0, 48);
        queue.submit([encoder.finish()]);
        let (tx, rx) = mpsc::channel();
        readback
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |result| tx.send(result).unwrap());
        device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
        rx.recv().unwrap().unwrap();
        let mapped = readback.slice(..).get_mapped_range();
        let values: Vec<[f32; 4]> = mapped
            .chunks_exact(16)
            .map(|v| {
                std::array::from_fn(|i| f32::from_ne_bytes(v[i * 4..i * 4 + 4].try_into().unwrap()))
            })
            .collect();
        assert!(values.iter().flatten().all(|v| v.is_finite()));
        assert!(values.iter().all(|v| (0.3 - 1e-5..=1.0).contains(&v[0])));
        assert_eq!(
            values[1][0], 1.0,
            "night-side surface must not receive extra cloud shadow"
        );
        assert_eq!(
            values[2][0], 1.0,
            "surface above the cloud layer must remain clear"
        );
        if mode == 0 {
            assert!(values[0][0] < 0.99, "cache must cast a shadow");
        } else {
            assert!(
                values.iter().all(|v| v[0] == 1.0),
                "disabled cloud/shadow must ignore stale cached density"
            );
        }
        drop(mapped);
        readback.unmap();
    }
    println!(
        "Reflection cube GPU: all six HDR faces finite; global cached density bounded; cached shadows, night, layer bounds and disabled controls passed"
    );
}

fn check_air(device: &wgpu::Device, queue: &wgpu::Queue, common: &str, base: &[f32; 28]) {
    let path = std::path::PathBuf::from(std::env::args_os().nth(1).unwrap());
    let air = std::fs::read_to_string(path.with_file_name("air.wgsl")).unwrap();
    let air = air
        .lines()
        .filter(|line| !line.starts_with('#'))
        .collect::<Vec<_>>()
        .join("\n");
    let source = format!(
        r#"{common}
{air}
@group(0) @binding(0) var<uniform> parameters:EnvironmentUniform;
@group(0) @binding(1) var<uniform> endpoint:vec4<f32>;
@group(0) @binding(2) var<storage,read_write> result:array<vec4<f32>>;
@compute @workgroup_size(1)
fn probe() {{
 let air=environment_air(parameters.viewer.xyz,endpoint.xyz,parameters);
 result[0]=vec4<f32>(air.transmission,0.0);
 result[1]=vec4<f32>(air.inscattering,0.0);
}}
"#
    );
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("shipping_foreground_air"),
        source: wgpu::ShaderSource::Wgsl(Cow::Owned(source)),
    });
    let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: None,
        layout: None,
        module: &shader,
        entry_point: Some("probe"),
        compilation_options: Default::default(),
        cache: None,
    });
    let output = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: 32,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: 32,
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    for mode in 0..7 {
        let mut parameters = *base;
        let mut endpoint = [0.0, 1.13, 0.0, 0.0];
        if mode == 1 {
            parameters[19] = 0.0;
        }
        if mode == 2 {
            parameters[21] = 0.0;
        }
        if mode == 3 {
            endpoint = [parameters[24], parameters[25], parameters[26], 0.0];
        }
        if mode == 4 {
            parameters[25] = 1.5;
            endpoint[1] = 1.8;
        }
        if mode == 5 {
            parameters[25] = 2.5;
        }
        if mode == 6 {
            parameters[25] = -1.0017;
            endpoint[1] = -1.13;
        }
        let settings = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: None,
            contents: &bytes(&parameters),
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let point = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: None,
            contents: &bytes(&endpoint),
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &pipeline.get_bind_group_layout(0),
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: settings.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: point.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: output.as_entire_binding(),
                },
            ],
        });
        let mut encoder = device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_compute_pass(&Default::default());
            pass.set_pipeline(&pipeline);
            pass.set_bind_group(0, &group, &[]);
            pass.dispatch_workgroups(1, 1, 1);
        }
        encoder.copy_buffer_to_buffer(&output, 0, &readback, 0, 32);
        queue.submit([encoder.finish()]);
        let (tx, rx) = mpsc::channel();
        readback
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |result| tx.send(result).unwrap());
        device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
        rx.recv().unwrap().unwrap();
        let mapped = readback.slice(..).get_mapped_range();
        let values: Vec<f32> = mapped
            .chunks_exact(4)
            .map(|v| f32::from_ne_bytes(v.try_into().unwrap()))
            .collect();
        assert!(
            values.iter().all(|v| v.is_finite()),
            "foreground air must remain finite in mode {mode}: {values:?}"
        );
        assert!(
            values[..3].iter().all(|v| *v > 0.0 && *v <= 1.0),
            "air transmission is outside0..1"
        );
        assert!(
            values[4..7].iter().all(|v| *v >= 0.0),
            "negative foreground scattering"
        );
        if (1..=4).contains(&mode) {
            assert!(
                values[..3].iter().all(|v| *v == 1.0) && values[4..7].iter().all(|v| *v == 0.0),
                "disabled/empty/vacuum air must be identity: {mode} {values:?}"
            );
        }
        if mode == 0 {
            assert!(
                values[2] < values[0],
                "blue light should scatter more strongly than red"
            );
            assert!(
                values[4..7].iter().all(|v| *v > 0.0),
                "daytime air must add haze to foreground cloud light"
            );
        }
        if mode == 6 {
            assert!(
                values[4..7].iter().all(|v| *v == 0.0),
                "planet-shadowed air must not add sunlit haze"
            );
        }
        drop(mapped);
        readback.unmap();
    }
    println!(
        "Foreground air GPU: finite bounded transmission/scattering, off/zero-haze/zero-path/vacuum identity, surface/orbit paths and night shadow passed"
    );
}
