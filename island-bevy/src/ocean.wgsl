// The open sea: one quad reaching past the far plane, shaded from how far the
// view ray runs through water before it meets whatever the depth prepass
// recorded underneath it.
//
// Only the forward fragment shader is replaced. The surface blends, so it is
// drawn after the sky pass and the prepass never records the water itself;
// what it samples there is the opaque island, which is exactly the bottom this
// water stands over.

#import bevy_pbr::{
    forward_io::{VertexOutput, FragmentOutput},
    mesh_view_bindings::view,
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::{alpha_discard, apply_pbr_lighting, main_pass_post_lighting_processing},
    prepass_utils,
    view_transformations::depth_ndc_to_view_z,
}
#import island_bevy::noise::{noise, noise_gradient, perturb}
#import island_bevy::debug

// Linear-space water tones, each written above as the sRGB triple it came from.
// DEEP is the generator's own deep-water preview colour, the one the terrain
// shader's seabed band already converges on, so the two agree by construction
// where the generated square ends.
const SHALLOW: vec3<f32> = vec3<f32>(0.03310, 0.21404, 0.19599); // 0.20, 0.50, 0.48
const DEEP: vec3<f32> = vec3<f32>(0.00310, 0.02452, 0.07324);    // 0.04, 0.17, 0.30
const FOAM: vec3<f32> = vec3<f32>(0.71057, 0.76777, 0.78741);    // 0.86, 0.89, 0.90

// Wavelength in metres, crest amplitude in metres and drift in metres per
// second, per layer. A light breeze, not a storm: the amplitudes are what a
// calm day actually carries, and the slope they produce is the real one.
const SWELL_METRES: f32 = 46.0;
const SWELL_AMPLITUDE: f32 = 1.15;
const SWELL_SPEED: f32 = 1.7;
const CHOP_METRES: f32 = 7.4;
const CHOP_AMPLITUDE: f32 = 0.24;
const CHOP_SPEED: f32 = 0.95;
const RIPPLE_METRES: f32 = 1.30;
const RIPPLE_AMPLITUDE: f32 = 0.040;
const RIPPLE_SPEED: f32 = 0.42;
/// Headings the layers drift along. Deliberately not parallel, so their crests
/// interfere instead of drawing one corduroy across the whole sea.
const SWELL_HEADING: vec2<f32> = vec2<f32>(0.940, 0.342);
const CHOP_HEADING: vec2<f32> = vec2<f32>(0.616, -0.788);
const RIPPLE_HEADING: vec2<f32> = vec2<f32>(0.208, 0.978);
/// Lattice units per second the third noise axis advances. Without it every
/// layer is a rigid sheet sliding past instead of a sea state evolving.
const EVOLVE: f32 = 0.055;

/// Fresnel reflectance of water at normal incidence, and the `reflectance`
/// parameter that reproduces it through Bevy's `0.16 * r * r` remapping.
const FRESNEL_F0: f32 = 0.02;
const REFLECTANCE: f32 = 0.354;
/// Roughness once every wave layer has faded below a pixel. Slope variance the
/// normal can no longer carry has to reappear as a broader highlight, or the
/// distant sea turns into a mirror and the sun into one aliasing dot.
const ROUGHNESS_FLAT: f32 = 0.34;
/// How much faster the colour goes than the opacity. Water absorbs red and
/// green within a metre or two and blue over tens, so a column turns deep long
/// before it turns opaque. One alpha channel cannot carry three extinctions, so
/// the difference between them is spent here: the tint saturates at this
/// multiple of the opacity's own coefficient.
const TINT_RATIO: f32 = 2.2;
/// Metres of water past which no light returns. Only ever used where the
/// prepass found nothing under the ray at all.
const LIGHTLESS: f32 = 1.0e4;
/// Metres of ground between the waterline and the outer edge of the surf. The
/// generator's shelf is flat enough that a band measured in depth instead would
/// reach hundreds of metres offshore and fill every cove.
const SURF_METRES: f32 = 3.0;
/// How level the bottom has to be under the surf for there to be any.
///
/// Surf is a wave running out of water on a shoaling bottom. Shallow water over
/// a wall is not that: it is still water standing against rock, and the phase D
/// captures ringed every steep-sided cove and plunge pool with an unbroken
/// white contour because depth alone could not tell the two apart. These are
/// the cosines of about eighty and fifty-five degrees off level: this
/// generator's coastal apron is steeper under the waterline than a real beach
/// is, so the gate is set to catch a wall and nothing gentler. What separates a
/// shore from a pool is the swell below.
const SURF_BED_LEVEL_LOW: f32 = 0.18;
const SURF_BED_LEVEL_HIGH: f32 = 0.56;
/// Wavelength the surf is broken at. Short enough that the band is ragged from
/// standing height, which is the only distance at which a continuous white
/// ribbon along a coastline reads as drawn on.
const SURF_BREAK_METRES: f32 = 3.2;
const SURF_BREAK_SPEED: f32 = 0.55;
/// Where a length of crest counts as breaking, and the most opaque one gets.
///
/// Surf is a wave that has run out of water, so it arrives as lengths of broken
/// crest with gaps between them and never as a ribbon following the waterline.
/// The band below says where surf is possible at all; this says where one is
/// actually breaking. Phase D multiplied the two and then saturated the result,
/// which turned every shallow margin — a beach, a cove, a plunge pool — into
/// one continuous white outline.
/// The floor is what the water between two broken crests still carries. It has
/// to be low: a floor high enough to see is a contour again, only fainter.
const SURF_CREST_LOW: f32 = 0.44;
const SURF_CREST_HIGH: f32 = 0.80;
const SURF_CREST_FLOOR: f32 = 0.12;
const SURF_PEAK: f32 = 0.95;
/// Where the swell has to stand for a wave to be arriving at all.
///
/// This is what separates a shore from a pool, which nothing local to a
/// fragment can: surf is the swell running out of water, so it arrives in
/// stretches as long as the swell itself and leaves gaps of the same length
/// between them. A beach several hundred metres long carries several of those
/// stretches and reads as surf; a cove twenty metres across carries part of one
/// and can no longer be outlined by it.
/// The floor is what a shore under a trough still carries, which is a trace
/// rather than a band.
const SURF_SWELL_LOW: f32 = 0.34;
const SURF_SWELL_HIGH: f32 = 0.68;
const SURF_SWELL_FLOOR: f32 = 0.10;

struct OceanSettings {
    /// Extinction per metre of water travelled along the view ray.
    absorption: f32,
    /// Metres at which the longest wave layer has faded out entirely. The
    /// shorter two are gone at fixed fractions of it.
    wave_range: f32,
    /// Multiplies the wave slope. One is the height field's own answer.
    wave_strength: f32,
    roughness: f32,
    /// Metres of bottom depth the surf band covers.
    foam_depth: f32,
    /// Metres of bottom depth the surface itself fades in over.
    shore_depth: f32,
    /// Metres at which the sea starts handing the frame back to the sky, and
    /// the metres it takes to finish.
    haze_start: f32,
    haze_range: f32,
    /// Seconds the sea has been running for. The app owns this rather than the
    /// shader reading `globals.time`, because a capture has to answer the same
    /// way twice and wall-clock time depends on how long the frames before it
    /// took.
    water_time: f32,
    /// The diagnostic channel this surface answers with, or `debug::OFF`.
    debug_view: u32,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(100) var<uniform> settings: OceanSettings;

/// One drifting layer, as `(value, world slope)`. The third lattice axis
/// carries time so the field evolves as it travels; its derivative is dropped
/// because it is not a direction in the world.
fn layer(
    point: vec2<f32>,
    heading: vec2<f32>,
    wavelength: f32,
    amplitude: f32,
    speed: f32,
) -> vec4<f32> {
    let drift = (point - heading * (speed * settings.water_time)) / wavelength;
    let sample = noise_gradient(vec3<f32>(drift.x, settings.water_time * EVOLVE, drift.y));
    let slope = amplitude / wavelength;
    return vec4<f32>(sample.x, sample.y * slope, 0.0, sample.w * slope);
}

@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
    var pbr_input = pbr_input_from_standard_material(in, is_front);

    let world = in.world_position.xyz;
    let to_view = view.world_position - world;
    let range = max(length(to_view), 1.0e-3);
    let ray = -to_view / range;

    // Metres of water the ray crosses before the bottom, and how far under the
    // surface that bottom lies. Nothing opaque along the ray is the same answer
    // as a bottom too deep to reach: past the generated square there is no
    // seabed at all, and both saturate to the same colour, which is what makes
    // the square's edge stop being a boundary.
    var path = LIGHTLESS;
    var under = LIGHTLESS;
#ifdef DEPTH_PREPASS
    let bottom_ndc = prepass_utils::prepass_depth(in.position, 0u);
    if bottom_ndc > 0.0 {
        // View depth and ray distance differ only by the pixel's angle off the
        // view axis, and the surface and the bottom share that angle.
        let surface_z = depth_ndc_to_view_z(in.position.z);
        let bottom_z = depth_ndc_to_view_z(bottom_ndc);
        path = max((surface_z - bottom_z) * range / max(-surface_z, 1.0e-3), 0.0);
        under = max(-ray.y * path, 0.0);
    }
#endif

    // Beer-Lambert along that path, twice: once for how much of the bottom is
    // still coming through and once, faster, for what is left of its colour.
    let absorbed = 1.0 - exp(-settings.absorption * path);
    let tinted = 1.0 - exp(-settings.absorption * TINT_RATIO * path);
    var albedo = mix(SHALLOW, DEEP, tinted);

    // Wave layers, each dropped once it is finer than a pixel. What is left
    // beyond that is roughness, not geometry.
    let swell_near = 1.0 - smoothstep(settings.wave_range * 0.35, settings.wave_range, range);
    let chop_near = 1.0 - smoothstep(settings.wave_range * 0.10, settings.wave_range * 0.30, range);
    let swell = layer(world.xz, SWELL_HEADING, SWELL_METRES, SWELL_AMPLITUDE, SWELL_SPEED);
    let chop = layer(world.xz, CHOP_HEADING, CHOP_METRES, CHOP_AMPLITUDE, CHOP_SPEED);
    var slope = swell.yzw * swell_near + chop.yzw * chop_near;
    let ripple_near = 1.0 - smoothstep(0.0, settings.wave_range * 0.08, range);
    if ripple_near > 0.01 {
        let ripple = layer(world.xz, RIPPLE_HEADING, RIPPLE_METRES, RIPPLE_AMPLITUDE, RIPPLE_SPEED);
        slope += ripple.yzw * ripple_near;
    }
    pbr_input.N = perturb(
        normalize(pbr_input.world_normal),
        slope,
        settings.wave_strength,
    );

    // Surf. Depth alone cannot place it: the generator's shelf is flat enough
    // that a depth band reaches hundreds of metres offshore and fills every
    // cove. So the bottom's own grade comes off the normal prepass, and depth
    // over grade is the metres of ground still to cross before the waterline —
    // which a flat shelf never runs out of, however shallow it is, and a beach
    // runs out of as soon as it is shallow at all. Both conditions have to hold,
    // and a short drifting layer breaks the result so the surf reads as moving
    // water rather than as a contour of the terrain mesh.
    var foam = 0.0;
    let shallow = 1.0 - smoothstep(0.0, settings.foam_depth, under);
    if shallow > 0.01 {
        var band = shallow;
#ifdef NORMAL_PREPASS
        let bottom = prepass_utils::prepass_normal(in.position, 0u);
        let shorewards = under * bottom.y / max(length(bottom.xz), 1.0e-3);
        band *= 1.0 - smoothstep(0.0, SURF_METRES, shorewards);
        // The same normal answers the other half of it: whether the bottom is
        // shoaling at all, or whether this is water standing against a wall.
        band *= smoothstep(SURF_BED_LEVEL_LOW, SURF_BED_LEVEL_HIGH, bottom.y);
#endif
        let drift = (world.xz - CHOP_HEADING * (SURF_BREAK_SPEED * settings.water_time))
            / SURF_BREAK_METRES;
        let broken = noise(vec3<f32>(drift.x, settings.water_time * EVOLVE, drift.y));
        let crest = mix(
            SURF_CREST_FLOOR,
            1.0,
            smoothstep(SURF_CREST_LOW, SURF_CREST_HIGH, broken),
        );
        let arriving = mix(
            SURF_SWELL_FLOOR,
            1.0,
            smoothstep(SURF_SWELL_LOW, SURF_SWELL_HIGH, swell.x),
        );
        foam = band * crest * arriving * SURF_PEAK;
        albedo = mix(albedo, FOAM, foam);
    }

    // The bottom showing through, the sky coming off the surface and the foam
    // sitting on it are three independent chances of the ray not carrying the
    // seabed to the eye, so they combine as one.
    let n_dot_v = clamp(dot(pbr_input.N, pbr_input.V), 0.0, 1.0);
    let fresnel = FRESNEL_F0 + (1.0 - FRESNEL_F0) * pow(1.0 - n_dot_v, 5.0);
    var alpha = 1.0 - (1.0 - absorbed) * (1.0 - fresnel) * (1.0 - foam);
    // Where the bottom reaches the surface the water has to run out, not stop
    // on a cut edge against the beach.
    alpha *= smoothstep(0.0, settings.shore_depth, under);
    // Blended surfaces are drawn after the sky pass and so carry no aerial
    // perspective of their own. Letting the sea thin out well past the island
    // hands the horizon back to the atmosphere already drawn behind it, without
    // reaching the distances the generated square ends at.
    alpha *= 1.0 - smoothstep(
        settings.haze_start,
        settings.haze_start + settings.haze_range,
        range,
    );

    pbr_input.material.base_color = vec4<f32>(albedo, clamp(alpha, 0.0, 1.0));
    pbr_input.material.perceptual_roughness = clamp(
        mix(settings.roughness, ROUGHNESS_FLAT, 1.0 - swell_near)
            + (chop.x - 0.5) * 0.05
            + foam * 0.55,
        0.05,
        1.0,
    );
    pbr_input.material.reflectance = vec3<f32>(REFLECTANCE);
    // The occlusion pass only ever saw the seabed at this pixel. Printing its
    // creases onto the surface standing over them is not an approximation of
    // anything.
    pbr_input.diffuse_occlusion = vec3<f32>(1.0);
    pbr_input.specular_occlusion = 1.0;
    pbr_input.material.base_color = alpha_discard(pbr_input.material, pbr_input.material.base_color);

    var out: FragmentOutput;
    out.color = apply_pbr_lighting(pbr_input);
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);
    // The one channel the sea carries: how much of the bottom the column over
    // it has already absorbed, which is the number every other decision above
    // is taken from. Written opaque, or a diagnostic of the water would arrive
    // blended with the ground it is measuring.
    if settings.debug_view == debug::DEPTH {
        out.color = vec4<f32>(debug::ramp(absorbed), 1.0);
    }
    return out;
}
