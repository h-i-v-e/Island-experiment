//! Island's irregular icosahedral stones and buried, refined large boulders.
//! Local drops settle downhill in radial gravity; overlapping bodies are omitted.
use super::snapshot::{Sample, Snapshot};
use crate::{collision::RockCollider, lod_material::DetailMapping};
use bevy::prelude::*;
use planet_rs::{DVec3, chunks::CellLocator};
use std::collections::{BTreeMap, HashMap};

#[derive(Default)]
pub struct MeshData {
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub colours: Vec<[f32; 4]>,
    pub indices: Vec<u32>,
}
pub struct Group {
    pub owner: u32,
    pub mesh: MeshData,
}
pub struct Patch {
    pub centre: DVec3,
    pub groups: Vec<Group>,
    pub colliders: Vec<(u32, RockCollider)>,
    pub placements: Vec<Placement>,
    pub mapping: DetailMapping,
    pub stones: usize,
    pub boulders: usize,
}
#[derive(Clone, Copy)]
pub struct Placement {
    pub point: DVec3,
    pub normal: DVec3,
    pub radius: f64,
    pub key: u64,
    pub owner: u32,
}

pub fn mix(mut value: u64) -> u64 {
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        mix(self.0)
    }
    #[allow(clippy::cast_precision_loss)]
    fn unit(&mut self) -> f64 {
        (self.next() >> 11) as f64 / 9_007_199_254_740_992.0
    }
    fn range(&mut self, low: f64, high: f64) -> f64 {
        (high - low).mul_add(self.unit(), low)
    }
}

fn settle(snapshot: &Snapshot, mut sample: Sample, radius: f64) -> Option<Sample> {
    for _ in 0..96 {
        let up = sample.point.normalize();
        let alignment = sample.normal.dot(up);
        if sample.point.length() < 1.0 - 10.0 / radius {
            return None;
        }
        if alignment >= 0.906_307_8 {
            return Some(sample);
        }
        // Advance in the downhill tangent direction. A radial step becomes
        // zero horizontal progress on a vertical cliff; this reaches its foot.
        let down = (sample.normal - up * alignment).try_normalize()?;
        let candidate = snapshot.sample(sample.point + down * (0.5 / radius))?;
        if candidate.point.length() > sample.point.length() + 0.02 / radius {
            return None;
        }
        sample = candidate;
    }
    None
}

#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::too_many_lines
)]
pub fn build(
    mut snapshot: Snapshot,
    centre: DVec3,
    radius: f64,
    seed: u64,
    level: u8,
    options: super::DetailSettings,
) -> Patch {
    let super::DetailSettings {
        rock_density: density,
        stones,
        boulders,
        ..
    } = options;
    snapshot.prepare();
    let locator = CellLocator::new(level).expect("validated streaming level");
    let up = centre.normalize();
    let tangent = up.any_orthonormal_vector();
    let bitangent = up.cross(tangent);
    let mapping = DetailMapping {
        origin: crate::surface::vector(centre * radius).extend(160.0),
        tangent: crate::surface::vector(tangent).extend(0.0),
        bitangent: crate::surface::vector(bitangent).extend(0.0),
    };
    let mut candidates = Vec::new();
    for triangle in &snapshot.triangles {
        let centroid = (triangle.points[0] + triangle.points[1] + triangle.points[2]) / 3.0;
        let alignment = triangle.normal().dot(centroid.normalize());
        let land = triangle
            .points
            .iter()
            .all(|p| p.length() > 1.0 + 1.0 / radius);
        let river = triangle.material.iter().any(|m| m[2] > 0.5);
        for population in 0..3_u64 {
            let eligible = if population == 2 {
                stones && river && alignment >= 0.906_307_8
            } else {
                land && alignment.abs() <= std::f64::consts::FRAC_1_SQRT_2
                    && if population == 0 { stones } else { boulders }
            };
            if !eligible {
                continue;
            }
            let per_area = if population == 0 {
                0.25
            } else if population == 1 {
                1.0 / 128.0
            } else {
                1.0 / 0.35
            };
            let expected = triangle.area() * radius * radius * per_area * f64::from(density);
            let domain = if population == 0 {
                0xd1b5_4a32_d192_ed03
            } else if population == 1 {
                0x626f_756c_6465_7273
            } else {
                0x72a4_6f63_6b73_21d9
            };
            let key = mix(seed ^ domain ^ triangle.id as u64);
            let mut rng = Rng(key);
            let count =
                (expected.floor() as usize + usize::from(rng.unit() < expected.fract())).min(2048);
            for index in 0..count {
                let root = rng.unit().sqrt();
                let along = rng.unit();
                let sample = triangle.sample([1.0 - root, root * (1.0 - along), root * along]);
                if sample.point.distance(centre) * radius > 125.0 {
                    continue;
                }
                let appearance = mix(key ^ index as u64);
                let mut appearance_rng = Rng(appearance);
                let diameter = if population == 1 {
                    appearance_rng.range(2.0, 6.0)
                } else if population == 0 && appearance_rng.unit() < 0.15 {
                    appearance_rng.range(0.30, 0.60)
                } else if population == 2 {
                    appearance_rng.range(0.06, 0.22)
                } else {
                    appearance_rng.range(0.10, 0.30)
                };
                let settled = if population == 2 {
                    Some(sample)
                } else {
                    settle(&snapshot, sample, radius)
                };
                let Some(settled) = settled else {
                    continue;
                };
                if settled.point.distance(centre) * radius > 95.0 {
                    continue;
                }
                let Some(owner) = locator.owner(settled.point) else {
                    continue;
                };
                candidates.push(Placement {
                    point: settled.point,
                    normal: settled.normal,
                    radius: diameter * 0.5,
                    key: appearance,
                    owner,
                });
                if candidates.len() >= 20_000 {
                    break;
                }
            }
        }
        if candidates.len() >= 20_000 {
            break;
        }
    }
    candidates.sort_unstable_by(|a, b| b.radius.total_cmp(&a.radius).then(a.key.cmp(&b.key)));
    let mut placed = Vec::<Placement>::new();
    let mut cells = HashMap::<[i32; 3], Vec<usize>>::new();
    let cell = |point: DVec3| (point * radius / 4.0).floor().to_array().map(|v| v as i32);
    for candidate in candidates {
        let [x, y, z] = cell(candidate.point);
        let mut overlaps = false;
        for dz in -2..=2 {
            for dy in -2..=2 {
                for dx in -2..=2 {
                    if let Some(indices) = cells.get(&[x + dx, y + dy, z + dz]) {
                        for &index in indices {
                            let other = placed[index];
                            if candidate.point.distance(other.point) * radius
                                < candidate.radius + other.radius + 0.03
                            {
                                overlaps = true;
                            }
                        }
                    }
                }
            }
        }
        if overlaps {
            continue;
        }
        cells.entry([x, y, z]).or_default().push(placed.len());
        placed.push(candidate);
    }
    let mut meshes = BTreeMap::<u32, MeshData>::new();
    let mut colliders = Vec::new();
    let mut stone_count = 0;
    let mut boulder_count = 0;
    for placement in &placed {
        let collider = append_rock(
            meshes.entry(placement.owner).or_default(),
            *placement,
            radius,
        );
        if placement.radius >= 1.0 {
            colliders.push((placement.owner, collider));
            boulder_count += 1;
        } else {
            stone_count += 1;
        }
    }
    Patch {
        centre,
        groups: meshes
            .into_iter()
            .map(|(owner, mesh)| Group { owner, mesh })
            .collect(),
        colliders,
        placements: placed,
        mapping,
        stones: stone_count,
        boulders: boulder_count,
    }
}

#[allow(clippy::cast_possible_truncation)]
fn append_rock(output: &mut MeshData, placement: Placement, planet_radius: f64) -> RockCollider {
    let mut rng = Rng(placement.key);
    let yaw = rng.range(0.0, std::f64::consts::TAU);
    let tangent = placement.normal.any_orthonormal_vector();
    let bitangent = placement.normal.cross(tangent);
    let width_axis = tangent * yaw.cos() + bitangent * yaw.sin();
    let breadth_axis = bitangent * yaw.cos() - tangent * yaw.sin();
    let width = placement.radius * rng.range(0.88, 1.16);
    let breadth = placement.radius * rng.range(0.82, 1.12);
    let boulder = placement.radius >= 0.15;
    let vertical = placement.radius
        * if boulder {
            rng.range(0.68, 1.02)
        } else {
            rng.range(0.58, 0.92)
        };
    let (directions, mut faces) = icosahedron();
    let mut positions: Vec<_> = directions
        .into_iter()
        .map(|direction| {
            let distortion = rng.range(0.86, 1.14);
            (width_axis * direction.x * width
                + breadth_axis * direction.y * breadth
                + placement.normal * direction.z * vertical)
                * distortion
        })
        .collect();
    let support = positions
        .iter()
        .map(|p| p.dot(placement.normal))
        .fold(f64::INFINITY, f64::min);
    let embed = vertical
        * if boulder {
            rng.range(0.55, 0.78)
        } else {
            rng.range(0.20, 0.48)
        };
    let centre = placement.point - placement.normal * ((support + embed) / planet_radius);
    let before = positions.iter().map(|p| p.length()).fold(0.0, f64::max);
    if placement.radius >= 1.0 {
        for _ in 0..2 {
            refine(&mut positions, &mut faces);
        }
        let after = positions.iter().map(|p| p.length()).fold(0.0, f64::max);
        for p in &mut positions {
            *p *= before / after;
        }
    }
    let mut normals = vec![DVec3::ZERO; positions.len()];
    for face in &faces {
        let [a, b, c] = face.map(|i| positions[i as usize]);
        let normal = (b - a).cross(c - a);
        for &i in face {
            normals[i as usize] += normal;
        }
    }
    let first = u32::try_from(output.positions.len()).expect("bounded local rock mesh");
    let tint = rng.range(0.86, 1.12) as f32;
    for (p, normal) in positions.iter().zip(normals) {
        output
            .positions
            .push((centre + *p / planet_radius).as_vec3().to_array());
        output.normals.push(normal.normalize().as_vec3().to_array());
        output.colours.push([tint, tint, tint, 1.0]);
    }
    output
        .indices
        .extend(faces.iter().flatten().map(|&i| i + first));
    RockCollider {
        centre: crate::surface::vector(centre),
        radius: (before / planet_radius) as f32,
    }
}
fn icosahedron() -> (Vec<DVec3>, Vec<[u32; 3]>) {
    let g = (1.0 + 5.0_f64.sqrt()) * 0.5;
    let vertices = [
        [-1.0, g, 0.0],
        [1.0, g, 0.0],
        [-1.0, -g, 0.0],
        [1.0, -g, 0.0],
        [0.0, -1.0, g],
        [0.0, 1.0, g],
        [0.0, -1.0, -g],
        [0.0, 1.0, -g],
        [g, 0.0, -1.0],
        [g, 0.0, 1.0],
        [-g, 0.0, -1.0],
        [-g, 0.0, 1.0],
    ]
    .map(|p| DVec3::from_array(p).normalize())
    .to_vec();
    let faces = vec![
        [0, 11, 5],
        [0, 5, 1],
        [0, 1, 7],
        [0, 7, 10],
        [0, 10, 11],
        [1, 5, 9],
        [5, 11, 4],
        [11, 10, 2],
        [10, 7, 6],
        [7, 1, 8],
        [3, 9, 4],
        [3, 4, 2],
        [3, 2, 6],
        [3, 6, 8],
        [3, 8, 9],
        [4, 9, 5],
        [2, 4, 11],
        [6, 2, 10],
        [8, 6, 7],
        [9, 8, 1],
    ];
    (vertices, faces)
}
fn refine(points: &mut Vec<DVec3>, faces: &mut Vec<[u32; 3]>) {
    let mut midpoints = HashMap::new();
    let mut next = Vec::with_capacity(faces.len() * 4);
    for &[a, b, c] in faces.iter() {
        let mut midpoint = |a: u32, b: u32| {
            *midpoints.entry((a.min(b), a.max(b))).or_insert_with(|| {
                let index = u32::try_from(points.len()).expect("small rock");
                points.push((points[a as usize] + points[b as usize]) * 0.5);
                index
            })
        };
        let ab = midpoint(a, b);
        let bc = midpoint(b, c);
        let ca = midpoint(c, a);
        next.extend([[a, ab, ca], [ab, b, bc], [ca, bc, c], [ab, bc, ca]]);
    }
    *faces = next;
    let mut neighbours = vec![Vec::new(); points.len()];
    for &[a, b, c] in faces.iter() {
        for (a, b) in [(a, b), (b, c), (c, a)] {
            neighbours[a as usize].push(b as usize);
            neighbours[b as usize].push(a as usize);
        }
    }
    let smoothed: Vec<_> = neighbours
        .iter_mut()
        .enumerate()
        .map(|(i, near)| {
            near.sort_unstable();
            near.dedup();
            let mean = near.iter().map(|&j| points[j]).sum::<DVec3>()
                / f64::from(u32::try_from(near.len()).unwrap());
            points[i].lerp(mean, 0.25)
        })
        .collect();
    *points = smoothed;
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
pub fn mask(patch: &Patch, radius: f64, visible: impl Fn(u32) -> bool) -> Vec<u8> {
    let mut pixels = vec![0_u8; 512 * 512 * 4];
    let origin = DVec3::from_array(patch.mapping.origin.truncate().to_array().map(f64::from));
    let tangent = DVec3::from_array(patch.mapping.tangent.truncate().to_array().map(f64::from));
    let bitangent = DVec3::from_array(patch.mapping.bitangent.truncate().to_array().map(f64::from));
    let pixel_metres = 160.0 / 512.0;
    for placement in patch.placements.iter().filter(|p| visible(p.owner)) {
        let p = placement.point * radius - origin;
        let x = p.dot(tangent) / pixel_metres + 256.0;
        let y = p.dot(bitangent) / pixel_metres + 256.0;
        let spread = placement.radius.max(0.2) + 1.0;
        let width = (spread / pixel_metres).ceil() as i32;
        for py in y.floor() as i32 - width..=y.ceil() as i32 + width {
            for px in x.floor() as i32 - width..=x.ceil() as i32 + width {
                if !(0..512).contains(&px) || !(0..512).contains(&py) {
                    continue;
                }
                let distance =
                    ((f64::from(px) + 0.5 - x).powi(2) + (f64::from(py) + 0.5 - y).powi(2)).sqrt()
                        * pixel_metres;
                let cover = ((spread - distance) / 0.8).clamp(0.0, 1.0);
                let footprint =
                    ((placement.radius.max(0.2) + 0.2 - distance) / 0.2).clamp(0.0, 1.0);
                let index = (py as usize * 512 + px as usize) * 4;
                pixels[index] = pixels[index].max((cover * 255.0) as u8);
                pixels[index + 1] = pixels[index + 1].max((footprint * 255.0) as u8);
            }
        }
    }
    pixels
}

#[cfg(test)]
mod tests {
    use super::*;

    fn slope_snapshot() -> Snapshot {
        use super::super::snapshot::Triangle;
        let point = |x, height, z| DVec3::new(x, height, z);
        let floor = [
            point(-0.1, 1.01, -0.1),
            point(0.1, 1.01, -0.1),
            point(0.1, 1.01, 0.1),
            point(-0.1, 1.01, 0.1),
        ];
        let slope = [
            point(-0.02, 1.01, -0.04),
            point(0.0, 1.05, -0.04),
            point(0.0, 1.05, 0.04),
            point(-0.02, 1.01, 0.04),
        ];
        Snapshot::from_triangles(
            [floor, slope]
                .into_iter()
                .enumerate()
                .flat_map(|(i, points)| {
                    [[0, 2, 1], [0, 3, 2]]
                        .into_iter()
                        .enumerate()
                        .map(move |(j, face)| Triangle {
                            id: i * 2 + j,
                            points: face.map(|v| points[v]),
                            material: [[0.5, 1.0, 0.0, 0.0]; 3],
                        })
                })
                .collect(),
        )
    }

    #[test]
    fn steep_sources_produce_deterministic_grounded_stones_and_large_boulders() {
        let centre = DVec3::new(-0.03, 1.01, 0.0);
        let first = build(
            slope_snapshot(),
            centre,
            1000.0,
            42,
            2,
            super::super::DetailSettings::default(),
        );
        let second = build(
            slope_snapshot(),
            centre,
            1000.0,
            42,
            2,
            super::super::DetailSettings::default(),
        );
        assert!(first.stones > 0 && first.boulders > 0);
        assert_eq!(
            (first.stones, first.boulders),
            (second.stones, second.boulders)
        );
        assert_eq!(
            first.placements.iter().map(|p| p.key).collect::<Vec<_>>(),
            second.placements.iter().map(|p| p.key).collect::<Vec<_>>()
        );
        let mut snapshot = slope_snapshot();
        snapshot.prepare();
        for rock in first.placements {
            let support = snapshot.sample(rock.point).unwrap();
            assert!(support.point.distance(rock.point) < 1.0e-8);
            assert!(rock.normal.dot(rock.point.normalize()) >= 0.906_307_8);
            assert!(rock.point.length() >= 0.99);
        }
        for (a, b) in first.groups.iter().zip(&second.groups) {
            assert_eq!(a.owner, b.owner);
            assert_eq!(a.mesh.positions, b.mesh.positions);
            assert_eq!(a.mesh.indices, b.mesh.indices);
        }
    }

    #[test]
    fn nearly_vertical_sources_advance_to_the_cliff_foot() {
        use super::super::snapshot::Triangle;
        let mut snapshot = slope_snapshot();
        snapshot.triangles.truncate(2);
        let cliff = Triangle {
            id: 2,
            points: [
                DVec3::new(-0.0001, 1.01, -0.04),
                DVec3::new(0.0, 1.09, 0.04),
                DVec3::new(0.0, 1.09, -0.04),
            ],
            material: [[0.5, 1.0, 0.0, 0.0]; 3],
        };
        let initial = cliff.sample([1.0 / 3.0; 3]);
        snapshot.triangles.push(cliff);
        snapshot.prepare();
        let settled = settle(&snapshot, initial, 1000.0).unwrap();
        assert!(settled.point.x < -0.0001);
        assert!((settled.point.y - 1.01).abs() < 1.0e-8);
    }

    fn placement(up: DVec3, radius: f64) -> Placement {
        Placement {
            point: up * 1.01,
            normal: up,
            radius,
            key: 42,
            owner: 0,
        }
    }

    #[test]
    fn stone_and_refined_boulder_meshes_are_closed_embedded_and_deterministic() {
        for up in [DVec3::Y, -DVec3::Y, DVec3::Z] {
            for radius in [0.1, 2.0] {
                let p = placement(up, radius);
                let mut first = MeshData::default();
                let collider = append_rock(&mut first, p, 1000.0);
                let mut second = MeshData::default();
                append_rock(&mut second, p, 1000.0);
                assert_eq!(first.positions, second.positions);
                assert_eq!(first.normals, second.normals);
                assert_eq!(first.colours, second.colours);
                assert_eq!(first.indices, second.indices);
                assert_eq!(first.positions.len(), if radius >= 1.0 { 162 } else { 12 });
                assert_eq!(first.indices.len(), if radius >= 1.0 { 960 } else { 60 });
                let mut edges = BTreeMap::new();
                for face in first.indices.chunks_exact(3) {
                    for (a, b) in [(face[0], face[1]), (face[1], face[2]), (face[2], face[0])] {
                        *edges.entry((a.min(b), a.max(b))).or_insert(0) += 1;
                    }
                }
                assert!(edges.values().all(|&count| count == 2));
                let mut bottom = f64::INFINITY;
                let mut top = f64::NEG_INFINITY;
                for point in &first.positions {
                    let point = DVec3::from_array(point.map(f64::from));
                    let support = (point - p.point).dot(up) * 1000.0;
                    bottom = bottom.min(support);
                    top = top.max(support);
                    let centre = DVec3::from_array(collider.centre.to_array().map(f64::from));
                    assert!(point.distance(centre) <= f64::from(collider.radius) + 1.0e-6);
                }
                assert!(
                    bottom < 0.0 && top > 0.0,
                    "rock must straddle its terrain support"
                );
                for normal in &first.normals {
                    assert!((Vec3::from_array(*normal).length() - 1.0).abs() < 1.0e-5);
                }
            }
        }
    }

    #[test]
    fn settling_obeys_radial_gravity_and_ten_metre_seabed_limit() {
        let scene = crate::collision::test_scene();
        let snapshot = Snapshot::new(&scene.terrain, false, &[]);
        for up in [DVec3::Y, -DVec3::Y, DVec3::Z] {
            let sample = Sample {
                point: up * 1.01,
                normal: up,
            };
            assert!(settle(&snapshot, sample, 1000.0).is_some());
            assert!(
                settle(
                    &snapshot,
                    Sample {
                        point: up * 0.989,
                        ..sample
                    },
                    1000.0
                )
                .is_none()
            );
            assert!(
                settle(
                    &snapshot,
                    Sample {
                        point: up * 0.991,
                        ..sample
                    },
                    1000.0
                )
                .is_some()
            );
            assert!(
                settle(
                    &snapshot,
                    Sample {
                        normal: up.any_orthonormal_vector(),
                        ..sample
                    },
                    1000.0
                )
                .is_none()
            );
        }
    }

    #[test]
    fn detail_mask_tracks_visible_rock_footprints() {
        let p = placement(DVec3::Y, 2.0);
        let patch = Patch {
            centre: p.point,
            groups: Vec::new(),
            colliders: Vec::new(),
            placements: vec![p],
            mapping: DetailMapping {
                origin: Vec4::new(0.0, 1010.0, 0.0, 160.0),
                tangent: Vec3::X.extend(0.0),
                bitangent: Vec3::Z.extend(0.0),
            },
            stones: 0,
            boulders: 1,
        };
        let mask = mask(&patch, 1000.0, |_| true);
        let centre = (256 * 512 + 256) * 4;
        assert_eq!(&mask[centre..centre + 2], &[255, 255]);
        assert_eq!(&mask[..4], &[0; 4]);
        assert!(
            super::mask(&patch, 1000.0, |_| false)
                .iter()
                .all(|&v| v == 0)
        );
    }
}
