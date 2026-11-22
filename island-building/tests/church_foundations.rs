use island_building::{
    BuildingMesh, BuildingRecipe, MaterialSlot,
    collision::BoxCollider,
    generate,
    terrain_support::{self, PlacementPose},
};

fn pier_bases(
    prototype: &island_building::BuildingPrototype,
) -> impl Iterator<Item = BoxCollider> + '_ {
    prototype.colliders.iter().copied().filter(|bounds| {
        (bounds.min[2] + 0.2).abs() < 0.00001
            && bounds.max[2] > 2.
            && (bounds.max[0] - bounds.min[0] - 0.84).abs() < 0.00001
            && (bounds.max[1] - bounds.min[1] - 0.9).abs() < 0.00001
    })
}

fn corners(bounds: BoxCollider) -> [[f32; 3]; 4] {
    [
        [bounds.min[0], bounds.min[1], bounds.min[2]],
        [bounds.max[0], bounds.min[1], bounds.min[2]],
        [bounds.max[0], bounds.max[1], bounds.min[2]],
        [bounds.min[0], bounds.max[1], bounds.min[2]],
    ]
}

fn upward_surface(mesh: &BuildingMesh, point: [f32; 3]) -> bool {
    mesh.indices.chunks_exact(3).any(|triangle| {
        if mesh.normals[triangle[0] as usize][2] < 0.99 {
            return false;
        }
        let vertices =
            [triangle[0], triangle[1], triangle[2]].map(|index| mesh.positions[index as usize]);
        if vertices
            .iter()
            .any(|vertex| (vertex[2] - point[2]).abs() > 0.00001)
        {
            return false;
        }
        let side = |from: [f32; 3], to: [f32; 3]| {
            (to[0] - from[0]) * (point[1] - from[1]) - (to[1] - from[1]) * (point[0] - from[0])
        };
        [
            side(vertices[0], vertices[1]),
            side(vertices[1], vertices[2]),
            side(vertices[2], vertices[0]),
        ]
        .iter()
        .all(|value| *value >= -0.00001)
    })
}

fn exterior_surface(mesh: &BuildingMesh, point: [f32; 3], direction: [f32; 2]) -> bool {
    let along = usize::from(direction[1].abs() > direction[0].abs());
    let across = 1 - along;
    let outward = [direction[1], -direction[0]][across].signum();
    mesh.indices.chunks_exact(3).any(|triangle| {
        if mesh.normals[triangle[0] as usize][across] * outward < 0.99 {
            return false;
        }
        let [a, b, c] =
            [triangle[0], triangle[1], triangle[2]].map(|index| mesh.positions[index as usize]);
        if [a, b, c]
            .iter()
            .any(|vertex| (vertex[across] - point[across]).abs() > 0.00001)
        {
            return false;
        }
        let sides = [(a, b), (b, c), (c, a)].map(|(from, to)| {
            (to[along] - from[along]) * (point[2] - from[2])
                - (to[2] - from[2]) * (point[along] - from[along])
        });
        sides.iter().all(|value| *value >= -0.00001) || sides.iter().all(|value| *value <= 0.00001)
    })
}

#[test]
fn exterior_masonry_covers_every_floor_edge_at_every_lod() {
    for mut recipe in [
        BuildingRecipe::gothic_church(),
        BuildingRecipe::parish_church(),
    ] {
        recipe.atlas_size = 128;
        recipe.foundation.stairs = false;
        let prototype = generate(&recipe).unwrap();
        let floor = &prototype.lods[0]
            .parts
            .iter()
            .find(|part| part.material == MaterialSlot::Floor)
            .unwrap()
            .mesh;
        let shell = terrain_support::supportless_shell(&prototype);
        let bearing = terrain_support::foundation_footprints(&recipe)[0].max[2];
        let outline = &shell.layout.church.as_ref().unwrap().outline;
        for (level, lod) in shell.lods.iter().enumerate() {
            let slot = if level == 2 {
                MaterialSlot::Atlas
            } else {
                MaterialSlot::Wall
            };
            let mesh = &lod
                .parts
                .iter()
                .find(|part| part.material == slot)
                .unwrap()
                .mesh;
            for (start, end) in outline.iter().zip(outline.iter().cycle().skip(1)) {
                let direction = [end[0] - start[0], end[1] - start[1]];
                for along in [0.01, 0.5, 0.99] {
                    for z in [-recipe.floor_thickness * 0.5, bearing + 0.0005] {
                        let point = [
                            start[0] + along * direction[0],
                            start[1] + along * direction[1],
                            z,
                        ];
                        assert!(
                            exterior_surface(mesh, point, direction),
                            "{} LOD{level} lacks outward-facing masonry at {point:?}",
                            recipe.name
                        );
                    }
                }
            }
        }
        assert!(floor.normals.iter().all(|normal| normal[2].abs() > 0.99));
    }
}

#[test]
fn standalone_church_has_masonry_and_collision_bearing_every_pier_corner() {
    for reach in [1.5, 3., 5.] {
        let mut recipe = BuildingRecipe::gothic_church();
        recipe.church.as_mut().unwrap().buttress_reach = reach;
        recipe.atlas_size = 128;
        let prototype = generate(&recipe).unwrap();
        let foundation = &prototype.lods[0]
            .parts
            .iter()
            .find(|part| part.material == MaterialSlot::Foundation)
            .unwrap()
            .mesh;
        let piers = pier_bases(&prototype).collect::<Vec<_>>();
        assert!(!piers.is_empty());
        for pier in piers {
            for corner in corners(pier) {
                assert!(
                    prototype.colliders.iter().any(|base| {
                        base.min[2] < -1.
                            && base.min[0] <= corner[0]
                            && base.max[0] >= corner[0]
                            && base.min[1] <= corner[1]
                            && base.max[1] >= corner[1]
                            && (base.max[2] - corner[2]).abs() < 0.00001
                    }),
                    "pier corner must bear on a foundation collider: {corner:?}"
                );
                assert!(
                    upward_surface(foundation, corner),
                    "pier corner must bear on rendered masonry: {corner:?}"
                );
            }
        }
        let shell = terrain_support::supportless_shell(&prototype);
        assert_eq!(pier_bases(&shell).count(), pier_bases(&prototype).count());
        assert_eq!(shell.support_colliders, prototype.support_colliders);
        let distant = &shell.lods[2]
            .parts
            .iter()
            .find(|part| part.material == MaterialSlot::Atlas)
            .unwrap()
            .mesh;
        assert!(
            !distant.indices.chunks_exact(3).any(|tri| {
                distant.normals[tri[0] as usize][2] > 0.99
                    && tri
                        .iter()
                        .all(|&index| (distant.positions[index as usize][2] + 0.2).abs() < 0.00001)
            }),
            "stripping removes the new footing's atlas top at the pier bearing height"
        );
        assert!(
            distant.indices.chunks_exact(3).any(|tri| {
                distant.normals[tri[0] as usize][2] < -0.99
                    && tri
                        .iter()
                        .all(|&index| (distant.positions[index as usize][2] + 0.2).abs() < 0.00001)
            }),
            "shell retains the pier's own underside when stripping its footing"
        );
    }
}

#[test]
fn rotated_sloping_terrain_fits_pier_footings_and_leaves_recesses_open() {
    let mut recipe = BuildingRecipe::gothic_church();
    recipe.atlas_size = 128;
    let prototype = generate(&recipe).unwrap();
    let sample = |x: f32, y: f32| Some(12. + 0.13 * x - 0.08 * y);
    for yaw in [0., 0.67, 1.8] {
        let mut pose = PlacementPose {
            origin: [120., -40., 0.],
            yaw,
        };
        let range = terrain_support::sampled_terrain_range(&recipe, pose, &sample, 0.25).unwrap();
        pose.origin[2] = range[1] + recipe.floor_thickness + 0.02;
        let fitted = terrain_support::fit(&recipe, pose, &sample).unwrap();
        for pier in pier_bases(&prototype) {
            for corner in corners(pier) {
                let base = fitted
                    .foundation_colliders
                    .iter()
                    .find(|base| {
                        (base.max[2] - pier.min[2]).abs() < 0.00001 && base.contains(corner)
                    })
                    .expect("fitted footing reaches the complete pier underside");
                let world = pose.world_point(corner);
                let ground = sample(world[0], world[1]).unwrap() - pose.origin[2];
                assert!(base.min[2] <= ground - recipe.foundation.burial_depth + 0.00001);
                assert!(upward_surface(&fitted.mesh, corner));
                assert!(fitted.clearance_bounds.contains(corner));
            }
        }
        assert!(
            !fitted.foundation_colliders.iter().any(|base| {
                base.min[0] < 12. && base.max[0] > 12. && base.min[1] < -10. && base.max[1] > -10.
            }),
            "new footings must not fill a whole church recess"
        );
    }
}

#[test]
fn terrain_sampling_includes_isolated_pier_high_and_low_points() {
    let mut recipe = BuildingRecipe::gothic_church();
    recipe.atlas_size = 128;
    let prototype = generate(&recipe).unwrap();
    let pier = pier_bases(&prototype)
        .find(|pier| {
            let middle = (pier.min[1] + pier.max[1]) * 0.5;
            (-13. ..-5.).contains(&middle)
        })
        .unwrap();
    let centre = [
        (pier.min[0] + pier.max[0]) * 0.5,
        (pier.min[1] + pier.max[1]) * 0.5,
    ];
    for height in [7., 12.] {
        let sample = |x: f32, y: f32| {
            Some(
                if (x - centre[0]).abs() < 0.2 && (y - centre[1]).abs() < 0.2 {
                    height
                } else {
                    10.
                },
            )
        };
        let range = terrain_support::sampled_terrain_range(
            &recipe,
            PlacementPose::default(),
            &sample,
            0.25,
        )
        .unwrap();
        assert!((range[0] - height.min(10.)).abs() < 0.00001);
        assert!((range[1] - height.max(10.)).abs() < 0.00001);
        let pose = PlacementPose {
            origin: [0., 0., range[1] + recipe.floor_thickness + 0.02],
            yaw: 0.,
        };
        let fitted = terrain_support::fit(&recipe, pose, &sample).unwrap();
        let pad = fitted
            .foundation_colliders
            .iter()
            .find(|base| {
                base.min[0] <= centre[0]
                    && base.max[0] >= centre[0]
                    && base.min[1] <= centre[1]
                    && base.max[1] >= centre[1]
                    && (base.max[2] + 0.2).abs() < 0.00001
            })
            .unwrap();
        if height < 10. {
            assert!(
                pad.max[2] - pad.min[2] > 3.,
                "an isolated pier depression increases actual footing depth"
            );
        }
    }
}
