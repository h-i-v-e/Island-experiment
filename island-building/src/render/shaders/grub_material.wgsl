#import bevy_pbr::{
    forward_io::{VertexOutput, FragmentOutput},
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::{alpha_discard, apply_pbr_lighting, main_pass_post_lighting_processing},
    pbr_types::STANDARD_MATERIAL_FLAGS_UNLIT_BIT,
    mesh_functions::{get_world_from_local, get_local_from_world},
}
#import island_building::grub::{GrubUniform, grub_weights}

#ifdef VISIBILITY_RANGE_DITHER
#import bevy_pbr::pbr_functions::visibility_range_dither
#endif

@group(#{MATERIAL_BIND_GROUP}) @binding(100) var<uniform> grub: GrubUniform;

@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
#ifdef VISIBILITY_RANGE_DITHER
    visibility_range_dither(in.position, in.visibility_range_dither);
#endif
    var pbr = pbr_input_from_standard_material(in, is_front);
    pbr.material.base_color = alpha_discard(pbr.material, pbr.material.base_color);
    if max(grub.strengths.x, grub.strengths.y) > 0.0 {
        var position = (get_local_from_world(in.instance_index) * in.world_position).xyz;
        // UV1 carries absolute building height even for separately hinged leaves.
#ifdef VERTEX_UVS_B
        position.y = in.uv_b.x;
#endif
        let up = normalize((get_world_from_local(in.instance_index) * vec4(0.0, 1.0, 0.0, 0.0)).xyz);
        let weights = grub_weights(position, position.y, dot(normalize(pbr.N), up), grub);
        let dirty = mix(pbr.material.base_color.rgb, grub.dirt.rgb, weights.x);
        pbr.material.base_color = vec4(mix(dirty, grub.moss.rgb, weights.y), pbr.material.base_color.a);
        pbr.material.perceptual_roughness = mix(pbr.material.perceptual_roughness, 0.96, max(weights.x, weights.y));
    }
    var out: FragmentOutput;
    if (pbr.material.flags & STANDARD_MATERIAL_FLAGS_UNLIT_BIT) == 0u {
        out.color = apply_pbr_lighting(pbr);
    } else {
        out.color = pbr.material.base_color;
    }
    out.color = main_pass_post_lighting_processing(pbr, out.color);
    return out;
}
