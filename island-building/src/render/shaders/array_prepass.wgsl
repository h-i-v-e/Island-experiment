#import bevy_pbr::{
    prepass_io,
    pbr_prepass_functions::calculate_motion_vector,
    pbr_functions::{prepare_world_normal, calculate_tbn_mikktspace, apply_normal_mapping},
    pbr_bindings::material,
    mesh_view_bindings::view,
}

#ifdef VISIBILITY_RANGE_DITHER
#import bevy_pbr::pbr_functions::visibility_range_dither
#endif

struct ArrayRows {
    colours: array<vec4<f32>, 32>,
    mapping: array<vec4<f32>, 32>,
    surface: array<vec4<f32>, 32>,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(100) var<uniform> rows: ArrayRows;
@group(#{MATERIAL_BIND_GROUP}) @binding(103) var normals: texture_2d_array<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(105) var map_sampler: sampler;

// The array path only accepts opaque, single-sided materials. Masked straw and
// transparent glass keep Bevy's original alpha-aware depth/shadow shaders.
#ifdef PREPASS_FRAGMENT
@fragment
fn fragment(in: prepass_io::VertexOutput, @builtin(front_facing) is_front: bool) -> prepass_io::FragmentOutput {
#ifdef VISIBILITY_RANGE_DITHER
    visibility_range_dither(in.position, in.visibility_range_dither);
#endif
    var out: prepass_io::FragmentOutput;
#ifdef UNCLIPPED_DEPTH_ORTHO_EMULATION
    out.frag_depth = in.unclipped_depth;
#endif
#ifdef NORMAL_PREPASS
    let world_normal = prepare_world_normal(in.world_normal, false, is_front);
    var normal = world_normal;
#ifdef VERTEX_TANGENTS
    let row = u32(clamp(round(in.uv_b.y), 0.0, 31.0));
    let uv = in.uv * rows.mapping[row].xy;
    let surface = rows.surface[row];
    let sample = textureSampleBias(normals, map_sampler, uv, i32(max(surface.z, 0.0)), view.mip_bias).rgb;
    let tbn = calculate_tbn_mikktspace(world_normal, in.world_tangent);
    let mapped = apply_normal_mapping(material.flags, tbn, false, is_front, sample);
    normal = select(world_normal, mapped, surface.z >= 0.0);
#endif
    out.normal = vec4(normal * 0.5 + vec3(0.5), 1.0);
#endif
#ifdef MOTION_VECTOR_PREPASS
    out.motion_vector = calculate_motion_vector(in.world_position, in.previous_world_position);
#endif
    return out;
}
#else
@fragment
fn fragment(in: prepass_io::VertexOutput) {
#ifdef VISIBILITY_RANGE_DITHER
    visibility_range_dither(in.position, in.visibility_range_dither);
#endif
}
#endif
