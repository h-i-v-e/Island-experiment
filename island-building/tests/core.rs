use island_building::{
    BuildingMesh, BuildingPrototype, BuildingRecipe, Facade, MaterialSlot, OpeningKind,
    PartitionAxis, PartitionRecipe, collision::BoxCollider, generate, generate_with_base, validate,
};

fn preview(mut recipe: BuildingRecipe) -> BuildingRecipe {
    recipe.atlas_size = 128;
    recipe
}

#[test]
fn presets_have_valid_accessible_geometry_and_three_distinct_lods() {
    for recipe in [
        BuildingRecipe::hut(),
        BuildingRecipe::cottage(),
        BuildingRecipe::workshop(),
    ] {
        let prototype = generate(&preview(recipe)).expect("valid preset");
        assert!(!prototype.doors.is_empty());
        assert_eq!(
            prototype.layout.room_count,
            prototype.recipe.partitions.len() + 1
        );
        assert!(prototype.lods[0].triangle_count() > prototype.lods[1].triangle_count());
        assert!(prototype.lods[1].triangle_count() > prototype.lods[2].triangle_count());
        for lod in &prototype.lods {
            for part in &lod.parts {
                check_mesh(&part.mesh);
            }
        }
        for lod in &prototype.lods[1..] {
            assert!(lod.parts.iter().all(|p| !matches!(
                p.material,
                MaterialSlot::Glass | MaterialSlot::Interior | MaterialSlot::Floor
            )));
        }
        assert!(
            prototype.lods[2]
                .parts
                .iter()
                .all(|p| p.material == MaterialSlot::Atlas)
        );
        assert_eq!(prototype.atlas.albedo.len(), 128 * 128 * 4);
        assert!(prototype.atlas.albedo.chunks_exact(4).all(|p| p[3] == 255));
    }
}
#[allow(clippy::many_single_char_names)] // Triangle winding uses conventional edge vectors.
fn check_mesh(mesh: &BuildingMesh) {
    assert_eq!(mesh.positions.len(), mesh.normals.len());
    assert_eq!(mesh.positions.len(), mesh.uvs.len());
    assert!(mesh.positions.iter().flatten().all(|v| v.is_finite()));
    for tri in mesh.indices.chunks_exact(3) {
        let a = mesh.positions[tri[0] as usize];
        let b = mesh.positions[tri[1] as usize];
        let c = mesh.positions[tri[2] as usize];
        let u = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
        let v = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
        let n = [
            u[1] * v[2] - u[2] * v[1],
            u[2] * v[0] - u[0] * v[2],
            u[0] * v[1] - u[1] * v[0],
        ];
        assert!(
            n.iter().map(|v| v * v).sum::<f32>() > 1e-14,
            "non-degenerate triangle"
        );
        assert!(
            n.iter()
                .zip(mesh.normals[tri[0] as usize])
                .map(|(a, b)| a * b)
                .sum::<f32>()
                > 0.,
            "normal follows winding"
        );
    }
}

#[test]
fn saved_recipe_reproduces_meshes_materials_and_atlas() {
    let recipe = preview(BuildingRecipe::hut());
    let bytes = serde_json::to_vec(&recipe).unwrap();
    let loaded = serde_json::from_slice(&bytes).unwrap();
    let first = generate(&recipe).unwrap();
    let second = generate(&loaded).unwrap();
    assert_eq!(first, second);
    let mut changed = recipe;
    changed.materials.wall = [0.1, 0.3, 0.6];
    let third = generate(&changed).unwrap();
    assert_eq!(first.layout, third.layout);
    assert_eq!(first.lods, third.lods);
    assert_ne!(first.hash, third.hash);
    assert_ne!(first.atlas.albedo, third.atlas.albedo);
}

#[test]
fn door_and_window_openings_are_true_holes_and_door_collision_follows_hinge() {
    let prototype = generate(&preview(BuildingRecipe::hut())).unwrap();
    let door = &prototype.doors[0];
    let origin = [0., -4., 1.];
    let direction = [0., 1., 0.];
    assert!(
        prototype
            .colliders
            .iter()
            .all(|c| c.ray_hit(origin, direction, 2.).is_none()),
        "wall collision leaves exterior door clear"
    );
    assert!(
        door.collider_at(0.)
            .ray_hit(origin, direction, 2.)
            .is_some(),
        "closed door blocks opening"
    );
    assert!(
        door.collider_at(door.swing_angle)
            .ray_hit(origin, direction, 2.)
            .is_none(),
        "open leaf clears doorway"
    );
    assert!(
        !prototype
            .colliders
            .iter()
            .any(|c| c.intersects_capsule([0., -2.4, 0.], 0.3, 1.8)),
        "flush floor contact and doorway have clearance"
    );
    assert!(
        prototype
            .colliders
            .iter()
            .any(|c| c.intersects_capsule([1.5, -2.4, 0.], 0.3, 1.8)),
        "solid wall blocks player"
    );
    let window = prototype
        .recipe
        .openings
        .iter()
        .find(|o| o.kind == OpeningKind::Window)
        .unwrap();
    let source = window.facade.point(
        &prototype.recipe,
        window.offset,
        window.sill + window.height * 0.5,
        -1.,
    );
    let n = window.facade.normal();
    let ray = n.map(|v| -v);
    assert!(
        !wall_hit(&prototype, source, ray, 1.2),
        "wall mesh does not cover window"
    );
    assert!(
        !wall_hit(&prototype, origin, direction, 2.),
        "wall mesh does not cover door"
    );
}
fn wall_hit(p: &BuildingPrototype, origin: [f32; 3], direction: [f32; 3], limit: f32) -> bool {
    p.lods[0]
        .parts
        .iter()
        .filter(|part| part.material == MaterialSlot::Wall)
        .any(|part| {
            part.mesh.indices.chunks_exact(3).any(|t| {
                triangle_hit(t.map_ref(&part.mesh.positions), origin, direction)
                    .is_some_and(|d| d < limit)
            })
        })
}
trait TrianglePoints {
    fn map_ref(&self, positions: &[[f32; 3]]) -> [[f32; 3]; 3];
}
impl TrianglePoints for [u32] {
    fn map_ref(&self, positions: &[[f32; 3]]) -> [[f32; 3]; 3] {
        [
            positions[self[0] as usize],
            positions[self[1] as usize],
            positions[self[2] as usize],
        ]
    }
}
#[allow(clippy::many_single_char_names)] // Independent Moller-Trumbore reference.
fn triangle_hit(p: [[f32; 3]; 3], o: [f32; 3], d: [f32; 3]) -> Option<f32> {
    let sub = |a: [f32; 3], b: [f32; 3]| std::array::from_fn::<_, 3, _>(|i| a[i] - b[i]);
    let cross = |a: [f32; 3], b: [f32; 3]| {
        [
            a[1] * b[2] - a[2] * b[1],
            a[2] * b[0] - a[0] * b[2],
            a[0] * b[1] - a[1] * b[0],
        ]
    };
    let dot = |a: [f32; 3], b: [f32; 3]| (0..3).map(|i| a[i] * b[i]).sum::<f32>();
    let a = sub(p[1], p[0]);
    let b = sub(p[2], p[0]);
    let h = cross(d, b);
    let det = dot(a, h);
    if det.abs() < 1e-7 {
        return None;
    }
    let q = sub(o, p[0]);
    let u = dot(q, h) / det;
    if !(0.0..=1.0).contains(&u) {
        return None;
    }
    let c = cross(q, a);
    let v = dot(d, c) / det;
    if v < 0. || u + v > 1. {
        return None;
    }
    let distance = dot(b, c) / det;
    (distance >= 0.).then_some(distance)
}

#[test]
fn invalid_layouts_are_diagnosed_before_building() {
    let mut r = BuildingRecipe::hut();
    r.width = f32::NAN;
    assert!(validate(&r).is_err());
    let mut r = BuildingRecipe::hut();
    r.openings.retain(|o| o.kind == OpeningKind::Window);
    assert!(
        validate(&r)
            .unwrap_err()
            .to_string()
            .contains("exterior door")
    );
    let mut r = BuildingRecipe::hut();
    r.openings[1].facade = Facade::South;
    r.openings[1].offset = 0.;
    assert!(validate(&r).unwrap_err().to_string().contains("overlaps"));
    let mut r = BuildingRecipe::cottage();
    r.partitions.push(PartitionRecipe {
        id: "crossing".into(),
        axis: PartitionAxis::Y,
        offset: 0.,
        door_offset: 0.,
        door_width: 1.1,
        door_height: 2.2,
    });
    assert!(
        validate(&r)
            .unwrap_err()
            .to_string()
            .contains("crossing partitions")
    );
    let mut r = BuildingRecipe::hut();
    r.atlas_size = u32::MAX;
    assert!(validate(&r).is_err());
}

#[test]
fn local_capsule_and_rotated_door_queries_are_consistent() {
    let collider = BoxCollider {
        min: [-1., -1., 0.],
        max: [1., 1., 2.],
    };
    assert!(collider.intersects_capsule([1.2, 0., 0.], 0.3, 1.8));
    assert!(!collider.intersects_capsule([1.4, 0., 0.], 0.3, 1.8));
    assert_eq!(collider.ray_hit([0., -3., 1.], [0., 1., 0.], 10.), Some(2.));
    assert!(collider.ray_hit([2., -3., 1.], [0., 1., 0.], 10.).is_none());
}

#[test]
fn atlas_changes_with_openings_and_retains_roof_chart_variation() {
    let recipe = preview(BuildingRecipe::hut());
    let first = generate(&recipe).unwrap();
    let mut moved = recipe;
    moved.openings[1].offset = 1.;
    let second = generate(&moved).unwrap();
    assert_ne!(first.atlas.albedo, second.atlas.albedo);
    assert!(
        first
            .atlas
            .normal
            .chunks_exact(4)
            .any(|p| p[0] != 128 || p[1] != 128)
    );
    assert!(
        first.lods[2].parts[0]
            .mesh
            .uvs
            .iter()
            .flatten()
            .all(|v| (0.0..=1.0).contains(v))
    );
}

#[test]
fn exported_material_paths_do_not_change_prototype_identity() {
    let folder =
        std::env::temp_dir().join(format!("building-material-hash-{}", std::process::id()));
    std::fs::create_dir_all(&folder).unwrap();
    let source = include_bytes!("../../island-rs/texture-recipes/cracked-stone.json");
    std::fs::write(folder.join("a.json"), source).unwrap();
    std::fs::write(folder.join("b.json"), source).unwrap();
    let mut a = preview(BuildingRecipe::hut());
    a.materials.wall_recipe = Some("a.json".into());
    let mut b = a.clone();
    b.materials.wall_recipe = Some("b.json".into());
    let first = generate_with_base(&a, &folder).unwrap();
    let second = generate_with_base(&b, &folder).unwrap();
    assert_eq!(first.hash, second.hash);
    assert_eq!(first.atlas, second.atlas);
    std::fs::remove_dir_all(folder).unwrap();
}

#[test]
fn configured_swings_reject_corner_and_partition_collisions() {
    let mut corner = BuildingRecipe::hut();
    corner.openings[0].offset = -2.1;
    assert!(
        validate(&corner)
            .unwrap_err()
            .to_string()
            .contains("door swing intersects")
    );
    corner.openings[0].hinge = island_building::HingeSide::Right;
    assert!(
        validate(&corner).is_ok(),
        "opposite hinge swings safely toward room centre"
    );
    let mut blocked = BuildingRecipe::hut();
    blocked.partitions.push(PartitionRecipe {
        id: "entrance-blocker".into(),
        axis: PartitionAxis::X,
        offset: -1.1,
        door_offset: 1.,
        door_width: 1.1,
        door_height: 2.2,
    });
    assert!(
        validate(&blocked)
            .unwrap_err()
            .to_string()
            .contains("door swing intersects")
    );
}

#[test]
fn both_hinges_and_roof_families_work_on_every_facade() {
    for facade in Facade::ALL {
        for hinge in [
            island_building::HingeSide::Left,
            island_building::HingeSide::Right,
        ] {
            for roof in [
                island_building::RoofKind::Gable,
                island_building::RoofKind::Shed,
            ] {
                let mut recipe = preview(BuildingRecipe::hut());
                recipe.seed = u64::try_from(facade.index()).unwrap() + 100;
                recipe.roof.kind = roof;
                recipe
                    .openings
                    .retain(|o| o.kind == OpeningKind::Door || o.facade != facade);
                recipe.openings[0].facade = facade;
                recipe.openings[0].hinge = hinge;
                let prototype = generate(&recipe).expect("valid facade/hinge/roof combination");
                let door = &prototype.doors[0];
                let origin = facade.point(&recipe, 0., 1., -2.);
                let direction = facade.normal().map(|v| -v);
                assert!(
                    door.collider_at(0.)
                        .ray_hit(origin, direction, 3.)
                        .is_some()
                );
                assert!(
                    door.collider_at(door.swing_angle)
                        .ray_hit(origin, direction, 3.)
                        .is_none()
                );
                for lod in &prototype.lods {
                    for part in &lod.parts {
                        check_mesh(&part.mesh);
                    }
                }
            }
        }
    }
}

#[test]
fn facade_atlas_preserves_opening_elevations_above_a_thick_floor() {
    let mut recipe = preview(BuildingRecipe::hut());
    recipe.floor_thickness = 0.5;
    let prototype = generate(&recipe).unwrap();
    let mesh = &prototype.lods[2].parts[0].mesh;
    let span = recipe.wall_height + recipe.roof.rise + recipe.floor_thickness;
    for (point, uv) in mesh.positions[..4].iter().zip(&mesh.uvs[..4]) {
        let normalized = (0.95 - uv[1] * 2.) / 0.9;
        let height = normalized * span - recipe.floor_thickness;
        assert!(
            (height - point[2]).abs() < 0.00001,
            "atlas facade uses the same absolute opening height as LOD0"
        );
    }
}

#[test]
fn fitted_door_leaves_have_small_perimeter_clearances_on_every_facade() {
    for facade in Facade::ALL {
        for hinge in [
            island_building::HingeSide::Left,
            island_building::HingeSide::Right,
        ] {
            let mut recipe = preview(BuildingRecipe::hut());
            recipe
                .openings
                .retain(|o| o.kind == OpeningKind::Door || o.facade != facade);
            recipe.openings[0].facade = facade;
            recipe.openings[0].hinge = hinge;
            let prototype =
                generate(&recipe).expect("fitted door retains its unobstructed full swing");
            let door = &prototype.doors[0];
            let opening = &recipe.openings[0];
            let tangent = facade.tangent();
            let (sine, cosine) = door.closed_angle.sin_cos();
            let leaf_points = door
                .mesh
                .positions
                .iter()
                .map(|point| {
                    [
                        door.hinge[0] + cosine * point[0] - sine * point[1],
                        door.hinge[1] + sine * point[0] + cosine * point[1],
                        door.hinge[2] + point[2],
                    ]
                })
                .collect::<Vec<_>>();
            let along = |point: &[f32; 3]| point[0] * tangent[0] + point[1] * tangent[1];
            let left = leaf_points.iter().map(along).fold(f32::INFINITY, f32::min);
            let right = leaf_points
                .iter()
                .map(along)
                .fold(f32::NEG_INFINITY, f32::max);
            let bottom = leaf_points
                .iter()
                .map(|point| point[2])
                .fold(f32::INFINITY, f32::min);
            let top = leaf_points
                .iter()
                .map(|point| point[2])
                .fold(f32::NEG_INFINITY, f32::max);
            assert!((left - (opening.offset - opening.width * 0.5) - 0.003).abs() < 0.00001);
            assert!((opening.offset + opening.width * 0.5 - right - 0.003).abs() < 0.00001);
            assert!((opening.height - top - 0.003).abs() < 0.00001);
            assert!((bottom - 0.006).abs() < 0.00001);
        }
    }
}

#[test]
fn doorway_linings_and_window_reveals_face_the_opening_instead_of_culling() {
    let prototype = generate(&preview(BuildingRecipe::hut())).unwrap();
    let recipe = &prototype.recipe;
    let trim = &prototype.lods[0]
        .parts
        .iter()
        .find(|part| part.material == MaterialSlot::Trim)
        .unwrap()
        .mesh;
    for opening in &recipe.openings {
        let centre = opening.facade.point(
            recipe,
            opening.offset,
            opening.sill + opening.height * 0.5,
            recipe.wall_thickness * 0.5,
        );
        let tangent = opening.facade.tangent();
        for direction in [tangent, tangent.map(|v| -v), [0., 0., 1.]] {
            assert!(
                front_facing_hit(trim, centre, direction, opening.width.max(opening.height)),
                "lining must render when viewed from the opening"
            );
        }
        if opening.kind == OpeningKind::Window {
            assert!(
                front_facing_hit(trim, centre, [0., 0., -1.], opening.height),
                "window sill faces upward into opening"
            );
        }
    }
}
fn front_facing_hit(
    mesh: &BuildingMesh,
    origin: [f32; 3],
    direction: [f32; 3],
    limit: f32,
) -> bool {
    mesh.indices.chunks_exact(3).any(|indices| {
        let normal = mesh.normals[indices[0] as usize];
        let facing = (0..3)
            .map(|axis| normal[axis] * direction[axis])
            .sum::<f32>();
        facing < -0.99
            && triangle_hit(indices.map_ref(&mesh.positions), origin, direction)
                .is_some_and(|distance| distance <= limit)
    })
}

#[test]
fn latch_side_knobs_are_smooth_geometry_on_both_faces_for_all_hinges_and_facades() {
    for facade in Facade::ALL {
        for hinge in [
            island_building::HingeSide::Left,
            island_building::HingeSide::Right,
        ] {
            let mut recipe = preview(BuildingRecipe::hut());
            recipe
                .openings
                .retain(|o| o.kind == OpeningKind::Door || o.facade != facade);
            recipe.openings[0].facade = facade;
            recipe.openings[0].hinge = hinge;
            let prototype = generate(&recipe).unwrap();
            let door = &prototype.doors[0];
            check_mesh(&door.hardware_mesh);
            assert!(!door.hardware_mesh.indices.is_empty());
            assert!(
                door.hardware_mesh
                    .positions
                    .iter()
                    .all(|point| point[0] > door.width - 0.12
                        && point[0] < door.width - 0.02
                        && point[2] > 0.95
                        && point[2] < 1.1)
            );
            assert!(
                door.hardware_mesh
                    .positions
                    .iter()
                    .any(|point| point[1] > door.thickness * 0.5 + 0.05)
            );
            assert!(
                door.hardware_mesh
                    .positions
                    .iter()
                    .any(|point| point[1] < -door.thickness * 0.5 - 0.05)
            );
            assert!(
                door.mesh
                    .positions
                    .iter()
                    .all(|point| point[1].abs() <= door.thickness * 0.5)
            );
            assert!(
                prototype
                    .lods
                    .iter()
                    .flat_map(|lod| &lod.parts)
                    .all(|part| part.material != MaterialSlot::Hardware)
            );
            let (sine, cosine) = door.closed_angle.sin_cos();
            let free_latch = [
                door.hinge[0] + cosine * (door.width - 0.075),
                door.hinge[1] + sine * (door.width - 0.075),
                door.hinge[2] + 1.02,
            ];
            let along = free_latch[0] * facade.tangent()[0] + free_latch[1] * facade.tangent()[1];
            let expected = if hinge == island_building::HingeSide::Left {
                recipe.openings[0].width * 0.5 - 0.078
            } else {
                -recipe.openings[0].width * 0.5 + 0.078
            };
            assert!(
                (along - expected).abs() < 0.00001,
                "knob follows the latch edge after the closed hinge rotation"
            );
        }
    }
}
