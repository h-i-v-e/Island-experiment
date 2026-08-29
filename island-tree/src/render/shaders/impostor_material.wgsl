#import bevy_pbr::{
    forward_io::{Vertex, VertexOutput, FragmentOutput},
    mesh_functions,
    mesh_view_bindings::view,
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::{alpha_discard, main_pass_post_lighting_processing},
    view_transformations::position_world_to_clip,
}

#ifdef VISIBILITY_RANGE_DITHER
#import bevy_pbr::pbr_functions::visibility_range_dither;
#endif

const ATLAS_GRID: vec2<f32> = vec2<f32>(4.0, 2.0);
const ADJACENT_VIEW_COSINE: f32 = 0.70710678;
const VIEW_STEP_RADIANS: f32 = 0.78539816;

fn screen_hash(pixel: vec2<f32>) -> f32 {
    let value = dot(floor(pixel), vec2<f32>(12.9898, 78.233));
    return fract(sin(value) * 43758.5453);
}

@vertex
fn vertex(vertex: Vertex) -> VertexOutput {
    var out: VertexOutput;
    let world_from_local = mesh_functions::get_world_from_local(vertex.instance_index);
    let origin = mesh_functions::mesh_position_local_to_world(
        world_from_local,
        vec4<f32>(0.0, 0.0, 0.0, 1.0),
    );
    let camera_offset = view.world_position - origin.xyz;
    let camera_forward = normalize(vec3<f32>(camera_offset.x, 0.0, camera_offset.z));
    let billboard_right = vec3<f32>(camera_forward.z, 0.0, -camera_forward.x);
    let horizontal_scale = length(world_from_local[0].xyz);
    let vertical_scale = length(world_from_local[1].xyz);
    out.world_position = vec4<f32>(
        origin.xyz
            + billboard_right * vertex.position.x * horizontal_scale
            + vec3<f32>(0.0, vertex.position.y * vertical_scale, 0.0),
        1.0,
    );
    out.position = position_world_to_clip(out.world_position.xyz);
    out.world_normal = mesh_functions::mesh_normal_local_to_world(
        vertex.normal,
        vertex.instance_index,
    );
    out.uv = vertex.uv;

#ifdef VERTEX_OUTPUT_INSTANCE_INDEX
    out.instance_index = vertex.instance_index;
#endif
#ifdef VISIBILITY_RANGE_DITHER
    out.visibility_range_dither = mesh_functions::get_visibility_range_dither_level(
        vertex.instance_index,
        world_from_local[3],
    );
#endif
    return out;
}

@fragment
fn fragment(
    in: VertexOutput,
    @builtin(front_facing) is_front: bool,
) -> FragmentOutput {
#ifdef VISIBILITY_RANGE_DITHER
    visibility_range_dither(in.position, in.visibility_range_dither);
#endif

    let camera_offset = view.world_position - in.world_position.xyz;
    let horizontal_view = normalize(vec3<f32>(camera_offset.x, 0.0, camera_offset.z));
    let horizontal_normal = normalize(vec3<f32>(in.world_normal.x, 0.0, in.world_normal.z));
    let facing = dot(horizontal_normal, horizontal_view);

    // Backface culling leaves four candidate cards. Only the closest one or
    // two of the eight directed atlas views survive this angular threshold.
    if facing <= ADJACENT_VIEW_COSINE {
        discard;
    }

    // Adjacent tiles have opposite parity. Using complementary noise means
    // their coverage sums to one during the angular transition: no dark gap,
    // transparent double image, or order-dependent alpha blend is introduced.
    let angle = acos(clamp(facing, ADJACENT_VIEW_COSINE, 1.0));
    let weight = 1.0 - angle / VIEW_STEP_RADIANS;
    let tile = floor(min(in.uv * ATLAS_GRID, ATLAS_GRID - vec2<f32>(0.001)));
    let tile_index = u32(tile.x + tile.y * ATLAS_GRID.x);
    let noise = screen_hash(in.position.xy);
    let threshold = select(noise, 1.0 - noise, tile_index % 2u == 1u);
    if threshold > weight {
        discard;
    }

    var pbr_input = pbr_input_from_standard_material(in, is_front);
    pbr_input.material.base_color = alpha_discard(
        pbr_input.material,
        pbr_input.material.base_color,
    );
    var out: FragmentOutput;
    out.color = pbr_input.material.base_color;
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);
    return out;
}
