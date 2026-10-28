//! Bind the complete river to final terrain inclination, then clip exact bank
//! contacts. Every patch uses the same rule; there is no face/foot switch.
use std::collections::{BTreeMap, HashMap};

use super::{Mesh, RIVER_SURFACE_OFFSET, Vec2, Vec3, WATERFALL_WATER_CLEARANCE, is_river_boundary};

pub(super) fn water_height(bed: f32, hydraulic: f32, normal: Vec3, boundary: bool) -> f32 {
    if hydraulic <= 0.0 {
        return hydraulic.min(bed + WATERFALL_WATER_CLEARANCE);
    }
    let alignment = normal.dot(Vec3::Z).clamp(0.0, 1.0);
    let flat = alignment * alignment * (3.0 - 2.0 * alignment);
    let regular = if boundary {
        hydraulic.min(bed)
    } else {
        hydraulic + RIVER_SURFACE_OFFSET
    };
    let hugging = bed + WATERFALL_WATER_CLEARANCE;
    let water = (regular - hugging).mul_add(flat, hugging);
    if boundary { water.min(bed) } else { water }
}

#[derive(Clone, Copy)]
struct Corner {
    id: u32,
    fields: [f32; 2],
}

pub(super) fn water_heights(terrain: &Mesh, coverage: &[u8], surfaces: &[f32]) -> Vec<f32> {
    terrain
        .vertices
        .iter()
        .enumerate()
        .map(|(i, position)| {
            water_height(
                position.z,
                surfaces[i],
                terrain.normals.get(i).copied().unwrap_or(Vec3::Z),
                is_river_boundary(coverage[i]),
            )
        })
        .collect()
}

#[cfg(test)]
pub(super) fn build(terrain: &Mesh, coverage: &[u8], surfaces: &[f32], uv: &[Vec2]) -> Mesh {
    build_with_heights(
        terrain,
        coverage,
        &water_heights(terrain, coverage, surfaces),
        uv,
    )
}

/// Keep the inclination blend frozen through final contact grading.
pub(super) fn build_with_heights(
    terrain: &Mesh,
    coverage: &[u8],
    heights: &[f32],
    uv: &[Vec2],
) -> Mesh {
    let mut out = Mesh::default();
    let mut vertices = vec![None; terrain.vertices.len()];
    let mut edges = BTreeMap::new();
    let fields: Vec<_> = heights
        .iter()
        .zip(&terrain.vertices)
        .map(|(&water, bed)| {
            // One global snap for all incident faces prevents endpoint slivers.
            [water - bed.z, water].map(|v| if v.abs() < 1.0e-9 { 0.0 } else { v })
        })
        .collect();
    let mut polygon = Vec::with_capacity(6);
    let mut clipped = Vec::with_capacity(6);
    for face in terrain.triangles.chunks_exact(3) {
        if face.iter().any(|&i| coverage[i as usize] == 0)
            || face.iter().all(|&i| fields[i as usize][0] <= 0.0)
            || face.iter().all(|&i| fields[i as usize][1] <= 0.0)
        {
            continue;
        }
        polygon.clear();
        for &i in face {
            let i = i as usize;
            let id = *vertices[i].get_or_insert_with(|| {
                let mut point = terrain.vertices[i];
                point.z = heights[i];
                append(&mut out, point, uv[i])
            });
            polygon.push(Corner {
                id,
                fields: fields[i],
            });
        }
        for plane in 0..2 {
            clipped.clear();
            for i in 0..polygon.len() {
                let a = polygon[i];
                let b = polygon[(i + 1) % polygon.len()];
                let wet_a = a.fields[plane] > 0.0;
                let wet_b = b.fields[plane] > 0.0;
                if wet_a {
                    clipped.push(a);
                }
                if wet_a != wet_b {
                    let (a, b) = if a.id < b.id { (a, b) } else { (b, a) };
                    let t = a.fields[plane] / (a.fields[plane] - b.fields[plane]);
                    let corner = if t <= 0.0 {
                        a
                    } else if t >= 1.0 {
                        b
                    } else {
                        let id = *edges.entry((plane, a.id, b.id)).or_insert_with(|| {
                            let point =
                                out.vertices[a.id as usize].lerp(out.vertices[b.id as usize], t);
                            let uv = out.uv[a.id as usize].lerp(out.uv[b.id as usize], t);
                            append(&mut out, point, uv)
                        });
                        Corner {
                            id,
                            fields: std::array::from_fn(|j| {
                                (b.fields[j] - a.fields[j]).mul_add(t, a.fields[j])
                            }),
                        }
                    };
                    clipped.push(corner);
                }
            }
            std::mem::swap(&mut polygon, &mut clipped);
        }
        for i in 1..polygon.len().saturating_sub(1) {
            let face = [polygon[0].id, polygon[i].id, polygon[i + 1].id];
            let [a, b, c] = face.map(|id| out.vertices[id as usize]);
            if (b.as_dvec3() - a.as_dvec3())
                .cross(c.as_dvec3() - a.as_dvec3())
                .z
                > 0.0
            {
                out.triangles.extend(face);
            }
        }
    }
    out.calculate_normals();
    clip_above_sea(out)
}

/// Float clipping can round two distinct edge intersections to the same
/// position. Drop only collapsed/inverted projected faces, with no area cutoff
/// that could remove a valid thin bank contact. Also run after final bank burial.
pub(crate) fn clip_above_sea(mut mesh: Mesh) -> Mesh {
    for point in &mut mesh.vertices {
        if point.z.abs() < 1.0e-9 {
            point.z = 0.0;
        }
    }
    let mut mesh = mesh.clipped_above(0.0);
    // Different source edges can round to the same contact. Retain one shared
    // index, otherwise the artificial seam is mistaken for a dry river bank.
    let mut contacts = HashMap::with_capacity(mesh.vertices.len());
    let mapping: Vec<_> = mesh
        .vertices
        .iter()
        .enumerate()
        .map(|(i, point)| {
            let key = point.to_array().map(f32::to_bits);
            *contacts
                .entry(key)
                .or_insert_with(|| u32::try_from(i).expect("bounded river mesh"))
        })
        .collect();
    let mut welded = false;
    for i in &mut mesh.triangles {
        let shared = mapping[*i as usize];
        welded |= shared != *i;
        *i = shared;
    }
    let mut write = 0;
    for read in (0..mesh.triangles.len()).step_by(3) {
        let [a, b, c] = [
            mesh.triangles[read],
            mesh.triangles[read + 1],
            mesh.triangles[read + 2],
        ]
        .map(|i| mesh.vertices[i as usize].as_dvec3());
        if (b - a).cross(c - a).z > 0.0 && a.z.max(b.z).max(c.z) > 0.0 {
            mesh.triangles.copy_within(read..read + 3, write);
            write += 3;
        }
    }
    if !welded && write == mesh.triangles.len() {
        return mesh;
    }
    mesh.triangles.truncate(write);
    mesh.calculate_normals();
    mesh.clipped_above(0.0)
}

#[allow(clippy::cast_possible_truncation)]
fn append(mesh: &mut Mesh, position: Vec3, uv: Vec2) -> u32 {
    let id = mesh.vertices.len() as u32;
    mesh.vertices.push(position);
    mesh.uv.push(uv);
    id
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coincident_contacts_are_shared_instead_of_becoming_artificial_dry_banks() {
        let mut vertices = vec![
            Vec3::new(0.0, 0.0, 0.01),
            Vec3::new(1.0, 0.0, 0.01),
            Vec3::new(1.0, 1.0, 0.01),
            Vec3::new(0.0, 1.0, 0.01),
        ];
        vertices.extend([Vec3::new(0.5, 0.5, 0.01); 4]);
        let mesh = clip_above_sea(Mesh {
            vertices,
            triangles: vec![0, 1, 4, 1, 2, 5, 2, 3, 6, 3, 0, 7],
            ..Mesh::default()
        });
        assert_eq!(mesh.vertices.len(), 5);
        assert_eq!(mesh.triangles.len(), 12);
        assert_eq!(mesh.perimeter_vertices().len(), 4);
    }

    #[test]
    fn rounded_sea_intersections_drop_only_collapsed_faces_and_keep_thin_contacts() {
        let source = Mesh {
            vertices: vec![
                Vec3::new(0.7, 0.8, -1.0e-12),
                Vec3::new(0.7001, 0.8001, 0.01),
                Vec3::new(0.7001, 0.8002, 0.01),
            ],
            triangles: vec![0, 1, 2],
            ..Mesh::default()
        };
        let raw = source.clipped_above(0.0);
        assert_eq!(raw.triangles.len(), 6);
        let cleaned = clip_above_sea(raw);
        assert_eq!(cleaned.triangles.len(), 3);
        assert_eq!(cleaned.vertices.len(), 3);
        let thin = Mesh {
            vertices: vec![
                Vec3::new(0.7, 0.8, 0.01),
                Vec3::new(0.7001, 0.8, 0.01),
                Vec3::new(0.7001, 0.8_f32.next_up(), 0.01),
            ],
            triangles: vec![0, 1, 2],
            ..Mesh::default()
        };
        let retained = clip_above_sea(thin);
        assert_eq!(retained.triangles.len(), 3);
    }

    #[test]
    fn inclination_blends_the_whole_surface_and_keeps_dry_banks_in_contact() {
        let bed = 0.04;
        let hydraulic = 0.05;
        let vertical = water_height(bed, hydraulic, Vec3::X, false);
        let flat = water_height(bed, hydraulic, Vec3::Z, false);
        let halfway = water_height(bed, hydraulic, Vec3::new(0.75_f32.sqrt(), 0.0, 0.5), false);
        assert!((vertical - bed - WATERFALL_WATER_CLEARANCE).abs() < 1.0e-8);
        assert!((flat - hydraulic - RIVER_SURFACE_OFFSET).abs() < 1.0e-8);
        assert!((halfway - (vertical + flat) * 0.5).abs() < 1.0e-8);
        for normal in [Vec3::X, Vec3::Z, Vec3::new(0.75_f32.sqrt(), 0.0, 0.5)] {
            assert!(water_height(bed, hydraulic, normal, true) <= bed);
            assert!(water_height(-0.001, 0.0, normal, false) <= 0.0);
        }
    }

    #[test]
    fn clipped_bank_intersections_touch_the_terrain_and_share_the_sea_handoff() {
        let terrain = Mesh {
            vertices: vec![
                Vec3::new(0.0, 0.0, -0.02),
                Vec3::new(1.0, 0.0, 0.02),
                Vec3::new(1.0, 1.0, 0.04),
                Vec3::new(0.0, 1.0, 0.02),
            ],
            normals: vec![Vec3::Z; 4],
            triangles: vec![0, 1, 2, 0, 2, 3],
            ..Mesh::default()
        };
        let water = build(
            &terrain,
            &[2, 2, 1 | super::super::RIVER_BOUNDARY, 2],
            &[-0.01, 0.03, 0.03, 0.03],
            &[Vec2::ZERO; 4],
        );
        assert!(!water.triangles.is_empty());
        assert!(water.vertices.iter().all(|p| p.z >= -1.0e-9));
        // The dry high corner is clipped back to its sloping terrain plane.
        assert!(!water.vertices.iter().any(|p| p.truncate() == Vec2::ONE));
        let mut contacts = 0;
        for p in &water.vertices {
            let ground = if p.x >= p.y {
                0.04 * p.x + 0.02 * p.y - 0.02
            } else {
                0.02 * p.x + 0.04 * p.y - 0.02
            };
            assert!(p.z >= ground - 1.0e-7);
            if (p.z - ground).abs() < 1.0e-7 && p.z > 0.0 {
                contacts += 1;
            }
        }
        assert!(contacts >= 2);
        assert!(water.vertices.iter().any(|p| p.z.abs() < 1.0e-8));
    }
}
