#import planet_bevy::waves::{ocean,waves_at,coast,allowance}
#import planet_bevy::weather::{weather_enabled,weather_lower}
struct FoamFrame {origin:vec4<f32>,tangent:vec4<f32>,bitangent:vec4<f32>}
struct Step {current:FoamFrame,previous:FoamFrame,parameters:vec4<f32>}
@group(0) @binding(0) var previous:texture_2d<f32>;
@group(0) @binding(1) var linear_sampler:sampler;
@group(0) @binding(2) var next:texture_storage_2d<rgba16float,write>;
@group(0) @binding(3) var<uniform> step:Step;
fn projection(n:vec3<f32>,frame:FoamFrame)->vec2<f32> {
 let scale=frame.tangent.w/(max(dot(n,frame.origin.xyz),0.00001)*2.0*frame.origin.w);
 return vec2<f32>(dot(n,frame.tangent.xyz),dot(n,frame.bitangent.xyz))*scale+0.5;
}
fn old(n:vec3<f32>)->vec4<f32>{
 if step.parameters.w>0.5 ||step.previous.origin.w<=0.0{return vec4<f32>(0.0);}
 let uv=projection(n,step.previous);
 if step.parameters.w>0.5 || any(uv<vec2<f32>(0.0))||any(uv>vec2<f32>(1.0))||dot(n,step.previous.origin.xyz)<=0.0 {return vec4<f32>(0.0);}
 return textureSampleLevel(previous,linear_sampler,uv,0.0);
}
@compute @workgroup_size(8,8) fn update(@builtin(global_invocation_id) id:vec3<u32>){
 let size=vec2<f32>(textureDimensions(next));let uv=(vec2<f32>(id.xy)+0.5)/size;
 let local=(uv-0.5)*2.0*step.current.origin.w;
 let n=normalize(step.current.origin.xyz+(step.current.tangent.xyz*local.x+step.current.bitangent.xyz*local.y)/ocean.shape.x);
 let wave=waves_at(n,0.5);let tangent=wave.displacement-n*dot(wave.displacement,n);
 let before=old(n);let dt=step.parameters.x;
 let c=coast(n);let wet=smoothstep(0.0,0.02,c.r*10.0);let water_active=smoothstep(0.0,0.1,allowance(c)*ocean.waves.x);
 if dt<=0.0 {
  textureStore(next,vec2<i32>(id.xy),vec4<f32>(select(0.0,before.r,wet>0.0&&water_active>0.0),tangent));return;
 }
 var velocity=(tangent-before.gba)/max(dt,0.001);
 velocity*=min(1.0,5.0/max(length(velocity),0.001));
 if step.parameters.w>0.5 {velocity=vec3<f32>(0.0);}
 let wind=select(vec3<f32>(0.08,0.0,0.025),weather_lower(n).xyz*0.025,weather_enabled());
 let drift=wind-n*dot(wind,n);
 let advected=normalize(n-(velocity+drift)*dt/ocean.shape.x);
 let foam=old(advected).r*exp(-dt/max(step.parameters.y,0.1))+wave.foam*(1.0-exp(-step.parameters.z*dt));
 textureStore(next,vec2<i32>(id.xy),vec4<f32>(clamp(foam,0.0,1.0)*wet*water_active,tangent));
}
