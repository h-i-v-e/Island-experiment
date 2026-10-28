#import planet_bevy::environment_material::material_cloud_light
// Motu/Terrain Grass: sixteen alpha-cut shells, radial growth and tangent wind.
#import bevy_pbr::{forward_io::{Vertex, VertexOutput, FragmentOutput}, mesh_functions,
    mesh_view_bindings::globals, view_transformations::position_world_to_clip,
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::{apply_pbr_lighting, main_pass_post_lighting_processing}}
#import planet_bevy::ground::{ground, cliff_noise, cliff_sampler, volume_noise, build_coverage,
    detail_environment, height_weights, layer_sample, ground_mask, ground_projection, GRASS_A, GRASS_B}
#import planet_bevy::radial::radial_mapping
struct GrassSettings { player: vec4<f32>, shape: vec4<f32>, layer: vec4<f32> }
@group(#{MATERIAL_BIND_GROUP}) @binding(115) var<uniform> grass: GrassSettings;
struct GrassOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) world_position: vec4<f32>,
    @location(1) world_normal: vec3<f32>,
    @location(2) surface_position: vec3<f32>,
    @location(3) material: vec4<f32>,
    @location(4) wind: vec3<f32>,
    @location(5) @interpolate(flat) instance_index: u32,
}
@vertex fn vertex(input: Vertex) -> GrassOutput {
    var out: GrassOutput;
    let transform = mesh_functions::get_world_from_local(input.instance_index);
    let root = mesh_functions::mesh_position_local_to_world(transform, vec4<f32>(input.position, 1.0)).xyz;
    let normal = mesh_functions::mesh_normal_local_to_world(input.normal, input.instance_index);
    let gust = textureSampleLevel(cliff_noise, cliff_sampler, root * ground.radius / 30.0 + vec3<f32>(globals.time * 0.015, 0.0, globals.time * 0.009), 0.0).g;
    let wind_direction = normalize(vec3<f32>(0.7, 0.1, 0.3));
    let tangent = wind_direction - normal * dot(wind_direction, normal);
    let wind = tangent * inverseSqrt(max(dot(tangent, tangent), 0.0001));
    let fraction = grass.layer.x;
    let world = root + normal * (grass.shape.x * fraction / ground.radius)
        + wind * (0.035 * gust * fraction * fraction / ground.radius);
    out.position = position_world_to_clip(world);
    out.world_position = vec4<f32>(world, 1.0);
    out.world_normal = normal;
    out.surface_position = root;
    out.material = input.color;
    out.wind = wind * gust * fraction;
    out.instance_index = input.instance_index;
    return out;
}
fn grass_hash(cell: vec2<f32>) -> f32 {
    var value = fract(vec3<f32>(cell.x, cell.y, cell.x) * 0.1031);
    value += dot(value, value.yzx + 33.33);
    return fract((value.x + value.y) * value.z);
}
fn grass_noise(p: vec2<f32>) -> f32 {
    let cell = floor(p); var f = fract(p);
    f = f*f*f*(f*(f*6.0-15.0)+10.0);
    return mix(mix(grass_hash(cell),grass_hash(cell+vec2<f32>(1.0,0.0)),f.x),
        mix(grass_hash(cell+vec2<f32>(0.0,1.0)),grass_hash(cell+1.0),f.x),f.y);
}
fn blade_field(p: vec2<f32>, layer: f32) -> f32 {
    let regular = p * grass.shape.y;
    let warp = vec2<f32>(grass_noise(regular*0.18+vec2<f32>(13.7,4.3)), grass_noise(regular*0.18+vec2<f32>(2.1,19.4))) - 0.5;
    let coordinate = regular + warp*1.65;
    let cell = floor(coordinate);
    let offset = vec2<f32>(grass_hash(cell+11.3),grass_hash(cell+29.7))-0.5;
    var position = fract(coordinate)-0.5-offset*0.25;
    let angle = grass_hash(cell+vec2<f32>(7.9,37.1))*6.2831853;
    position -= vec2<f32>(cos(angle),sin(angle))*layer*layer*0.18;
    let height = mix(0.55,1.0,grass_hash(cell));
    let edge = grass_noise(coordinate*3.5+vec2<f32>(layer*17.3,layer*31.7));
    let radius = mix(0.48,0.48*0.55,layer)*mix(0.72,1.18,edge);
    return min(radius-length(position),height-layer);
}
@fragment fn fragment(input: GrassOutput, @builtin(front_facing) front: bool) -> FragmentOutput {
    let p = input.surface_position * ground.radius;
    let up = normalize(p);
    let distance = length(input.surface_position - grass.player.xyz)*ground.radius;
    let radial = 1.0-smoothstep(grass.shape.z-grass.shape.w,grass.shape.z,distance);
    let normal = normalize(input.world_normal);
    let c = build_coverage(p,clamp(dot(normal,up),0.0,1.0),length(p)-ground.radius,input.material);
    let projection = ground_projection(p, max(length(dpdx(p)),length(dpdy(p))));
    let uv = radial_mapping(p, ground.radius).uv;
    let dirt = clamp(1.0-max(c.forest,c.stones),0.0,1.0);
    let dirt_height = layer_sample(ground_mask,p,0,projection).r;
    let forest_height = layer_sample(ground_mask,p,1,projection).r;
    let stones_height = layer_sample(ground_mask,p,5,projection).r;
    let weights = height_weights(array<f32,6>(dirt,c.forest,0.0,0.0,0.0,c.stones),array<f32,6>(dirt_height,forest_height,0.5,0.5,0.5,stones_height));
    let colour_noise=volume_noise(p/4096.0).b;
    let environment = detail_environment(p);
    if radial < grass_hash(floor(uv * grass.shape.y)+vec2<f32>(43.17,19.73)) { discard; }
    if length(p)<ground.radius || max(max(c.rock,c.river),max(c.beach,c.snow))>=0.5 || environment.g>0.35 { discard; }
    let cover = smoothstep(0.35,0.65,c.grass)*weights[0]*(1.0-c.snow);
    if cover<0.5 { discard; }
    // One radial tuft field; both hemispheres meet as a mirror at the equator.
    let layer = grass.layer.x;
    let field = blade_field(uv,layer);
    if field<0.0 { discard; }
    var vertex_out: VertexOutput;
    vertex_out.position=input.position; vertex_out.world_position=input.world_position; vertex_out.world_normal=normal;
#ifdef VERTEX_COLORS
    vertex_out.color=vec4<f32>(1.0);
#endif
#ifdef VERTEX_UVS_A
    vertex_out.uv=vec2<f32>(0.0);
#endif
#ifdef VERTEX_OUTPUT_INSTANCE_INDEX
    vertex_out.instance_index=input.instance_index;
#endif
    var pbr = pbr_input_from_standard_material(vertex_out,front);
    let variation=mix(0.90,1.10,grass_hash(floor(uv*grass.shape.y)+17.0));
    let colour=mix(GRASS_A,GRASS_B,smoothstep(0.1,0.9,colour_noise))*mix(0.72,1.18,layer)*variation;
    pbr.material.base_color=vec4<f32>(colour,1.0);
    pbr.material.perceptual_roughness=0.98;
    pbr.N=normalize(mix(normal,up,0.35)+input.wind*0.08);
    var out: FragmentOutput;
    let lighting=apply_pbr_lighting(pbr);
    out.color=main_pass_post_lighting_processing(pbr,vec4<f32>(lighting.rgb*material_cloud_light(input.world_position.xyz),lighting.a));
    return out;
}
