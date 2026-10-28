//! Native GPU checks for the actual production WGSL, including shifted camera viewports.
use std::{borrow::Cow, path::PathBuf, sync::mpsc, time::Duration};
use wgpu::util::DeviceExt;

const SIZE: u32 = 96;
const OUTPUT_BYTES: u64 = 8 * 16;
fn bytes(values: &[f32]) -> Vec<u8> {
    values
        .iter()
        .flat_map(|value| value.to_le_bytes())
        .collect()
}
fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn unit(value: [f32; 3]) -> [f32; 3] {
    let length = value.iter().map(|v| v * v).sum::<f32>().sqrt();
    value.map(|v| v / length)
}
fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a.into_iter().zip(b).map(|(x, y)| x * y).sum()
}
fn multiply(a: [f32; 16], b: [f32; 16]) -> [f32; 16] {
    std::array::from_fn(|i| {
        let column = i / 4;
        let row = i % 4;
        (0..4).map(|k| a[k * 4 + row] * b[column * 4 + k]).sum()
    })
}
fn shader_helpers(source: &str) -> String {
    source
        .lines()
        .filter(|line| !line.starts_with('#'))
        .collect::<Vec<_>>()
        .join("\n")
}

fn main() {
    let directory = PathBuf::from(
        std::env::args_os()
            .nth(1)
            .expect("environment source directory"),
    );
    let common = shader_helpers(&std::fs::read_to_string(directory.join("common.wgsl")).unwrap());
    let air = shader_helpers(&std::fs::read_to_string(directory.join("air.wgsl")).unwrap());
    let clouds = shader_helpers(&std::fs::read_to_string(directory.join("clouds.wgsl")).unwrap());
    let source = format!(
        r#"
struct FullscreenVertexOutput {{@builtin(position) position:vec4<f32>,@location(0) uv:vec2<f32>}}
struct View {{view_from_clip:mat4x4<f32>,world_from_view:mat4x4<f32>,viewport:vec4<f32>,world_position:vec3<f32>,exposure:f32}}
{common}
{air}
{clouds}
@group(0) @binding(11) var<storage,read_write> output:array<vec4<f32>>;
// Independent dense radial quadrature: these fixtures have constant spatial
// noise and radial views, so Beer-Lambert attenuation depends only on altitude.
// This retains the near-roof interior case when cloud shaping changes.
fn reference_transmission()->f32 {{
 let radius=weather.shape.x;
 let base=weather.shape.y;
 let roof=base+weather.shape.z;
 let eye_altitude=(length(view.world_position)-1.0)*radius;
 let inward=dot(ray_direction(vec2<f32>(0.5)),normalize(view.world_position))<0.0;
 let start=select(max(eye_altitude,base),base,inward);
 let step=max(roof-start,0.0)/512.0;
 var optical=0.0;
 for(var i=0u;i<512u;i+=1u) {{
   let altitude=start+(f32(i)+0.5)*step;
   let point=normalize(view.world_position)*(1.0+altitude/radius);
   optical+=environment_density(point,weather,cloud_noise,noise_sampler)*step*environment_extinction(weather);
 }}
 return exp(-optical);
}}
@compute @workgroup_size(1) fn probe() {{
 let low_size=ceil(view.viewport.zw/2.0);
 let estimate=estimate_at(low_size*0.5);
 output[0]=estimate.colour;
 let pixel=depth_pixel(vec2<f32>(0.5));
 output[1]=vec4<f32>(vec2<f32>(pixel),scene_distance(pixel),estimate.distance.x);
 output[2]=composite_at(pixel);
 output[3]=composite_at(vec2<i32>(0));
 let up=normalize(view.world_position);
 output[4]=vec4<f32>(environment_density(up*1.13,weather,cloud_noise,noise_sampler),
   environment_density(up*0.99,weather,cloud_noise,noise_sampler),
   environment_density(up*1.3,weather,cloud_noise,noise_sampler),0.0);
 output[5]=vec4<f32>(environment_density(normalize(vec3<f32>(0.000001,0.0,1.0))*1.13,weather,cloud_noise,noise_sampler),
   environment_density(normalize(vec3<f32>(-0.000001,0.0,1.0))*1.13,weather,cloud_noise,noise_sampler),0.0,0.0);
 output[6]=temporal_at(vec2<i32>(48)).colour;
 output[7]=vec4<f32>(previous_cloud_point(vec3<f32>(0.0,1.12,0.0)),reference_transmission());
}}
"#
    );
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = bevy_tasks::block_on(instance.request_adapter(&Default::default())).unwrap();
    let (device, queue) =
        bevy_tasks::block_on(adapter.request_device(&Default::default())).unwrap();
    println!("Cloud GPU: {:?}", adapter.get_info());
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("shipping_cloud_helpers"),
        source: wgpu::ShaderSource::Wgsl(Cow::Owned(source)),
    });
    let compute = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: None,
        layout: None,
        module: &shader,
        entry_point: Some("probe"),
        compilation_options: Default::default(),
        cache: None,
    });
    let texture = |label, dimension, format, size, mips, usage| {
        device.create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size,
            mip_level_count: mips,
            sample_count: 1,
            dimension,
            format,
            usage,
            view_formats: &[],
        })
    };
    let size = wgpu::Extent3d {
        width: SIZE,
        height: SIZE,
        depth_or_array_layers: 1,
    };
    let scene = texture(
        "scene",
        wgpu::TextureDimension::D2,
        wgpu::TextureFormat::Rgba32Float,
        size,
        1,
        wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
    );
    let cloud = texture(
        "cloud",
        wgpu::TextureDimension::D2,
        wgpu::TextureFormat::Rgba32Float,
        size,
        1,
        wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
    );
    let guide = texture(
        "guide",
        wgpu::TextureDimension::D2,
        wgpu::TextureFormat::Rg32Float,
        size,
        1,
        wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
    );
    let depth = texture(
        "depth",
        wgpu::TextureDimension::D2,
        wgpu::TextureFormat::Depth32Float,
        size,
        1,
        wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::RENDER_ATTACHMENT,
    );
    let noise = texture(
        "noise",
        wgpu::TextureDimension::D3,
        wgpu::TextureFormat::Rgba8Unorm,
        wgpu::Extent3d {
            width: 8,
            height: 8,
            depth_or_array_layers: 8,
        },
        2,
        wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
    );
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        address_mode_u: wgpu::AddressMode::Repeat,
        address_mode_v: wgpu::AddressMode::Repeat,
        address_mode_w: wgpu::AddressMode::Repeat,
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        mipmap_filter: wgpu::MipmapFilterMode::Linear,
        ..Default::default()
    });
    for (level, n) in [(0, 8), (1, 4)] {
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &noise,
                mip_level: level,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &[200, 200, 200, 255].repeat((n * n * n) as usize),
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(n * 4),
                rows_per_image: Some(n),
            },
            wgpu::Extent3d {
                width: n,
                height: n,
                depth_or_array_layers: n,
            },
        );
    }
    for (image, pixel) in [
        (&scene, [0.1, 0.2, 0.3, 1.0]),
        (&cloud, [1.0, 0.0, 0.0, 0.0]),
    ] {
        queue.write_texture(
            image.as_image_copy(),
            &bytes(&pixel.repeat((SIZE * SIZE) as usize)),
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(SIZE * 16),
                rows_per_image: Some(SIZE),
            },
            size,
        );
    }
    let history = texture(
        "history",
        wgpu::TextureDimension::D2,
        wgpu::TextureFormat::Rgba32Float,
        size,
        1,
        wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
    );
    let previous_guide = texture(
        "previous_guide",
        wgpu::TextureDimension::D2,
        wgpu::TextureFormat::Rg32Float,
        size,
        1,
        wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
    );
    let views = [
        &scene,
        &depth,
        &noise,
        &cloud,
        &guide,
        &history,
        &previous_guide,
    ]
    .map(|image| image.create_view(&Default::default()));
    let output = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: OUTPUT_BYTES,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: OUTPUT_BYTES,
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let mut surface_transmission = None;
    for (name, up, height, inward, foreground, offset, enabled, coverage) in [
        (
            "north",
            [0.0, 1.0, 0.0],
            1.0017,
            false,
            false,
            false,
            true,
            1.0,
        ),
        (
            "south",
            [0.0, -1.0, 0.0],
            1.0017,
            false,
            false,
            false,
            true,
            1.0,
        ),
        (
            "east",
            [1.0, 0.0, 0.0],
            1.0017,
            false,
            false,
            false,
            true,
            1.0,
        ),
        (
            "west",
            [-1.0, 0.0, 0.0],
            1.0017,
            false,
            false,
            false,
            true,
            1.0,
        ),
        (
            "front",
            [0.0, 0.0, 1.0],
            1.0017,
            false,
            false,
            false,
            true,
            1.0,
        ),
        (
            "back",
            [0.0, 0.0, -1.0],
            1.0017,
            false,
            false,
            false,
            true,
            1.0,
        ),
        ("orbit", [0.0, 1.0, 0.0], 3.5, true, false, false, true, 1.0),
        (
            "limb_near",
            [0.0, 1.0, 0.0],
            1.8,
            true,
            false,
            false,
            true,
            1.0,
        ),
        (
            "limb_far",
            [0.0, 1.0, 0.0],
            3.5,
            true,
            false,
            false,
            true,
            1.0,
        ),
        (
            "limb_offset",
            [0.0, 1.0, 0.0],
            3.5,
            true,
            false,
            true,
            true,
            1.0,
        ),
        (
            "beyond_clouds",
            [0.0, 1.0, 0.0],
            3.5,
            true,
            false,
            false,
            true,
            1.0,
        ),
        (
            "interior",
            [0.0, 1.0, 0.0],
            1.13,
            false,
            false,
            false,
            true,
            1.0,
        ),
        (
            "foreground",
            [0.0, 1.0, 0.0],
            1.0017,
            false,
            true,
            false,
            true,
            1.0,
        ),
        (
            "offset",
            [0.0, 1.0, 0.0],
            1.0017,
            false,
            false,
            true,
            true,
            1.0,
        ),
        (
            "offset_foreground",
            [0.0, 1.0, 0.0],
            1.0017,
            false,
            true,
            true,
            true,
            1.0,
        ),
        ("away", [0.0, 1.0, 0.0], 3.5, false, false, false, true, 1.0),
        (
            "disabled",
            [0.0, 1.0, 0.0],
            1.0017,
            false,
            false,
            false,
            false,
            1.0,
        ),
        (
            "empty",
            [0.0, 1.0, 0.0],
            1.0017,
            false,
            false,
            false,
            true,
            0.0,
        ),
        (
            "history_valid",
            [0.0, 1.0, 0.0],
            1.0017,
            false,
            false,
            false,
            true,
            1.0,
        ),
        (
            "history_reset",
            [0.0, 1.0, 0.0],
            1.0017,
            false,
            false,
            false,
            true,
            1.0,
        ),
        (
            "history_disabled",
            [0.0, 1.0, 0.0],
            1.0017,
            false,
            false,
            false,
            true,
            1.0,
        ),
        (
            "history_viewport",
            [0.0, 1.0, 0.0],
            1.0017,
            false,
            false,
            false,
            true,
            1.0,
        ),
        (
            "history_occluded",
            [0.0, 1.0, 0.0],
            1.0017,
            false,
            false,
            false,
            true,
            1.0,
        ),
        (
            "history_wrong_depth",
            [0.0, 1.0, 0.0],
            1.0017,
            false,
            false,
            false,
            true,
            1.0,
        ),
        (
            "history_empty",
            [0.0, 1.0, 0.0],
            1.0017,
            false,
            false,
            false,
            true,
            1.0,
        ),
        (
            "history_clamp",
            [0.0, 1.0, 0.0],
            1.0017,
            false,
            false,
            false,
            true,
            1.0,
        ),
        (
            "history_wind",
            [0.0, 1.0, 0.0],
            1.0017,
            false,
            false,
            false,
            true,
            1.0,
        ),
    ] {
        let history_fixture = name.starts_with("history_");
        let count = (SIZE * SIZE) as usize;
        let mut current_pixels = if history_fixture {
            (0..count)
                .flat_map(|i| {
                    if name == "history_wind" {
                        if i % 2 == 0 {
                            [0.0, 0.0, 0.0, 0.2]
                        } else {
                            [1.0, 1.0, 1.0, 0.6]
                        }
                    } else if i % 2 == 0 {
                        [0.4, 0.4, 0.4, 0.2]
                    } else {
                        [0.8, 0.8, 0.8, 0.6]
                    }
                })
                .collect::<Vec<f32>>()
        } else {
            [1.0, 0.0, 0.0, 0.0].repeat(count)
        };
        let centre = ((SIZE / 2 * SIZE + SIZE / 2) * 4) as usize;
        if history_fixture {
            current_pixels[centre..centre + 4].copy_from_slice(if name == "history_empty" {
                &[0.0, 0.0, 0.0, 1.0]
            } else {
                &[0.6, 0.6, 0.6, 0.4]
            });
        }
        queue.write_texture(
            cloud.as_image_copy(),
            &bytes(&current_pixels),
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(SIZE * 16),
                rows_per_image: Some(SIZE),
            },
            size,
        );
        let history_pixels = if name == "history_wind" {
            (0..count)
                .flat_map(|i| {
                    if i % (SIZE as usize) < (SIZE / 2) as usize {
                        [0.0, 1.0, 0.0, 0.5]
                    } else {
                        [1.0, 0.0, 0.0, 0.5]
                    }
                })
                .collect::<Vec<f32>>()
        } else if name == "history_clamp" {
            [100.0, 100.0, 100.0, 0.01].repeat(count)
        } else {
            [0.7, 0.7, 0.7, 0.5].repeat(count)
        };
        queue.write_texture(
            history.as_image_copy(),
            &bytes(&history_pixels),
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(SIZE * 16),
                rows_per_image: Some(SIZE),
            },
            size,
        );
        for (image, pixel) in [
            (
                &guide,
                [
                    1_000_000.0,
                    if name == "history_empty" { 0.0 } else { 120.0 },
                ],
            ),
            (
                &previous_guide,
                [
                    if name == "history_occluded" {
                        20.0
                    } else {
                        1_000_000.0
                    },
                    if name == "history_wrong_depth" {
                        300.0
                    } else {
                        120.0
                    },
                ],
            ),
        ] {
            queue.write_texture(
                image.as_image_copy(),
                &bytes(&pixel.repeat(count)),
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(SIZE * 8),
                    rows_per_image: Some(SIZE),
                },
                size,
            );
        }
        let forward = if name.starts_with("limb_") || name == "beyond_clouds" {
            // Impact radius lies outside terrain but inside the cloud shell.
            // The far camera enters the cloud shell more than one radius away.
            let impact = if name == "beyond_clouds" { 1.25 } else { 1.12 };
            let tangent = impact / height;
            [tangent, -(1.0_f32 - tangent * tangent).sqrt(), 0.0]
        } else {
            up.map(|v| if inward { -v } else { v })
        };
        let right = unit(cross(
            forward,
            if forward[1].abs() > 0.9 {
                [0.0, 0.0, 1.0]
            } else {
                [0.0, 1.0, 0.0]
            },
        ));
        let camera_up = cross(right, forward);
        let position = up.map(|v| v * height);
        let viewport = if offset {
            [16.0, 8.0, 64.0, 80.0]
        } else {
            [0.0, 0.0, SIZE as f32, SIZE as f32]
        };
        let mut values = vec![
            1.0 / 1.5,
            0.,
            0.,
            0.,
            0.,
            1.0 / 1.5,
            0.,
            0.,
            0.,
            0.,
            0.,
            20000.,
            0.,
            0.,
            -1.,
            0.,
        ];
        values.extend([
            right[0],
            right[1],
            right[2],
            0.,
            camera_up[0],
            camera_up[1],
            camera_up[2],
            0.,
            -forward[0],
            -forward[1],
            -forward[2],
            0.,
            position[0],
            position[1],
            position[2],
            1.,
        ]);
        values.extend(viewport);
        values.extend([position[0], position[1], position[2], 0.001]);
        let view = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: None,
            contents: &bytes(&values),
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let mut parameters = [
            1000.0,
            100.0,
            60.0,
            140.0,
            coverage,
            0.12,
            0.6,
            if enabled { 1.0 } else { 0.0 },
            0.,
            0.,
            0.,
            1.,
            0.,
            1.,
            0.,
            10000.,
            1.,
            1.,
            1.,
            1.,
            200.,
            1.,
            24.,
            0.,
            position[0],
            position[1],
            position[2],
            0.,
        ];
        if name == "history_wind" {
            parameters[8..12].copy_from_slice(&[0.0, 0.0, 0.015_f32.sin(), 0.015_f32.cos()]);
        }
        let back = forward.map(|v| -v);
        let world_to_view = [
            right[0],
            camera_up[0],
            back[0],
            0.0,
            right[1],
            camera_up[1],
            back[1],
            0.0,
            right[2],
            camera_up[2],
            back[2],
            0.0,
            -dot(right, position),
            -dot(camera_up, position),
            -dot(back, position),
            1.0,
        ];
        let projection = [
            1.5, 0.0, 0.0, 0.0, 0.0, 1.5, 0.0, 0.0, 0.0, 0.0, 0.0, -1.0, 0.0, 0.0, 0.00005, 0.0,
        ];
        let mut temporal_values = multiply(projection, world_to_view).to_vec();
        temporal_values.extend([position[0], position[1], position[2], 0.001]);
        let mut previous_viewport = viewport;
        if name == "history_viewport" {
            previous_viewport[0] += 8.0;
        }
        temporal_values.extend(previous_viewport);
        temporal_values.extend([0.0, 0.0, 0.0, 1.0]);
        temporal_values.extend([
            if history_fixture && name != "history_reset" {
                1.0
            } else {
                0.0
            },
            0.0,
            if history_fixture && name != "history_disabled" {
                1.0
            } else {
                0.0
            },
            0.85,
        ]);
        let temporal = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: None,
            contents: &bytes(&temporal_values),
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let weather = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: None,
            contents: &bytes(&parameters),
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &compute.get_bind_group_layout(0),
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&views[0]),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&views[1]),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: view.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: weather.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: wgpu::BindingResource::TextureView(&views[2]),
                },
                wgpu::BindGroupEntry {
                    binding: 6,
                    resource: wgpu::BindingResource::TextureView(&views[3]),
                },
                wgpu::BindGroupEntry {
                    binding: 7,
                    resource: wgpu::BindingResource::TextureView(&views[4]),
                },
                wgpu::BindGroupEntry {
                    binding: 8,
                    resource: temporal.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 9,
                    resource: wgpu::BindingResource::TextureView(&views[5]),
                },
                wgpu::BindGroupEntry {
                    binding: 10,
                    resource: wgpu::BindingResource::TextureView(&views[6]),
                },
                wgpu::BindGroupEntry {
                    binding: 11,
                    resource: output.as_entire_binding(),
                },
            ],
        });
        let mut encoder = device.create_command_encoder(&Default::default());
        {
            let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &views[1],
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(if foreground { 0.00005 / 0.02 } else { 0.0 }),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
        }
        {
            let mut pass = encoder.begin_compute_pass(&Default::default());
            pass.set_pipeline(&compute);
            pass.set_bind_group(0, &group, &[]);
            pass.dispatch_workgroups(1, 1, 1);
        }
        encoder.copy_buffer_to_buffer(&output, 0, &readback, 0, OUTPUT_BYTES);
        queue.submit([encoder.finish()]);
        let (tx, rx) = mpsc::channel();
        readback
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |result| tx.send(result).unwrap());
        device
            .poll(wgpu::PollType::Wait {
                submission_index: None,
                timeout: Some(Duration::from_secs(10)),
            })
            .unwrap();
        rx.recv().unwrap().unwrap();
        let mapped = readback.slice(..).get_mapped_range();
        let result: Vec<f32> = mapped
            .chunks_exact(4)
            .map(|v| f32::from_le_bytes(v.try_into().unwrap()))
            .collect();
        assert!(
            result.iter().all(|v| v.is_finite()),
            "{name}: nonfinite output"
        );
        let transmission = result[3];
        println!(
            "{name}: cloud transmittance={transmission:.5}, colour={:?}",
            &result[..3]
        );
        if foreground || name == "away" || name == "beyond_clouds" || !enabled || coverage == 0.0 {
            assert!(
                (transmission - 1.0).abs() < 0.0001,
                "{name}: cloud crossed occluder/disabled volume"
            );
        } else if name.starts_with("limb_") {
            assert!(
                (0.01..0.99).contains(&transmission),
                "{name}: cloud shell disappeared beyond the solid planet silhouette"
            );
        } else {
            let reference = result[31];
            assert!(
                (0.3..0.999).contains(&reference),
                "{name}: fixture must cross a nonempty, partially transmitting cloud"
            );
            assert!(
                (transmission - reference).abs() < 0.006,
                "{name}: attenuation {transmission} disagrees with dense radial reference {reference}"
            );
        }
        if name == "north" {
            surface_transmission = Some(transmission);
        }
        if ["south", "east", "west", "front", "back", "orbit", "offset"].contains(&name) {
            assert!(
                (transmission - surface_transmission.unwrap()).abs() < 0.025,
                "{name}: shell changed across pole/orbit/viewport"
            );
        }
        if history_fixture {
            let resolved = &result[24..28];
            println!("{name}: temporal={resolved:?}");
            match name {
                "history_valid" => {
                    assert!(
                        (resolved[0] - 0.685).abs() < 0.002 && (resolved[3] - 0.485).abs() < 0.002,
                        "valid history did not accumulate"
                    );
                }
                "history_clamp" => {
                    assert!(
                        (resolved[0] - 0.77).abs() < 0.002 && (resolved[3] - 0.23).abs() < 0.002,
                        "neighbourhood clamp failed"
                    );
                }
                "history_wind" => {
                    assert!(
                        resolved[1] > resolved[0] + 0.2,
                        "wind reprojected onto the wrong cloud feature"
                    );
                    assert!(
                        (result[28] + 1.12 * 0.03_f32.sin()).abs() < 0.0001,
                        "wind quaternion mapping wrong"
                    );
                }
                "history_empty" => assert_eq!(
                    resolved,
                    &[0.0, 0.0, 0.0, 1.0],
                    "vanished cloud left a ghost"
                ),
                _ => assert_eq!(
                    resolved,
                    &[0.6, 0.6, 0.6, 0.4],
                    "invalid history survived rejection"
                ),
            }
        } else if foreground {
            for (actual, expected) in result[8..11].iter().zip([0.1, 0.2, 0.3]) {
                assert!(
                    (actual - expected).abs() < 0.0001,
                    "{name}: cloud leaked across depth edge"
                );
            }
        } else {
            assert!(
                (result[8] - 1.0).abs() < 0.0001
                    && result[9].abs() < 0.0001
                    && result[10].abs() < 0.0001,
                "{name}: sky composite changed"
            );
        }
        if offset {
            for (actual, expected) in result[12..15].iter().zip([0.1, 0.2, 0.3]) {
                assert!(
                    (actual - expected).abs() < 0.0001,
                    "panel viewport modified outside camera"
                );
            }
        }
        assert_eq!(
            &result[4..6],
            &[48.0, 48.0],
            "offset viewport sampled different target pixel"
        );
        if enabled && coverage > 0.0 {
            assert!(result[16] > 0.1, "inside shell has no density");
        }
        assert_eq!(
            &result[17..19],
            &[0.0, 0.0],
            "density escaped spherical layer"
        );
        assert!(
            (result[20] - result[21]).abs() < 0.0001,
            "longitude seam in weather field"
        );
        drop(mapped);
        readback.unmap();
    }
    println!(
        "PASS: spherical clouds at six hemispheres, orbit and near/far/offset orbital limbs, inside layer, foreground depth, empty/disabled volume, seamless weather, offset-viewport bilateral composition, temporal reset/disocclusion/clamping and wind-aware reprojection."
    );
}
