#define_import_path island_bevy::noise

// World-space value noise on a hashed lattice. Nothing is sampled from a
// texture, and the lattice is aperiodic, so the only repetition any layer can
// show is whatever its own caller builds in.

/// Hoskins' `hash13`: one lattice corner to one value in `[0, 1)`. The lattice
/// coordinates stay under a few thousand, which is where the `fract` chain
/// still has enough mantissa left to separate neighbours.
fn hash(cell: vec3<f32>) -> f32 {
    var point = fract(cell * vec3<f32>(0.1031, 0.1030, 0.0973));
    point += dot(point, point.zyx + 33.33);
    return fract((point.x + point.y) * point.z);
}

/// Value noise in `[0, 1]`, quintic-interpolated so the second derivative is
/// continuous and the lattice does not show as creases.
fn noise(point: vec3<f32>) -> f32 {
    let cell = floor(point);
    let offset = point - cell;
    let blend = offset * offset * offset * (offset * (offset * 6.0 - 15.0) + 10.0);

    let x00 = mix(hash(cell), hash(cell + vec3<f32>(1.0, 0.0, 0.0)), blend.x);
    let x10 = mix(
        hash(cell + vec3<f32>(0.0, 1.0, 0.0)),
        hash(cell + vec3<f32>(1.0, 1.0, 0.0)),
        blend.x,
    );
    let x01 = mix(
        hash(cell + vec3<f32>(0.0, 0.0, 1.0)),
        hash(cell + vec3<f32>(1.0, 0.0, 1.0)),
        blend.x,
    );
    let x11 = mix(
        hash(cell + vec3<f32>(0.0, 1.0, 1.0)),
        hash(cell + vec3<f32>(1.0, 1.0, 1.0)),
        blend.x,
    );
    return mix(mix(x00, x10, blend.y), mix(x01, x11, blend.y), blend.z);
}

/// The same noise carrying its analytic gradient in `yzw`, which is what lets a
/// surface be bumped without sampling it four times.
fn noise_gradient(point: vec3<f32>) -> vec4<f32> {
    let cell = floor(point);
    let offset = point - cell;
    let blend = offset * offset * offset * (offset * (offset * 6.0 - 15.0) + 10.0);
    let slope = 30.0 * offset * offset * (offset * (offset - 2.0) + 1.0);

    let a = hash(cell);
    let b = hash(cell + vec3<f32>(1.0, 0.0, 0.0));
    let c = hash(cell + vec3<f32>(0.0, 1.0, 0.0));
    let d = hash(cell + vec3<f32>(1.0, 1.0, 0.0));
    let e = hash(cell + vec3<f32>(0.0, 0.0, 1.0));
    let f = hash(cell + vec3<f32>(1.0, 0.0, 1.0));
    let g = hash(cell + vec3<f32>(0.0, 1.0, 1.0));
    let h = hash(cell + vec3<f32>(1.0, 1.0, 1.0));

    let k1 = b - a;
    let k2 = c - a;
    let k3 = e - a;
    let k4 = a - b - c + d;
    let k5 = a - c - e + g;
    let k6 = a - b - e + f;
    let k7 = h - g - f + e - d + c + b - a;

    let value = a
        + k1 * blend.x
        + k2 * blend.y
        + k3 * blend.z
        + k4 * blend.x * blend.y
        + k5 * blend.y * blend.z
        + k6 * blend.z * blend.x
        + k7 * blend.x * blend.y * blend.z;
    let gradient = slope
        * vec3<f32>(
            k1 + k4 * blend.y + k6 * blend.z + k7 * blend.y * blend.z,
            k2 + k4 * blend.x + k5 * blend.z + k7 * blend.z * blend.x,
            k3 + k5 * blend.y + k6 * blend.x + k7 * blend.x * blend.y,
        );
    return vec4<f32>(value, gradient);
}

/// Octaves at halving amplitude and doubling frequency, renormalized to
/// `[0, 1]` so a caller can treat any octave count the same way.
fn fbm(point: vec3<f32>, octaves: i32) -> f32 {
    var total = 0.0;
    var normalization = 0.0;
    var amplitude = 1.0;
    var sample_point = point;
    for (var octave = 0; octave < octaves; octave += 1) {
        total += amplitude * noise(sample_point);
        normalization += amplitude;
        amplitude *= 0.5;
        sample_point *= 2.0;
    }
    return total / normalization;
}

/// [`fbm`] carrying the gradient of the same sum in `yzw`, in units of the
/// point space it was handed.
fn fbm_gradient(point: vec3<f32>, octaves: i32) -> vec4<f32> {
    var total = 0.0;
    var gradient = vec3<f32>(0.0);
    var normalization = 0.0;
    var amplitude = 1.0;
    var frequency = 1.0;
    for (var octave = 0; octave < octaves; octave += 1) {
        let octave_sample = noise_gradient(point * frequency);
        total += amplitude * octave_sample.x;
        gradient += (amplitude * frequency) * octave_sample.yzw;
        normalization += amplitude;
        amplitude *= 0.5;
        frequency *= 2.0;
    }
    return vec4<f32>(total, gradient) / normalization;
}

/// Bends a unit normal along the part of a height gradient that lies in the
/// surface, which needs no tangent frame and so behaves the same on a cliff as
/// on flat ground. The bend is capped because a normal that crosses the horizon
/// turns the surface inside out.
fn perturb(normal: vec3<f32>, gradient: vec3<f32>, strength: f32) -> vec3<f32> {
    let tangential = gradient - normal * dot(gradient, normal);
    let bend = tangential * strength;
    let magnitude = max(length(bend), 1e-6);
    return normalize(normal - bend * min(1.0, 0.7 / magnitude));
}
