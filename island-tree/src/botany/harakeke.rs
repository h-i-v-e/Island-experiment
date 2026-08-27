//! Species-specific harakeke architecture.
//!
//! Harakeke is a rhizomatous herb, not a small tree or a radial grass tuft.
//! This generator builds overlapping basal fans of broad strap leaves. Each
//! shared leaf archetype carries a different upper-blade decurve, while the
//! organ transforms preserve the characteristic planar fan arrangement.

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
const GOLDEN_ANGLE: f32 = 2.399_963_1;
const TEXTURE_SIZE: u32 = 256;
const LEAF_TILE_SIZE: u32 = 128;
const LEAF_ATLAS_COLUMNS: u32 = 2;
const LEAF_ATLAS_SIZE: u32 = LEAF_TILE_SIZE * LEAF_ATLAS_COLUMNS;

pub(super) fn generate_harakeke_prototype(
    seed: u64,
    recipe: BotanicalRecipe,
) -> Result<BotanicalPrototype, String> {
    let mut rng = Rng::new(seed ^ HARAKEKE_SEED_DOMAIN);
    let graph = harakeke_graph(recipe, &mut rng);
    let leaves = harakeke_leaves(recipe, &graph, &mut rng)?;
    let foliage_pads = harakeke_foliage_pads(recipe, &graph);
    let (wood, wood_bark) = basal_sheaths(recipe, &graph)?;
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

fn harakeke_graph(recipe: BotanicalRecipe, rng: &mut Rng) -> AxisGraph {
    let fan_count = usize::from(recipe.primary_count);
    let mut axes = Vec::with_capacity(fan_count + 1);
    axes.push(Axis {
        parent: None,
        order: 0,
        points_metres: std::array::from_fn(|index| {
            Vec3::Z * (index as f32 / (AXIS_POINTS - 1) as f32 * 0.10)
        }),
        radii_metres: std::array::from_fn(|index| {
            recipe.trunk_radius_metres * (1.0 - index as f32 * 0.07)
        }),
        exposure: 0.45,
        alive: true,
    });

    for index in 0..fan_count {
        let occupancy = ((index as f32 + 0.55) / fan_count as f32).sqrt();
        let placement_phase = index as f32 * GOLDEN_ANGLE + rng.range(-0.20, 0.20);
        let placement = Vec3::new(placement_phase.cos(), placement_phase.sin(), 0.0)
            * recipe.trunk_radius_metres
            * occupancy
            * 1.58;
        let orientation_phase = placement_phase + rng.range(-0.68, 0.68);
        let orientation = Vec3::new(orientation_phase.cos(), orientation_phase.sin(), 0.0);
        let base = placement + Vec3::Z * rng.range(0.012, 0.055);
        let fan_height = recipe.trunk_height_metres * rng.range(0.12, 0.17);
        axes.push(Axis {
            parent: Some(0),
            order: 1,
            points_metres: std::array::from_fn(|point| {
                let t = point as f32 / (AXIS_POINTS - 1) as f32;
                base + Vec3::Z * fan_height * t + orientation * 0.035 * (t * PI).sin()
            }),
            radii_metres: std::array::from_fn(|point| {
                let t = point as f32 / (AXIS_POINTS - 1) as f32;
                recipe.trunk_radius_metres * (0.24 - t * 0.15)
            }),
            exposure: rng.range(0.66, 0.96),
            alive: true,
        });
    }
    AxisGraph { axes }
}

fn harakeke_leaves(
    recipe: BotanicalRecipe,
    graph: &AxisGraph,
    rng: &mut Rng,
) -> Result<Vec<LeafOrgan>, String> {
    let leaves_per_fan = usize::from(recipe.leaves_per_terminal);
    let mut leaves =
        Vec::with_capacity(usize::from(recipe.primary_count).saturating_mul(leaves_per_fan));
    for (axis_index, axis) in graph.axes.iter().enumerate().skip(1) {
        let axis_id = u32::try_from(axis_index).map_err(|_| "harakeke fan index exceeds u32")?;
        let base = axis.points_metres[0];
        let fan_delta = axis.points_metres[2] - axis.points_metres[0];
        let fan_heading = (fan_delta - Vec3::Z * fan_delta.z)
            .try_normalize()
            .unwrap_or_else(|| {
                let phase = axis_index as f32 * GOLDEN_ANGLE;
                Vec3::new(phase.cos(), phase.sin(), 0.0)
            });
        let fan_lateral = Vec3::new(-fan_heading.y, fan_heading.x, 0.0);
        let fan_maturity = if axis_index.is_multiple_of(3) {
            rng.range(0.96, 1.0)
        } else {
            rng.range(0.80, 0.91)
        };

        for leaf_index in 0..leaves_per_fan {
            let centred = if leaves_per_fan <= 1 {
                0.0
            } else {
                leaf_index as f32 / (leaves_per_fan - 1) as f32 * 2.0 - 1.0
            };
            let shape_age = centred.abs().powf(0.92);
            let age = shape_age * fan_maturity;
            let side = if centred.abs() < 0.035 {
                if leaf_index.is_multiple_of(2) {
                    -1.0
                } else {
                    1.0
                }
            } else {
                centred.signum()
            };
            let horizontal = (fan_lateral * side * (0.80 + shape_age * 0.20)
                + fan_heading * (0.18 + rng.range(-0.10, 0.10)))
            .normalize_or(fan_lateral * side);
            let elevation = (1.47 - shape_age * 0.78 + rng.range(-0.045, 0.045)).clamp(0.62, 1.50);
            let direction =
                (horizontal * elevation.cos() + Vec3::Z * elevation.sin()).normalize_or(Vec3::Z);
            // The archetype's normal-displacement axis is scaled by blade
            // length. Point it outward and down within the growth plane so the
            // upper half can decurve by a physically meaningful fraction of
            // the leaf rather than by a fraction of its narrow width.
            let normal =
                (horizontal * elevation.sin() - Vec3::Z * elevation.cos()).normalize_or(horizontal);
            let length =
                recipe.trunk_height_metres * (1.01 - shape_age * 0.20) * rng.range(0.91, 1.045);
            let width = (length * rng.range(0.050, 0.068)).clamp(0.075, 0.145);
            let layer = (leaf_index % 5) as f32 - 2.0;
            let base_spread = recipe.trunk_radius_metres * 0.12;
            leaves.push(LeafOrgan {
                axis: axis_id,
                blade_base_metres: base
                    + fan_heading * layer * 0.012
                    + fan_lateral * centred * base_spread
                    + Vec3::Z * (0.035 + (1.0 - shape_age) * 0.065 + rng.range(-0.012, 0.012)),
                direction,
                normal,
                length_metres: length,
                width_metres: width,
                archetype: leaf_archetype(age, leaf_index),
                age: (0.08 + age * 0.84 + rng.range(-0.04, 0.04)).clamp(0.0, 1.0),
                light_exposure: (axis.exposure + direction.z * 0.08).clamp(0.0, 1.0),
                variation: rng.range(0.0, TAU),
            });
        }
    }
    Ok(leaves)
}

fn leaf_archetype(age: f32, index: usize) -> u8 {
    let cohort = if age < 0.25 {
        0
    } else if age < 0.60 {
        1
    } else if age < 0.94 {
        2
    } else {
        3
    };
    cohort + if index.is_multiple_of(3) { 4 } else { 0 }
}

fn harakeke_leaf_archetypes() -> [Mesh; LEAF_ARCHETYPE_COUNT] {
    std::array::from_fn(|index| strap_leaf_mesh(index as u8))
}

fn strap_leaf_mesh(archetype: u8) -> Mesh {
    const STATIONS: usize = 25;
    const COLUMNS: [f32; 7] = [-1.0, -0.67, -0.33, 0.0, 0.33, 0.67, 1.0];
    let cohort = archetype % 4;
    let variant = f32::from(archetype / 4);
    let bend = match cohort {
        0 => 0.05,
        1 => 0.18,
        2 => 0.48,
        _ => 0.62,
    } + variant * 0.06;
    let damaged_tip = cohort == 3;
    let sweep_sign = if archetype.is_multiple_of(2) {
        -1.0
    } else {
        1.0
    };
    let sweep = sweep_sign * (0.012 + variant * 0.024);
    let mut mesh = Mesh::default();
    for station in 0..STATIONS {
        let t = station as f32 / (STATIONS - 1) as f32;
        let base_taper = smoothstep((t / 0.085).clamp(0.0, 1.0));
        let tip_taper = if damaged_tip {
            0.38 + smoothstep(((1.0 - t) / 0.12).clamp(0.0, 1.0)) * 0.62
        } else {
            smoothstep(((1.0 - t) / 0.16).clamp(0.0, 1.0))
        };
        let width_profile = base_taper * tip_taper * (1.0 - t * 0.06);
        let bend_t = smoothstep(((t - 0.32) / 0.68).clamp(0.0, 1.0));
        let centreline = bend * bend_t.powf(1.42);
        for lateral in COLUMNS {
            let edge_ripple = lateral.abs().powf(5.0)
                * (t.mul_add(TAU * 3.2, f32::from(archetype) * 0.71)).sin()
                * (t * PI).sin()
                * 0.007;
            let basal_fold = 1.0 - smoothstep(((t - 0.18) / 0.28).clamp(0.0, 1.0));
            let keel =
                (1.0 - lateral.abs().powf(1.35)) * width_profile * (0.006 + basal_fold * 0.014);
            let corrugation = (lateral * PI * 3.0).cos() * width_profile * 0.0035;
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
            mesh.vertices.push(Vec3::new(
                t - tip_damage,
                lateral * width_profile * 0.50 + edge_ripple + sweep * (t * PI).sin(),
                centreline + keel + corrugation,
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

fn harakeke_foliage_pads(recipe: BotanicalRecipe, graph: &AxisGraph) -> Vec<FoliagePad> {
    graph
        .axes
        .iter()
        .enumerate()
        .skip(1)
        .map(|(axis_index, axis)| {
            let base = axis.points_metres[0];
            let delta = axis.points_metres[2] - base;
            let heading = (delta - Vec3::Z * delta.z).normalize_or(Vec3::X);
            FoliagePad {
                axis: axis_index as u32,
                centre_metres: base + Vec3::Z * 0.02,
                direction: Vec3::Z,
                normal: heading,
                half_extents_metres: Vec3::new(
                    recipe.trunk_height_metres,
                    recipe.trunk_radius_metres * 1.30,
                    recipe.trunk_height_metres * 0.68,
                ),
                archetype: (axis_index % FOLIAGE_PAD_ARCHETYPE_COUNT) as u8,
                mean_age: 0.54,
                light_exposure: axis.exposure,
                density: 0.96,
                variation: axis_index as f32 * GOLDEN_ANGLE,
            }
        })
        .collect()
}

fn harakeke_pad_archetypes() -> [Mesh; FOLIAGE_PAD_ARCHETYPE_COUNT] {
    [proxy_fan_mesh(0.78), proxy_fan_mesh(0.90)]
}

fn proxy_fan_mesh(droop: f32) -> Mesh {
    // Keep the middle-distance silhouette faithful to the default near fan.
    // A denser proxy makes the plant visibly gain foliage across the LOD cut.
    const LEAVES: usize = 9;
    const STATIONS: usize = 6;
    let mut mesh = Mesh::default();
    for leaf in 0..LEAVES {
        let centred = leaf as f32 / (LEAVES - 1) as f32 * 2.0 - 1.0;
        let age = centred.abs().powf(0.82);
        let length = 1.0 - age * 0.18;
        let base = mesh.vertices.len() as u32;
        for station in 0..STATIONS {
            let t = station as f32 / (STATIONS - 1) as f32;
            let tip_taper = ((1.0 - t) / 0.24).clamp(0.0, 1.0);
            let base_taper = (t / 0.10).clamp(0.0, 1.0);
            let half_width =
                (0.020 + (1.0 - age) * 0.010) * smoothstep(base_taper) * smoothstep(tip_taper);
            let vertical = length * (t - age * droop * t * t);
            let lateral = centred * (0.08 + t.powf(1.18) * 0.92);
            let outward = age * droop * t.powf(1.55) * 0.58;
            for side in [-1.0_f32, 1.0] {
                mesh.vertices
                    .push(Vec3::new(vertical, lateral + side * half_width, outward));
                mesh.uv
                    .push(leaf_uv(0, Vec2::new(side.mul_add(0.5, 0.5), t)));
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

fn basal_sheaths(
    recipe: BotanicalRecipe,
    graph: &AxisGraph,
) -> Result<(Mesh, Vec<BarkVertex>), String> {
    const SIDES: usize = 14;
    let mut mesh = Mesh::default();
    let mut bark = Vec::new();
    for axis in graph.axes.iter().skip(1) {
        let centre = axis.points_metres[0];
        let radius = recipe.trunk_radius_metres * 0.21;
        let base_index = u32::try_from(mesh.vertices.len())
            .map_err(|_| "harakeke basal sheath mesh exceeds u32 indices")?;
        for ring in 0..=2 {
            let t = ring as f32 / 2.0;
            for side in 0..=SIDES {
                let phase = side as f32 / SIDES as f32 * TAU;
                let radial = Vec3::new(phase.cos(), phase.sin(), 0.0);
                mesh.vertices.push(
                    centre
                        + radial * radius * (1.0 - t * 0.42)
                        + Vec3::Z * recipe.trunk_height_metres * 0.105 * t,
                );
                mesh.uv.push(Vec2::new(side as f32 / SIDES as f32, t));
                bark.push(BarkVertex {
                    radius_metres: radius * (1.0 - t * 0.42),
                    maturity: 0.80 - t * 0.38,
                });
            }
        }
        let stride = SIDES + 1;
        for ring in 0..2 {
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
        encode_colour(Vec3::new(0.24, 0.27, 0.12) + Vec3::splat(fibres + noise * 0.055))
    })
}

fn harakeke_leaf_albedo(seed: u64) -> BotanicalTexture {
    texture(LEAF_ATLAS_SIZE, LEAF_ATLAS_SIZE, |x, y| {
        let tile = x / LEAF_TILE_SIZE + y / LEAF_TILE_SIZE * 2;
        let local_x = (x % LEAF_TILE_SIZE) as f32 / (LEAF_TILE_SIZE - 1) as f32;
        let local_y = (y % LEAF_TILE_SIZE) as f32 / (LEAF_TILE_SIZE - 1) as f32;
        let noise = hash_unit(seed ^ u64::from(tile) ^ 0x6c65_6166, x, y) - 0.5;
        let longitudinal = (local_y * 73.0 + local_x * 8.0).sin() * 0.012;
        let midrib = (1.0 - ((local_x - 0.5).abs() * 34.0)).max(0.0);
        let edge = ((0.045 - local_x.min(1.0 - local_x)) / 0.045).clamp(0.0, 1.0);
        let base = match tile {
            1 => Vec3::new(0.17, 0.32, 0.12),
            2 => Vec3::new(0.20, 0.31, 0.11),
            3 => Vec3::new(0.29, 0.22, 0.065),
            _ => Vec3::new(0.14, 0.34, 0.13),
        };
        let margin = Vec3::new(0.20, 0.055, 0.025) * edge * (0.55 + local_y * 0.30);
        encode_colour(
            base + Vec3::splat(noise * 0.028 + longitudinal)
                + Vec3::new(0.045, 0.070, 0.020) * midrib
                + margin,
        )
    })
}

fn harakeke_leaf_metallic_roughness(seed: u64) -> BotanicalTexture {
    texture(LEAF_ATLAS_SIZE, LEAF_ATLAS_SIZE, |x, y| {
        let noise = hash_unit(seed ^ 0x726f_7567, x, y);
        [255, ((0.48 + noise * 0.14) * 255.0) as u8, 0, 255]
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
            leaf.length_metres >= recipe.trunk_height_metres * 0.68
                && leaf.length_metres <= recipe.trunk_height_metres * 1.06
                && (0.075..=0.145).contains(&leaf.width_metres)
                && leaf.direction.dot(leaf.normal).abs() < 0.001
        }));
        for mesh in &prototype.leaf_archetypes {
            assert_eq!(mesh.vertices.len(), 25 * 7);
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
            let tip = &mesh.vertices[mesh.vertices.len() - 7..];
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
}
