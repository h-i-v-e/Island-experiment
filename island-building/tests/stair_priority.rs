use island_building::{
    BuildingRecipe, DoorStyle, Facade, HingeSide, OpeningKind, OpeningOverride, StairAxis,
    WallStyle, generate, geometry::foundation, openings,
};

fn cottage() -> BuildingRecipe {
    let mut recipe = BuildingRecipe::procedural(6., 5.966_667);
    recipe.storeys = 3;
    recipe.wall_height = 2.4;
    recipe.atlas_size = 128;
    recipe.walls.style = WallStyle::WattleAndDaub;
    recipe.opening_defaults.window.width = 0.6;
    recipe
}

#[test]
fn moving_the_front_door_preference_keeps_the_stairs_against_the_same_external_wall() {
    let mut placement = None;
    let mut relocated = false;
    for preferred in [-0.65, 0., 0.65, 1.8, 2.1] {
        let mut recipe = cottage();
        recipe
            .opening_generation
            .as_mut()
            .unwrap()
            .overrides
            .push(OpeningOverride {
                id: "auto-door-south".into(),
                offset: Some(preferred),
                door_style: Some(DoorStyle::FrameAndPanel),
                ..Default::default()
            });
        let authoring = serde_json::to_vec(&recipe).unwrap();
        let prototype = generate(&recipe).unwrap();
        assert_eq!(serde_json::to_vec(&recipe).unwrap(), authoring);
        let stairs = prototype.layout.staircase.as_ref().unwrap();
        let current = (stairs.axis, stairs.position, stairs.reversed);
        assert_eq!(
            *placement.get_or_insert(current),
            current,
            "door preference must not move stairs"
        );
        let south_gap = stairs.opening.min[1] + recipe.depth * 0.5
            - recipe.wall_thickness
            - recipe.walls.frame_projection;
        assert!(
            (south_gap - 0.04).abs() < 0.00001,
            "stair footprint hugs the outside wall"
        );
        let entrance = prototype
            .recipe
            .openings
            .iter()
            .find(|o| o.id == "auto-door-south")
            .unwrap();
        relocated |= (entrance.offset - preferred).abs() > 0.1;
        if preferred > 2. {
            assert!(
                (entrance.offset - preferred).abs() < 0.00001,
                "clear near-corner preference stays put"
            );
        }
        assert_eq!(entrance.facade, Facade::South);
        assert_eq!(entrance.door_style, DoorStyle::FrameAndPanel);
        assert!(
            (foundation::stairs(&prototype.recipe).next().unwrap().offset - entrance.offset).abs()
                < 0.00001
        );
        island_building::validate(&recipe).unwrap();
    }
    assert!(
        relocated,
        "an obstructed front door must move along its facade"
    );
}

#[test]
fn manual_front_doors_can_move_without_changing_authoring_or_door_style() {
    let mut recipe = BuildingRecipe::procedural(8., 7.);
    recipe.storeys = 3;
    recipe.wall_height = 2.4;
    recipe.atlas_size = 128;
    recipe.chimney = None;
    recipe.staircase.axis = StairAxis::X;
    let mut recipe = openings::resolve(&recipe).unwrap().into_owned();
    recipe.opening_generation = None;
    let door = recipe
        .openings
        .iter_mut()
        .find(|o| o.kind == OpeningKind::Door)
        .unwrap();
    door.door_style = DoorStyle::FrameAndPanel;
    door.hinge = HingeSide::Right;
    let authoring = serde_json::to_vec(&recipe).unwrap();
    let prototype = generate(&recipe).unwrap();
    assert_eq!(serde_json::to_vec(&recipe).unwrap(), authoring);
    let moved = prototype
        .recipe
        .openings
        .iter()
        .find(|o| o.kind == OpeningKind::Door)
        .unwrap();
    assert!(moved.offset.abs() > 0.1);
    assert_eq!(moved.door_style, DoorStyle::FrameAndPanel);
    assert_eq!(moved.hinge, HingeSide::Right);
    // Low-room windows beside a landing may be omitted, but the remaining
    // manual windows keep their authored dimensions and positions.
    assert!(
        prototype
            .recipe
            .openings
            .iter()
            .filter(|o| o.kind == OpeningKind::Window)
            .all(|window| recipe.openings.iter().any(|original| original == window))
    );
}

#[test]
fn both_stair_axes_use_the_minimum_wall_clearance_with_multiple_entrances() {
    for axis in [StairAxis::X, StairAxis::Y] {
        let mut recipe = BuildingRecipe::procedural(9., 8.);
        recipe.storeys = 3;
        recipe.atlas_size = 128;
        recipe.chimney = None;
        recipe.walls.style = WallStyle::WattleAndDaub;
        recipe.staircase.axis = axis;
        recipe.opening_generation.as_mut().unwrap().door_facades =
            vec![Facade::South, Facade::West];
        let prototype = generate(&recipe).unwrap();
        let stairs = prototype.layout.staircase.as_ref().unwrap();
        let gap = (0..2)
            .flat_map(|i| {
                let half = [recipe.width, recipe.depth][i] * 0.5
                    - recipe.wall_thickness
                    - recipe.walls.frame_projection;
                [stairs.opening.min[i] + half, half - stairs.opening.max[i]]
            })
            .min_by(f32::total_cmp)
            .unwrap();
        assert!((gap - 0.04).abs() < 0.00001);
        assert_eq!(stairs.axis, axis);
        assert_eq!(prototype.doors.iter().filter(|d| d.exterior).count(), 2);
    }
}
