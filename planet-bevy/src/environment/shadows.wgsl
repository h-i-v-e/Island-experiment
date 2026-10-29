#import planet_bevy::environment::{EnvironmentUniform,environment_cube_direction,environment_shadow_optical_depth,environment_cloud_top,environment_cloud_bottom}
struct ShadowStep { environment: EnvironmentUniform, face: vec4<u32> }
@group(0) @binding(0) var noise: texture_3d<f32>;
@group(0) @binding(1) var noise_sampler: sampler;
@group(0) @binding(2) var result: texture_storage_2d_array<rgba16float,write>;
@group(0) @binding(3) var<uniform> step: ShadowStep;
@group(0) @binding(4) var weather_map:texture_cube_array<f32>;
@compute @workgroup_size(8,8,1)
fn update(@builtin(global_invocation_id) id: vec3<u32>) {
    let size = textureDimensions(result);
    if any(id.xy >= size) { return; }
    let face = (step.face.x + id.z) % 6u;
    let direction = environment_cube_direction(face, (vec2<f32>(id.xy) + 0.5) / vec2<f32>(size));
    let radius = 1.0 + ((environment_cloud_bottom(step.environment) + environment_cloud_top(step.environment)) * 0.5) / step.environment.shape.x;
    let optical_depth = environment_shadow_optical_depth(direction * radius, step.environment, noise, noise_sampler, weather_map);
    textureStore(result, vec2<i32>(id.xy), i32(face), vec4<f32>(optical_depth, 0.0, 0.0, 1.0));
}
