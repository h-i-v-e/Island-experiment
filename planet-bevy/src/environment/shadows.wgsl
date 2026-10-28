#import planet_bevy::environment::{EnvironmentUniform,environment_cube_direction,environment_shadow_optical_depth}
struct ShadowStep { environment: EnvironmentUniform, face: vec4<u32> }
@group(0) @binding(0) var noise: texture_3d<f32>;
@group(0) @binding(1) var noise_sampler: sampler;
@group(0) @binding(2) var result: texture_storage_2d_array<rgba16float,write>;
@group(0) @binding(3) var<uniform> step: ShadowStep;
@compute @workgroup_size(8,8,1)
fn update(@builtin(global_invocation_id) id: vec3<u32>) {
    let size = textureDimensions(result);
    if any(id.xy >= size) { return; }
    let face = (step.face.x + id.z) % 6u;
    let direction = environment_cube_direction(face, (vec2<f32>(id.xy) + 0.5) / vec2<f32>(size));
    let radius = 1.0 + (step.environment.shape.y + step.environment.shape.z * 0.5) / step.environment.shape.x;
    let optical_depth = environment_shadow_optical_depth(direction * radius, step.environment, noise, noise_sampler);
    textureStore(result, vec2<i32>(id.xy), i32(face), vec4<f32>(optical_depth, 0.0, 0.0, 1.0));
}
