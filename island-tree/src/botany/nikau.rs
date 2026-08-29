//! Species-specific nīkau palm architecture.
//!
//! Nīkau is deliberately not expressed as a pōhutukawa preset. Its solitary
//! trunk, crownshaft, unbranched frond axes and paired leaflets are a separate
//! growth program that compiles into the same renderer-neutral prototype.

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
        REPRODUCTIVE_ARCHETYPE_COUNT, ReproductiveOrgan, ReproductiveState,
    },
    random::Rng,
};

const NIKAU_SEED_DOMAIN: u64 = 0x6e69_6b61_755f_7061;
const TEXTURE_SIZE: u32 = 256;
const LEAF_TILE_SIZE: u32 = 128;
const LEAF_ATLAS_COLUMNS: u32 = 2;
const LEAF_ATLAS_SIZE: u32 = LEAF_TILE_SIZE * LEAF_ATLAS_COLUMNS;
const GOLDEN_ANGLE: f32 = 2.399_963_1;

#[derive(Clone, Copy)]
enum StemSurface {
    RingedTrunk,
    GreenCrown,
}

pub(super) fn generate_nikau_prototype(
    seed: u64,
    recipe: BotanicalRecipe,
) -> Result<BotanicalPrototype, String> {
    let mut rng = Rng::new(seed ^ NIKAU_SEED_DOMAIN);
    let graph = nikau_graph(recipe, &mut rng);
    let leaves = nikau_leaflets(recipe, &graph, &mut rng)?;
    let foliage_pads = nikau_foliage_pads(&graph);
    let reproductive_organs = nikau_reproductive_organs(recipe, &graph, &mut rng);
    let (wood, wood_bark) = nikau_wood(recipe, &graph)?;
    Ok(BotanicalPrototype {
        species: recipe.species,
        graph,
        wood,
        wood_bark,
        wood_scars: Mesh::default(),
        wood_scar_albedo: solid_texture(64, [96, 82, 59, 255]),
        microtwigs: Mesh::default(),
        microtwig_bark: Vec::new(),
        leaf_archetypes: nikau_leaf_archetypes(),
        shoot_tip_archetypes: std::array::from_fn(|_| Mesh::default()),
        reproductive_archetypes: nikau_reproductive_archetypes(),
        foliage_pad_archetypes: nikau_pad_archetypes(),
        leaves,
        shoot_tips: Vec::new(),
        reproductive_organs,
        foliage_pads,
        bark_albedo: nikau_bark_albedo(seed),
        bark_normal: nikau_bark_normal(seed),
        bark_depth: nikau_bark_depth(seed),
        bark_metallic_roughness: nikau_bark_metallic_roughness(seed),
        leaf_albedo: nikau_leaf_albedo(seed),
        leaf_metallic_roughness: nikau_leaf_metallic_roughness(seed),
    })
}

pub(super) fn generate_nikau_frond_prototype(
    seed: u64,
    recipe: BotanicalRecipe,
) -> Result<BotanicalPrototype, String> {
    let mut prototype = generate_nikau_prototype(seed, recipe)?;
    let frond_count = prototype.graph.axes.len().saturating_sub(2);
    // A mature outer frond exposes the full rachis arc and relaxed pinnae more
    // clearly than a near-vertical emerging spear.
    let selected_axis = 2 + frond_count.saturating_mul(3) / 4;
    let source = *prototype
        .graph
        .axes
        .get(selected_axis)
        .ok_or_else(|| "nīkau frond review requires a live frond axis".to_string())?;
    let translation = Vec3::Z * 3.0 - source.points_metres[0];
    let axis = Axis {
        parent: None,
        points_metres: std::array::from_fn(|index| source.points_metres[index] + translation),
        ..source
    };
    let (wood, wood_bark) = nikau_frond_wood(recipe, axis)?;

    prototype
        .leaves
        .retain(|leaf| leaf.axis == selected_axis as u32);
    for leaf in &mut prototype.leaves {
        leaf.axis = 0;
        leaf.blade_base_metres += translation;
    }
    prototype
        .foliage_pads
        .retain(|pad| pad.axis == selected_axis as u32);
    for pad in &mut prototype.foliage_pads {
        pad.axis = 0;
        pad.centre_metres += translation;
    }
    prototype.graph = AxisGraph { axes: vec![axis] };
    prototype.wood = wood;
    prototype.wood_bark = wood_bark;
    prototype.reproductive_organs.clear();
    Ok(prototype)
}

fn nikau_reproductive_organs(
    recipe: BotanicalRecipe,
    graph: &AxisGraph,
    rng: &mut Rng,
) -> Vec<ReproductiveOrgan> {
    let shaft = graph.axes[1];
    let (base, _, _) = shaft.sample(0.18);
    (0..7)
        .map(|index| {
            let phase = index as f32 / 7.0 * TAU + rng.range(-0.10, 0.10);
            let radial = Vec3::new(phase.cos(), phase.sin(), 0.0);
            let direction = (radial * rng.range(0.48, 0.64) - Vec3::Z * rng.range(0.76, 0.90))
                .normalize_or(-Vec3::Z);
            ReproductiveOrgan {
                axis: 1,
                base_metres: base
                    + radial * recipe.trunk_radius_metres * 1.12
                    + Vec3::Z * rng.range(-0.05, 0.06),
                direction,
                length_metres: rng.range(0.44, 0.58),
                radius_metres: rng.range(0.072, 0.092),
                state: if index == 0 {
                    ReproductiveState::Flower
                } else {
                    ReproductiveState::Fruit
                },
                variation: phase,
            }
        })
        .collect()
}

fn nikau_graph(recipe: BotanicalRecipe, rng: &mut Rng) -> AxisGraph {
    let trunk = trunk_axis(recipe, rng);
    let crownshaft = crownshaft_axis(recipe, trunk, rng);
    let mut axes = Vec::with_capacity(usize::from(recipe.primary_count) + 2);
    axes.extend([trunk, crownshaft]);
    for frond in 0..usize::from(recipe.primary_count) {
        axes.push(frond_axis(recipe, crownshaft, frond, rng));
    }
    AxisGraph { axes }
}

fn trunk_axis(recipe: BotanicalRecipe, rng: &mut Rng) -> Axis {
    let phase = rng.range(0.0, TAU);
    let lean = Vec3::new(phase.cos(), phase.sin(), 0.0)
        * rng.range(0.10, 0.24)
        * recipe.trunk_character_scale();
    let points_metres = std::array::from_fn(|index| {
        let t = index as f32 / (AXIS_POINTS - 1) as f32;
        let wind_curve =
            lean * t.powf(1.65) + Vec3::new(-lean.y, lean.x, 0.0) * (t * PI).sin() * 0.035;
        wind_curve + Vec3::Z * recipe.trunk_height_metres * t
    });
    let radii_metres = std::array::from_fn(|index| {
        let t = index as f32 / (AXIS_POINTS - 1) as f32;
        recipe.trunk_radius_metres * (1.0 - t * 0.10)
    });
    Axis {
        parent: None,
        order: 0,
        points_metres,
        radii_metres,
        exposure: 0.82,
        alive: true,
    }
}

fn crownshaft_axis(recipe: BotanicalRecipe, trunk: Axis, rng: &mut Rng) -> Axis {
    let trunk_tip = trunk.points_metres[AXIS_POINTS - 1];
    let (_, trunk_tangent, _) = trunk.sample(0.99);
    let overlap = (recipe.trunk_radius_metres * 0.65).clamp(0.10, 0.17);
    let base = trunk_tip - trunk_tangent * overlap;
    let shaft_height = (recipe.trunk_height_metres * 0.20).clamp(1.02, 1.22);
    let top = trunk_tip + Vec3::Z * shaft_height;
    let drift = Vec3::new(rng.range(-0.025, 0.025), rng.range(-0.025, 0.025), 0.0);
    let points_metres = std::array::from_fn(|index| {
        let t = index as f32 / (AXIS_POINTS - 1) as f32;
        base.lerp(top, t) + drift * (t * PI).sin()
    });
    let radii_metres = std::array::from_fn(|index| {
        let t = index as f32 / (AXIS_POINTS - 1) as f32;
        let bulge = (t * PI).sin().max(0.0);
        recipe.trunk_radius_metres * (1.06 + bulge * 0.34 - t * 0.10)
    });
    Axis {
        parent: Some(0),
        order: 0,
        points_metres,
        radii_metres,
        exposure: 1.0,
        alive: true,
    }
}

fn frond_axis(recipe: BotanicalRecipe, crownshaft: Axis, index: usize, rng: &mut Rng) -> Axis {
    let count = usize::from(recipe.primary_count);
    let age = (index as f32 / count.saturating_sub(1).max(1) as f32).powf(0.86);
    let phase = index as f32 * GOLDEN_ANGLE + rng.range(-0.14, 0.14);
    let radial = Vec3::new(phase.cos(), phase.sin(), 0.0);
    let tangent = Vec3::new(-phase.sin(), phase.cos(), 0.0);
    let length = (3.12 + age.sqrt() * 0.82) * rng.range(0.94, 1.06);
    let elevation = (1.32 - age * 1.15 + rng.range(-0.075, 0.075)).clamp(0.10, 1.38);
    let spread = recipe.crown_spread_scale();
    let droop = (0.025 + age.powf(1.45) * 0.72) * recipe.branch_droop_scale();
    let crown_top = crownshaft.points_metres[AXIS_POINTS - 1];
    let origin = crown_top - Vec3::Z * (0.03 + age * 0.62) + radial * (0.04 + age * 0.10);
    let points_metres = std::array::from_fn(|point| {
        let t = point as f32 / (AXIS_POINTS - 1) as f32;
        let outward = elevation.cos() * t;
        let rise = elevation.sin().mul_add(t, -droop * t.powf(2.15));
        origin
            + radial * length * outward * spread
            + tangent * length * rng.range(-0.012, 0.012) * (t * PI).sin()
            + Vec3::Z * length * rise
    });
    let radii_metres = std::array::from_fn(|point| {
        let t = point as f32 / (AXIS_POINTS - 1) as f32;
        (0.045 * (1.0 - t).powf(1.35) + 0.007) * (1.0 - age * 0.08)
    });
    Axis {
        parent: Some(1),
        order: 1,
        points_metres,
        radii_metres,
        exposure: (0.96 - age * 0.18).clamp(0.0, 1.0),
        alive: true,
    }
}

fn nikau_leaflets(
    recipe: BotanicalRecipe,
    graph: &AxisGraph,
    rng: &mut Rng,
) -> Result<Vec<LeafOrgan>, String> {
    let pair_count = usize::from(recipe.leaves_per_terminal);
    let frond_count = usize::from(recipe.primary_count);
    let mut leaves = Vec::with_capacity(frond_count.saturating_mul(pair_count).saturating_mul(2));
    for (frond_index, axis) in graph.axes.iter().enumerate().skip(2) {
        let age = (frond_index - 2) as f32 / frond_count.saturating_sub(1).max(1) as f32;
        let axis_id = u32::try_from(frond_index).map_err(|_| "nīkau frond index exceeds u32")?;
        for station in 0..pair_count {
            let station_t = station as f32 / pair_count.saturating_sub(1).max(1) as f32;
            let envelope = (station_t * PI).sin().max(0.0).powf(0.62);
            let base_length = (0.14 + envelope * 0.88) * (1.0 - age * 0.04);
            for sign in [-1.0_f32, 1.0] {
                // Nīkau pinnae are loosely paired, not mirrored clones. A small
                // alternating stagger and independent blade dimensions break
                // the synthetic ladder rhythm without losing the species'
                // strong pinnate structure.
                let attachment = (0.045
                    + station_t * 0.93
                    + sign * rng.range(0.0015, 0.0045)
                    + rng.range(-0.0025, 0.0025))
                .clamp(0.025, 0.985);
                let (origin, tangent, _) = axis.sample(attachment);
                let up = (Vec3::Z - tangent * tangent.z).normalize_or(Vec3::X);
                let side = tangent.cross(up).normalize_or(Vec3::Y);
                let damage_scale = if rng.unit() < 0.04 {
                    rng.range(0.72, 0.90)
                } else {
                    1.0
                };
                let length = base_length * rng.range(0.90, 1.12) * damage_scale;
                let width = length * rng.range(0.043, 0.061);
                let fold = (0.16 - age * 0.055
                    + (station_t * TAU * 2.3 + sign).sin() * 0.035
                    + rng.range(-0.035, 0.035))
                .clamp(0.045, 0.26);
                // Fan the pinnae progressively along the rachis. Real fronds
                // do not use one repeated right angle: basal blades trail,
                // mid-frond blades open broadside, and apical blades sweep
                // toward the tip.
                let forward = -0.10 + station_t * 0.95 + rng.range(-0.035, 0.035);
                let paired_plane = side * sign * fold.cos() + up * fold.sin();
                let gravity =
                    ((0.075 + age * 0.29) * (0.38 + envelope * 0.62) * rng.range(0.86, 1.18)
                        + rng.range(-0.012, 0.025))
                    .max(0.0);
                let direction = (paired_plane + tangent * forward - Vec3::Z * gravity)
                    .normalize_or(side * sign);
                // The renderer's reflected Bevy basis recovers botanical width
                // as direction × normal. Construct the normal from the rachis
                // projection, then orient every upper face toward the sky so
                // the archetype's negative-normal bend is gravitational on
                // both sides of the rachis.
                let width_axis =
                    (tangent - direction * tangent.dot(direction)).normalize_or(tangent);
                let base_normal = direction.cross(width_axis).normalize_or(up);
                let base_normal = if base_normal.dot(Vec3::Z) < 0.0 {
                    -base_normal
                } else {
                    base_normal
                };
                let roll = sign * (0.045 + age * 0.035)
                    + (station_t * TAU * 3.1 + sign).sin() * 0.065
                    + rng.range(-0.055, 0.055);
                let normal = (base_normal * roll.cos() + direction.cross(base_normal) * roll.sin())
                    .normalize_or(base_normal);
                let variation = rng.range(0.0, TAU);
                leaves.push(LeafOrgan {
                    axis: axis_id,
                    blade_base_metres: origin
                        + side * sign * axis.radii_metres[0] * 0.16
                        + tangent * sign * rng.range(-0.014, 0.014)
                        + Vec3::Z * rng.range(-0.022, 0.022),
                    direction,
                    normal,
                    length_metres: length,
                    width_metres: width,
                    archetype: leaflet_archetype(age, sign, station),
                    age: (0.15 + age * 0.76 + rng.range(-0.04, 0.04)).clamp(0.0, 1.0),
                    light_exposure: (axis.exposure + direction.z * 0.10).clamp(0.0, 1.0),
                    variation,
                });
            }
        }
    }
    Ok(leaves)
}

fn nikau_foliage_pads(graph: &AxisGraph) -> Vec<FoliagePad> {
    graph
        .axes
        .iter()
        .enumerate()
        .skip(2)
        .map(|(axis_index, axis)| {
            let start = axis.points_metres[0];
            let tip = axis.points_metres[AXIS_POINTS - 1];
            let direction = (tip - start).normalize_or(Vec3::X);
            let normal = (Vec3::Z - direction * direction.z).normalize_or(Vec3::Z);
            let length = start.distance(tip);
            FoliagePad {
                axis: axis_index as u32,
                centre_metres: (start + tip) * 0.5,
                direction,
                normal,
                // The generic pad transform maps Y to surface relief and Z to
                // the lateral frond span. Preserve that convention here.
                half_extents_metres: Vec3::new(length * 0.5, 0.34, 1.02),
                archetype: (axis_index % FOLIAGE_PAD_ARCHETYPE_COUNT) as u8,
                mean_age: ((axis_index - 2) as f32
                    / graph.axes.len().saturating_sub(3).max(1) as f32)
                    .clamp(0.0, 1.0),
                light_exposure: axis.exposure,
                density: 0.92,
                variation: axis_index as f32 * GOLDEN_ANGLE,
            }
        })
        .collect()
}

fn nikau_wood(
    recipe: BotanicalRecipe,
    graph: &AxisGraph,
) -> Result<(Mesh, Vec<BarkVertex>), String> {
    let mut mesh = Mesh::default();
    let mut bark = Vec::new();
    append_axis_tube(
        &mut mesh,
        &mut bark,
        graph.axes[0],
        48,
        24,
        StemSurface::RingedTrunk,
        true,
        false,
        recipe.trunk_height_metres,
    )?;
    append_axis_tube(
        &mut mesh,
        &mut bark,
        graph.axes[1],
        14,
        24,
        StemSurface::GreenCrown,
        false,
        true,
        recipe.trunk_height_metres,
    )?;
    for axis in &graph.axes[2..] {
        append_axis_tube(
            &mut mesh,
            &mut bark,
            *axis,
            12,
            8,
            StemSurface::GreenCrown,
            true,
            true,
            recipe.trunk_height_metres,
        )?;
    }
    mesh.calculate_normals();
    Ok((mesh, bark))
}

fn nikau_frond_wood(
    recipe: BotanicalRecipe,
    axis: Axis,
) -> Result<(Mesh, Vec<BarkVertex>), String> {
    let mut mesh = Mesh::default();
    let mut bark = Vec::new();
    append_axis_tube(
        &mut mesh,
        &mut bark,
        axis,
        32,
        10,
        StemSurface::GreenCrown,
        true,
        true,
        recipe.trunk_height_metres,
    )?;
    mesh.calculate_normals();
    Ok((mesh, bark))
}

#[allow(clippy::too_many_arguments)]
fn append_axis_tube(
    mesh: &mut Mesh,
    bark: &mut Vec<BarkVertex>,
    axis: Axis,
    rings: usize,
    sides: usize,
    surface: StemSurface,
    cap_base: bool,
    cap_tip: bool,
    trunk_height: f32,
) -> Result<(), String> {
    let base = u32::try_from(mesh.vertices.len()).map_err(|_| "nīkau wood exceeds u32")?;
    let ring_vertices = sides + 1;
    let mut frame = nikau_frame(axis.sample(0.01).1);
    let mut cumulative = 0.0;
    let mut previous = axis.points_metres[0];
    for ring in 0..rings {
        let t = ring as f32 / rings.saturating_sub(1).max(1) as f32;
        let (position, tangent, sampled_radius) = axis.sample(t);
        if ring > 0 {
            cumulative += position.distance(previous);
        }
        previous = position;
        frame = transport_frame(frame, tangent);
        for side_index in 0..=sides {
            let angle = side_index as f32 / sides as f32 * TAU;
            let mut radius = sampled_radius;
            if matches!(surface, StemSurface::RingedTrunk) {
                let ground_flare = (1.0 - (position.z / 0.72).clamp(0.0, 1.0)).powf(1.7);
                let scar_coordinate = position.z / 0.31 + (position.z * 2.37).sin() * 0.055;
                let scar_phase = scar_coordinate.fract();
                let scar = (1.0 - ((scar_phase - 0.5).abs() * 13.0).min(1.0)).powi(2);
                radius *= 1.0 + ground_flare * 0.40 + scar * 0.032;
            } else if axis.order == 0 {
                radius *= 1.0 + (t * PI).sin() * 0.10;
            }
            let radial = frame.0 * angle.cos() + frame.1 * angle.sin();
            mesh.vertices.push(position + radial * radius);
            let maturity = match surface {
                StemSurface::RingedTrunk => 0.82,
                StemSurface::GreenCrown => 0.06,
            };
            bark.push(BarkVertex {
                radius_metres: sampled_radius,
                maturity,
            });
            let u = match surface {
                StemSurface::RingedTrunk => side_index as f32 / sides as f32 * 0.46 + 0.01,
                StemSurface::GreenCrown => side_index as f32 / sides as f32 * 0.46 + 0.52,
            };
            let v = match surface {
                StemSurface::RingedTrunk => position.z / trunk_height.max(0.1) * 8.0,
                StemSurface::GreenCrown => cumulative * 0.72,
            };
            mesh.uv.push(Vec2::new(u, v));
        }
    }
    for ring in 0..rings - 1 {
        let lower = base + (ring * ring_vertices) as u32;
        let upper = lower + ring_vertices as u32;
        for side in 0..sides {
            let next = side + 1;
            mesh.triangles.extend([
                lower + side as u32,
                lower + next as u32,
                upper + side as u32,
                lower + next as u32,
                upper + next as u32,
                upper + side as u32,
            ]);
        }
    }
    if cap_base {
        append_tube_cap(mesh, bark, base, axis.points_metres[0], sides, false)?;
    }
    if cap_tip {
        let last = base + ((rings - 1) * ring_vertices) as u32;
        append_tube_cap(
            mesh,
            bark,
            last,
            axis.points_metres[AXIS_POINTS - 1],
            sides,
            true,
        )?;
    }
    Ok(())
}

fn append_tube_cap(
    mesh: &mut Mesh,
    bark: &mut Vec<BarkVertex>,
    ring: u32,
    position: Vec3,
    sides: usize,
    tip: bool,
) -> Result<(), String> {
    let centre = u32::try_from(mesh.vertices.len()).map_err(|_| "nīkau wood exceeds u32")?;
    mesh.vertices.push(position);
    mesh.uv.push(Vec2::new(0.75, 0.5));
    bark.push(BarkVertex {
        radius_metres: 0.01,
        maturity: 0.05,
    });
    for side in 0..sides {
        let next = side + 1;
        let triangle = if tip {
            [ring + side as u32, ring + next as u32, centre]
        } else {
            [ring + next as u32, ring + side as u32, centre]
        };
        mesh.triangles.extend(triangle);
    }
    Ok(())
}

fn nikau_frame(tangent: Vec3) -> (Vec3, Vec3, Vec3) {
    let tangent = tangent.normalize_or(Vec3::Z);
    let reference = if tangent.z.abs() < 0.88 {
        Vec3::Z
    } else {
        Vec3::X
    };
    let x = tangent.cross(reference).normalize_or(Vec3::X);
    let y = tangent.cross(x).normalize_or(Vec3::Y);
    (x, y, tangent)
}

fn transport_frame(frame: (Vec3, Vec3, Vec3), tangent: Vec3) -> (Vec3, Vec3, Vec3) {
    let tangent = tangent.normalize_or(frame.2);
    let x = (frame.0 - tangent * frame.0.dot(tangent))
        .try_normalize()
        .unwrap_or_else(|| nikau_frame(tangent).0);
    (x, tangent.cross(x).normalize_or(frame.1), tangent)
}

fn nikau_leaf_archetypes() -> [Mesh; LEAF_ARCHETYPE_COUNT] {
    std::array::from_fn(|index| leaflet_mesh(index as u8))
}

fn nikau_reproductive_archetypes() -> [Mesh; REPRODUCTIVE_ARCHETYPE_COUNT] {
    [cluster_mesh(0.72), cluster_mesh(1.0)]
}

fn cluster_mesh(scale: f32) -> Mesh {
    const SITES: [(f32, f32, f32); 13] = [
        (0.18, 0.00, 0.00),
        (0.28, 0.38, 0.08),
        (0.31, -0.34, -0.10),
        (0.42, 0.58, -0.18),
        (0.45, -0.55, 0.16),
        (0.53, 0.18, 0.46),
        (0.56, -0.15, -0.43),
        (0.64, 0.55, 0.28),
        (0.67, -0.58, -0.24),
        (0.75, 0.25, -0.50),
        (0.78, -0.22, 0.52),
        (0.87, 0.43, 0.02),
        (0.90, -0.40, -0.04),
    ];
    let mut mesh = Mesh::default();
    for (x, y, z) in SITES {
        let centre = Vec3::new(x, y, z);
        append_cluster_stem(&mut mesh, centre, scale * 0.022);
        append_cluster_ovoid(&mut mesh, centre, scale);
    }
    mesh.calculate_normals();
    mesh
}

fn append_cluster_stem(mesh: &mut Mesh, end: Vec3, half_width: f32) {
    let direction = end.normalize_or(Vec3::X);
    let first = direction.cross(Vec3::Z).normalize_or(Vec3::Y) * half_width;
    let second = direction.cross(first).normalize_or(Vec3::Z) * half_width;
    for side in [first, second] {
        let base = mesh.vertices.len() as u32;
        mesh.vertices.extend([-side, side, end + side, end - side]);
        mesh.uv.extend([
            Vec2::new(0.0, 0.0),
            Vec2::new(1.0, 0.0),
            Vec2::new(1.0, 1.0),
            Vec2::new(0.0, 1.0),
        ]);
        mesh.triangles
            .extend([base, base + 1, base + 2, base, base + 2, base + 3]);
    }
}

fn append_cluster_ovoid(mesh: &mut Mesh, centre: Vec3, scale: f32) {
    const LATITUDES: usize = 6;
    const LONGITUDES: usize = 8;
    let base = mesh.vertices.len() as u32;
    for latitude in 0..=LATITUDES {
        let v = latitude as f32 / LATITUDES as f32;
        let polar = v * PI;
        for longitude in 0..=LONGITUDES {
            let u = longitude as f32 / LONGITUDES as f32;
            let azimuth = u * TAU;
            let sphere = Vec3::new(
                polar.cos() * 0.030,
                polar.sin() * azimuth.cos() * 0.13,
                polar.sin() * azimuth.sin() * 0.10,
            ) * scale;
            mesh.vertices.push(centre + sphere);
            mesh.uv.push(Vec2::new(u, v));
        }
    }
    let stride = LONGITUDES + 1;
    for latitude in 0..LATITUDES {
        for longitude in 0..LONGITUDES {
            let lower = base + (latitude * stride + longitude) as u32;
            let upper = lower + stride as u32;
            mesh.triangles
                .extend([lower, upper, lower + 1, lower + 1, upper, upper + 1]);
        }
    }
}

fn leaflet_mesh(archetype: u8) -> Mesh {
    const STATIONS: usize = 21;
    const COLUMNS: [f32; 5] = [-1.0, -0.5, 0.0, 0.5, 1.0];
    let mut mesh = Mesh::default();
    let relief = 0.034 + f32::from(archetype % 4) * 0.007;
    let sweep = 0.014 + f32::from(archetype % 4) * 0.004;
    let twist = (f32::from(archetype) - 3.5) * 0.0035;
    let tip_droop = 0.140 + f32::from(archetype % 4) * 0.022 + f32::from(archetype / 4) * 0.065;
    for station in 0..STATIONS {
        let t = station as f32 / (STATIONS - 1) as f32;
        let base_taper = (t / 0.13).clamp(0.0, 1.0);
        let tip_taper = ((1.0 - t) / 0.30).clamp(0.0, 1.0);
        let base_taper = base_taper * base_taper * (3.0 - 2.0 * base_taper);
        let tip_taper = tip_taper * tip_taper * (3.0 - 2.0 * tip_taper);
        let profile = base_taper * tip_taper * (1.0 - t * 0.12);
        for lateral in COLUMNS {
            let edge_wave = if lateral.abs() > 0.9 {
                (t.mul_add(TAU * 4.7, f32::from(archetype) * 0.61)).sin() * (t * PI).sin() * 0.002
            } else {
                0.0
            };
            mesh.vertices.push(Vec3::new(
                t,
                lateral * (profile * 0.50 + edge_wave) - sweep * t.powf(1.85),
                (1.0 - lateral.abs().powf(1.35)) * profile * relief
                    + twist * lateral * (t * PI).sin()
                    - tip_droop * t.powf(1.70),
            ));
            mesh.uv.push(leaf_uv(
                archetype % 4,
                Vec2::new(lateral.mul_add(0.5, 0.5), t),
            ));
        }
    }
    append_grid_triangles(&mut mesh, STATIONS, COLUMNS.len());
    mesh.calculate_normals();
    mesh
}

fn leaflet_archetype(age: f32, sign: f32, station: usize) -> u8 {
    let cohort = if age < 0.16 {
        1
    } else if age < 0.58 {
        0
    } else if age < 0.84 {
        2
    } else {
        3
    };
    let alternate_shape = (station + usize::from(sign > 0.0)).is_multiple_of(3);
    cohort + if alternate_shape { 4 } else { 0 }
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

fn nikau_pad_archetypes() -> [Mesh; FOLIAGE_PAD_ARCHETYPE_COUNT] {
    [proxy_frond_mesh(0.08), proxy_frond_mesh(0.18)]
}

fn proxy_frond_mesh(droop: f32) -> Mesh {
    const PAIRS: usize = 20;
    let mut mesh = Mesh::default();
    for pair in 0..PAIRS {
        let t = pair as f32 / (PAIRS - 1) as f32;
        let x = t.mul_add(1.78, -0.89);
        let envelope = (t * PI).sin().max(0.0).powf(0.42);
        for sign in [-1.0_f32, 1.0] {
            let base = mesh.vertices.len() as u32;
            let root = Vec3::new(x, sign * 0.025, -droop * x.abs().powf(1.7));
            let tip = Vec3::new(
                x + 0.06,
                sign * (0.20 + envelope * 0.78),
                0.10 * envelope - droop * x.abs().powf(1.7),
            );
            let half_width = 0.025 + envelope * 0.028;
            mesh.vertices.extend([
                root - Vec3::X * half_width,
                root + Vec3::X * half_width,
                tip + Vec3::X * half_width * 0.18,
                tip - Vec3::X * half_width * 0.18,
            ]);
            mesh.uv.extend([
                leaf_uv(0, Vec2::new(0.0, 0.0)),
                leaf_uv(0, Vec2::new(1.0, 0.0)),
                leaf_uv(0, Vec2::new(1.0, 1.0)),
                leaf_uv(0, Vec2::new(0.0, 1.0)),
            ]);
            mesh.triangles
                .extend([base, base + 1, base + 2, base, base + 2, base + 3]);
        }
    }
    mesh.calculate_normals();
    mesh
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

fn nikau_bark_albedo(seed: u64) -> BotanicalTexture {
    texture(TEXTURE_SIZE, TEXTURE_SIZE, |x, y| {
        let noise = hash_unit(seed ^ 0x616c_6265, x, y) - 0.5;
        if x < TEXTURE_SIZE / 2 {
            let ring = ring_response(x, y);
            let vertical_stain = ((x as f32 / 11.0 + y as f32 / 53.0).sin()
                + (x as f32 / 29.0 - y as f32 / 37.0).sin())
                * 0.018;
            let base = Vec3::new(0.31, 0.34, 0.30) + Vec3::splat(noise * 0.045 + vertical_stain);
            encode_colour(base + Vec3::new(0.15, 0.14, 0.11) * ring)
        } else {
            let vertical = ((x as f32 / 9.0 + y as f32 / 47.0).sin() * 0.5 + 0.5) * 0.035;
            encode_colour(Vec3::new(0.14, 0.31, 0.16) + Vec3::splat(noise * 0.018 + vertical))
        }
    })
}

fn nikau_bark_normal(seed: u64) -> BotanicalTexture {
    texture(TEXTURE_SIZE, TEXTURE_SIZE, |x, y| {
        let left = bark_height(seed, x.cast_signed() - 1, y.cast_signed());
        let right = bark_height(seed, x.cast_signed() + 1, y.cast_signed());
        let down = bark_height(seed, x.cast_signed(), y.cast_signed() - 1);
        let up = bark_height(seed, x.cast_signed(), y.cast_signed() + 1);
        encode_normal(Vec3::new((left - right) * 1.4, (down - up) * 1.8, 1.0))
    })
}

fn nikau_bark_depth(seed: u64) -> BotanicalTexture {
    texture(TEXTURE_SIZE, TEXTURE_SIZE, |x, y| {
        let value = ((1.0 - bark_height(seed, x.cast_signed(), y.cast_signed())) * 255.0) as u8;
        [value, value, value, 255]
    })
}

fn nikau_bark_metallic_roughness(seed: u64) -> BotanicalTexture {
    texture(TEXTURE_SIZE, TEXTURE_SIZE, |x, y| {
        let noise = hash_unit(seed ^ 0x726f_7567, x, y);
        let roughness = if x < TEXTURE_SIZE / 2 {
            0.76 + noise * 0.10
        } else {
            0.52 + noise * 0.08
        };
        [255, (roughness * 255.0) as u8, 0, 255]
    })
}

fn bark_height(seed: u64, x: i32, y: i32) -> f32 {
    let x = x.rem_euclid(TEXTURE_SIZE.cast_signed()).cast_unsigned();
    let y = y.rem_euclid(TEXTURE_SIZE.cast_signed()).cast_unsigned();
    let noise = hash_unit(seed ^ 0x6865_6967, x, y) - 0.5;
    if x < TEXTURE_SIZE / 2 {
        (0.56 + ring_response(x, y) * 0.28 + noise * 0.045).clamp(0.0, 1.0)
    } else {
        (0.66 + noise * 0.018).clamp(0.0, 1.0)
    }
}

fn ring_response(x: u32, y: u32) -> f32 {
    let wobble = (x as f32 / 17.0).sin() * 2.2 + (y as f32 / 43.0).sin() * 1.4;
    let phase = ((y as f32 + wobble).rem_euclid(32.0)) / 32.0;
    (1.0 - ((phase - 0.52).abs() * 12.0).min(1.0)).powf(2.4)
}

fn nikau_leaf_albedo(seed: u64) -> BotanicalTexture {
    texture(LEAF_ATLAS_SIZE, LEAF_ATLAS_SIZE, |x, y| {
        let tile = (x / LEAF_TILE_SIZE) + (y / LEAF_TILE_SIZE) * 2;
        let local_x = x % LEAF_TILE_SIZE;
        let local_y = y % LEAF_TILE_SIZE;
        let noise = hash_unit(seed ^ u64::from(tile) ^ 0x6c65_6166, x, y) - 0.5;
        let vein =
            (1.0 - ((local_x as f32 / (LEAF_TILE_SIZE - 1) as f32 - 0.5).abs() * 20.0)).max(0.0);
        let fibres = (local_y as f32 * 0.43 + local_x as f32 * 0.11).sin() * 0.012;
        let base = match tile {
            1 => Vec3::new(0.19, 0.39, 0.14),
            2 => Vec3::new(0.26, 0.39, 0.13),
            3 => Vec3::new(0.31, 0.35, 0.11),
            _ => Vec3::new(0.14, 0.35, 0.12),
        };
        encode_colour(
            base + Vec3::splat(noise * 0.030 + fibres) + Vec3::new(0.03, 0.055, 0.018) * vein,
        )
    })
}

fn nikau_leaf_metallic_roughness(seed: u64) -> BotanicalTexture {
    texture(LEAF_ATLAS_SIZE, LEAF_ATLAS_SIZE, |x, y| {
        let noise = hash_unit(seed ^ 0x6d72_6c66, x, y);
        [255, ((0.47 + noise * 0.13) * 255.0) as u8, 0, 255]
    })
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

fn encode_normal(normal: Vec3) -> [u8; 4] {
    let normal = normal.normalize_or(Vec3::Z);
    [
        ((normal.x * 0.5 + 0.5) * 255.0) as u8,
        ((normal.y * 0.5 + 0.5) * 255.0) as u8,
        ((normal.z * 0.5 + 0.5) * 255.0) as u8,
        255,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{BotanicalSpecies, generate_botanical_prototype, generate_nikau_frond_prototype};

    #[test]
    fn nikau_is_deterministic_solitary_and_pinnate() {
        let recipe = BotanicalRecipe::for_species(BotanicalSpecies::Nikau);
        let first = generate_botanical_prototype(42, recipe).unwrap();
        let second = generate_botanical_prototype(42, recipe).unwrap();
        assert_eq!(first, second);
        assert_eq!(
            first.graph.axes.len(),
            usize::from(recipe.primary_count) + 2
        );
        assert_eq!(
            first
                .graph
                .axes
                .iter()
                .filter(|axis| axis.order == 0)
                .count(),
            2
        );
        assert!(first.graph.axes.iter().all(|axis| axis.order <= 1));
        let youngest_frond = first.graph.axes[2];
        let youngest_chord =
            youngest_frond.points_metres[AXIS_POINTS - 1] - youngest_frond.points_metres[0];
        let youngest_horizontal = Vec3::new(youngest_chord.x, youngest_chord.y, 0.0).length();
        assert!(youngest_horizontal > youngest_chord.length() * 0.20);
        let trunk_tip = first.graph.axes[0].points_metres[AXIS_POINTS - 1];
        let crownshaft_base = first.graph.axes[1].points_metres[0];
        assert!(crownshaft_base.z < trunk_tip.z);
        assert!((0.10..=0.171).contains(&crownshaft_base.distance(trunk_tip)));
        assert_eq!(
            first.leaves.len(),
            usize::from(recipe.primary_count) * usize::from(recipe.leaves_per_terminal) * 2
        );
        assert_eq!(first.foliage_pads.len(), usize::from(recipe.primary_count));
        assert!(first.shoot_tips.is_empty());
        assert_eq!(first.reproductive_organs.len(), 7);
        assert!(
            first
                .reproductive_archetypes
                .iter()
                .all(|mesh| !mesh.vertices.is_empty())
        );
        assert!(first.wood.vertices.len() > 1_000);
    }

    #[test]
    fn frond_prototype_contains_one_axis_and_only_its_leaflets() {
        let recipe = BotanicalRecipe::for_species(BotanicalSpecies::Nikau);
        let frond = generate_nikau_frond_prototype(42, recipe).unwrap();
        assert_eq!(frond.graph.axes.len(), 1);
        assert_eq!(
            frond.leaves.len(),
            usize::from(recipe.leaves_per_terminal) * 2
        );
        assert!(frond.leaves.iter().all(|leaf| leaf.axis == 0));
        assert_eq!(frond.foliage_pads.len(), 1);
        assert!(frond.reproductive_organs.is_empty());
        assert!(!frond.wood.vertices.is_empty());
    }
}
