// River rocks. The generator hands over one merged mesh, so the only per-body
// signal available is what `convert` writes into the colour attribute: a
// deterministic tint in the three colour channels, and in alpha how much spray
// from the nearest fall stands on the stone. Everything finer is world-space
// noise.

#import bevy_pbr::{
    forward_io::{VertexOutput, FragmentOutput},
    mesh_view_bindings::view,
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::{alpha_discard, apply_pbr_lighting, main_pass_post_lighting_processing},
}
#import island_bevy::noise::{hash, perturb}

// Linear-space minerals, each written above as the sRGB triple it came from.
const STONE_COOL: vec3<f32> = vec3<f32>(0.13287, 0.13998, 0.13998); // 0.40, 0.41, 0.41
const STONE_WARM: vec3<f32> = vec3<f32>(0.15487, 0.11280, 0.06838); // 0.43, 0.37, 0.29

/// Wavelengths of the two detail layers, in metres. The generator settles
/// stones of six to twenty-two centimetres and the occasional boulder up to
/// sixty-five, so both layers have to sit inside one hand-sized body.
const GRAIN_METRES: f32 = 0.055;
const MICRO_METRES: f32 = 0.011;
/// Lattice cells before rock detail repeats. Wrapping the integer corners keeps
/// the float hash inside its documented domain while leaving a seamless 113 m
/// grain tile and 23 m micro tile — both vastly larger than a rock body.
const DETAIL_LATTICE_PERIOD: f32 = 2048.0;

/// How far down soaked stone takes its own albedo and the roughness it
/// converges on. The same bargain `terrain.wgsl` strikes for a river bank, a
/// little harder: a boulder at the foot of a fall is running with water rather
/// than merely damp. Reflectance is left where the base material put it, so
/// dry stone anywhere on the island is untouched by this.
const ALBEDO_WET: f32 = 0.62;
const ROUGHNESS_WET: f32 = 0.16;

struct RockSettings {
    /// Metres at which the sub-metre layer has faded out entirely.
    detail_range: f32,
    normal_strength: f32,
    roughness: f32,
    roughness_spread: f32,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(100) var<uniform> settings: RockSettings;

fn detail_hash(cell: vec3<f32>) -> f32 {
    let wrapped = cell
        - floor(cell / vec3<f32>(DETAIL_LATTICE_PERIOD)) * DETAIL_LATTICE_PERIOD;
    return hash(wrapped);
}

/// The shared analytic value-noise gradient with only its lattice hash wrapped.
/// Wrapping corners rather than sample positions keeps both value and gradient
/// continuous where the detail tile joins.
fn detail_noise_gradient(point: vec3<f32>) -> vec4<f32> {
    let cell = floor(point);
    let offset = point - cell;
    let blend = offset * offset * offset * (offset * (offset * 6.0 - 15.0) + 10.0);
    let slope = 30.0 * offset * offset * (offset * (offset - 2.0) + 1.0);

    let a = detail_hash(cell);
    let b = detail_hash(cell + vec3<f32>(1.0, 0.0, 0.0));
    let c = detail_hash(cell + vec3<f32>(0.0, 1.0, 0.0));
    let d = detail_hash(cell + vec3<f32>(1.0, 1.0, 0.0));
    let e = detail_hash(cell + vec3<f32>(0.0, 0.0, 1.0));
    let f = detail_hash(cell + vec3<f32>(1.0, 0.0, 1.0));
    let g = detail_hash(cell + vec3<f32>(0.0, 1.0, 1.0));
    let h = detail_hash(cell + vec3<f32>(1.0, 1.0, 1.0));

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

fn detail_fbm_gradient(point: vec3<f32>, octaves: i32) -> vec4<f32> {
    var total = 0.0;
    var gradient = vec3<f32>(0.0);
    var normalization = 0.0;
    var amplitude = 1.0;
    var frequency = 1.0;
    for (var octave = 0; octave < octaves; octave += 1) {
        let octave_sample = detail_noise_gradient(point * frequency);
        total += amplitude * octave_sample.x;
        gradient += (amplitude * frequency) * octave_sample.yzw;
        normalization += amplitude;
        amplitude *= 0.5;
        frequency *= 2.0;
    }
    return vec4<f32>(total, gradient) / normalization;
}

@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
    var pbr_input = pbr_input_from_standard_material(in, is_front);

#ifdef VERTEX_COLORS
    let tint = in.color.rgb;
    let spray = clamp(in.color.a, 0.0, 1.0);
#else
    let tint = vec3<f32>(1.0);
    let spray = 0.0;
#endif

    let world = in.world_position.xyz;
    let normal = normalize(pbr_input.world_normal);
    let range = length(world - view.world_position);

    // Both detail layers are finer than a pixel well before the body itself
    // is, so they are faded out rather than left to alias into the temporal
    // resolve. The per-body tint carries the variation that survives distance.
    let near = 1.0 - smoothstep(0.0, max(settings.detail_range, 1.0), range);
    let grain = detail_fbm_gradient(world / GRAIN_METRES, 3);
    var albedo = mix(STONE_COOL, STONE_WARM, clamp(grain.x * 1.5 - 0.25, 0.0, 1.0))
        * tint
        * (1.0 + (grain.x - 0.5) * 0.44 * near);
    var gradient = grain.yzw * near;

    if near > 0.4 {
        let micro = detail_noise_gradient(world / MICRO_METRES);
        let closeness = (near - 0.4) / 0.6;
        albedo *= 1.0 + (micro.x - 0.5) * 0.18 * closeness;
        gradient += micro.yzw * 0.22 * closeness;
    }

    // Spray soaks the stone around a fall. The grain breaks its edge, the same
    // way the terrain's own mottle breaks a bank's, so what ends is wet stone
    // and not a disc laid over it.
    let wet = clamp(spray + (grain.x - 0.5) * 0.35, 0.0, 1.0);
    albedo *= mix(1.0, ALBEDO_WET, wet);

    pbr_input.material.base_color = vec4<f32>(albedo, 1.0);
    pbr_input.material.perceptual_roughness = clamp(
        mix(
            settings.roughness + (grain.x - 0.5) * settings.roughness_spread,
            ROUGHNESS_WET,
            wet * 0.85,
        ),
        0.1,
        1.0,
    );
    pbr_input.N = perturb(normal, gradient, settings.normal_strength);
    pbr_input.material.base_color = alpha_discard(pbr_input.material, pbr_input.material.base_color);

    var out: FragmentOutput;
    out.color = apply_pbr_lighting(pbr_input);
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);
    return out;
}
