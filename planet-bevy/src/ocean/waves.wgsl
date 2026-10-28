#define_import_path planet_bevy::waves
#import planet_bevy::environment_material::environment_sky_sampler
struct OceanUniform {shape:vec4<f32>,waves:vec4<f32>,shading:vec4<f32>,viewer:vec4<f32>,sources:array<vec4<f32>,4>}
@group(#{MATERIAL_BIND_GROUP}) @binding(100) var<uniform> ocean:OceanUniform;
@group(#{MATERIAL_BIND_GROUP}) @binding(101) var coast_map:texture_cube<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(103) var ocean_noise:texture_3d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(104) var noise_sampler:sampler;
struct OceanOutput {
 @builtin(position) position:vec4<f32>,
 @location(0) world_position:vec4<f32>,
 @location(1) world_normal:vec3<f32>,
 @location(2) base_direction:vec3<f32>,
 @location(3) height:f32,
 @location(4) @interpolate(flat) instance_index:u32,
}
struct Waves {displacement:vec3<f32>,slope:vec3<f32>,crest:f32,foam:f32,variance:f32,depth:f32,distance:f32}
const COMPONENTS=array<vec2<f32>,4>(vec2<f32>(30.0,0.34),vec2<f32>(15.0,0.18),vec2<f32>(7.5,0.09),vec2<f32>(4.0,0.04));
const SPEEDS=array<f32,4>(3.6,2.8,2.1,1.5);
fn coast(n:vec3<f32>)->vec4<f32> {return textureSampleLevel(coast_map,environment_sky_sampler,n,0.0);}
fn noise(p:vec3<f32>)->vec3<f32> {return textureSampleLevel(ocean_noise,noise_sampler,p,0.0).rgb;}
fn allowance(c:vec4<f32>)->f32 {
 let depth=c.r*10.0;let distance=c.g*128.0;
 let shore=pow(clamp(distance/16.0,0.0,1.0),1.15);
 let shallow=pow(clamp(depth/5.0,0.0,1.0),1.35);
 let maximum=max(ocean.waves.x*(0.65+0.16),0.001);
 return min(shore,shallow)*clamp(depth/maximum,0.0,1.0)*(1.0-c.b*0.85);
}
struct Swell {displacement:vec3<f32>,slope:vec3<f32>,crest:f32,variance:f32}
fn swell_at(n:vec3<f32>,source:vec4<f32>,wave:vec2<f32>,speed:f32,footprint:f32)->Swell {
 let centre=normalize(source.xyz);
 let side=cross(centre,n);let sine=length(side);
 // atan2 remains well-conditioned near a centre, unlike acos(dot).
 let angle=atan2(sine,clamp(dot(centre,n),-1.0,1.0));
 if angle<=0.000001 || angle>=1.0 {
  return Swell(vec3<f32>(0.0),vec3<f32>(0.0),0.0,0.0);
 }
 let direction=cross(side,n)/max(sine,0.000001);
 let envelope_sine=sin(3.14159265359*angle);
 // Zero height AND zero slope at the source and at one planet-radius arc.
 let envelope=envelope_sine*envelope_sine;
 let envelope_derivative=6.28318530718*envelope_sine*cos(3.14159265359*angle)/ocean.shape.x;
 let k=6.28318530718/wave.x;
 let phase=(angle*ocean.shape.x-ocean.viewer.w*speed)*k+source.w;
 let resolved=1.0-smoothstep(wave.x*0.25,wave.x*0.5,footprint);
 let amplitude=wave.y*ocean.waves.x;
 let height=sin(phase)*amplitude*envelope*resolved;
 let slope=direction*(amplitude*resolved*(envelope*k*cos(phase)+envelope_derivative*sin(phase)));
 let displacement=n*height+direction*(amplitude*envelope*ocean.waves.z*cos(phase)*resolved);
 let variance=pow(amplitude*envelope*k,2.0)*(1.0-resolved*resolved)*0.5;
 return Swell(displacement,slope,max(sin(phase),0.0)*amplitude*envelope,variance);
}
fn waves_at(n:vec3<f32>,footprint:f32)->Waves {
 let p=n*ocean.shape.x;let time=ocean.viewer.w;
 let c=coast(n);let a=allowance(c);
 if a<=0.000001 || ocean.waves.x<=0.0 {
  return Waves(vec3<f32>(0.0),vec3<f32>(0.0),0.0,0.0,0.0,c.r*10.0,c.g*128.0);
 }
 let axis=select(vec3<f32>(0.0,1.0,0.0),vec3<f32>(1.0,0.0,0.0),abs(n.y)>0.9);
 let tx=normalize(cross(axis,n));let ty=cross(n,tx);
 let step=max(ocean.shape.x*1.0/ocean.shading.w,0.1);
 let cx=coast(normalize(p+tx*step));let cy=coast(normalize(p+ty*step));
 let gradient=(tx*(allowance(cx)-a)+ty*(allowance(cy)-a))/step;
 let shore_gradient=(tx*(cx.g-c.g)+ty*(cy.g-c.g))*(128.0/step);
 var displacement=vec3<f32>(0.0);var slope=vec3<f32>(0.0);var height=0.0;var variance=0.0;var peak=0.0;
 for(var i=0u;i<4u;i+=1u) {
  let swell=swell_at(n,ocean.sources[i],COMPONENTS[i],SPEEDS[i],footprint);
  height+=dot(swell.displacement,n);
  displacement+=swell.displacement;
  slope+=swell.slope;variance+=swell.variance;peak+=swell.crest;
 }
 slope=slope*a+height*gradient;
 displacement*=a;
 let shore_envelope=(1.0-smoothstep(32.0,96.0,c.g*128.0))*(1.0-c.b)*a;
 let shore_phase=-c.g*128.0*(6.28318530718/12.0)-time*(2.2*6.28318530718/12.0);
 let shore_resolved=1.0-smoothstep(3.0,6.0,footprint);
 let shore_amplitude=0.16*ocean.waves.x*shore_envelope*shore_resolved;
 displacement+=n*(sin(shore_phase)*shore_amplitude);
 slope-=shore_gradient*(cos(shore_phase)*shore_amplitude*(6.28318530718/12.0));
 let foam_lod=max(log2(max(64.0*footprint/7.0,1.0)),0.0);
 let patches=textureSampleLevel(ocean_noise,noise_sampler,p/7.0-vec3<f32>(time*0.018,0.0,time*0.013),foam_lod).r;
 let crest=clamp(peak/max(ocean.waves.x*0.65,0.001),0.0,1.0);
 let coverage_width=mix(0.12,0.4,clamp(foam_lod*0.25,0.0,1.0));
 let coverage=smoothstep(0.42-coverage_width,0.42+coverage_width,patches);
 let whitecap=smoothstep(0.68,0.95,crest)*smoothstep(0.12,0.35,length(slope))*coverage;
 let breaking=smoothstep(0.25,0.9,sin(shore_phase))*(1.0-smoothstep(1.0,4.0,c.r*10.0))*smoothstep(0.25,2.0,c.g*128.0)*(1.0-c.b)*shore_resolved*smoothstep(0.0,0.1,ocean.waves.x);
 return Waves(displacement,slope,crest,clamp((whitecap+breaking*0.65)*ocean.waves.w,0.0,1.0),variance,c.r*10.0,c.g*128.0);
}
