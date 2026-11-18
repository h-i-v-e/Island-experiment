use island_building::{BuildingRecipe, MaterialSlot, PartitionAxis, WallStyle, generate};

#[test]
fn partition_frames_cover_both_sides_with_shared_clean_daub() {
    for axis in [PartitionAxis::X, PartitionAxis::Y] {
        let mut recipe = BuildingRecipe::cottage();
        recipe.chimney = None;
        recipe.atlas_size = 128;
        recipe.walls.style = WallStyle::WattleAndDaub;
        recipe.grub_filter.enabled = true;
        recipe.partitions[0].axis = axis;
        recipe.partitions[0].offset = 0.;
        recipe
            .openings
            .retain(|o| o.kind == island_building::OpeningKind::Door);
        recipe.openings[0].offset = 2.;
        let p = generate(&recipe).unwrap();
        let wall = p
            .materials
            .iter()
            .find(|m| m.slot == MaterialSlot::Wall)
            .unwrap();
        let interior = p
            .materials
            .iter()
            .find(|m| m.slot == MaterialSlot::Interior)
            .unwrap();
        assert_eq!(
            interior.physical_size.map(f32::to_bits),
            wall.physical_size.map(f32::to_bits)
        );
        assert_eq!(interior.normal, wall.normal, "same daub relief");
        assert_ne!(interior.albedo, wall.albedo, "independent interior palette");
        assert!(!island_building::grub::exterior_material(
            &recipe,
            MaterialSlot::Interior
        ));
        assert!(!island_building::grub::exterior_material(
            &recipe,
            MaterialSlot::Timber
        ));
        let across = usize::from(axis == PartitionAxis::X);
        let along = 1 - across;
        let timber = &p.lods[0]
            .parts
            .iter()
            .find(|p| p.material == MaterialSlot::Timber)
            .unwrap()
            .mesh;
        for sign in [-1., 1.] {
            let surface = sign * (recipe.wall_thickness * 0.5 + recipe.walls.frame_projection);
            assert!(
                timber
                    .positions
                    .iter()
                    .zip(&timber.normals)
                    .any(|(p, n)| (p[across] - surface).abs() < 0.0001
                        && p[along].abs() < 1.5
                        && p[2] > 0.2
                        && p[2] < 2.3
                        && n[across] * sign > 0.99),
                "both partition faces have framing"
            );
        }
        let host_position = [recipe.width, recipe.depth][along] * 0.5 - recipe.wall_thickness;
        let post_position = recipe.wall_thickness * 0.5 + recipe.walls.beam_width;
        for end in [-1., 1.] {
            for side in [-1., 1.] {
                assert!(
                    timber
                        .positions
                        .iter()
                        .zip(&timber.normals)
                        .any(|(point, normal)| {
                            (point[across] - side * post_position).abs() < 0.0001
                                && (point[along] - end * host_position).abs()
                                    < recipe.walls.beam_width + 0.01
                                && point[2] > 2.
                                && normal[across] * side > 0.99
                        }),
                    "partition end post at {axis:?} {end}, facing {side}"
                );
            }
        }
        let restored = generate(&p.recipe).unwrap();
        assert_eq!(p.hash, restored.hash);
        assert_eq!(p.lods, restored.lods);
    }
}

#[test]
fn plain_walls_keep_flat_untextured_interiors() {
    let mut recipe = BuildingRecipe::cottage();
    recipe.atlas_size = 128;
    recipe.walls.style = WallStyle::Plain;
    let p = generate(&recipe).unwrap();
    assert!(!p.materials.iter().any(|m| m.slot == MaterialSlot::Interior));
}

#[test]
fn stairwell_frames_follow_each_floor_on_both_axes() {
    for axis in [island_building::StairAxis::X, island_building::StairAxis::Y] {
        let mut recipe = BuildingRecipe::hut();
        recipe.chimney = None;
        recipe.width = 10.;
        recipe.depth = 12.;
        recipe.storeys = 2;
        recipe.walls.style = WallStyle::WattleAndDaub;
        recipe.atlas_size = 128;
        recipe.staircase.axis = axis;
        let p = generate(&recipe).unwrap();
        let stairs = p.layout.staircase.as_ref().unwrap();
        let room = stairs.stairwell.as_ref().unwrap();
        let plan = room.enclosure.unwrap();
        let normal = plan.door_side.outward();
        let a = stairs.point(0., 0., 0.);
        let b = stairs.point(normal[0], normal[1], 0.);
        let normal = [b[0] - a[0], b[1] - a[1]];
        let mesh = &p.lods[0]
            .parts
            .iter()
            .find(|p| p.material == MaterialSlot::Timber)
            .unwrap()
            .mesh;
        for doorway in &room.doorways {
            let floor = recipe.floor_height(doorway.storey);
            for sign in [-1., 1.] {
                let plane = doorway.centre[0] * normal[0]
                    + doorway.centre[1] * normal[1]
                    + sign * (room.wall_thickness * 0.5 + recipe.walls.frame_projection);
                assert!(
                    mesh.positions
                        .iter()
                        .zip(&mesh.normals)
                        .any(
                            |(p, n)| (p[0] * normal[0] + p[1] * normal[1] - plane).abs() < 0.0001
                                && p[2] > floor + room.door_height + 0.08
                                && p[2] <= floor + recipe.wall_height + 0.1
                                && (n[0] * normal[0] + n[1] * normal[1]) * sign > 0.99
                        ),
                    "framing above each landing door, both sides"
                );
            }
        }
    }
}
