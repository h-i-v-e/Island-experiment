use super::{
    BuildingBlocker, BuildingInstance, BuildingObserver, BuildingOccupant, BuildingRuntimeSettings,
    DoorLeaf,
    collision::{door_local, local_radius, native_point, ray_door, sphere_box},
};
use crate::{
    DoorDescriptor,
    render::{bevy_to_native, native_to_bevy},
};
use bevy::prelude::*;

#[allow(clippy::needless_pass_by_value)] // Bevy owns system parameters.
pub(super) fn interact_with_doors(
    settings: Res<BuildingRuntimeSettings>,
    keys: Option<Res<ButtonInput<KeyCode>>>,
    mut observers: Query<(&GlobalTransform, &mut BuildingObserver)>,
    mut buildings: Query<(Entity, &GlobalTransform, &mut BuildingInstance)>,
) {
    for (observer_transform, mut observer) in &mut observers {
        let requested = std::mem::take(&mut observer.interact)
            || keys
                .as_ref()
                .is_some_and(|keys| keys.just_pressed(KeyCode::KeyE));
        if !requested || !observer.interaction_enabled {
            continue;
        }
        let Some(direction) = observer.view_direction.try_normalize() else {
            continue;
        };
        let mut nearest: Option<(Entity, String, f32)> = None;
        for (entity, root, building) in &buildings {
            let eye = observer_transform.translation() + root.up().as_vec3() * observer.eye_height;
            let local_eye = native_point(root, eye);
            let local_direction = Vec3::from_array(bevy_to_native(
                root.affine().inverse().transform_vector3(direction),
            ));
            let world_scale = root.affine().transform_vector3(Vec3::X).length();
            let limit = settings.interaction_metres * world_scale;
            for door in &building.prototype.source.doors {
                let angle = building
                    .doors
                    .get(&door.id)
                    .map_or(0.0, |state| state.angle);
                let Some(distance) = ray_door(local_eye, local_direction, door, angle, limit)
                else {
                    continue;
                };
                // Structural walls and other doors must not be interactable through.
                let occluded = building
                    .raycast(root, eye, direction, distance)
                    .is_some_and(|hit| hit + world_scale * 0.01 < distance);
                if !occluded && nearest.as_ref().is_none_or(|(_, _, best)| distance < *best) {
                    nearest = Some((entity, door.id.clone(), distance));
                }
            }
        }
        if let Some((entity, id, _)) = nearest
            && let Ok((_, _, mut building)) = buildings.get_mut(entity)
        {
            building.toggle_door(&id);
        }
    }
}

#[allow(clippy::needless_pass_by_value)] // Bevy owns system parameters.
pub(super) fn animate_doors(
    time: Res<Time>,
    settings: Res<BuildingRuntimeSettings>,
    mut buildings: Query<(Entity, &GlobalTransform, &mut BuildingInstance)>,
    observers: Query<(&GlobalTransform, &BuildingObserver)>,
    occupants: Query<(&GlobalTransform, &BuildingOccupant)>,
    blockers: Query<(&GlobalTransform, &BuildingBlocker)>,
) {
    let max_delta = settings.door_speed_radians * time.delta_secs().min(0.25);
    for (entity, root, mut building) in &mut buildings {
        // Split the immutable shared descriptors from mutable instance state.
        let BuildingInstance {
            prototype, doors, ..
        } = &mut *building;
        for door in &prototype.source.doors {
            let Some(state) = doors.get_mut(&door.id) else {
                continue;
            };
            let next = approach(state.angle, state.target, max_delta);
            if (next - state.angle).abs() <= f32::EPSILON {
                continue;
            }
            let blocked = observers.iter().any(|(transform, observer)| {
                sweep_blocked(
                    root,
                    transform.translation(),
                    observer.radius,
                    observer.height,
                    door,
                    state.angle,
                    next,
                )
            }) || occupants
                .iter()
                .filter(|(_, occupant)| occupant.building == entity)
                .any(|(transform, occupant)| {
                    sweep_blocked(
                        root,
                        transform.translation(),
                        occupant.radius,
                        occupant.height,
                        door,
                        state.angle,
                        next,
                    )
                })
                || blockers.iter().any(|(transform, blocker)| {
                    sweep_blocked(
                        root,
                        transform.translation(),
                        blocker.radius,
                        blocker.height,
                        door,
                        state.angle,
                        next,
                    )
                });
            if !blocked {
                state.angle = next;
            }
        }
    }
}

fn approach(angle: f32, target: f32, max_delta: f32) -> f32 {
    angle + (target - angle).clamp(-max_delta, max_delta)
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn sweep_blocked(
    root: &GlobalTransform,
    feet: Vec3,
    radius: f32,
    height: f32,
    door: &DoorDescriptor,
    from: f32,
    to: f32,
) -> bool {
    let point = native_point(root, feet);
    let radius = local_radius(root, radius);
    let height = local_radius(root, height);
    // At most 2 cm of arc travel, including both endpoints. A frame spike cannot
    // tunnel through a player even when neither final leaf pose intersects them.
    // Validated leaf dimensions and bounded animation deltas fit u16 exactly.
    let steps = (((to - from).abs() * door.width / 0.02).ceil() as u16).max(1);
    (1..=steps).any(|step| {
        let angle = from + (to - from) * f32::from(step) / f32::from(steps);
        capsule_hits_door(point, radius, height, door, angle)
    })
}

fn capsule_hits_door(
    feet: Vec3,
    radius: f32,
    height: f32,
    door: &DoorDescriptor,
    angle: f32,
) -> bool {
    let mut point = door_local(feet, door, angle);
    let lower = point.z + radius;
    let upper = (point.z + height - radius).max(lower);
    point.z = (door.height * 0.5).clamp(lower, upper);
    sphere_box(
        point,
        radius,
        Vec3::new(0.0, -door.thickness * 0.5, 0.0),
        Vec3::new(door.width, door.thickness * 0.5, door.height),
    )
}

pub(super) fn sync_door_leaves(
    buildings: Query<&BuildingInstance>,
    mut leaves: Query<(&DoorLeaf, &mut Transform, &mut Visibility)>,
) {
    for (leaf, mut transform, mut visibility) in &mut leaves {
        let Ok(building) = buildings.get(leaf.building) else {
            continue;
        };
        let Some(door) = building
            .prototype
            .source
            .doors
            .iter()
            .find(|door| door.id == leaf.door_id)
        else {
            continue;
        };
        let angle = building
            .doors
            .get(&leaf.door_id)
            .map_or(0.0, |state| state.angle);
        transform.translation = native_to_bevy(door.hinge);
        transform.rotation = Quat::from_rotation_y(door.closed_angle + angle);
        *visibility = if building.current_lod == 0 {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }
}

#[cfg(test)]
#[allow(clippy::float_cmp)] // Exact bounded-step endpoints are the tested contract.
mod tests {
    use super::*;
    fn descriptor() -> DoorDescriptor {
        DoorDescriptor {
            id: "test".into(),
            hinge: [0.0; 3],
            closed_angle: 0.0,
            swing_angle: std::f32::consts::FRAC_PI_2,
            width: 1.2,
            height: 2.2,
            thickness: 0.05,
            mesh: crate::BuildingMesh::default(),
            hardware_mesh: crate::BuildingMesh::default(),
            exterior: true,
        }
    }
    #[test]
    fn swept_leaf_catches_blocker_between_clear_endpoints() {
        let door = descriptor();
        let feet = Vec3::new(0.7, 0.7, 0.0);
        assert!(!capsule_hits_door(feet, 0.15, 1.8, &door, 0.0));
        assert!(!capsule_hits_door(
            feet,
            0.15,
            1.8,
            &door,
            std::f32::consts::FRAC_PI_2
        ));
        let world = native_to_bevy(feet.to_array());
        assert!(sweep_blocked(
            &GlobalTransform::IDENTITY,
            world,
            0.15,
            1.8,
            &door,
            0.0,
            std::f32::consts::FRAC_PI_2
        ));
    }
    #[test]
    fn interaction_ray_hits_either_side_of_closed_leaf() {
        let door = descriptor();
        for side in [-1.0, 1.0] {
            assert!(
                ray_door(
                    Vec3::new(0.6, side * 1.0, 1.6),
                    Vec3::Y * -side,
                    &door,
                    0.0,
                    3.0
                )
                .is_some()
            );
        }
    }
    #[test]
    fn approach_is_bounded_and_reaches_target() {
        assert_eq!(approach(0.0, 1.0, 0.1), 0.1);
        assert_eq!(approach(0.95, 1.0, 0.1), 1.0);
        assert_eq!(approach(0.2, -1.0, 0.1), 0.1);
    }
}
