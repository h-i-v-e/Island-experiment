//! Grade the visible clipped contact, rather than only its outer terrain bank.
//! Freeze the inclination blend before splitting/lifting so changed normals
//! cannot recreate dips or move the water away from its supporting terrain.
use std::collections::{BTreeMap, HashMap, VecDeque};

use super::geometry::visible_river_water_coverage;
use super::{
    Mesh, RiverNetwork, SurfaceMaterial, WaterfallPatch, geometry::RiverMeshBuffers, surface,
};

const CONTACT_EPSILON: f32 = 1.0e-9;

pub(super) fn finish(
    terrain: &mut Mesh,
    material: &mut SurfaceMaterial,
    network: &RiverNetwork,
    buffers: &mut RiverMeshBuffers,
    patches: &[WaterfallPatch],
) -> Mesh {
    terrain.calculate_normals();
    let mut visible =
        visible_river_water_coverage(network, terrain, &buffers.coverage, &buffers.owners);
    let mut water = surface::water_heights(terrain, &visible, &buffers.surfaces);
    let mut stations = bank_stations(terrain, buffers, patches);
    insert_contacts(
        terrain,
        material,
        buffers,
        &mut visible,
        &mut water,
        &mut stations,
    );
    grade_contacts(terrain, network, buffers, &visible, &mut water, &stations);
    terrain.calculate_normals();
    surface::build_with_heights(terrain, &visible, &water, &buffers.river_uv)
}

pub(super) fn bank_stations(
    terrain: &Mesh,
    buffers: &RiverMeshBuffers,
    patches: &[WaterfallPatch],
) -> Vec<f32> {
    let mut by_river = HashMap::<u32, Vec<&WaterfallPatch>>::new();
    for patch in patches {
        by_river.entry(patch.river).or_default().push(patch);
    }
    terrain
        .vertices
        .iter()
        .enumerate()
        .map(|(i, position)| {
            let Some(owner) = buffers.owners[i] else {
                return buffers.river_uv[i].y;
            };
            let patch = by_river
                .get(&owner.river)
                .into_iter()
                .flatten()
                .copied()
                .filter(|patch| {
                    let (along, lateral) = patch.local_coordinates(position.truncate());
                    patch.owns_local_channel(owner)
                        && along >= -WaterfallPatch::face_run() - super::WATERFALL_EDGE_BLEND_RUN
                        && along <= patch.downstream_extent()
                        && lateral.abs() <= patch.half_width * 2.5
                })
                .min_by(|a, b| {
                    a.local_coordinates(position.truncate())
                        .1
                        .abs()
                        .total_cmp(&b.local_coordinates(position.truncate()).1.abs())
                });
            patch.map_or(buffers.river_uv[i].y, |patch| {
                buffers.river_uv[patch.upper_vertex].y
                    + patch.signed_distance_to_face_plane(position.truncate())
            })
        })
        .collect()
}

fn clearance(water: f32, bed: f32) -> f32 {
    let gap = water - bed;
    if gap.abs() < CONTACT_EPSILON {
        0.0
    } else {
        gap
    }
}

fn insert_contacts(
    terrain: &mut Mesh,
    material: &mut SurfaceMaterial,
    buffers: &mut RiverMeshBuffers,
    visible: &mut Vec<u8>,
    water: &mut Vec<f32>,
    stations: &mut Vec<f32>,
) {
    let mut fractions = BTreeMap::new();
    for face in terrain.triangles.chunks_exact(3) {
        if face.iter().any(|&i| visible[i as usize] == 0)
            || face.iter().all(|&i| water[i as usize] <= 0.0)
        {
            continue;
        }
        for [a, b] in [[face[0], face[1]], [face[1], face[2]], [face[2], face[0]]] {
            let (a, b) = (a.min(b), a.max(b));
            let [first, second] =
                [a, b].map(|i| clearance(water[i as usize], terrain.vertices[i as usize].z));
            if (first > 0.0 && second < 0.0) || (first < 0.0 && second > 0.0) {
                let t = first / (first - second);
                if t > 0.0 && t < 1.0 {
                    fractions.insert((a, b), t);
                }
            }
        }
    }
    let splits = terrain.split_edges(&fractions);
    material.extend_after_edge_splits(&splits);
    buffers.extend_after_edge_splits(&splits);
    for split in &splits {
        let [a, b] = split.edge.map(|i| i as usize);
        visible.push(visible[a].min(visible[b]));
        // A clipped bank lies on the original planar terrain edge. Make its
        // height exactly shared, avoiding a second near-endpoint intersection.
        water.push(terrain.vertices[split.vertex as usize].z);
        stations.push((stations[b] - stations[a]).mul_add(split.interpolation, stations[a]));
    }
    for (height, bed) in water.iter_mut().zip(&terrain.vertices) {
        if clearance(*height, bed.z) == 0.0 {
            *height = bed.z;
        }
    }
}

fn dry_edges(terrain: &Mesh, visible: &[u8], water: &[f32]) -> Vec<(usize, usize)> {
    let mut edges = BTreeMap::<(u32, u32), usize>::new();
    for face in terrain.triangles.chunks_exact(3) {
        if face.iter().any(|&i| visible[i as usize] == 0)
            || !face
                .iter()
                .any(|&i| clearance(water[i as usize], terrain.vertices[i as usize].z) > 0.0)
        {
            continue;
        }
        let [a, b, c] =
            [face[0], face[1], face[2]].map(|i| terrain.vertices[i as usize].as_dvec3());
        if (b - a).cross(c - a).z <= 0.0 {
            continue;
        }
        for [a, b] in [[face[0], face[1]], [face[1], face[2]], [face[2], face[0]]] {
            *edges.entry((a.min(b), a.max(b))).or_default() += 1;
        }
    }
    edges
        .into_iter()
        .filter_map(|((a, b), count)| {
            let (a, b) = (a as usize, b as usize);
            (count == 1
                && [a, b].into_iter().all(|i| {
                    water[i] > CONTACT_EPSILON && clearance(water[i], terrain.vertices[i].z) == 0.0
                }))
            .then_some((a, b))
        })
        .collect()
}

fn grade_contacts(
    terrain: &mut Mesh,
    network: &RiverNetwork,
    buffers: &RiverMeshBuffers,
    visible: &[u8],
    water: &mut [f32],
    stations: &[f32],
) {
    let outlets = super::finishing::outlet_ids(network);
    let mut upstream = HashMap::<usize, Vec<usize>>::new();
    let mut queue = VecDeque::new();
    for (a, b) in dry_edges(terrain, visible, water) {
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
        upstream.entry(down).or_default().push(up);
        queue.push_back(down);
    }
    while let Some(down) = queue.pop_front() {
        for &up in upstream.get(&down).into_iter().flatten() {
            let lift = terrain.vertices[down].z - terrain.vertices[up].z;
            if lift > 0.0 {
                terrain.vertices[up].z = terrain.vertices[down].z;
                // Preserve the frozen clearance, including the 3cm face gap.
                water[up] = terrain.vertices[up].z;
                queue.push_back(up);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::{
        River, RiverOwnerKey, WATERFALL_WATER_CLEARANCE, geometry::RiverMeshAttributes,
    };
    use super::*;
    use crate::{Vec2, Vec3};

    fn strip() -> (Mesh, RiverMeshBuffers, RiverNetwork) {
        let mut terrain = Mesh::default();
        for row in 0..7 {
            for col in 0..5 {
                let height = if row == 0 || row == 6 {
                    -0.005
                } else if col == 2 {
                    0.02
                } else {
                    0.025
                };
                let point = Vec3::new(row as f32 * 0.001, (col as f32 - 2.0) * 0.001, height);
                terrain.vertices.push(point);
                terrain.uv.push(point.truncate());
            }
        }
        for row in 0..6 {
            for col in 0..4 {
                let a = row * 5 + col;
                terrain
                    .triangles
                    .extend([a, a + 5, a + 1, a + 1, a + 5, a + 6]);
            }
        }
        terrain.calculate_normals();
        let count = terrain.vertices.len();
        let buffers = RiverMeshBuffers::new(
            vec![2; count],
            RiverMeshAttributes {
                surfaces: vec![0.021; count],
                uv: terrain
                    .vertices
                    .iter()
                    .map(|p| Vec2::new(p.y, p.x))
                    .collect(),
                owners: vec![Some(RiverOwnerKey { river: 0, node: 1 }); count],
                waterfall_lips: vec![false; count],
                target_half_widths: vec![0.002; count],
                target_depths: vec![0.001; count],
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
            max_height: 0.025,
            ocean: terrain.vertices.iter().map(|p| p.z <= 0.0).collect(),
            perimeter: terrain.perimeter_mask(),
            cross_sections: vec![Vec::new()],
        };
        (terrain, buffers, network)
    }

    fn surface_dips(mesh: &Mesh) -> usize {
        let mut edges = BTreeMap::<(u32, u32), usize>::new();
        for face in mesh.triangles.chunks_exact(3) {
            for [a, b] in [[face[0], face[1]], [face[1], face[2]], [face[2], face[0]]] {
                *edges.entry((a.min(b), a.max(b))).or_default() += 1;
            }
        }
        edges
            .into_iter()
            .filter(|&((a, b), count)| {
                let (a, b) = (a as usize, b as usize);
                if count != 1 || mesh.vertices[a].z.min(mesh.vertices[b].z) <= CONTACT_EPSILON {
                    return false;
                }
                let delta = mesh.uv[a].y - mesh.uv[b].y;
                let (down, up) = if delta > 0.0 { (a, b) } else { (b, a) };
                delta.abs() > 1.0e-8 && mesh.vertices[up].z + 1.0e-8 < mesh.vertices[down].z
            })
            .count()
    }

    #[test]
    fn clipped_contacts_are_shared_and_graded_without_changing_sea_or_face_clearance() {
        let (mut terrain, mut buffers, network) = strip();
        let before = terrain.vertices.clone();
        let mut material = SurfaceMaterial::empty(before.len());
        let mut visible = buffers.coverage.clone();
        let mut water: Vec<_> = terrain
            .vertices
            .iter()
            .enumerate()
            .map(|(i, p)| {
                let (row, col) = (i / 5, i % 5);
                if p.z < 0.0 || col == 0 || col == 4 {
                    p.z - 0.001
                } else if col == 2 && row == 3 {
                    p.z + WATERFALL_WATER_CLEARANCE
                } else if col == 2 {
                    0.021
                } else if row % 2 == 0 {
                    0.0249
                } else {
                    0.0216
                }
            })
            .collect();
        let old_water = water.clone();
        let mut stations: Vec<_> = buffers.river_uv.iter().map(|uv| uv.y).collect();
        let initial = surface::build_with_heights(&terrain, &visible, &water, &buffers.river_uv);
        assert!(
            surface_dips(&initial) > 0,
            "level outer banks can still have dipping clipped contacts"
        );
        insert_contacts(
            &mut terrain,
            &mut material,
            &mut buffers,
            &mut visible,
            &mut water,
            &mut stations,
        );
        grade_contacts(
            &mut terrain,
            &network,
            &buffers,
            &visible,
            &mut water,
            &stations,
        );
        let finished = surface::build_with_heights(&terrain, &visible, &water, &buffers.river_uv);
        assert_eq!(surface_dips(&finished), 0);
        assert!(terrain.vertices.len() > before.len());
        for (i, old) in before.iter().enumerate() {
            assert!(terrain.vertices[i].z >= old.z, "grading must only lift");
            if old.z < 0.0 || i % 5 == 2 {
                assert_eq!(
                    terrain.vertices[i], *old,
                    "sea openings and the thin face stay unchanged"
                );
                assert_eq!(water[i].to_bits(), old_water[i].to_bits());
            }
        }
        for i in finished.perimeter_vertices() {
            let point = finished.vertices[i];
            if point.z > CONTACT_EPSILON {
                assert!(
                    terrain.vertices.contains(&point),
                    "dry water contacts must be shared terrain vertices: {point:?}"
                );
            }
        }
        let perimeter = terrain.perimeter_mask();
        for (i, &edge) in perimeter.iter().enumerate().skip(before.len()) {
            assert!(
                !edge,
                "an internal split must not open a terrain seam at {i}"
            );
        }
        for face in terrain.triangles.chunks_exact(3) {
            let [a, b, c] =
                [face[0], face[1], face[2]].map(|i| terrain.vertices[i as usize].as_dvec3());
            assert!(
                (b - a).cross(c - a).z > 0.0,
                "contact splits must preserve the projected winding"
            );
        }
        for count in [
            material.depths().len(),
            buffers.coverage.len(),
            buffers.surfaces.len(),
            buffers.owners.len(),
            buffers.river_uv.len(),
            buffers.target_depths.len(),
            buffers.target_half_widths.len(),
            buffers.waterfall_lips.len(),
            visible.len(),
            water.len(),
            stations.len(),
        ] {
            assert_eq!(count, terrain.vertices.len());
        }
    }

    #[test]
    fn final_builder_grades_the_inclination_blended_contact() {
        let (mut terrain, mut buffers, network) = strip();
        for (i, height) in buffers.surfaces.iter_mut().enumerate() {
            if i % 5 != 2 {
                *height = if (i / 5) % 2 == 0 { 0.0249 } else { 0.0216 };
            }
        }
        let visible =
            visible_river_water_coverage(&network, &terrain, &buffers.coverage, &buffers.owners);
        let before = surface::build(&terrain, &visible, &buffers.surfaces, &buffers.river_uv);
        assert!(surface_dips(&before) > 0);
        let mut material = SurfaceMaterial::empty(terrain.vertices.len());
        let finished = finish(&mut terrain, &mut material, &network, &mut buffers, &[]);
        assert_eq!(surface_dips(&finished), 0);
        assert!(!finished.triangles.is_empty());
    }

    #[test]
    fn waterfall_banks_use_one_flow_axis_without_changing_animation_uvs_or_other_branches() {
        let (mut terrain, mut buffers, _) = strip();
        let patch = WaterfallPatch {
            river: 0,
            segment: 1,
            upper_vertex: 17,
            upper_centre: Vec2::new(0.003, 0.0),
            direction: Vec2::X,
            across: Vec2::Y,
            upper_surface: 0.025,
            lower_surface: 0.01,
            lower_floor: 0.009,
            half_width: 0.002,
            support_run: 0.001,
            pool: None,
        };
        buffers.river_uv[16].y = 0.03;
        buffers.river_uv[18].y = 0.01;
        // A different branch crossing the waterfall band keeps its station.
        buffers.owners[19] = Some(RiverOwnerKey { river: 1, node: 1 });
        terrain.vertices[19].y = 0.001;
        let stations = bank_stations(&terrain, &buffers, &[patch]);
        assert_eq!(stations[16].to_bits(), stations[18].to_bits());
        assert_eq!(stations[19].to_bits(), buffers.river_uv[19].y.to_bits());
        assert_eq!(buffers.river_uv[16].y.to_bits(), 0.03_f32.to_bits());
        assert_eq!(buffers.river_uv[18].y.to_bits(), 0.01_f32.to_bits());
    }
}
