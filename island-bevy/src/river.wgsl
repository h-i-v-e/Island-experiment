// The generator's river surface. Its UVs are the channel's own parametrisation:
// v is the distance already travelled downstream and u is the distance to the
// nearest bank, both in normalized island units. Which way the water runs, how
// fast, where it whitens and where it meets the ground all come from those two
// numbers, the surface normal, and the bed the depth prepass recorded under it.
//
// Only the forward fragment shader is replaced. Fresh water is a separate
// extension from the sea rather than the same one under a switch: it is thin,
// its bed has to stay readable, and it is the only surface here that has a
// direction.

#import bevy_pbr::{
    forward_io::{VertexOutput, FragmentOutput},
    mesh_view_bindings::{view, globals},
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::{alpha_discard, apply_pbr_lighting, main_pass_post_lighting_processing},
    prepass_utils,
    view_transformations::depth_ndc_to_view_z,
}
#import island_bevy::noise::{noise_gradient, perturb}

// Linear-space water tones, each written above as the sRGB triple it came from.
// Fresh water carries its own load rather than the open sea's salt, so it runs
// greener and lets far more through than the ocean palette does.
const SHALLOW: vec3<f32> = vec3<f32>(0.04696, 0.17887, 0.13287); // 0.24, 0.46, 0.40
const DEEP: vec3<f32> = vec3<f32>(0.00490, 0.03310, 0.04696);    // 0.06, 0.20, 0.24
const FOAM: vec3<f32> = vec3<f32>(0.74841, 0.80735, 0.82757);    // 0.88, 0.91, 0.92

// Wavelengths along the channel and across it, in metres, and the crest
// amplitude each layer carries. Both flow layers are anisotropic because water
// running down something draws streaks, not a lattice; the rush layer is the
// long thin one that turns a steep face into falling water. The ripple's
// lateral wavelength is wider than any channel the generator cuts, so its
// crests span one: the coordinate underneath is a distance to the nearest bank,
// and sampling that finely rings every local maximum of the field.
const RIPPLE_ALONG: f32 = 1.70;
const RIPPLE_ACROSS: f32 = 4.00;
const RIPPLE_AMPLITUDE: f32 = 0.020;
const RUSH_ALONG: f32 = 3.20;
const RUSH_ACROSS: f32 = 0.26;
const RUSH_AMPLITUDE: f32 = 0.055;
/// A world-space layer over the two flow ones. It costs one sample and pays for
/// itself twice: near-camera surface detail, and the break in the mirror
/// symmetry a bank distance has about its own centreline.
const CHOP_METRES: f32 = 0.55;
const CHOP_AMPLITUDE: f32 = 0.020;
const CHOP_SPEED: f32 = 0.30;
/// Lattice units per second the flow layers' third axis advances, so a reach
/// evolves as it travels instead of running as one rigid belt.
const EVOLVE: f32 = 0.09;

/// Grade span over which the surface finishes whitening, above the uniform's
/// threshold. Kept narrow: foam belongs on the drops, not on the reaches.
const FOAM_SPAN: f32 = 0.28;
/// Roughness of clear water and of aerated water. Broken water scatters, and a
/// waterfall shaded at the roughness of a pool reads as a sheet of glass.
const ROUGHNESS_CLEAR: f32 = 0.10;
const ROUGHNESS_FOAM: f32 = 0.62;

/// Fresnel reflectance of water at normal incidence, and the `reflectance`
/// parameter that reproduces it through Bevy's `0.16 * r * r` remapping.
const FRESNEL_F0: f32 = 0.02;
const REFLECTANCE: f32 = 0.354;
/// Metres of water past which no light returns. Only ever used where the
/// prepass found nothing under the ray at all.
const LIGHTLESS: f32 = 1.0e4;

struct RiverSettings {
    /// Metres of world space the generator's normalized island unit stands for,
    /// which is what turns both UV channels into distances.
    world_metres: f32,
    /// Extinction per metre of water travelled along the view ray. Far weaker
    /// than the sea's: a channel is a metre deep and its bed is the point.
    absorption: f32,
    /// Metres per second the surface travels on the flat, and the metres per
    /// second a full grade adds to it.
    flow_speed: f32,
    grade_speed: f32,
    /// Metres of bank distance the surface fades in over.
    bank_metres: f32,
    /// Grade at which the surface starts to break white.
    foam_grade: f32,
    /// Metres at which the world-space chop layer has faded out entirely.
    detail_range: f32,
    /// Multiplies the wave slope. One is the height field's own answer.
    wave_strength: f32,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(100) var<uniform> settings: RiverSettings;

/// One flow-space layer, as `(value, slope along, slope across)`. The third
/// lattice axis carries time; its derivative is dropped because it is not a
/// direction on the surface.
fn flow_layer(
    along: f32,
    across: f32,
    wave_along: f32,
    wave_across: f32,
    amplitude: f32,
) -> vec3<f32> {
    let sample = noise_gradient(vec3<f32>(
        along / wave_along,
        across / wave_across,
        globals.time * EVOLVE,
    ));
    return vec3<f32>(
        sample.x,
        sample.y * amplitude / wave_along,
        sample.z * amplitude / wave_across,
    );
}

@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
    var pbr_input = pbr_input_from_standard_material(in, is_front);

    let world = in.world_position.xyz;
    let normal = normalize(pbr_input.world_normal);
    let range = max(length(view.world_position - world), 1.0e-3);

    // The generator's channel coordinates in metres. Without them there is no
    // river here at all, only a translucent lid, so the fallback is a still one.
#ifdef VERTEX_UVS_A
    let downstream = in.uv.y * settings.world_metres;
    let bank = in.uv.x * settings.world_metres;
#else
    let downstream = 0.0;
    let bank = settings.bank_metres;
#endif

    // A falling sheet tilts its normal downstream, so the length of that
    // normal's horizontal part is the grade the water is running down: nothing
    // on a pool, one on a fall.
    let grade = clamp(length(normal.xz), 0.0, 1.0);

    // v is the only channel coordinate that increases monotonically along a
    // reach, so its screen derivatives are what recover the downstream
    // direction in the world. Per-vertex tangents cannot: u is a bank distance
    // and turns over at the centreline, where its own gradient vanishes.
    let along_world = dpdx(world) * dpdx(downstream) + dpdy(world) * dpdy(downstream);
    let tangential = along_world - normal * dot(along_world, normal);
    var flow = vec3<f32>(1.0, 0.0, 0.0);
    if length(tangential) > 1.0e-9 {
        flow = normalize(tangential);
    } else if grade > 1.0e-3 {
        flow = normalize(vec3<f32>(normal.x, 0.0, normal.z));
    }
    let across = normalize(cross(normal, flow));

    // Two travelling layers. The calm one runs everywhere at one speed; the
    // rush runs at the grade's own speed but only carries amplitude where there
    // is a grade to run down, so the shear between neighbouring speeds never
    // shows on the reaches where it would.
    let calm = downstream - settings.flow_speed * globals.time;
    let rushed = downstream - (settings.flow_speed + grade * settings.grade_speed) * globals.time;
    let ripple = flow_layer(calm, bank, RIPPLE_ALONG, RIPPLE_ACROSS, RIPPLE_AMPLITUDE);
    let rush = flow_layer(rushed, bank, RUSH_ALONG, RUSH_ACROSS, RUSH_AMPLITUDE * grade);
    var slope = flow * (ripple.y + rush.y) + across * (ripple.z + rush.z);

    let near = 1.0 - smoothstep(settings.detail_range * 0.4, settings.detail_range, range);
    if near > 0.01 {
        // Drifting along world up rather than any surface direction: the layer
        // exists to break the flow layers up, and a channel is thin enough that
        // its own height barely moves across one.
        let drift = world - vec3<f32>(0.0, globals.time * CHOP_SPEED, 0.0);
        let chop = noise_gradient(drift / CHOP_METRES);
        let amplitude = CHOP_AMPLITUDE / CHOP_METRES * near;
        slope += vec3<f32>(chop.y, 0.0, chop.w) * amplitude;
    }
    pbr_input.N = perturb(normal, slope, settings.wave_strength);

    // Metres of water the ray crosses before the bed. A channel is thin, so
    // this is short almost everywhere and the bed and its stones stay readable;
    // it only runs long where the view is grazing, which is exactly where real
    // water stops showing its bed too.
    var path = LIGHTLESS;
#ifdef DEPTH_PREPASS
    let bed_ndc = prepass_utils::prepass_depth(in.position, 0u);
    if bed_ndc > 0.0 {
        let surface_z = depth_ndc_to_view_z(in.position.z);
        let bed_z = depth_ndc_to_view_z(bed_ndc);
        path = max((surface_z - bed_z) * range / max(-surface_z, 1.0e-3), 0.0);
    }
#endif
    let absorbed = 1.0 - exp(-settings.absorption * path);
    var albedo = mix(SHALLOW, DEEP, absorbed);

    // Whitening is the grade's alone. A pool, a slack reach and a bend all read
    // as clear water; a drop breaks, and the rush layer's own value is what
    // stops that break from covering the fall evenly.
    let whitening = smoothstep(settings.foam_grade, settings.foam_grade + FOAM_SPAN, grade);
    let foam = clamp(whitening * (0.30 + rush.x * 1.15), 0.0, 1.0);
    albedo = mix(albedo, FOAM, foam);

    // The bed showing through, the sky coming off the surface and the aeration
    // sitting in it are three independent chances of the ray not carrying the
    // bed to the eye.
    let n_dot_v = clamp(dot(pbr_input.N, pbr_input.V), 0.0, 1.0);
    let fresnel = FRESNEL_F0 + (1.0 - FRESNEL_F0) * pow(1.0 - n_dot_v, 5.0);
    var alpha = 1.0 - (1.0 - absorbed) * (1.0 - fresnel) * (1.0 - foam);
    // The channel's edge is a bank, not a cut.
    alpha *= smoothstep(0.0, settings.bank_metres, bank);

    pbr_input.material.base_color = vec4<f32>(albedo, clamp(alpha, 0.0, 1.0));
    pbr_input.material.perceptual_roughness = clamp(
        mix(ROUGHNESS_CLEAR + (ripple.x - 0.5) * 0.06, ROUGHNESS_FOAM, foam),
        0.05,
        1.0,
    );
    pbr_input.material.reflectance = vec3<f32>(REFLECTANCE);
    // The occlusion pass only ever saw the bed at this pixel; the surface over
    // it is not in shadow of its own channel walls.
    pbr_input.diffuse_occlusion = vec3<f32>(1.0);
    pbr_input.specular_occlusion = 1.0;
    pbr_input.material.base_color = alpha_discard(pbr_input.material, pbr_input.material.base_color);

    var out: FragmentOutput;
    out.color = apply_pbr_lighting(pbr_input);
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);
    return out;
}
