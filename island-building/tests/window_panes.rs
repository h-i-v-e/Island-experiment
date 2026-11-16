use island_building::{
    BuildingLod, BuildingRecipe, Facade, MaterialSlot, OpeningGeneration, OpeningKind,
    OpeningOverride, WindowPanes, generate, openings,
};

fn shop() -> BuildingRecipe {
    let mut recipe: BuildingRecipe =
        serde_json::from_str(include_str!("../recipes/shopfront.json")).unwrap();
    recipe.atlas_size = 128;
    recipe.chimney = None;
    recipe
}

fn front_window(recipe: &mut BuildingRecipe) -> &mut island_building::OpeningRecipe {
    recipe
        .openings
        .iter_mut()
        .find(|o| o.id == "shop-window")
        .unwrap()
}

#[test]
fn legacy_windows_follow_two_by_two_default_and_pane_overrides_round_trip() {
    let mut value = serde_json::to_value(BuildingRecipe::cottage()).unwrap();
    value["opening_defaults"]
        .as_object_mut()
        .unwrap()
        .remove("window_panes");
    let mut recipe: BuildingRecipe = serde_json::from_value(value).unwrap();
    assert_eq!(
        recipe.opening_defaults.window_panes,
        WindowPanes {
            columns: 2,
            rows: 2
        }
    );
    assert!(recipe.openings.iter().all(|o| o.window_panes.is_none()));
    recipe.openings[1].window_panes = Some(WindowPanes {
        columns: 8,
        rows: 4,
    });
    recipe.opening_defaults.window_panes = WindowPanes {
        columns: 1,
        rows: 1,
    };
    let restored: BuildingRecipe =
        serde_json::from_str(&serde_json::to_string(&recipe).unwrap()).unwrap();
    assert_eq!(restored, recipe);
}

#[test]
fn automatic_pane_overrides_follow_ids_without_pinning_dimensions_or_other_windows() {
    let mut recipe = BuildingRecipe::procedural(10., 7.);
    recipe.chimney = None;
    recipe.opening_defaults.window_panes = WindowPanes {
        columns: 1,
        rows: 1,
    };
    let id = "auto-window-east-0";
    recipe
        .opening_generation
        .as_mut()
        .unwrap()
        .overrides
        .push(OpeningOverride {
            id: id.into(),
            window_panes: Some(WindowPanes {
                columns: 8,
                rows: 4,
            }),
            ..Default::default()
        });
    recipe.depth = 12.;
    recipe.opening_defaults.window.width = 1.6;
    let resolved = openings::resolve(&recipe).unwrap();
    for window in resolved
        .openings
        .iter()
        .filter(|o| o.kind == OpeningKind::Window)
    {
        assert!((window.width - 1.6).abs() < 0.00001);
        assert_eq!(
            window.window_panes,
            (window.id == id).then_some(WindowPanes {
                columns: 8,
                rows: 4
            })
        );
    }
    assert!(resolved.openings.iter().any(|o| o.id == id));
    let patch = &recipe.opening_generation.as_ref().unwrap().overrides[0];
    assert!(patch.width.is_none() && patch.height.is_none() && patch.sill.is_none());
}

#[test]
fn pane_centres_stay_clear_and_dividers_exist_on_all_facades_at_near_and_middle_lods() {
    for facade in Facade::ALL {
        for panes in [
            WindowPanes {
                columns: 1,
                rows: 1,
            },
            WindowPanes {
                columns: 8,
                rows: 4,
            },
            WindowPanes {
                columns: 1,
                rows: 5,
            },
            WindowPanes {
                columns: 4,
                rows: 1,
            },
        ] {
            let mut recipe = shop();
            recipe
                .openings
                .retain(|o| o.kind == OpeningKind::Door || o.id == "shop-window");
            recipe.openings[0].facade = Facade::ALL[(facade.index() + 2) % 4];
            recipe.openings[0].offset = 0.;
            let window = front_window(&mut recipe);
            window.facade = facade;
            window.offset = 0.;
            window.window_panes = None;
            recipe.opening_defaults.window_panes = panes;
            let prototype = generate(&recipe).unwrap();
            let window = recipe.openings.last().unwrap();
            let left = -window.width * 0.5 + 0.055;
            let bottom = window.sill + 0.055;
            let dx = (window.width - 0.11) / f32::from(panes.columns);
            let dz = (window.height - 0.11) / f32::from(panes.rows);
            for lod in &prototype.lods[..2] {
                for column in 0..panes.columns {
                    for row in 0..panes.rows {
                        assert!(!trim_at(
                            lod,
                            &recipe,
                            facade,
                            [
                                left + (f32::from(column) + 0.5) * dx,
                                bottom + (f32::from(row) + 0.5) * dz
                            ]
                        ));
                    }
                }
                for column in 1..panes.columns {
                    assert!(trim_at(
                        lod,
                        &recipe,
                        facade,
                        [left + f32::from(column) * dx, bottom + dz * 0.5]
                    ));
                }
                for row in 1..panes.rows {
                    assert!(trim_at(
                        lod,
                        &recipe,
                        facade,
                        [left + dx * 0.5, bottom + f32::from(row) * dz]
                    ));
                }
            }
            assert!(
                prototype.lods[0]
                    .parts
                    .iter()
                    .any(|p| p.material == MaterialSlot::Glass)
            );
            assert!(
                prototype.lods[1]
                    .parts
                    .iter()
                    .any(|p| p.material == MaterialSlot::WindowProxy)
            );
        }
    }
}

fn trim_at(lod: &BuildingLod, recipe: &BuildingRecipe, facade: Facade, point: [f32; 2]) -> bool {
    let mesh = &lod
        .parts
        .iter()
        .find(|p| p.material == MaterialSlot::Trim)
        .unwrap()
        .mesh;
    let normal = facade.normal();
    let tangent = facade.tangent();
    mesh.indices.chunks_exact(3).any(|tri| {
        let n = mesh.normals[tri[0] as usize];
        let surface = facade.point(recipe, 0., 0., 0.);
        let p = mesh.positions[tri[0] as usize];
        let depth = (surface[0] - p[0]) * normal[0] + (surface[1] - p[1]) * normal[1];
        if n[0] * normal[0] + n[1] * normal[1] < 0.99 || depth > recipe.wall_thickness + 0.03 {
            return false;
        }
        let vertices = [tri[0], tri[1], tri[2]].map(|index| {
            let p = mesh.positions[index as usize];
            [p[0] * tangent[0] + p[1] * tangent[1], p[2]]
        });
        let sides = std::array::from_fn::<_, 3, _>(|i| {
            let a = vertices[i];
            let b = vertices[(i + 1) % 3];
            (b[0] - a[0]) * (point[1] - a[1]) - (b[1] - a[1]) * (point[0] - a[0])
        });
        sides.iter().all(|s| *s >= -0.000_001) || sides.iter().all(|s| *s <= 0.000_001)
    })
}

#[test]
fn pane_edits_change_joinery_and_far_bake_without_changing_physics_or_material_maps() {
    let mut recipe = shop();
    front_window(&mut recipe).window_panes = Some(WindowPanes {
        columns: 1,
        rows: 1,
    });
    let single = generate(&recipe).unwrap();
    front_window(&mut recipe).window_panes = Some(WindowPanes {
        columns: 8,
        rows: 4,
    });
    let many = generate(&recipe).unwrap();
    assert_ne!(single.hash, many.hash);
    assert_ne!(single.atlas.albedo, many.atlas.albedo);
    for lod in 0..2 {
        assert_ne!(single.lods[lod], many.lods[lod]);
    }
    assert_eq!(single.lods[2], many.lods[2]);
    assert_eq!(single.colliders, many.colliders);
    assert_eq!(single.support_colliders, many.support_colliders);
    assert_eq!(single.stair_colliders, many.stair_colliders);
    assert_eq!(single.doors, many.doors);
    assert_eq!(single.materials, many.materials);
    let restored: BuildingRecipe =
        serde_json::from_str(&serde_json::to_string(&recipe).unwrap()).unwrap();
    assert_eq!(generate(&restored).unwrap().hash, many.hash);
}

#[test]
fn invalid_counts_are_rejected_including_inactive_overrides_and_maximum_grid_is_valid() {
    for count in [0, 33] {
        for panes in [
            WindowPanes {
                columns: count,
                rows: 1,
            },
            WindowPanes {
                columns: 1,
                rows: count,
            },
        ] {
            let mut recipe = shop();
            recipe.opening_defaults.window_panes = panes;
            assert!(generate(&recipe).unwrap_err().to_string().contains("pane"));
            recipe.opening_defaults.window_panes = WindowPanes::default();
            front_window(&mut recipe).window_panes = Some(panes);
            assert!(generate(&recipe).unwrap_err().to_string().contains("pane"));
            front_window(&mut recipe).window_panes = None;
            recipe.opening_generation = Some(OpeningGeneration {
                overrides: vec![OpeningOverride {
                    id: "auto-window-east-14".into(),
                    window_panes: Some(panes),
                    ..Default::default()
                }],
                ..Default::default()
            });
            assert!(
                openings::resolve(&recipe)
                    .unwrap_err()
                    .to_string()
                    .contains("pane")
            );
        }
    }
    let mut recipe = shop();
    let window = front_window(&mut recipe);
    window.width = 0.35;
    window.height = 0.35;
    window.window_panes = Some(WindowPanes {
        columns: 32,
        rows: 32,
    });
    let prototype = generate(&recipe).unwrap();
    assert!(
        prototype
            .lods
            .iter()
            .flat_map(|lod| &lod.parts)
            .flat_map(|p| &p.mesh.positions)
            .flatten()
            .all(|v| v.is_finite())
    );
}
