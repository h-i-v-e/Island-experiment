#![allow(clippy::too_many_lines)] // Fixture-heavy scenarios keep setup beside their assertions.

use super::*;
use super::{carving::*, channel::*, geometry::*, tracing::*, waterfalls::*};
use crate::Vec2;

#[test]
fn waterfall_plane_zones_and_upstream_blend_follow_the_classification_plane() {
    let patch = WaterfallPatch {
        river: 0,
        segment: 0,
        upper_vertex: 0,
        upper_centre: Vec2::new(0.4, 0.6),
        direction: Vec2::X,
        across: Vec2::Y,
        upper_surface: 0.3,
        lower_surface: 0.1,
        lower_floor: 0.08,
        half_width: 0.02,
        support_run: 0.03,
        pool: None,
    };
    assert_eq!(
        patch.plane_zone(patch.upper_centre - patch.direction * WaterfallPatch::face_run()),
        WaterfallPlaneZone::Face
    );
    assert_eq!(
        patch.plane_zone(patch.upper_centre),
        WaterfallPlaneZone::Face
    );
    assert_eq!(
        patch.plane_zone(
            patch.upper_centre
                - patch.direction * (WaterfallPatch::face_run() + WATERFALL_TARGET_EDGE_LENGTH)
        ),
        WaterfallPlaneZone::BeforeLip
    );
    assert_eq!(
        patch.plane_zone(patch.upper_centre + patch.direction * WATERFALL_TARGET_EDGE_LENGTH),
        WaterfallPlaneZone::AfterFoot
    );
    let lip = patch.upper_centre - patch.direction * WaterfallPatch::face_run();
    let blend_midpoint = lip - patch.direction * (WATERFALL_EDGE_BLEND_RUN * 0.5);
    let blend_end = lip - patch.direction * WATERFALL_EDGE_BLEND_RUN;
    assert_eq!(
        patch.upstream_pin_smoothing_weight(lip).to_bits(),
        1.0_f32.to_bits()
    );
    let midpoint_weight = patch.upstream_pin_smoothing_weight(blend_midpoint);
    assert!(
        (midpoint_weight - 0.5).abs() < 1.0e-4,
        "midpoint weight {midpoint_weight}"
    );
    assert_eq!(
        patch.upstream_pin_smoothing_weight(blend_end).to_bits(),
        0.0_f32.to_bits()
    );
    assert_eq!(
        patch
            .upstream_pin_smoothing_weight(patch.upper_centre)
            .to_bits(),
        0.0_f32.to_bits()
    );
    assert!(patch.contains_upstream_pin_blend(lip));
    assert!(patch.contains_upstream_pin_blend(blend_midpoint));
    assert!(patch.contains_upstream_pin_blend(blend_end));
    assert!(!patch.contains_upstream_pin_blend(patch.upper_centre));
}

fn build_test_river_mesh(
    network: &RiverNetwork,
    terrain: &mut Mesh,
    adjacency: &Adjacency,
) -> Mesh {
    let mut material = SurfaceMaterial::empty(terrain.vertices.len());
    network.build_mesh(terrain, adjacency, &mut material)
}

fn test_river_terrain<'a>(
    mesh: &'a mut Mesh,
    adjacency: &'a Adjacency,
    material: &'a mut SurfaceMaterial,
    bedrock_rates: &'a [f32],
    control_areas: &'a [f32],
) -> RiverTerrain<'a> {
    RiverTerrain {
        mesh,
        adjacency,
        material,
        bedrock_rates,
        control_areas,
    }
}

#[test]
fn pre_carve_valley_gently_lowers_several_rings_around_the_course() {
    let points = (0..3)
        .flat_map(|y| (0..=12).map(move |x| Vec2::new(x as f32, y as f32)))
        .collect::<Vec<_>>();
    let mut triangles = Vec::new();
    for y in 0..2 {
        for x in 0..12 {
            let lower_left = (y * 13 + x) as u32;
            let lower_right = lower_left + 1;
            let upper_left = lower_left + 13;
            let upper_right = upper_left + 1;
            triangles.extend([
                lower_left,
                lower_right,
                upper_left,
                upper_left,
                lower_right,
                upper_right,
            ]);
        }
    }
    let mut mesh = Mesh {
        vertices: points.iter().map(|point| point.extend(1.0)).collect(),
        triangles,
        ..Mesh::default()
    };
    let original = mesh.clone();
    let adjacency = mesh.adjacency();
    let vertex_at = |mesh: &Mesh, x: f32, y: f32| {
        mesh.vertices
            .iter()
            .position(|vertex| vertex.truncate() == Vec2::new(x, y))
            .unwrap()
    };
    let nodes = (0..3)
        .map(|y| {
            let vertex = vertex_at(&mesh, 6.0, y as f32);
            RiverNode {
                vertex,
                flow: 1,
                surface: 1.0,
                position: mesh.vertices[vertex],
            }
        })
        .collect::<Vec<_>>();
    let network = RiverNetwork {
        rivers: vec![River { nodes, join: None }],
        join_vertices: vec![None],
        waterfalls: vec![vec![false; 3]],
        river_mesh_ends: vec![None],
        max_flow: 1,
        max_height: 1.0,
        ocean: vec![false; mesh.vertices.len()],
        perimeter: vec![false; mesh.vertices.len()],
        cross_sections: vec![Vec::new()],
    };

    assert!(lower_precarve_river_valleys(&network, &mut mesh, &adjacency) > 0);

    let depths = (6..=10)
        .map(|x| 1.0 - mesh.vertices[vertex_at(&mesh, x as f32, 1.0)].z)
        .collect::<Vec<_>>();
    assert!((depths[0] - PRECARVE_VALLEY_CENTRE_DEPTH).abs() < f32::EPSILON);
    assert!(depths.windows(2).all(|pair| pair[0] > pair[1]));
    assert!(depths[3] > 0.0);
    assert_eq!(depths[4].to_bits(), 0.0_f32.to_bits());
    assert_eq!(mesh.triangles, original.triangles);
    assert!(mesh.vertices.iter().zip(original.vertices).all(
        |(lowered, initial)| lowered.truncate() == initial.truncate() && lowered.z <= initial.z
    ));
}

#[test]
fn reconciled_profile_pulls_terrain_and_water_below_the_stale_corridor() {
    let width = 7;
    let points = (0..width)
        .flat_map(|y| (0..width).map(move |x| Vec2::new(x as f32, y as f32)))
        .collect::<Vec<_>>();
    let mut triangles = Vec::new();
    for y in 0..width - 1 {
        for x in 0..width - 1 {
            let lower_left = (y * width + x) as u32;
            let lower_right = lower_left + 1;
            let upper_left = lower_left + width as u32;
            let upper_right = upper_left + 1;
            triangles.extend([
                lower_left,
                lower_right,
                upper_left,
                upper_left,
                lower_right,
                upper_right,
            ]);
        }
    }
    let mut mesh = Mesh {
        vertices: points.iter().map(|point| point.extend(1.0)).collect(),
        triangles,
        ..Mesh::default()
    };
    let node = |x: usize| {
        let vertex = 3 * width + x;
        RiverNode {
            vertex,
            flow: 10,
            surface: 0.25,
            position: mesh.vertices[vertex],
        }
    };
    let network = RiverNetwork {
        rivers: vec![River {
            nodes: (1..width - 1).map(node).collect(),
            join: None,
        }],
        join_vertices: vec![None],
        waterfalls: vec![vec![false; width - 2]],
        river_mesh_ends: vec![None],
        max_flow: 10,
        max_height: 1.0,
        ocean: vec![false; mesh.vertices.len()],
        perimeter: vec![false; mesh.vertices.len()],
        cross_sections: vec![vec![
            RiverCrossSection {
                target_half_width: 1.0,
                required_depth: 0.1,
                ..RiverCrossSection::default()
            };
            width - 2
        ]],
    };
    let adjacency = mesh.adjacency();

    assert!(lower_precarve_river_corridors_to_profiles(&network, &mut mesh, &adjacency) > 0);

    let footprint = build_river_footprint(&network, &mesh, &adjacency, false);
    let attributes = river_mesh_attributes(&mesh, &footprint.owner);
    let buffers = RiverMeshBuffers::new(footprint.coverage, attributes);
    let protected = vec![false; mesh.vertices.len()];
    let perimeter = mesh.perimeter_mask();
    lift_river_banks_to_surface(
        &mut mesh,
        &adjacency,
        &buffers.coverage,
        &buffers.surfaces,
        &buffers.target_half_widths,
        RiverBankLiftMasks {
            ocean: &network.ocean,
            perimeter: &perimeter,
            protected: &protected,
        },
    );
    for (vertex, owner) in footprint.owner.iter().copied().enumerate() {
        if let Some(owner) = owner {
            assert!(mesh.vertices[vertex].z <= owner.surface + f32::EPSILON);
        }
    }

    let constraints = WaterfallTerrainConstraints {
        patch: protected.clone(),
        pinned: protected.clone(),
        support: protected.clone(),
        water_unclamped: protected,
        terrain_ceiling: vec![f32::INFINITY; mesh.vertices.len()],
    };
    let river_mesh = duplicate_river_topology(
        &mesh,
        &buffers.coverage,
        &buffers.surfaces,
        &buffers.river_uv,
        &constraints,
    );
    assert!(!river_mesh.vertices.is_empty());
    assert!(
        river_mesh
            .vertices
            .iter()
            .all(|vertex| vertex.z <= 0.25 + RIVER_SURFACE_OFFSET + f32::EPSILON)
    );
}

#[test]
fn pre_carve_waterfall_shoulders_raise_low_banks_without_moving_the_channel() {
    let points = (0..=10)
        .flat_map(|y| (0..=8).map(move |x| Vec2::new(x as f32, y as f32)))
        .collect::<Vec<_>>();
    let mut triangles = Vec::new();
    for y in 0..10 {
        for x in 0..8 {
            let lower_left = (y * 9 + x) as u32;
            let lower_right = lower_left + 1;
            let upper_left = lower_left + 9;
            let upper_right = upper_left + 1;
            triangles.extend([
                lower_left,
                lower_right,
                upper_left,
                upper_left,
                lower_right,
                upper_right,
            ]);
        }
    }
    let mut mesh = Mesh {
        vertices: points.iter().map(|point| point.extend(1.0)).collect(),
        triangles,
        ..Mesh::default()
    };
    let vertex_at = |x: f32, y: f32| {
        mesh.vertices
            .iter()
            .position(|vertex| vertex.truncate() == Vec2::new(x, y))
            .unwrap()
    };
    let upper = vertex_at(3.0, 5.0);
    let lower = vertex_at(4.0, 5.0);
    let bank = vertex_at(3.0, 3.0);
    let second_bank_ring = vertex_at(3.0, 2.0);
    let blended_outer_ring = vertex_at(3.0, 1.0);
    let downstream_land = vertex_at(5.0, 3.0);
    for vertex in [bank, second_bank_ring, blended_outer_ring, downstream_land] {
        mesh.vertices[vertex].z = 0.1;
    }
    let adjacency = mesh.adjacency();
    let node = |vertex, surface| RiverNode {
        vertex,
        flow: 1,
        surface,
        position: mesh.vertices[vertex],
    };
    let network = RiverNetwork {
        rivers: vec![River {
            nodes: vec![node(upper, 0.8), node(lower, 0.4)],
            join: None,
        }],
        join_vertices: vec![None],
        waterfalls: vec![vec![true, false]],
        river_mesh_ends: vec![None],
        max_flow: 1,
        max_height: 1.0,
        ocean: vec![false; mesh.vertices.len()],
        perimeter: mesh.perimeter_mask(),
        cross_sections: vec![vec![
            RiverCrossSection {
                target_half_width: 0.5,
                ..RiverCrossSection::default()
            };
            2
        ]],
    };

    lower_precarve_river_valleys(&network, &mut mesh, &adjacency);
    let channel_heights = [mesh.vertices[upper].z, mesh.vertices[lower].z];
    let bank_before = mesh.vertices[bank].z;
    let outer_before = mesh.vertices[blended_outer_ring].z;
    let downstream_before = mesh.vertices[downstream_land].z;
    let lip_height = network.rivers[0].nodes[0].surface + mesh.vertices[upper].z
        - network.rivers[0].nodes[0].position.z;

    assert!(raise_precarve_waterfall_shoulders(&network, &mut mesh, &adjacency) > 0);

    assert_eq!(
        [
            mesh.vertices[upper].z.to_bits(),
            mesh.vertices[lower].z.to_bits(),
        ],
        channel_heights.map(f32::to_bits)
    );
    assert!((mesh.vertices[bank].z - lip_height).abs() < f32::EPSILON);
    assert!((mesh.vertices[second_bank_ring].z - lip_height).abs() < f32::EPSILON);
    assert!(mesh.vertices[bank].z > bank_before);
    assert!(mesh.vertices[blended_outer_ring].z > outer_before);
    assert!(mesh.vertices[blended_outer_ring].z < lip_height);
    assert_eq!(
        mesh.vertices[downstream_land].z.to_bits(),
        downstream_before.to_bits()
    );
}

#[test]
fn final_one_ring_footprint_bridges_an_early_confluence_gap() {
    let points = (0..3)
        .flat_map(|y| (0..=6).map(move |x| Vec2::new(x as f32, y as f32)))
        .collect::<Vec<_>>();
    let mut triangles = Vec::new();
    for y in 0..2 {
        for x in 0..6 {
            let lower_left = (y * 7 + x) as u32;
            let lower_right = lower_left + 1;
            let upper_left = lower_left + 7;
            let upper_right = upper_left + 1;
            triangles.extend([
                lower_left,
                lower_right,
                upper_left,
                upper_left,
                lower_right,
                upper_right,
            ]);
        }
    }
    let mesh = Mesh {
        vertices: points.iter().map(|point| point.extend(1.0)).collect(),
        triangles,
        ..Mesh::default()
    };
    let adjacency = mesh.adjacency();
    let vertex_at = |x: f32, y: f32| {
        mesh.vertices
            .iter()
            .position(|vertex| vertex.truncate() == Vec2::new(x, y))
            .unwrap()
    };
    let join = vertex_at(5.0, 1.0);
    let terminal = vertex_at(1.0, 1.0);
    let node = |vertex, surface| RiverNode {
        vertex,
        flow: 1,
        surface,
        position: mesh.vertices[vertex],
    };
    let network = RiverNetwork {
        rivers: vec![
            River {
                nodes: vec![node(join, 0.4), node(vertex_at(6.0, 1.0), 0.3)],
                join: None,
            },
            River {
                nodes: vec![node(vertex_at(0.0, 1.0), 0.8), node(terminal, 0.4)],
                join: Some(0),
            },
        ],
        join_vertices: vec![None, Some(join)],
        waterfalls: vec![vec![false; 2]; 2],
        river_mesh_ends: vec![None; 2],
        max_flow: 1,
        max_height: 1.0,
        ocean: vec![false; mesh.vertices.len()],
        perimeter: mesh.perimeter_mask(),
        cross_sections: vec![
            vec![
                RiverCrossSection {
                    target_half_width: 0.4,
                    required_depth: 0.2,
                    ..RiverCrossSection::default()
                };
                2
            ],
            vec![
                RiverCrossSection {
                    target_half_width: 0.3,
                    required_depth: 0.1,
                    ..RiverCrossSection::default()
                };
                2
            ],
        ],
    };

    let path = confluence_connector(&network, &adjacency, terminal, join);
    assert!(path.len() > 3);
    let footprint = build_river_footprint(&network, &mesh, &adjacency, true);

    assert!(path.iter().all(|&vertex| footprint.coverage[vertex] != 0));
    assert!(
        path.iter()
            .skip(1)
            .take(path.len() - 2)
            .all(|&vertex| footprint.ring_distance[vertex] == 0)
    );
    let middle = footprint.owner[path[path.len() / 2]].unwrap();
    assert!(middle.floor_override.is_some());
    assert!(middle.target_half_width > 0.3 && middle.target_half_width < 0.4);
}

#[test]
fn pre_carve_valley_connects_touching_river_centrelines_without_a_dam() {
    let points = (0..3)
        .flat_map(|y| (0..=6).map(move |x| Vec2::new(x as f32, y as f32)))
        .collect::<Vec<_>>();
    let mut triangles = Vec::new();
    for y in 0..2 {
        for x in 0..6 {
            let lower_left = (y * 7 + x) as u32;
            let lower_right = lower_left + 1;
            let upper_left = lower_left + 7;
            let upper_right = upper_left + 1;
            triangles.extend([
                lower_left,
                lower_right,
                upper_left,
                upper_left,
                lower_right,
                upper_right,
            ]);
        }
    }
    let mut mesh = Mesh {
        vertices: points.iter().map(|point| point.extend(1.0)).collect(),
        triangles,
        ..Mesh::default()
    };
    let adjacency = mesh.adjacency();
    let vertex_at = |x: f32, y: f32| {
        mesh.vertices
            .iter()
            .position(|vertex| vertex.truncate() == Vec2::new(x, y))
            .unwrap()
    };
    let join = vertex_at(5.0, 1.0);
    let tributary_terminal = vertex_at(1.0, 1.0);
    let node = |vertex| RiverNode {
        vertex,
        flow: 1,
        surface: 1.0,
        position: mesh.vertices[vertex],
    };
    let network = RiverNetwork {
        rivers: vec![
            River {
                nodes: vec![node(join), node(vertex_at(6.0, 1.0))],
                join: None,
            },
            River {
                nodes: vec![node(vertex_at(0.0, 1.0)), node(tributary_terminal)],
                join: Some(0),
            },
        ],
        join_vertices: vec![None, Some(join)],
        waterfalls: vec![vec![false; 2]; 2],
        river_mesh_ends: vec![None; 2],
        max_flow: 1,
        max_height: 1.0,
        ocean: vec![false; mesh.vertices.len()],
        perimeter: vec![false; mesh.vertices.len()],
        cross_sections: vec![
            vec![
                RiverCrossSection {
                    required_depth: 0.1,
                    ..RiverCrossSection::default()
                };
                2
            ];
            2
        ],
    };

    lower_precarve_river_valleys(&network, &mut mesh, &adjacency);

    let centre_height = 1.0 - PRECARVE_VALLEY_CENTRE_DEPTH;
    let mut reached = vec![false; mesh.vertices.len()];
    let mut pending = VecDeque::from([tributary_terminal]);
    reached[tributary_terminal] = true;
    while let Some(vertex) = pending.pop_front() {
        for &neighbour in &adjacency[vertex] {
            if !reached[neighbour] && mesh.vertices[neighbour].z <= centre_height + f32::EPSILON {
                reached[neighbour] = true;
                pending.push_back(neighbour);
            }
        }
    }
    assert!(reached[join]);

    // Corridor smoothing and the two independently owned river footprints
    // can leave this short connector raised again. The final confluence
    // pass must cut the whole centreline through to the interpolated river
    // floors, not merely lower a broad valley around it.
    let path = confluence_connector(&network, &adjacency, tributary_terminal, join);
    for &vertex in &path {
        mesh.vertices[vertex].z = 1.2;
    }
    let footprint = RiverFootprint {
        coverage: vec![2; mesh.vertices.len()],
        ring_distance: vec![0; mesh.vertices.len()],
        owner: vec![None; mesh.vertices.len()],
    };
    let mut material = SurfaceMaterial::empty(mesh.vertices.len());
    let bedrock_rates = vec![1.0; mesh.vertices.len()];
    let control_areas = projected_vertex_control_areas(&mesh);
    let mut budgets = vec![RiverSedimentBudget::default(); 2];
    let mut terrain = test_river_terrain(
        &mut mesh,
        &adjacency,
        &mut material,
        &bedrock_rates,
        &control_areas,
    );

    assert!(
        carve_confluence_connectors(
            &network,
            &mut terrain,
            &footprint,
            RiverChannelParameters {
                depth_multiplier: 1.0,
            },
            &mut budgets,
        ) >= path.len()
    );
    assert!(
        path.iter()
            .all(|&vertex| terrain.mesh.vertices[vertex].z <= 0.9 + f32::EPSILON)
    );
}

#[test]
fn river_uv_u_is_shortest_mesh_distance_from_the_bank() {
    let points: Vec<Vec2> = (0..=2)
        .flat_map(|y| (0..=2).map(move |x| Vec2::new(x as f32 * 0.5, y as f32 * 0.5)))
        .collect();
    let mut mesh = Mesh::delaunay(&points);
    mesh.uv = mesh
        .vertices
        .iter()
        .map(|vertex| Vec2::new(-1.0, vertex.y + 3.0))
        .collect();
    let downstream = mesh.uv.iter().map(|uv| uv.y).collect::<Vec<_>>();
    let perimeter = mesh.perimeter_mask();
    let centre = mesh
        .vertices
        .iter()
        .position(|vertex| vertex.truncate() == Vec2::splat(0.5))
        .unwrap();

    encode_bank_distance_in_uv(&mut mesh);

    for (vertex, &is_bank) in perimeter.iter().enumerate() {
        if is_bank {
            assert_eq!(mesh.uv[vertex].x.to_bits(), 0.0_f32.to_bits());
        }
        assert_eq!(mesh.uv[vertex].y.to_bits(), downstream[vertex].to_bits());
    }
    assert!((mesh.uv[centre].x - 0.5).abs() < 1.0e-6);
}

#[test]
fn source_cutoff_is_an_absolute_world_space_area() {
    let rule = RiverSourceRule::new(0.5, 1.0, 0.0, 0.2);

    assert_eq!(
        rule.required_catchment(0.0, 0.0).to_bits(),
        5_000_f32.to_bits()
    );
    assert_eq!(
        rule.required_catchment(1.0, 0.2).to_bits(),
        5_000_f32.to_bits()
    );
}

#[test]
fn catchment_accumulates_projected_land_area_in_square_metres() {
    let mesh = Mesh {
        vertices: vec![
            Vec3::new(0.0, 0.0, 4.0),
            Vec3::new(1.0, 0.0, 3.0),
            Vec3::new(1.0, 1.0, 2.0),
            Vec3::new(0.0, 1.0, 1.0),
        ],
        triangles: vec![0, 1, 2, 0, 2, 3],
        ..Mesh::default()
    };
    let downstream = [1, 2, 3, 3];

    let (flow, catchment) = calculate_flow_and_catchment(&mesh, &downstream);

    assert_eq!(flow, [1, 2, 3, 4]);
    assert!((catchment[3] - ISLAND_WORLD_METRES * ISLAND_WORLD_METRES).abs() < 0.5);
}

#[test]
fn source_cutoff_rises_smoothly_with_routing_grade() {
    let rule = RiverSourceRule::new(0.5, 4.0, 0.0, 0.2);

    assert_eq!(
        rule.required_catchment(0.0, 0.2).to_bits(),
        5_000_f32.to_bits()
    );
    assert_eq!(
        rule.required_catchment(0.5, 0.2).to_bits(),
        8_750_f32.to_bits()
    );
    assert_eq!(
        rule.required_catchment(1.0, 0.2).to_bits(),
        20_000_f32.to_bits()
    );
    assert_eq!(
        RiverSourceRule::new(0.5, 1.0, 0.0, 0.2)
            .required_catchment(1.0, 0.2)
            .to_bits(),
        5_000_f32.to_bits()
    );
}

#[test]
fn source_cutoff_falls_smoothly_with_elevation() {
    let rule = RiverSourceRule::new(0.5, 1.0, 9.0, 0.2);

    assert_eq!(
        rule.required_catchment(0.0, 0.0).to_bits(),
        50_000_f32.to_bits()
    );
    assert_eq!(
        rule.required_catchment(0.0, 0.1).to_bits(),
        27_500_f32.to_bits()
    );
    assert_eq!(
        rule.required_catchment(0.0, 0.2).to_bits(),
        5_000_f32.to_bits()
    );
    assert_eq!(
        rule.required_catchment(0.0, 0.4).to_bits(),
        5_000_f32.to_bits()
    );
}

#[test]
fn source_grade_uses_the_routed_edge_and_handles_sinks() {
    let mesh = Mesh {
        vertices: vec![
            Vec3::new(0.0, 0.0, 1.0),
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(0.0, 1.0, 2.0),
        ],
        ..Mesh::default()
    };

    assert!((source_grade(&mesh, 0, 1) - 0.5_f32.sqrt()).abs() < 1.0e-6);
    assert_eq!(source_grade(&mesh, 0, 0).to_bits(), 0.0_f32.to_bits());
    assert_eq!(source_grade(&mesh, 0, 2).to_bits(), 0.0_f32.to_bits());
}

#[test]
fn effective_height_noise_can_select_a_gentler_edge_on_flat_but_not_steep_terrain() {
    let mut mesh = Mesh {
        vertices: vec![
            Vec3::new(0.0, 0.0, 0.1),
            Vec3::new(0.01, 0.0, 0.099_99),
            Vec3::new(0.0, 0.01, 0.099_985),
        ],
        ..Mesh::default()
    };
    let meander_scale = RiverMeanderScale::from_average_edge_length(0.01);
    let seed = (0..1_024)
        .find(|&seed| {
            let current_height = river_effective_height(&mesh, 0, seed, meander_scale);
            river_route_score(&mesh, 0, 1, seed, meander_scale, current_height)
                > river_route_score(&mesh, 0, 2, seed, meander_scale, current_height)
        })
        .expect("at least one seed should bias the flat route away from its steepest edge");

    mesh.vertices[1].z = 0.098;
    mesh.vertices[2].z = 0.096;
    let current_height = river_effective_height(&mesh, 0, seed, meander_scale);

    assert!(
        river_route_score(&mesh, 0, 2, seed, meander_scale, current_height)
            > river_route_score(&mesh, 0, 1, seed, meander_scale, current_height)
    );
}

#[test]
fn meander_noise_scale_is_directly_proportional_to_average_edge_length() {
    let coarse = RiverMeanderScale::from_average_edge_length(0.02);
    let fine = RiverMeanderScale::from_average_edge_length(0.01);

    assert_eq!(
        coarse.wavelength.to_bits(),
        (fine.wavelength * 2.0).to_bits()
    );
    assert_eq!(coarse.height.to_bits(), (fine.height * 2.0).to_bits());
}

#[test]
fn sources_are_the_upstream_boundary_of_local_candidates() {
    let mesh = Mesh {
        vertices: vec![
            Vec3::new(0.0, 0.0, 0.2),
            Vec3::new(1.0, 0.0, 0.2),
            Vec3::new(0.0, 1.0, 0.1),
            Vec3::new(1.0, 1.0, 0.1),
        ],
        triangles: vec![0, 1, 2, 1, 3, 2],
        ..Mesh::default()
    };
    let adjacency = mesh.adjacency();
    let downstream = [2, 3, 2, 3];
    let catchment_areas = [50_000.0, 200_000.0, 300_000.0, 400_000.0];

    let sources = find_sources(
        &mesh,
        &adjacency,
        &downstream,
        &catchment_areas,
        RiverSourceRule::new(1.0, 1.0, 0.0, 0.2),
    );

    assert_eq!(sources, [1, 0]);
}

#[test]
fn low_elevation_sources_require_more_catchment_instead_of_being_excluded() {
    let mesh = Mesh {
        vertices: vec![
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(0.0, 1.0, 0.2),
        ],
        triangles: vec![0, 1, 2],
        ..Mesh::default()
    };
    let adjacency = mesh.adjacency();
    let downstream = [0, 1, 2];
    let catchment_areas = [49_999.0, 50_000.0, 5_000.0];

    let sources = find_sources(
        &mesh,
        &adjacency,
        &downstream,
        &catchment_areas,
        RiverSourceRule::new(0.5, 1.0, 9.0, 0.2),
    );

    assert_eq!(sources, [1, 2]);
}

#[test]
fn vertices_within_ten_centimetres_of_sea_level_move_away_from_it() {
    let mut terrain = Mesh {
        vertices: vec![
            Vec3::new(0.0, 0.0, -0.000_01),
            Vec3::new(0.0, 0.0, -0.001),
            Vec3::ZERO,
            Vec3::new(0.0, 0.0, 0.000_01),
            Vec3::new(0.0, 0.0, 0.001),
        ],
        ..Mesh::default()
    };

    enforce_sea_plane_clearance(&mut terrain, &[]);

    assert_eq!(
        terrain.vertices[0].z.to_bits(),
        (-SEA_PLANE_CLEARANCE).to_bits()
    );
    assert_eq!(terrain.vertices[1].z.to_bits(), (-0.001_f32).to_bits());
    assert_eq!(
        terrain.vertices[2].z.to_bits(),
        (-SEA_PLANE_CLEARANCE).to_bits()
    );
    assert_eq!(
        terrain.vertices[3].z.to_bits(),
        SEA_PLANE_CLEARANCE.to_bits()
    );
    assert_eq!(terrain.vertices[4].z.to_bits(), 0.001_f32.to_bits());
}

#[test]
fn final_clearance_keeps_flood_filled_ocean_vertices_below_sea() {
    let mut terrain = Mesh {
        vertices: vec![Vec3::new(0.0, 0.0, 0.001), Vec3::new(1.0, 0.0, 0.000_01)],
        ..Mesh::default()
    };

    enforce_sea_plane_clearance(&mut terrain, &[true, false]);

    assert_eq!(
        terrain.vertices[0].z.to_bits(),
        (-SEA_PLANE_CLEARANCE).to_bits()
    );
    assert_eq!(
        terrain.vertices[1].z.to_bits(),
        SEA_PLANE_CLEARANCE.to_bits()
    );
}

#[test]
fn final_sharp_point_repair_refines_and_rounds_an_isolated_spike() {
    let points: Vec<Vec2> = (0..=4)
        .flat_map(|y| (0..=4).map(move |x| Vec2::new(x as f32 / 4.0, y as f32 / 4.0)))
        .collect();
    let mut terrain = Mesh::delaunay(&points);
    let center = points
        .iter()
        .position(|point| *point == Vec2::new(0.5, 0.5))
        .unwrap();
    terrain.vertices[center].z = 0.2;
    let original_vertex_count = terrain.vertices.len();
    let original_height = terrain.vertices[center].z;
    let mut material = SurfaceMaterial::empty(original_vertex_count);
    let mut buffers = RiverMeshBuffers {
        coverage: vec![0; original_vertex_count],
        surfaces: vec![0.0; original_vertex_count],
        river_uv: vec![Vec2::ZERO; original_vertex_count],
        owners: vec![None; original_vertex_count],
        waterfall_lips: vec![false; original_vertex_count],
        target_half_widths: vec![0.0; original_vertex_count],
        target_depths: vec![0.0; original_vertex_count],
    };

    buffers.repair_sharp_points(&mut terrain, &mut material);

    assert!(terrain.vertices.len() > original_vertex_count);
    assert!(terrain.vertices[center].z < original_height * 0.6);
    let repaired_adjacency = terrain.adjacency();
    let repaired_perimeter = terrain.perimeter_mask();
    assert!(!sharp_point_mask(&terrain, &repaired_adjacency, &repaired_perimeter)[center]);
    assert_eq!(material.depths().len(), terrain.vertices.len());
    assert_eq!(buffers.coverage.len(), terrain.vertices.len());
    assert_eq!(buffers.surfaces.len(), terrain.vertices.len());
    assert_eq!(buffers.river_uv.len(), terrain.vertices.len());
    assert_eq!(buffers.owners.len(), terrain.vertices.len());
    assert_eq!(buffers.waterfall_lips.len(), terrain.vertices.len());
    assert_eq!(buffers.target_half_widths.len(), terrain.vertices.len());
    assert_eq!(buffers.target_depths.len(), terrain.vertices.len());
}

#[test]
fn final_sharp_point_repair_leaves_an_inclined_plane_unchanged() {
    let points: Vec<Vec2> = (0..=4)
        .flat_map(|y| (0..=4).map(move |x| Vec2::new(x as f32 / 4.0, y as f32 / 4.0)))
        .collect();
    let mut terrain = Mesh::delaunay(&points);
    terrain
        .vertices
        .iter_mut()
        .for_each(|vertex| vertex.z = vertex.x.mul_add(0.2, vertex.y * 0.1));
    let original = terrain.clone();
    let vertex_count = terrain.vertices.len();
    let mut material = SurfaceMaterial::empty(vertex_count);
    let mut buffers = RiverMeshBuffers {
        coverage: vec![0; vertex_count],
        surfaces: vec![0.0; vertex_count],
        river_uv: vec![Vec2::ZERO; vertex_count],
        owners: vec![None; vertex_count],
        waterfall_lips: vec![false; vertex_count],
        target_half_widths: vec![0.0; vertex_count],
        target_depths: vec![0.0; vertex_count],
    };

    buffers.repair_sharp_points(&mut terrain, &mut material);

    assert_eq!(terrain, original);
}

#[test]
fn ocean_mask_excludes_a_disconnected_subsea_basin() {
    let points: Vec<Vec2> = (0..=4)
        .flat_map(|y| (0..=4).map(move |x| Vec2::new(x as f32 / 4.0, y as f32 / 4.0)))
        .collect();
    let mut mesh = Mesh::delaunay(&points);
    mesh.vertices
        .iter_mut()
        .enumerate()
        .for_each(|(index, vertex)| {
            let (x, y) = (index % 5, index / 5);
            vertex.z = if x == 0 || x == 4 || y == 0 || y == 4 {
                -0.1
            } else {
                0.1
            };
        });
    let basin = points
        .iter()
        .position(|point| *point == Vec2::splat(0.5))
        .unwrap();
    mesh.vertices[basin].z = -0.1;
    let adjacency = mesh.adjacency();

    let ocean = fix_inland_seas(&mut mesh, &adjacency);

    assert!(!ocean[basin]);
    assert_eq!(mesh.vertices[basin].z.to_bits(), f32::EPSILON.to_bits());
    assert!(ocean.iter().enumerate().all(|(vertex, &is_ocean)| {
        let (x, y) = (vertex % 5, vertex / 5);
        x != 0 && x != 4 && y != 0 && y != 4 || is_ocean
    }));
}

#[test]
fn river_reaches_sea_only_when_its_terminal_vertex_is_in_the_ocean_mask() {
    let river = River {
        nodes: vec![RiverNode {
            vertex: 1,
            flow: 1,
            surface: -0.1,
            position: Vec3::new(0.5, 0.5, -0.1),
        }],
        join: None,
    };

    assert!(!river_reaches_ocean(&river, &[true, false]));
    assert!(river_reaches_ocean(&river, &[false, true]));
}

#[test]
fn waterfall_height_and_frequency_increase_with_smoothed_gradient() {
    let mut surface = 0.0_f32;
    let mut nodes = Vec::new();
    for index in (0..=20).rev() {
        if index < 20 {
            surface += if index < 10 { 0.012 } else { 0.002 };
        }
        nodes.push(RiverNode {
            vertex: index,
            flow: 10,
            surface,
            position: Vec3::new(index as f32 * 0.02, 0.5, surface),
        });
    }
    nodes.reverse();
    let outlet_surface = nodes[20].surface;
    let mut waterfalls = vec![false; nodes.len()];
    let mut scratch = Vec::new();

    form_stepped_profile(&mut nodes, &mut waterfalls, &[], 20, 0.2, &mut scratch);

    let mut gentle = Vec::new();
    let mut steep = Vec::new();
    for (index, pair) in nodes.windows(2).enumerate() {
        let drop = pair[0].surface - pair[1].surface;
        if waterfalls[index] {
            if index < 10 {
                steep.push(drop);
            } else {
                gentle.push(drop);
            }
        } else {
            assert!(drop.abs() < 1.0e-7);
        }
    }
    assert!(!gentle.is_empty());
    assert!(!steep.is_empty());
    assert!(steep.len() > gentle.len());
    assert!(steep.len() >= 8);
    let gentle_average = gentle.iter().sum::<f32>() / gentle.len() as f32;
    let steep_average = steep.iter().sum::<f32>() / steep.len() as f32;
    assert!(steep_average > gentle_average * 1.35);
    assert!(steep.iter().all(|height| *height <= 0.0036 + 1.0e-7));
    assert!((nodes[20].surface - outlet_surface).abs() < f32::EPSILON);
}

#[test]
fn waterfall_spacing_contains_the_full_channel_width_patch() {
    let mut nodes = (0..=30)
        .map(|index| RiverNode {
            vertex: index,
            flow: 10,
            surface: (30 - index) as f32 * 0.005,
            position: Vec3::new(index as f32 * 0.001, 0.5, 0.0),
        })
        .collect::<Vec<_>>();
    let sections = vec![
        RiverCrossSection {
            target_half_width: 0.002,
            ..RiverCrossSection::default()
        };
        nodes.len()
    ];
    let mut waterfalls = vec![false; nodes.len()];
    let mut scratch = Vec::new();

    form_stepped_profile(
        &mut nodes,
        &mut waterfalls,
        &sections,
        30,
        0.2,
        &mut scratch,
    );

    let waterfall_segments = waterfalls
        .iter()
        .enumerate()
        .filter_map(|(segment, &waterfall)| waterfall.then_some(segment))
        .collect::<Vec<_>>();
    let minimum_spacing = WATERFALL_SUPPORT_RUN
        + sections[0].target_half_width * (1.0 + WATERFALL_LANDING_LENGTH_MULTIPLIER);
    assert!(waterfall_segments.len() >= 2);
    assert!(
        waterfall_segments
            .windows(2)
            .all(|pair| { (pair[1] - pair[0]) as f32 * 0.001 + f32::EPSILON >= minimum_spacing })
    );
}

#[test]
fn final_profile_limits_bed_grade_without_creating_late_waterfalls() {
    let mesh = Mesh {
        vertices: vec![
            Vec3::new(0.0, 0.0, 1.0),
            Vec3::new(0.1, 0.0, 1.0),
            Vec3::new(0.2, 0.0, 0.9),
        ],
        ..Mesh::default()
    };
    let nodes = vec![
        RiverNode {
            vertex: 0,
            flow: 1,
            surface: 1.0,
            position: mesh.vertices[0],
        },
        RiverNode {
            vertex: 1,
            flow: 2,
            surface: 1.0,
            position: mesh.vertices[1],
        },
        RiverNode {
            vertex: 2,
            flow: 3,
            surface: 0.9,
            position: mesh.vertices[2],
        },
    ];
    let waterfalls = vec![false; nodes.len()];
    let mut sections = vec![
        RiverCrossSection {
            required_depth: 0.10,
            ..RiverCrossSection::default()
        },
        RiverCrossSection {
            required_depth: 0.30,
            ..RiverCrossSection::default()
        },
        RiverCrossSection {
            required_depth: 0.50,
            ..RiverCrossSection::default()
        },
    ];

    let adjusted = enforce_gentle_river_profile(&mesh, &nodes, &waterfalls, &mut sections);

    let maximum_gentle_drop = 0.1 * MAXIMUM_GENTLE_RIVER_GRADE;
    let first_floor_drop = (nodes[0].surface - sections[0].required_depth)
        - (nodes[1].surface - sections[1].required_depth);
    assert!(first_floor_drop <= maximum_gentle_drop + f32::EPSILON);
    assert!(!waterfalls[1]);
    assert_eq!(sections[2].required_depth.to_bits(), 0.50_f32.to_bits());
    assert_eq!(adjusted, 1);
}

#[test]
fn higher_confluence_reach_is_lowered_back_to_the_nearest_waterfall() {
    let initial_surfaces = [0.8, 0.8, 0.5, 0.5, 0.5];
    let mut mesh = Mesh {
        vertices: initial_surfaces
            .map(|surface| Vec3::new(0.0, 0.0, surface))
            .to_vec(),
        ..Mesh::default()
    };
    let adjacency = mesh.adjacency();
    let mut material = SurfaceMaterial::empty(initial_surfaces.len());
    let bedrock_rates = vec![0.0; initial_surfaces.len()];
    let control_areas = vec![1.0; initial_surfaces.len()];
    let mut nodes = initial_surfaces
        .into_iter()
        .enumerate()
        .map(|(vertex, surface)| RiverNode {
            vertex,
            flow: 10,
            surface,
            position: Vec3::ZERO,
        })
        .collect::<Vec<_>>();
    let waterfalls = [false, true, false, false, false];
    let mut budget = RiverSedimentBudget::default();

    level_confluence_reach(
        &mut test_river_terrain(
            &mut mesh,
            &adjacency,
            &mut material,
            &bedrock_rates,
            &control_areas,
        ),
        &mut nodes,
        &waterfalls,
        0.3,
        &mut budget,
    );

    let surfaces = nodes.iter().map(|node| node.surface).collect::<Vec<_>>();
    assert_eq!(surfaces[0].to_bits(), 0.8_f32.to_bits());
    assert_eq!(surfaces[1].to_bits(), 0.8_f32.to_bits());
    assert!((surfaces[2] - 0.3).abs() < 1.0e-6);
    assert!((surfaces[3] - 0.3).abs() < 1.0e-6);
    assert!((surfaces[4] - 0.3).abs() < 1.0e-6);
    assert_eq!(mesh.vertices[0].z.to_bits(), 0.8_f32.to_bits());
    assert_eq!(mesh.vertices[1].z.to_bits(), 0.8_f32.to_bits());
    assert!((mesh.vertices[2].z - 0.3).abs() < 1.0e-6);
    assert!((mesh.vertices[3].z - 0.3).abs() < 1.0e-6);
    assert!((mesh.vertices[4].z - 0.3).abs() < 1.0e-6);
    assert!((budget.bedrock_eroded - 0.6).abs() < 1.0e-6);
}

#[test]
fn lower_joined_river_reaches_previous_waterfall_and_keeps_flow_downhill() {
    let mut nodes = [0.9, 0.9, 0.7, 0.7, 0.5, 0.5]
        .into_iter()
        .enumerate()
        .map(|(vertex, surface)| RiverNode {
            vertex,
            flow: 10,
            surface,
            position: Vec3::new(vertex as f32, 0.0, surface),
        })
        .collect::<Vec<_>>();
    let mut waterfalls = vec![false, true, false, true, false, false];

    let reached_terminal =
        lower_profile_reach_through_confluence(&mut nodes, &mut waterfalls, 2, 0.6);

    let surfaces = nodes.iter().map(|node| node.surface).collect::<Vec<_>>();
    assert_eq!(surfaces, [0.9, 0.9, 0.6, 0.6, 0.5, 0.5]);
    assert!(!reached_terminal);
    assert!(waterfalls[1]);
    assert!(waterfalls[3]);
    assert!(surfaces.windows(2).all(|pair| pair[0] >= pair[1]));
}

#[test]
fn confluence_lowering_crosses_and_clears_consumed_waterfalls() {
    let mut nodes = [0.9, 0.9, 0.7, 0.7, 0.5]
        .into_iter()
        .enumerate()
        .map(|(vertex, surface)| RiverNode {
            vertex,
            flow: 10,
            surface,
            position: Vec3::new(vertex as f32, 0.0, surface),
        })
        .collect::<Vec<_>>();
    let mut waterfalls = vec![false, true, false, true, false];

    let reached_terminal =
        lower_profile_reach_through_confluence(&mut nodes, &mut waterfalls, 2, 0.4);

    let surfaces = nodes.iter().map(|node| node.surface).collect::<Vec<_>>();
    assert_eq!(surfaces, [0.9, 0.9, 0.4, 0.4, 0.4]);
    assert!(reached_terminal);
    assert!(waterfalls[1]);
    assert!(!waterfalls[3]);
    assert!(surfaces.windows(2).all(|pair| pair[0] >= pair[1]));
}

#[test]
fn confluence_lowering_propagates_through_a_join_chain() {
    let node = |vertex, surface| RiverNode {
        vertex,
        flow: 10,
        surface,
        position: Vec3::new(vertex as f32, 0.0, surface),
    };
    let mut network = RiverNetwork {
        rivers: vec![
            River {
                nodes: vec![node(0, 0.7), node(1, 0.7)],
                join: None,
            },
            River {
                nodes: vec![node(2, 0.9), node(3, 0.9)],
                join: Some(0),
            },
            River {
                nodes: vec![node(4, 0.5), node(5, 0.5)],
                join: Some(1),
            },
        ],
        join_vertices: vec![None, Some(1), Some(3)],
        waterfalls: vec![vec![false; 2]; 3],
        river_mesh_ends: vec![None; 3],
        max_flow: 10,
        max_height: 1.0,
        ocean: vec![false; 6],
        perimeter: vec![false; 6],
        cross_sections: vec![vec![RiverCrossSection::default(); 2]; 3],
    };

    network.reconcile_confluence_profile(2);

    assert!(
        network.rivers[0]
            .nodes
            .iter()
            .all(|node| node.surface.to_bits() == 0.5_f32.to_bits())
    );
    assert!(
        network.rivers[1]
            .nodes
            .iter()
            .all(|node| node.surface.to_bits() == 0.5_f32.to_bits())
    );
    assert!(
        network.rivers[2]
            .nodes
            .iter()
            .all(|node| node.surface.to_bits() == 0.5_f32.to_bits())
    );
}

#[test]
fn lower_sibling_tributary_sets_the_shared_confluence_level() {
    let node = |vertex, surface| RiverNode {
        vertex,
        flow: 10,
        surface,
        position: Vec3::new(vertex as f32, 0.0, surface),
    };
    let mut network = RiverNetwork {
        rivers: vec![
            River {
                nodes: vec![node(0, 0.7), node(1, 0.7)],
                join: None,
            },
            River {
                nodes: vec![node(2, 0.8), node(3, 0.8)],
                join: Some(0),
            },
            River {
                nodes: vec![node(4, 0.5), node(5, 0.5)],
                join: Some(0),
            },
        ],
        join_vertices: vec![None, Some(1), Some(1)],
        waterfalls: vec![vec![false; 2]; 3],
        river_mesh_ends: vec![None; 3],
        max_flow: 10,
        max_height: 1.0,
        ocean: vec![false; 6],
        perimeter: vec![false; 6],
        cross_sections: vec![vec![RiverCrossSection::default(); 2]; 3],
    };

    network.reconcile_confluence_profiles();

    assert!(
        network
            .rivers
            .iter()
            .flat_map(|river| &river.nodes)
            .all(|node| node.surface.to_bits() == 0.5_f32.to_bits())
    );
}

#[test]
fn nearby_river_pushes_a_waterfall_drop_upstream() {
    let mut mesh = Mesh {
        vertices: vec![
            Vec3::new(0.0, 0.0, 0.10),
            Vec3::new(1.0, 0.0, 0.10),
            Vec3::new(2.0, 0.0, 0.10),
            Vec3::new(3.0, 0.0, 0.05),
            Vec3::new(2.5, 0.02, 0.08),
        ],
        ..Mesh::default()
    };
    mesh.calculate_normals();
    let mut nodes: Vec<RiverNode> = (0..4)
        .map(|vertex| RiverNode {
            vertex,
            flow: 10,
            surface: mesh.vertices[vertex].z,
            position: mesh.vertices[vertex],
        })
        .collect();
    let rivers = vec![
        River {
            nodes: nodes.clone(),
            join: None,
        },
        River {
            nodes: vec![RiverNode {
                vertex: 4,
                flow: 10,
                surface: mesh.vertices[4].z,
                position: mesh.vertices[4],
            }],
            join: None,
        },
    ];
    let clearance = WaterfallClearanceIndex::new(&rivers, &mesh, 10, 0.01);
    let mut waterfalls = vec![false, false, true, false];
    let mut scratch = Vec::new();

    relocate_conflicting_waterfalls(
        &mesh,
        &mut nodes,
        &mut waterfalls,
        3,
        WaterfallRelocation {
            clearance: &clearance,
            site: None,
            river: 0,
        },
        &[],
        &mut scratch,
    );

    assert_eq!(waterfalls, [false, true, false, false]);
    assert!(!clearance.conflicts(0, &mesh, &nodes, 1));
    assert!((nodes[0].surface - nodes[3].surface - 0.05).abs() < 1.0e-6);
    assert!((nodes[2].surface - nodes[3].surface).abs() < 1.0e-6);
}

#[test]
fn intermediate_lod_keeps_the_original_drop_when_every_site_conflicts() {
    let mesh = Mesh {
        vertices: vec![
            Vec3::new(0.0, 0.0, 0.2),
            Vec3::new(1.0, 0.0, 0.2),
            Vec3::new(2.0, 0.0, 0.2),
            Vec3::new(3.0, 0.0, 0.1),
            Vec3::new(0.5, 0.01, 0.2),
            Vec3::new(1.5, 0.01, 0.2),
            Vec3::new(2.5, 0.01, 0.2),
        ],
        ..Mesh::default()
    };
    let mut nodes = (0..4)
        .map(|vertex| RiverNode {
            vertex,
            flow: 10,
            surface: mesh.vertices[vertex].z,
            position: mesh.vertices[vertex],
        })
        .collect::<Vec<_>>();
    let blockers = (4..7)
        .map(|vertex| RiverNode {
            vertex,
            flow: 10,
            surface: mesh.vertices[vertex].z,
            position: mesh.vertices[vertex],
        })
        .collect::<Vec<_>>();
    let rivers = vec![
        River {
            nodes: nodes.clone(),
            join: None,
        },
        River {
            nodes: blockers,
            join: None,
        },
    ];
    let clearance = WaterfallClearanceIndex::new(&rivers, &mesh, 10, 0.01);
    let mut waterfalls = vec![false, false, true, false];
    let mut scratch = Vec::new();

    assert!(relocate_conflicting_waterfalls(
        &mesh,
        &mut nodes,
        &mut waterfalls,
        3,
        WaterfallRelocation {
            clearance: &clearance,
            site: None,
            river: 0,
        },
        &[],
        &mut scratch,
    ));
    assert_eq!(waterfalls, [false, false, true, false]);
}

#[test]
fn side_bypass_pushes_a_waterfall_to_the_next_complete_cross_channel_cut() {
    let width = 7;
    let mut points = (0..5)
        .flat_map(|y| (0..width).map(move |x| Vec2::new(x as f32, y as f32)))
        .collect::<Vec<_>>();
    let bypass = points.len();
    points.push(Vec2::new(3.0, 4.0));
    let mut triangles = Vec::new();
    for y in 0..4 {
        for x in 0..width - 1 {
            let lower_left = (y * width + x) as u32;
            let lower_right = lower_left + 1;
            let upper_left = lower_left + width as u32;
            let upper_right = upper_left + 1;
            triangles.extend([
                lower_left,
                lower_right,
                upper_left,
                upper_left,
                lower_right,
                upper_right,
            ]);
        }
    }
    let vertex = |x: usize, y: usize| y * width + x;
    // This two-edge route skips the proposed x=3 face and represents the
    // short side flow seen in a failed, partly bypassed waterfall.
    triangles.extend([
        vertex(2, 2) as u32,
        bypass as u32,
        vertex(2, 4) as u32,
        bypass as u32,
        vertex(4, 2) as u32,
        vertex(4, 4) as u32,
    ]);
    let mesh = Mesh {
        vertices: points.iter().map(|point| point.extend(0.2)).collect(),
        triangles,
        ..Mesh::default()
    };
    let adjacency = mesh.adjacency();
    let mut nodes = (0..width)
        .map(|x| RiverNode {
            vertex: vertex(x, 2),
            flow: 10,
            surface: if x <= 3 { 0.2 } else { 0.1 },
            position: mesh.vertices[vertex(x, 2)],
        })
        .collect::<Vec<_>>();
    let rivers = vec![River {
        nodes: nodes.clone(),
        join: None,
    }];
    let clearance = WaterfallClearanceIndex::new(&rivers, &mesh, 10, 0.01);
    let mut coverage = vec![0_u8; mesh.vertices.len()];
    for y in 1..=3 {
        for x in 0..width {
            coverage[vertex(x, y)] = 1;
        }
    }
    coverage[bypass] = 1;
    let rejected = HashSet::new();
    let site = WaterfallSiteEnvironment {
        adjacency: &adjacency,
        coverage: &coverage,
        ocean: &vec![false; mesh.vertices.len()],
        perimeter: &vec![false; mesh.vertices.len()],
        rejected: &rejected,
    };
    let sections = vec![
        RiverCrossSection {
            target_half_width: 1.2,
            ..RiverCrossSection::default()
        };
        nodes.len()
    ];
    let mut waterfalls = vec![false; nodes.len()];
    waterfalls[3] = true;
    let mut scratch = Vec::new();

    assert!(relocate_conflicting_waterfalls(
        &mesh,
        &mut nodes,
        &mut waterfalls,
        width - 1,
        WaterfallRelocation {
            clearance: &clearance,
            site: Some(site),
            river: 0,
        },
        &sections,
        &mut scratch,
    ));
    assert!(waterfalls[2]);
    assert!(!waterfalls[3]);

    let blocked = vec![true; mesh.vertices.len()];
    let blocked_site = WaterfallSiteEnvironment {
        adjacency: &adjacency,
        coverage: &coverage,
        ocean: &blocked,
        perimeter: &blocked,
        rejected: &rejected,
    };
    assert!(!relocate_conflicting_waterfalls(
        &mesh,
        &mut nodes,
        &mut waterfalls,
        width - 1,
        WaterfallRelocation {
            clearance: &clearance,
            site: Some(blocked_site),
            river: 0,
        },
        &sections,
        &mut scratch,
    ));
    assert!(!waterfalls.iter().any(|&waterfall| waterfall));
}

#[test]
fn removing_an_invalid_parent_also_removes_its_dependent_tributaries() {
    let node = |vertex| RiverNode {
        vertex,
        flow: 1,
        surface: 1.0,
        position: Vec3::new(vertex as f32, 0.0, 1.0),
    };
    let mut network = RiverNetwork {
        rivers: vec![
            River {
                nodes: vec![node(0), node(1)],
                join: None,
            },
            River {
                nodes: vec![node(2), node(1)],
                join: Some(0),
            },
            River {
                nodes: vec![node(3), node(4)],
                join: None,
            },
        ],
        join_vertices: vec![None, Some(1), None],
        waterfalls: vec![vec![false; 2]; 3],
        river_mesh_ends: vec![None; 3],
        max_flow: 1,
        max_height: 1.0,
        ocean: vec![false; 5],
        perimeter: vec![false; 5],
        cross_sections: vec![vec![RiverCrossSection::default(); 2]; 3],
    };

    assert_eq!(network.remove_invalid_rivers(&[true, false, false]), 2);
    assert_eq!(network.rivers.len(), 1);
    assert_eq!(network.rivers[0].nodes[0].vertex, 3);
    assert_eq!(network.join_vertices, [None]);
    assert_eq!(network.waterfalls.len(), 1);
    assert_eq!(network.river_mesh_ends.len(), 1);
    assert_eq!(network.cross_sections.len(), 1);
}

#[test]
fn waterfall_terraces_are_carved_into_the_river_bed() {
    let points: Vec<Vec2> = (0..=2)
        .flat_map(|y| (0..=4).map(move |x| Vec2::new(x as f32 * 0.25, y as f32 * 0.5)))
        .collect();
    let mut mesh = Mesh::delaunay(&points);
    mesh.vertices.iter_mut().for_each(|vertex| vertex.z = 0.12);
    let channel: Vec<usize> = (0..4)
        .map(|x| {
            points
                .iter()
                .position(|point| *point == Vec2::new(x as f32 * 0.25, 0.5))
                .unwrap()
        })
        .collect();
    let surfaces = [0.06, 0.06, 0.03, 0.03];
    for (&vertex, &surface) in channel.iter().zip(&surfaces) {
        mesh.vertices[vertex].z = surface;
    }
    let nodes: Vec<RiverNode> = channel
        .iter()
        .zip(surfaces)
        .map(|(&vertex, surface)| RiverNode {
            vertex,
            flow: 10,
            surface,
            position: mesh.vertices[vertex],
        })
        .collect();
    let waterfalls = [false, true, false, false];
    let adjacency = mesh.adjacency();
    let parameters = RiverCarveParameters {
        downstream_surface: 0.0,
        terminal_ocean: false,
        max_height: 0.2,
        max_flow: 10,
        depth_multiplier: 1.0 / 10.0_f32.sqrt(),
        cross_sections: &[],
    };
    let mut targets = Vec::new();
    let mut material = SurfaceMaterial::empty(mesh.vertices.len());
    let bedrock_rates = vec![1.0; mesh.vertices.len()];
    let control_areas = projected_vertex_control_areas(&mesh);
    let mut budget = RiverSedimentBudget::default();

    {
        let mut terrain = test_river_terrain(
            &mut mesh,
            &adjacency,
            &mut material,
            &bedrock_rates,
            &control_areas,
        );
        carve_stepped_bed(
            &mut terrain,
            &nodes,
            &waterfalls,
            3,
            parameters,
            &mut targets,
            &mut budget,
        );
    }

    let beds: Vec<f32> = channel
        .iter()
        .map(|&vertex| mesh.vertices[vertex].z)
        .collect();
    assert!((beds[0] - beds[1]).abs() < f32::EPSILON);
    assert!((beds[2] - beds[3]).abs() < f32::EPSILON);
    assert!(beds[1] > beds[2]);
    assert!(budget.carried > 0.0);
}

#[test]
fn waterfall_lip_refinement_adds_detail_and_rounds_along_the_normal() {
    let points: Vec<Vec2> = (0..=4)
        .flat_map(|y| (0..=4).map(move |x| Vec2::new(x as f32 / 4.0, y as f32 / 4.0)))
        .collect();
    let mut water = Mesh::delaunay(&points);
    for vertex in &mut water.vertices {
        vertex.z = if vertex.y <= 0.5 { 0.1 } else { 0.0 };
    }
    water.uv = water
        .vertices
        .iter()
        .map(|vertex| vertex.truncate())
        .collect();
    let center = points
        .iter()
        .position(|point| *point == Vec2::new(0.5, 0.5))
        .unwrap();
    let original = water.vertices[center];
    let original_vertices = water.vertices.clone();
    let original_vertex_count = water.vertices.len();
    let mut lips = vec![false; water.vertices.len()];
    for (vertex, position) in water.vertices.iter().enumerate() {
        lips[vertex] = (position.y - 0.5).abs() < f32::EPSILON;
    }

    let minimum_heights = water.vertices.iter().map(|vertex| vertex.z - 0.1).collect();
    let rounded = round_waterfall_lips(water, lips, minimum_heights);

    assert!(rounded.vertices.len() > original_vertex_count);
    assert_ne!(rounded.vertices[center], original);
    assert_eq!(rounded.vertices[center].truncate(), original.truncate());
    assert!(rounded.vertices[center].z < original.z);
    assert!(rounded.vertices[center].z > 0.0);
    for (vertex, original) in original_vertices.iter().enumerate() {
        if original.y > 0.5 {
            assert_eq!(rounded.vertices[vertex], *original);
        }
    }
}

#[test]
fn plunge_pool_is_centred_on_the_pulled_down_waterfall_fan() {
    let points: Vec<Vec2> = (0..=8)
        .flat_map(|y| (0..=8).map(move |x| Vec2::new(x as f32 / 8.0, y as f32 / 8.0)))
        .collect();
    let mut terrain = Mesh::delaunay(&points);
    terrain
        .vertices
        .iter_mut()
        .for_each(|vertex| vertex.z = 0.42 - vertex.x * 0.4);
    let original = terrain.vertices.clone();
    let mut material = SurfaceMaterial::empty(terrain.vertices.len());
    material.depths_mut().fill(0.1);
    let mut coverage = vec![0; terrain.vertices.len()];
    let mut surfaces = vec![0.0; terrain.vertices.len()];
    let mut waterfall_lips = vec![false; terrain.vertices.len()];
    let lip = points
        .iter()
        .position(|point| *point == Vec2::new(0.25, 0.5))
        .unwrap();
    let support = points
        .iter()
        .position(|point| *point == Vec2::new(0.375, 0.5))
        .unwrap();
    let non_river_pool = points
        .iter()
        .position(|point| *point == Vec2::new(0.25, 0.625))
        .unwrap();
    coverage[lip] = 2;
    coverage[support] = 2;
    surfaces[support] = 0.35;
    let patch = WaterfallPatch {
        river: 0,
        segment: 0,
        upper_vertex: lip,
        upper_centre: Vec2::new(0.25, 0.5),
        direction: Vec2::X,
        across: Vec2::Y,
        upper_surface: 0.4,
        lower_surface: 0.2,
        lower_floor: 0.15,
        half_width: 0.2,
        support_run: 0.25,
        pool: Some(PlungePool {
            centre: Vec2::new(0.25, 0.5),
            downstream_radius: 0.2,
            lateral_radius: 0.15,
            depth: 0.05,
        }),
    };
    let neighbours = terrain.adjacency()[lip].to_vec();
    let notch_owners = recess_waterfall_notches(&mut terrain, &mut material, &[patch], &coverage);

    let constraints = pin_waterfalls_to_terrain(
        &terrain,
        &mut material,
        &[patch],
        &notch_owners,
        &mut coverage,
        &mut surfaces,
        &mut waterfall_lips,
    );

    let outside = points
        .iter()
        .position(|point| *point == Vec2::new(1.0, 1.0))
        .unwrap();
    assert!(constraints.pinned[lip]);
    assert!(!constraints.pinned[support]);
    assert!(!constraints.support[support]);
    assert!(constraints.water_unclamped[support]);
    assert_eq!(
        constraints.terrain_ceiling[support].to_bits(),
        terrain.vertices[support].z.to_bits()
    );
    assert!(!waterfall_lips[support]);
    assert!(constraints.support[lip]);
    assert!(waterfall_lips[lip]);
    assert!((terrain.vertices[lip].z - 0.1).abs() < f32::EPSILON);
    assert!(
        neighbours
            .iter()
            .all(|&vertex| terrain.vertices[vertex].z <= original[vertex].z)
    );
    assert!(terrain.vertices[support].z < original[support].z);
    assert_eq!(terrain.vertices[outside], original[outside]);
    assert!(terrain.vertices[non_river_pool].z < original[non_river_pool].z);
    assert!(constraints.patch[lip]);
    assert!(!constraints.patch[support]);
    assert!(neighbours.iter().all(|&vertex| {
        constraints.patch[vertex]
            == (notch_owners[vertex].is_some()
                && patch
                    .face_surface_at(terrain.vertices[vertex].truncate())
                    .is_some())
    }));
    assert_eq!(surfaces[lip].to_bits(), patch.lower_surface.to_bits());
    assert_eq!(surfaces[support].to_bits(), patch.lower_surface.to_bits());
    assert_eq!(surfaces[non_river_pool].to_bits(), 0.0_f32.to_bits());
    assert!(
        notch_owners
            .iter()
            .enumerate()
            .filter(|(_, owner)| owner.is_some())
            .all(|(vertex, _)| material.depths()[vertex] <= f32::EPSILON)
    );
    assert_eq!(coverage[outside], 0);
    assert_eq!(coverage[non_river_pool], 0);

    let fan_ceiling = constraints.terrain_ceiling[support];
    terrain.vertices[support].z = patch.upper_surface;
    assert_eq!(
        enforce_waterfall_downstream_ceiling(&mut terrain, &constraints.terrain_ceiling),
        1
    );
    assert_eq!(terrain.vertices[support].z.to_bits(), fan_ceiling.to_bits());
}

#[test]
fn waterfall_face_is_flat_across_the_channel_and_smooth_along_flow() {
    let patch = WaterfallPatch {
        river: 0,
        segment: 0,
        upper_vertex: 0,
        upper_centre: Vec2::ZERO,
        direction: Vec2::X,
        across: Vec2::Y,
        upper_surface: 0.4,
        lower_surface: 0.2,
        lower_floor: 0.15,
        half_width: 0.2,
        support_run: 0.25,
        pool: None,
    };
    let face_run = 2.0 * WATERFALL_TARGET_EDGE_LENGTH;
    let halfway = Vec2::new(-face_run * 0.5, 0.0);
    let across = halfway + Vec2::Y * 0.15;
    let upstream = Vec2::new(-face_run, 0.0);
    let behind = Vec2::new(-face_run - WATERFALL_TARGET_EDGE_LENGTH, 0.0);
    let downstream = Vec2::new(face_run, 0.0);

    let halfway_surface = patch.face_surface_at(halfway).unwrap();
    assert_eq!(
        halfway_surface.to_bits(),
        patch.face_surface_at(across).unwrap().to_bits()
    );
    assert!((halfway_surface - 0.3).abs() < 1.0e-6);
    assert_eq!(
        patch.face_surface_at(upstream).unwrap().to_bits(),
        patch.upper_surface.to_bits()
    );
    assert_eq!(
        patch.face_surface_at(behind).unwrap().to_bits(),
        patch.upper_surface.to_bits()
    );
    assert!(patch.face_surface_at(downstream).is_none());
}

#[test]
fn waterfall_face_reaches_both_banks_when_coverage_is_wider_than_nominal_width() {
    let spacing = WATERFALL_TARGET_EDGE_LENGTH;
    let points = (-2..=2)
        .flat_map(|y| (-2..=2).map(move |x| Vec2::new(x as f32 * spacing, y as f32 * spacing)))
        .collect::<Vec<_>>();
    let mut triangles = Vec::new();
    for y in 0..4 {
        for x in 0..4 {
            let lower_left = (y * 5 + x) as u32;
            let lower_right = lower_left + 1;
            let upper_left = lower_left + 5;
            let upper_right = upper_left + 1;
            triangles.extend([
                lower_left,
                lower_right,
                upper_left,
                upper_left,
                lower_right,
                upper_right,
            ]);
        }
    }
    let mut terrain = Mesh {
        vertices: points.iter().map(|point| point.extend(0.8)).collect(),
        triangles,
        uv: points,
        ..Mesh::default()
    };
    let upper_vertex = terrain
        .vertices
        .iter()
        .position(|position| position.truncate() == Vec2::ZERO)
        .unwrap();
    let patch = WaterfallPatch {
        river: 0,
        segment: 0,
        upper_vertex,
        upper_centre: Vec2::ZERO,
        direction: Vec2::X,
        across: Vec2::Y,
        upper_surface: 0.8,
        lower_surface: 0.2,
        lower_floor: 0.15,
        half_width: spacing * 0.5,
        support_run: spacing,
        pool: None,
    };
    let mut material = SurfaceMaterial::empty(terrain.vertices.len());
    let mut coverage = vec![2; terrain.vertices.len()];
    let mut surfaces = vec![0.8; terrain.vertices.len()];
    let owners = vec![Some(RiverOwnerKey { river: 0, node: 0 }); terrain.vertices.len()];
    let mut waterfall_lips = vec![false; terrain.vertices.len()];

    let notch_owners = recess_waterfall_notches(&mut terrain, &mut material, &[patch], &coverage);
    let mut constraints = pin_waterfalls_to_terrain(
        &terrain,
        &mut material,
        &[patch],
        &notch_owners,
        &mut coverage,
        &mut surfaces,
        &mut waterfall_lips,
    );
    constraints.support.fill(false);
    assert!(
        rebuild_final_waterfall_support_mask(
            &terrain,
            &[patch],
            &coverage,
            &owners,
            &mut constraints,
        ) > 0
    );

    for y in -2..=2 {
        let point = Vec2::new(0.0, y as f32 * spacing);
        let vertex = terrain
            .vertices
            .iter()
            .position(|position| position.truncate() == point)
            .unwrap();
        assert!(constraints.pinned[vertex], "unspanned bank row {y}");
        assert!(constraints.support[vertex], "unsupported bank row {y}");
        assert!((surfaces[vertex] - patch.lower_surface).abs() < f32::EPSILON);
    }
}

#[test]
fn waterfall_face_refinement_includes_one_ring_beyond_the_banks() {
    let adjacency = Mesh {
        vertices: vec![Vec3::ZERO; 7],
        triangles: vec![0, 1, 2, 2, 1, 3, 2, 3, 4, 4, 3, 5, 4, 5, 6],
        ..Mesh::default()
    }
    .adjacency();

    assert_eq!(
        expand_vertex_mask_to_banks(
            &adjacency,
            &[true, false, false, false, false, false, false],
            &[true; 7],
            &[true, true, true, true, true, true, false],
            &[false, false, false, true, true, false, false],
        ),
        vec![true, true, true, true, true, true, false]
    );
}

#[test]
fn completed_waterfall_rejects_a_bank_dragged_toward_the_lower_terrace() {
    let spacing = WATERFALL_TARGET_EDGE_LENGTH;
    let width = 7_usize;
    let height = 5_usize;
    let points = (0..height)
        .flat_map(|y| {
            (0..width)
                .map(move |x| Vec2::new((x as f32 - 4.0) * spacing, (y as f32 - 2.0) * spacing))
        })
        .collect::<Vec<_>>();
    let mut triangles = Vec::new();
    for y in 0..height - 1 {
        for x in 0..width - 1 {
            let lower_left = (y * width + x) as u32;
            let lower_right = lower_left + 1;
            let upper_left = lower_left + width as u32;
            let upper_right = upper_left + 1;
            triangles.extend([
                lower_left,
                lower_right,
                upper_left,
                upper_left,
                lower_right,
                upper_right,
            ]);
        }
    }
    let upper_surface = 0.8;
    let lower_surface = 0.2;
    let mut terrain = Mesh {
        vertices: points
            .iter()
            .map(|point| point.extend(upper_surface))
            .collect(),
        triangles,
        uv: points,
        ..Mesh::default()
    };
    let vertex_at = |x: usize, y: usize| y * width + x;
    let patch = WaterfallPatch {
        river: 0,
        segment: 0,
        upper_vertex: vertex_at(4, 2),
        upper_centre: Vec2::ZERO,
        direction: Vec2::X,
        across: Vec2::Y,
        upper_surface,
        lower_surface,
        lower_floor: lower_surface - 0.05,
        half_width: spacing,
        support_run: spacing,
        pool: None,
    };
    let mut coverage = terrain
        .vertices
        .iter()
        .map(|position| u8::from(position.y.abs() <= spacing * 1.01) * 2)
        .collect::<Vec<_>>();
    mark_river_boundary(
        &terrain.adjacency(),
        &terrain.perimeter_mask(),
        &mut coverage,
    );

    assert!(detect_failed_final_waterfalls(&terrain, &[patch], &coverage).is_empty());

    let collapsed_bank = vertex_at(2, 3);
    terrain.vertices[collapsed_bank].z = lower_surface + 0.05;
    assert_eq!(
        detect_failed_final_waterfalls(&terrain, &[patch], &coverage),
        vec![patch.upper_vertex]
    );
}

#[test]
fn final_waterfall_smoothing_freely_relaxes_face_banks_and_outer_ring() {
    let spacing = WATERFALL_TARGET_EDGE_LENGTH;
    let points = (0..7)
        .flat_map(|y| {
            (0..3).map(move |x| Vec2::new((x as f32 - 2.0) * spacing, (y as f32 - 3.0) * spacing))
        })
        .collect::<Vec<_>>();
    let mut triangles = Vec::new();
    for y in 0..6 {
        for x in 0..2 {
            let lower_left = (y * 3 + x) as u32;
            let lower_right = lower_left + 1;
            let upper_left = lower_left + 3;
            let upper_right = upper_left + 1;
            triangles.extend([
                lower_left,
                lower_right,
                upper_left,
                upper_left,
                lower_right,
                upper_right,
            ]);
        }
    }
    let mut terrain = Mesh {
        vertices: points.iter().map(|point| point.extend(1.0)).collect(),
        triangles,
        uv: points,
        ..Mesh::default()
    };
    let mut coverage = terrain
        .vertices
        .iter()
        .map(|position| u8::from(position.y.abs() <= spacing * 1.01) * 2)
        .collect::<Vec<_>>();
    mark_river_boundary(
        &terrain.adjacency(),
        &terrain.perimeter_mask(),
        &mut coverage,
    );
    let vertex_at = |point: Vec2| {
        terrain
            .vertices
            .iter()
            .position(|position| position.truncate().distance_squared(point) < 1.0e-12)
            .unwrap()
    };
    let core = vertex_at(Vec2::new(-spacing, 0.0));
    let bank = vertex_at(Vec2::new(-spacing, spacing));
    let apron = vertex_at(Vec2::new(-spacing, spacing * 2.0));
    let outside = vertex_at(Vec2::new(-spacing, spacing * 3.0));
    let patch = WaterfallPatch {
        river: 0,
        segment: 0,
        upper_vertex: vertex_at(Vec2::ZERO),
        upper_centre: Vec2::ZERO,
        direction: Vec2::X,
        across: Vec2::Y,
        upper_surface: 0.8,
        lower_surface: 0.2,
        lower_floor: 0.15,
        half_width: spacing * 1.1,
        support_run: spacing,
        pool: None,
    };
    let mut material = SurfaceMaterial::empty(terrain.vertices.len());
    recess_waterfall_notches(&mut terrain, &mut material, &[patch], &coverage);
    for (position, &remaining) in terrain.vertices.iter_mut().zip(&coverage) {
        if remaining == 0 {
            position.z = 1.0;
        }
    }
    terrain.vertices[core].z -= 0.1;
    let mut surfaces = terrain
        .vertices
        .iter()
        .map(|position| position.z)
        .collect::<Vec<_>>();
    let owners = coverage
        .iter()
        .map(|&remaining| (remaining != 0).then_some(RiverOwnerKey { river: 0, node: 0 }))
        .collect::<Vec<_>>();
    let core_height = terrain.vertices[core].z;
    let bank_height = terrain.vertices[bank].z;
    let bank_surface = surfaces[bank];

    assert!(!terrain.perimeter_mask()[bank]);
    assert_ne!(coverage[bank], 0);
    assert!(patch.contains_face_point(terrain.vertices[bank].truncate()));
    let adjacency = terrain.adjacency();
    assert!(!adjacency[bank].is_empty());
    let perimeter = terrain.perimeter_mask();
    let face = terrain
        .vertices
        .iter()
        .map(|position| patch.contains_face_point(position.truncate()))
        .collect::<Vec<_>>();
    let smoothing_band = terrain
        .vertices
        .iter()
        .map(|position| patch.contains_face_smoothing_band(position.truncate()))
        .collect::<Vec<_>>();
    let eligible = coverage
        .iter()
        .zip(&smoothing_band)
        .map(|(&remaining, &inside_band)| remaining != 0 && inside_band)
        .collect::<Vec<_>>();
    let banks = coverage
        .iter()
        .enumerate()
        .map(|(vertex, &remaining)| {
            remaining != 0
                && (perimeter[vertex]
                    || adjacency[vertex]
                        .iter()
                        .any(|&neighbour| coverage[neighbour] == 0))
        })
        .collect::<Vec<_>>();
    let broad_patch = WaterfallPatch {
        half_width: spacing * 4.0,
        ..patch
    };
    let bank_apron =
        waterfall_face_bank_apron_for_patch(&terrain, broad_patch, &coverage, &adjacency, &banks);
    assert!(bank_apron[bank]);
    assert!(bank_apron[apron]);
    assert!(!bank_apron[outside]);
    let selected =
        expand_vertex_mask_to_banks(&adjacency, &face, &eligible, &smoothing_band, &banks);
    assert!(selected[bank]);
    let bank_average = adjacency[bank]
        .iter()
        .fold(terrain.vertices[bank], |total, &neighbour| {
            total + terrain.vertices[neighbour]
        })
        / (adjacency[bank].len() + 1) as f32;
    assert!(bank_average.distance_squared(terrain.vertices[bank]) > f32::EPSILON);

    let vertex_count = terrain.vertices.len();
    let triangles = terrain.triangles.clone();
    let moved = smooth_final_waterfall_patches(&mut terrain, &mut surfaces, &[patch], &coverage);
    let first_pass_positions = terrain.vertices.clone();
    let moved_again =
        smooth_final_waterfall_patches(&mut terrain, &mut surfaces, &[patch], &coverage);

    assert!(moved > 0);
    assert!(moved_again > 0);
    assert_ne!(terrain.vertices, first_pass_positions);
    assert_eq!(terrain.vertices.len(), vertex_count);
    assert_eq!(terrain.triangles, triangles);
    assert_ne!(terrain.vertices[core].z.to_bits(), core_height.to_bits());
    assert!(terrain.vertices[bank].z > bank_height);
    assert!(terrain.vertices[bank].z < terrain.vertices[apron].z);
    assert!(terrain.vertices[apron].z < 1.0);
    assert_ne!(surfaces[bank].to_bits(), bank_surface.to_bits());
    for vertex in [core, bank] {
        let expected = terrain.vertices[vertex].z + WATERFALL_WATER_CLEARANCE;
        assert!((surfaces[vertex] - expected).abs() < f32::EPSILON);
    }
    assert_eq!(terrain.vertices[outside].z.to_bits(), 1.0_f32.to_bits());

    let smoothed_positions = terrain.vertices.clone();
    let mut constraints = WaterfallTerrainConstraints {
        patch: vec![false; terrain.vertices.len()],
        pinned: vec![false; terrain.vertices.len()],
        support: vec![false; terrain.vertices.len()],
        water_unclamped: vec![false; terrain.vertices.len()],
        terrain_ceiling: vec![f32::INFINITY; terrain.vertices.len()],
    };
    rebuild_final_waterfall_support_mask(&terrain, &[patch], &coverage, &owners, &mut constraints);

    assert_eq!(terrain.vertices, smoothed_positions);
    assert!(constraints.patch[core]);
    assert!(constraints.patch[bank]);
    assert!(constraints.support[core]);
    assert!(constraints.support[bank]);
    assert!(!constraints.support[apron]);

    let river_uv = vec![Vec2::ZERO; terrain.vertices.len()];
    let water = duplicate_river_topology(&terrain, &coverage, &surfaces, &river_uv, &constraints);
    for vertex in [core, bank] {
        let water_vertex = water
            .vertices
            .iter()
            .find(|position| {
                position
                    .truncate()
                    .distance_squared(terrain.vertices[vertex].truncate())
                    < 1.0e-12
            })
            .unwrap();
        let expected = terrain.vertices[vertex].z + WATERFALL_WATER_CLEARANCE;
        assert!((water_vertex.z - expected).abs() < f32::EPSILON);
    }
    assert_eq!(terrain.vertices[outside].z.to_bits(), 1.0_f32.to_bits());
}

#[test]
fn final_waterfall_edges_switch_relationship_at_the_lip_and_foot_planes() {
    let spacing = WATERFALL_TARGET_EDGE_LENGTH;
    let points = (-3..=3)
        .flat_map(|y| (-4..=4).map(move |x| Vec2::new(x as f32 * spacing, y as f32 * spacing)))
        .collect::<Vec<_>>();
    let mut triangles = Vec::new();
    for y in 0..6 {
        for x in 0..8 {
            let lower_left = (y * 9 + x) as u32;
            let lower_right = lower_left + 1;
            let upper_left = lower_left + 9;
            let upper_right = upper_left + 1;
            triangles.extend([
                lower_left,
                lower_right,
                upper_left,
                upper_left,
                lower_right,
                upper_right,
            ]);
        }
    }
    let mut terrain = Mesh {
        vertices: points.iter().map(|point| point.extend(0.1)).collect(),
        triangles,
        uv: points,
        ..Mesh::default()
    };
    let mut coverage = terrain
        .vertices
        .iter()
        .map(|position| u8::from(position.y.abs() <= spacing * 1.01) * 2)
        .collect::<Vec<_>>();
    mark_river_boundary(
        &terrain.adjacency(),
        &terrain.perimeter_mask(),
        &mut coverage,
    );
    let vertex_at = |x: i32, y: i32| {
        let target = Vec2::new(x as f32 * spacing, y as f32 * spacing);
        terrain
            .vertices
            .iter()
            .position(|position| position.truncate().distance_squared(target) < 1.0e-12)
            .unwrap()
    };
    let patch = WaterfallPatch {
        river: 0,
        segment: 2,
        upper_vertex: vertex_at(0, 0),
        upper_centre: Vec2::ZERO,
        direction: Vec2::X,
        across: Vec2::Y,
        upper_surface: 0.8,
        lower_surface: 0.2,
        lower_floor: 0.15,
        half_width: spacing * 1.1,
        support_run: spacing,
        pool: None,
    };
    let upstream = [vertex_at(-3, 0), vertex_at(-3, 1), vertex_at(-3, 2)];
    let downstream = [vertex_at(1, 0), vertex_at(1, 1), vertex_at(1, 2)];
    let before_first = [vertex_at(-4, 0), vertex_at(-4, 1), vertex_at(-4, 2)];
    let after_last = [vertex_at(3, 0), vertex_at(3, 1), vertex_at(3, 2)];
    let lip = [vertex_at(-2, 0), vertex_at(-2, 1), vertex_at(-2, 2)];
    let face = [vertex_at(-1, 0), vertex_at(-1, 1), vertex_at(-1, 2)];
    let foot = [vertex_at(0, 0), vertex_at(0, 1), vertex_at(0, 2)];
    let outside = [vertex_at(-3, 3), vertex_at(1, 3)];
    let mut surfaces = vec![0.5; terrain.vertices.len()];
    for &vertex in &upstream {
        surfaces[vertex] = patch.upper_surface;
    }
    for &vertex in &downstream {
        surfaces[vertex] = patch.lower_surface;
    }
    for &vertex in &before_first {
        surfaces[vertex] = 0.1;
    }
    for &vertex in &after_last {
        surfaces[vertex] = 0.9;
    }
    surfaces[lip[0]] = patch.upper_surface;
    surfaces[foot[0]] = patch.lower_surface;
    let mut owners = coverage
        .iter()
        .map(|&remaining| (remaining != 0).then_some(RiverOwnerKey { river: 0, node: 2 }))
        .collect::<Vec<_>>();
    for &vertex in &downstream[..2] {
        owners[vertex] = Some(RiverOwnerKey { river: 0, node: 1 });
    }
    owners[before_first[1]] = Some(RiverOwnerKey { river: 0, node: 3 });
    terrain.vertices[upstream[0]].z = 0.95;
    terrain.vertices[downstream[0]].z = 0.9;
    terrain.vertices[after_last[0]].z = 0.9;
    let mut constraints = WaterfallTerrainConstraints {
        patch: vec![false; terrain.vertices.len()],
        pinned: vec![false; terrain.vertices.len()],
        support: vec![false; terrain.vertices.len()],
        water_unclamped: vec![false; terrain.vertices.len()],
        terrain_ceiling: vec![f32::INFINITY; terrain.vertices.len()],
    };

    assert!(
        enforce_final_waterfall_edge_relationships(
            &mut terrain,
            &mut surfaces,
            &[patch],
            &coverage,
            &owners,
            &mut constraints,
        ) > 0
    );

    assert_eq!(
        terrain.vertices[upstream[0]].z.to_bits(),
        0.95_f32.to_bits()
    );
    let first_step_blend = patch.edge_normal_blend(terrain.vertices[upstream[1]].truncate());
    assert!((first_step_blend - 0.5).abs() < f32::EPSILON);
    let expected_upstream_bank = (patch.upper_surface - 0.1).mul_add(first_step_blend, 0.1);
    assert!((terrain.vertices[upstream[1]].z - expected_upstream_bank).abs() < f32::EPSILON);
    assert_eq!(terrain.vertices[upstream[2]].z.to_bits(), 0.1_f32.to_bits());
    assert!((terrain.vertices[downstream[0]].z - patch.lower_surface).abs() <= f32::EPSILON);
    let downstream_blend = patch.edge_normal_blend(terrain.vertices[downstream[1]].truncate());
    assert!((downstream_blend - 1.0).abs() < f32::EPSILON);
    let expected_downstream_bank = (patch.lower_surface - 0.1).mul_add(downstream_blend, 0.1);
    assert!((terrain.vertices[downstream[1]].z - expected_downstream_bank).abs() < f32::EPSILON);
    assert_eq!(
        terrain.vertices[downstream[2]].z.to_bits(),
        0.1_f32.to_bits()
    );
    let outer_plane_blend = patch.edge_normal_blend(terrain.vertices[before_first[0]].truncate());
    assert!((outer_plane_blend - 1.0).abs() < f32::EPSILON);
    let expected_before_first_terrain = (patch.upper_surface - 0.1).mul_add(outer_plane_blend, 0.1);
    let expected_before_first_surface = (patch.upper_surface
        - (expected_before_first_terrain + WATERFALL_WATER_CLEARANCE))
        .mul_add(
            outer_plane_blend,
            expected_before_first_terrain + WATERFALL_WATER_CLEARANCE,
        );
    assert!((surfaces[before_first[0]] - expected_before_first_surface).abs() < f32::EPSILON);
    assert!(
        (terrain.vertices[before_first[1]].z - expected_before_first_terrain).abs() < f32::EPSILON
    );
    assert_eq!(
        surfaces[after_last[0]].to_bits(),
        patch.lower_surface.to_bits()
    );
    assert!((terrain.vertices[after_last[0]].z - patch.lower_surface).abs() <= f32::EPSILON);
    assert!((terrain.vertices[after_last[1]].z - patch.lower_surface).abs() <= f32::EPSILON);
    for vertices in [&lip, &face, &foot] {
        assert_eq!(terrain.vertices[vertices[0]].z.to_bits(), 0.1_f32.to_bits());
        assert_eq!(terrain.vertices[vertices[1]].z.to_bits(), 0.1_f32.to_bits());
        assert_eq!(terrain.vertices[vertices[2]].z.to_bits(), 0.1_f32.to_bits());
        assert!((surfaces[vertices[1]] - (0.1 + WATERFALL_WATER_CLEARANCE)).abs() < f32::EPSILON);
    }
    assert!(
        outside
            .iter()
            .all(|&vertex| terrain.vertices[vertex].z.to_bits() == 0.1_f32.to_bits())
    );

    rebuild_final_waterfall_support_mask(&terrain, &[patch], &coverage, &owners, &mut constraints);
    assert!(constraints.water_unclamped[before_first[0]]);
    assert!(constraints.water_unclamped[after_last[0]]);
    let water = duplicate_river_topology(
        &terrain,
        &coverage,
        &surfaces,
        &vec![Vec2::ZERO; terrain.vertices.len()],
        &constraints,
    );
    let downstream_water = water
        .vertices
        .iter()
        .find(|position| {
            position
                .truncate()
                .distance_squared(terrain.vertices[after_last[0]].truncate())
                < 1.0e-12
        })
        .unwrap();
    let upstream_water = water
        .vertices
        .iter()
        .find(|position| {
            position
                .truncate()
                .distance_squared(terrain.vertices[upstream[0]].truncate())
                < 1.0e-12
        })
        .unwrap();
    let expected_upstream_surface = (patch.upper_surface
        - (terrain.vertices[upstream[0]].z + WATERFALL_WATER_CLEARANCE))
        .mul_add(
            first_step_blend,
            terrain.vertices[upstream[0]].z + WATERFALL_WATER_CLEARANCE,
        );
    assert!(
        (upstream_water.z - (expected_upstream_surface + RIVER_SURFACE_OFFSET)).abs()
            < f32::EPSILON
    );
    assert!(upstream_water.z < terrain.vertices[upstream[0]].z);
    assert!(
        (downstream_water.z - (patch.lower_surface + RIVER_SURFACE_OFFSET)).abs() < f32::EPSILON,
        "downstream water {}, expected {}, hydraulic surface {}",
        downstream_water.z,
        patch.lower_surface + RIVER_SURFACE_OFFSET,
        surfaces[after_last[0]],
    );
    assert!(downstream_water.z > terrain.vertices[after_last[0]].z);

    let mut smoothed_terrain = terrain.clone();
    let mut smoothed_surfaces = surfaces.clone();
    let midpoint_bank_before = smoothed_terrain.vertices[upstream[1]];
    let midpoint_apron_before = smoothed_terrain.vertices[upstream[2]];
    let blend_end_before = before_first.map(|vertex| smoothed_terrain.vertices[vertex]);
    let interior_surface_before = smoothed_surfaces[upstream[0]];
    let apron_surface_before = smoothed_surfaces[upstream[2]];
    let triangles_before = smoothed_terrain.triangles.clone();
    assert!(
        smooth_pinned_waterfall_terrain(
            &mut smoothed_terrain,
            &mut smoothed_surfaces,
            &[patch],
            &coverage,
            &owners,
        ) > 0
    );
    assert_ne!(smoothed_terrain.vertices[upstream[1]], midpoint_bank_before);
    assert_ne!(
        smoothed_terrain.vertices[upstream[2]],
        midpoint_apron_before
    );
    assert_eq!(
        before_first.map(|vertex| smoothed_terrain.vertices[vertex]),
        blend_end_before
    );
    assert_eq!(
        smoothed_surfaces[upstream[0]].to_bits(),
        interior_surface_before.to_bits()
    );
    assert_eq!(
        smoothed_surfaces[upstream[2]].to_bits(),
        apron_surface_before.to_bits()
    );
    assert_eq!(
        smoothed_surfaces[upstream[1]].to_bits(),
        (smoothed_terrain.vertices[upstream[1]].z + WATERFALL_WATER_CLEARANCE).to_bits()
    );
    assert_eq!(smoothed_terrain.triangles, triangles_before);
}

#[test]
fn river_reach_is_bounded_by_the_next_lip_and_previous_waterfall_bottom() {
    let terrain = Mesh {
        vertices: [-3.0, 1.0, 2.0, 11.0, 2.0]
            .into_iter()
            .map(|x| Vec3::new(x, 0.0, 0.0))
            .collect(),
        ..Mesh::default()
    };
    let first = WaterfallPatch {
        river: 0,
        segment: 1,
        upper_vertex: 0,
        upper_centre: Vec2::ZERO,
        direction: Vec2::X,
        across: Vec2::Y,
        upper_surface: 0.8,
        lower_surface: 0.6,
        lower_floor: 0.55,
        half_width: WATERFALL_TARGET_EDGE_LENGTH,
        support_run: WATERFALL_TARGET_EDGE_LENGTH,
        pool: None,
    };
    let second = WaterfallPatch {
        segment: 4,
        upper_centre: Vec2::splat(10.0),
        upper_surface: 0.4,
        lower_surface: 0.2,
        lower_floor: 0.15,
        ..first
    };
    let coverage = vec![2; terrain.vertices.len()];
    let owners = [
        Some(RiverOwnerKey { river: 0, node: 0 }),
        Some(RiverOwnerKey { river: 0, node: 2 }),
        Some(RiverOwnerKey { river: 0, node: 3 }),
        Some(RiverOwnerKey { river: 0, node: 5 }),
        Some(RiverOwnerKey { river: 1, node: 2 }),
    ];
    let levels = [
        WaterfallChannelLevels {
            lip: first.upper_surface,
            bottom: first.lower_surface,
        },
        WaterfallChannelLevels {
            lip: second.upper_surface,
            bottom: second.lower_surface,
        },
    ];
    let mut surfaces = vec![0.1, 0.9, 0.1, 0.5, 0.7];

    let (adjusted, reaches) = enforce_waterfall_reach_surface_levels(
        &mut surfaces,
        WaterfallReachEnvironment {
            terrain: &terrain,
            patches: &[first, second],
            levels: &levels,
            coverage: &coverage,
            owners: &owners,
        },
    );

    assert_eq!(adjusted, 4);
    assert_eq!(surfaces, vec![0.8, 0.6, 0.4, 0.2, 0.7]);
    assert_eq!(reaches.constrained, vec![true, true, true, true, false]);
    assert_eq!(
        reaches.downstream_ceiling,
        vec![f32::INFINITY, 0.6, 0.6, 0.2, f32::INFINITY]
    );
}

#[test]
fn final_waterfall_face_refinement_is_unconditional_and_reprojects_the_face() {
    let spacing = WATERFALL_TARGET_EDGE_LENGTH * 0.25;
    let points = [
        Vec2::new(-spacing, -spacing),
        Vec2::new(0.0, -spacing),
        Vec2::new(-spacing, spacing),
        Vec2::new(0.0, spacing),
    ];
    let mut terrain = Mesh {
        vertices: points.iter().map(|point| point.extend(0.5)).collect(),
        triangles: vec![0, 1, 2, 2, 1, 3],
        uv: points.to_vec(),
        ..Mesh::default()
    };
    let patch = WaterfallPatch {
        river: 0,
        segment: 0,
        upper_vertex: 1,
        upper_centre: Vec2::ZERO,
        direction: Vec2::X,
        across: Vec2::Y,
        upper_surface: 0.4,
        lower_surface: 0.2,
        lower_floor: 0.15,
        half_width: WATERFALL_TARGET_EDGE_LENGTH,
        support_run: WATERFALL_TARGET_EDGE_LENGTH,
        pool: None,
    };
    let original_vertices = terrain.vertices.len();
    let mut material = SurfaceMaterial::empty(original_vertices);
    let mut buffers = RiverMeshBuffers {
        coverage: vec![2; original_vertices],
        surfaces: vec![0.3; original_vertices],
        river_uv: vec![Vec2::ZERO; original_vertices],
        owners: vec![Some(RiverOwnerKey { river: 0, node: 0 }); original_vertices],
        waterfall_lips: vec![false; original_vertices],
        target_half_widths: vec![patch.half_width; original_vertices],
        target_depths: vec![0.05; original_vertices],
    };

    let added = buffers.tessellate_final_waterfall_faces(&mut terrain, &mut material, &[patch]);
    recess_waterfall_notches(&mut terrain, &mut material, &[patch], &buffers.coverage);

    assert!(added > 0);
    assert!(terrain.vertices.len() > original_vertices);
    for position in &terrain.vertices {
        if !patch.contains_face_point(position.truncate()) {
            continue;
        }
        let expected =
            patch.face_surface_at(position.truncate()).unwrap() - WATERFALL_WATER_CLEARANCE;
        assert!((position.z - expected).abs() < 1.0e-6);
    }
}

#[test]
fn final_downstream_cleanup_only_squishes_convex_spikes() {
    let points = (0..=4)
        .flat_map(|y| (0..=4).map(move |x| Vec2::new(x as f32 / 4.0, y as f32 / 4.0)))
        .collect::<Vec<_>>();
    let mut terrain = Mesh::delaunay(&points);
    terrain
        .vertices
        .iter_mut()
        .for_each(|position| position.z = 0.1);
    let centre = terrain
        .vertices
        .iter()
        .position(|position| position.truncate() == Vec2::splat(0.5))
        .unwrap();
    let outside = terrain
        .vertices
        .iter()
        .position(|position| position.truncate() == Vec2::ZERO)
        .unwrap();
    terrain.vertices[centre].z = 0.9;
    let mut surfaces = vec![0.2; terrain.vertices.len()];
    surfaces[centre] = 0.8;
    let downstream = vec![true; terrain.vertices.len()];

    let adjusted = squish_waterfall_downstream_spikes(&mut terrain, &mut surfaces, &downstream);

    assert_eq!(adjusted, 1);
    assert!(terrain.vertices[centre].z < 0.2);
    assert!(surfaces[centre] < 0.3);
    assert_eq!(terrain.vertices[outside].z.to_bits(), 0.1_f32.to_bits());
    assert_eq!(surfaces[outside].to_bits(), 0.2_f32.to_bits());
}

#[test]
fn final_refined_surface_relaxation_follows_a_continuous_cross_channel_profile() {
    let points = (0..=6)
        .flat_map(|y| (0..=6).map(move |x| Vec2::new(x as f32 / 6.0, y as f32 / 6.0)))
        .collect::<Vec<_>>();
    let mut terrain = Mesh::delaunay(&points);
    terrain
        .vertices
        .iter_mut()
        .for_each(|vertex| vertex.z = 1.0);
    let centre = terrain
        .vertices
        .iter()
        .position(|vertex| vertex.truncate() == Vec2::splat(0.5))
        .unwrap();
    let adjacency = terrain.adjacency();
    let pinned_neighbour = adjacency[centre][0];
    let outer = terrain
        .vertices
        .iter()
        .position(|vertex| vertex.truncate() == Vec2::ZERO)
        .unwrap();
    let coverage = vec![2; terrain.vertices.len()];
    let mut surfaces = vec![1.0; terrain.vertices.len()];
    let river_uv = terrain
        .vertices
        .iter()
        .map(|vertex| Vec2::new(vertex.x - 0.5, vertex.y))
        .collect::<Vec<_>>();
    let target_half_widths = vec![0.5; terrain.vertices.len()];
    let target_depths = vec![0.25; terrain.vertices.len()];
    let mut pinned = vec![false; terrain.vertices.len()];
    pinned[pinned_neighbour] = true;
    let waterfall = WaterfallTerrainConstraints {
        patch: vec![false; terrain.vertices.len()],
        pinned,
        support: vec![false; terrain.vertices.len()],
        water_unclamped: vec![false; terrain.vertices.len()],
        terrain_ceiling: vec![f32::INFINITY; terrain.vertices.len()],
    };

    let moved = relax_refined_river_surface(
        &mut terrain,
        &coverage,
        &mut surfaces,
        &river_uv,
        &target_half_widths,
        &target_depths,
        &waterfall,
    );

    assert!(moved > 0);
    assert!(terrain.vertices[centre].z < 0.8);
    assert!(terrain.vertices[centre].z >= 0.75);
    assert_eq!(
        terrain.vertices[pinned_neighbour].z.to_bits(),
        1.0_f32.to_bits()
    );
    assert_eq!(terrain.vertices[outer].z.to_bits(), 1.0_f32.to_bits());
}

#[test]
fn final_relaxation_does_not_collapse_waterfall_support_to_lower_water() {
    let points = (0..=6)
        .flat_map(|y| (0..=6).map(move |x| Vec2::new(x as f32 / 6.0, y as f32 / 6.0)))
        .collect::<Vec<_>>();
    let mut terrain = Mesh::delaunay(&points);
    terrain
        .vertices
        .iter_mut()
        .for_each(|vertex| vertex.z = 0.3);
    let centre = terrain
        .vertices
        .iter()
        .position(|vertex| vertex.truncate() == Vec2::splat(0.5))
        .unwrap();
    terrain.vertices[centre].z = 0.35;

    let coverage = vec![2; terrain.vertices.len()];
    let mut surfaces = vec![0.2; terrain.vertices.len()];
    let river_uv = vec![Vec2::ZERO; terrain.vertices.len()];
    let target_half_widths = vec![0.5; terrain.vertices.len()];
    let target_depths = vec![0.0; terrain.vertices.len()];
    let mut waterfall = WaterfallTerrainConstraints {
        patch: vec![false; terrain.vertices.len()],
        pinned: vec![false; terrain.vertices.len()],
        support: vec![false; terrain.vertices.len()],
        water_unclamped: vec![false; terrain.vertices.len()],
        terrain_ceiling: vec![f32::INFINITY; terrain.vertices.len()],
    };
    waterfall.patch[centre] = true;
    waterfall.support[centre] = true;
    let network = RiverNetwork {
        rivers: Vec::new(),
        join_vertices: Vec::new(),
        waterfalls: Vec::new(),
        river_mesh_ends: Vec::new(),
        max_flow: 1,
        max_height: 1.0,
        ocean: vec![false; terrain.vertices.len()],
        perimeter: terrain.perimeter_mask(),
        cross_sections: Vec::new(),
    };
    let waterfall_lips = vec![false; terrain.vertices.len()];

    ensure_clear_river_channel(
        &network,
        &mut terrain,
        &coverage,
        &surfaces,
        &waterfall_lips,
        &waterfall.pinned,
        &waterfall.patch,
    );
    assert_eq!(terrain.vertices[centre].z.to_bits(), 0.35_f32.to_bits());

    relax_refined_river_surface(
        &mut terrain,
        &coverage,
        &mut surfaces,
        &river_uv,
        &target_half_widths,
        &target_depths,
        &waterfall,
    );

    assert!(terrain.vertices[centre].z > surfaces[centre]);
    assert!(terrain.vertices[centre].z > 0.3);
}

#[test]
fn waterfall_face_is_pinned_while_downstream_water_allows_terrain_to_pierce() {
    let terrain = Mesh {
        vertices: vec![
            Vec3::new(-0.1, -0.1, 0.3),
            Vec3::new(-0.1, 0.1, 0.3),
            Vec3::new(0.1, -0.1, 0.3),
            Vec3::new(0.1, 0.1, 0.3),
            Vec3::new(0.2, -0.1, 0.3),
            Vec3::new(0.2, 0.1, 0.3),
        ],
        triangles: vec![0, 2, 1, 1, 2, 3, 2, 4, 3, 3, 4, 5],
        ..Mesh::default()
    };
    let mut coverage = vec![2; terrain.vertices.len()];
    coverage[2] |= RIVER_BOUNDARY;
    coverage[3] |= RIVER_BOUNDARY;
    let pinned = 0.3 + WATERFALL_WATER_CLEARANCE;
    let surfaces = vec![0.4, 0.4, 0.9, 0.9, 0.2, 0.2];
    let river_uv = vec![Vec2::ZERO; terrain.vertices.len()];
    let waterfall = WaterfallTerrainConstraints {
        patch: vec![true; terrain.vertices.len()],
        pinned: vec![false; terrain.vertices.len()],
        support: vec![false, false, true, true, false, false],
        water_unclamped: vec![false, false, false, false, true, true],
        terrain_ceiling: vec![f32::INFINITY; terrain.vertices.len()],
    };

    let water = duplicate_river_topology(&terrain, &coverage, &surfaces, &river_uv, &waterfall);

    assert_eq!(water.triangles, terrain.triangles);
    assert_eq!(water.vertices[2].z.to_bits(), pinned.to_bits());
    assert!(water.vertices[2].z > terrain.vertices[2].z);
    assert!(water.vertices[4].z < terrain.vertices[4].z);
    assert_eq!(
        water.vertices[4].z.to_bits(),
        (surfaces[4] + RIVER_SURFACE_OFFSET).to_bits()
    );
}

#[cfg(any())]
#[test]
fn bank_grading_spreads_the_cut_beyond_the_first_ring() {
    let points: Vec<Vec2> = (0..=6)
        .flat_map(|y| (0..=6).map(move |x| Vec2::new(x as f32 / 6.0, y as f32 / 6.0)))
        .collect();
    let mut mesh = Mesh::delaunay(&points);
    mesh.vertices.iter_mut().for_each(|vertex| vertex.z = 0.18);
    let center = points
        .iter()
        .position(|point| *point == Vec2::new(0.5, 0.5))
        .unwrap();
    mesh.vertices[center].z = 0.02;
    let adjacency = mesh.adjacency();
    let first_ring = adjacency[center].to_vec();
    let node = RiverNode {
        vertex: center,
        flow: 10,
        surface: 0.03,
        position: mesh.vertices[center],
    };
    let mut scratch = BankScratch::new(mesh.vertices.len());
    let mut material = SurfaceMaterial::empty(mesh.vertices.len());
    let bedrock_rates = vec![1.0; mesh.vertices.len()];
    let control_areas = projected_vertex_control_areas(&mesh);
    let mut budget = RiverSedimentBudget::default();
    let base_width = average_edge_length(&mesh, &adjacency);
    let shelf_width = river_half_width(node.flow, 10, base_width);
    let ocean = vec![false; mesh.vertices.len()];

    {
        let mut terrain = test_river_terrain(
            &mut mesh,
            &adjacency,
            &mut material,
            &bedrock_rates,
            &control_areas,
        );
        carve_banks(
            &mut terrain,
            &[node],
            &[true],
            RiverCarveParameters {
                downstream_surface: 0.0,
                terminal_ocean: false,
                max_height: 0.2,
                max_flow: 10,
                depth_multiplier: 1.0,
                base_width,
                form_waterfall_shelves: true,
                cross_sections: &[],
            },
            &mut budget,
            &mut scratch,
            &ocean,
        );
    }

    assert!(
        first_ring
            .iter()
            .all(|&bank| (mesh.vertices[bank].z - mesh.vertices[center].z).abs() < 1.0e-6)
    );
    assert!(mesh.vertices.iter().all(|position| {
        let distance = (*position - mesh.vertices[center]).truncate().length();
        distance > shelf_width || (position.z - mesh.vertices[center].z).abs() < 1.0e-6
    }));
    assert!(mesh.vertices.iter().enumerate().any(|(vertex, position)| {
        vertex != center && !first_ring.contains(&vertex) && position.z < 0.18
    }));
    assert!(budget.carried > 0.0);
}

#[cfg(any())]
#[test]
fn submerged_channel_grades_the_ocean_bed_beyond_its_centreline() {
    let points: Vec<Vec2> = (0..=6)
        .flat_map(|y| (0..=10).map(move |x| Vec2::new(x as f32 * 0.01, y as f32 * 0.01)))
        .collect();
    let mut mesh = Mesh::delaunay(&points);
    mesh.vertices
        .iter_mut()
        .for_each(|vertex| vertex.z = -0.001);
    let vertex_at = |x: usize, y: usize| {
        points
            .iter()
            .position(|point| *point == Vec2::new(x as f32 * 0.01, y as f32 * 0.01))
            .unwrap()
    };
    let centre = vertex_at(5, 3);
    let first_bank = vertex_at(5, 2);
    let second_bank = vertex_at(5, 1);
    let distant = vertex_at(0, 0);
    mesh.vertices[centre].z = -0.02;
    let node = RiverNode {
        vertex: centre,
        flow: 10,
        surface: -0.019,
        position: mesh.vertices[centre],
    };
    let adjacency = mesh.adjacency();
    let base_width = average_edge_length(&mesh, &adjacency);
    let mut scratch = BankScratch::new(mesh.vertices.len());
    let mut material = SurfaceMaterial::empty(mesh.vertices.len());
    let bedrock_rates = vec![1.0; mesh.vertices.len()];
    let control_areas = projected_vertex_control_areas(&mesh);
    let mut budget = RiverSedimentBudget::default();
    let ocean = vec![true; mesh.vertices.len()];

    {
        let mut terrain = test_river_terrain(
            &mut mesh,
            &adjacency,
            &mut material,
            &bedrock_rates,
            &control_areas,
        );
        carve_banks(
            &mut terrain,
            &[node],
            &[false],
            RiverCarveParameters {
                downstream_surface: f32::NEG_INFINITY,
                terminal_ocean: true,
                max_height: 0.2,
                max_flow: 10,
                depth_multiplier: 1.0,
                base_width,
                form_waterfall_shelves: false,
                cross_sections: &[],
            },
            &mut budget,
            &mut scratch,
            &ocean,
        );
    }

    assert!(mesh.vertices[first_bank].z < -0.001);
    assert!(mesh.vertices[second_bank].z < -0.001);
    assert_eq!(mesh.vertices[distant].z.to_bits(), (-0.001_f32).to_bits());
    assert!(budget.carried > 0.0);
}

#[cfg(any())]
#[test]
fn waterfall_banks_follow_the_nearest_terrace_instead_of_the_lowest_one() {
    let points: Vec<Vec2> = (0..=6)
        .flat_map(|y| (0..=6).map(move |x| Vec2::new(x as f32 / 6.0, y as f32 / 6.0)))
        .collect();
    let mut mesh = Mesh::delaunay(&points);
    mesh.vertices.iter_mut().for_each(|vertex| vertex.z = 0.18);
    let vertex_at = |x: usize, y: usize| {
        points
            .iter()
            .position(|point| *point == Vec2::new(x as f32 / 6.0, y as f32 / 6.0))
            .unwrap()
    };
    let upper = vertex_at(2, 3);
    let lower = vertex_at(3, 3);
    let upper_bank = vertex_at(2, 2);
    let lower_bank = vertex_at(3, 2);
    mesh.vertices[upper].z = 0.08;
    mesh.vertices[lower].z = 0.02;
    mesh.vertices[upper_bank].z = 0.01;
    let nodes = [
        RiverNode {
            vertex: upper,
            flow: 10,
            surface: 0.09,
            position: mesh.vertices[upper],
        },
        RiverNode {
            vertex: lower,
            flow: 10,
            surface: 0.03,
            position: mesh.vertices[lower],
        },
    ];
    let adjacency = mesh.adjacency();
    let base_width = average_edge_length(&mesh, &adjacency);
    let mut scratch = BankScratch::new(mesh.vertices.len());
    let mut material = SurfaceMaterial::empty(mesh.vertices.len());
    let bedrock_rates = vec![1.0; mesh.vertices.len()];
    let control_areas = projected_vertex_control_areas(&mesh);
    let mut budget = RiverSedimentBudget::default();
    let ocean = vec![false; mesh.vertices.len()];

    {
        let mut terrain = test_river_terrain(
            &mut mesh,
            &adjacency,
            &mut material,
            &bedrock_rates,
            &control_areas,
        );
        carve_banks(
            &mut terrain,
            &nodes,
            &[true, false],
            RiverCarveParameters {
                downstream_surface: 0.0,
                terminal_ocean: false,
                max_height: 0.2,
                max_flow: 10,
                depth_multiplier: 1.0,
                base_width,
                form_waterfall_shelves: true,
                cross_sections: &[],
            },
            &mut budget,
            &mut scratch,
            &ocean,
        );
    }

    assert!((mesh.vertices[upper_bank].z - mesh.vertices[upper].z).abs() < 1.0e-6);
    assert!((mesh.vertices[lower_bank].z - mesh.vertices[lower].z).abs() < 1.0e-6);
    assert!(mesh.vertices[upper_bank].z > mesh.vertices[lower_bank].z + 0.05);
}

#[test]
fn all_channel_footprints_use_one_ring_regardless_of_flow() {
    let points: Vec<Vec2> = (0..=8)
        .flat_map(|y| (0..=8).map(move |x| Vec2::new(x as f32 / 8.0, y as f32 / 8.0)))
        .collect();
    let terrain = Mesh::delaunay(&points);
    let center = points
        .iter()
        .position(|point| *point == Vec2::new(0.5, 0.5))
        .unwrap();
    let network = |flow| RiverNetwork {
        rivers: vec![River {
            nodes: vec![RiverNode {
                vertex: center,
                flow,
                surface: 0.01,
                position: terrain.vertices[center],
            }],
            join: None,
        }],
        join_vertices: vec![None],
        waterfalls: vec![vec![false]],
        river_mesh_ends: vec![None],
        max_flow: 100,
        max_height: 0.2,
        ocean: vec![false; terrain.vertices.len()],
        perimeter: vec![false; terrain.vertices.len()],
        cross_sections: Vec::new(),
    };
    let adjacency = terrain.adjacency();
    let narrow = build_river_footprint(&network(1), &terrain, &adjacency, false);
    let broad = build_river_footprint(&network(100), &terrain, &adjacency, false);

    for footprint in [&narrow, &broad] {
        assert!(
            footprint
                .ring_distance
                .iter()
                .zip(&footprint.coverage)
                .all(|(&distance, &coverage)| coverage == 0 || distance <= 1)
        );
        assert!(footprint.ring_distance.contains(&1));
    }
    assert_eq!(narrow.coverage, broad.coverage);
    assert!(
        broad.owner[center].unwrap().target_half_width
            > narrow.owner[center].unwrap().target_half_width
    );
}

#[test]
fn channel_targets_only_widen_and_deepen_downstream() {
    let river = River {
        nodes: [1, 4, 2, 16]
            .into_iter()
            .enumerate()
            .map(|(vertex, flow)| RiverNode {
                vertex,
                flow,
                surface: 0.1,
                position: Vec3::new(vertex as f32, 0.0, 0.1),
            })
            .collect(),
        join: None,
    };
    let settings = RiverChannelSettings {
        source_width: 0.01,
        maximum_width: 0.08,
        source_depth: 0.001,
        maximum_depth: 0.01,
    };
    let published_widths = river.target_half_widths(settings.source_width, settings.maximum_width);
    let sections = target_cross_sections(&[river], settings);

    assert!(sections[0].windows(2).all(|pair| pair[0].target_half_width
        <= pair[1].target_half_width
        && pair[0].nominal_depth <= pair[1].nominal_depth));
    assert_eq!(
        sections[0][0].target_half_width.to_bits(),
        (settings.source_width * 0.5).to_bits()
    );
    assert_eq!(
        sections[0].last().unwrap().target_half_width.to_bits(),
        (settings.maximum_width * 0.5).to_bits()
    );
    assert!(
        published_widths
            .iter()
            .zip(&sections[0])
            .all(|(width, section)| width.to_bits() == section.target_half_width.to_bits())
    );
}

#[test]
fn width_error_adds_or_removes_a_bounded_depth_compensation() {
    let section = RiverCrossSection {
        target_half_width: 0.02,
        nominal_depth: 0.004,
        achieved_width: 0.0,
        required_depth: 0.0,
    };

    let exact = compensated_channel_depth(section, 0.04, 0.02);
    let broad = compensated_channel_depth(section, 0.06, 0.02);
    let narrow = compensated_channel_depth(section, 0.02, 0.02);

    assert!(broad < exact);
    assert!(narrow > broad);
    assert_eq!(exact.to_bits(), section.nominal_depth.to_bits());
    assert_eq!(
        compensated_channel_depth(section, 1.0e-6, 0.006).to_bits(),
        0.006_f32.to_bits()
    );
}

#[test]
fn channel_nodes_keep_their_individual_floor_targets() {
    let mut mesh = Mesh {
        vertices: vec![Vec3::new(0.0, 0.0, 0.1), Vec3::new(1.0, 0.0, 0.1)],
        ..Mesh::default()
    };
    let adjacency = mesh.adjacency();
    let nodes = [
        RiverNode {
            vertex: 0,
            flow: 1,
            surface: 0.1,
            position: mesh.vertices[0],
        },
        RiverNode {
            vertex: 1,
            flow: 2,
            surface: 0.1,
            position: mesh.vertices[1],
        },
    ];
    let mut material = SurfaceMaterial::empty(mesh.vertices.len());
    let bedrock_rates = vec![1.0; mesh.vertices.len()];
    let control_areas = vec![1.0; mesh.vertices.len()];
    let mut budget = RiverSedimentBudget::default();
    {
        let mut terrain = test_river_terrain(
            &mut mesh,
            &adjacency,
            &mut material,
            &bedrock_rates,
            &control_areas,
        );
        carve_bed_reach(&mut terrain, &nodes, &[0.09, 0.04], false, &mut budget);
    }

    assert!((mesh.vertices[0].z - 0.09).abs() < 1.0e-6);
    assert!((mesh.vertices[1].z - 0.04).abs() < 1.0e-6);
}

#[test]
fn channel_rings_are_shaped_without_changing_topology() {
    let points: Vec<Vec2> = (0..=8)
        .flat_map(|y| (0..=8).map(move |x| Vec2::new(x as f32 / 8.0, y as f32 / 8.0)))
        .collect();
    let mut mesh = Mesh::delaunay(&points);
    mesh.vertices.iter_mut().for_each(|vertex| vertex.z = 0.1);
    let path: Vec<usize> = [2, 4, 6]
        .into_iter()
        .map(|x| {
            points
                .iter()
                .position(|point| *point == Vec2::new(x as f32 / 8.0, 0.5))
                .unwrap()
        })
        .collect();
    let nodes: Vec<RiverNode> = path
        .iter()
        .enumerate()
        .map(|(index, &vertex)| RiverNode {
            vertex,
            flow: (index + 1) as u32,
            surface: 0.11,
            position: mesh.vertices[vertex],
        })
        .collect();
    let adjacency = mesh.adjacency();
    let perimeter = mesh.perimeter_mask();
    let mut network = RiverNetwork {
        rivers: vec![River { nodes, join: None }],
        join_vertices: vec![None],
        waterfalls: vec![vec![false; path.len()]],
        river_mesh_ends: vec![None],
        max_flow: 3,
        max_height: 0.2,
        ocean: vec![false; mesh.vertices.len()],
        perimeter,
        cross_sections: Vec::new(),
    };
    let original_vertices = mesh.vertices.clone();
    let settings = RiverChannelSettings {
        source_width: 0.04,
        maximum_width: 0.20,
        source_depth: 0.004,
        maximum_depth: 0.012,
    };
    network.cross_sections = target_cross_sections(&network.rivers, settings);
    network.form_channel_rings(&mut mesh, &adjacency);
    let footprint = build_river_footprint(&network, &mesh, &adjacency, false);
    update_achieved_cross_sections(&mut network, &mesh, &footprint, settings.maximum_depth);

    assert_eq!(mesh.vertices.len(), original_vertices.len());
    assert!(
        mesh.vertices
            .iter()
            .zip(&original_vertices)
            .all(|(current, original)| current.z.to_bits() == original.z.to_bits())
    );
    assert!(mesh.triangles.chunks_exact(3).all(|triangle| {
        let [a, b, c] = [
            mesh.vertices[triangle[0] as usize].truncate(),
            mesh.vertices[triangle[1] as usize].truncate(),
            mesh.vertices[triangle[2] as usize].truncate(),
        ];
        (b - a).perp_dot(c - a).abs() > 1.0e-9
    }));
    assert!(
        network.cross_sections[0]
            .windows(2)
            .all(|pair| pair[0].target_half_width <= pair[1].target_half_width)
    );
    assert!(network.cross_sections[0].iter().all(|section| {
        section.required_depth >= section.nominal_depth * 0.5
            && section.required_depth <= section.nominal_depth * 1.5 + f32::EPSILON
    }));
}

#[test]
fn corridor_rings_share_the_centre_floor_then_smooth_into_the_banks() {
    let points: Vec<Vec2> = (0..=8)
        .flat_map(|y| (0..=8).map(move |x| Vec2::new(x as f32 / 8.0, y as f32 / 8.0)))
        .collect();
    let mut mesh = Mesh::delaunay(&points);
    mesh.vertices.iter_mut().for_each(|vertex| vertex.z = 0.2);
    let centre = points
        .iter()
        .position(|point| *point == Vec2::splat(0.5))
        .unwrap();
    let adjacency = mesh.adjacency();
    let network = RiverNetwork {
        rivers: vec![River {
            nodes: vec![RiverNode {
                vertex: centre,
                flow: 1,
                surface: 0.1,
                position: mesh.vertices[centre],
            }],
            join: None,
        }],
        join_vertices: vec![None],
        waterfalls: vec![vec![false]],
        river_mesh_ends: vec![None],
        max_flow: 1,
        max_height: 0.2,
        ocean: vec![false; mesh.vertices.len()],
        perimeter: mesh.perimeter_mask(),
        cross_sections: vec![vec![RiverCrossSection {
            target_half_width: 0.2,
            nominal_depth: 0.02,
            achieved_width: 0.4,
            required_depth: 0.02,
        }]],
    };
    network.form_channel_rings(&mut mesh, &adjacency);
    let footprint = build_river_footprint(&network, &mesh, &adjacency, false);
    let boundary = footprint
        .coverage
        .iter()
        .map(|&coverage| is_river_boundary(coverage))
        .collect::<Vec<_>>();
    let naturally_low = boundary
        .iter()
        .enumerate()
        .find_map(|(vertex, &is_boundary)| is_boundary.then_some(vertex))
        .unwrap();
    let apron_vertex = boundary
        .iter()
        .enumerate()
        .filter(|(_, is_boundary)| **is_boundary)
        .flat_map(|(vertex, _)| adjacency[vertex].iter().copied())
        .find(|&vertex| footprint.coverage[vertex] == 0)
        .unwrap();
    mesh.vertices[naturally_low].z = 0.05;
    let mut material = SurfaceMaterial::empty(mesh.vertices.len());
    let bedrock_rates = vec![1.0; mesh.vertices.len()];
    let control_areas = projected_vertex_control_areas(&mesh);
    let mut budgets = vec![RiverSedimentBudget::default()];
    let parameters = RiverChannelParameters {
        depth_multiplier: 1.0,
    };
    let carve = {
        let mut terrain = test_river_terrain(
            &mut mesh,
            &adjacency,
            &mut material,
            &bedrock_rates,
            &control_areas,
        );
        let carve =
            carve_river_corridor(&network, &mut terrain, &footprint, parameters, &mut budgets);
        lower_river_surroundings(
            &network,
            &mut terrain,
            &footprint,
            parameters,
            &carve,
            &mut budgets,
        );
        carve
    };
    let floor = 0.08;
    assert!(
        mesh.vertices
            .iter()
            .enumerate()
            .filter(|(vertex, _)| footprint.coverage[*vertex] != 0)
            .all(|(vertex, position)| if vertex == naturally_low {
                (position.z - 0.05).abs() < 1.0e-6
            } else {
                (position.z - floor).abs() < 1.0e-6
            })
    );

    assert!(mesh.vertices[apron_vertex].z < 0.2);
    smooth_river_corridor(
        &network, &mut mesh, &adjacency, &footprint, parameters, &carve,
    );

    assert_smoothed_corridor(&mesh, &footprint, &boundary, naturally_low, floor);
}

fn assert_smoothed_corridor(
    mesh: &Mesh,
    footprint: &RiverFootprint,
    boundary: &[bool],
    naturally_low: usize,
    floor: f32,
) {
    assert!(mesh.vertices.iter().enumerate().all(|(vertex, position)| {
        footprint.coverage[vertex] == 0
            || ((if vertex == naturally_low { 0.05 } else { floor })..=0.1 - RIVER_SURFACE_OFFSET)
                .contains(&position.z)
    }));
    assert!(mesh.vertices[naturally_low].z <= 0.05);
    assert!(mesh.vertices.iter().enumerate().any(|(vertex, position)| {
        vertex != naturally_low && boundary[vertex] && position.z > floor + f32::EPSILON
    }));
}

#[test]
fn river_mesh_is_hard_clipped_at_sea_level() {
    let points: Vec<Vec2> = (0..=8)
        .flat_map(|y| (0..=8).map(move |x| Vec2::new(x as f32 / 8.0, y as f32 / 8.0)))
        .collect();
    let mut terrain = Mesh::delaunay(&points);
    let path: Vec<usize> = (1..=7)
        .map(|x| {
            points
                .iter()
                .position(|point| *point == Vec2::new(x as f32 / 8.0, 0.5))
                .unwrap()
        })
        .collect();
    let nodes: Vec<RiverNode> = path
        .iter()
        .enumerate()
        .map(|(index, &vertex)| RiverNode {
            vertex,
            flow: 1,
            surface: if index < 3 { 0.01 } else { -0.001 },
            position: terrain.vertices[vertex],
        })
        .collect();
    let omitted_terminal = terrain.vertices[*path.last().unwrap()].truncate();
    let network = RiverNetwork {
        rivers: vec![River { nodes, join: None }],
        join_vertices: vec![None],
        waterfalls: vec![vec![false, false, true, false, false, false, false]],
        river_mesh_ends: vec![Some(3)],
        max_flow: 1,
        max_height: 0.2,
        ocean: vec![false; terrain.vertices.len()],
        perimeter: vec![false; terrain.vertices.len()],
        cross_sections: Vec::new(),
    };
    let adjacency = terrain.adjacency();
    let river_mesh = build_test_river_mesh(&network, &mut terrain, &adjacency);

    assert!(river_mesh.vertices.iter().all(|vertex| vertex.z >= 0.0));
    assert!(river_mesh.vertices.iter().any(|vertex| vertex.z == 0.0));
    assert!(
        river_mesh
            .vertices
            .iter()
            .all(|vertex| vertex.truncate() != omitted_terminal)
    );
}

#[test]
fn low_river_banks_are_lifted_with_a_smooth_outer_falloff() {
    let points: Vec<Vec2> = (0..=16)
        .flat_map(|y| {
            (0..=16).map(move |x| {
                Vec2::new(
                    x as f32 * 10.0 / ISLAND_WORLD_METRES,
                    y as f32 * 10.0 / ISLAND_WORLD_METRES,
                )
            })
        })
        .collect();
    let mut terrain = Mesh::delaunay(&points);
    let adjacency = terrain.adjacency();
    let perimeter = terrain.perimeter_mask();
    let mut coverage = terrain
        .vertices
        .iter()
        .map(|vertex| {
            u8::from(
                (60.0 / ISLAND_WORLD_METRES..=100.0 / ISLAND_WORLD_METRES).contains(&vertex.x)
                    && (50.0 / ISLAND_WORLD_METRES..=110.0 / ISLAND_WORLD_METRES)
                        .contains(&vertex.y),
            )
        })
        .collect::<Vec<_>>();
    mark_river_boundary(&adjacency, &perimeter, &mut coverage);
    let surfaces = vec![0.1; terrain.vertices.len()];
    let target_half_widths = vec![10.0 / ISLAND_WORLD_METRES; terrain.vertices.len()];
    let ocean = vec![false; terrain.vertices.len()];
    let protected = vec![false; terrain.vertices.len()];
    let banks = river_topology_masks(&terrain, &coverage).1;

    let mut protected_terrain = terrain.clone();
    let protected_patch = vec![true; terrain.vertices.len()];
    let protected_original = protected_terrain.vertices.clone();
    let protected_raised = lift_river_banks_to_surface(
        &mut protected_terrain,
        &adjacency,
        &coverage,
        &surfaces,
        &target_half_widths,
        RiverBankLiftMasks {
            ocean: &ocean,
            perimeter: &perimeter,
            protected: &protected_patch,
        },
    );
    assert_eq!(protected_raised, 0);
    assert_eq!(protected_terrain.vertices, protected_original);

    let raised = lift_river_banks_to_surface(
        &mut terrain,
        &adjacency,
        &coverage,
        &surfaces,
        &target_half_widths,
        RiverBankLiftMasks {
            ocean: &ocean,
            perimeter: &perimeter,
            protected: &protected,
        },
    );

    let eligible_banks = banks
        .iter()
        .enumerate()
        .filter(|(vertex, is_bank)| **is_bank && !perimeter[*vertex])
        .count();
    assert!(
        raised > eligible_banks,
        "raised={raised}, banks={}, outer={}",
        eligible_banks,
        terrain
            .vertices
            .iter()
            .enumerate()
            .filter(|(vertex, position)| coverage[*vertex] == 0 && position.z > 0.0)
            .count()
    );
    assert!(
        terrain
            .vertices
            .iter()
            .enumerate()
            .filter(|(vertex, _)| banks[*vertex] && !perimeter[*vertex])
            .all(|(_, vertex)| (vertex.z - 0.1).abs() < 1.0e-6)
    );
    assert!(
        terrain
            .vertices
            .iter()
            .enumerate()
            .any(|(vertex, position)| {
                coverage[vertex] == 0 && position.z > 0.0 && position.z < 0.1
            })
    );
    let distant = points
        .iter()
        .position(|point| {
            *point == Vec2::new(10.0 / ISLAND_WORLD_METRES, 80.0 / ISLAND_WORLD_METRES)
        })
        .unwrap();
    assert_eq!(terrain.vertices[distant].z.to_bits(), 0.0_f32.to_bits());
}

#[test]
fn river_corridor_refinement_caps_large_faces_in_the_bank_apron() {
    let points: Vec<Vec2> = (0..=6)
        .flat_map(|y| {
            (0..=6).map(move |x| {
                Vec2::new(
                    x as f32 * 8.0 / ISLAND_WORLD_METRES,
                    y as f32 * 8.0 / ISLAND_WORLD_METRES,
                )
            })
        })
        .collect();
    let mut terrain = Mesh::delaunay(&points);
    let mut coverage = terrain
        .vertices
        .iter()
        .map(|vertex| {
            u8::from(
                (16.0 / ISLAND_WORLD_METRES..=32.0 / ISLAND_WORLD_METRES).contains(&vertex.x)
                    && (8.0 / ISLAND_WORLD_METRES..=40.0 / ISLAND_WORLD_METRES).contains(&vertex.y),
            )
        })
        .collect::<Vec<_>>();
    let adjacency = terrain.adjacency();
    let perimeter = terrain.perimeter_mask();
    mark_river_boundary(&adjacency, &perimeter, &mut coverage);
    let original_vertex_count = terrain.vertices.len();
    let mut material = SurfaceMaterial::empty(original_vertex_count);
    let mut buffers = RiverMeshBuffers {
        coverage,
        surfaces: vec![0.05; original_vertex_count],
        river_uv: vec![Vec2::ZERO; original_vertex_count],
        owners: vec![None; original_vertex_count],
        waterfall_lips: vec![false; original_vertex_count],
        target_half_widths: vec![2.0 / ISLAND_WORLD_METRES; original_vertex_count],
        target_depths: vec![0.5 / ISLAND_WORLD_METRES; original_vertex_count],
    };

    let added = buffers.refine_corridor(&mut terrain, &mut material);

    assert!(added > 0);
    assert!(terrain.vertices.len() > original_vertex_count);
    assert_eq!(material.depths().len(), terrain.vertices.len());
    assert_eq!(buffers.coverage.len(), terrain.vertices.len());
    assert_eq!(buffers.surfaces.len(), terrain.vertices.len());
    assert_eq!(buffers.river_uv.len(), terrain.vertices.len());
    assert_eq!(buffers.owners.len(), terrain.vertices.len());
    assert_eq!(buffers.waterfall_lips.len(), terrain.vertices.len());
    assert_eq!(buffers.target_half_widths.len(), terrain.vertices.len());
    assert_eq!(buffers.target_depths.len(), terrain.vertices.len());

    let adjacency = terrain.adjacency();
    let targets = river_refinement_edge_targets(
        &adjacency,
        &buffers.coverage,
        &buffers.target_half_widths,
        RIVER_REFINEMENT_APRON_RINGS,
    );
    assert!(terrain.triangles.chunks_exact(3).all(|triangle| {
        let indices = [
            triangle[0] as usize,
            triangle[1] as usize,
            triangle[2] as usize,
        ];
        let target = indices
            .iter()
            .map(|&vertex| targets[vertex])
            .fold(f32::INFINITY, f32::min);
        if !target.is_finite() {
            return true;
        }
        let [a, b, c] = indices.map(|vertex| terrain.vertices[vertex].truncate());
        a.distance(b).max(b.distance(c)).max(c.distance(a)) <= target * 1.001
    }));
}

#[test]
fn final_channel_integrity_lowers_the_core_and_keeps_banks_pinned() {
    let points: Vec<Vec2> = (0..=8)
        .flat_map(|y| (0..=8).map(move |x| Vec2::new(x as f32 / 8.0, y as f32 / 8.0)))
        .collect();
    let mut terrain = Mesh::delaunay(&points);
    terrain
        .vertices
        .iter_mut()
        .for_each(|vertex| vertex.z = 0.12);
    let center = points
        .iter()
        .position(|point| *point == Vec2::new(0.5, 0.5))
        .unwrap();
    let adjacency = terrain.adjacency();
    let perimeter = terrain.perimeter_mask();
    let mut coverage = terrain
        .vertices
        .iter()
        .map(|vertex| {
            u8::from((0.25..=0.75).contains(&vertex.x) && (0.25..=0.75).contains(&vertex.y))
        })
        .collect::<Vec<_>>();
    mark_river_boundary(&adjacency, &perimeter, &mut coverage);
    let surfaces = vec![0.1; terrain.vertices.len()];
    let waterfall_lips = vec![false; terrain.vertices.len()];
    let banks = river_topology_masks(&terrain, &coverage).1;
    let network = RiverNetwork {
        rivers: vec![River {
            nodes: vec![RiverNode {
                vertex: center,
                flow: 100,
                surface: 0.1,
                position: terrain.vertices[center],
            }],
            join: None,
        }],
        join_vertices: vec![None],
        waterfalls: vec![vec![false]],
        river_mesh_ends: vec![None],
        max_flow: 100,
        max_height: 0.2,
        ocean: vec![false; terrain.vertices.len()],
        perimeter: vec![false; terrain.vertices.len()],
        cross_sections: vec![vec![RiverCrossSection {
            target_half_width: 0.02,
            nominal_depth: 0.02,
            achieved_width: 0.02,
            required_depth: 0.02,
        }]],
    };

    let lowered = ensure_clear_river_channel(
        &network,
        &mut terrain,
        &coverage,
        &surfaces,
        &waterfall_lips,
        &vec![false; coverage.len()],
        &vec![false; coverage.len()],
    );

    assert!(lowered > 0);
    assert!((terrain.vertices[center].z - 0.08).abs() < 1.0e-6);
    assert!(
        adjacency[center].iter().copied().any(|neighbour| {
            !banks[neighbour] && terrain.vertices[neighbour].z <= 0.09 + 1.0e-6
        })
    );
    assert!(
        terrain
            .vertices
            .iter()
            .enumerate()
            .filter(|(vertex, _)| banks[*vertex])
            .all(|(_, vertex)| (vertex.z - 0.12).abs() < 1.0e-6)
    );
}

#[test]
fn river_mesh_banks_never_climb_terrain_and_centreline_stays_below_water() {
    let points: Vec<Vec2> = (0..=8)
        .flat_map(|y| (0..=8).map(move |x| Vec2::new(x as f32 / 8.0, y as f32 / 8.0)))
        .collect();
    let mut terrain = Mesh::delaunay(&points);
    for vertex in &mut terrain.vertices {
        vertex.z = vertex.x.mul_add(0.03, vertex.y * 0.02);
    }
    let center = points
        .iter()
        .position(|point| *point == Vec2::new(0.5, 0.5))
        .unwrap();
    let surface = 0.025;
    let network = RiverNetwork {
        rivers: vec![River {
            nodes: vec![RiverNode {
                vertex: center,
                flow: 100,
                surface,
                position: terrain.vertices[center],
            }],
            join: None,
        }],
        join_vertices: vec![None],
        waterfalls: vec![vec![false]],
        river_mesh_ends: vec![None],
        max_flow: 100,
        max_height: 0.2,
        ocean: vec![false; terrain.vertices.len()],
        perimeter: vec![false; terrain.vertices.len()],
        cross_sections: Vec::new(),
    };
    let adjacency = terrain.adjacency();
    let river_mesh = build_test_river_mesh(&network, &mut terrain, &adjacency);
    assert!(!river_mesh.triangles.is_empty());
    let banks = river_mesh.perimeter_mask();
    for &water_vertex in &river_mesh.triangles {
        let water_vertex = water_vertex as usize;
        let water = river_mesh.vertices[water_vertex];
        let ground = terrain
            .vertices
            .iter()
            .find(|ground| {
                ground.x.to_bits() == water.x.to_bits() && ground.y.to_bits() == water.y.to_bits()
            })
            .expect("river topology should share its XY vertices with the terrain");
        if banks[water_vertex] {
            assert!(
                water.z <= ground.z + 1.0e-6,
                "river bank at {water:?} climbs terrain vertex {ground:?}"
            );
            assert!(
                (water.z - surface).abs() < 1.0e-6,
                "river bank at {water:?} was pulled below surface {surface}"
            );
        } else {
            assert!(
                water.z + 1.0e-6 >= ground.z + RIVER_SURFACE_OFFSET,
                "interior water at {water:?} is below terrain vertex {ground:?}"
            );
        }
    }
    assert!(terrain.vertices[center].z <= surface - RIVER_SURFACE_OFFSET);
    assert!(
        river_mesh
            .triangles
            .iter()
            .all(|&vertex| (vertex as usize) < river_mesh.vertices.len())
    );
}

#[test]
fn river_mesh_omits_triangles_made_only_from_its_outer_ring() {
    let points: Vec<Vec2> = (0..=8)
        .flat_map(|y| (0..=8).map(move |x| Vec2::new(x as f32 / 8.0, y as f32 / 8.0)))
        .collect();
    let mut terrain = Mesh::delaunay(&points);
    let center = points
        .iter()
        .position(|point| *point == Vec2::new(0.5, 0.5))
        .unwrap();
    let surface = 0.12;
    let network = RiverNetwork {
        rivers: vec![River {
            nodes: vec![RiverNode {
                vertex: center,
                flow: 100,
                surface,
                position: terrain.vertices[center],
            }],
            join: None,
        }],
        join_vertices: vec![None],
        waterfalls: vec![vec![false]],
        river_mesh_ends: vec![None],
        max_flow: 100,
        max_height: 0.2,
        ocean: vec![false; terrain.vertices.len()],
        perimeter: vec![false; terrain.vertices.len()],
        cross_sections: Vec::new(),
    };
    let adjacency = terrain.adjacency();
    let river_mesh = build_test_river_mesh(&network, &mut terrain, &adjacency);

    assert!(!river_mesh.triangles.is_empty());
    assert!(river_mesh.triangles.chunks_exact(3).all(|triangle| {
        triangle
            .iter()
            .any(|&vertex| river_mesh.vertices[vertex as usize].z > RIVER_SURFACE_OFFSET + 1.0e-6)
    }));
}

#[test]
fn river_mesh_extraction_refines_the_authoritative_terrain_topology() {
    let points: Vec<Vec2> = (0..=8)
        .flat_map(|y| (0..=8).map(move |x| Vec2::new(x as f32 / 8.0, y as f32 / 8.0)))
        .collect();
    let mut terrain = Mesh::delaunay(&points);
    let center = points
        .iter()
        .position(|point| *point == Vec2::new(0.5, 0.5))
        .unwrap();
    let network = RiverNetwork {
        rivers: vec![River {
            nodes: vec![RiverNode {
                vertex: center,
                flow: 100,
                surface: 0.12,
                position: terrain.vertices[center],
            }],
            join: None,
        }],
        join_vertices: vec![None],
        waterfalls: vec![vec![false]],
        river_mesh_ends: vec![None],
        max_flow: 100,
        max_height: 0.2,
        ocean: vec![false; terrain.vertices.len()],
        perimeter: vec![false; terrain.vertices.len()],
        cross_sections: Vec::new(),
    };
    let original_vertices = terrain.vertices.clone();
    let original_perimeter = terrain.perimeter_mask();
    let adjacency = terrain.adjacency();

    let river_mesh = build_test_river_mesh(&network, &mut terrain, &adjacency);
    assert!(terrain.vertices.len() > original_vertices.len());
    assert!(
        terrain.vertices[..original_vertices.len()]
            .iter()
            .zip(&original_vertices)
            .zip(&original_perimeter)
            .filter(|(_, perimeter)| **perimeter)
            .all(|((refined, original), _)| refined.truncate() == original.truncate())
    );
    assert_eq!(
        terrain.vertices[center].truncate(),
        original_vertices[center].truncate()
    );
    assert!(
        terrain.vertices[..original_vertices.len()]
            .iter()
            .zip(&original_vertices)
            .zip(&original_perimeter)
            .any(|((refined, original), &perimeter)| {
                !perimeter && refined.truncate() != original.truncate()
            })
    );
    assert!(
        river_mesh
            .triangles
            .iter()
            .all(|&vertex| (vertex as usize) < river_mesh.vertices.len())
    );
}

#[cfg(any())]
#[test]
fn river_refinement_preserves_bank_heights_while_lowering_the_bed() {
    let points: Vec<Vec2> = (0..=4)
        .flat_map(|y| (0..=4).map(move |x| Vec2::new(x as f32 / 4.0, y as f32 / 4.0)))
        .collect();
    let mut terrain = Mesh::delaunay(&points);
    terrain
        .vertices
        .iter_mut()
        .for_each(|vertex| vertex.z = 0.2);
    let under_river: Vec<bool> = terrain
        .vertices
        .iter()
        .map(|vertex| (0.25..=0.75).contains(&vertex.x) && (0.25..=0.75).contains(&vertex.y))
        .collect();
    let bank: Vec<bool> = terrain
        .vertices
        .iter()
        .zip(&under_river)
        .map(|(vertex, &under_river)| {
            under_river
                && ([0.25_f32.to_bits(), 0.75_f32.to_bits()].contains(&vertex.x.to_bits())
                    || [0.25_f32.to_bits(), 0.75_f32.to_bits()].contains(&vertex.y.to_bits()))
        })
        .collect();
    let center = terrain
        .vertices
        .iter()
        .position(|vertex| {
            vertex
                .truncate()
                .abs_diff_eq(Vec2::splat(0.5), f32::EPSILON)
        })
        .unwrap();
    let adjacency = terrain.adjacency();
    let surfaces = vec![0.05; terrain.vertices.len()];

    smooth_river_terrain_vertices(&mut terrain, &adjacency, &under_river, &bank, &surfaces);

    assert!(
        terrain
            .vertices
            .iter()
            .zip(&bank)
            .filter(|(_, is_bank)| **is_bank)
            .all(|(vertex, _)| (vertex.z - 0.2).abs() <= f32::EPSILON)
    );
    assert!(terrain.vertices[center].z <= 0.05 - RIVER_SURFACE_OFFSET);
}

#[test]
fn river_mesh_extraction_repairs_an_isolated_sharp_bed_point() {
    let points: Vec<Vec2> = (0..=8)
        .flat_map(|y| (0..=8).map(move |x| Vec2::new(x as f32 / 8.0, y as f32 / 8.0)))
        .collect();
    let mut terrain = Mesh::delaunay(&points);
    let center = points
        .iter()
        .position(|point| *point == Vec2::new(0.5, 0.5))
        .unwrap();
    terrain.vertices[center].z = -0.1;
    let network = RiverNetwork {
        rivers: vec![River {
            nodes: vec![RiverNode {
                vertex: center,
                flow: 100,
                surface: 0.12,
                position: terrain.vertices[center],
            }],
            join: None,
        }],
        join_vertices: vec![None],
        waterfalls: vec![vec![false]],
        river_mesh_ends: vec![None],
        max_flow: 100,
        max_height: 0.2,
        ocean: vec![false; terrain.vertices.len()],
        perimeter: vec![false; terrain.vertices.len()],
        cross_sections: Vec::new(),
    };
    let original_vertex_count = terrain.vertices.len();
    let adjacency = terrain.adjacency();

    let river_mesh = build_test_river_mesh(&network, &mut terrain, &adjacency);

    assert!(terrain.vertices.len() > original_vertex_count);
    assert!(terrain.vertices[center].z > -0.1);
    assert!(terrain.vertices[center].z < 0.12);
    assert!(
        river_mesh
            .vertices
            .iter()
            .any(|vertex| vertex.truncate() == terrain.vertices[center].truncate())
    );
}

#[test]
fn river_mouth_reuses_the_last_existing_waterfall_for_its_submerged_channel() {
    let mut mesh = Mesh {
        vertices: (0..12)
            .map(|index| {
                let height = if index >= 8 {
                    -0.000_01
                } else {
                    0.08 - index as f32 * 0.008
                };
                Vec3::new(index as f32 * 0.01, 0.5, height)
            })
            .collect(),
        ..Mesh::default()
    };
    let mut nodes: Vec<RiverNode> = mesh
        .vertices
        .iter()
        .copied()
        .enumerate()
        .map(|(vertex, position)| RiverNode {
            vertex,
            flow: 10,
            surface: position.z.max(0.0),
            position,
        })
        .collect();
    let original_waterfall_lip = mesh.vertices[6].z;
    let mut material = SurfaceMaterial::empty(mesh.vertices.len());
    let bedrock_rates = vec![1.0; mesh.vertices.len()];
    let control_areas = vec![1.0; mesh.vertices.len()];
    let mut budget = RiverSedimentBudget::default();
    let mut waterfalls = vec![false; nodes.len()];
    waterfalls[3] = true;
    waterfalls[6] = true;
    let ocean: Vec<bool> = (0..nodes.len()).map(|index| index >= 8).collect();
    let ocean_entry = river_ocean_entry(&nodes, &ocean).unwrap();
    let mouth = river_mouth_transition(ocean_entry, &waterfalls);

    assert_eq!(
        mouth,
        RiverMouthTransition {
            waterfall_segment: Some(6),
            river_mesh_end: 7,
        }
    );

    {
        let adjacency = mesh.adjacency();
        let mut terrain = test_river_terrain(
            &mut mesh,
            &adjacency,
            &mut material,
            &bedrock_rates,
            &control_areas,
        );
        carve_submerged_river_mouth(
            &mut terrain,
            &mut nodes,
            &mut waterfalls,
            mouth,
            0.2,
            &[],
            &mut budget,
        );
    }

    assert_eq!(
        mesh.vertices[6].z.to_bits(),
        original_waterfall_lip.to_bits()
    );
    assert!(waterfalls[3]);
    assert!(waterfalls[6]);
    assert!(waterfalls[7..].iter().all(|&waterfall| !waterfall));
    assert!(nodes[7..].iter().all(|node| node.surface < 0.0));
    assert!(
        nodes[7..]
            .windows(2)
            .all(|pair| pair[0].surface + f32::EPSILON >= pair[1].surface)
    );
    let mouth_depth = 0.2 * 0.0025;
    assert!(
        mesh.vertices[7..]
            .iter()
            .zip(&nodes[7..])
            .all(|(vertex, node)| vertex.z <= node.surface - mouth_depth + f32::EPSILON)
    );
    assert!(budget.carried > 0.0);
}

#[test]
fn river_without_a_waterfall_is_carved_entirely_below_the_sea_plane() {
    let mut mesh = Mesh {
        vertices: (0..6)
            .map(|index| Vec3::new(index as f32 * 0.01, 0.5, 0.06 - index as f32 * 0.008))
            .collect(),
        ..Mesh::default()
    };
    let mut nodes: Vec<RiverNode> = mesh
        .vertices
        .iter()
        .copied()
        .enumerate()
        .map(|(vertex, position)| RiverNode {
            vertex,
            flow: 10,
            surface: position.z,
            position,
        })
        .collect();
    let mut waterfalls = vec![false; nodes.len()];
    let mouth = river_mouth_transition(4, &waterfalls);
    assert_eq!(
        mouth,
        RiverMouthTransition {
            waterfall_segment: None,
            river_mesh_end: 0,
        }
    );

    let mut material = SurfaceMaterial::empty(mesh.vertices.len());
    let bedrock_rates = vec![1.0; mesh.vertices.len()];
    let control_areas = vec![1.0; mesh.vertices.len()];
    let mut budget = RiverSedimentBudget::default();
    {
        let adjacency = mesh.adjacency();
        let mut terrain = test_river_terrain(
            &mut mesh,
            &adjacency,
            &mut material,
            &bedrock_rates,
            &control_areas,
        );
        carve_submerged_river_mouth(
            &mut terrain,
            &mut nodes,
            &mut waterfalls,
            mouth,
            0.2,
            &[],
            &mut budget,
        );
    }

    assert!(waterfalls.iter().all(|&waterfall| !waterfall));
    assert!(nodes.iter().all(|node| node.surface < 0.0));
    assert!(mesh.vertices.iter().all(|vertex| vertex.z < 0.0));
    assert!(budget.carried > 0.0);
}

#[test]
fn delta_builds_a_raised_valley_and_spreads_offshore() {
    let mut points = Vec::new();
    for y in 0..5 {
        for x in 0..5 {
            points.push(Vec2::new(x as f32 * 0.25, y as f32 * 0.25));
        }
    }
    let mut mesh = Mesh::delaunay(&points);
    for vertex in &mut mesh.vertices {
        vertex.z = if vertex.x < 0.25 {
            0.04
        } else if vertex.x < 0.5 {
            0.001
        } else {
            -0.02 - (vertex.y - 0.5).abs() * 0.004 - vertex.x * 0.002
        };
    }
    let previous = points
        .iter()
        .position(|point| *point == Vec2::new(0.25, 0.5))
        .unwrap();
    let outlet = points
        .iter()
        .position(|point| *point == Vec2::new(0.5, 0.5))
        .unwrap();
    let nodes = [
        RiverNode {
            vertex: previous,
            flow: 10,
            surface: 0.01,
            position: mesh.vertices[previous],
        },
        RiverNode {
            vertex: outlet,
            flow: 10,
            surface: 0.0,
            position: mesh.vertices[outlet],
        },
    ];
    let adjacency = mesh.adjacency();
    let edge_length = average_edge_length(&mesh, &adjacency);
    let before: Vec<f32> = mesh.vertices.iter().map(|vertex| vertex.z).collect();
    let channel_before = [before[previous], before[outlet]];
    let mut scratch = DeltaScratch::new(mesh.vertices.len());
    let mut material = SurfaceMaterial::empty(mesh.vertices.len());
    let control_areas = projected_vertex_control_areas(&mesh);
    let mut budget = RiverSedimentBudget {
        carried: 1.0,
        bedrock_eroded: 1.0,
        ..RiverSedimentBudget::default()
    };

    {
        let bedrock_rates = vec![1.0; mesh.vertices.len()];
        let mut terrain = test_river_terrain(
            &mut mesh,
            &adjacency,
            &mut material,
            &bedrock_rates,
            &control_areas,
        );
        create_delta(
            &mut terrain,
            &nodes,
            &mut budget,
            0.2,
            edge_length,
            &mut scratch,
        );
    }

    let changed: Vec<usize> = mesh
        .vertices
        .iter()
        .enumerate()
        .filter_map(|(index, vertex)| (vertex.z > before[index]).then_some(index))
        .collect();
    assert!(changed.len() > 3);
    assert!(changed.iter().any(|&index| {
        mesh.vertices[index].x > 0.5 && (mesh.vertices[index].y - 0.5).abs() > 0.1
    }));
    assert!(changed.iter().any(|&index| mesh.vertices[index].z > 0.0));
    assert!((mesh.vertices[previous].z - channel_before[0]).abs() < f32::EPSILON);
    assert!((mesh.vertices[outlet].z - channel_before[1]).abs() < f32::EPSILON);
}

#[test]
fn outer_valley_hardness_preserves_resistant_banks_after_loose_cover() {
    let mut mesh = Mesh {
        vertices: vec![Vec3::new(0.0, 0.0, 0.1), Vec3::new(1.0, 0.0, 0.1)],
        ..Mesh::default()
    };
    let mut material = SurfaceMaterial::empty(mesh.vertices.len());
    material.depths_mut().fill(0.02);
    let bedrock_rates = [0.05, 1.0];
    let control_areas = [1.0, 1.0];
    let mut hard_budget = RiverSedimentBudget::default();
    let mut soft_budget = RiverSedimentBudget::default();
    let adjacency = mesh.adjacency();
    {
        let mut terrain = test_river_terrain(
            &mut mesh,
            &adjacency,
            &mut material,
            &bedrock_rates,
            &control_areas,
        );
        terrain.carve_vertex(0, 0.0, 0.0, &mut hard_budget);
        terrain.carve_vertex(1, 0.0, 0.0, &mut soft_budget);
    }

    assert_eq!(material.depths(), &[0.0, 0.0]);
    assert!(mesh.vertices[0].z > mesh.vertices[1].z + 0.07);
    assert!((hard_budget.loose_eroded - soft_budget.loose_eroded).abs() < 1.0e-7);
    assert!(soft_budget.bedrock_eroded > hard_budget.bedrock_eroded * 10.0);
}

#[test]
fn tributary_budget_transfer_and_outlet_export_are_conservative() {
    let mut tributary = RiverSedimentBudget::default();
    tributary.record_erosion(0.2, 0.3, 2.0);
    let mut main_stem = RiverSedimentBudget::default();
    main_stem.record_erosion(0.1, 0.4, 1.0);

    main_stem.absorb(tributary);
    main_stem.export_remaining();

    assert!(main_stem.is_balanced());
    assert_eq!(main_stem.carried.to_bits(), 0.0_f64.to_bits());
    assert!((main_stem.exported - 1.5).abs() < 1.0e-6);
}

#[test]
fn routed_rivers_have_monotonic_surfaces_and_valid_joins() {
    let points = [
        Vec2::new(0.0, 0.0),
        Vec2::new(1.0, 0.0),
        Vec2::new(1.0, 1.0),
        Vec2::new(0.0, 1.0),
        Vec2::new(0.5, 0.5),
        Vec2::new(0.25, 0.65),
        Vec2::new(0.75, 0.65),
    ];
    let mut mesh = Mesh::delaunay(&points);
    for vertex in &mut mesh.vertices {
        vertex.z = 0.2 - (vertex.y - 0.5).abs() * 0.3;
    }
    mesh.vertices[0].z = -0.02;
    mesh.vertices[1].z = -0.02;
    let adjacency = mesh.adjacency();
    let mut network = RiverNetwork::generate(
        &mut mesh,
        &adjacency,
        RiverSourceRule::new(0.0, 1.0, 0.0, 1.0),
        0,
    );
    let mut material = SurfaceMaterial::empty(mesh.vertices.len());
    network.shape(&mut mesh, &adjacency, &mut material, true, true);
    for (index, river) in network.rivers.iter().enumerate() {
        assert!(river.join.is_none_or(|join| join < index));
        if let (Some(join), Some(join_vertex), Some(terminal)) =
            (river.join, network.join_vertices[index], river.nodes.last())
        {
            let joined_surface = network.rivers[join]
                .nodes
                .iter()
                .find(|node| node.vertex == join_vertex)
                .map(|node| node.surface)
                .expect("join vertex belongs to the receiving river");
            assert!((terminal.surface - joined_surface).abs() <= 1.0e-6);
        }
        let mut outlet = index;
        while let Some(join) = network.rivers[outlet].join {
            outlet = join;
        }
        assert!(
            network.rivers[outlet]
                .nodes
                .last()
                .is_some_and(|node| network.ocean[node.vertex])
        );
        assert!(
            river
                .nodes
                .windows(2)
                .all(|pair| pair[0].surface + 1.0e-6 >= pair[1].surface)
        );
    }
}

#[test]
fn self_contact_index_ignores_the_local_tail_but_detects_a_return() {
    let mesh = Mesh {
        vertices: vec![Vec3::ZERO; 9],
        triangles: vec![0, 6, 7, 6, 7, 8],
        ..Mesh::default()
    };
    let adjacency = mesh.adjacency();
    let mut contact = RiverSelfContactIndex::new(mesh.vertices.len());
    contact.register(0, 0);
    contact.register(6, 6);

    assert!(contact.touches_earlier(&adjacency, 7, 1, 4));
    assert!(!contact.touches_earlier(&adjacency, 8, 1, 4));
}

#[test]
fn rivers_join_when_their_mesh_rings_touch_before_their_centrelines() {
    let main = [0, 1, 2, 3, 4];
    let tributary = [5, 6, 7, 8, 9];
    let shared_bank = 18;
    let mut vertices = vec![Vec3::new(0.0, 0.0, 3.0); 21];
    for (step, &centre) in main.iter().enumerate() {
        vertices[centre] = Vec3::new(step as f32, 0.0, 0.9 - step as f32 * 0.1);
    }
    for (step, &centre) in tributary.iter().enumerate() {
        vertices[centre] = Vec3::new(step as f32, 2.0, 0.9 - step as f32 * 0.1);
    }
    vertices[shared_bank] = Vec3::new(4.0, 1.0, 3.0);
    let mut triangles = Vec::new();
    for (edge, pair) in main.windows(2).chain(tributary.windows(2)).enumerate() {
        triangles.extend([pair[0] as u32, pair[1] as u32, (10 + edge) as u32]);
    }
    triangles.extend([4, shared_bank as u32, 19, 9, shared_bank as u32, 20]);
    let mut mesh = Mesh {
        vertices,
        triangles,
        ..Mesh::default()
    };
    let adjacency = mesh.adjacency();
    let mut flow = vec![1; mesh.vertices.len()];
    for &centre in &tributary {
        flow[centre] = 2;
    }
    flow[10] = 100;
    let mut ocean = vec![false; mesh.vertices.len()];
    ocean[*main.last().unwrap()] = true;
    let (rivers, join_vertices) = trace_rivers(
        &mut mesh,
        &adjacency,
        &flow,
        &[main[0], tributary[0]],
        &ocean,
        0,
    );

    assert_eq!(rivers.len(), 2);
    assert_eq!(rivers[1].join, Some(0));
    assert_eq!(
        rivers[1].nodes.last().map(|node| node.vertex),
        Some(tributary[4])
    );
    assert!(!main.contains(&tributary[4]));
    assert_eq!(join_vertices[1], Some(main[4]));
    assert_eq!(rivers[0].nodes.last().map(|node| node.flow), Some(2));

    let mut main_footprint = RiverFootprintIndex::new(mesh.vertices.len());
    main_footprint.register_river(0, &rivers[0], &adjacency, 100);
    assert!(
        main_footprint
            .touching(&mesh, &adjacency, tributary[4], 1)
            .is_some()
    );
    assert!(adjacency[tributary[4]].contains(&shared_bank));
    assert!(adjacency[main[4]].contains(&shared_bank));
}

#[test]
fn later_river_sources_are_rejected_near_an_accepted_river_path() {
    let separation = 20.0 / ISLAND_WORLD_METRES;
    let mut mesh = Mesh {
        vertices: vec![
            Vec3::new(0.0, 0.0, 0.2),
            Vec3::new(0.01, 0.0, 0.1),
            Vec3::new(0.02, 0.0, 0.0),
            Vec3::new(0.01, -0.01, 0.3),
            Vec3::new(0.0, separation, 0.2),
            Vec3::new(0.01, separation, 0.1),
            Vec3::new(0.02, separation, 0.0),
            Vec3::new(0.01, separation + 0.01, 0.3),
        ],
        triangles: vec![0, 1, 3, 1, 2, 3, 4, 5, 7, 5, 6, 7],
        ..Mesh::default()
    };
    let adjacency = mesh.adjacency();
    let flow = [3, 4, 5, 1, 3, 4, 5, 1];
    let mut ocean = [false; 8];
    ocean[2] = true;
    ocean[6] = true;

    let (rivers, join_vertices) = trace_rivers(&mut mesh, &adjacency, &flow, &[0, 4], &ocean, 0);

    assert_eq!(rivers.len(), 1);
    assert_eq!(rivers[0].nodes.first().map(|node| node.vertex), Some(0));
    assert_eq!(join_vertices, [None]);
}

#[test]
fn trace_discards_a_landlocked_path_when_no_ocean_route_exists() {
    let mut mesh = Mesh {
        vertices: vec![
            Vec3::new(0.0, 0.0, 0.3),
            Vec3::new(1.0, 0.0, 0.2),
            Vec3::new(0.5, 1.0, 0.1),
        ],
        triangles: vec![0, 1, 2],
        ..Mesh::default()
    };
    let adjacency = mesh.adjacency();
    let flow = [1, 2, 3];

    let (rivers, join_vertices) = trace_rivers(&mut mesh, &adjacency, &flow, &[0], &[false; 3], 0);

    assert!(rivers.is_empty());
    assert!(join_vertices.is_empty());
}
