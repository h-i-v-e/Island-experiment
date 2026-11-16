//! Concentric circular stone bands, with the same two-centred profile as the windows.
use super::{atlas, plan::Wall, windows};
use crate::{MaterialSlot, collision::BoxCollider, geometry::Builder};

const THICKNESS: f32 = 0.48;

pub(super) struct Arch {
    pub span: f32,
    pub pier_height: f32,
    centre: [f32; 2],
    radius: f32,
}
impl Arch {
    pub fn new(reach: f32, wall_height: f32) -> Self {
        let span = reach + 0.18;
        let crown = wall_height - 1.;
        let mut radius = span * 2.;
        let mut spring = crown - 3_f32.sqrt() * span;
        if spring < 0.6 {
            // Fit a circular segment into unusually short, wide recipes without
            // sending its foot below the floor or its wall joint above the eaves.
            spring = wall_height * 0.52 * 0.63;
            radius = (span * span + (crown - spring).powi(2)) / (2. * span);
        }
        Self {
            span,
            pier_height: (wall_height * 0.52).max(spring + 0.25),
            centre: [span - radius, spring],
            radius,
        }
    }
    fn point(&self, t: f32, offset: f32) -> [f32; 2] {
        let r = self.radius + offset;
        let angle = (-self.centre[0] / r).acos() * (1. - t);
        [
            if t == 0. {
                0.
            } else {
                self.centre[0] + r * angle.cos()
            },
            self.centre[1] + r * angle.sin(),
        ]
    }
    pub fn generate(
        &self,
        b: &mut Builder,
        wall: Wall,
        level: u8,
        colliders: &mut Vec<BoxCollider>,
    ) {
        let segments = match level {
            0 => 18_u16,
            1 => 8,
            _ => 3,
        };
        for i in 0..segments {
            let ts = [f32::from(i), f32::from(i + 1)].map(|i| i / f32::from(segments));
            let inner = ts.map(|t| self.point(t, -THICKNESS * 0.5));
            let outer = ts.map(|t| self.point(t, THICKNESS * 0.5));
            let front =
                [inner[0], inner[1], outer[1], outer[0]].map(|p| wall.point(p[0], p[1], -0.23));
            let back =
                [inner[0], inner[1], outer[1], outer[0]].map(|p| wall.point(p[0], p[1], 0.23));
            for face in [
                front,
                [back[1], back[0], back[3], back[2]],
                [front[1], front[0], back[0], back[1]],
                [front[3], front[2], back[2], back[3]],
            ] {
                surface(b, level, face);
            }
            if i == 0 {
                surface(b, level, [front[0], front[3], back[3], back[0]]);
            }
            if i + 1 == segments {
                surface(b, level, [front[2], front[1], back[1], back[2]]);
            }
            if level == 0 {
                colliders.push(BoxCollider {
                    min: std::array::from_fn(|axis| {
                        front
                            .iter()
                            .chain(&back)
                            .map(|p| p[axis])
                            .fold(f32::INFINITY, f32::min)
                    }),
                    max: std::array::from_fn(|axis| {
                        front
                            .iter()
                            .chain(&back)
                            .map(|p| p[axis])
                            .fold(f32::NEG_INFINITY, f32::max)
                    }),
                });
            }
        }
    }
}
fn surface(b: &mut Builder, level: u8, face: [[f32; 3]; 4]) {
    if level == 2 {
        atlas::face(b, 0, face);
    } else {
        windows::surface(b, MaterialSlot::Wall, face);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn circular_band_has_constant_normal_thickness_and_matches_the_window_profile() {
        let arch = Arch::new(3., 10.);
        for i in 0..=36_u16 {
            let t = f32::from(i) / 36.;
            let radii = [-0.5, 0.5].map(|side| {
                let p = arch.point(t, side * THICKNESS);
                (p[0] - arch.centre[0]).hypot(p[1] - arch.centre[1])
            });
            assert!((radii[1] - radii[0] - THICKNESS).abs() < 0.00001);
            let p = arch.point(t, 0.);
            let window = windows::arch(p[0], arch.span, arch.centre[1], 3_f32.sqrt() * arch.span);
            assert!((p[1] - window).abs() < 0.00001);
        }
        assert!((arch.span - 3.18).abs() < 0.00001);
        assert!(arch.point(0., -THICKNESS * 0.5)[0].abs() < 0.00001);
        assert!(arch.point(0., THICKNESS * 0.5)[0].abs() < 0.00001);
    }

    #[test]
    fn short_and_wide_arcs_stay_above_ground_and_meet_their_piers() {
        for reach in [1.5, 3., 5.] {
            for height in [6., 10., 16.] {
                let arch = Arch::new(reach, height);
                for side in [-0.5, 0.5] {
                    let foot = arch.point(1., side * THICKNESS);
                    assert!(foot[1] > 0.);
                    assert!(foot[1] < arch.pier_height);
                    assert!((foot[0] - arch.span).abs() < 0.42);
                    assert!(arch.point(0., side * THICKNESS)[1] < height);
                }
            }
        }
    }
}
