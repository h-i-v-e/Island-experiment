// Independent final-depth SSAO: view-space hemisphere probes and bilateral blur.
#import bevy_core_pipeline::fullscreen_vertex_shader::FullscreenVertexOutput
#import bevy_render::view::View
struct AoSettings { parameters: vec4<f32>, scale: vec4<f32> }
@group(0) @binding(0) var scene_colour: texture_2d<f32>;
@group(0) @binding(1) var linear_sampler: sampler;
@group(0) @binding(2) var scene_depth: texture_depth_2d;
@group(0) @binding(3) var<uniform> view: View;
@group(0) @binding(4) var<uniform> settings: AoSettings;
@group(0) @binding(5) var ao_texture: texture_2d<f32>;

// XYZ is camera-relative view position in metres; W marks valid geometry.
// Zero is the reversed-Z clear value. Small positive depth is still geometry.
fn position_pixel(pixel: vec2<i32>) -> vec4<f32> {
    let size = vec2<i32>(textureDimensions(scene_depth));
    if any(pixel < vec2<i32>(0)) || any(pixel >= size) { return vec4<f32>(0.0); }
    let depth = textureLoad(scene_depth, pixel, 0);
    if depth <= 0.0 { return vec4<f32>(0.0); }
    let centre_uv = (vec2<f32>(pixel) + 0.5) / vec2<f32>(size);
    let clip = vec4<f32>(centre_uv * vec2<f32>(2.0, -2.0) + vec2<f32>(-1.0, 1.0), depth, 1.0);
    let p = view.view_from_clip * clip;
    return vec4<f32>(p.xyz / p.w * settings.scale.x, 1.0);
}
fn position_at(uv: vec2<f32>) -> vec4<f32> {
    if any(uv < vec2<f32>(0.0)) || any(uv >= vec2<f32>(1.0)) { return vec4<f32>(0.0); }
    // Depth and its reconstruction ray always belong to the same texel.
    return position_pixel(vec2<i32>(uv * vec2<f32>(textureDimensions(scene_depth))));
}
fn normal_at(uv: vec2<f32>, centre: vec3<f32>) -> vec3<f32> {
    // Integer neighbours avoid sampling the centre twice when half-resolution
    // fragment interpolation lands just to either side of a depth texel edge.
    let pixel = vec2<i32>(uv * vec2<f32>(textureDimensions(scene_depth)));
    let left = position_pixel(pixel - vec2<i32>(1, 0));
    let right = position_pixel(pixel + vec2<i32>(1, 0));
    let above = position_pixel(pixel - vec2<i32>(0, 1));
    let below = position_pixel(pixel + vec2<i32>(0, 1));
    var dx = right.xyz - centre;
    var dy = below.xyz - centre;
    if right.w == 0.0 || (left.w > 0.0 && abs(left.z-centre.z) < abs(right.z-centre.z)) { dx = centre-left.xyz; }
    if below.w == 0.0 || (above.w > 0.0 && abs(above.z-centre.z) < abs(below.z-centre.z)) { dy = centre-above.xyz; }
    var n = cross(dy, dx);
    if dot(n,n) < 1e-20 { return normalize(-centre); }
    n = normalize(n);
    return select(n,-n,dot(n,-centre)<0.0);
}
fn phase_at(pixel: vec2<u32>) -> f32 {
    var hash = pixel.x * 1597334677u ^ pixel.y * 3812015801u;
    hash = (hash ^ (hash >> 16u)) * 2246822519u;
    return f32(hash & 65535u) * (6.28318530718/65536.0);
}
fn estimate_at(uv: vec2<f32>, pixel: vec2<u32>) -> vec4<f32> {
    let centre = position_at(uv);
    if centre.w == 0.0 { return vec4<f32>(0.0); }
    let normal = normal_at(uv,centre.xyz);
    let radius = settings.parameters.x;
    let projected_radius = abs(view.clip_from_view[1][1]) * radius / max(-centre.z,0.001)
        * f32(textureDimensions(scene_depth).y) * 0.5;
    if projected_radius < 0.5 { return vec4<f32>(0.0,normal); }
    let axis = select(vec3<f32>(0.0,0.0,1.0),vec3<f32>(1.0,0.0,0.0),abs(normal.z)>0.9);
    let tangent = normalize(cross(axis,normal));
    let bitangent = cross(normal,tangent);
    let phase = phase_at(pixel);
    let count = settings.parameters.z;
    let bias = max(radius * 0.015,0.0005);
    var blocked = 0.0;
    var weight = 0.0;
    for (var i=0u;i<12u;i+=1u) {
        if f32(i)>=count { break; }
        let t = (f32(i)+0.5)/count;
        let angle = f32(i)*2.39996322973+phase;
        let ring = sqrt(1.0-t*t);
        let direction = (tangent*cos(angle)+bitangent*sin(angle))*ring+normal*t;
        let probe = centre.xyz+direction*(radius*sqrt(t));
        let projected = view.clip_from_view*vec4<f32>(probe/settings.scale.x,1.0);
        if projected.w <= 0.0 { continue; }
        let uv = projected.xy/projected.w*vec2<f32>(0.5,-0.5)+0.5;
        let visible = position_at(uv);
        if visible.w == 0.0 { continue; }
        let difference = visible.xyz-centre.xyz;
        let range = 1.0-smoothstep(radius*0.5,radius,length(difference));
        let facing = smoothstep(bias,bias*3.0,dot(normal,difference));
        blocked += smoothstep(bias,bias*3.0,visible.z-probe.z)*range*facing;
        weight += 1.0;
    }
    return vec4<f32>(blocked/max(weight,1.0)*smoothstep(0.5,2.0,projected_radius),normal);
}
@fragment fn estimate(input: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    return estimate_at(input.uv, vec2<u32>(input.position.xy));
}
fn compatible(centre: vec4<f32>, sample: vec4<f32>, uv: vec2<f32>, depth: f32) -> f32 {
    let neighbour = position_at(uv);
    if neighbour.w == 0.0 || dot(sample.yzw,sample.yzw)<0.1 { return 0.0; }
    let normal_weight = pow(clamp(dot(centre.yzw,normalize(sample.yzw)),0.0,1.0),12.0);
    let depth_weight = exp(-abs(neighbour.z-depth)/max(settings.parameters.x*0.2,0.001));
    return normal_weight*depth_weight;
}
fn blur(uv: vec2<f32>, axis: vec2<f32>) -> vec4<f32> {
    let centre = textureSampleLevel(ao_texture,linear_sampler,uv,0.0);
    let position = position_at(uv);
    if position.w == 0.0 { return vec4<f32>(0.0); }
    let step = axis/vec2<f32>(textureDimensions(ao_texture));
    var total = centre.x*6.0;
    var weight = 6.0;
    for (var i=-2;i<=2;i+=1) {
        if i==0 { continue; }
        let sample_uv = uv+step*f32(i);
        let sample = textureSampleLevel(ao_texture,linear_sampler,sample_uv,0.0);
        let gaussian = select(4.0,1.0,abs(i)==2);
        let w = compatible(centre,sample,sample_uv,position.z)*gaussian;
        total += sample.x*w; weight += w;
    }
    return vec4<f32>(total/weight,centre.yzw);
}
@fragment fn horizontal(input: FullscreenVertexOutput) -> @location(0) vec4<f32> { return blur(input.uv,vec2<f32>(1.0,0.0)); }
@fragment fn vertical(input: FullscreenVertexOutput) -> @location(0) vec4<f32> { return blur(input.uv,vec2<f32>(0.0,1.0)); }
@fragment fn composite(input: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    let colour = textureSampleLevel(scene_colour,linear_sampler,input.uv,0.0);
    let position = position_at(input.uv);
    // Keep the sky unchanged, including at silhouettes during half-res upsampling.
    if position.w == 0.0 { return select(colour,vec4<f32>(1.0),settings.parameters.w>0.0); }
    let centre = vec4<f32>(0.0,normal_at(input.uv,position.xyz));
    let pixel = 1.0/vec2<f32>(textureDimensions(ao_texture));
    var total = 0.0; var weight = 0.0;
    for (var y=-1;y<=1;y+=1) {
        for (var x=-1;x<=1;x+=1) {
            let uv = input.uv+vec2<f32>(f32(x),f32(y))*pixel*0.5;
            let sample = textureSampleLevel(ao_texture,linear_sampler,uv,0.0);
            let w = compatible(centre,sample,uv,position.z);
            total += sample.x*w; weight += w;
        }
    }
    let visibility = pow(1.0-clamp(total/max(weight,0.0001),0.0,0.95),settings.parameters.y*2.0);
    if settings.parameters.w>0.0 { return vec4<f32>(vec3<f32>(visibility),1.0); }
    return vec4<f32>(colour.rgb*visibility,colour.a);
}
