//! GPU regression coverage for the shipping ocean's wave field and cube convention.
use std::{borrow::Cow, sync::mpsc, time::Duration};
use wgpu::util::DeviceExt;

fn bytes(values: &[f32]) -> Vec<u8> {
    values.iter().flat_map(|v| v.to_le_bytes()).collect()
}
fn weather_state(current: [f32;12]) -> [f32;24] {
    let mut state=[0.;24];
    state[..12].copy_from_slice(&current);
    state[12..20].copy_from_slice(&current[4..12]);
    state[20]=1.;
    state
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
    let source=source+"\n"+&sampler+r#"
struct WeatherFixture {wind:vec4<f32>,first:vec4<f32>,second:vec4<f32>,previous_first:vec4<f32>,previous_second:vec4<f32>,parameters:vec4<f32>}
@group(0) @binding(129) var<uniform> weather_fixture:WeatherFixture;
fn weather_enabled()->bool {return weather_fixture.wind.w>0.5;}
fn weather_blend()->f32 {return clamp(weather_fixture.parameters.x,0.0,1.0);}
fn weather_lower(n:vec3<f32>)->vec4<f32> {let w=weather_fixture.wind.xyz;return vec4<f32>(w-n*dot(n,w),0.0);}
fn weather_swell(n:vec3<f32>,index:u32)->vec4<f32> {let w=select(weather_fixture.second,weather_fixture.first,index==0u);return vec4<f32>(w.xyz-n*dot(n,w.xyz),w.w);}
fn weather_swell_previous(n:vec3<f32>,index:u32)->vec4<f32> {let w=select(weather_fixture.previous_second,weather_fixture.previous_first,index==0u);return vec4<f32>(w.xyz-n*dot(n,w.xyz),w.w);}
"#;
    let source = source + "\n@group(0) @binding(0) var<storage,read> inputs:array<vec4<f32>>;\n@group(0) @binding(1) var<storage,read_write> output:array<vec4<f32>>;\n@compute @workgroup_size(1) fn probe(@builtin(global_invocation_id) id:vec3<u32>){let input=inputs[id.x];if input.w < -2.0 {
 let p=input.xyz*ocean.shape.x;let axis=normalize(vec3<f32>(0.4,0.7,0.3));
 let domain=weather_domain(p,15.0,1u);let gradient=weather_domain_gradient(axis,domain);
 let ex=vec3<f32>(0.125,0.0,0.0);let ey=ex.zxy;let ez=ex.yzx;
 let fd=vec3<f32>(
 dot(weather_domain(p+ex,15.0,1u).position-weather_domain(p-ex,15.0,1u).position,axis),
 dot(weather_domain(p+ey,15.0,1u).position-weather_domain(p-ey,15.0,1u).position,axis),
 dot(weather_domain(p+ez,15.0,1u).position-weather_domain(p-ez,15.0,1u).position,axis))/0.25;
 output[id.x*4u]=vec4<f32>(gradient,0.0);output[id.x*4u+1u]=vec4<f32>(fd,0.0);return;
}var w=waves_at(input.xyz,abs(input.w));if input.w<0.0 {let s=swell_at(input.xyz,ocean.sources[0],COMPONENTS[0],SPEEDS[0],0.1);w=Waves(s.displacement,s.slope,s.crest,0.0,s.variance,0.0,0.0);}let offset=id.x*4u;output[offset]=vec4<f32>(w.displacement,w.crest);output[offset+1u]=vec4<f32>(w.slope,w.foam);output[offset+2u]=vec4<f32>(w.variance,w.depth,w.distance,0.0);output[offset+3u]=coast(input.xyz);}\n";
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
    let seam_start = directions.len();
    for point in [
        [1.0_f32, 1.0, 0.2],
        [0.8, 1.0, 0.6],
        [0.8, 0.0, 0.6],
        [-0.36, 0.8, 0.48],
        [-0.48, -0.6, 0.64],
    ] {
        for epsilon in [-0.000001_f32, 0.000001] {
            let mut p = point;
            p[0] += epsilon;
            let length = p.iter().map(|x| x * x).sum::<f32>().sqrt();
            directions.push(p.map(|x| x / length));
        }
    }
    let radial_start = directions.len();
    for theta in [0.0_f32, 0.00001, 0.001, 0.1, 0.25, 0.5, 0.75, 0.9, 0.99999, 1.0, 1.0001, std::f32::consts::PI] {
        for azimuth in [0.0_f32, 1.1, 2.2, 3.3] {
            directions.push([theta.sin()*azimuth.cos(), theta.cos(), theta.sin()*azimuth.sin()]);
        }
    }
    let weather_fixture = std::cell::RefCell::new([0.0_f32;24]);
    let dispatch = |directions: &[[f32;3]], pixels: &[u8], time:f32, anchor:f32, amplitude:f32, footprint:f32| -> Vec<[f32;16]> {
        let count = directions.len() as u32;
        let readback = device.create_buffer(&wgpu::BufferDescriptor { label:None, size:u64::from(count)*64, usage:wgpu::BufferUsages::COPY_DST|wgpu::BufferUsages::MAP_READ, mapped_at_creation:false });
        queue.write_texture(cube.as_image_copy(), pixels, wgpu::TexelCopyBufferLayout { offset:0, bytes_per_row:Some(32), rows_per_image:Some(8) }, cube.size());
        let input:Vec<f32> = directions.iter().flat_map(|p| [p[0],p[1],p[2],footprint]).collect();
        let input = device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label:None, contents:&bytes(&input), usage:wgpu::BufferUsages::STORAGE });
        let uniform = device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label:None, contents:&bytes(&[1000.,192.,256.,1., amplitude,1.,0.18,0.85, 0.16,0.8,0.,1024., anchor,0.,0.,time, 0.,1.,0.,0.7, 1.,0.,0.,0.4, 0.,0.,1.,1.2, -1.,0.,0.,2.]), usage:wgpu::BufferUsages::UNIFORM });
        let weather = device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label:None, contents:&bytes(&*weather_fixture.borrow()), usage:wgpu::BufferUsages::UNIFORM });
        let output = device.create_buffer(&wgpu::BufferDescriptor { label:None, size:u64::from(count)*64, usage:wgpu::BufferUsages::STORAGE|wgpu::BufferUsages::COPY_SRC, mapped_at_creation:false });
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor { label:None, layout:&pipeline.get_bind_group_layout(0), entries:&[
            wgpu::BindGroupEntry { binding:0, resource:input.as_entire_binding() },
            wgpu::BindGroupEntry { binding:1, resource:output.as_entire_binding() },
            wgpu::BindGroupEntry { binding:100, resource:uniform.as_entire_binding() },
            wgpu::BindGroupEntry { binding:101, resource:wgpu::BindingResource::TextureView(&cube_view) },
            wgpu::BindGroupEntry { binding:102, resource:wgpu::BindingResource::Sampler(&sampler) },
            wgpu::BindGroupEntry { binding:103, resource:wgpu::BindingResource::TextureView(&noise_view) },
            wgpu::BindGroupEntry { binding:104, resource:wgpu::BindingResource::Sampler(&sampler) },
            wgpu::BindGroupEntry { binding:129, resource:weather.as_entire_binding() },
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
    let baseline = dispatch(&directions, &wet, 0., 0., 1., 0.1);
    let moved = dispatch(&directions, &wet, 0., 1., 1., 0.1);
    assert_eq!(baseline, moved, "rotating mesh anchor dragged global wave phase");
    assert!(baseline.iter().any(|row| row[..3].iter().map(|v|v*v).sum::<f32>()>0.00001));
    let later = dispatch(&directions, &wet, 2., 0., 1., 0.1);
    assert!(baseline.iter().zip(later).any(|(a,b)| (a[0]-b[0]).abs()>0.01));
    for row in &baseline {
        assert!(row.iter().all(|v|v.is_finite()));

    }
    for (label,pixels,amp,footprint) in [
        ("dry",constant([0,0,0,128]),1.,0.1),
        ("zero amplitude",wet.clone(),0.,0.1),
        ("unresolved",wet.clone(),1.,100.),
    ] {
        for row in dispatch(&directions, &pixels,0.,0.,amp,footprint) {
            assert!(row[0..3].iter().all(|v|v.abs()<1e-6), "{label}: {row:?}");
            assert!(row[7].abs()<1e-6, "{label} produced foam");
        }
    }
    let shallow = constant([5,255,0,128]);
    for t in 0..20 {
        for (i,row) in dispatch(&directions, &shallow,t as f32*0.3,0.,3.,0.1).iter().enumerate() {
            let height:f32 = (0..3).map(|j| directions[i][j]*row[j]).sum();
            assert!(height.abs()<5./25.5, "shallow water penetrated bed: {height}");
            let radial_slope:f32 = (0..3).map(|j| directions[i][j]*row[j+4]).sum();
            assert!(radial_slope.abs()<1e-5);
        }
    }
    // Isolate one source: verify exact spherical sine height and its derivative,
    // radial symmetry, outward time propagation, and smooth endpoints.
    for time in [0.0_f32, 2.0, 5.0] {
        let samples = dispatch(&directions, &wet,time,0.,1.,-1.0);
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
    for (i,row) in dispatch(&directions, &pattern,0.,0.,1.,0.1).iter().take(24).enumerate() {
        for j in 0..3 {
            assert!((row[12+j]-(directions[i][j]*0.5+0.5)).abs()<0.003, "cube orientation mismatch: {i} {j}");
        }
    }
    // Shipping weather path: source centres and mesh anchoring cannot move
    // phase, opposite/changed winds change the sea, and swapping dominant
    // spectrum ranks only exchanges energy contributions.
    let weather = weather_state([
        12., 0., 3., 1., 0.418879, 0., 0., 0.45, 0., 0., 0.1570796, 0.3,
    ]);
    weather_fixture.replace(weather);
    let storm = dispatch(&directions, &wet, 1.2, 0., 1., 0.1);
    let anchor = dispatch(&directions, &wet, 1.2, 1., 1., 0.1);
    assert_eq!(storm, anchor, "weather waves moved with the mesh anchor");
    assert!(
        storm
            .iter()
            .any(|p| p[..3].iter().map(|v| v * v).sum::<f32>() > 0.01),
        "storm has no waves"
    );
    let swapped = weather_state([
        12., 0., 3., 1., 0., 0., 0.1570796, 0.3, 0.418879, 0., 0., 0.45,
    ]);
    weather_fixture.replace(swapped);
    let ranked = dispatch(&directions, &wet, 1.2, 0., 1., 0.1);
    for (a, b) in storm.iter().zip(&ranked) {
        for i in 0..12 {
            assert!(
                (a[i] - b[i]).abs() < 0.00001,
                "spectrum rank reset crest phase"
            );
        }
    }
    let opposing = weather_state([0.,0.,0.,1., 0.418879,0.,0.,0.45, -0.418879,0.,0.,0.45]);
    weather_fixture.replace(opposing);
    let opposing_reference=dispatch(&directions, &wet,1.2,0.,1.,0.1);
    for blend in [0.0,0.25,0.5,0.75,1.0] {
        let mut crossing=opposing;
        crossing[4..8].copy_from_slice(&opposing[8..12]);
        crossing[8..12].copy_from_slice(&opposing[4..8]);
        crossing[20]=blend;
        weather_fixture.replace(crossing);
        let interpolated=dispatch(&directions, &wet,1.2,0.,1.,0.1);
        for (a,b) in opposing_reference.iter().zip(interpolated) {
            for i in 0..12 {assert!((a[i]-b[i]).abs()<0.00001,"opposing swells cancelled during rank interpolation at {blend}");}
        }
    }
    weather_fixture.replace(weather);
    let close_time = dispatch(&directions, &wet, 1.2001, 0., 1., 0.1);
    assert!(
        storm
            .iter()
            .zip(&close_time)
            .all(|(a, b)| (a[0] - b[0]).abs() < 0.003),
        "weather phase is discontinuous"
    );
    for pair in storm[seam_start..seam_start + 10].chunks_exact(2) {
        for i in 0..7 {
            assert!(
                (pair[0][i] - pair[1][i]).abs() < 0.01,
                "weather wave seam or axis discontinuity"
            );
        }
    }
    let filtered = dispatch(&directions, &wet, 1.2, 0., 1., 100.);
    let energy = |samples: &[[f32; 16]]| {
        samples
            .iter()
            .map(|p| p[..3].iter().map(|v| v * v).sum::<f32>())
            .sum::<f32>()
    };
    assert!(
        energy(&filtered) < energy(&storm) * 0.02,
        "distant weather waves did not filter out"
    );
    assert!(
        filtered.iter().any(|p| p[8] > 0.),
        "filtered wave variance disappeared from shading"
    );
    for row in &storm {
        assert!(
            row.iter().all(|v| v.is_finite()),
            "storm nonfinite at a pole or axis"
        );
    }
    let mut reversed = weather;
    reversed[..3].copy_from_slice(&[-12., 0., -3.]);
    weather_fixture.replace(reversed);
    let opposite = dispatch(&directions, &wet, 1.2, 0., 1., 0.1);
    assert!(
        storm
            .iter()
            .zip(opposite)
            .any(|(a, b)| (a[0] - b[0]).abs() > 0.01),
        "wind direction did not change chop"
    );
    let mut calm = weather;
    calm[..3].fill(0.);
    weather_fixture.replace(calm);
    let swell_only = dispatch(&directions, &wet, 1.2, 0., 1., 0.1);
    assert!(
        swell_only
            .iter()
            .any(|p| p[..3].iter().map(|v| v * v).sum::<f32>() > 0.01),
        "persisting swell vanished with local wind"
    );
    weather_fixture.replace(weather_state([0., 0., 0., 1., 0., 0., 0., 0., 0., 0., 0., 0.]));
    for row in dispatch(&directions, &wet, 1.2, 0., 1., 0.1) {
        assert!(
            row[..3].iter().all(|v| v.abs() < 0.00001),
            "calm sea inherited manual sources"
        );
    }
    weather_fixture.replace(weather);
    for (i, row) in dispatch(&directions, &shallow, 2.2, 0., 3., 0.1).iter().enumerate() {
        let height: f32 = (0..3).map(|j| directions[i][j] * row[j]).sum();
        assert!(
            height.abs() < 5. / 25.5,
            "storm wave penetrated shallow seabed"
        );
    }
    for row in dispatch(&directions, &wet, 1.2, 0., 1., -3.0) {
        for j in 0..3 {
            assert!(
                (row[j] - row[j + 4]).abs() < 0.003,
                "bent-wave normal disagrees with actual phase derivative: {row:?}"
            );
        }
    }
    // Two equally energetic perpendicular swells on a uniform deep sea must
    // not turn into the same rectangular lattice everywhere. Measure the
    // fourfold angular harmonic of the actual GPU height spectrum. It detects
    // perpendicular repeated crest families independently of their rotation.
    const PATCH_SIZE: usize = 128;
    let patch: Vec<[f32; 3]> = (0..PATCH_SIZE * PATCH_SIZE)
        .map(|i| {
            let x = (i % PATCH_SIZE) as f32 - 63.5;
            let z = (i / PATCH_SIZE) as f32 - 63.5;
            let length = (x * x + 1_000_000.0 + z * z).sqrt();
            [x / length, 1000.0 / length, z / length]
        })
        .collect();
    weather_fixture.replace(weather_state([
        0., 0., 0., 1., 0.418879, 0., 0., 0.6, 0., 0., 0.418879, 0.6,
    ]));
    let field = dispatch(&patch, &wet, 1.2, 0., 1., 0.1);
    let heights: Vec<f32> = field
        .iter()
        .zip(&patch)
        .map(|(row, n)| (0..3).map(|j| row[j] * n[j]).sum())
        .collect();
    if let Some(path) = std::env::var_os("PLANET_OCEAN_PATTERN_PATH") {
        std::fs::write(path, bytes(&heights)).unwrap();
    }
    let mean = heights.iter().sum::<f32>() / heights.len() as f32;
    let rms = (heights.iter().map(|h| (h - mean) * (h - mean)).sum::<f32>() / heights.len() as f32)
        .sqrt();
    assert!(
        (0.45..0.85).contains(&rms),
        "grid reduction lost or amplified the sea energy: {rms}"
    );
    let fourfold = fourfold_spectrum(&heights, PATCH_SIZE);
    println!("Crossing-sea fourfold angular concentration: {fourfold:.5}");
    assert!(
        fourfold < 0.55,
        "crossing seas collapsed into a rectangular wave lattice: {fourfold}"
    );
    println!(
        "Weather ocean GPU PASS: anchored phases, spectrum-rank/mid-blend invariance, smooth time, directional chop, persistent swell, calm identity, cube/axis continuity, footprint filtering, shallow-bed bounds, nonlattice crossing seas and bent-wave derivatives."
    );
    println!("Ocean GPU PASS: anchored animation; dry/shallow/zero/far limits; finite poles; tangent slopes; all six hardware cube faces; spherical sine profile, radial symmetry, outward propagation and zero-height/zero-slope endpoints.");
}

// Separable DFT for this small diagnostic; no extra runtime/dependency state.
fn fourfold_spectrum(heights: &[f32], size: usize) -> f64 {
    let mean = heights.iter().map(|&x| f64::from(x)).sum::<f64>() / heights.len() as f64;
    let phases: Vec<[f64; 2]> = (0..size * size)
        .map(|i| {
            let (s, c) = (-std::f64::consts::TAU * (i / size) as f64 * (i % size) as f64
                / size as f64)
                .sin_cos();
            [c, s]
        })
        .collect();
    let window: Vec<f64> = (0..size)
        .map(|i| 0.5 - 0.5 * (std::f64::consts::TAU * i as f64 / (size - 1) as f64).cos())
        .collect();
    let mut rows = vec![[0.0; 2]; size * size];
    for y in 0..size {
        for k in 0..size {
            for x in 0..size {
                let value = (f64::from(heights[y * size + x]) - mean) * window[x] * window[y];
                rows[y * size + k][0] += value * phases[k * size + x][0];
                rows[y * size + k][1] += value * phases[k * size + x][1];
            }
        }
    }
    let mut harmonic = [0.0_f64; 2];
    let mut total = 0.0;
    for ky in 0..size {
        for kx in 0..size {
            let signed = |k: usize| {
                if k <= size / 2 {
                    k as f64
                } else {
                    k as f64 - size as f64
                }
            };
            let x = signed(kx);
            let y = signed(ky);
            let radius = x.hypot(y);
            // Exclude the DC/window lobe and sub-3 m ripple frequencies.
            if radius < 3.0 || radius > size as f64 / 3.0 {
                continue;
            }
            let mut value = [0.0; 2];
            for row in 0..size {
                let [a, b] = rows[row * size + kx];
                let [c, s] = phases[ky * size + row];
                value[0] += a * c - b * s;
                value[1] += a * s + b * c;
            }
            let energy = value[0] * value[0] + value[1] * value[1];
            let (s, c) = (4.0 * y.atan2(x)).sin_cos();
            harmonic[0] += energy * c;
            harmonic[1] += energy * s;
            total += energy;
        }
    }
    assert!(total > 0.01, "crossing-sea test lost its wave energy");
    harmonic[0].hypot(harmonic[1]) / total
}
