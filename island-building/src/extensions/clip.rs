//! Convex subtraction preserves source winding, UVs and interpolated normals.
use crate::{BuildingMesh, collision::BoxCollider};

const EPSILON: f32 = 0.00001;

#[derive(Clone, Copy)]
pub(super) struct Plane {
    pub normal: [f32; 3],
    pub distance: f32,
}

pub(super) struct Volume {
    pub planes: Vec<Plane>,
    pub bounds: BoxCollider,
    directional_boundary: bool,
}

impl Volume {
    pub fn bounds(bounds: BoxCollider) -> Self {
        let mut planes = Vec::with_capacity(6);
        for axis in 0..3 {
            for sign in [-1., 1.] {
                let mut normal = [0.; 3];
                normal[axis] = sign;
                planes.push(Plane {
                    normal,
                    distance: if sign < 0. {
                        -bounds.min[axis]
                    } else {
                        bounds.max[axis]
                    },
                });
            }
        }
        Self {
            planes,
            bounds,
            directional_boundary: false,
        }
    }

    /// Pair the overlapping portions of an upper roof face and a soffit.
    pub fn intersect(&self, underside: &Self) -> Option<Self> {
        let bounds = BoxCollider {
            min: std::array::from_fn(|i| self.bounds.min[i].max(underside.bounds.min[i])),
            max: std::array::from_fn(|i| self.bounds.max[i].min(underside.bounds.max[i])),
        };
        if (0..2).any(|i| bounds.min[i] >= bounds.max[i]) || bounds.min[2] > bounds.max[2] {
            return None;
        }
        Some(Self {
            planes: self
                .planes
                .iter()
                .chain(&underside.planes)
                .copied()
                .collect(),
            bounds,
            directional_boundary: true,
        })
    }

    pub fn lower_at(&self, point: [f32; 3]) -> Option<f32> {
        let mut low = self.bounds.min[2];
        let mut high = self.bounds.max[2];
        for plane in &self.planes {
            let limit = plane.distance - plane.normal[0] * point[0] - plane.normal[1] * point[1];
            let slope = plane.normal[2];
            if slope.abs() < EPSILON {
                if limit < -EPSILON {
                    return None;
                }
            } else if slope < 0. {
                low = low.max(limit / slope);
            } else {
                high = high.min(limit / slope);
            }
        }
        (low <= high + EPSILON).then_some(low)
    }
}

#[derive(Clone, Copy)]
pub(super) struct Vertex {
    pub point: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
}

impl Vertex {
    fn lerp(self, other: Self, amount: f32) -> Self {
        Self {
            point: std::array::from_fn(|i| {
                self.point[i] + (other.point[i] - self.point[i]) * amount
            }),
            normal: std::array::from_fn(|i| {
                self.normal[i] + (other.normal[i] - self.normal[i]) * amount
            }),
            uv: std::array::from_fn(|i| self.uv[i] + (other.uv[i] - self.uv[i]) * amount),
        }
    }
}

fn distance(plane: Plane, point: [f32; 3]) -> f32 {
    plane
        .normal
        .iter()
        .zip(point)
        .map(|(n, p)| n * p)
        .sum::<f32>()
        - plane.distance
}

pub(super) fn split(polygon: &[Vertex], plane: Plane) -> (Vec<Vertex>, Vec<Vertex>) {
    let mut inside = Vec::new();
    let mut outside = Vec::new();
    let mut has_outside = false;
    for index in 0..polygon.len() {
        let a = polygon[index];
        let b = polygon[(index + 1) % polygon.len()];
        let da = distance(plane, a.point);
        let db = distance(plane, b.point);
        let sa = if da.abs() <= EPSILON { 0. } else { da };
        let sb = if db.abs() <= EPSILON { 0. } else { db };
        if sa <= 0. {
            inside.push(a);
        }
        if sa >= 0. {
            outside.push(a);
        }
        has_outside |= sa > 0.;
        if (sa < 0. && sb > 0.) || (sa > 0. && sb < 0.) {
            let crossing = a.lerp(b, da / (da - db));
            inside.push(crossing);
            outside.push(crossing);
        }
    }
    // Boundary vertices belong to either adjoining fragment. A wholly
    // coplanar polygon belongs only to the inside, so subtraction removes it.
    if !has_outside {
        outside.clear();
    }
    (inside, outside)
}

pub(super) fn subtract(
    polygon: Vec<Vertex>,
    volume: &Volume,
    keep_boundary: bool,
) -> Vec<Vec<Vertex>> {
    if (0..3).any(|axis| {
        polygon
            .iter()
            .all(|v| v.point[axis] < volume.bounds.min[axis] - EPSILON)
            || polygon
                .iter()
                .all(|v| v.point[axis] > volume.bounds.max[axis] + EPSILON)
    }) {
        return vec![polygon];
    }
    if volume
        .planes
        .iter()
        .any(|&p| polygon.iter().all(|v| distance(p, v.point) > EPSILON))
        || (keep_boundary
            && volume.planes.iter().any(|&p| {
                polygon.iter().all(|v| distance(p, v.point).abs() < EPSILON)
                    && (!volume.directional_boundary
                        || p.normal
                            .iter()
                            .zip(polygon[0].normal)
                            .map(|(a, b)| a * b)
                            .sum::<f32>()
                            > 0.001)
            }))
    {
        return vec![polygon];
    }
    let mut remaining = polygon;
    let mut result = Vec::new();
    for &plane in &volume.planes {
        if remaining.len() < 3 {
            break;
        }
        let (inside, outside) = split(&remaining, plane);
        if outside.len() >= 3 {
            result.push(outside);
        }
        remaining = inside;
    }
    result
}

pub(super) fn roof_volume(
    points: [[f32; 3]; 3],
    bottom: f32,
    restriction: Option<BoxCollider>,
) -> Option<Volume> {
    surface_volume(points, bottom, 1., restriction)
}

pub(super) fn soffit_volume(points: [[f32; 3]; 3], top: f32) -> Option<Volume> {
    surface_volume(points, top, -1., None)
}

fn surface_volume(
    points: [[f32; 3]; 3],
    limit: f32,
    up: f32,
    restriction: Option<BoxCollider>,
) -> Option<Volume> {
    let a = std::array::from_fn::<_, 3, _>(|i| points[1][i] - points[0][i]);
    let b = std::array::from_fn::<_, 3, _>(|i| points[2][i] - points[0][i]);
    let normal = [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ];
    let length = normal.iter().map(|n| n * n).sum::<f32>().sqrt();
    if normal[2] * up <= length * 0.01 {
        return None;
    }
    let normal = normal.map(|n| n / length);
    let mut bounds = BoxCollider {
        min: std::array::from_fn(|i| points.iter().map(|p| p[i]).fold(f32::INFINITY, f32::min)),
        max: std::array::from_fn(|i| {
            points
                .iter()
                .map(|p| p[i])
                .fold(f32::NEG_INFINITY, f32::max)
        }),
    };
    if (up > 0. && bounds.max[2] < limit) || (up < 0. && bounds.min[2] > limit) {
        return None;
    }
    if up > 0. {
        bounds.min[2] = limit;
    } else {
        bounds.max[2] = limit;
    }
    let mut planes = vec![
        Plane {
            normal,
            distance: normal.iter().zip(points[0]).map(|(a, b)| a * b).sum(),
        },
        Plane {
            normal: [0., 0., -up],
            distance: -up * limit,
        },
    ];
    for index in 0..3 {
        let a = points[index];
        let b = points[(index + 1) % 3];
        let n = [(b[1] - a[1]) * up, (a[0] - b[0]) * up, 0.];
        let length = n[0].hypot(n[1]);
        let n = n.map(|v| v / length);
        planes.push(Plane {
            normal: n,
            distance: n[0] * a[0] + n[1] * a[1],
        });
    }
    if let Some(restriction) = restriction {
        planes.extend(Volume::bounds(restriction).planes.into_iter().take(4));
        for axis in 0..2 {
            bounds.min[axis] = bounds.min[axis].max(restriction.min[axis]);
            bounds.max[axis] = bounds.max[axis].min(restriction.max[axis]);
        }
        if (0..2).any(|i| bounds.max[i] <= bounds.min[i]) {
            return None;
        }
    }
    Some(Volume {
        planes,
        bounds,
        directional_boundary: true,
    })
}

pub(super) fn append(mesh: &mut BuildingMesh, polygon: &[Vertex]) {
    for index in 1..polygon.len().saturating_sub(1) {
        for vertex in [polygon[0], polygon[index], polygon[index + 1]] {
            mesh.indices
                .push(u32::try_from(mesh.positions.len()).expect("bounded extension mesh"));
            mesh.positions.push(vertex.point);
            let length = vertex
                .normal
                .iter()
                .map(|n| n * n)
                .sum::<f32>()
                .sqrt()
                .max(f32::EPSILON);
            mesh.normals.push(vertex.normal.map(|n| n / length));
            mesh.uvs.push(vertex.uv);
        }
    }
}

pub(super) fn box_difference(shape: BoxCollider, cut: BoxCollider) -> Vec<BoxCollider> {
    let intersection = BoxCollider {
        min: std::array::from_fn(|i| shape.min[i].max(cut.min[i])),
        max: std::array::from_fn(|i| shape.max[i].min(cut.max[i])),
    };
    if (0..3).any(|i| intersection.max[i] - intersection.min[i] <= EPSILON) {
        return vec![shape];
    }
    let mut centre = shape;
    let mut result = Vec::new();
    for axis in 0..3 {
        if centre.min[axis] < intersection.min[axis] - EPSILON {
            let mut part = centre;
            part.max[axis] = intersection.min[axis];
            result.push(part);
        }
        if centre.max[axis] > intersection.max[axis] + EPSILON {
            let mut part = centre;
            part.min[axis] = intersection.max[axis];
            result.push(part);
        }
        centre.min[axis] = intersection.min[axis];
        centre.max[axis] = intersection.max[axis];
    }
    result
}
