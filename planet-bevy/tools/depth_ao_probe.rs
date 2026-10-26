//! Executes the actual AO WGSL against a synthetic steep plane and foreground.
//! Run via check_depth_ao.py after building the viewer; requires a native GPU.
use std::borrow::Cow;
use wgpu::util::DeviceExt;
const N: u32 = 96;
fn bytes(values: &[f32]) -> Vec<u8> {
    values.iter().flat_map(|v| v.to_le_bytes()).collect()
}
fn main() {
    let source =
        std::fs::read_to_string(std::env::args_os().nth(1).expect("AO shader path")).unwrap();
    let mut source = source
        .lines()
        .filter(|l| !l.starts_with("#import"))
        .collect::<Vec<_>>()
        .join("\n");
    while let Some(start) = source.find("@fragment") {
        let body = source[start..].find('{').unwrap() + start;
        let mut nesting = 1;
        let mut end = body + 1;
        while nesting > 0 {
            match source.as_bytes()[end] {
                b'{' => nesting += 1,
                b'}' => nesting -= 1,
                _ => {}
            }
            end += 1;
        }
        source.replace_range(start..end, "");
    }
    source.insert_str(
        0,
        "struct View {view_from_clip:mat4x4<f32>,clip_from_view:mat4x4<f32>}\n",
    );
    source.push_str("\n@group(0) @binding(6) var<storage,read_write> output:array<vec4<f32>>;\n@compute @workgroup_size(8,8) fn probe(@builtin(global_invocation_id) id:vec3<u32>) {let n=u32(settings.scale.z);if id.x>=n||id.y>=n{return;}var uv=(vec2<f32>(id.xy)+0.5)/f32(n);uv.y+=select(-0.00000005,0.00000005,id.y%2u==0u);output[id.y*n+id.x]=estimate_at(uv,id.xy);}\n");
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = bevy_tasks::block_on(instance.request_adapter(&Default::default())).unwrap();
    let (device, queue) =
        bevy_tasks::block_on(adapter.request_device(&Default::default())).unwrap();
    println!("Adapter: {:?}", adapter.get_info());
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("actual_planet_depth_ao"),
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
    let depth_shader=device.create_shader_module(wgpu::ShaderModuleDescriptor {label:None,source:wgpu::ShaderSource::Wgsl(Cow::Borrowed("@group(0) @binding(0) var<storage,read> depths:array<f32>;\n@vertex fn vertex(@builtin(vertex_index) i:u32)->@builtin(position) vec4<f32>{let uv=vec2<f32>(f32(i>>1u),f32(i&1u))*2.0;return vec4<f32>(uv*vec2<f32>(2.0,-2.0)+vec2<f32>(-1.0,1.0),0.0,1.0);}\n@fragment fn fragment(@builtin(position) p:vec4<f32>)->@builtin(frag_depth) f32{return depths[u32(p.y)*96u+u32(p.x)];}"))});
    let depth_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: None,
        layout: None,
        vertex: wgpu::VertexState {
            module: &depth_shader,
            entry_point: Some("vertex"),
            compilation_options: Default::default(),
            buffers: &[],
        },
        primitive: Default::default(),
        depth_stencil: Some(wgpu::DepthStencilState {
            format: wgpu::TextureFormat::Depth32Float,
            depth_write_enabled: Some(true),
            depth_compare: Some(wgpu::CompareFunction::Always),
            stencil: Default::default(),
            bias: Default::default(),
        }),
        multisample: Default::default(),
        fragment: Some(wgpu::FragmentState {
            module: &depth_shader,
            entry_point: Some("fragment"),
            compilation_options: Default::default(),
            targets: &[],
        }),
        multiview_mask: None,
        cache: None,
    });
    let near = 0.00005_f32;
    let f = 1.5_f32;
    let matrices = [
        1.0 / f,
        0.,
        0.,
        0.,
        0.,
        1.0 / f,
        0.,
        0.,
        0.,
        0.,
        0.,
        1.0 / near,
        0.,
        0.,
        -1.,
        0.,
        f,
        0.,
        0.,
        0.,
        0.,
        f,
        0.,
        0.,
        0.,
        0.,
        0.,
        -1.,
        0.,
        0.,
        near,
        0.,
    ];
    let view = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: None,
        contents: &bytes(&matrices),
        usage: wgpu::BufferUsages::UNIFORM,
    });
    for half in [false, true] {
        for foreground in [false, true] {
            let n = if half { N / 2 } else { N };
            let mut values = Vec::new();
            for y in 0..N {
                for x in 0..N {
                    let ray_x = ((x as f32 + 0.5) / N as f32 * 2.0 - 1.0) / f;
                    let distance = 3.0 / (0.6 - 0.8 * ray_x);
                    let distance = if foreground && (36..60).contains(&x) && (32..72).contains(&y) {
                        distance - 0.25
                    } else {
                        distance
                    };
                    values.push(if y < 10 {
                        0.0
                    } else {
                        near / (distance / 1000.0)
                    });
                }
            }
            let data = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: None,
                contents: &bytes(&values),
                usage: wgpu::BufferUsages::STORAGE,
            });
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: None,
                size: wgpu::Extent3d {
                    width: N,
                    height: N,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Depth32Float,
                usage: wgpu::TextureUsages::TEXTURE_BINDING
                    | wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            });
            let depth = texture.create_view(&Default::default());
            let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: None,
                layout: &depth_pipeline.get_bind_group_layout(0),
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: data.as_entire_binding(),
                }],
            });
            let settings = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: None,
                contents: &bytes(&[
                    1.5,
                    0.6,
                    12.0,
                    0.0,
                    1000.0,
                    if half { 2.0 } else { 1.0 },
                    n as f32,
                    0.0,
                ]),
                usage: wgpu::BufferUsages::UNIFORM,
            });
            let size = u64::from(n * n * 16);
            let output = device.create_buffer(&wgpu::BufferDescriptor {
                label: None,
                size,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
                mapped_at_creation: false,
            });
            let staging = device.create_buffer(&wgpu::BufferDescriptor {
                label: None,
                size,
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
            });
            let compute_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: None,
                layout: &compute.get_bind_group_layout(0),
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::TextureView(&depth),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: view.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 4,
                        resource: settings.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 6,
                        resource: output.as_entire_binding(),
                    },
                ],
            });
            let mut encoder = device.create_command_encoder(&Default::default());
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: None,
                    color_attachments: &[],
                    depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                        view: &depth,
                        depth_ops: Some(wgpu::Operations {
                            load: wgpu::LoadOp::Clear(0.0),
                            store: wgpu::StoreOp::Store,
                        }),
                        stencil_ops: None,
                    }),
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                });
                pass.set_pipeline(&depth_pipeline);
                pass.set_bind_group(0, &group, &[]);
                pass.draw(0..3, 0..1);
            }
            {
                let mut pass = encoder.begin_compute_pass(&Default::default());
                pass.set_pipeline(&compute);
                pass.set_bind_group(0, &compute_group, &[]);
                pass.dispatch_workgroups(n.div_ceil(8), n.div_ceil(8), 1);
            }
            encoder.copy_buffer_to_buffer(&output, 0, &staging, 0, size);
            queue.submit([encoder.finish()]);
            let (sender, receiver) = std::sync::mpsc::channel();
            staging
                .slice(..)
                .map_async(wgpu::MapMode::Read, move |r| sender.send(r).unwrap());
            device
                .poll(wgpu::PollType::Wait {
                    submission_index: None,
                    timeout: Some(std::time::Duration::from_secs(10)),
                })
                .unwrap();
            receiver.recv().unwrap().unwrap();
            let mapped = staging.slice(..).get_mapped_range();
            let floats: Vec<f32> = mapped
                .chunks_exact(4)
                .map(|v| f32::from_le_bytes(v.try_into().unwrap()))
                .collect();
            assert!(floats.iter().all(|v| v.is_finite()));
            let mut maximum = 0.0_f32;
            let mut positive = 0;
            for y in 0..n {
                for x in 0..n {
                    let row = &floats[((y * n + x) * 4) as usize..];
                    if y < if half { 5 } else { 10 } {
                        assert!(row[..4].iter().all(|v| v.abs() < 1e-7));
                    } else if y > if half { 7 } else { 12 } && y < n - 3 && x > 2 && x < n - 3 {
                        maximum = maximum.max(row[0]);
                        positive += usize::from(row[0] > 0.0001);
                    }
                }
            }
            println!(
                "half={half}, foreground={foreground}, maximum_ao={maximum}, occluded_pixels={positive}"
            );
            if foreground {
                assert!(
                    maximum > 0.001 && positive > 10,
                    "foreground must occlude nearby geometry"
                );
            } else {
                assert!(maximum < 0.0001, "a planar surface must not occlude itself");
            }
        }
    }
    println!(
        "PASS: actual AO WGSL on a steep plane, empty sky and an occluding surface at full and half resolution"
    );
}
