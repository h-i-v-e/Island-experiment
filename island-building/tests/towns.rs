use island_building::{BuildingRecipe, MaterialSlot, town::*};

#[allow(clippy::cast_precision_loss)] // Fixture coordinates are bounded by 201 cells.
fn terrain(height: impl Fn(f32, f32) -> f32) -> HeightGrid {
    let origin = [-200., -100.];
    let spacing = 2.;
    let width = 201;
    let depth = 201;
    let mut heights = Vec::new();
    for y in 0..depth {
        for x in 0..width {
            heights.push(height(
                origin[0] + x as f32 * spacing,
                origin[1] + y as f32 * spacing,
            ));
        }
    }
    HeightGrid {
        origin,
        spacing,
        width,
        height: depth,
        heights,
    }
}
fn input() -> TownInput {
    TownInput {
        tile_seed: 543,
        sea_level: 0.,
        terrain: terrain(|_, y| (y * 0.2).clamp(-2., 4.)),
        rivers: vec![RiverPolyline {
            join: None,
            points: vec![[0., 150.], [0., -5.]],
            widths: vec![4., 6.],
        }],
    }
}
fn entry(id: &str, count: u32, placement: TownPlacement) -> TownCatalogueEntry {
    let mut building = BuildingRecipe::hut();
    building.atlas_size = 128;
    TownCatalogueEntry {
        id: id.into(),
        building,
        min_count: count,
        max_count: count,
        placement,
    }
}
fn recipe() -> TownRecipe {
    TownRecipe {
        schema_version: 1,
        catalogue: vec![
            entry("house", 3, TownPlacement::Other),
            entry("centre", 1, TownPlacement::Centre),
        ],
        settings: TownSettings::default(),
    }
}

#[test]
fn deterministic_catalogue_counts_and_centre_priority() {
    let recipe = recipe();
    let input = input();
    let a = generate_town(&recipe, &input).unwrap();
    let b = generate_town(&recipe, &input).unwrap();
    assert!(a == b, "town generation must be deterministic");
    assert_eq!(a.instances.len(), 4);
    assert_eq!(a.skipped_count, 0);
    assert_eq!(a.prototypes.len(), 2);
    assert_eq!(a.prototypes[0].entry_id, "centre");
    assert_eq!(
        a.centre,
        Some([a.instances[0].position[0], a.instances[0].position[1]])
    );
    assert_eq!(
        a.instances
            .iter()
            .filter(|i| i.prototype_index == 1)
            .count(),
        3
    );
    assert_eq!(
        a.prototypes[1].building.recipe.seed,
        b.prototypes[1].building.recipe.seed
    );
    let mut more = recipe.clone();
    more.catalogue[0].min_count = 4;
    more.catalogue[0].max_count = 4;
    let changed = generate_town(&more, &input).unwrap();
    assert!(
        a.prototypes[1].building == changed.prototypes[1].building,
        "changing copy count must preserve the shared building variant"
    );
}

#[test]
fn longest_head_to_sea_route_counts_shared_tail_once() {
    let mut input = input();
    // Main is 100m to coast; tributary is 120m to join + 50m tail.
    input.rivers = vec![
        RiverPolyline {
            join: None,
            points: vec![[0., 100.], [0., 50.], [0., -5.]],
            widths: vec![],
        },
        RiverPolyline {
            join: None,
            points: vec![[-120., 50.], [0., 50.]],
            widths: vec![],
        },
        RiverPolyline {
            join: None,
            points: vec![[80., 60.], [80., -5.]],
            widths: vec![],
        },
    ];
    let empty = TownRecipe {
        catalogue: vec![],
        ..recipe()
    };
    let town = generate_town(&empty, &input).unwrap();
    let mouth = town.mouth.unwrap();
    assert_eq!(mouth.river_index, 1);
    assert!((mouth.river_length - 170.).abs() < 0.01, "{mouth:?}");
    assert!(mouth.position[0].abs() < 0.01);
    assert!(
        input
            .terrain
            .sample(mouth.position[0], mouth.position[1])
            .unwrap()
            <= 0.001
    );
}

#[test]
fn no_river_and_inland_endings_have_no_town() {
    let mut input = input();
    input.rivers.clear();
    let output = generate_town(&recipe(), &input).unwrap();
    assert!(output.instances.is_empty() && output.mouth.is_none());
    input.rivers.push(RiverPolyline {
        join: None,
        points: vec![[0., 150.], [0., 50.]],
        widths: vec![],
    });
    assert!(generate_town(&recipe(), &input).unwrap().mouth.is_none());
}

#[test]
fn steep_banks_skip_instead_of_flattening_terrain() {
    let mut input = input();
    input.terrain = terrain(|x, y| {
        if y <= 0. {
            y * 0.2
        } else {
            y * 0.6 + x.abs() * 0.4
        }
    });
    let mut recipe = recipe();
    recipe.settings.attempts_per_copy = 4;
    let output = generate_town(&recipe, &input).unwrap();
    assert!(output.instances.is_empty());
    assert_eq!(output.skipped_count, 4);
    assert!(output.supports.positions.is_empty());
}

#[test]
fn clears_shells_and_entrances_and_combines_locally_weathered_supports() {
    let mut input = input();
    input.terrain = terrain(|x, y| (y * 0.2).clamp(-2., 4.) + if y > 25. { x * 0.015 } else { 0. });
    let output = generate_town(&recipe(), &input).unwrap();
    assert!(!output.instances.is_empty());
    assert_eq!(output.clearance.len(), output.instances.len());
    assert_eq!(
        output.supports.positions.len(),
        output.supports_grub_positions.len()
    );
    assert!(!output.supports.indices.is_empty());
    assert_eq!(
        output.support_material.as_ref().unwrap().slot,
        MaterialSlot::Foundation
    );
    assert_eq!(output.support_material.as_ref().unwrap().size, 512);
    for (instance, clearance) in output.instances.iter().zip(&output.clearance) {
        assert!(clearance.contains([instance.position[0], instance.position[1]], 0.));
        let shell = &output.prototypes[instance.prototype_index].building;
        assert!(
            shell
                .lods
                .iter()
                .all(|lod| lod.parts.iter().all(|part| !part.material.is_foundation()))
        );
        assert!(shell.stair_colliders.is_empty());
        assert!(
            shell
                .materials
                .iter()
                .all(|material| material.slot != MaterialSlot::Foundation)
        );
    }
    assert!(output.supports_grub_positions.iter().all(|p| p[2] < 1.));
    assert!(output.supports.positions.iter().any(|p| p[2] > 3.));
    assert!(!output.support_colliders.is_empty());
}

#[test]
fn validates_before_search_and_honours_cancellation() {
    let mut input = input();
    input.terrain.heights.pop();
    assert!(matches!(
        generate_town(&recipe(), &input),
        Err(TownError::InvalidInput(_))
    ));
    let input = self::input();
    assert!(matches!(
        generate_town_with_base(&recipe(), &input, std::path::Path::new("."), &|| true),
        Err(TownError::Cancelled)
    ));
}

#[test]
fn grid_sampling_is_bilinear_and_never_extrapolates() {
    let grid = HeightGrid {
        origin: [10., 20.],
        spacing: 2.,
        width: 2,
        height: 2,
        heights: vec![0., 2., 4., 6.],
    };
    assert_eq!(grid.sample(11., 21.), Some(3.));
    assert_eq!(grid.sample(12., 22.), Some(6.));
    assert_eq!(grid.sample(9.99, 21.), None);
    assert_eq!(grid.sample(f32::NAN, 21.), None);
}

#[test]
fn default_catalogue_is_portable_and_has_one_central_cathedral() {
    let recipe: TownRecipe =
        serde_json::from_str(include_str!("../recipes/towns/coastal.json")).unwrap();
    let output = generate_town(&recipe, &input()).unwrap();
    assert!(!output.instances.is_empty());
    assert_eq!(output.prototypes[0].entry_id, "cathedral");
    assert_eq!(
        output
            .instances
            .iter()
            .filter(|instance| instance.prototype_index == 0)
            .count(),
        1
    );
    assert!(
        output
            .prototypes
            .iter()
            .all(|prototype| prototype.building.recipe.atlas_size == 128)
    );
}

#[test]
fn authoritative_channel_edge_join_follows_the_shared_main_stem() {
    let mut input = input();
    input.rivers = vec![
        RiverPolyline {
            points: vec![[0., 100.], [0., 50.], [0., -5.]],
            widths: vec![],
            join: None,
        },
        RiverPolyline {
            points: vec![[-120., 53.], [-3., 53.]],
            widths: vec![],
            join: Some(0),
        },
    ];
    let recipe = TownRecipe {
        catalogue: vec![],
        ..recipe()
    };
    let mouth = generate_town(&recipe, &input).unwrap().mouth.unwrap();
    assert_eq!(mouth.river_index, 1);
    assert!((mouth.river_length - 170.).abs() < 0.01);
    input.rivers[0].join = Some(1);
    assert!(matches!(
        generate_town(&recipe, &input),
        Err(TownError::InvalidInput(_))
    ));
}

#[test]
fn mouth_tangent_does_not_hide_a_dry_downstream_bank() {
    let mut input = input();
    input.terrain = terrain(|x, y| {
        if (-80. ..-10.).contains(&x) && (-80. ..-8.).contains(&y) {
            4.
        } else if x.abs() < 4. && y > 0. {
            (y * 0.2).min(4.)
        } else {
            -2.
        }
    });
    let recipe = TownRecipe {
        catalogue: vec![entry("bank", 1, TownPlacement::Centre)],
        ..recipe()
    };
    let town = generate_town(&recipe, &input).unwrap();
    assert_eq!(
        town.instances.len(),
        1,
        "the complete dry bank must be searched"
    );
    assert!(town.instances[0].position[0] < -10. && town.instances[0].position[1] < -8.);
}
