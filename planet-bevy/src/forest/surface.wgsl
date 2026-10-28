#import bevy_pbr::{forward_io::{Vertex,VertexOutput,FragmentOutput},mesh_functions,
    view_transformations::position_world_to_clip,
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::{apply_pbr_lighting,main_pass_post_lighting_processing}}
#import planet_bevy::forest::{forest,tree_wind,tree_clip,tree_noise}
#import planet_bevy::environment_material::material_cloud_light
@vertex fn vertex(input:Vertex)->VertexOutput {
    var out:VertexOutput;
    let transform=mesh_functions::get_world_from_local(input.instance_index);
    let root=transform[3].xyz;
    let world=mesh_functions::mesh_position_local_to_world(transform,vec4<f32>(input.position,1.0)).xyz;
    out.world_position=vec4<f32>(world+tree_wind(world,root),1.0);
    out.position=position_world_to_clip(out.world_position.xyz);
    out.world_normal=mesh_functions::mesh_normal_local_to_world(input.normal,input.instance_index);
#ifdef VERTEX_COLORS
    out.color=input.color;
#endif
#ifdef VERTEX_OUTPUT_INSTANCE_INDEX
    out.instance_index=input.instance_index;
#endif
    return out;
}
@fragment fn fragment(input:VertexOutput,@builtin(front_facing) front:bool)->FragmentOutput {
    let root=mesh_functions::get_world_from_local(input.instance_index)[3].xyz;
    tree_clip(input.world_position.xyz,root,input.color.r);
    let p=input.world_position.xyz*forest.shape.x;
    let broad=tree_noise(p/24.0);
    let detail=tree_noise(p/4.8+0.173);
    var pbr=pbr_input_from_standard_material(input,front);
    var colour:vec3<f32>;
    if forest.shape.y<0.5 {
        let up=normalize(root);
        let height=dot(input.world_position.xyz-root,up)*forest.shape.x;
        let tangent=p-up*height*0.96;
        let bark=tree_noise(tangent/0.8);
        colour=mix(vec3<f32>(0.021,0.011,0.006),vec3<f32>(0.10,0.061,0.027),bark.r)*mix(0.75,1.15,detail.g);
    } else {
        let light=clamp(0.25+broad.g*0.55+detail.r*0.20,0.0,1.0);
        colour=mix(vec3<f32>(0.008,0.038,0.006),vec3<f32>(0.064,0.196,0.022),light);
        colour*=mix(0.88,1.12,input.color.r);
        pbr.N=normalize(pbr.N+(detail-0.5)*0.24);
        // Leaf transmission remains sun-dependent, so the night side stays dark.
        pbr.material.diffuse_transmission=0.22;
        pbr.material.thickness=0.08;
    }
    pbr.material.base_color=vec4<f32>(colour,1.0);
    pbr.material.perceptual_roughness=0.94;
    var out:FragmentOutput;
    let lit=apply_pbr_lighting(pbr);
    out.color=main_pass_post_lighting_processing(pbr,vec4<f32>(lit.rgb*material_cloud_light(input.world_position.xyz),lit.a));
    return out;
}
