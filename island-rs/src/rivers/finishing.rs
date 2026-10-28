//! Final channel constraints ported from Planet, expressed in Island's XY/Z
//! coordinates. Hydraulic station coordinates survive refinement and shaping.
use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};

use super::{
    Mesh, RIVER_BOUNDARY, RiverNetwork, SurfaceMaterial, WaterfallPatch,
    WaterfallTerrainConstraints,
    geometry::{RiverMeshBuffers, river_bank_edges},
    is_river_boundary, mark_river_boundary,
    surface::water_height,
};

pub(super) fn align_waterfall_corridors(
    terrain: &Mesh,
    buffers: &mut RiverMeshBuffers,
    patches: &[WaterfallPatch],
) {
    for (i, position) in terrain.vertices.iter().enumerate() {
        let Some(owner) = buffers.owners[i] else {
            continue;
        };
        let patch = patches
            .iter()
            .filter(|patch| {
                let along = patch.signed_distance_to_face_plane(position.truncate());
                patch.owns_local_channel(owner)
                    && along >= -WaterfallPatch::face_run() - super::WATERFALL_EDGE_BLEND_RUN
                    && along <= patch.downstream_extent()
            })
            .min_by(|a, b| {
                a.local_coordinates(position.truncate())
                    .1
                    .abs()
                    .total_cmp(&b.local_coordinates(position.truncate()).1.abs())
            });
        let Some(patch) = patch else {
            continue;
        };
        let (distance, width) = patch.corridor_at(
            position.truncate(),
            buffers.river_uv[i].x,
            buffers.target_half_widths[i],
        );
        buffers.river_uv[i].x = distance;
        buffers.target_half_widths[i] = width;
        // Keep the inherited topology on the shoulders; use one constant span
        // throughout the face. The next refinement adds detail to this outline.
        if patch.plane_zone(position.truncate()) == super::waterfalls::WaterfallPlaneZone::Face {
            buffers.coverage[i] = u8::from(distance.abs() <= width * 1.25) * 2;
        }
    }
    for coverage in &mut buffers.coverage {
        *coverage &= !RIVER_BOUNDARY;
    }
    mark_river_boundary(
        &terrain.adjacency(),
        &terrain.perimeter_mask(),
        &mut buffers.coverage,
    );
}

/// A sea-level channel is a submerged opening, even when topology calls its
/// terminal cap a bank. Its carved cross-section is a ceiling for every pass.
pub(super) fn protect_sea_outlets(
    terrain: &mut Mesh,
    material: &mut SurfaceMaterial,
    buffers: &RiverMeshBuffers,
    constraints: &mut WaterfallTerrainConstraints,
) -> usize {
    let mut carved = 0;
    for (i, position) in terrain.vertices.iter_mut().enumerate() {
        let width = buffers.target_half_widths[i];
        let surface = buffers.surfaces[i];
        if buffers.coverage[i] == 0 || width <= 0.0 || !surface.is_finite() || surface > 0.0 {
            continue;
        }
        let lateral = buffers.river_uv[i].x.abs() / width;
        if lateral >= 1.0 {
            continue;
        }
        let depth = buffers.target_depths[i].max(super::SEA_PLANE_CLEARANCE);
        let ceiling = surface - depth * (1.0 - lateral.powi(4));
        constraints.terrain_ceiling[i] =
            constraints.terrain_ceiling[i].min(position.z.min(ceiling));
        constraints.patch[i] = true;
        constraints.water_unclamped[i] = true;
        if position.z > ceiling {
            material.depths_mut()[i] = (material.depths()[i] - (position.z - ceiling)).max(0.0);
            position.z = ceiling;
            material.submerged_river_carves_mut()[i] = 1.0;
            carved += 1;
        }
    }
    carved
}

pub(super) fn outlet_ids(network: &RiverNetwork) -> Vec<usize> {
    (0..network.rivers.len())
        .map(|start| {
            let mut current = start;
            for _ in 0..network.rivers.len() {
                let Some(next) = network.rivers[current].join else {
                    break;
                };
                if next >= network.rivers.len() || next == current {
                    break;
                }
                current = next;
            }
            current
        })
        .collect()
}

fn wet_outline(terrain: &Mesh, buffers: &RiverMeshBuffers) -> Vec<[u32; 2]> {
    let wet: Vec<_> = terrain
        .vertices
        .iter()
        .enumerate()
        .map(|(i, position)| {
            let water = water_height(
                position.z,
                buffers.surfaces[i],
                terrain.normals[i],
                is_river_boundary(buffers.coverage[i]),
            );
            buffers.coverage[i] != 0 && water > 0.0 && water - position.z > 1.0e-9
        })
        .collect();
    let mut edges = BTreeMap::<(u32, u32), usize>::new();
    for face in terrain.triangles.chunks_exact(3) {
        if !face.iter().any(|&i| wet[i as usize]) {
            continue;
        }
        for [a, b] in [[face[0], face[1]], [face[1], face[2]], [face[2], face[0]]] {
            if !wet[a as usize] && !wet[b as usize] {
                *edges.entry((a.min(b), a.max(b))).or_default() += 1;
            }
        }
    }
    edges
        .into_iter()
        .filter_map(|((a, b), count)| (count == 1).then_some([a, b]))
        .collect()
}

/// Walk both the authored banks and every newly exposed wet outline from sea
/// toward the source. Accumulate constraints as lifting reveals new contacts.
pub(super) fn grade_upstream_banks(
    terrain: &mut Mesh,
    network: &RiverNetwork,
    buffers: &RiverMeshBuffers,
    patches: &[WaterfallPatch],
) -> usize {
    let outlets = outlet_ids(network);
    let stations = super::contact_banks::bank_stations(terrain, buffers, patches);
    let mut retained = BTreeSet::new();
    let mut upstream = HashMap::<usize, Vec<usize>>::new();
    let mut total = 0;
    loop {
        terrain.calculate_normals();
        let edges = river_bank_edges(terrain, &buffers.coverage)
            .into_iter()
            .chain(wet_outline(terrain, buffers));
        let mut queue = VecDeque::new();
        for [a, b] in edges {
            let [a, b] = [a as usize, b as usize];
            if terrain.vertices[a].z <= 0.0 || terrain.vertices[b].z <= 0.0 {
                continue;
            }
            let (Some(first), Some(second)) = (buffers.owners[a], buffers.owners[b]) else {
                continue;
            };
            if outlets.get(first.river as usize) != outlets.get(second.river as usize) {
                continue;
            }
            let delta = stations[a] - stations[b];
            if delta.abs() <= 1.0e-8 {
                continue;
            }
            let (down, up) = if delta > 0.0 { (a, b) } else { (b, a) };
            if retained.insert((down, up)) {
                upstream.entry(down).or_default().push(up);
                queue.push_back(down);
            }
        }
        let mut raised = 0;
        while let Some(down) = queue.pop_front() {
            let Some(feeders) = upstream.get(&down) else {
                continue;
            };
            for &up in feeders {
                let target = terrain.vertices[down].z;
                if terrain.vertices[up].z < target {
                    terrain.vertices[up].z = target;
                    queue.push_back(up);
                    raised += 1;
                }
            }
        }
        total += raised;
        if raised == 0 {
            break;
        }
    }
    total
}

#[cfg(test)]
mod tests {
    use super::super::{
        River, RiverOwnerKey, WATERFALL_WATER_CLEARANCE,
        geometry::{RiverBankLiftMasks, RiverMeshAttributes, lift_river_banks_to_surface},
        waterfalls::{smooth_final_waterfall_patches, smooth_pinned_waterfall_terrain},
    };
    use super::*;
    use crate::{Vec2, Vec3};

    fn fixture() -> (Mesh, RiverMeshBuffers, RiverNetwork, WaterfallPatch) {
        let step = 1.0 / super::super::ISLAND_WORLD_METRES;
        let mut terrain = Mesh::default();
        for y in 0..9 {
            for x in 0..15 {
                let point = Vec2::new((x as f32 - 10.0) * step, (y as f32 - 4.0) * step);
                terrain
                    .vertices
                    .push(point.extend(if x >= 11 { -step } else { step * 4.0 }));
                terrain.uv.push(point);
            }
        }
        for y in 0..8 {
            for x in 0..14 {
                let a = y * 15 + x;
                terrain
                    .triangles
                    .extend([a, a + 1, a + 15, a + 15, a + 1, a + 16]);
            }
        }
        let mut coverage: Vec<_> = terrain
            .vertices
            .iter()
            .map(|p| u8::from(p.y.abs() <= step * 2.1) * 2)
            .collect();
        mark_river_boundary(
            &terrain.adjacency(),
            &terrain.perimeter_mask(),
            &mut coverage,
        );
        let patch = WaterfallPatch {
            river: 0,
            segment: 1,
            upper_vertex: 4 * 15 + 10,
            upper_centre: Vec2::ZERO,
            direction: Vec2::X,
            across: Vec2::Y,
            upper_surface: step * 4.0,
            lower_surface: 0.0,
            lower_floor: -step,
            half_width: step * 2.5,
            support_run: step,
            pool: None,
        };
        let surfaces: Vec<_> = terrain
            .vertices
            .iter()
            .map(|p| {
                patch.face_surface_at(p.truncate()).unwrap_or(if p.x > 0.0 {
                    0.0
                } else {
                    patch.upper_surface
                })
            })
            .collect();
        let buffers = RiverMeshBuffers::new(
            coverage,
            RiverMeshAttributes {
                surfaces,
                uv: terrain
                    .vertices
                    .iter()
                    .map(|p| Vec2::new(p.y, p.x))
                    .collect(),
                owners: vec![Some(RiverOwnerKey { river: 0, node: 1 }); terrain.vertices.len()],
                waterfall_lips: vec![false; terrain.vertices.len()],
                target_half_widths: vec![patch.half_width; terrain.vertices.len()],
                target_depths: vec![step; terrain.vertices.len()],
            },
        );
        let network = RiverNetwork {
            rivers: vec![River {
                nodes: Vec::new(),
                join: None,
            }],
            join_vertices: vec![None],
            waterfalls: vec![Vec::new()],
            river_mesh_ends: vec![None],
            max_flow: 1,
            max_height: 0.01,
            ocean: terrain.vertices.iter().map(|p| p.z <= 0.0).collect(),
            perimeter: terrain.perimeter_mask(),
            cross_sections: vec![Vec::new()],
        };
        (terrain, buffers, network, patch)
    }

    fn constraints(count: usize) -> WaterfallTerrainConstraints {
        WaterfallTerrainConstraints {
            patch: vec![false; count],
            pinned: vec![false; count],
            support: vec![false; count],
            water_unclamped: vec![false; count],
            terrain_ceiling: vec![f32::INFINITY; count],
        }
    }

    #[test]
    fn sea_outlet_keeps_its_channel_depth_through_all_lifts_and_smoothing() {
        let (mut terrain, mut buffers, network, patch) = fixture();
        let count = terrain.vertices.len();
        let mut constraints = constraints(count);
        let mut material = SurfaceMaterial::empty(count);
        let mouth = 4 * 15 + 10;
        assert_eq!(buffers.surfaces[mouth].to_bits(), 0.0_f32.to_bits());
        assert!(terrain.vertices[mouth].z > 0.0);
        assert!(protect_sea_outlets(&mut terrain, &mut material, &buffers, &mut constraints) > 0);
        let bed = terrain.vertices[mouth].z;
        assert!(bed <= -buffers.target_depths[mouth]);
        assert!(bed < -WATERFALL_WATER_CLEARANCE);
        assert!((material.submerged_river_carves().unwrap()[mouth] - 1.0).abs() < f32::EPSILON);
        let adjacency = terrain.adjacency();
        lift_river_banks_to_surface(
            &mut terrain,
            &adjacency,
            &buffers.coverage,
            &buffers.surfaces,
            &buffers.target_half_widths,
            RiverBankLiftMasks {
                ocean: &network.ocean,
                perimeter: &network.perimeter,
                protected: &constraints.patch,
            },
        );
        smooth_pinned_waterfall_terrain(
            &mut terrain,
            &mut buffers.surfaces,
            &[patch],
            &buffers.coverage,
            &buffers.owners,
        );
        for _ in 0..2 {
            smooth_final_waterfall_patches(
                &mut terrain,
                &mut buffers.surfaces,
                &[patch],
                &buffers.coverage,
                &buffers.owners,
            );
        }
        grade_upstream_banks(&mut terrain, &network, &buffers, &[patch]);
        assert_eq!(terrain.vertices[mouth].z.to_bits(), bed.to_bits());
        assert!(water_height(bed, buffers.surfaces[mouth], Vec3::X, false) <= 0.0);
    }

    #[test]
    fn whole_corridor_smoothing_reaches_both_sides_of_the_waterfall_without_moving_xy() {
        let (mut terrain, mut buffers, _, patch) = fixture();
        let before = 4 * 15 + 2;
        let landing = 4 * 15 + 9;
        terrain.vertices[before].z += 0.003;
        terrain.vertices[landing].z += 0.003;
        let before_height = terrain.vertices[before].z;
        let landing_height = terrain.vertices[landing].z;
        let xy: Vec<_> = terrain.vertices.iter().map(|p| p.truncate()).collect();
        let hydraulic: Vec<_> = buffers.surfaces.iter().map(|v| v.to_bits()).collect();
        smooth_final_waterfall_patches(
            &mut terrain,
            &mut buffers.surfaces,
            &[patch],
            &buffers.coverage,
            &buffers.owners,
        );
        assert!(terrain.vertices[before].z < before_height);
        assert!(terrain.vertices[landing].z < landing_height);
        assert!(buffers.surfaces.iter().map(|v| v.to_bits()).eq(hydraulic));
        assert!(
            terrain
                .vertices
                .iter()
                .zip(xy)
                .all(|(p, xy)| p.truncate() == xy)
        );
    }

    #[test]
    fn upstream_bank_walk_repairs_inserted_dips_without_closing_the_sea() {
        let (mut terrain, mut buffers, network, _) = fixture();
        for (i, p) in terrain.vertices.iter_mut().enumerate() {
            if p.z > 0.0 {
                p.z = if i % 2 == 0 { 0.002 } else { 0.004 };
            }
            buffers.surfaces[i] = if p.z > 0.0 { 0.005 } else { 0.0 };
        }
        let sea = terrain.vertices[4 * 15 + 12];
        assert!(grade_upstream_banks(&mut terrain, &network, &buffers, &[]) > 0);
        for [a, b] in river_bank_edges(&terrain, &buffers.coverage) {
            let [a, b] = [a as usize, b as usize];
            if terrain.vertices[a].z <= 0.0 || terrain.vertices[b].z <= 0.0 {
                continue;
            }
            let delta = buffers.river_uv[a].y - buffers.river_uv[b].y;
            if delta > 1.0e-8 {
                assert!(terrain.vertices[b].z >= terrain.vertices[a].z);
            }
            if delta < -1.0e-8 {
                assert!(terrain.vertices[a].z >= terrain.vertices[b].z);
            }
        }
        assert_eq!(terrain.vertices[4 * 15 + 12], sea);
    }

    #[test]
    fn lateral_frame_keeps_the_crest_and_foot_aligned_while_leaving_a_sloping_drop() {
        let (_, _, _, patch) = fixture();
        for lateral in [-patch.half_width, 0.0, patch.half_width] {
            let crest = patch.upper_centre - patch.direction * WaterfallPatch::face_run()
                + patch.across * lateral;
            let foot = patch.upper_centre + patch.across * lateral;
            let top = patch.corridor_at(crest, lateral * 0.5, patch.half_width * 1.5);
            let bottom = patch.corridor_at(foot, lateral * 1.5, patch.half_width * 0.5);
            assert!((top.0 - bottom.0).abs() < 1.0e-8);
            assert!((top.1 - bottom.1).abs() < 1.0e-8);
            assert!(crest.distance(foot) > 0.0);
        }
    }
    #[test]
    fn alignment_fades_through_the_whole_upstream_shoulder() {
        let (terrain, mut buffers, _, patch) = fixture();
        buffers.target_half_widths.fill(patch.half_width * 2.0);
        let outer = 5 * 15 + 5;
        let shoulder = 5 * 15 + 6;
        let natural_width = buffers.target_half_widths[shoulder];
        align_waterfall_corridors(&terrain, &mut buffers, &[patch]);
        assert_eq!(
            buffers.target_half_widths[outer].to_bits(),
            natural_width.to_bits()
        );
        assert!(buffers.target_half_widths[shoulder] < natural_width);
        assert!(buffers.target_half_widths[shoulder] > patch.half_width);
    }

    #[test]
    fn waterfall_placement_uses_the_incoming_heading_at_a_bend() {
        let (terrain, _, mut network, _) = fixture();
        network.rivers[0].nodes = [4 * 15 + 7, 4 * 15 + 10, 5 * 15 + 12]
            .into_iter()
            .enumerate()
            .map(|(i, vertex)| super::super::RiverNode {
                vertex,
                flow: 1,
                surface: if i < 2 { 0.002 } else { 0.0 },
                position: terrain.vertices[vertex],
            })
            .collect();
        network.waterfalls[0] = vec![false, true, false];
        let patches = super::super::waterfalls::derive_waterfall_patches(&network, &terrain);
        assert_eq!(patches.len(), 1);
        assert!(patches[0].direction.distance(Vec2::X) < 1.0e-6);
        let outgoing = (network.rivers[0].nodes[2].position - network.rivers[0].nodes[1].position)
            .truncate()
            .normalize();
        assert!(outgoing.distance(patches[0].direction) > 0.1);
    }
}
