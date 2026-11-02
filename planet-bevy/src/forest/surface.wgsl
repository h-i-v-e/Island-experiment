#import bevy_pbr::{forward_io::{Vertex,VertexOutput,FragmentOutput},mesh_functions,
    view_transformations::position_world_to_clip,
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::{apply_pbr_lighting,main_pass_post_lighting_processing}}
#import planet_bevy::forest::{forest,tree_wind,tree_clip,undergrowth_clip,tree_noise,reed_head,pine_clip}
#import bevy_pbr::mesh_view_bindings::view
#import planet_bevy::environment_material::{material_cloud_light,material_ambient_weight,material_twilight_tint}
@vertex fn vertex(input:Vertex)->VertexOutput {
    var out:VertexOutput;
    let transform=mesh_functions::get_world_from_local(input.instance_index);
    let root=transform[3].xyz;
    let world=mesh_functions::mesh_position_local_to_world(transform,vec4<f32>(input.position,1.0)).xyz;
    var progress=0.0;
#ifdef VERTEX_UVS_A
    progress=input.uv.y;
    out.uv=input.uv;
#endif
    out.world_position=vec4<f32>(world+tree_wind(world,root,progress),1.0);
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
    tree_clip(input.world_position.xyz,root,input.color.r,input.color.b);
#ifdef VERTEX_UVS_A
    undergrowth_clip(input.world_position.xyz,root,input.uv,input.color);
    pine_clip(input.world_position.xyz,input.uv,input.color);
#endif
    let p=input.world_position.xyz*forest.shape.x;
    let broad=tree_noise(p/24.0);
    let detail=tree_noise(p/4.8+0.173);
    var pbr=pbr_input_from_standard_material(input,front);
    var colour:vec3<f32>;
    // Coarse trees carry the Island LOD2 trunk and crown in one shared mesh.
    let wood=forest.shape.y<0.5 || (forest.shape.y>2.5 && input.color.g<0.5);
    if forest.shape.y>1.5 && forest.shape.y<2.5 && input.color.b<0.5 {
#ifdef VERTEX_UVS_A
        colour=mix(vec3<f32>(0.008,0.035,0.017),vec3<f32>(0.025,0.075,0.029),input.color.r);
        let clusters=mix(0.88,1.08,detail.g);
        colour*=clusters;
        pbr.N=normalize(pbr.N+(detail-0.5)*0.2);
        pbr.N=normalize(pbr.N+normalize(root)*0.35);
#else
        colour=vec3<f32>(0.015,0.07,0.028);
#endif
        pbr.material.diffuse_transmission=0.30;
        pbr.material.thickness=0.015;
    } else if forest.shape.y>3.5 && forest.shape.y<4.5 {
#ifdef VERTEX_UVS_A
        colour=mix(vec3<f32>(0.012,0.065,0.008),vec3<f32>(0.11,0.28,0.035),input.uv.y*0.72);
#else
        colour=vec3<f32>(0.025,0.13,0.012);
#endif
        colour*=mix(0.84,1.16,input.color.g);
        pbr.N=normalize(pbr.N+normalize(root)*0.25);
        pbr.material.diffuse_transmission=0.26;
        pbr.material.thickness=0.025;
    } else if forest.shape.y>5.5 {
#ifdef VERTEX_UVS_A
        colour=mix(vec3<f32>(0.022,0.055,0.006),vec3<f32>(0.18,0.24,0.027),input.uv.y);
        colour=mix(colour,vec3<f32>(0.055,0.023,0.007),reed_head(input.uv,input.color).y);
#else
        colour=vec3<f32>(0.06,0.12,0.012);
#endif
        colour*=mix(0.8,1.15,input.color.g);
        pbr.N=normalize(pbr.N+normalize(root)*0.35);
        pbr.material.diffuse_transmission=0.2;
        pbr.material.thickness=0.02;
    } else if forest.shape.y>4.5 {
        let transform=mesh_functions::get_world_from_local(input.instance_index);
        let axis=normalize(transform[0].xyz);
        let relative=p-root*forest.shape.x;
        let bark=tree_noise((relative-axis*dot(relative,axis)*0.97)/0.35);
        colour=mix(vec3<f32>(0.021,0.011,0.006),vec3<f32>(0.11,0.066,0.028),bark.r);
#ifdef VERTEX_UVS_A
        if input.color.g>0.5 {
            let r=length((input.uv-0.5)*2.0);
            let grain=0.90+0.10*sin(r*65.0+detail.r*3.0);
            colour=mix(vec3<f32>(0.06,0.028,0.010),vec3<f32>(0.21,0.12,0.047),detail.g)*grain;
        }
#endif
    } else if wood {
        let up=normalize(root);
        let height=dot(input.world_position.xyz-root,up)*forest.shape.x;
        let tangent=p-up*height*0.96;
        let bark=tree_noise(tangent/0.8);
        colour=mix(vec3<f32>(0.021,0.011,0.006),vec3<f32>(0.10,0.061,0.027),bark.r)*mix(0.75,1.15,detail.g);
    } else {
        let light=clamp(0.25+broad.g*0.55+detail.r*0.20,0.0,1.0);
        colour=mix(vec3<f32>(0.008,0.038,0.006),vec3<f32>(0.064,0.196,0.022),light);
        if input.color.b < 0.5 {
            colour=mix(vec3<f32>(0.005,0.026,0.014),vec3<f32>(0.035,0.115,0.055),light);
            let close=1.0-smoothstep(12.0,30.0,distance(input.world_position.xyz,view.world_position)*forest.shape.x);
            if forest.shape.w>0.5 && forest.shape.y<1.5 && close>0.001 {
                let transform=mesh_functions::get_world_from_local(input.instance_index);
                let relative=p-root*forest.shape.x;
                let local=vec3<f32>(dot(relative,normalize(transform[0].xyz)),dot(relative,normalize(transform[1].xyz)),dot(relative,normalize(transform[2].xyz)));
                let needles=tree_noise(local*vec3<f32>(12.0,12.0,3.0)+0.37);
                colour*=mix(1.0,mix(0.65,1.35,needles.r),close);
                pbr.N=normalize(pbr.N+(needles-0.5)*0.30*close);
            }
        }
        colour*=mix(0.88,1.12,input.color.r);
        pbr.N=normalize(pbr.N+(detail-0.5)*0.24);
        // Leaf transmission remains sun-dependent, so the night side stays dark.
        pbr.material.diffuse_transmission=0.22;
        pbr.material.thickness=0.08;
    }
    pbr.material.base_color=vec4<f32>(colour,1.0);
    pbr.material.perceptual_roughness=0.94;
    var out:FragmentOutput;
    let ambient=material_ambient_weight(input.world_position.xyz);
    pbr.diffuse_occlusion*=vec3<f32>(ambient);
    pbr.specular_occlusion*=ambient;
    // Bevy's leaf-transmission ambient lobe bypasses diffuse occlusion.
    pbr.material.diffuse_transmission*=ambient;
    let lit=apply_pbr_lighting(pbr);
    out.color=main_pass_post_lighting_processing(pbr,vec4<f32>(lit.rgb*material_cloud_light(input.world_position.xyz)*material_twilight_tint(input.world_position.xyz),lit.a));
    return out;
}
