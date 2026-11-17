use island_building::{
    BuildingRecipe, Facade, MaterialSlot, StairRun, generate,
    terrain_support::{self, PlacementPose, SupportError, SupportOptions},
};

#[allow(clippy::many_single_char_names)] // Triangle vertices and cross-product vectors.
fn assert_valid(mesh: &island_building::BuildingMesh) {
    assert!(!mesh.indices.is_empty());
    assert_eq!(mesh.positions.len(), mesh.normals.len());
    assert_eq!(mesh.positions.len(), mesh.uvs.len());
    assert!(mesh.indices.len().is_multiple_of(3));
    assert!(
        mesh.positions
            .iter()
            .flatten()
            .chain(mesh.normals.iter().flatten())
            .chain(mesh.uvs.iter().flatten())
            .all(|v| v.is_finite())
    );
    assert!(
        mesh.indices
            .iter()
            .all(|&i| (i as usize) < mesh.positions.len())
    );
    for tri in mesh.indices.chunks_exact(3) {
        let [a, b, c] = [tri[0], tri[1], tri[2]].map(|i| mesh.positions[i as usize]);
        let u = std::array::from_fn::<_, 3, _>(|i| b[i] - a[i]);
        let v = std::array::from_fn::<_, 3, _>(|i| c[i] - a[i]);
        let cross = [
            u[1] * v[2] - u[2] * v[1],
            u[2] * v[0] - u[0] * v[2],
            u[0] * v[1] - u[1] * v[0],
        ];
        assert!(cross.iter().map(|x| x * x).sum::<f32>() > 0.);
    }
}

#[test]
fn flat_terrain_uses_only_local_drop_and_one_short_entrance_step() {
    let recipe = BuildingRecipe::hut();
    let pose = PlacementPose {
        origin: [123., -47., 10.15],
        yaw: 0.8,
    };
    let supports = terrain_support::fit(&recipe, pose, &|_, _| Some(10.)).unwrap();
    assert_valid(&supports.mesh);
    assert_eq!(supports.foundation_colliders.len(), 1);
    let foundation = supports.foundation_colliders[0];
    assert!((foundation.min[2] + 0.15 + recipe.foundation.burial_depth).abs() < 0.0001);
    assert!(foundation.min[2] > -1.);
    assert_eq!(supports.stairs[0].steps, 1);
    assert!(supports.clearance_bounds.max[2] < 0.01);
    assert!(supports.stair_colliders.iter().all(|b| b.min[2] < b.max[2]));
}

#[test]
fn steep_downhill_flight_ends_at_actual_ground_with_standard_risers() {
    let recipe = BuildingRecipe::hut();
    let terrain = |_: f32, y: f32| Some(y * 30_f32.to_radians().tan());
    let range =
        terrain_support::sampled_terrain_range(&recipe, PlacementPose::default(), &terrain, 0.5)
            .unwrap();
    let pose = PlacementPose {
        origin: [0., 0., range[1] + 0.15],
        yaw: 0.,
    };
    let supports = terrain_support::fit(&recipe, pose, &terrain).unwrap();
    assert_valid(&supports.mesh);
    assert!(supports.stairs[0].steps > 5);
    let cap = supports.stair_colliders.last().unwrap();
    for x in [cap.min[0], cap.max[0]] {
        for y in [cap.min[1], cap.max[1]] {
            assert!(cap.max[2] + pose.origin[2] <= terrain(x, y).unwrap() + 0.001);
        }
    }
    assert!(supports.stairs[0].step_rise <= 0.2);
    assert!(supports.stairs[0].step_run >= 0.24);
    assert!(supports.foundation_colliders[0].min[2] < -3.);
}

#[test]
fn rotated_sampling_catches_an_interior_terrain_dip() {
    let recipe = BuildingRecipe::hut();
    let pose = PlacementPose {
        origin: [20., -12., 2.],
        yaw: 1.1,
    };
    let terrain = |x: f32, y: f32| Some(1. - 3. * (-(x - 20.).powi(2) - (y + 12.).powi(2)).exp());
    let supports = terrain_support::fit(&recipe, pose, &terrain).unwrap();
    assert!(supports.foundation_colliders[0].min[2] <= -4.);
    assert_valid(&supports.mesh);
}

#[test]
fn church_and_extensions_preserve_the_occupied_plan() {
    let church = BuildingRecipe::gothic_church();
    let supports = terrain_support::fit(
        &church,
        PlacementPose {
            origin: [0., 0., 0.5],
            yaw: 0.,
        },
        &|_, _| Some(0.),
    )
    .unwrap();
    assert!(
        supports.foundation_colliders.len() > 5,
        "each outer pier needs its own footing"
    );
    assert!(
        !supports
            .foundation_colliders
            .iter()
            .any(|b| b.contains([12., -10., -0.4]))
    );
    assert_valid(&supports.mesh);
    let mut recipe = BuildingRecipe::hut();
    recipe
        .extensions
        .push(island_building::ExtensionRecipe::default());
    let supports = terrain_support::fit(
        &recipe,
        PlacementPose {
            origin: [0., 0., 0.3],
            yaw: 0.,
        },
        &|_, _| Some(0.),
    )
    .unwrap();
    assert_eq!(supports.foundation_colliders.len(), 2);
    let enclosing_corner = [
        supports.footprint_bounds.max[0] - 0.01,
        supports.footprint_bounds.max[1] - 0.01,
        -0.2,
    ];
    assert!(
        !supports
            .foundation_colliders
            .iter()
            .any(|b| b.contains(enclosing_corner))
    );
    assert_valid(&supports.mesh);
    let floors = supports
        .occupied_footprints
        .iter()
        .map(|b| {
            let inset = recipe.foundation.outset;
            (
                [b.min[0] + inset, b.min[1] + inset],
                [b.max[0] - inset, b.max[1] - inset],
            )
        })
        .collect::<Vec<_>>();
    for triangle in supports.mesh.indices.chunks_exact(3) {
        let points =
            [triangle[0], triangle[1], triangle[2]].map(|i| supports.mesh.positions[i as usize]);
        if points.iter().all(|p| p[2].abs() < 0.00001) {
            let centre = std::array::from_fn::<_, 2, _>(|axis| {
                points.iter().map(|p| p[axis]).sum::<f32>() / 3.
            });
            assert!(
                !floors.iter().any(|(min, max)| (0..2)
                    .all(|axis| centre[axis] > min[axis] + 0.00001
                        && centre[axis] < max[axis] - 0.00001)),
                "stone cap must not be coplanar with a floor inside an attached room"
            );
        }
    }
}

#[test]
fn all_facades_and_both_stair_runs_keep_threshold_and_caps() {
    for facade in Facade::ALL {
        for run in [StairRun::AlongWall, StairRun::Outward] {
            let mut recipe = BuildingRecipe::hut();
            recipe.foundation.stair_run = run;
            recipe.openings[0].facade = facade;
            let pose = PlacementPose {
                origin: [0., 0., 0.45],
                yaw: 0.4,
            };
            let supports = terrain_support::fit(&recipe, pose, &|_, _| Some(0.)).unwrap();
            let threshold = facade.point(&recipe, recipe.openings[0].offset, 0.002, 0.);
            assert!(
                supports
                    .stair_colliders
                    .iter()
                    .any(|b| b.contains(threshold))
            );
            let cap = supports.stair_colliders[1];
            assert!((cap.max[2] - cap.min[2] - 0.045).abs() < 0.00001);
            assert_valid(&supports.mesh);
        }
    }
}

#[test]
fn unavailable_terrain_and_unreachable_stairs_return_errors() {
    let recipe = BuildingRecipe::hut();
    assert!(matches!(
        terrain_support::fit(&recipe, PlacementPose::default(), &|_, _| None),
        Err(SupportError::TerrainUnavailable(_))
    ));
    let options = SupportOptions {
        sample_spacing: 0.5,
        max_stair_steps: 2,
    };
    assert!(matches!(
        terrain_support::fit_with_options(
            &recipe,
            PlacementPose {
                origin: [0., 0., 10.],
                yaw: 0.
            },
            &|_, _| Some(0.),
            options
        ),
        Err(SupportError::StairsCannotReachTerrain(_))
    ));
}

#[test]
fn reusable_shell_removes_only_external_support_and_retains_internal_structure() {
    let mut recipe = BuildingRecipe::hut();
    recipe.width = 10.;
    recipe.depth = 12.;
    recipe.storeys = 2;
    recipe.atlas_size = 128;
    let original = generate(&recipe).unwrap();
    let shell = terrain_support::supportless_shell(&original);
    assert!(shell.stair_colliders.is_empty());
    assert_eq!(shell.support_colliders, original.support_colliders);
    assert!(shell.colliders.len() < original.colliders.len());
    assert!(!shell.support_colliders.is_empty());
    for (before, after) in original.lods.iter().zip(&shell.lods) {
        assert!(after.parts.iter().all(|p| !p.material.is_foundation()));
        for part in after
            .parts
            .iter()
            .filter(|p| p.material != MaterialSlot::Atlas)
        {
            assert_eq!(
                Some(part),
                before.parts.iter().find(|p| p.material == part.material)
            );
        }
    }
    assert_eq!(shell.doors, original.doors);
}

#[test]
fn church_shell_strips_atlased_distant_base_without_removing_floor() {
    let mut recipe = BuildingRecipe::gothic_church();
    recipe.atlas_size = 128;
    let original = generate(&recipe).unwrap();
    let shell = terrain_support::supportless_shell(&original);
    assert_eq!(
        original.colliders.len() - shell.colliders.len(),
        terrain_support::foundation_footprints(&recipe).len()
    );
    assert_eq!(original.support_colliders, shell.support_colliders);
    assert!(shell.lods[2].triangle_count() < original.lods[2].triangle_count());
    assert!(
        shell
            .lods
            .iter()
            .flat_map(|l| &l.parts)
            .flat_map(|p| &p.mesh.positions)
            .all(|p| p[2] >= -recipe.floor_thickness - 0.001)
    );
}

#[test]
fn extension_shell_removes_each_base_and_retains_room_floors() {
    let mut recipe = BuildingRecipe::hut();
    recipe.chimney = None;
    recipe
        .extensions
        .push(island_building::ExtensionRecipe::default());
    recipe.atlas_size = 128;
    let original = generate(&recipe).unwrap();
    let shell = terrain_support::supportless_shell(&original);
    assert_eq!(original.colliders.len() - shell.colliders.len(), 2);
    let floors = |prototype: &island_building::BuildingPrototype| {
        prototype
            .colliders
            .iter()
            .filter(|b| b.min[2] >= -recipe.floor_thickness - 0.001 && b.max[2] <= 0.)
            .count()
    };
    assert_eq!(floors(&original), floors(&shell));
    assert!(floors(&shell) >= 2);
}
