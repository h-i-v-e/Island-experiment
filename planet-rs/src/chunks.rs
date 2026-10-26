//! Icosahedral terrain patches with shared, undisplaced cut vertices.

use std::{borrow::Cow, collections::HashMap};

use crate::{DVec3, PlanetError};

const MAX_PARTITION_LEVEL: u8 = 6;
const PLANE_EPSILON: f64 = 2.0e-13;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ChunkVertex {
    pub position: DVec3,
    pub normal: DVec3,
    pub colour: [f32; 4],
    /// Preserve authored features and all partition edges during simplification.
    pub protected: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ChunkMesh {
    pub vertices: Vec<ChunkVertex>,
    pub triangles: Vec<[u32; 3]>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Chunk {
    pub id: u32,
    pub mesh: ChunkMesh,
    pub bounds: ChunkBounds,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ChunkBounds {
    pub centre: DVec3,
    pub radius: f64,
    pub direction: DVec3,
    pub half_angle: f64,
    pub max_radius: f64,
}

impl ChunkBounds {
    /// Bounds enclose every triangle, including its interior, not just its vertices.
    #[must_use]
    pub fn from_mesh(mesh: &ChunkMesh) -> Self {
        let Some(first) = mesh.vertices.first() else {
            return Self::default();
        };
        let (minimum, maximum, sum) = mesh.vertices.iter().fold(
            (first.position, first.position, DVec3::ZERO),
            |(minimum, maximum, sum), vertex| {
                (
                    minimum.min(vertex.position),
                    maximum.max(vertex.position),
                    sum + vertex.position.normalize_or_zero(),
                )
            },
        );
        let centre = (minimum + maximum) * 0.5;
        let direction = sum.normalize_or_zero();
        let mut result = Self {
            centre,
            direction,
            ..Self::default()
        };
        for vertex in &mesh.vertices {
            result.radius = result.radius.max(centre.distance(vertex.position));
            result.max_radius = result.max_radius.max(vertex.position.length());
            result.half_angle = result.half_angle.max(
                direction
                    .dot(vertex.position.normalize_or_zero())
                    .clamp(-1.0, 1.0)
                    .acos(),
            );
        }
        // A sub-hemispherical angular cone is convex. Wider caps cannot bound
        // triangle interiors this way, so conservatively disable angular culling.
        if direction == DVec3::ZERO || result.half_angle >= std::f64::consts::FRAC_PI_2 {
            result.half_angle = std::f64::consts::PI;
        }
        result.radius += result.radius.max(1.0) * 1.0e-12;
        result.half_angle += 1.0e-12;
        result.max_radius += result.max_radius.max(1.0) * 1.0e-12;
        result
    }

    /// Whether any part of the patch could remain above a spherical horizon.
    ///
    /// The angular cap and its greatest height make this deliberately conservative:
    /// tall peaks remain visible beyond the sea-level horizon. `occluder_radius`
    /// must describe a sphere wholly inside every rendered terrain representation.
    #[must_use]
    pub fn horizon_visible(self, camera: DVec3, occluder_radius: f64) -> bool {
        let camera_radius = camera.length();
        if !camera_radius.is_finite()
            || !occluder_radius.is_finite()
            || occluder_radius <= 0.0
            || camera_radius <= occluder_radius
            || self.half_angle >= std::f64::consts::PI
        {
            return true;
        }
        let camera_horizon = (occluder_radius / camera_radius).clamp(0.0, 1.0).acos();
        let patch_horizon = (occluder_radius / self.max_radius.max(occluder_radius))
            .clamp(0.0, 1.0)
            .acos();
        let separation = self
            .direction
            .dot(camera / camera_radius)
            .clamp(-1.0, 1.0)
            .acos();
        separation <= camera_horizon + patch_horizon + self.half_angle + 1.0e-12
    }
}

#[derive(Clone, Copy, Debug)]
struct Side {
    plane: usize,
    sign: f64,
}

#[derive(Debug)]
struct Cell {
    sides: [Side; 3],
    children: Vec<Self>,
    leaf: usize,
}

#[derive(Default)]
struct Partition {
    rays: Vec<DVec3>,
    planes: Vec<DVec3>,
    edges: HashMap<(usize, usize), usize>,
    corners: HashMap<(usize, usize), Vec<usize>>,
    leaves: usize,
}

impl Partition {
    fn side(&mut self, a: usize, b: usize, inside: usize) -> Side {
        let key = ordered(a, b);
        let plane = *self.edges.entry(key).or_insert_with(|| {
            let index = self.planes.len();
            self.planes
                .push(self.rays[key.0].cross(self.rays[key.1]).normalize());
            index
        });
        Side {
            plane,
            sign: self.planes[plane].dot(self.rays[inside]).signum(),
        }
    }

    fn midpoint(
        &mut self,
        a: usize,
        b: usize,
        midpoints: &mut HashMap<(usize, usize), usize>,
    ) -> usize {
        *midpoints.entry(ordered(a, b)).or_insert_with(|| {
            let index = self.rays.len();
            self.rays.push((self.rays[a] + self.rays[b]).normalize());
            index
        })
    }

    fn cell(
        &mut self,
        corners: [usize; 3],
        inherited: [Option<Side>; 3],
        level: u8,
        midpoints: &mut HashMap<(usize, usize), usize>,
    ) -> Cell {
        let sides = std::array::from_fn(|i| {
            let mut side = inherited[i].unwrap_or_else(|| {
                self.side(corners[i], corners[(i + 1) % 3], corners[(i + 2) % 3])
            });
            side.sign = self.planes[side.plane]
                .dot(self.rays[corners[(i + 2) % 3]])
                .signum();
            side
        });
        for i in 0..3 {
            let key = ordered(sides[i].plane, sides[(i + 2) % 3].plane);
            let rays = self.corners.entry(key).or_default();
            if !rays.contains(&corners[i]) {
                rays.push(corners[i]);
            }
        }
        let children = if level == 0 {
            Vec::new()
        } else {
            let [first, second, third] = corners;
            let midpoint_ab = self.midpoint(first, second, midpoints);
            let midpoint_bc = self.midpoint(second, third, midpoints);
            let midpoint_ca = self.midpoint(third, first, midpoints);
            vec![
                self.cell(
                    [first, midpoint_ab, midpoint_ca],
                    [Some(sides[0]), None, Some(sides[2])],
                    level - 1,
                    midpoints,
                ),
                self.cell(
                    [midpoint_ab, second, midpoint_bc],
                    [Some(sides[0]), Some(sides[1]), None],
                    level - 1,
                    midpoints,
                ),
                self.cell(
                    [midpoint_ca, midpoint_bc, third],
                    [None, Some(sides[1]), Some(sides[2])],
                    level - 1,
                    midpoints,
                ),
                self.cell(
                    [midpoint_ab, midpoint_bc, midpoint_ca],
                    [None; 3],
                    level - 1,
                    midpoints,
                ),
            ]
        };
        let leaf = self.leaves;
        if level == 0 {
            self.leaves += 1;
        }
        Cell {
            sides,
            children,
            leaf,
        }
    }

    fn new(level: u8) -> (Self, Vec<Cell>) {
        let phi = f64::midpoint(1.0, 5.0_f64.sqrt());
        let mut partition = Self {
            rays: [
                (-1.0, phi, 0.0),
                (1.0, phi, 0.0),
                (-1.0, -phi, 0.0),
                (1.0, -phi, 0.0),
                (0.0, -1.0, phi),
                (0.0, 1.0, phi),
                (0.0, -1.0, -phi),
                (0.0, 1.0, -phi),
                (phi, 0.0, -1.0),
                (phi, 0.0, 1.0),
                (-phi, 0.0, -1.0),
                (-phi, 0.0, 1.0),
            ]
            .map(|(x, y, z)| DVec3::new(x, y, z).normalize())
            .to_vec(),
            ..Self::default()
        };
        let faces = [
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
        let mut midpoints = HashMap::new();
        let roots = faces
            .into_iter()
            .map(|corners| partition.cell(corners, [None; 3], level, &mut midpoints))
            .collect();
        (partition, roots)
    }
}

fn ordered<T: Ord>(a: T, b: T) -> (T, T) {
    if a <= b { (a, b) } else { (b, a) }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum VertexKey {
    Original(u32),
    Edge(u32, u32, usize),
    Corner(usize, usize),
}

#[derive(Clone, Copy)]
struct ClipVertex {
    key: VertexKey,
    vertex: ChunkVertex,
    barycentric: DVec3,
}

struct TriangleClip<'a> {
    source: &'a ChunkMesh,
    partition: &'a Partition,
    triangle: [u32; 3],
    triangle_id: usize,
    cache: &'a mut HashMap<VertexKey, ChunkVertex>,
    failed: bool,
}

impl TriangleClip<'_> {
    fn intersection(
        &mut self,
        a: ClipVertex,
        b: ClipVertex,
        plane: usize,
        active: &[usize],
    ) -> ClipVertex {
        let normal = self.partition.planes[plane];
        let da = normal.dot(a.vertex.position);
        let db = normal.dot(b.vertex.position);
        let factor = da / (da - db);
        let candidate = a.vertex.position.lerp(b.vertex.position, factor);
        let source_edge = (0..3).find(|&i| {
            a.barycentric[i].abs() < PLANE_EPSILON && b.barycentric[i].abs() < PLANE_EPSILON
        });
        let (key, position, barycentric) = if let Some(opposite) = source_edge {
            let (start_index, end_index) = ordered(
                self.triangle[(opposite + 1) % 3],
                self.triangle[(opposite + 2) % 3],
            );
            let start = self.source.vertices[start_index as usize].position;
            let end = self.source.vertices[end_index as usize].position;
            let fraction = normal.dot(start) / normal.dot(start - end);
            let mut bary = DVec3::ZERO;
            for k in 0..3 {
                bary[k] = if self.triangle[k] == start_index {
                    1.0 - fraction
                } else if self.triangle[k] == end_index {
                    fraction
                } else {
                    0.0
                };
            }
            (
                VertexKey::Edge(start_index, end_index, plane),
                start.lerp(end, fraction),
                bary,
            )
        } else {
            let other = active.iter().copied().find(|&other| {
                other != plane
                    && [a, b].iter().all(|point| {
                        self.partition.planes[other]
                            .dot(point.vertex.position)
                            .abs()
                            <= point.vertex.position.length().max(1.0) * PLANE_EPSILON * 8.0
                    })
            });
            let corner = other
                .and_then(|other| self.partition.corners.get(&ordered(plane, other)))
                .and_then(|rays| {
                    rays.iter().copied().max_by(|&left, &right| {
                        self.partition.rays[left]
                            .dot(candidate)
                            .total_cmp(&self.partition.rays[right].dot(candidate))
                    })
                });
            if let Some(corner) = corner {
                let [source_start, source_middle, source_end] = self
                    .triangle
                    .map(|i| self.source.vertices[i as usize].position);
                let face_normal = (source_middle - source_start).cross(source_end - source_start);
                let ray = self.partition.rays[corner];
                let position = ray * (face_normal.dot(source_start) / face_normal.dot(ray));
                (
                    VertexKey::Corner(self.triangle_id, corner),
                    position,
                    barycentric_at(position, source_start, source_middle, source_end),
                )
            } else {
                // Every non-source edge must lie on a previously clipped cone
                // plane. Reject unresolvable geometry instead of moving a seam.
                self.failed = true;
                return ClipVertex {
                    key: a.key,
                    vertex: a.vertex,
                    barycentric: a.barycentric,
                };
            }
        };
        if !position.is_finite() || !barycentric.is_finite() {
            self.failed = true;
            return a;
        }
        let vertex = *self.cache.entry(key).or_insert_with(|| {
            interpolated_vertex(self.source, self.triangle, position, barycentric)
        });
        ClipVertex {
            key,
            vertex,
            barycentric,
        }
    }

    fn clip(&mut self, polygon: &[ClipVertex], side: Side, active: &[usize]) -> Vec<ClipVertex> {
        let mut output = Vec::with_capacity(polygon.len() + 1);
        let normal = self.partition.planes[side.plane] * side.sign;
        let mut previous = *polygon.last().expect("nonempty clipped polygon");
        let distance = |point: ClipVertex| {
            let value = normal.dot(point.vertex.position);
            let epsilon = point.vertex.position.length().max(1.0) * PLANE_EPSILON;
            if value.abs() <= epsilon { 0.0 } else { value }
        };
        let mut previous_distance = distance(previous);
        for &current in polygon {
            let current_distance = distance(current);
            if (previous_distance < 0.0 && current_distance > 0.0)
                || (previous_distance > 0.0 && current_distance < 0.0)
            {
                output.push(self.intersection(previous, current, side.plane, active));
            }
            if current_distance >= 0.0 {
                let mut current = current;
                current.vertex.protected |= current_distance == 0.0;
                output.push(current);
            }
            previous = current;
            previous_distance = current_distance;
        }
        output.dedup_by_key(|point| point.key);
        if output.len() > 1 && output.first().map(|p| p.key) == output.last().map(|p| p.key) {
            output.pop();
        }
        output
    }

    fn visit(
        &mut self,
        cell: &Cell,
        polygon: &[ClipVertex],
        active: &mut Vec<usize>,
        output: &mut [MeshBuilder],
    ) {
        let previous_planes = active.len();
        let mut clipped = Cow::Borrowed(polygon);
        for side in cell.sides {
            if active.contains(&side.plane) {
                continue;
            }
            let normal = self.partition.planes[side.plane] * side.sign;
            let mut inside = false;
            let mut outside = false;
            let mut on_plane = false;
            for point in clipped.iter() {
                let distance = normal.dot(point.vertex.position);
                let epsilon = point.vertex.position.length().max(1.0) * PLANE_EPSILON;
                inside |= distance > epsilon;
                outside |= distance < -epsilon;
                on_plane |= distance.abs() <= epsilon;
            }
            // Assign an exactly radial face to one side of the shared plane.
            // Such faces occur in waterfall curtains and have finite 3D area.
            if !inside && (outside || side.sign < 0.0) {
                active.truncate(previous_planes);
                return;
            }
            if outside {
                clipped = Cow::Owned(self.clip(&clipped, side, active));
            } else if on_plane {
                for point in clipped.to_mut() {
                    if normal.dot(point.vertex.position).abs()
                        <= point.vertex.position.length().max(1.0) * PLANE_EPSILON
                    {
                        point.vertex.protected = true;
                    }
                }
            }
            active.push(side.plane);
            if clipped.len() < 3 {
                break;
            }
        }
        if clipped.len() >= 3 {
            if cell.children.is_empty() {
                output[cell.leaf].append(&clipped);
            } else {
                for child in &cell.children {
                    self.visit(child, &clipped, active, output);
                }
            }
        }
        active.truncate(previous_planes);
    }
}

fn interpolated_vertex(
    source: &ChunkMesh,
    triangle: [u32; 3],
    position: DVec3,
    barycentric: DVec3,
) -> ChunkVertex {
    let vertices = triangle.map(|i| source.vertices[i as usize]);
    let normal = vertices
        .iter()
        .zip(barycentric.to_array())
        .map(|(vertex, w)| vertex.normal * w)
        .sum::<DVec3>()
        .normalize_or_zero();
    #[allow(clippy::cast_possible_truncation)] // Colours are deliberately stored as f32.
    let colour = std::array::from_fn(|channel| {
        vertices
            .iter()
            .zip(barycentric.to_array())
            .map(|(vertex, w)| f64::from(vertex.colour[channel]) * w)
            .sum::<f64>() as f32
    });
    ChunkVertex {
        position,
        normal,
        colour,
        protected: true,
    }
}

fn barycentric_at(point: DVec3, a: DVec3, b: DVec3, c: DVec3) -> DVec3 {
    let ab = b - a;
    let ac = c - a;
    let relative = point - a;
    let aa = ab.dot(ab);
    let bb = ab.dot(ac);
    let cc = ac.dot(ac);
    let inverse = (aa * cc - bb * bb).recip();
    let weight_b = (cc * relative.dot(ab) - bb * relative.dot(ac)) * inverse;
    let weight_c = (aa * relative.dot(ac) - bb * relative.dot(ab)) * inverse;
    DVec3::new(1.0 - weight_b - weight_c, weight_b, weight_c)
}

#[derive(Default)]
struct MeshBuilder {
    mesh: ChunkMesh,
    indices: HashMap<VertexKey, u32>,
}

impl MeshBuilder {
    fn append(&mut self, polygon: &[ClipVertex]) {
        for i in 1..polygon.len() - 1 {
            let face = [polygon[0], polygon[i], polygon[i + 1]];
            let [a, b, c] = face.map(|point| point.vertex.position);
            // Keep arbitrarily small valid faces; an absolute epsilon creates holes.
            if (b - a).cross(c - a).length_squared() == 0.0 {
                continue;
            }
            let indices = face.map(|point| {
                *self.indices.entry(point.key).or_insert_with(|| {
                    let index = u32::try_from(self.mesh.vertices.len())
                        .expect("chunk vertex count fits u32");
                    self.mesh.vertices.push(point.vertex);
                    index
                })
            });
            if indices[0] != indices[1] && indices[1] != indices[2] && indices[2] != indices[0] {
                self.mesh.triangles.push(indices);
            }
        }
    }
}

/// Reusable ownership query for complete detail objects attached to the same
/// cells as cut terrain. Points on a cut choose the first stable leaf.
pub struct CellLocator {
    partition: Partition,
    roots: Vec<Cell>,
}

impl CellLocator {
    /// # Errors
    /// Rejects subdivision levels above six, matching terrain partitioning.
    pub fn new(level: u8) -> Result<Self, PlanetError> {
        if level > MAX_PARTITION_LEVEL {
            return Err(PlanetError::InvalidOptions("chunk level must be at most 6"));
        }
        let (partition, roots) = Partition::new(level);
        Ok(Self { partition, roots })
    }

    #[must_use]
    pub fn owner(&self, point: DVec3) -> Option<u32> {
        fn visit(partition: &Partition, cell: &Cell, point: DVec3) -> Option<u32> {
            if cell
                .sides
                .iter()
                .any(|side| partition.planes[side.plane].dot(point) * side.sign < -PLANE_EPSILON)
            {
                return None;
            }
            if cell.children.is_empty() {
                return u32::try_from(cell.leaf).ok();
            }
            cell.children
                .iter()
                .find_map(|child| visit(partition, child, point))
        }
        if !point.is_finite() || point.length_squared() <= 0.0 {
            return None;
        }
        self.roots
            .iter()
            .find_map(|cell| visit(&self.partition, cell, point.normalize()))
    }
}

/// Cut a mesh into `20 * 4^level` spherical triangular regions.
///
/// Cuts interpolate the original planar triangles without changing their shape.
/// Canonical edge and cone-corner intersections give neighbouring patches exactly
/// identical positions, colours, and normals. Source input is borrowed; the result
/// owns independent render buffers. Empty regions retain their stable chunk IDs.
///
/// # Errors
/// Rejects excessive subdivision, invalid indices, non-finite or zero positions,
/// and degenerate cone intersections that cannot preserve a shared seam.
pub fn partition(source: &ChunkMesh, level: u8) -> Result<Vec<Chunk>, PlanetError> {
    if level > MAX_PARTITION_LEVEL {
        return Err(PlanetError::InvalidOptions("chunk level must be at most 6"));
    }
    if source.vertices.iter().any(|vertex| {
        !vertex.position.is_finite()
            || vertex.position.length_squared() == 0.0
            || !vertex.normal.is_finite()
            || vertex.colour.iter().any(|value| !value.is_finite())
    }) {
        return Err(PlanetError::InvalidOptions(
            "chunk attributes must be finite and positions nonzero",
        ));
    }
    if source
        .triangles
        .iter()
        .flatten()
        .any(|&i| i as usize >= source.vertices.len())
    {
        return Err(PlanetError::InvalidTopology(
            "chunk triangle index out of range",
        ));
    }
    let (partition, roots) = Partition::new(level);
    let mut output: Vec<MeshBuilder> = (0..partition.leaves)
        .map(|_| MeshBuilder::default())
        .collect();
    let mut cache = HashMap::new();
    let mut active = Vec::with_capacity(3 * (usize::from(level) + 1));
    for (triangle_id, &triangle) in source.triangles.iter().enumerate() {
        let polygon: [ClipVertex; 3] = std::array::from_fn(|i| ClipVertex {
            key: VertexKey::Original(triangle[i]),
            vertex: source.vertices[triangle[i] as usize],
            barycentric: match i {
                0 => DVec3::X,
                1 => DVec3::Y,
                _ => DVec3::Z,
            },
        });
        let [a, b, c] = polygon.map(|point| point.vertex.position);
        if (b - a).cross(c - a).length_squared() == 0.0 {
            continue;
        }
        let mut clip = TriangleClip {
            source,
            partition: &partition,
            triangle,
            triangle_id,
            cache: &mut cache,
            failed: false,
        };
        for root in &roots {
            clip.visit(root, &polygon, &mut active, &mut output);
            if clip.failed {
                return Err(PlanetError::InvalidTopology(
                    "unresolvable cone intersection",
                ));
            }
        }
    }
    Ok(output
        .into_iter()
        .zip(0_u32..)
        .map(|(builder, id)| Chunk {
            id,
            bounds: ChunkBounds::from_mesh(&builder.mesh),
            mesh: builder.mesh,
        })
        .collect())
}

/// Radius of a sphere that cannot protrude through any triangle in this mesh.
/// The caller must supply a closed, origin-enclosing planet surface to use this
/// as a horizon occluder. Open river or sea fragments do not enclose the origin.
#[must_use]
pub fn occluder_radius(mesh: &ChunkMesh) -> f64 {
    mesh.triangles
        .iter()
        .map(|face| {
            let [a, b, c] = face.map(|i| mesh.vertices[i as usize].position);
            origin_triangle_distance(a, b, c)
        })
        .reduce(f64::min)
        .unwrap_or(0.0)
        * (1.0 - 1.0e-12)
}

fn origin_triangle_distance(a: DVec3, b: DVec3, c: DVec3) -> f64 {
    let normal = (b - a).cross(c - a);
    let normal_squared = normal.length_squared();
    if normal_squared > 0.0 {
        let projection = normal * (normal.dot(a) / normal_squared);
        let inside = [(a, b), (b, c), (c, a)]
            .into_iter()
            .all(|(start, end)| (end - start).cross(projection - start).dot(normal) >= 0.0);
        if inside {
            return projection.length();
        }
    }
    [(a, b), (b, c), (c, a)]
        .into_iter()
        .map(|(start, end)| {
            let delta = end - start;
            let t = if delta.length_squared() > 0.0 {
                (-start.dot(delta) / delta.length_squared()).clamp(0.0, 1.0)
            } else {
                0.0
            };
            (start + delta * t).length()
        })
        .fold(f64::INFINITY, f64::min)
}
