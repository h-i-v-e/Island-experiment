// The generator's river surface, in the four states water takes between a
// spring and the sea: running in its channel, falling as a sheet, turning over
// in the water that receives a fall, and lying still.
//
// Two things place those states. The UVs are the channel's own parametrisation
// — v is the distance already travelled downstream and u the distance to the
// nearest bank, both in normalized island units — and the colour attribute is
// what `island_gen` measured off the generator's river node profile: how near
// the vertex stands to a lip, how much falling sheet is at it and how far that
// sheet has already fallen, how near it stands to a foot, and how tall the fall
// it belongs to is. Those six numbers, the surface normal and the bed the depth
// prepass recorded under the water decide everything below.
//
// Only the forward fragment shader is replaced. Fresh water is a separate
// extension from the sea rather than the same one under a switch: it is thin,
// its bed has to stay readable, and it is the only surface here that has a
// direction.

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
// Fresh water carries its own load rather than the open sea's salt, so it runs
// greener and lets far more through than the ocean palette does.
const SHALLOW: vec3<f32> = vec3<f32>(0.04696, 0.17887, 0.13287); // 0.24, 0.46, 0.40
const DEEP: vec3<f32> = vec3<f32>(0.00490, 0.03310, 0.04696);    // 0.06, 0.20, 0.24
const FOAM: vec3<f32> = vec3<f32>(0.74841, 0.80735, 0.82757);    // 0.88, 0.91, 0.92

// Wavelengths along the channel and across it, in metres, and the crest
// amplitude each layer carries. Every flow layer is anisotropic because water
// running down something draws streaks, not a lattice. The ripple's lateral
// wavelength is wider than any channel the generator cuts, so its crests span
// one: the coordinate underneath is a distance to the nearest bank, and
// sampling that finely rings every local maximum of the field.
const RIPPLE_ALONG: f32 = 1.70;
const RIPPLE_ACROSS: f32 = 4.00;
const RIPPLE_AMPLITUDE: f32 = 0.020;
/// The rush is the reach's own broken water: short, still fairly wide, and
/// carried only by grade.
const RUSH_ALONG: f32 = 3.20;
const RUSH_ACROSS: f32 = 0.26;
const RUSH_AMPLITUDE: f32 = 0.028;
/// The streak is the falling sheet's: far longer than it is wide, so a face
/// reads as water drawn out by its own fall rather than as a rippled wall. The
/// along wavelength is longer than any fall the generator cuts, which is what
/// keeps the lattice's own cell ends from crossing the face as chevrons.
/// The amplitude is deliberately tiny against the wavelength it is spread over.
/// A layer this anisotropic has a lateral slope of amplitude over wavelength,
/// and `perturb` caps how far a normal may bend: past the cap the bend keeps
/// its direction and loses its size, so the surface stops shading as relief and
/// starts shading as facets — which on a face this wide reads as herringbone.
/// What draws a fall into streaks is the lane field below, in albedo and foam
/// where nothing can saturate, not the normal.
const STREAK_ALONG: f32 = 14.0;
const STREAK_ACROSS: f32 = 0.16;
const STREAK_AMPLITUDE: f32 = 0.010;
/// The lanes a sheet breaks into, in metres. Short enough that a face carries
/// a dozen of them across and a couple down.
const LANE_ALONG: f32 = 2.20;
const LANE_ACROSS: f32 = 0.30;
/// A world-space layer over the flow ones. It costs one sample and pays for
/// itself twice: near-camera surface detail, and the break in the mirror
/// symmetry a bank distance has about its own centreline.
const CHOP_METRES: f32 = 0.55;
const CHOP_AMPLITUDE: f32 = 0.020;
const CHOP_SPEED: f32 = 0.30;
/// The receiving water's own layer: nearly round, because water turning over at
/// the foot of a fall has no direction left, and fast, because it is the one
/// place on the river where the surface is rebuilt every second.
const CHURN_METRES: f32 = 0.85;
const CHURN_AMPLITUDE: f32 = 0.105;
const CHURN_SPEED: f32 = 1.30;
/// Lattice units per second the flow layers' third axis advances, so a reach
/// evolves as it travels instead of running as one rigid belt.
const EVOLVE: f32 = 0.09;
/// The bounded phase used by every aperiodic water field, and the end of that
/// phase spent dissolving into its next copy. Five hundred seconds keeps even
/// the fastest, shortest flow layer inside the lattice hash's useful domain;
/// the blend makes the reset continuous rather than moving every crest at once.
const WATER_PHASE_SECONDS: f32 = 500.0;
const WATER_PHASE_BLEND_SECONDS: f32 = 20.0;
/// Metres the lateral world coordinate is wrapped over.
///
/// The flow layers need a coordinate that runs across a channel without
/// folding. A distance to the nearest bank cannot be one: it turns over at the
/// centreline, so every layer sampled on it is mirrored about that line, and on
/// anything wider than the layer's own lateral wavelength — a falling sheet
/// several metres across — the mirror reads as a chevron down the middle.
/// Mixing a share of signed lateral position into it does not help; the fold
/// only moves. The lateral world position on its own does not fold at all, and
/// wrapping it keeps the lattice coordinates inside the few thousand the hash
/// can still separate neighbours over. The wrap leaves one seam every
/// [`SIDEWAYS_WRAP`] metres, and no channel here runs that far across.
const SIDEWAYS_WRAP: f32 = 256.0;

/// What `DropField::fall` carries at a lip, which is also the value the sheet
/// is read as present from. `island_gen::DROP_FALL_FLOOR` spells the first of
/// them and the two are only correct read together.
const FALL_FLOOR: f32 = 0.2;
const SHEET_ONSET: f32 = 0.08;
/// Metres the plunge was measured over, and the metres it dissipates across
/// once it is downstream of the foot. `island_gen::DROP_PLUNGE_METRES` spells
/// the first of them.
const PLUNGE_RANGE: f32 = 14.0;
const PLUNGE_DECAY: f32 = 3.2;
/// The wavelengths the foam a plunge throws is broken at, in metres, and the
/// values a length of it counts as carrying foam between. Advected foam that
/// covered its whole tail evenly would be a stain on the water rather than
/// something leaving the fall.
const CARRY_ALONG: f32 = 2.00;
const CARRY_ACROSS: f32 = 0.90;
const CARRY_LOW: f32 = 0.32;
const CARRY_HIGH: f32 = 0.82;

/// Grade the surface is read as falling over, above the drop field's own
/// answer, and the grade a reach is read as running at. The first pair is only
/// ever consulted where a drop already stands.
const FALL_GRADE_LOW: f32 = 0.30;
const FALL_GRADE_HIGH: f32 = 0.62;
const RUN_GRADE_LOW: f32 = 0.02;
const RUN_GRADE_HIGH: f32 = 0.10;
/// Grade span over which a running reach finishes breaking white, above the
/// uniform's threshold.
const RAPID_SPAN: f32 = 0.30;
/// Metres of bank distance no grade-driven foam is allowed inside at all.
///
/// Where the water surface meets its own bank it tilts up to it, and that rim
/// has the grade of a fall and none of the water. Reading it as foam is what
/// drew an unbroken white contour around every pool in the phase D captures.
/// Foam that belongs to a fall arrives from the drop field instead, which no
/// rim can produce.
const RIM_CLEAR_METRES: f32 = 0.85;

/// How much of a falling sheet is aerated at its lip and by how much that grows
/// on the way down, and how much of the rock behind a sheet the water itself
/// hides. The last one is what has to hold up with the foam turned off: a fall
/// is a body of water before it is a white one.
const SHEET_FOAM_LIP: f32 = 0.10;
const SHEET_FOAM_GAIN: f32 = 0.75;
const SHEET_BODY: f32 = 0.86;
/// Metres of bank distance a sheet's own edge is torn back over, and the
/// wavelengths of the field that wanders it. A sheet that ends on a straight
/// line is a rectangle however it is shaded.
///
/// The reach has to stay short. The generator's narrowest channel is two metres
/// across, so a bank distance never exceeds a metre there, and an erosion run
/// any longer than this would not be tearing an edge back — it would be eating
/// the whole sheet and leaving the lattice it was torn on across the middle
/// of it.
const TEAR_METRES: f32 = 0.18;
const TEAR_ALONG: f32 = 1.60;
const TEAR_ACROSS: f32 = 0.55;
/// The pale tone a sheet carries as it aerates.
///
/// Almost none of the view ray runs through falling water, so the channel's own
/// absorbed colour has nothing to develop over and a sheet shaded from it alone
/// comes out the flat green of a shallow reach. What a fall actually carries is
/// light scattered by the air it has taken in, which is paler and far less
/// saturated — and, unlike foam, it is still water: this is what the surface
/// has left to read as a body when the foam is turned off.
const SHEET_TONE: vec3<f32> = vec3<f32>(0.26225, 0.41789, 0.41789); // 0.55, 0.68, 0.68

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

/// The colours `debug::STATE` answers with, one per state.
const STATE_CALM: vec3<f32> = vec3<f32>(0.05, 0.12, 0.90);
const STATE_RUNNING: vec3<f32> = vec3<f32>(0.10, 0.80, 0.18);
const STATE_CHURN: vec3<f32> = vec3<f32>(0.95, 0.30, 0.03);
const STATE_FALLING: vec3<f32> = vec3<f32>(1.0, 1.0, 1.0);

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
    /// Grade at which a running reach starts to break white.
    foam_grade: f32,
    /// Metres at which the world-space chop layer has faded out entirely.
    detail_range: f32,
    /// Multiplies the wave slope. One is the height field's own answer.
    wave_strength: f32,
    /// Seconds the water has been running for. The app owns this rather than
    /// the shader reading `globals.time`, because a capture has to answer the
    /// same way twice and wall-clock time depends on how long the frames before
    /// it took.
    water_time: f32,
    /// The diagnostic channel this surface answers with, or `debug::OFF`.
    debug_view: u32,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(100) var<uniform> settings: RiverSettings;

/// One flow-space layer, as `(value, slope along, slope across)`. The third
/// lattice axis carries time; its derivative is dropped because it is not a
/// direction on the surface. `phase` offsets that axis, which is what lets one
/// layer be sampled twice without the two samples evolving together.
fn water_phase() -> f32 {
    return settings.water_time - floor(settings.water_time / WATER_PHASE_SECONDS) * WATER_PHASE_SECONDS;
}

fn phase_blend(time: f32) -> f32 {
    return smoothstep(
        WATER_PHASE_SECONDS - WATER_PHASE_BLEND_SECONDS,
        WATER_PHASE_SECONDS,
        time,
    );
}

fn flow_layer_at(
    downstream: f32,
    speed: f32,
    across: f32,
    wave_along: f32,
    wave_across: f32,
    amplitude: f32,
    phase: f32,
    time: f32,
) -> vec3<f32> {
    let along = downstream - speed * time;
    let sample = noise_gradient(vec3<f32>(
        along / wave_along,
        across / wave_across,
        time * EVOLVE + phase,
    ));
    return vec3<f32>(
        sample.x,
        sample.y * amplitude / wave_along,
        sample.z * amplitude / wave_across,
    );
}

/// One layer sampled twice, half a wavelength apart along the flow and half a
/// lattice cell apart in time, at half amplitude each.
///
/// A single layer advected along a flow line is one ridge the whole reach
/// travels on, and a reach long enough shows it as a rib running the full width
/// of the channel. Two samples out of phase cross each other instead, so no rib
/// ever closes, and splitting their time axes as well keeps the pair from
/// beating in and out together — which is what one layer sampled twice at the
/// same instant would do.
fn flow_pair_at(
    downstream: f32,
    speed: f32,
    across: f32,
    wave_along: f32,
    wave_across: f32,
    amplitude: f32,
    time: f32,
) -> vec3<f32> {
    let first = flow_layer_at(
        downstream,
        speed,
        across,
        wave_along,
        wave_across,
        amplitude * 0.5,
        0.0,
        time,
    );
    let second = flow_layer_at(
        downstream + wave_along * 0.5,
        speed,
        across + wave_across * 0.5,
        wave_along,
        wave_across,
        amplitude * 0.5,
        0.5,
        time,
    );
    return vec3<f32>((first.x + second.x) * 0.5, first.y + second.y, first.z + second.z);
}

fn flow_pair(
    downstream: f32,
    speed: f32,
    across: f32,
    wave_along: f32,
    wave_across: f32,
    amplitude: f32,
) -> vec3<f32> {
    let time = water_phase();
    let current = flow_pair_at(downstream, speed, across, wave_along, wave_across, amplitude, time);
    let blend = phase_blend(time);
    if blend <= 0.0 {
        return current;
    }
    let wrapped = flow_pair_at(
        downstream,
        speed,
        across,
        wave_along,
        wave_across,
        amplitude,
        time - WATER_PHASE_SECONDS,
    );
    return mix(current, wrapped, blend);
}

fn flow_noise_at(
    downstream: f32,
    speed: f32,
    across: f32,
    wave_along: f32,
    wave_across: f32,
    evolution: f32,
    time: f32,
    transposed: bool,
) -> f32 {
    let along = (downstream - speed * time) / wave_along;
    let lateral = across / wave_across;
    let evolved = time * EVOLVE * evolution;
    if transposed {
        return noise(vec3<f32>(lateral, along, evolved));
    }
    return noise(vec3<f32>(along, lateral, evolved));
}

fn flow_noise(
    downstream: f32,
    speed: f32,
    across: f32,
    wave_along: f32,
    wave_across: f32,
    evolution: f32,
    transposed: bool,
) -> f32 {
    let time = water_phase();
    let current = flow_noise_at(
        downstream,
        speed,
        across,
        wave_along,
        wave_across,
        evolution,
        time,
        transposed,
    );
    let blend = phase_blend(time);
    if blend <= 0.0 {
        return current;
    }
    let wrapped = flow_noise_at(
        downstream,
        speed,
        across,
        wave_along,
        wave_across,
        evolution,
        time - WATER_PHASE_SECONDS,
        transposed,
    );
    return mix(current, wrapped, blend);
}

fn world_layer_at(world: vec3<f32>, speed: f32, wavelength: f32, time: f32) -> vec4<f32> {
    let drift = world - vec3<f32>(0.0, time * speed, 0.0);
    return noise_gradient(drift / wavelength);
}

fn world_layer(world: vec3<f32>, speed: f32, wavelength: f32) -> vec4<f32> {
    let time = water_phase();
    let current = world_layer_at(world, speed, wavelength, time);
    let blend = phase_blend(time);
    if blend <= 0.0 {
        return current;
    }
    let wrapped = world_layer_at(world, speed, wavelength, time - WATER_PHASE_SECONDS);
    return mix(current, wrapped, blend);
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

    // What the vertex knows about the nearest fall. Without it every reach is
    // an ordinary channel, which is what the surface was before phase E.
#ifdef VERTEX_COLORS
    let drop = clamp(in.color, vec4<f32>(0.0), vec4<f32>(1.0));
#else
    let drop = vec4<f32>(0.0);
#endif
    let approach = drop.x;
    let aeration = drop.y;
    let plunge_near = drop.z;
    let strength = drop.w;

    // A falling sheet tilts its normal downstream, so the length of that
    // normal's horizontal part is the grade the water is running down: nothing
    // on a pool, one on a fall.
    let grade = clamp(length(normal.xz), 0.0, 1.0);

    // The four states, as weights that never overlap. Falling comes first
    // because it overrides everything under it; the plunge takes what the sheet
    // left; a reach runs on whatever grade or draw towards a lip is left after
    // that; and calm is simply the rest, which is what makes a still pool a
    // state in its own right rather than the absence of foam.
    //
    // Grade alone never promotes water to falling. A pool's own rim has the
    // grade of a fall, and phase D read exactly that as one.
    let sheet = smoothstep(0.0, SHEET_ONSET, aeration);
    let near_drop = max(approach, plunge_near);
    let steep = smoothstep(FALL_GRADE_LOW, FALL_GRADE_HIGH, grade);
    let falling = clamp(sheet + steep * near_drop, 0.0, 1.0);
    // Metres already travelled from the foot, and what is left of the plunge
    // after them. The tail is exponential and shifted to reach zero exactly at
    // the range it was measured over, so the far edge of a plunge pool has no
    // step in it.
    let travelled = (1.0 - plunge_near) * PLUNGE_RANGE;
    let dissipated = exp(-PLUNGE_RANGE / PLUNGE_DECAY);
    let tail = max(exp(-travelled / PLUNGE_DECAY) - dissipated, 0.0) / (1.0 - dissipated);
    let churn = tail * (1.0 - falling);
    let remaining = max(1.0 - falling - churn, 0.0);
    let running = max(smoothstep(RUN_GRADE_LOW, RUN_GRADE_HIGH, grade), approach)
        * remaining;
    let calm = max(1.0 - falling - churn - running, 0.0);

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
    var across = cross(normal, flow);
    if length(across) <= 1.0e-9 {
        // An exactly vertical fallback flow can be parallel to the surface
        // normal. Pick a stable tangent in that degenerate derivative lane
        // rather than normalizing a zero cross product into NaNs.
        let reference = select(
            vec3<f32>(0.0, 1.0, 0.0),
            vec3<f32>(1.0, 0.0, 0.0),
            abs(normal.y) > 0.9,
        );
        across = cross(normal, reference);
    }
    across = normalize(across);

    // Where a fragment stands across the channel, signed and unfolded. The bank
    // distance still decides what happens at an edge; this decides what the
    // layers in between look like.
    let lateral = dot(world.xz, across.xz);
    let sideways = lateral - round(lateral / SIDEWAYS_WRAP) * SIDEWAYS_WRAP;

    // Three travelling layers. The calm one runs everywhere at one speed; the
    // rush runs at the grade's own speed but only carries amplitude where there
    // is a grade to run down; the streak runs at the fall's speed and only on
    // a fall. Neither of the last two shows on the reaches where the shear
    // between neighbouring speeds would.
    let speed = settings.flow_speed + max(grade, falling * 0.85) * settings.grade_speed;
    let ripple = flow_pair(
        downstream,
        settings.flow_speed,
        bank,
        RIPPLE_ALONG,
        RIPPLE_ACROSS,
        RIPPLE_AMPLITUDE,
    );
    let rush = flow_pair(
        downstream,
        speed,
        sideways,
        RUSH_ALONG,
        RUSH_ACROSS,
        RUSH_AMPLITUDE * grade * (1.0 - falling),
    );
    let streak = flow_pair(
        downstream,
        speed,
        sideways,
        STREAK_ALONG,
        STREAK_ACROSS,
        STREAK_AMPLITUDE * falling,
    );
    var slope = flow * (ripple.y + rush.y + streak.y) + across * (ripple.z + rush.z + streak.z);

    let near = 1.0 - smoothstep(settings.detail_range * 0.4, settings.detail_range, range);
    if near > 0.01 {
        // Drifting along world up rather than any surface direction: the layer
        // exists to break the flow layers up, and a channel is thin enough that
        // its own height barely moves across one.
        let chop = world_layer(world, CHOP_SPEED, CHOP_METRES);
        let amplitude = CHOP_AMPLITUDE / CHOP_METRES * near;
        slope += vec3<f32>(chop.y, 0.0, chop.w) * amplitude;
    }

    // The lanes a sheet breaks into. One sample rather than a pair: what a
    // falling face needs here is contrast, and a pair averages towards its own
    // mean and leaves the whole face evenly bright — which is the rectangle
    // phase D drew. Sampled only where there is a sheet to break.
    var lanes = 0.5;
    if falling > 0.01 {
        lanes = flow_noise(downstream, speed, sideways, LANE_ALONG, LANE_ACROSS, 1.5, false);
    }

    // The receiving water. Read in world space and drifting upward rather than
    // downstream, because what a plunge does is turn over in place; the foam it
    // throws is what travels, further down.
    var boil = 0.0;
    if churn > 0.01 {
        let turning = world_layer(world, CHURN_SPEED, CHURN_METRES);
        boil = turning.x;
        let amplitude = CHURN_AMPLITUDE / CHURN_METRES * churn * (0.45 + strength * 0.55);
        slope += vec3<f32>(turning.y, 0.0, turning.w) * amplitude;
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
    // A sheet is thin, so the channel's own absorbed colour has nothing to
    // develop over and it takes the pale tone of aerated water instead. Mostly
    // by weight rather than by how much aeration the vertex field reports: that
    // field is interpolated across triangles a metre and a quarter across, and
    // leaning on it any harder draws them.
    albedo = mix(albedo, SHEET_TONE, falling * clamp(0.45 + aeration * 0.35, 0.0, 1.0));
    // What varies across a sheet is its own thickness, and the lanes carry it:
    // where they run high the water is heaped and bright, where they run low it
    // has thinned to a veil and the rock behind shows.
    albedo *= 1.0 + (lanes - 0.5) * 0.75 * falling;

    // Foam has three sources and no fourth. A drop aerates the water going over
    // it, the water it lands in turns that over and carries it away, and a
    // reach steep enough breaks white on its own. None of the three can reach
    // still water, so a calm pool has no foam at all rather than a little.
    let rim_clear = smoothstep(0.0, RIM_CLEAR_METRES, bank);
    let sheet_foam = falling
        * clamp(SHEET_FOAM_LIP + aeration * SHEET_FOAM_GAIN, 0.0, 1.0)
        * (0.40 + strength * 0.60)
        * clamp(0.15 + lanes * 1.45, 0.0, 1.0);
    // Generated at the foot and advected downstream at the surface's own speed,
    // so what the eye follows is foam leaving the fall rather than a stain
    // lying on the water under it.
    let carried = flow_noise(downstream, speed, sideways, CARRY_ALONG, CARRY_ACROSS, 2.0, false);
    let plunge_foam = churn
        * (0.45 + strength * 0.55)
        * clamp(smoothstep(CARRY_LOW, CARRY_HIGH, carried) + boil * 0.30, 0.0, 1.0);
    let rapid = smoothstep(settings.foam_grade, settings.foam_grade + RAPID_SPAN, grade);
    let rapid_foam = running * rapid * rim_clear * clamp(0.20 + rush.x * 1.25, 0.0, 1.0);
    var foam = clamp(sheet_foam + plunge_foam + rapid_foam, 0.0, 1.0);
    if settings.debug_view == debug::FOAMLESS {
        foam = 0.0;
    }
    albedo = mix(albedo, FOAM, foam);

    // The bed showing through, the sky coming off the surface and the aeration
    // sitting in it are three independent chances of the ray not carrying the
    // bed to the eye.
    let n_dot_v = clamp(dot(pbr_input.N, pbr_input.V), 0.0, 1.0);
    let fresnel = FRESNEL_F0 + (1.0 - FRESNEL_F0) * pow(1.0 - n_dot_v, 5.0);
    var alpha = 1.0 - (1.0 - absorbed) * (1.0 - fresnel) * (1.0 - foam);
    // Falling water has left its bed, so how much of the rock behind it shows
    // is the sheet's own thickness and not the depth of a column. This is the
    // term the foam-off test rests on.
    let body = SHEET_BODY * clamp(0.40 + aeration * 0.75, 0.0, 1.0) * (0.80 + lanes * 0.40);
    alpha = mix(alpha, max(alpha, clamp(body, 0.0, 1.0)), falling);
    // The channel's edge is a bank, not a cut.
    alpha *= smoothstep(0.0, settings.bank_metres, bank);
    // A sheet's own edge is neither. The run it is torn back over wanders down
    // the fall, so what leaves the rock is a set of ribbons rather than a
    // rectangle with two straight sides.
    if falling > 0.01 {
        // The tear field historically carries across-channel distance on the
        // first lattice axis; preserve that orientation while its second axis
        // is advected downstream.
        let tear = flow_noise(downstream, speed, sideways, TEAR_ALONG, TEAR_ACROSS, 1.0, true);
        let torn = smoothstep(0.0, TEAR_METRES * (0.20 + tear * 1.60), bank);
        alpha *= mix(1.0, torn, falling);
    }

    pbr_input.material.base_color = vec4<f32>(albedo, clamp(alpha, 0.0, 1.0));
    pbr_input.material.perceptual_roughness = clamp(
        mix(ROUGHNESS_CLEAR + (ripple.x - 0.5) * 0.06, ROUGHNESS_FOAM, foam)
            + churn * 0.18
            + falling * 0.10,
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
    // The four channels the river answers `--debug-view` with, all written
    // opaque so a diagnostic of the water never arrives blended with the bed it
    // is measuring. Flow carries the downstream heading in red and blue and the
    // speed that heading is travelled at in green; grade carries the surface
    // grade in red and the whitening a running reach takes from it in green;
    // state carries the four water states as four colours; and foamless is the
    // ordinary surface with every foam contribution removed, which is where a
    // fall has to still read as a body of water.
    if settings.debug_view == debug::FLOW {
        out.color = vec4<f32>(
            flow.x * 0.5 + 0.5,
            speed / (settings.flow_speed + settings.grade_speed),
            flow.z * 0.5 + 0.5,
            1.0,
        );
    } else if settings.debug_view == debug::GRADE {
        out.color = vec4<f32>(grade, rapid * rim_clear, 0.0, 1.0);
    } else if settings.debug_view == debug::DEPTH {
        out.color = vec4<f32>(debug::ramp(absorbed), 1.0);
    } else if settings.debug_view == debug::STATE {
        out.color = vec4<f32>(
            STATE_CALM * calm
                + STATE_RUNNING * running
                + STATE_CHURN * churn
                + STATE_FALLING * falling,
            1.0,
        );
    }
    return out;
}
