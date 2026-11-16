use island_building::{BuildingRecipe, MaterialSlot, generate};

#[test]
fn foundation_hides_ground_underside_but_upper_floor_and_ceiling_sides_remain() {
    let recipe = BuildingRecipe {
        storeys: 2,
        atlas_size: 128,
        ..BuildingRecipe::procedural(7., 6.)
    };
    let prototype = generate(&recipe).unwrap();
    let near = &prototype.lods[0];
    let floor = &near
        .parts
        .iter()
        .find(|p| p.material == MaterialSlot::Floor)
        .unwrap()
        .mesh;
    assert!(floor.normals.iter().all(|n| n[2] >= -0.5));
    let foundation = island_building::geometry::foundation::bounds(&recipe);
    assert!(prototype.colliders.contains(&foundation));
    assert!(floor.positions.iter().all(|p| {
        p[2] > 0.
            || (0..3).all(|axis| p[axis] >= foundation.min[axis] && p[axis] <= foundation.max[axis])
    }));
    let ceiling = &near
        .parts
        .iter()
        .find(|p| p.material == MaterialSlot::Ceiling)
        .unwrap()
        .mesh;
    let underside = recipe.floor_height(1) - recipe.floor_thickness;
    assert!(
        ceiling
            .positions
            .iter()
            .zip(&ceiling.normals)
            .any(|(p, n)| { (p[2] - underside).abs() < 0.00001 && n[2] < -0.5 })
    );
    assert!(
        ceiling.normals.iter().all(|n| n[2] < 0.5),
        "only roof-level ceiling tops are enclosed"
    );
}

#[test]
fn hideable_stairs_do_not_change_permanent_foundation_or_moving_doors() {
    let mut recipe = BuildingRecipe {
        atlas_size: 128,
        ..BuildingRecipe::hut()
    };
    let stairs = generate(&recipe).unwrap();
    recipe.foundation.stairs = false;
    let hidden = generate(&recipe).unwrap();
    for (visible, hidden) in stairs.lods.iter().zip(&hidden.lods) {
        for slot in [
            MaterialSlot::Foundation,
            MaterialSlot::Floor,
            MaterialSlot::Ceiling,
        ] {
            assert_eq!(part(visible, slot), part(hidden, slot));
        }
    }
    assert_eq!(stairs.doors, hidden.doors);
}

fn part(
    lod: &island_building::BuildingLod,
    slot: MaterialSlot,
) -> Option<&island_building::BuildingMesh> {
    lod.parts
        .iter()
        .find(|p| p.material == slot)
        .map(|p| &p.mesh)
}
