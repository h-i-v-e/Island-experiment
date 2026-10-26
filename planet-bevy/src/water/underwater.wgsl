#import bevy_core_pipeline::fullscreen_vertex_shader::FullscreenVertexOutput
#import bevy_render::view::View
#import planet_bevy::waves::{ocean,waves_at}
struct Submerge {parameters:vec4<f32>,absorption:vec4<f32>,scatter:vec4<f32>}
@group(0) @binding(0) var source:texture_2d<f32>;
@group(0) @binding(1) var linear_sampler:sampler;
@group(0) @binding(2) var scene_depth:texture_depth_2d;
@group(0) @binding(3) var<uniform> view:View;
@group(0) @binding(4) var<uniform> settings:Submerge;
fn world_position(uv:vec2<f32>,depth:f32)->vec3<f32>{
 let q=view.world_from_clip*vec4<f32>(uv*vec2<f32>(2.0,-2.0)+vec2<f32>(-1.0,1.0),depth,1.0);return q.xyz/q.w;
}
@fragment fn composite(input:FullscreenVertexOutput)->@location(0) vec4<f32> {
 let colour=textureSampleLevel(source,linear_sampler,input.uv,0.0);
 let radius=settings.parameters.x;
 let maximum=ocean.waves.x*0.81*ocean.shape.w;
 if (length(view.world_position)-1.0)*radius>maximum+0.1 {return colour;}
 let near_position=world_position(input.uv,1.0);
 var n=normalize(near_position);var displacement=vec3<f32>(0.0);
 for(var i=0u;i<4u;i+=1u) {
  displacement=waves_at(n,0.5).displacement*ocean.shape.w;
  let lateral=displacement-n*dot(displacement,n);
  n=normalize(near_position-lateral/radius);
 }
 let surface=length(n+displacement/radius);
 let signed_depth=(surface-length(near_position))*radius;
 let softness=max(settings.scatter.w,fwidth(signed_depth));
 let wet=smoothstep(-softness,softness,signed_depth);
 if wet<=0.0001 {return colour;}
 let size=vec2<i32>(textureDimensions(scene_depth));
 let pixel=clamp(vec2<i32>(input.uv*vec2<f32>(size)),vec2<i32>(0),size-1);
 let raw=textureLoad(scene_depth,pixel,0);
 let ray=normalize(near_position-view.world_position);
 var distance=settings.parameters.w;
 if raw>0.0 {distance=min(distance,length(world_position((vec2<f32>(pixel)+0.5)/vec2<f32>(size),raw)-near_position)*radius);}
 else {
  let b=dot(near_position,ray);let c=dot(near_position,near_position)-surface*surface;
  let discriminant=b*b-c;
  if discriminant>=0.0 {distance=min(distance,max(-b+sqrt(discriminant),0.0)*radius);}
 }
 let extinction=max(settings.absorption.rgb,vec3<f32>(0.0))*settings.absorption.a+settings.parameters.z;
 let transmission=exp(-extinction*max(distance,0.0));
 let water=colour.rgb*transmission+settings.scatter.rgb*(1.0-transmission);
 return vec4<f32>(mix(colour.rgb,water,wet),colour.a);
}
