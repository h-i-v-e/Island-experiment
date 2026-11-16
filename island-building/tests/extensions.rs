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
            let middle = std::array::from_fn::<_, 3, _>(|i| {
                (extension.room_bounds.min[i] + extension.room_bounds.max[i]) * 0.5
            });
            assert!(p.layout.contains(middle, 0.));
            assert!(p.support_colliders.iter().any(|b| b.min[0] <= middle[0]
                && b.max[0] >= middle[0]
                && b.min[1] <= middle[1]
                && b.max[1] >= middle[1]));
            assert_eq!(p.doors.len(), 1);
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
            .map(|p| horizontal_faces_at(&p.mesh, point))
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

fn horizontal_faces_at(mesh: &island_building::BuildingMesh, point: [f32; 3]) -> usize {
    mesh.indices
        .chunks_exact(3)
        .filter(|tri| {
            let vertices = [tri[0], tri[1], tri[2]].map(|i| mesh.positions[i as usize]);
            if vertices.iter().any(|p| (p[2] - point[2]).abs() > 0.00001)
                || mesh.normals[tri[0] as usize][2] < 0.5
            {
                return false;
            }
            let [a, b, c] = vertices;
            let area = |a: [f32; 3], b: [f32; 3], c: [f32; 3]| {
                (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])
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
    let foundation: island_building::collision::BoxCollider =
        serde_json::from_slice(&std::fs::read(directory.join("foundation.json")).unwrap()).unwrap();
    assert_eq!(
        foundation,
        island_building::geometry::foundation::enclosing_bounds(&p.recipe)
    );
    assert_eq!(manifest.geometry_version, 54);
    std::fs::remove_dir_all(directory).unwrap();
}
