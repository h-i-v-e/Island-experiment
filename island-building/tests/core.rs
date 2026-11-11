use island_building::{
    BuildingMesh, BuildingPrototype, BuildingRecipe, Facade, MaterialSlot, OpeningKind,
    PartitionAxis, PartitionRecipe, RoofCovering, RoofKind, WallStyle, collision::BoxCollider,
    generate, generate_with_base, validate,
};

fn preview(mut recipe: BuildingRecipe) -> BuildingRecipe {
    recipe.atlas_size = 128;
    recipe
}

#[test]
fn half_timbered_walls_keep_clear_openings_real_relief_and_a_baked_distant_frame() {
    for roof in [RoofKind::Gable, RoofKind::Shed] {
        let mut recipe = preview(BuildingRecipe::hut());
        recipe.roof.kind = roof;
        recipe.roof.overhang = 0.;
        let plain = generate(&recipe).unwrap();
        recipe.walls.style = WallStyle::WattleAndDaub;
        recipe.materials.wall = recipe.walls.style.default_colour();
        let prototype = generate(&recipe).unwrap();
        assert_eq!(
            prototype
                .colliders
                .iter()
                .filter(|c| c.min[2] <= 0.)
                .count(),
            plain.colliders.iter().filter(|c| c.min[2] <= 0.).count() + 4
        );
        assert_eq!(prototype.doors, plain.doors);
        for lod in &prototype.lods[..2] {
            let mesh = &lod
                .parts
                .iter()
                .find(|p| p.material == MaterialSlot::Timber)
                .expect("real timber frame")
                .mesh;
            check_mesh(mesh);
            assert!(
                mesh.positions
                    .iter()
                    .all(|p| prototype.layout.contains(*p, 0.00001))
            );
            for facade in Facade::ALL {
                let post = facade.point(
                    &recipe,
                    -facade.length(&recipe) * 0.5 + recipe.walls.beam_width * 0.5,
                    0.5,
                    -0.3,
                );
                let distance =
                    mesh_ray(mesh, post, facade.normal().map(|v| -v)).expect("corner post front");
                assert!((distance - (0.3 - recipe.walls.frame_projection)).abs() < 0.00001);
            }
            for opening in &recipe.openings {
                let point = opening.facade.point(
                    &recipe,
                    opening.offset,
                    opening.sill + opening.height * 0.75,
                    -0.3,
                );
                assert!(
                    mesh_ray(mesh, point, opening.facade.normal().map(|v| -v))
                        .is_none_or(|d| d > 0.3 + recipe.wall_thickness)
                );
            }
        }
        assert!(prototype.lods[0].triangle_count() > prototype.lods[1].triangle_count());
        assert!(
            prototype.lods[2]
                .parts
                .iter()
                .all(|p| p.material == MaterialSlot::Atlas)
        );
        assert_eq!(prototype.lods[2], plain.lods[2]);
        assert_ne!(prototype.atlas.albedo, plain.atlas.albedo);
        assert!(
            prototype
                .materials
                .iter()
                .any(|m| m.slot == MaterialSlot::Timber)
        );
        let regenerated = generate(&recipe).unwrap();
        assert_eq!(prototype.hash, regenerated.hash);
        assert_eq!(prototype.lods, regenerated.lods);
        assert_eq!(prototype.atlas, regenerated.atlas);
    }
}

#[test]
fn old_recipes_default_to_plain_walls_and_frame_controls_are_validated() {
    let mut value = serde_json::to_value(BuildingRecipe::hut()).unwrap();
    value.as_object_mut().unwrap().remove("walls");
    for field in ["timber", "timber_recipe"] {
        value["materials"].as_object_mut().unwrap().remove(field);
    }
    let mut recipe: BuildingRecipe = serde_json::from_value(value).unwrap();
    assert_eq!(recipe.walls.style, WallStyle::Plain);
    assert!(recipe.materials.timber_recipe.is_none());
    for invalid in [f32::NAN, -0.01, 0.5] {
        recipe.walls.frame_projection = invalid;
        assert!(validate(&recipe).is_err());
    }
}

#[test]
fn opening_crosspieces_fit_against_casings_and_sills_without_blocking_apertures() {
    for facade in Facade::ALL {
        for roof in [RoofKind::Gable, RoofKind::Shed] {
            let mut recipe = preview(BuildingRecipe::hut());
            recipe.chimney = None;
            recipe.roof.kind = roof;
            recipe.walls.style = WallStyle::WattleAndDaub;
            recipe.openings.truncate(2);
            recipe.openings[0].facade = facade;
            recipe.openings[0].offset = -1.1;
            recipe.openings[1].facade = facade;
            recipe.openings[1].offset = 1.1;
            recipe.openings[1].width = 0.7;
            let prototype = generate(&recipe).unwrap();
            for lod in &prototype.lods[..2] {
                let timber = &lod
                    .parts
                    .iter()
                    .find(|p| p.material == MaterialSlot::Timber)
                    .unwrap()
                    .mesh;
                check_mesh(timber);
                for opening in &recipe.openings {
                    let above = opening.sill
                        + opening.height
                        + if opening.kind == OpeningKind::Window {
                            0.065
                        } else {
                            0.07
                        };
                    for fraction in [-0.45, 0., 0.45] {
                        let along = opening.offset + opening.width * fraction;
                        let heights = [
                            Some(above + 0.002),
                            (opening.kind == OpeningKind::Window).then_some(opening.sill - 0.072),
                        ];
                        for z in heights.into_iter().flatten() {
                            let origin = facade.point(&recipe, along, z, -0.3);
                            let distance = mesh_ray(timber, origin, facade.normal().map(|v| -v))
                                .expect("horizontal timber meets opening trim");
                            assert!(
                                (distance - (0.3 - recipe.walls.frame_projection)).abs() < 0.00001
                            );
                        }
                    }
                    let origin = facade.point(
                        &recipe,
                        opening.offset + opening.width * 0.25,
                        opening.sill + opening.height * 0.75,
                        -0.3,
                    );
                    assert!(
                        mesh_ray(timber, origin, facade.normal().map(|v| -v))
                            .is_none_or(|d| d > 0.3 + recipe.wall_thickness),
                        "crosspieces leave the aperture clear"
                    );
                }
            }
        }
    }
}

#[test]
fn lod0_timber_passes_through_walls_and_faces_the_room_without_blocking_openings() {
    for facade in Facade::ALL {
        for roof in [RoofKind::Gable, RoofKind::Shed] {
            for thickness in [0.08, 0.2, 0.6] {
                let mut recipe = preview(BuildingRecipe::hut());
                recipe.chimney = None;
                recipe.roof.kind = roof;
                recipe.wall_thickness = thickness;
                recipe.walls.style = WallStyle::WattleAndDaub;
                recipe.openings.truncate(2);
                recipe.openings[0].facade = facade;
                recipe.openings[0].offset = -0.8;
                recipe.openings[1].facade = facade;
                recipe.openings[1].offset = 0.8;
                recipe.openings[1].width = 0.7;
                let prototype = generate(&recipe).unwrap();
                let timber = &prototype.lods[0]
                    .parts
                    .iter()
                    .find(|p| p.material == MaterialSlot::Timber)
                    .unwrap()
                    .mesh;
                check_mesh(timber);
                let inward_face = |wall: Facade, along, z, projection: f32| {
                    let origin = wall.point(&recipe, along, z, thickness + 0.3);
                    let direction = wall.normal();
                    let distance = timber
                        .indices
                        .chunks_exact(3)
                        .filter_map(|tri| {
                            let normal = timber.normals[tri[0] as usize];
                            (normal
                                .iter()
                                .zip(direction)
                                .map(|(a, b)| a * b)
                                .sum::<f32>()
                                < -0.99)
                                .then(|| {
                                    triangle_hit(tri.map_ref(&timber.positions), origin, direction)
                                })
                                .flatten()
                        })
                        .min_by(f32::total_cmp)
                        .expect("timber face visible from inside");
                    assert!((distance - (0.3 - projection)).abs() < 0.00001);
                };
                for wall in Facade::ALL {
                    let corner =
                        wall.length(&recipe) * 0.5 - thickness - recipe.walls.beam_width * 0.5;
                    for along in [-corner, corner] {
                        inward_face(wall, along, 0.5, recipe.walls.beam_width);
                    }
                }
                for opening in &recipe.openings {
                    let above = opening.sill
                        + opening.height
                        + if opening.kind == OpeningKind::Window {
                            0.065
                        } else {
                            0.07
                        };
                    inward_face(
                        facade,
                        opening.offset,
                        above + 0.002,
                        recipe.walls.frame_projection,
                    );
                    if opening.kind == OpeningKind::Window {
                        inward_face(
                            facade,
                            opening.offset,
                            opening.sill - 0.072,
                            recipe.walls.frame_projection,
                        );
                    }
                    let origin = facade.point(
                        &recipe,
                        opening.offset,
                        opening.sill + opening.height * 0.75,
                        thickness + 0.3,
                    );
                    assert!(
                        mesh_ray(timber, origin, facade.normal())
                            .is_none_or(|d| d > thickness + 0.3),
                        "clear aperture from inside"
                    );
                }
                let other = Facade::ALL[(facade.index() + 1) % 4];
                inward_face(other, 0., 0.5, recipe.walls.frame_projection);
                let origin = other.point(&recipe, 0.2, 0.5, thickness * 0.5);
                let hit = mesh_ray(timber, origin, other.tangent().map(|v| -v)).unwrap();
                assert!(
                    (hit - (0.2 - recipe.walls.beam_width * 0.5)).abs() < 0.00001,
                    "post side extends through the wall core"
                );
            }
        }
    }
}

#[test]
fn ceiling_meets_wall_plate_tops_and_keeps_plaster_fireplace_and_collision_closed() {
    let mut recipe = preview(BuildingRecipe::hut());
    recipe.walls.style = WallStyle::WattleAndDaub;
    recipe.roof.covering = RoofCovering::Thatch;
    recipe.roof.rise = 4.;
    let prototype = generate(&recipe).unwrap();
    let mesh = |slot| {
        &prototype.lods[0]
            .parts
            .iter()
            .find(|p| p.material == slot)
            .unwrap()
            .mesh
    };
    let ceiling = recipe.wall_height + 0.1;
    let eye = [0., 0.4, 1.65];
    let hit = mesh_ray(mesh(MaterialSlot::Interior), eye, [0., 0., 1.]).unwrap();
    assert!((hit - (ceiling - eye[2])).abs() < 0.00001);
    let collision = prototype
        .colliders
        .iter()
        .filter_map(|c| c.ray_hit(eye, [0., 0., 1.], 10.))
        .min_by(f32::total_cmp)
        .unwrap();
    assert!(
        (hit - collision).abs() < 0.00001,
        "ceiling collision follows its new height"
    );
    for facade in [Facade::East, Facade::West] {
        let origin = facade.point(
            &recipe,
            0.8,
            recipe.wall_height + 0.095,
            recipe.wall_thickness + 0.3,
        );
        let timber = mesh_ray(mesh(MaterialSlot::Timber), origin, facade.normal()).unwrap();
        assert!(
            (timber - (0.3 - recipe.walls.frame_projection)).abs() < 0.00001,
            "full top rail is exposed below the ceiling"
        );
        let origin = facade.point(&recipe, 0.8, ceiling - 0.02, recipe.wall_thickness + 0.3);
        let infill = mesh_ray(mesh(MaterialSlot::Interior), origin, facade.normal()).unwrap();
        assert!(
            (infill - 0.3).abs() < 0.00001,
            "plaster remains behind the bearer up to the ceiling"
        );
    }
    let chimney = recipe.chimney.as_ref().unwrap();
    let front = recipe.wall_thickness + chimney.depth;
    let origin = chimney
        .facade
        .point(&recipe, chimney.offset, ceiling - 0.02, front + 0.3);
    let masonry = mesh_ray(mesh(MaterialSlot::Chimney), origin, chimney.facade.normal()).unwrap();
    assert!(
        (masonry - 0.3).abs() < 0.00001,
        "straight breast reaches the raised ceiling"
    );
}

#[test]
fn near_joists_span_the_window_walls_at_the_plate_soffit_with_matching_collision() {
    for roof in [RoofKind::Gable, RoofKind::Shed] {
        for covering in [
            RoofCovering::StandingSeam,
            RoofCovering::Slate,
            RoofCovering::Thatch,
        ] {
            let mut recipe = preview(BuildingRecipe::hut());
            recipe.walls.style = WallStyle::WattleAndDaub;
            recipe.roof.kind = roof;
            recipe.roof.covering = covering;
            recipe.roof.rise = 4.;
            let prototype = generate(&recipe).unwrap();
            let timber = &prototype.lods[0]
                .parts
                .iter()
                .find(|p| p.material == MaterialSlot::Timber)
                .unwrap()
                .mesh;
            check_mesh(timber);
            let plate_top = recipe.wall_height
                + if covering == RoofCovering::Thatch {
                    0.1
                } else {
                    0.
                };
            let bottom = plate_top - recipe.walls.beam_width;
            let end = recipe.width * 0.5 - recipe.wall_thickness - recipe.walls.frame_projection;
            for x in [-end + 0.001, 0., end - 0.001] {
                let eye = [x, 0., 1.65];
                let hit =
                    mesh_ray(timber, eye, [0., 0., 1.]).expect("joist across the entire room");
                assert!(
                    (hit - (bottom - eye[2])).abs() < 0.00001,
                    "underside meets plate bottom"
                );
                let collision = prototype
                    .colliders
                    .iter()
                    .filter_map(|c| c.ray_hit(eye, [0., 0., 1.], 10.))
                    .min_by(f32::total_cmp)
                    .unwrap();
                assert!((collision - hit).abs() < 0.00001);
            }
            let middle = &prototype.lods[1]
                .parts
                .iter()
                .find(|p| p.material == MaterialSlot::Timber)
                .unwrap()
                .mesh;
            assert!(
                mesh_ray(middle, [0., 0., 1.65], [0., 0., 1.]).is_none(),
                "LOD1 has no interior joists"
            );
        }
    }
}

#[test]
fn half_timber_corners_close_and_roof_bands_cover_the_thatch_wall_overlap() {
    for roof in [RoofKind::Gable, RoofKind::Shed] {
        for projection in [0.005, 0.04] {
            let mut recipe = preview(BuildingRecipe::hut());
            recipe.roof.kind = roof;
            recipe.walls.style = WallStyle::WattleAndDaub;
            recipe.walls.frame_projection = projection;
            recipe.roof.covering = RoofCovering::Thatch;
            recipe.roof.rise = 4.;
            recipe.roof.thickness = 0.5;
            let prototype = generate(&recipe).unwrap();
            for lod in &prototype.lods[..2] {
                let timber = &lod
                    .parts
                    .iter()
                    .find(|p| p.material == MaterialSlot::Timber)
                    .unwrap()
                    .mesh;
                check_mesh(timber);
                assert!(
                    timber
                        .positions
                        .iter()
                        .all(|p| prototype.layout.contains(*p, 0.00001))
                );
                for facade in Facade::ALL {
                    let half = facade.length(&recipe) * 0.5;
                    let inward = facade.normal().map(|v| -v);
                    for side in [-1., 1.] {
                        let origin =
                            facade.point(&recipe, side * (half + projection * 0.5), 0.5, -0.3);
                        let distance = mesh_ray(timber, origin, inward)
                            .expect("continuous corner post outside both wall planes");
                        assert!((distance - (0.3 - projection)).abs() < 0.00001);
                        let along = side * half * 0.7;
                        let x = facade.point(&recipe, along, 0., 0.)[0];
                        let z = island_building::geometry::chimney::roof_height(&recipe, x)
                            - recipe.roof.effective_thickness()
                            + 0.02;
                        let origin = facade.point(&recipe, along, z, -0.3);
                        let distance = mesh_ray(timber, origin, inward)
                            .expect("timber covers the former white roof-joint strip");
                        assert!((distance - (0.3 - projection)).abs() < 0.00001);
                    }
                }
            }
        }
    }
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
            "normal follows winding: triangle {a:?} {b:?} {c:?}, normal {:?}",
            mesh.normals[tri[0] as usize]
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
                recipe.chimney = None;
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
    for enabled in [false, true] {
        let mut recipe = preview(BuildingRecipe::hut());
        recipe.floor_thickness = 0.5;
        if !enabled {
            recipe.chimney = None;
        }
        let prototype = generate(&recipe).unwrap();
        let mesh = &prototype.lods[2].parts[0].mesh;
        let span = recipe.wall_height + recipe.roof.rise + recipe.floor_thickness;
        let rows = if enabled { 4. } else { 2. };
        for (point, uv) in mesh.positions[..4].iter().zip(&mesh.uvs[..4]) {
            let normalized = (0.95 - uv[1] * rows) / 0.9;
            let height = normalized * span - recipe.floor_thickness;
            assert!(
                (height - point[2]).abs() < 0.00001,
                "both atlas layouts retain absolute opening elevations"
            );
        }
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
            recipe.chimney = None;
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
#[test]
fn near_windows_have_recessed_sashes_clear_glass_and_projecting_sills_on_every_facade() {
    for facade in Facade::ALL {
        for thickness in [0.08, 0.2, 0.6] {
            let mut recipe = preview(BuildingRecipe::hut());
            recipe.chimney = None;
            recipe.roof.overhang = 0.;
            recipe.wall_thickness = thickness;
            let mut opening = recipe
                .openings
                .iter()
                .find(|o| o.kind == OpeningKind::Window)
                .unwrap()
                .clone();
            recipe.openings.retain(|o| o.kind == OpeningKind::Door);
            opening.facade = facade;
            opening.offset = if facade == Facade::South { 1.65 } else { 0. };
            opening.width = 0.7;
            opening.height = 1.1;
            opening.sill = 0.95;
            recipe.openings.push(opening.clone());
            let prototype = generate(&recipe).unwrap();
            let trim = &prototype.lods[0]
                .parts
                .iter()
                .find(|p| p.material == MaterialSlot::Trim)
                .unwrap()
                .mesh;
            check_mesh(trim);
            assert!(
                trim.positions
                    .iter()
                    .all(|p| prototype.layout.contains(*p, 0.00001))
            );
            let inward = facade.normal().map(|v| -v);
            let source = facade.point(
                &recipe,
                opening.offset - opening.width * 0.5 + 0.035,
                opening.sill + opening.height * 0.75,
                -0.3,
            );
            let sash = mesh_ray(trim, source, inward).expect("recessed sash front");
            assert!(
                sash > 0.32 && sash < 0.3 + thickness * 0.48,
                "the sash sits behind the wall face and directly around the glass"
            );
            let clear = facade.point(
                &recipe,
                opening.offset + opening.width * 0.25,
                opening.sill + opening.height * 0.75,
                -0.3,
            );
            assert!(
                mesh_ray(trim, clear, inward)
                    .is_none_or(|distance| distance > 0.3 + thickness + 0.03),
                "the casing and sill leave the panes open"
            );
            let glass = &prototype.lods[0]
                .parts
                .iter()
                .find(|p| p.material == MaterialSlot::Glass)
                .unwrap()
                .mesh;
            let distance = mesh_ray(glass, clear, inward).expect("transparent glass behind casing");
            assert!(
                glass.positions.iter().all(|p| {
                    p[2] >= opening.sill + 0.039 && p[2] <= opening.sill + opening.height - 0.039
                }),
                "glass stays beneath the sash beads and above the sill"
            );
            assert!((distance - (0.3 + thickness * 0.48)).abs() < 0.00001);
            let above = facade.point(&recipe, opening.offset, opening.sill + 0.15, -0.1);
            let sill = mesh_ray(trim, above, [0., 0., -1.]).expect("sill projects beyond the wall");
            assert!(
                sill > 0.13 && sill < 0.17,
                "the sill has a shallow outward slope"
            );
            let seam = facade.point(
                &recipe,
                opening.offset + opening.width * 0.25,
                opening.sill + 0.003,
                -0.3,
            );
            let sill_front = mesh_ray(trim, seam, inward).expect("closed sill-to-wall joint");
            assert!(
                (sill_front - 0.18).abs() < 0.00001,
                "the raised sill hides the lower wall cut before reaching the lining"
            );
            assert!(
                prototype.lods[1..]
                    .iter()
                    .all(|lod| lod.parts.iter().all(|p| p.material != MaterialSlot::Glass))
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
            recipe.chimney = None;
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

#[test]
fn legacy_recipes_omit_chimneys_and_keep_the_original_atlas_grid() {
    let mut value = serde_json::to_value(BuildingRecipe::hut()).unwrap();
    value.as_object_mut().unwrap().remove("chimney");
    value["materials"]
        .as_object_mut()
        .unwrap()
        .remove("chimney");
    value["materials"]
        .as_object_mut()
        .unwrap()
        .remove("chimney_recipe");
    let mut recipe: BuildingRecipe = serde_json::from_value(value).unwrap();
    recipe.atlas_size = 128;
    assert!(recipe.chimney.is_none());
    let prototype = generate(&recipe).unwrap();
    assert!(
        prototype
            .lods
            .iter()
            .flat_map(|lod| &lod.parts)
            .all(|part| !matches!(part.material, MaterialSlot::Chimney | MaterialSlot::Firebox))
    );
    assert!(
        prototype.lods[2].parts[0]
            .mesh
            .uvs
            .iter()
            .all(|uv| (0.0..=1.0).contains(&uv[1]))
    );
    let roof = &prototype.lods[2].parts[0].mesh;
    assert!(
        roof.uvs.iter().any(|uv| uv[1] > 0.5),
        "legacy charts still use both halves of a two-row atlas"
    );
}

#[test]
fn chimney_presets_have_real_fireboxes_and_no_fireplace_geometry_in_lower_lods() {
    for recipe in [
        BuildingRecipe::hut(),
        BuildingRecipe::cottage(),
        BuildingRecipe::workshop(),
    ] {
        let prototype = generate(&preview(recipe)).unwrap();
        let recipe = &prototype.recipe;
        let chimney = recipe.chimney.as_ref().unwrap();
        let origin = chimney.facade.point(
            recipe,
            chimney.offset,
            chimney.hearth_height + chimney.firebox_height * 0.5,
            recipe.wall_thickness + chimney.depth + 0.4,
        );
        let direction = chimney.facade.normal();
        let render = prototype.lods[0]
            .parts
            .iter()
            .filter(|part| matches!(part.material, MaterialSlot::Chimney | MaterialSlot::Firebox))
            .filter_map(|part| mesh_ray(&part.mesh, origin, direction))
            .min_by(f32::total_cmp)
            .unwrap();
        assert!(
            (render - (0.4 + chimney.firebox_depth)).abs() < 0.01,
            "front aperture reaches the soot-lined back of the firebox"
        );
        let collision = prototype
            .colliders
            .iter()
            .filter_map(|collider| collider.ray_hit(origin, direction, 3.))
            .min_by(f32::total_cmp)
            .unwrap();
        assert!(
            (collision - (0.4 + chimney.firebox_depth)).abs() < 0.01,
            "collision also leaves the firebox cavity open"
        );
        assert!(
            prototype.lods[0]
                .parts
                .iter()
                .any(|part| part.material == MaterialSlot::Firebox)
        );
        assert!(
            prototype.lods[1]
                .parts
                .iter()
                .all(|part| part.material != MaterialSlot::Firebox)
        );
        assert!(
            prototype.lods[1]
                .parts
                .iter()
                .filter(|part| part.material == MaterialSlot::Chimney)
                .flat_map(|part| &part.mesh.positions)
                .all(|point| point[2] > recipe.wall_height)
        );
        assert!(
            prototype.lods[2]
                .parts
                .iter()
                .all(|part| part.material == MaterialSlot::Atlas)
        );
        let hearth = island_building::geometry::chimney::hearth_bounds(recipe).unwrap();
        let stood = chimney.facade.point(
            recipe,
            chimney.offset,
            0.,
            recipe.wall_thickness + chimney.depth + chimney.hearth_depth - 0.05,
        );
        assert!(
            hearth.intersects_capsule(stood, 0.28, 1.8),
            "raised hearth is a non-walkable obstacle"
        );
    }
}

#[test]
fn open_flue_passes_through_ceiling_roof_and_ridge_without_hidden_planes() {
    let prototype = generate(&preview(BuildingRecipe::hut())).unwrap();
    let recipe = &prototype.recipe;
    let bounds = island_building::geometry::chimney::stack_bounds(recipe).unwrap();
    let centre = [
        (bounds.min[0] + bounds.max[0]) * 0.5,
        (bounds.min[1] + bounds.max[1]) * 0.5,
        bounds.min[2] + 0.1,
    ];
    for (origin, direction) in [
        (centre, [0., 0., 1.]),
        (
            [centre[0] - 0.05, centre[1], centre[2]],
            [
                0.05 / (island_building::geometry::chimney::roof_height(recipe, 0.)
                    - 0.06
                    - centre[2]),
                0.,
                1.,
            ],
        ),
    ] {
        assert!(
            prototype.lods[0]
                .parts
                .iter()
                .all(|part| mesh_ray(&part.mesh, origin, direction).is_none()),
            "flue remains visibly open through slab thickness and ridge fascia"
        );
        assert!(
            prototype
                .colliders
                .iter()
                .all(|collider| collider.ray_hit(origin, direction, 10.).is_none()),
            "ceiling collision is cut to the same shaft footprint"
        );
    }
}

#[test]
fn fireplace_geometry_and_roof_clearance_work_on_all_facades_and_roofs() {
    for facade in Facade::ALL {
        for roof in [
            island_building::RoofKind::Gable,
            island_building::RoofKind::Shed,
        ] {
            let mut recipe = preview(BuildingRecipe::hut());
            let opposite = match facade {
                Facade::South => Facade::North,
                Facade::North => Facade::South,
                Facade::East => Facade::West,
                Facade::West => Facade::East,
            };
            recipe.roof.kind = roof;
            recipe.openings.retain(|opening| {
                opening.kind == OpeningKind::Door
                    || (opening.facade != facade && opening.facade != opposite)
            });
            recipe.openings[0].facade = opposite;
            recipe.chimney.as_mut().unwrap().facade = facade;
            let prototype = generate(&recipe).unwrap();
            let stack = island_building::geometry::chimney::stack_bounds(&recipe).unwrap();
            let clearance = recipe.chimney.as_ref().unwrap().height_above_roof;
            assert!(
                (stack.max[2]
                    - island_building::geometry::chimney::highest_roof(&recipe, stack)
                    - clearance)
                    .abs()
                    < 0.00001
            );
            for lod in &prototype.lods {
                for part in &lod.parts {
                    check_mesh(&part.mesh);
                    assert!(
                        part.mesh
                            .positions
                            .iter()
                            .all(|point| (0..3).all(|axis| point[axis]
                                >= prototype.layout.bounds_min[axis] - 0.00001
                                && point[axis] <= prototype.layout.bounds_max[axis] + 0.00001)),
                        "bounds include chimney cap and every clipped roof vertex"
                    );
                }
            }
        }
    }
}

#[test]
fn fireplace_validation_rejects_opening_route_and_dimension_conflicts() {
    let mut opening = BuildingRecipe::hut();
    opening.chimney.as_mut().unwrap().facade = Facade::South;
    assert!(
        validate(&opening)
            .unwrap_err()
            .to_string()
            .contains("opening")
    );
    let mut shallow = BuildingRecipe::hut();
    shallow.chimney.as_mut().unwrap().firebox_depth = 0.6;
    assert!(
        validate(&shallow)
            .unwrap_err()
            .to_string()
            .contains("backing masonry")
    );
    let mut route = BuildingRecipe::hut();
    route.partitions.push(PartitionRecipe {
        id: "hearth-route".into(),
        axis: PartitionAxis::X,
        offset: 1.2,
        door_offset: 0.,
        door_width: 1.1,
        door_height: 2.2,
    });
    assert!(validate(&route).unwrap_err().to_string().contains("hearth"));
    let mut invalid = BuildingRecipe::hut();
    invalid.chimney.as_mut().unwrap().height_above_roof = f32::NAN;
    assert!(
        validate(&invalid)
            .unwrap_err()
            .to_string()
            .contains("finite")
    );
}
fn mesh_ray(mesh: &BuildingMesh, origin: [f32; 3], direction: [f32; 3]) -> Option<f32> {
    mesh.indices
        .chunks_exact(3)
        .filter_map(|indices| triangle_hit(indices.map_ref(&mesh.positions), origin, direction))
        .min_by(f32::total_cmp)
}

#[test]
fn straight_fireplace_and_room_wall_surround_keep_the_aperture_and_flue_clear() {
    for facade in Facade::ALL {
        let mut recipe = preview(BuildingRecipe::hut());
        recipe
            .openings
            .retain(|opening| opening.kind == OpeningKind::Door);
        recipe.openings[0].facade = if facade == Facade::South {
            Facade::North
        } else {
            Facade::South
        };
        recipe.chimney.as_mut().unwrap().facade = facade;
        let prototype = generate(&recipe).unwrap();
        let chimney = recipe.chimney.as_ref().unwrap();
        let front = recipe.wall_thickness + chimney.depth;
        let stone = &prototype.lods[0]
            .parts
            .iter()
            .find(|part| part.material == MaterialSlot::Chimney)
            .unwrap()
            .mesh;
        for along in [-chimney.width * 0.42, chimney.width * 0.42] {
            for height in [1.6, recipe.wall_height - 0.15] {
                let origin = facade.point(&recipe, along, height, front + 0.5);
                let hit = mesh_ray(stone, origin, facade.normal()).unwrap();
                assert!(
                    (hit - 0.5).abs() < 0.005,
                    "stone breast stays full-width to the ceiling"
                );
                let collision = prototype
                    .colliders
                    .iter()
                    .filter_map(|collider| collider.ray_hit(origin, facade.normal(), 2.))
                    .min_by(f32::total_cmp)
                    .unwrap();
                assert!(
                    (collision - hit).abs() < 0.005,
                    "straight breast collision follows its visible surface"
                );
            }
        }
        let room_wall = &prototype.lods[0]
            .parts
            .iter()
            .find(|part| part.material == MaterialSlot::Interior)
            .unwrap()
            .mesh;
        let opening_top = chimney.hearth_height + chimney.firebox_height;
        for (along, height, projection) in [
            (
                -chimney.firebox_width * 0.5 - 0.09,
                opening_top * 0.5,
                0.045,
            ),
            (chimney.firebox_width * 0.5 + 0.09, opening_top * 0.5, 0.045),
            (0., opening_top + 0.07, 0.045),
            (chimney.width * 0.5 + 0.05, opening_top + 0.19, 0.12),
        ] {
            let origin = facade.point(&recipe, along, height, front + 0.5);
            let hit = mesh_ray(room_wall, origin, facade.normal()).unwrap();
            assert!(
                (hit - (0.5 - projection)).abs() < 0.005,
                "both surround legs, header and mantel share the actual room-wall material slot"
            );
        }
        let origin = facade.point(&recipe, 0., opening_top * 0.5, front + 0.4);
        let hit = prototype.lods[0]
            .parts
            .iter()
            .filter_map(|part| mesh_ray(&part.mesh, origin, facade.normal()))
            .min_by(f32::total_cmp)
            .unwrap();
        assert!(
            (hit - (0.4 + chimney.firebox_depth)).abs() < 0.01,
            "the new surround does not close or shrink the firebox opening"
        );
    }
}

#[test]
fn legacy_roof_json_defaults_to_standing_seam_and_coverings_round_trip() {
    let mut value = serde_json::to_value(BuildingRecipe::hut()).unwrap();
    value["roof"].as_object_mut().unwrap().remove("covering");
    let legacy: BuildingRecipe = serde_json::from_value(value).unwrap();
    assert_eq!(legacy.roof.covering, RoofCovering::StandingSeam);
    for (covering, name) in [
        (RoofCovering::StandingSeam, "standing_seam"),
        (RoofCovering::Slate, "slate"),
        (RoofCovering::Thatch, "thatch"),
    ] {
        let mut recipe = legacy.clone();
        recipe.roof.covering = covering;
        let saved = serde_json::to_value(&recipe).unwrap();
        assert_eq!(saved["roof"]["covering"], name);
        assert_eq!(
            serde_json::from_value::<BuildingRecipe>(saved).unwrap(),
            recipe
        );
    }
}

#[test]
fn roof_coverings_have_distinct_deterministic_sources_and_opaque_atlases() {
    let mut recipe = preview(BuildingRecipe::hut());
    recipe.chimney = None;
    let standing = generate(&recipe).unwrap();
    recipe.roof.covering = RoofCovering::Slate;
    let slate = generate(&recipe).unwrap();
    assert_eq!(slate, generate(&recipe).unwrap());
    recipe.roof.covering = RoofCovering::Thatch;
    let thatch = generate(&recipe).unwrap();
    assert_eq!(thatch, generate(&recipe).unwrap());
    recipe.seed ^= 0x7a31;
    let varied = generate(&recipe).unwrap();
    let original_roof = &thatch.lods[0]
        .parts
        .iter()
        .find(|part| part.material == MaterialSlot::Roof)
        .unwrap()
        .mesh;
    let varied_roof = &varied.lods[0]
        .parts
        .iter()
        .find(|part| part.material == MaterialSlot::Roof)
        .unwrap()
        .mesh;
    assert_ne!(
        original_roof.positions, varied_roof.positions,
        "thatch surface and silhouette variation respond deterministically to the building seed"
    );

    for (first, second) in [(&standing, &slate), (&standing, &thatch), (&slate, &thatch)] {
        let first_roof = first
            .materials
            .iter()
            .find(|m| m.slot == MaterialSlot::Roof)
            .unwrap();
        let second_roof = second
            .materials
            .iter()
            .find(|m| m.slot == MaterialSlot::Roof)
            .unwrap();
        assert_ne!(first_roof.source_hash, second_roof.source_hash);
        assert_ne!(first_roof.albedo, second_roof.albedo);
        assert_ne!(first_roof.normal, second_roof.normal);
        assert_ne!(first.hash, second.hash);
        assert_ne!(first.atlas.albedo, second.atlas.albedo);
    }
    for prototype in [&standing, &slate, &thatch] {
        assert!(
            prototype
                .atlas
                .albedo
                .chunks_exact(4)
                .all(|pixel| pixel[3] == 255)
        );
        assert!(
            prototype
                .materials
                .iter()
                .filter(|material| material.slot.thatch_shell_layer().is_none())
                .all(|material| { material.albedo.chunks_exact(4).all(|pixel| pixel[3] == 255) })
        );
        if prototype.recipe.roof.covering == RoofCovering::Thatch {
            check_thatch_shape(prototype);
            check_thatch_uv_pitch(prototype);
            check_thatch_shell_placement(prototype);
            check_thatch_shell_layers(prototype);
            check_thatch_shell_materials(prototype);
        } else {
            assert!(
                prototype
                    .materials
                    .iter()
                    .all(|material| { material.slot.thatch_shell_layer().is_none() })
            );
            assert!(
                prototype
                    .lods
                    .iter()
                    .flat_map(|lod| &lod.parts)
                    .all(|part| { part.material.thatch_shell_layer().is_none() })
            );
            check_roof_uv_pitch(prototype);
        }
        if prototype.recipe.roof.covering == RoofCovering::Slate {
            check_slate_course_lips(prototype);
        }
        if prototype.recipe.roof.covering != RoofCovering::StandingSeam {
            check_roof_atlas_mirror_alignment(prototype);
        }
    }
}

// Match physical points between the source roof and its atlas chart. This catches
// a slope/ridge axis swap or a missing renderer UV scale without relying on pixels.
fn check_roof_uv_pitch(prototype: &BuildingPrototype) {
    let recipe = &prototype.recipe;
    let near = &prototype.lods[0]
        .parts
        .iter()
        .find(|part| part.material == MaterialSlot::Roof)
        .unwrap()
        .mesh;
    let far = &prototype.lods[2].parts[0].mesh;
    let slope_width = if recipe.roof.kind == RoofKind::Gable {
        recipe.width * 0.5 + recipe.roof.overhang
    } else {
        recipe.width + recipe.roof.overhang * 2.
    };
    let rise = if recipe.roof.kind == RoofKind::Gable {
        recipe.roof.rise / (recipe.width * 0.5)
    } else {
        recipe.roof.rise / recipe.width
    };
    let slope_length = slope_width * (1. + rise * rise).sqrt();
    let depth = recipe.depth + recipe.roof.overhang * 2.;
    let rows = if recipe.chimney.is_some() { 4. } else { 2. };
    let close =
        |a: [f32; 3], b: [f32; 3]| a.into_iter().zip(b).all(|(a, b)| (a - b).abs() < 0.00001);
    let mut matched = 0;
    for ((point, normal), uv) in near.positions.iter().zip(&near.normals).zip(&near.uvs) {
        if normal[2] < 0.5 {
            continue;
        }
        let Some(index) = far
            .positions
            .iter()
            .zip(&far.normals)
            .position(|(p, n)| close(*point, *p) && close(*normal, *n))
        else {
            continue;
        };
        let atlas = far.uvs[index];
        let chart_u = ((atlas[0] * 4.).fract() - 0.05) / 0.9;
        // Chart 5 retains its left-to-right geometry coordinates while baking
        // the new coverings from the right eave towards the ridge.
        let chart_u = if recipe.roof.kind == RoofKind::Gable
            && recipe.roof.covering != RoofCovering::StandingSeam
            && (0.25..0.5).contains(&atlas[0])
        {
            1. - chart_u
        } else {
            chart_u
        };
        let chart_v = (0.95 - (atlas[1] * rows).fract()) / 0.9;
        assert!(
            (uv[0] * 2. - chart_u * slope_length).abs() < 0.0001,
            "source U and atlas U measure distance along the roof slope"
        );
        assert!(
            (uv[1] * 2. - chart_v * depth).abs() < 0.0001,
            "source V and atlas V measure distance along the ridge"
        );
        matched += 1;
    }
    assert!(
        matched >= 4,
        "source roof corners occur in the distant roof chart"
    );
}

// Equal physical stations on the two gable charts must remain equal after any
// reconstruction filter. The source is asymmetric, so an omitted U flip fails.
fn check_roof_atlas_mirror_alignment(prototype: &BuildingPrototype) {
    let size = usize::from(u16::try_from(prototype.atlas.size).unwrap());
    let width = size / 4;
    let height = size / 2;
    let mut asymmetric_samples = 0;
    for local_y in [height / 4, height / 2, height * 3 / 4] {
        for local_x in [width / 4, width / 3, width * 2 / 5] {
            let row = (height + local_y) * size;
            let left = (row + local_x) * 4;
            let right = (row + width * 2 - local_x) * 4;
            let opposite = (row + width - local_x) * 4;
            for channel in 0..4 {
                assert!(
                    prototype.atlas.albedo[left + channel]
                        .abs_diff(prototype.atlas.albedo[right + channel])
                        <= 1,
                    "mirrored charts sample identical physical roof stations after filtering"
                );
                assert!(
                    prototype.atlas.masks[left + channel]
                        .abs_diff(prototype.atlas.masks[right + channel])
                        <= 1
                );
                let normal = prototype.atlas.normal[left + channel];
                let expected = if channel == 0 { 255 - normal } else { normal };
                assert!(
                    expected.abs_diff(prototype.atlas.normal[right + channel]) <= 1,
                    "mirroring a roof chart reverses tangent red while retaining green and blue"
                );
            }
            if (0..3).any(|channel| {
                prototype.atlas.albedo[left + channel]
                    .abs_diff(prototype.atlas.albedo[opposite + channel])
                    > 2
            }) {
                asymmetric_samples += 1;
            }
        }
    }
    assert!(
        asymmetric_samples > 0,
        "the filtered source retains enough asymmetry to detect an omitted chart flip"
    );
}

// The two lower LODs keep the original closed ridge and level eave cuts.
fn check_thatch_shape(prototype: &BuildingPrototype) {
    let recipe = &prototype.recipe;
    let edge = recipe.width * 0.5 + recipe.roof.overhang;
    let high = if recipe.roof.kind == RoofKind::Gable {
        0.
    } else {
        edge
    };
    let slope = recipe.roof.rise
        / if recipe.roof.kind == RoofKind::Gable {
            recipe.width * 0.5
        } else {
            recipe.width
        };
    let width = (1.1 / (1. + slope * slope).sqrt()).min(edge * 0.6);
    let depth = recipe.depth * 0.5 + recipe.roof.overhang;
    let origin_z = recipe.wall_height
        + recipe.roof.rise
        + recipe.roof.effective_thickness()
        + recipe.roof.overhang * slope
        + 1.;
    for lod in &prototype.lods[1..] {
        let top = |x| origin_z - roof_hit(lod, [x, 0., origin_z], [0., 0., -1.]).unwrap();
        assert!(
            top(high - width + 0.01) - top(high - width - 0.01) > 0.09,
            "a clean shoulder step distinguishes the solid ridge from the roof runs"
        );
        let below = recipe.wall_height + 0.01;
        let floor = horizontal_roof_hit(lod, [high, 0., below]).unwrap() + below;
        for y in [-depth + 0.04, depth * 0.35, depth - 0.04] {
            let height = horizontal_roof_hit(lod, [high, y, below]).unwrap() + below;
            assert!(
                (height - floor).abs() < 0.0001,
                "the ridge is a full-length prism with one horizontal bottom"
            );
        }
        for side in [-1., 1.] {
            for x in [high - width * 0.5, high] {
                let distance = roof_hit(
                    lod,
                    [x, side * (depth + 0.5), floor + 0.04],
                    [0., -side, 0.],
                )
                .unwrap();
                assert!(
                    (distance - 0.38).abs() < 0.004,
                    "solid ridge ends project 12 cm past both gable roof edges at every LOD"
                );
            }
        }
        let sides: &[f32] = if recipe.roof.kind == RoofKind::Gable {
            &[-1., 1.]
        } else {
            &[-1.]
        };
        for &side in sides {
            check_horizontal_eave(lod, side * edge);
        }
    }
}

fn roof_hit(
    lod: &island_building::BuildingLod,
    origin: [f32; 3],
    direction: [f32; 3],
) -> Option<f32> {
    lod.parts
        .iter()
        .filter(|part| {
            matches!(
                part.material,
                MaterialSlot::Roof | MaterialSlot::ThatchEnd | MaterialSlot::Atlas
            )
        })
        .filter_map(|part| mesh_ray(&part.mesh, origin, direction))
        .min_by(f32::total_cmp)
}

fn horizontal_roof_hit(lod: &island_building::BuildingLod, origin: [f32; 3]) -> Option<f32> {
    lod.parts
        .iter()
        .filter(|part| matches!(part.material, MaterialSlot::ThatchEnd | MaterialSlot::Atlas))
        .flat_map(|part| {
            part.mesh
                .indices
                .chunks_exact(3)
                .filter(|indices| {
                    indices.iter().all(|index| {
                        let n = part.mesh.normals[*index as usize];
                        n[2] < -0.9999 && n[0].abs() + n[1].abs() < 0.00001
                    })
                })
                .filter_map(|indices| {
                    triangle_hit(indices.map_ref(&part.mesh.positions), origin, [0., 0., 1.])
                })
        })
        .min_by(f32::total_cmp)
}

fn check_horizontal_eave(lod: &island_building::BuildingLod, edge: f32) {
    let level = lod
        .parts
        .iter()
        .filter(|part| matches!(part.material, MaterialSlot::ThatchEnd | MaterialSlot::Atlas))
        .flat_map(|part| part.mesh.positions.iter().zip(&part.mesh.normals))
        .filter(|(p, n)| (p[0] - edge).abs() < 0.0001 && n[2] < -0.9999)
        .map(|(p, _)| p[2])
        .max_by(f32::total_cmp)
        .expect("each downward run ends in a horizontal cut");
    let mut extent = [edge, edge];
    for part in &lod.parts {
        if !matches!(part.material, MaterialSlot::ThatchEnd | MaterialSlot::Atlas) {
            continue;
        }
        for (point, normal) in part.mesh.positions.iter().zip(&part.mesh.normals) {
            if normal[2] < -0.9999
                && (point[2] - level).abs() < 0.0001
                && point[0].signum() == edge.signum()
            {
                extent[0] = extent[0].min(point[0]);
                extent[1] = extent[1].max(point[0]);
            }
        }
    }
    assert!(
        extent[1] - extent[0] > 0.01,
        "the level straw cut has area, not just a horizontal rim line"
    );
    let x = (edge + (if edge < 0. { extent[1] } else { extent[0] })) * 0.5;
    let hit = horizontal_roof_hit(lod, [x, 0., level - 0.04])
        .expect("flat eave underside closes the roof run");
    assert!((hit - 0.04).abs() < 0.001);
}

#[test]
fn thatch_smooths_near_lod_and_keeps_the_lower_lod_prisms() {
    for kind in [RoofKind::Gable, RoofKind::Shed] {
        for rise in [1.4, 4., 6.] {
            let mut recipe = preview(BuildingRecipe::hut());
            recipe.chimney = None;
            recipe.roof.covering = RoofCovering::Thatch;
            recipe.roof.kind = kind;
            recipe.roof.rise = rise;
            let prototype = generate(&recipe).unwrap();
            check_thatch_shape(&prototype);
            check_prism_shells(&prototype);
        }
    }
}

fn check_prism_shells(prototype: &BuildingPrototype) {
    check_thatch_shell_layers(prototype);
    let base = &prototype.lods[0]
        .parts
        .iter()
        .find(|p| p.material == MaterialSlot::ThatchEnd)
        .unwrap()
        .mesh;
    let shell = &prototype.lods[0]
        .parts
        .iter()
        .find(|p| p.material == MaterialSlot::ThatchShell1)
        .unwrap()
        .mesh;
    for point in &shell.positions {
        assert!(
            (0..3).any(|axis| [-1., 1.].into_iter().any(|sign| {
                let mut root = *point;
                root[axis] -= sign * 0.005;
                base.positions
                    .iter()
                    .any(|p| p.iter().zip(root).all(|(a, b)| (a - b).abs() < 0.00002))
            })),
            "the first shell follows the subdivided cut-straw surface"
        );
    }
    let depth = prototype.recipe.depth * 0.5 + prototype.recipe.roof.overhang;
    assert!(
        shell.positions.iter().any(|p| p[1].abs() > depth + 0.12),
        "the smoothed ridge keeps its gable projection"
    );
    let roof = &prototype.lods[0]
        .parts
        .iter()
        .find(|p| p.material == MaterialSlot::Roof)
        .unwrap()
        .mesh;
    assert!(
        roof.normals.iter().any(|n| n[1].abs() > 0.05),
        "LOD0 roof normals follow the rounded ends"
    );
}

// Tessellation changes between LODs, so verify central coat UVs against physical
// coordinates directly rather than requiring identical vertices and normals.
fn check_thatch_uv_pitch(prototype: &BuildingPrototype) {
    let recipe = &prototype.recipe;
    let edge = recipe.width * 0.5 + recipe.roof.overhang;
    let depth = recipe.depth * 0.5 + recipe.roof.overhang + 0.12;
    let scale = (1. + (recipe.roof.rise / (recipe.width * 0.5)).powi(2)).sqrt();
    for (index, lod) in prototype.lods.iter().enumerate() {
        let mut checked = 0;
        for part in &lod.parts {
            if !matches!(part.material, MaterialSlot::Roof | MaterialSlot::Atlas) {
                continue;
            }
            for indices in part.mesh.indices.chunks_exact(3) {
                let centre = |values: &[[f32; 3]]| {
                    std::array::from_fn::<_, 3, _>(|axis| {
                        indices
                            .iter()
                            .map(|i| values[*i as usize][axis])
                            .sum::<f32>()
                            / 3.
                    })
                };
                let point = centre(&part.mesh.positions);
                let normal = centre(&part.mesh.normals);
                let uv = std::array::from_fn::<_, 2, _>(|axis| {
                    indices
                        .iter()
                        .map(|i| part.mesh.uvs[*i as usize][axis])
                        .sum::<f32>()
                        / 3.
                });
                if point[0].abs() < 0.1
                    || point[0].abs() > recipe.width * 0.4
                    || point[1].abs() > recipe.depth * 0.35
                    || normal[2] < 0.5
                    || point[2]
                        < recipe.wall_height
                            + recipe.roof.rise * (1. - point[0].abs() / (recipe.width * 0.5))
                            + recipe.roof.effective_thickness() * 0.5
                {
                    continue;
                }
                let expected = [(edge - point[0].abs()) * scale, point[1] + depth];
                let actual = if index == 2 {
                    let chart_u = ((uv[0] * 4.).fract() - 0.05) / 0.9;
                    let chart_u = if point[0] > 0. { 1. - chart_u } else { chart_u };
                    [
                        chart_u * edge * scale,
                        (0.95 - (uv[1] * 2.).fract()) / 0.9 * depth * 2.,
                    ]
                } else {
                    uv.map(|value| value * 2.)
                };
                assert!(
                    actual
                        .into_iter()
                        .zip(expected)
                        .all(|(actual, expected)| (actual - expected).abs() < 0.0001),
                    "the central thatch coat retains slope U and ridge V at physical scale"
                );
                checked += 1;
            }
        }
        assert!(checked >= 4, "each LOD contains central coat UV samples");
    }
}

// The embedded slate material places eight courses across its two-metre U
// tile. Check a real lip on both sides of the gable.
fn check_slate_course_lips(prototype: &BuildingPrototype) {
    let recipe = &prototype.recipe;
    let pitch = 0.25;
    let roof = &prototype.lods[0]
        .parts
        .iter()
        .find(|part| part.material == MaterialSlot::Roof)
        .unwrap()
        .mesh;
    let edge = recipe.width * 0.5 + recipe.roof.overhang;
    let slope_scale = (1. + (recipe.roof.rise / (recipe.width * 0.5)).powi(2)).sqrt();
    let origin_z = recipe.wall_height + recipe.roof.rise + recipe.roof.effective_thickness() + 1.;
    for sign in [-1., 1.] {
        let relief_at = |distance: f32| {
            let x = sign * (edge - distance / slope_scale);
            let hit = mesh_ray(roof, [x, 0., origin_z], [0., 0., -1.]).unwrap();
            let shell_z = recipe.wall_height
                + recipe.roof.effective_thickness()
                + recipe.roof.rise * (1. - x.abs() / (recipe.width * 0.5));
            origin_z - hit - shell_z
        };
        assert!(
            relief_at(pitch - 0.002) < 0.006,
            "the previous course tapers just before the material's next course"
        );
        assert!(
            relief_at(pitch + 0.002) > 0.02,
            "the next raised lip starts at the material's physical course pitch from either eave"
        );
    }
}

#[test]
fn slate_and_thatch_keep_roof_bounds_lods_and_flues_valid_at_both_scales() {
    for covering in [RoofCovering::Slate, RoofCovering::Thatch] {
        for kind in [RoofKind::Gable, RoofKind::Shed] {
            for chimney in [false, true] {
                let mut recipe = preview(BuildingRecipe::hut());
                recipe.roof.covering = covering;
                recipe.roof.kind = kind;
                if chimney && covering == RoofCovering::Thatch {
                    // Intersect a horizontal ridge cut in each roof family,
                    // so the flat shell clipping is actually exercised.
                    recipe.wall_thickness = 0.1;
                    recipe.roof.overhang = 0.;
                    if kind == RoofKind::Shed {
                        let width = recipe.width;
                        let wall = recipe.wall_thickness;
                        let chimney = recipe.chimney.as_mut().unwrap();
                        chimney.offset = -(width * 0.5 - chimney.width * 0.5 - wall - 0.31);
                    }
                }
                if !chimney {
                    recipe.chimney = None;
                    // Exercise the authored size limit without growing the texture bake.
                    if kind == RoofKind::Shed {
                        recipe.width = 40.;
                        recipe.depth = 40.;
                    }
                }
                let prototype = generate(&recipe).unwrap();
                assert!(prototype.lods[0].triangle_count() > prototype.lods[1].triangle_count());
                assert!(prototype.lods[1].triangle_count() > prototype.lods[2].triangle_count());
                for (index, lod) in prototype.lods.iter().enumerate() {
                    for part in &lod.parts {
                        check_mesh(&part.mesh);
                        let outside = part
                            .mesh
                            .positions
                            .iter()
                            .find(|point| !prototype.layout.contains(**point, 0.00001));
                        assert!(
                            outside.is_none(),
                            "bounds enclose {covering:?}/{kind:?}/chimney={chimney} LOD{index} {:?}: outside {outside:?}, layout {:?}",
                            part.material,
                            prototype.layout
                        );
                    }
                }
                for lod in &prototype.lods[1..] {
                    assert!(lod.parts.iter().all(|part| !matches!(
                        part.material,
                        MaterialSlot::Glass
                            | MaterialSlot::Interior
                            | MaterialSlot::Floor
                            | MaterialSlot::Firebox
                    )));
                }
                assert!(
                    prototype.lods[2]
                        .parts
                        .iter()
                        .all(|part| part.material == MaterialSlot::Atlas)
                );
                let roof_triangles: usize = prototype.lods[0]
                    .parts
                    .iter()
                    .filter(|part| part.material == MaterialSlot::Roof)
                    .map(|part| part.mesh.triangle_count())
                    .sum();
                if covering == RoofCovering::Slate {
                    assert!(
                        roof_triangles < 10_000,
                        "slate roof detail stays bounded at the 40 metre size limit"
                    );
                } else {
                    for (lod, budget) in prototype.lods.iter().zip([250_000, 15_000, 4_000]) {
                        assert!(
                            lod.triangle_count() < budget,
                            "the curved thatch coat retains a bounded budget in every LOD"
                        );
                    }
                }
                if covering == RoofCovering::Thatch {
                    check_thatch_shell_placement(&prototype);
                    if chimney {
                        check_thatch_shell_chimney_clip(&prototype);
                    }
                }
                if chimney {
                    check_covering_flue_clearance(&prototype);
                }
            }
        }
    }
}

fn check_thatch_shell_placement(prototype: &BuildingPrototype) {
    for slot in MaterialSlot::THATCH_SHELLS {
        let part = prototype.lods[0]
            .parts
            .iter()
            .find(|part| part.material == slot)
            .expect("each of the four cut-end shell layers has geometry");
        assert!(part.mesh.triangle_count() > 0);
        assert!(
            part.mesh
                .normals
                .iter()
                .all(|normal| { (normal.iter().map(|v| v * v).sum::<f32>() - 1.).abs() < 0.0001 }),
            "straw shells follow unit normals on the smoothed cut faces and caps"
        );
    }
    assert!(
        prototype.lods[1..]
            .iter()
            .flat_map(|lod| &lod.parts)
            .all(|part| part.material.thatch_shell_layer().is_none()),
        "straw-end shell geometry is restricted to the near LOD"
    );
}

fn check_thatch_shell_layers(prototype: &BuildingPrototype) {
    let shells = MaterialSlot::THATCH_SHELLS.map(|slot| {
        &prototype.lods[0]
            .parts
            .iter()
            .find(|part| part.material == slot)
            .unwrap()
            .mesh
    });
    for pair in shells.windows(2) {
        assert_eq!(pair[0].indices, pair[1].indices);
        assert_eq!(pair[0].uvs, pair[1].uvs);
        assert_eq!(pair[0].normals, pair[1].normals);
        for (inner, outer) in pair[0].positions.iter().zip(&pair[1].positions) {
            let delta = std::array::from_fn::<_, 3, _>(|axis| outer[axis] - inner[axis]);
            assert!(
                (delta.iter().map(|d| d * d).sum::<f32>().sqrt() - 0.005).abs() < 0.00002,
                "adjacent cut-face shells advance five millimetres along the reed direction"
            );
        }
    }
}

fn check_thatch_shell_materials(prototype: &BuildingPrototype) {
    let ends = prototype
        .materials
        .iter()
        .find(|m| m.slot == MaterialSlot::ThatchEnd)
        .expect("opaque cut straw finish");
    assert!(ends.albedo.chunks_exact(4).all(|pixel| pixel[3] == 255));
    assert!(
        ends.normal
            .chunks_exact(4)
            .any(|pixel| pixel[0].abs_diff(128) > 8 || pixel[1].abs_diff(128) > 8),
        "packed reed ends retain normal relief"
    );
    let [min, max] = ends
        .albedo
        .chunks_exact(4)
        .fold([255, 0], |[min, max], pixel| {
            [min.min(pixel[0]), max.max(pixel[0])]
        });
    assert!(
        max.abs_diff(min) > 20,
        "reed rims and interstices retain visible pigment variation"
    );
    let shells = MaterialSlot::THATCH_SHELLS.map(|slot| {
        prototype
            .materials
            .iter()
            .find(|material| material.slot == slot)
            .unwrap()
    });
    let roof = prototype
        .materials
        .iter()
        .find(|material| material.slot == MaterialSlot::Roof)
        .unwrap();
    let roof_colour = visible_mean_rgb(roof);
    for material in &shells {
        let pixels = &material.albedo;
        assert!(pixels.chunks_exact(4).any(|pixel| pixel[3] == 0));
        assert!(pixels.chunks_exact(4).any(|pixel| pixel[3] == 255));
        assert!(
            pixels
                .chunks_exact(4)
                .any(|pixel| pixel[3] > 0 && pixel[3] < 255),
            "cut-end maps retain antialiased cutout edges"
        );
        assert!(
            material
                .normal
                .chunks_exact(4)
                .all(|pixel| pixel == [128, 128, 255, 255])
        );
        assert!(
            material
                .masks
                .chunks_exact(4)
                .all(|pixel| pixel[2] == 0 && pixel[3] == 255)
        );
        let colour = visible_mean_rgb(material);
        for (first, second) in [(0, 1), (0, 2), (1, 2)] {
            if (roof_colour[first] - roof_colour[second]).abs() > 2. {
                assert!(
                    (colour[first] - colour[second]) * (roof_colour[first] - roof_colour[second])
                        > 0.,
                    "cut-end pigment retains the roof material's dominant colour channels"
                );
            }
        }
    }
    for pair in shells.windows(2) {
        assert_eq!(pair[0].size, pair[1].size);
        let mut lower_coverage = 0;
        let mut upper_coverage = 0;
        for (lower, upper) in pair[0]
            .albedo
            .chunks_exact(4)
            .zip(pair[1].albedo.chunks_exact(4))
        {
            if lower[3] >= 128 {
                lower_coverage += 1;
            }
            if upper[3] >= 128 {
                upper_coverage += 1;
            }
            assert!(
                upper[3] < 128 || lower[3] >= 128,
                "outer shell coverage is a nested subset of the inner reed cross-sections"
            );
        }
        assert!(
            upper_coverage < lower_coverage,
            "reed tips become sparser as the shells move outward"
        );
        assert_ne!(pair[0].source_hash, pair[1].source_hash);
    }
}

fn visible_mean_rgb(material: &island_building::MaterialTexture) -> [f64; 3] {
    let mut sum = [0_u32; 3];
    let mut count = 0_u32;
    for pixel in material
        .albedo
        .chunks_exact(4)
        .filter(|pixel| pixel[3] >= 128)
    {
        for channel in 0..3 {
            sum[channel] += u32::from(pixel[channel]);
        }
        count += 1;
    }
    assert!(count > 0);
    sum.map(|channel| f64::from(channel) / f64::from(count))
}

fn check_thatch_shell_chimney_clip(prototype: &BuildingPrototype) {
    let stack = island_building::geometry::chimney::stack_bounds(&prototype.recipe).unwrap();
    for part in prototype.lods[0]
        .parts
        .iter()
        .filter(|part| part.material.thatch_shell_layer().is_some())
    {
        assert!(
            part.mesh.positions.iter().all(|point| {
                point[0] <= stack.min[0] + 0.0001
                    || point[0] >= stack.max[0] - 0.0001
                    || point[1] <= stack.min[1] + 0.0001
                    || point[1] >= stack.max[1] - 0.0001
            }),
            "offset cut-face shells are clipped outside the complete chimney shaft footprint"
        );
        assert!(
            part.mesh.positions.iter().any(|point| {
                point[0] >= stack.min[0] - 0.03
                    && point[0] <= stack.max[0] + 0.03
                    && (point[1] - stack.max[1]).abs() < 0.04
            }),
            "the test chimney actually intersects each shell's cut-face band"
        );
    }
}

fn check_covering_flue_clearance(prototype: &BuildingPrototype) {
    let recipe = &prototype.recipe;
    let chimney = recipe.chimney.as_ref().unwrap();
    let stack = island_building::geometry::chimney::stack_bounds(recipe).unwrap();
    let roof_x = match recipe.roof.kind {
        RoofKind::Gable => 0_f32.clamp(stack.min[0], stack.max[0]),
        RoofKind::Shed => stack.max[0],
    };
    let slope_height = match recipe.roof.kind {
        RoofKind::Gable => recipe.roof.rise * (1. - roof_x.abs() / (recipe.width * 0.5)),
        RoofKind::Shed => recipe.roof.rise * (roof_x / recipe.width + 0.5),
    };
    let (thickness, detail) = match recipe.roof.covering {
        RoofCovering::Thatch => (recipe.roof.thickness.max(0.45), 0.22),
        RoofCovering::Slate => (recipe.roof.thickness, 0.025),
        RoofCovering::StandingSeam => (recipe.roof.thickness, 0.018),
    };
    assert!(
        stack.max[2] + 0.00001
            >= recipe.wall_height + slope_height + thickness + detail + chimney.height_above_roof,
        "chimney clearance includes thick covering and its relief"
    );
    let centre = [
        (stack.min[0] + stack.max[0]) * 0.5,
        (stack.min[1] + stack.max[1]) * 0.5,
        stack.min[2] + 0.1,
    ];
    for (x, y) in [(0., 0.), (-0.05, 0.), (0.05, 0.), (0., -0.05), (0., 0.05)] {
        let origin = [centre[0] + x, centre[1] + y, centre[2]];
        assert!(
            prototype.lods[0]
                .parts
                .iter()
                .all(|part| { mesh_ray(&part.mesh, origin, [0., 0., 1.]).is_none() }),
            "covering courses and ridge leave the flue visibly open"
        );
        assert!(
            prototype
                .colliders
                .iter()
                .all(|collider| { collider.ray_hit(origin, [0., 0., 1.], 10.).is_none() })
        );
    }
}
