#define_import_path planet_bevy::waves
#import planet_bevy::environment_material::environment_sky_sampler
#import planet_bevy::weather::{weather_enabled,weather_lower,weather_swell,weather_swell_previous,weather_blend}
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
// Persistent global modes: weather changes their energies, never their phases.
// A non-orthogonal hemispherical distribution and detuned wavelengths avoid
// turning crossing swells into a Cartesian lattice. Both propagation signs
// share each axis; tangent projection stays continuous across cube seams.
const WEATHER_MODE_COUNT=6u;
const WEATHER_AXES=array<vec3<f32>,6>(
 vec3<f32>(0.99652173,0.08333333,0.00000000),
 vec3<f32>(-0.71395435,0.25000000,0.65404067),
 vec3<f32>(0.07947517,0.41666667,-0.90557859),
 vec3<f32>(0.49419404,0.58333333,0.64458861),
 vec3<f32>(-0.65132675,0.75000000,-0.11521053),
 vec3<f32>(0.33720902,0.91666667,-0.21450478));
const WEATHER_LENGTHS=array<f32,3>(5.0,15.0,40.0);
const WEATHER_DETUNE=array<vec3<f32>,6>(
 vec3<f32>(0.823,1.137,0.947),vec3<f32>(1.193,0.871,1.079),
 vec3<f32>(0.967,1.217,0.811),vec3<f32>(1.071,0.943,1.183),
 vec3<f32>(0.899,1.031,1.223),vec3<f32>(1.157,0.791,0.883));
// Smooth, wavelength-scaled phase bending breaks up long global grooves.
// Its Jacobian perturbation is bounded by 0.45*TAU/8 < 0.36, so the domain
// cannot fold. Fixed planet coordinates keep moving mesh anchors/wind out of it.
const WEATHER_BEND_AXES=array<vec3<f32>,3>(
 vec3<f32>(0.80,0.00,0.60),vec3<f32>(-0.36,0.80,0.48),vec3<f32>(-0.48,-0.60,0.64));
struct WeatherDomain {position:vec3<f32>,derivative:vec3<f32>}
fn weather_domain(p:vec3<f32>,wavelength:f32,band:u32)->WeatherDomain {
 let k=6.28318530718/(wavelength*8.0);
 let q=vec3<f32>(dot(p,WEATHER_BEND_AXES[0]),dot(p,WEATHER_BEND_AXES[1]),dot(p,WEATHER_BEND_AXES[2]))*k
   +vec3<f32>(0.731,2.193,4.917)+f32(band)*vec3<f32>(1.713,2.917,0.941);
 let amplitude=wavelength*0.45;
 return WeatherDomain(p+sin(q)*amplitude,cos(q)*(amplitude*k));
}
fn weather_domain_gradient(axis:vec3<f32>,domain:WeatherDomain)->vec3<f32> {
 return axis+WEATHER_BEND_AXES[0]*(axis.x*domain.derivative.x)
   +WEATHER_BEND_AXES[1]*(axis.y*domain.derivative.y)
   +WEATHER_BEND_AXES[2]*(axis.z*domain.derivative.z);
}
fn mode_weight(n:vec3<f32>,direction:vec3<f32>,axis:vec3<f32>)->f32 {
 let tangent=axis-n*dot(n,axis);
 let square=dot(tangent,tangent);
 let alignment=max(dot(direction,tangent)*inverseSqrt(max(square,0.000001)),0.0);
 return alignment*alignment*alignment*alignment*square*square;
}
fn direction_normalizer(n:vec3<f32>,direction:vec3<f32>)->f32 {
 var sum=0.0;
 for(var i=0u;i<WEATHER_MODE_COUNT;i+=1u) {
  sum+=mode_weight(n,direction,WEATHER_AXES[i])+mode_weight(n,direction,-WEATHER_AXES[i]);
 }
 return max(sum,0.000001);
}
fn spectral_band_weights(wave:vec4<f32>)->vec3<f32> {
 let k=length(wave.xyz);
 if k<=0.000001 || wave.w<=0.0 {return vec3<f32>(0.0);}
 var weights=vec3<f32>(0.0);
 for(var band=0u;band<3u;band+=1u) {
  let offset=log(max(k*WEATHER_LENGTHS[band]/6.28318530718,0.000001));
  weights[band]=exp(-offset*offset*3.5);
 }
 return weights/max(weights.x+weights.y+weights.z,0.000001);
}
fn weather_waves(n:vec3<f32>,wind:vec3<f32>,packets:array<vec4<f32>,4>,blend:f32,footprint:f32)->Swell {
 var domains:array<WeatherDomain,3>;
 for(var band=0u;band<3u;band+=1u) {
  domains[band]=weather_domain(n*ocean.shape.x,WEATHER_LENGTHS[band],band);
 }
 var directions:array<vec3<f32>,4>;
 var bands:array<vec3<f32>,4>;
 var normalizers:array<f32,4>;
 // Ranked packets are not persistent identities. Blend their directional
 // MODE ENERGIES, never their vectors: a rank swap between opposing swells
 // must not cancel the entire sea halfway through a weather upload.
 for(var packet=0u;packet<4u;packet+=1u) {
  let wave=packets[packet];
  directions[packet]=wave.xyz*inverseSqrt(max(dot(wave.xyz,wave.xyz),0.000001));
  let frame_weight=select(1.0-blend,blend,packet<2u);
  bands[packet]=spectral_band_weights(wave)*wave.w*wave.w*frame_weight;
  normalizers[packet]=direction_normalizer(n,directions[packet]);
 }
 let wind_speed=length(wind);
 let wind_direction=wind/max(wind_speed,0.001);
 let chop_amplitude=min(wind_speed*wind_speed*0.0012,0.45);
 let chop_bands=vec3<f32>(0.85,0.15,0.0)*chop_amplitude*chop_amplitude;
 let wind_norm=direction_normalizer(n,wind_direction);
 var displacement=vec3<f32>(0.0);var slope=vec3<f32>(0.0);var crest=0.0;var variance=0.0;
 for(var axis_index=0u;axis_index<WEATHER_MODE_COUNT;axis_index+=1u) {
  let axis=WEATHER_AXES[axis_index];
  for(var sign_index=0u;sign_index<2u;sign_index+=1u) {
   let sign=select(-1.0,1.0,sign_index==0u);
   let wind_weight=mode_weight(n,wind_direction,axis*sign)/wind_norm;
   var mode_energy=chop_bands*wind_weight;
   for(var packet=0u;packet<4u;packet+=1u) {
    mode_energy+=bands[packet]*(mode_weight(n,directions[packet],axis*sign)/normalizers[packet]);
   }
   for(var band=0u;band<3u;band+=1u) {
    let energy=mode_energy[band];
    if energy<=0.0000001 {continue;}
    let amplitude=sqrt(max(energy,0.0))*ocean.waves.x;
    let axis_gradient=weather_domain_gradient(axis,domains[band]);
    let tangent=axis_gradient-n*dot(n,axis_gradient);
    let length_tangent=length(tangent);
    let mode_length=WEATHER_LENGTHS[band]*WEATHER_DETUNE[axis_index][band];
    let k=6.28318530718/mode_length;
    let wavelength=mode_length/max(length_tangent,0.001);
    let resolved=1.0-smoothstep(wavelength*0.25,wavelength*0.5,footprint);
    variance+=amplitude*amplitude*k*k*length_tangent*length_tangent*(1.0-resolved*resolved)*0.5;
    if resolved<=0.0 {continue;}
    let omega=sqrt(9.81*k);
    let phase=dot(domains[band].position,axis)*k-sign*omega*ocean.viewer.w
      +f32(axis_index)*1.713+f32(band)*2.417+f32(sign_index)*0.927;
    let sine=sin(phase);let cosine=cos(phase);
    displacement+=n*(sine*amplitude*resolved)
      +tangent*(cosine*amplitude*resolved*ocean.waves.z/max(length_tangent,0.001));
    slope+=tangent*(cosine*amplitude*resolved*k);
    crest+=max(sine,0.0)*amplitude*resolved;
   }
  }
 }
 return Swell(displacement,slope,crest,variance);
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
 var wind=vec3<f32>(0.0);var sea_activity=1.0;
 if weather_enabled() {
  wind=weather_lower(n).xyz;wind-=n*dot(wind,n);
  let first=weather_swell(n,0u);let second=weather_swell(n,1u);
  let previous_first=weather_swell_previous(n,0u);let previous_second=weather_swell_previous(n,1u);
  let blend=weather_blend();
  let swell=weather_waves(n,wind,array<vec4<f32>,4>(first,second,previous_first,previous_second),blend,footprint);
  height=dot(swell.displacement,n);displacement=swell.displacement;
  slope=swell.slope;variance=swell.variance;peak=swell.crest;
  let wave_energy=mix(previous_first.w*previous_first.w+previous_second.w*previous_second.w,
    first.w*first.w+second.w*second.w,blend);
  sea_activity=clamp((sqrt(max(wave_energy,0.0))+length(wind)*0.015)*2.0,0.0,1.0);
 } else {
  for(var i=0u;i<4u;i+=1u) {
   let swell=swell_at(n,ocean.sources[i],COMPONENTS[i],SPEEDS[i],footprint);
   height+=dot(swell.displacement,n);
   displacement+=swell.displacement;
   slope+=swell.slope;variance+=swell.variance;peak+=swell.crest;
  }
 }
 slope=slope*a+height*gradient;
 displacement*=a;
 let shore_envelope=(1.0-smoothstep(32.0,96.0,c.g*128.0))*(1.0-c.b)*a;
 let shore_phase=-c.g*128.0*(6.28318530718/12.0)-time*(2.2*6.28318530718/12.0);
 let shore_resolved=1.0-smoothstep(3.0,6.0,footprint);
 let shore_amplitude=0.16*ocean.waves.x*shore_envelope*shore_resolved*sea_activity;
 displacement+=n*(sin(shore_phase)*shore_amplitude);
 slope-=shore_gradient*(cos(shore_phase)*shore_amplitude*(6.28318530718/12.0));
 if weather_enabled() {
  // An energetic storm spectrum can exceed the old four-source amplitude
  // bound. Smoothly compress radial troughs before they reach the seabed.
  let depth_limit=max(c.r*10.0*0.7,0.001);
  let height=dot(displacement,n);
  let limited=tanh(height/depth_limit);
  displacement+=n*(limited*depth_limit-height);
  slope*=1.0-limited*limited;
 }
 let foam_lod=max(log2(max(64.0*footprint/7.0,1.0)),0.0);
 let foam_drift=select(vec3<f32>(0.018,0.0,0.013),wind*0.005,weather_enabled());
 let patches=textureSampleLevel(ocean_noise,noise_sampler,p/7.0-foam_drift*time,foam_lod).r;
 let crest=clamp(peak/max(ocean.waves.x*0.65,0.001),0.0,1.0);
 let coverage_width=mix(0.12,0.4,clamp(foam_lod*0.25,0.0,1.0));
 let coverage=smoothstep(0.42-coverage_width,0.42+coverage_width,patches);
 let whitecap=smoothstep(0.68,0.95,crest)*smoothstep(0.12,0.35,length(slope))*coverage;
 let breaking=smoothstep(0.25,0.9,sin(shore_phase))*(1.0-smoothstep(1.0,4.0,c.r*10.0))*smoothstep(0.25,2.0,c.g*128.0)*(1.0-c.b)*shore_resolved*smoothstep(0.0,0.1,ocean.waves.x)*sea_activity;
 return Waves(displacement,slope,crest,clamp((whitecap+breaking*0.65)*ocean.waves.w,0.0,1.0),variance,c.r*10.0,c.g*128.0);
}
