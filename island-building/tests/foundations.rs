#![allow(clippy::float_cmp)] // Exact authored floor elevation and serialized defaults.
use island_building::{
    BuildingRecipe, Facade, MaterialSlot, StairRun, generate, geometry::foundation,
};

#[test]
fn foundation_bottom_stays_buried_with_a_high_point_anywhere_in_the_footprint() {
    for (width, depth) in [(3., 3.), (6., 5.), (40., 3.), (40., 40.)] {
        let recipe = BuildingRecipe {
            width,
            depth,
            ..BuildingRecipe::hut()
        };
        let bounds = foundation::bounds(&recipe);
        assert!(bounds.max[0] > width * 0.5 && bounds.max[1] > depth * 0.5);
        assert_eq!(bounds.max[2], 0.);
        for ix in 0_u16..=4 {
            for iy in 0_u16..=4 {
                let highest = [
                    bounds.min[0] + f32::from(ix) / 4. * bounds.max[0] * 2.,
                    bounds.min[1] + f32::from(iy) / 4. * bounds.max[1] * 2.,
                ];
                for x in [bounds.min[0], bounds.max[0]] {
                    for y in [bounds.min[1], bounds.max[1]] {
                        // A cone bounds every terrain surface with gradient <= tan(30°),
                        // including a high point inside rather than only at a corner.
                        let terrain =
                            -(x - highest[0]).hypot(y - highest[1]) * 30_f32.to_radians().tan();
                        assert!(
                            bounds.min[2] <= terrain - recipe.foundation.burial_depth + 0.00001
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn stairs_reach_downhill_ground_beyond_every_facade_and_foundation_corner() {
    for (width, depth) in [(6., 5.), (40., 3.), (40., 40.)] {
        for facade in Facade::ALL {
            for offset in [-0.5, 0., 0.5] {
                for orientation in [StairRun::Outward, StairRun::AlongWall] {
                    let mut recipe = BuildingRecipe {
                        width,
                        depth,
                        ..BuildingRecipe::hut()
                    };
                    recipe.foundation.stair_run = orientation;
                    recipe.openings[0].facade = facade;
                    recipe.openings[0].offset = offset;
                    let stairs = foundation::stairs(&recipe).next().unwrap();
                    let bounds = foundation::bounds(&recipe);
                    let colliders = foundation::stair_colliders(&recipe);
                    let toe = colliders.last().unwrap();
                    assert!(stairs.steps < 1000);
                    for heading in 0_u16..360 {
                        let (sx, sy) = f32::from(heading).to_radians().sin_cos();
                        let peak = sx.abs() * bounds.max[0] + sy.abs() * bounds.max[1];
                        for x in [toe.min[0], toe.max[0]] {
                            for y in [toe.min[1], toe.max[1]] {
                                let ground = (sx * x + sy * y - peak) * 30_f32.to_radians().tan();
                                assert!(
                                    toe.max[2] <= ground - recipe.foundation.burial_depth + 0.0001,
                                    "{facade:?} {orientation:?} heading {heading}"
                                );
                            }
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn along_wall_stairs_turn_toward_the_longer_edge_and_stay_outside_the_plinth() {
    for facade in Facade::ALL {
        for offset in [-1., 0., 1.] {
            let mut recipe = BuildingRecipe::hut();
            recipe.foundation.stair_run = StairRun::AlongWall;
            recipe.openings[0].facade = facade;
            recipe.openings[0].offset = offset;
            let stairs = foundation::stairs(&recipe).next().unwrap();
            let tangent = facade.tangent();
            let sign = if offset > 0. { -1. } else { 1. };
            assert_eq!(stairs.direction, [tangent[0] * sign, tangent[1] * sign]);
            let door = facade.point(&recipe, offset, stairs.landing_height, 0.);
            assert!(stairs.landing_bounds.contains(door));
            let normal = facade.normal();
            let outer_wall = facade.point(&recipe, 0., 0., -recipe.foundation.outset);
            let shapes = foundation::stair_colliders(&recipe);
            let first = &shapes[1];
            assert!(
                (0..2).any(
                    |axis| (first.min[axis] - stairs.landing_bounds.max[axis]).abs() < 0.00001
                        || (first.max[axis] - stairs.landing_bounds.min[axis]).abs() < 0.00001
                ),
                "flight meets landing"
            );
            for step in &shapes[1..] {
                for x in [step.min[0], step.max[0]] {
                    for y in [step.min[1], step.max[1]] {
                        assert!(
                            (x - outer_wall[0]) * normal[0] + (y - outer_wall[1]) * normal[1]
                                >= -0.00001,
                            "steps never enter the foundation or house"
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn stair_direction_is_backward_compatible_and_round_trips_with_the_recipe() {
    let mut value = serde_json::to_value(BuildingRecipe::hut()).unwrap();
    value["foundation"]
        .as_object_mut()
        .unwrap()
        .remove("stair_run");
    let mut recipe: BuildingRecipe = serde_json::from_value(value).unwrap();
    assert_eq!(recipe.foundation.stair_run, StairRun::Outward);
    recipe.foundation.stair_run = StairRun::AlongWall;
    let encoded = serde_json::to_value(&recipe).unwrap();
    assert_eq!(encoded["foundation"]["stair_run"], "along_wall");
    assert_eq!(
        serde_json::from_value::<BuildingRecipe>(encoded).unwrap(),
        recipe
    );
    recipe.atlas_size = 128;
    let prototype = generate(&recipe).unwrap();
    for lod in &prototype.lods {
        let mesh = &lod
            .parts
            .iter()
            .find(|p| p.material == MaterialSlot::Stairs)
            .unwrap()
            .mesh;
        assert!(
            mesh.positions
                .iter()
                .all(|p| prototype.layout.contains(*p, 0.0001))
        );
        assert!(mesh.normals.iter().any(|n| n[2] > 0.5));
    }
}

#[test]
fn foundation_floor_and_hideable_stairs_have_distinct_geometry_and_materials() {
    let mut recipe = BuildingRecipe::hut();
    recipe.atlas_size = 128;
    let visible = generate(&recipe).unwrap();
    let floor = visible.lods[0]
        .parts
        .iter()
        .find(|p| p.material == MaterialSlot::Floor)
        .unwrap();
    assert!(
        floor
            .mesh
            .positions
            .iter()
            .all(|p| p[2] >= -recipe.floor_thickness)
    );
    let bounds = foundation::bounds(&recipe);
    assert!(visible.colliders.contains(&bounds));
    assert!(!visible.stair_colliders.is_empty());
    for lod in &visible.lods {
        assert!(
            lod.parts
                .iter()
                .any(|p| p.material == MaterialSlot::Foundation)
        );
        assert!(lod.parts.iter().any(|p| p.material == MaterialSlot::Stairs));
        for part in &lod.parts {
            for p in &part.mesh.positions {
                assert!(visible.layout.contains(*p, 0.0001));
            }
            for triangle in part.mesh.indices.chunks_exact(3) {
                let p =
                    std::array::from_fn::<_, 3, _>(|i| part.mesh.positions[triangle[i] as usize]);
                let a = std::array::from_fn::<_, 3, _>(|i| p[1][i] - p[0][i]);
                let b = std::array::from_fn::<_, 3, _>(|i| p[2][i] - p[0][i]);
                let cross = [
                    a[1] * b[2] - a[2] * b[1],
                    a[2] * b[0] - a[0] * b[2],
                    a[0] * b[1] - a[1] * b[0],
                ];
                assert!(cross.iter().map(|v| v * v).sum::<f32>() > 1e-12);
            }
        }
    }
    let stair_count = |level: usize| {
        visible.lods[level]
            .parts
            .iter()
            .find(|p| p.material == MaterialSlot::Stairs)
            .unwrap()
            .mesh
            .triangle_count()
    };
    assert!(stair_count(2) < stair_count(0) / 4);
    recipe.foundation.stairs = false;
    recipe.materials.foundation = [0.5, 0.4, 0.3];
    let hidden = generate(&recipe).unwrap();
    assert!(hidden.stair_colliders.is_empty());
    assert!(hidden.lods.iter().all(|lod| {
        lod.parts
            .iter()
            .all(|part| part.material != MaterialSlot::Stairs)
    }));

    assert_eq!(
        texture(&visible, MaterialSlot::Floor),
        texture(&hidden, MaterialSlot::Floor)
    );
    assert_ne!(
        texture(&visible, MaterialSlot::Foundation),
        texture(&hidden, MaterialSlot::Foundation)
    );
}

#[test]
fn old_recipes_gain_foundations_and_invalid_settings_are_diagnosed() {
    let mut value = serde_json::to_value(BuildingRecipe::hut()).unwrap();
    value.as_object_mut().unwrap().remove("foundation");
    value["materials"]
        .as_object_mut()
        .unwrap()
        .remove("foundation");
    let mut recipe: BuildingRecipe = serde_json::from_value(value).unwrap();
    assert_eq!(
        recipe.foundation,
        island_building::FoundationRecipe::default()
    );
    recipe.foundation.burial_depth = f32::NAN;
    assert!(island_building::validate_recipe(&recipe).is_err());
}

fn texture(p: &island_building::BuildingPrototype, slot: MaterialSlot) -> &[u8] {
    &p.materials.iter().find(|m| m.slot == slot).unwrap().albedo
}
