#define_import_path planet_bevy::environment_material
#import planet_bevy::environment::{EnvironmentUniform, environment_extinction}
@group(#{MATERIAL_BIND_GROUP}) @binding(123) var<storage, read> environment: EnvironmentUniform;
@group(#{MATERIAL_BIND_GROUP}) @binding(126) var environment_sky: texture_cube<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(102) var environment_sky_sampler: sampler;
// The cube's alpha stores the global cloud-column density. Projecting a
// surface point toward the sun keeps shadows consistent across terrain, grass
// and water, with one filtered lookup rather than a per-fragment volume march.
fn material_cloud_shadow(p: vec3<f32>) -> f32 {
    if environment.weather.w < 0.5 || environment.weather.z <= 0.0 || environment.weather.x <= 0.0 {
        return 1.0;
    }
    let direction = normalize(environment.sun.xyz);
    let elevation = dot(normalize(p), direction);
    if elevation <= 0.015 || (length(p) - 1.0) * environment.shape.x >= environment.shape.y + environment.shape.z {
        return 1.0;
    }
    let radius = 1.0 + (environment.shape.y + environment.shape.z * 0.45) / environment.shape.x;
    let projection = dot(p, direction);
    let discriminant = projection * projection - dot(p, p) + radius * radius;
    if discriminant <= 0.0 { return 1.0; }
    let travel = -projection + sqrt(discriminant);
    if travel <= 0.0 { return 1.0; }
    let density = textureSampleLevel(environment_sky, environment_sky_sampler,
        normalize(p + direction * travel), 0.0).a;
    let slant = min(1.0 / max(elevation, 0.12), 5.0);
    let transmission = exp(-density * environment.shape.z * slant * environment_extinction(environment));
    return mix(1.0, transmission, environment.weather.z * smoothstep(0.015, 0.13, elevation));
}
fn material_cloud_light(p: vec3<f32>) -> f32 {
    return mix(0.35, 1.0, material_cloud_shadow(p));
}
fn material_sky(direction: vec3<f32>) -> vec3<f32> {
    return textureSampleLevel(environment_sky, environment_sky_sampler, direction, 0.0).rgb;
}
