use island_building::{BuildingRecipe, MaterialSlot, generate};

fn church() -> island_building::BuildingPrototype {
    let mut r = BuildingRecipe::gothic_church();
    r.atlas_size = 128;
    r.foundation.stairs = false;
    generate(&r).unwrap()
}
#[test]
fn paired_leaves_fill_the_pointed_entrance_without_a_glass_fanlight() {
    let p = church();
    let r = &p.recipe;
    let opening = &r.openings[0];
    assert_eq!(p.doors.len(), 2);
    let left = &p.doors[0];
    let right = &p.doors[1];
    assert!(left.id.ends_with("-left") && right.id.ends_with("-right"));
    assert!(left.hinge[0] < 0. && right.hinge[0] > 0.);
    assert!(left.swing_angle > 0. && right.swing_angle < 0.);
    assert!((right.closed_angle - left.closed_angle - std::f32::consts::PI).abs() < 0.00001);
    let apex = opening.height + opening.width * 0.5 * 3_f32.sqrt();
    for door in &p.doors {
        assert!((door.hinge[2] + door.height - apex).abs() < 0.04);
        assert!(!door.hardware_mesh.positions.is_empty());
        assert!(
            (door.mesh.positions.iter().map(|p| p[0]).fold(0., f32::max) - door.width).abs()
                < 0.00001
        );
        assert!(
            door.mesh
                .positions
                .iter()
                .any(|p| p[2] > opening.height + 0.8)
        );
        assert_eq!(door.collision_sections.len(), 12);
    }
    let seam = right.hinge[0] - right.width - (left.hinge[0] + left.width);
    assert!((seam - 0.008).abs() < 0.00001);
    for lod in &p.lods[..2] {
        assert!(
            !lod.parts
                .iter()
                .filter(|part| matches!(
                    part.material,
                    MaterialSlot::Glass | MaterialSlot::WindowProxy
                ))
                .flat_map(|part| &part.mesh.positions)
                .any(|point| point[0].abs() < opening.width * 0.5
                    && point[1] < -r.depth * 0.5 + r.wall_thickness
                    && point[2] > opening.height)
        );
    }
    let proxy = &p.lods[1]
        .parts
        .iter()
        .find(|part| part.material == MaterialSlot::Door)
        .unwrap()
        .mesh;
    assert!(proxy.positions.iter().any(|p| p[2] > apex - 0.04));
}

#[test]
fn shaped_collision_follows_each_hinge_without_filling_the_arch_corners() {
    let p = church();
    for door in &p.doors {
        for angle in [0., door.swing_angle * 0.5, door.swing_angle] {
            let (sin, cos) = (door.closed_angle + angle).sin_cos();
            let world = |x: f32, z: f32| {
                [
                    door.hinge[0] + cos * x - sin * door.depth_offset,
                    door.hinge[1] + sin * x + cos * door.depth_offset,
                    door.hinge[2] + z,
                ]
            };
            let solid = world(door.width * 0.8, 4.1);
            assert!(
                door.colliders_at(angle)
                    .any(|c| c.intersects_capsule(solid, 0.015, 0.1))
            );
            let missing_corner = world(0.025, door.height - 0.15);
            assert!(
                door.collider_at(angle)
                    .intersects_capsule(missing_corner, 0.015, 0.1)
            );
            assert!(!door.colliders_at(angle).any(|c| c.intersects_capsule(
                missing_corner,
                0.015,
                0.1
            )));
        }
    }
}

#[cfg(feature = "bevy")]
#[test]
fn leaves_open_and_restore_independently_and_block_their_own_half_of_the_doorway() {
    use bevy::prelude::*;
    use island_building::{
        interaction::BuildingInstance,
        render::{compile_building, native_to_bevy},
    };
    use std::sync::Arc;
    let p = church();
    let depth = p.recipe.depth;
    let compiled = compile_building(
        Arc::new(p),
        &mut Assets::default(),
        &mut Assets::default(),
        &mut Assets::default(),
    );
    let left = &compiled.source.doors[0];
    let right = &compiled.source.doors[1];
    let mut instance = BuildingInstance::new(Arc::clone(&compiled), "church".into());
    assert!(instance.toggle_door(&left.id));
    assert!(instance.doors[&left.id].target > 0.);
    assert!(instance.doors[&right.id].target.abs() < 0.00001);
    instance.doors.get_mut(&left.id).unwrap().angle = left.swing_angle;
    let state = instance.snapshot();
    let mut restored = BuildingInstance::new(Arc::clone(&compiled), "church".into());
    assert!(restored.restore_state(&state));
    assert_eq!(instance.doors, restored.doors);
    let feet = |x| native_to_bevy([x, -depth * 0.5 + 0.52, 0.02]);
    assert!(!instance.blocks_capsule(&GlobalTransform::IDENTITY, feet(-0.6), 0.15, 1.8));
    assert!(instance.blocks_capsule(&GlobalTransform::IDENTITY, feet(0.6), 0.15, 1.8));
}
