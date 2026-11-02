//! Root rows follow the actual clipped banks, in radial metres on every hemisphere.
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss
)]
use super::{Plant, Settings};
use crate::{
    Scene,
    collision::Ground,
    command::BedView,
    forest::{mean_temperature, noise},
    surface,
};
use bevy::prelude::*;
use std::collections::{BTreeMap, HashMap};
use std::f32::consts::TAU;
struct Edge {
    a: Vec3,
    b: Vec3,
    wet: Vec3,
    river: bool,
}
fn mix(mut key: u64) -> u64 {
    key = (key ^ (key >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    key = (key ^ (key >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    key ^ (key >> 31)
}
fn unit(key: u64) -> f32 {
    (mix(key) >> 40) as f32 / 16_777_216.0
}
fn edges(scene: &Scene) -> Vec<Edge> {
    let terrain = &scene.terrain;
    let water = &terrain.river_surface;
    let mut counts = BTreeMap::new();
    for &[a, b, c] in &water.triangles {
        for (a, b, c) in [(a, b, c), (b, c, a), (c, a, b)] {
            let entry = counts.entry((a.min(b), a.max(b))).or_insert((0, c));
            entry.0 += 1;
        }
    }
    let mut edges: Vec<_> = counts
        .into_iter()
        .filter_map(|((a, b), (count, c))| {
            let a = water.positions[a as usize];
            let b = water.positions[b as usize];
            (count == 1 && a.length().min(b.length()) > 1.0 + 1e-9).then(|| Edge {
                a: surface::vector(a),
                b: surface::vector(b),
                wet: surface::vector(water.positions[c as usize]),
                river: true,
            })
        })
        .collect();
    for face in &terrain.topology.triangles {
        let ids = face.map(|i| i as usize);
        let h = ids.map(|i| terrain.elevations[i]);
        if !h.iter().any(|&h| h < 0.0) || !h.iter().any(|&h| h > 0.0) {
            continue;
        }
        // Newly excavated inland beds are still rivers, not new sea shores.
        if !ids.iter().any(|&i| {
            terrain.elevations[i] < 0.0
                && terrain.elevations[i] + terrain.channel_depths[i] - terrain.bank_lifts[i] < 0.0
        }) || ids.iter().any(|&i| {
            terrain.water_elevations[i] > 0.0
                && terrain.water_elevations[i] > terrain.elevations[i] + 1e-8
        }) {
            continue;
        }
        let points = ids.map(|i| surface::vector(terrain.topology.directions[i]));
        let mut cut = [Vec3::ZERO; 2];
        let mut count = 0;
        for (a, b) in [(0, 1), (1, 2), (2, 0)] {
            if (h[a] > 0.0) != (h[b] > 0.0) {
                let t = (h[a] / (h[a] - h[b])) as f32;
                cut[count] = points[a].lerp(points[b], t).normalize();
                count += 1;
            }
        }
        if count == 2 {
            let wet = points[h.iter().position(|&h| h < 0.0).unwrap()];
            edges.push(Edge {
                a: cut[0],
                b: cut[1],
                wet,
                river: false,
            });
        }
    }
    edges
}
#[derive(Default)]
struct Spacing(HashMap<[i32; 3], Vec<Vec3>>);
impl Spacing {
    fn accept(&mut self, point: Vec3, spacing: f32) -> bool {
        let cell = (point / spacing).floor().as_ivec3();
        for x in -1..=1 {
            for y in -1..=1 {
                for z in -1..=1 {
                    if self
                        .0
                        .get(&(cell + IVec3::new(x, y, z)).to_array())
                        .is_some_and(|points| {
                            points
                                .iter()
                                .any(|p| p.distance_squared(point) < spacing * spacing)
                        })
                    {
                        return false;
                    }
                }
            }
        }
        self.0.entry(cell.to_array()).or_default().push(point);
        true
    }
}
fn near_fall(scene: &Scene, up: Vec3) -> bool {
    let p = planet_rs::DVec3::new(f64::from(up.x), f64::from(up.y), f64::from(up.z));
    let margin = 3.0 / scene.terrain.topology.radius;
    scene.terrain.waterfalls.iter().any(|fall| {
        if p.dot(fall.lip) < 0.999 {
            return false;
        }
        let along = (p - fall.lip).dot(fall.direction);
        let side = p.dot(fall.lip.cross(fall.direction)).abs();
        along >= -fall.face_run * 2.0 - margin
            && along <= fall.half_width * 1.5 + margin
            && side < fall.half_width * 2.5 + margin
    })
}
#[allow(clippy::too_many_lines)] // One deterministic pass owns spacing and the bounded reservoir.
pub(super) fn build(scene: &Scene, ground: &Ground, settings: Settings) -> Vec<Plant> {
    if settings.density <= 0.0 {
        return Vec::new();
    }
    let radius = scene.terrain.topology.radius as f32;
    let original = scene.bed == BedView::Original;
    let edges = edges(scene);
    let spacing = 0.65 / settings.density.sqrt();
    let estimate: f32 = edges
        .iter()
        .map(|e| (e.a.distance(e.b) * radius / spacing).ceil().max(1.0) * 2.0)
        .sum();
    let thinning = (160_000.0 / estimate.max(1.0)).min(1.0);
    let mut occupied = Spacing::default();
    let mut plants = Vec::new();
    let mut accepted = 0;
    for (id, edge) in edges.iter().enumerate() {
        let middle = (edge.a + edge.b) * 0.5;
        let up = middle.normalize();
        let tangent = (edge.b - edge.a) - up * (edge.b - edge.a).dot(up);
        if tangent.length_squared() < 1e-16 || near_fall(scene, up) {
            continue;
        }
        let mut outward = tangent.normalize().cross(up);
        if outward.dot(edge.wet - middle) > 0.0 {
            outward = -outward;
        }
        let steps = (tangent.length() * radius / spacing)
            .ceil()
            .clamp(1.0, 4096.0) as usize;
        for step in 0..steps {
            for row in 0..2 {
                let key = mix(scene.seed
                    ^ id as u64
                    ^ (step as u64).rotate_left(23)
                    ^ (row as u64).rotate_left(47));
                if unit(key) > thinning {
                    continue;
                }
                let t = (step as f32 + 0.15 + unit(key ^ 1) * 0.7) / steps as f32;
                let bank = edge.a.lerp(edge.b, t);
                let up = bank.normalize();
                if near_fall(scene, up) {
                    continue;
                }
                if noise(bank * radius / 8.0, scene.seed ^ 0x7265_6564)
                    < if edge.river { 0.38 } else { 0.60 }
                {
                    continue;
                }
                let offset = if row == 0 {
                    0.18 + unit(key ^ 2) * 0.28
                } else {
                    0.62 + unit(key ^ 2) * 0.45
                };
                let Some(sample) = ground.surface_sample(
                    &scene.terrain,
                    (bank + outward * (offset / radius)).normalize(),
                    original,
                ) else {
                    continue;
                };
                let height = (sample.point.length() - 1.0) * radius;
                let difference = (sample.point.length() - bank.length()) * radius;
                let rush = row == 1 || (!edge.river && unit(key ^ 5) < 0.65);
                if sample.normal.dot(up) < 0.85
                    || sample.material[1] < 0.12
                    || sample.clearance > 1e-8
                    || difference < -0.04
                    || difference > if rush { 0.85 } else { 0.45 }
                    || mean_temperature(up, height, scene.seed) < 1.0
                {
                    continue;
                }
                if !edge.river {
                    let seaward = (bank - outward * (3.0 / radius)).normalize();
                    if ground
                        .surface_sample(&scene.terrain, seaward, original)
                        .is_none_or(|probe| (probe.point.length() - 1.0) * radius < -1.0)
                    {
                        continue;
                    }
                }
                if near_fall(scene, sample.point.normalize())
                    || !occupied.accept(sample.point * radius, spacing * 0.65)
                {
                    continue;
                }
                let height = if rush {
                    0.65 + unit(key ^ 3) * 0.70
                } else {
                    1.05 + unit(key ^ 3) * 1.05
                };
                let root = sample.point - up * (height * 0.10 / radius);
                let rotation = Quat::from_rotation_arc(Vec3::Z, up)
                    * Quat::from_rotation_z(unit(key ^ 4) * TAU);
                let plant = Plant {
                    root,
                    outward,
                    rush,
                    river: edge.river,
                    variant: (mix(key ^ 6) % 4) as usize,
                    transform: Transform::from_translation(root)
                        .with_rotation(rotation)
                        .with_scale(Vec3::splat(motu::ISLAND_WORLD_METRES * height / radius)),
                };
                accepted += 1;
                if plants.len() < 60_000 {
                    plants.push(plant);
                } else {
                    let slot = (mix(key ^ 7) as usize) % accepted;
                    if slot < plants.len() {
                        plants[slot] = plant;
                    }
                }
            }
        }
    }
    plants
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bank_clumps_are_deterministic_radial_and_keep_clear_of_water_and_falls() {
        let mut scene = crate::collision::test_scene();
        scene.terrain = planet_rs::generate_terrain(
            planet_rs::PlanetOptions {
                vertex_count: 1024,
                ..default()
            },
            planet_rs::TerrainOptions {
                refinement_passes: 2,
                ..default()
            },
        )
        .unwrap()
        .into();
        let ground = Ground::new(&scene);
        let first = build(&scene, &ground, Settings::default());
        let second = build(&scene, &ground, Settings::default());
        assert!(
            !first.is_empty(),
            "representative planet needs actual bank vegetation"
        );
        assert!(first.iter().any(|p| p.rush) && first.iter().any(|p| !p.rush));
        assert!(
            first.iter().any(|p| p.river),
            "river banks must receive plants too"
        );
        assert_eq!(first.len(), second.len());
        let radius = scene.terrain.topology.radius as f32;
        for (a, b) in first.iter().zip(&second) {
            assert_eq!(a.transform, b.transform);
            assert_eq!(a.rush, b.rush);
            let sample = ground
                .surface_sample(&scene.terrain, a.root.normalize(), false)
                .unwrap();
            assert!(sample.clearance <= 1e-8);
            assert!(!near_fall(&scene, a.root.normalize()));
            assert!(sample.normal.dot(a.root.normalize()) > 0.84);
            assert!((a.transform.rotation * Vec3::Z).dot(a.root.normalize()) > 0.999);
            assert!((sample.point - a.root).length() * radius < 0.25);
        }
        assert!(
            build(
                &scene,
                &ground,
                Settings {
                    density: 0.0,
                    ..default()
                }
            )
            .is_empty()
        );
    }
}
