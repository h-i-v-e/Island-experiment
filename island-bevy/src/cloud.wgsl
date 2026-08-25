// The cloud layer in the sky, seen from under it.
//
// One horizontal plane, shaded entirely here: nothing about it goes through the
// standard material, because a cloud is not a surface answering a sun. What
// leaves the underside of one is light that came in at the top and scattered
// its way down, so the shading is a transmission through a thickness rather
// than a reflection off a normal.
//
// The field is the same image the sun carries as its light texture, read
// through the same projection, so the cloud here and the shadow it lays on the
// ground below are one feature. That projection is the inverse of the light's
// own transform: its local XY plane is square to the sunlight, so every point
// on one sun ray lands on the same texel whatever height it stands at. The
// image holds the share of sunlight that survives, which is what Bevy multiplies
// the direct term by, so the cloud is recovered by dividing what it took away
// back out.
//
// The finer noise on top of that is this stage's own and is deliberately not in
// the shadow: a cloud a kilometre and a half up has lost its fine edges to the
// sun's angular size long before its shadow reaches the ground.

#import bevy_pbr::{
    forward_io::{VertexOutput, FragmentOutput},
    mesh_view_bindings::view,
}
#import island_bevy::noise::fbm
#import island_bevy::debug

/// One over pi, taking an illuminance in lux to the radiance a lambertian
/// surface of albedo one returns under it. Everything else in the frame is lit
/// through the same conversion, so the layer lands in the same range without a
/// scale of its own to tune.
///
/// The other half of that is `view.exposure`, applied at the end. Bevy applies
/// exposure inside the lighting functions rather than in the tone mapper, so a
/// fragment stage like this one — which writes its own colour and never calls
/// them — has to apply it itself. Without it the layer arrives a full exposure
/// too bright and clips to white whatever else is done to it.
const INV_PI: f32 = 0.318309886;
/// What the underside of a cloud returns, against the sunlight falling on its
/// top. Not the albedo of the cloud, which is most of what falls on it: this is
/// the far smaller share that makes it all the way through and leaves the
/// bottom, and the layer is only ever seen from below. At the exposure this
/// camera meters direct sun at, the whole cloud albedo would put a thin edge
/// well over a stop above white.
const ALBEDO: f32 = 0.26;
/// The tones a thin edge and a thick core take. The edge is what the sun is
/// coming through and is very slightly warm; the core is lit by the sky alone
/// and is cool.
const EDGE_TINT: vec3<f32> = vec3<f32>(1.00, 0.99, 0.96);
const CORE_TINT: vec3<f32> = vec3<f32>(0.76, 0.81, 0.92);
/// How much brighter the layer goes looking into the sun, and how tightly that
/// falls off. Forward scattering, and the one thing that stops an overcast deck
/// reading as flat paint.
const FORWARD_GAIN: f32 = 2.2;
const FORWARD_TIGHTNESS: f32 = 7.0;
/// How much of the field's own value the finer octaves may move, and the share
/// of a thin cloud that is opaque at all. Nothing reaches one: a layer with no
/// sky through it anywhere has no depth in it either.
const DETAIL_STRENGTH: f32 = 0.85;
const OPACITY_KNEE: f32 = 0.32;
const OPACITY_CEILING: f32 = 0.96;

struct CloudSettings {
    light_right: vec3<f32>,
    /// One over the light texture's own half tile: what a unit of its local
    /// space is worth in metres.
    tile_scale: f32,
    light_up: vec3<f32>,
    /// The share of direct sun the image already took out of a covered patch,
    /// divided back out here to recover the cloud that took it.
    shadow: f32,
    light_origin: vec3<f32>,
    thickness: f32,
    /// The direction sunlight travels.
    sun_direction: vec3<f32>,
    sun_illuminance: f32,
    fade_start: f32,
    fade_range: f32,
    detail_metres: f32,
    /// The diagnostic channel in force, or `debug::OFF`. The layer carries no
    /// channel of its own, and no diagnostic of the ground should be read
    /// through cloud, so any channel at all takes it out of the frame.
    debug_view: u32,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(100) var<uniform> settings: CloudSettings;
@group(#{MATERIAL_BIND_GROUP}) @binding(101) var field: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(102) var field_sampler: sampler;

@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
    let world = in.world_position.xyz;

    // The same mapping `pbr_lighting` uses for a directional light texture:
    // into the light's local space, then across its own two-unit tile. The
    // sampler wraps, so nothing has to be brought back into range here.
    let local = world - settings.light_origin;
    let plane = vec2<f32>(
        dot(local, settings.light_right),
        dot(local, settings.light_up),
    ) * settings.tile_scale;
    let uv = plane * vec2<f32>(-0.5, 0.5) + 0.5;
    let sunlight = textureSample(field, field_sampler, uv).r;
    let covered = clamp((1.0 - sunlight) / max(settings.shadow, 1.0e-3), 0.0, 1.0);

    // The detail is proportional, so clear sky stays clear and a core stays a
    // core; only the edges, where the field is already between the two, break
    // up. Read on the layer's own plane, so it drifts with the field above it.
    let detail = fbm(vec3<f32>(local.x, world.y, local.z) / settings.detail_metres, 3);
    let density = clamp(covered * (1.0 + (detail - 0.5) * DETAIL_STRENGTH), 0.0, 1.0);

    // Transmission through the thickness: an edge passes almost everything, a
    // core very little. This is what the underside of a cloud actually is.
    let transmit = mix(1.0, 1.0 - settings.thickness, density);
    let to_sun = -settings.sun_direction;
    let towards = normalize(world - view.world_position);
    let forward = pow(max(dot(towards, to_sun), 0.0), FORWARD_TIGHTNESS);
    let tone = mix(CORE_TINT, EDGE_TINT, transmit);
    let radiance = settings.sun_illuminance * INV_PI * ALBEDO
        * transmit * (1.0 + FORWARD_GAIN * forward);

    // Out towards the horizon the layer hands the frame back to the sky it
    // stands in, well inside its own mesh, so nothing in a frame is ever the
    // edge of the plane.
    let radial = length(world.xz - view.world_position.xz);
    let reach = 1.0 - smoothstep(
        settings.fade_start,
        settings.fade_start + settings.fade_range,
        radial,
    );
    var alpha = smoothstep(0.0, OPACITY_KNEE, density) * OPACITY_CEILING * reach;
    if settings.debug_view != debug::OFF {
        alpha = 0.0;
    }

    var out: FragmentOutput;
    // Straight alpha, not premultiplied: `AlphaMode::Blend` is the one blend
    // state Bevy leaves un-premultiplied in the shader.
    out.color = vec4<f32>(tone * radiance * view.exposure, clamp(alpha, 0.0, 1.0));
    return out;
}
