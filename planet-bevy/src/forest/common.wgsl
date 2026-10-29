#define_import_path planet_bevy::forest
#import planet_bevy::environment_material::{environment,material_cloud_light}
#import planet_bevy::weather::{weather_enabled,weather_wind}
#import bevy_pbr::mesh_view_bindings::view
struct ForestUniform {shape:vec4<f32>}
@group(#{MATERIAL_BIND_GROUP}) @binding(100) var<uniform> forest:ForestUniform;
@group(#{MATERIAL_BIND_GROUP}) @binding(110) var forest_noise:texture_3d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(111) var forest_sampler:sampler;
fn tree_noise(p:vec3<f32>)->vec3<f32> {
    return textureSampleLevel(forest_noise,forest_sampler,p,0.0).rgb;
}
// Root-anchored, radial-up wind; identical in colour, final depth and shadows.
fn tree_wind(point:vec3<f32>,root:vec3<f32>)->vec3<f32> {
    let up=normalize(root);
    let height=max(dot(point-root,up)*forest.shape.x,0.0);
    let wind=weather_wind(root);
    let wind_strength=select(1.0,clamp(length(wind)/10.0,0.0,3.0),weather_enabled());
    let tangent=normalize(wind-up*dot(wind,up)+vec3<f32>(0.00001));
    let phase=dot(root*forest.shape.x,vec3<f32>(0.031,0.027,0.043))+environment.air.w*1.3;
    let strength=min(height*height/144.0,1.0);
    return tangent*(sin(phase)*0.12+sin(phase*2.37)*0.035)*strength*wind_strength/forest.shape.x;
}
fn canopy_alpha(point:vec3<f32>,root:vec3<f32>)->f32 {
    let up=normalize(root);
    let column=(point-up*dot(point-root,up))*forest.shape.x;
    let broad=tree_noise(column/32.0+vec3<f32>(0.13,0.47,0.71));
    let detail=tree_noise(column/13.0+0.37);
    return smoothstep(0.30,0.38,broad.r*0.65+detail.b*0.35);
}
fn tree_clip(point:vec3<f32>,root:vec3<f32>,layer:f32) {
    if forest.shape.y<0.5 || forest.shape.y>2.5 {return;}
    if canopy_alpha(point,root)<0.5 {discard;}
    if forest.shape.y>1.5 {
        if distance(point,view.world_position)*forest.shape.x>forest.shape.z {discard;}
        let p=point*forest.shape.x/1.25;
        let broad=tree_noise(p+vec3<f32>(0.41,0.17,0.73));
        let detail=tree_noise(p*1.85+vec3<f32>(0.13,0.79,0.37));
        let leaves=dot(broad.rg,vec2<f32>(0.50,0.25))+detail.b*0.25;
        let threshold=mix(0.28,0.78,pow(layer,1.25));
        if leaves<threshold {discard;}
    }
}
