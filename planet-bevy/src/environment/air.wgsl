#define_import_path planet_bevy::environment_air
#import planet_bevy::environment::{EnvironmentUniform,environment_clear_sky,environment_sun_visibility}

// This is a bounded foreground-air approximation for newly integrated cloud
// light. The opaque scene already contains Bevy's atmospheric scattering and
// must never be fed through this function a second time.
struct EnvironmentAir {
    transmission: vec3<f32>,
    inscattering: vec3<f32>,
}

fn environment_air_densities(altitude: f32, height: f32) -> vec3<f32> {
    let fraction = clamp(altitude / height, 0.0, 1.0);
    // Match ScatteringMedium::earth's relative profiles. Its Rayleigh and Mie
    // scale heights are 8/60 and 1.2/60 of the atmosphere's physical thickness.
    let rayleigh = max((exp(-fraction * 7.5) - 0.0005530844) / 0.9994469156, 0.0);
    let mie = exp(-fraction * 50.0);
    let ozone = max(1.0 - abs(fraction - 0.75) / 0.30, 0.0);
    return vec3<f32>(rayleigh, mie, ozone);
}

// Both vectors are unexposed linear HDR. Evaluate once per cloud shell segment
// at a representative point, then use L_cloud * transmission + inscattering
// for that segment's cloud samples. Callers apply view.exposure exactly once.
fn environment_air(origin: vec3<f32>, point: vec3<f32>, parameters: EnvironmentUniform) -> EnvironmentAir {
    let identity = EnvironmentAir(vec3<f32>(1.0), vec3<f32>(0.0));
    if parameters.light.w < 0.5 || parameters.air.y <= 0.0 {return identity;}
    let radius = max(parameters.shape.x, 0.001);
    let height = max(parameters.air.x, 0.001);
    let length_to_point = length(point - origin);
    if length_to_point <= 0.0000001 {return identity;}
    let direction = (point - origin) / length_to_point;
    let outer = 1.0 + height / radius;
    let projection = dot(origin, direction);
    let discriminant = projection * projection - dot(origin, origin) + outer * outer;
    if discriminant <= 0.0 {return identity;}
    let root = sqrt(discriminant);
    let near = max(-projection - root, 0.0);
    let far = min(-projection + root, length_to_point);
    if far <= near {return identity;}
    let interval = far - near;
    // Two-point Gauss-Legendre integration captures the curved, planet-centred
    // air column without a second volume march or any extra noise lookups.
    let first = origin + direction * (near + interval * 0.2113248654);
    let second = origin + direction * (near + interval * 0.7886751346);
    let densities = (environment_air_densities((length(first) - 1.0) * radius, height)
        + environment_air_densities((length(second) - 1.0) * radius, height)) * 0.5;
    let column = densities * (interval * radius) * (60000.0 / height) * parameters.air.y;
    let scattering = vec3<f32>(5.802e-6, 13.558e-6, 33.100e-6) * column.x
        + vec3<f32>(0.444e-6 * column.y);
    let absorption = vec3<f32>(3.996e-6 * column.y)
        + vec3<f32>(0.650e-6, 1.881e-6, 0.085e-6) * column.z;
    let extinction = scattering + absorption;
    let transmission = exp(-min(extinction, vec3<f32>(30.0)));
    let scattering_fraction = scattering / max(extinction, vec3<f32>(0.000000001));
    let sunlight = (environment_sun_visibility(first, parameters)
        + environment_sun_visibility(second, parameters)) * 0.5;
    let source = environment_clear_sky(origin, direction, parameters) * sunlight;
    return EnvironmentAir(transmission, source * (1.0 - transmission) * scattering_fraction);
}
