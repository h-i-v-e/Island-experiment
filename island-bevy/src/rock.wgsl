// River rocks. The generator hands over one merged mesh, so the only per-body
// signal available is the deterministic tint `convert` hashes into the colour
// attribute; everything finer is world-space noise.

#import bevy_pbr::{
    forward_io::{VertexOutput, FragmentOutput},
    mesh_view_bindings::view,
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::{alpha_discard, apply_pbr_lighting, main_pass_post_lighting_processing},
}
#import island_bevy::noise::{fbm_gradient, noise_gradient, perturb}

// Linear-space minerals, each written above as the sRGB triple it came from.
const STONE_COOL: vec3<f32> = vec3<f32>(0.13287, 0.13998, 0.13998); // 0.40, 0.41, 0.41
const STONE_WARM: vec3<f32> = vec3<f32>(0.15487, 0.11280, 0.06838); // 0.43, 0.37, 0.29

/// Wavelengths of the two detail layers, in metres. The generator settles
/// stones of six to twenty-two centimetres and the occasional boulder up to
/// sixty-five, so both layers have to sit inside one hand-sized body.
const GRAIN_METRES: f32 = 0.055;
const MICRO_METRES: f32 = 0.011;

struct RockSettings {
    /// Metres at which the sub-metre layer has faded out entirely.
    detail_range: f32,
    normal_strength: f32,
    roughness: f32,
    roughness_spread: f32,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(100) var<uniform> settings: RockSettings;

@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
    var pbr_input = pbr_input_from_standard_material(in, is_front);

#ifdef VERTEX_COLORS
    let tint = in.color.rgb;
#else
    let tint = vec3<f32>(1.0);
#endif

    let world = in.world_position.xyz;
    let normal = normalize(pbr_input.world_normal);
    let range = length(world - view.world_position);

    // Both detail layers are finer than a pixel well before the body itself
    // is, so they are faded out rather than left to alias into the temporal
    // resolve. The per-body tint carries the variation that survives distance.
    let near = 1.0 - smoothstep(0.0, max(settings.detail_range, 1.0), range);
    let grain = fbm_gradient(world / GRAIN_METRES, 3);
    var albedo = mix(STONE_COOL, STONE_WARM, clamp(grain.x * 1.5 - 0.25, 0.0, 1.0))
        * tint
        * (1.0 + (grain.x - 0.5) * 0.44 * near);
    var gradient = grain.yzw * near;

    if near > 0.4 {
        let micro = noise_gradient(world / MICRO_METRES);
        let closeness = (near - 0.4) / 0.6;
        albedo *= 1.0 + (micro.x - 0.5) * 0.18 * closeness;
        gradient += micro.yzw * 0.22 * closeness;
    }

    pbr_input.material.base_color = vec4<f32>(albedo, 1.0);
    pbr_input.material.perceptual_roughness =
        clamp(settings.roughness + (grain.x - 0.5) * settings.roughness_spread, 0.2, 1.0);
    pbr_input.N = perturb(normal, gradient, settings.normal_strength);
    pbr_input.material.base_color = alpha_discard(pbr_input.material, pbr_input.material.base_color);

    var out: FragmentOutput;
    out.color = apply_pbr_lighting(pbr_input);
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);
    return out;
}
