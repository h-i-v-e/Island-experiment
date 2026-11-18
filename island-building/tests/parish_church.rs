use island_building::{BuildingRecipe, ChurchTowerStyle, MaterialSlot, generate, validate};

fn recipe() -> BuildingRecipe {
    let mut recipe = BuildingRecipe::parish_church();
    recipe.atlas_size = 128;
    recipe.foundation.stairs = false;
    recipe
}

fn contains(bounds: &island_building::collision::BoxCollider, point: [f32; 3]) -> bool {
    (0..3).all(|axis| point[axis] > bounds.min[axis] && point[axis] < bounds.max[axis])
}

#[test]
fn smaller_cruciform_church_has_one_centred_front_tower_and_connected_floor() {
    let recipe = recipe();
    let cathedral = BuildingRecipe::gothic_church();
    assert!(recipe.width < cathedral.width && recipe.depth < cathedral.depth);
    assert_eq!(recipe.materials, cathedral.materials);
    let prototype = generate(&recipe).unwrap();
    let layout = prototype.layout.church.as_ref().unwrap();
    assert_eq!(layout.towers.len(), 1);
    let tower = layout.towers[0];
    assert!((tower.min[0] + tower.max[0]).abs() < 0.0001);
    assert!((tower.min[1] + recipe.depth * 0.5).abs() < 0.0001);
    assert!((tower.max[1] - layout.nave.min[1]).abs() < 0.0001);
    assert!((tower.max[2] - 14.).abs() < 0.0001);
    for point in [
        [0., -10., -0.1],
        [0., -5., -0.1],
        [6., 3., -0.1],
        [-6., 3., -0.1],
        [0., 10., -0.1],
    ] {
        assert!(
            prototype
                .support_colliders
                .iter()
                .any(|bounds| contains(bounds, point)),
            "missing floor at {point:?}"
        );
    }
    for point in [[5., -5., -0.1], [-5., -5., -0.1], [5., 9., -0.1]] {
        assert!(
            !prototype
                .support_colliders
                .iter()
                .any(|bounds| contains(bounds, point)),
            "filled cruciform recess at {point:?}"
        );
    }
    for index in -21_i16..=21 {
        let point = [0., f32::from(index) * 0.5, 1.6];
        assert!(
            !prototype
                .colliders
                .iter()
                .any(|bounds| contains(bounds, point)),
            "blocked nave or tower at {point:?}"
        );
    }
    for x in -14_i16..=14 {
        let point = [f32::from(x) * 0.5, 3., 1.6];
        assert!(
            !prototype
                .colliders
                .iter()
                .any(|bounds| contains(bounds, point)),
            "blocked transept at {point:?}"
        );
    }
    assert_eq!(prototype.doors.len(), 2);
}

#[test]
fn no_buttresses_or_pier_foundations_survive_in_any_lod_or_collisions() {
    let recipe = recipe();
    let prototype = generate(&recipe).unwrap();
    let nave_half = recipe.church.unwrap().nave_width * 0.5;
    for lod in &prototype.lods {
        for part in &lod.parts {
            for point in &part.mesh.positions {
                // Away from the tower, transepts and chancel, the nave has no outer piers or arches.
                if (-6.5..-0.5).contains(&point[1]) {
                    assert!(
                        point[0].abs() < nave_half + 0.5,
                        "unexpected outer structure {:?}: {point:?}",
                        part.material
                    );
                }
            }
        }
    }
    for bounds in &prototype.colliders {
        if bounds.min[1] > -7. && bounds.max[1] < 0. {
            assert!(bounds.min[0] >= -nave_half - 0.1 && bounds.max[0] <= nave_half + 0.1);
        }
    }
    // Interior Gothic ribs remain even when the exterior buttresses are disabled.
    assert!(
        prototype.lods[0]
            .parts
            .iter()
            .any(|part| part.material == MaterialSlot::Trim
                && part.mesh.positions.iter().any(|point| point[0].abs() < 0.2
                    && point[1] > -7.
                    && point[1] < 0.
                    && point[2] > recipe.wall_height))
    );
}

#[test]
fn roof_and_front_gable_do_not_cut_through_the_tower() {
    let recipe = recipe();
    let prototype = generate(&recipe).unwrap();
    let tower = prototype.layout.church.as_ref().unwrap().towers[0];
    for lod in &prototype.lods[..2] {
        for part in &lod.parts {
            for point in &part.mesh.positions {
                if matches!(part.material, MaterialSlot::Roof | MaterialSlot::Ceiling)
                    && point[2] < tower.max[2] - 0.2
                {
                    assert!(
                        point[1] >= tower.max[1] - 0.001 || point[0].abs() >= tower.max[0] - 0.001,
                        "roof passes through tower: {point:?}"
                    );
                }
            }
        }
    }
}

#[test]
fn authored_recipes_round_trip_and_legacy_cathedral_keeps_its_features() {
    let legacy: BuildingRecipe =
        serde_json::from_str(include_str!("../recipes/gothic-church.json")).unwrap();
    let church = legacy.church.unwrap();
    assert_eq!(church.tower_style, ChurchTowerStyle::Twin);
    assert!(church.flying_buttresses);
    let authored: BuildingRecipe =
        serde_json::from_str(include_str!("../recipes/parish-church.json")).unwrap();
    assert_eq!(authored, BuildingRecipe::parish_church());
    let prototype = generate(&recipe()).unwrap();
    let loaded = serde_json::from_slice(&serde_json::to_vec(&prototype.recipe).unwrap()).unwrap();
    let regenerated = generate(&loaded).unwrap();
    assert_eq!(prototype.hash, regenerated.hash);
    assert_eq!(prototype.layout, regenerated.layout);
    assert_eq!(prototype.lods, regenerated.lods);
    let layout: island_building::ChurchLayout =
        serde_json::from_value(serde_json::to_value(prototype.layout.church.unwrap()).unwrap())
            .unwrap();
    assert_eq!(layout.towers.len(), 1);
}

#[test]
fn invalid_front_tower_and_entrance_proportions_are_rejected() {
    let mut recipe = recipe();
    recipe.church.as_mut().unwrap().tower_width = 6.4;
    assert!(validate(&recipe).is_err());
    let mut recipe = BuildingRecipe::parish_church();
    recipe.opening_defaults.door.width = 3.5;
    assert!(validate(&recipe).is_err());
}

#[test]
// Atlas dimensions are fixed at 512 and all sampled coordinates are bounded.
#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]
fn distant_bake_keeps_the_entrance_width_and_rear_window_above_the_nave_roof() {
    let mut recipe = recipe();
    recipe.atlas_size = 512;
    let prototype = generate(&recipe).unwrap();
    let church = recipe.church.unwrap();
    let atlas = &prototype.atlas;
    let size = atlas.size as usize;
    let mut rear_glass = 0;
    for y in size / 2..size {
        for x in size / 4..size / 2 {
            let index = (y * size + x) * 4;
            if atlas.albedo[index..index + 4] == [47, 70, 78, 255] {
                rear_glass += 1;
                let v = ((0.95 - (y as f32 / size as f32 * 2. - 1.)) / 0.9).clamp(0., 1.);
                let height = v * (church.tower_height - recipe.wall_height);
                assert!(
                    height >= recipe.roof.rise + 0.2,
                    "distant rear window below nave ridge: {height}"
                );
            }
        }
    }
    assert!(
        rear_glass > 0,
        "rear belfry window is missing from its bake"
    );
    let timber = recipe.materials.door.map(|value| {
        let encoded = if value <= 0.003_130_8 {
            value * 12.92
        } else {
            1.055 * value.powf(1. / 2.4) - 0.055
        };
        (encoded * 255.).round() as u8
    });
    let u = 0.5 + recipe.opening_defaults.door.width * 0.4 / church.tower_width;
    let x = ((3. + 0.05 + 0.9 * u) * size as f32 * 0.25) as usize;
    let y = ((0.95 - 0.9 * 0.15) * size as f32 * 0.5) as usize;
    let index = (y * size + x) * 4;
    assert_eq!(
        &atlas.albedo[index..index + 3],
        &timber,
        "distant entrance is narrower than the detailed door"
    );
}
