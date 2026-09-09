//! Small river-style stones along actual cave wall/floor intersections.
use super::{Cave, clearance::Triangles};
use crate::{ISLAND_WORLD_METRES, Mesh, Vec3, rivers, rng::Rng, terrain::SettledRock};

const MAX_STONES: usize = 1024;
const EDGE_BAND: f32 = 0.85;

pub(super) fn build(cave: &Cave) -> Mesh {
    let rocks = placements(cave);
    let mut mesh = Mesh::default();
    rivers::append_settled_rocks(cave.id, &rocks, &mut mesh);
    mesh
}

#[allow(clippy::many_single_char_names)]
fn placements(cave: &Cave) -> Vec<SettledRock> {
    let triangles = Triangles::new(&cave.chunks, cave.minimum, cave.maximum);
    let mut rng = Rng::new(cave.id ^ 0x5354_4f4e_4553);
    let mut rocks = Vec::<SettledRock>::new();
    for mesh in &cave.chunks {
        for face in mesh.triangles.chunks_exact(3) {
            let [a, b, c] = [face[0], face[1], face[2]]
                .map(|i| mesh.vertices[i as usize] * ISLAND_WORLD_METRES);
            let cross = (b - a).cross(c - a);
            let normal = cross.normalize_or_zero();
            if normal.z < 0.92 {
                continue;
            }
            // Sample by area, so tiny marching-cube faces do not receive more
            // stones than the larger floor triangles in swept passages.
            let expected = cross.length() * 0.5 / 0.45;
            let attempts = expected.floor() as usize + usize::from(rng.unit() < expected.fract());
            for _ in 0..attempts {
                let u = rng.unit().sqrt();
                let v = rng.unit();
                let p = a * (1.0 - u) + b * (u * (1.0 - v)) + c * (u * v);
                if (p - cave.portal.throat()).truncate().dot(cave.inward) < 1.0
                    || rng.unit()
                        > rivers::rock_density_acceptance(
                            cave.id,
                            p.truncate() / ISLAND_WORLD_METRES,
                        )
                {
                    continue;
                }
                let (boulder, diameter) = rivers::sample_rock_size(&mut rng);
                let radius = diameter * 0.5;
                let footprint = radius * 1.5;
                if !route_clear(cave, p, footprint)
                    || !triangles
                        .wall_distance_squared(p + Vec3::Z * 0.15, EDGE_BAND)
                        .is_some_and(|distance| distance > footprint * footprint)
                    || ![Vec3::X, -Vec3::X, Vec3::Y, -Vec3::Y]
                        .into_iter()
                        .all(|axis| {
                            triangles
                                .floor(p + axis * footprint)
                                .is_some_and(|height| (height - p.z).abs() < 0.08)
                        })
                    || rocks.iter().any(|rock| {
                        (rock.anchor * ISLAND_WORLD_METRES - p).truncate().length()
                            < (rock.radius * ISLAND_WORLD_METRES + radius) * 1.5 + 0.03
                    })
                {
                    continue;
                }
                rocks.push(SettledRock {
                    anchor: p / ISLAND_WORLD_METRES,
                    normal,
                    radius: radius / ISLAND_WORLD_METRES,
                    boulder,
                    appearance_id: rng.next_u64() as u32,
                });
                // A decoration-only limit; never truncates the cave or its routes.
                if rocks.len() == MAX_STONES {
                    return rocks;
                }
            }
        }
    }
    rocks
}

fn route_clear(cave: &Cave, p: Vec3, footprint: f32) -> bool {
    let clearance = 0.65 + footprint;
    cave.paths().all(|path| {
        path.windows(2).all(|pair| {
            let a = pair[0].floor.truncate();
            let edge = (pair[1].floor - pair[0].floor).truncate();
            let t =
                ((p.truncate() - a).dot(edge) / edge.length_squared().max(1.0e-8)).clamp(0.0, 1.0);
            p.truncate().distance_squared(a + edge * t) >= clearance * clearance
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        Vec2,
        caves::{
            CaveWalkOptions, placement,
            tests::{cliff, options},
            wandering,
        },
    };

    #[test]
    fn stones_follow_real_edges_preserve_routes_and_repeat_after_snapshot() {
        let terrain = cliff();
        let options = options();
        for wandering in [false, true] {
            let mut cave = placement::layout(
                17,
                &terrain,
                Vec3::new(999., 1000., 10.),
                Vec2::X,
                &options,
                &|_| false,
            )
            .unwrap()
            .unwrap();
            if wandering {
                wandering::expand(
                    &mut cave,
                    &terrain,
                    &options,
                    CaveWalkOptions {
                        enabled: 1,
                        ..CaveWalkOptions::default()
                    },
                    &|_| false,
                    &[],
                )
                .unwrap();
            }
            if !wandering {
                cave.chunks = vec![crate::caves::passage::build(&cave, &options)];
            }
            let rocks = placements(&cave);
            assert!(
                rocks.len() > 8,
                "Fixture needs visible edge scatter: {}",
                rocks.len()
            );
            let triangles = Triangles::new(&cave.chunks, cave.minimum, cave.maximum);
            for rock in &rocks {
                let p = rock.anchor * ISLAND_WORLD_METRES;
                assert!(route_clear(
                    &cave,
                    p,
                    rock.radius * ISLAND_WORLD_METRES * 1.5
                ));
                assert!(
                    triangles
                        .wall_distance_squared(p + Vec3::Z * 0.15, EDGE_BAND)
                        .is_some()
                );
                assert!((triangles.floor(p).unwrap() - p.z).abs() < 0.002);
            }
            let original = cave.clone();
            let mesh = build(&cave);
            assert_eq!(
                cave, original,
                "Decorations must not modify structural geometry."
            );
            assert_eq!(mesh.triangles.len(), rocks.len() * 20 * 3);
            assert_eq!(mesh.normals.len(), mesh.vertices.len());
            assert!(
                mesh.vertices
                    .iter()
                    .chain(&mesh.normals)
                    .all(|p| p.is_finite())
            );
            let restored: Cave = bincode::deserialize(&bincode::serialize(&cave).unwrap()).unwrap();
            assert_eq!(mesh, build(&restored));
            eprintln!(
                "cave stones: wandering={wandering}, stones={}, triangles={}",
                rocks.len(),
                mesh.triangles.len() / 3
            );
            if wandering && let Some(path) = std::env::var_os("MOTU_CAVE_STONES_FIXTURE_OUTPUT") {
                let data = serde_json::json!({
                    "vertices":mesh.vertices.iter().flat_map(|v|[(v.x-0.5)*ISLAND_WORLD_METRES,v.z*ISLAND_WORLD_METRES,(v.y-0.5)*ISLAND_WORLD_METRES]).collect::<Vec<_>>(),
                    "normals":mesh.normals.iter().flat_map(|v|[v.x,v.z,v.y]).collect::<Vec<_>>(),
                    "triangles":mesh.triangles.chunks_exact(3).flat_map(|t|[t[0],t[2],t[1]]).collect::<Vec<_>>(),
                    "floorStones":true,
                });
                std::fs::write(path, serde_json::to_vec(&data).unwrap()).unwrap();
            }
        }
    }
}
