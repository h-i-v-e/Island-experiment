use island_building::{
    BuildingRecipe, ExtensionKind, ExtensionRecipe, Facade, MaterialSlot, RoofCovering, WallStyle,
    generate,
};

fn house(facade: Facade, kind: ExtensionKind) -> BuildingRecipe {
    let mut recipe = BuildingRecipe::hut();
    recipe.atlas_size = 128;
    recipe.chimney = None;
    recipe.width = 8.;
    recipe.depth = 7.;
    recipe.openings[0].facade = Facade::ALL[(facade.index() + 2) % 4];
    let entrance = recipe.openings[0].facade;
    recipe
        .openings
        .retain(|o| o.kind == island_building::OpeningKind::Door || o.facade != entrance);
    recipe.extensions.push(ExtensionRecipe {
        facade,
        kind,
        width: 3.5,
        wall_height: 2.6,
        roof_rise: 0.7,
        ..Default::default()
    });
    recipe
}

#[test]
fn attachments_join_all_facades_with_clear_passages_and_ground_support() {
    for facade in Facade::ALL {
        for kind in [ExtensionKind::LeanTo, ExtensionKind::Wing] {
            let recipe = house(facade, kind);
            let p = generate(&recipe).unwrap();
            let extension = &p.layout.extensions[0];
            let centre = facade.point(&recipe, 0., 0.025, 0.);
            let normal = facade.normal();
            for distance in [-0.5, 0., 0.5, 1.5] {
                let feet = [
                    centre[0] + normal[0] * distance,
                    centre[1] + normal[1] * distance,
                    0.025,
                ];
                assert!(
                    !p.colliders
                        .iter()
                        .any(|b| b.intersects_capsule(feet, 0.28, 1.8)),
                    "blocked {facade:?} {kind:?} at {distance}"
                );
            }
            assert_connection_surfaces(&p, &recipe, facade, kind);
            assert_room_door(&p, &recipe, facade);
            let middle = std::array::from_fn::<_, 3, _>(|i| {
                (extension.room_bounds.min[i] + extension.room_bounds.max[i]) * 0.5
            });
            assert!(p.layout.contains(middle, 0.));
            assert!(p.support_colliders.iter().any(|b| b.min[0] <= middle[0]
                && b.max[0] >= middle[0]
                && b.min[1] <= middle[1]
                && b.max[1] >= middle[1]));
            assert_eq!(p.doors.len(), 2);
            assert!(
                p.lods[0]
                    .parts
                    .iter()
                    .any(|part| part.material == MaterialSlot::Glass)
            );
            assert!(
                !p.lods[1..]
                    .iter()
                    .flat_map(|lod| &lod.parts)
                    .any(|part| part.material == MaterialSlot::Glass)
            );
            for lod in &p.lods {
                for part in &lod.parts {
                    assert!(part.mesh.positions.iter().flatten().all(|v| v.is_finite()));
                    assert!(part.mesh.normals.iter().flatten().all(|v| v.is_finite()));
                }
            }
        }
    }
}

fn assert_room_door(
    p: &island_building::BuildingPrototype,
    recipe: &BuildingRecipe,
    facade: Facade,
) {
    let room = &p.layout.extensions[0];
    let aperture = room.doorway.expect("every extension has a doorway");
    let leaf = p.doors.iter().find(|d| d.id == room.door_id).unwrap();
    assert!(!leaf.exterior);
    let centre = [
        (aperture.min[0] + aperture.max[0]) * 0.5,
        (aperture.min[1] + aperture.max[1]) * 0.5,
        0.025,
    ];
    assert!(
        leaf.collider_at(0.).intersects_capsule(centre, 0.28, 1.8),
        "closed leaf blocks passage"
    );
    assert!(
        !leaf
            .collider_at(leaf.swing_angle)
            .intersects_capsule(centre, 0.28, 1.8),
        "open leaf clears passage"
    );
    let axis = usize::from(facade.tangent()[0].abs() <= 0.5);
    let mut beside = centre;
    beside[axis] += recipe.opening_defaults.door.width * 0.5 + 0.22;
    assert!(
        p.colliders
            .iter()
            .any(|b| b.intersects_capsule(beside, 0.1, 1.8)),
        "divider blocks walking around the door"
    );
    let direction = facade.normal().map(|v| -v);
    let normal = facade.normal();
    beside[2] = 1.2;
    for i in 0..2 {
        beside[i] += normal[i] * 0.7;
    }
    assert!(
        p.lods[0]
            .parts
            .iter()
            .filter(|p| p.material == MaterialSlot::Interior)
            .any(|p| ray_hits(&p.mesh, beside, direction, 1.5)),
        "divider has visible clean interior faces"
    );
    for lod in &p.lods[1..] {
        assert!(
            !lod.parts
                .iter()
                .any(|p| p.material == MaterialSlot::Interior)
        );
    }
}

fn assert_connection_surfaces(
    p: &island_building::BuildingPrototype,
    recipe: &BuildingRecipe,
    facade: Facade,
    kind: ExtensionKind,
) {
    // Include the former wall footprint, where both full-width floor
    // slabs overlap. There must be one visible surface, without gaps.
    for distance in [-0.433, -0.173, -0.073, 0.193, 0.713] {
        let point = facade.point(recipe, 0.137, 0., -distance);
        let count: usize = p.lods[0]
            .parts
            .iter()
            .filter(|p| matches!(p.material, MaterialSlot::Floor | MaterialSlot::Foundation))
            .map(|p| faces_at(&p.mesh, point, 2, 1.))
            .sum();
        assert_eq!(
            count, 1,
            "floor overlap/gap at {point:?}, {facade:?} {kind:?}"
        );
    }
    let origin = facade.point(recipe, 0., 1.2, -0.7);
    let direction = facade.normal().map(|v| -v);
    assert!(
        !p.lods[0]
            .parts
            .iter()
            .filter(|p| matches!(
                p.material,
                MaterialSlot::Wall
                    | MaterialSlot::Interior
                    | MaterialSlot::Timber
                    | MaterialSlot::Trim
            ))
            .any(|p| ray_hits(&p.mesh, origin, direction, 1.5)),
        "visible wall across {facade:?} {kind:?}"
    );
    let loft = facade.point(recipe, 0.137, recipe.wall_height - 0.02, -0.7);
    assert!(
        p.lods[0]
            .parts
            .iter()
            .filter(|p| p.material == MaterialSlot::Wall)
            .any(|p| ray_hits(&p.mesh, loft, direction, 1.5)),
        "missing loft infill above {facade:?} {kind:?}"
    );
}

fn faces_at(
    mesh: &island_building::BuildingMesh,
    point: [f32; 3],
    axis: usize,
    sign: f32,
) -> usize {
    let [first, second] = [(axis + 1) % 3, (axis + 2) % 3];
    mesh.indices
        .chunks_exact(3)
        .filter(|tri| {
            let vertices = [tri[0], tri[1], tri[2]].map(|i| mesh.positions[i as usize]);
            if vertices
                .iter()
                .any(|p| (p[axis] - point[axis]).abs() > 0.00001)
                || mesh.normals[tri[0] as usize][axis] * sign < 0.5
            {
                return false;
            }
            let [a, b, c] = vertices;
            let area = |a: [f32; 3], b: [f32; 3], c: [f32; 3]| {
                (b[first] - a[first]) * (c[second] - a[second])
                    - (b[second] - a[second]) * (c[first] - a[first])
            };
            let total = area(a, b, c);
            total.abs() > 0.00001
                && [area(point, b, c), area(a, point, c), area(a, b, point)]
                    .iter()
                    .all(|v| *v / total > 0.00001)
        })
        .count()
}

#[test]
fn timber_posts_cover_both_attachment_seams_at_every_detail_level() {
    for facade in Facade::ALL {
        for kind in [ExtensionKind::Wing, ExtensionKind::LeanTo] {
            let mut recipe = house(facade, kind);
            recipe.walls.style = WallStyle::WattleAndDaub;
            recipe.roof.covering = RoofCovering::StandingSeam;
            let p = generate(&recipe).unwrap();
            let ext = &recipe.extensions[0];
            let width = recipe.walls.beam_width;
            let normal = facade.normal();
            let axis = usize::from(normal[1].abs() > 0.5);
            let timber_faces = |level: usize, along: f32, z: f32| {
                let point = facade.point(&recipe, along, z, -width);
                p.lods[level]
                    .parts
                    .iter()
                    .filter(|p| p.material == MaterialSlot::Timber)
                    .map(|p| faces_at(&p.mesh, point, axis, normal[axis]))
                    .sum::<usize>()
            };
            for sign in [-1., 1.] {
                let exterior = ext.offset + sign * (ext.width * 0.5 + width * 0.43);
                let interior =
                    ext.offset + sign * (ext.width * 0.5 - recipe.wall_thickness - width * 0.43);
                for level in 0..3 {
                    for height in [0.29, ext.wall_height - 0.22] {
                        assert_eq!(
                            timber_faces(level, exterior, height),
                            1,
                            "{facade:?} {kind:?} LOD{level}"
                        );
                    }
                }
                assert_eq!(
                    timber_faces(0, interior, 1.17),
                    1,
                    "inside {facade:?} {kind:?}"
                );
            }
        }
    }
}

#[test]
fn multi_storey_wing_keeps_one_exposed_face_over_the_floor_band() {
    for facade in Facade::ALL {
        let mut recipe = house(facade, ExtensionKind::Wing);
        recipe.storeys = 2;
        recipe.floor_thickness = 0.05;
        recipe.walls.style = WallStyle::WattleAndDaub;
        recipe.roof.covering = RoofCovering::Thatch;
        let wing = &mut recipe.extensions[0];
        wing.wall_height = recipe.wall_height;
        wing.roof_rise = 2.4;
        let p = generate(&recipe).unwrap();
        let normal = facade.normal();
        let axis = usize::from(normal[1].abs() > 0.5);
        let count = |slot, along, height| {
            let point = facade.point(&recipe, along, height, 0.);
            p.lods[0]
                .parts
                .iter()
                .filter(|p| p.material == slot)
                .map(|p| faces_at(&p.mesh, point, axis, normal[axis]))
                .sum::<usize>()
        };
        for along in [-1.12, 0.137, 1.05] {
            let band = recipe.wall_height + 0.024;
            assert_eq!(count(MaterialSlot::Timber, along, band), 1, "{facade:?}");
            assert_eq!(
                count(MaterialSlot::Interior, along, band),
                0,
                "plaster overlaps the timber floor band on {facade:?}"
            );
            for height in [recipe.wall_height - 0.023, recipe.floor_height(1) + 0.023] {
                assert_eq!(
                    count(MaterialSlot::Interior, along, height),
                    1,
                    "{facade:?} at {height}"
                );
            }
        }
    }
}

#[test]
fn intersecting_slate_roofs_keep_a_continuous_weather_surface() {
    let mut recipe = house(Facade::North, ExtensionKind::Wing);
    recipe.width = 9.;
    recipe.depth = 7.;
    recipe.wall_height = 2.8;
    recipe.roof.rise = 2.;
    recipe.roof.covering = RoofCovering::Slate;
    recipe.roof.thickness = 0.12;
    let wing = &mut recipe.extensions[0];
    wing.width = 3.2;
    wing.depth = 5.;
    wing.offset = 1.3;
    wing.wall_height = 2.8;
    wing.roof_rise = 1.4;
    let p = generate(&recipe).unwrap();
    for level in 0..3 {
        for x in -20_i16..20 {
            for y in -16_i16..41 {
                let point = [f32::from(x) * 0.2 + 0.013, f32::from(y) * 0.2 + 0.017, 10.];
                if point[1] > 3.4 && !(-2.8..0.2).contains(&point[0]) {
                    continue;
                }
                assert!(
                    p.lods[level]
                        .parts
                        .iter()
                        .filter(|p| matches!(p.material, MaterialSlot::Roof | MaterialSlot::Atlas))
                        .any(|p| ray_hits(&p.mesh, point, [0., 0., -1.], 8.)),
                    "roof gap LOD{level} at {point:?}"
                );
            }
        }
    }
}

#[test]
fn materials_and_atlas_budget_are_shared_and_recipes_regenerate() {
    let mut recipe = house(Facade::East, ExtensionKind::Wing);
    let extension = recipe.extensions.pop().unwrap();
    let plain = generate(&recipe).unwrap();
    recipe.extensions.push(extension);
    let p = generate(&recipe).unwrap();
    assert_eq!(p.materials, plain.materials);
    assert_eq!(p.atlas.size, recipe.atlas_size);
    assert!(p.lods[0].triangle_count() > plain.lods[0].triangle_count());
    let restored: BuildingRecipe =
        serde_json::from_str(&serde_json::to_string(&p.recipe).unwrap()).unwrap();
    let again = generate(&restored).unwrap();
    assert_eq!(p.hash, again.hash);
    assert_eq!(p.lods, again.lods);
    assert_eq!(p.atlas, again.atlas);
    assert_eq!(p.doors, again.doors);
}

#[test]
fn extension_door_moves_clear_of_a_partition_and_keeps_its_id() {
    let mut recipe = house(Facade::East, ExtensionKind::Wing);
    recipe
        .openings
        .retain(|o| o.kind == island_building::OpeningKind::Door);
    recipe.openings[0].facade = Facade::South;
    recipe.openings[0].offset = -2.;
    recipe.partitions.push(island_building::PartitionRecipe {
        id: "middle-divider".into(),
        storey: 0,
        axis: island_building::PartitionAxis::X,
        offset: 0.,
        door_offset: 0.,
        door_width: 1.,
        door_height: 2.1,
        use_default_door_dimensions: false,
    });
    let p = generate(&recipe).unwrap();
    let room = &p.layout.extensions[0];
    let aperture = room.doorway.unwrap();
    let centre = (aperture.min[1] + aperture.max[1]) * 0.5;
    assert!(centre.abs() > 0.3, "door approach clears the partition end");
    assert_eq!(room.door_id, "extension-door-extension");
    let again = generate(&p.recipe).unwrap();
    assert_eq!(p.doors, again.doors);
    recipe.opening_defaults.door.height = 2.7;
    assert!(generate(&recipe).is_err());
    assert!(island_building::validate_recipe(&recipe).is_err());
}

#[test]
fn foundations_use_the_complete_diagonal_and_empty_courtyard_is_not_occupied() {
    let mut recipe = house(Facade::East, ExtensionKind::Wing);
    recipe.extensions[0].depth = 8.;
    let p = generate(&recipe).unwrap();
    let bounds = island_building::geometry::foundation::enclosing_bounds(&recipe);
    let required = (bounds.max[0] - bounds.min[0]).hypot(bounds.max[1] - bounds.min[1])
        * 30_f32.to_radians().tan()
        + recipe.foundation.burial_depth;
    assert!((-bounds.min[2] - required).abs() < 0.0001);
    let child = &p.layout.extensions[0];
    assert!((child.bounds.min[2] - bounds.min[2]).abs() < 0.0001);
    assert!(!p.layout.contains(
        [child.room_bounds.max[0] - 0.5, recipe.depth * 0.5 + 1., 1.],
        0.
    ));
}

#[test]
fn timber_thatch_slate_and_upper_storeys_keep_connected_extensions() {
    for covering in [RoofCovering::Slate, RoofCovering::Thatch] {
        let mut recipe = house(Facade::East, ExtensionKind::LeanTo);
        recipe.walls.style = WallStyle::WattleAndDaub;
        recipe.roof.covering = covering;
        recipe.extensions[0].wall_height = recipe.wall_height;
        let p = generate(&recipe).unwrap();
        assert!(
            p.lods[0]
                .parts
                .iter()
                .any(|p| p.material == MaterialSlot::Timber)
        );
        let point = Facade::East.point(&recipe, 0., 0.025, 0.);
        assert!(
            !p.colliders
                .iter()
                .any(|b| b.intersects_capsule(point, 0.28, 1.8))
        );
    }
    let mut recipe = house(Facade::East, ExtensionKind::Wing);
    recipe.storeys = 2;
    recipe.opening_generation = Some(island_building::OpeningGeneration::default());
    let p = generate(&recipe).unwrap();
    assert!(p.layout.staircase.is_some());
    assert_eq!(p.layout.extensions.len(), 1);
    assert_eq!(p.hash, generate(&p.recipe).unwrap().hash);
}

#[allow(clippy::many_single_char_names)] // Standard ray/triangle coordinates.
fn ray_hits(
    mesh: &island_building::BuildingMesh,
    origin: [f32; 3],
    direction: [f32; 3],
    limit: f32,
) -> bool {
    let sub = |a: [f32; 3], b: [f32; 3]| std::array::from_fn::<_, 3, _>(|i| a[i] - b[i]);
    let cross = |a: [f32; 3], b: [f32; 3]| {
        [
            a[1] * b[2] - a[2] * b[1],
            a[2] * b[0] - a[0] * b[2],
            a[0] * b[1] - a[1] * b[0],
        ]
    };
    let dot = |a: [f32; 3], b: [f32; 3]| a.iter().zip(b).map(|(a, b)| a * b).sum::<f32>();
    mesh.indices.chunks_exact(3).any(|tri| {
        let a = mesh.positions[tri[0] as usize];
        let b = mesh.positions[tri[1] as usize];
        let c = mesh.positions[tri[2] as usize];
        let e1 = sub(b, a);
        let e2 = sub(c, a);
        let p = cross(direction, e2);
        let det = dot(e1, p);
        if det.abs() < 0.00001 {
            return false;
        }
        let t = sub(origin, a);
        let u = dot(t, p) / det;
        if !(0. ..=1.).contains(&u) {
            return false;
        }
        let q = cross(t, e1);
        let v = dot(direction, q) / det;
        if v < 0. || u + v > 1. {
            return false;
        }
        let distance = dot(e2, q) / det;
        (0. ..limit).contains(&distance)
    })
}

#[test]
fn invalid_or_obstructed_attachments_fail_without_recursing() {
    let mut recipe = house(Facade::East, ExtensionKind::Wing);
    recipe.extensions[0].depth = f32::NAN;
    assert!(generate(&recipe).is_err());
    recipe.extensions[0].depth = 3.;
    recipe.extensions[0].facade = recipe.openings[0].facade;
    assert!(
        generate(&recipe)
            .unwrap_err()
            .to_string()
            .contains("entrance")
    );
    let mut church = BuildingRecipe::gothic_church();
    church.extensions.push(ExtensionRecipe::default());
    assert!(generate(&church).is_err());
}

#[test]
fn compound_export_preserves_layout_foundations_and_regenerates_identically() {
    let mut recipe = house(Facade::East, ExtensionKind::LeanTo);
    recipe.extensions.push(ExtensionRecipe {
        id: "north-wing".into(),
        kind: ExtensionKind::Wing,
        facade: Facade::North,
        width: 3.,
        depth: 4.,
        wall_height: 2.7,
        roof_rise: 1.1,
        ..Default::default()
    });
    let p = generate(&recipe).unwrap();
    let directory =
        std::env::temp_dir().join(format!("building-extensions-export-{}", std::process::id()));
    let manifest =
        island_building::export::export_package(&p, &directory, std::path::Path::new(".")).unwrap();
    let restored: BuildingRecipe =
        serde_json::from_slice(&std::fs::read(directory.join("recipe.json")).unwrap()).unwrap();
    let again = island_building::generate_with_base(&restored, &directory).unwrap();
    assert_eq!(p.hash, again.hash);
    assert_eq!(p.layout, again.layout);
    assert_eq!(p.lods, again.lods);
    assert_eq!(p.atlas, again.atlas);
    assert_eq!(p.doors, again.doors);
    let foundation: island_building::collision::BoxCollider =
        serde_json::from_slice(&std::fs::read(directory.join("foundation.json")).unwrap()).unwrap();
    assert_eq!(
        foundation,
        island_building::geometry::foundation::enclosing_bounds(&p.recipe)
    );
    assert_eq!(manifest.geometry_version, 64);
    std::fs::remove_dir_all(directory).unwrap();
}

fn walk_through_extension(prototype: &island_building::BuildingPrototype) -> [f32; 3] {
    let ext = &prototype.layout.extensions[0];
    let door = prototype
        .doors
        .iter()
        .find(|d| d.id == ext.door_id)
        .unwrap();
    let aperture = ext.doorway.unwrap();
    let centre = std::array::from_fn(|i| (aperture.min[i] + aperture.max[i]) * 0.5);
    let normal = ext.facade.normal();
    for sample in -10_i16..=10 {
        let distance = f32::from(sample) * 0.05;
        let feet = [
            centre[0] + normal[0] * distance,
            centre[1] + normal[1] * distance,
            0.025,
        ];
        assert!(
            !prototype
                .colliders
                .iter()
                .any(|b| b.intersects_capsule(feet, 0.28, 1.8))
                && !door
                    .collider_at(door.swing_angle)
                    .intersects_capsule(feet, 0.28, 1.8),
            "door approach blocked at {feet:?}"
        );
    }
    centre
}

#[test]
fn extensions_can_share_a_fireplace_wall_and_put_the_door_near_a_corner() {
    for kind in [ExtensionKind::LeanTo, ExtensionKind::Wing] {
        let mut recipe = house(Facade::East, kind);
        recipe.depth = 9.;
        recipe.openings.retain(|o| o.facade != Facade::East);
        recipe.chimney = Some(island_building::ChimneyRecipe {
            facade: Facade::East,
            offset: 0.,
            ..Default::default()
        });
        recipe.extensions[0].width = 6.;
        recipe.extensions[0].roof_rise = 3.1;
        let p = generate(&recipe).unwrap();
        let centre = walk_through_extension(&p);
        assert!(centre[1].abs() > 1.45, "{kind:?} doorway at {centre:?}");
        let hearth = island_building::geometry::chimney::hearth_bounds(&recipe).unwrap();
        assert!(p.colliders.contains(&hearth));

        let mut host = recipe.clone();
        host.extensions.clear();
        let host = generate(&host).unwrap();
        let part_size = |building: &island_building::BuildingPrototype, slot| {
            building.lods[0]
                .parts
                .iter()
                .find(|p| p.material == slot)
                .unwrap()
                .mesh
                .indices
                .len()
        };
        for slot in [
            MaterialSlot::Chimney,
            MaterialSlot::StoneSlab,
            MaterialSlot::Firebox,
        ] {
            assert_eq!(
                part_size(&p, slot),
                part_size(&host, slot),
                "lost {slot:?} geometry"
            );
        }

        let shaft = island_building::geometry::chimney::stack_bounds(&recipe).unwrap();
        let point = [
            (shaft.min[0] + shaft.max[0]) * 0.5,
            (shaft.min[1] + shaft.max[1]) * 0.5,
            shaft.max[2] + 1.,
        ];
        for lod in &p.lods[..2] {
            assert!(
                !lod.parts
                    .iter()
                    .filter(|p| p.material == MaterialSlot::Roof)
                    .any(|p| { ray_hits(&p.mesh, point, [0., 0., -1.], point[2] - shaft.min[2]) }),
                "extension roof covers the flue"
            );
        }
        recipe.extensions[0].width = 3.5;
        assert!(
            generate(&recipe)
                .unwrap_err()
                .to_string()
                .contains("No clear door")
        );
    }
}

#[test]
fn a_stairwell_on_the_join_moves_the_opening_to_a_clear_end() {
    let mut recipe = house(Facade::East, ExtensionKind::Wing);
    recipe.width = 10.;
    recipe.depth = 9.;
    recipe.storeys = 2;
    recipe.staircase.axis = island_building::StairAxis::Y;
    recipe.staircase.position = Some([3.5, 0.]);
    recipe.extensions[0].width = 8.;
    let p = generate(&recipe).unwrap();
    let centre = walk_through_extension(&p);
    let stairs = p
        .layout
        .staircase
        .as_ref()
        .unwrap()
        .stairwell
        .as_ref()
        .unwrap();
    assert!(
        centre[1] < stairs.bounds.min[1] - 0.27 || centre[1] > stairs.bounds.max[1] + 0.27,
        "doorway {centre:?} still meets the stairwell {:?}",
        stairs.bounds
    );
}

#[test]
fn lean_to_framing_stays_outside_the_stairwell_at_the_floor_join() {
    let mut recipe = house(Facade::North, ExtensionKind::LeanTo);
    recipe.width = 7.;
    recipe.depth = 8.;
    recipe.storeys = 3;
    recipe.wall_height = 2.9;
    recipe.floor_thickness = 0.16;
    recipe.wall_thickness = 0.2;
    recipe.walls.style = WallStyle::WattleAndDaub;
    recipe.walls.frame_projection = 0.025;
    recipe.roof.covering = RoofCovering::Thatch;
    recipe.staircase.wall = Some(Facade::North);
    recipe.staircase.enclosed = true;
    let extension = &mut recipe.extensions[0];
    extension.width = 6.;
    extension.depth = 4.;
    extension.wall_height = recipe.wall_height;
    extension.roof_rise = 2.;

    let p = generate(&recipe).unwrap();
    let stairwell = p.layout.staircase.unwrap().stairwell.unwrap().bounds;
    let wall = recipe.depth * 0.5 - recipe.wall_thickness;
    let half_width = recipe.width * 0.5 - recipe.wall_thickness;
    let join = recipe.floor_height(1) - recipe.floor_thickness;
    for lod in &p.lods[..2] {
        for part in lod
            .parts
            .iter()
            .filter(|part| matches!(part.material, MaterialSlot::Timber | MaterialSlot::Interior))
        {
            let mesh = &part.mesh;
            for triangle in mesh.indices.chunks_exact(3) {
                let points =
                    [triangle[0], triangle[1], triangle[2]].map(|i| mesh.positions[i as usize]);
                let in_join = points
                    .iter()
                    .all(|p| (join - 0.0101..=join + 0.0001).contains(&p[2]))
                    && points.iter().any(|p| p[2] < join - 0.009)
                    && points.iter().any(|p| p[2] > join - 0.001);
                let projects_into_stairs = points
                    .iter()
                    .all(|p| p[0].abs() < half_width - 0.02 && p[1] < wall - 0.001)
                    && points.iter().any(|p| {
                        (stairwell.min[0]..stairwell.max[0]).contains(&p[0])
                            && (wall - 0.3..wall - 0.04).contains(&p[1])
                    });
                assert!(
                    !(in_join && projects_into_stairs),
                    "{:?} projects into the stairwell at {points:?}",
                    part.material
                );
            }
        }
    }
}
