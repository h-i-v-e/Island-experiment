#define_import_path planet_bevy::environment

// All positions are normalized render-world coordinates. Heights, wavelengths
// and extinction lengths remain in metres, independent of the planet radius.
struct EnvironmentUniform {
    shape: vec4<f32>, weather: vec4<f32>, wind: vec4<f32>,
    sun: vec4<f32>, light: vec4<f32>, air: vec4<f32>, viewer: vec4<f32>,
}
fn environment_rotate(q: vec4<f32>, p: vec3<f32>) -> vec3<f32> {
    return p + 2.0 * cross(q.xyz, cross(q.xyz, p) + q.w * p);
}
fn environment_density(p: vec3<f32>, parameters: EnvironmentUniform,
    noise: texture_3d<f32>, noise_sampler: sampler) -> f32 {
    if parameters.weather.w < 0.5 || parameters.weather.x <= 0.0 || parameters.weather.y <= 0.0 {
        return 0.0;
    }
    let altitude = (length(p) - 1.0) * parameters.shape.x;
    let height = (altitude - parameters.shape.y) / max(parameters.shape.z, 0.001);
    if height <= 0.0 || height >= 1.0 { return 0.0; }
    // A seamless Cartesian volume is sampled on the sphere: no pole or
    // longitude seam, and wind rotates the same field used by every renderer.
    let rotated = environment_rotate(parameters.wind, p) * parameters.shape.x;
    let seed = vec3<f32>(0.371, 0.619, 0.237) * parameters.viewer.w;
    let coordinate = rotated / max(parameters.shape.w * 16.0, 1.0) + seed;
    let broad = textureSampleLevel(noise, noise_sampler, coordinate * 0.27 + 0.413, 1.0).g;
    let structure = textureSampleLevel(noise, noise_sampler, coordinate, 0.0);
    let detail = textureSampleLevel(noise, noise_sampler, coordinate * 3.7 + 0.173, 0.0).b;
    let coverage = clamp(parameters.weather.x + (broad - 0.5) * 0.42, 0.0, 1.0);
    let threshold = 1.0 - coverage;
    // A narrow formation threshold leaves truly empty sky between cumulus
    // masses. Broad nonzero tails would integrate into an opaque global sheet.
    let structure_value = 0.5 + ((structure.r - 0.5) * 0.72
        + (structure.g - 0.5) * 0.28) * 1.55 - max(detail - 0.45, 0.0) * 0.16;
    let formation = smoothstep(threshold - 0.035, threshold + 0.085, structure_value);
    if formation <= 0.001 { return 0.0; }
    // Keep small billows about 14 metres across independently of the larger
    // formation scale. Both erosion octaves sculpt the volume and its roof;
    // only eroding threshold edges would leave opaque, featureless plates.
    let fine_coordinate = rotated / (14.0 * 16.0) + seed + 0.619;
    let fine = textureSampleLevel(noise, noise_sampler, fine_coordinate, 0.0).r;
    let edge = 4.0 * formation * (1.0 - formation);
    let erosion = max(detail - 0.25, 0.0) * 0.40
        + max(fine - 0.35, 0.0) * (0.22 + edge * 0.24);
    let eroded_formation = max(formation - erosion, 0.0);
    let body = eroded_formation * eroded_formation;
    let bottom = smoothstep(0.0, 0.14, height);
    // Dense cores rise higher than their eroded edges, producing rounded
    // billows rather than a flat ceiling shared by every cloud formation.
    let ceiling = clamp(0.32 + structure.b * 0.32 + formation * 0.40
        - detail * 0.22 - fine * 0.10, 0.25, 1.0);
    let top = 1.0 - smoothstep(ceiling - 0.27, ceiling, height);
    return clamp(body * bottom * top, 0.0, 1.0);
}
// Density is a dimensionless optical-thickness multiplier across the layer.
fn environment_extinction(parameters: EnvironmentUniform) -> f32 {
    return parameters.weather.y * 5.0 / max(parameters.shape.z, 0.001);
}
fn environment_sun_visibility(p: vec3<f32>, parameters: EnvironmentUniform) -> f32 {
    let direction = normalize(parameters.sun.xyz);
    let projection = dot(p, direction);
    let discriminant = projection * projection - dot(p, p) + 1.0;
    return select(1.0, 0.0, projection < 0.0 && discriminant > 0.0);
}
// Signed sunward entry/exit distances through the shell, measured from an
// anchor inside it. The inner positive root selects this planet's near layer.
fn environment_shadow_interval(anchor: vec3<f32>, parameters: EnvironmentUniform) -> vec2<f32> {
    let direction = normalize(parameters.sun.xyz);
    let inner = 1.0 + parameters.shape.y / parameters.shape.x;
    let outer = inner + parameters.shape.z / parameters.shape.x;
    let projection = dot(anchor, direction);
    let square = dot(anchor, anchor);
    return vec2<f32>(-projection + sqrt(max(projection * projection - square + inner * inner, 0.0)),
        -projection + sqrt(max(projection * projection - square + outer * outer, 0.0)));
}
// A sun ray is indexed by its intersection with the middle cloud sphere.
// This is independent of the viewer and applies to every terrain LOD. The
// fourth component clips the optical path for receivers inside the clouds.
fn environment_shadow_coordinate(p: vec3<f32>, parameters: EnvironmentUniform) -> vec4<f32> {
    let direction = normalize(parameters.sun.xyz);
    let elevation = dot(normalize(p), direction);
    if elevation <= 0.015 || (length(p) - 1.0) * parameters.shape.x >= parameters.shape.y + parameters.shape.z {
        return vec4<f32>(0.0);
    }
    let middle_radius = 1.0 + (parameters.shape.y + parameters.shape.z * 0.5) / parameters.shape.x;
    let projection = dot(p, direction);
    let discriminant = projection * projection - dot(p, p) + middle_radius * middle_radius;
    if discriminant <= 0.0 { return vec4<f32>(0.0); }
    let distance = -projection + sqrt(discriminant);
    let middle = p + direction * distance;
    let interval = environment_shadow_interval(middle, parameters);
    let remaining = clamp((interval.y - max(interval.x, -distance)) / max(interval.y - interval.x, 0.000001), 0.0, 1.0);
    return vec4<f32>(middle / middle_radius, remaining);
}
fn environment_shadow_optical_depth(anchor: vec3<f32>, parameters: EnvironmentUniform,
    noise: texture_3d<f32>, noise_sampler: sampler) -> f32 {
    if parameters.weather.w < 0.5 || parameters.weather.x <= 0.0 || parameters.weather.y <= 0.0
        || dot(anchor, parameters.sun.xyz) <= 0.0 { return 0.0; }
    let interval = environment_shadow_interval(anchor, parameters);
    let stride = max(interval.y - interval.x, 0.0) / 8.0;
    let direction = normalize(parameters.sun.xyz);
    var optical_depth = 0.0;
    for (var i = 0u; i < 8u; i += 1u) {
        let sample = anchor + direction * (interval.x + (f32(i) + 0.5) * stride);
        optical_depth += environment_density(sample, parameters, noise, noise_sampler);
    }
    return optical_depth * stride * parameters.shape.x * environment_extinction(parameters);
}
fn environment_shadow(p: vec3<f32>, parameters: EnvironmentUniform,
    noise: texture_3d<f32>, noise_sampler: sampler) -> f32 {
    if parameters.weather.w < 0.5 || parameters.weather.z <= 0.0 || parameters.weather.x <= 0.0 {
        return 1.0;
    }
    let coordinate = environment_shadow_coordinate(p, parameters);
    if coordinate.w <= 0.0 { return 1.0; }
    let radius = 1.0 + (parameters.shape.y + parameters.shape.z * 0.5) / parameters.shape.x;
    let optical_depth = environment_shadow_optical_depth(coordinate.xyz * radius, parameters, noise, noise_sampler);
    let transmission = exp(-optical_depth * coordinate.w);
    let elevation = dot(normalize(p), normalize(parameters.sun.xyz));
    let strength = parameters.weather.z * smoothstep(0.015, 0.13, elevation);
    return mix(1.0, transmission, strength);
}
fn environment_cube_direction(face: u32, uv: vec2<f32>) -> vec3<f32> {
    let p = uv * 2.0 - 1.0;
    switch face {
        case 0u: { return normalize(vec3<f32>(1.0, -p.y, -p.x)); }
        case 1u: { return normalize(vec3<f32>(-1.0, -p.y, p.x)); }
        case 2u: { return normalize(vec3<f32>(p.x, 1.0, p.y)); }
        case 3u: { return normalize(vec3<f32>(p.x, -1.0, -p.y)); }
        case 4u: { return normalize(vec3<f32>(p.x, -p.y, 1.0)); }
        default: { return normalize(vec3<f32>(-p.x, -p.y, -1.0)); }
    }
}
// Exposed by the surface integration as an approximate direct-light factor.
// A floor preserves indirect skylight beneath dense cloud shadows.
fn environment_surface_light(p: vec3<f32>, parameters: EnvironmentUniform,
    noise: texture_3d<f32>, noise_sampler: sampler) -> f32 {
    return mix(0.35, 1.0, environment_shadow(p, parameters, noise, noise_sampler));
}
fn environment_cloud_light(p: vec3<f32>, view_direction: vec3<f32>, parameters: EnvironmentUniform,
    noise: texture_3d<f32>, noise_sampler: sampler) -> vec3<f32> {
    let sun = normalize(parameters.sun.xyz);
    let daylight = environment_sun_visibility(p, parameters);
    // Two midpoint samples integrate adjacent short sunward segments. Sampling
    // beyond the whole layer misses the roof's billows and flattens their light.
    let first_length = parameters.shape.z * 0.20;
    let second_length = parameters.shape.z * 0.36;
    let first = environment_density(p + sun * (first_length * 0.5 / parameters.shape.x), parameters, noise, noise_sampler);
    let second = environment_density(p + sun * ((first_length + second_length * 0.5) / parameters.shape.x), parameters, noise, noise_sampler);
    let optical = (first * first_length + second * second_length) * environment_extinction(parameters);
    let transmission = exp(-optical);
    let cosine = clamp(dot(view_direction, sun), -1.0, 1.0);
    let phase = 0.6975 / pow(max(1.3025 - 1.1 * cosine, 0.08), 1.5);
    let altitude_light = smoothstep(-0.15, 0.15, dot(normalize(p), sun));
    let sunset = 1.0 - smoothstep(0.0, 0.3, abs(dot(normalize(p), sun)));
    let direct_colour = mix(parameters.light.rgb, vec3<f32>(1.0, 0.48, 0.22), sunset * 0.45);
    let ambient = vec3<f32>(0.34, 0.48, 0.70) * parameters.sun.w * 0.007 * altitude_light;
    let direct = direct_colour * parameters.sun.w * (0.035 + phase * 0.022) * transmission * daylight;
    return ambient + direct + vec3<f32>(0.12, 0.17, 0.28);
}
fn environment_clear_sky(p: vec3<f32>, direction: vec3<f32>, parameters: EnvironmentUniform) -> vec3<f32> {
    if parameters.light.w < 0.5 { return vec3<f32>(0.0); }
    let up = normalize(p);
    let elevation = clamp(dot(up, direction), 0.0, 1.0);
    let sun_height = dot(up, normalize(parameters.sun.xyz));
    let day = smoothstep(-0.16, 0.12, sun_height);
    let horizon = vec3<f32>(0.43, 0.61, 0.88);
    let zenith = vec3<f32>(0.09, 0.27, 0.60);
    let sky = mix(horizon, zenith, pow(elevation, 0.35));
    let sunset = exp(-abs(sun_height) * 8.0) * pow(max(dot(direction, parameters.sun.xyz), 0.0), 4.0);
    return (sky * day + vec3<f32>(1.0, 0.22, 0.05) * sunset * 0.32) * parameters.sun.w * 0.018
        + vec3<f32>(0.08, 0.12, 0.22);
}
