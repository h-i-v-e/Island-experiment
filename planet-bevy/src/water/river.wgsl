#define_import_path planet_bevy::river
#import planet_bevy::environment_material::material_cloud_light
#import planet_bevy::water::{WaterUniform,shade_water,water_shadow}
#import bevy_pbr::{forward_io::VertexOutput,pbr_types::PbrInput,pbr_functions::apply_pbr_lighting,mesh_view_bindings::{view,lights}}
@group(#{MATERIAL_BIND_GROUP}) @binding(120) var<uniform> water:WaterUniform;
@group(#{MATERIAL_BIND_GROUP}) @binding(121) var river_noise:texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(122) var river_sampler:sampler;
fn noise(uv:vec2<f32>,lod:f32)->vec2<f32> {return textureSampleLevel(river_noise,river_sampler,uv,lod).rg;}
fn filtered_noise(uv:vec2<f32>)->vec2<f32>{
 let texels=max(length(dpdx(uv)),length(dpdy(uv)))*256.0;
 return noise(uv,max(0.0,log2(max(texels,1.0))));
}
fn noise_slope(uv:vec2<f32>,fine:bool)->vec2<f32>{
 let dx=dpdx(uv)*256.0;let dy=dpdy(uv)*256.0;
 let lod=max(1.0,0.5*log2(max(max(dot(dx,dx),dot(dy,dy)),1.0)));
 let step=2.0/256.0;
 let u=noise(uv+vec2<f32>(step,0.0),lod)-noise(uv-vec2<f32>(step,0.0),lod);
 let v=noise(uv+vec2<f32>(0.0,step),lod)-noise(uv-vec2<f32>(0.0,step),lod);
 return select(vec2<f32>(u.r,v.r),vec2<f32>(u.g,v.g),fine);
}
fn river_water(input:VertexOutput,pbr:PbrInput,front:bool)->vec4<f32> {
 let position=input.world_position.xyz;let metres=position*water.shape.x;
 let up=normalize(position);let base=normalize(pbr.world_normal);
 let rapids=smoothstep(0.05,0.55,1.0-abs(dot(base,up)));
#ifdef VERTEX_UVS_A
 let river_metres=input.uv;
#else
 let river_metres=vec2<f32>(0.0);
#endif
 let dx=dpdx(metres);let dy=dpdy(metres);
 let du=dpdx(river_metres);let dv=dpdy(river_metres);
 let across=cross(dy,base);let along=cross(base,dx);
 let determinant=dot(dx,across);let orientation=select(-1.0,1.0,determinant>=0.0);
 let ug=across*du.x+along*dv.x;let vg=across*du.y+along*dv.y;
 let scale=orientation/max(max(length(ug),length(vg)),0.000000000001);
 let coarse_uv=(river_metres-vec2<f32>(0.0,water.shape.y*0.8))/(1.2*32.0);
 let fine_uv=(river_metres-vec2<f32>(0.0,water.shape.y*1.4))/(0.65*32.0)+vec2<f32>(0.371,0.619);
 let footprint=max(length(dx),length(dy));
 let resolved=1.0-smoothstep(vec2<f32>(1.2,0.65)*0.3,vec2<f32>(1.2,0.65)*2.0,vec2<f32>(footprint));
 let slope=noise_slope(coarse_uv,false)*resolved.x+noise_slope(fine_uv,true)*resolved.y*0.25;
 let strength=water.river.x*mix(1.0,1.5,rapids);
 var normal=normalize(base-(ug*slope.x+vg*slope.y)*scale*strength*1.5);
 let v=normalize(view.world_position-position);
 let face_normal=normalize(cross(dx,dy));
 let geometric=select(-face_normal,face_normal,dot(face_normal,v)>=0.0);
 normal=select(-normal,normal,dot(normal,v)>=0.0);
 let variance=max(dot(dpdx(normal),dpdx(normal)),dot(dpdy(normal),dpdy(normal)));
 let roughness=clamp(sqrt(pow(water.river.w+rapids*0.18,2.0)+strength*strength*(1.0-resolved.y*resolved.y)*0.5+min(variance,0.2)),0.04,0.8);
 let falling=vec2<f32>(filtered_noise((river_metres-vec2<f32>(0.0,water.shape.y*water.river.z*0.25))/6.0).r,
     filtered_noise((river_metres-vec2<f32>(0.0,water.shape.y*water.river.z))/3.0).g);
 let calm=vec2<f32>(filtered_noise(coarse_uv).r,filtered_noise(fine_uv).g);
 let distortion=mix(calm,falling,rapids)-0.5;
 let span=0.7*32.0;
 let water_patch=filtered_noise((river_metres-vec2<f32>(0.0,water.shape.y*0.8))/vec2<f32>(span,span*3.0)).r;
 let detail=filtered_noise((river_metres-vec2<f32>(0.0,water.shape.y*1.4))/(span*0.65)+vec2<f32>(0.173,0.419)).g;
 let patches=smoothstep(0.43,0.72,water_patch*0.8+detail*0.2);
 let falling_foam=smoothstep(0.42,0.68,falling.x)*0.03+smoothstep(0.24,0.80,falling.y)*0.65;
 let foam=clamp(mix(patches*0.025,max(patches*0.55,falling_foam),rapids)*water.river.y,0.0,1.0);
 let estuary=1.0-smoothstep(0.0,2.0,max((length(position)-1.0)*water.shape.x,0.0));
 let body=vec3<f32>(0.002322,0.063724,0.263273);
 var lit=pbr;lit.N=normal;lit.material.base_color=vec4<f32>(body,1.0);lit.material.specular_transmission=0.0;
 lit.material.perceptual_roughness=1.0;lit.material.reflectance=vec3<f32>(0.0);
 let cloud_light=material_cloud_light(position);
 let lit_body=apply_pbr_lighting(lit).rgb*cloud_light;
 let ripple=(view.view_from_world*vec4<f32>(normal-base,0.0)).xy+distortion;
 var parameters=water;
 let estuary_surface=estuary*smoothstep(0.65,0.96,abs(dot(base,up)));
 parameters.absorption=mix(water.absorption,vec4<f32>(0.45,0.12,0.055,water.absorption.a),estuary_surface);
 parameters.optics.x=mix(water.optics.x,0.65,estuary_surface);
 let optics=shade_water(position,input.position.xy,normal,geometric,up,v,ripple,lit_body,roughness,parameters,false,!front);
 lit.material.base_color=vec4<f32>(0.85,0.93,1.0,1.0);
 let lit_foam=apply_pbr_lighting(lit).rgb*cloud_light;
 return vec4<f32>(mix(optics.colour,lit_foam,foam),1.0);
}
