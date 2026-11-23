use super::*;
use crate::{
    ForestOptions, ISLAND_WORLD_METRES, Mesh, Terrain, TownStairFoot, Vec3, Vec4,
    forest::{ForestMeshes, TreePlacement},
    roads::{RoadNetwork, RoadStation, RoadStrip},
};
use geometry::{intersects_circle, segment_near};

const TRACK_Y: f32 = 800.;

fn fixture(height: impl Fn(Vec2) -> f32) -> Island {
    const SIDE: u32 = 61;
    let mut island = Island::shared_forest_fixture(ForestOptions::default());
    let mut vertices = Vec::new();
    for y in 0..SIDE {
        for x in 0..SIDE {
            let xy = Vec2::new(550. + x as f32 * 8., 550. + y as f32 * 8.);
            vertices.push(xy.extend(height(xy)) / ISLAND_WORLD_METRES);
        }
    }
    let triangles = (0..SIDE - 1)
        .flat_map(|y| {
            (0..SIDE - 1).flat_map(move |x| {
                let a = y * SIDE + x;
                [a, a + 1, a + SIDE, a + 1, a + SIDE + 1, a + SIDE]
            })
        })
        .collect();
    island.material.values = vec![Vec4::ZERO; vertices.len()];
    island.environment.values = vec![Vec2::ZERO; vertices.len()];
    island.distance_to_land = vec![0.; vertices.len()];
    island.submerged_river_carve = vec![0.; vertices.len()];
    let mut mesh = Mesh {
        vertices,
        triangles,
        ..Mesh::default()
    };
    mesh.calculate_normals();
    island.coarser_lods = [mesh.clone(), mesh.clone()];
    island.terrain = Terrain::new(mesh);
    island.forest = ForestMeshes::default();
    island
}

fn run(points: impl IntoIterator<Item = Vec2>) -> CartTrackRun {
    CartTrackRun {
        link_index: 0,
        points: points.into_iter().map(|xy| [xy.x, xy.y, 35.]).collect(),
        joins_track: false,
    }
}

fn straight(a: f32, b: f32) -> CartTrackRun {
    run([Vec2::new(a, TRACK_Y), Vec2::new(b, TRACK_Y)])
}

fn positions(values: &[f32]) -> impl Iterator<Item = Vec3> + '_ {
    values.chunks_exact(3).map(|p| Vec3::new(p[0], p[1], p[2]))
}

fn xy(values: &[f32]) -> Vec<Vec2> {
    positions(values).map(Vec3::truncate).collect()
}

fn quads(field: &Field) -> impl Iterator<Item = [Vec2; 4]> {
    let corners = xy(&field.outline);
    let count = corners.len() / 2;
    (0..count - 1).map(move |i| {
        [
            corners[i],
            corners[i + 1],
            corners[2 * count - i - 2],
            corners[2 * count - i - 1],
        ]
    })
}

#[test]
fn curves_fit_both_sides_and_emit_real_grounded_gate_openings() {
    let rising_ground = |p: Vec2| 35. + (p.x - 800.) * 0.04 + (p.y - TRACK_Y) * 0.03;
    let island = fixture(rising_ground);
    let curve = |x: f32| TRACK_Y + (x - 800.).powi(2) / 875.;
    let runs = [run((0..=80).map(|i| {
        let x = 600. + 5. * i as f32;
        Vec2::new(x, curve(x))
    }))];
    let output = island.plan_roadside_fields(&runs, &[], [0.25, 1.], 42);
    assert_eq!(
        output,
        island.plan_roadside_fields(&runs, &[], [0.25, 1.], 42)
    );
    assert!(output.fields.len() >= 10, "{} parcels", output.fields.len());
    assert_eq!(output.fields.len(), output.gates.len());
    assert!(output.fields.iter().any(|field| field.wheat));
    assert!(output.fields.iter().any(|field| !field.wheat));
    assert!(!output.windbreaks.is_empty());
    for side in [-1., 1.] {
        assert!(
            output.fields.iter().any(|field| {
                positions(&field.outline).all(|p| (p.y - curve(p.x)) * side > 3.5)
            })
        );
    }
    for (field, gate) in output.fields.iter().zip(&output.gates) {
        let outline = xy(&field.outline);
        let count = outline.len() / 2;
        assert_eq!(field.id, gate.field_id);
        assert!((25. ..=60.).contains(&polyline_length(&outline[..count])));
        assert!((19.99..=50.01).contains(&outline[0].distance(outline[2 * count - 1])));
        let [left, right] = [gate.left, gate.right].map(|point| Vec3::from_array(point).truncate());
        assert!((1.74..=1.841).contains(&left.distance(right)));
        let centre = (left + right) * 0.5;
        assert!(output.walls.iter().all(|wall| {
            let points = xy(&wall.points);
            points
                .windows(2)
                .all(|pair| !geometry::segments_within(pair[0], pair[1], centre, centre, 0.78))
        }));
    }
    for wall in &output.walls {
        assert!(positions(&wall.points).count() > 1);
        assert!(positions(&wall.points).all(|p| (p.z - rising_ground(p.truncate())).abs() < 0.001));
        assert!(
            xy(&wall.points)
                .windows(2)
                .all(|pair| pair[0].distance(pair[1]) <= 1.751)
        );
    }
    let all: Vec<_> = output
        .fields
        .iter()
        .map(|field| quads(field).collect::<Vec<_>>())
        .collect();
    for (i, field) in all.iter().enumerate() {
        assert!(all[..i].iter().all(|other| {
            field
                .iter()
                .all(|quad| other.iter().all(|p| !overlaps(quad, p)))
        }));
    }
}

#[test]
fn open_windbreak_keeps_dense_rows_and_places_bushes_close_to_the_wall() {
    let island = fixture(|_| 35.);
    let surface = Surface::new(&island, &[]);
    let run = straight(640., 960.);
    let network = TrackNetwork::new(&[run]);
    let layout = Layout::new(&network.routes[0], 1., 30., 85., 30.).unwrap();
    let plants = layout.windbreak(Vec2::NEG_Y, 42, &surface, &SpatialBins::new());
    assert!(
        (10..=13).contains(&plants.len()),
        "{} trees in a 55 m row",
        plants.len()
    );
    let rear_wall = TRACK_Y + FRONT_SETBACK + 30.;
    for tree in &plants {
        assert!((rear_wall - tree.position[1] - 1.5).abs() < 0.001);
    }
    assert!(plants[0].bushes.iter().all(Option::is_none));
    for pair in plants.windows(2) {
        let high_x = pair[0].position[0];
        let low_x = pair[1].position[0];
        let step = high_x - low_x;
        assert!((3.749..=5.001).contains(&step), "tree spacing {step} m");
        let [Some(first), Some(second)] = &pair[1].bushes else {
            panic!("two shrubs must fill each gap on an open, straight wall");
        };
        for bush in [first, second] {
            let radius = BUSH_RADIUS_PER_SCALE * bush.scale;
            assert!((1.45..=1.75).contains(&bush.scale));
            assert!((1.949..=2.201).contains(&(rear_wall - bush.position[1])));
            assert!((low_x..high_x).contains(&bush.position[0]));
            assert_eq!(bush.clearance_radius, radius + BUSH_WHEAT_MARGIN);
            assert!((bush.position[2] - 35.).abs() < 0.001);
        }
        let first_radius = first.scale * BUSH_RADIUS_PER_SCALE;
        let second_radius = second.scale * BUSH_RADIUS_PER_SCALE;
        assert!(first.position[0] + first_radius >= high_x - 0.7);
        assert!(second.position[0] - second_radius <= low_x + 0.7);
        assert!(
            horizontal_position(first.position).distance(horizontal_position(second.position))
                < first_radius + second_radius
        );
    }
}

#[test]
fn windbreak_uses_shared_forest_catalogue_and_collision() {
    let mut island = Island::shared_forest_fixture(ForestOptions {
        hybrid: Some(crate::forest::HybridForestOptions {
            latitude_degrees: 35.,
            altitude_cooling: 18.,
            variants_per_species: 2,
        }),
        ..ForestOptions::default()
    });
    let catalogue = std::sync::Arc::clone(&island.forest.hybrid.as_ref().unwrap().prototypes);
    let before = island.forest.placements().len();
    let before_crowns = island.forest.lod1_foliage.vertices.len();
    let (vertex, point) = island
        .terrain
        .mesh()
        .vertices
        .iter()
        .copied()
        .enumerate()
        .find(|(index, _)| {
            !island
                .forest
                .placements()
                .iter()
                .any(|tree| tree.terrain_vertex as usize == *index)
        })
        .unwrap();
    let mut plan = RoadsideFieldPlan {
        windbreaks: vec![Windbreak {
            position: (point * ISLAND_WORLD_METRES).to_array(),
            yaw: 0.4,
            scale: 0.8,
            clearance_radius: MIN_TRUNK_WHEAT_RADIUS,
            bushes: [
                Some(WindbreakBush {
                    position: ((point * ISLAND_WORLD_METRES) + Vec3::X * 2.).to_array(),
                    yaw: 0.7,
                    scale: 0.9,
                    clearance_radius: 0.972,
                }),
                None,
            ],
        }],
        ..RoadsideFieldPlan::default()
    };
    island.plant_roadside_windbreaks(&mut plan, 42).unwrap();
    assert_eq!(plan.windbreaks.len(), 1);
    assert_eq!(plan.bushes.len(), 1);
    let (catalogue_after, placements) = island.hybrid_forest().unwrap();
    assert!(std::sync::Arc::ptr_eq(
        &catalogue,
        &catalogue_after.prototypes
    ));
    assert_eq!(placements.len(), before + 1);
    let tree = placements.last().unwrap();
    assert_eq!(tree.terrain_vertex as usize, vertex);
    assert!((tree.anchor.truncate() - point.truncate()).length() < 1e-6);
    assert!(usize::from(tree.prototype) < catalogue.len());
    assert_eq!(island.forest.lod1_foliage.vertices.len(), before_crowns + 5);
    let colliders = island
        .forest
        .trunk_colliders()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(colliders.len(), before + 1);
    let trunk = colliders.last().unwrap();
    let tree = &plan.windbreaks[0];
    let centre = horizontal_position(tree.position);
    let trunk_offset = centre.distance(trunk.bottom.truncate() * ISLAND_WORLD_METRES);
    assert!(tree.clearance_radius >= trunk.radius * ISLAND_WORLD_METRES + trunk_offset + 0.37);
    island.plant_roadside_windbreaks(&mut plan, 42).unwrap();
    assert!(plan.windbreaks.is_empty());
    assert!(plan.bushes.is_empty());
    assert_eq!(island.forest.placements().len(), before + 1);
}

#[test]
fn adjacent_parcels_build_their_common_divider_once() {
    let island = fixture(|_| 35.);
    let output = island.plan_roadside_fields(&[straight(600., 1000.)], &[], [0., 1.], 19);
    let mut adjacent = 0;
    for pair in output.fields.windows(2) {
        let a = xy(&pair[0].outline);
        let b = xy(&pair[1].outline);
        let a_half = a.len() / 2;
        let b_half = b.len() / 2;
        let divider = a[a_half - 1];
        if divider.distance(b[0]) > 0.002 {
            continue;
        }
        adjacent += 1;
        let depths = [
            divider.distance(a[a_half]),
            divider.distance(b[2 * b_half - 1]),
        ];
        let segments = output.walls.iter().flat_map(|wall| {
            xy(&wall.points)
                .windows(2)
                .filter(|points| {
                    points.iter().all(|p| {
                        (p.x - divider.x).abs() < 0.002
                            && (p.y - TRACK_Y).signum() == (divider.y - TRACK_Y).signum()
                    })
                })
                .map(|points| points[0].distance(points[1]))
                .collect::<Vec<_>>()
        });
        assert!((segments.sum::<f32>() - depths[0].max(depths[1])).abs() < 0.01);
    }
    assert!(adjacent >= 4);
}

#[test]
fn reserves_bridge_approaches_junctions_water_forest_and_structure() {
    let mut island = fixture(|p| if p.y <= 759. { 25. } else { 35. });
    let tree = Vec2::new(688., 826.);
    island.forest.placements.push(TreePlacement {
        seed_ordinal: 0,
        terrain_vertex: 0,
        anchor: tree.extend(35.) / ISLAND_WORLD_METRES,
        yaw_radians: 0.,
        scale: 2.,
        prototype: 0,
    });
    island.river_mesh = Mesh {
        vertices: [
            Vec2::new(740., 824.),
            Vec2::new(745., 824.),
            Vec2::new(740., 1010.),
            Vec2::new(745., 1010.),
        ]
        .map(|xy| xy.extend(34.5) / ISLAND_WORLD_METRES)
        .into(),
        triangles: vec![0, 1, 2, 1, 3, 2],
        ..Mesh::default()
    };
    let structure = DecorationClearance {
        centre: [940., 826.],
        half_extents: [8., 8.],
        yaw: 0.4,
    };
    let runs = [
        straight(608., 781.),
        straight(804., 1000.),
        run([Vec2::new(843., 890.), Vec2::new(843., TRACK_Y)]),
    ];
    let output = island.plan_roadside_fields(&runs, &[structure], [0., 1.], 17);
    assert!(!output.fields.is_empty());
    for field in &output.fields {
        for quad in quads(field) {
            assert!(!intersects_circle(&quad, tree, 6.74));
            assert!(!intersects_circle(
                &quad,
                Vec2::from_array(structure.centre),
                8.
            ));
            assert!(!segment_near(
                &quad,
                Vec2::new(740., 824.),
                Vec2::new(740., 1010.),
                2.99
            ));
            assert!(quad.iter().all(|p| p.y >= 764.));
        }
        let outline = xy(&field.outline);
        let front = &outline[..outline.len() / 2];
        assert!(front.iter().all(|p| !(765.01..819.99).contains(&p.x)));
        assert!(
            front
                .iter()
                .all(|p| !(829.99..856.01).contains(&p.x) || p.y >= 817.)
        );
    }
}

#[test]
fn broad_bare_rock_bars_cultivation_and_upwind_trees_reduce_planting() {
    let mut island = fixture(|_| 35.);
    let runs = [straight(650., 946.)];
    for (i, vertex) in island.terrain.vertices().iter().enumerate() {
        let p = vertex.truncate() * ISLAND_WORLD_METRES;
        if p.y < TRACK_Y && p.x < 810. {
            island.material.values[i] = Vec4::new(1., 0., 0., 0.);
        }
    }
    let unsheltered = island.plan_roadside_fields(&runs, &[], [0., 1.], 23);
    assert!(
        unsheltered
            .fields
            .iter()
            .all(|field| { positions(&field.outline).all(|p| p.y > TRACK_Y || p.x > 798.) })
    );
    // Tree trunks sit beyond the furthest southern boundary, within its shelter distance.
    island.forest.placements.extend((0..24).map(|i| {
        let anchor = Vec3::new(664. + 12. * i as f32, 738., 35.) / ISLAND_WORLD_METRES;
        TreePlacement {
            seed_ordinal: i,
            terrain_vertex: 0,
            anchor,
            yaw_radians: 0.,
            scale: 2.,
            prototype: 0,
        }
    }));
    let sheltered = island.plan_roadside_fields(&runs, &[], [0., 1.], 23);
    assert_eq!(unsheltered.fields, sheltered.fields);
    assert!(unsheltered.windbreaks.len() > sheltered.windbreaks.len());
}

#[test]
fn parcels_respect_installed_paving_and_stair_footpaths() {
    let mut island = fixture(|_| 35.);
    let roads = RoadNetwork {
        strips: vec![RoadStrip {
            source: 0,
            start: 0,
            end: 1,
            half_width: 2.,
            stations: [Vec2::new(640., 834.), Vec2::new(960., 834.)]
                .into_iter()
                .map(|point| RoadStation { point, height: 35. })
                .collect(),
        }],
        junctions: Vec::new(),
        shoulder: 1.5,
    };
    island.apply_town_roads(&roads).unwrap();
    let foot = TownStairFoot {
        position: [800., 815.],
        house: [800., 811.],
    };
    let foundation = DecorationClearance {
        centre: foot.house,
        half_extents: [1.5, 2.],
        yaw: 0.,
    };
    assert!(island.paint_town_footpaths(&[foot], &[foundation]).unwrap() > 1);
    let terrain = island.terrain();
    let paint = terrain.footpaths.as_ref().unwrap();
    let marked: Vec<_> = terrain
        .vertices()
        .iter()
        .enumerate()
        .filter(|(i, _)| paint.contains_vertex(*i))
        .map(|(_, point)| point.truncate() * ISLAND_WORLD_METRES)
        .filter(|point| point.y < 825.)
        .collect();
    assert!(!marked.is_empty());
    let output = island.plan_roadside_fields(&[straight(640., 960.)], &[], [0., -1.], 14);
    assert!(
        output
            .fields
            .iter()
            .any(|field| xy(&field.outline)[0].y > TRACK_Y)
    );
    for field in &output.fields {
        for quad in quads(field) {
            assert!(!terrain.road_corridor_overlaps(bounds(&quad)[0], bounds(&quad)[1]));
            assert!(marked.iter().all(|point| !contains_convex(&quad, *point)));
        }
    }
}
