#![allow(clippy::float_cmp)] // Exact authored dimensions and metadata.
use island_building::{BuildingRecipe, StairAxis, generate};

fn recipe(axis: StairAxis) -> BuildingRecipe {
    let mut recipe = BuildingRecipe::procedural(7., 7.);
    recipe.storeys = 3;
    recipe.atlas_size = 128;
    recipe.staircase.axis = axis;
    recipe
}

#[test]
fn stair_room_encloses_every_level_and_exports_one_internal_door_per_floor() {
    for axis in [StairAxis::X, StairAxis::Y] {
        let recipe = recipe(axis);
        let prototype = generate(&recipe).unwrap();
        let stairs = prototype.layout.staircase.as_ref().unwrap();
        let room = stairs.stairwell.as_ref().unwrap();
        assert_eq!(room.doorways.len(), 3);
        assert_eq!(room.door_width, recipe.opening_defaults.door.width);
        assert_eq!(room.door_height, recipe.opening_defaults.door.height);
        assert_eq!(prototype.doors.iter().filter(|d| !d.exterior).count(), 3);
        assert_eq!(prototype.layout.room_count, 4);
        for doorway in &room.doorways {
            let door = prototype
                .doors
                .iter()
                .find(|d| d.id == format!("stairwell-door-{}", doorway.storey))
                .unwrap();
            assert!(!door.exterior);
            assert_eq!(door.hinge[2], recipe.floor_height(doorway.storey) + 0.006);
            assert_eq!(door.swing_angle, -std::f32::consts::FRAC_PI_2);
            assert!(!door.mesh.positions.is_empty() && !door.hardware_mesh.positions.is_empty());
            let enclosure = room.enclosure.unwrap();
            for side in island_building::StairwellSide::ALL {
                if enclosure.shared_sides[side.index()] {
                    continue;
                }
                let offset = if side == enclosure.door_side {
                    enclosure.door_offset - room.door_width * 0.5 - 0.06
                } else if side.across() {
                    (enclosure.min[0] + enclosure.max[0]) * 0.5
                } else {
                    (enclosure.min[1] + enclosure.max[1]) * 0.5
                };
                let p = enclosure.surface(side, offset);
                let n = side.outward();
                let point = stairs.point(
                    p[0] + n[0] * room.wall_thickness * 0.5,
                    p[1] + n[1] * room.wall_thickness * 0.5,
                    recipe.floor_height(doorway.storey) + 1.2,
                );
                assert!(
                    prototype.colliders.iter().any(|c| c.contains(point)),
                    "retained wall at {point:?}"
                );
            }
        }
        let decoded: island_building::layout::BuildingLayout =
            serde_json::from_str(&serde_json::to_string(&prototype.layout).unwrap()).unwrap();
        assert_eq!(decoded, prototype.layout);
    }
}

#[cfg(feature = "bevy")]
#[test]
fn each_floor_door_blocks_when_closed_and_opens_to_a_supported_capsule_route() {
    use bevy::prelude::*;
    use island_building::{
        interaction::BuildingInstance,
        render::{compile_building, native_to_bevy},
    };
    for axis in [StairAxis::X, StairAxis::Y] {
        let source = std::sync::Arc::new(generate(&recipe(axis)).unwrap());
        let compiled = compile_building(
            source,
            &mut Assets::default(),
            &mut Assets::default(),
            &mut Assets::default(),
        );
        let mut instance = BuildingInstance::new(compiled, "stair-room".into());
        instance.stairs_visible = false;
        let placement = GlobalTransform::IDENTITY;
        for storey in 0..3 {
            let stairs = instance.prototype.source.layout.staircase.as_ref().unwrap();
            let room = stairs.stairwell.as_ref().unwrap();
            let doorway = room.doorways[storey];
            let crossing = [
                doorway.centre[0],
                doorway.centre[1],
                stairs.floor_heights[storey] + 0.025,
            ];
            let door = instance
                .prototype
                .source
                .doors
                .iter()
                .find(|d| d.id == format!("stairwell-door-{storey}"))
                .unwrap();
            let angle = door.swing_angle;
            let id = door.id.clone();
            assert!(instance.blocks_capsule(&placement, native_to_bevy(crossing), 0.28, 1.8));
            instance.doors.get_mut(&id).unwrap().angle = angle;
            for lod in 0..3 {
                instance.current_lod = lod;
                for step in 0..=40_u16 {
                    let t = f32::from(step) / 40.;
                    let point = std::array::from_fn(|i| {
                        doorway.room_approach[i]
                            + (doorway.stair_approach[i] - doorway.room_approach[i]) * t
                    });
                    let feet = native_to_bevy(point);
                    assert!(
                        !instance.blocks_capsule(&placement, feet, 0.28, 1.8),
                        "floor {storey}, LOD{lod}, {point:?}"
                    );
                    let support = instance
                        .support_capsule_point(&placement, feet, 0.26, 0.28)
                        .unwrap();
                    assert!((support.y - (feet.y - 0.025)).abs() < 0.001);
                }
            }
            instance.doors.get_mut(&id).unwrap().angle = 0.;
        }
    }
}

#[test]
fn open_stairs_remain_available_and_invalid_stair_doors_are_diagnosed() {
    let mut recipe = recipe(StairAxis::Auto);
    recipe.staircase.enclosed = false;
    let open = generate(&recipe).unwrap();
    assert!(open.layout.staircase.as_ref().unwrap().stairwell.is_none());
    assert!(open.doors.iter().all(|d| d.exterior));
    assert_eq!(open.layout.room_count, 3);
    let mut json = serde_json::to_value(&recipe).unwrap();
    json["staircase"]
        .as_object_mut()
        .unwrap()
        .remove("enclosed");
    let restored: BuildingRecipe = serde_json::from_value(json).unwrap();
    assert!(restored.staircase.enclosed);
    recipe.staircase.enclosed = true;
    recipe.opening_defaults.door.width = 3.;
    assert!(
        generate(&recipe)
            .unwrap_err()
            .to_string()
            .contains("staircase.door")
    );
    recipe.opening_defaults.door.width = 1.15;
    recipe.openings = BuildingRecipe::hut().openings;
    recipe.openings[0].id = "stairwell-door-0".into();
    recipe.opening_generation = None;
    assert!(
        generate(&recipe)
            .unwrap_err()
            .to_string()
            .contains("reserved")
    );
}

#[test]
fn compact_offset_entrance_cottage_keeps_a_shared_stair_room_clear_of_the_chimney() {
    let mut recipe = BuildingRecipe::procedural(6., 5.966_667);
    recipe.wall_height = 2.4;
    recipe.storeys = 3;
    recipe.atlas_size = 128;
    recipe.walls.style = island_building::WallStyle::WattleAndDaub;
    recipe.roof.covering = island_building::RoofCovering::Thatch;
    recipe.opening_defaults.window.width = 0.6;
    recipe
        .opening_generation
        .as_mut()
        .unwrap()
        .overrides
        .push(island_building::OpeningOverride {
            id: "auto-door-south".into(),
            offset: Some(0.65),
            ..Default::default()
        });
    let prototype = generate(&recipe).unwrap();
    let stairs = prototype.layout.staircase.as_ref().unwrap();
    let enclosure = stairs.stairwell.as_ref().unwrap().enclosure.unwrap();
    assert!(enclosure.shared_sides.iter().any(|s| *s));
    assert_eq!(prototype.doors.iter().filter(|d| !d.exterior).count(), 3);
    assert!(enclosure.shared_sides[island_building::StairwellSide::Far.index()]);
    for height in [stairs.floor_heights[2], stairs.turn_landings[0].max[2]] {
        let edge = stairs.point(
            stairs.span() * 0.5,
            stairs.length() - 0.0175,
            height + 0.025,
        );
        assert!(
            prototype
                .colliders
                .iter()
                .any(|c| c.intersects_capsule(edge, 0.28, 1.8)),
            "shared stairwell edges retain a guard at {edge:?}"
        );
    }
}
