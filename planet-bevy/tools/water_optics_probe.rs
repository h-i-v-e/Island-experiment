//! Run the actual water lookup helpers against known geometry, not a CPU copy.
use std::{borrow::Cow, sync::mpsc, time::Duration};
use wgpu::util::DeviceExt;

const SIZE: u32 = 256;

fn bytes(values: &[f32]) -> Vec<u8> {
    values.iter().flat_map(|v| v.to_le_bytes()).collect()
}

fn main() {
    let source = std::fs::read_to_string(std::env::args_os().nth(1).unwrap()).unwrap();
    let start = source.find("struct ReflectionHit").unwrap();
    let end = source.find("struct WaterOptics").unwrap();
    let viewport = source
        .lines()
        .find(|s| s.starts_with("fn viewport_fade"))
        .unwrap();
    let mapping = source
        .lines()
        .filter(|s| {
            s.starts_with("fn view_uv(")
                || s.starts_with("fn view_pixel(")
                || s.starts_with("fn view_texture_uv(")
        })
        .collect::<Vec<_>>()
        .join("\n");
    let shader_source = format!(
        r#"
struct View {{clip_from_world:mat4x4<f32>,view_from_world:mat4x4<f32>,clip_from_view:mat4x4<f32>,viewport:vec4<f32>}}
struct ScenePoint {{position:vec3<f32>,eye:f32,valid:f32}}
@group(0) @binding(0) var<uniform> view:View;
@group(0) @binding(1) var scene:texture_2d<f32>;
@group(0) @binding(2) var<uniform> request:vec4<f32>;
@group(0) @binding(3) var<storage,read_write> output:array<vec4<f32>>;
fn scene_point(uv:vec2<f32>,radius:f32)->ScenePoint {{
 let size=vec2<i32>(textureDimensions(scene));
 let pixel=clamp(vec2<i32>(view_pixel(uv)),vec2<i32>(0),size-1);
 let value=textureLoad(scene,pixel,0);
 return ScenePoint(value.xyz,-value.z*radius,value.w);
}}
{viewport}
{mapping}
{helpers}
@compute @workgroup_size(1) fn probe() {{
 let p=vec3<f32>(0.0,0.0,-0.01);
 let v=vec3<f32>(0.0,0.0,1.0);
 let bank_normal=vec3<f32>(0.0,0.8,0.6);
 let normal=select(bank_normal,vec3<f32>(0.0,0.0,1.0),request.x==1.0);
 let detail=select(normal,normalize(vec3<f32>(0.0,0.995,0.1)),request.x==2.0);
 let hit=reflected_scene(p,detail,normal,v,1000.0);
 output[0]=vec4<f32>(hit.uv,hit.weight,0.0);
 // The maximum physical distortion must remain bounded when moving the
 // camera from two metres to two hundred metres above shallow water.
 let close=bounded_refraction_offset(vec2<f32>(0.01,0.0),2.0,0.5);
 let far=bounded_refraction_offset(vec2<f32>(0.01,0.0),200.0,0.5);
 let dry=bounded_refraction_offset(vec2<f32>(0.01,0.0),200.0,0.0);
 output[1]=vec4<f32>(close.x*4.0,far.x*400.0,length(dry),0.0);
 // This land is behind the water in camera depth but above its plane.
 // Camera-depth-only validity incorrectly refracts it into the river.
 let above=ScenePoint(p+vec3<f32>(0.0,0.001,-0.001),11.0,1.0);
 let below=ScenePoint(p+vec3<f32>(0.0,0.0,-0.001),11.0,1.0);
 output[2]=vec4<f32>(select(0.0,1.0,behind_water(above,p,bank_normal,10.0,1000.0)),
  select(0.0,1.0,behind_water(below,p,bank_normal,10.0,1000.0)),0.0,0.0);
 let uv=vec2<f32>(0.25,0.75);
 output[3]=vec4<f32>(view_texture_uv(uv,vec2<f32>(256.0)),view_uv(view_pixel(uv)));
}}
"#,
        helpers = &source[start..end],
    );
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = bevy_tasks::block_on(instance.request_adapter(&Default::default())).unwrap();
    let (device, queue) =
        bevy_tasks::block_on(adapter.request_device(&Default::default())).unwrap();
    println!("Water optics GPU: {:?}", adapter.get_info());
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("actual_water_optics_helpers"),
        source: wgpu::ShaderSource::Wgsl(Cow::Owned(shader_source)),
    });
    let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: None,
        layout: None,
        module: &shader,
        entry_point: Some("probe"),
        compilation_options: Default::default(),
        cache: None,
    });
    let projection = [
        1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, -1.0, 0.0, 0.0, 0.00005, 0.0,
    ];
    let identity = [
        1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
    ];
    let mut values = projection.to_vec();
    values.extend(identity);
    values.extend(projection);
    values.extend([0.0, 0.0, SIZE as f32, SIZE as f32]);
    let view = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: None,
        contents: &bytes(&values),
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
    });
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: None,
        size: wgpu::Extent3d {
            width: SIZE,
            height: SIZE,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba32Float,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    let texture_view = texture.create_view(&Default::default());
    let output = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: 64,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: 64,
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    for (fixture, mode, expected_hit) in [
        ("bank", 0.0, true),
        ("offset_viewport", 0.0, true),
        ("sky", 0.0, false),
        ("overhead", 1.0, false),
        ("silhouette", 0.0, false),
        ("foreground", 0.0, false),
        ("bed", 2.0, false),
    ] {
        let offset = fixture == "offset_viewport";
        values[48..52].copy_from_slice(if offset {
            &[64.0, 32.0, 160.0, 192.0]
        } else {
            &[0.0, 0.0, 256.0, 256.0]
        });
        queue.write_buffer(&view, 0, &bytes(&values));
        let mut pixels = Vec::new();
        for y in 0..SIZE {
            for x in 0..SIZE {
                let uv = [
                    (x as f32 + 0.5 - values[48]) / values[50],
                    (y as f32 + 0.5 - values[49]) / values[51],
                ];
                let depth = match fixture {
                    "sky" => 0.0,
                    "foreground" => 9.8,
                    "silhouette" if uv[1] < 0.42 => 9.8,
                    "silhouette" => 25.0,
                    _ => 12.0,
                };
                pixels.extend([
                    (uv[0] * 2.0 - 1.0) * depth / 1000.0,
                    (1.0 - uv[1] * 2.0) * depth / 1000.0,
                    -depth / 1000.0,
                    if depth > 0.0 { 1.0 } else { 0.0 },
                ]);
            }
        }
        queue.write_texture(
            texture.as_image_copy(),
            &bytes(&pixels),
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(SIZE * 16),
                rows_per_image: Some(SIZE),
            },
            texture.size(),
        );
        let request = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: None,
            contents: &bytes(&[mode, 0.0, 0.0, 0.0]),
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &pipeline.get_bind_group_layout(0),
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: view.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&texture_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: request.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
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
        encoder.copy_buffer_to_buffer(&output, 0, &readback, 0, 64);
        queue.submit([encoder.finish()]);
        let (tx, rx) = mpsc::channel();
        readback
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |r| tx.send(r).unwrap());
        device
            .poll(wgpu::PollType::Wait {
                submission_index: None,
                timeout: Some(Duration::from_secs(10)),
            })
            .unwrap();
        rx.recv().unwrap().unwrap();
        let data = readback.slice(..).get_mapped_range();
        let result: Vec<f32> = data
            .chunks_exact(4)
            .map(|v| f32::from_le_bytes(v.try_into().unwrap()))
            .collect();
        assert!(
            result.iter().all(|v| v.is_finite()),
            "{fixture}: non-finite result"
        );
        let weight = result[2];
        println!("{fixture}: reflection weight={weight}");
        if expected_hit {
            assert!(weight > 0.5, "lost real reflected bank");
        } else {
            assert!(weight < 0.001, "{fixture}: false reflection hit");
        }
        assert!(
            result[4] > 0.0 && result[4] <= 0.2 && result[5] <= 0.20001 && result[6] == 0.0,
            "distortion grew beyond the shallow-water bound"
        );
        assert_eq!(
            &result[8..10],
            &[0.0, 1.0],
            "land above water passed refraction validity"
        );
        let expected = if offset {
            [0.40625, 0.6875, 0.25, 0.75]
        } else {
            [0.25, 0.75, 0.25, 0.75]
        };
        assert_eq!(
            &result[12..16],
            &expected,
            "shifted viewport sampled the wrong scene pixels"
        );
        drop(data);
        readback.unmap();
    }
    println!(
        "PASS: reflected bank retained; sky, overhead, silhouette, foreground and bed rejected; depth-bounded refraction at both camera distances; above-water land excluded; shifted viewport colour/depth coordinates round-trip correctly."
    );
}
