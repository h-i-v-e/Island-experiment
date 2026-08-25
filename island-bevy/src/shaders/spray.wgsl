// The mist standing at the foot of a fall.
//
// One quad per droplet, all of them merged into one mesh and all of them drawn
// by this shader: the vertex stage is where a droplet is actually thrown. Its
// launch point rides in the position attribute, its launch velocity in the
// normal, its corner in the UV, and the four numbers that make it its own — the
// phase it starts its arc at, its size, how long it lives and how brightly —
// in the colour. Everything else is arithmetic on the water clock, so the CPU
// touches none of it after the mesh is built and a capture's frozen clock holds
// every droplet exactly where it stood last time.
//
// Both stages are replaced. The fragment stage still shades through the
// standard material, because mist that did not answer the sun would read as a
// decal rather than as water in the air.

#import bevy_pbr::{
    forward_io::{Vertex, VertexOutput, FragmentOutput},
    mesh_functions,
    mesh_view_bindings::view,
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::{alpha_discard, apply_pbr_lighting, main_pass_post_lighting_processing},
    view_transformations::position_world_to_clip,
}
#import island_bevy::debug

/// Metres per second squared. Every droplet is on a ballistic arc and nothing
/// but gravity acts on it: air drag at this size and over this second would
/// only shorten the arc the launch speed already decides.
const GRAVITY: f32 = 9.81;
/// How much of its final size a droplet opens at and how much it grows to.
/// Mist expands as it entrains air, and a cloud whose parts keep one size
/// reads as a sprite sheet.
const OPEN: f32 = 0.45;
const GROWTH: f32 = 1.60;
/// The fraction of a droplet's life spent fading in and the fraction it starts
/// fading out at. Nothing may appear or vanish at full opacity, or the cloud
/// flickers at the loop.
const FADE_IN: f32 = 0.18;
const FADE_OUT: f32 = 0.42;
/// Where a droplet's disc starts falling off and how opaque its centre is.
/// Restrained on purpose: this is mist at the base of a fall, not a fountain,
/// and what sells it is many faint droplets rather than a few solid ones. The
/// disc falls off from its own centre, so nothing in the cloud has an edge.
const CORE: f32 = 0.0;
const OPACITY: f32 = 0.05;
/// Roughness of airborne water. Whiter and flatter than any surface: what
/// returns from a droplet cloud is scattered light, not a reflection.
const ROUGHNESS: f32 = 0.95;
const ALBEDO: vec3<f32> = vec3<f32>(0.90, 0.92, 0.94);

struct SpraySettings {
    /// Seconds the water has been running for, shared with both water
    /// surfaces. The app owns it so a capture answers the same way twice.
    water_time: f32,
    /// The diagnostic channel in force, or `debug::OFF`. Spray carries no
    /// channel of its own, and a diagnostic of the water at the foot of a fall
    /// must not be read through the mist standing over it, so any channel at
    /// all takes the whole cloud out of the frame.
    debug_view: u32,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(100) var<uniform> settings: SpraySettings;

@vertex
fn vertex(vertex: Vertex) -> VertexOutput {
    var out: VertexOutput;

    // The mesh is spawned at the identity, so this is the launch point as it
    // was written; going through the transform anyway is what keeps that true
    // if the cloud is ever parented to something that moves.
    let world_from_local = mesh_functions::get_world_from_local(vertex.instance_index);
    let launch = mesh_functions::mesh_position_local_to_world(
        world_from_local,
        vec4<f32>(vertex.position, 1.0),
    ).xyz;
    let velocity = vertex.normal;
    let phase = vertex.color.x;
    let size = vertex.color.y;
    let life = max(vertex.color.z, 0.1);
    let brightness = vertex.color.w;

    // One arc, restarted forever. The phase is hashed per droplet, so a cloud
    // is spread across its own cycle instead of pulsing as one.
    let progress = fract(settings.water_time / life + phase);
    let age = progress * life;
    let centre = launch + velocity * age
        - vec3<f32>(0.0, 0.5 * GRAVITY * age * age, 0.0);

    // Camera-facing rather than velocity-aligned. A droplet at this size has no
    // silhouette worth orienting, and a quad that turns with the flow shows its
    // own edge the moment the view crosses it.
    let to_view = view.world_position - centre;
    let forward = normalize(select(vec3<f32>(0.0, 0.0, 1.0), to_view, length(to_view) > 1.0e-4));
    var right = cross(vec3<f32>(0.0, 1.0, 0.0), forward);
    // Straight up or straight down the view axis leaves that cross product at
    // zero; any perpendicular will do there.
    right = normalize(select(vec3<f32>(1.0, 0.0, 0.0), right, length(right) > 1.0e-4));
    let up = cross(forward, right);

    let grown = size * (OPEN + progress * GROWTH);
    let corner = vertex.uv * 2.0 - 1.0;
    let world = centre + right * corner.x * grown + up * corner.y * grown;

    out.world_position = vec4<f32>(world, 1.0);
    out.position = position_world_to_clip(world);
    // Between the view and world up: a droplet cloud lit only from the front
    // never answers a low sun, and one lit only from above has no shape at all.
    out.world_normal = normalize(forward * 0.40 + vec3<f32>(0.0, 1.0, 0.0) * 0.92);
#ifdef VERTEX_UVS_A
    out.uv = vertex.uv;
#endif
#ifdef VERTEX_COLORS
    let fade = smoothstep(0.0, FADE_IN, progress) * (1.0 - smoothstep(FADE_OUT, 1.0, progress));
    out.color = vec4<f32>(fade, brightness, 0.0, 1.0);
#endif
#ifdef VERTEX_OUTPUT_INSTANCE_INDEX
    out.instance_index = vertex.instance_index;
#endif
    return out;
}

@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
    var pbr_input = pbr_input_from_standard_material(in, is_front);

#ifdef VERTEX_COLORS
    let fade = in.color.x;
    let brightness = in.color.y;
#else
    let fade = 1.0;
    let brightness = 1.0;
#endif
#ifdef VERTEX_UVS_A
    let corner = in.uv * 2.0 - 1.0;
#else
    let corner = vec2<f32>(0.0);
#endif

    // A round droplet with no edge: the quad it is drawn on must never be
    // visible, and a hard rim at this opacity would be the only thing that is.
    let disc = 1.0 - smoothstep(CORE, 1.0, length(corner));
    var alpha = disc * disc * fade * OPACITY * brightness;
    if settings.debug_view != debug::OFF {
        alpha = 0.0;
    }

    pbr_input.material.base_color = vec4<f32>(ALBEDO, clamp(alpha, 0.0, 1.0));
    pbr_input.material.perceptual_roughness = ROUGHNESS;
    // Nothing behind this fragment was ever in the occlusion pass's idea of
    // this pixel, and mist is not shadowed by the ground it stands over.
    pbr_input.diffuse_occlusion = vec3<f32>(1.0);
    pbr_input.specular_occlusion = 1.0;
    pbr_input.material.base_color = alpha_discard(pbr_input.material, pbr_input.material.base_color);

    var out: FragmentOutput;
    out.color = apply_pbr_lighting(pbr_input);
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);
    return out;
}
