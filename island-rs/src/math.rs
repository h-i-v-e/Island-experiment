use glam::{Vec2, Vec3};

#[derive(Clone, Copy, Debug, PartialEq)]
#[repr(C)]
pub struct BoundingBox {
    pub min: Vec3,
    pub max: Vec3,
}

impl BoundingBox {
    #[must_use]
    pub const fn new(min: Vec3, max: Vec3) -> Self {
        Self { min, max }
    }

    #[must_use]
    pub fn contains_xy(self, point: Vec2) -> bool {
        point.x >= self.min.x
            && point.x <= self.max.x
            && point.y >= self.min.y
            && point.y <= self.max.y
    }
}

impl Default for BoundingBox {
    fn default() -> Self {
        Self::new(Vec3::new(0.0, 0.0, f32::MIN), Vec3::new(1.0, 1.0, f32::MAX))
    }
}
