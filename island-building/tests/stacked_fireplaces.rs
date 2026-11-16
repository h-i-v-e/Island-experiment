use island_building::{
    BuildingMesh, BuildingRecipe, Facade, MaterialSlot, WallStyle, generate, geometry::chimney,
};

#[test]
fn every_floor_has_an_open_fireplace_and_hearth_under_one_roof_stack_on_each_facade() {
    for facade in Facade::ALL {
        for storeys in [2, 3, 6] {
            let mut recipe = BuildingRecipe::procedural(12., 10.);
            recipe.storeys = storeys;
            recipe.wall_height = 2.4;
            recipe.atlas_size = 128;
            recipe.walls.style = WallStyle::WattleAndDaub;
            recipe.chimney.as_mut().unwrap().facade = facade;
            let prototype = generate(&recipe).unwrap();
            let c = recipe.chimney.as_ref().unwrap();
            let room = prototype
                .layout
                .staircase
                .as_ref()
                .unwrap()
                .stairwell
                .as_ref()
                .unwrap();
            let normal = facade.normal();
            let axis = usize::from(normal[1].abs() > 0.5);
            let edge = if normal[axis] > 0. {
                room.bounds.max[axis]
            } else {
                -room.bounds.min[axis]
            };
            let distance = [recipe.width, recipe.depth][axis] * 0.5 - recipe.wall_thickness - edge;
            assert!(distance >= c.depth + c.hearth_depth + 0.66 - 0.00001);
            let slab = &prototype.lods[0]
                .parts
                .iter()
                .find(|p| p.material == MaterialSlot::StoneSlab)
                .unwrap()
                .mesh;
            let chimney_mesh = &prototype.lods[0]
                .parts
                .iter()
                .find(|p| p.material == MaterialSlot::Chimney)
                .unwrap()
                .mesh;
            for storey in 0..storeys {
                let hearth = chimney::hearth_bounds_on_floor(&recipe, storey).unwrap();
                assert!(prototype.colliders.contains(&hearth));
                assert!(
                    slab.positions
                        .iter()
                        .any(|p| (0..3).all(|i| (p[i] - hearth.max[i]).abs() < 0.00001))
                );
                let target = chimney::fireplace_centre_on_floor(&recipe, storey).unwrap();
                assert!(
                    !room.bounds.contains(target),
                    "fireplace must belong to the room"
                );
                let origin = facade.point(
                    &recipe,
                    c.offset,
                    target[2],
                    recipe.wall_thickness + c.depth + 0.4,
                );
                let hit = prototype
                    .colliders
                    .iter()
                    .filter_map(|b| b.ray_hit(origin, normal, 3.))
                    .min_by(f32::total_cmp)
                    .unwrap();
                assert!(
                    (hit - 0.4 - c.firebox_depth).abs() < 0.0001,
                    "floor {storey}: unobstructed firebox depth {hit}"
                );
                assert_breast_height(chimney_mesh, &recipe, storey);
            }
            assert!(chimney::hearth_bounds_on_floor(&recipe, storeys).is_none());
            let top = chimney::stack_bounds(&recipe).unwrap().max[2];
            assert_shared_stack_cap(chimney_mesh, top);
            assert!(
                prototype.lods[1]
                    .parts
                    .iter()
                    .all(|p| p.material != MaterialSlot::Firebox)
            );
        }
    }
}

fn assert_breast_height(mesh: &BuildingMesh, recipe: &BuildingRecipe, storey: u8) {
    let chimney = recipe.chimney.as_ref().unwrap();
    let base = recipe.floor_height(storey);
    let breast_start = base + chimney.hearth_height + chimney.firebox_height + 0.18;
    assert!(
        mesh.indices.chunks_exact(3).any(|triangle| {
            let heights =
                std::array::from_fn::<_, 3, _>(|i| mesh.positions[triangle[i] as usize][2]);
            heights.into_iter().fold(f32::INFINITY, f32::min) <= breast_start + 0.00001
                && heights.into_iter().fold(f32::NEG_INFINITY, f32::max)
                    >= base + recipe.wall_height - 0.00001
        }),
        "floor {storey}: rendered chimney breast spans continuously to the ceiling"
    );
}

fn assert_shared_stack_cap(mesh: &BuildingMesh, top: f32) {
    let cap_triangles = mesh
        .indices
        .chunks_exact(3)
        .filter(|triangle| {
            triangle.iter().all(|&index| {
                let index = index as usize;
                (mesh.positions[index][2] - top).abs() < 0.00001 && mesh.normals[index][2] > 0.5
            })
        })
        .count();
    assert_eq!(
        cap_triangles, 8,
        "one four-sided stone cap around the open flue"
    );
}

#[test]
fn manually_placing_the_stairwell_on_the_chimney_wall_is_rejected() {
    let mut recipe = BuildingRecipe::procedural(12., 10.);
    recipe.storeys = 3;
    recipe.staircase.axis = island_building::StairAxis::X;
    recipe.staircase.position = Some([0., 3.5]);
    let error = island_building::stairs::resolve(&recipe).unwrap_err();
    assert!(error.to_string().contains("staircase"));
    recipe.chimney = None;
    assert!(island_building::stairs::resolve(&recipe).is_ok());
}
