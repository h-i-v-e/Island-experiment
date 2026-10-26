#define_import_path planet_bevy::water_impacts
struct ImpactUniform {count:vec4<f32>,centres:array<vec4<f32>,32>,shapes:array<vec4<f32>,32>}
fn water_impacts(n:vec3<f32>,radius:f32,footprint:f32,impacts:ImpactUniform)->vec4<f32>{
 var foam=0.0;var slope=vec3<f32>(0.0);
 for(var i=0u;i<32u;i+=1u){
  if f32(i)>=impacts.count.x {break;}
  let centre=impacts.centres[i];let shape=impacts.shapes[i];
  let delta=n-centre.xyz;let distance=length(delta)*radius;
  if distance>centre.w+shape.z*4.0 {continue;}
  let width=max(shape.z,footprint*0.65);
  let band=(distance-centre.w)/max(width,0.001);
  let inner=(distance-centre.w*0.64)/max(width*0.8,0.001);
  let outer_ring=exp2(-band*band*2.0);let inner_ring=exp2(-inner*inner*2.0)*0.45;
  let water_filter=shape.z/max(width,0.001);
  foam=max(foam,(outer_ring+inner_ring)*shape.x*water_filter);
  let tangent=delta-n*dot(delta,n);
  slope+=tangent/max(length(tangent),0.001)*(band*outer_ring+inner*inner_ring)*shape.y*water_filter;
 }
 return vec4<f32>(slope,clamp(foam,0.0,1.0));
}
