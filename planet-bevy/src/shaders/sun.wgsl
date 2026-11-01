#import bevy_pbr::{forward_io::VertexOutput, mesh_view_bindings::view, mesh_functions::get_world_from_local}
@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> phase: vec4<f32>;
fn hash(p: vec3<f32>) -> f32 { return fract(sin(dot(p, vec3<f32>(127.1,311.7,74.7))) * 43758.5453); }
fn noise(p: vec3<f32>) -> f32 {
    let i = floor(p); let f = fract(p); let u = f*f*(3.0-2.0*f);
    return mix(mix(mix(hash(i),hash(i+vec3<f32>(1,0,0)),u.x),mix(hash(i+vec3<f32>(0,1,0)),hash(i+vec3<f32>(1,1,0)),u.x),u.y),mix(mix(hash(i+vec3<f32>(0,0,1)),hash(i+vec3<f32>(1,0,1)),u.x),mix(hash(i+vec3<f32>(0,1,1)),hash(i+vec3<f32>(1,1,1)),u.x),u.y),u.z);
}
@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let transform = get_world_from_local(in.instance_index);
    // Sphere normals transformed back to object space avoid a UV seam and
    // retain stable solar features while the planet's render frame rotates.
    let axes = mat3x3<f32>(normalize(transform[0].xyz),normalize(transform[1].xyz),normalize(transform[2].xyz));
    let normal = normalize(in.world_normal);
    let p = transpose(axes) * normal;
    let drift = phase.x * 0.001;
    let granules = noise(p*150.0+vec3<f32>(drift));
    let convection = noise(p*30.0+vec3<f32>(0,drift,0));
    let spots = smoothstep(0.65,0.8,noise(p*7.0)) * smoothstep(0.68,0.9,noise(p*50.0));
    let limb = pow(clamp(dot(normal,normalize(view.world_position-in.world_position.xyz)),0.0,1.0),0.35);
    let brightness = (0.72+granules*0.35+convection*0.18)*(1.0-spots*0.82);
    return vec4<f32>(vec3<f32>(6.0,3.5,1.2)*brightness*(0.35+0.65*limb),1.0);
}
