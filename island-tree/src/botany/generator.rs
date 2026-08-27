//! Renderer-neutral pōhutukawa generator used by the tree laboratory.
//!
//! This is intentionally separate from the island-wide forest compiler. The
//! lab owns one reviewed species prototype, while forest integration remains
//! gated on close and middle-distance visual evidence.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::many_single_char_names
)]

use std::f32::consts::{PI, TAU};

use motu::{Mesh, Vec2, Vec3};

use super::{
    model::{
        AXIS_POINTS, Axis, AxisGraph, BarkVertex, BotanicalPrototype, BotanicalRecipe,
        BotanicalSpecies, BotanicalTexture, FOLIAGE_PAD_ARCHETYPE_COUNT, FoliagePad,
        LEAF_ARCHETYPE_COUNT, LeafOrgan, SHOOT_TIP_ARCHETYPE_COUNT, ShootTipOrgan, ShootTipState,
    },
    nikau::generate_nikau_prototype,
    random::Rng,
};

const BOTANICAL_SEED_DOMAIN: u64 = 0x626f_7461_6e69_6361;
const CROWN_ENVIRONMENT_SEED_DOMAIN: u64 = 0x6372_6f77_6e5f_656e;
const LEAF_SEED_DOMAIN: u64 = 0x6c65_6166_5f6f_7267;
const FOLIAGE_COHORT_SEED_DOMAIN: u64 = 0x666f_6c69_6167_655f;
const SHOOT_TIP_SEED_DOMAIN: u64 = 0x7469_705f_7374_6174;
const EPICORMIC_SEED_DOMAIN: u64 = 0x6570_6963_6f72_6d69;
const TEXTURE_SEED_DOMAIN: u64 = 0x7465_7874_7572_6573;
const SCAFFOLD_HISTORY_SEED_DOMAIN: u64 = 0x7363_6166_666f_6c64;
const SCAR_TEXTURE_SEED_DOMAIN: u64 = 0x656e_645f_6772_6169;
const LEAF_TILE_SIZE: u32 = 128;
const LEAF_ATLAS_COLUMNS: u32 = 2;
const LEAF_ATLAS_TILE_COUNT: u32 = LEAF_ATLAS_COLUMNS * LEAF_ATLAS_COLUMNS;
const LEAF_ATLAS_SIZE: u32 = LEAF_TILE_SIZE * LEAF_ATLAS_COLUMNS;
const BARK_TEXTURE_WIDTH: u32 = 256;
const BARK_TEXTURE_HEIGHT: u32 = 512;
const MAX_GROWTH_NODES: usize = 470;
const GROWTH_STEPS: usize = 24;
const INFLUENCE_RADIUS_METRES: f32 = 3.2;
const KILL_RADIUS_METRES: f32 = 0.58;
const MIN_NODE_SPACING_METRES: f32 = 0.34;
const MIN_FINE_SHOOTS_PER_TERMINAL: usize = 3;
const MAX_FINE_SHOOTS_PER_TERMINAL: usize = 7;
const MAX_SECONDARY_FINE_SHOOTS_PER_TERMINAL: usize = 2;
const MIN_LEAVES_FOR_SECONDARY_FINE_SHOOT: usize = 6;
const SECONDARY_FINE_SHOOT_MIN_VIGOUR: f32 = 0.44;
const MAX_PREVIOUS_FLUSH_LEAVES_PER_SHOOT: usize = 4;
const MAX_EPICORMIC_SHOOTS: usize = 3;
const EPICORMIC_LEAVES_PER_SHOOT: usize = 5;
const MAX_EPICORMIC_LEAVES: usize = MAX_EPICORMIC_SHOOTS * EPICORMIC_LEAVES_PER_SHOOT;
const PREVIOUS_FLUSH_START: f32 = 0.14;
const PREVIOUS_FLUSH_END: f32 = 0.43;
const CURRENT_FLUSH_START: f32 = 0.50;
const CURRENT_FLUSH_END: f32 = 0.93;
const MAX_PERSISTENT_DEAD_STUBS: usize = 5;
const MIN_HISTORY_SUPPORT_RADIUS_METRES: f32 = 0.070;
const MIN_DEAD_STUB_ROOT_RADIUS_METRES: f32 = 0.024;
const SCAR_RING_SIDES: usize = 16;
const SCAR_VERTEX_COUNT: usize = SCAR_RING_SIDES + 2;
const SCAR_TEXTURE_SIZE: u32 = 128;
const YOUNG_BARK_TILE_WIDTH_METRES: f32 = 0.32;
const MATURE_BARK_TILE_WIDTH_METRES: f32 = 1.05;
const MATURE_BARK_RING_SPACING_METRES: f32 = 0.10;
const YOUNG_BARK_RING_SPACING_METRES: f32 = 0.22;
const CANOPY_LIGHT_CELL_METRES: f32 = 0.42;
const CANOPY_LIGHT_RAY_STEPS: usize = 24;
const CANOPY_LIGHT_STEP_METRES: f32 = CANOPY_LIGHT_CELL_METRES * 0.5;
const CANOPY_EXTINCTION: f32 = 2.35;
const POHUTUKAWA_LEAF_LENGTH_RANGE_METRES: (f32, f32) = (0.060, 0.105);
const POHUTUKAWA_LEAF_LENGTH_BOUNDS_METRES: (f32, f32) = (0.050, 0.120);
const MIDDLE_PROXY_LEAF_LENGTH_RANGE: (f32, f32) = (0.18, 0.25);
const MIDDLE_PROXY_LEAF_WIDTH_RANGE: (f32, f32) = (0.13, 0.18);
const CANOPY_SKY_DIRECTIONS: [Vec3; 9] = [
    Vec3::new(0.0, 0.0, 1.0),
    Vec3::new(0.573_576, 0.0, 0.819_152),
    Vec3::new(-0.573_576, 0.0, 0.819_152),
    Vec3::new(0.0, 0.573_576, 0.819_152),
    Vec3::new(0.0, -0.573_576, 0.819_152),
    Vec3::new(0.405_58, 0.405_58, 0.819_152),
    Vec3::new(-0.405_58, 0.405_58, 0.819_152),
    Vec3::new(0.405_58, -0.405_58, 0.819_152),
    Vec3::new(-0.405_58, -0.405_58, 0.819_152),
];

/// Builds one deterministic species prototype in metres, Z-up.
///
/// # Errors
///
/// Returns an error when the recipe is outside its documented bounds or the
/// generated organ and mesh indices cannot be represented safely.
pub fn generate_botanical_prototype(
    seed: u64,
    recipe: BotanicalRecipe,
) -> Result<BotanicalPrototype, String> {
    let recipe = recipe.validate()?;
    match recipe.species {
        BotanicalSpecies::Pohutukawa => generate_pohutukawa_prototype(seed, recipe),
        BotanicalSpecies::Nikau => generate_nikau_prototype(seed, recipe),
    }
}

fn generate_pohutukawa_prototype(
    seed: u64,
    recipe: BotanicalRecipe,
) -> Result<BotanicalPrototype, String> {
    let graph = generate_graph(seed, recipe)?;
    let FineOrgans {
        microtwigs,
        microtwig_bark,
        leaves,
        shoot_tips,
    } = generate_fine_organs(seed, recipe, &graph)?;
    let foliage_pads = generate_foliage_pads(&graph, &leaves);
    let (wood, wood_bark, wood_scars) = generate_wood(seed, &graph)?;
    Ok(BotanicalPrototype {
        species: recipe.species,
        graph,
        wood,
        wood_bark,
        wood_scars,
        wood_scar_albedo: scar_texture(seed ^ SCAR_TEXTURE_SEED_DOMAIN),
        microtwigs,
        microtwig_bark,
        leaf_archetypes: leaf_archetypes(),
        shoot_tip_archetypes: shoot_tip_archetypes(),
        reproductive_archetypes: std::array::from_fn(|_| Mesh::default()),
        foliage_pad_archetypes: foliage_pad_archetypes(),
        leaves,
        shoot_tips,
        reproductive_organs: Vec::new(),
        foliage_pads,
        bark_albedo: bark_texture(seed ^ TEXTURE_SEED_DOMAIN),
        bark_normal: bark_normal_texture(seed ^ TEXTURE_SEED_DOMAIN),
        bark_depth: bark_depth_texture(seed ^ TEXTURE_SEED_DOMAIN),
        bark_metallic_roughness: bark_metallic_roughness_texture(seed ^ TEXTURE_SEED_DOMAIN),
        leaf_albedo: leaf_texture(seed ^ LEAF_SEED_DOMAIN),
        leaf_metallic_roughness: leaf_metallic_roughness_texture(seed ^ LEAF_SEED_DOMAIN),
    })
}

fn generate_graph(seed: u64, recipe: BotanicalRecipe) -> Result<AxisGraph, String> {
    let graph = generate_competition_graph(seed, recipe)?;
    let leaf_bearing = graph.axes.iter().filter(|axis| axis.order == 3).count();
    if leaf_bearing >= usize::from(recipe.primary_count) * 2 {
        Ok(graph)
    } else {
        generate_tiered_graph(seed, recipe)
    }
}

#[derive(Clone, Copy, Debug)]
struct GrowthNode {
    position: Vec3,
    direction: Vec3,
    parent: Option<usize>,
    depth: u8,
    children: u8,
    exposure: f32,
    apical_control: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct CrownEnvironment {
    storm_direction: f32,
    shape_axis: f32,
    major_radius_metres: f32,
    minor_radius_metres: f32,
    half_height_metres: f32,
    lee_extension: f32,
    upper_lean_metres: f32,
    gap_direction: f32,
    gap_half_width: f32,
}

fn crown_environment(seed: u64, storm_direction: f32) -> CrownEnvironment {
    let mut rng = Rng::new(seed ^ CROWN_ENVIRONMENT_SEED_DOMAIN);
    CrownEnvironment {
        storm_direction,
        shape_axis: storm_direction + rng.range(-0.82, 0.82),
        major_radius_metres: rng.range(7.4, 8.5),
        minor_radius_metres: rng.range(5.2, 6.2),
        half_height_metres: rng.range(2.55, 3.15),
        lee_extension: rng.range(0.12, 0.28),
        upper_lean_metres: rng.range(0.55, 1.35),
        gap_direction: storm_direction + rng.range(-0.58, 0.58),
        gap_half_width: rng.range(0.32, 0.52),
    }
}

fn generate_competition_graph(seed: u64, recipe: BotanicalRecipe) -> Result<AxisGraph, String> {
    let mut rng = Rng::new(seed ^ BOTANICAL_SEED_DOMAIN);
    let storm_direction = rng.range(-PI, PI);
    let wind = Vec3::new(storm_direction.cos(), storm_direction.sin(), 0.0);
    let crosswind = Vec3::new(-wind.y, wind.x, 0.0);
    let drift = -wind * rng.range(0.45, 1.05) + crosswind * rng.range(-0.28, 0.28);
    let trunk = trunk_axis(recipe, drift);
    let environment = crown_environment(seed, storm_direction);
    let attraction_count = usize::from(recipe.primary_count)
        * usize::from(recipe.secondaries_per_primary)
        * usize::from(recipe.terminals_per_secondary);
    let mut attractions = crown_attractions(&mut rng, recipe, environment, attraction_count, drift);
    let mut nodes = crown_seeds(&mut rng, recipe, environment, trunk);
    colonise_crown(
        &mut rng,
        &mut nodes,
        &mut attractions,
        environment.storm_direction,
    );
    let mut axes = Vec::with_capacity(nodes.len() + 1);
    axes.push(trunk);
    append_growth_axes(&mut axes, &nodes, &mut rng)?;
    Ok(AxisGraph { axes })
}

fn trunk_axis(recipe: BotanicalRecipe, drift: Vec3) -> Axis {
    Axis {
        parent: None,
        order: 0,
        points_metres: std::array::from_fn(|index| {
            let t = index as f32 / (AXIS_POINTS - 1) as f32;
            let bend = t * t;
            Vec3::new(
                drift.x * bend + (t * PI).sin() * 0.12,
                drift.y * bend + (t * 1.7 * PI).sin() * 0.08,
                recipe.trunk_height_metres * t,
            )
        }),
        radii_metres: std::array::from_fn(|index| {
            let t = index as f32 / (AXIS_POINTS - 1) as f32;
            recipe.trunk_radius_metres * (1.0 - 0.90 * t).max(0.08)
        }),
        exposure: 1.0,
        alive: true,
    }
}

fn crown_seeds(
    rng: &mut Rng,
    recipe: BotanicalRecipe,
    environment: CrownEnvironment,
    trunk: Axis,
) -> Vec<GrowthNode> {
    let seed_count = usize::from(recipe.primary_count).saturating_sub(2).max(5);
    (0..seed_count)
        .map(|seed| {
            let level = seed as f32 / (seed_count - 1) as f32;
            let attachment = 0.18 + level * 0.38 + rng.range(-0.018, 0.018);
            let azimuth = seed as f32 * 2.399_963_1 + rng.range(-0.28, 0.28);
            let horizontal = Vec3::new(azimuth.cos(), azimuth.sin(), 0.0);
            let wind_alignment = (azimuth - environment.storm_direction).cos();
            let windward = wind_alignment.max(0.0);
            let leeward = (-wind_alignment).max(0.0);
            let in_gap =
                (azimuth - environment.gap_direction).cos() > environment.gap_half_width.cos();
            let (position, trunk_tangent, _) = trunk.sample(attachment);
            let leader = seed + 1 == seed_count;
            let horizontal_weight = if leader {
                0.70
            } else {
                (1.0 - windward * 0.30 + leeward * 0.16) * if in_gap { 0.62 } else { 1.0 }
            };
            let mut apical_weight = if leader {
                rng.range(0.48, 0.62)
            } else {
                rng.range(0.10, 0.24) + level * 0.08
            };
            if in_gap && !leader {
                apical_weight += 0.20;
            }
            GrowthNode {
                position,
                direction: (horizontal * horizontal_weight
                    + trunk_tangent * apical_weight
                    + Vec3::Z * level * 0.05)
                    .normalize_or(Vec3::Z),
                parent: None,
                depth: 0,
                children: 0,
                exposure: (0.72 + level * 0.22 - windward * 0.14).clamp(0.2, 1.0),
                apical_control: if leader { 0.58 } else { 0.06 + level * 0.12 },
            }
        })
        .collect()
}

fn crown_attractions(
    rng: &mut Rng,
    recipe: BotanicalRecipe,
    environment: CrownEnvironment,
    count: usize,
    drift: Vec3,
) -> Vec<Vec3> {
    let mut points = Vec::with_capacity(count);
    let mut attempts = 0;
    while points.len() < count && attempts < count * 32 {
        attempts += 1;
        let unit = Vec3::new(
            rng.range(-1.0, 1.0),
            rng.range(-1.0, 1.0),
            rng.range(-1.0, 1.0),
        );
        if unit.length_squared() > 1.0 || unit.z < -0.92 {
            continue;
        }
        let sector = unit.y.atan2(unit.x);
        let horizontal_unit = Vec3::new(unit.x, unit.y, 0.0).normalize_or(Vec3::X);
        let wind = Vec3::new(
            environment.storm_direction.cos(),
            environment.storm_direction.sin(),
            0.0,
        );
        let major_axis = Vec3::new(
            environment.shape_axis.cos(),
            environment.shape_axis.sin(),
            0.0,
        );
        let minor_axis = Vec3::new(-major_axis.y, major_axis.x, 0.0);
        let wind_alignment = horizontal_unit.dot(wind);
        let windward = wind_alignment.max(0.0);
        let leeward = (-wind_alignment).max(0.0);
        let lobe = 0.98
            + (sector * 3.0 + environment.shape_axis * 0.37).sin() * 0.18
            + (sector * 5.0 - environment.shape_axis * 0.21).sin() * 0.09;
        let vertical_profile = 1.0 + (-unit.z).max(0.0) * 0.12 - unit.z.max(0.0) * 0.08;
        let wind_scale = 1.0 - windward * 0.16 + leeward * environment.lee_extension;
        let horizontal = (major_axis * unit.dot(major_axis) * environment.major_radius_metres
            + minor_axis * unit.dot(minor_axis) * environment.minor_radius_metres)
            * lobe
            * vertical_profile
            * wind_scale;
        let upper_lean =
            -wind * environment.upper_lean_metres * unit.z.mul_add(0.74, 0.26).clamp(0.0, 1.0);
        let lobe_height = (sector * 2.0 + environment.shape_axis * 0.41).sin() * 0.34
            + (sector * 5.0 - environment.shape_axis * 0.19).sin() * 0.16;
        let position = horizontal
            + upper_lean
            + Vec3::Z
                * (recipe.trunk_height_metres * 0.58
                    + unit.z * environment.half_height_metres
                    + lobe_height)
            + drift * (0.45 + unit.z.max(0.0) * 0.55);
        let radial = position.x.hypot(position.y);
        let protected_core = radial < 1.05 && position.z < recipe.trunk_height_metres * 0.68;
        let storm_gap = (sector - environment.gap_direction).cos()
            > environment.gap_half_width.cos()
            && unit.z > -0.30
            && radial > 1.6
            && rng.unit() > 0.20;
        if position.z > 1.75 && !protected_core && !storm_gap {
            points.push(position);
        }
    }
    points
}

#[allow(clippy::too_many_lines)]
fn colonise_crown(
    rng: &mut Rng,
    nodes: &mut Vec<GrowthNode>,
    attractions: &mut Vec<Vec3>,
    storm_direction: f32,
) {
    for _ in 0..GROWTH_STEPS {
        let existing = nodes.len();
        let mut direction_sums = vec![Vec3::ZERO; existing];
        let mut influence_counts = vec![0_u16; existing];
        for &point in attractions.iter() {
            if let Some(index) = nearest_influenced_node(point, nodes) {
                direction_sums[index] +=
                    (point - nodes[index].position).normalize_or(nodes[index].direction);
                influence_counts[index] += 1;
            }
        }

        let mut candidates = Vec::new();
        for index in 0..existing {
            let count = influence_counts[index];
            if count == 0
                || nodes[index].children >= 3
                || nodes.len() + candidates.len() >= MAX_GROWTH_NODES
            {
                continue;
            }
            let average = direction_sums[index] / f32::from(count);
            let wind = Vec3::new(storm_direction.cos(), storm_direction.sin(), 0.0);
            let direction = (average * 1.35
                + nodes[index].direction * 0.82
                + Vec3::Z * (rng.range(-0.02, 0.09) + nodes[index].apical_control * 0.08)
                - wind * rng.range(0.0, 0.08) * (1.0 - nodes[index].apical_control * 0.25))
                .normalize_or(nodes[index].direction);
            if has_similar_child(nodes, index, direction) {
                continue;
            }
            let step =
                rng.range(0.68, 0.94) * (1.0 - f32::from(nodes[index].depth).min(12.0) * 0.012);
            let position = nodes[index].position + direction * step;
            if separated(position, nodes, &candidates) {
                candidates.push(GrowthNode {
                    position,
                    direction,
                    parent: Some(index),
                    depth: nodes[index].depth.saturating_add(1),
                    children: 0,
                    exposure: (nodes[index].exposure
                        + position.z.mul_add(0.025, -0.12)
                        + rng.range(-0.08, 0.08))
                    .clamp(0.15, 1.0),
                    apical_control: nodes[index].apical_control
                        * direction.dot(nodes[index].direction).max(0.0).powi(2)
                        * 0.94,
                });
            }
        }
        if candidates.is_empty() {
            break;
        }
        for candidate in &candidates {
            if let Some(parent) = candidate.parent {
                nodes[parent].children += 1;
            }
            nodes.push(*candidate);
        }
        attractions.retain(|point| {
            nodes
                .iter()
                .all(|node| node.position.distance_squared(*point) > KILL_RADIUS_METRES.powi(2))
        });
        if attractions.is_empty() || nodes.len() >= MAX_GROWTH_NODES {
            break;
        }
    }
}

fn nearest_influenced_node(point: Vec3, nodes: &[GrowthNode]) -> Option<usize> {
    let mut nearest = None;
    let mut nearest_distance = INFLUENCE_RADIUS_METRES.powi(2);
    for (index, node) in nodes.iter().enumerate() {
        let distance = node.position.distance_squared(point);
        if distance < KILL_RADIUS_METRES.powi(2) {
            return None;
        }
        if node.children < 3 && distance < nearest_distance {
            nearest = Some(index);
            nearest_distance = distance;
        }
    }
    nearest
}

fn has_similar_child(nodes: &[GrowthNode], parent: usize, direction: Vec3) -> bool {
    nodes
        .iter()
        .any(|node| node.parent == Some(parent) && node.direction.dot(direction) > 0.94)
}

fn separated(position: Vec3, nodes: &[GrowthNode], candidates: &[GrowthNode]) -> bool {
    let minimum = MIN_NODE_SPACING_METRES.powi(2);
    nodes
        .iter()
        .chain(candidates)
        .all(|node| node.position.distance_squared(position) > minimum)
}

fn append_growth_axes(
    axes: &mut Vec<Axis>,
    nodes: &[GrowthNode],
    rng: &mut Rng,
) -> Result<(), String> {
    let loads = subtree_loads(nodes);
    let mut axis_indices = vec![None; nodes.len()];
    for (node_index, node) in nodes.iter().enumerate() {
        let Some(parent_node) = node.parent else {
            continue;
        };
        let origin = nodes[parent_node].position;
        let parent_axis = axis_indices[parent_node].unwrap_or(0);
        let order = match (node.children, node.depth) {
            (0, _) | (_, 6..) => 3,
            (_, 3..) => 2,
            _ => 1,
        };
        let load_radius = f32::from(loads[node_index])
            .sqrt()
            .mul_add(0.040, 0.034)
            .clamp(0.042, 0.40);
        let root_radius = load_radius.min(match order {
            1 => 0.40,
            2 => 0.14,
            _ => 0.058,
        });
        let tip_scale = if node.children == 0 { 0.28 } else { 0.72 };
        let growth_segment = growth_axis(
            Some(parent_axis),
            order,
            origin,
            node.position,
            nodes[parent_node].direction,
            node.direction,
            root_radius,
            tip_scale,
            rng,
            node.exposure,
            true,
        );
        axis_indices[node_index] = Some(push_axis(axes, growth_segment)?);
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn growth_axis(
    parent: Option<u32>,
    order: u8,
    origin: Vec3,
    end: Vec3,
    start_direction: Vec3,
    end_direction: Vec3,
    root_radius: f32,
    tip_scale: f32,
    rng: &mut Rng,
    exposure: f32,
    alive: bool,
) -> Axis {
    let chord = end - origin;
    let length = chord.length();
    let start_tangent = start_direction.normalize_or(chord) * length * 0.62;
    let end_tangent = end_direction.normalize_or(chord) * length * 0.62;
    let side = chord.cross(Vec3::Z).normalize_or(Vec3::X);
    let sweep = side * length * rng.range(-0.025, 0.025);
    let points_metres = std::array::from_fn(|index| {
        let t = index as f32 / (AXIS_POINTS - 1) as f32;
        let t2 = t * t;
        let t3 = t2 * t;
        let h00 = t3.mul_add(2.0, -3.0 * t2) + 1.0;
        let h10 = t3 - 2.0 * t2 + t;
        let h01 = -2.0 * t3 + 3.0 * t2;
        let h11 = t3 - t2;
        origin * h00 + start_tangent * h10 + end * h01 + end_tangent * h11 + sweep * (t * PI).sin()
    });
    let radii_metres = std::array::from_fn(|index| {
        let t = index as f32 / (AXIS_POINTS - 1) as f32;
        root_radius * (1.0 - (1.0 - tip_scale) * t.powf(0.86))
    });
    Axis {
        parent,
        order,
        points_metres,
        radii_metres,
        exposure,
        alive,
    }
}

fn subtree_loads(nodes: &[GrowthNode]) -> Vec<u16> {
    let mut loads = vec![0_u16; nodes.len()];
    for index in (0..nodes.len()).rev() {
        loads[index] = loads[index].max(u16::from(nodes[index].children == 0));
        if let Some(parent) = nodes[index].parent {
            loads[parent] = loads[parent].saturating_add(loads[index].max(1));
        }
    }
    loads
}

#[allow(clippy::too_many_lines)]
fn generate_tiered_graph(seed: u64, recipe: BotanicalRecipe) -> Result<AxisGraph, String> {
    let mut rng = Rng::new(seed ^ BOTANICAL_SEED_DOMAIN);
    let mut axes = Vec::with_capacity(
        1 + usize::from(recipe.primary_count)
            * (1 + usize::from(recipe.secondaries_per_primary)
                * (1 + usize::from(recipe.terminals_per_secondary))),
    );
    let drift = Vec3::new(rng.range(-0.38, 0.38), rng.range(-0.24, 0.24), 0.0);
    let trunk_points = std::array::from_fn(|index| {
        let t = index as f32 / (AXIS_POINTS - 1) as f32;
        let bend = t * t;
        Vec3::new(
            drift.x * bend + (t * PI).sin() * 0.12,
            drift.y * bend + (t * 1.7 * PI).sin() * 0.08,
            recipe.trunk_height_metres * t,
        )
    });
    let trunk_radii = std::array::from_fn(|index| {
        let t = index as f32 / (AXIS_POINTS - 1) as f32;
        recipe.trunk_radius_metres * (1.0 - 0.90 * t).max(0.08)
    });
    axes.push(Axis {
        parent: None,
        order: 0,
        points_metres: trunk_points,
        radii_metres: trunk_radii,
        exposure: 1.0,
        alive: true,
    });

    let storm_direction = rng.range(-PI, PI);
    for primary in 0..recipe.primary_count {
        let p = f32::from(primary);
        let count = f32::from(recipe.primary_count);
        let crown_level = p / (count - 1.0);
        let attachment = 0.25 + crown_level * 0.56;
        let azimuth = p * 2.399_963_1 + rng.range(-0.22, 0.22);
        let lee = (azimuth - storm_direction).cos();
        let length = rng.range(5.4, 7.6) * (1.0 - 0.10 * lee.max(0.0)) * (1.0 - crown_level * 0.08);
        let rise = rng.range(0.65, 1.05) + crown_level * 2.35;
        let (origin, trunk_tangent, parent_radius) = axes[0].sample(attachment);
        let horizontal = Vec3::new(azimuth.cos(), azimuth.sin(), 0.0);
        let direction =
            (horizontal * length + Vec3::Z * rise + trunk_tangent * 0.3).normalize_or(Vec3::X);
        let primary_axis = curved_axis(
            Some(0),
            1,
            origin,
            direction,
            length,
            parent_radius * 0.62,
            0.16,
            &mut rng,
            0.82 - 0.13 * lee.max(0.0),
            true,
        );
        let primary_index = push_axis(&mut axes, primary_axis)?;

        for secondary in 0..recipe.secondaries_per_primary {
            let s = f32::from(secondary);
            let secondary_attachment = 0.16
                + s / f32::from(recipe.secondaries_per_primary.max(1)) * 0.76
                + rng.range(-0.035, 0.035);
            let (secondary_origin, primary_tangent, radius) =
                axes[primary_index as usize].sample(secondary_attachment);
            let side = if secondary % 2 == 0 { 1.0 } else { -1.0 };
            let lateral = primary_tangent.cross(Vec3::Z).normalize_or(horizontal) * side;
            let secondary_direction = (primary_tangent * rng.range(0.42, 0.66)
                + lateral * rng.range(0.68, 1.02)
                + Vec3::Z * rng.range(0.02, 0.26))
            .normalize_or(Vec3::Z);
            let exposure =
                (0.46 + secondary_attachment * 0.54 + rng.range(-0.12, 0.12)).clamp(0.15, 1.0);
            let secondary_axis = curved_axis(
                Some(primary_index),
                2,
                secondary_origin,
                secondary_direction,
                rng.range(1.8, 3.0) * exposure.mul_add(0.3, 0.85),
                radius * 0.50,
                0.13,
                &mut rng,
                exposure,
                true,
            );
            let secondary_index = push_axis(&mut axes, secondary_axis)?;

            for terminal in 0..recipe.terminals_per_secondary {
                let t = f32::from(terminal);
                let terminal_attachment = 0.23
                    + t / f32::from(recipe.terminals_per_secondary.max(1)) * 0.72
                    + rng.range(-0.025, 0.025);
                let (terminal_origin, secondary_tangent, radius) =
                    axes[secondary_index as usize].sample(terminal_attachment);
                let fan = (t - (f32::from(recipe.terminals_per_secondary) - 1.0) * 0.5) * 0.24;
                let side_axis = secondary_tangent.cross(Vec3::Z).normalize_or(Vec3::X);
                let direction = (secondary_tangent * rng.range(0.36, 0.62)
                    + side_axis * fan * 1.65
                    + Vec3::Z * rng.range(-0.12, 0.34))
                .normalize_or(Vec3::Z);
                let sector = direction.y.atan2(direction.x);
                let ecological_gap = ((sector - storm_direction - 1.1).cos() > 0.87
                    && terminal % 3 == 0)
                    || rng.unit() > 0.93;
                let terminal_exposure = (exposure + rng.range(-0.18, 0.18)).clamp(0.0, 1.0);
                let alive = !ecological_gap && terminal_exposure > 0.22;
                let terminal_axis = curved_axis(
                    Some(secondary_index),
                    3,
                    terminal_origin,
                    direction,
                    rng.range(0.92, 1.52),
                    radius * 0.46,
                    0.22,
                    &mut rng,
                    terminal_exposure,
                    alive,
                );
                push_axis(&mut axes, terminal_axis)?;
            }
        }
    }
    Ok(AxisGraph { axes })
}

#[allow(clippy::too_many_arguments)]
fn curved_axis(
    parent: Option<u32>,
    order: u8,
    origin: Vec3,
    direction: Vec3,
    length: f32,
    root_radius: f32,
    tip_scale: f32,
    rng: &mut Rng,
    exposure: f32,
    alive: bool,
) -> Axis {
    let side = direction.cross(Vec3::Z).normalize_or(Vec3::X);
    let upward_fraction = match order {
        1 => 0.05,
        2 => 0.08,
        _ => 0.045,
    };
    let points_metres = std::array::from_fn(|index| {
        let t = index as f32 / (AXIS_POINTS - 1) as f32;
        let upward = Vec3::Z * length * t * t * upward_fraction;
        let sweep = side * (t * PI).sin() * length * rng.range(-0.035, 0.035);
        origin + direction * length * t + upward + sweep
    });
    let radii_metres = std::array::from_fn(|index| {
        let t = index as f32 / (AXIS_POINTS - 1) as f32;
        root_radius * (1.0 - (1.0 - tip_scale) * t.powf(0.82))
    });
    Axis {
        parent,
        order,
        points_metres,
        radii_metres,
        exposure,
        alive,
    }
}

fn push_axis(graph_axes: &mut Vec<Axis>, new_axis: Axis) -> Result<u32, String> {
    let index =
        u32::try_from(graph_axes.len()).map_err(|_| "botanical graph exceeds u32 capacity")?;
    graph_axes.push(new_axis);
    Ok(index)
}

struct FineOrgans {
    microtwigs: Mesh,
    microtwig_bark: Vec<BarkVertex>,
    leaves: Vec<LeafOrgan>,
    shoot_tips: Vec<ShootTipOrgan>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct EpicormicShoot {
    support_axis: u32,
    shoot: Axis,
    leaf_phase: f32,
    vigour: f32,
    seed: u64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct FoliageCohort {
    size_scale: f32,
    upward_bias: f32,
    sky_alignment: f32,
    roll_bias: f32,
    age_centre: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct LeafShootPlan {
    axis_id: u32,
    leaf_count: usize,
    vigour: f32,
    base_phase: f32,
    cohort: FoliageCohort,
    flush: LeafFlush,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LeafFlush {
    Previous,
    Current,
}

impl LeafShootPlan {
    fn new(
        axis_id: u32,
        leaf_count: usize,
        vigour: f32,
        base_phase: f32,
        cohort: FoliageCohort,
        flush: LeafFlush,
    ) -> Self {
        Self {
            axis_id,
            leaf_count,
            vigour,
            base_phase,
            cohort,
            flush,
        }
    }
}

fn current_flush_plan(
    axis_id: u32,
    leaf_count: usize,
    vigour: f32,
    base_phase: f32,
    cohort: FoliageCohort,
) -> LeafShootPlan {
    LeafShootPlan::new(
        axis_id,
        leaf_count,
        vigour,
        base_phase,
        cohort,
        LeafFlush::Current,
    )
}

impl LeafFlush {
    fn bounds(self) -> (f32, f32) {
        match self {
            Self::Previous => (PREVIOUS_FLUSH_START, PREVIOUS_FLUSH_END),
            Self::Current => (CURRENT_FLUSH_START, CURRENT_FLUSH_END),
        }
    }

    fn leaf_age(self, cohort: FoliageCohort, attachment: f32, jitter: f32) -> f32 {
        match self {
            Self::Previous => previous_flush_leaf_age(cohort, attachment, jitter),
            Self::Current => flush_leaf_age(cohort, attachment, jitter),
        }
    }
}

fn generate_fine_organs(
    seed: u64,
    recipe: BotanicalRecipe,
    graph: &AxisGraph,
) -> Result<FineOrgans, String> {
    let (terminal_count, capacity) = fine_organ_capacity(recipe, graph)?;
    let mut leaves = Vec::with_capacity(capacity);
    let mut shoot_tips = Vec::with_capacity(
        terminal_count
            .saturating_mul(MAX_FINE_SHOOTS_PER_TERMINAL)
            .saturating_add(MAX_EPICORMIC_SHOOTS),
    );
    let mut microtwigs = Mesh::default();
    let mut microtwig_bark = Vec::new();
    append_terminal_fine_organs(
        seed,
        recipe,
        graph,
        &mut microtwigs,
        &mut microtwig_bark,
        &mut leaves,
        &mut shoot_tips,
    )?;
    append_epicormic_organs(
        seed,
        graph,
        &mut microtwigs,
        &mut microtwig_bark,
        &mut leaves,
        &mut shoot_tips,
    )?;
    finish_fine_organs(microtwigs, microtwig_bark, leaves, shoot_tips)
}

fn append_terminal_fine_organs(
    seed: u64,
    recipe: BotanicalRecipe,
    graph: &AxisGraph,
    microtwigs: &mut Mesh,
    microtwig_bark: &mut Vec<BarkVertex>,
    leaves: &mut Vec<LeafOrgan>,
    shoot_tips: &mut Vec<ShootTipOrgan>,
) -> Result<(), String> {
    for (axis_index, &axis) in graph.axes.iter().enumerate() {
        if axis.order != 3 || !axis.alive {
            continue;
        }
        let mut rng = Rng::new(seed ^ LEAF_SEED_DOMAIN ^ axis_index as u64);
        let axis_id =
            u32::try_from(axis_index).map_err(|_| "botanical leaf axis exceeds u32 capacity")?;
        let vigour = terminal_vigour(&axis);
        let shoot_count = retained_fine_shoot_count(vigour);
        let secondary_count = retained_secondary_fine_shoot_count(vigour, shoot_count);
        let leaf_budget = retained_leaf_budget(recipe.leaves_per_terminal, vigour);
        let remainder = leaf_budget % shoot_count;
        for twig_index in 0..shoot_count {
            let twig =
                primary_fine_shoot(&axis, axis_id, twig_index, shoot_count, vigour, &mut rng);
            append_axis_sweep(microtwigs, microtwig_bark, twig)?;
            let leaves_on_twig = leaf_budget / shoot_count + usize::from(twig_index < remainder);
            let base_phase = twig_index as f32 * 2.399_963_1 + rng.range(-0.16, 0.16);
            let grows_secondary = leaves_on_twig >= MIN_LEAVES_FOR_SECONDARY_FINE_SHOOT
                && receives_secondary_fine_shoot(twig_index, shoot_count, secondary_count);
            let secondary_leaf_count = if grows_secondary {
                secondary_fine_shoot_leaf_count(leaves_on_twig)
            } else {
                0
            };
            let primary_cohort = foliage_cohort(
                foliage_cohort_seed(seed, axis_index, twig_index, false),
                vigour,
                axis.exposure,
                false,
            );
            let primary_plan = current_flush_plan(
                axis_id,
                leaves_on_twig - secondary_leaf_count,
                vigour,
                base_phase,
                primary_cohort,
            );
            append_leaves_on_shoot(leaves, &mut rng, &twig, primary_plan);
            append_previous_flush_leaves(
                leaves,
                &mut rng,
                &twig,
                leaves_on_twig,
                primary_plan,
                axis.exposure,
            );
            if grows_secondary {
                let secondary = secondary_fine_shoot(
                    &twig,
                    axis_id,
                    twig_index,
                    vigour,
                    axis.exposure,
                    &mut rng,
                );
                append_axis_sweep(microtwigs, microtwig_bark, secondary)?;
                let secondary_cohort = foliage_cohort(
                    foliage_cohort_seed(seed, axis_index, twig_index, true),
                    vigour * 0.94,
                    axis.exposure,
                    true,
                );
                let secondary_plan = current_flush_plan(
                    axis_id,
                    secondary_leaf_count,
                    vigour * 0.94,
                    base_phase + PI * 0.72,
                    secondary_cohort,
                );
                append_leaves_on_shoot(leaves, &mut rng, &secondary, secondary_plan);
                shoot_tips.push(shoot_tip_organ(
                    &secondary,
                    axis_id,
                    vigour * 0.94,
                    axis.exposure,
                    true,
                    shoot_tip_seed(seed, axis_index, twig_index, true),
                ));
            }
            shoot_tips.push(shoot_tip_organ(
                &twig,
                axis_id,
                vigour,
                axis.exposure,
                false,
                shoot_tip_seed(seed, axis_index, twig_index, false),
            ));
        }
    }
    Ok(())
}

fn append_epicormic_organs(
    seed: u64,
    graph: &AxisGraph,
    microtwigs: &mut Mesh,
    microtwig_bark: &mut Vec<BarkVertex>,
    leaves: &mut Vec<LeafOrgan>,
    shoot_tips: &mut Vec<ShootTipOrgan>,
) -> Result<(), String> {
    for epicormic in epicormic_shoots(seed, graph) {
        append_axis_sweep(microtwigs, microtwig_bark, epicormic.shoot)?;
        let mut rng = Rng::new(epicormic.seed);
        let cohort = FoliageCohort {
            size_scale: rng.range(0.88, 0.98),
            upward_bias: rng.range(0.015, 0.070),
            sky_alignment: rng.range(0.78, 0.90),
            roll_bias: rng.range(-0.16, 0.16),
            age_centre: rng.range(0.34, 0.52),
        };
        append_leaves_on_shoot(
            leaves,
            &mut rng,
            &epicormic.shoot,
            current_flush_plan(
                epicormic.support_axis,
                EPICORMIC_LEAVES_PER_SHOOT,
                epicormic.vigour,
                epicormic.leaf_phase,
                cohort,
            ),
        );
        shoot_tips.push(shoot_tip_organ(
            &epicormic.shoot,
            epicormic.support_axis,
            epicormic.vigour,
            0.34,
            false,
            epicormic.seed ^ SHOOT_TIP_SEED_DOMAIN,
        ));
    }
    Ok(())
}

fn epicormic_shoots(seed: u64, graph: &AxisGraph) -> Vec<EpicormicShoot> {
    let Some(&trunk) = graph.axes.first() else {
        return Vec::new();
    };
    let mut rng = Rng::new(seed ^ EPICORMIC_SEED_DOMAIN);
    let cluster_count = if rng.unit() < 0.22 { 2 } else { 1 };
    let primary_attachment = rng.range(0.12, 0.27);
    let primary_phase = rng.range(0.0, TAU);
    let secondary_side = if rng.unit() < 0.5 { -1.0 } else { 1.0 };
    let mut shoots = Vec::with_capacity(MAX_EPICORMIC_SHOOTS);
    for cluster in 0..cluster_count {
        let attachment = if cluster == 0 {
            primary_attachment
        } else {
            (primary_attachment + secondary_side * rng.range(0.08, 0.13)).clamp(0.08, 0.34)
        };
        let (centre, tangent, trunk_radius) = trunk.sample(attachment);
        let reference = if tangent.z.abs() < 0.88 {
            Vec3::Z
        } else {
            Vec3::X
        };
        let radial = tangent.cross(reference).normalize_or(Vec3::X);
        let binormal = tangent.cross(radial).normalize_or(Vec3::Y);
        let cluster_phase = if cluster == 0 {
            primary_phase
        } else {
            primary_phase + secondary_side * rng.range(0.45, 1.05)
        };
        let shoot_count = if rng.unit() < 0.28 { 2 } else { 1 };
        for shoot_index in 0..shoot_count {
            let phase =
                cluster_phase + shoot_index as f32 * rng.range(0.52, 0.78) + rng.range(-0.10, 0.10);
            let fan = radial * phase.cos() + binormal * phase.sin();
            let surface_origin = centre
                + fan * trunk_radius * rng.range(0.72, 0.84)
                + tangent * rng.range(-0.025, 0.025);
            let direction = (fan * rng.range(0.74, 0.90)
                + tangent * rng.range(0.18, 0.32)
                + Vec3::Z * rng.range(0.15, 0.30))
            .normalize_or(fan);
            let vigour = rng.range(0.38, 0.58);
            let root_radius = rng.range(0.006, 0.010);
            let shoot = curved_axis(
                Some(0),
                4,
                surface_origin,
                direction,
                rng.range(0.30, 0.52),
                root_radius,
                rng.range(0.12, 0.20),
                &mut rng,
                0.34,
                true,
            );
            shoots.push(EpicormicShoot {
                support_axis: 0,
                shoot,
                leaf_phase: phase + PI * 0.5,
                vigour,
                seed: rng.next_u64(),
            });
        }
    }
    shoots.truncate(MAX_EPICORMIC_SHOOTS);
    shoots
}

fn finish_fine_organs(
    mut microtwigs: Mesh,
    microtwig_bark: Vec<BarkVertex>,
    mut leaves: Vec<LeafOrgan>,
    shoot_tips: Vec<ShootTipOrgan>,
) -> Result<FineOrgans, String> {
    estimate_leaf_exposure(&mut leaves)?;
    microtwigs.calculate_normals();
    Ok(FineOrgans {
        microtwigs,
        microtwig_bark,
        leaves,
        shoot_tips,
    })
}

fn fine_organ_capacity(
    recipe: BotanicalRecipe,
    graph: &AxisGraph,
) -> Result<(usize, usize), String> {
    let terminal_count = graph
        .axes
        .iter()
        .filter(|axis| axis.order == 3 && axis.alive)
        .count();
    let leaves_per_terminal = usize::from(recipe.leaves_per_terminal)
        .checked_add(MAX_FINE_SHOOTS_PER_TERMINAL * MAX_PREVIOUS_FLUSH_LEAVES_PER_SHOOT)
        .ok_or("botanical leaf capacity exceeds addressable memory")?;
    let terminal_leaf_capacity = terminal_count
        .checked_mul(leaves_per_terminal)
        .ok_or("botanical leaf count exceeds addressable memory")?;
    let leaf_capacity = terminal_leaf_capacity
        .checked_add(MAX_EPICORMIC_LEAVES)
        .ok_or("botanical epicormic leaf count exceeds addressable memory")?;
    Ok((terminal_count, leaf_capacity))
}

fn shoot_tip_organ(
    shoot: &Axis,
    axis_id: u32,
    vigour: f32,
    exposure: f32,
    secondary: bool,
    seed: u64,
) -> ShootTipOrgan {
    let mut rng = Rng::new(seed);
    let tip = shoot.points_metres[AXIS_POINTS - 1];
    let previous = shoot.points_metres[AXIS_POINTS - 2];
    let direction = (tip - previous).normalize_or(Vec3::Z);
    let stress = 1.0 - vigour.mul_add(0.62, exposure * 0.38).clamp(0.0, 1.0);
    let roll = rng.unit();
    let broken_threshold = 0.025 + stress * 0.085 + if secondary { 0.018 } else { 0.0 };
    let dormant_threshold = broken_threshold + 0.14 + stress * 0.30;
    let state = if roll < broken_threshold {
        ShootTipState::Broken
    } else if roll < dormant_threshold {
        ShootTipState::DormantBud
    } else {
        ShootTipState::ActiveBud
    };
    let (length_metres, radius_scale) = match state {
        ShootTipState::ActiveBud => (rng.range(0.030, 0.046), rng.range(1.05, 1.30)),
        ShootTipState::DormantBud => (rng.range(0.020, 0.034), rng.range(0.90, 1.12)),
        ShootTipState::Broken => (rng.range(0.014, 0.026), rng.range(0.72, 0.96)),
    };
    ShootTipOrgan {
        axis: axis_id,
        base_metres: tip,
        direction,
        length_metres,
        radius_metres: (shoot.radii_metres[AXIS_POINTS - 1] * radius_scale).clamp(0.003, 0.014),
        state,
        variation: rng.range(0.0, TAU),
    }
}

fn shoot_tip_seed(seed: u64, axis_index: usize, shoot_index: usize, secondary: bool) -> u64 {
    seed ^ SHOOT_TIP_SEED_DOMAIN
        ^ (axis_index as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15)
        ^ (shoot_index as u64).wrapping_mul(0xc2b2_ae3d_27d4_eb4f)
        ^ if secondary { 0xa4b1_c3d7_e9f2_560b } else { 0 }
}

fn foliage_cohort_seed(seed: u64, axis_index: usize, shoot_index: usize, secondary: bool) -> u64 {
    seed ^ FOLIAGE_COHORT_SEED_DOMAIN
        ^ (axis_index as u64).wrapping_mul(0xd6e8_feb8_6659_fd93)
        ^ (shoot_index as u64).wrapping_mul(0xa5a3_564e_27f8_864f)
        ^ if secondary { 0x9e37_79b9_7f4a_7c15 } else { 0 }
}

fn foliage_cohort(seed: u64, vigour: f32, exposure: f32, secondary: bool) -> FoliageCohort {
    let mut rng = Rng::new(seed);
    let exposure = exposure.clamp(0.0, 1.0);
    let vigour = vigour.clamp(0.0, 1.0);
    let size_scale = (1.08 - exposure * 0.15 + rng.range(-0.070, 0.070)
        - if secondary { 0.055 } else { 0.0 })
    .clamp(0.82, 1.13);
    let upward_bias = (exposure * 0.085 + rng.range(-0.040, 0.055)).clamp(-0.04, 0.14);
    let sky_alignment = (0.94 - exposure * 0.18 + rng.range(-0.040, 0.040)).clamp(0.68, 0.98);
    let roll_bias = rng.range(-0.22, 0.22) * exposure.mul_add(0.35, 0.65);
    let age_centre = (0.53 + (1.0 - vigour) * 0.12 + rng.range(-0.10, 0.10)
        - if secondary { 0.055 } else { 0.0 })
    .clamp(0.30, 0.76);
    FoliageCohort {
        size_scale,
        upward_bias,
        sky_alignment,
        roll_bias,
        age_centre,
    }
}

fn previous_foliage_cohort(current: FoliageCohort) -> FoliageCohort {
    FoliageCohort {
        size_scale: (current.size_scale * 0.985).clamp(0.82, 1.10),
        upward_bias: (current.upward_bias * 0.72 - 0.018).clamp(-0.06, 0.10),
        sky_alignment: (current.sky_alignment * 0.96).clamp(0.66, 0.94),
        roll_bias: current.roll_bias * 0.82,
        age_centre: (current.age_centre + 0.30).clamp(0.72, 0.94),
    }
}

fn primary_fine_shoot(
    terminal: &Axis,
    terminal_id: u32,
    shoot_index: usize,
    shoot_count: usize,
    vigour: f32,
    rng: &mut Rng,
) -> Axis {
    let station = shoot_index as f32 / shoot_count.saturating_sub(1).max(1) as f32;
    let attachment = (0.38 + station * 0.55 + rng.range(-0.022, 0.022)).clamp(0.34, 0.96);
    let (origin, tangent, parent_radius) = terminal.sample(attachment);
    let phase = shoot_index as f32 * 2.399_963_1 + rng.range(-0.20, 0.20);
    let radial = tangent.cross(Vec3::Z).normalize_or(Vec3::X);
    let binormal = tangent.cross(radial).normalize_or(Vec3::Y);
    let fan = radial * phase.cos() + binormal * phase.sin();
    let direction = (tangent * rng.range(0.28, 0.46)
        + fan * rng.range(0.72, 0.94)
        + Vec3::Z * rng.range(0.08, 0.30))
    .normalize_or(fan);
    let length_response = vigour.mul_add(0.48, 0.72);
    let radius_response = vigour.mul_add(0.35, 0.72);
    let mut shoot = curved_axis(
        Some(terminal_id),
        4,
        origin,
        direction,
        rng.range(0.38, 0.60) * length_response,
        (parent_radius * rng.range(0.18, 0.26) * radius_response).clamp(0.005, 0.014),
        vigour.mul_add(0.13, 0.12),
        rng,
        terminal.exposure,
        true,
    );
    apply_seasonal_turn(&mut shoot, radial, binormal, phase, vigour);
    shoot
}

fn apply_seasonal_turn(shoot: &mut Axis, radial: Vec3, binormal: Vec3, phase: f32, vigour: f32) {
    let sideways = (radial * -phase.sin() + binormal * phase.cos()).normalize_or(radial);
    let length = axis_length(shoot);
    let renewal = sideways * length * vigour.mul_add(0.018, 0.034) + Vec3::Z * length * 0.012;
    for point_index in 2..AXIS_POINTS {
        let progress = (point_index as f32 / (AXIS_POINTS - 1) as f32 - 0.38) / 0.62;
        shoot.points_metres[point_index] += renewal * smoothstep(progress.clamp(0.0, 1.0));
    }
}

fn secondary_fine_shoot(
    primary: &Axis,
    terminal_id: u32,
    shoot_index: usize,
    vigour: f32,
    exposure: f32,
    rng: &mut Rng,
) -> Axis {
    let attachment = rng.range(0.42, 0.66);
    let (origin, tangent, parent_radius) = primary.sample(attachment);
    let radial = tangent.cross(Vec3::Z).normalize_or(Vec3::X);
    let binormal = tangent.cross(radial).normalize_or(Vec3::Y);
    let handedness = if shoot_index.is_multiple_of(2) {
        1.0
    } else {
        -1.0
    };
    let phase = shoot_index as f32 * 2.399_963_1 + handedness * 1.08 + rng.range(-0.18, 0.18);
    let fan = radial * phase.cos() + binormal * phase.sin();
    let direction = (tangent * rng.range(0.46, 0.62)
        + fan * rng.range(0.68, 0.86)
        + Vec3::Z * rng.range(0.08, 0.22))
    .normalize_or(fan);
    curved_axis(
        Some(terminal_id),
        5,
        origin,
        direction,
        axis_length(primary) * rng.range(0.42, 0.58) * vigour.mul_add(0.20, 0.80),
        (parent_radius * rng.range(0.42, 0.56)).clamp(0.003, 0.0075),
        rng.range(0.10, 0.17),
        rng,
        exposure,
        true,
    )
}

fn append_leaves_on_shoot(
    leaves: &mut Vec<LeafOrgan>,
    rng: &mut Rng,
    shoot: &Axis,
    plan: LeafShootPlan,
) {
    let node_count = plan.leaf_count.div_ceil(2);
    let (flush_start, flush_end) = plan.flush.bounds();
    let mut paired_node_jitter = 0.0;
    for local_index in 0..plan.leaf_count {
        let sampled_jitter = rng.range(-0.010, 0.010);
        if local_index.is_multiple_of(2) {
            paired_node_jitter = sampled_jitter;
        }
        let attachment = clustered_decussate_attachment(
            local_index,
            node_count,
            flush_start,
            flush_end,
            paired_node_jitter,
        );
        let (position, tangent, twig_radius) = shoot.sample(attachment);
        let phase = decussate_phase(plan.base_phase, local_index) + rng.range(-0.09, 0.09);
        let radial = tangent.cross(Vec3::Z).normalize_or(Vec3::X);
        let binormal = tangent.cross(radial).normalize_or(Vec3::Y);
        let fan = radial * phase.cos() + binormal * phase.sin();
        let direction = (tangent * rng.range(0.40, 0.56)
            + fan * rng.range(0.74, 0.92)
            + Vec3::Z * (rng.range(0.08, 0.20) + plan.cohort.upward_bias))
            .normalize_or(fan);
        let normal = pohutukawa_leaf_normal(direction, tangent, local_index, plan.cohort, rng);
        let paired_offset = if local_index.is_multiple_of(2) {
            -0.006
        } else {
            0.006
        };
        let productive = plan.vigour.mul_add(0.30, 0.78);
        let age = plan
            .flush
            .leaf_age(plan.cohort, attachment, rng.range(-0.065, 0.065));
        let variation = rng.range(0.0, TAU);
        let archetype = leaf_archetype(age, variation);
        let (length_metres, width_metres) =
            leaf_dimensions(rng, productive, archetype, plan.cohort.size_scale);
        leaves.push(LeafOrgan {
            axis: plan.axis_id,
            blade_base_metres: position + fan * (twig_radius + 0.006) + tangent * paired_offset,
            direction,
            normal,
            length_metres,
            width_metres,
            archetype,
            age,
            light_exposure: 0.0,
            variation,
        });
    }
}

fn pohutukawa_leaf_normal(
    direction: Vec3,
    tangent: Vec3,
    local_index: usize,
    cohort: FoliageCohort,
    rng: &mut Rng,
) -> Vec3 {
    let sky = (Vec3::Z - direction * direction.z).normalize_or(Vec3::Z);
    let lateral = direction
        .cross(sky)
        .normalize_or(tangent.cross(direction).normalize_or(Vec3::Y));
    // Mature pōhutukawa foliage forms dense, wind-combed terminal sprays. The
    // blade planes are biased toward the sky but are not a horizontal shell:
    // opposite leaves open into shallow alternating Vs, with exposed cohorts
    // carrying the largest inclination. This distribution became necessary
    // once the renderer stopped swapping blade width and surface relief.
    let pair_sign = if local_index.is_multiple_of(2) {
        -1.0
    } else {
        1.0
    };
    let alternate_sign = if (local_index / 2).is_multiple_of(2) {
        -1.0
    } else {
        1.0
    };
    let inclination = (0.82
        + (1.0 - cohort.sky_alignment) * 1.82
        + cohort.roll_bias.abs() * 0.42
        + rng.range(-0.14, 0.28))
    .clamp(0.65, 1.44);
    let signed_inclination = pair_sign * inclination + alternate_sign * cohort.roll_bias * 0.42;
    (sky * signed_inclination.cos() + lateral * signed_inclination.sin()).normalize_or(sky)
}

fn append_previous_flush_leaves(
    leaves: &mut Vec<LeafOrgan>,
    rng: &mut Rng,
    shoot: &Axis,
    current_leaf_count: usize,
    current: LeafShootPlan,
    exposure: f32,
) {
    let leaf_count =
        retained_previous_flush_leaf_count(current_leaf_count, current.vigour, exposure);
    if leaf_count == 0 {
        return;
    }
    let previous = LeafShootPlan::new(
        current.axis_id,
        leaf_count,
        current.vigour,
        current.base_phase + PI * 0.5,
        previous_foliage_cohort(current.cohort),
        LeafFlush::Previous,
    );
    append_leaves_on_shoot(leaves, rng, shoot, previous);
}

fn axis_length(axis: &Axis) -> f32 {
    axis.points_metres
        .windows(2)
        .map(|points| points[0].distance(points[1]))
        .sum()
}

fn terminal_vigour(axis: &Axis) -> f32 {
    let length = axis_length(axis);
    let length_response = ((length - 0.48) / 0.92).clamp(0.0, 1.0);
    let radius_response = ((axis.radii_metres[0] - 0.014) / 0.044).clamp(0.0, 1.0);
    (axis.exposure.clamp(0.0, 1.0) * 0.58 + length_response * 0.27 + radius_response * 0.15)
        .clamp(0.0, 1.0)
}

fn retained_fine_shoot_count(vigour: f32) -> usize {
    (MIN_FINE_SHOOTS_PER_TERMINAL as f32
        + (MAX_FINE_SHOOTS_PER_TERMINAL - MIN_FINE_SHOOTS_PER_TERMINAL) as f32 * vigour)
        .round()
        .clamp(
            MIN_FINE_SHOOTS_PER_TERMINAL as f32,
            MAX_FINE_SHOOTS_PER_TERMINAL as f32,
        ) as usize
}

fn retained_secondary_fine_shoot_count(vigour: f32, primary_count: usize) -> usize {
    if vigour < SECONDARY_FINE_SHOOT_MIN_VIGOUR || primary_count < 2 {
        return 0;
    }
    let response = ((vigour - SECONDARY_FINE_SHOOT_MIN_VIGOUR)
        / (1.0 - SECONDARY_FINE_SHOOT_MIN_VIGOUR))
        .clamp(0.0, 1.0);
    (primary_count as f32 * 0.46 * response)
        .round()
        .max(1.0)
        .min((primary_count / 2).min(MAX_SECONDARY_FINE_SHOOTS_PER_TERMINAL) as f32) as usize
}

fn receives_secondary_fine_shoot(
    shoot_index: usize,
    primary_count: usize,
    secondary_count: usize,
) -> bool {
    (0..secondary_count).any(|slot| {
        let selected = ((slot * 2 + 1) * primary_count / (secondary_count * 2))
            .min(primary_count.saturating_sub(1));
        selected == shoot_index
    })
}

fn secondary_fine_shoot_leaf_count(primary_leaf_count: usize) -> usize {
    if primary_leaf_count >= 8 { 4 } else { 2 }
}

fn retained_leaf_budget(maximum: u8, vigour: f32) -> usize {
    (f32::from(maximum) * vigour.mul_add(0.42, 0.58))
        .round()
        .clamp(8.0, f32::from(maximum)) as usize
}

fn retained_previous_flush_leaf_count(
    current_leaf_count: usize,
    vigour: f32,
    exposure: f32,
) -> usize {
    if current_leaf_count < 4 {
        return 0;
    }
    let retention = vigour.clamp(0.0, 1.0) * 0.68 + (1.0 - exposure.clamp(0.0, 1.0)) * 0.32;
    match retention {
        value if value < 0.34 => 0,
        value if value < 0.67 => 2,
        _ => MAX_PREVIOUS_FLUSH_LEAVES_PER_SHOOT,
    }
}

#[cfg(test)]
fn decussate_attachment(local_index: usize, node_count: usize) -> f32 {
    decussate_attachment_in_range(
        local_index,
        node_count,
        CURRENT_FLUSH_START,
        CURRENT_FLUSH_END,
    )
}

fn decussate_attachment_in_range(
    local_index: usize,
    node_count: usize,
    start: f32,
    end: f32,
) -> f32 {
    if node_count <= 1 {
        return f32::midpoint(start, end);
    }
    let node = local_index / 2;
    let station = node as f32 / node_count.saturating_sub(1).max(1) as f32;
    let growth_progress = 1.0 - (1.0 - station).powf(1.38);
    (end - start).mul_add(growth_progress, start)
}

fn clustered_decussate_attachment(
    local_index: usize,
    node_count: usize,
    start: f32,
    end: f32,
    node_jitter: f32,
) -> f32 {
    let attachment = decussate_attachment_in_range(local_index, node_count, start, end);
    if node_count <= 1 {
        return attachment;
    }
    let node = local_index / 2;
    let station = node as f32 / node_count.saturating_sub(1) as f32;
    let interior = (station * PI).sin().max(0.0);
    (attachment + node_jitter.clamp(-0.010, 0.010) * interior).clamp(start, end)
}

fn flush_leaf_age(cohort: FoliageCohort, attachment: f32, jitter: f32) -> f32 {
    let progression =
        (attachment - CURRENT_FLUSH_START) / (CURRENT_FLUSH_END - CURRENT_FLUSH_START);
    (cohort.age_centre + (0.5 - progression) * 0.13 + jitter).clamp(0.08, 0.98)
}

fn previous_flush_leaf_age(cohort: FoliageCohort, attachment: f32, jitter: f32) -> f32 {
    let progression =
        (attachment - PREVIOUS_FLUSH_START) / (PREVIOUS_FLUSH_END - PREVIOUS_FLUSH_START);
    (cohort.age_centre + (0.5 - progression) * 0.055 + jitter * 0.55).clamp(0.66, 1.0)
}

fn decussate_phase(base_phase: f32, local_index: usize) -> f32 {
    let node = local_index / 2;
    let opposite = if local_index.is_multiple_of(2) {
        0.0
    } else {
        PI
    };
    base_phase + node as f32 * PI * 0.5 + opposite
}

fn leaf_archetype(age: f32, variation: f32) -> u8 {
    let phase = (variation / TAU).rem_euclid(1.0);
    if age < 0.26 {
        if phase < 0.5 { 1 } else { 5 }
    } else if age > 0.78 {
        match phase {
            value if value < 0.25 => 2,
            value if value < 0.50 => 6,
            value if value < 0.75 => 3,
            _ => 7,
        }
    } else if phase < 0.5 {
        0
    } else {
        4
    }
}

fn leaf_dimensions(rng: &mut Rng, productive: f32, archetype: u8, cohort_scale: f32) -> (f32, f32) {
    let length = (rng.range(
        POHUTUKAWA_LEAF_LENGTH_RANGE_METRES.0,
        POHUTUKAWA_LEAF_LENGTH_RANGE_METRES.1,
    ) * productive
        * cohort_scale)
        .clamp(
            POHUTUKAWA_LEAF_LENGTH_BOUNDS_METRES.0,
            POHUTUKAWA_LEAF_LENGTH_BOUNDS_METRES.1,
        );
    let aspect_ratio = match archetype {
        1 | 5 => rng.range(2.05, 2.55),
        2 | 6 => rng.range(1.70, 2.15),
        3 | 7 => rng.range(1.80, 2.35),
        _ => rng.range(1.85, 2.35),
    };
    (length, (length / aspect_ratio).clamp(0.018, 0.060))
}

#[derive(Debug)]
struct CanopyLightField {
    minimum: Vec3,
    dimensions: [usize; 3],
    density: Vec<f32>,
}

impl CanopyLightField {
    fn from_leaves(leaves: &[LeafOrgan]) -> Result<Self, String> {
        let (minimum, maximum) = leaves.iter().map(leaf_centre).fold(
            (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY)),
            |(minimum, maximum), centre| (minimum.min(centre), maximum.max(centre)),
        );
        if leaves.is_empty() {
            return Ok(Self {
                minimum: Vec3::ZERO,
                dimensions: [1; 3],
                density: vec![0.0],
            });
        }
        let margin = Vec3::splat(CANOPY_LIGHT_CELL_METRES);
        let minimum = minimum - margin;
        let extent = maximum - minimum + margin;
        let dimensions = [
            grid_dimension(extent.x),
            grid_dimension(extent.y),
            grid_dimension(extent.z),
        ];
        let cell_count = dimensions
            .into_iter()
            .try_fold(1_usize, usize::checked_mul)
            .ok_or("botanical canopy light field exceeds addressable memory")?;
        let mut field = Self {
            minimum,
            dimensions,
            density: vec![0.0; cell_count],
        };
        for leaf in leaves {
            let projected_area = leaf.length_metres * leaf.width_metres * 0.72;
            if let Some(index) = field.cell_index(leaf_centre(leaf)) {
                field.density[index] += projected_area / CANOPY_LIGHT_CELL_METRES.powi(2);
            }
        }
        Ok(field)
    }

    fn cell_index(&self, point: Vec3) -> Option<usize> {
        let local = (point - self.minimum) / CANOPY_LIGHT_CELL_METRES;
        if local.x < 0.0 || local.y < 0.0 || local.z < 0.0 {
            return None;
        }
        let cell = [
            local.x.floor() as usize,
            local.y.floor() as usize,
            local.z.floor() as usize,
        ];
        (cell[0] < self.dimensions[0]
            && cell[1] < self.dimensions[1]
            && cell[2] < self.dimensions[2])
            .then(|| cell[0] + self.dimensions[0] * (cell[1] + self.dimensions[1] * cell[2]))
    }

    fn sky_visibility(&self, origin: Vec3) -> f32 {
        let origin_cell = self.cell_index(origin);
        let visibility = CANOPY_SKY_DIRECTIONS
            .iter()
            .map(|&direction| {
                let mut optical_depth = 0.0;
                let mut previous_cell = origin_cell;
                for step in 1..=CANOPY_LIGHT_RAY_STEPS {
                    let distance = step as f32 * CANOPY_LIGHT_STEP_METRES;
                    let Some(cell) = self.cell_index(origin + direction * distance) else {
                        break;
                    };
                    if Some(cell) == origin_cell || Some(cell) == previous_cell {
                        continue;
                    }
                    optical_depth += self.density[cell];
                    previous_cell = Some(cell);
                }
                (-optical_depth * CANOPY_EXTINCTION).exp()
            })
            .sum::<f32>()
            / CANOPY_SKY_DIRECTIONS.len() as f32;
        visibility.clamp(0.0, 1.0)
    }
}

fn grid_dimension(extent: f32) -> usize {
    (extent / CANOPY_LIGHT_CELL_METRES).ceil().max(1.0) as usize
}

fn leaf_centre(leaf: &LeafOrgan) -> Vec3 {
    leaf.blade_base_metres + leaf.direction * (leaf.length_metres * 0.52)
}

fn estimate_leaf_exposure(leaves: &mut [LeafOrgan]) -> Result<(), String> {
    let field = CanopyLightField::from_leaves(leaves)?;
    for leaf in leaves {
        let sky_visibility = field.sky_visibility(leaf_centre(leaf));
        let face_to_sky = leaf.normal.z.abs().sqrt();
        leaf.light_exposure = (sky_visibility * 0.88 + face_to_sky * 0.12).clamp(0.0, 1.0);
    }
    Ok(())
}

fn generate_foliage_pads(graph: &AxisGraph, leaves: &[LeafOrgan]) -> Vec<FoliagePad> {
    graph
        .axes
        .iter()
        .enumerate()
        .filter(|(_, axis)| axis.order == 3 && axis.alive)
        .filter_map(|(axis_index, axis)| {
            let axis_id = u32::try_from(axis_index).ok()?;
            let (origin, direction, _) = axis.sample(0.68);
            let side = direction.cross(Vec3::Z).normalize_or(Vec3::X);
            let normal = side.cross(direction).normalize_or(Vec3::Z);
            let organs = leaves.iter().filter(|leaf| leaf.axis == axis_id);
            let (count, minimum, maximum, age_total, exposure_total, variation_total) = organs
                .fold(
                    (
                        0_u32,
                        Vec3::splat(f32::INFINITY),
                        Vec3::splat(f32::NEG_INFINITY),
                        0.0_f32,
                        0.0_f32,
                        0.0_f32,
                    ),
                    |(count, minimum, maximum, age, exposure, variation), leaf| {
                        let tip = leaf.blade_base_metres + leaf.direction * leaf.length_metres;
                        let local = |point: Vec3| {
                            let offset = point - origin;
                            Vec3::new(offset.dot(direction), offset.dot(normal), offset.dot(side))
                        };
                        let base = local(leaf.blade_base_metres);
                        let tip = local(tip);
                        (
                            count + 1,
                            minimum.min(base.min(tip)),
                            maximum.max(base.max(tip)),
                            age + leaf.age,
                            exposure + leaf.light_exposure,
                            variation + leaf.variation,
                        )
                    },
                );
            (count > 0).then(|| {
                let inverse = 1.0 / count as f32;
                let local_centre = (minimum + maximum) * 0.5;
                FoliagePad {
                    axis: axis_id,
                    centre_metres: origin
                        + direction * local_centre.x
                        + normal * local_centre.y
                        + side * local_centre.z,
                    direction,
                    normal,
                    half_extents_metres: ((maximum - minimum) * 0.54)
                        .max(Vec3::new(0.52, 0.28, 0.34)),
                    archetype: (axis_id as usize % FOLIAGE_PAD_ARCHETYPE_COUNT) as u8,
                    mean_age: age_total * inverse,
                    light_exposure: exposure_total * inverse,
                    density: (count as f32 / 52.0).clamp(0.25, 1.0),
                    variation: variation_total * inverse,
                }
            })
        })
        .collect()
}

fn generate_wood(seed: u64, graph: &AxisGraph) -> Result<(Mesh, Vec<BarkVertex>, Mesh), String> {
    let mut mesh = Mesh::default();
    let mut bark = Vec::new();
    for run in branch_runs(graph) {
        append_branch_run(&mut mesh, &mut bark, &run)?;
    }
    let mut scars = Mesh::default();
    append_persistent_dead_stubs(seed, graph, &mut mesh, &mut bark, &mut scars)?;
    mesh.calculate_normals();
    Ok((mesh, bark, scars))
}

fn append_persistent_dead_stubs(
    seed: u64,
    graph: &AxisGraph,
    wood: &mut Mesh,
    bark: &mut Vec<BarkVertex>,
    scars: &mut Mesh,
) -> Result<(), String> {
    let mut candidates = graph
        .axes
        .iter()
        .enumerate()
        .filter(|(_, axis)| axis.alive && axis.order == 1)
        .map(|(axis_index, axis)| {
            let variation = scaffold_history_variation(seed, axis_index);
            let attachment = variation.mul_add(0.40, 0.32);
            let support_radius = axis.sample(attachment).2;
            let score = support_radius * variation.mul_add(0.20, 0.80);
            (
                score,
                variation,
                axis_index,
                scaffold_lineage(graph, axis_index),
                support_radius,
            )
        })
        .filter_map(
            |(score, variation, axis_index, primary_lineage, support_radius)| {
                (support_radius >= MIN_HISTORY_SUPPORT_RADIUS_METRES).then_some((
                    score,
                    variation,
                    axis_index,
                    primary_lineage,
                ))
            },
        )
        .collect::<Vec<_>>();
    candidates.sort_by(|left, right| right.0.total_cmp(&left.0));
    let mut used_lineages = vec![false; graph.axes.len()];
    let mut retained = 0_usize;
    for (_, variation, axis_index, primary_lineage) in candidates {
        if retained == MAX_PERSISTENT_DEAD_STUBS {
            break;
        }
        if used_lineages[primary_lineage] {
            continue;
        }
        let stub = persistent_dead_stub(graph.axes[axis_index], axis_index, variation);
        let run = BranchRun {
            samples: stub
                .points_metres
                .into_iter()
                .zip(stub.radii_metres)
                .map(|(position, radius)| BranchSample { position, radius })
                .collect(),
            order: stub.order,
            cap_base: false,
            cap_tip: false,
            axis_count: 1,
            uv_offset: variation,
        };
        append_branch_run(wood, bark, &run)?;
        append_scaffold_scar(scars, stub, variation)?;
        used_lineages[primary_lineage] = true;
        retained += 1;
    }
    Ok(())
}

fn scaffold_lineage(graph: &AxisGraph, mut axis_index: usize) -> usize {
    let mut lineage = axis_index;
    while let Some(parent) = graph.axes[axis_index].parent.map(|parent| parent as usize) {
        if graph.axes[parent].order == 0 {
            break;
        }
        lineage = parent;
        axis_index = parent;
    }
    lineage
}

fn scaffold_history_variation(seed: u64, axis_index: usize) -> f32 {
    let mut rng = Rng::new(
        seed ^ SCAFFOLD_HISTORY_SEED_DOMAIN
            ^ (axis_index as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15),
    );
    rng.unit()
}

fn persistent_dead_stub(support: Axis, support_index: usize, variation: f32) -> Axis {
    let attachment = variation.mul_add(0.40, 0.32);
    let (origin, tangent, support_radius) = support.sample(attachment);
    let reference = if tangent.z.abs() < 0.86 {
        Vec3::Z
    } else {
        Vec3::X
    };
    let radial = tangent.cross(reference).normalize_or(Vec3::X);
    let binormal = tangent.cross(radial).normalize_or(Vec3::Y);
    let phase = variation.mul_add(TAU, support_index as f32 * 2.399_963_1);
    let fan = radial * phase.cos() + binormal * phase.sin();
    let direction = (fan * 0.98
        + tangent * variation.mul_add(0.18, -0.10)
        + Vec3::Z * variation.mul_add(0.18, -0.14))
    .normalize_or(fan);
    let base_radius = (support_radius * variation.mul_add(0.10, 0.30))
        .clamp(MIN_DEAD_STUB_ROOT_RADIUS_METRES, 0.060);
    let length = (base_radius * variation.mul_add(1.2, 3.2)).clamp(0.10, 0.24);
    let bend = (binormal * (variation - 0.5) - Vec3::Z * 0.12) * length * 0.025;
    let points_metres = std::array::from_fn(|point_index| {
        let local = point_index as f32 / (AXIS_POINTS - 1) as f32;
        origin + direction * length * local + bend * (local * PI).sin()
    });
    let radii_metres = std::array::from_fn(|point_index| {
        let local = point_index as f32 / (AXIS_POINTS - 1) as f32;
        base_radius * (1.0 - local * (0.14 + variation * 0.08))
    });
    Axis {
        parent: Some(support_index as u32),
        order: 1,
        points_metres,
        radii_metres,
        exposure: support.exposure,
        alive: true,
    }
}

fn append_scaffold_scar(mesh: &mut Mesh, stub: Axis, variation: f32) -> Result<(), String> {
    let tip = stub.points_metres[AXIS_POINTS - 1];
    let direction = (tip - stub.points_metres[AXIS_POINTS - 2]).normalize_or(Vec3::Z);
    let frame = initial_frame(direction);
    let radius = stub.radii_metres[AXIS_POINTS - 1] * 0.97;
    let base = u32::try_from(mesh.vertices.len()).map_err(|_| "botanical scars exceed u32")?;
    mesh.vertices.reserve(SCAR_VERTEX_COUNT);
    mesh.normals.reserve(SCAR_VERTEX_COUNT);
    mesh.uv.reserve(SCAR_VERTEX_COUNT);
    mesh.triangles.reserve(SCAR_RING_SIDES * 3);
    mesh.vertices
        .push(tip - direction * radius * (0.105 + variation * 0.035));
    mesh.normals.push(direction);
    mesh.uv.push(Vec2::new(0.5, 0.5));
    for side in 0..=SCAR_RING_SIDES {
        let angle = side as f32 / SCAR_RING_SIDES as f32 * TAU;
        let irregularity = 1.0
            + (angle * 3.0 + variation * TAU).sin() * 0.075
            + (angle * 7.0 - variation * PI).sin() * 0.035;
        let edge_depth = radius
            * (0.025
                + (angle * 3.0 - variation * TAU).sin() * 0.035
                + (angle * 5.0 + variation * PI).sin() * 0.020);
        let radial = frame.0 * angle.cos() + frame.1 * angle.sin();
        mesh.vertices
            .push(tip + direction * edge_depth + radial * radius * irregularity);
        mesh.normals.push(direction);
        mesh.uv.push(Vec2::new(
            angle.cos().mul_add(0.48, 0.5),
            angle.sin().mul_add(0.48, 0.5),
        ));
    }
    for side in 0..SCAR_RING_SIDES {
        mesh.triangles
            .extend([base, base + side as u32 + 1, base + side as u32 + 2]);
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct BranchSample {
    position: Vec3,
    radius: f32,
}

#[derive(Clone, Debug, PartialEq)]
struct BranchRun {
    samples: Vec<BranchSample>,
    order: u8,
    cap_base: bool,
    cap_tip: bool,
    axis_count: usize,
    uv_offset: f32,
}

fn branch_runs(graph: &AxisGraph) -> Vec<BranchRun> {
    let continuations = dominant_continuations(graph);
    let mut continued_axes = vec![false; graph.axes.len()];
    continuations
        .iter()
        .flatten()
        .for_each(|&index| continued_axes[index] = true);

    graph
        .axes
        .iter()
        .enumerate()
        .filter(|(index, axis)| axis.alive && !continued_axes[*index])
        .map(|(start, axis)| collect_branch_run(graph, &continuations, start, *axis))
        .collect()
}

fn dominant_continuations(graph: &AxisGraph) -> Vec<Option<usize>> {
    graph
        .axes
        .iter()
        .enumerate()
        .map(|(parent_index, parent)| {
            if !parent.alive {
                return None;
            }
            let parent_tip = parent.points_metres[AXIS_POINTS - 1];
            let parent_tangent =
                (parent_tip - parent.points_metres[AXIS_POINTS - 2]).normalize_or(Vec3::Z);
            let tolerance = parent.radii_metres[AXIS_POINTS - 1].mul_add(1.5, 0.045);
            graph
                .axes
                .iter()
                .enumerate()
                .filter(|(_, child)| {
                    child.alive
                        && child.parent.map(|index| index as usize) == Some(parent_index)
                        && child.points_metres[0].distance_squared(parent_tip) <= tolerance.powi(2)
                })
                .filter_map(|(child_index, child)| {
                    let child_direction = (child.points_metres[2] - child.points_metres[0])
                        .normalize_or(parent_tangent);
                    let alignment = parent_tangent.dot(child_direction);
                    (alignment > 0.15).then(|| {
                        let parent_radius = parent.radii_metres[AXIS_POINTS - 1].max(1.0e-4);
                        let child_radius = child.radii_metres[0].max(1.0e-4);
                        let radius_match =
                            parent_radius.min(child_radius) / parent_radius.max(child_radius);
                        (child_index, alignment.mul_add(0.82, radius_match * 0.18))
                    })
                })
                .max_by(|left, right| left.1.total_cmp(&right.1))
                .map(|(index, _)| index)
        })
        .collect()
}

fn collect_branch_run(
    graph: &AxisGraph,
    continuations: &[Option<usize>],
    start: usize,
    first_axis: Axis,
) -> BranchRun {
    let mut samples = Vec::with_capacity(AXIS_POINTS * 3);
    let mut current = start;
    let mut axis_count = 0;
    loop {
        let axis = graph.axes[current];
        if samples.is_empty() {
            samples.extend(
                axis.points_metres
                    .into_iter()
                    .zip(axis.radii_metres)
                    .map(|(position, radius)| BranchSample { position, radius }),
            );
        } else {
            if let Some(joint) = samples.last_mut() {
                joint.radius = joint.radius.max(axis.radii_metres[0]);
            }
            samples.extend(
                axis.points_metres
                    .into_iter()
                    .zip(axis.radii_metres)
                    .skip(1)
                    .map(|(position, radius)| BranchSample { position, radius }),
            );
        }
        axis_count += 1;
        let Some(next) = continuations[current] else {
            break;
        };
        current = next;
    }
    BranchRun {
        samples,
        order: first_axis.order,
        cap_base: first_axis.parent.is_some(),
        cap_tip: true,
        axis_count,
        uv_offset: (start as f32 * 0.618_034).fract(),
    }
}

fn append_branch_run(
    mesh: &mut Mesh,
    bark: &mut Vec<BarkVertex>,
    run: &BranchRun,
) -> Result<(), String> {
    let ring_spacing = if run.order <= 1 {
        MATURE_BARK_RING_SPACING_METRES
    } else {
        YOUNG_BARK_RING_SPACING_METRES
    };
    let samples = smooth_branch_run(&run.samples, ring_spacing);
    let sides = sides_for_order(run.order);
    let ring_vertices = sides + 1;
    let tile_width = bark_tile_width(run.samples[0].radius, run.order);
    let base = u32::try_from(mesh.vertices.len()).map_err(|_| "botanical wood exceeds u32")?;
    let mut cumulative = 0.0_f32;
    let mut frame = initial_frame(samples[1].position - samples[0].position);
    for (ring_index, sample) in samples.iter().enumerate() {
        if ring_index > 0 {
            cumulative += sample.position.distance(samples[ring_index - 1].position);
        }
        let tangent = match ring_index {
            0 => samples[1].position - sample.position,
            index if index + 1 == samples.len() => sample.position - samples[index - 1].position,
            index => samples[index + 1].position - samples[index - 1].position,
        }
        .normalize_or(frame.2);
        frame = transport_frame(frame, tangent);
        for side in 0..=sides {
            let angle = side as f32 * TAU / sides as f32;
            let mut radius = sample.radius;
            if run.order == 0 {
                if ring_index == 0 {
                    let buttress = (angle * 4.0 + 0.45).cos().max(0.0).powf(2.5);
                    radius *= 1.42 + buttress * 0.78;
                } else if cumulative < 1.6 {
                    radius *= (1.0 - cumulative / 1.6).mul_add(0.12, 1.0);
                }
            }
            radius += mature_bark_radial_offset(
                run.order,
                sample.radius,
                angle,
                cumulative,
                run.uv_offset,
            );
            let radial = frame.0 * angle.cos() + frame.1 * angle.sin();
            mesh.vertices.push(sample.position + radial * radius);
            bark.push(bark_vertex(sample.radius, run.order));
            let circumference_tiles = sample.radius * TAU / tile_width;
            mesh.uv.push(Vec2::new(
                side as f32 / sides as f32 * circumference_tiles + run.uv_offset * 0.37,
                cumulative / (tile_width * 2.0) + run.uv_offset,
            ));
        }
    }
    append_sweep_triangles(mesh, base, samples.len(), sides, ring_vertices);
    if run.cap_base {
        append_sweep_cap(mesh, bark, base, samples[0], run.order, sides, false)?;
    }
    if run.cap_tip {
        let last = base + ((samples.len() - 1) * ring_vertices) as u32;
        append_sweep_cap(
            mesh,
            bark,
            last,
            samples[samples.len() - 1],
            run.order,
            sides,
            true,
        )?;
    }
    Ok(())
}

fn smooth_branch_run(control: &[BranchSample], maximum_spacing: f32) -> Vec<BranchSample> {
    let mut samples = Vec::with_capacity(control.len() * 2);
    for segment_index in 0..control.len() - 1 {
        let start = control[segment_index];
        let end = control[segment_index + 1];
        let chord = end.position - start.position;
        let start_tangent = if segment_index == 0 {
            chord
        } else {
            (end.position - control[segment_index - 1].position) * 0.5
        };
        let end_tangent = if segment_index + 2 == control.len() {
            chord
        } else {
            (control[segment_index + 2].position - start.position) * 0.5
        };
        let subdivisions = (chord.length() / maximum_spacing).ceil().clamp(1.0, 12.0) as usize;
        for step in 0..subdivisions {
            let fraction = step as f32 / subdivisions as f32;
            let position = cubic_hermite(
                start.position,
                end.position,
                bounded_spline_tangent(start_tangent, chord),
                bounded_spline_tangent(end_tangent, chord),
                fraction,
            );
            let radius = (end.radius - start.radius).mul_add(smoothstep(fraction), start.radius);
            samples.push(BranchSample { position, radius });
        }
    }
    samples.push(control[control.len() - 1]);
    samples
}

fn mature_bark_radial_offset(
    order: u8,
    radius_metres: f32,
    angle: f32,
    longitudinal_metres: f32,
    variation: f32,
) -> f32 {
    if order > 1 {
        return 0.0;
    }
    let maturity = ((bark_vertex(radius_metres, order).maturity - 0.30) / 0.70).clamp(0.0, 1.0);
    let envelope = smoothstep(maturity);
    let amplitude = (radius_metres * 0.046).clamp(0.0012, 0.027) * envelope;
    let phase = variation * TAU;
    let furrow_count = if order == 0 { 8.0 } else { 6.0 };
    let wander = (longitudinal_metres * 0.73 + phase).sin() * 0.24
        + (longitudinal_metres * 1.81 - phase * 0.37).sin() * 0.10;
    let furrow_wave = (angle * furrow_count + wander).cos().mul_add(0.5, 0.5);
    let furrow = furrow_wave.powf(7.0);
    let cross_phase = longitudinal_metres * 7.4 + angle * 2.0 + phase;
    let cross_furrow = cross_phase.cos().mul_add(0.5, 0.5).powf(10.0);
    let plate = (cross_phase.sin() * 0.58
        + (longitudinal_metres * 3.1 - angle * 3.0 + phase * 0.43).sin() * 0.42)
        * 0.34;
    amplitude * (0.20 + plate - furrow - cross_furrow * 0.42)
}

fn bounded_spline_tangent(tangent: Vec3, chord: Vec3) -> Vec3 {
    let maximum = chord.length() * 1.35;
    tangent
        .try_normalize()
        .unwrap_or_else(|| chord.normalize_or(Vec3::Z))
        * tangent.length().min(maximum)
}

fn cubic_hermite(start: Vec3, end: Vec3, start_tangent: Vec3, end_tangent: Vec3, t: f32) -> Vec3 {
    let t2 = t * t;
    let t3 = t2 * t;
    start * (t3.mul_add(2.0, -3.0 * t2) + 1.0)
        + start_tangent * (t3 - 2.0 * t2 + t)
        + end * (-2.0 * t3 + 3.0 * t2)
        + end_tangent * (t3 - t2)
}

fn sides_for_order(order: u8) -> usize {
    match order {
        0 => 28,
        1 => 22,
        2 => 12,
        4 => 6,
        5.. => 4,
        _ => 8,
    }
}

fn append_sweep_triangles(
    mesh: &mut Mesh,
    base: u32,
    rings: usize,
    sides: usize,
    ring_vertices: usize,
) {
    for ring in 0..rings - 1 {
        let lower = base + (ring * ring_vertices) as u32;
        let upper = base + ((ring + 1) * ring_vertices) as u32;
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
}

fn append_sweep_cap(
    mesh: &mut Mesh,
    bark: &mut Vec<BarkVertex>,
    ring: u32,
    sample: BranchSample,
    order: u8,
    sides: usize,
    tip: bool,
) -> Result<(), String> {
    let centre = u32::try_from(mesh.vertices.len()).map_err(|_| "botanical wood exceeds u32")?;
    mesh.vertices.push(sample.position);
    bark.push(bark_vertex(sample.radius, order));
    mesh.uv.push(Vec2::new(0.5, 0.0));
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

fn append_axis_sweep(
    mesh: &mut Mesh,
    bark: &mut Vec<BarkVertex>,
    axis: Axis,
) -> Result<(), String> {
    let sides = sides_for_order(axis.order);
    let ring_vertices = sides + 1;
    let tile_width = bark_tile_width(axis.radii_metres[0], axis.order);
    let base = u32::try_from(mesh.vertices.len()).map_err(|_| "botanical wood exceeds u32")?;
    let mut cumulative = 0.0_f32;
    let mut frame = initial_frame(axis.points_metres[1] - axis.points_metres[0]);
    for point_index in 0..AXIS_POINTS {
        if point_index > 0 {
            cumulative +=
                axis.points_metres[point_index].distance(axis.points_metres[point_index - 1]);
        }
        let tangent = if point_index + 1 < AXIS_POINTS {
            (axis.points_metres[point_index + 1] - axis.points_metres[point_index])
                .normalize_or(Vec3::Z)
        } else {
            (axis.points_metres[point_index] - axis.points_metres[point_index - 1])
                .normalize_or(Vec3::Z)
        };
        frame = transport_frame(frame, tangent);
        for side in 0..=sides {
            let angle = side as f32 * TAU / sides as f32;
            let mut radius = axis.radii_metres[point_index];
            if axis.order == 0 && point_index == 0 {
                let buttress = (angle * 4.0 + 0.45).cos().max(0.0).powf(2.5);
                radius *= 1.42 + buttress * 0.78;
            } else if axis.order == 0 && point_index == 1 {
                radius *= 1.12;
            }
            let radial = frame.0 * angle.cos() + frame.1 * angle.sin();
            mesh.vertices
                .push(axis.points_metres[point_index] + radial * radius);
            bark.push(bark_vertex(axis.radii_metres[point_index], axis.order));
            let circumference_tiles = axis.radii_metres[point_index] * TAU / tile_width;
            mesh.uv.push(Vec2::new(
                side as f32 / sides as f32 * circumference_tiles,
                cumulative / (tile_width * 2.0),
            ));
        }
    }
    append_sweep_triangles(mesh, base, AXIS_POINTS, sides, ring_vertices);
    if axis.parent.is_some() {
        let centre =
            u32::try_from(mesh.vertices.len()).map_err(|_| "botanical wood exceeds u32")?;
        mesh.vertices.push(axis.points_metres[0]);
        bark.push(bark_vertex(axis.radii_metres[0], axis.order));
        mesh.uv.push(Vec2::new(0.5, 0.0));
        for side in 0..sides {
            let next = side + 1;
            mesh.triangles
                .extend([base + next as u32, base + side as u32, centre]);
        }
    }
    let tip = u32::try_from(mesh.vertices.len()).map_err(|_| "botanical wood exceeds u32")?;
    mesh.vertices.push(axis.points_metres[AXIS_POINTS - 1]);
    bark.push(bark_vertex(axis.radii_metres[AXIS_POINTS - 1], axis.order));
    mesh.uv
        .push(Vec2::new(0.5, cumulative / (tile_width * 2.0)));
    let last = base + ((AXIS_POINTS - 1) * ring_vertices) as u32;
    for side in 0..sides {
        let next = side + 1;
        mesh.triangles
            .extend([last + side as u32, last + next as u32, tip]);
    }
    Ok(())
}

fn bark_vertex(radius_metres: f32, order: u8) -> BarkVertex {
    let radius_response = ((radius_metres - 0.005) / 0.175).clamp(0.0, 1.0).sqrt();
    let order_response = 1.0 - f32::from(order).min(4.0) * 0.25;
    BarkVertex {
        radius_metres,
        maturity: radius_response.mul_add(0.82, order_response * 0.18),
    }
}

fn bark_tile_width(radius_metres: f32, order: u8) -> f32 {
    let maturity = bark_vertex(radius_metres, order).maturity;
    (MATURE_BARK_TILE_WIDTH_METRES - YOUNG_BARK_TILE_WIDTH_METRES)
        .mul_add(smoothstep(maturity), YOUNG_BARK_TILE_WIDTH_METRES)
}

fn initial_frame(tangent: Vec3) -> (Vec3, Vec3, Vec3) {
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
    let projected = frame.0 - tangent * frame.0.dot(tangent);
    let projected = projected.try_normalize().unwrap_or_else(|| {
        let reference = if tangent.z.abs() < 0.88 {
            Vec3::Z
        } else {
            Vec3::X
        };
        tangent.cross(reference).normalize_or(Vec3::X)
    });
    let y = tangent.cross(projected).normalize_or(frame.1);
    (projected, y, tangent)
}

fn shoot_tip_archetypes() -> [Mesh; SHOOT_TIP_ARCHETYPE_COUNT] {
    [bud_mesh(), broken_tip_mesh()]
}

fn bud_mesh() -> Mesh {
    const SIDES: usize = 8;
    const PROFILE: [(f32, f32); 5] = [
        (-0.08, 0.58),
        (0.10, 0.82),
        (0.36, 1.00),
        (0.66, 0.76),
        (0.86, 0.36),
    ];
    let mut mesh = Mesh::default();
    for (ring, (x, radius)) in PROFILE.into_iter().enumerate() {
        for side in 0..SIDES {
            let angle = side as f32 * TAU / SIDES as f32 + ring as f32 * 0.045;
            let scale_ridge = 1.0 + (angle * 3.0 + x * 2.4).cos() * 0.045;
            mesh.vertices.push(Vec3::new(
                x,
                angle.cos() * radius * scale_ridge,
                angle.sin() * radius * scale_ridge,
            ));
            mesh.uv
                .push(Vec2::new(side as f32 / SIDES as f32, (x + 0.08) / 1.08));
        }
    }
    append_closed_profile(&mut mesh, PROFILE.len(), SIDES, 1.0);
    mesh.calculate_normals();
    mesh
}

fn broken_tip_mesh() -> Mesh {
    const SIDES: usize = 7;
    const PROFILE: [(f32, f32); 3] = [(-0.08, 0.82), (0.40, 1.0), (0.84, 0.82)];
    let mut mesh = Mesh::default();
    for (ring, (x, radius)) in PROFILE.into_iter().enumerate() {
        for side in 0..SIDES {
            let angle = side as f32 * TAU / SIDES as f32;
            let irregular = 1.0 + (angle * 2.0 + ring as f32).sin() * 0.075;
            let jagged_tip = if ring + 1 == PROFILE.len() {
                (angle * 3.0 + 0.7).sin() * 0.10
            } else {
                0.0
            };
            mesh.vertices.push(Vec3::new(
                x + jagged_tip,
                angle.cos() * radius * irregular,
                angle.sin() * radius * irregular,
            ));
            mesh.uv
                .push(Vec2::new(side as f32 / SIDES as f32, (x + 0.08) / 1.08));
        }
    }
    append_closed_profile(&mut mesh, PROFILE.len(), SIDES, 0.82);
    mesh.calculate_normals();
    mesh
}

fn append_closed_profile(mesh: &mut Mesh, rings: usize, sides: usize, tip_x: f32) {
    for ring in 0..rings - 1 {
        let lower = (ring * sides) as u32;
        let upper = ((ring + 1) * sides) as u32;
        for side in 0..sides {
            let next = (side + 1) % sides;
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
    let base = u32::try_from(mesh.vertices.len()).expect("shoot-tip archetype fits u32");
    mesh.vertices.push(Vec3::new(-0.08, 0.0, 0.0));
    mesh.uv.push(Vec2::new(0.5, 0.0));
    for side in 0..sides {
        let next = (side + 1) % sides;
        mesh.triangles.extend([next as u32, side as u32, base]);
    }
    let tip = u32::try_from(mesh.vertices.len()).expect("shoot-tip archetype fits u32");
    mesh.vertices.push(Vec3::new(tip_x, 0.0, 0.0));
    mesh.uv.push(Vec2::new(0.5, 1.0));
    let last = ((rings - 1) * sides) as u32;
    for side in 0..sides {
        let next = (side + 1) % sides;
        mesh.triangles
            .extend([last + side as u32, last + next as u32, tip]);
    }
}

fn leaf_archetypes() -> [Mesh; LEAF_ARCHETYPE_COUNT] {
    LEAF_SHAPES.map(leaf_mesh)
}

#[derive(Clone, Copy)]
struct LeafShape {
    atlas_tile: u8,
    width_scale: f32,
    shoulder: f32,
    base_taper: f32,
    tip_taper: f32,
    fold: f32,
    cup: f32,
    droop: f32,
    torsion: f32,
    side_bias: f32,
    sweep: f32,
    margin_wave: f32,
    damage: Option<(usize, i8, f32)>,
}

const LEAF_SHAPES: [LeafShape; LEAF_ARCHETYPE_COUNT] = [
    LeafShape {
        atlas_tile: 0,
        width_scale: 1.0,
        shoulder: 0.43,
        base_taper: 0.58,
        tip_taper: 0.82,
        fold: 0.055,
        cup: 0.075,
        droop: 0.090,
        torsion: 0.022,
        side_bias: 0.045,
        sweep: 0.018,
        margin_wave: 0.002,
        damage: None,
    },
    LeafShape {
        atlas_tile: 1,
        width_scale: 1.0,
        shoulder: 0.40,
        base_taper: 0.72,
        tip_taper: 1.00,
        fold: 0.035,
        cup: 0.120,
        droop: 0.055,
        torsion: -0.028,
        side_bias: -0.030,
        sweep: -0.014,
        margin_wave: 0.001,
        damage: None,
    },
    LeafShape {
        atlas_tile: 2,
        width_scale: 1.0,
        shoulder: 0.46,
        base_taper: 0.55,
        tip_taper: 0.72,
        fold: 0.080,
        cup: 0.060,
        droop: 0.180,
        torsion: 0.034,
        side_bias: 0.075,
        sweep: 0.030,
        margin_wave: 0.004,
        damage: Some((17, 1, 0.28)),
    },
    LeafShape {
        atlas_tile: 3,
        width_scale: 1.0,
        shoulder: 0.44,
        base_taper: 0.64,
        tip_taper: 0.88,
        fold: 0.045,
        cup: 0.095,
        droop: 0.125,
        torsion: -0.031,
        side_bias: -0.065,
        sweep: -0.026,
        margin_wave: 0.006,
        damage: Some((14, -1, 0.38)),
    },
    LeafShape {
        atlas_tile: 0,
        width_scale: 1.0,
        shoulder: 0.47,
        base_taper: 0.66,
        tip_taper: 0.76,
        fold: 0.070,
        cup: 0.090,
        droop: 0.120,
        torsion: -0.036,
        side_bias: -0.055,
        sweep: 0.026,
        margin_wave: 0.003,
        damage: None,
    },
    LeafShape {
        atlas_tile: 1,
        width_scale: 1.0,
        shoulder: 0.42,
        base_taper: 0.76,
        tip_taper: 0.92,
        fold: 0.026,
        cup: 0.105,
        droop: 0.072,
        torsion: 0.027,
        side_bias: 0.040,
        sweep: 0.012,
        margin_wave: 0.002,
        damage: None,
    },
    LeafShape {
        atlas_tile: 2,
        width_scale: 1.0,
        shoulder: 0.48,
        base_taper: 0.60,
        tip_taper: 0.70,
        fold: 0.068,
        cup: 0.072,
        droop: 0.165,
        torsion: -0.040,
        side_bias: -0.082,
        sweep: -0.034,
        margin_wave: 0.005,
        damage: Some((22, -1, 0.20)),
    },
    LeafShape {
        atlas_tile: 3,
        width_scale: 1.0,
        shoulder: 0.45,
        base_taper: 0.57,
        tip_taper: 0.84,
        fold: 0.058,
        cup: 0.082,
        droop: 0.145,
        torsion: 0.038,
        side_bias: 0.070,
        sweep: 0.031,
        margin_wave: 0.007,
        damage: Some((19, 1, 0.31)),
    },
];

const LEAF_STATION_COUNT: usize = 33;
const LEAF_COLUMNS: [f32; 11] = [-1.0, -0.8, -0.6, -0.4, -0.2, 0.0, 0.2, 0.4, 0.6, 0.8, 1.0];

fn leaf_mesh(shape: LeafShape) -> Mesh {
    let mut mesh = Mesh::default();
    for station in 0..LEAF_STATION_COUNT {
        let x = station as f32 / (LEAF_STATION_COUNT - 1) as f32;
        let width = leaf_profile_width(x, shape.shoulder, shape.base_taper, shape.tip_taper);
        let asymmetry_envelope = (x * PI).sin().max(0.0) * x.sqrt();
        let centreline = shape.sweep * (x * PI).sin().max(0.0) * x;
        for side in LEAF_COLUMNS {
            let edge = side.abs();
            let side_sign = side.signum() as i8;
            let margin = if side == 0.0 {
                0.0
            } else {
                let margin_envelope = (x * PI).sin().max(0.0).powf(0.45);
                let wave = (x.mul_add(TAU * 10.6, side.signum() * 0.42)).sin()
                    * shape.margin_wave
                    * margin_envelope
                    * edge.powf(5.0);
                let damage = shape
                    .damage
                    .map_or(1.0, |(damaged_station, damaged_side, depth)| {
                        let distance = damaged_station.abs_diff(station);
                        if damaged_side == side_sign && distance <= 2 {
                            let station_response = match distance {
                                0 => 1.0,
                                1 => 0.52,
                                _ => 0.18,
                            };
                            1.0 - depth * station_response * edge.powf(4.0)
                        } else {
                            1.0
                        }
                    });
                let side_scale = 1.0 + side.signum() * shape.side_bias * asymmetry_envelope;
                side * width * 0.5 * side_scale * (1.0 + wave) * damage
            };
            let y = (margin + centreline) * shape.width_scale;
            let midrib = if side == 0.0 {
                0.010 * (1.0 - x).powf(0.65)
            } else {
                0.0
            };
            let twist_envelope = (x * PI).sin().max(0.0) * x.sqrt();
            let z = shape.cup * (1.0 - edge.powf(1.6)) + shape.fold * edge.powf(1.35)
                - shape.droop * x.powf(2.4)
                + shape.torsion * side * twist_envelope
                + midrib;
            mesh.vertices.push(Vec3::new(x, y, z));
            mesh.uv.push(leaf_atlas_uv(
                shape.atlas_tile,
                Vec2::new(side.mul_add(0.5, 0.5), x),
            ));
        }
    }
    for station in 0..LEAF_STATION_COUNT - 1 {
        let row = (station * LEAF_COLUMNS.len()) as u32;
        let next_row = row + LEAF_COLUMNS.len() as u32;
        for column in 0..LEAF_COLUMNS.len() - 1 {
            let left = row + column as u32;
            let right = left + 1;
            let next_left = next_row + column as u32;
            let next_right = next_left + 1;
            if (station + column).is_multiple_of(2) {
                mesh.triangles
                    .extend([left, next_left, next_right, left, next_right, right]);
            } else {
                mesh.triangles
                    .extend([left, next_left, right, right, next_left, next_right]);
            }
        }
    }
    append_leaf_margin_edges(&mut mesh);
    append_petiole(&mut mesh, shape.width_scale, shape.atlas_tile);
    mesh.calculate_normals();
    mesh
}

fn leaf_profile_width(x: f32, shoulder: f32, base_taper: f32, tip_taper: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    let profile = if x <= shoulder {
        let phase = x / shoulder;
        (phase * PI * 0.5).sin().powf(base_taper)
    } else {
        let phase = (1.0 - x) / (1.0 - shoulder);
        (phase * PI * 0.5).sin().powf(tip_taper)
    };
    0.006 + profile * 0.994
}

fn append_leaf_margin_edges(mesh: &mut Mesh) {
    const EDGE_THICKNESS: f32 = 0.004;
    for (margin, reverse) in [(0, true), (LEAF_COLUMNS.len() - 1, false)] {
        let bottom = u32::try_from(mesh.vertices.len()).expect("leaf edge fits u32");
        for station in 0..LEAF_STATION_COUNT {
            let top = station * LEAF_COLUMNS.len() + margin;
            let mut vertex = mesh.vertices[top];
            vertex.z -= EDGE_THICKNESS;
            mesh.vertices.push(vertex);
            mesh.uv.push(mesh.uv[top]);
        }
        for station in 0..LEAF_STATION_COUNT - 1 {
            let top = (station * LEAF_COLUMNS.len() + margin) as u32;
            let next_top = ((station + 1) * LEAF_COLUMNS.len() + margin) as u32;
            let lower = bottom + station as u32;
            let next_lower = lower + 1;
            let triangles = if reverse {
                [top, lower, next_top, lower, next_lower, next_top]
            } else {
                [top, next_top, lower, lower, next_top, next_lower]
            };
            mesh.triangles.extend(triangles);
        }
    }
}

fn append_petiole(mesh: &mut Mesh, width_scale: f32, atlas_tile: u8) {
    const SIDES: usize = 6;
    let base = u32::try_from(mesh.vertices.len()).expect("leaf petiole fits u32");
    for (ring, x) in [-0.10_f32, 0.025].into_iter().enumerate() {
        let taper = if ring == 0 { 0.78 } else { 1.0 };
        for side in 0..SIDES {
            let phase = side as f32 / SIDES as f32 * TAU;
            mesh.vertices.push(Vec3::new(
                x,
                phase.cos() * 0.032 * width_scale * taper,
                phase.sin() * 0.0075 * taper,
            ));
            mesh.uv.push(leaf_atlas_uv(
                atlas_tile,
                Vec2::new(0.18 + phase.cos() * 0.02, 0.50 + ring as f32 * 0.08),
            ));
        }
    }
    for side in 0..SIDES {
        let next = (side + 1) % SIDES;
        let start = base + side as u32;
        let start_next = base + next as u32;
        let end = base + SIDES as u32 + side as u32;
        let end_next = base + SIDES as u32 + next as u32;
        mesh.triangles
            .extend([start, start_next, end, start_next, end_next, end]);
    }
}

fn leaf_atlas_uv(tile: u8, local: Vec2) -> Vec2 {
    let tile = u32::from(tile).min(LEAF_ATLAS_TILE_COUNT - 1);
    let column = tile % LEAF_ATLAS_COLUMNS;
    let row = tile / LEAF_ATLAS_COLUMNS;
    let tile_scale = 1.0 / LEAF_ATLAS_COLUMNS as f32;
    let inset = 1.0 / LEAF_ATLAS_SIZE as f32;
    let usable = tile_scale - inset * 2.0;
    Vec2::new(
        column as f32 * tile_scale + inset + local.x.clamp(0.0, 1.0) * usable,
        row as f32 * tile_scale + inset + local.y.clamp(0.0, 1.0) * usable,
    )
}

fn foliage_pad_archetypes() -> [Mesh; FOLIAGE_PAD_ARCHETYPE_COUNT] {
    [pad_mesh(0x8d3a_6f11), pad_mesh(0x51c2_e7a9)]
}

fn pad_mesh(seed: u64) -> Mesh {
    let source = pad_leaf_mesh();
    let mut rng = Rng::new(seed);
    let mut result = Mesh::default();
    for spray in 0..5 {
        append_pad_spray(&mut result, &source, &mut rng, spray);
    }
    result.calculate_normals();
    result
}

fn pad_leaf_mesh() -> Mesh {
    const STATIONS: usize = 5;
    const COLUMNS: [f32; 3] = [-1.0, 0.0, 1.0];
    let mut mesh = Mesh::default();
    for station in 0..STATIONS {
        let x = station as f32 / (STATIONS - 1) as f32;
        let width = pad_leaf_profile_width(x, station + 1 == STATIONS) * 0.76;
        let longitudinal_curve = (x * PI).sin();
        for lateral in COLUMNS {
            let y = lateral * width;
            let cup = longitudinal_curve * (1.0 - lateral * lateral) * 0.055;
            let fold = lateral.abs() * longitudinal_curve * 0.028;
            let droop = x * x * 0.080;
            mesh.vertices.push(Vec3::new(x, y, cup + fold - droop));
            mesh.uv
                .push(leaf_atlas_uv(0, Vec2::new(lateral.mul_add(0.5, 0.5), x)));
        }
    }
    for station in 0..STATIONS - 1 {
        let row = (station * COLUMNS.len()) as u32;
        let next_row = row + COLUMNS.len() as u32;
        for column in 0..COLUMNS.len() - 1 {
            let left = row + column as u32;
            let right = left + 1;
            let next_left = next_row + column as u32;
            let next_right = next_left + 1;
            mesh.triangles
                .extend([left, next_left, next_right, left, next_right, right]);
        }
    }
    mesh.calculate_normals();
    mesh
}

fn pad_leaf_profile_width(x: f32, tip: bool) -> f32 {
    if tip {
        return 0.012;
    }
    let shoulder = (PI * x.powf(0.92)).sin().max(0.0).powf(0.70);
    (0.045 + shoulder * 0.955) * (1.0 - x * 0.08)
}

fn append_pad_spray(destination: &mut Mesh, source: &Mesh, rng: &mut Rng, spray: usize) {
    let base_phase = spray as f32 * 2.399_963_1 + rng.range(-0.22, 0.22);
    let spray_direction = Vec3::new(
        rng.range(0.78, 1.0),
        base_phase.cos() * rng.range(0.12, 0.34),
        base_phase.sin() * rng.range(0.16, 0.40),
    )
    .normalize_or(Vec3::X);
    let spray_side = spray_direction.cross(Vec3::Y).normalize_or(Vec3::Z);
    let spray_up = spray_side.cross(spray_direction).normalize_or(Vec3::Y);
    let spray_origin = Vec3::new(
        rng.range(-0.62, -0.38),
        base_phase.cos() * rng.range(0.04, 0.20),
        base_phase.sin() * rng.range(0.06, 0.22),
    );
    for node in 0..4 {
        let station = 0.16 + node as f32 * 0.22 + rng.range(-0.022, 0.022);
        let node_phase = base_phase + node as f32 * PI * 0.5 + rng.range(-0.10, 0.10);
        let fan = spray_up * node_phase.cos() + spray_side * node_phase.sin();
        for pair in [-1.0_f32, 1.0] {
            let direction = (spray_direction * rng.range(0.28, 0.40)
                + fan * pair * rng.range(0.88, 1.02)
                + Vec3::Y * rng.range(0.04, 0.14))
            .normalize_or(fan * pair);
            let projected_up = (Vec3::Y - direction * direction.y).normalize_or(spray_up);
            let cross_roll = spray_direction.cross(direction).normalize_or(spray_side);
            let normal = (projected_up * 0.92 + cross_roll * rng.range(-0.12, 0.12))
                .normalize_or(projected_up);
            let side = normal.cross(direction).normalize_or(spray_side);
            let origin =
                spray_origin + spray_direction * station + fan * pair * rng.range(0.012, 0.035);
            append_transformed_mesh(
                destination,
                source,
                origin,
                direction,
                side,
                normal,
                rng.range(
                    MIDDLE_PROXY_LEAF_LENGTH_RANGE.0,
                    MIDDLE_PROXY_LEAF_LENGTH_RANGE.1,
                ) + node as f32 * 0.008,
                rng.range(
                    MIDDLE_PROXY_LEAF_WIDTH_RANGE.0,
                    MIDDLE_PROXY_LEAF_WIDTH_RANGE.1,
                ),
            );
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn append_transformed_mesh(
    destination: &mut Mesh,
    source: &Mesh,
    origin: Vec3,
    x: Vec3,
    y: Vec3,
    z: Vec3,
    length: f32,
    width: f32,
) {
    let base = u32::try_from(destination.vertices.len()).expect("pad archetype fits u32");
    destination
        .vertices
        .extend(source.vertices.iter().map(|vertex| {
            origin + x * vertex.x * length + y * vertex.y * width + z * vertex.z * length
        }));
    destination.uv.extend_from_slice(&source.uv);
    destination
        .triangles
        .extend(source.triangles.iter().map(|index| base + index));
}

fn bark_texture(seed: u64) -> BotanicalTexture {
    texture(BARK_TEXTURE_WIDTH, BARK_TEXTURE_HEIGHT, |x, y| {
        let height = bark_height(seed, x.cast_signed(), y.cast_signed());
        let broad = periodic_value_noise(seed ^ 0xa241, x, y, 5, 8);
        let lichen = bark_lichen(seed, x, y);
        let shade = 0.22 + height * 0.36 + broad * 0.035;
        let bark = [shade * 0.94, shade * 0.92, shade * 0.88];
        let lichen_colour = [
            0.365 + broad * 0.055,
            0.385 + broad * 0.055,
            0.335 + broad * 0.040,
        ];
        let blend = lichen * 0.36;
        [
            ((lichen_colour[0] - bark[0]).mul_add(blend, bark[0]) * 255.0) as u8,
            ((lichen_colour[1] - bark[1]).mul_add(blend, bark[1]) * 255.0) as u8,
            ((lichen_colour[2] - bark[2]).mul_add(blend, bark[2]) * 255.0) as u8,
            255,
        ]
    })
}

fn scar_texture(seed: u64) -> BotanicalTexture {
    texture(SCAR_TEXTURE_SIZE, SCAR_TEXTURE_SIZE, |x, y| {
        let point = Vec2::new(
            (x as f32 + 0.5) / SCAR_TEXTURE_SIZE as f32 * 2.0 - 1.0,
            (y as f32 + 0.5) / SCAR_TEXTURE_SIZE as f32 * 2.0 - 1.0,
        );
        let angle = point.y.atan2(point.x);
        let radius = point.length();
        let grain = value_noise(seed ^ 0x6772_6169, x, y, 9);
        let pore = value_noise(seed ^ 0x706f_7265, x, y, 3);
        let warped_radius = radius
            + (angle * 3.0 + grain * 2.2).sin() * 0.018
            + (angle * 7.0 - pore * 1.7).sin() * 0.007;
        let ring = ((warped_radius * 12.5 + grain * 0.45) * TAU)
            .cos()
            .mul_add(0.5, 0.5)
            .powf(7.0);
        let radial_crack = ((angle * 5.0 + grain * 1.6).sin().abs() / 0.055).clamp(0.0, 1.0);
        let crack = (1.0 - radial_crack) * smoothstep(((radius - 0.28) / 0.58).clamp(0.0, 1.0));
        let rim = smoothstep(((radius - 0.80) / 0.18).clamp(0.0, 1.0));
        let colour = Vec3::new(0.365, 0.315, 0.255)
            + Vec3::splat((grain - 0.5) * 0.045 + (pore - 0.5) * 0.020)
            - Vec3::new(0.065, 0.052, 0.035) * ring
            - Vec3::new(0.15, 0.13, 0.10) * crack
            - Vec3::new(0.12, 0.11, 0.09) * rim;
        encode_colour(colour)
    })
}

fn bark_normal_texture(seed: u64) -> BotanicalTexture {
    texture(BARK_TEXTURE_WIDTH, BARK_TEXTURE_HEIGHT, |x, y| {
        let left = bark_height(seed, x.cast_signed() - 1, y.cast_signed());
        let right = bark_height(seed, x.cast_signed() + 1, y.cast_signed());
        let down = bark_height(seed, x.cast_signed(), y.cast_signed() - 1);
        let up = bark_height(seed, x.cast_signed(), y.cast_signed() + 1);
        encode_normal(Vec3::new((left - right) * 2.1, (down - up) * 1.6, 1.0))
    })
}

fn bark_depth_texture(seed: u64) -> BotanicalTexture {
    texture(BARK_TEXTURE_WIDTH, BARK_TEXTURE_HEIGHT, |x, y| {
        let height = bark_height(seed, x.cast_signed(), y.cast_signed());
        let depth = ((1.0 - height) * 255.0).round() as u8;
        [depth, depth, depth, 255]
    })
}

fn bark_metallic_roughness_texture(seed: u64) -> BotanicalTexture {
    texture(BARK_TEXTURE_WIDTH, BARK_TEXTURE_HEIGHT, |x, y| {
        let height = bark_height(seed, x.cast_signed(), y.cast_signed());
        let pore = periodic_value_noise(seed ^ 0x711f, x, y, 29, 47);
        let lichen = bark_lichen(seed, x, y);
        let roughness =
            (0.75 + (1.0 - height) * 0.14 + pore * 0.055 + lichen * 0.035).clamp(0.0, 1.0);
        [255, (roughness * 255.0) as u8, 0, 255]
    })
}

fn bark_lichen(seed: u64, x: u32, y: u32) -> f32 {
    let broad = periodic_value_noise(seed ^ 0x6c69_6368, x, y, 4, 7);
    let breakup = periodic_value_noise(seed ^ 0x6d69_6372, x, y, 13, 19);
    let patch = smoothstep(((broad - 0.62) * 6.2).clamp(0.0, 1.0));
    let porous_edge = smoothstep(((breakup - 0.43) * 4.0).clamp(0.0, 1.0));
    patch * (0.52 + porous_edge * 0.48)
}

fn bark_height(seed: u64, x: i32, y: i32) -> f32 {
    let x = x
        .rem_euclid(BARK_TEXTURE_WIDTH.cast_signed())
        .cast_unsigned();
    let y = y
        .rem_euclid(BARK_TEXTURE_HEIGHT.cast_signed())
        .cast_unsigned();
    let (nearest, next_nearest, longitudinal_edge) = bark_cell_distances(seed ^ 0x4f11, x, y);
    let edge_distance = next_nearest - nearest;
    let cell_edge = 1.0 - smoothstep(((edge_distance - 0.018) / 0.12).clamp(0.0, 1.0));
    let cross_fissure = cell_edge * smoothstep(((longitudinal_edge - 0.46) / 0.36).clamp(0.0, 1.0));
    let plate = periodic_value_noise(seed ^ 0x9365, x, y, 5, 7);
    let primary = periodic_value_noise(seed ^ 0xa241, x, y, 13, 4);
    let secondary = periodic_value_noise(seed ^ 0x1eaf, x, y, 29, 11);
    let transverse = periodic_value_noise(seed ^ 0x706c_6174, x, y, 5, 19);
    let transverse_breakup = periodic_value_noise(seed ^ 0x6372_6163, x, y, 17, 7);
    let primary_fissure = (1.0 - (primary * 2.0 - 1.0).abs()).powf(3.1);
    let secondary_fissure = (1.0 - (secondary * 2.0 - 1.0).abs()).powf(6.0);
    let transverse_line = (1.0 - (transverse * 2.0 - 1.0).abs()).powf(8.0);
    let transverse_fissure =
        transverse_line * smoothstep(((transverse_breakup - 0.36) / 0.46).clamp(0.0, 1.0));
    let longitudinal = periodic_value_noise(seed ^ 0x51a7, x, y, 19, 5);
    let grain = periodic_value_noise(seed ^ 0x711f, x, y, 31, 47);
    let micro_relief = bark_micro_relief(seed, x, y);
    (0.31 + plate * 0.22 + longitudinal * 0.12 + grain * 0.05 + micro_relief
        - primary_fissure * 0.42
        - secondary_fissure * 0.090
        - transverse_fissure * 0.14
        - cross_fissure * 0.105)
        .clamp(0.0, 1.0)
}

fn bark_micro_relief(seed: u64, x: u32, y: u32) -> f32 {
    let fine_fibre = periodic_value_noise(seed ^ 0xf1b2, x, y, 47, 131);
    let pore_field = periodic_value_noise(seed ^ 0x706f_7265, x, y, 89, 157);
    let fine_relief = (fine_fibre - 0.5) * 0.036;
    let pores = smoothstep(((pore_field - 0.72) / 0.22).clamp(0.0, 1.0)) * 0.040;
    fine_relief - pores
}

fn bark_cell_distances(seed: u64, x: u32, y: u32) -> (f32, f32, f32) {
    const CELLS_X: i32 = 8;
    const CELLS_Y: i32 = 7;
    let point_x = x as f32 / BARK_TEXTURE_WIDTH as f32 * CELLS_X as f32;
    let point_y = y as f32 / BARK_TEXTURE_HEIGHT as f32 * CELLS_Y as f32;
    let base_x = point_x.floor() as i32;
    let base_y = point_y.floor() as i32;
    let mut nearest = f32::MAX;
    let mut next_nearest = f32::MAX;
    let mut nearest_feature = (0.0, 0.0);
    let mut next_feature = (0.0, 0.0);

    for offset_y in -1..=1 {
        for offset_x in -1..=1 {
            let cell_x = base_x + offset_x;
            let cell_y = base_y + offset_y;
            let wrapped_x = cell_x.rem_euclid(CELLS_X) as u32;
            let wrapped_y = cell_y.rem_euclid(CELLS_Y) as u32;
            let jitter_x = hash_unit(seed, wrapped_x, wrapped_y).mul_add(0.68, 0.16);
            let jitter_y = hash_unit(seed ^ 0x7869, wrapped_x, wrapped_y).mul_add(0.68, 0.16);
            let feature = (cell_x as f32 + jitter_x, cell_y as f32 + jitter_y);
            let delta_x = feature.0 - point_x;
            let delta_y = feature.1 - point_y;
            let distance = delta_x.hypot(delta_y * 0.85);
            if distance < nearest {
                next_nearest = nearest;
                next_feature = nearest_feature;
                nearest = distance;
                nearest_feature = feature;
            } else if distance < next_nearest {
                next_nearest = distance;
                next_feature = feature;
            }
        }
    }

    let horizontal_span = (next_feature.0 - nearest_feature.0).abs();
    let vertical_span = ((next_feature.1 - nearest_feature.1) * 0.85).abs();
    let longitudinal_edge = horizontal_span / (horizontal_span + vertical_span).max(1.0e-4);
    (nearest, next_nearest, longitudinal_edge)
}

#[derive(Clone, Copy)]
struct LeafAtlasTexel {
    tile: u8,
    x: u32,
    y: u32,
    u: f32,
}

fn leaf_texture(seed: u64) -> BotanicalTexture {
    texture(LEAF_ATLAS_SIZE, LEAF_ATLAS_SIZE, |x, y| {
        let texel = leaf_atlas_texel(x, y);
        let tile_seed = leaf_tile_seed(seed, texel.tile);
        let broad = value_noise(tile_seed, texel.x, texel.y, 11);
        let micro = hash_unit(tile_seed ^ 0xa951, texel.x, texel.y) - 0.5;
        let vein = (1.0 - (texel.u - 0.5).abs() * 22.0).max(0.0);
        encode_colour(leaf_colour(
            tile_seed, texel.tile, broad, micro, vein, texel,
        ))
    })
}

fn leaf_colour(
    seed: u64,
    tile: u8,
    broad: f32,
    micro: f32,
    vein: f32,
    texel: LeafAtlasTexel,
) -> Vec3 {
    let (base, variation, vein_colour) = match tile {
        1 => (
            Vec3::new(0.28, 0.39, 0.23),
            Vec3::new(0.07, 0.09, 0.06),
            Vec3::new(0.07, 0.09, 0.05),
        ),
        2 => (
            Vec3::new(0.23, 0.31, 0.11),
            Vec3::new(0.08, 0.09, 0.04),
            Vec3::new(0.07, 0.08, 0.04),
        ),
        3 => (
            Vec3::new(0.17, 0.25, 0.08),
            Vec3::new(0.08, 0.08, 0.04),
            Vec3::new(0.06, 0.07, 0.035),
        ),
        _ => (
            Vec3::new(0.15, 0.34, 0.12),
            Vec3::new(0.07, 0.10, 0.04),
            Vec3::new(0.07, 0.10, 0.045),
        ),
    };
    let colour = base + variation * broad + Vec3::splat(micro * 0.018) + vein_colour * vein;
    let spot = smoothstep(
        ((value_noise(seed ^ 0x5a07, texel.x, texel.y, 19) - 0.58) * 4.8).clamp(0.0, 1.0),
    );
    match tile {
        2 => colour.lerp(Vec3::new(0.42, 0.31, 0.09), spot * 0.34),
        3 => colour.lerp(Vec3::new(0.35, 0.21, 0.07), spot * 0.58),
        _ => colour,
    }
}

fn leaf_metallic_roughness_texture(seed: u64) -> BotanicalTexture {
    texture(LEAF_ATLAS_SIZE, LEAF_ATLAS_SIZE, |x, y| {
        let texel = leaf_atlas_texel(x, y);
        let tile_seed = leaf_tile_seed(seed, texel.tile);
        let fleck = value_noise(tile_seed ^ 0x9ac3, texel.x, texel.y, 9);
        let edge = ((texel.u - 0.5).abs() * 2.0).powf(2.0);
        let base = match texel.tile {
            1 => 0.54,
            2 => 0.62,
            3 => 0.70,
            _ => 0.48,
        };
        let roughness = (base + fleck * 0.12 + edge * 0.08).clamp(0.0, 1.0);
        [255, (roughness * 255.0) as u8, 0, 255]
    })
}

fn leaf_atlas_texel(x: u32, y: u32) -> LeafAtlasTexel {
    let column = (x / LEAF_TILE_SIZE).min(LEAF_ATLAS_COLUMNS - 1);
    let row = (y / LEAF_TILE_SIZE).min(LEAF_ATLAS_COLUMNS - 1);
    let local_x = x % LEAF_TILE_SIZE;
    let local_y = y % LEAF_TILE_SIZE;
    LeafAtlasTexel {
        tile: (row * LEAF_ATLAS_COLUMNS + column) as u8,
        x: local_x,
        y: local_y,
        u: local_x as f32 / (LEAF_TILE_SIZE - 1) as f32,
    }
}

fn leaf_tile_seed(seed: u64, tile: u8) -> u64 {
    seed ^ u64::from(tile).wrapping_mul(0x9e37_79b9_7f4a_7c15)
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
        ((normal.x * 0.5 + 0.5) * 255.0).round() as u8,
        ((normal.y * 0.5 + 0.5) * 255.0).round() as u8,
        ((normal.z * 0.5 + 0.5) * 255.0).round() as u8,
        255,
    ]
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
    ((value ^ (value >> 31)) >> 40) as f32 / 16_777_216.0
}

fn value_noise(seed: u64, x: u32, y: u32, cell_size: u32) -> f32 {
    let cell_x = x / cell_size;
    let cell_y = y / cell_size;
    let local_x = smoothstep((x % cell_size) as f32 / cell_size as f32);
    let local_y = smoothstep((y % cell_size) as f32 / cell_size as f32);
    let bottom = hash_unit(seed, cell_x, cell_y)
        .mul_add(1.0 - local_x, hash_unit(seed, cell_x + 1, cell_y) * local_x);
    let top = hash_unit(seed, cell_x, cell_y + 1).mul_add(
        1.0 - local_x,
        hash_unit(seed, cell_x + 1, cell_y + 1) * local_x,
    );
    (top - bottom).mul_add(local_y, bottom)
}

fn periodic_value_noise(seed: u64, x: u32, y: u32, frequency_x: u32, frequency_y: u32) -> f32 {
    let point_x = x as f32 / BARK_TEXTURE_WIDTH as f32 * frequency_x as f32;
    let point_y = y as f32 / BARK_TEXTURE_HEIGHT as f32 * frequency_y as f32;
    let lattice_x = point_x.floor() as u32;
    let lattice_y = point_y.floor() as u32;
    let local_x = smoothstep(point_x.fract());
    let local_y = smoothstep(point_y.fract());
    let sample = |offset_x: u32, offset_y: u32| {
        hash_unit(
            seed,
            (lattice_x + offset_x) % frequency_x,
            (lattice_y + offset_y) % frequency_y,
        )
    };
    let bottom = sample(0, 0).mul_add(1.0 - local_x, sample(1, 0) * local_x);
    let top = sample(0, 1).mul_add(1.0 - local_x, sample(1, 1) * local_x);
    (top - bottom).mul_add(local_y, bottom)
}

fn smoothstep(value: f32) -> f32 {
    value * value * (3.0 - 2.0 * value)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_mesh(mesh: &Mesh) {
        assert!(!mesh.vertices.is_empty());
        assert_eq!(mesh.vertices.len(), mesh.normals.len());
        assert_eq!(mesh.vertices.len(), mesh.uv.len());
        assert_eq!(mesh.triangles.len() % 3, 0);
        assert!(mesh.vertices.iter().all(|vertex| vertex.is_finite()));
        assert!(
            mesh.normals
                .iter()
                .all(|normal| { normal.is_finite() && (normal.length() - 1.0).abs() < 1.0e-3 })
        );
        assert!(mesh.uv.iter().all(|uv| uv.is_finite()));
        assert!(
            mesh.triangles
                .iter()
                .all(|&index| (index as usize) < mesh.vertices.len())
        );
    }

    fn assert_bark_channels(mesh: &Mesh, bark: &[BarkVertex]) {
        assert_eq!(mesh.vertices.len(), bark.len());
        assert!(bark.iter().all(|vertex| {
            vertex.radius_metres.is_finite()
                && vertex.radius_metres > 0.0
                && (0.0..=1.0).contains(&vertex.maturity)
        }));
    }

    fn test_leaf(position: Vec3) -> LeafOrgan {
        LeafOrgan {
            axis: 0,
            blade_base_metres: position,
            direction: Vec3::X,
            normal: Vec3::Z,
            length_metres: 0.22,
            width_metres: 0.07,
            archetype: 0,
            age: 0.5,
            light_exposure: 0.0,
            variation: 0.0,
        }
    }

    #[derive(Clone, Copy, Debug)]
    struct CrownMetrics {
        aspect: f32,
        horizontal_offset_metres: f32,
        leader_efficiency: f32,
        leader_alignment: f32,
        largest_vertical_gap: f32,
        tier_valley: f32,
        radial_concavity: f32,
        front_tier_valley: f32,
        quarter_tier_valley: f32,
    }

    fn crown_metrics(graph: &AxisGraph) -> CrownMetrics {
        let mut tips: Vec<_> = graph
            .axes
            .iter()
            .filter(|axis| axis.order == 3)
            .map(|axis| axis.points_metres[AXIS_POINTS - 1])
            .collect();
        let (aspect, horizontal_offset_metres) = horizontal_shape(&tips);
        let (leader_efficiency, leader_alignment) = leader_shape(graph);
        let radial_concavity = radial_concavity(&tips);
        let front_tier_valley = projected_tier_valley(&tips, Vec3::new(-18.4, 16.6, 0.0));
        let quarter_tier_valley = projected_tier_valley(&tips, Vec3::new(-16.6, -18.4, 0.0));
        tips.sort_by(|left, right| left.z.total_cmp(&right.z));
        CrownMetrics {
            aspect,
            horizontal_offset_metres,
            leader_efficiency,
            leader_alignment,
            largest_vertical_gap: largest_vertical_gap(&tips),
            tier_valley: tier_valley(&tips),
            radial_concavity,
            front_tier_valley,
            quarter_tier_valley,
        }
    }

    fn horizontal_shape(tips: &[Vec3]) -> (f32, f32) {
        let count = tips.len() as f32;
        let mean = tips.iter().copied().sum::<Vec3>() / count;
        let (xx, yy, xy) = tips
            .iter()
            .fold((0.0_f32, 0.0_f32, 0.0_f32), |(xx, yy, xy), point| {
                let offset = *point - mean;
                (
                    xx + offset.x * offset.x,
                    yy + offset.y * offset.y,
                    xy + offset.x * offset.y,
                )
            });
        let trace = (xx + yy) / count;
        let discriminant = ((xx - yy).mul_add(xx - yy, 4.0 * xy * xy)).sqrt() / count;
        let major = f32::midpoint(trace, discriminant).sqrt();
        let minor = f32::midpoint(trace, -discriminant).max(0.001).sqrt();
        (major / minor, mean.x.hypot(mean.y))
    }

    fn leader_shape(graph: &AxisGraph) -> (f32, f32) {
        let (mut index, tip) = graph
            .axes
            .iter()
            .enumerate()
            .filter(|(_, axis)| axis.order == 3)
            .map(|(index, axis)| (index, axis.points_metres[AXIS_POINTS - 1]))
            .max_by(|(_, left), (_, right)| left.z.total_cmp(&right.z))
            .expect("competition crown has a terminal leader");
        let mut base = tip;
        let mut path_length = 0.0;
        let mut alignment_total = 0.0;
        let mut joint_count = 0_u16;
        while index != 0 {
            let axis = graph.axes[index];
            base = axis.points_metres[0];
            path_length += axis_length(&axis);
            let Some(parent_index) = axis.parent.map(|parent| parent as usize) else {
                break;
            };
            if parent_index != 0 {
                let parent = graph.axes[parent_index];
                let parent_tangent = (parent.points_metres[AXIS_POINTS - 1]
                    - parent.points_metres[AXIS_POINTS - 2])
                    .normalize_or(Vec3::Z);
                let child_tangent =
                    (axis.points_metres[1] - axis.points_metres[0]).normalize_or(Vec3::Z);
                alignment_total += parent_tangent.dot(child_tangent).clamp(-1.0, 1.0);
                joint_count += 1;
            }
            index = parent_index;
        }
        let efficiency = (tip.z - base.z) / path_length.max(0.001);
        let alignment = alignment_total / f32::from(joint_count.max(1));
        (efficiency, alignment)
    }

    fn largest_vertical_gap(sorted_tips: &[Vec3]) -> f32 {
        let crown_height =
            (sorted_tips.last().expect("tip").z - sorted_tips.first().expect("tip").z).max(0.001);
        sorted_tips
            .windows(2)
            .map(|pair| (pair[1].z - pair[0].z) / crown_height)
            .fold(0.0, f32::max)
    }

    fn tier_valley(tips: &[Vec3]) -> f32 {
        const BINS: usize = 8;
        let minimum = tips.first().expect("tip").z;
        let height = (tips.last().expect("tip").z - minimum).max(0.001);
        let mut bins = [0_u16; BINS];
        for tip in tips {
            let bin = (((tip.z - minimum) / height) * BINS as f32) as usize;
            bins[bin.min(BINS - 1)] += 1;
        }
        (1..BINS - 1)
            .filter_map(|index| {
                let shoulders =
                    f32::midpoint(f32::from(bins[index - 1]), f32::from(bins[index + 1]));
                (shoulders > 0.0)
                    .then(|| ((shoulders - f32::from(bins[index])) / shoulders).max(0.0))
            })
            .fold(0.0, f32::max)
    }

    fn radial_concavity(tips: &[Vec3]) -> f32 {
        const SECTORS: usize = 16;
        let mean = tips.iter().copied().sum::<Vec3>() / tips.len() as f32;
        let mut radii = [0.0_f32; SECTORS];
        for tip in tips {
            let offset = *tip - mean;
            let sector =
                ((offset.y.atan2(offset.x).rem_euclid(TAU) / TAU) * SECTORS as f32) as usize;
            radii[sector.min(SECTORS - 1)] =
                radii[sector.min(SECTORS - 1)].max(offset.x.hypot(offset.y));
        }
        let mean_radius = radii.into_iter().sum::<f32>() / SECTORS as f32;
        (0..SECTORS)
            .map(|index| {
                let left = radii[(index + SECTORS - 2) % SECTORS];
                let right = radii[(index + 2) % SECTORS];
                (f32::midpoint(left, right) - radii[index]).max(0.0) / mean_radius.max(0.001)
            })
            .fold(0.0, f32::max)
    }

    fn projected_tier_valley(tips: &[Vec3], screen_axis: Vec3) -> f32 {
        const BINS: usize = 10;
        let screen_axis = screen_axis.normalize_or(Vec3::X);
        let (minimum_z, maximum_z) = tips.iter().fold(
            (f32::INFINITY, f32::NEG_INFINITY),
            |(minimum, maximum), tip| (minimum.min(tip.z), maximum.max(tip.z)),
        );
        let height = (maximum_z - minimum_z).max(0.001);
        let mut minimum = [f32::INFINITY; BINS];
        let mut maximum = [f32::NEG_INFINITY; BINS];
        let mut counts = [0_u16; BINS];
        for tip in tips {
            let bin = ((((tip.z - minimum_z) / height) * BINS as f32) as usize).min(BINS - 1);
            let projected = tip.dot(screen_axis);
            minimum[bin] = minimum[bin].min(projected);
            maximum[bin] = maximum[bin].max(projected);
            counts[bin] += 1;
        }
        let widths = std::array::from_fn::<_, BINS, _>(|index| {
            if counts[index] >= 3 {
                maximum[index] - minimum[index]
            } else {
                0.0
            }
        });
        (1..BINS - 1)
            .filter(|&index| widths[index - 1] > 0.0 && widths[index + 1] > 0.0)
            .map(|index| {
                let shoulders = f32::midpoint(widths[index - 1], widths[index + 1]);
                ((shoulders - widths[index]) / shoulders.max(0.001)).max(0.0)
            })
            .fold(0.0, f32::max)
    }

    #[test]
    fn prototype_is_deterministic_bounded_and_complete() {
        let mut tip_states = [0_usize; 3];
        for seed in [42, 666, 2026] {
            let first =
                generate_botanical_prototype(seed, BotanicalRecipe::default()).expect("prototype");
            let second =
                generate_botanical_prototype(seed, BotanicalRecipe::default()).expect("prototype");
            assert_eq!(first, second);
            assert!(first.graph.axes.len() <= 512);
            assert!(first.leaves.len() <= 30_000);
            assert!(first.foliage_pads.len() <= 512);
            assert_mesh(&first.wood);
            assert_mesh(&first.wood_scars);
            assert_mesh(&first.microtwigs);
            assert_bark_channels(&first.wood, &first.wood_bark);
            assert_bark_channels(&first.microtwigs, &first.microtwig_bark);
            assert!(
                first.microtwigs.vertices.len() <= 100_000,
                "seed {seed} generated {} microtwig vertices",
                first.microtwigs.vertices.len()
            );
            assert!(first.wood.uv.iter().any(|uv| uv.x > 3.0));
            assert_eq!(first.wood_scars.vertices.len() % SCAR_VERTEX_COUNT, 0);
            assert!(
                (1..=MAX_PERSISTENT_DEAD_STUBS)
                    .contains(&(first.wood_scars.vertices.len() / SCAR_VERTEX_COUNT))
            );
            let maturity_span = first.wood_bark.iter().map(|vertex| vertex.maturity).fold(
                (f32::INFINITY, f32::NEG_INFINITY),
                |(minimum, maximum), value| (minimum.min(value), maximum.max(value)),
            );
            assert!(maturity_span.1 - maturity_span.0 > 0.6);
            first.leaf_archetypes.iter().for_each(assert_mesh);
            first.shoot_tip_archetypes.iter().for_each(assert_mesh);
            first.foliage_pad_archetypes.iter().for_each(assert_mesh);
            assert!(!first.shoot_tips.is_empty());
            assert!(first.shoot_tips.len() <= first.leaves.len() / 2);
            let mut epicormic_tip_count = 0_usize;
            for tip in &first.shoot_tips {
                let support_order = first.graph.axes[tip.axis as usize].order;
                assert!(matches!(support_order, 0 | 3));
                epicormic_tip_count += usize::from(support_order == 0);
                assert!(tip.base_metres.is_finite());
                assert!(tip.direction.is_finite());
                assert!((tip.direction.length() - 1.0).abs() < 1.0e-3);
                assert!((0.014..=0.046).contains(&tip.length_metres));
                assert!((0.003..=0.014).contains(&tip.radius_metres));
                let state = match tip.state {
                    ShootTipState::ActiveBud => 0,
                    ShootTipState::DormantBud => 1,
                    ShootTipState::Broken => 2,
                };
                tip_states[state] += 1;
            }
            assert!((1..=MAX_EPICORMIC_SHOOTS).contains(&epicormic_tip_count));
        }
        assert!(tip_states.into_iter().all(|count| count > 0));
    }

    #[test]
    fn graph_has_competition_hierarchy_and_exposed_terminal_organs() {
        let prototype =
            generate_botanical_prototype(42, BotanicalRecipe::default()).expect("prototype");
        assert_eq!(prototype.graph.axes[0].parent, None);
        for order in 0..=3 {
            assert!(prototype.graph.axes.iter().any(|axis| axis.order == order));
        }
        assert!(prototype.graph.axes.iter().all(|axis| axis.alive));
        let terminal_tips = prototype
            .graph
            .axes
            .iter()
            .filter(|axis| axis.order == 3)
            .map(|axis| axis.points_metres[AXIS_POINTS - 1]);
        let (minimum, maximum, count) = terminal_tips.fold(
            (
                Vec3::splat(f32::INFINITY),
                Vec3::splat(f32::NEG_INFINITY),
                0_usize,
            ),
            |(minimum, maximum, count), tip| (minimum.min(tip), maximum.max(tip), count + 1),
        );
        assert!(count >= usize::from(BotanicalRecipe::default().primary_count) * 2);
        assert!(maximum.x - minimum.x > 4.0);
        assert!(maximum.y - minimum.y > 4.0);
        assert!(maximum.z - minimum.z > 1.5);
        assert!(prototype.leaves.iter().all(|leaf| {
            matches!(prototype.graph.axes[leaf.axis as usize].order, 0 | 3)
                && prototype.graph.axes[leaf.axis as usize].alive
                && (0.05..=0.12).contains(&leaf.length_metres)
                && (0.015..=0.060).contains(&leaf.width_metres)
                && (1.70..=2.55).contains(&(leaf.length_metres / leaf.width_metres))
                && (0.0..=1.0).contains(&leaf.light_exposure)
        }));
        let epicormic_leaf_count = prototype
            .leaves
            .iter()
            .filter(|leaf| prototype.graph.axes[leaf.axis as usize].order == 0)
            .count();
        assert!(
            (EPICORMIC_LEAVES_PER_SHOOT..=MAX_EPICORMIC_LEAVES).contains(&epicormic_leaf_count)
        );
        let (minimum_exposure, maximum_exposure) = prototype
            .leaves
            .iter()
            .map(|leaf| leaf.light_exposure)
            .fold(
                (f32::INFINITY, f32::NEG_INFINITY),
                |(minimum, maximum), value| (minimum.min(value), maximum.max(value)),
            );
        assert!(maximum_exposure - minimum_exposure > 0.25);
    }

    #[test]
    fn epicormic_shoots_are_sparse_low_and_surface_rooted() {
        for seed in [42, 666, 2026] {
            let graph = generate_graph(seed, BotanicalRecipe::default()).expect("graph");
            let trunk = graph.axes[0];
            let trunk_height = trunk.points_metres[AXIS_POINTS - 1].z;
            let shoots = epicormic_shoots(seed, &graph);
            assert!((1..=MAX_EPICORMIC_SHOOTS).contains(&shoots.len()));
            for epicormic in shoots {
                assert_eq!(epicormic.support_axis, 0);
                assert_eq!(epicormic.shoot.parent, Some(0));
                assert_eq!(epicormic.shoot.order, 4);
                assert!((0.28..=0.56).contains(&axis_length(&epicormic.shoot)));
                assert!((0.006..=0.010).contains(&epicormic.shoot.radii_metres[0]));
                let base = epicormic.shoot.points_metres[0];
                assert!((trunk_height * 0.06..=trunk_height * 0.38).contains(&base.z));
                let nearest = (0..=128)
                    .map(|sample| trunk.sample(sample as f32 / 128.0).0)
                    .min_by(|left, right| {
                        left.distance_squared(base)
                            .total_cmp(&right.distance_squared(base))
                    })
                    .expect("sampled trunk");
                let surface_offset = base - nearest;
                assert!((0.20..=0.62).contains(&surface_offset.length()));
                let shoot_direction =
                    (epicormic.shoot.points_metres[1] - base).normalize_or(surface_offset);
                assert!(shoot_direction.dot(surface_offset.normalize_or(Vec3::X)) > 0.42);
            }
        }
    }

    #[test]
    fn leaf_organs_match_mature_pohutukawa_scale() {
        let (count, total_length, total_width) = [42, 666, 2026]
            .into_iter()
            .flat_map(|seed| {
                generate_botanical_prototype(seed, BotanicalRecipe::default())
                    .expect("prototype")
                    .leaves
            })
            .fold((0_usize, 0.0_f32, 0.0_f32), |accumulator, leaf| {
                assert!((0.05..=0.12).contains(&leaf.length_metres));
                assert!((0.015..=0.060).contains(&leaf.width_metres));
                (
                    accumulator.0 + 1,
                    accumulator.1 + leaf.length_metres,
                    accumulator.2 + leaf.width_metres,
                )
            });
        let mean_length = total_length / count as f32;
        let mean_width = total_width / count as f32;
        assert!((0.065..=0.090).contains(&mean_length));
        assert!((0.028..=0.045).contains(&mean_width));
    }

    #[test]
    fn pohutukawa_leaf_planes_form_oblique_terminal_sprays() {
        let prototype =
            generate_botanical_prototype(42, BotanicalRecipe::default()).expect("prototype");
        let oblique = prototype
            .leaves
            .iter()
            .filter(|leaf| {
                assert!(leaf.direction.dot(leaf.normal).abs() < 1.0e-4);
                let sky = (Vec3::Z - leaf.direction * leaf.direction.z).normalize_or(Vec3::Z);
                let inclination = leaf.normal.dot(sky).clamp(-1.0, 1.0).acos();
                (0.55..=1.54).contains(&inclination)
            })
            .count();
        assert!(oblique * 4 > prototype.leaves.len() * 3);
    }

    #[test]
    fn canopy_light_field_darkens_leaves_below_real_foliage() {
        let mut leaves = vec![test_leaf(Vec3::ZERO), test_leaf(Vec3::new(2.0, 0.0, 0.0))];
        for layer in 1..=8 {
            let height = layer as f32 * CANOPY_LIGHT_CELL_METRES;
            for x in -2..=2 {
                for y in -2..=2 {
                    leaves.push(test_leaf(Vec3::new(
                        x as f32 * 0.13,
                        y as f32 * 0.13,
                        height,
                    )));
                }
            }
        }
        estimate_leaf_exposure(&mut leaves).expect("canopy exposure");
        assert!(leaves[0].light_exposure + 0.20 < leaves[1].light_exposure);
        assert!(
            leaves
                .iter()
                .all(|leaf| (0.0..=1.0).contains(&leaf.light_exposure))
        );
    }

    #[test]
    fn decussate_stations_form_opposite_pairs_and_reserve_the_tip() {
        let base = 0.37;
        assert!((decussate_phase(base, 1) - decussate_phase(base, 0) - PI).abs() < 1.0e-6);
        assert!((decussate_phase(base, 2) - decussate_phase(base, 0) - PI * 0.5).abs() < 1.0e-6);
        assert_eq!(
            decussate_attachment(0, 4).to_bits(),
            decussate_attachment(1, 4).to_bits()
        );
        assert_eq!(
            decussate_attachment(2, 4).to_bits(),
            decussate_attachment(3, 4).to_bits()
        );
        assert!(decussate_attachment(2, 4) > decussate_attachment(0, 4));
        assert_eq!(
            decussate_attachment(0, 4).to_bits(),
            CURRENT_FLUSH_START.to_bits()
        );
        assert_eq!(
            decussate_attachment(7, 4).to_bits(),
            CURRENT_FLUSH_END.to_bits()
        );

        let attachments: [f32; 5] = std::array::from_fn(|node| {
            clustered_decussate_attachment(
                node * 2,
                5,
                CURRENT_FLUSH_START,
                CURRENT_FLUSH_END,
                0.006,
            )
        });
        assert!(attachments.windows(2).all(|pair| pair[0] < pair[1]));
        assert!(
            attachments[1] - attachments[0] > attachments[4] - attachments[3],
            "new terminal internodes should compress toward the growing tip"
        );
        for leaf in 0..10 {
            assert_eq!(
                clustered_decussate_attachment(
                    leaf,
                    5,
                    CURRENT_FLUSH_START,
                    CURRENT_FLUSH_END,
                    0.006,
                )
                .to_bits(),
                clustered_decussate_attachment(
                    leaf ^ 1,
                    5,
                    CURRENT_FLUSH_START,
                    CURRENT_FLUSH_END,
                    0.006,
                )
                .to_bits(),
                "opposite leaves must share one botanical node"
            );
        }
    }

    #[test]
    fn previous_flush_is_basal_older_and_bounded_by_retention() {
        assert_eq!(retained_previous_flush_leaf_count(3, 1.0, 0.0), 0);
        assert_eq!(retained_previous_flush_leaf_count(8, 0.0, 1.0), 0);
        assert_eq!(retained_previous_flush_leaf_count(8, 0.70, 0.80), 2);
        assert_eq!(
            retained_previous_flush_leaf_count(8, 1.0, 0.0),
            MAX_PREVIOUS_FLUSH_LEAVES_PER_SHOOT
        );
        let previous =
            decussate_attachment_in_range(3, 2, PREVIOUS_FLUSH_START, PREVIOUS_FLUSH_END);
        assert!(previous <= PREVIOUS_FLUSH_END);
        assert!(previous < CURRENT_FLUSH_START);
        let current_cohort = FoliageCohort {
            size_scale: 1.0,
            upward_bias: 0.0,
            sky_alignment: 0.8,
            roll_bias: 0.0,
            age_centre: 0.52,
        };
        let previous_cohort = previous_foliage_cohort(current_cohort);
        assert!(
            previous_flush_leaf_age(previous_cohort, PREVIOUS_FLUSH_END, -0.065)
                > flush_leaf_age(current_cohort, CURRENT_FLUSH_START, 0.065)
        );
    }

    #[test]
    fn competition_growth_is_productive_and_bounded_across_seeds() {
        let recipe = BotanicalRecipe::default();
        for seed in [42, 666, 2026, 9_001] {
            let graph = generate_competition_graph(seed, recipe).expect("competition graph");
            let terminals = graph.axes.iter().filter(|axis| axis.order == 3).count();
            assert!(terminals >= usize::from(recipe.primary_count) * 2);
            assert!(graph.axes.len() <= MAX_GROWTH_NODES + 1);
            assert!(graph.axes.iter().all(|axis| axis.alive));
        }
    }

    #[test]
    fn crown_environment_is_deterministic_asymmetric_and_bounded() {
        for seed in [3, 42, 666, 2026, 9_001] {
            let environment = crown_environment(seed, 0.37);
            assert_eq!(environment, crown_environment(seed, 0.37));
            assert!((7.4..=8.5).contains(&environment.major_radius_metres));
            assert!((5.2..=6.2).contains(&environment.minor_radius_metres));
            assert!(environment.major_radius_metres > environment.minor_radius_metres);
            assert!((2.55..=3.15).contains(&environment.half_height_metres));
            assert!((0.12..=0.28).contains(&environment.lee_extension));
            assert!((0.55..=1.35).contains(&environment.upper_lean_metres));
            assert!((0.32..=0.52).contains(&environment.gap_half_width));
        }
    }

    #[test]
    fn pohutukawa_crowns_are_broad_low_forking_coastal_forms() {
        for seed in [3, 17, 42, 81, 137, 233, 377, 512, 666, 1_001, 2_026, 9_001] {
            let graph = generate_competition_graph(seed, BotanicalRecipe::default())
                .expect("competition graph");
            let tips: Vec<_> = graph
                .axes
                .iter()
                .filter(|axis| axis.order == 3)
                .map(|axis| axis.points_metres[AXIS_POINTS - 1])
                .collect();
            let vertical_extent = tips.iter().map(|tip| tip.z).fold(
                (f32::INFINITY, f32::NEG_INFINITY),
                |(minimum, maximum), z| (minimum.min(z), maximum.max(z)),
            );
            let horizontal_diameter = tips
                .iter()
                .enumerate()
                .flat_map(|(index, left)| {
                    tips[index + 1..].iter().map(move |right| (*left, *right))
                })
                .map(|(left, right)| {
                    let separation = left - right;
                    separation.x.hypot(separation.y)
                })
                .fold(0.0, f32::max);
            let crown_height = vertical_extent.1 - vertical_extent.0;
            assert!(
                horizontal_diameter / crown_height.max(0.001) >= 1.55,
                "seed {seed} crown is not broad enough: diameter {horizontal_diameter}, height {crown_height}"
            );

            let trunk_height = graph.axes[0].points_metres[AXIS_POINTS - 1].z;
            let low_scaffolds = graph
                .axes
                .iter()
                .filter(|axis| {
                    axis.order == 1
                        && axis.parent == Some(0)
                        && axis.points_metres[0].z <= trunk_height * 0.38
                })
                .count();
            assert!(
                low_scaffolds >= 2,
                "seed {seed} has only {low_scaffolds} low scaffold limbs"
            );
        }
    }

    #[test]
    fn competition_crowns_meet_bounded_shape_gates() {
        for seed in [3, 17, 42, 81, 137, 233, 377, 512, 666, 1_001, 2_026, 9_001] {
            let graph = generate_competition_graph(seed, BotanicalRecipe::default())
                .expect("competition graph");
            let metrics = crown_metrics(&graph);
            assert!(
                (1.07..=1.65).contains(&metrics.aspect),
                "seed {seed} crown aspect {} is outside the species gate",
                metrics.aspect
            );
            assert!(
                (0.45..=2.60).contains(&metrics.horizontal_offset_metres),
                "seed {seed} crown offset {} is outside the species gate",
                metrics.horizontal_offset_metres
            );
            assert!(
                (0.38..=0.88).contains(&metrics.leader_efficiency),
                "seed {seed} leader efficiency {} is outside the species gate",
                metrics.leader_efficiency
            );
            assert!(
                (0.90..=1.0).contains(&metrics.leader_alignment),
                "seed {seed} leader alignment {} is outside the species gate",
                metrics.leader_alignment
            );
            assert!(
                metrics.largest_vertical_gap <= 0.075,
                "seed {seed} vertical gap {} is outside the species gate",
                metrics.largest_vertical_gap
            );
            assert!(
                metrics.tier_valley <= 0.50,
                "seed {seed} tier valley {} is outside the species gate",
                metrics.tier_valley
            );
            assert!(
                (0.14..=0.56).contains(&metrics.radial_concavity),
                "seed {seed} radial concavity {} is outside the species gate",
                metrics.radial_concavity
            );
            assert!(
                metrics.front_tier_valley <= 0.32 && metrics.quarter_tier_valley <= 0.32,
                "seed {seed} projected tier valleys ({}, {}) are outside the species gate",
                metrics.front_tier_valley,
                metrics.quarter_tier_valley
            );
        }
    }

    #[test]
    fn branch_runs_cover_every_live_axis_and_join_continuations() {
        for seed in [42, 666, 2026] {
            let graph = generate_graph(seed, BotanicalRecipe::default()).expect("graph");
            let runs = branch_runs(&graph);
            let live_axes = graph.axes.iter().filter(|axis| axis.alive).count();
            assert_eq!(
                runs.iter().map(|run| run.axis_count).sum::<usize>(),
                live_axes
            );
            assert!(runs.len() < live_axes);
            assert!(runs.iter().any(|run| run.axis_count > 1));
            assert!(runs.iter().all(|run| {
                run.samples.len() >= AXIS_POINTS
                    && run
                        .samples
                        .iter()
                        .all(|sample| sample.position.is_finite() && sample.radius > 0.0)
            }));
        }
    }

    #[test]
    fn persistent_stub_is_short_thick_blunt_and_finite() {
        let support = Axis {
            parent: Some(0),
            order: 1,
            points_metres: std::array::from_fn(|index| Vec3::new(index as f32 * 0.32, 0.0, 4.0)),
            radii_metres: [0.13, 0.125, 0.12, 0.115, 0.11],
            exposure: 0.7,
            alive: true,
        };
        let stub = persistent_dead_stub(support, 7, 0.5);
        let length = stub.points_metres[0].distance(stub.points_metres[AXIS_POINTS - 1]);
        let root_radius = stub.radii_metres[0];
        let tip_ratio = stub.radii_metres[AXIS_POINTS - 1] / root_radius;
        assert_eq!(stub.parent, Some(7));
        assert_eq!(stub.order, 1);
        assert!((0.10..=0.24).contains(&length));
        assert!((2.5..=4.5).contains(&(length / root_radius)));
        assert!((0.75..=0.90).contains(&tip_ratio));
        assert!(stub.points_metres.iter().all(|point| point.is_finite()));
    }

    #[test]
    fn mature_bark_relief_is_seam_safe_bounded_and_maturity_gated() {
        let trunk =
            |angle, longitudinal| mature_bark_radial_offset(0, 0.58, angle, longitudinal, 0.37);
        assert!((trunk(0.0, 1.4) - trunk(TAU, 1.4)).abs() < 1.0e-5);
        let (minimum, maximum) = (0..=64)
            .flat_map(|angle| {
                (0..=48).map(move |height| trunk(angle as f32 / 64.0 * TAU, height as f32 / 12.0))
            })
            .fold(
                (f32::INFINITY, f32::NEG_INFINITY),
                |(minimum, maximum), value| (minimum.min(value), maximum.max(value)),
            );
        assert!(minimum >= -0.040);
        assert!(maximum <= 0.040);
        assert!(maximum - minimum > 0.018);
        assert_eq!(
            mature_bark_radial_offset(3, 0.04, 0.7, 1.2, 0.37).to_bits(),
            0.0_f32.to_bits()
        );
        assert!(mature_bark_radial_offset(1, 0.015, 0.7, 1.2, 0.37).abs() < 0.001);
    }

    #[test]
    fn fine_shoot_retention_tracks_terminal_vigour() {
        let graph = generate_graph(42, BotanicalRecipe::default()).expect("graph");
        let terminal = *graph
            .axes
            .iter()
            .find(|axis| axis.order == 3 && axis.alive)
            .expect("terminal axis");
        let low_axis = Axis {
            exposure: 0.0,
            ..terminal
        };
        let high_axis = Axis {
            exposure: 1.0,
            ..terminal
        };
        let low = terminal_vigour(&low_axis);
        let high = terminal_vigour(&high_axis);
        assert!(low < high);
        assert!(retained_fine_shoot_count(low) < retained_fine_shoot_count(high));
        assert!(
            retained_leaf_budget(BotanicalRecipe::default().leaves_per_terminal, low)
                < retained_leaf_budget(BotanicalRecipe::default().leaves_per_terminal, high)
        );

        let retained = graph
            .axes
            .iter()
            .filter(|axis| axis.order == 3 && axis.alive)
            .map(|axis| retained_fine_shoot_count(terminal_vigour(axis)));
        let minimum = retained.clone().min().expect("retained shoots");
        let maximum = retained.max().expect("retained shoots");
        assert!(minimum >= MIN_FINE_SHOOTS_PER_TERMINAL);
        assert!(maximum <= MAX_FINE_SHOOTS_PER_TERMINAL);
        assert!(minimum < maximum);
    }

    #[test]
    fn foliage_cohorts_are_bounded_and_exposure_responsive() {
        let shade = foliage_cohort(42, 0.7, 0.0, false);
        let sun = foliage_cohort(42, 0.7, 1.0, false);
        let secondary = foliage_cohort(42, 0.7, 0.0, true);
        assert_eq!(shade, foliage_cohort(42, 0.7, 0.0, false));
        assert!(shade.size_scale > sun.size_scale);
        assert!(shade.sky_alignment > sun.sky_alignment);
        assert!(secondary.size_scale < shade.size_scale);
        for cohort in [shade, sun, secondary] {
            assert!((0.82..=1.13).contains(&cohort.size_scale));
            assert!((-0.04..=0.14).contains(&cohort.upward_bias));
            assert!((0.68..=0.98).contains(&cohort.sky_alignment));
            assert!((-0.22..=0.22).contains(&cohort.roll_bias));
            assert!((0.30..=0.76).contains(&cohort.age_centre));
        }
    }

    #[test]
    fn flush_leaf_age_is_coherent_and_younger_toward_the_tip() {
        let cohort = foliage_cohort(42, 0.7, 0.5, false);
        let basal = flush_leaf_age(cohort, CURRENT_FLUSH_START, 0.0);
        let distal = flush_leaf_age(cohort, CURRENT_FLUSH_END, 0.0);
        assert!(basal > distal);
        assert!((0.08..=0.98).contains(&flush_leaf_age(cohort, CURRENT_FLUSH_START, -0.065,)));
        assert!((0.08..=0.98).contains(&flush_leaf_age(cohort, CURRENT_FLUSH_END, 0.065,)));
    }

    #[test]
    fn seasonal_turn_preserves_the_old_base_and_bends_the_distal_flush() {
        let mut shoot = Axis {
            parent: Some(0),
            order: 4,
            points_metres: std::array::from_fn(|index| Vec3::Z * index as f32 * 0.1),
            radii_metres: [0.01; AXIS_POINTS],
            exposure: 0.7,
            alive: true,
        };
        let original = shoot;
        apply_seasonal_turn(&mut shoot, Vec3::X, Vec3::Y, 0.0, 0.8);
        assert_eq!(shoot.points_metres[0], original.points_metres[0]);
        assert_eq!(shoot.points_metres[1], original.points_metres[1]);
        assert!(shoot.points_metres[2].y > original.points_metres[2].y);
        assert!(shoot.points_metres[4].y > shoot.points_metres[2].y);
        assert!(shoot.points_metres[4].z > original.points_metres[4].z);
    }

    #[test]
    fn secondary_fine_ramification_is_sparse_bounded_and_leaf_neutral() {
        for primary_count in MIN_FINE_SHOOTS_PER_TERMINAL..=MAX_FINE_SHOOTS_PER_TERMINAL {
            assert_eq!(retained_secondary_fine_shoot_count(0.0, primary_count), 0);
            let secondary_count = retained_secondary_fine_shoot_count(1.0, primary_count);
            assert!(secondary_count > 0);
            assert!(secondary_count <= primary_count / 2);
            assert_eq!(
                (0..primary_count)
                    .filter(|&index| receives_secondary_fine_shoot(
                        index,
                        primary_count,
                        secondary_count
                    ))
                    .count(),
                secondary_count
            );
        }
        assert_eq!(
            retained_secondary_fine_shoot_count(1.0, MAX_FINE_SHOOTS_PER_TERMINAL),
            MAX_SECONDARY_FINE_SHOOTS_PER_TERMINAL
        );
        for original in MIN_LEAVES_FOR_SECONDARY_FINE_SHOOT..=10 {
            let lateral = secondary_fine_shoot_leaf_count(original);
            assert!(lateral.is_multiple_of(2));
            assert!(lateral < original);
            assert_eq!((original - lateral) + lateral, original);
        }
        assert_eq!(
            decussate_attachment(0, 1).to_bits(),
            f32::midpoint(CURRENT_FLUSH_START, CURRENT_FLUSH_END).to_bits()
        );
        assert_eq!(
            decussate_attachment(0, 1).to_bits(),
            decussate_attachment(1, 1).to_bits()
        );
    }

    #[test]
    fn leaf_archetypes_have_petiole_relief_and_age_semantics() {
        let archetypes = leaf_archetypes();
        for (archetype, mesh) in archetypes.iter().enumerate() {
            assert_mesh(mesh);
            assert!(mesh.vertices.len() >= 230);
            assert!(mesh.vertices.iter().any(|vertex| vertex.x < -0.09));
            let station = LEAF_STATION_COUNT / 2;
            let row = station * LEAF_COLUMNS.len();
            let centre_column = LEAF_COLUMNS.len() / 2;
            let left = mesh.vertices[row + centre_column - 1];
            let centre = mesh.vertices[row + centre_column];
            let right = mesh.vertices[row + centre_column + 1];
            assert!(centre.z > left.z.min(right.z));
            let edge_height_delta =
                (mesh.vertices[row].z - mesh.vertices[row + LEAF_COLUMNS.len() - 1].z).abs();
            assert!((0.015..=0.065).contains(&edge_height_delta));
            let surface_vertex_count = LEAF_STATION_COUNT * LEAF_COLUMNS.len();
            let lower_left = mesh.vertices[surface_vertex_count + station];
            assert!((mesh.vertices[row].z - lower_left.z - 0.004).abs() < 1.0e-6);
            let tile = usize::from(LEAF_SHAPES[archetype].atlas_tile);
            let column = tile % LEAF_ATLAS_COLUMNS as usize;
            let row = tile / LEAF_ATLAS_COLUMNS as usize;
            let minimum = Vec2::new(column as f32 * 0.5, row as f32 * 0.5);
            let maximum = minimum + Vec2::splat(0.5);
            assert!(mesh.uv.iter().all(|uv| uv.cmpgt(minimum).all()));
            assert!(mesh.uv.iter().all(|uv| uv.cmplt(maximum).all()));
        }
        for archetype in 0..4 {
            assert_ne!(
                archetypes[archetype].vertices,
                archetypes[archetype + 4].vertices
            );
            assert_eq!(
                LEAF_SHAPES[archetype].atlas_tile,
                LEAF_SHAPES[archetype + 4].atlas_tile
            );
        }
        assert_eq!(leaf_archetype(0.20, 1.0), 1);
        assert_eq!(leaf_archetype(0.20, 4.0), 5);
        assert_eq!(leaf_archetype(0.50, PI * 0.25), 0);
        assert_eq!(leaf_archetype(0.50, PI * 1.25), 4);
        assert_eq!(leaf_archetype(0.88, PI * 0.25), 2);
        assert_eq!(leaf_archetype(0.88, PI * 0.75), 6);
        assert_eq!(leaf_archetype(0.88, PI * 1.25), 3);
        assert_eq!(leaf_archetype(0.88, PI * 1.75), 7);
    }

    #[test]
    fn leaf_profile_has_an_offset_shoulder_and_a_long_distal_taper() {
        let profile = |x| leaf_profile_width(x, 0.43, 0.58, 0.82);
        assert!(profile(0.0) < 0.01);
        assert!(profile(0.43) > 0.99);
        assert!(profile(0.75) > 0.60);
        assert!(profile(0.85) < 0.52);
        assert!(profile(1.0) < 0.01);
    }

    #[test]
    fn pads_are_derived_from_real_terminal_leaf_envelopes() {
        let prototype =
            generate_botanical_prototype(666, BotanicalRecipe::default()).expect("prototype");
        for pad in &prototype.foliage_pads {
            let leaves = prototype.leaves.iter().filter(|leaf| leaf.axis == pad.axis);
            assert!(leaves.count() > 0);
            assert!(pad.centre_metres.is_finite());
            assert!(pad.half_extents_metres.cmpgt(Vec3::ZERO).all());
            assert!(pad.density > 0.0 && pad.density <= 1.0);
            assert!((0.0..=1.0).contains(&pad.light_exposure));
        }
        for archetype in &prototype.foliage_pad_archetypes {
            assert!(archetype.vertices.len() <= 1_000);
            assert!(archetype.triangles.len() <= 4_000);
        }
    }

    #[test]
    fn generated_textures_have_complete_rgba_storage() {
        let prototype =
            generate_botanical_prototype(2026, BotanicalRecipe::default()).expect("prototype");
        for texture in [
            &prototype.bark_albedo,
            &prototype.bark_normal,
            &prototype.bark_depth,
            &prototype.bark_metallic_roughness,
            &prototype.wood_scar_albedo,
            &prototype.leaf_albedo,
            &prototype.leaf_metallic_roughness,
        ] {
            assert_eq!(
                texture.rgba.len(),
                (texture.width * texture.height * 4) as usize
            );
            let (pixels, remainder) = texture.rgba.as_chunks::<4>();
            assert!(remainder.is_empty());
            assert!(pixels.iter().all(|pixel| pixel[3] == 255));
        }
        assert_eq!(prototype.leaf_albedo.width, LEAF_ATLAS_SIZE);
        assert_eq!(prototype.leaf_albedo.height, LEAF_ATLAS_SIZE);
    }

    #[test]
    fn bark_height_wraps_and_depth_is_grayscale_with_relief() {
        let seed = 0x07ea ^ TEXTURE_SEED_DOMAIN;
        let width = BARK_TEXTURE_WIDTH.cast_signed();
        let height = BARK_TEXTURE_HEIGHT.cast_signed();
        for (x, y) in [(-1, -1), (0, 0), (37, 211), (width - 1, height - 1)] {
            let sample = bark_height(seed, x, y);
            assert_eq!(sample.to_bits(), bark_height(seed, x + width, y).to_bits());
            assert_eq!(sample.to_bits(), bark_height(seed, x, y + height).to_bits());
        }

        let depth = bark_depth_texture(seed);
        let mut darkest = u8::MAX;
        let mut lightest = u8::MIN;
        let (texels, remainder) = depth.rgba.as_chunks::<4>();
        assert!(remainder.is_empty());
        for texel in texels {
            assert_eq!(texel[0], texel[1]);
            assert_eq!(texel[1], texel[2]);
            assert_eq!(texel[3], u8::MAX);
            darkest = darkest.min(texel[0]);
            lightest = lightest.max(texel[0]);
        }
        assert!(lightest.saturating_sub(darkest) > 32);
    }

    #[test]
    fn bark_micro_relief_is_bounded_and_contains_fibres_and_pores() {
        let seed = 0x07ea ^ TEXTURE_SEED_DOMAIN;
        let (minimum, maximum) = (0..BARK_TEXTURE_HEIGHT)
            .step_by(7)
            .flat_map(|y| {
                (0..BARK_TEXTURE_WIDTH)
                    .step_by(5)
                    .map(move |x| bark_micro_relief(seed, x, y))
            })
            .fold((f32::MAX, f32::MIN), |(minimum, maximum), sample| {
                (minimum.min(sample), maximum.max(sample))
            });
        assert!((-0.060..=-0.020).contains(&minimum));
        assert!((0.004..=0.018).contains(&maximum));
    }
}
