// Reduced-resolution spherical volume integration. Radiance is premultiplied;
// alpha stores remaining transmittance rather than ordinary surface opacity.
#import bevy_core_pipeline::fullscreen_vertex_shader::FullscreenVertexOutput
#import bevy_render::view::View
#import planet_bevy::environment_air::environment_air
#import planet_bevy::environment::{EnvironmentUniform,environment_density,environment_cloud_light,environment_extinction,environment_rotate}
@group(0) @binding(0) var scene_colour: texture_2d<f32>;
@group(0) @binding(1) var noise_sampler: sampler;
@group(0) @binding(2) var scene_depth: texture_depth_2d;
@group(0) @binding(3) var<uniform> view: View;
@group(0) @binding(4) var<uniform> weather: EnvironmentUniform;
@group(0) @binding(5) var cloud_noise: texture_3d<f32>;
@group(0) @binding(6) var cloud_colour: texture_2d<f32>;
@group(0) @binding(7) var cloud_distance: texture_2d<f32>;
struct TemporalSettings {
    previous_clip_from_world:mat4x4<f32>, previous_eye:vec4<f32>,
    previous_viewport:vec4<f32>, previous_wind:vec4<f32>, controls:vec4<f32>,
}
@group(0) @binding(8) var<uniform> temporal_settings:TemporalSettings;
@group(0) @binding(9) var history_colour:texture_2d<f32>;
@group(0) @binding(10) var history_distance:texture_2d<f32>;

fn sphere_interval(origin: vec3<f32>, direction: vec3<f32>, radius: f32) -> vec2<f32> {
    let b=dot(origin,direction);
    let discriminant=b*b-dot(origin,origin)+radius*radius;
    if discriminant<0.0 {return vec2<f32>(1.0,-1.0);}
    let root=sqrt(max(discriminant,0.0));
    return vec2<f32>(-b-root,-b+root);
}
fn viewport_uv(pixel: vec2<f32>) -> vec2<f32> {
    return (pixel-view.viewport.xy)/view.viewport.zw;
}
fn depth_pixel(uv:vec2<f32>)->vec2<i32> {
    let minimum=vec2<i32>(view.viewport.xy);
    let maximum=min(minimum+vec2<i32>(view.viewport.zw),vec2<i32>(textureDimensions(scene_depth)))-1;
    return clamp(vec2<i32>(uv*view.viewport.zw+view.viewport.xy),minimum,maximum);
}
fn scene_distance(pixel:vec2<i32>)->f32 {
    let raw=textureLoad(scene_depth,pixel,0);
    if raw<=0.0 {return 1000000.0;}
    let uv=viewport_uv(vec2<f32>(pixel)+0.5);
    let clip=vec4<f32>(uv*vec2<f32>(2.0,-2.0)+vec2<f32>(-1.0,1.0),raw,1.0);
    let p=view.view_from_clip*clip;
    return length(p.xyz/p.w)*weather.shape.x;
}
fn ray_direction(uv:vec2<f32>)->vec3<f32> {
    let clip=vec4<f32>(uv*vec2<f32>(2.0,-2.0)+vec2<f32>(-1.0,1.0),1.0,1.0);
    let p=view.view_from_clip*clip;
    return normalize((view.world_from_view*vec4<f32>(normalize(p.xyz),0.0)).xyz);
}
fn phase(pixel:vec2<u32>)->f32 {
    var n=pixel.x*1597334677u ^ pixel.y*3812015801u;
    n=(n^(n>>16u))*2246822519u;
    let shift=select(0.0,temporal_settings.controls.y*0.61803398875,temporal_settings.controls.z>0.5);
    return 0.15+fract(f32(n&65535u)/65536.0+shift)*0.7;
}
struct CloudResult {
    @location(0) colour:vec4<f32>,
    @location(1) distance:vec2<f32>,
}
struct CloudIntegration {colour:vec4<f32>,moment:f32}
fn integrate_segment(origin:vec3<f32>,direction:vec3<f32>,interval:vec2<f32>,count:u32,jitter:f32,initial:CloudIntegration)->CloudIntegration {
    if interval.y<=interval.x || count==0u {return initial;}
    let step=(interval.y-interval.x)/f32(count);
    let step_metres=step*weather.shape.x;
    let air=environment_air(origin,origin+direction*((interval.x+interval.y)*0.5),weather);
    var colour=initial.colour.rgb;var transmission=initial.colour.a;var moment=initial.moment;
    for(var i=0u;i<40u;i+=1u) {
        if i>=count || transmission<0.012 {break;}
        let distance=interval.x+(f32(i)+jitter)*step;
        let point=origin+direction*distance;
        let density=environment_density(point,weather,cloud_noise,noise_sampler);
        // Empty samples never perform the extra sun-directed density lookups.
        if density<0.001 {continue;}
        let optical=density*environment_extinction(weather)*step_metres;
        let opacity=1.0-exp(-min(optical,30.0));
        let lighting=(environment_cloud_light(point,direction,weather,cloud_noise,noise_sampler)*air.transmission+air.inscattering)*view.exposure;
        colour+=transmission*opacity*lighting;
        moment+=transmission*opacity*distance*weather.shape.x;
        transmission*=1.0-opacity;
    }
    return CloudIntegration(vec4<f32>(colour,transmission),moment);
}
fn estimate_at(low_pixel:vec2<f32>)->CloudResult {
    // This pass binds scene_colour as a dummy cloud_colour input. Its physical
    // size is the full target, so derive the reduced size from the viewport.
    let divisor=select(2.0,4.0,weather.air.z<=16.0);
    let low_size=ceil(view.viewport.zw/divisor);
    let pixel=depth_pixel(low_pixel/low_size);
    let uv=viewport_uv(vec2<f32>(pixel)+0.5);
    let direction=ray_direction(uv);
    let origin=view.world_position;
    let maximum=scene_distance(pixel);
    var result=CloudResult(vec4<f32>(0.0,0.0,0.0,1.0),vec2<f32>(maximum,0.0));
    if weather.weather.w<=0.0 || weather.weather.x<=0.0 || weather.weather.y<=0.0 {return result;}
    let inner=1.0+weather.shape.y/weather.shape.x;
    let outer=inner+weather.shape.z/weather.shape.x;
    let shell=sphere_interval(origin,direction,outer);
    var end=min(shell.y,maximum/weather.shape.x);
    let body=sphere_interval(origin,direction,1.0);
    // A miss returns an inverted interval. Only real ground intersections
    // occlude the shell; otherwise clouds must remain visible past the limb.
    if length(origin)>1.00001 && body.y>=body.x && body.x>0.0 {end=min(end,body.x);}
    var start=max(shell.x,0.0);
    if end<=start {return result;}
    let cavity=sphere_interval(origin,direction,inner);
    var first=vec2<f32>(start,end);var second=vec2<f32>(0.0);
    if cavity.y>start && cavity.x<end && cavity.y>cavity.x {
        first.y=clamp(cavity.x,start,end);
        second=vec2<f32>(clamp(cavity.y,start,end),end);
    }
    let first_length=max(first.y-first.x,0.0);
    let second_length=max(second.y-second.x,0.0);
    let total=first_length+second_length;
    if total<=0.0 {return result;}
    let budget=u32(clamp(weather.air.z,8.0,40.0));
    let first_budget=select(budget,budget-1u,second_length>0.0);
    let first_steps=select(u32(clamp(round(f32(budget)*first_length/total),1.0,f32(first_budget))),0u,first_length<=0.0);
    let second_steps=select(budget-first_steps,0u,second_length<=0.0);
    let jitter=phase(vec2<u32>(low_pixel));
    var accumulated=integrate_segment(origin,direction,first,first_steps,jitter,CloudIntegration(result.colour,0.0));
    accumulated=integrate_segment(origin,direction,second,second_steps,jitter,accumulated);
    result.colour=accumulated.colour;
    if accumulated.colour.a<0.9999 {result.distance.y=accumulated.moment/max(1.0-accumulated.colour.a,0.0001);}
    return result;
}
// Reproject the same advected density feature, not the stationary terrain.
fn previous_cloud_point(point:vec3<f32>)->vec3<f32> {
    let inverse_previous=vec4<f32>(-temporal_settings.previous_wind.xyz,temporal_settings.previous_wind.w);
    return environment_rotate(inverse_previous,environment_rotate(weather.wind,point));
}
fn history_uv(point:vec3<f32>)->vec3<f32> {
    let previous=temporal_settings.previous_clip_from_world*vec4<f32>(point,1.0);
    if previous.w<=0.0 {return vec3<f32>(0.0,0.0,0.0);}
    let uv=previous.xy/previous.w*vec2<f32>(0.5,-0.5)+0.5;
    return vec3<f32>(uv,select(0.0,1.0,all(uv>vec2<f32>(0.001))&&all(uv<vec2<f32>(0.999))));
}
fn accept_history(guide:vec2<f32>,previous_guide:vec2<f32>,cloud_point:vec3<f32>,opaque_point:vec3<f32>)->bool {
    if guide.y<=0.0 || previous_guide.y<=0.0 {return false;}
    let expected_cloud=length(cloud_point-temporal_settings.previous_eye.xyz)*weather.shape.x;
    let cloud_tolerance=max(6.0,weather.shape.z*0.4);
    if abs(previous_guide.y-expected_cloud)>cloud_tolerance {return false;}
    // Reject newly exposed ground and cloud visibility that changed at a
    // terrain/water silhouette. Clouds alone are accumulated, never the scene.
    let old_sky=previous_guide.x>=999999.0;
    let new_sky=guide.x>=999999.0;
    if old_sky!=new_sky {return false;}
    if !new_sky {
        let expected_opaque=length(opaque_point-temporal_settings.previous_eye.xyz)*weather.shape.x;
        if abs(previous_guide.x-expected_opaque)>max(1.0,expected_opaque*0.02) {return false;}
        if previous_guide.x+0.1<expected_cloud {return false;}
    }
    return true;
}
fn temporal_at(pixel:vec2<i32>)->CloudResult {
    let size=vec2<i32>(textureDimensions(cloud_colour));
    let current=textureLoad(cloud_colour,pixel,0);
    let guide=textureLoad(cloud_distance,pixel,0).rg;
    var result=CloudResult(current,guide);
    if temporal_settings.controls.x<0.5 || temporal_settings.controls.z<0.5 || current.a>=0.9999 || guide.y<=0.0 {return result;}
    if any(abs(temporal_settings.previous_viewport-view.viewport)>vec4<f32>(0.5)) {return result;}
    let uv=(vec2<f32>(pixel)+0.5)/vec2<f32>(size);
    let ray=ray_direction(uv);
    let point=view.world_position+ray*(guide.y/weather.shape.x);
    let old_point=previous_cloud_point(point);
    let projected=history_uv(old_point);
    if projected.z<0.5 {return result;}
    let opaque_point=view.world_position+ray*(guide.x/weather.shape.x);
    let coordinate=projected.xy*vec2<f32>(size)-0.5;
    let base=vec2<i32>(floor(coordinate));let fraction=fract(coordinate);
    var old=vec4<f32>(0.0);var weight_sum=0.0;
    for(var y=0;y<2;y+=1) {
        for(var x=0;x<2;x+=1) {
            let q=clamp(base+vec2<i32>(x,y),vec2<i32>(0),size-1);
            let previous_guide=textureLoad(history_distance,q,0).rg;
            if !accept_history(guide,previous_guide,old_point,opaque_point) {continue;}
            let weight=select(1.0-fraction.x,fraction.x,x==1)*select(1.0-fraction.y,fraction.y,y==1);
            old+=textureLoad(history_colour,q,0)*weight;weight_sum+=weight;
        }
    }
    if weight_sum<0.05 {return result;}
    old/=weight_sum;
    old=vec4<f32>(old.rgb*(view.exposure/max(temporal_settings.previous_eye.w,0.00000001)),old.a);
    var lower=current;var upper=current;
    for(var y=-1;y<=1;y+=1) {
        for(var x=-1;x<=1;x+=1) {
            let neighbour=textureLoad(cloud_colour,clamp(pixel+vec2<i32>(x,y),vec2<i32>(0),size-1),0);
            lower=min(lower,neighbour);upper=max(upper,neighbour);
        }
    }
    // History cannot introduce a brightness/opacity absent from this frame's
    // local cloud neighbourhood, preventing long white trails behind edges.
    old=clamp(old,lower,upper);
    let motion=length((projected.xy-uv)*vec2<f32>(size));
    let blend=temporal_settings.controls.w*mix(1.0,0.55,smoothstep(2.0,20.0,motion));
    result.colour=mix(current,old,blend);
    return result;
}
fn composite_at(pixel:vec2<i32>)->vec4<f32> {
    let scene=textureLoad(scene_colour,pixel,0);
    let uv=viewport_uv(vec2<f32>(pixel)+0.5);
    if any(uv<vec2<f32>(0.0)) || any(uv>=vec2<f32>(1.0)) {return scene;}
    let distance=scene_distance(pixel);
    let size=vec2<i32>(textureDimensions(cloud_colour));
    let coordinate=uv*vec2<f32>(size)-0.5;
    let base=vec2<i32>(floor(coordinate));let fraction=fract(coordinate);
    var sum=vec4<f32>(0.0);var weight_sum=0.0;
    for(var y=0;y<2;y+=1) {
        for(var x=0;x<2;x+=1) {
            let q=clamp(base+vec2<i32>(x,y),vec2<i32>(0),size-1);
            let guide=textureLoad(cloud_distance,q,0).r;
            let spatial=select(1.0-fraction.x,fraction.x,x==1)*select(1.0-fraction.y,fraction.y,y==1);
            let tolerance=max(0.5,distance*0.015);
            let weight=spatial*exp2(-abs(guide-distance)/tolerance);
            sum+=textureLoad(cloud_colour,q,0)*weight;weight_sum+=weight;
        }
    }
    // A foreground silhouette must never inherit a sky neighbour's cloud fog.
    if weight_sum<0.001 {return scene;}
    let clouds=sum/weight_sum;
    return vec4<f32>(clouds.rgb+scene.rgb*clamp(clouds.a,0.0,1.0),scene.a);
}

@fragment fn estimate(input:FullscreenVertexOutput)->CloudResult {return estimate_at(input.position.xy);}
@fragment fn temporal(input:FullscreenVertexOutput)->CloudResult {return temporal_at(vec2<i32>(input.position.xy));}
@fragment fn composite(input:FullscreenVertexOutput)->@location(0) vec4<f32> {return composite_at(vec2<i32>(input.position.xy));}
