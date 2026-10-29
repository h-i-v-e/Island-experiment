#import planet_bevy::weather::weather_wind
#import planet_bevy::environment_material::environment
#import bevy_pbr::{forward_io::{VertexOutput,FragmentOutput},mesh_functions::get_world_from_local,mesh_view_bindings::{view,lights}}
#import planet_bevy::water::{scene_point,water_shadow,view_uv}
struct MistUniform {shape:vec4<f32>,timing:vec4<f32>}
@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> mist:MistUniform;
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var mist_noise:texture_3d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(2) var mist_sampler:sampler;
fn volume_noise(p:vec3<f32>)->f32 {return textureSampleLevel(mist_noise,mist_sampler,p/16.0,1.0).r;}
@fragment fn fragment(input:VertexOutput)->FragmentOutput {
 let transform=get_world_from_local(input.instance_index);
 let scale=vec3<f32>(length(transform[0].xyz),length(transform[1].xyz),length(transform[2].xyz));
 let basis=mat3x3<f32>(transform[0].xyz/scale.x,transform[1].xyz/scale.y,transform[2].xyz/scale.z);
 let local_from_world=transpose(basis);
 let origin=(local_from_world*(view.world_position-transform[3].xyz))/scale;
 let local=(local_from_world*(input.world_position.xyz-transform[3].xyz))/scale;
 let ray=normalize(local-origin);
 let safe=sign(ray+vec3<f32>(0.00000001))*max(abs(ray),vec3<f32>(0.00001));
 let a=(-vec3<f32>(0.5)-origin)/safe;let b=(vec3<f32>(0.5)-origin)/safe;
 let near3=min(a,b);let far3=max(a,b);
 let near=max(max(near3.x,near3.y),max(near3.z,0.0));let far=min(far3.x,min(far3.y,far3.z));
 if far<=near {discard;}
 let step=(far-near)/28.0;var optical=0.0;
 let background=scene_point(view_uv(input.position.xy),mist.shape.x);
 for(var i=0u;i<28u;i+=1u) {
  let point=origin+ray*(near+(f32(i)+0.5)*step);
  let world=transform[3].xyz+basis*(point*scale);
  let eye=-(view.view_from_world*vec4<f32>(world,1.0)).z*mist.shape.x;
  if background.valid>0.0 &&eye>=background.eye {continue;}
  let q=point*3.2+vec3<f32>(environment.air.w*0.035,environment.air.w*0.055,-environment.air.w*0.025)-transpose(basis)*weather_wind(world)*environment.air.w*0.012;
  let broad=volume_noise(q);let detail=volume_noise(q*2.13+7.1);
  let warp=vec3<f32>(volume_noise(q*0.57+11.7),volume_noise(q*0.51-5.3),volume_noise(q*0.63+23.1))-0.5;
  let footprint=(point.xz+warp.xz*vec2<f32>(0.055,0.085))*2.0;
  let horizontal=smoothstep(-0.16,0.16,1.0-pow(abs(footprint.x),6.0)-pow(abs(footprint.y),4.0)+(broad-0.5)*0.28);
  let foot_edge=1.0-smoothstep(0.04,0.96,clamp((point.z+0.5)*(1.0+(broad-0.5)*0.22+warp.z*0.16),0.0,1.0));
  let h=clamp(point.y+0.5,0.0,1.0);
  let rising=volume_noise(vec3<f32>(q.x*0.72,q.z*0.72,environment.air.w*0.075));
  let ceiling=mix(0.32,1.02,smoothstep(0.14,0.86,rising));
  let top=1.0-smoothstep(ceiling-0.15,ceiling+0.035,h);
  let threshold=mix(0.18,0.61,h);let water_coherent=broad*0.52+detail*0.25+rising*0.23;
  let wisps=smoothstep(threshold,min(threshold+0.24,0.9),water_coherent);
  let blanket=1.0-smoothstep(0.08,0.34,h);
  optical+=horizontal*foot_edge*top*mix(0.04,1.75,pow(1.0-h,1.55))*mix(1.0,1.35,blanket)
      *mix(wisps,0.92+broad*0.2,blanket)*step;
 }
 let alpha=clamp((1.0-exp(-optical*mist.shape.y))*0.68,0.0,0.85);
 let up=normalize(input.world_position.xyz);
 let daylight=0.03+0.97*smoothstep(-0.08,0.2,dot(up,lights.directional_lights[0].direction_to_light));
 let shadow=water_shadow(input.world_position.xyz,up,input.position.xy);
 let colour=vec3<f32>(0.78,0.86,0.89)*(0.72+shadow*0.72)*daylight;
 return FragmentOutput(vec4<f32>(colour,alpha));
}
