//! Stable equal-area roots, independent of terrain tessellation and render LOD.
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss
)]
use crate::{Scene, collision::Ground, command::BedView};
use bevy::prelude::*;
use planet_rs::{DVec3, chunks::CellLocator};
use std::f32::consts::TAU;

pub(super) const VARIANTS: usize = 4;
const SPACING: f32 = 10.0;
const MAX_CANDIDATES: usize = 200_000;
const MAX_TREES: usize = 64_000;

pub(super) struct Plant {
    pub transform: Transform,
    pub root: Vec3,
    pub owner: u32,
    pub variant: usize,
}
pub(super) struct Built {
    pub plants: Vec<Plant>,
    pub undergrowth: super::undergrowth::Built,
    pub prototypes: Vec<motu::TreeMeshes>,
    pub ground: Ground,
    pub upper: f32,
    pub occluder: f32,
    pub pixels: Vec<u8>,
}

pub(super) fn build(scene: Scene, shared: Option<Ground>, density: f32) -> Built {
    let ground = shared.unwrap_or_else(|| Ground::new(&scene));
    let radius = scene.terrain.topology.radius as f32;
    let base_count = ((4.0 * std::f32::consts::PI * radius * radius / SPACING.powi(2)) as usize)
        .clamp(32, MAX_CANDIDATES);
    let count = (base_count as f32 * density) as usize;
    let mut accepted = 0;
    let locator = CellLocator::new(2).expect("fixed forest cell depth");
    let original = scene.bed == BedView::Original;
    let mut plants = Vec::new();
    for index in 0..count {
        let up = candidate(index, count, scene.seed);
        if coverage(up * radius, scene.seed) < 0.58 {
            continue;
        }
        let Some((point, normal, material)) = ground.forest_sample(&scene.terrain, up, original)
        else {
            continue;
        };
        if !suitable(point, normal, material, radius) {
            continue;
        }
        let tangent = up.any_orthonormal_vector();
        let bitangent = up.cross(tangent);
        // Crown clearance from channels, beaches and cliffs, measured along
        // the sphere rather than in a world XZ plane.
        let clear = [tangent, -tangent, bitangent, -bitangent]
            .into_iter()
            .all(|offset| {
                let probe = (point + offset * (3.0 / radius)).normalize();
                ground
                    .forest_sample(&scene.terrain, probe, original)
                    .is_some_and(|(p, n, m)| suitable(p, n, m, radius))
            });
        if !clear {
            continue;
        }
        let key = mix(scene.seed ^ index as u64 ^ 0x7472_6565_726f_6f74);
        let height = (point.length() - 1.0) * radius;
        let temperature = mean_temperature(up, height, scene.seed);
        let survival = ((temperature + 10.0) / 6.0).clamp(0.0, 1.0);
        if unit(mix(key ^ 0x626f_7265_616c)) > survival {
            continue;
        }
        let species = species_offset(unit(mix(key ^ 0x7069_6e65)), temperature, up.y, height);
        let scale = (1.0 + unit(key) * 0.60) * (0.70 + 0.30 * survival);
        let rotation =
            Quat::from_rotation_arc(Vec3::Z, up) * Quat::from_rotation_z(unit(mix(key)) * TAU);
        let root = point;
        let plant = Plant {
            root,
            transform: Transform::from_translation(root)
                .with_rotation(rotation)
                .with_scale(Vec3::splat(motu::ISLAND_WORLD_METRES * scale / radius)),
            owner: locator
                .owner(DVec3::from_array(up.to_array().map(f64::from)))
                .unwrap(),
            variant: (mix(key ^ 0x0013) % VARIANTS as u64) as usize + species * VARIANTS,
        };
        accepted += 1;
        if plants.len() < MAX_TREES {
            plants.push(plant);
        } else {
            let slot = mix(key ^ 0x7265_7365_7276) as usize % accepted;
            if slot < MAX_TREES {
                plants[slot] = plant;
            }
        }
    }
    let mut prototypes: Vec<_> = [2018, 42, 1234, 98765]
        .into_iter()
        .map(motu::generate_tree)
        .collect();
    prototypes.extend((0..VARIANTS).map(super::models::conifer));
    prototypes.extend((0..VARIANTS).map(super::models::palm));
    fit_roots(&mut plants, &prototypes, &ground, &scene);
    let upper = 1.0 + scene.terrain.elevations.iter().copied().fold(0.0, f64::max) as f32;
    let occluder = 1.0 + scene.terrain.elevations.iter().copied().fold(0.0, f64::min) as f32;
    let pixels = super::mask::build(&plants, radius);
    let undergrowth = super::undergrowth::build(&scene, &ground, &plants);
    Built {
        plants,
        undergrowth,
        prototypes,
        ground,
        upper,
        occluder,
        pixels,
    }
}

fn fit_roots(
    plants: &mut [Plant],
    prototypes: &[motu::TreeMeshes],
    ground: &Ground,
    scene: &Scene,
) {
    let radius = scene.terrain.topology.radius as f32;
    let original = scene.bed == BedView::Original;
    // Cache each shared base rim once; fitting only retained trees bounds terrain queries.
    let rims: Vec<_> = prototypes.iter().map(base_rim).collect();
    for plant in plants {
        plant.root = fitted_root(
            plant.root,
            plant.transform.rotation,
            plant.transform.scale.x,
            &rims[plant.variant],
            radius,
            |up| {
                ground
                    .forest_sample(&scene.terrain, up, original)
                    .map(|s| s.0)
            },
        );
        plant.transform.translation = plant.root;
    }
}

fn base_rim(tree: &motu::TreeMeshes) -> Vec<Vec3> {
    let mut rim = Vec::new();
    for point in tree
        .lod0_wood
        .vertices
        .iter()
        .chain(&tree.lod1_wood.vertices)
        .chain(&tree.lod2_wood.vertices)
        .filter(|p| p.z.abs() <= f32::EPSILON)
    {
        let point = Vec3::from_array(point.to_array());
        if !rim.contains(&point) {
            rim.push(point);
        }
    }
    rim
}

fn fitted_root(
    point: Vec3,
    rotation: Quat,
    scale: f32,
    rim: &[Vec3],
    radius: f32,
    sample: impl Fn(Vec3) -> Option<Vec3>,
) -> Vec3 {
    let up = point.normalize();
    let centre_height = sample(up).unwrap_or(point).dot(up);
    let lowest = rim
        .iter()
        .filter_map(|p| {
            let offset = rotation * (*p * scale);
            sample((point + offset).normalize()).map(|ground| (ground - offset).dot(up))
        })
        .fold(centre_height, f32::min);
    up * (lowest - 0.08 / radius)
}

fn suitable(point: Vec3, normal: Vec3, material: [f32; 4], radius: f32) -> bool {
    let height = (point.length() - 1.0) * radius;
    (4.0..400.0).contains(&height)
        && normal.dot(point.normalize()) >= 0.927_183_87
        && material[1] >= 0.25
        && material[2] < 0.03
}

// Annual-mean estimate mirrors the weather model's latitude/lapse-rate baseline.
// Mature biomes do not change species when a transient cold front passes.
pub(crate) fn mean_temperature(up: Vec3, height: f32, seed: u64) -> f32 {
    20.0 - 30.0 * up.y * up.y - height.max(0.0) * 0.09
        + (noise(up * 5.0, seed ^ 0x636c_696d_6174) - 0.5) * 4.0
}
pub(super) fn conifer_weight(temperature: f32) -> f32 {
    let cold = ((12.0 - temperature) / 10.0).clamp(0.0, 1.0);
    cold * cold * (3.0 - 2.0 * cold)
}

fn palm_weight(temperature: f32, latitude_sine: f32, height: f32) -> f32 {
    let warmth = smooth_unit((temperature - 12.0) / 8.0);
    let lowland = 1.0 - smooth_unit((height - 40.0) / 120.0);
    let tropical = 1.0 - smooth_unit((latitude_sine * latitude_sine - 0.08) / 0.42);
    0.72 * warmth * lowland * tropical
}

fn species_offset(choice: f32, temperature: f32, latitude_sine: f32, height: f32) -> usize {
    let conifer = conifer_weight(temperature);
    if choice < conifer {
        1
    } else if choice < conifer + (1.0 - conifer) * palm_weight(temperature, latitude_sine, height) {
        2
    } else {
        0
    }
}

fn smooth_unit(value: f32) -> f32 {
    let value = value.clamp(0.0, 1.0);
    value * value * (3.0 - 2.0 * value)
}

fn candidate(index: usize, count: usize, seed: u64) -> Vec3 {
    let y = 1.0 - 2.0 * (index as f32 + 0.5) / count as f32;
    let azimuth = index as f64 * 2.399_963_229_728_653 + f64::from(unit(mix(seed)) * TAU);
    let radial = (1.0 - y * y).sqrt();
    let up = Vec3::new(
        radial * azimuth.cos() as f32,
        y,
        radial * azimuth.sin() as f32,
    );
    let key = mix(seed ^ index as u64 ^ 0x6a69_7474_6572);
    let angle = unit(key) * TAU;
    let extent = (4.0 * std::f32::consts::PI / count as f32).sqrt() * 0.22 * unit(mix(key)).sqrt();
    let tangent = up.any_orthonormal_vector();
    (up + (tangent * angle.cos() + up.cross(tangent) * angle.sin()) * extent).normalize()
}

fn mix(mut value: u64) -> u64 {
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}
fn unit(value: u64) -> f32 {
    (value >> 40) as f32 / 16_777_215.0
}
pub(crate) fn noise(point: Vec3, seed: u64) -> f32 {
    let cell = point.floor().as_ivec3();
    // Use point-floor for negative coordinates too: the field wraps all poles.
    let f = point - point.floor();
    let f = f * f * (Vec3::splat(3.0) - 2.0 * f);
    let sample = |x: i32, y: i32, z: i32| {
        unit(mix(seed
            ^ (i64::from(cell.x + x) as u64).wrapping_mul(0x8da6_b343)
            ^ (i64::from(cell.y + y) as u64).wrapping_mul(0xd816_3841)
            ^ (i64::from(cell.z + z) as u64).wrapping_mul(0xcb1a_b31f)))
    };
    let plane = |z| {
        let a = sample(0, 0, z).lerp(sample(1, 0, z), f.x);
        let b = sample(0, 1, z).lerp(sample(1, 1, z), f.x);
        a.lerp(b, f.y)
    };
    plane(0).lerp(plane(1), f.z)
}
pub(super) fn coverage(point: Vec3, seed: u64) -> f32 {
    noise(point / 75.0, seed ^ 0x8c2d_4e7a_51b3_9f06) * 0.70
        + noise(point / 24.0, seed ^ 0x1f3d_5b79_a7c9_e2d4) * 0.30
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_species_roots_bury_the_complete_shared_rim_on_radial_slopes() {
        for tree in [
            motu::generate_tree(2018),
            motu::generate_conifer(0),
            motu::generate_palm(0),
        ] {
            let rim = base_rim(&tree);
            assert!(!rim.is_empty());
            for up in [Vec3::Y, -Vec3::Y, Vec3::X] {
                let point = up * 1.02;
                let tangent = up.any_orthonormal_vector();
                for slope in [-0.4, 0.0, 0.4] {
                    let normal = (up - tangent * slope).normalize();
                    let sample =
                        |probe: Vec3| Some(probe * (normal.dot(point) / normal.dot(probe)));
                    for scale in [0.7, 1.0, 2.0] {
                        let scale = scale * motu::ISLAND_WORLD_METRES / 1000.0;
                        let rotation =
                            Quat::from_rotation_arc(Vec3::Z, up) * Quat::from_rotation_z(0.9);
                        let root = fitted_root(point, rotation, scale, &rim, 1000.0, sample);
                        for p in &rim {
                            let placed = root + rotation * (*p * scale);
                            assert!(normal.dot(placed) < normal.dot(point));
                        }
                        assert!(
                            root.distance(fitted_root(root, rotation, scale, &rim, 1000.0, sample))
                                < 1e-6
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn palms_have_a_smooth_mixed_warm_lowland_range_and_no_cold_or_high_range() {
        assert_eq!(palm_weight(20.0, 0.0, 0.0), 0.72);
        assert!(palm_weight(18.0, 0.0, 20.0) > palm_weight(16.0, 0.0, 20.0));
        assert!(palm_weight(20.0, 0.0, 60.0) > palm_weight(20.0, 0.0, 100.0));
        for latitude in [-1.0, -0.8, 0.8, 1.0] {
            assert_eq!(palm_weight(20.0, latitude, 20.0), 0.0);
        }
        assert_eq!(palm_weight(12.0, 0.0, 20.0), 0.0);
        assert_eq!(palm_weight(20.0, 0.0, 160.0), 0.0);
        let mut counts = [0; 3];
        for i in 0..1000 {
            counts[species_offset(i as f32 / 1000.0, 20.0, 0.0, 20.0)] += 1;
            assert_ne!(species_offset(i as f32 / 1000.0, 0.0, 0.0, 20.0), 2);
            assert_ne!(species_offset(i as f32 / 1000.0, 20.0, 0.0, 180.0), 2);
        }
        assert_eq!(counts, [280, 0, 720]);
        assert_eq!(species_offset(0.5, 0.0, 0.0, 20.0), 1);
        assert_eq!(palm_weight(20.0, 0.4, 20.0), palm_weight(20.0, -0.4, 20.0));
    }

    #[test]
    fn cold_biomes_follow_latitude_and_elevation_with_a_mixed_transition() {
        let equator = mean_temperature(Vec3::X, 20.0, 42);
        let mountain = mean_temperature(Vec3::X, 200.0, 42);
        let boreal = mean_temperature(Vec3::new(0.6, 0.8, 0.0), 20.0, 42);
        assert!(equator > mountain + 10.0 && equator > boreal + 10.0);
        assert!(conifer_weight(equator) < 0.05);
        assert!(conifer_weight(mountain) > 0.7 && conifer_weight(boreal) > 0.9);
        assert!((conifer_weight(7.0) - 0.5).abs() < 1e-6);
        assert!(mean_temperature(Vec3::Y, 300.0, 42) < -10.0);
        assert!(mean_temperature(-Vec3::Y, 300.0, 42) < -10.0);
    }
    #[test]
    fn roots_are_repeatable_equal_area_and_radially_upright() {
        for index in 0..1000 {
            let up = candidate(index, 1000, 42);
            assert!((up.length() - 1.0).abs() < 1.0e-6);
            assert_eq!(up, candidate(index, 1000, 42));
            assert_ne!(up, candidate(index, 1000, 43));
            let rotation = Quat::from_rotation_arc(Vec3::Z, up) * Quat::from_rotation_z(1.8);
            assert!((rotation * Vec3::Z).dot(up) > 0.99999);
            assert!((0.0..=1.0).contains(&coverage(up * 1000.0, 42)));
        }
    }

    #[test]
    fn beaches_channels_cliffs_and_snow_are_excluded_on_every_hemisphere() {
        for up in [Vec3::Y, -Vec3::Y, Vec3::X, -Vec3::Z] {
            let point = up * 1.02;
            assert!(suitable(point, up, [0.3, 0.8, 0.0, 0.0], 1000.0));
            assert!(!suitable(up * 1.001, up, [0.3, 0.8, 0.0, 1.0], 1000.0));
            assert!(!suitable(point, up, [0.3, 0.8, 0.2, 0.0], 1000.0));
            assert!(!suitable(
                point,
                up.any_orthonormal_vector(),
                [0.3, 0.8, 0.0, 0.0],
                1000.0
            ));
            assert!(!suitable(up * 1.5, up, [0.3, 0.8, 0.0, 0.0], 1000.0));
        }
    }

    #[test]
    fn placement_samples_the_displayed_mesh_and_interpolates_its_material() {
        let mut scene = crate::collision::test_scene();
        let terrain = std::sync::Arc::get_mut(&mut scene.terrain).unwrap();
        terrain.elevations.fill(0.025);
        terrain.soil_depth.fill(3.0 / terrain.topology.radius);
        terrain.channel_depths.fill(0.0);
        let ground = Ground::new(&scene);
        for up in [Vec3::Y, -Vec3::Y, Vec3::X, -Vec3::X, Vec3::Z, -Vec3::Z] {
            let (point, normal, material) =
                ground.forest_sample(&scene.terrain, up, false).unwrap();
            let radius = ground.radius(&scene.terrain, up).unwrap();
            assert!(point.distance(up * radius) < 1.0e-6);
            assert!(normal.dot(up) > 0.95);
            assert!((material[1] - 0.84375).abs() < 1.0e-5);
            assert_eq!(material[2].to_bits(), 0.0_f32.to_bits());
        }
    }
}
