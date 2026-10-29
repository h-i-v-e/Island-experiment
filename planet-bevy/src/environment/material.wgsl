#define_import_path planet_bevy::environment_material
#import planet_bevy::environment::{EnvironmentUniform, environment_shadow_coordinate,environment_ambient_weight,environment_surface_tint}
@group(#{MATERIAL_BIND_GROUP}) @binding(123) var<storage, read> environment: EnvironmentUniform;
@group(#{MATERIAL_BIND_GROUP}) @binding(126) var environment_sky: texture_cube<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(127) var environment_shadows: texture_cube<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(102) var environment_sky_sampler: sampler;
// The dedicated global cube stores sunward integrated optical depth. Surface
// fragments use one filtered lookup, including distant orbital terrain.
fn material_cloud_shadow(p: vec3<f32>) -> f32 {
    if environment.weather.w < 0.5 || environment.weather.z <= 0.0 || environment.weather.x <= 0.0 || environment.weather.y <= 0.0 {
        return 1.0;
    }
    let coordinate = environment_shadow_coordinate(p, environment);
    if coordinate.w <= 0.0 { return 1.0; }
    let optical_depth = textureSampleLevel(environment_shadows, environment_sky_sampler, coordinate.xyz, 0.0).r;
    let transmission = exp(-optical_depth * coordinate.w);
    let elevation = dot(normalize(p), normalize(environment.sun.xyz));
    return mix(1.0, transmission, environment.weather.z * smoothstep(0.015, 0.13, elevation));
}
fn material_cloud_light(p: vec3<f32>) -> f32 {
    return mix(0.35, 1.0, material_cloud_shadow(p));
}
fn material_ambient_weight(p:vec3<f32>)->f32 {
    return environment_ambient_weight(p,environment);
}
fn material_twilight_tint(p:vec3<f32>)->vec3<f32> {
    return environment_surface_tint(p,environment);
}
fn material_sky(direction: vec3<f32>) -> vec3<f32> {
    return textureSampleLevel(environment_sky, environment_sky_sampler, direction, 0.0).rgb;
}
