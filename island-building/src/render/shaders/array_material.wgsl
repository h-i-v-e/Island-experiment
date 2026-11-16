#import bevy_pbr::{
    forward_io::{VertexOutput, FragmentOutput},
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::{apply_pbr_lighting, main_pass_post_lighting_processing, calculate_tbn_mikktspace, apply_normal_mapping},
    mesh_functions::{get_world_from_local, get_local_from_world},
    mesh_view_bindings::view,
}
#import island_building::grub::{GrubUniform, grub_weights}

#ifdef SCREEN_SPACE_AMBIENT_OCCLUSION
#import bevy_pbr::{
    mesh_view_bindings::screen_space_ambient_occlusion_texture,
    ssao_utils::ssao_multibounce,
    lighting::perceptualRoughnessToRoughness,
}
#endif

#ifdef VISIBILITY_RANGE_DITHER
#import bevy_pbr::pbr_functions::visibility_range_dither
#endif

struct ArrayRows {
    colours: array<vec4<f32>, 32>,
    mapping: array<vec4<f32>, 32>,
    surface: array<vec4<f32>, 32>,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(100) var<uniform> rows: ArrayRows;
@group(#{MATERIAL_BIND_GROUP}) @binding(101) var<uniform> grub: GrubUniform;
@group(#{MATERIAL_BIND_GROUP}) @binding(102) var albedo: texture_2d_array<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(103) var normals: texture_2d_array<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(104) var masks: texture_2d_array<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(105) var map_sampler: sampler;

@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
#ifdef VISIBILITY_RANGE_DITHER
    visibility_range_dither(in.position, in.visibility_range_dither);
#endif
    var pbr = pbr_input_from_standard_material(in, is_front);
    // Each source part has its own vertices, so every triangle carries a
    // constant integer row. UV1.x remains absolute native floor height.
    let row = u32(clamp(round(in.uv_b.y), 0.0, 31.0));
    let mapping = rows.mapping[row];
    let surface = rows.surface[row];
    let uv = in.uv * mapping.xy;
    let layer = i32(max(surface.z, 0.0));
    // Evaluate samples before the per-triangle branch to keep derivatives valid
    // across adjacent triangles with different material IDs.
    let colour = textureSampleBias(albedo, map_sampler, uv, layer, view.mip_bias);
#ifndef LOAD_PREPASS_NORMALS
    let normal = textureSampleBias(normals, map_sampler, uv, layer, view.mip_bias).rgb;
#endif
    let mask = textureSampleBias(masks, map_sampler, uv, layer, view.mip_bias);
    pbr.material.base_color = rows.colours[row];
    pbr.material.perceptual_roughness = surface.x;
    pbr.material.metallic = surface.y;
    pbr.material.reflectance = vec3(mapping.z);
    pbr.diffuse_occlusion = vec3(1.0);
    pbr.specular_occlusion = 1.0;
    if surface.z >= 0.0 {
        pbr.material.base_color *= colour;
        pbr.material.perceptual_roughness *= mask.g;
        pbr.material.metallic *= mask.b;
        pbr.diffuse_occlusion = vec3(mask.r);
#ifdef VERTEX_TANGENTS
#ifndef LOAD_PREPASS_NORMALS
        let tbn = calculate_tbn_mikktspace(pbr.world_normal, in.world_tangent);
        pbr.N = apply_normal_mapping(pbr.material.flags, tbn, false, is_front, normal);
#endif
#endif
    }
#ifdef SCREEN_SPACE_AMBIENT_OCCLUSION
    // Match Bevy's AO combination after resolving this triangle's albedo and
    // roughness, rather than retaining the neutral base material's SSAO values.
    let ndotv = max(dot(pbr.N, pbr.V), 0.0001);
    let ssao = textureLoad(screen_space_ambient_occlusion_texture, vec2<i32>(in.position.xy), 0i).r;
    pbr.diffuse_occlusion = min(pbr.diffuse_occlusion, ssao_multibounce(ssao, pbr.material.base_color.rgb));
    let roughness = perceptualRoughnessToRoughness(pbr.material.perceptual_roughness);
    pbr.specular_occlusion = clamp(pow(ndotv + ssao, exp2(-16.0 * roughness - 1.0)) - 1.0 + ssao, 0.0, 1.0);
#endif
    if mapping.w > 0.5 && max(grub.strengths.x, grub.strengths.y) > 0.0 {
        var position = (get_local_from_world(in.instance_index) * in.world_position).xyz;
        position.y = in.uv_b.x;
        let up = normalize((get_world_from_local(in.instance_index) * vec4(0.0, 1.0, 0.0, 0.0)).xyz);
        let weights = grub_weights(position, position.y, dot(normalize(pbr.N), up), grub);
        let dirty = mix(pbr.material.base_color.rgb, grub.dirt.rgb, weights.x);
        pbr.material.base_color = vec4(mix(dirty, grub.moss.rgb, weights.y), pbr.material.base_color.a);
        pbr.material.perceptual_roughness = mix(pbr.material.perceptual_roughness, 0.96, max(weights.x, weights.y));
    }
    var out: FragmentOutput;
    out.color = apply_pbr_lighting(pbr);
    out.color = main_pass_post_lighting_processing(pbr, out.color);
    return out;
}
