#import planet_bevy::environment_material::{material_cloud_light,material_ambient_weight}
#import planet_bevy::weather::{weather_enabled,weather_debug_mode,weather_debug_colour,weather_lower,weather_swell}
// Coupled wind/swell waves, with manual sources as a fallback.
#import bevy_pbr::{forward_io::{Vertex,VertexOutput,FragmentOutput},mesh_functions,
 view_transformations::position_world_to_clip,mesh_view_bindings::{view,lights},
 pbr_fragment::pbr_input_from_standard_material,
 pbr_functions::{apply_pbr_lighting,main_pass_post_lighting_processing}}
#import planet_bevy::water::{WaterUniform,shade_water,water_shadow}
#import planet_bevy::water_impacts::{ImpactUniform,water_impacts}
#import planet_bevy::waves::{ocean,OceanOutput,waves_at,noise,coast,ocean_noise,noise_sampler}
@group(#{MATERIAL_BIND_GROUP}) @binding(105) var<uniform> optics:WaterUniform;
struct FoamFrame {origin:vec4<f32>,tangent:vec4<f32>,bitangent:vec4<f32>}
@group(#{MATERIAL_BIND_GROUP}) @binding(106) var foam_history:texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(107) var foam_sampler:sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(108) var<uniform> foam_frame:FoamFrame;
@group(#{MATERIAL_BIND_GROUP}) @binding(109) var<uniform> contacts:ImpactUniform;
fn ripple_gradient(uv:vec3<f32>,lod:f32)->vec3<f32>{
 let d=2.0/64.0;
 return vec3<f32>(
  textureSampleLevel(ocean_noise,noise_sampler,uv+vec3<f32>(d,0.0,0.0),lod).r-textureSampleLevel(ocean_noise,noise_sampler,uv-vec3<f32>(d,0.0,0.0),lod).r,
  textureSampleLevel(ocean_noise,noise_sampler,uv+vec3<f32>(0.0,d,0.0),lod).r-textureSampleLevel(ocean_noise,noise_sampler,uv-vec3<f32>(0.0,d,0.0),lod).r,
  textureSampleLevel(ocean_noise,noise_sampler,uv+vec3<f32>(0.0,0.0,d),lod).r-textureSampleLevel(ocean_noise,noise_sampler,uv-vec3<f32>(0.0,0.0,d),lod).r);
}
@vertex fn vertex(input:Vertex)->OceanOutput {
 var out:OceanOutput;
 let transform=mesh_functions::get_world_from_local(input.instance_index);
 let root=mesh_functions::mesh_position_local_to_world(transform,vec4<f32>(input.position,1.0)).xyz;
 let n=normalize(root);
 let fade=(1.0-smoothstep(ocean.shape.y,ocean.shape.z,input.uv.x))*ocean.shape.w;
 var displacement=vec3<f32>(0.0);
 if fade>0.0 {displacement=waves_at(n,input.uv.y).displacement*fade;}
 let world=n+displacement/ocean.shape.x;
 out.position=position_world_to_clip(world);out.world_position=vec4<f32>(world,1.0);
 out.world_normal=n;out.base_direction=n;out.height=dot(displacement,n);out.instance_index=input.instance_index;return out;
}
@fragment fn fragment(input:OceanOutput,@builtin(front_facing) front:bool)->FragmentOutput {
 let n=normalize(input.base_direction);let p=n*ocean.shape.x;
 let footprint=max(length(dpdx(p)),length(dpdy(p)));
 let wave=waves_at(n,footprint);
 let travel=vec3<f32>(ocean.viewer.w*1.08,0.0,ocean.viewer.w*0.2);
 let ripple_span=1.2*16.0; // The 64³ volume has 16 coherent cells per repeat.
 let ripple_lod=max(1.0,log2(max(64.0*footprint/(ripple_span*0.62),1.0)));
 let c=0.9063078;let s=0.4226183;
 let rotation=mat3x3<f32>(vec3<f32>(c,0.0,-s),vec3<f32>(0.0,1.0,0.0),vec3<f32>(s,0.0,c));
 let first=ripple_gradient((p-travel)/ripple_span,ripple_lod);
 let second=transpose(rotation)*ripple_gradient(rotation*(p-travel*1.67)/(ripple_span*0.62)+vec3<f32>(0.371,0.619,0.23),ripple_lod);
 let fine=first+second*0.25;
 let ripple=fine-n*dot(fine,n);
 let resolved=1.0-smoothstep(0.36,2.4,footprint);
 var normal=normalize(n-wave.slope-ripple*(0.12*resolved*1.5));
 let impact=water_impacts(n,ocean.shape.x,footprint,contacts);
 normal=normalize(normal+impact.xyz);
 let view_direction=normalize(view.world_position-input.world_position.xyz);
 let face_normal=normalize(cross(dpdx(input.world_position.xyz),dpdy(input.world_position.xyz)));
 let geometric=select(-face_normal,face_normal,dot(face_normal,view_direction)>=0.0);
 normal=select(-normal,normal,dot(normal,view_direction)>=0.0);
 var vertex_out:VertexOutput;
 vertex_out.position=input.position;vertex_out.world_position=input.world_position;vertex_out.world_normal=n;
#ifdef VERTEX_UVS_A
 vertex_out.uv=vec2<f32>(0.0);
#endif
#ifdef VERTEX_OUTPUT_INSTANCE_INDEX
 vertex_out.instance_index=input.instance_index;
#endif
 var pbr=pbr_input_from_standard_material(vertex_out,front);pbr.N=normal;
 let shallow=1.0-smoothstep(0.3,8.0,wave.depth);
 let deep=vec3<f32>(0.002322,0.063724,0.263273);
 let coastal=vec3<f32>(0.008,0.12,0.14);
 let water=mix(deep,coastal,shallow*0.65);
 let foam_scale=foam_frame.tangent.w/(max(dot(n,foam_frame.origin.xyz),0.00001)*2.0*max(foam_frame.origin.w,0.001));
 let foam_uv=vec2<f32>(dot(n,foam_frame.tangent.xyz),dot(n,foam_frame.bitangent.xyz))*foam_scale+0.5;
 let foam_radius=length((foam_uv-0.5)*2.0);
 let stored_foam=textureSampleLevel(foam_history,foam_sampler,clamp(foam_uv,vec2<f32>(0.0),vec2<f32>(1.0)),0.0).r
     *(1.0-smoothstep(0.5,1.0,foam_radius))*optics.effects.x*select(0.0,1.0,dot(n,foam_frame.origin.xyz)>0.0);
 let foam=clamp(max(max(wave.foam,stored_foam),impact.w),0.0,1.0);
 pbr.material.base_color=vec4<f32>(water,1.0);
 pbr.material.specular_transmission=0.0;
 pbr.material.reflectance=vec3<f32>(0.0);
 pbr.material.perceptual_roughness=1.0;
 pbr.material.emissive=vec4<f32>(0.0);
 let shadow=water_shadow(input.world_position.xyz,n,input.position.xy);
 let backlight=pow(max(dot(view_direction,-lights.directional_lights[0].direction_to_light),0.0),4.0);
 let face_weight=pow(1.0-max(dot(normal,view_direction),0.0),2.0);
 let daylight=smoothstep(0.0,0.1,dot(n,lights.directional_lights[0].direction_to_light));
 let ambient=material_ambient_weight(input.world_position.xyz);
 pbr.diffuse_occlusion*=vec3<f32>(ambient);pbr.specular_occlusion*=ambient;
 let crest_light=vec3<f32>(0.006,0.264,0.147)*wave.crest*face_weight*(0.03*ambient+6.0*backlight*shadow*daylight)*lights.directional_lights[0].color.rgb*view.exposure*0.08;
 let cloud_light=material_cloud_light(input.world_position.xyz);
 let lit_body=apply_pbr_lighting(pbr).rgb*cloud_light+crest_light;
 let analytic=normalize(n-wave.slope);
 let distortion=noise((p-vec3<f32>(ocean.viewer.w*0.20,0.0,ocean.viewer.w*0.16))/2.0)-0.5;
 let detail=(view.view_from_world*vec4<f32>(normal-analytic,0.0)).xy;
 let optical_ripple=detail+distortion.rg;
 let normal_variance=max(dot(dpdx(normal),dpdx(normal)),dot(dpdy(normal),dpdy(normal)));
 let roughness=clamp(sqrt(pow(ocean.shading.x+0.09,2.0)+min(wave.variance,0.35)+0.007*(1.0-resolved*resolved)+min(normal_variance,0.2)),0.04,0.8);
 var optical_settings=optics;optical_settings.shape.z*=ocean.shading.y/0.8;
 let shaded=shade_water(input.world_position.xyz,input.position.xy,normal,geometric,n,view_direction,optical_ripple,lit_body,roughness,optical_settings,true,!front);
 let coastal_tint=(1.0-clamp(wave.depth/10.0,0.0,1.0))*mix(1.0,0.35,clamp(wave.distance/16.0,0.0,1.0))*optics.effects.w;
 pbr.material.base_color=vec4<f32>(0.79,0.91,1.0,1.0);
 let lit_foam=apply_pbr_lighting(pbr).rgb*cloud_light;
 let colour=mix(mix(shaded.colour,lit_body,coastal_tint),lit_foam,foam);
 var out:FragmentOutput;
 out.color=main_pass_post_lighting_processing(pbr,vec4<f32>(colour,1.0));
 if ocean.shading.z>0.5 && ocean.shading.z<1.5 {out.color=vec4<f32>(vec3<f32>(wave.depth/10.0),1.0);}
 if ocean.shading.z>1.5 && ocean.shading.z<2.5 {out.color=vec4<f32>(vec3<f32>(wave.distance/128.0),1.0);}
 if ocean.shading.z>2.5 {
  let colours=array<vec3<f32>,4>(vec3<f32>(1.0,0.15,0.05),vec3<f32>(0.1,0.8,1.0),vec3<f32>(0.5,1.0,0.1),vec3<f32>(0.8,0.2,1.0));
  var colour=vec3<f32>(0.015,0.025,0.04);
  for(var i=0u;i<4u;i+=1u) {
   let angle=atan2(length(cross(n,ocean.sources[i].xyz)),dot(n,ocean.sources[i].xyz));
   let band=sin(3.14159265359*clamp(angle,0.0,1.0));
   let marker=1.0-smoothstep(0.0,max(4.0,footprint*2.0),angle*ocean.shape.x);
   let edge=1.0-smoothstep(max(1.0,footprint),max(2.0,footprint*2.0),abs(angle-1.0)*ocean.shape.x);
   colour+=colours[i]*(band*band*0.3+marker+edge*0.5);
  }
  out.color=vec4<f32>(clamp(colour,vec3<f32>(0.0),vec3<f32>(1.0)),1.0);
 }
 if ocean.shading.z>2.5 && weather_enabled() {
  let drivers=vec2<f32>(length(weather_lower(n).xyz)/20.0,(weather_swell(n,0u).w+weather_swell(n,1u).w)/2.0);
  out.color=vec4<f32>(clamp(vec3<f32>(drivers.x,drivers.y,1.0-drivers.x),vec3<f32>(0.0),vec3<f32>(1.0)),1.0);
 }
 if weather_enabled() && weather_debug_mode()>0u {out.color=vec4<f32>(weather_debug_colour(input.world_position.xyz),1.0);}
 return out;
}
