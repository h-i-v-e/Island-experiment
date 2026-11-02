#define_import_path planet_bevy::forest
#import planet_bevy::environment_material::{environment,material_cloud_light}
#import planet_bevy::weather::{weather_enabled,weather_wind}
#import bevy_pbr::mesh_view_bindings::view
struct ForestUniform {shape:vec4<f32>,viewer:vec4<f32>}
@group(#{MATERIAL_BIND_GROUP}) @binding(100) var<uniform> forest:ForestUniform;
@group(#{MATERIAL_BIND_GROUP}) @binding(110) var forest_noise:texture_3d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(111) var forest_sampler:sampler;
fn tree_noise(p:vec3<f32>)->vec3<f32> {
    return textureSampleLevel(forest_noise,forest_sampler,p,0.0).rgb;
}
// Root-anchored, radial-up wind; identical in colour, final depth and shadows.
fn tree_wind(point:vec3<f32>,root:vec3<f32>,progress:f32)->vec3<f32> {
    if forest.shape.y>4.5 && forest.shape.y<5.5 {return vec3<f32>(0.0);}
    let up=normalize(root);
    let height=max(dot(point-root,up)*forest.shape.x,0.0);
    let wind=weather_wind(root);
    let wind_strength=select(1.0,clamp(length(wind)/10.0,0.0,3.0),weather_enabled());
    let tangent=normalize(wind-up*dot(wind,up)+vec3<f32>(0.00001));
    let phase=dot(root*forest.shape.x,vec3<f32>(0.031,0.027,0.043))+environment.air.w*1.3;
    if forest.shape.y>3.5 {
        let bend=progress*progress*select(1.0,select(2.8,1.8,forest.shape.y>6.5),forest.shape.y>5.5);
        return tangent*(sin(phase)*0.045+sin(phase*2.37)*0.015)*bend*wind_strength/forest.shape.x;
    }
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
fn tree_clip(point:vec3<f32>,root:vec3<f32>,layer:f32,species:f32) {
    if species<0.5 {return;}
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

// Island's analytic leaflet silhouette: no textures or fur shells required.
fn fern_silhouette(uv:vec2<f32>,data:vec4<f32>)->f32 {
    let edge=max(fwidth(uv.x),fwidth(uv.y))*1.5;
    let taper=pow(clamp(1.0-uv.y,0.0,1.0),0.56);
    let rachis=mix(0.036,0.015,uv.y);
    var mask=1.0-smoothstep(rachis,rachis+max(edge,0.0001),abs(uv.x-0.5));
    for(var leaflet=0u;leaflet<8u;leaflet+=1u) {
        let progress=f32(leaflet+1u)/10.0;
        let width=(0.34*taper+0.04)*mix(0.92,1.08,fract(data.x*7.3+f32(leaflet)*0.37));
        let height=mix(0.055,0.032,progress);
        let offset=select(-0.010,0.010,(leaflet&1u)!=0u);
        let left=vec2<f32>((uv.x-(0.5-width*0.52))/max(width,0.01),(uv.y-progress-offset)/height);
        let right=vec2<f32>((uv.x-(0.5+width*0.52))/max(width,0.01),(uv.y-progress+offset)/height);
        mask=max(mask,max(1.0-smoothstep(0.82,1.0,dot(left,left)),1.0-smoothstep(0.82,1.0,dot(right,right))));
    }
    let tip=vec2<f32>((uv.x-0.5)/0.11,(uv.y-0.94)/0.09);
    mask=max(mask,1.0-smoothstep(0.72,1.0,dot(tip,tip)));
    return mask*smoothstep(0.0,0.025,uv.y);
}
fn reed_hash(v:f32)->f32 {return fract(sin(v*91.3458+17.13)*47453.5453);}
fn reed_head(uv:vec2<f32>,data:vec4<f32>)->vec2<f32> {
    var mask=0.0;var heads=0.0;
    for(var i=0u;i<7u;i+=1u) {
        let seed=f32(i)+data.w*19.0+data.y*37.0+data.x*11.0;
        let centre=(f32(i)+0.5)/7.0+(reed_hash(seed)-0.5)*0.075;
        let height=mix(0.58,0.88,reed_hash(seed+2.7))*mix(1.0,0.78,data.x);
        let lean=(reed_hash(seed+7.1)-0.5)*0.12*uv.y;
        let width=mix(0.010,0.025,reed_hash(seed+4.2))*mix(0.55,1.0,data.x);
        let progress=clamp(uv.y/height,0.0,1.0);
        let taper=mix(1.0,0.03,smoothstep(0.55,1.0,progress));
        var stem=1.0-smoothstep(width*taper,width*taper+max(fwidth(uv.x)*1.5,0.0001),abs(uv.x-centre-lean));
        stem*=1.0-smoothstep(0.92,1.0,progress);
        stem*=1.0-smoothstep(height,height+max(fwidth(uv.y)*2.0,0.0001),uv.y);
        stem*=step(0.015,uv.y);
        let delta=vec2<f32>((uv.x-centre-lean)/0.035,(uv.y-height+0.015)/0.075);
        let head=(1.0-smoothstep(0.70,1.0,dot(delta,delta)))*(1.0-data.x);
        heads=max(heads,head);mask=max(mask,max(stem,head));
    }
    return vec2<f32>(mask,heads);
}
fn undergrowth_clip(point:vec3<f32>,root:vec3<f32>,uv:vec2<f32>,data:vec4<f32>) {
    if forest.shape.y<3.5 {return;}
    if forest.shape.y<4.5 && fern_silhouette(uv,data)<0.44 {discard;}
    if forest.shape.y>5.5 && reed_head(uv,data).x<0.44 {discard;}
    let distance=length(root-forest.viewer.xyz)*forest.shape.x;
    let fade=1.0-smoothstep(forest.shape.z*0.80,forest.shape.z,distance);
    // Stable world-space dither agrees in colour, depth and shadow passes.
    let cell=floor(point*forest.shape.x*64.0);
    let threshold=fract(sin(dot(cell,vec3<f32>(12.9898,78.233,37.719)))*43758.5453);
    if fade<=threshold {discard;}
}

// Integer hashing avoids the repeating bands that large sin() seeds acquire
// when physical UVs reach thousands of needle cells around the crown.
fn pine_hash(cell:vec2<i32>)->vec3<f32> {
    let key=bitcast<vec2<u32>>(cell);
    var h=key.x*0x9e3779b9u ^ key.y*0x85ebca6bu;
    h=(h ^ (h>>16u))*0x7feb352du;
    h=(h ^ (h>>15u))*0x846ca68bu;
    h=h ^ (h>>16u);
    return vec3<f32>(f32(h & 0xffffu),f32((h>>16u) & 0xffffu),f32((h*0x9e3779b9u) & 0xffffu))/65535.0;
}
fn pine_strands(uv:vec2<f32>,layer:f32)->f32 {
    // Longitudinal fibres follow the slope of the branch. Warping their roots
    // breaks up rows while keeping all four layers attached during wind.
    let flow=vec2<f32>(sin(dot(uv,vec2<f32>(3.1,1.7))+sin(uv.y*2.3)),sin(dot(uv,vec2<f32>(-1.3,2.7))+cos(uv.x*4.1)));
    let position=uv*vec2<f32>(70.0,20.0)+flow*0.65+vec2<f32>(0.0,layer*0.45);
    let cluster=0.5+0.25*(sin(uv.x*2.7+sin(uv.y*3.3))+cos(uv.y*1.7+uv.x*0.8));
    let cell=vec2<i32>(floor(position));
    let edge=max(length(fwidth(position))*0.45,0.012);
    var mask=0.0;
    let direction=select(vec2<i32>(-1),vec2<i32>(1),fract(position)>=vec2<f32>(0.5));
    for(var y=0;y<=1;y+=1) {for(var x=0;x<=1;x+=1) {
        let neighbour=cell+vec2<i32>(x,y)*direction;
        let random=pine_hash(neighbour);
        let height=0.30+random.z*mix(0.30,0.70,cluster);
        let lean=(random.xy-0.5)*0.20*layer;
        let centre=vec2<f32>(neighbour)+0.1+random.xy*0.8+lean;
        let radius=0.33*pow(max(1.0-layer/(height+0.08),0.0),0.55)+0.025;
        let strand=(1.0-smoothstep(radius,radius+edge,length(position-centre)))*(1.0-smoothstep(height,height+0.04,layer));
        mask=max(mask,strand);
    }}
    return mask;
}
fn pine_clip(point:vec3<f32>,uv:vec2<f32>,data:vec4<f32>) {
    if forest.shape.y<1.5 || forest.shape.y>2.5 || data.b>0.5 {return;}
    if forest.shape.w<0.5 {discard;}
    if pine_strands(uv,data.r)<0.44 {discard;}
    let distance=length(point-forest.viewer.xyz)*forest.shape.x;
    let fade=1.0-smoothstep(14.0,28.0,distance);
    let cell=floor(point*forest.shape.x*128.0);
    let threshold=fract(sin(dot(cell,vec3<f32>(12.9898,78.233,37.719)))*43758.5453);
    if fade<=threshold {discard;}
}
