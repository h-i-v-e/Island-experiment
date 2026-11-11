use super::{
    BuildingInstance, BuildingLodRoot, BuildingObserver, BuildingOccupant, BuildingRuntimeSettings,
    LodMode,
};
use crate::render::{bevy_to_native, native_to_bevy};
use bevy::prelude::*;

#[allow(clippy::needless_pass_by_value)] // Bevy owns system parameters.
pub(super) fn select_lods(
    settings: Res<BuildingRuntimeSettings>,
    mut buildings: Query<(Entity, &GlobalTransform, &mut BuildingInstance)>,
    observers: Query<(&GlobalTransform, &BuildingObserver)>,
    occupants: Query<&BuildingOccupant>,
) {
    for (entity, root, mut building) in &mut buildings {
        let occupied = occupants.iter().any(|occupant| occupant.building == entity);
        let mut pinned = occupied;
        let mut pixels: f32 = 0.0;
        for (transform, observer) in &observers {
            let feet = super::collision::local_point(root, transform.translation());
            let eye = feet + Vec3::Y * super::collision::local_radius(root, observer.eye_height);
            pinned |= building
                .prototype
                .source
                .layout
                .contains(bevy_to_native(feet), 0.05)
                || building
                    .prototype
                    .inspection_points
                    .iter()
                    .any(|point| point.distance(eye) <= settings.opening_inspection_metres)
                || near_opening(&building, eye, settings.opening_inspection_metres);
            let distance = eye.distance(building.prototype.centre).max(0.01);
            let fov = observer.vertical_fov_radians.clamp(0.1, 3.0);
            let projected = building.prototype.radius / distance
                * observer.viewport_height.max(1.0)
                / (fov * 0.5).tan();
            pixels = pixels.max(projected);
        }
        building.current_lod = if pinned {
            0
        } else {
            match building.lod_mode {
                LodMode::Auto => hysteretic_lod(building.current_lod, pixels, &settings),
                LodMode::Lod0 => 0,
                LodMode::Lod1 => 1,
                LodMode::Lod2 => 2,
            }
        };
    }
}

fn near_opening(building: &BuildingInstance, eye: Vec3, range: f32) -> bool {
    let recipe = &building.prototype.source.recipe;
    recipe.openings.iter().any(|opening| {
        let a = native_to_bevy(opening.facade.point(
            recipe,
            opening.offset - opening.width * 0.5,
            opening.sill,
            0.0,
        ));
        let b = native_to_bevy(opening.facade.point(
            recipe,
            opening.offset + opening.width * 0.5,
            opening.sill + opening.height,
            0.0,
        ));
        eye.distance(eye.clamp(a.min(b), a.max(b))) <= range
    })
}

fn hysteretic_lod(current: usize, pixels: f32, settings: &BuildingRuntimeSettings) -> usize {
    let hysteresis = settings.hysteresis.clamp(0.0, 0.45);
    let near = settings.near_pixels
        * if current == 0 {
            1.0 - hysteresis
        } else {
            1.0 + hysteresis
        };
    let far = settings.far_pixels
        * if current == 2 {
            1.0 + hysteresis
        } else {
            1.0 - hysteresis
        };
    if pixels >= near {
        0
    } else if pixels >= far {
        1
    } else {
        2
    }
}

pub(super) fn sync_lod_roots(
    buildings: Query<&BuildingInstance>,
    mut roots: Query<(&BuildingLodRoot, &mut Visibility)>,
) {
    for (lod, mut visibility) in &mut roots {
        let Ok(building) = buildings.get(lod.building) else {
            continue;
        };
        *visibility = if building.current_lod == lod.level {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }
}

pub(super) fn sync_stairs(
    buildings: Query<&BuildingInstance>,
    mut stairs: Query<(&super::BuildingStairs, &mut Visibility), Without<BuildingLodRoot>>,
) {
    for (stairs, mut visibility) in &mut stairs {
        if let Ok(building) = buildings.get(stairs.building) {
            *visibility = if building.stairs_visible {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn thresholds_have_non_overlapping_return_bands() {
        let settings = BuildingRuntimeSettings::default();
        assert_eq!(hysteretic_lod(0, 250.0, &settings), 0);
        assert_eq!(hysteretic_lod(1, 250.0, &settings), 1);
        assert_eq!(hysteretic_lod(1, 75.0, &settings), 1);
        assert_eq!(hysteretic_lod(2, 75.0, &settings), 2);
        assert_eq!(hysteretic_lod(2, 1000.0, &settings), 0);
    }
}
