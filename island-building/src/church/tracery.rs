//! Two complete pointed lights and a tangent circular eye share one pattern.
use super::{plan::Wall, windows};
use crate::{MaterialSlot, OpeningDimensions, geometry::Builder};

#[derive(Clone, Copy)]
pub(super) struct Pattern {
    pub half: f32,
    pub sill: f32,
    pub spring: f32,
    pub bar: f32,
}
impl Pattern {
    pub fn new(size: OpeningDimensions) -> Self {
        let half = size.width * 0.5;
        Self {
            half,
            sill: size.sill,
            spring: size.sill + size.height - 3_f32.sqrt() * half,
            bar: (size.width * 0.04).clamp(0.04, 0.12),
        }
    }
    pub fn eye(self) -> ([f32; 2], f32) {
        (
            [0., self.spring + 5_f32.sqrt() * self.half * 0.5],
            self.half * 0.5,
        )
    }
    /// Samples the same radial stone bands used by the geometric tracery.
    pub fn stone_at(self, x: f32, z: f32, padding: f32) -> bool {
        let edge = self.bar * 0.5 + padding;
        if x.abs() <= edge && z >= self.sill && z <= self.spring {
            return true;
        }
        let (eye, radius) = self.eye();
        if (x.hypot(z - eye[1]) - radius).abs() <= edge {
            return true;
        }
        if z < self.spring || x.abs() > self.half + edge {
            return false;
        }
        [-self.half * 0.5, self.half * 0.5]
            .into_iter()
            .any(|centre| {
                if (x - centre).abs() > self.half * 0.5 + edge {
                    return false;
                }
                let circle = centre
                    + if x < centre {
                        self.half * 0.5
                    } else {
                        -self.half * 0.5
                    };
                ((x - circle).hypot(z - self.spring) - self.half).abs() <= edge
            })
    }
}

pub(super) fn generate(
    b: &mut Builder,
    wall: Wall,
    size: OpeningDimensions,
    depth: [f32; 2],
    level: u8,
) {
    let pattern = Pattern::new(size);
    let mid = wall.length() * 0.5;
    windows::prism(
        b,
        wall,
        [mid - pattern.bar * 0.5, mid + pattern.bar * 0.5],
        [size.sill; 2],
        [pattern.spring; 2],
        depth,
        MaterialSlot::Trim,
    );
    let segments = if level == 0 { 12_u16 } else { 4 };
    for centre in [-pattern.half * 0.5, pattern.half * 0.5] {
        for left in [true, false] {
            for i in 0..segments {
                let ts = [
                    f32::from(i) / f32::from(segments),
                    f32::from(i + 1) / f32::from(segments),
                ];
                let contours =
                    [-0.5, 0.5].map(|side| ts.map(|t| head_point(pattern, centre, left, t, side)));
                band(b, wall, mid, contours[0], contours[1], depth);
            }
        }
    }
    let circle_segments = if level == 0 { 48_u16 } else { 16 };
    circle(
        b,
        wall,
        mid,
        pattern,
        [depth[0] - 0.004, depth[1] + 0.004],
        circle_segments,
    );
}

fn head_point(p: Pattern, centre: f32, left: bool, t: f32, side: f32) -> [f32; 2] {
    let radius = p.half + side * p.bar;
    let junction = (p.half * 0.5 / radius).acos();
    let circle = centre + if left { p.half * 0.5 } else { -p.half * 0.5 };
    let angle = if left {
        std::f32::consts::PI - junction * t
    } else {
        junction * (1. - t)
    };
    // Both branches use the exact same miter point at their pointed junction.
    if (left && t >= 1.) || (!left && t <= 0.) {
        return [
            centre,
            p.spring + (radius * radius - (p.half * 0.5).powi(2)).sqrt(),
        ];
    }
    [
        circle + radius * angle.cos(),
        p.spring + radius * angle.sin(),
    ]
}
fn circle(b: &mut Builder, wall: Wall, mid: f32, p: Pattern, depth: [f32; 2], segments: u16) {
    let (centre, radius) = p.eye();
    for i in 0..segments {
        let angles = [f32::from(i), f32::from(i + 1)]
            .map(|i| i / f32::from(segments) * std::f32::consts::TAU);
        let contours = [-0.5, 0.5].map(|side| {
            angles.map(|angle| {
                let r = radius + side * p.bar;
                [centre[0] + r * angle.cos(), centre[1] + r * angle.sin()]
            })
        });
        band(b, wall, mid, contours[0], contours[1], depth);
    }
}
fn band(
    b: &mut Builder,
    wall: Wall,
    mid: f32,
    inner: [[f32; 2]; 2],
    outer: [[f32; 2]; 2],
    depth: [f32; 2],
) {
    let front =
        [inner[0], inner[1], outer[1], outer[0]].map(|p| wall.point(mid + p[0], p[1], depth[0]));
    let back =
        [inner[0], inner[1], outer[1], outer[0]].map(|p| wall.point(mid + p[0], p[1], depth[1]));
    // Radial contours may run clockwise or anticlockwise; orient their front face.
    let area = (inner[1][0] - inner[0][0]) * (outer[0][1] - inner[0][1])
        - (inner[1][1] - inner[0][1]) * (outer[0][0] - inner[0][0]);
    let mut front = front;
    let mut back = back;
    if area < 0. {
        front.reverse();
        back.reverse();
    }
    windows::surface(b, MaterialSlot::Trim, front);
    windows::surface(b, MaterialSlot::Trim, [back[1], back[0], back[3], back[2]]);
    windows::surface(
        b,
        MaterialSlot::Trim,
        [front[1], front[0], back[0], back[1]],
    );
    windows::surface(
        b,
        MaterialSlot::Trim,
        [front[3], front[2], back[2], back[3]],
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{BuildingMesh, BuildingRecipe, church::plan::Plan};

    fn ray_hits(mesh: &BuildingMesh, x: f32, z: f32) -> bool {
        mesh.indices.chunks_exact(3).any(|ids| {
            let [a, b, c] = [ids[0], ids[1], ids[2]].map(|i| mesh.positions[i as usize]);
            let determinant = (b[0] - a[0]) * (c[2] - a[2]) - (c[0] - a[0]) * (b[2] - a[2]);
            if determinant.abs() < 0.000_001 {
                return false;
            }
            let u = ((x - a[0]) * (c[2] - a[2]) - (z - a[2]) * (c[0] - a[0])) / determinant;
            let v = ((b[0] - a[0]) * (z - a[2]) - (b[2] - a[2]) * (x - a[0])) / determinant;
            u >= -0.00001 && v >= -0.00001 && u + v <= 1.00001
        })
    }

    #[test]
    fn circle_meets_the_outer_arch_and_both_complete_pointed_heads() {
        for width in [1.0, 2.25, 2.7] {
            let p = Pattern::new(OpeningDimensions {
                width,
                height: 6.,
                sill: 1.65,
            });
            let (eye, r) = p.eye();
            let distance = p.half.hypot(eye[1] - p.spring);
            assert!((distance - (p.half + r)).abs() < 0.00001);
            assert!((distance + r - p.half * 2.).abs() < 0.00001);
            for centre in [-p.half * 0.5, p.half * 0.5] {
                let left = head_point(p, centre, true, 1., 0.5);
                let right = head_point(p, centre, false, 0., 0.5);
                assert!(
                    (left[0] - right[0]).abs() < 0.000_001
                        && (left[1] - right[1]).abs() < 0.000_001
                );
            }
        }
    }

    #[test]
    fn mesh_and_bake_agree_on_two_open_lights_central_mullion_and_clear_circle() {
        let size = OpeningDimensions {
            width: 2.25,
            height: 6.,
            sill: 1.65,
        };
        let p = Pattern::new(size);
        let wall = Wall {
            a: [-2., 0.],
            b: [2., 0.],
        };
        let (eye, radius) = p.eye();
        let probes = [
            (0., p.sill + 0.5, true),
            (-p.half * 0.5, p.spring - 0.5, false),
            (p.half * 0.5, p.spring - 0.5, false),
            (0., eye[1], false),
            (radius, eye[1], true),
            (0., eye[1] - radius, true),
            (-p.half * 0.5, p.spring + 3_f32.sqrt() * p.half * 0.5, true),
            (p.half * 0.5, p.spring + 3_f32.sqrt() * p.half * 0.5, true),
            (-p.half / 3., p.sill + 0.5, false),
        ];
        for level in [0, 1] {
            let mut b = Builder::default();
            generate(&mut b, wall, size, [0.1, 0.3], level);
            let mesh = b.finish().parts.remove(0).mesh;
            for (x, z, stone) in probes {
                assert_eq!(p.stone_at(x, z, 0.), stone);
                assert_eq!(ray_hits(&mesh, x, z), stone, "LOD{level}, x{x} z{z}");
            }
        }
    }

    #[test]
    fn changing_tower_height_keeps_the_eye_round_and_the_window_inside_its_wall() {
        let r = BuildingRecipe::gothic_church();
        let plan = Plan::new(&r);
        for height in [3., 7., 10., 16.] {
            let section = plan.at_height(height);
            let p = Pattern::new(section.window);
            assert!(section.window.sill + section.window.height < height);
            let mut b = Builder::default();
            let wall = Wall {
                a: [-2., 0.],
                b: [2., 0.],
            };
            circle(&mut b, wall, 2., p, [0.1, 0.3], 48);
            let mesh = b.finish().parts.remove(0).mesh;
            let extent = |axis| {
                let lo = mesh
                    .positions
                    .iter()
                    .map(|p| p[axis])
                    .fold(f32::INFINITY, f32::min);
                let hi = mesh
                    .positions
                    .iter()
                    .map(|p| p[axis])
                    .fold(f32::NEG_INFINITY, f32::max);
                hi - lo
            };
            assert!((extent(0) - extent(2)).abs() < 0.00001);
            for normal in mesh.normals {
                assert!(normal.iter().all(|v| v.is_finite()));
            }
        }
    }
}
