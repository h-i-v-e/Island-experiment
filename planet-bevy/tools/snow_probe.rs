//! Run the actual weather snow-line helper on the native GPU, with distinct
//! current/previous cube faces and cloud-ceiling/temperature-reference heights.
use std::{borrow::Cow, sync::mpsc, time::Duration};
use wgpu::util::DeviceExt;

fn bytes(values: &[f32]) -> Vec<u8> {
    values.iter().flat_map(|v| v.to_le_bytes()).collect()
}
fn main() {
    let path = std::env::args_os().nth(1).expect("weather material WGSL");
    let source = std::fs::read_to_string(path).unwrap();
    let mut source = source
        .lines()
        .filter(|l| !l.starts_with('#'))
        .collect::<Vec<_>>()
        .join("\n")
        .replace("#{MATERIAL_BIND_GROUP}", "0")
        .replace("@binding(129)", "@binding(2)");
    source.insert_str(0, "struct Environment {simulation:vec4<f32>,shape:vec4<f32>}\n@group(0) @binding(0) var<uniform> environment:Environment;\n@group(0) @binding(1) var environment_sky_sampler:sampler;\n");
    source.push_str(r#"
@group(0) @binding(3) var<storage,read_write> output:array<vec4<f32>>;
@compute @workgroup_size(1) fn probe() {
 output[0]=vec4<f32>(weather_snow_line(vec3<f32>(1,0,0),100.0),weather_snow_line(vec3<f32>(-1,0,0),100.0),weather_snow_line(vec3<f32>(0,1,0),100.0),weather_snow_line(vec3<f32>(0,-1,0),100.0));
 output[1]=vec4<f32>(weather_snow_line(vec3<f32>(0,0,1),100.0),weather_snow_line(vec3<f32>(0,0,-1),100.0),weather_snow_line(vec3<f32>(1.00001,0,0.99999),100.0),weather_snow_line(vec3<f32>(0.99999,0,1.00001),100.0));
}
"#);
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = bevy_tasks::block_on(instance.request_adapter(&Default::default())).unwrap();
    let (device, queue) =
        bevy_tasks::block_on(adapter.request_device(&Default::default())).unwrap();
    println!("Snow GPU: {:?}", adapter.get_info());
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("actual_weather_snow"),
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
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("snow_climate_fixture"),
        size: wgpu::Extent3d {
            width: 4,
            height: 4,
            depth_or_array_layers: 72,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba16Float,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    let temperatures = [263.25, 293.25, 273.25, 280.0, 281.0, 288.75];
    let references = [0.0, 0.0, 50.0, 30.0, 60.0, 0.0];
    let mut pixels = Vec::new();
    for snapshot in 0..2 {
        for layer in 0..6 {
            for face in 0..6 {
                let fields = match layer {
                    2 => [
                        temperatures[face] + if snapshot == 0 { 5.25 } else { 0.0 },
                        1013.0,
                        0.8,
                        0.1,
                    ],
                    5 => [0.4, 0.2, 400.0, references[face]],
                    _ => [0.0; 4],
                };
                for _ in 0..16 {
                    for value in fields {
                        pixels.extend(half::f16::from_f32(value).to_le_bytes());
                    }
                }
            }
        }
    }
    queue.write_texture(
        texture.as_image_copy(),
        &pixels,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(32),
            rows_per_image: Some(4),
        },
        texture.size(),
    );
    let texture = texture.create_view(&wgpu::TextureViewDescriptor {
        dimension: Some(wgpu::TextureViewDimension::CubeArray),
        ..Default::default()
    });
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        ..Default::default()
    });
    let uniform = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: None,
        contents: &bytes(&[0.0; 8]),
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
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
    let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &pipeline.get_bind_group_layout(0),
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(&sampler),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::TextureView(&texture),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: output.as_entire_binding(),
            },
        ],
    });
    let mut frames = Vec::new();
    for mode in [0.0, 1.0, 1.5, 2.0] {
        queue.write_buffer(
            &uniform,
            0,
            &bytes(&[mode, 0.0, 0.0, 0.0, 1000.0, 45.0, 55.0, 0.0]),
        );
        let mut encoder = device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_compute_pass(&Default::default());
            pass.set_pipeline(&pipeline);
            pass.set_bind_group(0, &group, &[]);
            pass.dispatch_workgroups(1, 1, 1);
        }
        encoder.copy_buffer_to_buffer(&output, 0, &readback, 0, 32);
        queue.submit([encoder.finish()]);
        let (sender, receiver) = mpsc::channel();
        readback
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |r| sender.send(r).unwrap());
        device
            .poll(wgpu::PollType::Wait {
                submission_index: None,
                timeout: Some(Duration::from_secs(10)),
            })
            .unwrap();
        receiver.recv().unwrap().unwrap();
        let mapped = readback.slice(..).get_mapped_range();
        let values: Vec<f32> = mapped
            .chunks_exact(4)
            .map(|b| f32::from_le_bytes(b.try_into().unwrap()))
            .collect();
        assert!(values.iter().all(|v| v.is_finite()));
        println!("mode {mode}: {values:?}");
        frames.push(values);
        drop(mapped);
        readback.unmap();
    }
    assert!(
        frames[0].iter().all(|v| (*v - 100.0).abs() < 1e-6),
        "disabled weather must keep the fixed fallback"
    );
    let previous = &frames[1];
    assert!(
        previous[0] < 0.0 && previous[1] > 150.0,
        "cold and warm regions must have different snow lines"
    );
    assert!(
        (previous[2] - 50.77).abs() < 0.02,
        "freezing is relative to mean terrain height, not cloud ceiling"
    );
    assert!(
        (previous[4] - previous[5]).abs() < 1.0,
        "correct terrain reference must avoid double altitude cooling"
    );
    for i in 0..8 {
        assert!(
            (frames[2][i] - (frames[1][i] + frames[3][i]) * 0.5).abs() < 0.01,
            "snapshots must interpolate smoothly"
        );
        assert!(
            frames[3][i] > frames[1][i] + 39.0,
            "warming must lift the snow line"
        );
    }
    assert!(
        (previous[6] - previous[7]).abs() < 0.1,
        "cube seam must be continuous"
    );
    println!(
        "PASS: regional temperature, warming, mean orography, cube seams, snapshot interpolation, disabled fallback"
    );
}
