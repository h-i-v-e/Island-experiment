//! Species-specific mānuka brush architecture.
//!
//! This mature coastal form keeps one slender clear leader below a compact,
//! wind-shaped umbrella crown. Crown scaffolds arise from the upper leader
//! rather than independently at ground level; their fine ascending sprays
//! carry sharp narrow leaves, solitary five-petalled flowers and papery bark.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss
)]

use std::f32::consts::{PI, TAU};

use motu::{Mesh, Vec2, Vec3};

use super::{
    generator::{foliage_pad_archetypes, generate_foliage_pads, generate_wood},
    model::{
        AXIS_POINTS, Axis, AxisGraph, BotanicalPrototype, BotanicalRecipe, BotanicalTexture,
        LEAF_ARCHETYPE_COUNT, LeafOrgan,
    },
    random::Rng,
};

const MANUKA_SEED_DOMAIN: u64 = 0x6d61_6e75_6b61_5f31;
const TEXTURE_SIZE: u32 = 256;
const BARK_HEIGHT: u32 = 512;
const ATLAS_COLUMNS: u32 = 2;
const ATLAS_SIZE: u32 = TEXTURE_SIZE;
const TILE_SIZE: u32 = ATLAS_SIZE / ATLAS_COLUMNS;

pub(super) fn generate_manuka_prototype(
    seed: u64,
    recipe: BotanicalRecipe,
) -> Result<BotanicalPrototype, String> {
    let mut rng = Rng::new(seed ^ MANUKA_SEED_DOMAIN);
    let graph = manuka_graph(recipe, &mut rng)?;
    let leaves = manuka_foliage(recipe, &graph, &mut rng)?;
    let foliage_pads = generate_foliage_pads(&graph, &leaves);
    let (wood, wood_bark, wood_scars) = generate_wood(seed ^ MANUKA_SEED_DOMAIN, &graph)?;

    Ok(BotanicalPrototype {
        species: recipe.species,
        graph,
        wood,
        wood_bark,
        wood_scars,
        wood_scar_albedo: solid_texture(64, [111, 87, 62, 255]),
        microtwigs: Mesh::default(),
        microtwig_bark: Vec::new(),
        leaf_archetypes: manuka_archetypes(),
        shoot_tip_archetypes: std::array::from_fn(|_| Mesh::default()),
        reproductive_archetypes: std::array::from_fn(|_| Mesh::default()),
        foliage_pad_archetypes: foliage_pad_archetypes(),
        leaves,
        shoot_tips: Vec::new(),
        reproductive_organs: Vec::new(),
        foliage_pads,
        bark_albedo: manuka_bark_albedo(seed),
        bark_normal: manuka_bark_normal(seed),
        bark_depth: manuka_bark_depth(seed),
        bark_metallic_roughness: solid_texture(TEXTURE_SIZE, [255, 224, 0, 255]),
        leaf_albedo: manuka_leaf_albedo(seed),
        leaf_metallic_roughness: manuka_leaf_roughness(seed),
    })
}

fn manuka_graph(recipe: BotanicalRecipe, rng: &mut Rng) -> Result<AxisGraph, String> {
    let mut axes = Vec::with_capacity(
        1 + usize::from(recipe.primary_count)
            * (1 + usize::from(recipe.secondaries_per_primary)
                * (1 + usize::from(recipe.terminals_per_secondary))),
    );
    axes.push(basal_axis(recipe, rng));

    let stem_count = usize::from(recipe.primary_count);
    let crown_lean_phase = rng.range(0.0, TAU);
    let crown_lean = Vec3::new(crown_lean_phase.cos(), crown_lean_phase.sin(), 0.0);
    let leader_phase = rng.range(0.0, TAU);
    let leader_base = axes[0].sample(0.16).0;
    let leader = manuka_leader_axis(recipe, leader_base, crown_lean, leader_phase, rng);
    let leader_id = push_axis(&mut axes, leader)?;

    for stem_index in 1..stem_count {
        let crown_rank = (stem_index - 1) as f32 / stem_count.saturating_sub(2).max(1) as f32;
        let stem_phase = stem_index as f32 * 2.399_963_1 + rng.range(-0.26, 0.26);
        let radial = Vec3::new(stem_phase.cos(), stem_phase.sin(), 0.0);
        let attachment = (0.36 + crown_rank * 0.32 + rng.range(-0.030, 0.030)).clamp(0.32, 0.72);
        let (base, _, leader_radius) = axes[leader_id as usize].sample(attachment);
        let wind_shaping = radial.dot(crown_lean).mul_add(0.22, 1.0);
        let reach = recipe.trunk_height_metres
            * rng.range(0.12, 0.22)
            * wind_shaping
            * recipe.crown_spread_scale();
        let target_height = recipe.trunk_height_metres
            * (0.74 + rng.range(0.02, 0.14)
                - crown_rank * rng.range(0.0, 0.025)
                - (recipe.branch_droop_scale() - 1.0) * 0.08);
        let rise = (target_height - base.z).max(recipe.trunk_height_metres * 0.16);
        let tip = base
            + radial * reach
            + crown_lean * recipe.trunk_height_metres * rng.range(0.035, 0.075)
            + Vec3::Z * rise;
        let stem = curved_axis(
            Some(leader_id),
            1,
            base,
            tip,
            radial * reach * rng.range(0.16, 0.34)
                + crown_lean * recipe.trunk_height_metres * 0.025,
            leader_radius * rng.range(0.42, 0.62),
            recipe.trunk_radius_metres * rng.range(0.025, 0.045),
            rng.range(0.0, TAU),
        );
        push_axis(&mut axes, stem)?;
    }

    append_manuka_sprays(&mut axes, recipe, crown_lean_phase, crown_lean, rng)?;
    Ok(AxisGraph { axes })
}

fn manuka_leader_axis(
    recipe: BotanicalRecipe,
    base: Vec3,
    crown_lean: Vec3,
    phase: f32,
    rng: &mut Rng,
) -> Axis {
    const GNARL: [f32; AXIS_POINTS] = [0.0, 0.018, -0.012, 0.031, 0.047];
    const TAPER: [f32; AXIS_POINTS] = [0.82, 0.70, 0.55, 0.32, 0.050];
    let height = recipe.trunk_height_metres;
    let side = Vec3::new(-crown_lean.y, crown_lean.x, 0.0);
    let gnarl_scale = rng.range(0.82, 1.18) * recipe.trunk_character_scale();
    Axis {
        parent: Some(0),
        order: 1,
        points_metres: std::array::from_fn(|index| {
            let t = index as f32 / (AXIS_POINTS - 1) as f32;
            base + Vec3::Z * height * t
                + crown_lean * height * (0.018 * t + 0.035 * t * t)
                + side * height * GNARL[index] * gnarl_scale
        }),
        radii_metres: std::array::from_fn(|index| {
            let irregularity = (phase + index as f32 * 1.73).sin() * 0.055 + 1.0;
            recipe.trunk_radius_metres * TAPER[index] * irregularity
        }),
        exposure: 0.65,
        alive: true,
    }
}

fn append_manuka_sprays(
    axes: &mut Vec<Axis>,
    recipe: BotanicalRecipe,
    crown_lean_phase: f32,
    crown_lean: Vec3,
    rng: &mut Rng,
) -> Result<(), String> {
    let stem_count = usize::from(recipe.primary_count);
    for scaffold_index in 0..stem_count {
        let stem_id =
            u32::try_from(scaffold_index + 1).map_err(|_| "mānuka scaffold axis exceeds u32")?;
        let stem_phase = scaffold_index as f32 * 2.399_963_1 + crown_lean_phase * 0.22;
        for lateral_index in 0..usize::from(recipe.secondaries_per_primary) {
            let attachment_start = if scaffold_index == 0 { 0.50 } else { 0.28 };
            let attachment_span = if scaffold_index == 0 { 0.30 } else { 0.64 };
            let attachment = attachment_start
                + lateral_index as f32
                    / usize::from(recipe.secondaries_per_primary)
                        .saturating_sub(1)
                        .max(1) as f32
                    * attachment_span
                + rng.range(-0.035, 0.035);
            let (base, stem_direction, stem_radius) = axes[stem_id as usize].sample(attachment);
            let phase = stem_phase + lateral_index as f32 * 2.399_963_1 + rng.range(-0.34, 0.34);
            let outward = Vec3::new(phase.cos(), phase.sin(), 0.0);
            let length = recipe.trunk_height_metres * rng.range(0.09, 0.16);
            let direction = (outward * rng.range(0.70, 0.92) * recipe.crown_spread_scale()
                + Vec3::Z * (rng.range(-0.05, 0.15) - (recipe.branch_droop_scale() - 1.0) * 0.16)
                + stem_direction * 0.16)
                .normalize_or(Vec3::Z);
            let lateral = curved_axis(
                Some(stem_id),
                2,
                base,
                base + direction * length,
                crown_lean * length * 0.08 + Vec3::Z * length * 0.06,
                stem_radius * rng.range(0.34, 0.50),
                stem_radius * rng.range(0.075, 0.13),
                rng.range(0.0, TAU),
            );
            let lateral_id = push_axis(axes, lateral)?;

            for terminal_index in 0..usize::from(recipe.terminals_per_secondary) {
                let fraction = 0.38
                    + terminal_index as f32
                        / usize::from(recipe.terminals_per_secondary)
                            .saturating_sub(1)
                            .max(1) as f32
                        * 0.52
                    + rng.range(-0.025, 0.025);
                let (base, lateral_direction, radius) = axes[lateral_id as usize].sample(fraction);
                let alternating = if terminal_index.is_multiple_of(2) {
                    1.0
                } else {
                    -1.0
                };
                let side = lateral_direction.cross(Vec3::Z).normalize_or(outward) * alternating;
                let terminal_length = recipe.trunk_height_metres * rng.range(0.055, 0.090);
                let terminal_direction = (lateral_direction * rng.range(0.52, 0.76)
                    + side * rng.range(0.42, 0.70)
                    + Vec3::Z
                        * (rng.range(-0.05, 0.18) - (recipe.branch_droop_scale() - 1.0) * 0.20))
                    .normalize_or(lateral_direction);
                push_axis(
                    axes,
                    curved_axis(
                        Some(lateral_id),
                        3,
                        base,
                        base + terminal_direction * terminal_length,
                        side * terminal_length * rng.range(-0.10, 0.10),
                        radius * rng.range(0.30, 0.44),
                        (radius * 0.08).max(0.001_8),
                        rng.range(0.0, TAU),
                    ),
                )?;
            }
        }
    }
    Ok(())
}

fn basal_axis(recipe: BotanicalRecipe, rng: &mut Rng) -> Axis {
    let phase = rng.range(0.0, TAU);
    let drift = Vec3::new(phase.cos(), phase.sin(), 0.0)
        * recipe.trunk_radius_metres
        * 0.22
        * recipe.trunk_character_scale();
    Axis {
        parent: None,
        order: 0,
        points_metres: std::array::from_fn(|index| {
            let t = index as f32 / (AXIS_POINTS - 1) as f32;
            drift * t * t + Vec3::Z * recipe.trunk_height_metres.min(0.42) * t
        }),
        radii_metres: std::array::from_fn(|index| {
            let t = index as f32 / (AXIS_POINTS - 1) as f32;
            recipe.trunk_radius_metres * (1.15 - t * 0.46)
        }),
        exposure: 0.42,
        alive: true,
    }
}

#[allow(clippy::too_many_arguments)]
fn curved_axis(
    parent: Option<u32>,
    order: u8,
    base: Vec3,
    tip: Vec3,
    bow: Vec3,
    base_radius: f32,
    tip_radius: f32,
    phase: f32,
) -> Axis {
    let chord = tip - base;
    let cross = chord.cross(Vec3::Z).normalize_or(Vec3::X);
    Axis {
        parent,
        order,
        points_metres: std::array::from_fn(|index| {
            let t = index as f32 / (AXIS_POINTS - 1) as f32;
            let envelope = (t * PI).sin();
            base.lerp(tip, t)
                + bow * envelope
                + cross * (phase + t * PI * 1.7).sin() * chord.length() * 0.012 * envelope
        }),
        radii_metres: std::array::from_fn(|index| {
            let t = index as f32 / (AXIS_POINTS - 1) as f32;
            base_radius + (tip_radius - base_radius) * t.powf(0.82)
        }),
        exposure: (0.52 + f32::from(order) * 0.13).clamp(0.0, 1.0),
        alive: true,
    }
}

fn push_axis(graph_axes: &mut Vec<Axis>, new_axis: Axis) -> Result<u32, String> {
    let index = u32::try_from(graph_axes.len()).map_err(|_| "mānuka axis graph exceeds u32")?;
    graph_axes.push(new_axis);
    Ok(index)
}

fn manuka_foliage(
    recipe: BotanicalRecipe,
    graph: &AxisGraph,
    rng: &mut Rng,
) -> Result<Vec<LeafOrgan>, String> {
    let terminal_count = graph.axes.iter().filter(|axis| axis.order == 3).count();
    let mut leaves =
        Vec::with_capacity(terminal_count * (usize::from(recipe.leaves_per_terminal) + 2));
    for (axis_index, axis) in graph.axes.iter().enumerate() {
        if axis.order != 3 || !axis.alive {
            continue;
        }
        let axis_id = u32::try_from(axis_index).map_err(|_| "mānuka leaf axis exceeds u32")?;
        for leaf_index in 0..usize::from(recipe.leaves_per_terminal) {
            let fraction = 0.56
                + leaf_index as f32
                    / usize::from(recipe.leaves_per_terminal)
                        .saturating_sub(1)
                        .max(1) as f32
                    * 0.40;
            let (base, tangent, _) = axis.sample(fraction);
            let phase = leaf_index as f32 * 2.399_963_1 + rng.range(-0.22, 0.22);
            let first = tangent.cross(Vec3::Z).normalize_or(Vec3::X);
            let second = tangent.cross(first).normalize_or(Vec3::Y);
            let radial = (first * phase.cos() + second * phase.sin()).normalize_or(first);
            let direction = tangent;
            let normal = tangent.cross(radial).normalize_or(Vec3::Z);
            let exposure =
                (0.52 + base.z / recipe.trunk_height_metres * 0.44 + normal.z.abs() * 0.08)
                    .clamp(0.0, 1.0);
            leaves.push(LeafOrgan {
                axis: axis_id,
                blade_base_metres: base + radial * rng.range(-0.012, 0.012),
                direction,
                normal,
                length_metres: rng.range(0.18, 0.28),
                width_metres: rng.range(0.18, 0.29),
                archetype: (rng.next_u64() % 6) as u8,
                age: rng.range(0.14, 0.92),
                light_exposure: exposure,
                variation: phase,
            });
        }

        // Solitary flowers sit toward the exposed tip rather than forming a
        // terminal pom-pom. They share the leaf organ transform but use a
        // dedicated five-petal archetype and white atlas tile.
        let flower_count = 2 + (rng.next_u64() % 3) as usize;
        for flower_index in 0..flower_count {
            let fraction = 0.62
                + flower_index as f32 / flower_count.saturating_sub(1).max(1) as f32 * 0.40
                + rng.range(-0.035, 0.035);
            let (base, tangent, _) = axis.sample(fraction.clamp(0.0, 0.98));
            let first = tangent.cross(Vec3::Z).normalize_or(Vec3::X);
            let phase = rng.range(0.0, TAU);
            let second = tangent.cross(first).normalize_or(Vec3::Y);
            let facing = (first * phase.cos() + second * phase.sin()).normalize_or(first);
            leaves.push(LeafOrgan {
                axis: axis_id,
                blade_base_metres: base,
                direction: tangent,
                normal: facing,
                length_metres: rng.range(0.040, 0.058),
                width_metres: rng.range(0.040, 0.058),
                archetype: 6 + flower_index.min(1) as u8,
                age: 0.18,
                light_exposure: 0.92,
                variation: phase,
            });
        }
    }
    Ok(leaves)
}

fn manuka_archetypes() -> [Mesh; LEAF_ARCHETYPE_COUNT] {
    std::array::from_fn(|index| {
        if index >= 6 {
            manuka_flower_mesh(index == 7)
        } else {
            manuka_leaf_mesh(index as u8 % 3, index)
        }
    })
}

fn manuka_leaf_mesh(tile: u8, variant: usize) -> Mesh {
    let mut mesh = Mesh::default();
    let phase = variant as f32 * 0.37;
    for leaf in 0..7 {
        let attachment = 0.06 + leaf as f32 * 0.135;
        let side = if (leaf + variant).is_multiple_of(2) {
            1.0
        } else {
            -1.0
        };
        let reach = 0.15 + (leaf as f32 * 1.91 + phase).sin().abs() * 0.035;
        let base = Vec2::new(attachment, side * 0.035);
        let tip = base + Vec2::new(reach * 0.30, side * reach);
        append_leaf_polygon(&mut mesh, tile, base, tip, 0.027, side * 0.012);
    }
    mesh.calculate_normals();
    mesh
}

fn append_leaf_polygon(
    mesh: &mut Mesh,
    tile: u8,
    base: Vec2,
    tip: Vec2,
    half_width: f32,
    lift: f32,
) {
    let direction = (tip - base).normalize_or(Vec2::X);
    let side = Vec2::new(-direction.y, direction.x);
    let shoulder = base.lerp(tip, 0.44);
    let centre = base.lerp(tip, 0.52);
    let vertices = [
        Vec3::new(base.x, base.y, 0.0),
        Vec3::new(
            shoulder.x + side.x * half_width,
            shoulder.y + side.y * half_width,
            lift * 0.45,
        ),
        Vec3::new(tip.x, tip.y, 0.0),
        Vec3::new(
            shoulder.x - side.x * half_width,
            shoulder.y - side.y * half_width,
            -lift * 0.25,
        ),
        Vec3::new(centre.x, centre.y, lift),
    ];
    let base_index = u32::try_from(mesh.vertices.len()).expect("mānuka leaf spray fits u32");
    mesh.vertices.extend(vertices);
    for local in [
        Vec2::new(0.5, 0.0),
        Vec2::new(0.0, 0.44),
        Vec2::new(0.5, 1.0),
        Vec2::new(1.0, 0.44),
        Vec2::new(0.5, 0.52),
    ] {
        mesh.uv.push(atlas_uv(tile, local));
    }
    mesh.triangles.extend([
        base_index,
        base_index + 1,
        base_index + 4,
        base_index + 1,
        base_index + 2,
        base_index + 4,
        base_index + 2,
        base_index + 3,
        base_index + 4,
        base_index + 3,
        base_index,
        base_index + 4,
    ]);
}

fn manuka_flower_mesh(rotated: bool) -> Mesh {
    const PETALS: usize = 5;
    let mut mesh = Mesh::default();
    let phase_offset = if rotated { 0.22 } else { 0.0 };
    for petal in 0..PETALS {
        let phase = petal as f32 / PETALS as f32 * TAU + phase_offset;
        let radial = Vec2::new(phase.cos(), phase.sin());
        let side = Vec2::new(-radial.y, radial.x);
        let base = u32::try_from(mesh.vertices.len()).expect("mānuka flower fits u32");
        for point in [
            Vec2::ZERO,
            radial * 0.34 + side * 0.24,
            radial * 0.52,
            radial * 0.34 - side * 0.24,
        ] {
            mesh.vertices
                .push(Vec3::new(point.x, point.y, 0.035 * point.length_squared()));
            mesh.uv
                .push(atlas_uv(3, Vec2::new(point.x + 0.5, point.y + 0.5)));
        }
        mesh.triangles
            .extend([base, base + 1, base + 2, base, base + 2, base + 3]);
    }
    mesh.calculate_normals();
    mesh
}

fn atlas_uv(tile: u8, local: Vec2) -> Vec2 {
    let tile = u32::from(tile).min(3);
    let column = tile % ATLAS_COLUMNS;
    let row = tile / ATLAS_COLUMNS;
    let scale = 1.0 / ATLAS_COLUMNS as f32;
    let inset = 1.0 / ATLAS_SIZE as f32;
    Vec2::new(
        column as f32 * scale + inset + local.x.clamp(0.0, 1.0) * (scale - inset * 2.0),
        row as f32 * scale + inset + local.y.clamp(0.0, 1.0) * (scale - inset * 2.0),
    )
}

fn manuka_bark_albedo(seed: u64) -> BotanicalTexture {
    texture(TEXTURE_SIZE, BARK_HEIGHT, |x, y| {
        let height = manuka_bark_height(seed, x.cast_signed(), y.cast_signed());
        let fibre =
            ((x as f32 * 0.15 + (y as f32 * 0.025).sin() * 2.3).sin() * 0.5 + 0.5).powf(5.0);
        let noise = hash_unit(seed ^ 0xb411, x, y) - 0.5;
        let base = Vec3::new(0.31, 0.22, 0.14)
            + Vec3::new(0.24, 0.20, 0.15) * height
            + Vec3::splat(noise * 0.045)
            + Vec3::new(0.11, 0.09, 0.065) * fibre;
        encode_colour(base)
    })
}

fn manuka_bark_height(seed: u64, x: i32, y: i32) -> f32 {
    let x = x.rem_euclid(TEXTURE_SIZE.cast_signed()).cast_unsigned();
    let y = y.rem_euclid(BARK_HEIGHT.cast_signed()).cast_unsigned();
    let broad = value_noise(seed ^ 0x91a7, x, y, 23);
    let papery =
        ((y as f32 * 0.11 + broad * 5.5 + (x as f32 * 0.035).sin()).sin() * 0.5 + 0.5).powf(7.0);
    (broad * 0.54 + papery * 0.46).clamp(0.0, 1.0)
}

fn manuka_bark_normal(seed: u64) -> BotanicalTexture {
    texture(TEXTURE_SIZE, BARK_HEIGHT, |x, y| {
        let left = manuka_bark_height(seed, x.cast_signed() - 1, y.cast_signed());
        let right = manuka_bark_height(seed, x.cast_signed() + 1, y.cast_signed());
        let down = manuka_bark_height(seed, x.cast_signed(), y.cast_signed() - 1);
        let up = manuka_bark_height(seed, x.cast_signed(), y.cast_signed() + 1);
        let normal = Vec3::new((left - right) * 2.3, (down - up) * 1.6, 1.0).normalize_or(Vec3::Z);
        encode_normal(normal)
    })
}

fn manuka_bark_depth(seed: u64) -> BotanicalTexture {
    texture(TEXTURE_SIZE, BARK_HEIGHT, |x, y| {
        let value = (manuka_bark_height(seed, x.cast_signed(), y.cast_signed()) * 255.0) as u8;
        [value, value, value, 255]
    })
}

fn manuka_leaf_albedo(seed: u64) -> BotanicalTexture {
    texture(ATLAS_SIZE, ATLAS_SIZE, |x, y| {
        let tile_x = x / TILE_SIZE;
        let tile_y = y / TILE_SIZE;
        let tile = tile_y * ATLAS_COLUMNS + tile_x;
        let u = (x % TILE_SIZE) as f32 / (TILE_SIZE - 1) as f32;
        let v = (y % TILE_SIZE) as f32 / (TILE_SIZE - 1) as f32;
        if tile == 3 {
            let radius = Vec2::new(u - 0.5, v - 0.5).length();
            let centre = (1.0 - (radius / 0.18).clamp(0.0, 1.0)).powf(2.0);
            return encode_colour(
                Vec3::new(0.91, 0.88, 0.82).lerp(Vec3::new(0.42, 0.075, 0.055), centre),
            );
        }
        let base = match tile {
            1 => Vec3::new(0.12, 0.27, 0.13),
            2 => Vec3::new(0.21, 0.30, 0.13),
            _ => Vec3::new(0.08, 0.23, 0.11),
        };
        let vein = (1.0 - (u - 0.5).abs() * 26.0).max(0.0);
        let noise = value_noise(seed ^ u64::from(tile), x % TILE_SIZE, y % TILE_SIZE, 13) - 0.5;
        encode_colour(base + Vec3::splat(noise * 0.035) + Vec3::new(0.06, 0.08, 0.035) * vein)
    })
}

fn manuka_leaf_roughness(seed: u64) -> BotanicalTexture {
    texture(ATLAS_SIZE, ATLAS_SIZE, |x, y| {
        let tile = y / TILE_SIZE * ATLAS_COLUMNS + x / TILE_SIZE;
        let noise = value_noise(seed ^ 0x4a11, x % TILE_SIZE, y % TILE_SIZE, 11);
        let base = if tile == 3 { 0.58 } else { 0.68 };
        [
            255,
            ((base + noise * 0.12).clamp(0.0, 1.0) * 255.0) as u8,
            0,
            255,
        ]
    })
}

fn solid_texture(size: u32, rgba: [u8; 4]) -> BotanicalTexture {
    BotanicalTexture {
        width: size,
        height: size,
        rgba: rgba.repeat((size * size) as usize),
    }
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

fn value_noise(seed: u64, x: u32, y: u32, cell: u32) -> f32 {
    let x0 = x / cell;
    let y0 = y / cell;
    let tx = smoothstep((x % cell) as f32 / cell as f32);
    let ty = smoothstep((y % cell) as f32 / cell as f32);
    let lower_left = hash_unit(seed, x0, y0);
    let lower_right = hash_unit(seed, x0 + 1, y0);
    let upper_left = hash_unit(seed, x0, y0 + 1);
    let upper_right = hash_unit(seed, x0 + 1, y0 + 1);
    let lower = lower_left + (lower_right - lower_left) * tx;
    let upper = upper_left + (upper_right - upper_left) * tx;
    lower + (upper - lower) * ty
}

fn hash_unit(seed: u64, x: u32, y: u32) -> f32 {
    let mut value =
        seed ^ u64::from(x).wrapping_mul(0x9e37_79b9) ^ u64::from(y).wrapping_mul(0x85eb_ca6b);
    value ^= value >> 30;
    value = value.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value ^= value >> 27;
    (value >> 40) as f32 / 16_777_216.0
}

fn encode_colour(colour: Vec3) -> [u8; 4] {
    let colour = colour.clamp(Vec3::ZERO, Vec3::ONE) * 255.0;
    [colour.x as u8, colour.y as u8, colour.z as u8, 255]
}

fn encode_normal(normal: Vec3) -> [u8; 4] {
    let encoded = (normal * 0.5 + Vec3::splat(0.5)) * 255.0;
    [encoded.x as u8, encoded.y as u8, encoded.z as u8, 255]
}

fn smoothstep(value: f32) -> f32 {
    value * value * (3.0 - 2.0 * value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{BotanicalSpecies, generate_botanical_prototype};

    #[test]
    fn manuka_is_deterministic_single_trunked_and_flowering() {
        let recipe = BotanicalRecipe::for_species(BotanicalSpecies::Manuka);
        let first = generate_botanical_prototype(42, recipe).expect("mānuka prototype");
        let second = generate_botanical_prototype(42, recipe).expect("mānuka prototype");

        assert_eq!(first, second);
        assert_eq!(first.species, BotanicalSpecies::Manuka);
        let crown_scaffolds: Vec<_> = first
            .graph
            .axes
            .iter()
            .filter(|axis| axis.order == 1)
            .collect();
        assert_eq!(crown_scaffolds.len(), usize::from(recipe.primary_count));
        assert!(first.leaves.iter().any(|leaf| leaf.archetype >= 6));
        let leader = crown_scaffolds[0];
        assert_eq!(leader.parent, Some(0));
        assert!(leader.points_metres[0].z < recipe.trunk_height_metres * 0.10);
        assert!(leader.radii_metres[0] > recipe.trunk_radius_metres * 0.70);
        let leader_chord = leader.points_metres[AXIS_POINTS - 1] - leader.points_metres[0];
        let leader_direction = leader_chord.normalize_or(Vec3::Z);
        let maximum_gnarl = leader
            .points_metres
            .iter()
            .skip(1)
            .take(AXIS_POINTS - 2)
            .map(|point| {
                let offset = *point - leader.points_metres[0];
                (offset - leader_direction * offset.dot(leader_direction)).length()
            })
            .fold(0.0_f32, f32::max);
        assert!(maximum_gnarl > recipe.trunk_height_metres * 0.015);
        assert!(crown_scaffolds.iter().skip(1).all(|axis| {
            axis.parent == Some(1)
                && axis.points_metres[0].z > recipe.trunk_height_metres * 0.29
                && axis.radii_metres[0] < recipe.trunk_radius_metres * 0.55
        }));
        assert!(
            leader.points_metres[AXIS_POINTS - 1].z
                > first
                    .graph
                    .axes
                    .iter()
                    .filter(|axis| axis.order == 1)
                    .skip(1)
                    .map(|axis| axis.points_metres[AXIS_POINTS - 1].z)
                    .fold(f32::NEG_INFINITY, f32::max)
        );
        let average_foliage_height = first
            .leaves
            .iter()
            .map(|leaf| leaf.blade_base_metres.z)
            .sum::<f32>()
            / first.leaves.len() as f32;
        assert!(average_foliage_height > recipe.trunk_height_metres * 0.60);
        assert!(
            first.leaf_archetypes[..6]
                .iter()
                .all(|mesh| mesh.vertices.len() >= 35)
        );
    }
}
