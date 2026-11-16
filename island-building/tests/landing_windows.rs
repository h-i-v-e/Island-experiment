use island_building::{
    BuildingRecipe, DoorStyle, Facade, HingeSide, MaterialSlot, OpeningKind, OpeningRecipe,
    StairAxis, WallStyle, generate, openings, stairs,
};

fn fixture(height: f32, axis: StairAxis, reversed: bool) -> BuildingRecipe {
    let mut recipe = BuildingRecipe::procedural(12., 10.);
    recipe.storeys = 3;
    recipe.wall_height = height;
    recipe.atlas_size = 128;
    recipe.chimney = None;
    recipe.walls.style = WallStyle::WattleAndDaub;
    recipe.staircase.axis = axis;
    recipe.opening_generation.as_mut().unwrap().door_facades = vec![Facade::North];
    let initial = stairs::resolve(&recipe).unwrap().unwrap();
    let mut position = initial.position;
    if !reversed {
        position[usize::from(axis == StairAxis::Y)] = 0.;
    }
    recipe.staircase.position = Some(position);
    let layout = stairs::resolve(&recipe).unwrap().unwrap();
    assert_eq!(layout.reversed, reversed);
    let facade = if axis == StairAxis::Y {
        Facade::West
    } else {
        Facade::South
    };
    let other = if axis == StairAxis::Y {
        Facade::East
    } else {
        Facade::North
    };
    let centre = layout.point(
        layout.span() * 0.5,
        layout.length() - layout.landing_depth * 0.5,
        0.,
    );
    let tangent = facade.tangent();
    let offset = centre[0] * tangent[0] + centre[1] * tangent[1];
    recipe = openings::resolve(&recipe).unwrap().into_owned();
    recipe.opening_generation = None;
    recipe.openings.retain(|o| o.kind == OpeningKind::Door);
    for storey in 0..3 {
        for (name, facade) in [("landing", facade), ("clear", other)] {
            recipe.openings.push(OpeningRecipe {
                id: format!("{name}-window-{storey}"),
                storey,
                facade,
                kind: OpeningKind::Window,
                width: 0.7,
                height: 1.1,
                sill: 0.95,
                offset,
                use_default_dimensions: false,
                window_panes: None,
                hinge: HingeSide::Left,
                swing_degrees: 105.,
                door_style: DoorStyle::Plank,
            });
        }
    }
    recipe
}

#[test]
fn windows_rise_above_landings_with_the_sill_clear_on_both_axes_and_flight_directions() {
    for axis in [StairAxis::X, StairAxis::Y] {
        for reversed in [false, true] {
            let recipe = fixture(4., axis, reversed);
            let authoring = serde_json::to_vec(&recipe).unwrap();
            let prototype = generate(&recipe).unwrap();
            assert_eq!(serde_json::to_vec(&recipe).unwrap(), authoring);
            let stairs = prototype.layout.staircase.as_ref().unwrap();
            for storey in 0..3 {
                let opening = prototype
                    .recipe
                    .openings
                    .iter()
                    .find(|o| o.id == format!("landing-window-{storey}"))
                    .unwrap();
                if storey < 2 {
                    let landing = stairs.turn_landings[usize::from(storey)];
                    let bottom = recipe.floor_height(storey) + opening.sill - 0.07;
                    assert!((bottom - landing.max[2] - 0.02).abs() < 0.00001);
                    assert!(opening.sill + opening.height <= recipe.wall_height - 0.12 + 0.00001);
                } else {
                    assert!((opening.sill - 0.95).abs() < 0.00001);
                }
                let clear = prototype
                    .recipe
                    .openings
                    .iter()
                    .find(|o| o.id == format!("clear-window-{storey}"))
                    .unwrap();
                assert!((clear.sill - 0.95).abs() < 0.00001);
            }
            island_building::validate(&recipe).unwrap();
            let regenerated = generate(&prototype.recipe).unwrap();
            assert_eq!(regenerated.hash, prototype.hash);
            assert_eq!(regenerated.lods, prototype.lods);
        }
    }
}

#[test]
fn windows_are_removed_if_a_landing_would_push_them_above_the_lintel() {
    for axis in [StairAxis::X, StairAxis::Y] {
        for reversed in [false, true] {
            let recipe = fixture(2.4, axis, reversed);
            let prototype = generate(&recipe).unwrap();
            for storey in 0..3 {
                let present = prototype
                    .recipe
                    .openings
                    .iter()
                    .any(|o| o.id == format!("landing-window-{storey}"));
                assert_eq!(present, storey == 2);
                assert!(
                    prototype
                        .recipe
                        .openings
                        .iter()
                        .any(|o| o.id == format!("clear-window-{storey}"))
                );
            }
            let regenerated = generate(&prototype.recipe).unwrap();
            assert_eq!(regenerated.hash, prototype.hash);
            assert_eq!(regenerated.lods, prototype.lods);
        }
    }
}

#[test]
fn automatic_window_fitting_is_repeatable_and_preserves_the_source_recipe() {
    for height in [2.4, 4.] {
        for axis in [StairAxis::X, StairAxis::Y] {
            let mut recipe = BuildingRecipe::procedural(12., 10.);
            recipe.storeys = 3;
            recipe.wall_height = height;
            recipe.atlas_size = 128;
            recipe.chimney = None;
            recipe.staircase.axis = axis;
            let authoring = serde_json::to_vec(&recipe).unwrap();
            let prototype = generate(&recipe).unwrap();
            assert_eq!(serde_json::to_vec(&recipe).unwrap(), authoring);
            let regenerated = generate(&prototype.recipe).unwrap();
            assert_eq!(regenerated.hash, prototype.hash);
            assert_eq!(regenerated.lods, prototype.lods);
            island_building::validate(&recipe).unwrap();
        }
    }
}

#[test]
fn upper_stair_door_jambs_clear_the_floor_while_the_leaves_keep_their_original_height() {
    let recipe = fixture(2.4, StairAxis::Y, false);
    let prototype = generate(&recipe).unwrap();
    let trim = &prototype.lods[0]
        .parts
        .iter()
        .find(|p| p.material == MaterialSlot::Trim)
        .unwrap()
        .mesh;
    for storey in 0..3 {
        let floor = recipe.floor_height(storey);
        let door = prototype
            .doors
            .iter()
            .find(|d| d.id == format!("stairwell-door-{storey}"))
            .unwrap();
        assert!((door.hinge[2] - floor - 0.006).abs() < 0.00001);
        let (sin, cos) = door.closed_angle.sin_cos();
        let bottom = trim
            .positions
            .iter()
            .filter(|p| {
                let x = p[0] - door.hinge[0];
                let y = p[1] - door.hinge[1];
                let along = x * cos + y * sin;
                let depth = x * sin - y * cos;
                (-0.07..=0.01).contains(&along)
                    && (-0.3..=0.1).contains(&depth)
                    && p[2] >= floor - 0.00001
                    && p[2] <= floor + recipe.opening_defaults.door.height + 0.06
            })
            .map(|p| p[2])
            .min_by(f32::total_cmp)
            .unwrap();
        let gap = 0.02;
        assert!((bottom - floor - gap).abs() < 0.00001);
        if storey > 0 {
            let interior = &prototype.lods[0]
                .parts
                .iter()
                .find(|p| p.material == MaterialSlot::Interior)
                .unwrap()
                .mesh;
            assert!(
                !interior
                    .positions
                    .iter()
                    .zip(&interior.normals)
                    .any(|(p, n)| {
                        let x = p[0] - door.hinge[0];
                        let y = p[1] - door.hinge[1];
                        let along = x * cos + y * sin;
                        let depth = x * sin - y * cos;
                        n[2] > 0.9
                            && (p[2] - floor).abs() < 0.00001
                            && (-0.01..=door.width + 0.01).contains(&along)
                            && (-0.3..=0.1).contains(&depth)
                    }),
                "a wall cap remains coplanar with floor {storey}"
            );
        }
    }
}
