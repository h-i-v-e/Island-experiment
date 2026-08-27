#import bevy_pbr::{
    forward_io::{VertexOutput, FragmentOutput},
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::{alpha_discard, apply_pbr_lighting, main_pass_post_lighting_processing},
}

fn smooth_response(value: f32) -> f32 {
    return value * value * (3.0 - 2.0 * value);
}

@fragment
fn fragment(
    in: VertexOutput,
    @builtin(front_facing) is_front: bool,
) -> FragmentOutput {
    var pbr_input = pbr_input_from_standard_material(in, is_front);
    var maturity = 1.0;

#ifdef VERTEX_COLORS
    maturity = smooth_response(clamp(in.color.a, 0.0, 1.0));
#endif

    let geometric_normal = normalize(in.world_normal);
    let relief = mix(0.18, 1.65, maturity);
    pbr_input.world_normal = geometric_normal;
    pbr_input.N = normalize(geometric_normal + (pbr_input.N - geometric_normal) * relief);

    let bark = pbr_input.material.base_color.rgb;
    let luminance = dot(bark, vec3<f32>(0.2126, 0.7152, 0.0722));
    let cavity_hint = 1.0 - smoothstep(0.025, 0.105, luminance);
    let cavity = cavity_hint * maturity;
    let lichen_hint = smoothstep(-0.006, 0.018, bark.g - bark.r);
    let upward = smoothstep(-0.20, 0.72, geometric_normal.y);
    let lichen = lichen_hint * maturity * mix(0.28, 1.0, upward);
    let cavity_colour = bark * vec3<f32>(0.79, 0.76, 0.72);
    let lichen_colour = bark * vec3<f32>(0.94, 1.08, 0.82) + vec3<f32>(0.003, 0.004, 0.002);
    let coloured_bark = mix(mix(bark, cavity_colour, cavity * 0.22), lichen_colour, lichen * 0.28);

    // Vertex alpha is a data lane, not optical opacity. Restore the opaque
    // material contract after reading maturity from it.
    pbr_input.material.base_color = vec4<f32>(coloured_bark, 1.0);
    let young_roughness = pbr_input.material.perceptual_roughness * 0.90;
    let mature_roughness = min(pbr_input.material.perceptual_roughness + 0.055, 0.98);
    pbr_input.material.perceptual_roughness = clamp(
        mix(young_roughness, mature_roughness, maturity) + cavity * 0.025 + lichen * 0.018,
        0.62,
        0.99,
    );

    pbr_input.material.base_color = alpha_discard(
        pbr_input.material,
        pbr_input.material.base_color,
    );
    var out: FragmentOutput;
    out.color = apply_pbr_lighting(pbr_input);
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);
    return out;
}
