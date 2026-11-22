use super::*;
use crate::{ForestOptions, ISLAND_WORLD_METRES, River, RiverNode, Terrain};

pub(super) fn fixture(elevation: impl Fn(Vec2) -> f32) -> Island {
    fixture_at(100., elevation)
}

fn fixture_at(origin_x: f32, elevation: impl Fn(Vec2) -> f32) -> Island {
    let mut island = Island::shared_forest_fixture(ForestOptions::default());
    let mut mesh = Mesh::default();
    for y in 0..=16 {
        for x in 0..=24 {
            let p = Vec2::new(origin_x + 5. * x as f32, 100. + 5. * y as f32);
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

#[test]
fn adjacent_sections_only_accept_bridges_on_their_side_of_the_seam() {
    // Model the same ravine running through two loaded sections. Place each
    // mesh at its actual local edge so the same world seam is tested twice.
    for (origin_x, seam_x, inward) in [
        (ISLAND_WORLD_METRES - 120., ISLAND_WORLD_METRES, -1_f32),
        (0., 0., 1.),
    ] {
        let island = fixture_at(origin_x, |p| {
            if (135.0..=145.0).contains(&p.y) {
                4.
            } else {
                10.
            }
        });
        let surface = Surface::new(&island, &[]);
        let point = |offset, y| Vec2::new(seam_x + inward * offset, y);
        let crossing = |start, end| surface.crossing(point(start, 130.), point(end, 150.));

        assert_eq!(crossing(8., 8.), Some(Crossing::Ravine));
        assert_eq!(crossing(-8., -8.), None, "the neighbour owns this span");
        assert_eq!(crossing(-8., 8.), None, "banks straddle the seam");
        assert_eq!(crossing(0., 0.), None, "deck width straddles the seam");
        assert_eq!(crossing(1.4, 8.), None, "near bank has no local approach");
    }
}

fn exit(site: usize, x: f32, y: f32, dx: f32) -> CartTrackExit {
    CartTrackExit {
        settlement_index: site,
        position: [x, y, 6.],
        direction: [dx, 0.],
        half_width: 2.,
        curb: false,
    }
}

#[test]
fn routing_around_a_blocked_shortcut_stays_deterministic() {
    let island = fixture(|p| 6. + p.y * 0.04);
    let exits = [exit(0, 154., 140., 1.), exit(2, 208., 140., -1.)];
    let blocker = CartTrackBlocker {
        centre: [180., 140.],
        half_extents: [3., 9.],
        yaw: 0.35,
    };
    let planner = CartTrackPlanner::new(&island, std::slice::from_ref(&blocker));
    let plan = planner.plan(&exits, Some(0), &[], exits.len());
    assert_eq!(plan.links[0].to_settlement, 2);
    assert!(plan.diagnostics.is_empty());
    assert!(plan.bridges.is_empty());
    assert_eq!(
        serde_json::to_string(&plan).unwrap(),
        serde_json::to_string(&planner.plan(&exits, Some(0), &[], exits.len())).unwrap()
    );
    for run in &plan.runs {
        for p in &run.points {
            assert!(!blocker.intersects_circle(Vec2::new(p[0], p[1]), HALF_WIDTH));
            assert!((p[2] - (6. + p[1] * 0.04)).abs() < 0.001);
        }
    }
}

#[test]
fn villages_join_the_capital_tree_in_distance_order() {
    let island = fixture(|_| 10.);
    let mut south = exit(4, 153., 130., 0.);
    south.direction = [0., -1.];
    let mut north = exit(4, 153., 150., 0.);
    north.direction = [0., 1.];
    let mut curb = exit(4, 167., 142., 1.);
    curb.curb = true;
    let exits = [
        south,
        exit(4, 143., 140., -1.),
        north,
        exit(4, 163., 140., 1.),
        curb,
        exit(0, 119., 140., 1.),
        exit(1, 171., 140., -1.),
        exit(1, 179., 140., 1.),
        exit(2, 203., 140., -1.),
        exit(9, 215., 145., -1.),
    ];
    let centres = [
        Some(Vec2::new(113., 140.)),
        Some(Vec2::new(175., 140.)),
        Some(Vec2::new(207., 140.)),
        None,
        Some(Vec2::new(153., 140.)),
    ];
    let plan = CartTrackPlanner::new(&island, &[]).plan(&exits, Some(4), &centres, 9);
    let branches: Vec<_> = plan
        .links
        .iter()
        .map(|link| (link.from_settlement, link.to_settlement))
        .collect();
    assert_eq!(branches, [(4, 1), (4, 0), (1, 2), (2, 9)]);
    assert!(plan.diagnostics.is_empty());
}

#[test]
fn a_farther_cross_street_wins_when_the_nearest_usable_one_needs_a_detour() {
    let island = fixture(|_| 10.);
    let exits = [
        exit(0, 188., 144., 1.),
        exit(0, 178., 144., 1.),
        exit(0, 177., 162., 1.),
        exit(1, 207., 141., -1.),
    ];
    let blocker = CartTrackBlocker {
        centre: [187., 139.],
        half_extents: [5., 19.],
        yaw: 0.,
    };
    let planner = CartTrackPlanner::new(&island, &[blocker]);
    let mut graph = Graph::new(planner.mesh, &planner.surface);
    let entrances: Vec<_> = exits.iter().map(|exit| graph.entrance(exit)).collect();
    assert!(!entrances[0].usable());
    assert!(entrances[1].usable() && entrances[2].usable());
    assert!(exits[1].xy().distance(exits[3].xy()) < exits[2].xy().distance(exits[3].xy()));
    assert!(
        graph
            .route(
                &[1],
                &[3],
                &exits,
                &entrances,
                &BTreeSet::new(),
                exits[1].xy()
            )
            .is_some()
    );

    let plan = planner.plan(&exits, Some(0), &[], exits.len());
    assert_eq!(plan.links[0].from_exit, 2);
    assert_eq!(plan.links[0].to_exit, 3);
    assert_eq!(&plan.runs[0].points[0][..2], &exits[2].position[..2]);
    assert!(plan.diagnostics.is_empty());
}

#[test]
fn a_villages_outer_ring_exit_can_beat_a_closer_cross_street() {
    let island = fixture(|_| 10.);
    let mut curb = exit(1, 177., 162., 1.);
    curb.curb = true;
    let exits = [
        exit(0, 117., 162., 1.),
        exit(1, 178., 144., 1.),
        curb,
        exit(2, 207., 141., -1.),
    ];
    let blocker = CartTrackBlocker {
        centre: [187., 139.],
        half_extents: [5., 19.],
        yaw: 0.,
    };
    let planner = CartTrackPlanner::new(&island, &[blocker]);
    let centres = [
        Some(Vec2::new(116., 162.)),
        Some(Vec2::new(170., 152.)),
        Some(Vec2::new(209., 141.)),
    ];
    let plan = planner.plan(&exits, Some(0), &centres, exits.len());
    let branches: Vec<_> = plan
        .links
        .iter()
        .map(|link| (link.from_settlement, link.to_settlement, link.from_exit))
        .collect();
    assert_eq!(branches, [(0, 1, 0), (1, 2, 2)]);
    assert!(plan.diagnostics.is_empty());
}

#[test]
fn villages_without_access_to_the_capital_do_not_start_a_separate_network() {
    let island = fixture(|_| 10.);
    let exits = [exit(1, 125., 140., 1.), exit(2, 190., 140., -1.)];
    let plan = CartTrackPlanner::new(&island, &[]).plan(&exits, Some(0), &[], exits.len());
    assert!(plan.links.is_empty());
    assert_eq!(plan.diagnostics.len(), 2);
}

#[test]
fn later_village_branches_from_the_existing_track_without_redrawing_it() {
    let island = fixture(|_| 10.);
    let mut north = exit(2, 155., 172., 0.);
    north.direction = [0., -1.];
    let exits = [exit(0, 110., 120., 1.), exit(1, 175., 120., -1.), north];
    let plan = CartTrackPlanner::new(&island, &[]).plan(&exits, Some(0), &[], exits.len());
    assert_eq!(plan.links.len(), 2);
    assert_eq!(plan.links[0].to_settlement, 1);
    let branch = &plan.links[1];
    assert_eq!(branch.to_settlement, 2);
    assert!(branch.from_track);
    let [trunk, branch] = plan.runs.as_slice() else {
        panic!("expected one trunk and one branch");
    };
    assert!(branch.joins_track);
    assert!(trunk.points.contains(&branch.points[0]));
    assert!(branch.points[0][0] > 135. && branch.points[0][0] < 165.);
}

#[test]
fn report_places_departures_at_both_ends_and_each_real_branch_terminal() {
    let mut island = fixture(|_| 10.);
    let mut north = exit(2, 155., 172., 0.);
    north.direction = [0., -1.];
    let exits = [
        exit(0, 110., 120., 1.),
        exit(1, 175., 120., -1.),
        north,
        exit(0, 110., 155., -1.),
    ];
    let blocker = CartTrackBlocker {
        centre: [112.75, 122.05],
        half_extents: [0.3, 0.3],
        yaw: 0.,
    };
    let closed_exit = CartTrackBlocker {
        centre: [110., 155.],
        half_extents: [2., 2.],
        yaw: 0.,
    };
    let report = island
        .prepare_cart_tracks(&exits, &[blocker, closed_exit], false, 3, Some(0), &[])
        .unwrap();
    let report = serde_json::to_value(report).unwrap();
    assert!(
        report["links"]
            .as_array()
            .unwrap()
            .iter()
            .any(|link| link["from_track"] == true)
    );
    let signs = report["departures"].as_array().unwrap();
    assert_eq!(signs.len(), 3, "{signs:?}");
    for (index, sign) in signs.iter().enumerate() {
        let [x, y, z]: [f32; 3] = serde_json::from_value(sign["position"].clone()).unwrap();
        let direction: [f32; 2] = serde_json::from_value(sign["direction"].clone()).unwrap();
        assert_eq!(sign["settlement_index"], index);
        assert_eq!(sign["exit_index"], index);
        assert_eq!(sign["portal_side"], -1);
        assert_ne!(sign["destination_index"], index);
        let point = Vec2::new(x, y);
        let exit = exits[index];
        let outward = Vec2::from_array(exit.direction);
        assert!((point - exit.xy()).dot(outward) > 0.);
        assert!(Vec2::from_array(direction).dot(outward) > 0.);
        assert!((z - 10.).abs() < 0.001);
        assert!(!blocker.intersects_circle(point, 0.85));
        assert!(
            !island
                .terrain
                .cart_track_corridor_overlaps(point - 0.85, point + 0.85)
        );
    }
    let first: [f32; 3] = serde_json::from_value(signs[0]["position"].clone()).unwrap();
    assert!(first[1] < 120., "the building blocks the preferred verge");
}

#[test]
fn paved_streets_are_cheaper_than_equivalent_ground() {
    use crate::roads::{RoadNetwork, RoadStation, RoadStrip};

    let mut island = Island::shared_forest_fixture(ForestOptions::default());
    let station = |x| RoadStation {
        point: Vec2::new(x, 650.),
        height: 39.,
    };
    let network = RoadNetwork {
        strips: vec![RoadStrip {
            source: 0,
            start: 0,
            end: 1,
            half_width: 2.,
            stations: vec![station(635.), station(685.)],
        }],
        junctions: Vec::new(),
        shoulder: 2.,
    };
    island.apply_town_roads(&network).unwrap();
    let surface = Surface::new(&island, &[]);
    let cost = |y| {
        surface
            .dry_cost(Vec2::new(645., y), Vec2::new(675., y))
            .unwrap()
    };
    assert!(cost(650.) < 0.2 * cost(660.));
}

fn river_fixture(water_height: f32) -> Island {
    let mut island = fixture(|p| {
        if (155.0..=165.).contains(&p.x) {
            2.
        } else {
            10.
        }
    });
    let nodes = [101., 179.].map(|y| RiverNode {
        vertex: 0,
        flow: 1,
        surface: water_height / ISLAND_WORLD_METRES,
        position: Vec3::new(160., y, 2.) / ISLAND_WORLD_METRES,
    });
    island.rivers.push(River {
        nodes: Vec::from(nodes),
        join: None,
    });
    island
}

#[test]
fn bridge_reports_banks_and_splits_every_dry_run() {
    let island = river_fixture(4.);
    let planner = CartTrackPlanner::new(&island, &[]);
    let plan = planner.plan(
        &[exit(0, 115., 140., 1.), exit(1, 205., 140., -1.)],
        Some(0),
        &[],
        2,
    );
    assert_eq!(plan.bridges.len(), 1, "{plan:?}");
    let bridge = &plan.bridges[0];
    assert_eq!(bridge.kind, Crossing::River);
    assert_eq!(plan.runs.len(), 2);
    assert_eq!(plan.runs[0].points.last(), Some(&bridge.start));
    assert_eq!(plan.runs[1].points.first(), Some(&bridge.end));
    let before = &plan.runs[0].points;
    let after = &plan.runs[1].points;
    assert_eq!(bridge.start_heading, heading(&before[before.len() - 2..]));
    assert_eq!(bridge.end_heading, heading(&after[..2]));
    assert!(bridge.width > WHEEL_OFFSET * 2.);
    let json = serde_json::to_value(bridge).unwrap();
    assert_eq!(json["start_heading"].as_array().unwrap().len(), 2);
    assert_eq!(json["end_heading"].as_array().unwrap().len(), 2);
    assert!(json["width"].as_f64().is_some());
    let middle = Vec3::from_array(bridge.start).lerp(Vec3::from_array(bridge.end), 0.5);
    assert!(
        plan.bridge_clearance()
            .any(|area| area.intersects_circle(middle.truncate(), 0.))
    );
}

#[test]
fn a_straight_span_reserves_its_outer_wall_and_can_be_reused_by_another_route() {
    let island = river_fixture(4.);
    let surface = Surface::new(&island, &[]);
    let mut request = cart_bridge_request(
        [150., 140., 10.],
        [170., 140., 10.],
        BRIDGE_WIDTH,
        Crossing::River,
    );
    request.start_heading = Some([1., 0.4]);
    request.end_heading = Some([1., -0.4]);
    let layout = surface.bridge_layout(request).unwrap();
    let point = layout.point(10., layout.width * 0.5 + 0.35);
    let blocker = CartTrackBlocker {
        centre: point.to_array(),
        half_extents: [0.1; 2],
        yaw: 0.3,
    };
    let obstructed = Surface::new(&island, &[blocker]);
    let start = Vec3::from_array(request.start).truncate();
    assert!(obstructed.dry_cost(start - 3. * Vec2::X, start).is_some());
    assert!((point.y - start.y - (layout.width * 0.5 + 0.35)).abs() < 0.001);
    assert!(obstructed.bridge_layout(request).is_none());
    let occupied = BridgeFootprint::new(ordered(1, 2), &layout);
    assert!(
        occupied
            .sections
            .iter()
            .any(|area| area.intersects_circle(point, 0.))
    );
    assert!(!occupied.conflicts(&BridgeFootprint::new(ordered(2, 1), &layout)));
    request.start[1] += 1.;
    request.end[1] += 1.;
    let overlapping = surface.bridge_layout(request).unwrap();
    assert!(occupied.conflicts(&BridgeFootprint::new((3, 4), &overlapping)));
}

#[test]
fn an_angled_dry_approach_meets_the_straight_bridge_axis_at_both_banks() {
    let island = fixture(|_| 10.);
    let planner = CartTrackPlanner::new(&island, &[]);
    let start = [150., 140., 10.];
    let end = [170., 140., 10.];
    for (at_start, points, away) in [
        (false, vec![[147., 139., 10.], start], -Vec2::X),
        (true, vec![end, [173., 141., 10.]], Vec2::X),
    ] {
        let mut run = CartTrackRun {
            link_index: 0,
            points,
            joins_track: false,
        };
        let original = run.points.clone();
        let aligned = planner.align_landing(&mut run, at_start, away).unwrap();
        assert!(Vec2::from_array(aligned).distance(Vec2::X) < 0.001);
        assert_eq!(run.points.len(), original.len() + 1);
        assert_eq!(run.points.first(), original.first());
        assert_eq!(run.points.last(), original.last());
    }
}

#[test]
fn a_channel_above_the_banks_cannot_connect_settlements() {
    let island = river_fixture(18.);
    let plan = CartTrackPlanner::new(&island, &[]).plan(
        &[exit(0, 115., 140., 1.), exit(1, 205., 140., -1.)],
        Some(0),
        &[],
        2,
    );
    assert!(plan.bridges.is_empty());
    assert!(plan.links.is_empty());
    assert_eq!(plan.diagnostics.len(), 1);
    assert_eq!(plan.diagnostics[0].settlement_index, 1);
}

#[test]
fn dry_ravine_accepts_a_span_and_sea_disconnects_the_route() {
    for (floor, expected) in [(3., Some(Crossing::Ravine)), (-2., None)] {
        let island = fixture(|p| {
            if (155.0..=165.).contains(&p.x) {
                floor
            } else {
                10.
            }
        });
        let planner = CartTrackPlanner::new(&island, &[]);
        let plan = planner.plan(
            &[exit(0, 115., 140., 1.), exit(1, 205., 140., -1.)],
            Some(0),
            &[],
            2,
        );
        assert_eq!(plan.bridges.first().map(|b| b.kind), expected, "{plan:?}");
        assert_eq!(plan.links.len(), usize::from(expected.is_some()));
        if expected.is_none() {
            assert_eq!(plan.diagnostics.len(), 1);
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
