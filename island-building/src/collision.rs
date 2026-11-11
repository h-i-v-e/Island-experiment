//! Allocation-free structural collision in native Z-up building coordinates.
use crate::DoorDescriptor;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub struct BoxCollider {
    pub min: [f32; 3],
    pub max: [f32; 3],
}
impl BoxCollider {
    #[must_use]
    pub fn contains(&self, p: [f32; 3]) -> bool {
        (0..3).all(|i| p[i] >= self.min[i] && p[i] <= self.max[i])
    }
    #[must_use]
    pub fn intersects_sphere(&self, p: [f32; 3], radius: f32) -> bool {
        (0..3)
            .map(|i| (p[i] - p[i].clamp(self.min[i], self.max[i])).powi(2))
            .sum::<f32>()
            < radius * radius
    }
    /// Vertical capsule whose bottom is `feet.z` and whose complete height includes its end caps.
    #[must_use]
    pub fn intersects_capsule(&self, feet: [f32; 3], radius: f32, height: f32) -> bool {
        let horizontal = (0..2)
            .map(|i| (feet[i] - feet[i].clamp(self.min[i], self.max[i])).powi(2))
            .sum::<f32>();
        let low = feet[2] + radius;
        let high = feet[2] + height - radius;
        let gap = (self.min[2] - high).max(low - self.max[2]).max(0.);
        horizontal + gap * gap < radius * radius
    }
    /// Slab ray test; direction need not be normalized. Returns its nonnegative parameter.
    #[must_use]
    pub fn ray_hit(&self, origin: [f32; 3], direction: [f32; 3], limit: f32) -> Option<f32> {
        let mut near = 0_f32;
        let mut far = limit;
        for axis in 0..3 {
            let d = direction[axis];
            if d.abs() < 1e-8 {
                if origin[axis] < self.min[axis] || origin[axis] > self.max[axis] {
                    return None;
                }
            } else {
                let a = (self.min[axis] - origin[axis]) / d;
                let b = (self.max[axis] - origin[axis]) / d;
                near = near.max(a.min(b));
                far = far.min(a.max(b));
                if near > far {
                    return None;
                }
            }
        }
        Some(near)
    }
}
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub struct OrientedBox {
    pub centre: [f32; 3],
    pub half_extents: [f32; 3],
    pub yaw: f32,
}
impl OrientedBox {
    fn local(&self, p: [f32; 3]) -> [f32; 3] {
        let offset = std::array::from_fn::<_, 3, _>(|i| p[i] - self.centre[i]);
        let (sine, cosine) = self.yaw.sin_cos();
        [
            cosine * offset[0] + sine * offset[1],
            -sine * offset[0] + cosine * offset[1],
            offset[2],
        ]
    }
    fn aabb(&self) -> BoxCollider {
        BoxCollider {
            min: self.half_extents.map(|v| -v),
            max: self.half_extents,
        }
    }
    /// Separating-axis test against a structural box. Tangency is not penetration.
    #[must_use]
    pub fn intersects_box(&self, other: &BoxCollider) -> bool {
        let centre = std::array::from_fn::<_, 3, _>(|i| (other.min[i] + other.max[i]) * 0.5);
        let half = std::array::from_fn::<_, 3, _>(|i| (other.max[i] - other.min[i]) * 0.5);
        let delta = std::array::from_fn::<_, 3, _>(|i| self.centre[i] - centre[i]);
        if delta[2].abs() >= self.half_extents[2] + half[2] - 0.001 {
            return false;
        }
        let (s, c) = self.yaw.sin_cos();
        let u = [c, s];
        let v = [-s, c];
        [[1., 0.], [0., 1.], u, v].into_iter().all(|axis| {
            let distance = (delta[0] * axis[0] + delta[1] * axis[1]).abs();
            let box_radius = half[0] * axis[0].abs() + half[1] * axis[1].abs();
            let door_radius = self.half_extents[0] * (axis[0] * u[0] + axis[1] * u[1]).abs()
                + self.half_extents[1] * (axis[0] * v[0] + axis[1] * v[1]).abs();
            distance < box_radius + door_radius - 0.001
        })
    }
    #[must_use]
    pub fn intersects_capsule(&self, feet: [f32; 3], radius: f32, height: f32) -> bool {
        self.aabb()
            .intersects_capsule(self.local(feet), radius, height)
    }
    #[must_use]
    pub fn intersects_sphere(&self, p: [f32; 3], radius: f32) -> bool {
        self.aabb().intersects_sphere(self.local(p), radius)
    }
    #[must_use]
    pub fn ray_hit(&self, origin: [f32; 3], direction: [f32; 3], limit: f32) -> Option<f32> {
        let (s, c) = self.yaw.sin_cos();
        self.aabb().ray_hit(
            self.local(origin),
            [
                c * direction[0] + s * direction[1],
                -s * direction[0] + c * direction[1],
                direction[2],
            ],
            limit,
        )
    }
}
impl DoorDescriptor {
    /// Solid leaf bounds in the same hinge-local frame as its mesh and hardware.
    #[must_use]
    pub fn local_bounds(&self) -> BoxCollider {
        BoxCollider {
            min: [0., self.depth_offset - self.thickness * 0.5, 0.],
            max: [
                self.width,
                self.depth_offset + self.thickness * 0.5,
                self.height,
            ],
        }
    }

    /// `angle` is relative to the closed pose. Positive rotations are native +Z yaw.
    #[must_use]
    pub fn collider_at(&self, angle: f32) -> OrientedBox {
        let yaw = self.closed_angle + angle;
        let (s, c) = yaw.sin_cos();
        OrientedBox {
            centre: [
                self.hinge[0] + c * self.width * 0.5 - s * self.depth_offset,
                self.hinge[1] + s * self.width * 0.5 + c * self.depth_offset,
                self.hinge[2] + self.height * 0.5,
            ],
            half_extents: [self.width * 0.5, self.thickness * 0.5, self.height * 0.5],
            yaw,
        }
    }
}
