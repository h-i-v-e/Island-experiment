#![allow(clippy::float_cmp)] // Authored elevations and compatibility defaults.
use island_building::{
    BuildingRecipe, MaterialSlot, OpeningKind, StairAxis, generate, openings, validate,
};

fn building(storeys: u8) -> BuildingRecipe {
    BuildingRecipe {
        storeys,
        atlas_size: 128,
        ..BuildingRecipe::procedural(7., 6.)
    }
}

#[test]
fn legacy_recipes_default_to_one_storey_and_have_no_internal_stairs() {
    let mut json = serde_json::to_value(BuildingRecipe::hut()).unwrap();
    json.as_object_mut().unwrap().remove("storeys");
    json.as_object_mut().unwrap().remove("staircase");
    for o in json["openings"].as_array_mut().unwrap() {
        o.as_object_mut().unwrap().remove("storey");
    }
    let recipe: BuildingRecipe = serde_json::from_value(json).unwrap();
    assert_eq!(recipe.storeys, 1);
    assert!(recipe.openings.iter().all(|o| o.storey == 0));
    let prototype = generate(&BuildingRecipe {
        atlas_size: 128,
        ..recipe
    })
    .unwrap();
    assert!(prototype.layout.staircase.is_none());
    assert!(prototype.support_colliders.is_empty());
}

#[test]
fn added_storeys_have_distinct_windows_floor_landings_and_only_one_roof() {
    for storeys in [2, 3, 6] {
        let recipe = building(storeys);
        let prototype = generate(&recipe).unwrap();
        let stairs = prototype.layout.staircase.as_ref().unwrap();
        assert_eq!(stairs.landings.len(), usize::from(storeys));
        assert_eq!(stairs.turn_landings.len(), usize::from(storeys - 1));
        assert!(stairs.riser <= recipe.staircase.target_riser);
        for level in 0..storeys {
            assert_eq!(
                stairs.landings[usize::from(level)].max[2],
                recipe.floor_height(level)
            );
            assert!(
                prototype
                    .recipe
                    .openings
                    .iter()
                    .any(|o| o.kind == OpeningKind::Window && o.storey == level)
            );
        }
        assert!(
            prototype
                .recipe
                .openings
                .iter()
                .filter(|o| o.kind == OpeningKind::Door)
                .all(|o| o.storey == 0)
        );
        for lod in &prototype.lods {
            let roof = lod
                .parts
                .iter()
                .find(|p| p.material == MaterialSlot::Roof || p.material == MaterialSlot::Atlas)
                .unwrap();
            assert!(
                roof.mesh
                    .positions
                    .iter()
                    .any(|p| p[2] >= recipe.eaves_height())
            );
        }
        assert_eq!(prototype.layout.room_count, usize::from(storeys) + 1);
        assert!(prototype.layout.bounds_max[2] > recipe.eaves_height());
    }
}

#[test]
fn automatic_windows_and_overrides_address_upper_storeys_without_moving_ground_ids() {
    let mut recipe = building(3);
    recipe
        .opening_generation
        .as_mut()
        .unwrap()
        .overrides
        .push(island_building::OpeningOverride {
            id: "auto-window-east-0-storey-2".into(),
            width: Some(1.2),
            ..Default::default()
        });
    let resolved = openings::resolve(&recipe).unwrap();
    let window = resolved
        .openings
        .iter()
        .find(|o| o.id == "auto-window-east-0-storey-2")
        .unwrap();
    assert_eq!(window.width, 1.2);
    assert_eq!(window.storey, 2);
    assert!(resolved.openings.iter().any(|o| o.id == "auto-door-south"));
    let json = serde_json::to_string(&recipe).unwrap();
    assert_eq!(
        serde_json::from_str::<BuildingRecipe>(&json).unwrap(),
        recipe
    );
}

#[test]
fn stairwells_cut_floors_and_joists_but_floor_landings_remain_supported() {
    let mut recipe = building(3);
    recipe.walls.style = island_building::WallStyle::WattleAndDaub;
    let prototype = generate(&recipe).unwrap();
    let stairs = prototype.layout.staircase.as_ref().unwrap();
    for level in 1..recipe.storeys {
        let floor = recipe.floor_height(level);
        let hole = stairs.point(
            stairs.width * 0.5,
            stairs.landing_depth + stairs.run() * 0.5,
            floor,
        );
        assert!(
            !prototype
                .support_colliders
                .iter()
                .any(|b| (b.max[2] - floor).abs() < 0.00001 && b.contains(hole))
        );
        let landing = stairs.point(stairs.width * 0.5, stairs.landing_depth * 0.5, floor);
        assert!(
            prototype
                .support_colliders
                .iter()
                .any(|b| b.contains(landing))
        );
        let width = recipe.walls.beam_width;
        let under_floor = floor - recipe.floor_thickness - width * 0.5;
        for [u, v, z] in [
            [-width * 0.5, stairs.length() * 0.5, under_floor],
            [
                stairs.span() + width * 0.5,
                stairs.length() * 0.5,
                under_floor,
            ],
            [
                stairs.span() * 0.5,
                stairs.length() + width * 0.5,
                under_floor,
            ],
            [
                stairs.span() * 0.5,
                stairs.landing_depth - width * 0.5,
                floor - 0.09,
            ],
        ] {
            let point = stairs.point(u, v, z);
            assert!(
                prototype.colliders.iter().any(|b| b.contains(point)),
                "solid opening frame at {point:?}"
            );
        }
    }
    let timber = &prototype.lods[0]
        .parts
        .iter()
        .find(|p| p.material == MaterialSlot::Timber)
        .unwrap()
        .mesh;
    assert!(timber.positions.len() > 500);
    assert!(prototype.lods[1].parts.iter().all(|p| !matches!(
        p.material,
        MaterialSlot::Floor | MaterialSlot::Ceiling | MaterialSlot::Interior
    )));
}

#[test]
fn small_footprints_conflicting_positions_and_out_of_range_storeys_are_diagnosed() {
    let mut recipe = building(2);
    recipe.width = 3.;
    recipe.depth = 3.;
    recipe.chimney = None;
    assert!(
        validate(&recipe)
            .unwrap_err()
            .to_string()
            .contains("staircase")
    );
    recipe.width = 6.;
    recipe.depth = 5.;
    recipe.staircase.position = Some([100., 0.]);
    assert!(
        validate(&recipe)
            .unwrap_err()
            .to_string()
            .contains("staircase")
    );
    recipe.staircase.position = None;
    recipe.storeys = 0;
    assert!(
        validate(&recipe)
            .unwrap_err()
            .to_string()
            .contains("storeys")
    );
    recipe.storeys = 2;
    recipe.opening_generation = None;
    recipe.openings = BuildingRecipe::hut().openings;
    recipe.openings[1].storey = 2;
    assert!(
        validate(&recipe)
            .unwrap_err()
            .to_string()
            .contains("opening storey")
    );
}

#[cfg(feature = "bevy")]
#[test]
fn player_capsule_climbs_and_descends_every_flight_at_all_lods_with_entrance_stairs_hidden() {
    walk(&building(3));
}

#[cfg(feature = "bevy")]
#[test]
fn capsule_routes_remain_clear_with_thatch_timber_low_ceilings_and_the_other_axis() {
    let mut recipe = building(3);
    recipe.wall_height = 2.4;
    recipe.walls.style = island_building::WallStyle::WattleAndDaub;
    recipe.roof.covering = island_building::RoofCovering::Thatch;
    walk(&recipe);
    recipe.chimney = None;
    recipe.staircase.axis = StairAxis::X;
    walk(&recipe);
    let mut compact = BuildingRecipe::procedural(6., 5.966_667);
    compact.storeys = 3;
    compact.wall_height = 2.4;
    compact.atlas_size = 128;
    compact.walls.style = island_building::WallStyle::WattleAndDaub;
    compact.roof.covering = island_building::RoofCovering::Thatch;
    compact.opening_defaults.window.width = 0.6;
    compact
        .opening_generation
        .as_mut()
        .unwrap()
        .overrides
        .push(island_building::OpeningOverride {
            id: "auto-door-south".into(),
            offset: Some(0.65),
            ..Default::default()
        });
    walk(&compact);
}

#[cfg(feature = "bevy")]
fn walk(recipe: &BuildingRecipe) {
    use bevy::prelude::*;
    use island_building::{
        interaction::BuildingInstance,
        render::{bevy_to_native, compile_building, native_to_bevy},
    };
    let source = std::sync::Arc::new(generate(recipe).unwrap());
    let stairs = source.layout.staircase.clone().unwrap();
    let compiled = compile_building(
        source,
        &mut Assets::default(),
        &mut Assets::default(),
        &mut Assets::default(),
    );
    let mut instance = BuildingInstance::new(compiled, "stairs-review".into());
    instance.stairs_visible = false;
    let root = GlobalTransform::IDENTITY;
    let mut path = Vec::new();
    let lane = stairs.width * 0.5;
    let other = stairs.width + island_building::stairs::FLIGHT_GAP + lane;
    for level in 0..2 {
        let z = stairs.floor_heights[level];
        let middle = stairs.landing_depth + stairs.run() + stairs.landing_depth * 0.5;
        let points = [
            [lane, stairs.landing_depth * 0.5],
            [lane, middle],
            [other, middle],
            [other, stairs.landing_depth * 0.5],
        ];
        for pair in points.windows(2) {
            for step in 0..=120_u16 {
                let t = f32::from(step) / 120.;
                path.push(stairs.point(
                    pair[0][0] + (pair[1][0] - pair[0][0]) * t,
                    pair[0][1] + (pair[1][1] - pair[0][1]) * t,
                    z,
                ));
            }
        }
        // Walk across the floor landing into the next ascending lane.
        if level == 0 {
            for step in 0..=60_u16 {
                path.push(stairs.point(
                    other + (lane - other) * f32::from(step) / 60.,
                    stairs.landing_depth * 0.5,
                    stairs.floor_heights[1],
                ));
            }
        }
    }
    for lod in 0..3 {
        instance.current_lod = lod;
        let mut feet = path[0];
        feet[2] = 0.025;
        for point in path.iter().chain(path.iter().rev()) {
            let proposed = native_to_bevy([point[0], point[1], feet[2]]);
            let support = instance
                .support_capsule_point(&root, proposed, 0.26, 0.28)
                .expect("stairs and landings support the capsule");
            let next = support + Vec3::Y * 0.025;
            assert!(
                !instance.blocks_capsule(&root, next, 0.28, 1.8),
                "LOD{lod}: {:?} -> {:?}",
                feet,
                bevy_to_native(next)
            );
            feet = bevy_to_native(next);
        }
        assert!((feet[2] - 0.025).abs() < 0.0001);
    }
}

#[test]
fn alternate_axis_and_partitioned_cottages_can_place_the_staircase() {
    let mut recipe = BuildingRecipe::cottage();
    recipe.storeys = 2;
    recipe.atlas_size = 128;
    recipe.opening_generation = Some(island_building::OpeningGeneration::default());
    validate(&recipe).unwrap();
    recipe.partitions.clear();
    recipe.chimney = None;
    recipe.staircase.axis = StairAxis::X;
    let prototype = generate(&recipe).unwrap();
    assert_eq!(prototype.layout.staircase.unwrap().axis, StairAxis::X);
}

#[test]
fn portable_packages_preserve_floor_support_stair_metadata_and_regeneration() {
    let recipe = building(3);
    let prototype = generate(&recipe).unwrap();
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path =
        std::env::temp_dir().join(format!("building-storeys-{}-{nonce}", std::process::id()));
    let manifest =
        island_building::export::export_package(&prototype, &path, std::path::Path::new("."))
            .unwrap();
    assert!(
        manifest
            .files
            .iter()
            .any(|f| f.path == "support-collision.json")
    );
    let supports: Vec<island_building::collision::BoxCollider> =
        serde_json::from_slice(&std::fs::read(path.join("support-collision.json")).unwrap())
            .unwrap();
    assert_eq!(supports, prototype.support_colliders);
    let layout: island_building::layout::BuildingLayout =
        serde_json::from_slice(&std::fs::read(path.join("layout.json")).unwrap()).unwrap();
    assert_eq!(layout, prototype.layout);
    let restored: BuildingRecipe =
        serde_json::from_slice(&std::fs::read(path.join("recipe.json")).unwrap()).unwrap();
    let rebuilt = island_building::generate_with_base(&restored, &path).unwrap();
    assert_eq!(rebuilt.hash, prototype.hash);
    assert_eq!(rebuilt.support_colliders, prototype.support_colliders);
    std::fs::remove_dir_all(path).unwrap();
}
