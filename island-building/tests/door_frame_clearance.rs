use island_building::{BuildingRecipe, Facade, MaterialSlot, OpeningKind, generate};

#[test]
fn exterior_frame_feet_clear_the_ground_floor_on_all_facades() {
    for facade in Facade::ALL {
        let mut r = BuildingRecipe::hut();
        r.width = 10.;
        r.depth = 8.;
        r.storeys = 2;
        r.chimney = None;
        r.atlas_size = 128;
        r.openings.retain(|o| o.kind == OpeningKind::Door);
        r.openings[0].facade = facade;
        let p = generate(&r).unwrap();
        let mesh = &p.lods[0]
            .parts
            .iter()
            .find(|p| p.material == MaterialSlot::Trim)
            .unwrap()
            .mesh;
        for door in p
            .recipe
            .openings
            .iter()
            .filter(|o| o.kind == OpeningKind::Door)
        {
            let floor = r.floor_height(door.storey);
            let t = facade.tangent();
            let n = facade.normal();
            let plane = if matches!(facade, Facade::North | Facade::South) {
                r.depth * 0.5
            } else {
                r.width * 0.5
            };
            let bottom = mesh
                .positions
                .iter()
                .filter(|point| {
                    let along = point[0] * t[0] + point[1] * t[1];
                    let outward = point[0] * n[0] + point[1] * n[1];
                    (along - door.offset).abs() < door.width * 0.5 + 0.08
                        && (plane - 0.01..=plane + 0.06).contains(&outward)
                        && point[2] > floor - 0.1
                        && point[2] < floor + door.height + 0.08
                })
                .map(|p| p[2])
                .min_by(f32::total_cmp)
                .unwrap();
            assert!(
                (bottom - floor - 0.02).abs() < 0.00001,
                "{facade:?} storey {}: frame at {bottom}, floor {floor}",
                door.storey
            );
        }
    }
}
