#import bevy_pbr::{prepass_io::{Vertex,VertexOutput,FragmentOutput},mesh_functions,
    mesh_view_bindings::view,view_transformations::position_world_to_clip,
    pbr_prepass_functions::calculate_motion_vector}
#import planet_bevy::forest::{tree_wind,tree_clip}
@vertex fn vertex(input:Vertex)->VertexOutput {
    var out:VertexOutput;
    let transform=mesh_functions::get_world_from_local(input.instance_index);
    let root=transform[3].xyz;
    let world=mesh_functions::mesh_position_local_to_world(transform,vec4<f32>(input.position,1.0)).xyz;
    out.world_position=vec4<f32>(world+tree_wind(world,root),1.0);
    out.position=position_world_to_clip(out.world_position.xyz);
#ifdef UNCLIPPED_DEPTH_ORTHO_EMULATION
    out.unclipped_depth=out.position.z;
    out.position.z=min(out.position.z,1.0);
#endif
#ifdef NORMAL_PREPASS_OR_DEFERRED_PREPASS
    out.world_normal=mesh_functions::mesh_normal_local_to_world(input.normal,input.instance_index);
#endif
#ifdef VERTEX_OUTPUT_INSTANCE_INDEX
    out.instance_index=input.instance_index;
#endif
#ifdef VERTEX_COLORS
    out.color=input.color;
#endif
#ifdef MOTION_VECTOR_PREPASS
    out.previous_world_position=out.world_position;
#endif
    return out;
}
#ifdef PREPASS_FRAGMENT
@fragment fn fragment(input:VertexOutput)->FragmentOutput {
    let root=mesh_functions::get_world_from_local(input.instance_index)[3].xyz;
    tree_clip(input.world_position.xyz,root,input.color.r);
    var out:FragmentOutput;
#ifdef NORMAL_PREPASS
    out.normal=vec4<f32>(input.world_normal*0.5+0.5,1.0);
#endif
#ifdef UNCLIPPED_DEPTH_ORTHO_EMULATION
    out.frag_depth=input.unclipped_depth;
#endif
#ifdef MOTION_VECTOR_PREPASS
    out.motion_vector=calculate_motion_vector(input.world_position,input.previous_world_position);
#endif
    return out;
}
#else
@fragment fn fragment(input:VertexOutput) {
    let root=mesh_functions::get_world_from_local(input.instance_index)[3].xyz;
    tree_clip(input.world_position.xyz,root,input.color.r);
}
#endif
