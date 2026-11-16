#![allow(clippy::float_cmp)] // Authored dimensions and serialized inheritance are exact.
use std::borrow::Cow;

use island_building::{
    BuildingRecipe, DoorStyle, Facade, HingeSide, OpeningGeneration, OpeningKind, OpeningOverride,
    PartitionAxis, generate, openings, validate,
};

fn automatic(width: f32, depth: f32) -> BuildingRecipe {
    BuildingRecipe {
        atlas_size: 128,
        ..BuildingRecipe::procedural(width, depth)
    }
}

#[test]
fn legacy_recipes_retain_authored_openings_and_new_manual_openings_can_inherit() {
    let original = BuildingRecipe::cottage();
    let mut json = serde_json::to_value(&original).unwrap();
    json.as_object_mut().unwrap().remove("opening_defaults");
    for opening in json["openings"].as_array_mut().unwrap() {
        opening
            .as_object_mut()
            .unwrap()
            .remove("use_default_dimensions");
    }
    let mut loaded: BuildingRecipe = serde_json::from_value(json).unwrap();
    assert_eq!(loaded.openings, original.openings);
    assert!(matches!(
        openings::resolve(&loaded).unwrap(),
        Cow::Borrowed(_)
    ));
    loaded.opening_defaults.window.width = 1.6;
    loaded.openings[1].use_default_dimensions = true;
    let resolved = openings::resolve(&loaded).unwrap();
    assert_eq!(resolved.openings[1].width, 1.6);
    assert_eq!(resolved.openings[2].width, original.openings[2].width);
    assert_eq!(loaded.openings[1].width, original.openings[1].width);
    validate(&loaded).unwrap();
}

#[test]
fn facade_layouts_resize_deterministically_and_keep_uniform_dimensions() {
    let mut previous = 0;
    for size in [3., 6., 12., 40.] {
        let mut recipe = automatic(size, size);
        recipe.chimney = None;
        validate(&recipe).unwrap();
        let resolved = openings::resolve(&recipe).unwrap();
        assert!(resolved.openings.len() >= previous);
        previous = resolved.openings.len();
        assert!(previous <= 64);
        assert_eq!(resolved, openings::resolve(&recipe).unwrap());
        assert_eq!(
            resolved
                .openings
                .iter()
                .filter(|o| o.kind == OpeningKind::Door)
                .count(),
            1
        );
        for opening in &resolved.openings {
            let size = recipe.opening_defaults.for_kind(opening.kind);
            assert_eq!(
                [opening.width, opening.height, opening.sill],
                [size.width, size.height, size.sill]
            );
            assert!(
                opening.offset.abs() + opening.width * 0.5
                    <= opening.facade.length(&recipe) * 0.5 - 0.45
            );
        }
    }
}

#[test]
fn fireplace_and_partition_ends_reserve_clear_spans_for_generated_windows() {
    for axis in [PartitionAxis::X, PartitionAxis::Y] {
        let mut recipe = BuildingRecipe::cottage();
        recipe.opening_generation = Some(OpeningGeneration::default());
        recipe.partitions[0].axis = axis;
        validate(&recipe).unwrap();
        let resolved = openings::resolve(&recipe).unwrap();
        for opening in resolved
            .openings
            .iter()
            .filter(|o| o.facade == Facade::North)
        {
            let chimney = recipe.chimney.as_ref().unwrap();
            assert!(
                (opening.offset - chimney.offset).abs()
                    >= (opening.width + chimney.width) * 0.5 + 0.2
            );
        }
    }
}

#[test]
fn field_overrides_and_suppression_survive_resizes_and_default_changes() {
    let mut recipe = automatic(9., 7.);
    recipe.opening_generation.as_mut().unwrap().overrides = vec![
        OpeningOverride {
            id: "auto-door-south".into(),
            offset: Some(0.7),
            width: Some(1.3),
            hinge: Some(HingeSide::Right),
            door_style: Some(DoorStyle::FrameAndPanel),
            ..Default::default()
        },
        OpeningOverride {
            id: "auto-window-east-0".into(),
            disabled: true,
            ..Default::default()
        },
    ];
    recipe.opening_defaults.door.height = 2.25;
    recipe.depth = 10.;
    validate(&recipe).unwrap();
    let resolved = openings::resolve(&recipe).unwrap();
    let door = resolved
        .openings
        .iter()
        .find(|o| o.id == "auto-door-south")
        .unwrap();
    assert_eq!((door.offset, door.width, door.height), (0.7, 1.3, 2.25));
    assert_eq!(door.hinge, HingeSide::Right);
    assert_eq!(door.door_style, DoorStyle::FrameAndPanel);
    assert!(
        !resolved
            .openings
            .iter()
            .any(|o| o.id == "auto-window-east-0")
    );
    assert!(
        openings::generated_slots(&recipe)
            .unwrap()
            .iter()
            .any(|o| o.id == "auto-window-east-0")
    );
    let json = serde_json::to_string(&recipe).unwrap();
    let reloaded: BuildingRecipe = serde_json::from_str(&json).unwrap();
    assert_eq!(reloaded, recipe);
    assert_eq!(openings::resolve(&reloaded).unwrap(), resolved);
}

#[test]
fn absent_slot_overrides_are_preserved_until_the_building_grows_again() {
    let mut recipe = automatic(6., 5.);
    recipe
        .opening_generation
        .as_mut()
        .unwrap()
        .overrides
        .push(OpeningOverride {
            id: "auto-window-east-3".into(),
            height: Some(1.2),
            ..Default::default()
        });
    let small = openings::resolve(&recipe).unwrap();
    assert!(!small.openings.iter().any(|o| o.id == "auto-window-east-3"));
    recipe.depth = 18.;
    let large = openings::resolve(&recipe).unwrap();
    assert_eq!(
        large
            .openings
            .iter()
            .find(|o| o.id == "auto-window-east-3")
            .unwrap()
            .height,
        1.2
    );
}

#[test]
fn all_lods_collision_stairs_and_atlas_use_the_same_resolved_layout() {
    let mut recipe = automatic(8., 7.);
    recipe.chimney = None;
    recipe.opening_generation.as_mut().unwrap().door_facades = vec![Facade::South, Facade::North];
    let prototype = generate(&recipe).unwrap();
    assert_eq!(prototype.doors.len(), 2);
    assert!(prototype.recipe.openings.len() > 3);
    let mut manual = openings::resolve(&recipe).unwrap().into_owned();
    manual.opening_generation = None;
    let baked = generate(&manual).unwrap();
    assert_eq!(prototype.lods, baked.lods);
    assert_eq!(prototype.doors, baked.doors);
    assert_eq!(prototype.colliders, baked.colliders);
    assert_eq!(prototype.stair_colliders, baked.stair_colliders);
    assert_eq!(prototype.layout, baked.layout);
    assert_eq!(prototype.atlas, baked.atlas);
    assert_eq!(generate(&prototype.recipe).unwrap().hash, prototype.hash);
}

#[test]
fn invalid_settings_and_conflicting_overrides_are_diagnosed_before_generation() {
    let mut recipe = automatic(6., 5.);
    recipe.opening_defaults.window.width = f32::NAN;
    assert!(
        generate(&recipe)
            .unwrap_err()
            .to_string()
            .contains("opening_defaults.window.width")
    );
    recipe.opening_defaults.window.width = 1.4;
    recipe.opening_generation.as_mut().unwrap().window_spacing = 0.;
    assert!(
        validate(&recipe)
            .unwrap_err()
            .to_string()
            .contains("window_spacing")
    );
    recipe.opening_generation.as_mut().unwrap().window_spacing = 3.;
    recipe
        .opening_generation
        .as_mut()
        .unwrap()
        .overrides
        .push(OpeningOverride {
            id: "auto-door-south".into(),
            disabled: true,
            ..Default::default()
        });
    assert!(
        validate(&recipe)
            .unwrap_err()
            .to_string()
            .contains("at least one openable exterior door")
    );
    recipe.opening_generation.as_mut().unwrap().overrides[0].disabled = false;
    recipe.opening_generation.as_mut().unwrap().overrides[0].offset = Some(20.);
    assert!(
        validate(&recipe)
            .unwrap_err()
            .to_string()
            .contains("solid wall at corners")
    );
}

#[test]
fn documents_keep_generation_overrides_and_shared_sizes_in_history() {
    let mut document = island_building::document::StudioDocument::new(automatic(6., 5.));
    document.edit(|recipe| {
        recipe.opening_defaults.window.width = 1.6;
        recipe
            .opening_generation
            .as_mut()
            .unwrap()
            .overrides
            .push(OpeningOverride {
                id: "auto-door-south".into(),
                door_style: Some(DoorStyle::FrameAndPanel),
                ..Default::default()
            });
    });
    let edited = document.recipe().clone();
    document.undo();
    assert_eq!(document.recipe().opening_defaults.window.width, 1.4);
    document.redo();
    assert_eq!(document.recipe(), &edited);
}

#[test]
fn typo_ids_are_reported_and_half_timbered_thatch_uses_generated_openings() {
    let mut recipe = automatic(6., 6.);
    recipe.walls.style = island_building::WallStyle::WattleAndDaub;
    recipe.roof.covering = island_building::RoofCovering::Thatch;
    recipe.wall_height = 2.4;
    let prototype = generate(&recipe).unwrap();
    assert_eq!(prototype.doors.len(), 1);
    assert_eq!(
        prototype
            .recipe
            .openings
            .iter()
            .filter(|o| o.kind == OpeningKind::Window)
            .count(),
        8
    );
    recipe
        .opening_generation
        .as_mut()
        .unwrap()
        .overrides
        .push(OpeningOverride {
            id: "auto-door-sooth".into(),
            offset: Some(0.5),
            ..Default::default()
        });
    assert!(
        validate(&recipe)
            .unwrap_err()
            .to_string()
            .contains("auto-door-sooth")
    );
}

#[test]
fn shared_door_sizes_cover_internal_doorways_with_independent_overrides() {
    let mut recipe = BuildingRecipe::cottage();
    let authored_width = recipe.partitions[0].door_width;
    recipe.opening_defaults.door.width = 1.3;
    assert_eq!(
        openings::resolve(&recipe).unwrap().partitions[0].door_width,
        authored_width
    );
    recipe.partitions[0].use_default_door_dimensions = true;
    let resolved = openings::resolve(&recipe).unwrap();
    assert_eq!(resolved.partitions[0].door_width, 1.3);
    assert_eq!(recipe.partitions[0].door_width, authored_width);
    validate(&recipe).unwrap();
    recipe.partitions[0].use_default_door_dimensions = false;
    assert_eq!(
        openings::resolve(&recipe).unwrap().partitions[0].door_width,
        authored_width
    );
}
