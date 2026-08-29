//! Species-specific mature kauri architecture.
//!
//! A mature kauri is defined by its clean monumental bole, high self-pruned
//! crown, heavy whorled scaffold limbs and spreading surface roots. This growth
//! program keeps that hierarchy explicit instead of stretching a generic tree.

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

const KAURI_SEED_DOMAIN: u64 = 0x6b61_7572_695f_3031;
const TEXTURE_SIZE: u32 = 256;
const BARK_HEIGHT: u32 = 512;
const ATLAS_COLUMNS: u32 = 2;
const ATLAS_SIZE: u32 = 256;
const TILE_SIZE: u32 = ATLAS_SIZE / ATLAS_COLUMNS;

pub(super) fn generate_kauri_prototype(
    seed: u64,
    recipe: BotanicalRecipe,
) -> Result<BotanicalPrototype, String> {
    let mut rng = Rng::new(seed ^ KAURI_SEED_DOMAIN);
    let graph = kauri_graph(recipe, &mut rng)?;
    let leaves = kauri_leaves(recipe, &graph, &mut rng)?;
    let foliage_pads = generate_foliage_pads(&graph, &leaves);
    let (wood, wood_bark, wood_scars) = generate_wood(seed ^ KAURI_SEED_DOMAIN, &graph)?;

    Ok(BotanicalPrototype {
        species: recipe.species,
        graph,
        wood,
        wood_bark,
        wood_scars,
        wood_scar_albedo: solid_texture(64, [124, 104, 78, 255]),
        microtwigs: Mesh::default(),
        microtwig_bark: Vec::new(),
        leaf_archetypes: kauri_leaf_archetypes(),
        shoot_tip_archetypes: std::array::from_fn(|_| Mesh::default()),
        reproductive_archetypes: std::array::from_fn(|_| Mesh::default()),
        foliage_pad_archetypes: foliage_pad_archetypes(),
        leaves,
        shoot_tips: Vec::new(),
        reproductive_organs: Vec::new(),
        foliage_pads,
        bark_albedo: kauri_bark_albedo(seed),
        bark_normal: kauri_bark_normal(seed),
        bark_depth: kauri_bark_depth(seed),
        bark_metallic_roughness: solid_texture(TEXTURE_SIZE, [255, 240, 0, 255]),
        leaf_albedo: kauri_leaf_albedo(seed),
        leaf_metallic_roughness: kauri_leaf_roughness(seed),
    })
}

#[allow(clippy::too_many_lines)]
fn kauri_graph(recipe: BotanicalRecipe, rng: &mut Rng) -> Result<AxisGraph, String> {
    let mut axes = Vec::with_capacity(
        9 + usize::from(recipe.primary_count)
            * (1 + usize::from(recipe.secondaries_per_primary)
                * (1 + usize::from(recipe.terminals_per_secondary))),
    );
    axes.push(kauri_trunk(recipe, rng));
    append_surface_roots(&mut axes, recipe, rng)?;

    let primary_count = usize::from(recipe.primary_count);
    let whorl_count = primary_count.div_ceil(4);
    let crown_bias_phase = rng.range(0.0, TAU);
    let crown_bias = Vec3::new(crown_bias_phase.cos(), crown_bias_phase.sin(), 0.0);
    for primary_index in 0..primary_count {
        let whorl = primary_index / 4;
        let in_whorl = primary_index % 4;
        let level = whorl as f32 / whorl_count.saturating_sub(1).max(1) as f32;
        let attachment = (0.66 + level * 0.27 + rng.range(-0.012, 0.012)).clamp(0.62, 0.94);
        let (base, _, trunk_radius) = axes[0].sample(attachment);
        let phase = in_whorl as f32 / 4.0 * TAU + whorl as f32 * 0.63 + rng.range(-0.16, 0.16);
        let radial = Vec3::new(phase.cos(), phase.sin(), 0.0);
        let length = recipe.trunk_height_metres
            * (0.32 - level * 0.06)
            * rng.range(0.90, 1.09)
            * recipe.crown_spread_scale();
        let lift = (0.16 + level * 0.26 + rng.range(-0.035, 0.055)
            - (recipe.branch_droop_scale() - 1.0) * 0.12)
            .clamp(-0.08, 0.58);
        let direction =
            (radial * (1.0 - lift) + Vec3::Z * lift + crown_bias * 0.08).normalize_or(radial);
        let branch = curved_axis(
            Some(0),
            1,
            base,
            base + direction * length,
            Vec3::Z * length * (0.10 + level * 0.08 - (recipe.branch_droop_scale() - 1.0) * 0.14)
                + crown_bias * length * 0.04,
            trunk_radius * rng.range(0.30, 0.43),
            trunk_radius * rng.range(0.055, 0.085),
            rng.range(0.0, TAU),
        );
        let branch_id = push_axis(&mut axes, branch)?;

        for secondary_index in 0..usize::from(recipe.secondaries_per_primary) {
            let fraction = 0.30
                + secondary_index as f32
                    / usize::from(recipe.secondaries_per_primary)
                        .saturating_sub(1)
                        .max(1) as f32
                    * 0.64
                + rng.range(-0.025, 0.025);
            let (base, branch_direction, radius) = axes[branch_id as usize].sample(fraction);
            let alternate = if secondary_index.is_multiple_of(2) {
                1.0
            } else {
                -1.0
            };
            let lateral = branch_direction
                .cross(Vec3::Z)
                .normalize_or(Vec3::new(-radial.y, radial.x, 0.0))
                * alternate;
            let secondary_length = length * rng.range(0.32, 0.48);
            let secondary_direction = (branch_direction * rng.range(0.48, 0.64)
                + lateral * rng.range(0.56, 0.76)
                + Vec3::Z * rng.range(0.20, 0.34))
            .normalize_or(branch_direction);
            let secondary = curved_axis(
                Some(branch_id),
                2,
                base,
                base + secondary_direction * secondary_length,
                Vec3::Z * secondary_length * 0.10,
                radius * rng.range(0.42, 0.58),
                radius * rng.range(0.08, 0.13),
                rng.range(0.0, TAU),
            );
            let secondary_id = push_axis(&mut axes, secondary)?;

            for terminal_index in 0..usize::from(recipe.terminals_per_secondary) {
                let fraction = 0.42
                    + terminal_index as f32
                        / usize::from(recipe.terminals_per_secondary)
                            .saturating_sub(1)
                            .max(1) as f32
                        * 0.54
                    + rng.range(-0.025, 0.025);
                let (base, secondary_direction, radius) =
                    axes[secondary_id as usize].sample(fraction);
                let spin = terminal_index as f32 * 2.399_963_1 + rng.range(-0.24, 0.24);
                let first = secondary_direction.cross(Vec3::Z).normalize_or(lateral);
                let second = secondary_direction.cross(first).normalize_or(Vec3::Z);
                let spread = first * spin.cos() + second * spin.sin();
                let terminal_length = recipe.trunk_height_metres * rng.range(0.055, 0.085);
                let terminal_direction =
                    (secondary_direction * 0.64 + spread * 0.46 + Vec3::Z * 0.22)
                        .normalize_or(secondary_direction);
                push_axis(
                    &mut axes,
                    curved_axis(
                        Some(secondary_id),
                        3,
                        base,
                        base + terminal_direction * terminal_length,
                        spread * terminal_length * rng.range(-0.06, 0.08),
                        radius * rng.range(0.34, 0.48),
                        (radius * 0.07).max(0.003),
                        rng.range(0.0, TAU),
                    ),
                )?;
            }
        }
    }
    Ok(AxisGraph { axes })
}

fn kauri_trunk(recipe: BotanicalRecipe, rng: &mut Rng) -> Axis {
    let phase = rng.range(0.0, TAU);
    let lean = Vec3::new(phase.cos(), phase.sin(), 0.0)
        * recipe.trunk_height_metres
        * 0.012
        * recipe.trunk_character_scale();
    Axis {
        parent: None,
        order: 0,
        points_metres: std::array::from_fn(|index| {
            let t = index as f32 / (AXIS_POINTS - 1) as f32;
            lean * t.powf(1.85)
                + Vec3::new(-lean.y, lean.x, 0.0) * (t * PI).sin() * 0.06
                + Vec3::Z * recipe.trunk_height_metres * t
        }),
        radii_metres: std::array::from_fn(|index| {
            let t = index as f32 / (AXIS_POINTS - 1) as f32;
            recipe.trunk_radius_metres * (1.0 - t * 0.46).max(0.46)
        }),
        exposure: 0.88,
        alive: true,
    }
}

fn append_surface_roots(
    axes: &mut Vec<Axis>,
    recipe: BotanicalRecipe,
    rng: &mut Rng,
) -> Result<(), String> {
    let count = 10;
    for index in 0..count {
        let phase = index as f32 / count as f32 * TAU + rng.range(-0.15, 0.15);
        let radial = Vec3::new(phase.cos(), phase.sin(), 0.0);
        let character = recipe.trunk_character_scale();
        let length = recipe.trunk_radius_metres * rng.range(1.8, 2.8) * character;
        let base = radial * recipe.trunk_radius_metres * 0.52 + Vec3::Z * 0.22;
        let tip = radial * length + Vec3::Z * rng.range(-0.10, 0.035);
        push_axis(
            axes,
            curved_axis(
                Some(0),
                1,
                base,
                tip,
                Vec3::Z * recipe.trunk_radius_metres * rng.range(0.12, 0.28),
                recipe.trunk_radius_metres * rng.range(0.22, 0.34) * character,
                recipe.trunk_radius_metres * rng.range(0.025, 0.055),
                phase,
            ),
        )?;
    }
    Ok(())
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
                + cross * (phase + t * PI * 1.35).sin() * chord.length() * 0.010 * envelope
        }),
        radii_metres: std::array::from_fn(|index| {
            let t = index as f32 / (AXIS_POINTS - 1) as f32;
            base_radius + (tip_radius - base_radius) * t.powf(0.84)
        }),
        exposure: (0.48 + f32::from(order) * 0.15).clamp(0.0, 1.0),
        alive: true,
    }
}

fn push_axis(graph_axes: &mut Vec<Axis>, new_axis: Axis) -> Result<u32, String> {
    let index = u32::try_from(graph_axes.len()).map_err(|_| "kauri axis graph exceeds u32")?;
    graph_axes.push(new_axis);
    Ok(index)
}

fn kauri_leaves(
    recipe: BotanicalRecipe,
    graph: &AxisGraph,
    rng: &mut Rng,
) -> Result<Vec<LeafOrgan>, String> {
    let terminal_count = graph.axes.iter().filter(|axis| axis.order == 3).count();
    let mut leaves = Vec::with_capacity(terminal_count * usize::from(recipe.leaves_per_terminal));
    for (axis_index, axis) in graph.axes.iter().enumerate() {
        if axis.order != 3 || !axis.alive {
            continue;
        }
        let axis_id = u32::try_from(axis_index).map_err(|_| "kauri leaf axis exceeds u32")?;
        for leaf_index in 0..usize::from(recipe.leaves_per_terminal) {
            let rank = leaf_index as f32
                / usize::from(recipe.leaves_per_terminal)
                    .saturating_sub(1)
                    .max(1) as f32;
            let fraction =
                (0.72 + rank.powf(0.58) * 0.25 + rng.range(-0.018, 0.018)).clamp(0.68, 0.99);
            let (base, tangent, _) = axis.sample(fraction);
            let phase = leaf_index as f32 * 2.399_963_1 + rng.range(-0.16, 0.16);
            let first = tangent.cross(Vec3::Z).normalize_or(Vec3::X);
            let second = tangent.cross(first).normalize_or(Vec3::Y);
            let radial = (first * phase.cos() + second * phase.sin()).normalize_or(first);
            // Mature kauri carry their stiff leaves on dense distal twig
            // systems. A handful of bounded tufts form each discrete terminal
            // foliage mass instead of coating the full branch like a feather.
            let direction = (tangent * 0.58 + radial * 0.72).normalize_or(tangent);
            let normal = direction.cross(tangent).normalize_or(second);
            let age = (leaf_index as f32 / usize::from(recipe.leaves_per_terminal).max(1) as f32
                + rng.range(0.04, 0.36))
            .clamp(0.0, 1.0);
            leaves.push(LeafOrgan {
                axis: axis_id,
                blade_base_metres: base + radial * rng.range(-0.08, 0.28),
                direction,
                normal,
                length_metres: rng.range(1.05, 1.40),
                width_metres: rng.range(1.05, 1.42),
                archetype: (rng.next_u64() % LEAF_ARCHETYPE_COUNT as u64) as u8,
                age,
                light_exposure: (0.58
                    + base.z / recipe.trunk_height_metres * 0.34
                    + normal.z.abs() * 0.08)
                    .clamp(0.0, 1.0),
                variation: phase,
            });
        }
    }
    Ok(leaves)
}

fn kauri_leaf_archetypes() -> [Mesh; LEAF_ARCHETYPE_COUNT] {
    std::array::from_fn(kauri_leaf_mesh)
}

fn kauri_leaf_mesh(variant: usize) -> Mesh {
    let tile = (variant % 4) as u8;
    let mut mesh = Mesh::default();
    let phase = variant as f32 * 0.055;
    for twiglet in 0..5 {
        let ray_phase = -1.16 + twiglet as f32 * 0.58 + phase;
        let ray = Vec2::new(ray_phase.cos(), ray_phase.sin());
        let ray_side = Vec2::new(-ray.y, ray.x);
        for leaf in 0..8 {
            let side = if (leaf + twiglet + variant).is_multiple_of(2) {
                1.0
            } else {
                -1.0
            };
            let attachment = 0.035 + leaf as f32 * 0.052;
            let base = ray * attachment + ray_side * side * 0.022;
            let tip = base + ray * 0.092 + ray_side * side * (0.060 + leaf as f32 * 0.003);
            append_leaf_polygon(&mut mesh, tile, base, tip, 0.040, side * 0.016);
        }
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
    let shoulder = base.lerp(tip, 0.48);
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
            -lift * 0.22,
        ),
        Vec3::new(centre.x, centre.y, lift),
    ];
    let base_index = u32::try_from(mesh.vertices.len()).expect("kauri leaf spray fits u32");
    mesh.vertices.extend(vertices);
    for local in [
        Vec2::new(0.5, 0.0),
        Vec2::new(0.0, 0.48),
        Vec2::new(0.5, 1.0),
        Vec2::new(1.0, 0.48),
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

fn kauri_bark_albedo(seed: u64) -> BotanicalTexture {
    texture(TEXTURE_SIZE, BARK_HEIGHT, |x, y| {
        let height = kauri_bark_height(seed, x.cast_signed(), y.cast_signed());
        let fleck = hash_unit(seed ^ 0x7ca1, x, y) - 0.5;
        let warm = value_noise(seed ^ 0x3917, x, y, 47);
        let base = Vec3::new(0.29, 0.28, 0.25).lerp(Vec3::new(0.59, 0.55, 0.46), height)
            + Vec3::new(0.075, 0.045, 0.018) * warm
            + Vec3::splat(fleck * 0.052);
        encode_colour(base)
    })
}

fn kauri_bark_height(seed: u64, x: i32, y: i32) -> f32 {
    let x = x.rem_euclid(TEXTURE_SIZE.cast_signed()).cast_unsigned();
    let y = y.rem_euclid(BARK_HEIGHT.cast_signed()).cast_unsigned();
    let broad = value_noise(seed ^ 0xa61a, x, y, 37);
    let medium = value_noise(seed ^ 0xc114, x, y, 11);
    let hammer_x = 0.5 - ((x as f32 / 31.0 + broad * 0.72).fract() - 0.5).abs();
    let hammer_y = 0.5 - ((y as f32 / 43.0 + medium * 0.58).fract() - 0.5).abs();
    let plate_crack = (1.0 - hammer_x.min(hammer_y) * 14.0)
        .clamp(0.0, 1.0)
        .powf(1.65);
    let flake = ((y as f32 * 0.083 + broad * 7.2).sin() * 0.5 + 0.5).powf(5.0);
    (0.18 + broad * 0.55 + medium * 0.22 + flake * 0.18 - plate_crack * 0.42).clamp(0.0, 1.0)
}

fn kauri_bark_normal(seed: u64) -> BotanicalTexture {
    texture(TEXTURE_SIZE, BARK_HEIGHT, |x, y| {
        let left = kauri_bark_height(seed, x.cast_signed() - 1, y.cast_signed());
        let right = kauri_bark_height(seed, x.cast_signed() + 1, y.cast_signed());
        let down = kauri_bark_height(seed, x.cast_signed(), y.cast_signed() - 1);
        let up = kauri_bark_height(seed, x.cast_signed(), y.cast_signed() + 1);
        encode_normal(Vec3::new((left - right) * 4.2, (down - up) * 4.2, 1.0).normalize_or(Vec3::Z))
    })
}

fn kauri_bark_depth(seed: u64) -> BotanicalTexture {
    texture(TEXTURE_SIZE, BARK_HEIGHT, |x, y| {
        let value = (kauri_bark_height(seed, x.cast_signed(), y.cast_signed()) * 255.0) as u8;
        [value, value, value, 255]
    })
}

fn kauri_leaf_albedo(seed: u64) -> BotanicalTexture {
    texture(ATLAS_SIZE, ATLAS_SIZE, |x, y| {
        let tile = y / TILE_SIZE * ATLAS_COLUMNS + x / TILE_SIZE;
        let local_x = x % TILE_SIZE;
        let local_y = y % TILE_SIZE;
        let u = local_x as f32 / (TILE_SIZE - 1) as f32;
        let base = match tile {
            0 => Vec3::new(0.14, 0.31, 0.15),
            1 => Vec3::new(0.19, 0.37, 0.18),
            2 => Vec3::new(0.40, 0.31, 0.12),
            _ => Vec3::new(0.24, 0.34, 0.15),
        };
        let broad = value_noise(seed ^ u64::from(tile), local_x, local_y, 15) - 0.5;
        let midrib = (1.0 - (u - 0.5).abs() * 20.0).max(0.0);
        encode_colour(base + Vec3::splat(broad * 0.035) + Vec3::new(0.07, 0.08, 0.035) * midrib)
    })
}

fn kauri_leaf_roughness(seed: u64) -> BotanicalTexture {
    texture(ATLAS_SIZE, ATLAS_SIZE, |x, y| {
        let tile = y / TILE_SIZE * ATLAS_COLUMNS + x / TILE_SIZE;
        let noise = value_noise(seed ^ 0xd134, x % TILE_SIZE, y % TILE_SIZE, 9);
        let base = if tile == 2 { 0.52 } else { 0.43 };
        [
            255,
            ((base + noise * 0.10).clamp(0.0, 1.0) * 255.0) as u8,
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
    fn kauri_is_deterministic_clean_stemmed_and_emergent() {
        let recipe = BotanicalRecipe::for_species(BotanicalSpecies::Kauri);
        let first = generate_botanical_prototype(42, recipe).expect("kauri prototype");
        let second = generate_botanical_prototype(42, recipe).expect("kauri prototype");

        assert_eq!(first, second);
        assert_eq!(first.species, BotanicalSpecies::Kauri);
        let trunk = first.graph.axes[0];
        assert!(trunk.points_metres[AXIS_POINTS - 1].z >= recipe.trunk_height_metres * 0.99);
        assert!(
            first
                .graph
                .axes
                .iter()
                .filter(|axis| axis.order == 1 && axis.points_metres[0].z < 0.5)
                .count()
                >= 7
        );
        assert!(
            first
                .graph
                .axes
                .iter()
                .filter(|axis| axis.order == 1
                    && axis.points_metres[0].z > recipe.trunk_height_metres * 0.60)
                .count()
                >= usize::from(recipe.primary_count)
        );
        assert!(
            first
                .leaf_archetypes
                .iter()
                .all(|mesh| mesh.vertices.len() >= 200)
        );
        assert!(first.leaves.iter().all(|leaf| {
            let tip = first.graph.axes[leaf.axis as usize].sample(1.0).0;
            leaf.blade_base_metres.distance(tip) < 1.5
        }));
        let (minimum_depth, maximum_depth) = first
            .bark_depth
            .rgba
            .iter()
            .step_by(4)
            .copied()
            .fold((u8::MAX, u8::MIN), |(minimum, maximum), value| {
                (minimum.min(value), maximum.max(value))
            });
        assert!(maximum_depth.saturating_sub(minimum_depth) > 220);
    }
}
