#![allow(clippy::cast_precision_loss)]

use std::{
    collections::{HashMap, HashSet},
    fs,
    time::{SystemTime, UNIX_EPOCH},
};

use motu::{BoundingBox, Island, IslandOptions, write_png};

#[test]
fn public_vectors_are_glam_types() {
    let vector: glam::Vec3 = motu::Vec3::new(1.0, 2.0, 3.0);
    assert_eq!(vector.truncate(), glam::Vec2::new(1.0, 2.0));
}

fn small_options() -> IslandOptions {
    IslandOptions {
        terrain_size: 65,
        ..IslandOptions::default()
    }
}

fn mapped_waterfall_uv_segments(island: &Island) -> usize {
    let river_uv_by_xy: HashMap<(u32, u32), glam::Vec2> = island
        .river_mesh()
        .vertices
        .iter()
        .zip(&island.river_mesh().uv)
        .map(|(vertex, &uv)| ((vertex.x.to_bits(), vertex.y.to_bits()), uv))
        .collect();
    island
        .rivers()
        .iter()
        .flat_map(|river| river.nodes.windows(2))
        .filter(|pair| {
            let drop = pair[0].surface - pair[1].surface;
            if drop < 0.001 {
                return false;
            }
            let upstream = (pair[0].position.x.to_bits(), pair[0].position.y.to_bits());
            let downstream = (pair[1].position.x.to_bits(), pair[1].position.y.to_bits());
            river_uv_by_xy
                .get(&upstream)
                .zip(river_uv_by_xy.get(&downstream))
                .is_some_and(|(upstream_uv, downstream_uv)| {
                    downstream_uv.y - upstream_uv.y >= drop * 0.5
                })
        })
        .count()
}

fn assert_river_mesh_bank_and_centreline_clearance(island: &Island) {
    let banks = island.river_mesh().perimeter_vertices();
    for &river_vertex in &island.river_mesh().triangles {
        let river_vertex = river_vertex as usize;
        let water = island.river_mesh().vertices[river_vertex];
        let ground = island.terrain().sample(water.x, water.y);
        if banks.contains(&river_vertex) {
            assert!(
                water.z <= ground + 1.0e-6,
                "river bank climbs terrain at {water:?}; terrain height is {ground}"
            );
        }
    }
    for node in island.rivers().iter().flat_map(|river| &river.nodes) {
        assert!(
            node.position.z <= node.surface - 0.000_01 + 1.0e-6,
            "river centreline terrain at {:?} is not below surface {}",
            node.position,
            node.surface
        );
    }
}

fn assert_river_mesh_bank_distance_uv(island: &Island) {
    let river_mesh = island.river_mesh();
    assert!(
        river_mesh
            .uv
            .iter()
            .all(|uv| uv.x.is_finite() && uv.x >= 0.0)
    );
    assert!(river_mesh.uv.iter().any(|uv| uv.x > 0.0));
    assert!(
        river_mesh
            .perimeter_vertices()
            .iter()
            .all(|&vertex| river_mesh.uv[vertex].x == 0.0)
    );
}

#[test]
fn generation_is_deterministic() {
    let first = Island::generate(2018, small_options()).unwrap();
    let second = Island::generate(2018, small_options()).unwrap();
    assert_eq!(first.terrain(), second.terrain());
    assert_eq!(first.rivers(), second.rivers());
    assert_eq!(first.decorations(), second.decorations());
}

#[test]
fn generation_is_deterministic_across_worker_counts() {
    let generate = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                Island::generate(
                    2018,
                    IslandOptions {
                        terrain_size: 24,
                        ..IslandOptions::default()
                    },
                )
                .unwrap()
            })
    };
    let single = generate(1);
    let parallel = generate(4);
    assert_eq!(single.terrain(), parallel.terrain());
    assert_eq!(single.rivers(), parallel.rivers());
}

#[test]
fn final_terrain_has_ten_centimetres_clearance_from_the_sea_plane() {
    let island = Island::generate(
        2018,
        IslandOptions {
            terrain_size: 24,
            ..IslandOptions::default()
        },
    )
    .unwrap();
    let clearance = 0.10 / 2_000.0;

    assert!(
        island
            .terrain()
            .vertices()
            .iter()
            .all(|vertex| vertex.z <= -clearance || vertex.z >= clearance)
    );
}

#[test]
fn water_ratio_increases_connected_ocean_coverage() {
    let coverage = |water_ratio| {
        let island = Island::generate(
            7,
            IslandOptions {
                water_ratio,
                ..small_options()
            },
        )
        .unwrap();
        let mesh = island.terrain().mesh();
        let (water, total) =
            mesh.triangles
                .chunks_exact(3)
                .fold((0.0_f32, 0.0_f32), |(water, total), triangle| {
                    let [a, b, c] = [
                        mesh.vertices[triangle[0] as usize],
                        mesh.vertices[triangle[1] as usize],
                        mesh.vertices[triangle[2] as usize],
                    ];
                    let area = (b - a).truncate().perp_dot((c - a).truncate()).abs() * 0.5;
                    let underwater = [a, b, c]
                        .into_iter()
                        .filter(|vertex| vertex.z <= 0.0)
                        .count() as f32
                        / 3.0;
                    (water + area * underwater, total + area)
                });
        water / total
    };
    assert!(coverage(0.6) < coverage(0.85));
}

#[test]
fn hydraulic_erosion_strength_changes_terrain() {
    let without_hydraulic = Island::generate(
        17,
        IslandOptions {
            hydraulic_erosion_strength: 0.0,
            ..small_options()
        },
    )
    .unwrap();
    let stronger_hydraulic = Island::generate(
        17,
        IslandOptions {
            hydraulic_erosion_strength: 2.0,
            ..small_options()
        },
    )
    .unwrap();

    assert_ne!(
        without_hydraulic.terrain().vertices(),
        stronger_hydraulic.terrain().vertices()
    );
}

#[test]
fn rejects_out_of_range_hydraulic_erosion_strength() {
    let error = Island::generate(
        1,
        IslandOptions {
            hydraulic_erosion_strength: 8.01,
            ..small_options()
        },
    )
    .unwrap_err();
    assert!(error.contains("hydraulic_erosion_strength"));
}

#[test]
fn hydraulic_deposition_controls_change_terrain() {
    let without_deposition = Island::generate(
        17,
        IslandOptions {
            hydraulic_erosion_strength: 2.0,
            hydraulic_deposition_strength: 0.0,
            ..small_options()
        },
    )
    .unwrap();
    let stronger_gentle_deposition = Island::generate(
        17,
        IslandOptions {
            hydraulic_erosion_strength: 2.0,
            hydraulic_deposition_strength: 4.0,
            hydraulic_deposition_slope_degrees: 20.0,
            ..small_options()
        },
    )
    .unwrap();

    assert_ne!(
        without_deposition.terrain().vertices(),
        stronger_gentle_deposition.terrain().vertices()
    );
}

#[test]
fn rejects_out_of_range_hydraulic_deposition_options() {
    for options in [
        IslandOptions {
            hydraulic_deposition_strength: 4.01,
            ..small_options()
        },
        IslandOptions {
            hydraulic_deposition_slope_degrees: 0.99,
            ..small_options()
        },
        IslandOptions {
            hydraulic_deposition_slope_degrees: 45.01,
            ..small_options()
        },
    ] {
        assert!(Island::generate(1, options).is_err());
    }
}

#[test]
fn absolute_river_source_catchment_changes_river_generation() {
    let many_sources = Island::generate(
        53,
        IslandOptions {
            river_source_catchment_hectares: 0.005,
            river_source_steep_multiplier: 1.0,
            ..small_options()
        },
    )
    .unwrap();
    let few_sources = Island::generate(
        53,
        IslandOptions {
            river_source_catchment_hectares: 5.0,
            river_source_steep_multiplier: 1.0,
            ..small_options()
        },
    )
    .unwrap();

    assert_ne!(many_sources.rivers(), few_sources.rivers());
    assert_ne!(many_sources.terrain(), few_sources.terrain());
}

#[test]
fn every_final_river_reaches_the_sea_or_joins_one_that_does() {
    let island = Island::generate(
        2018,
        IslandOptions {
            water_ratio: 0.95,
            ..IslandOptions::default()
        },
    )
    .unwrap();

    assert!(!island.rivers().is_empty());
    for index in 0..island.rivers().len() {
        let mut outlet = index;
        while let Some(join) = island.rivers()[outlet].join {
            assert!(join < outlet);
            outlet = join;
        }
        assert!(
            island.rivers()[outlet]
                .nodes
                .last()
                .is_some_and(|node| node.position.z < 0.0),
            "river {index} terminates on land through outlet {outlet}"
        );
    }
}

#[test]
fn native_options_are_not_limited_to_viewer_control_ranges() {
    let island = Island::generate(
        1,
        IslandOptions {
            water_ratio: 0.5,
            river_source_catchment_hectares: 10.0,
            ..small_options()
        },
    );
    assert!(island.is_ok());
}

#[test]
fn lods_reduce_mesh_density() {
    let island = Island::generate(19, small_options()).unwrap();
    let counts: Vec<usize> = (0..3)
        .map(|lod| island.lod(lod).unwrap().triangles.len())
        .collect();
    assert!(counts[0] > counts[1]);
    assert!(counts[1] > counts[2]);
    assert!(island.mesh_in(0, BoundingBox::default()).is_some());
}

#[test]
fn exported_terrain_is_clipped_five_metres_below_sea_level() {
    let island = Island::generate(19, small_options()).unwrap();
    let floor = -5.0 / motu::ISLAND_WORLD_METRES;
    let source = island.lod(0).unwrap();
    let support = island.mesh_in(0, BoundingBox::default()).unwrap();
    let render = island.render_mesh_in(0, BoundingBox::default(), 0).unwrap();
    let tiles = island
        .render_mesh_grid(0, BoundingBox::default(), 2, 0)
        .unwrap();

    assert!(source.vertices.iter().any(|vertex| vertex.z < floor));
    assert!(support.vertices.len() < source.vertices.len());
    for mesh in std::iter::once(&support)
        .chain(std::iter::once(&render))
        .chain(&tiles)
    {
        assert!(mesh.vertices.iter().all(|vertex| vertex.z >= floor));
        let mut used = vec![false; mesh.vertices.len()];
        for &vertex in &mesh.triangles {
            used[vertex as usize] = true;
        }
        assert!(used.into_iter().all(std::convert::identity));
    }
}

#[test]
fn coarser_lods_are_refined_and_pinned_to_the_final_lod0_surface() {
    let island = Island::generate(23, small_options()).unwrap();
    let lod0 = island.lod(0).unwrap();
    let lod1 = island.lod(1).unwrap();
    let lod2 = island.lod(2).unwrap();

    assert!(lod0.triangles.len() > lod1.triangles.len());
    assert!(lod1.triangles.len() > lod2.triangles.len());
    for mesh in [lod1, lod2] {
        assert!(mesh.vertices.iter().all(|vertex| {
            (island.terrain().sample(vertex.x, vertex.y) - vertex.z).abs() < 1.0e-5
        }));
    }
}

#[test]
fn lod0_render_uses_the_corrected_support_mesh() {
    let island = Island::generate(
        23,
        IslandOptions {
            hydraulic_erosion_strength: 8.0,
            ..small_options()
        },
    )
    .unwrap();
    let support = island.lod(0).unwrap();
    let render = island.render_lod(0).unwrap();

    assert_eq!(render, support);
}

#[test]
fn hydraulic_erosion_does_not_reverse_projected_faces() {
    let options = |hydraulic_erosion_strength| IslandOptions {
        hydraulic_erosion_strength,
        river_source_catchment_hectares: 5.0,
        ..small_options()
    };
    let reversed = |mesh: &motu::Mesh| {
        mesh.triangles
            .chunks_exact(3)
            .fold((0, 0.0_f32), |(count, minimum), triangle| {
                let [a, b, c] = [
                    mesh.vertices[triangle[0] as usize].truncate(),
                    mesh.vertices[triangle[1] as usize].truncate(),
                    mesh.vertices[triangle[2] as usize].truncate(),
                ];
                let area = (b - a).perp_dot(c - a);
                (count + usize::from(area < -1.0e-10), minimum.min(area))
            })
    };
    let without = Island::generate(23, options(0.0)).unwrap();
    let strong = Island::generate(23, options(8.0)).unwrap();
    let (without_reversed, without_minimum) = reversed(without.lod(0).unwrap());
    let (strong_reversed, strong_minimum) = reversed(strong.lod(0).unwrap());
    assert_eq!(
        strong_reversed, without_reversed,
        "hydraulic changed reversed faces from {without_reversed} ({without_minimum}) to {strong_reversed} ({strong_minimum})"
    );
}

#[test]
fn terrain_topology_is_free_form_delaunay() {
    use std::collections::HashSet;

    let island = Island::generate(41, small_options()).unwrap();
    let coarse = island.lod(2).unwrap();
    let unique_x: HashSet<u32> = coarse
        .vertices
        .iter()
        .map(|vertex| vertex.x.to_bits())
        .collect();
    let unique_y: HashSet<u32> = coarse
        .vertices
        .iter()
        .map(|vertex| vertex.y.to_bits())
        .collect();
    assert!(coarse.vertices.len() > 65);
    assert!(unique_x.len() * unique_y.len() > coarse.vertices.len() * 8);
    assert!((coarse.surface_area_xy() - 1.0).abs() < 1.0e-4);
    let lod1 = island.lod(1).unwrap();
    let lod0 = island.lod(0).unwrap();
    assert!(lod1.triangles.len() > coarse.triangles.len());
    assert!(
        lod1.triangles.len() < coarse.triangles.len() * 48,
        "adaptive refinement grew LOD1 from {} to {} indices",
        coarse.triangles.len(),
        lod1.triangles.len()
    );
    assert!(lod0.triangles.len() > lod1.triangles.len());
}

#[test]
fn finest_lod_concentrates_detail_on_land() {
    let island = Island::generate(43, small_options()).unwrap();
    let mesh = island.lod(0).unwrap();
    let mut land = (0.0_f32, 0_usize);
    let mut sea = (0.0_f32, 0_usize);
    for triangle in mesh.triangles.chunks_exact(3) {
        let vertices = [
            mesh.vertices[triangle[0] as usize],
            mesh.vertices[triangle[1] as usize],
            mesh.vertices[triangle[2] as usize],
        ];
        let area = (vertices[1].truncate() - vertices[0].truncate())
            .x
            .mul_add(
                (vertices[2].truncate() - vertices[0].truncate()).y,
                -((vertices[1].truncate() - vertices[0].truncate()).y
                    * (vertices[2].truncate() - vertices[0].truncate()).x),
            )
            .abs()
            * 0.5;
        let target = if vertices.iter().any(|vertex| vertex.z > 0.0) {
            &mut land
        } else if vertices.iter().all(|vertex| vertex.z < -0.02) {
            // The wave-cut platform is intentionally refined on both sides
            // of sea level. Compare land against deep seabed, which should
            // still retain materially larger faces.
            &mut sea
        } else {
            continue;
        };
        target.0 += area;
        target.1 += 1;
    }
    let average_land_area = land.0 / land.1 as f32;
    let average_sea_area = sea.0 / sea.1 as f32;
    assert!(average_land_area < average_sea_area * 0.7);
}

#[test]
fn map_exports_have_expected_lengths() {
    let island = Island::generate(23, small_options()).unwrap();
    let heights = island.height_map(31, 17);
    assert_eq!(heights.len(), 31 * 17);
    assert!(heights.iter().all(|height| height.is_finite()));
    for (actual, expected) in [
        (heights[0], island.terrain().sample(0.0, 0.0)),
        (heights[30], island.terrain().sample(1.0, 0.0)),
        (heights[16 * 31], island.terrain().sample(0.0, 1.0)),
        (heights[17 * 31 - 1], island.terrain().sample(1.0, 1.0)),
    ] {
        assert!((actual - expected).abs() <= f32::EPSILON);
    }
    assert!(island.height_map(0, 17).is_empty());
    assert!(island.height_map(31, 0).is_empty());
    assert_eq!(island.sea_depth_map(31, 17).len(), 31 * 17);
    assert_eq!(island.normal_map(31, 17).len(), 31 * 17 * 3);
    assert_eq!(island.foliage_map(31).len(), 31 * 31);
}

#[test]
fn coarse_lod_surface_maps_capture_detail_and_occlusion() {
    let island = Island::generate(23, small_options()).unwrap();
    let lod0 = island.surface_maps(0, 64, 48).unwrap();
    let lod1 = island.surface_maps(1, 64, 48).unwrap();
    let lod2 = island.surface_maps(2, 64, 48).unwrap();

    assert!(
        lod0.normal_rgb()
            .chunks_exact(3)
            .any(|pixel| pixel != [127, 127, 255])
    );
    assert!(lod0.occlusion().iter().any(|value| *value < u8::MAX));
    assert_eq!(lod1.width(), 64);
    assert_eq!(lod1.height(), 48);
    assert_eq!(lod1.normal_rgb().len(), 64 * 48 * 3);
    assert_eq!(lod1.occlusion().len(), 64 * 48);
    assert!(
        lod1.normal_rgb()
            .chunks_exact(3)
            .any(|pixel| pixel != [127, 127, 255])
    );
    assert!(lod1.occlusion().iter().any(|value| *value < u8::MAX));
    assert_ne!(lod1.normal_rgb(), lod2.normal_rgb());
    assert!(island.surface_maps(3, 8, 8).is_none());
}

#[test]
fn rivers_are_continuous_flowing_terrain_submeshes_with_waterfalls() {
    let island = Island::generate(666, small_options()).unwrap();
    assert!(!island.rivers().is_empty());
    let terrain = island.terrain().mesh();
    let adjacency = terrain.adjacency();
    let corner = terrain
        .vertices
        .iter()
        .enumerate()
        .min_by(|(_, left), (_, right)| {
            left.x
                .total_cmp(&right.x)
                .then_with(|| left.y.total_cmp(&right.y))
        })
        .map(|(vertex, _)| vertex)
        .unwrap();
    let mut ocean = vec![false; terrain.vertices.len()];
    let mut fringe = vec![corner];
    ocean[corner] = true;
    while let Some(vertex) = fringe.pop() {
        for &neighbour in &adjacency[vertex] {
            if !ocean[neighbour] && terrain.vertices[neighbour].z < 0.0 {
                ocean[neighbour] = true;
                fringe.push(neighbour);
            }
        }
    }
    assert!(island.rivers().iter().all(|river| {
        river.join.is_some() || river.nodes.last().is_some_and(|node| ocean[node.vertex])
    }));
    let total_nodes: usize = island.rivers().iter().map(|river| river.nodes.len()).sum();
    let total_segments: usize = island
        .rivers()
        .iter()
        .map(|river| river.nodes.len().saturating_sub(1))
        .sum();
    assert!(island.river_mesh().vertices.len() >= total_nodes);
    assert_eq!(
        island.river_mesh().uv.len(),
        island.river_mesh().vertices.len()
    );
    assert_river_mesh_bank_distance_uv(&island);
    assert!(island.river_mesh().triangles.len() > total_segments * 3);
    assert!(
        island
            .river_mesh()
            .triangles
            .iter()
            .all(|&vertex| (vertex as usize) < island.river_mesh().vertices.len())
    );
    assert_river_mesh_bank_and_centreline_clearance(&island);
    let unique_xy: HashSet<(u32, u32)> = island
        .river_mesh()
        .vertices
        .iter()
        .map(|vertex| (vertex.x.to_bits(), vertex.y.to_bits()))
        .collect();
    assert_eq!(unique_xy.len(), island.river_mesh().vertices.len());
    assert!(island.rivers().iter().any(|river| river.join.is_some()));
    let mut flat_segments = 0_usize;
    let mut substantial_drops = Vec::new();
    for (river_index, river) in island.rivers().iter().enumerate() {
        for (node_index, pair) in river.nodes.windows(2).enumerate() {
            let drop = pair[0].surface - pair[1].surface;
            if drop.abs() < 1.0e-7 {
                flat_segments += 1;
            } else if drop >= 0.001 {
                substantial_drops.push(drop);
            }
            assert!(
                pair[0].surface + 1.0e-6 >= pair[1].surface,
                "river {river_index} surface rises at node {node_index}: {} -> {}",
                pair[0].surface,
                pair[1].surface
            );
            assert!(
                pair[0].flow <= pair[1].flow,
                "river {river_index} flow falls at node {node_index}: {} -> {}",
                pair[0].flow,
                pair[1].flow
            );
        }
    }
    assert!(flat_segments > total_segments / 3);
    let smallest_drop = substantial_drops
        .iter()
        .copied()
        .fold(f32::INFINITY, f32::min);
    let largest_drop = substantial_drops.iter().copied().fold(0.0_f32, f32::max);
    assert!(substantial_drops.len() > 2);
    assert!(mapped_waterfall_uv_segments(&island) > 2);
    assert!(largest_drop > smallest_drop * 1.1);
    assert!(
        island
            .rivers()
            .iter()
            .flat_map(|river| &river.nodes)
            .any(|node| { node.position.z + 1.0e-6 < node.surface })
    );
}

#[test]
fn png_writer_emits_png_signature() {
    let island = Island::generate(29, small_options()).unwrap();
    let raster = island.render(32, 24);
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!("island-rs-{unique}.png"));
    write_png(&path, raster.width(), raster.height(), raster.pixels()).unwrap();
    let bytes = fs::read(&path).unwrap();
    fs::remove_file(path).unwrap();
    assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n");
}

#[test]
fn save_and_load_regenerates_identical_island() {
    let island = Island::generate(
        31,
        IslandOptions {
            hydraulic_erosion_strength: 1.75,
            hydraulic_deposition_strength: 2.25,
            hydraulic_deposition_slope_degrees: 18.0,
            river_source_catchment_hectares: 0.75,
            river_source_steep_multiplier: 5.0,
            river_source_elevation_boost: 8.5,
            ..small_options()
        },
    )
    .unwrap();
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!("island-rs-{unique}.motu"));
    island.save(&path).unwrap();
    let loaded = Island::load(&path).unwrap();
    fs::remove_file(path).unwrap();
    assert_eq!(island, loaded);
    assert_eq!(
        loaded.options().river_source_elevation_boost.to_bits(),
        8.5_f32.to_bits()
    );
}

#[test]
fn version_ten_save_uses_new_river_source_defaults() {
    let defaults = IslandOptions::default();
    let mut bytes = b"MOTURS\0\x0a".to_vec();
    bytes.extend(77_u64.to_le_bytes());
    for value in [
        0.2_f32, 0.6, 1.3, 1.0, 1.0, 1.0, 0.0, 1.5, 12.0, 0.35, 0.65, 1.0, 1.3, 1.6,
    ] {
        bytes.extend(value.to_le_bytes());
    }
    bytes.extend(24_u32.to_le_bytes());
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!("island-rs-v10-{unique}.motu"));
    fs::write(&path, bytes).unwrap();

    let loaded = Island::load(&path).unwrap();

    fs::remove_file(path).unwrap();
    assert_eq!(loaded.seed(), 77);
    assert_eq!(
        loaded.options().river_source_catchment_hectares.to_bits(),
        defaults.river_source_catchment_hectares.to_bits()
    );
    assert_eq!(
        loaded.options().river_source_steep_multiplier.to_bits(),
        defaults.river_source_steep_multiplier.to_bits()
    );
    assert_eq!(
        loaded.options().river_source_elevation_boost.to_bits(),
        defaults.river_source_elevation_boost.to_bits()
    );
}

#[test]
fn version_thirteen_fraction_is_migrated_to_an_absolute_land_area() {
    let mut bytes = b"MOTURS\0\x0d".to_vec();
    bytes.extend(78_u64.to_le_bytes());
    for value in [0.2_f32, 0.95, 1.3, 1.0, 1.0, 1.5, 12.0, 0.002, 4.0, 5.0] {
        bytes.extend(value.to_le_bytes());
    }
    bytes.extend(24_u32.to_le_bytes());
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!("island-rs-v13-{unique}.motu"));
    fs::write(&path, bytes).unwrap();

    let loaded = Island::load(&path).unwrap();

    fs::remove_file(path).unwrap();
    assert_eq!(loaded.seed(), 78);
    assert!((loaded.options().river_source_catchment_hectares - 0.04).abs() < 1.0e-6);
    assert_eq!(
        loaded.options().river_source_elevation_boost.to_bits(),
        IslandOptions::default()
            .river_source_elevation_boost
            .to_bits()
    );
}

#[test]
fn version_fourteen_minimum_elevation_is_replaced_by_the_default_boost() {
    let mut bytes = b"MOTURS\0\x0e".to_vec();
    bytes.extend(79_u64.to_le_bytes());
    for value in [0.2_f32, 0.95, 1.3, 1.0, 1.0, 1.5, 12.0, 0.05, 4.0, 75.0] {
        bytes.extend(value.to_le_bytes());
    }
    bytes.extend(24_u32.to_le_bytes());
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!("island-rs-v14-{unique}.motu"));
    fs::write(&path, bytes).unwrap();

    let loaded = Island::load(&path).unwrap();
    fs::remove_file(path).unwrap();

    assert_eq!(
        loaded.options().river_source_elevation_boost.to_bits(),
        IslandOptions::default()
            .river_source_elevation_boost
            .to_bits()
    );
}
