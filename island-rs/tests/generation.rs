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
fn coastal_erosion_strength_changes_terrain() {
    let without_coast = Island::generate(
        12,
        IslandOptions {
            coastal_erosion_strength: 0.0,
            beach_formation_strength: 0.0,
            ..small_options()
        },
    )
    .unwrap();
    let with_coast = Island::generate(
        12,
        IslandOptions {
            coastal_erosion_strength: 2.0,
            beach_formation_strength: 1.0,
            ..small_options()
        },
    )
    .unwrap();
    assert_ne!(
        without_coast.terrain().vertices(),
        with_coast.terrain().vertices()
    );
    assert!(with_coast.terrain().vertex_count() > without_coast.terrain().vertex_count());
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
fn river_source_thresholds_change_river_generation() {
    let many_sources = Island::generate(
        53,
        IslandOptions {
            river_lod2_source_threshold: 0.0,
            river_lod1_source_threshold: 0.0,
            river_broad_source_threshold: 0.0,
            river_land_source_threshold: 0.0,
            river_final_source_threshold: 0.0,
            ..small_options()
        },
    )
    .unwrap();
    let few_sources = Island::generate(
        53,
        IslandOptions {
            river_lod2_source_threshold: 16.0,
            river_lod1_source_threshold: 16.0,
            river_broad_source_threshold: 16.0,
            river_land_source_threshold: 16.0,
            river_final_source_threshold: 16.0,
            ..small_options()
        },
    )
    .unwrap();

    assert_ne!(many_sources.rivers(), few_sources.rivers());
    assert_ne!(many_sources.terrain(), few_sources.terrain());
}

#[test]
fn native_options_are_not_limited_to_viewer_control_ranges() {
    let island = Island::generate(
        1,
        IslandOptions {
            water_ratio: 0.5,
            river_final_source_threshold: 32.0,
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
        coastal_erosion_strength: 0.0,
        beach_formation_strength: 0.0,
        noise_multiplier: 0.0,
        river_lod2_source_threshold: 16.0,
        river_lod1_source_threshold: 16.0,
        river_broad_source_threshold: 16.0,
        river_land_source_threshold: 16.0,
        river_final_source_threshold: 16.0,
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
    // The coastal stage adds one conforming refinement ring around the actual
    // sea-level contour, while inland and deep-sea faces remain adaptive.
    assert!(
        lod1.triangles.len() < coarse.triangles.len() * 48,
        "coastal refinement grew LOD1 from {} to {} indices",
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
    assert_eq!(island.height_map(31, 17).len(), 31 * 17);
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
    assert!(island.river_mesh().triangles.len() > total_segments * 3);
    assert!(
        island
            .river_mesh()
            .triangles
            .iter()
            .all(|&vertex| (vertex as usize) < island.river_mesh().vertices.len())
    );
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
            coastal_erosion_strength: 2.0,
            beach_formation_strength: 3.0,
            river_lod2_source_threshold: 0.5,
            river_lod1_source_threshold: 0.8,
            river_broad_source_threshold: 1.1,
            river_land_source_threshold: 1.4,
            river_final_source_threshold: 1.9,
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
}
