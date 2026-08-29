//! Species-specific mature rimu architecture.
//!
//! Rimu is an emergent podocarp with a fine central leader inside a narrow,
//! almost ground-reaching crown. Tapered horizontal scaffold limbs carry
//! unmistakably pendulous branchlets in overlapping curtains, leaving the
//! trunk mostly hidden. The foliage instances are bounded sprays of tiny
//! appressed leaves, not broad blades, so the near model keeps that outline
//! without allocating one renderer entity per scale leaf.

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

const RIMU_SEED_DOMAIN: u64 = 0x7269_6d75_5f30_3031;
const TEXTURE_WIDTH: u32 = 256;
const BARK_HEIGHT: u32 = 512;
const ATLAS_COLUMNS: u32 = 2;
const ATLAS_SIZE: u32 = 256;
const TILE_SIZE: u32 = ATLAS_SIZE / ATLAS_COLUMNS;
const RIMU_ARMS_PER_TIER: usize = 3;

pub(super) fn generate_rimu_prototype(
    seed: u64,
    recipe: BotanicalRecipe,
) -> Result<BotanicalPrototype, String> {
    let mut rng = Rng::new(seed ^ RIMU_SEED_DOMAIN);
    let graph = rimu_graph(recipe, &mut rng)?;
    let leaves = rimu_foliage(recipe, &graph, &mut rng)?;
    let foliage_pads = generate_foliage_pads(&graph, &leaves);
    let (wood, wood_bark, wood_scars) = generate_wood(seed ^ RIMU_SEED_DOMAIN, &graph)?;

    Ok(BotanicalPrototype {
        species: recipe.species,
        graph,
        wood,
        wood_bark,
        wood_scars,
        wood_scar_albedo: solid_texture(64, [92, 62, 38, 255]),
        microtwigs: Mesh::default(),
        microtwig_bark: Vec::new(),
        leaf_archetypes: rimu_spray_archetypes(),
        shoot_tip_archetypes: std::array::from_fn(|_| Mesh::default()),
        reproductive_archetypes: std::array::from_fn(|_| Mesh::default()),
        foliage_pad_archetypes: foliage_pad_archetypes(),
        leaves,
        shoot_tips: Vec::new(),
        reproductive_organs: Vec::new(),
        foliage_pads,
        bark_albedo: rimu_bark_albedo(seed),
        bark_normal: rimu_bark_normal(seed),
        bark_depth: rimu_bark_depth(seed),
        bark_metallic_roughness: solid_texture(TEXTURE_WIDTH, [255, 224, 0, 255]),
        leaf_albedo: rimu_leaf_albedo(seed),
        leaf_metallic_roughness: rimu_leaf_roughness(seed),
    })
}

fn rimu_graph(recipe: BotanicalRecipe, rng: &mut Rng) -> Result<AxisGraph, String> {
    let primary_count = usize::from(recipe.primary_count);
    let secondary_count = usize::from(recipe.secondaries_per_primary);
    let terminal_count = usize::from(recipe.terminals_per_secondary);
    let mut axes = Vec::with_capacity(
        8 + primary_count * RIMU_ARMS_PER_TIER * (1 + secondary_count * (1 + terminal_count)),
    );
    axes.push(rimu_trunk(recipe, rng));
    append_basal_roots(&mut axes, recipe, rng)?;

    let prevailing_phase = rng.range(0.0, TAU);
    let prevailing = Vec3::new(prevailing_phase.cos(), prevailing_phase.sin(), 0.0);
    for primary_index in 0..primary_count * RIMU_ARMS_PER_TIER {
        append_rimu_crown_arm(&mut axes, recipe, primary_index, prevailing, rng)?;
    }
    Ok(AxisGraph { axes })
}

fn append_rimu_crown_arm(
    axes: &mut Vec<Axis>,
    recipe: BotanicalRecipe,
    primary_index: usize,
    prevailing: Vec3,
    rng: &mut Rng,
) -> Result<(), String> {
    let primary_count = usize::from(recipe.primary_count);
    let tier = primary_index / RIMU_ARMS_PER_TIER;
    let arm = primary_index % RIMU_ARMS_PER_TIER;
    let rank = tier as f32 / primary_count.saturating_sub(1).max(1) as f32;
    let attachment = (0.12 + rank * 0.86 + rng.range(-0.018, 0.018)).clamp(0.10, 0.985);
    let (base, _, trunk_radius) = axes[0].sample(attachment);
    let phase = tier as f32 * 2.399_963_1
        + arm as f32 / RIMU_ARMS_PER_TIER as f32 * TAU
        + rng.range(-0.16, 0.16);
    let radial = Vec3::new(phase.cos(), phase.sin(), 0.0);
    let crown_envelope = (1.0 - rank).powf(0.62);
    let length = recipe.trunk_height_metres
        * (0.070 + crown_envelope * 0.145)
        * rng.range(0.90, 1.10)
        * recipe.crown_spread_scale();
    let lift = rng.range(0.01, 0.10) + rank * 0.20;
    let direction = (radial * 0.92 + Vec3::Z * lift + prevailing * 0.08).normalize_or(radial);
    let primary = curved_axis(
        Some(0),
        1,
        base,
        base + direction * length,
        Vec3::Z * length * (0.07 + rank * 0.04) + prevailing * length * 0.025,
        trunk_radius * rng.range(0.26, 0.39),
        trunk_radius * rng.range(0.045, 0.070),
        rng.range(0.0, TAU),
    );
    let primary_id = push_axis(axes, primary)?;

    let secondary_count = usize::from(recipe.secondaries_per_primary);
    for secondary_index in 0..secondary_count {
        let fraction = 0.25
            + secondary_index as f32 / secondary_count.saturating_sub(1).max(1) as f32 * 0.70
            + rng.range(-0.025, 0.025);
        let (base, primary_direction, radius) = axes[primary_id as usize].sample(fraction);
        let alternate = if secondary_index.is_multiple_of(2) {
            1.0
        } else {
            -1.0
        };
        let lateral = primary_direction
            .cross(Vec3::Z)
            .normalize_or(Vec3::new(-radial.y, radial.x, 0.0))
            * alternate;
        let secondary_length = length * rng.range(0.28, 0.44);
        let droop = recipe.branch_droop_scale();
        let secondary_direction = (primary_direction * rng.range(0.42, 0.60)
            + lateral * rng.range(0.58, 0.78)
            - Vec3::Z * rng.range(0.02, 0.14) * droop)
            .normalize_or(primary_direction);
        let secondary = curved_axis(
            Some(primary_id),
            2,
            base,
            base + secondary_direction * secondary_length,
            -Vec3::Z * secondary_length * rng.range(0.05, 0.16) * droop,
            radius * rng.range(0.38, 0.54),
            radius * rng.range(0.065, 0.11),
            rng.range(0.0, TAU),
        );
        let secondary_id = push_axis(axes, secondary)?;
        append_rimu_terminals(axes, recipe, secondary_id, rng)?;
    }
    Ok(())
}

fn append_rimu_terminals(
    axes: &mut Vec<Axis>,
    recipe: BotanicalRecipe,
    secondary_id: u32,
    rng: &mut Rng,
) -> Result<(), String> {
    let terminal_count = usize::from(recipe.terminals_per_secondary);
    for terminal_index in 0..terminal_count {
        let fraction = 0.28
            + terminal_index as f32 / terminal_count.saturating_sub(1).max(1) as f32 * 0.67
            + rng.range(-0.025, 0.025);
        let (base, secondary_direction, radius) = axes[secondary_id as usize].sample(fraction);
        let lateral = secondary_direction.cross(Vec3::Z).normalize_or(Vec3::X)
            * if terminal_index.is_multiple_of(2) {
                1.0
            } else {
                -1.0
            };
        let terminal_length = recipe.trunk_height_metres * rng.range(0.055, 0.095);
        let outward = (secondary_direction * 0.58 + lateral * rng.range(0.12, 0.32))
            .normalize_or(secondary_direction);
        let droop = recipe.branch_droop_scale();
        let tip = base + outward * terminal_length * rng.range(0.42, 0.62)
            - Vec3::Z * terminal_length * rng.range(0.72, 1.02) * droop;
        push_axis(
            axes,
            curved_axis(
                Some(secondary_id),
                3,
                base,
                tip,
                outward * terminal_length * rng.range(0.10, 0.20)
                    + Vec3::Z * terminal_length * rng.range(0.18, 0.30) * droop,
                radius * rng.range(0.30, 0.44),
                (radius * 0.055).max(0.002_5),
                rng.range(0.0, TAU),
            ),
        )?;
    }
    Ok(())
}

fn rimu_trunk(recipe: BotanicalRecipe, rng: &mut Rng) -> Axis {
    let phase = rng.range(0.0, TAU);
    let lean = Vec3::new(phase.cos(), phase.sin(), 0.0)
        * recipe.trunk_height_metres
        * 0.010
        * recipe.trunk_character_scale();
    Axis {
        parent: None,
        order: 0,
        points_metres: std::array::from_fn(|index| {
            let t = index as f32 / (AXIS_POINTS - 1) as f32;
            lean * t.powf(1.7)
                + Vec3::new(-lean.y, lean.x, 0.0) * (t * PI).sin() * 0.05
                + Vec3::Z * recipe.trunk_height_metres * t
        }),
        radii_metres: std::array::from_fn(|index| {
            let t = index as f32 / (AXIS_POINTS - 1) as f32;
            recipe.trunk_radius_metres * (1.0 - t * 0.78).max(0.20)
        }),
        exposure: 0.82,
        alive: true,
    }
}

fn append_basal_roots(
    axes: &mut Vec<Axis>,
    recipe: BotanicalRecipe,
    rng: &mut Rng,
) -> Result<(), String> {
    for root_index in 0..7 {
        let phase = root_index as f32 / 7.0 * TAU + rng.range(-0.18, 0.18);
        let radial = Vec3::new(phase.cos(), phase.sin(), 0.0);
        let length = recipe.trunk_radius_metres * rng.range(1.25, 2.0);
        push_axis(
            axes,
            curved_axis(
                Some(0),
                1,
                radial * recipe.trunk_radius_metres * 0.36 + Vec3::Z * 0.14,
                radial * length - Vec3::Z * rng.range(0.01, 0.07),
                Vec3::Z * recipe.trunk_radius_metres * rng.range(0.08, 0.18),
                recipe.trunk_radius_metres * rng.range(0.16, 0.24),
                recipe.trunk_radius_metres * rng.range(0.020, 0.040),
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
                + cross * (phase + t * PI * 1.2).sin() * chord.length() * 0.008 * envelope
        }),
        radii_metres: std::array::from_fn(|index| {
            let t = index as f32 / (AXIS_POINTS - 1) as f32;
            base_radius + (tip_radius - base_radius) * t.powf(0.86)
        }),
        exposure: (0.48 + f32::from(order) * 0.15).clamp(0.0, 1.0),
        alive: true,
    }
}

fn push_axis(graph_axes: &mut Vec<Axis>, new_axis: Axis) -> Result<u32, String> {
    let index = u32::try_from(graph_axes.len()).map_err(|_| "rimu axis graph exceeds u32")?;
    graph_axes.push(new_axis);
    Ok(index)
}

fn rimu_foliage(
    recipe: BotanicalRecipe,
    graph: &AxisGraph,
    rng: &mut Rng,
) -> Result<Vec<LeafOrgan>, String> {
    let terminal_count = graph.axes.iter().filter(|axis| axis.order == 3).count();
    let sprays_per_terminal = usize::from(recipe.leaves_per_terminal);
    let mut leaves = Vec::with_capacity(terminal_count * sprays_per_terminal);
    for (axis_index, axis) in graph.axes.iter().enumerate() {
        if axis.order != 3 || !axis.alive {
            continue;
        }
        let axis_id = u32::try_from(axis_index).map_err(|_| "rimu leaf axis exceeds u32")?;
        for spray_index in 0..sprays_per_terminal {
            let rank = spray_index as f32 / sprays_per_terminal.saturating_sub(1).max(1) as f32;
            let fraction = (0.18 + rank * 0.79 + rng.range(-0.025, 0.025)).clamp(0.12, 0.99);
            let (base, tangent, _) = axis.sample(fraction);
            let side = tangent.cross(Vec3::Z).normalize_or(Vec3::X);
            let direction = (tangent * 0.74 + side * rng.range(-0.16, 0.16)
                - Vec3::Z * rng.range(0.18, 0.34))
            .normalize_or(tangent);
            let normal = direction.cross(side).normalize_or(Vec3::Y);
            leaves.push(LeafOrgan {
                axis: axis_id,
                blade_base_metres: base + side * rng.range(-0.055, 0.055),
                direction,
                normal,
                length_metres: rng.range(0.82, 1.08),
                width_metres: rng.range(0.34, 0.46),
                archetype: (rng.next_u64() % LEAF_ARCHETYPE_COUNT as u64) as u8,
                age: (rank * 0.38 + rng.range(0.0, 0.58)).clamp(0.0, 1.0),
                light_exposure: (0.50 + base.z / recipe.trunk_height_metres * 0.42).clamp(0.0, 1.0),
                variation: rng.range(0.0, TAU),
            });
        }
    }
    Ok(leaves)
}

fn rimu_spray_archetypes() -> [Mesh; LEAF_ARCHETYPE_COUNT] {
    std::array::from_fn(rimu_spray_mesh)
}

fn rimu_spray_mesh(variant: usize) -> Mesh {
    let mut mesh = Mesh::default();
    let phase = variant as f32 * 0.41;
    for thread in 0..4 {
        let lateral = (thread as f32 - 1.5) * 0.075;
        let thread_phase = phase + thread as f32 * 1.9;
        for station in 0..22 {
            let t = 0.02 + station as f32 / 21.0 * 0.96;
            let envelope = (t * PI).sin().max(0.0);
            let sway = (t * PI * 2.1 + thread_phase).sin() * 0.030 * envelope;
            let base = Vec3::new(t, lateral + sway, -0.12 * t.powf(1.45));
            let roll = station as f32 * 2.399_963_1 + thread_phase;
            let leaf_length = 0.027 + envelope * 0.013;
            let around = Vec3::new(0.0, roll.cos() * 0.009, roll.sin() * 0.005);
            let tip = base + Vec3::X * leaf_length + around;
            append_scale_leaf(
                &mut mesh,
                variant as u8 % 4,
                base,
                tip,
                0.006 + envelope * 0.002,
                roll,
            );
        }
    }
    mesh.calculate_normals();
    mesh
}

fn append_scale_leaf(mesh: &mut Mesh, tile: u8, base: Vec3, tip: Vec3, half_width: f32, roll: f32) {
    let direction = (tip - base).normalize_or(Vec3::X);
    let first = direction.cross(Vec3::Z).normalize_or(Vec3::Y);
    let second = direction.cross(first).normalize_or(Vec3::Z);
    let side = (first * roll.cos() + second * roll.sin()) * half_width;
    let ridge = direction.cross(side).normalize_or(Vec3::Z) * half_width * 0.22;
    let shoulder = base.lerp(tip, 0.70);
    let centre = base.lerp(tip, 0.64) + ridge;
    let offset = u32::try_from(mesh.vertices.len()).expect("rimu spray mesh fits u32");
    mesh.vertices
        .extend([base, shoulder + side, tip, shoulder - side, centre]);
    for uv in [
        Vec2::new(0.5, 0.0),
        Vec2::new(0.0, 0.70),
        Vec2::new(0.5, 1.0),
        Vec2::new(1.0, 0.70),
        Vec2::new(0.5, 0.64),
    ] {
        mesh.uv.push(atlas_uv(tile, uv));
    }
    mesh.triangles.extend([
        offset,
        offset + 1,
        offset + 4,
        offset + 1,
        offset + 2,
        offset + 4,
        offset + 2,
        offset + 3,
        offset + 4,
        offset + 3,
        offset,
        offset + 4,
    ]);
}

fn atlas_uv(tile: u8, local: Vec2) -> Vec2 {
    let tile = u32::from(tile).min(3);
    let scale = 1.0 / ATLAS_COLUMNS as f32;
    let inset = 1.0 / ATLAS_SIZE as f32;
    Vec2::new(
        (tile % ATLAS_COLUMNS) as f32 * scale
            + inset
            + local.x.clamp(0.0, 1.0) * (scale - inset * 2.0),
        (tile / ATLAS_COLUMNS) as f32 * scale
            + inset
            + local.y.clamp(0.0, 1.0) * (scale - inset * 2.0),
    )
}

fn rimu_bark_albedo(seed: u64) -> BotanicalTexture {
    texture(TEXTURE_WIDTH, BARK_HEIGHT, |x, y| {
        let height = rimu_bark_height(seed, x.cast_signed(), y.cast_signed());
        let grain = value_noise(seed ^ 0xa841, x, y, 19) - 0.5;
        let warm = value_noise(seed ^ 0x316f, x, y, 53);
        let base = Vec3::new(0.20, 0.135, 0.085).lerp(Vec3::new(0.49, 0.33, 0.19), height)
            + Vec3::new(0.055, 0.025, 0.008) * warm
            + Vec3::splat(grain * 0.040);
        encode_colour(base)
    })
}

fn rimu_bark_height(seed: u64, x: i32, y: i32) -> f32 {
    let x = x.rem_euclid(TEXTURE_WIDTH.cast_signed()).cast_unsigned();
    let y = y.rem_euclid(BARK_HEIGHT.cast_signed()).cast_unsigned();
    let broad = value_noise(seed ^ 0xdac1, x, y, 43);
    let fine = value_noise(seed ^ 0x712b, x, y, 9);
    let vertical = ((x as f32 / 14.0 + broad * 2.8).sin() * 0.5 + 0.5).powf(1.8);
    let broken_flake = ((y as f32 / 37.0 + fine * 1.6).fract() - 0.5).abs();
    let edge = (1.0 - broken_flake * 11.0).clamp(0.0, 1.0).powf(1.5);
    (0.13 + broad * 0.35 + fine * 0.18 + vertical * 0.31 - edge * 0.22).clamp(0.0, 1.0)
}

fn rimu_bark_normal(seed: u64) -> BotanicalTexture {
    texture(TEXTURE_WIDTH, BARK_HEIGHT, |x, y| {
        let left = rimu_bark_height(seed, x.cast_signed() - 1, y.cast_signed());
        let right = rimu_bark_height(seed, x.cast_signed() + 1, y.cast_signed());
        let down = rimu_bark_height(seed, x.cast_signed(), y.cast_signed() - 1);
        let up = rimu_bark_height(seed, x.cast_signed(), y.cast_signed() + 1);
        encode_normal(Vec3::new((left - right) * 3.8, (down - up) * 2.6, 1.0))
    })
}

fn rimu_bark_depth(seed: u64) -> BotanicalTexture {
    texture(TEXTURE_WIDTH, BARK_HEIGHT, |x, y| {
        let value = (rimu_bark_height(seed, x.cast_signed(), y.cast_signed()) * 255.0) as u8;
        [value, value, value, 255]
    })
}

fn rimu_leaf_albedo(seed: u64) -> BotanicalTexture {
    texture(ATLAS_SIZE, ATLAS_SIZE, |x, y| {
        let tile = y / TILE_SIZE * ATLAS_COLUMNS + x / TILE_SIZE;
        let local_x = x % TILE_SIZE;
        let local_y = y % TILE_SIZE;
        let base = match tile {
            0 => Vec3::new(0.14, 0.32, 0.085),
            1 => Vec3::new(0.21, 0.43, 0.10),
            2 => Vec3::new(0.40, 0.57, 0.12),
            _ => Vec3::new(0.23, 0.46, 0.09),
        };
        let grain = value_noise(seed ^ u64::from(tile), local_x, local_y, 13) - 0.5;
        let midline = (1.0 - (local_x as f32 / (TILE_SIZE - 1) as f32 - 0.5).abs() * 16.0).max(0.0);
        encode_colour(base + Vec3::splat(grain * 0.030) + Vec3::new(0.035, 0.05, 0.012) * midline)
    })
}

fn rimu_leaf_roughness(seed: u64) -> BotanicalTexture {
    texture(ATLAS_SIZE, ATLAS_SIZE, |x, y| {
        let noise = value_noise(seed ^ 0x5a17, x % TILE_SIZE, y % TILE_SIZE, 9);
        [255, ((0.52 + noise * 0.13) * 255.0) as u8, 0, 255]
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
    let normal = normal.normalize_or(Vec3::Z);
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
    fn rimu_is_deterministic_emergent_and_weeping() {
        let recipe = BotanicalRecipe::for_species(BotanicalSpecies::Rimu);
        let first = generate_botanical_prototype(61, recipe).expect("rimu prototype");
        let second = generate_botanical_prototype(61, recipe).expect("rimu prototype");

        assert_eq!(first, second);
        assert_eq!(first.species, BotanicalSpecies::Rimu);
        assert!(
            first.graph.axes[0].points_metres[AXIS_POINTS - 1].z
                > recipe.trunk_height_metres * 0.99
        );
        let crown_axes: Vec<_> = first
            .graph
            .axes
            .iter()
            .filter(|axis| {
                axis.order == 1 && axis.points_metres[0].z > recipe.trunk_height_metres * 0.05
            })
            .collect();
        assert_eq!(
            crown_axes.len(),
            usize::from(recipe.primary_count) * RIMU_ARMS_PER_TIER
        );
        let lowest_attachment = crown_axes
            .iter()
            .map(|axis| axis.points_metres[0].z)
            .fold(f32::INFINITY, f32::min);
        let highest_attachment = crown_axes
            .iter()
            .map(|axis| axis.points_metres[0].z)
            .fold(f32::NEG_INFINITY, f32::max);
        assert!(lowest_attachment < recipe.trunk_height_metres * 0.16);
        assert!(highest_attachment > recipe.trunk_height_metres * 0.90);
        let chord_length =
            |axis: &&Axis| (axis.points_metres[AXIS_POINTS - 1] - axis.points_metres[0]).length();
        assert!(chord_length(&crown_axes[0]) > chord_length(crown_axes.last().unwrap()) * 1.35);
        let terminals: Vec<_> = first
            .graph
            .axes
            .iter()
            .filter(|axis| axis.order == 3)
            .collect();
        assert!(!terminals.is_empty());
        assert!(
            terminals
                .iter()
                .all(|axis| { axis.points_metres[AXIS_POINTS - 1].z < axis.points_metres[0].z })
        );
        assert!(
            first
                .leaf_archetypes
                .iter()
                .all(|mesh| mesh.vertices.len() >= 120)
        );
        assert!(first.leaf_archetypes.iter().all(|mesh| {
            let (minimum, maximum) = mesh
                .vertices
                .iter()
                .map(|vertex| vertex.y)
                .fold((f32::INFINITY, f32::NEG_INFINITY), |(low, high), y| {
                    (low.min(y), high.max(y))
                });
            maximum - minimum < 0.36
        }));
        assert_eq!(
            first.leaves.len(),
            terminals.len() * usize::from(recipe.leaves_per_terminal)
        );
    }
}
