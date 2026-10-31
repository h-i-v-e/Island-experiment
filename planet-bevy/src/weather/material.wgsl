#define_import_path planet_bevy::weather
#import planet_bevy::environment_material::{environment,environment_sky_sampler}
@group(#{MATERIAL_BIND_GROUP}) @binding(129) var global_weather: texture_cube_array<f32>;
fn weather_enabled() -> bool { return environment.simulation.x > 0.5; }
fn weather_active() -> bool { return weather_enabled(); }
fn weather_sample(n:vec3<f32>, layer:i32)->vec4<f32> {
 let direction=normalize(n); let blend=clamp(environment.simulation.x-1.0,0.0,1.0);
 if blend>=0.9999 {return textureSampleLevel(global_weather,environment_sky_sampler,direction,layer,0.0);}
 if blend<=0.0001 {return textureSampleLevel(global_weather,environment_sky_sampler,direction,layer+6,0.0);}
 return mix(textureSampleLevel(global_weather,environment_sky_sampler,direction,layer+6,0.0),textureSampleLevel(global_weather,environment_sky_sampler,direction,layer,0.0),blend);
}
fn weather_lower(n: vec3<f32>) -> vec4<f32> { return weather_sample(n,0); }
fn weather_upper(n: vec3<f32>) -> vec4<f32> { return weather_sample(n,1); }
fn weather_climate(n: vec3<f32>) -> vec4<f32> { return weather_sample(n,2); }
fn weather_snow_line(n: vec3<f32>, fallback: f32) -> f32 {
    if !weather_enabled() { return fallback; }
    let temperature_kelvin = weather_climate(n).x;
    let reference_metres = weather_sample(n,5).w;
    // Match the small world's weather orography: 20 climate metres per
    // terrain metre, at 0.0065 K per climate metre. Temperature already
    // includes cooling at reference_metres; do not cool it a second time.
    return reference_metres + (temperature_kelvin - 273.15) / 0.13;
}
fn weather_blend() -> f32 { return clamp(environment.simulation.x - 1.0, 0.0, 1.0); }
fn weather_swell(n: vec3<f32>, index: u32) -> vec4<f32> { if weather_blend()<=0.0001 {return vec4<f32>(0.0);} return textureSampleLevel(global_weather,environment_sky_sampler,normalize(n),i32(3u+min(index,1u)),0.0); }
fn weather_swell_previous(n: vec3<f32>, index: u32) -> vec4<f32> { if weather_blend()>=0.9999 {return vec4<f32>(0.0);} return textureSampleLevel(global_weather,environment_sky_sampler,normalize(n),i32(9u+min(index,1u)),0.0); }

fn weather_debug_mode() -> u32 { return u32(environment.simulation.w + 0.5); }
fn weather_wind(p: vec3<f32>) -> vec3<f32> {
    if !weather_enabled() { return vec3<f32>(0.7, 0.1, 0.3); }
    let altitude = (length(p) - 1.0) * environment.shape.x;
    let layer = smoothstep(environment.shape.y, environment.shape.y + environment.shape.z * 1.5, altitude);
    let wind = mix(weather_lower(p).xyz, weather_upper(p).xyz, layer);
    let up = normalize(p);
    return wind - up * dot(wind, up);
}
fn weather_debug_colour(n: vec3<f32>) -> vec3<f32> {
    let climate = weather_climate(n);
    let lower = weather_lower(n);
    var value = 0.0;
    switch weather_debug_mode() {
        case 1u: { value = clamp((climate.y - 980.0) / 60.0, 0.0, 1.0); }
        case 2u: { value = clamp(length(lower.xyz) / 15.0, 0.0, 1.0); }
        case 3u: { value = climate.z; }
        case 4u: { value = max(lower.w, weather_upper(n).w); }
        case 5u: { value = climate.w; }
        case 6u: { value = clamp((weather_swell(n, 0u).w + weather_swell(n, 1u).w) / 2.0, 0.0, 1.0); }
        default: { value = clamp((climate.x - 250.0) / 60.0, 0.0, 1.0); }
    }
    return mix(vec3<f32>(0.025, 0.12, 0.65), vec3<f32>(1.0, 0.24, 0.025), value);
}
