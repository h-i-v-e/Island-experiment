//! Clip the shared water field against actual terrain triangle planes. Bank
//! intersections are cached per terrain edge, so adjacent water faces agree.
use crate::{DVec3, PlanetError, PlanetMesh};
use std::collections::BTreeMap;

#[derive(Debug, Default, PartialEq)]
pub struct RiverSurface {
    pub positions: Vec<DVec3>,
    pub normals: Vec<DVec3>,
    pub triangles: Vec<[u32; 3]>,
    /// The supporting terrain triangle for each water triangle.
    pub terrain_faces: Vec<usize>,
}

#[derive(Clone, Copy)]
struct Corner {
    id: u32,
    fields: [f64; 2],
}

#[cfg(test)]
pub(super) fn build(mesh: &PlanetMesh, heights: &[f64], water: &[f64]) -> RiverSurface {
    build_with_sources(mesh, heights, water).surface
}

#[derive(Clone, Copy, Debug)]
pub(super) enum Source {
    Terrain(usize),
    Edge { vertices: [u32; 2], fraction: f64 },
    Sea,
}

pub(super) struct BuiltSurface {
    pub surface: RiverSurface,
    pub sources: Vec<Source>,
}

pub(super) fn build_with_sources(
    mesh: &PlanetMesh,
    heights: &[f64],
    water: &[f64],
) -> BuiltSurface {
    let mut result = RiverSurface::default();
    let mut sources = Vec::new();
    let mut vertices = vec![None; heights.len()];
    let mut edges = BTreeMap::new();
    let fields: Vec<_> = water
        .iter()
        .zip(heights)
        .map(|(&w, &h)| clip_fields(h, w))
        .collect();
    let mut polygon = Vec::with_capacity(6);
    let mut clipped = Vec::with_capacity(6);
    for (face_id, &face) in mesh.triangles.iter().enumerate() {
        if face.iter().all(|&i| fields[i as usize][1] <= 0.0)
            || face.iter().all(|&i| fields[i as usize][0] <= 0.0)
        {
            continue;
        }
        polygon.clear();
        for vertex in face {
            let i = vertex as usize;
            let id = *vertices[i].get_or_insert_with(|| {
                sources.push(Source::Terrain(i));
                append(&mut result, mesh.directions[i] * (1.0 + fields[i][1]))
            });
            polygon.push(Corner {
                id,
                fields: fields[i],
            });
        }
        // The bank is an actual terrain intersection, never a width clip
        // through open water. The second plane is the sea handoff.
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
                    let intersection = if t <= 0.0 {
                        a
                    } else if t >= 1.0 {
                        b
                    } else {
                        let id = *edges.entry((plane, a.id, b.id)).or_insert_with(|| {
                            let point = result.positions[a.id as usize]
                                .lerp(result.positions[b.id as usize], t);
                            let source = if plane == 0 {
                                bank_source(
                                    mesh,
                                    heights,
                                    sources[a.id as usize],
                                    sources[b.id as usize],
                                    point,
                                )
                            } else {
                                Source::Sea
                            };
                            sources.push(source);
                            append(&mut result, point)
                        });
                        Corner {
                            id,
                            fields: std::array::from_fn(|j| {
                                a.fields[j] + (b.fields[j] - a.fields[j]) * t
                            }),
                        }
                    };
                    clipped.push(intersection);
                }
            }
            std::mem::swap(&mut polygon, &mut clipped);
        }
        for i in 1..polygon.len().saturating_sub(1) {
            let face = [polygon[0].id, polygon[i].id, polygon[i + 1].id];
            let [a, b, c] = face.map(|id| result.positions[id as usize]);
            // Thin contact triangles still close the bank. Dropping them by an
            // absolute area cutoff opens a seam along their much longer edge.
            if (b - a).cross(c - a).length_squared() <= f64::MIN_POSITIVE {
                continue;
            }
            result.triangles.push(face);
            result.terrain_faces.push(face_id);
        }
    }
    calculate_normals(&mut result);
    BuiltSurface {
        surface: result,
        sources,
    }
}

fn clip_fields(h: f64, w: f64) -> [f64; 2] {
    // Snap both clip fields globally. Tiny positive sea heights from
    // minimum drainage grades otherwise create duplicate near-endpoint
    // intersections and inverted slivers at the sea handoff.
    let w = if w.abs() < 1.0e-10 { 0.0 } else { w };
    let clearance = (1.0 + w) / (1.0 + h) - 1.0;
    // Snap near-equal input corners once for ALL incident faces. Per-edge
    // snapping can merge one side of a tiny cut while leaving the other.
    // This is below the existing bed/bank tolerances and GPU precision.
    [
        if clearance.abs() < 1.0e-10 {
            0.0
        } else {
            clearance
        },
        w,
    ]
}

fn calculate_normals(surface: &mut RiverSurface) {
    surface.normals.resize(surface.positions.len(), DVec3::ZERO);
    for face in &surface.triangles {
        let [a, b, c] = face.map(|i| surface.positions[i as usize]);
        let normal = (b - a).cross(c - a);
        for &i in face {
            surface.normals[i as usize] += normal;
        }
    }
    for normal in &mut surface.normals {
        *normal = normal.normalize_or_zero();
    }
}

fn bank_source(
    mesh: &PlanetMesh,
    heights: &[f64],
    left: Source,
    right: Source,
    point: DVec3,
) -> Source {
    let (Source::Terrain(left), Source::Terrain(right)) = (left, right) else {
        unreachable!("bank clipping precedes sea clipping")
    };
    let start = mesh.directions[left] * (1.0 + heights[left]);
    let end = mesh.directions[right] * (1.0 + heights[right]);
    Source::Edge {
        vertices: [u32::try_from(left).unwrap(), u32::try_from(right).unwrap()],
        fraction: ((point - start).dot(end - start) / end.distance_squared(start)).clamp(0.0, 1.0),
    }
}

#[allow(clippy::cast_possible_truncation)]
fn append(surface: &mut RiverSurface, point: DVec3) -> u32 {
    // Input geometry is bounded to two million vertices.
    let id = surface.positions.len() as u32;
    surface.positions.push(point);
    id
}

impl RiverSurface {
    pub(super) fn dry_bank_edges(&self) -> Vec<(u32, u32)> {
        let mut edges = BTreeMap::<_, usize>::new();
        for &[a, b, c] in &self.triangles {
            for (a, b) in [(a, b), (b, c), (c, a)] {
                *edges.entry((a.min(b), a.max(b))).or_default() += 1;
            }
        }
        edges
            .into_iter()
            .filter_map(|((a, b), count)| {
                let left = self.positions[a as usize];
                let right = self.positions[b as usize];
                (count == 1
                    && left.length().min(right.length()) > 1.0 + 1.0e-9
                    && left.distance_squared(right) > 1.0e-18)
                    .then_some((a, b))
            })
            .collect()
    }

    /// # Errors
    /// Rejects invalid or submerged-in-terrain water triangles.
    pub fn validate(&self, mesh: &PlanetMesh, heights: &[f64]) -> Result<(), PlanetError> {
        if self.triangles.len() != self.terrain_faces.len()
            || self.positions.len() != self.normals.len()
        {
            return Err(PlanetError::InvalidTopology("river surface field lengths"));
        }
        for (face, &terrain_face) in self.triangles.iter().zip(&self.terrain_faces) {
            let [a, b, c] = mesh.triangles[terrain_face]
                .map(|i| mesh.directions[i as usize] * (1.0 + heights[i as usize]));
            let normal = (b - a).cross(c - a).normalize();
            let [wa, wb, wc] = face.map(|i| self.positions[i as usize]);
            if (wb - wa).cross(wc - wa).dot(wa + wb + wc) <= 0.0 {
                return Err(PlanetError::InvalidTopology("inverted river surface"));
            }
            for &i in face {
                let point = self.positions[i as usize];
                if !point.is_finite() || normal.dot(point - a) < -1.0e-9 {
                    return Err(PlanetError::InvalidTopology(
                        "water surface lies below its carved terrain face",
                    ));
                }
            }
        }
        self.validate_banks(mesh, heights)?;
        Ok(())
    }

    fn validate_banks(&self, mesh: &PlanetMesh, heights: &[f64]) -> Result<(), PlanetError> {
        let mut edges = BTreeMap::<_, (usize, usize)>::new();
        for (face, &support) in self.triangles.iter().zip(&self.terrain_faces) {
            for (a, b) in [(face[0], face[1]), (face[1], face[2]), (face[2], face[0])] {
                let entry = edges.entry((a.min(b), a.max(b))).or_insert((0, support));
                entry.0 += 1;
            }
        }
        for ((left, right), (count, support)) in edges {
            if count != 1 {
                continue;
            }
            let left = self.positions[left as usize];
            let right = self.positions[right as usize];
            // Sub-pixel numerical slivers and the sea handoff are not dry banks.
            if left.distance_squared(right) < 1.0e-18
                || (left.length() <= 1.0 + 1.0e-9 && right.length() <= 1.0 + 1.0e-9)
            {
                continue;
            }
            let [a, b, c] = mesh.triangles[support]
                .map(|i| mesh.directions[i as usize] * (1.0 + heights[i as usize]));
            let normal = (b - a).cross(c - a).normalize();
            if normal.dot(left - a).abs() > 1.0e-8 || normal.dot(right - a).abs() > 1.0e-8 {
                return Err(PlanetError::InvalidTopology(
                    "river bank floats above its terrain",
                ));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_bank_contacts_reuse_vertices_without_cracks_or_sliver_faces() {
        let mesh = PlanetMesh {
            directions: vec![
                DVec3::Z,
                DVec3::X,
                DVec3::Y,
                -DVec3::X,
                -DVec3::Y,
                -DVec3::Z,
            ],
            triangles: vec![
                [0, 1, 2],
                [0, 2, 3],
                [0, 3, 4],
                [0, 4, 1],
                [5, 2, 1],
                [5, 3, 2],
                [5, 4, 3],
                [5, 1, 4],
            ],
            radius: 1000.0,
        };
        let surface = build(&mesh, &[0.02; 6], &[0.03, 0.03, 0.02, 0.02, 0.02, 0.02]);
        assert_eq!(
            surface.positions.len(),
            6,
            "bank intersections must reuse terrain corners"
        );
        assert_eq!(surface.triangles.len(), 6);
        surface.validate(&mesh, &[0.02; 6]).unwrap();
        let mut heights = [0.02; 6];
        heights[2] += 2.0e-16;
        let surface = build(&mesh, &heights, &[0.03, 0.03, 0.02, 0.02, 0.02, 0.02]);
        assert_eq!(
            surface.positions.len(),
            6,
            "near-equal bank contacts must also reuse corners"
        );
        surface.validate(&mesh, &heights).unwrap();
    }

    #[test]
    fn fine_fall_triangles_share_nearly_pinned_endpoints_instead_of_opening_a_crack() {
        let mesh = PlanetMesh {
            directions: [
                (-0.0001, 0.0),
                (0.0001, 0.0),
                (0.0, 0.0002),
                (-0.0003, 0.0),
                (0.0, -0.0002),
                (0.0003, 0.0),
            ]
            .map(|(x, y)| DVec3::new(x, y, 1.0).normalize())
            .to_vec(),
            triangles: vec![
                [0, 1, 2],
                [0, 2, 3],
                [0, 3, 4],
                [0, 4, 1],
                [1, 4, 5],
                [1, 5, 2],
            ],
            radius: 1000.0,
        };
        let mut heights = [0.02, 0.02, 0.0203, 0.0203, 0.0203, 0.0203];
        heights[2] += 8.0e-13;
        let surface = build(&mesh, &heights, &[0.0203; 6]);
        assert_eq!(surface.positions.len(), 6);
        assert_eq!(surface.triangles.len(), 6);
        surface.validate(&mesh, &heights).unwrap();
    }

    #[test]
    fn a_surface_above_the_bed_is_rejected_if_its_open_banks_float() {
        let mesh = PlanetMesh {
            directions: vec![
                DVec3::Z,
                DVec3::new(0.01, 0.0, 1.0).normalize(),
                DVec3::new(0.0, 0.01, 1.0).normalize(),
            ],
            triangles: vec![[0, 1, 2]],
            radius: 1000.0,
        };
        let mut surface = RiverSurface {
            positions: mesh.directions.iter().map(|p| p * 1.03).collect(),
            normals: vec![DVec3::Z; 3],
            triangles: vec![[0, 1, 2]],
            terrain_faces: vec![0],
        };
        assert_eq!(
            surface.validate(&mesh, &[0.02; 3]),
            Err(PlanetError::InvalidTopology(
                "river bank floats above its terrain"
            ))
        );
        for (point, direction) in surface.positions.iter_mut().zip(&mesh.directions) {
            *point = direction * 1.02;
        }
        surface.validate(&mesh, &[0.02; 3]).unwrap();
    }
}
