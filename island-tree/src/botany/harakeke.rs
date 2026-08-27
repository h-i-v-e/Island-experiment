//! Species-specific harakeke architecture.
//!
//! Harakeke is a rhizomatous herb, not a small tree or a radial grass tuft.
//! A clump spreads by offsets, so fans arrive as short rhizome chains: one
//! established parent fan with its younger offsets crowded beside it, the whole
//! chain sharing roughly a single growth plane. This generator builds those
//! chains, then fills each fan with an equitant stack of strap blades whose
//! length, arch, twist and colour follow rank within the fan. Each shared leaf
//! archetype carries a different upper-blade decurve, while the organ
//! transforms preserve the characteristic planar fan arrangement.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss
)]

use std::f32::consts::{PI, TAU};

use motu::{Mesh, Vec2, Vec3};

use super::{
    model::{
        AXIS_POINTS, Axis, AxisGraph, BarkVertex, BotanicalPrototype, BotanicalRecipe,
        BotanicalTexture, FOLIAGE_PAD_ARCHETYPE_COUNT, FoliagePad, LEAF_ARCHETYPE_COUNT, LeafOrgan,
    },
    random::Rng,
};

const HARAKEKE_SEED_DOMAIN: u64 = 0x6861_7261_6b65_6b65;
/// Nominal blade width as a fraction of blade length. The archetype meshes are
/// scaled anisotropically at render time, so the longitudinal twist is applied
/// in a common physical space built from this ratio rather than in local units.
const BLADE_ASPECT: f32 = 0.058;
const TEXTURE_SIZE: u32 = 256;
const LEAF_TILE_SIZE: u32 = 128;
const LEAF_ATLAS_COLUMNS: u32 = 2;
const LEAF_ATLAS_SIZE: u32 = LEAF_TILE_SIZE * LEAF_ATLAS_COLUMNS;

/// Vascular fibre bundles across the blade. Close to five texels each at the
/// tile size, which is the finest striation the atlas can carry before bilinear
/// filtering turns the lines into moire.
const FIBRE_BUNDLES: f32 = 26.0;
/// A stronger bundle every few fibres. Real blades group their vasculature
/// rather than carrying one uniform comb, and the beat between the two
/// frequencies is what stops the striation reading as print.
const FIBRE_GROUPS: f32 = 5.0;
/// Half-width of the painted fold, in blade widths: two texels either side of
/// the midline. Narrower and filtering erases it, wider and it reads as the
/// broad pale stripe it replaces.
const FOLD_HALF_WIDTH: f32 = 2.0 / LEAF_TILE_SIZE as f32;
/// Half-width of the modelled crease, in blade half-widths, and how far the
/// apex is rounded off within it as a fraction of the fold's own height. The
/// crease is a fold in a stiff sheet, so it wants a small radius rather than a
/// razor edge, and the shading break has to fall inside one column pair to stay
/// a seam instead of spreading across the limb.
const FOLD_CREASE: f32 = 0.045;
const FOLD_CREASE_ROUND: f32 = 0.012;
/// Where the blade starts turning its margin down, in blade half-widths, and
/// how far it turns as a fraction of the fold height. The roll gives the
/// marginal line its own narrow facet, which is why it reads in the references
/// as a fine dark thread rather than as painted-on colour.
const MARGIN_ROLL_START: f32 = 0.62;
const MARGIN_ROLL: f32 = 0.085;
/// Where the marginal pigment stops being solid and where it has faded out, in
/// blade widths. On a blade a hundred millimetres across this is under two
/// millimetres of colour and one more of falloff, which is the fine red-orange
/// cuticle line the reference plants carry rather than a coloured band.
const MARGIN_SOLID: f32 = 1.6 / LEAF_TILE_SIZE as f32;
const MARGIN_FADE: f32 = 3.4 / LEAF_TILE_SIZE as f32;
/// Sheen limits for a waxy blade. The shared leaf shader clamps perceptual
/// roughness to `[0.38, 0.96]`, so the polished end is authored just inside
/// that floor and nothing is written where the clamp would flatten it. The
/// span is wide enough to separate a bloomed blade from a burnished one and
/// far short of the mirror finish that would read as wet plastic.
const ROUGHNESS_POLISHED: f32 = 0.38;
const ROUGHNESS_MATT: f32 = 0.70;
/// Pigment the waxy bloom greys a blade toward: barely lighter than the
/// lamina, but enough less saturated to read as a glaucous wash.
const GLAUCOUS_PIGMENT: Vec3 = Vec3::new(0.255, 0.330, 0.240);
/// Marginal pigment per age tile: red-orange while the blade is growing, and
/// browner once it has died back.
const MARGIN_PIGMENT: [Vec3; 4] = [
    Vec3::new(0.440, 0.170, 0.075),
    Vec3::new(0.520, 0.135, 0.050),
    Vec3::new(0.550, 0.120, 0.040),
    Vec3::new(0.410, 0.165, 0.070),
];
/// How much waxy bloom each age tile carries. A mature blade is the most
/// glaucous; the youngest is still glossy green and the dead one has lost its
/// wax along with its pigment.
const TILE_BLOOM: [f32; 4] = [0.35, 0.62, 0.70, 0.34];
/// How weathered each age tile is, which sets both its base roughness and how
/// often it carries a blemish.
const TILE_WEATHERING: [f32; 4] = [0.10, 0.35, 0.65, 1.00];
/// Colour of the fine pair either side of the fold. The mesh carries the real
/// crease; these keep it legible when the blade turns its fold away from the
/// light and on the middle-distance pads, which have no fold geometry.
const FOLD_HIGHLIGHT: Vec3 = Vec3::new(0.042, 0.047, 0.041);
const FOLD_SHADOW: Vec3 = Vec3::new(-0.034, -0.039, -0.029);
/// Lamina pigment per age tile, before bloom. The tiles differ mostly in hue
/// and saturation rather than in lightness, so a fan reads as one plant under
/// changing light instead of as a value ramp from a dark centre to a pale rim.
const TILE_LAMINA: [Vec3; 4] = [
    Vec3::new(0.130, 0.325, 0.130),
    Vec3::new(0.150, 0.300, 0.115),
    Vec3::new(0.215, 0.280, 0.088),
    Vec3::new(0.290, 0.250, 0.105),
];
/// Dead tissue in a scar or fleck. Dull and brown at any age, which is what
/// separates a blemish from the living lamina around it.
const BLEMISH_PIGMENT: Vec3 = Vec3::new(0.185, 0.150, 0.075);
/// Blade cross-section stations, in half-widths either side of the crease. The
/// pair at the crease shoulder exists only to resolve the fold: the whole
/// shading break from crease to limb happens inside that span, which is the
/// fine seam the reference blades carry the length of the fold. The next pair
/// then holds each limb planar out to the margin, so the section reads as two
/// flat facets meeting at an angle rather than as a dome.
const BLADE_COLUMNS: [f32; 11] = [
    -1.0,
    -MARGIN_ROLL_START,
    -0.32,
    -0.16,
    -FOLD_CREASE,
    0.0,
    FOLD_CREASE,
    0.16,
    0.32,
    MARGIN_ROLL_START,
    1.0,
];

pub(super) fn generate_harakeke_prototype(
    seed: u64,
    recipe: BotanicalRecipe,
) -> Result<BotanicalPrototype, String> {
    let mut rng = Rng::new(seed ^ HARAKEKE_SEED_DOMAIN);
    let (graph, fans) = harakeke_graph(recipe, &mut rng);
    let leaves = harakeke_leaves(recipe, &fans, &mut rng)?;
    let foliage_pads = harakeke_foliage_pads(recipe, &fans);
    let (wood, wood_bark) = basal_sheaths(recipe, &fans)?;
    Ok(BotanicalPrototype {
        species: recipe.species,
        graph,
        wood,
        wood_bark,
        wood_scars: Mesh::default(),
        wood_scar_albedo: solid_texture(32, [91, 74, 46, 255]),
        microtwigs: Mesh::default(),
        microtwig_bark: Vec::new(),
        leaf_archetypes: harakeke_leaf_archetypes(),
        shoot_tip_archetypes: std::array::from_fn(|_| Mesh::default()),
        reproductive_archetypes: std::array::from_fn(|_| Mesh::default()),
        foliage_pad_archetypes: harakeke_pad_archetypes(),
        leaves,
        shoot_tips: Vec::new(),
        reproductive_organs: Vec::new(),
        foliage_pads,
        bark_albedo: harakeke_base_albedo(seed),
        bark_normal: flat_normal_texture(TEXTURE_SIZE),
        bark_depth: solid_texture(TEXTURE_SIZE, [128, 128, 128, 255]),
        bark_metallic_roughness: solid_texture(TEXTURE_SIZE, [255, 225, 0, 255]),
        leaf_albedo: harakeke_leaf_albedo(seed),
        leaf_metallic_roughness: harakeke_leaf_metallic_roughness(seed),
    })
}

/// Deterministic per-fan growth state. Every representation of a fan — its
/// axis, its blades, its basal sheath and its middle-distance pad — is derived
/// from one of these, so the near and proxy forms cannot drift apart. It stays
/// private: the public organ model is unchanged.
#[derive(Clone, Copy)]
struct Fan {
    base: Vec3,
    /// Unit normal of the fan's growth plane; the fan is seen face-on from here.
    heading: Vec3,
    /// In-plane horizontal axis the blades splay along.
    lateral: Vec3,
    /// Horizontal direction the whole fan leans toward, and how far.
    lean: Vec3,
    lean_strength: f32,
    height_metres: f32,
    /// 0 = a young offset still standing upright, 1 = a fully established fan.
    maturity: f32,
    exposure: f32,
    variation: f32,
}

fn harakeke_graph(recipe: BotanicalRecipe, rng: &mut Rng) -> (AxisGraph, Vec<Fan>) {
    let fan_count = usize::from(recipe.primary_count);
    let mut fans = Vec::with_capacity(fan_count);
    let mut axes = Vec::with_capacity(fan_count + 1);
    axes.push(Axis {
        parent: None,
        order: 0,
        points_metres: std::array::from_fn(|index| {
            Vec3::Z * (index as f32 / (AXIS_POINTS - 1) as f32 * 0.10)
        }),
        // A low rhizome mass rather than a trunk stump, so the far impostor
        // reads as a clustered base instead of a ball of bark.
        radii_metres: std::array::from_fn(|index| {
            recipe.trunk_radius_metres * (0.55 - index as f32 * 0.06)
        }),
        exposure: 0.45,
        alive: true,
    });

    // One lean shared by the whole clump, so the plant reads as having grown in
    // a particular place rather than as a symmetric specimen.
    let clump_phase = rng.range(0.0, TAU);
    let clump_lean = Vec3::new(clump_phase.cos(), clump_phase.sin(), 0.0);

    // Rhizome chains. Uneven bearing steps leave the wide gaps and near-touching
    // pairs that a golden-angle sweep cannot produce; within a chain the fans
    // keep almost one plane, so several fans are seen edge-on together instead
    // of each facing its own direction.
    let mut bearing = rng.range(0.0, TAU);
    while fans.len() < fan_count {
        bearing += rng.range(1.05, 2.95);
        let chain_direction = Vec3::new(bearing.cos(), bearing.sin(), 0.0);
        // Blades splay along the rhizome, so the plane normal is across it.
        let plane_phase = bearing + PI * 0.5 + rng.range(-0.22, 0.22);
        let chain_root = chain_direction * recipe.trunk_radius_metres * rng.range(0.05, 0.62);
        let chain_length = (2 + (rng.unit() * 1.9) as usize).min(fan_count - fans.len());

        for step in 0..chain_length {
            // The parent fan is established; anything budded off beside it is a
            // younger, shorter, more upright offset.
            let maturity = if step == 0 {
                rng.range(0.86, 1.0)
            } else {
                rng.range(0.42, 0.80)
            };
            let heading_phase = plane_phase + rng.range(-0.26, 0.26);
            let heading = Vec3::new(heading_phase.cos(), heading_phase.sin(), 0.0);
            let lateral = Vec3::new(-heading.y, heading.x, 0.0);
            let base = chain_root
                + chain_direction
                    * recipe.trunk_radius_metres
                    * step as f32
                    * rng.range(0.44, 0.86)
                + heading * recipe.trunk_radius_metres * rng.range(-0.20, 0.20)
                + Vec3::Z * rng.range(0.010, 0.045);
            let outward = (base - Vec3::Z * base.z)
                .try_normalize()
                .unwrap_or(chain_direction);
            let lean = (outward * 0.62 + clump_lean * 0.55 + heading * rng.range(-0.25, 0.25))
                .normalize_or(outward);
            let lean_strength = rng.range(0.05, 0.16) * (0.65 + maturity * 0.55);
            let height_metres = recipe.trunk_height_metres * (0.095 + maturity * 0.080);

            let fan = Fan {
                base,
                heading,
                lateral,
                lean,
                lean_strength,
                height_metres,
                maturity,
                exposure: (0.58 + maturity * 0.34 + rng.range(-0.07, 0.07)).clamp(0.0, 1.0),
                variation: rng.unit(),
            };
            axes.push(Axis {
                parent: Some(0),
                order: 1,
                points_metres: std::array::from_fn(|point| {
                    let t = point as f32 / (AXIS_POINTS - 1) as f32;
                    fan.base
                        + Vec3::Z * fan.height_metres * t
                        + fan.lean * fan.height_metres * fan.lean_strength * t * t
                        + fan.heading * 0.020 * (t * PI).sin()
                }),
                radii_metres: std::array::from_fn(|point| {
                    let t = point as f32 / (AXIS_POINTS - 1) as f32;
                    recipe.trunk_radius_metres * (0.16 + fan.maturity * 0.10 - t * 0.14)
                }),
                exposure: fan.exposure,
                alive: true,
            });
            fans.push(fan);
        }
    }
    (AxisGraph { axes }, fans)
}

fn harakeke_leaves(
    recipe: BotanicalRecipe,
    fans: &[Fan],
    rng: &mut Rng,
) -> Result<Vec<LeafOrgan>, String> {
    let leaves_per_fan = usize::from(recipe.leaves_per_terminal);
    let outermost = leaves_per_fan.saturating_sub(1).max(1) as f32;
    let base_spread = recipe.trunk_radius_metres * 0.16;
    let mut leaves = Vec::with_capacity(fans.len().saturating_mul(leaves_per_fan));
    for (fan_index, fan) in fans.iter().enumerate() {
        let axis_id = u32::try_from(fan_index + 1).map_err(|_| "harakeke fan index exceeds u32")?;

        for leaf_index in 0..leaves_per_fan {
            // Blades are equitant: each new one emerges in the centre of the
            // fan on the side opposite the last and pushes its predecessors
            // outward, so rank in the stack is age and the sides alternate.
            let rank = leaves_per_fan - 1 - leaf_index;
            let splay = rank as f32 / outermost;
            let side = if rank.is_multiple_of(2) { -1.0 } else { 1.0 };

            // A blade that has already fallen toward the lean hangs lower still,
            // which stops the fan from splaying as a symmetric pair of wings.
            let lean_alignment = fan.lateral.dot(fan.lean) * side;
            let horizontal = (fan.lateral * side * (0.62 + splay * 0.38)
                + fan.heading * rng.range(-0.16, 0.16)
                + fan.lean * fan.lean_strength * 1.40)
                .normalize_or(fan.lateral * side);
            let elevation = (1.49
                - splay.powf(1.25) * fan.maturity * 1.05
                - lean_alignment * fan.lean_strength * 1.60
                + rng.range(-0.14, 0.14))
            // Keep even the oldest blade's shared decurve above its own base.
            // Lower angles let the cohort-3 archetype curl underground and
            // present its damaged tip as a detached vertical strip.
            .clamp(0.66, 1.52);
            let direction =
                (horizontal * elevation.cos() + Vec3::Z * elevation.sin()).normalize_or(Vec3::Z);
            // The archetype's normal-displacement axis is scaled by blade
            // length. Point it outward and down within the growth plane so the
            // upper half can decurve by a physically meaningful fraction of
            // the leaf rather than by a fraction of its narrow width.
            let normal =
                (horizontal * elevation.sin() - Vec3::Z * elevation.cos()).normalize_or(horizontal);

            // A fountain rather than a ramp: blades keep extending until they
            // are mature, so the longest are the arching middle cohort, while
            // the upright centre is still short and the oldest have broken back.
            let growth = smoothstep((splay / 0.62).clamp(0.0, 1.0));
            let decline = smoothstep(((splay - 0.78) / 0.22).clamp(0.0, 1.0));
            let reach = 0.58 + growth * 0.42 - decline * 0.16;
            let length = recipe.trunk_height_metres
                * reach
                * (0.72 + fan.maturity * 0.30)
                * rng.range(0.93, 1.03);
            let width =
                (length * rng.range(0.048, 0.064) * (0.86 + splay * 0.22)).clamp(0.075, 0.145);

            // Only the outermost blade of an established fan has fully broken
            // down. Rationing senescence to one blade per parent fan keeps the
            // dead colour where it belongs instead of ringing the whole clump.
            let senescent = rank + 1 == leaves_per_fan && fan.maturity > 0.82;
            let age = (0.06 + splay * 0.80 * fan.maturity + rng.range(-0.10, 0.10)).clamp(0.0, 1.0);
            // Bias the shared pigment bands so the cool glaucous wash falls on
            // the sheltered young centre and the warm one on genuinely old
            // blades, rather than scattering both at random through the fan.
            let pigment = (rng.unit() * 0.90 + 0.05 + (splay - 0.5) * 0.34).clamp(0.02, 0.98);

            leaves.push(LeafOrgan {
                axis: axis_id,
                blade_base_metres: fan.base
                    + fan.lateral * side * splay * base_spread
                    + fan.heading * rng.range(-0.007, 0.007)
                    + Vec3::Z * (0.030 + (1.0 - splay) * 0.085 + rng.range(-0.010, 0.010)),
                direction,
                normal,
                length_metres: length,
                width_metres: width,
                archetype: leaf_archetype(age, senescent, leaf_index),
                age,
                // Crowded young blades sit inside the fan and are shaded by it;
                // the arching outer blades take the full sky.
                light_exposure: (fan.exposure * (0.72 + splay * 0.30) + rng.range(-0.05, 0.05))
                    .clamp(0.0, 1.0),
                variation: pigment * TAU,
            });
        }
    }
    Ok(leaves)
}

/// Senescence is decided by the caller rather than by an age threshold. A
/// threshold turns the age ramp into concentric colour bands; an explicit flag
/// keeps dead blades to the few places a real clump carries them.
fn leaf_archetype(age: f32, senescent: bool, index: usize) -> u8 {
    let cohort = if senescent {
        3
    } else if age < 0.30 {
        0
    } else if age < 0.62 {
        1
    } else {
        2
    };
    // Put the strongest distal-flop variant on a mature outer blade, not on
    // the single senescent edge blade or every third blade indiscriminately.
    cohort + if index % 3 == 1 { 4 } else { 0 }
}

fn harakeke_leaf_archetypes() -> [Mesh; LEAF_ARCHETYPE_COUNT] {
    std::array::from_fn(|index| strap_leaf_mesh(index as u8))
}

fn strap_leaf_mesh(archetype: u8) -> Mesh {
    const STATIONS: usize = 25;
    let cohort = archetype % 4;
    let variant = f32::from(archetype / 4);
    // Most blades retain a smooth arch, while the alternate archetypes carry
    // a much stronger distal flop. Keeping that variation sparse avoids making
    // the whole clump look storm-flattened.
    let bend = match cohort {
        0 => 0.05 + variant * 0.04,
        1 => 0.18 + variant * 0.08,
        2 => 0.48 + variant * 0.22,
        _ => 0.62 + variant * 0.18,
    };
    let damaged_tip = cohort == 3;
    let sweep_sign = if archetype.is_multiple_of(2) {
        -1.0
    } else {
        1.0
    };
    let sweep = sweep_sign * (0.012 + variant * 0.024);
    // Flax blades twist along their length, so an arching blade presents its
    // paler underside and its edge by turns instead of reading as a flat strap.
    // The sign is taken from a different bit than the sweep's so the eight
    // archetypes cover every sweep-and-twist combination.
    let twist_handedness = if (archetype / 2).is_multiple_of(2) {
        -1.0
    } else {
        1.0
    };
    let twist = twist_handedness
        * (match cohort {
            0 => 0.10,
            1 => 0.20,
            2 => 0.30,
            _ => 0.36,
        } + variant * 0.07);
    let mut mesh = Mesh::default();
    for station in 0..STATIONS {
        let t = station as f32 / (STATIONS - 1) as f32;
        let (twist_sin, twist_cos) = (twist * smoothstep(t)).sin_cos();
        let base_taper = smoothstep((t / 0.085).clamp(0.0, 1.0));
        let tip_taper = if damaged_tip {
            0.38 + smoothstep(((1.0 - t) / 0.12).clamp(0.0, 1.0)) * 0.62
        } else {
            smoothstep(((1.0 - t) / 0.16).clamp(0.0, 1.0))
        };
        let width_profile = base_taper * tip_taper * (1.0 - t * 0.06);
        let bend_start = if cohort >= 2 && variant > 0.5 {
            0.52
        } else {
            0.32
        };
        let bend_t = smoothstep(((t - bend_start) / (1.0 - bend_start)).clamp(0.0, 1.0));
        let centreline = bend * bend_t.powf(1.42);
        for lateral in BLADE_COLUMNS {
            let edge_ripple = lateral.abs().powf(5.0)
                * (t.mul_add(TAU * 3.2, f32::from(archetype) * 0.71)).sin()
                * (t * PI).sin()
                * 0.007;
            let basal_fold = 1.0 - smoothstep(((t - 0.18) / 0.28).clamp(0.0, 1.0));
            // A flax blade is folded, not cambered: two nearly planar limbs
            // meeting along a crease. Each limb is left straight so it takes
            // one tone across its whole width, the crease is rounded by a
            // fraction of its own height so the seam is a fine radius rather
            // than a razor edge, and the outer span rolls a little further
            // down so the margin keeps a facet of its own.
            let across_unit = lateral.abs();
            let crease_round =
                (1.0 - smoothstep((across_unit / FOLD_CREASE).min(1.0))) * FOLD_CREASE_ROUND;
            let margin_roll = smoothstep(
                ((across_unit - MARGIN_ROLL_START) / (1.0 - MARGIN_ROLL_START)).clamp(0.0, 1.0),
            ) * MARGIN_ROLL;
            let keel = (1.0 - across_unit - crease_round - margin_roll)
                * width_profile
                * (0.0072 + basal_fold * 0.0138);
            // Hold the cross ripple off the crease and far below the keel, so
            // the blade reads as folded rather than fluted. With four columns
            // to a limb the ripple can only be sampled coarsely, so it is kept
            // to a fraction of the limb's own fall: enough to stop the facet
            // being mirror-flat, not enough to give it a second ridge.
            let corrugation = (lateral * PI * 3.0).cos()
                * smoothstep((lateral.abs() / 0.30).min(1.0))
                * width_profile
                * 0.00025;
            let damage_side = if archetype.is_multiple_of(2) {
                lateral.mul_add(0.5, 0.5)
            } else {
                (-lateral).mul_add(0.5, 0.5)
            };
            let tip_damage = if damaged_tip {
                smoothstep(((t - 0.84) / 0.16).clamp(0.0, 1.0)) * (0.018 + damage_side * 0.052)
            } else {
                0.0
            };
            // Rotate the cross-section about the blade's own spine. Width and
            // the normal axis take different scales at render time, so the
            // rotation happens in a shared physical space before the width
            // component is returned to local units.
            let across = lateral * width_profile * 0.50 + edge_ripple + sweep * (t * PI).sin();
            let across_metres = across * BLADE_ASPECT;
            let out_of_plane = keel + corrugation;
            mesh.vertices.push(Vec3::new(
                t - tip_damage,
                across_metres.mul_add(twist_cos, -(out_of_plane * twist_sin)) / BLADE_ASPECT,
                centreline + across_metres.mul_add(twist_sin, out_of_plane * twist_cos),
            ));
            mesh.uv.push(leaf_uv(
                archetype % 4,
                Vec2::new(lateral.mul_add(0.5, 0.5), t),
            ));
        }
    }
    append_grid_triangles(&mut mesh, STATIONS, BLADE_COLUMNS.len());
    mesh.calculate_normals();
    mesh
}

fn harakeke_foliage_pads(recipe: BotanicalRecipe, fans: &[Fan]) -> Vec<FoliagePad> {
    fans.iter()
        .enumerate()
        .map(|(fan_index, fan)| {
            // The pad's own vertical axis leans with the fan, and the plane
            // normal is re-squared against it, so the proxy inherits the same
            // lean and plane the near blades were built on.
            let direction = (Vec3::Z + fan.lean * fan.lean_strength * 1.20).normalize_or(Vec3::Z);
            let normal =
                (fan.heading - direction * fan.heading.dot(direction)).normalize_or(fan.heading);
            // A young offset carries a smaller, more upright envelope than its
            // parent, matching the size spread the near blades now have.
            let vigour = 0.72 + fan.maturity * 0.32;
            FoliagePad {
                axis: (fan_index + 1) as u32,
                centre_metres: fan.base + Vec3::Z * 0.02,
                direction,
                normal,
                half_extents_metres: Vec3::new(
                    recipe.trunk_height_metres * vigour,
                    recipe.trunk_radius_metres * 1.34,
                    recipe.trunk_height_metres * 0.74 * vigour,
                ),
                archetype: u8::from(fan.maturity > 0.82),
                mean_age: 0.26 + fan.maturity * 0.42,
                light_exposure: fan.exposure,
                density: 0.88 + fan.maturity * 0.10,
                variation: (fan.variation * 0.90 + 0.05 + (fan.maturity - 0.7) * 0.40)
                    .clamp(0.02, 0.98)
                    * TAU,
            }
        })
        .collect()
}

fn harakeke_pad_archetypes() -> [Mesh; FOLIAGE_PAD_ARCHETYPE_COUNT] {
    // Index 0 is the upright young offset, index 1 the arching parent fan.
    [proxy_fan_mesh(0.62), proxy_fan_mesh(0.95)]
}

fn proxy_fan_mesh(droop: f32) -> Mesh {
    // Keep the middle-distance silhouette faithful to the default near fan.
    // A denser proxy makes the plant visibly gain foliage across the LOD cut.
    const LEAVES: usize = 9;
    const STATIONS: usize = 10;
    let mut mesh = Mesh::default();
    for leaf in 0..LEAVES {
        // Same equitant ranking and the same fountain length profile as the
        // near fan, so the silhouette does not change shape across the LOD cut.
        let rank = LEAVES - 1 - leaf;
        let splay = rank as f32 / (LEAVES - 1) as f32;
        let blade_side = if rank.is_multiple_of(2) { -1.0 } else { 1.0 };
        // A static stand-in for the near form's per-blade jitter, so the proxy
        // fan is not a perfectly regular sweep at the distance it takes over.
        let wobble = (leaf as f32).mul_add(2.39, droop * 7.0).sin() * 0.5 + 0.5;
        let growth = smoothstep((splay / 0.62).clamp(0.0, 1.0));
        let decline = smoothstep(((splay - 0.78) / 0.22).clamp(0.0, 1.0));
        let length = (0.58 + growth * 0.42 - decline * 0.16) * (0.90 + wobble * 0.16);
        let arch = splay.powf(1.25) * droop * (0.82 + wobble * 0.30);
        let base = mesh.vertices.len() as u32;
        for station in 0..STATIONS {
            let t = station as f32 / (STATIONS - 1) as f32;
            let tip_taper = ((1.0 - t) / 0.24).clamp(0.0, 1.0);
            let base_taper = (t / 0.10).clamp(0.0, 1.0);
            let half_width =
                (0.018 + splay * 0.008) * smoothstep(base_taper) * smoothstep(tip_taper);
            let distal = smoothstep(((t - 0.46) / 0.54).clamp(0.0, 1.0));
            let vertical = length * (t - arch * distal.powf(1.34));
            let lateral = blade_side * splay.max(0.05) * t.powf(1.15).mul_add(0.90, 0.10);
            let outward = arch * t.powf(1.55) * 0.46;
            // Sample the same age tiles the near blades use, so the proxy
            // carries the fan's colour spread rather than one flat green.
            let tile = if splay < 0.30 {
                0
            } else if splay < 0.72 {
                1
            } else {
                2
            };
            for edge in [-1.0_f32, 1.0] {
                mesh.vertices
                    .push(Vec3::new(vertical, lateral + edge * half_width, outward));
                mesh.uv
                    .push(leaf_uv(tile, Vec2::new(edge.mul_add(0.5, 0.5), t)));
            }
        }
        for station in 0..STATIONS - 1 {
            let lower = base + (station * 2) as u32;
            let upper = lower + 2;
            mesh.triangles
                .extend([lower, upper, upper + 1, lower, upper + 1, lower + 1]);
        }
    }
    mesh.calculate_normals();
    mesh
}

fn basal_sheaths(recipe: BotanicalRecipe, fans: &[Fan]) -> Result<(Mesh, Vec<BarkVertex>), String> {
    const SIDES: usize = 14;
    const RINGS: usize = 4;
    let mut mesh = Mesh::default();
    let mut bark = Vec::new();
    for fan in fans {
        // A flax butt is a stack of folded sheaths, not a stem: it is broad in
        // the fan's plane and thin across it. Aligning the section with the fan
        // makes the plane readable right down at the ground, which is where the
        // paired chain structure is easiest to see.
        let in_plane = recipe.trunk_radius_metres * 0.27;
        let across_plane = recipe.trunk_radius_metres * 0.11;
        // The sheath stack should ground the fan without reading as a row of
        // exposed conical stems; most of it remains concealed by blade bases.
        let height = fan.height_metres * 0.45;
        let base_index = u32::try_from(mesh.vertices.len())
            .map_err(|_| "harakeke basal sheath mesh exceeds u32 indices")?;
        for ring in 0..RINGS {
            let t = ring as f32 / (RINGS - 1) as f32;
            let taper = 1.0 - t * 0.46;
            for side in 0..=SIDES {
                let phase = side as f32 / SIDES as f32 * TAU;
                let section = fan.lateral * phase.cos() * in_plane * taper
                    + fan.heading * phase.sin() * across_plane * (1.0 - t * 0.30);
                mesh.vertices.push(
                    fan.base
                        + section
                        + Vec3::Z * height * t
                        + fan.lean * height * fan.lean_strength * t * t,
                );
                mesh.uv.push(Vec2::new(side as f32 / SIDES as f32, t));
                bark.push(BarkVertex {
                    radius_metres: in_plane * taper,
                    // Established fans carry weathered brown butts; a fresh
                    // offset beside them is still green at the base.
                    maturity: (0.42 + fan.maturity * 0.42 - t * 0.38).clamp(0.0, 1.0),
                });
            }
        }
        let stride = SIDES + 1;
        for ring in 0..RINGS - 1 {
            for side in 0..SIDES {
                let lower = base_index + (ring * stride + side) as u32;
                let upper = lower + stride as u32;
                mesh.triangles
                    .extend([lower, upper, lower + 1, lower + 1, upper, upper + 1]);
            }
        }
    }
    mesh.calculate_normals();
    Ok((mesh, bark))
}

fn append_grid_triangles(mesh: &mut Mesh, rows: usize, columns: usize) {
    for row in 0..rows - 1 {
        let start = (row * columns) as u32;
        let next = start + columns as u32;
        for column in 0..columns - 1 {
            let left = start + column as u32;
            let right = left + 1;
            let next_left = next + column as u32;
            let next_right = next_left + 1;
            mesh.triangles
                .extend([left, next_left, next_right, left, next_right, right]);
        }
    }
}

fn leaf_uv(tile: u8, local: Vec2) -> Vec2 {
    let tile = u32::from(tile).min(3);
    let scale = 1.0 / LEAF_ATLAS_COLUMNS as f32;
    let inset = 1.0 / LEAF_ATLAS_SIZE as f32;
    let usable = scale - inset * 2.0;
    Vec2::new(
        (tile % LEAF_ATLAS_COLUMNS) as f32 * scale + inset + local.x * usable,
        (tile / LEAF_ATLAS_COLUMNS) as f32 * scale + inset + local.y * usable,
    )
}

fn harakeke_base_albedo(seed: u64) -> BotanicalTexture {
    texture(TEXTURE_SIZE, TEXTURE_SIZE, |x, y| {
        let fibres = (x as f32 * 0.22 + y as f32 * 0.035).sin() * 0.030;
        let noise = hash_unit(seed ^ 0x6261_7365, x, y) - 0.5;
        // Flax butts are pale green where they leave the sheath stack and turn
        // red-brown at the ground, which the bark maturity channel then reads.
        let height = y as f32 / (TEXTURE_SIZE - 1) as f32;
        let base =
            Vec3::new(0.26, 0.20, 0.09).lerp(Vec3::new(0.23, 0.29, 0.13), smoothstep(height));
        encode_colour(base + Vec3::splat(fibres + noise * 0.055))
    })
}

/// Age tile and blade-local coordinates for an atlas texel. Both leaf atlases
/// go through it, so the painted seam, the fibres and the marginal line in the
/// albedo land on exactly the sheen features in the metallic-roughness map.
fn leaf_atlas_sample(x: u32, y: u32) -> (usize, f32, f32) {
    let tile = (x / LEAF_TILE_SIZE + y / LEAF_TILE_SIZE * LEAF_ATLAS_COLUMNS) as usize;
    let local_x = (x % LEAF_TILE_SIZE) as f32 / (LEAF_TILE_SIZE - 1) as f32;
    let local_y = (y % LEAF_TILE_SIZE) as f32 / (LEAF_TILE_SIZE - 1) as f32;
    (tile.min(3), local_x, local_y)
}

/// Longitudinal fibre bundles as a signed unit field. Vascular bundles run the
/// length of a flax blade, so the pattern varies across the blade and only
/// drifts along it; the drift and the coarser grouping keep the striation from
/// reading as a printed comb.
fn fibre_field(local_x: f32, local_y: f32) -> f32 {
    let drift = (local_y * 5.7).sin() * 0.30 + (local_y * 2.3 + 1.4).sin() * 0.45;
    let bundles = local_x.mul_add(FIBRE_BUNDLES, drift * 0.05) * TAU;
    let grouping = (local_x * FIBRE_GROUPS).mul_add(TAU, 0.7);
    bundles.sin().mul_add(0.72, grouping.sin() * 0.28)
}

/// How much waxy bloom sits on a blade-local point. Bloom is heaviest low down
/// where it has not been rubbed off, and it arrives in slow patches rather than
/// as an even coat.
fn bloom_field(tile: usize, local_x: f32, local_y: f32) -> f32 {
    let patch = local_y.mul_add(3.1, tile as f32 * 1.7).sin() * 0.16
        + local_x.mul_add(2.4, local_y * 1.3).cos() * 0.10;
    (TILE_BLOOM[tile] * (0.74 + patch + (1.0 - local_y) * 0.20)).clamp(0.0, 1.0)
}

/// The fold seam, peaking on the midline and gone a couple of texels either
/// side of it.
fn fold_seam(local_x: f32) -> f32 {
    1.0 - smoothstep(((local_x - 0.5).abs() / FOLD_HALF_WIDTH).min(1.0))
}

/// The marginal line: solid over the outermost texel and a half, faded out by
/// the third. On a blade a hundred millimetres across that is a thread of
/// colour, which is what the reference plants carry.
fn margin_mask(local_x: f32) -> f32 {
    let from_margin = local_x.min(1.0 - local_x);
    1.0 - smoothstep(((from_margin - MARGIN_SOLID) / (MARGIN_FADE - MARGIN_SOLID)).clamp(0.0, 1.0))
}

/// Sparse surface blemish for an atlas texel: 0 on clean lamina and 1 at the
/// centre of a mark. The cell grid is tile-local, so a mark cannot straddle two
/// age tiles, and both atlases read the same field, so a scar is dull in the
/// same place it is brown.
fn leaf_blemish(seed: u64, tile: usize, x: u32, y: u32) -> f32 {
    const CELL: u32 = 16;
    let weathering = TILE_WEATHERING[tile];
    let mark = hash_unit(
        seed ^ tile as u64 ^ 0x626c_656d,
        (x % LEAF_TILE_SIZE) / CELL,
        (y % LEAF_TILE_SIZE) / CELL,
    );
    let density = 0.05 + weathering * 0.16;
    if mark >= density {
        return 0.0;
    }
    // Place and size the mark from the same hash rather than drawing more of
    // them, so an occupied cell does not advertise the grid it came from.
    let jitter = mark / density;
    let centre_x = (jitter * 7.0).fract() - 0.5;
    let centre_y = (jitter * 13.0).fract() - 0.5;
    let radius = (jitter * 3.0)
        .fract()
        .mul_add(0.10 + weathering * 0.14, 0.12);
    let offset_x = (x % CELL) as f32 / CELL as f32 - 0.5 - centre_x * 0.6;
    // Damage on a strap leaf runs with the grain, so the mark is drawn out
    // along the blade rather than round.
    let offset_y = ((y % CELL) as f32 / CELL as f32 - 0.5 - centre_y * 0.6) * 0.45;
    let distance = offset_x.hypot(offset_y);
    1.0 - smoothstep((distance / radius).min(1.0))
}

fn harakeke_leaf_albedo(seed: u64) -> BotanicalTexture {
    texture(LEAF_ATLAS_SIZE, LEAF_ATLAS_SIZE, |x, y| {
        let (tile, local_x, local_y) = leaf_atlas_sample(x, y);
        let noise = hash_unit(seed ^ tile as u64 ^ 0x6c65_6166, x, y) - 0.5;

        let lamina = TILE_LAMINA[tile];
        // Even a dead blade keeps its colour longest at the sheath, so the
        // straw tile fades in along the blade rather than being straw outright.
        let lamina = if tile == 3 {
            TILE_LAMINA[2].lerp(lamina, smoothstep(local_y))
        } else {
            lamina
        };
        // Bloom greys the blade toward the glaucous pigment. It is a wash and
        // not a coat: even the most bloomed tile keeps most of its own hue.
        let lamina = lamina.lerp(GLAUCOUS_PIGMENT, bloom_field(tile, local_x, local_y) * 0.55);

        // Fibres and a slow lengthwise mottle are multiplied into the pigment
        // rather than added to it, so the blade varies in shading without its
        // hue wandering off the one the tile was authored at.
        let mottle =
            local_y.mul_add(7.0, tile as f32 * 1.7).sin() * local_x.mul_add(2.6, 0.4).cos() * 0.016;
        let shading = fibre_field(local_x, local_y).mul_add(0.052, mottle + noise * 0.018);
        let colour = lamina * (1.0 + shading);

        // The mesh carries the real crease; the painted pair keeps it legible
        // when the blade turns its fold away from the light and on the
        // middle-distance pads, which have no fold geometry.
        let seam_tint = if local_x < 0.5 {
            FOLD_HIGHLIGHT
        } else {
            FOLD_SHADOW
        };
        let colour = colour + seam_tint * fold_seam(local_x);

        // A fine cuticle thread of red-orange, strongest on the exposed upper
        // blade. Blended rather than added, so it stays the muted colour the
        // tile was given instead of glowing at the silhouette edge.
        let margin = margin_mask(local_x) * local_y.mul_add(0.32, 0.58);
        let colour = colour.lerp(MARGIN_PIGMENT[tile], margin);

        let blemish = leaf_blemish(seed, tile, x, y) * TILE_WEATHERING[tile];
        encode_colour(colour.lerp(BLEMISH_PIGMENT, blemish * 0.30))
    })
}

fn harakeke_leaf_metallic_roughness(seed: u64) -> BotanicalTexture {
    texture(LEAF_ATLAS_SIZE, LEAF_ATLAS_SIZE, |x, y| {
        let (tile, local_x, local_y) = leaf_atlas_sample(x, y);
        let weathering = TILE_WEATHERING[tile];
        let span = ROUGHNESS_MATT - ROUGHNESS_POLISHED;
        // A flax blade is waxy: shiny rather than diffuse. Weathering is what
        // takes the polish off, so the tile's age sets where in the band the
        // blade starts and everything below moves it from there.
        let base = span.mul_add(weathering.mul_add(0.62, 0.10), ROUGHNESS_POLISHED);
        // Directional sheen. The shared leaf shader is isotropic, so the
        // anisotropy is authored: the fibre ridges hold the polish and the
        // grooves between them scatter, which draws the highlight out into a
        // streak running the length of the blade. The albedo reads the same
        // field, so the bright lines and the shiny lines are the same lines.
        let fibres = -fibre_field(local_x, local_y) * (0.055 - weathering * 0.020);
        // Bloom is a microcrystalline wax; it scatters where it lies thickest.
        let bloom = bloom_field(tile, local_x, local_y) * 0.060;
        // The crease and the cuticle rim are where the blade is burnished
        // hardest, and they are narrow enough to stay highlights rather than
        // turning the whole surface to plastic.
        let burnish = fold_seam(local_x).mul_add(0.055, margin_mask(local_x) * 0.045);
        // Blades weather from the tip back, and a scar is dull dead tissue.
        let tip = smoothstep(((local_y - 0.68) / 0.32).clamp(0.0, 1.0)) * weathering * 0.10;
        let scar = leaf_blemish(seed, tile, x, y) * (0.10 + weathering * 0.12);
        let grain = (hash_unit(seed ^ tile as u64 ^ 0x726f_7567, x, y) - 0.5) * 0.030;
        let roughness = (base + fibres + bloom - burnish + tip + scar + grain)
            .clamp(ROUGHNESS_POLISHED, ROUGHNESS_MATT);
        [255, (roughness * 255.0) as u8, 0, 255]
    })
}

fn flat_normal_texture(size: u32) -> BotanicalTexture {
    solid_texture(size, [128, 128, 255, 255])
}

fn solid_texture(size: u32, colour: [u8; 4]) -> BotanicalTexture {
    texture(size, size, |_, _| colour)
}

fn texture(width: u32, height: u32, pixel: impl Fn(u32, u32) -> [u8; 4]) -> BotanicalTexture {
    let mut rgba = Vec::with_capacity((width * height * 4) as usize);
    for y in 0..height {
        for x in 0..width {
            rgba.extend(pixel(x, y));
        }
    }
    BotanicalTexture {
        width,
        height,
        rgba,
    }
}

fn hash_unit(seed: u64, x: u32, y: u32) -> f32 {
    let mut value = seed
        ^ u64::from(x).wrapping_mul(0x9e37_79b9_7f4a_7c15)
        ^ u64::from(y).wrapping_mul(0xc2b2_ae3d_27d4_eb4f);
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    (value ^ (value >> 31)) as f32 / u64::MAX as f32
}

fn encode_colour(colour: Vec3) -> [u8; 4] {
    let colour = colour.clamp(Vec3::ZERO, Vec3::ONE);
    [
        (colour.x * 255.0) as u8,
        (colour.y * 255.0) as u8,
        (colour.z * 255.0) as u8,
        255,
    ]
}

const fn smoothstep(value: f32) -> f32 {
    value * value * (3.0 - 2.0 * value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{BotanicalSpecies, generate_botanical_prototype};

    #[test]
    fn harakeke_is_deterministic_dense_and_fan_built() {
        let recipe = BotanicalRecipe::for_species(BotanicalSpecies::Harakeke);
        let first = generate_botanical_prototype(42, recipe).expect("harakeke prototype");
        let second = generate_botanical_prototype(42, recipe).expect("harakeke prototype");
        assert_eq!(first, second);
        assert_eq!(first.species, BotanicalSpecies::Harakeke);
        assert_eq!(
            first.graph.axes.len(),
            usize::from(recipe.primary_count) + 1
        );
        assert_eq!(
            first.leaves.len(),
            usize::from(recipe.primary_count) * usize::from(recipe.leaves_per_terminal)
        );
        let senescent = first
            .leaves
            .iter()
            .filter(|leaf| leaf.archetype % 4 == 3)
            .count();
        assert!(senescent > 0);
        assert!(senescent * 8 < first.leaves.len());
        assert!(
            first
                .graph
                .axes
                .iter()
                .skip(1)
                .all(|axis| axis.parent == Some(0))
        );
        assert!(
            first
                .leaves
                .iter()
                .all(|leaf| leaf.blade_base_metres.z < 0.18)
        );
    }

    #[test]
    fn harakeke_straps_are_broad_curved_and_physically_bounded() {
        let recipe = BotanicalRecipe::for_species(BotanicalSpecies::Harakeke);
        let prototype = generate_botanical_prototype(666, recipe).expect("harakeke prototype");
        assert!(prototype.leaves.iter().all(|leaf| {
            leaf.length_metres >= recipe.trunk_height_metres * 0.38
                && leaf.length_metres <= recipe.trunk_height_metres * 1.06
                && (0.075..=0.145).contains(&leaf.width_metres)
                && leaf.direction.dot(leaf.normal).abs() < 0.001
        }));
        assert!(
            prototype
                .leaves
                .chunks_exact(usize::from(recipe.leaves_per_terminal))
                .all(|fan| {
                    let centre = fan.last().expect("fan has a centre blade");
                    fan.iter()
                        .any(|mature| mature.length_metres > centre.length_metres * 1.25)
                })
        );
        for mesh in &prototype.leaf_archetypes {
            assert_eq!(mesh.vertices.len(), 25 * BLADE_COLUMNS.len());
            assert_eq!(mesh.normals.len(), mesh.vertices.len());
            assert!(mesh.vertices.iter().any(|vertex| vertex.y > 0.35));
        }
        assert_eq!(
            prototype.foliage_pads.len(),
            usize::from(recipe.primary_count)
        );
    }

    #[test]
    fn harakeke_recipe_rejects_tree_like_fan_counts() {
        let mut recipe = BotanicalRecipe::for_species(BotanicalSpecies::Harakeke);
        recipe.primary_count = 17;
        assert!(generate_botanical_prototype(42, recipe).is_err());
    }

    #[test]
    fn old_leaf_archetypes_keep_a_broad_asymmetric_damaged_tip() {
        for archetype in [3_usize, 7] {
            let mesh = strap_leaf_mesh(archetype as u8);
            let tip = &mesh.vertices[mesh.vertices.len() - BLADE_COLUMNS.len()..];
            let (minimum_x, maximum_x) = tip.iter().map(|vertex| vertex.x).fold(
                (f32::INFINITY, f32::NEG_INFINITY),
                |(minimum, maximum), x| (minimum.min(x), maximum.max(x)),
            );
            let width = tip.iter().map(|vertex| vertex.y).fold(
                (f32::INFINITY, f32::NEG_INFINITY),
                |(minimum, maximum), y| (minimum.min(y), maximum.max(y)),
            );
            assert!(maximum_x - minimum_x > 0.045);
            assert!(width.1 - width.0 > 0.30);
        }
    }

    #[test]
    fn blade_material_is_waxy_and_thread_edged() {
        let albedo = harakeke_leaf_albedo(7);
        let sheen = harakeke_leaf_metallic_roughness(7);
        assert_eq!(albedo.rgba.len(), sheen.rgba.len());

        let (texels, remainder) = sheen.rgba.as_chunks::<4>();
        assert!(remainder.is_empty());
        let (minimum, maximum) = texels.iter().fold((255_u8, 0_u8), |(low, high), texel| {
            (low.min(texel[1]), high.max(texel[1]))
        });
        assert!(minimum >= (ROUGHNESS_POLISHED * 255.0) as u8);
        assert!(maximum <= (ROUGHNESS_MATT * 255.0) as u8);
        // Shiny rather than diffuse, and varied: a single flat value across the
        // atlas is what made the blades read as plastic strapping.
        assert!(maximum - minimum > 40);
        // The sheen lives in the roughness channel alone; a blade is not metal.
        assert!(texels.iter().all(|texel| texel[2] == 0));

        // The marginal pigment is a thread, not a band: warm at the very edge
        // and gone again a twentieth of the way across the blade.
        let redness = |x: u32| {
            let index = ((LEAF_TILE_SIZE / 2 * LEAF_ATLAS_SIZE + x) * 4) as usize;
            i32::from(albedo.rgba[index]) - i32::from(albedo.rgba[index + 1])
        };
        assert!(redness(0) > 12);
        assert!(redness(LEAF_TILE_SIZE / 20) < 0);
        assert!(redness(LEAF_TILE_SIZE - 1) > 12);
        assert!(redness(LEAF_TILE_SIZE - 1 - LEAF_TILE_SIZE / 20) < 0);

        let brightness = |x: u32| {
            let index = ((LEAF_TILE_SIZE / 2 * LEAF_ATLAS_SIZE + x) * 4) as usize;
            u16::from(albedo.rgba[index])
                + u16::from(albedo.rgba[index + 1])
                + u16::from(albedo.rgba[index + 2])
        };
        let centre = LEAF_TILE_SIZE / 2;
        assert!(brightness(centre - 1) > brightness(centre + 1) + 12);
    }
}
