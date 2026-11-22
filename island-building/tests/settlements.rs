use island_building::{BuildingRecipe, MaterialSlot, town::*};
use std::collections::BTreeSet;

fn input(outlets: &[f32]) -> TownInput {
    let heights = (0_u16..126)
        .flat_map(|row| {
            let y = 2. * f32::from(row) - 20.;
            std::iter::repeat_n((0.4 * y).clamp(-2., 6.), 201)
        })
        .collect();
    TownInput {
        tile_seed: 543,
        sea_level: 0.,
        terrain: HeightGrid {
            origin: [-200., -20.],
            spacing: 2.,
            width: 201,
            height: 126,
            heights,
        },
        rivers: outlets
            .iter()
            .map(|&x| RiverPolyline {
                points: vec![[x, 200.], [x, -8.]],
                widths: vec![4., 4.],
                join: None,
            })
            .collect(),
        bridge_exclusions: Vec::new(),
        disable_bridges: false,
        protected_terrain: Vec::new(),
    }
}

fn entry(id: &str) -> TownCatalogueEntry {
    let mut building = BuildingRecipe::hut();
    building.atlas_size = 128;
    TownCatalogueEntry {
        id: id.into(),
        building,
        placement: TownPlacement::Other,
        min_count: 1,
        max_count: 1,
    }
}

fn recipe(catalogue: Vec<TownCatalogueEntry>) -> TownRecipe {
    TownRecipe {
        schema_version: TOWN_SCHEMA_VERSION,
        catalogue,
        settings: TownSettings {
            site_search_radius: 64.,
            ..TownSettings::default()
        },
    }
}

#[test]
fn middle_of_the_coast_uses_actual_outlets_instead_of_longest_source_or_median_rank() {
    let mut input = input(&[-140., 140., -30., -110., -90., -29.9]);
    input.rivers.push(RiverPolyline {
        points: vec![[156., 180.], [-137., 163.]],
        widths: Vec::new(),
        join: Some(0),
    });
    input.rivers.push(RiverPolyline {
        points: vec![[125., 180.], [143., -2.]],
        widths: Vec::new(),
        join: Some(1),
    });
    input.rivers.push(RiverPolyline {
        points: vec![[24., 155.], [35., 114.]],
        widths: Vec::new(),
        join: None,
    });
    let empty = recipe(Vec::new());
    let output = generate_settlements(&empty, &empty, &input).unwrap();
    assert_eq!(
        output
            .settlements
            .iter()
            .map(|site| site.river_index)
            .collect::<Vec<_>>(),
        [2, 0, 3, 4, 1],
    );
    assert_eq!(
        output.mouth.unwrap().position.map(f32::to_bits),
        output.settlements[0].mouth.map(f32::to_bits),
    );
    assert_eq!(output.settlements[0].kind, TownSettlementKind::Capital);
    assert!(
        output.settlements[1..]
            .iter()
            .all(|site| site.kind == TownSettlementKind::Village)
    );
    assert!(output.settlements.iter().all(|site| site.centre.is_none()));
    let legacy = generate_town(&empty, &input).unwrap();
    assert_eq!(legacy.mouth.unwrap().river_index, 6);
    assert_eq!(legacy.settlements.len(), 1);
}

#[test]
fn effective_prototypes_are_shared_across_catalogue_aliases_and_villages() {
    let mut capital_alias = entry("capital-cottage-alias");
    capital_alias.building.materials.foundation_recipe = Some("unused-stone.json".into());
    let capital = recipe(vec![entry("capital-cottage"), capital_alias]);
    let mut different = entry("capital-cottage");
    different.building.seed ^= 1;
    let village = recipe(vec![entry("village-cottage"), different]);
    let input = input(&[-135., 0., 140.]);
    let output = generate_settlements(&capital, &village, &input).unwrap();
    assert_eq!(output.instances.len(), 6);
    assert_eq!(output.skipped_count, 0);
    assert_eq!(output.prototypes.len(), 2);
    assert!(
        output
            .settlements
            .iter()
            .all(|site| site.building_count == 2)
    );
    for instance in &output.instances {
        let seed = output.prototypes[instance.prototype_index]
            .building
            .recipe
            .seed;
        if instance.settlement_index != 0 && instance.entry_id == "capital-cottage" {
            assert_eq!(seed, village.catalogue[1].building.seed);
            assert_eq!(instance.prototype_index, 1);
        } else {
            assert_eq!(seed, capital.catalogue[0].building.seed);
            assert_eq!(instance.prototype_index, 0);
        }
    }
    assert_eq!(
        output.support_material.unwrap().slot,
        MaterialSlot::Foundation
    );
    assert_eq!(
        output.supports_grub_positions.len(),
        output.supports.positions.len()
    );
    assert!(
        output
            .supports
            .indices
            .iter()
            .all(|&i| (i as usize) < output.supports.positions.len())
    );
    assert_eq!(output.support_colliders.len(), 6);
    assert_eq!(output.clearance.len(), 6);
    for (instance, clearance) in output.instances.iter().zip(&output.clearance) {
        assert!(clearance.contains([instance.position[0], instance.position[1]], 0.));
    }
    let legacy = generate_town(&capital, &input).unwrap();
    assert_eq!(legacy.prototypes.len(), 2);
    assert_ne!(
        legacy.prototypes[0].building.recipe.seed,
        legacy.prototypes[1].building.recipe.seed
    );
}

#[test]
fn nearby_sites_keep_road_and_building_clearance_and_valid_graded_frontage() {
    let input = input(&[-42., 0., 42.]);
    let mut capital = recipe(vec![entry("central")]);
    // Keep the fixture's 4m paved rings and 2m shoulders for the minimum field size.
    capital.settings.roads = Some(TownRoadSettings {
        half_width: 8. / 3.,
        access_half_width: 4.5,
        shoulder: 3.,
        ..TownRoadSettings::default()
    });
    let mut village = recipe(vec![entry("cottage")]);
    village.settings.roads = Some(TownRoadSettings {
        ring_spacing: 16.,
        half_width: 2.,
        access_half_width: 4.5,
        ..TownRoadSettings::default()
    });
    let solo = generate_settlements(&capital, &recipe(Vec::new()), &input).unwrap();
    let output = generate_settlements(&capital, &village, &input).unwrap();
    assert!(output.clearance.starts_with(&solo.clearance));
    assert!(output.road_exits.starts_with(&solo.road_exits));
    assert!(
        output
            .road_exits
            .iter()
            .any(|exit| exit.settlement_index > 0)
    );
    assert!(
        output
            .road_exits
            .windows(2)
            .all(|pair| pair[0].settlement_index <= pair[1].settlement_index)
    );
    assert_eq!(output.routing_blockers.len(), output.instances.len() * 2);
    for blocker in &output.routing_blockers {
        assert!(output.clearance.contains(blocker));
    }
    assert!(output.clearance.len() > output.routing_blockers.len());
    assert_eq!(output.instances.len(), 3);
    let roads = output.roads.as_ref().unwrap();
    roads.validate().unwrap();
    assert_eq!(
        roads
            .strips
            .iter()
            .map(|strip| strip.source)
            .collect::<BTreeSet<_>>()
            .len(),
        roads.strips.len()
    );
    let previous = solo.roads.unwrap();
    let first_node = roads.strips[previous.strips.len()..]
        .iter()
        .map(|strip| strip.start.min(strip.end))
        .min()
        .unwrap();
    assert!(
        previous
            .strips
            .iter()
            .all(|strip| strip.start < first_node && strip.end < first_node)
    );
    for instance in &output.instances {
        let [x, y, height] = instance.frontage.as_ref().unwrap().street_edge;
        assert!((roads.surface_height(motu::Vec2::new(x, y)).unwrap() - height).abs() < 0.001);
    }
    for village_rect in &output.clearance[solo.clearance.len()..] {
        assert!(
            !solo
                .clearance
                .iter()
                .any(|area| overlaps(area, village_rect))
        );
    }

    assert!(!solo.wheat_fields.is_empty());
    assert!(!output.wheat_fields.is_empty());
    for (i, field) in output.wheat_fields.iter().enumerate() {
        assert!(field.half_extents[0] >= 4.5 && field.half_extents[1] >= 3.);
        assert!(!output.clearance.iter().any(|area| overlaps(area, field)));
        assert!(
            !output.wheat_fields[..i]
                .iter()
                .any(|other| overlaps(other, field))
        );
        let heading = motu::Vec2::from_angle(field.yaw);
        assert!(roads.strips.iter().any(|strip| {
            let first = strip.stations.first().unwrap().point;
            let last = strip.stations.last().unwrap().point;
            heading.dot((last - first).normalize()).abs() > 0.9999
                && motu::Vec2::from_array(field.centre).distance((first + last) * 0.5) < 30.
        }));
    }
}

fn overlaps(a: &ClearanceRect, b: &ClearanceRect) -> bool {
    use motu::Vec2;
    let axes = [a, b].map(|rect| Vec2::from_angle(rect.yaw));
    let separation = Vec2::from_array(a.centre) - Vec2::from_array(b.centre);
    axes.into_iter().chain(axes.map(Vec2::perp)).all(|axis| {
        let radius = [a, b]
            .iter()
            .zip(axes)
            .map(|(rect, forward)| {
                rect.half_extents[0] * axis.dot(forward).abs()
                    + rect.half_extents[1] * axis.dot(forward.perp()).abs()
            })
            .sum::<f32>();
        axis.dot(separation).abs() < radius
    })
}
