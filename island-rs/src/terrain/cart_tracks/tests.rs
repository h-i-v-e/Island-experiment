use super::*;
use crate::{ForestOptions, ISLAND_WORLD_METRES, River, RiverNode, Terrain};

fn fixture(elevation: impl Fn(Vec2) -> f32) -> Island {
    let mut island = Island::shared_forest_fixture(ForestOptions::default());
    let mut mesh = Mesh::default();
    for y in 0..=16 {
        for x in 0..=24 {
            let p = Vec2::new(100. + 5. * x as f32, 100. + 5. * y as f32);
            mesh.vertices
                .push(p.extend(elevation(p)) / ISLAND_WORLD_METRES);
            if x > 0 && y > 0 {
                let i = y * 25 + x;
                mesh.triangles.extend([i - 26, i - 25, i, i - 26, i, i - 1]);
            }
        }
    }
    island.coarser_lods[0] = mesh.clone();
    island.terrain = Terrain::new(mesh);
    island.rivers.clear();
    island.river_mesh = Mesh::default();
    island
}

fn exit(site: usize, x: f32, y: f32, dx: f32) -> CartTrackExit {
    CartTrackExit {
        settlement_index: site,
        position: [x, y, 6.],
        direction: [dx, 0.],
        half_width: 2.,
    }
}

#[test]
fn directional_choice_and_blocked_shortcut_stay_deterministic() {
    let island = fixture(|p| 6. + p.y * 0.04);
    let exits = [
        exit(0, 154., 140., 1.),
        exit(1, 135., 140., 1.),
        exit(2, 208., 140., -1.),
    ];
    let blocker = CartTrackBlocker {
        centre: [180., 140.],
        half_extents: [3., 9.],
        yaw: 0.35,
    };
    let planner = CartTrackPlanner::new(&island, std::slice::from_ref(&blocker));
    let plan = planner.plan(&exits);
    assert_eq!(plan.links[0].to_settlement, 2);
    assert!(plan.diagnostics.is_empty());
    assert!(plan.bridges.is_empty());
    assert_eq!(
        serde_json::to_string(&plan).unwrap(),
        serde_json::to_string(&planner.plan(&exits)).unwrap()
    );
    for run in &plan.runs {
        for p in &run.points {
            assert!(!blocker.intersects_circle(Vec2::new(p[0], p[1]), HALF_WIDTH));
            assert!((p[2] - (6. + p[1] * 0.04)).abs() < 0.001);
        }
    }
}

#[test]
fn bridge_reports_banks_and_splits_every_dry_run() {
    let mut island = fixture(|_| 6.);
    let nodes = [101., 179.].map(|y| RiverNode {
        vertex: 0,
        flow: 1,
        surface: 0.002,
        position: Vec3::new(160., y, 5.) / ISLAND_WORLD_METRES,
    });
    island.rivers.push(River {
        nodes: Vec::from(nodes),
        join: None,
    });
    let planner = CartTrackPlanner::new(&island, &[]);
    let plan = planner.plan(&[exit(0, 115., 140., 1.), exit(1, 205., 140., -1.)]);
    assert_eq!(plan.bridges.len(), 1, "{plan:?}");
    let bridge = &plan.bridges[0];
    assert_eq!(bridge.kind, Crossing::River);
    assert_eq!(plan.runs.len(), 2);
    assert_eq!(plan.runs[0].points.last(), Some(&bridge.start));
    assert_eq!(plan.runs[1].points.first(), Some(&bridge.end));
}

#[test]
fn dry_ravine_is_pending_and_sea_is_never_a_span() {
    for (floor, expected) in [(3., Some(Crossing::Ravine)), (-2., None)] {
        let island = fixture(|p| {
            if (155.0..=165.).contains(&p.x) {
                floor
            } else {
                10.
            }
        });
        let planner = CartTrackPlanner::new(&island, &[]);
        let plan = planner.plan(&[exit(0, 115., 140., 1.), exit(1, 205., 140., -1.)]);
        assert_eq!(plan.bridges.first().map(|b| b.kind), expected, "{plan:?}");
        assert_eq!(plan.links.len(), usize::from(expected.is_some()));
        if expected.is_none() {
            assert_eq!(plan.diagnostics.len(), 2);
        }
    }
}

#[test]
fn costs_prefer_level_ground_and_reject_excessive_crossfall() {
    let flat = fixture(|_| 10.);
    let side = fixture(|p| 10. + (p.y - 100.) * 0.30);
    let steep = fixture(|p| 10. + (p.y - 100.) * 0.65);
    let a = Vec2::new(120., 140.);
    let b = Vec2::new(190., 140.);
    let cost = |island| Surface::new(island, &[]).dry_cost(a, b);
    assert!(cost(&side).unwrap() > cost(&flat).unwrap());
    assert!(cost(&steep).is_none());
}

#[test]
fn river_water_inside_a_single_large_face_blocks_tracks() {
    let mut island = fixture(|_| 10.);
    island.river_mesh = Mesh {
        vertices: [
            Vec2::new(105., 105.),
            Vec2::new(195., 105.),
            Vec2::new(105., 175.),
        ]
        .map(|p| p.extend(11.) / ISLAND_WORLD_METRES)
        .to_vec(),
        triangles: vec![0, 1, 2],
        ..Mesh::default()
    };
    let surface = Surface::new(&island, &[]);
    assert!(
        surface
            .dry_cost(Vec2::new(120., 120.), Vec2::new(135., 120.))
            .is_none()
    );
    assert!(
        surface
            .dry_cost(Vec2::new(190., 150.), Vec2::new(205., 150.))
            .is_some()
    );
}

#[test]
fn bridge_rejects_banks_too_steep_for_an_abutment() {
    let island = fixture(|p| {
        if p.x < 150. {
            10. + (p.x - 150.) * 0.42
        } else if (155.0..=165.).contains(&p.x) {
            4.
        } else {
            10.
        }
    });
    let surface = Surface::new(&island, &[]);
    let bank = Vec2::new(150., 140.);
    assert!(surface.dry_cost(bank - 3. * Vec2::X, bank).is_some());
    assert_eq!(surface.crossing(bank, Vec2::new(170., 140.)), None);
}

#[test]
fn height_uses_the_uppermost_accurate_face_and_preserves_misses() {
    let mut island = fixture(|_| 5.);
    let mut mesh = island.terrain.mesh().clone();
    let start = mesh.vertices.len() as u32;
    for (x, y, z) in [(120., 120., 8.), (170., 120., 14.), (120., 170., 17.)] {
        mesh.vertices.push(Vec3::new(x, y, z) / ISLAND_WORLD_METRES);
    }
    mesh.triangles.extend([start, start + 1, start + 2]);
    island.terrain = Terrain::new(mesh);
    let surface = Surface::new(&island, &[]);
    for point in [
        Vec2::new(130., 130.),
        Vec2::new(120., 120.),
        Vec2::new(180., 120.),
        Vec2::new(99., 99.),
        Vec2::new(-2., -2.),
    ] {
        let hit = island
            .terrain
            .sample_accurate_surface(point / ISLAND_WORLD_METRES)
            .map(|hit| hit.position.z * ISLAND_WORLD_METRES);
        assert_eq!(surface.height(point), hit);
    }
}
