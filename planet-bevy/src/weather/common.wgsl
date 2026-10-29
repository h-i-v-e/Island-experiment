#define_import_path planet_bevy::weather_common
fn weather_lower(n: vec3<f32>, field: texture_cube_array<f32>, field_sampler: sampler) -> vec4<f32> {
    return textureSampleLevel(field, field_sampler, normalize(n), 0, 0.0);
}
fn weather_upper(n: vec3<f32>, field: texture_cube_array<f32>, field_sampler: sampler) -> vec4<f32> {
    return textureSampleLevel(field, field_sampler, normalize(n), 1, 0.0);
}
fn weather_climate(n: vec3<f32>, field: texture_cube_array<f32>, field_sampler: sampler) -> vec4<f32> {
    return textureSampleLevel(field, field_sampler, normalize(n), 2, 0.0);
}
