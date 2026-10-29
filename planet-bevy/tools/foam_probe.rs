//! Execute the shipping history kernel: accumulation, pause, masks and spherical reprojection.
use std::{borrow::Cow,sync::mpsc,time::Duration};
use wgpu::util::DeviceExt;
const SIZE:u32=64;
fn bytes(v:&[f32])->Vec<u8>{v.iter().flat_map(|v|v.to_le_bytes()).collect()}
fn main(){
    let args:Vec<_>=std::env::args_os().collect();
    let waves=std::fs::read_to_string(&args[1]).unwrap();
    let waves=format!("struct OceanUniform{}",waves.split_once("struct OceanUniform").unwrap().1).replace("#{MATERIAL_BIND_GROUP}","0");
    let path=std::path::PathBuf::from(&args[1]);
    let material=std::fs::read_to_string(path.parent().unwrap().parent().unwrap().join("environment/material.wgsl")).unwrap();
    let shared_sampler=material.lines().find(|line|line.contains("@binding(102)")).unwrap().replace("#{MATERIAL_BIND_GROUP}","0");
    let waves=waves+"\n"+&shared_sampler+"\nfn weather_blend()->f32{return 1.0;}\nfn weather_swell_previous(n:vec3<f32>,i:u32)->vec4<f32>{return vec4<f32>(0.0);}\nfn weather_enabled()->bool{return false;}\nfn weather_lower(n:vec3<f32>)->vec4<f32>{return vec4<f32>(0.0); }\nfn weather_swell(n:vec3<f32>,i:u32)->vec4<f32>{return vec4<f32>(0.0); }\n";
    let foam=std::fs::read_to_string(&args[2]).unwrap().lines().filter(|l|!l.starts_with("#import")).collect::<Vec<_>>().join("\n");
    let instance=wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter=bevy_tasks::block_on(instance.request_adapter(&Default::default())).unwrap();
    let (device,queue)=bevy_tasks::block_on(adapter.request_device(&Default::default())).unwrap();
    let shader=device.create_shader_module(wgpu::ShaderModuleDescriptor{label:Some("shipping_foam_history"),source:wgpu::ShaderSource::Wgsl(Cow::Owned(waves+&foam))});
    let pipeline=device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor{label:None,layout:None,module:&shader,entry_point:Some("update"),compilation_options:Default::default(),cache:None});
    let texture=|dimension,width,height,depth,format,usage|device.create_texture(&wgpu::TextureDescriptor{label:None,size:wgpu::Extent3d{width,height,depth_or_array_layers:depth},mip_level_count:1,sample_count:1,dimension,format,usage,view_formats:&[]});
    let sampled=wgpu::TextureUsages::TEXTURE_BINDING|wgpu::TextureUsages::COPY_DST;
    let cube=texture(wgpu::TextureDimension::D2,1,1,6,wgpu::TextureFormat::Rgba8Unorm,sampled);
    let volume=device.create_texture(&wgpu::TextureDescriptor{label:None,size:wgpu::Extent3d{width:64,height:64,depth_or_array_layers:64},mip_level_count:7,sample_count:1,dimension:wgpu::TextureDimension::D3,format:wgpu::TextureFormat::Rgba8Unorm,usage:sampled,view_formats:&[]});
    for mip in 0..7 {
        let size=64>>mip;
        queue.write_texture(wgpu::TexelCopyTextureInfo{texture:&volume,mip_level:mip,origin:wgpu::Origin3d::ZERO,aspect:wgpu::TextureAspect::All},&vec![128;(size*size*size*4) as usize],wgpu::TexelCopyBufferLayout{offset:0,bytes_per_row:Some(size*4),rows_per_image:Some(size)},wgpu::Extent3d{width:size,height:size,depth_or_array_layers:size});
    }
    let cube_view=cube.create_view(&wgpu::TextureViewDescriptor{dimension:Some(wgpu::TextureViewDimension::Cube),..Default::default()});
    let noise_view=volume.create_view(&Default::default());
    let textures=[0,1].map(|_|texture(wgpu::TextureDimension::D2,SIZE,SIZE,1,wgpu::TextureFormat::Rgba16Float,sampled|wgpu::TextureUsages::STORAGE_BINDING|wgpu::TextureUsages::COPY_SRC));
    let views=textures.each_ref().map(|t|t.create_view(&Default::default()));
    let sampler=device.create_sampler(&wgpu::SamplerDescriptor{mag_filter:wgpu::FilterMode::Linear,min_filter:wgpu::FilterMode::Linear,..Default::default()});
    let ocean=device.create_buffer_init(&wgpu::util::BufferInitDescriptor{label:None,contents:&bytes(&[
        1000.,192.,256.,1.,2.,1.,0.18,0.85,0.16,0.8,0.,1024.,0.,1.,0.,0.,0.,1.,0.,0.7,1.,0.,0.,0.4,0.,0.,1.,1.2,-1.,0.,0.,2.]),usage:wgpu::BufferUsages::UNIFORM|wgpu::BufferUsages::COPY_DST});
    let river=std::cell::Cell::new(0u8);
    let frame=[0.,1.,0.,128.,1.,0.,0.,1000.,0.,0.,1.,0.];
    let readback=device.create_buffer(&wgpu::BufferDescriptor{label:None,size:u64::from(SIZE*SIZE)*8,usage:wgpu::BufferUsages::COPY_DST|wgpu::BufferUsages::MAP_READ,mapped_at_creation:false});
    let run=|previous:usize,current:[f32;12],old:[f32;12],dt:f32,reset:bool,depth:u8|->Vec<f32>{
        // 19.6 m from land: the crest of the 24 m asymmetric breaker at t=0.
        let coast:Vec<_>=(0..6).flat_map(|_|[depth,39,river.get(),255]).collect();
        queue.write_texture(cube.as_image_copy(),&coast,wgpu::TexelCopyBufferLayout{offset:0,bytes_per_row:Some(4),rows_per_image:Some(1)},cube.size());
        let mut values=current.to_vec();values.extend(old);values.extend([dt,6.,2.,if reset{1.}else{0.}]);
        let step=device.create_buffer_init(&wgpu::util::BufferInitDescriptor{label:None,contents:&bytes(&values),usage:wgpu::BufferUsages::UNIFORM});
        let next=1-previous;
        let entries=[
            wgpu::BindGroupEntry{binding:0,resource:wgpu::BindingResource::TextureView(&views[previous])},
            wgpu::BindGroupEntry{binding:1,resource:wgpu::BindingResource::Sampler(&sampler)},
            wgpu::BindGroupEntry{binding:2,resource:wgpu::BindingResource::TextureView(&views[next])},
            wgpu::BindGroupEntry{binding:3,resource:step.as_entire_binding()},
            wgpu::BindGroupEntry{binding:100,resource:ocean.as_entire_binding()},
            wgpu::BindGroupEntry{binding:101,resource:wgpu::BindingResource::TextureView(&cube_view)},
            wgpu::BindGroupEntry{binding:102,resource:wgpu::BindingResource::Sampler(&sampler)},
            wgpu::BindGroupEntry{binding:103,resource:wgpu::BindingResource::TextureView(&noise_view)},
            wgpu::BindGroupEntry{binding:104,resource:wgpu::BindingResource::Sampler(&sampler)},
        ];
        let group=device.create_bind_group(&wgpu::BindGroupDescriptor{label:None,layout:&pipeline.get_bind_group_layout(0),entries:&entries});
        let mut encoder=device.create_command_encoder(&Default::default());
        {let mut pass=encoder.begin_compute_pass(&Default::default());pass.set_pipeline(&pipeline);pass.set_bind_group(0,&group,&[]);pass.dispatch_workgroups(SIZE/8,SIZE/8,1);}
        encoder.copy_texture_to_buffer(textures[next].as_image_copy(),wgpu::TexelCopyBufferInfo{buffer:&readback,layout:wgpu::TexelCopyBufferLayout{offset:0,bytes_per_row:Some(SIZE*8),rows_per_image:Some(SIZE)}},textures[next].size());
        queue.submit([encoder.finish()]);
        let (tx,rx)=mpsc::channel();readback.slice(..).map_async(wgpu::MapMode::Read,move|r|{tx.send(r).unwrap();});
        device.poll(wgpu::PollType::Wait{submission_index:None,timeout:Some(Duration::from_secs(10))}).unwrap();rx.recv().unwrap().unwrap();
        let data=readback.slice(..).get_mapped_range();
        let result=data.chunks_exact(8).map(|p|half::f16::from_bits(u16::from_le_bytes([p[0],p[1]])).to_f32()).collect::<Vec<_>>();
        drop(data);readback.unmap();assert!(result.iter().all(|v|v.is_finite()&&*v>=0.&&*v<=1.));result
    };
    let mean=|data:&[f32]|data.iter().sum::<f32>()/data.len() as f32;
    let first=run(0,frame,frame,0.1,true,26);assert!(mean(&first)>0.01,"no foam deposit: {}",mean(&first));
    let mut previous=1;let mut grown=first.clone();
    for _ in 0..9 {grown=run(previous,frame,frame,0.1,false,26);previous=1-previous;}
    assert!(mean(&grown)>mean(&first)*2.,"foam did not accumulate");
    let paused=run(previous,frame,frame,0.,false,13);previous=1-previous;
    assert!((mean(&paused)-mean(&grown)).abs()<0.0001,"paused weak coastal activity erased history");
    let reset=run(previous,frame,frame,0.1,true,26);previous=1-previous;
    assert!((mean(&reset)-mean(&first)).abs()<0.0001,"reset retained old foam");
    let dry=run(previous,frame,frame,0.1,false,0);
    assert!(mean(&dry)<0.00001,"dry terrain retained foam");

    river.set(255);
    let mouth=run(previous,frame,frame,0.1,false,26);
    assert!(mean(&mouth)<0.00001,"river mouth retained sea foam");river.set(0);
    queue.write_buffer(&ocean,16,&bytes(&[0.1]));
    let quiet=run(previous,frame,frame,0.1,false,26);
    assert!(mean(&quiet)<0.00001,"sub-threshold waves retained sea foam");
    queue.write_buffer(&ocean,16,&bytes(&[2.]));

    // A known foam patch must stay at its world position when the viewer moves.
    let mut blob=Vec::new();
    for y in 0..SIZE {for x in 0..SIZE {
        let d=(x as f32+0.5-SIZE as f32*0.5).powi(2)+(y as f32+0.5-SIZE as f32*0.5).powi(2);
        for value in [0.8*(-d/32.).exp(),0.,0.,0.] {blob.extend(half::f16::from_f32(value).to_bits().to_le_bytes());}
    }}
    queue.write_texture(textures[0].as_image_copy(),&blob,wgpu::TexelCopyBufferLayout{offset:0,bytes_per_row:Some(SIZE*8),rows_per_image:Some(SIZE)},textures[0].size());
    let angle=0.017453292_f32;let (sin,cos)=angle.sin_cos();
    let moved=[sin,cos,0.,128.,cos,-sin,0.,1000.,0.,0.,1.,0.];
    let reprojected=run(0,moved,frame,0.,false,26);
    let peak=reprojected.iter().enumerate().max_by(|a,b|a.1.total_cmp(b.1)).unwrap().0;
    let expected_x=(0.5-1000.*angle.tan()/256.)*SIZE as f32-0.5;
    assert!(((peak as u32%SIZE) as f32-expected_x).abs()<2.,"foam followed the camera");
    assert!(reprojected[peak]>0.7);
    // Fine source gaps must survive accumulation. At mip 2 this periodic
    // pattern is exactly grey, reproducing the old blurred deposition bug.
    for mip in 0..2 {
        let size=64>>mip;let stripe=4>>mip;
        let pixels:Vec<u8>=(0..size*size*size).flat_map(|i| {
            let v=if i%size%stripe<stripe/2 {0}else{255};[v,v,v,255]
        }).collect();
        queue.write_texture(wgpu::TexelCopyTextureInfo{texture:&volume,mip_level:mip,origin:wgpu::Origin3d::ZERO,aspect:wgpu::TextureAspect::All},&pixels,wgpu::TexelCopyBufferLayout{offset:0,bytes_per_row:Some(size*4),rows_per_image:Some(size)},wgpu::Extent3d{width:size,height:size,depth_or_array_layers:size});
    }
    let mut detailed=run(0,frame,frame,0.1,true,51);let mut previous=1;
    for _ in 0..9 {detailed=run(previous,frame,frame,0.1,false,51);previous=1-previous;}
    let gaps=detailed.iter().filter(|&&v|v<0.0001).count();
    assert!(gaps>detailed.len()/10,"history filled fine noise gaps: {gaps}");
    assert!(detailed.iter().any(|&v|v>0.1),"history detail test lost foam entirely");
    println!("Foam GPU PASS: deposit, accumulation, paused weak-coast preservation, reset, dry/river/low-wave masks, detailed deposition and spherical world-anchored reprojection.");
}
