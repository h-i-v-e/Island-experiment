//! Joint-free stone pieces, with local physical charts shared by every face.
use super::Builder;
use crate::{MaterialSlot, collision::BoxCollider};

/// Surface of an explicitly joined opaque component. Only these boxes may hide
/// one another: callers must keep different materials and visibility groups apart.
/// Exact axis-aligned intervals avoid any positional tolerance or scene inference.
pub(crate) fn joined_boxes(builder: &mut Builder, slot: MaterialSlot, boxes: &[BoxCollider]) {
    for (index, bounds) in boxes.iter().enumerate() {
        if !valid_box(*bounds) {
            continue;
        }
        for axis in 0..3 {
            for positive in [false, true] {
                box_face(builder, slot, boxes, index, axis, positive);
            }
        }
    }
}

fn valid_box(bounds: BoxCollider) -> bool {
    (0..3).all(|axis| {
        bounds.min[axis].is_finite()
            && bounds.max[axis].is_finite()
            && bounds.min[axis] < bounds.max[axis]
    })
}

#[allow(clippy::float_cmp)] // Exact coplanar ownership, never a proximity tolerance.
fn box_face(
    builder: &mut Builder,
    slot: MaterialSlot,
    boxes: &[BoxCollider],
    index: usize,
    axis: usize,
    positive: bool,
) {
    let bounds = boxes[index];
    let plane = if positive {
        bounds.max[axis]
    } else {
        bounds.min[axis]
    };
    let u = (axis + 1) % 3;
    let v = (axis + 2) % 3;
    let mut rectangles = vec![Rect {
        min: [bounds.min[u], bounds.min[v]],
        max: [bounds.max[u], bounds.max[v]],
    }];
    for (other_index, other) in boxes.iter().enumerate() {
        // Real solid volume beyond this face always hides it. At an exactly
        // shared exterior plane, the first input box owns the overlapping area.
        let beyond = if positive {
            other.min[axis] <= plane && other.max[axis] > plane
        } else {
            other.min[axis] < plane && other.max[axis] >= plane
        };
        let owned_boundary = other_index < index
            && if positive {
                other.max[axis] == plane
            } else {
                other.min[axis] == plane
            };
        if other_index == index || !valid_box(*other) || !(beyond || owned_boundary) {
            continue;
        }
        let cover = Rect {
            min: [other.min[u], other.min[v]],
            max: [other.max[u], other.max[v]],
        };
        if !rectangles.iter().any(|r| r.overlaps(cover)) {
            continue;
        }
        rectangles = rectangles
            .into_iter()
            .flat_map(|r| r.subtract(cover))
            .collect();
        if rectangles.is_empty() {
            return;
        }
    }
    for rectangle in rectangles {
        let [a, b] = rectangle.min;
        let [c, d] = rectangle.max;
        let mut points = [[a, b], [c, b], [c, d], [a, d]].map(|uv| {
            let mut point = [0.; 3];
            point[axis] = plane;
            point[u] = uv[0];
            point[v] = uv[1];
            point
        });
        if !positive {
            points.reverse();
        }
        // Keep the source box's chart origin when a face is split. Callers may
        // subsequently replace this with component-wide physical coordinates.
        let uv = points.map(|p| source_uv(bounds, p, axis, positive));
        builder.quad(slot, points, uv);
    }
}

fn source_uv(bounds: BoxCollider, p: [f32; 3], axis: usize, positive: bool) -> [f32; 2] {
    let uv = match (axis, positive) {
        (0, true) => [p[1] - bounds.min[1], p[2] - bounds.min[2]],
        (0, false) => [bounds.max[1] - p[1], p[2] - bounds.min[2]],
        (1, true) => [bounds.max[0] - p[0], p[2] - bounds.min[2]],
        (1, false) => [p[0] - bounds.min[0], p[2] - bounds.min[2]],
        (2, true) => [p[0] - bounds.min[0], p[1] - bounds.min[1]],
        _ => [p[0] - bounds.min[0], bounds.max[1] - p[1]],
    };
    uv.map(|coordinate| coordinate * 0.5)
}

#[derive(Clone, Copy)]
struct Rect {
    min: [f32; 2],
    max: [f32; 2],
}

impl Rect {
    fn overlaps(self, other: Self) -> bool {
        (0..2).all(|i| self.min[i] < other.max[i] && self.max[i] > other.min[i])
    }

    fn subtract(self, other: Self) -> impl Iterator<Item = Self> {
        let min = std::array::from_fn::<_, 2, _>(|i| self.min[i].max(other.min[i]));
        let max = std::array::from_fn::<_, 2, _>(|i| self.max[i].min(other.max[i]));
        let overlap = (0..2).all(|i| min[i] < max[i]);
        let strips = if overlap {
            [
                Some(Self {
                    min: self.min,
                    max: [min[0], self.max[1]],
                }),
                Some(Self {
                    min: [max[0], self.min[1]],
                    max: self.max,
                }),
                Some(Self {
                    min: [min[0], self.min[1]],
                    max: [max[0], min[1]],
                }),
                Some(Self {
                    min: [min[0], max[1]],
                    max: [max[0], self.max[1]],
                }),
            ]
        } else {
            [Some(self), None, None, None]
        };
        strips
            .into_iter()
            .flatten()
            .filter(|r| (0..2).all(|i| r.min[i] < r.max[i]))
    }
}

pub(super) fn stone_box(builder: &mut Builder, slot: MaterialSlot, bounds: BoxCollider) {
    let start = builder.mesh(slot).positions.len();
    builder.cube(slot, bounds.min, bounds.max);
    let mesh = builder.mesh(slot);
    for ((point, normal), uv) in mesh.positions[start..]
        .iter()
        .zip(&mesh.normals[start..])
        .zip(&mut mesh.uvs[start..])
    {
        *uv = crate::units::box_uv(bounds, *point, *normal);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::BuildingMesh;

    fn surface(boxes: &[BoxCollider]) -> BuildingMesh {
        let mut builder = Builder::default();
        joined_boxes(&mut builder, MaterialSlot::Ceiling, boxes);
        builder.finish().parts.remove(0).mesh
    }

    fn area(mesh: &BuildingMesh) -> f32 {
        mesh.positions
            .chunks_exact(4)
            .map(|p| super::super::distance(p[0], p[1]) * super::super::distance(p[1], p[2]))
            .sum()
    }

    #[test]
    #[allow(clippy::float_cmp)] // Match exact authored cube vertices across face ordering.
    fn uncut_faces_keep_the_original_cube_uv_orientation_on_every_axis() {
        let bounds = BoxCollider {
            min: [-2., -3., -4.],
            max: [5., 6., 7.],
        };
        let joined = surface(&[bounds]);
        let mut builder = Builder::default();
        builder.cube(MaterialSlot::Ceiling, bounds.min, bounds.max);
        let cube = builder.finish().parts.remove(0).mesh;
        for ((point, normal), uv) in joined
            .positions
            .iter()
            .zip(&joined.normals)
            .zip(&joined.uvs)
        {
            let index = cube
                .positions
                .iter()
                .zip(&cube.normals)
                .position(|(p, n)| p == point && n == normal)
                .unwrap();
            assert_eq!(*uv, cube.uvs[index]);
        }
    }

    #[test]
    fn joined_box_interfaces_disappear_but_outside_and_bottom_faces_remain() {
        let boxes = [
            BoxCollider {
                min: [0., 0., 0.],
                max: [1., 1., 2.],
            },
            BoxCollider {
                min: [1., 0., 0.],
                max: [2., 1., 1.],
            },
        ];
        let mesh = surface(&boxes);
        assert_eq!(mesh.triangle_count(), 22);
        assert!((area(&mesh) - 14.).abs() < 0.00001);
        // The tall box's exposed upper riser survives; only the covered
        // lower section and the shorter box's whole uphill face disappear.
        for (points, normals) in mesh
            .positions
            .chunks_exact(4)
            .zip(mesh.normals.chunks_exact(4))
        {
            if normals[0][0] > 0.5 && points.iter().all(|p| (p[0] - 1.).abs() < 0.00001) {
                assert!(points.iter().all(|p| p[2] >= 1.));
            }
        }
        assert!(mesh.normals.iter().any(|n| n[2] < -0.5));
    }

    #[test]
    fn ceiling_cutout_retains_both_sides_and_inner_boundary() {
        let mut boxes = Vec::new();
        for x in 0_u16..3 {
            for y in 0_u16..3 {
                if x == 1 && y == 1 {
                    continue;
                }
                boxes.push(BoxCollider {
                    min: [f32::from(x), f32::from(y), 0.],
                    max: [f32::from(x + 1), f32::from(y + 1), 0.1],
                });
            }
        }
        let mesh = surface(&boxes);
        assert_eq!(mesh.triangle_count(), 64);
        assert!((area(&mesh) - 17.6).abs() < 0.00001);
        assert_eq!(mesh.normals.iter().filter(|n| n[2] > 0.5).count(), 32);
        assert_eq!(mesh.normals.iter().filter(|n| n[2] < -0.5).count(), 32);
    }

    #[test]
    fn disjoint_boundaries_survive_and_first_box_owns_coincident_boundaries() {
        let unit = BoxCollider {
            min: [0.; 3],
            max: [1.; 3],
        };
        let disjoint = BoxCollider {
            min: [2., 0., 0.],
            max: [3., 1., 1.],
        };
        assert_eq!(surface(&[unit, disjoint]).triangle_count(), 24);
        assert_eq!(surface(&[unit, unit]).triangle_count(), 12);
    }

    #[test]
    fn partial_coplanar_overlap_has_one_owner_and_thin_exposed_strips_keep_uv_phase() {
        let boxes = [
            BoxCollider {
                min: [0.; 3],
                max: [1.; 3],
            },
            BoxCollider {
                min: [0.5, 0., 0.],
                max: [1.000_001, 1., 1.],
            },
        ];
        let mesh = surface(&boxes);
        let delta = boxes[1].max[0] - 1.;
        assert!((area(&mesh) - (6. + delta * 4.)).abs() < 0.00001);
        let mut found_thin = false;
        for ((points, normals), uvs) in mesh
            .positions
            .chunks_exact(4)
            .zip(mesh.normals.chunks_exact(4))
            .zip(mesh.uvs.chunks_exact(4))
        {
            if points.iter().all(|p| p[0] >= 1.) && normals[0][2] > 0.5 {
                found_thin = true;
                for (point, uv) in points.iter().zip(uvs) {
                    assert!((uv[0] - (point[0] - 0.5) * 0.5).abs() < 0.000_000_1);
                    assert!((uv[1] - point[1] * 0.5).abs() < 0.000_000_1);
                }
            }
        }
        assert!(found_thin, "sub-micrometre strips are not tolerance-culled");
    }
}
