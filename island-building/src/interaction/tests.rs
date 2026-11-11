#![allow(clippy::float_cmp)] // Retained state must preserve exact stored angles.

use super::*;
use crate::{
    BuildingRecipe, generate,
    render::{CompiledBuildingPrototype, compile_building, native_to_bevy},
};
use bevy::prelude::*;
use std::sync::Arc;

mod repeated;

fn prototype() -> Arc<CompiledBuildingPrototype> {
    let mut recipe = BuildingRecipe::hut();
    recipe.atlas_size = 128;
    compile_building(
        Arc::new(generate(&recipe).unwrap()),
        &mut Assets::default(),
        &mut Assets::default(),
        &mut Assets::default(),
    )
}

#[test]
fn occupancy_pins_near_lod_while_observer_leaves_and_preserves_doors() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_plugins(BuildingRuntimePlugin);
    let mut instance = BuildingInstance::new(prototype(), "hut-a".into());
    instance.lod_mode = LodMode::Lod2;
    instance.doors.get_mut("entrance").unwrap().angle = 0.8;
    instance.doors.get_mut("entrance").unwrap().target = 0.8;
    let root = app
        .world_mut()
        .spawn((instance, GlobalTransform::IDENTITY))
        .id();
    let occupant = app
        .world_mut()
        .spawn((
            BuildingOccupant {
                building: root,
                radius: 0.28,
                height: 1.8,
            },
            GlobalTransform::from_translation(Vec3::ZERO),
        ))
        .id();
    app.world_mut().spawn((
        BuildingObserver::default(),
        GlobalTransform::from_translation(Vec3::splat(10000.0)),
    ));
    app.update();
    let instance = app.world().get::<BuildingInstance>(root).unwrap();
    assert_eq!(instance.current_lod, 0);
    assert_eq!(instance.doors["entrance"].angle, 0.8);
    app.world_mut().despawn(occupant);
    app.update();
    let instance = app.world().get::<BuildingInstance>(root).unwrap();
    assert_eq!(instance.current_lod, 2);
    assert_eq!(instance.doors["entrance"].angle, 0.8);
}

#[test]
fn scene_state_migrates_by_stable_id_without_mutating_shared_assets() {
    let prototype = prototype();
    let mut first = BuildingInstance::new(Arc::clone(&prototype), "a".into());
    let second = BuildingInstance::new(prototype, "b".into());
    assert!(first.toggle_door("entrance"));
    first.doors.get_mut("entrance").unwrap().angle = 0.7;
    let saved = first.snapshot();
    assert_eq!(second.doors["entrance"].angle, 0.0);
    let mut replacement = BuildingInstance::new(Arc::clone(&first.prototype), "a".into());
    assert!(replacement.restore_state(&saved));
    assert_eq!(replacement.doors["entrance"], first.doors["entrance"]);
    assert!(Arc::ptr_eq(&first.prototype, &replacement.prototype));
    let encoded = serde_json::to_string(&saved).unwrap();
    assert_eq!(
        serde_json::from_str::<BuildingSceneState>(&encoded).unwrap(),
        saved
    );
    replacement.placement_id = "wrong".into();
    assert!(!replacement.restore_state(&saved));
}

#[test]
fn radial_scaled_collision_and_floor_queries_preserve_metres() {
    let instance = BuildingInstance::new(prototype(), "radial".into());
    let rotation = Quat::from_rotation_z(0.8) * Quat::from_rotation_x(-0.4);
    // Translation near a normalized planet introduces ordinary f32 precision;
    // use a scale that still permits millimetre-scale assertions in this test.
    let root = GlobalTransform::from(
        Transform::from_translation(Vec3::new(0.3, 0.8, -0.5))
            .with_rotation(rotation)
            .with_scale(Vec3::splat(0.01)),
    );
    let world = |point| root.affine().transform_point3(native_to_bevy(point));
    assert!(instance.blocks_sphere(&root, world([2.95, 0.0, 1.0]), 0.001));
    assert!(!instance.blocks_sphere(&root, world([0.0, 0.0, 1.0]), 0.001));
    let floor = instance
        .support_point(&root, world([0.0, 0.0, 0.2]), 0.001)
        .unwrap();
    assert!(floor.distance(world([0.0, 0.0, 0.0])) < 1e-6);
    let direction = (world([0.0, 0.0, 1.6]) - world([0.0, -4.0, 1.6])).normalize();
    assert!(
        instance
            .raycast(&root, world([0.0, -4.0, 1.6]), direction, 0.1)
            .is_some()
    );
}

#[test]
fn raised_floor_support_starts_when_capsule_reaches_doorway_threshold() {
    let mut instance = BuildingInstance::new(prototype(), "raised".into());
    let open = instance.prototype.source.doors[0].swing_angle;
    instance.doors.get_mut("entrance").unwrap().angle = open;
    let root = GlobalTransform::from_translation(Vec3::Y * 0.1);
    let feet = Vec3::new(0.0, 0.0, 2.65);
    assert!(instance.support_point(&root, feet, 0.35).is_none());
    let support = instance
        .support_capsule_point(&root, feet, 0.35, 0.27)
        .unwrap();
    assert!((support.y - 0.1).abs() < 1e-6);
    assert!(!instance.blocks_capsule(&root, support + Vec3::Y * 0.005, 0.27, 1.795));
    assert!(
        instance
            .support_capsule_point(&root, feet + Vec3::Z, 0.35, 0.27)
            .is_none()
    );
}

#[test]
fn compiled_lods_never_share_transparent_glass_with_far_shells() {
    let compiled = prototype();
    assert!(compiled.lods[0].iter().any(|part| part.glass));
    assert!(compiled.lods[1..].iter().flatten().all(|part| !part.glass));
    assert_eq!(compiled.doors.len(), compiled.source.doors.len());
}

#[test]
fn spawned_leaf_visibility_and_collider_pose_survive_far_lod_roundtrip() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_plugins(BuildingRuntimePlugin);
    let compiled = prototype();
    let opened = compiled.source.doors[0].swing_angle;
    let root = crate::render::spawn_building(
        &mut app.world_mut().commands(),
        Arc::clone(&compiled),
        "visible",
        Transform::IDENTITY,
    );
    app.world_mut().flush();
    {
        let mut instance = app.world_mut().get_mut::<BuildingInstance>(root).unwrap();
        instance.lod_mode = LodMode::Lod2;
        instance.doors.get_mut("entrance").unwrap().angle = opened;
        instance.doors.get_mut("entrance").unwrap().target = opened;
    }
    app.update();
    let mut leaves = app
        .world_mut()
        .query::<(&DoorLeaf, &Visibility, &Transform)>();
    let (_, visibility, transform) = leaves.single(app.world()).unwrap();
    assert_eq!(*visibility, Visibility::Hidden);
    assert!(
        transform
            .rotation
            .abs_diff_eq(Quat::from_rotation_y(opened), 1e-6)
    );
    app.world_mut()
        .get_mut::<BuildingInstance>(root)
        .unwrap()
        .lod_mode = LodMode::Lod0;
    app.update();
    let (_, visibility, transform) = leaves.single(app.world()).unwrap();
    assert_eq!(*visibility, Visibility::Inherited);
    assert!(
        transform
            .rotation
            .abs_diff_eq(Quat::from_rotation_y(opened), 1e-6)
    );
    let instance = app.world().get::<BuildingInstance>(root).unwrap();
    assert!(!instance.blocks_capsule(
        &GlobalTransform::IDENTITY,
        native_to_bevy([0.0, -2.4, 0.005]),
        0.27,
        1.795
    ));
    // Removing the placement removes all render and dynamic leaf children.
    app.world_mut().despawn(root);
    assert_eq!(leaves.iter(app.world()).count(), 0);
}

#[test]
fn metal_knobs_inherit_each_hinge_transform_and_near_lod_visibility() {
    for facade in crate::Facade::ALL {
        for hinge in [crate::HingeSide::Left, crate::HingeSide::Right] {
            let mut recipe = BuildingRecipe::hut();
            recipe.chimney = None; // Isolate door hardware from fireplace placement constraints.
            recipe.atlas_size = 128;
            recipe.openings.retain(|opening| {
                opening.kind == crate::OpeningKind::Door || opening.facade != facade
            });
            recipe.openings[0].facade = facade;
            recipe.openings[0].hinge = hinge;
            assert_knob_runtime(&recipe);
        }
    }
}
fn assert_knob_runtime(recipe: &BuildingRecipe) {
    let mut meshes = Assets::default();
    let mut materials = Assets::default();
    let mut images = Assets::default();
    let prototype = compile_building(
        Arc::new(generate(recipe).unwrap()),
        &mut meshes,
        &mut materials,
        &mut images,
    );
    let hardware = &prototype.door_hardware[0];
    let metal = materials.get(&hardware.material).unwrap();
    assert!(
        metal.base_color_texture.is_none()
            && metal.normal_map_texture.is_none()
            && metal.metallic_roughness_texture.is_none()
    );
    assert!(metal.metallic > 0.8 && metal.perceptual_roughness < 0.3);
    assert_ne!(hardware.material, prototype.doors[0].material);
    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins,
        bevy::transform::TransformPlugin,
        BuildingRuntimePlugin,
    ));
    let placement = Transform::from_xyz(4., 3., -2.)
        .with_rotation(Quat::from_rotation_z(0.4))
        .with_scale(Vec3::splat(0.6));
    let root = crate::render::spawn_building(
        &mut app.world_mut().commands(),
        Arc::clone(&prototype),
        "knob-transform",
        placement,
    );
    app.world_mut().flush();
    let leaf = app
        .world_mut()
        .query_filtered::<Entity, With<DoorLeaf>>()
        .single(app.world())
        .unwrap();
    let knob = app
        .world_mut()
        .query::<(Entity, &Mesh3d)>()
        .iter(app.world())
        .find(|(_, mesh)| mesh.0 == hardware.mesh)
        .map(|(entity, _)| entity)
        .unwrap();
    assert_eq!(app.world().get::<ChildOf>(knob).unwrap().parent(), leaf);
    assert_eq!(
        *app.world().get::<Transform>(knob).unwrap(),
        Transform::IDENTITY
    );
    let angle = 0.6 * prototype.source.doors[0].swing_angle.signum();
    {
        let mut instance = app.world_mut().get_mut::<BuildingInstance>(root).unwrap();
        instance.lod_mode = LodMode::Lod0;
        let state = instance.doors.get_mut("entrance").unwrap();
        state.angle = angle;
        state.target = angle;
    }
    app.update();
    let descriptor = &prototype.source.doors[0];
    let point = native_to_bevy(descriptor.hardware_mesh.positions[0]);
    let expected = placement.transform_point(
        native_to_bevy(descriptor.hinge)
            + Quat::from_rotation_y(descriptor.closed_angle + angle) * point,
    );
    let actual = app
        .world()
        .get::<GlobalTransform>(knob)
        .unwrap()
        .transform_point(point);
    assert!(
        actual.distance(expected) < 0.00001,
        "knob moves with the hinged leaf under a rotated/scaled placement"
    );
    app.world_mut()
        .get_mut::<BuildingInstance>(root)
        .unwrap()
        .lod_mode = LodMode::Lod2;
    app.update();
    assert_eq!(
        *app.world().get::<Visibility>(leaf).unwrap(),
        Visibility::Hidden
    );
    assert_eq!(
        *app.world().get::<Visibility>(knob).unwrap(),
        Visibility::Inherited
    );
    app.world_mut().despawn(root);
    assert!(app.world().get_entity(knob).is_err());
}

#[test]
fn fireplace_hearth_blocks_walking_at_every_visual_lod_and_keeps_floor_support() {
    let compiled = prototype();
    let recipe = &compiled.source.recipe;
    let chimney = recipe.chimney.as_ref().unwrap();
    let hearth = crate::geometry::chimney::hearth_bounds(recipe).unwrap();
    let hearth_centre: [f32; 3] = std::array::from_fn(|i| (hearth.min[i] + hearth.max[i]) * 0.5);
    let blocked = native_to_bevy([hearth_centre[0], hearth_centre[1], 0.005]);
    let clear = native_to_bevy(chimney.facade.point(
        recipe,
        chimney.offset,
        0.005,
        recipe.wall_thickness + chimney.depth + chimney.hearth_depth + 0.5,
    ));
    let root = GlobalTransform::IDENTITY;
    let mut instance = BuildingInstance::new(compiled, "hearth-collision".into());
    for level in 0..3 {
        instance.current_lod = level;
        assert!(instance.blocks_capsule(&root, blocked, 0.28, 1.8));
        assert!(!instance.blocks_capsule(&root, clear, 0.28, 1.8));
        let support = instance
            .support_point(&root, blocked + Vec3::Y * 0.2, 0.3)
            .unwrap();
        assert!(
            support.y.abs() < 1.0e-6,
            "the hearth is an obstacle, not a raised walking surface"
        );
    }
}
