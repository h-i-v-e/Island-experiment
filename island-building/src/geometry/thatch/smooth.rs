//! One Catmull-Clark subdivision of a closed control cage, refitted to its bounds.
use super::unit;
use std::collections::BTreeMap;

type Point = [f32; 3];

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Sample {
    pub point: Point,
    pub normal: Point,
}

#[derive(Debug, PartialEq)]
pub(super) struct Patch {
    pub vertices: [Sample; 4],
    pub source_normal: Point,
}

impl Patch {
    pub fn normal(&self) -> Point {
        face_normal(&self.vertices.map(|v| v.point), &[0, 1, 2, 3])
    }
}

#[derive(Debug, PartialEq)]
pub(super) struct Surface {
    pub patches: Vec<Patch>,
    bounds: [Point; 2],
}

struct Edge {
    faces: [usize; 2],
    count: usize,
    index: usize,
}

impl Surface {
    pub fn matches(&self, profile: &[[f32; 2]], depth: f32) -> bool {
        let bounds = profile_bounds(profile, depth);
        self.bounds == bounds
    }

    pub fn sample(&self, x: f32, y: f32) -> Option<Sample> {
        self.patches
            .iter()
            .flat_map(|patch| {
                [[0, 1, 2], [0, 2, 3]]
                    .into_iter()
                    .filter_map(|indices| triangle_sample(indices.map(|i| patch.vertices[i]), x, y))
            })
            .max_by(|a, b| a.point[2].total_cmp(&b.point[2]))
    }
}

/// Short supporting edges keep one pass local to the corners of long roof slabs.
pub(super) fn prism(profile: &[[f32; 2]], depth: f32) -> Surface {
    let mut ring = Vec::with_capacity(profile.len() * 3);
    for (a, b) in profile
        .iter()
        .zip(profile.iter().cycle().skip(1))
        .take(profile.len())
    {
        let length = (b[0] - a[0]).hypot(b[1] - a[1]);
        let inset = (0.10 / length).min(0.25);
        for t in [0., inset, 1. - inset] {
            ring.push([a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]);
        }
    }
    let inset = 0.10_f32.min(depth * 0.25);
    let points: Vec<_> = [-depth, -depth + inset, depth - inset, depth]
        .into_iter()
        .flat_map(|y| ring.iter().map(move |p| [p[0], y, p[1]]))
        .collect();
    let count = ring.len();
    let mut faces = Vec::with_capacity(count * 3 + 2);
    for row in 0..3 {
        for i in 0..count {
            let next = (i + 1) % count;
            faces.push(vec![
                row * count + i,
                (row + 1) * count + i,
                (row + 1) * count + next,
                row * count + next,
            ]);
        }
    }
    faces.push((0..count).collect());
    faces.push((count * 3..count * 4).rev().collect());
    subdivide(&points, &faces, profile_bounds(profile, depth))
}

fn subdivide(points: &[Point], faces: &[Vec<usize>], bounds: [Point; 2]) -> Surface {
    let centres: Vec<_> = faces
        .iter()
        .map(|face| average(face.iter().map(|&i| points[i])))
        .collect();
    let mut edges = edges(faces);
    let mut refined = refine_vertices(points, faces, &centres, &mut edges);
    refit(&mut refined, bounds);
    let quads = split_faces(points, faces, &edges);
    let normals = smooth_normals(&refined, &quads);
    Surface {
        patches: quads
            .into_iter()
            .map(|(quad, source_normal)| Patch {
                vertices: quad.map(|i| Sample {
                    point: refined[i],
                    normal: unit(normals[i]),
                }),
                source_normal,
            })
            .collect(),
        bounds,
    }
}

fn refine_vertices(
    points: &[Point],
    faces: &[Vec<usize>],
    centres: &[Point],
    edges: &mut BTreeMap<[usize; 2], Edge>,
) -> Vec<Point> {
    let mut refined: Vec<_> = points
        .iter()
        .enumerate()
        .map(|(i, point)| {
            let face_mean = average(
                faces
                    .iter()
                    .zip(centres)
                    .filter(|(face, _)| face.contains(&i))
                    .map(|(_, &centre)| centre),
            );
            let neighbours = || edges.keys().filter(|edge| edge.contains(&i));
            let edge_mean =
                average(neighbours().map(|edge| average(edge.iter().map(|&j| points[j]))));
            let n = f32::from(u16::try_from(neighbours().count()).expect("small cage valence"));
            std::array::from_fn(|axis| {
                (face_mean[axis] + 2. * edge_mean[axis] + (n - 3.) * point[axis]) / n
            })
        })
        .collect();
    refined.extend_from_slice(centres);
    for (key, edge) in edges {
        assert_eq!(edge.count, 2, "closed manifold control cage");
        edge.index = refined.len();
        refined.push(average(
            [
                points[key[0]],
                points[key[1]],
                centres[edge.faces[0]],
                centres[edge.faces[1]],
            ]
            .into_iter(),
        ));
    }
    refined
}

fn split_faces(
    points: &[Point],
    faces: &[Vec<usize>],
    edges: &BTreeMap<[usize; 2], Edge>,
) -> Vec<([usize; 4], Point)> {
    let centre_start = points.len();
    let mut quads = Vec::new();
    for (index, face) in faces.iter().enumerate() {
        let source_normal = face_normal(points, face);
        for (i, &corner) in face.iter().enumerate() {
            let next = face[(i + 1) % face.len()];
            let previous = face[(i + face.len() - 1) % face.len()];
            quads.push((
                [
                    corner,
                    edges[&edge_key(corner, next)].index,
                    centre_start + index,
                    edges[&edge_key(previous, corner)].index,
                ],
                source_normal,
            ));
        }
    }
    quads
}

fn smooth_normals(points: &[Point], quads: &[([usize; 4], Point)]) -> Vec<Point> {
    let mut normals = vec![[0.; 3]; points.len()];
    for (quad, _) in quads {
        for triangle in [[quad[0], quad[1], quad[2]], [quad[0], quad[2], quad[3]]] {
            let normal = face_normal(points, &triangle);
            for i in 0..3 {
                let point = points[triangle[i]];
                let a = unit(std::array::from_fn(|axis| {
                    points[triangle[(i + 1) % 3]][axis] - point[axis]
                }));
                let b = unit(std::array::from_fn(|axis| {
                    points[triangle[(i + 2) % 3]][axis] - point[axis]
                }));
                let angle = a
                    .into_iter()
                    .zip(b)
                    .map(|(a, b)| a * b)
                    .sum::<f32>()
                    .clamp(-1., 1.)
                    .acos();
                for axis in 0..3 {
                    normals[triangle[i]][axis] += normal[axis] * angle;
                }
            }
        }
    }
    normals
}

fn edges(faces: &[Vec<usize>]) -> BTreeMap<[usize; 2], Edge> {
    let mut edges = BTreeMap::new();
    for (index, face) in faces.iter().enumerate() {
        for i in 0..face.len() {
            let key = edge_key(face[i], face[(i + 1) % face.len()]);
            let edge = edges.entry(key).or_insert(Edge {
                faces: [0; 2],
                count: 0,
                index: 0,
            });
            edge.faces[edge.count] = index;
            edge.count += 1;
        }
    }
    edges
}

fn edge_key(a: usize, b: usize) -> [usize; 2] {
    [a.min(b), a.max(b)]
}

fn average(points: impl Iterator<Item = Point>) -> Point {
    let (sum, count) = points.fold(([0.; 3], 0_u16), |(sum, n), p| {
        (std::array::from_fn(|axis| sum[axis] + p[axis]), n + 1)
    });
    sum.map(|v| v / f32::from(count))
}

fn profile_bounds(profile: &[[f32; 2]], depth: f32) -> [Point; 2] {
    point_bounds(
        profile
            .iter()
            .flat_map(|&[x, z]| [[x, -depth, z], [x, depth, z]]),
    )
}

fn point_bounds(points: impl Iterator<Item = Point>) -> [Point; 2] {
    points.fold([[f32::INFINITY; 3], [f32::NEG_INFINITY; 3]], |bounds, p| {
        [
            std::array::from_fn(|i| bounds[0][i].min(p[i])),
            std::array::from_fn(|i| bounds[1][i].max(p[i])),
        ]
    })
}

fn refit(points: &mut [Point], original: [Point; 2]) {
    let bounds = point_bounds(points.iter().copied());
    for point in points {
        for axis in 0..3 {
            point[axis] = original[0][axis]
                + (point[axis] - bounds[0][axis]) / (bounds[1][axis] - bounds[0][axis])
                    * (original[1][axis] - original[0][axis]);
        }
    }
}

fn face_vector(points: &[Point], indices: &[usize]) -> Point {
    let mut normal = [0.; 3];
    for i in 0..indices.len() {
        let a = points[indices[i]];
        let b = points[indices[(i + 1) % indices.len()]];
        for (axis, value) in normal.iter_mut().enumerate() {
            let j = (axis + 1) % 3;
            let k = (axis + 2) % 3;
            *value += (a[j] - b[j]) * (a[k] + b[k]);
        }
    }
    normal
}

fn face_normal(points: &[Point], indices: &[usize]) -> Point {
    unit(face_vector(points, indices))
}

fn triangle_sample(vertices: [Sample; 3], x: f32, y: f32) -> Option<Sample> {
    let [a, b, c] = vertices.map(|v| v.point);
    let det = (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0]);
    if det <= 0.000_000_1 {
        return None;
    }
    let u = ((x - a[0]) * (c[1] - a[1]) - (y - a[1]) * (c[0] - a[0])) / det;
    let v = ((b[0] - a[0]) * (y - a[1]) - (b[1] - a[1]) * (x - a[0])) / det;
    if u < -0.00001 || v < -0.00001 || u + v > 1.00001 {
        return None;
    }
    let weights = [1. - u - v, u, v];
    let interpolate = |values: [Point; 3]| {
        std::array::from_fn(|axis| {
            values
                .into_iter()
                .zip(weights)
                .map(|(p, w)| p[axis] * w)
                .sum()
        })
    };
    Some(Sample {
        point: interpolate(vertices.map(|v| v.point)),
        normal: unit(interpolate(vertices.map(|v| v.normal))),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_single_pass_produces_four_quads_per_cube_face_and_restores_dimensions() {
        let points = [
            [-2., -3., -1.],
            [2., -3., -1.],
            [2., 3., -1.],
            [-2., 3., -1.],
            [-2., -3., 1.],
            [2., -3., 1.],
            [2., 3., 1.],
            [-2., 3., 1.],
        ];
        let faces = [
            vec![0, 3, 2, 1],
            vec![4, 5, 6, 7],
            vec![0, 1, 5, 4],
            vec![1, 2, 6, 5],
            vec![2, 3, 7, 6],
            vec![3, 0, 4, 7],
        ];
        let bounds = point_bounds(points.into_iter());
        let surface = subdivide(&points, &faces, bounds);
        assert_eq!(surface.patches.len(), 24);
        let actual = point_bounds(
            surface
                .patches
                .iter()
                .flat_map(|p| p.vertices.map(|v| v.point)),
        );
        assert_eq!(actual, bounds);
        assert!(
            surface.patches.iter().flat_map(|p| &p.vertices).all(|v| {
                !points.contains(&v.point) && v.normal.into_iter().all(f32::is_finite)
            })
        );
    }

    #[test]
    fn supported_prisms_are_closed_smooth_and_deterministic_after_refitting() {
        let profile = [[-0.7, 5.], [0.7, 5.], [0.7, 5.135], [0., 6.], [-0.7, 5.135]];
        let surface = prism(&profile, 3.);
        assert_eq!(surface, prism(&profile, 3.));
        assert_eq!(
            point_bounds(
                surface
                    .patches
                    .iter()
                    .flat_map(|p| p.vertices.map(|v| v.point))
            ),
            profile_bounds(&profile, 3.)
        );
        let mut edges = BTreeMap::new();
        for patch in &surface.patches {
            for i in 0..4 {
                let key = |v: Sample| v.point.map(f32::to_bits);
                let a = key(patch.vertices[i]);
                let b = key(patch.vertices[(i + 1) % 4]);
                *edges
                    .entry(if a < b { [a, b] } else { [b, a] })
                    .or_insert(0) += 1;
            }
        }
        assert!(edges.values().all(|&count| count == 2));
        let top = surface.sample(0., 0.).unwrap();
        assert!((top.point[2] - 6.).abs() < 0.0001);
        assert!(top.normal[2] > 0.99);
    }
}
