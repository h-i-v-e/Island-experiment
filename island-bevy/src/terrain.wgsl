// Terrain surface. The generator's material triple arrives per vertex and
// everything else — bands, detail, roughness, wetness — is resolved per pixel.
//
// Only the forward fragment shader is replaced. The prepass and deferred
// shaders stay the standard ones, so depth, motion vectors, occlusion, contact
// shadows and the shadow cascades all still see this surface.

#import bevy_pbr::{
    forward_io::{VertexOutput, FragmentOutput},
    mesh_view_bindings::view,
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::{alpha_discard, apply_pbr_lighting, main_pass_post_lighting_processing},
}
#import island_bevy::noise::{band_limit, fbm, fbm_gradient, fbm_gradient_limited, noise, noise_gradient, perturb}

// Linear-space band colours, each written above as the sRGB triple it came
// from. These are the generator's own preview palette in island-rs/src/raster.rs
// and are the only place terrain colour is decided.
const DEEP: vec3<f32> = vec3<f32>(0.00310, 0.02452, 0.07324);       // 0.04, 0.17, 0.30
const SEABED: vec3<f32> = vec3<f32>(0.21404, 0.19599, 0.10048);     // 0.50, 0.48, 0.35
const SAND: vec3<f32> = vec3<f32>(0.53982, 0.44515, 0.18138);       // 0.761, 0.698, 0.463
const DIRT: vec3<f32> = vec3<f32>(0.14732, 0.09463, 0.03968);       // 0.42, 0.34, 0.22
const GRASS_LOW: vec3<f32> = vec3<f32>(0.02949, 0.18138, 0.02323);  // 0.188, 0.463, 0.165
const GRASS_HIGH: vec3<f32> = vec3<f32>(0.08461, 0.10965, 0.02323); // 0.322, 0.365, 0.165
const ROCK: vec3<f32> = vec3<f32>(0.16186, 0.14143, 0.10470);       // 0.439, 0.412, 0.357
const SNOW: vec3<f32> = vec3<f32>(0.83165, 0.85430, 0.87100);       // 0.922, 0.933, 0.941
// Not a band of its own: the tone the macro layer dries established cover
// towards, which is what breaks the single saturated green.
const GRASS_DRY: vec3<f32> = vec3<f32>(0.09463, 0.10654, 0.03310);  // 0.34, 0.36, 0.20
// Rock is one colour in the palette but two minerals under it.
const ROCK_WARM: vec3<f32> = vec3<f32>(0.17064, 0.11928, 0.06838);  // 0.45, 0.38, 0.29
const ROCK_COOL: vec3<f32> = vec3<f32>(0.13998, 0.15487, 0.15487);  // 0.41, 0.43, 0.43

// Wavelengths of the detail layers, in metres of world space.
const REGION_METRES: f32 = 430.0;
const PATCH_METRES: f32 = 34.0;
const GRAIN_METRES: f32 = 2.4;
const MICRO_METRES: f32 = 0.42;
/// How far the macro field is allowed to drag the finer layers sideways.
///
/// What has to stay small is the gradient of that drag, not the drag itself:
/// the field it is taken from turns over every hundred metres or so, so tens of
/// metres of it stretch the domain being dragged by as much as the domain
/// itself, and the finer layers arrive combed into filaments along wherever the
/// stretch runs. At this much the warp is a perturbation, which is all the job
/// asks of it — the lattice underneath is already aperiodic, and the frequency
/// jitter below does more against repetition than the offset ever did.
const WARP_METRES: f32 = 14.0;

/// The widest ratio the detail layers are sampled at between the two screen
/// axes. Four is enough that ground the view runs along keeps its own texture
/// and short of where the same texture starts reading as strokes.
const DETAIL_ANISOTROPY: f32 = 4.0;
/// Cosines of the incidence relief is down to its floor at and back to full at:
/// about four and twenty-four degrees off edge-on.
const GRAZING_INCIDENCE: f32 = 0.07;
const FACING_INCIDENCE: f32 = 0.40;
/// What is left of the relief edge-on. Not zero, or a slope seen along itself
/// would go to flat paint where the one beside it is still ground.
const GRAZING_RELIEF: f32 = 0.30;

/// Roughness each band answers with before any variation.
const ROUGHNESS_ROCK: f32 = 0.90;
const ROUGHNESS_GRASS: f32 = 0.85;
const ROUGHNESS_SAND: f32 = 0.70;
const ROUGHNESS_SNOW: f32 = 0.62;
/// Roughness a soaked surface converges on, and the reflectance it gains.
const ROUGHNESS_WET: f32 = 0.22;
const REFLECTANCE_DRY: f32 = 0.04;
const REFLECTANCE_WET: f32 = 0.30;
/// How far down the tideline and a river bank each take the ground under them.
/// A bank stops well short of the tideline's soak: it is damp ground beside
/// running water, not a surface the sea has just left.
const ALBEDO_TIDELINE: f32 = 0.55;
const ALBEDO_BANK: f32 = 0.74;

struct TerrainSettings {
    /// Metres of elevation standing for the generator's normalized height 1.
    max_height: f32,
    /// Metres at which the metre-scale relief has faded out entirely. The
    /// sub-metre layer is gone at a fixed fraction of it.
    detail_range: f32,
    normal_strength: f32,
    /// Metres above the sea plane the waterline damp reaches.
    wet_band: f32,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(100) var<uniform> settings: TerrainSettings;

@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
    var pbr_input = pbr_input_from_standard_material(in, is_front);

    // The colour attribute carries the generator's raw material triple, not a
    // colour: x is bedrock hardness (exactly one means forced rock), y is loose
    // cover, z is sea proximity. Alpha is the renderer's own measurement of how
    // near the vertex stands to running water. The fallback matches `convert`.
#ifdef VERTEX_COLORS
    let weights = clamp(in.color.rgb, vec3<f32>(0.0), vec3<f32>(1.0));
    let river_proximity = clamp(in.color.a, 0.0, 1.0);
#else
    let weights = vec3<f32>(0.5, 1.0, 0.0);
    let river_proximity = 0.0;
#endif

    let world = in.world_position.xyz;
    let normal = normalize(pbr_input.world_normal);
    // Island space put sea level at zero and elevation on world up, so world
    // height is already the generator's elevation in metres.
    let metres = world.y;
    let height = clamp(metres / max(settings.max_height, 1.0), 0.0, 1.0);
    let slope = clamp(1.0 - normal.y, 0.0, 1.0);
    let range = length(world - view.world_position);

    // How square-on the ground is seen, from edge-on at zero. What answers to it
    // is relief, further down.
    let facing = smoothstep(
        GRAZING_INCIDENCE,
        FACING_INCIDENCE,
        abs(dot(normalize(view.world_position - world), normal)),
    );

    // One macro field carries three jobs: the hundred-metre albedo drift, the
    // phase every finer layer is offset by, and the frequency each is jittered
    // with. Both offset and jitter vary continuously, so no two regions of the
    // island present the same pattern at any distance.
    let region = fbm_gradient(world / REGION_METRES, 3);
    let detail_point = (world + region.yzw * WARP_METRES) * (0.86 + region.x * 0.28);
    // How far one pixel of screen carries in the space the finer layers are
    // read in — taken off that space itself rather than off world metres,
    // because range, incidence, the jitter and the warp's own stretch all move
    // it and only the last is invisible from the world side.
    //
    // Edge-on, the two screen axes disagree by orders of magnitude, and which
    // of them a layer is held to decides what the ground looks like: the long
    // one filters the pattern away and leaves flat paint, the short one keeps
    // every bit of it and the eye reads what is left as strokes drawn along the
    // view. Capping the ratio between them keeps some of both, the same
    // bargain anisotropic texture filtering strikes.
    let along = length(dpdx(detail_point));
    let across = length(dpdy(detail_point));
    let detail_footprint = max(
        min(along, across),
        max(along, across) / DETAIL_ANISOTROPY,
    );

    let patchiness = fbm(detail_point / PATCH_METRES, 3);
    let grain = fbm_gradient_limited(
        detail_point / GRAIN_METRES,
        3,
        detail_footprint / GRAIN_METRES,
    );

    // Slope and cover are nudged, never replaced: the generator stays the
    // authority on what is rock, cover and shore.
    let hardness = weights.x;
    let cover = clamp(weights.y + (patchiness - 0.5) * 0.18, 0.0, 1.0);
    let sea = weights.z;
    let broken_slope = clamp(slope + (grain.x - 0.5) * 0.07, 0.0, 1.0);

    // Bands, thresholds unchanged from the palette this replaces. How far cover
    // has dried out is the macro layer's main job: a single saturated green
    // across a whole island is the one thing the palette on its own could not
    // avoid.
    let grass = mix(
        mix(GRASS_LOW, GRASS_HIGH, height),
        GRASS_DRY,
        clamp(0.40 + region.x * 0.52 + (patchiness - 0.5) * 0.40, 0.0, 1.0),
    );
    var ground = mix(DIRT, grass, smoothstep(0.05, 0.70, cover));
    let shore = 1.0 - smoothstep(2.0, 6.0, metres);
    let sand_weight = smoothstep(0.08, 0.45, cover * sea * shore);
    let forced_rock = smoothstep(0.97, 1.0, hardness);
    let geology_rock = smoothstep(0.20, 0.60, broken_slope * (1.3 + hardness * 1.7));
    let alpine_rock = smoothstep(0.55, 0.80, height);
    let rock_weight = max(max(forced_rock, geology_rock), alpine_rock);
    let snow_weight = smoothstep(0.72, 1.0, height) * (1.0 - slope);

    // Strata are horizontal beds that tighten as the massif rises. The bedding
    // plane is warped by the finer layer, because a clean horizontal sheet cut
    // by terrain draws contour lines rather than rock. Only a face steep enough
    // to expose a bed shows one.
    let bedding = detail_point + grain.yzw * 9.0;
    let bed_tightness = 5.0 + height * 26.0;
    // The bed spacing is what this layer has to be sampled against. Across the
    // face it runs over twenty metres, but a bed is under a metre thick where
    // the massif is highest, and that is the part a grazing view cannot hold.
    let strata = (noise(
        vec3<f32>(bedding.x, bedding.y * bed_tightness, bedding.z) / 22.0,
    ) - 0.5) * band_limit(22.0 / bed_tightness, detail_footprint);
    let mineral = mix(ROCK_COOL, ROCK_WARM, clamp(strata + region.x * 0.7 + 0.15, 0.0, 1.0));
    var rock = mix(ROCK, mineral, 0.70) * (1.0 + strata * (0.16 + broken_slope * 0.44));

    // Metre-scale mottling. Loose ground takes the most, sand a little, snow
    // almost none, so each band keeps its own texture. Loose ground also picks
    // up the macro layer's brightness, which is what keeps a whole hillside
    // from reading as one flat green.
    let mottle = grain.x - 0.5;
    ground *= (1.0 + mottle * 0.40 + (patchiness - 0.5) * 0.30) * (0.82 + region.x * 0.30);
    rock *= 1.0 + mottle * 0.24;
    var sand = SAND * (1.0 + mottle * 0.12);
    let snow = SNOW * (1.0 + mottle * 0.05);

    // Sub-metre grain, only where it still resolves: inside the range it was
    // given, and inside the pixel footprint as well, which is what takes it off
    // ground the view runs along rather than looks at.
    let near = band_limit(MICRO_METRES, detail_footprint)
        * (1.0 - smoothstep(0.0, settings.detail_range * 0.22, range));
    var micro_gradient = vec3<f32>(0.0);
    if near > 0.01 {
        let micro = noise_gradient(detail_point / MICRO_METRES);
        sand *= 1.0 + (micro.x - 0.5) * 0.20 * near;
        rock *= 1.0 + (micro.x - 0.5) * 0.14 * near;
        micro_gradient = micro.yzw * near;
    }

    var albedo = mix(ground, sand, sand_weight);
    albedo = mix(albedo, rock, rock_weight);
    albedo = mix(albedo, snow, snow_weight);

    // Below the sea plane the surface is seabed read through the water. The
    // generator's whole shelf is one to three metres deep and then stops at the
    // terrain square, so the band has to reach the deep tone inside that or the
    // square's edge stays visible through water that is still translucent over
    // it. DEEP is the same constant the ocean absorbs towards, which is what
    // makes the two agree there.
    let depth = smoothstep(0.0, 1.2, -metres);
    albedo = mix(albedo, mix(SEABED, DEEP, depth) * (1.0 + mottle * 0.15), step(metres, 0.0));

    // The same weights again as an exclusive split, so roughness and relief can
    // be asked of one band at a time.
    let snow_band = snow_weight;
    let rock_band = rock_weight * (1.0 - snow_band);
    let sand_band = sand_weight * (1.0 - rock_weight) * (1.0 - snow_band);
    let ground_band = max(1.0 - snow_band - rock_band - sand_band, 0.0);
    var roughness = ROUGHNESS_ROCK * rock_band
        + ROUGHNESS_GRASS * ground_band
        + ROUGHNESS_SAND * sand_band
        + ROUGHNESS_SNOW * snow_band
        + mottle * 0.14 * rock_band;

    // The waterline stays damp, and so does a river bank. Sea proximity places
    // the first; the second arrives per vertex, because the generator publishes
    // channels rather than a distance to them and a fragment cannot measure
    // one. Squaring holds the band close to the water instead of spreading it
    // over the whole range it was measured across, and the metre-scale layer
    // breaks its edge, so what ends is ground and not a contour line.
    let tideline = sea * (1.0 - smoothstep(0.0, max(settings.wet_band, 0.1), max(metres, 0.0)));
    let damp = clamp(river_proximity + mottle * 0.30, 0.0, 1.0);
    let bank = damp * damp;
    let wet = max(tideline, bank);
    albedo *= mix(1.0, ALBEDO_TIDELINE, tideline) * mix(1.0, ALBEDO_BANK, bank);
    roughness = mix(roughness, ROUGHNESS_WET, wet * 0.85);

    // Rock carries full relief and snow none, because a bumped normal on a snow
    // field only sparkles. All of it fades with distance for the same reason,
    // and with the incidence for a stronger one: a bump answers the sun out of
    // all proportion to the relief it stands for once the surface is edge-on,
    // where a normal that is texture face-on swings between lit and unlit. The
    // metre scale of that swing is what draws the ground into strokes along the
    // view. Albedo does not have the same problem and keeps its own detail, so
    // ground the view runs along stays ground rather than going flat.
    let relief = (rock_band + sand_band * 0.7 + ground_band * 0.7)
        * mix(GRAZING_RELIEF, 1.0, facing)
        * (1.0 - smoothstep(settings.detail_range * 0.45, settings.detail_range, range));
    // The sub-metre gradient is steeper per metre than the metre-scale one by
    // the ratio of their wavelengths, so it enters far weaker than it reads.
    let gradient = grain.yzw + micro_gradient * 0.14;

    pbr_input.material.base_color = vec4<f32>(albedo, 1.0);
    pbr_input.material.perceptual_roughness = clamp(roughness, 0.15, 1.0);
    pbr_input.material.reflectance = vec3<f32>(mix(REFLECTANCE_DRY, REFLECTANCE_WET, wet));
    pbr_input.N = perturb(normal, gradient, settings.normal_strength * relief);
    pbr_input.material.base_color = alpha_discard(pbr_input.material, pbr_input.material.base_color);

    var out: FragmentOutput;
    out.color = apply_pbr_lighting(pbr_input);
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);
    return out;
}
