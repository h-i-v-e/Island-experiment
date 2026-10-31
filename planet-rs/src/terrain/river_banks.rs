//! Radial port of Island's pre-tessellation channel-ring pull. Heights and all
//! centreline vertices stay fixed; safe tangent moves concentrate bank detail.
use super::{
    RiverSegment, TerrainOptions,
    graph::{SurfaceGraph, arc},
    river_widths,
};
use crate::{DVec3, PlanetMesh};
use std::collections::{BTreeMap, HashMap};

#[derive(Clone, Copy)]
struct Pull {
    centre: DVec3,
    width: f64,
    weight: f64,
    distance: f64,
    reach: usize,
}

pub(super) fn narrow(
    mesh: &mut PlanetMesh,
    graph: &SurfaceGraph,
    heights: &[f64],
    rivers: &[RiverSegment],
    widths: &[f64],
    settings: &TerrainOptions,
) -> usize {
    if settings.river_taper == 0.0 || rivers.is_empty() {
        return 0;
    }
    let mut fixed = vec![false; mesh.directions.len()];
    let mut incoming = HashMap::<usize, usize>::new();
    for r in rivers {
        fixed[r.from] = true;
        fixed[r.to] = true;
        *incoming.entry(r.to).or_default() += 1;
    }
    // Keep merge corners in place; later refinement resolves the tapered
    // tributary-to-main-channel transition using their common junction width.
    for (vertex, degree) in incoming {
        if degree > 1 {
            for &i in &graph.neighbours[vertex] {
                fixed[i] = true;
            }
        }
    }
    let mut pulls = BTreeMap::<usize, Pull>::new();
    for (id, r) in rivers.iter().enumerate() {
        let a = mesh.directions[r.from];
        let b = mesh.directions[r.to];
        let edge = b - a;
        let old = river_widths::legacy(r.catchment, settings);
        for endpoint in [r.from, r.to] {
            for &i in &graph.neighbours[endpoint] {
                if fixed[i] || heights[i] < 0.0 || edge.length_squared() < 1e-24 {
                    continue;
                }
                let p = mesh.directions[i];
                let t = ((p - a).dot(edge) / edge.length_squared()).clamp(0.0, 1.0);
                let centre = a.lerp(b, t).normalize();
                let width = widths[r.from] + (widths[r.to] - widths[r.from]) * t;
                let weight = ((1.0 - width / old) / 0.85).clamp(0.0, 1.0);
                let distance = arc(centre, p);
                if weight <= 1e-12 || distance <= width {
                    continue;
                }
                let candidate = Pull {
                    centre,
                    width,
                    weight,
                    distance,
                    reach: id,
                };
                if pulls.get(&i).is_none_or(|old| {
                    distance
                        .total_cmp(&old.distance)
                        .then(id.cmp(&old.reach))
                        .is_lt()
                }) {
                    pulls.insert(i, candidate);
                }
            }
        }
    }
    let mut incident: HashMap<_, Vec<usize>> = pulls.keys().map(|&i| (i, Vec::new())).collect();
    for (id, face) in mesh.triangles.iter().enumerate() {
        for &i in face {
            if let Some(faces) = incident.get_mut(&(i as usize)) {
                faces.push(id);
            }
        }
    }
    let mut moves = 0;
    for _ in 0..2 {
        for (&i, &pull) in &pulls {
            let p = mesh.directions[i];
            let distance = arc(pull.centre, p);
            let tangent = (p - pull.centre * p.dot(pull.centre)).normalize_or_zero();
            if distance <= pull.width || tangent.length_squared() < 0.5 {
                continue;
            }
            let angle = distance + (pull.width - distance) * pull.weight;
            let target = (pull.centre * angle.cos() + tangent * angle.sin()).normalize();
            let fraction = safe_fraction(mesh, &incident[&i], i, target);
            if fraction > 1e-12 {
                mesh.directions[i] = p.lerp(target, fraction).normalize();
                moves += 1;
            }
        }
    }
    moves
}

fn safe_fraction(mesh: &PlanetMesh, incident: &[usize], i: usize, target: DVec3) -> f64 {
    let mut fraction = 0.8_f64;
    for &face in incident {
        let indices = mesh.triangles[face];
        let before = indices.map(|j| mesh.directions[j as usize]);
        let after = indices.map(|j| {
            if j as usize == i {
                target
            } else {
                mesh.directions[j as usize]
            }
        });
        let area = before[0].dot(before[1].cross(before[2]));
        let next = after[0].dot(after[1].cross(after[2]));
        if area <= 1e-18 {
            fraction = 0.0;
            break;
        }
        if next < area * 0.25 {
            fraction = fraction.min((area * 0.75 / (area - next)).clamp(0.0, 1.0));
        }
    }
    fraction
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terrain::{Drainage, river_widths};
    fn point(row: u32, col: u32, pole: f64) -> DVec3 {
        DVec3::new(f64::from(col) * 0.006, (f64::from(row) - 2.0) * 0.006, pole).normalize()
    }
    fn fixture(
        pole: f64,
    ) -> (
        PlanetMesh,
        Vec<f64>,
        Vec<RiverSegment>,
        Vec<f64>,
        TerrainOptions,
    ) {
        let mut mesh = PlanetMesh {
            directions: (0..5)
                .flat_map(|row| (0..7).map(move |col| point(row, col, pole)))
                .collect(),
            triangles: Vec::new(),
            radius: 1000.0,
        };
        for row in 0..4 {
            for col in 0..6 {
                let a = row * 7 + col;
                for mut face in [[a, a + 1, a + 7], [a + 1, a + 8, a + 7]] {
                    let [p, q, r] = face.map(|i| mesh.directions[i as usize]);
                    if p.dot(q.cross(r)) < 0.0 {
                        face.swap(1, 2);
                    }
                    mesh.triangles.push(face);
                }
            }
        }
        let settings = TerrainOptions::default();
        let q = settings.river_catchment * 4.0 * std::f64::consts::PI;
        let mut drainage = Drainage {
            receivers: vec![None; 35],
            catchment: vec![0.; 35],
            order: (14..21).rev().collect(),
        };
        let flows = [1., 2., 4., 9., 16., 24., 25.];
        let mut rivers = Vec::new();
        for (col, &flow) in flows.iter().enumerate() {
            let i = 14 + col;
            drainage.catchment[i] = flow * q;
            if col < 6 {
                drainage.receivers[i] = Some(i + 1);
                rivers.push(RiverSegment {
                    from: i,
                    to: i + 1,
                    catchment: flow * q,
                });
            }
        }
        let widths = river_widths::build(&mesh, &drainage, &rivers, &settings);
        let heights = (0..35)
            .map(|i| {
                if i % 7 == 6 {
                    -0.01
                } else {
                    0.06 - f64::from(i % 7) * 0.005
                }
            })
            .collect();
        (mesh, heights, rivers, widths, settings)
    }
    #[test]
    fn source_banks_move_inward_before_refinement_without_moving_centres_or_sea_or_folding_faces() {
        for pole in [1.0, -1.0] {
            let (mut mesh, heights, rivers, widths, settings) = fixture(pole);
            let graph = SurfaceGraph::new(&mesh);
            assert!(narrow(&mut mesh, &graph, &heights, &rivers, &widths, &settings) > 0);
            for r in &rivers {
                for i in [r.from, r.to] {
                    assert_eq!(
                        mesh.directions[i],
                        point(2, u32::try_from(i - 14).unwrap(), pole)
                    );
                }
            }
            for row in 0..5 {
                assert_eq!(mesh.directions[(row * 7 + 6) as usize], point(row, 6, pole));
            }
            for i in [7, 21] {
                let old = point(u32::try_from(i / 7).unwrap(), 0, pole);
                let centre = point(2, 0, pole);
                assert!(arc(mesh.directions[i], centre) < arc(old, centre) * 0.5);
            }
            for face in &mesh.triangles {
                let [a, b, c] = face.map(|i| mesh.directions[i as usize]);
                assert!(a.dot(b.cross(c)) > 0.0);
            }
            assert!(
                mesh.directions
                    .iter()
                    .all(|p| (p.length_squared() - 1.0).abs() < 1e-12)
            );
            assert_eq!(mesh.radius.to_bits(), 1000.0_f64.to_bits());
        }
    }
    #[test]
    fn zero_taper_leaves_every_mesh_direction_exactly_unchanged() {
        let (mut mesh, heights, rivers, widths, mut settings) = fixture(1.0);
        let graph = SurfaceGraph::new(&mesh);
        settings.river_taper = 0.0;
        assert_eq!(
            narrow(&mut mesh, &graph, &heights, &rivers, &widths, &settings),
            0
        );
        for row in 0..5 {
            for col in 0..7 {
                assert_eq!(
                    mesh.directions[(row * 7 + col) as usize],
                    point(row, col, 1.0)
                );
            }
        }
    }
}
