use super::BuildingInstance;
use crate::{
    DoorDescriptor,
    render::{bevy_to_native, native_to_bevy},
};
use bevy::prelude::*;

/// Conservative radius conversion also supports non-uniform placement scales.
pub(crate) fn local_radius(root: &GlobalTransform, world_radius: f32) -> f32 {
    let inverse = root.affine().inverse();
    // Inverse rows have reciprocal TRS scale lengths, independent of rotation.
    let rows = inverse.matrix3.transpose();
    world_radius
        * rows
            .x_axis
            .length()
            .max(rows.y_axis.length())
            .max(rows.z_axis.length())
}

pub(crate) fn native_point(root: &GlobalTransform, world: Vec3) -> Vec3 {
    Vec3::from_array(bevy_to_native(local_point(root, world)))
}

pub(crate) fn local_point(root: &GlobalTransform, world: Vec3) -> Vec3 {
    // Subtract the nearby anchor before scaling. Applying the inverse affine's
    // translated matrix directly would cancel two large scaled planet positions.
    root.affine()
        .inverse()
        .transform_vector3(world - root.translation())
}

pub(crate) fn sphere_box(point: Vec3, radius: f32, min: Vec3, max: Vec3) -> bool {
    point.distance_squared(point.clamp(min, max)) < radius * radius
}

pub(crate) fn door_local(point: Vec3, door: &DoorDescriptor, angle: f32) -> Vec3 {
    Quat::from_rotation_z(-door.closed_angle - angle) * (point - Vec3::from_array(door.hinge))
}

pub(crate) fn sphere_door(point: Vec3, radius: f32, door: &DoorDescriptor, angle: f32) -> bool {
    let bounds = door.local_bounds();
    sphere_box(
        door_local(point, door, angle),
        radius,
        Vec3::from_array(bounds.min),
        Vec3::from_array(bounds.max),
    )
}

/// Slab ray query. Direction need not be normalized; parameter units are retained.
pub(crate) fn ray_box(
    origin: Vec3,
    direction: Vec3,
    min: Vec3,
    max: Vec3,
    limit: f32,
) -> Option<f32> {
    let mut near: f32 = 0.0;
    let mut far = limit;
    for axis in 0..3 {
        if direction[axis].abs() < 1e-12 {
            if origin[axis] < min[axis] || origin[axis] > max[axis] {
                return None;
            }
        } else {
            let first = (min[axis] - origin[axis]) / direction[axis];
            let second = (max[axis] - origin[axis]) / direction[axis];
            near = near.max(first.min(second));
            far = far.min(first.max(second));
            if far < near {
                return None;
            }
        }
    }
    Some(near)
}

pub(crate) fn ray_door(
    origin: Vec3,
    direction: Vec3,
    door: &DoorDescriptor,
    angle: f32,
    limit: f32,
) -> Option<f32> {
    let rotation = Quat::from_rotation_z(-door.closed_angle - angle);
    let bounds = door.local_bounds();
    ray_box(
        door_local(origin, door, angle),
        rotation * direction,
        Vec3::from_array(bounds.min),
        Vec3::from_array(bounds.max),
        limit,
    )
}

impl BuildingInstance {
    pub(crate) fn active_colliders(&self) -> impl Iterator<Item = &crate::collision::BoxCollider> {
        self.prototype.source.colliders.iter().chain(
            self.prototype
                .source
                .stair_colliders
                .iter()
                .filter(|_| self.stairs_visible),
        )
    }

    /// Exact structural and moving-leaf capsule query for a root-aligned player.
    #[must_use]
    pub fn blocks_capsule(
        &self,
        root: &GlobalTransform,
        feet: Vec3,
        radius: f32,
        height: f32,
    ) -> bool {
        let feet = native_point(root, feet).to_array();
        let radius = local_radius(root, radius);
        let height = local_radius(root, height);
        let [min, max] = self.prototype.native_collision_bounds;
        let extent = Vec3::new(radius, radius, 0.0);
        let point = Vec3::from_array(feet);
        if !(point + extent + Vec3::Z * height).cmpge(min).all()
            || !(point - extent).cmple(max).all()
        {
            return false;
        }
        self.active_colliders()
            .any(|shape| shape.intersects_capsule(feet, radius, height))
            || self.prototype.source.doors.iter().any(|door| {
                door.collider_at(self.doors.get(&door.id).map_or(0.0, |state| state.angle))
                    .intersects_capsule(feet, radius, height)
            })
    }

    #[must_use]
    pub fn blocks_sphere(
        &self,
        root: &GlobalTransform,
        world_centre: Vec3,
        world_radius: f32,
    ) -> bool {
        let point = native_point(root, world_centre);
        let radius = local_radius(root, world_radius);
        let [min, max] = self.prototype.native_collision_bounds;
        if !point.cmpge(min - Vec3::splat(radius)).all()
            || !point.cmple(max + Vec3::splat(radius)).all()
        {
            return false;
        }
        self.active_colliders()
            .any(|shape| sphere_box(point, radius, shape.min.into(), shape.max.into()))
            || self.prototype.source.doors.iter().any(|door| {
                sphere_door(
                    point,
                    radius,
                    door,
                    self.doors.get(&door.id).map_or(0.0, |state| state.angle),
                )
            })
    }

    /// Returns distance along a normalized world ray, including moving doors.
    #[must_use]
    pub fn raycast(
        &self,
        root: &GlobalTransform,
        origin: Vec3,
        direction: Vec3,
        max_distance: f32,
    ) -> Option<f32> {
        let normalized = direction.try_normalize()?;
        let local_origin = native_point(root, origin);
        let local_direction = Vec3::from_array(bevy_to_native(
            root.affine().inverse().transform_vector3(normalized),
        ));
        let [min, max] = self.prototype.native_collision_bounds;
        ray_box(local_origin, local_direction, min, max, max_distance)?;
        self.active_colliders()
            .filter_map(|shape| {
                ray_box(
                    local_origin,
                    local_direction,
                    shape.min.into(),
                    shape.max.into(),
                    max_distance,
                )
            })
            .chain(self.prototype.source.doors.iter().filter_map(|door| {
                ray_door(
                    local_origin,
                    local_direction,
                    door,
                    self.doors.get(&door.id).map_or(0.0, |state| state.angle),
                    max_distance,
                )
            }))
            .min_by(f32::total_cmp)
    }

    /// Floor and visible stair support independent of render LOD and roof bounds.
    /// The input is feet position, and step allowance is in world units.
    #[must_use]
    pub fn support_point(
        &self,
        root: &GlobalTransform,
        world_position: Vec3,
        max_step_world: f32,
    ) -> Option<Vec3> {
        self.support_capsule_point(root, world_position, max_step_world, 0.0)
    }

    /// Floor support begins when the capsule footprint first reaches the slab,
    /// allowing a raised threshold to be stepped onto before the feet centre is
    /// inside the footprint. Returned height remains under the original feet.
    #[must_use]
    pub fn support_capsule_point(
        &self,
        root: &GlobalTransform,
        world_position: Vec3,
        max_step_world: f32,
        world_radius: f32,
    ) -> Option<Vec3> {
        let point = native_point(root, world_position);
        let radius = local_radius(root, world_radius);
        let max_step = max_step_world / root.affine().matrix3.y_axis.length();
        let floor = crate::geometry::foundation::bounds(&self.prototype.source.recipe);
        std::iter::once(&floor)
            .chain(
                self.prototype
                    .source
                    .stair_colliders
                    .iter()
                    .filter(|_| self.stairs_visible),
            )
            .filter(|shape| {
                let closest = Vec2::new(
                    point.x.clamp(shape.min[0], shape.max[0]),
                    point.y.clamp(shape.min[1], shape.max[1]),
                );
                Vec2::new(point.x, point.y).distance_squared(closest) <= radius * radius
                    && shape.max[2] <= point.z + max_step + 0.00001
            })
            .map(|shape| shape.max[2])
            .max_by(f32::total_cmp)
            .map(|height| {
                root.affine()
                    .transform_point3(native_to_bevy([point.x, point.y, height]))
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn slab_ray_preserves_normalized_planet_units() {
        let hit = ray_box(
            Vec3::new(0.0, -3.0, 1.0),
            Vec3::Y * 6_000_000.0,
            Vec3::new(-1.0, -0.1, 0.0),
            Vec3::new(1.0, 0.1, 2.0),
            1.0,
        )
        .unwrap();
        assert!((hit - 2.9 / 6_000_000.0).abs() < 1e-10);
    }
    #[test]
    fn parallel_ray_outside_slab_misses() {
        assert!(
            ray_box(
                Vec3::new(2.0, 0.0, 0.0),
                Vec3::Y,
                Vec3::ZERO,
                Vec3::ONE,
                10.0
            )
            .is_none()
        );
    }
}
