//! GPU regression coverage for the shipping ocean's wave field and cube convention.
use std::{borrow::Cow, sync::mpsc, time::Duration};
use wgpu::util::DeviceExt;

fn bytes(values: &[f32]) -> Vec<u8> {
    values.iter().flat_map(|v| v.to_le_bytes()).collect()
}
fn direction(face: usize, u: f32, v: f32) -> [f32; 3] {
    let p = match face {
        0 => [1., -v, -u], 1 => [-1., -v, u], 2 => [u, 1., v],
        3 => [u, -1., -v], 4 => [u, -v, 1.], _ => [-u, -v, -1.],
    };
    let length = p.iter().map(|x| x*x).sum::<f32>().sqrt();
    p.map(|x| x/length)
}
fn main() {
    let source = std::fs::read_to_string(std::env::args_os().nth(1).unwrap()).unwrap();
    let source = source.split("struct OceanUniform").nth(1).unwrap();
    let source = format!("struct OceanUniform{}", source.split("@vertex fn vertex").next().unwrap())
        .replace("#{MATERIAL_BIND_GROUP}", "0");
    // The shipping material and ocean now share their clamp/filter sampler.
    let path=std::path::PathBuf::from(std::env::args_os().nth(1).unwrap());
    let material=std::fs::read_to_string(path.parent().unwrap().parent().unwrap().join("environment/material.wgsl")).unwrap();
    let sampler=material.lines().find(|line|line.contains("@binding(102)")).unwrap().replace("#{MATERIAL_BIND_GROUP}","0");
    let source=source+"\n"+&sampler;
    let source = source + "\n@group(0) @binding(0) var<storage,read> inputs:array<vec4<f32>>;\n@group(0) @binding(1) var<storage,read_write> output:array<vec4<f32>>;\n@compute @workgroup_size(1) fn probe(@builtin(global_invocation_id) id:vec3<u32>){let input=inputs[id.x];var w=waves_at(input.xyz,abs(input.w));if input.w<0.0 {let s=swell_at(input.xyz,ocean.sources[0],COMPONENTS[0],SPEEDS[0],0.1);w=Waves(s.displacement,s.slope,s.crest,0.0,s.variance,0.0,0.0);}let offset=id.x*4u;output[offset]=vec4<f32>(w.displacement,w.crest);output[offset+1u]=vec4<f32>(w.slope,w.foam);output[offset+2u]=vec4<f32>(w.variance,w.depth,w.distance,0.0);output[offset+3u]=coast(input.xyz);}\n";
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = bevy_tasks::block_on(instance.request_adapter(&Default::default())).unwrap();
    let (device, queue) = bevy_tasks::block_on(adapter.request_device(&Default::default())).unwrap();
    println!("Ocean GPU: {:?}", adapter.get_info());
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("shipping_ocean_wave_functions"), source: wgpu::ShaderSource::Wgsl(Cow::Owned(source)),
    });
    let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: None, layout: None, module: &shader, entry_point: Some("probe"),
        compilation_options: Default::default(), cache: None,
    });
    let texture = |dimension, width, height, depth| device.create_texture(&wgpu::TextureDescriptor {
        label: None, size: wgpu::Extent3d { width, height, depth_or_array_layers: depth },
        mip_level_count: 1, sample_count: 1, dimension, format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST, view_formats: &[],
    });
    let cube = texture(wgpu::TextureDimension::D2, 8, 8, 6);
    let volume = texture(wgpu::TextureDimension::D3, 1, 1, 1);
    queue.write_texture(volume.as_image_copy(), &[128;4], wgpu::TexelCopyBufferLayout { offset:0, bytes_per_row:Some(4), rows_per_image:Some(1) }, volume.size());
    let cube_view = cube.create_view(&wgpu::TextureViewDescriptor { dimension:Some(wgpu::TextureViewDimension::Cube), ..Default::default() });
    let noise_view = volume.create_view(&Default::default());
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor { mag_filter:wgpu::FilterMode::Linear, min_filter:wgpu::FilterMode::Linear, ..Default::default() });
    let mut directions = Vec::new();
    for face in 0..6 {
        for (x,y) in [(1,1),(6,1),(1,6),(6,6)] {
            directions.push(direction(face, (x as f32+0.5)/4.-1., (y as f32+0.5)/4.-1.));
        }
    }
    for p in [[0.,1.,0.], [0.,-1.,0.], [1.,1.,1.], [-1.,-1.,-1.]] {
        let length = p.iter().map(|x| x*x).sum::<f32>().sqrt();
        directions.push(p.map(|x| x/length));
    }
    let radial_start = directions.len();
    for theta in [0.0_f32, 0.00001, 0.001, 0.1, 0.25, 0.5, 0.75, 0.9, 0.99999, 1.0, 1.0001, std::f32::consts::PI] {
        for azimuth in [0.0_f32, 1.1, 2.2, 3.3] {
            directions.push([theta.sin()*azimuth.cos(), theta.cos(), theta.sin()*azimuth.sin()]);
        }
    }
    let count = directions.len() as u32;
    let readback = device.create_buffer(&wgpu::BufferDescriptor { label:None, size:u64::from(count)*64, usage:wgpu::BufferUsages::COPY_DST|wgpu::BufferUsages::MAP_READ, mapped_at_creation:false });
    let dispatch = |pixels: &[u8], time:f32, anchor:f32, amplitude:f32, footprint:f32| -> Vec<[f32;16]> {
        queue.write_texture(cube.as_image_copy(), pixels, wgpu::TexelCopyBufferLayout { offset:0, bytes_per_row:Some(32), rows_per_image:Some(8) }, cube.size());
        let input:Vec<f32> = directions.iter().flat_map(|p| [p[0],p[1],p[2],footprint]).collect();
        let input = device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label:None, contents:&bytes(&input), usage:wgpu::BufferUsages::STORAGE });
        let uniform = device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label:None, contents:&bytes(&[1000.,192.,256.,1., amplitude,1.,0.18,0.85, 0.16,0.8,0.,1024., anchor,0.,0.,time, 0.,1.,0.,0.7, 1.,0.,0.,0.4, 0.,0.,1.,1.2, -1.,0.,0.,2.]), usage:wgpu::BufferUsages::UNIFORM });
        let output = device.create_buffer(&wgpu::BufferDescriptor { label:None, size:u64::from(count)*64, usage:wgpu::BufferUsages::STORAGE|wgpu::BufferUsages::COPY_SRC, mapped_at_creation:false });
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor { label:None, layout:&pipeline.get_bind_group_layout(0), entries:&[
            wgpu::BindGroupEntry { binding:0, resource:input.as_entire_binding() },
            wgpu::BindGroupEntry { binding:1, resource:output.as_entire_binding() },
            wgpu::BindGroupEntry { binding:100, resource:uniform.as_entire_binding() },
            wgpu::BindGroupEntry { binding:101, resource:wgpu::BindingResource::TextureView(&cube_view) },
            wgpu::BindGroupEntry { binding:102, resource:wgpu::BindingResource::Sampler(&sampler) },
            wgpu::BindGroupEntry { binding:103, resource:wgpu::BindingResource::TextureView(&noise_view) },
            wgpu::BindGroupEntry { binding:104, resource:wgpu::BindingResource::Sampler(&sampler) },
        ] });
        let mut encoder = device.create_command_encoder(&Default::default());
        { let mut pass = encoder.begin_compute_pass(&Default::default()); pass.set_pipeline(&pipeline); pass.set_bind_group(0,&group,&[]); pass.dispatch_workgroups(count,1,1); }
        encoder.copy_buffer_to_buffer(&output,0,&readback,0,u64::from(count)*64);
        queue.submit([encoder.finish()]);
        let (tx,rx)=mpsc::channel();
        readback.slice(..).map_async(wgpu::MapMode::Read,move|result| {tx.send(result).unwrap();});
        device.poll(wgpu::PollType::Wait { submission_index:None, timeout:Some(Duration::from_secs(10)) }).unwrap();
        rx.recv().unwrap().unwrap();
        let values=readback.slice(..).get_mapped_range();
        let result=values.chunks_exact(64).map(|chunk| std::array::from_fn(|i| f32::from_le_bytes(chunk[i*4..i*4+4].try_into().unwrap()))).collect();
        drop(values);readback.unmap();result
    };
    let constant = |rgba:[u8;4]| (0..8*8*6).flat_map(|_|rgba).collect::<Vec<_>>();
    let wet = constant([255,255,0,255]);
    let baseline = dispatch(&wet, 0., 0., 1., 0.1);
    let moved = dispatch(&wet, 0., 1., 1., 0.1);
    assert_eq!(baseline, moved, "rotating mesh anchor dragged global wave phase");
    assert!(baseline.iter().any(|row| row[..3].iter().map(|v|v*v).sum::<f32>()>0.00001));
    let later = dispatch(&wet, 2., 0., 1., 0.1);
    assert!(baseline.iter().zip(later).any(|(a,b)| (a[0]-b[0]).abs()>0.01));
    for row in &baseline {
        assert!(row.iter().all(|v|v.is_finite()));

    }
    for (label,pixels,amp,footprint) in [
        ("dry",constant([0,0,0,128]),1.,0.1),
        ("zero amplitude",wet.clone(),0.,0.1),
        ("unresolved",wet.clone(),1.,100.),
    ] {
        for row in dispatch(&pixels,0.,0.,amp,footprint) {
            assert!(row[0..3].iter().all(|v|v.abs()<1e-6), "{label}: {row:?}");
            assert!(row[7].abs()<1e-6, "{label} produced foam");
        }
    }
    let shallow = constant([5,255,0,128]);
    for t in 0..20 {
        for (i,row) in dispatch(&shallow,t as f32*0.3,0.,3.,0.1).iter().enumerate() {
            let height:f32 = (0..3).map(|j| directions[i][j]*row[j]).sum();
            assert!(height.abs()<5./25.5, "shallow water penetrated bed: {height}");
            let radial_slope:f32 = (0..3).map(|j| directions[i][j]*row[j+4]).sum();
            assert!(radial_slope.abs()<1e-5);
        }
    }
    // Isolate one source: verify exact spherical sine height and its derivative,
    // radial symmetry, outward time propagation, and smooth endpoints.
    for time in [0.0_f32, 2.0, 5.0] {
        let samples = dispatch(&wet,time,0.,1.,-1.0);
        for (i,row) in samples.iter().enumerate().skip(radial_start) {
            let n = directions[i];
            let sine = (n[0]*n[0]+n[2]*n[2]).sqrt();
            let theta = sine.atan2(n[1]);
            let (envelope, derivative) = if theta < 1.0 {
                let a=std::f32::consts::PI*theta;
                (a.sin().powi(2),std::f32::consts::TAU*a.sin()*a.cos()/1000.)
            } else {(0.,0.)};
            let k=std::f32::consts::TAU/30.;
            let phase=(theta*1000.-time*3.6)*k+0.7;
            let expected=0.34*envelope*phase.sin();
            let height:f32=(0..3).map(|j|n[j]*row[j]).sum();
            assert!((height-expected).abs()<0.00002,"radial sine mismatch theta={theta} time={time}: {height} vs {expected}");
            let tangent=if sine>0.000001 {[n[1]*n[0]/sine,-sine,n[1]*n[2]/sine]} else {[0.;3]};
            let expected_slope=0.34*(envelope*k*phase.cos()+derivative*phase.sin());
            for j in 0..3 {assert!((row[4+j]-tangent[j]*expected_slope).abs()<0.00001,"radial normal mismatch at {theta}");}
            if theta<=0.000001 || theta>=1.0 {
                assert!(row[..3].iter().all(|v|v.abs()<0.000001),"source/outer endpoint nonzero");
                assert!(row[4..7].iter().all(|v|v.abs()<0.000001),"source/outer endpoint slope nonzero");
            }
            assert!(row.iter().all(|v|v.is_finite()));
        }
    }
    let pattern:Vec<u8>=(0..6).flat_map(|face| (0..8).flat_map(move |y| (0..8).flat_map(move |x| {
        let n=direction(face,(x as f32+0.5)/4.-1.,(y as f32+0.5)/4.-1.);
        [(n[0]*0.5+0.5).mul_add(255.,0.).round() as u8, (n[1]*0.5+0.5).mul_add(255.,0.).round() as u8, (n[2]*0.5+0.5).mul_add(255.,0.).round() as u8,255]
    }))).collect();
    for (i,row) in dispatch(&pattern,0.,0.,1.,0.1).iter().take(24).enumerate() {
        for j in 0..3 {
            assert!((row[12+j]-(directions[i][j]*0.5+0.5)).abs()<0.003, "cube orientation mismatch: {i} {j}");
        }
    }
    println!("Ocean GPU PASS: anchored animation; dry/shallow/zero/far limits; finite poles; tangent slopes; all six hardware cube faces; spherical sine profile, radial symmetry, outward propagation and zero-height/zero-slope endpoints.");
}
