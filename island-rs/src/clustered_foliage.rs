#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::many_single_char_names
)]

use std::{collections::BTreeMap, f32::consts::TAU};

use crate::{ISLAND_WORLD_METRES, Mesh, Vec2, Vec3, noise};

const FOLIAGE_SEED_DOMAIN: u64 = 0x636c_7573_7465_7265;
const ALPHA_EDGE_MAX_METRES: f32 = 6.0;
const ALPHA_CIRCUMRADIUS_MAX_METRES: f32 = 4.5;
const MINIMUM_HEIGHT_METRES: f32 = 2.5;
const HEIGHT_PER_SPREAD_METRES: f32 = 0.10;
const MAX_SPREAD_HEIGHT_METRES: f32 = 3.0;
const BOUNDARY_EXPANSION_METRES: f32 = 2.0;
const MID_RING_SCALE: f32 = 1.32;
const TOP_RING_SCALE: f32 = 1.16;
const TOP_TIP_CLEARANCE_METRES: f32 = 0.85;
const TOP_MAXIMUM_SLOPE: f32 = 0.48;
const TOP_SHOULDER_PROPAGATION_ITERATIONS: usize = 8;
const LOD0_DISPLACEMENT_METRES: f32 = 0.12;
const LOD0_BILLOW_METRES: f32 = 0.55;
const LOD0_SMOOTHING_ITERATIONS: usize = 4;
const LOD0_COARSE_SMOOTHING_AMOUNT: f32 = 0.12;
const LOD0_SUBDIVISION_SMOOTHING_AMOUNT: f32 = 0.42;
const CONTROL_HEIGHT_SMOOTHING_ITERATIONS: usize = 3;
const CONTROL_HEIGHT_SMOOTHING_AMOUNT: f32 = 0.45;
const TOP_HEIGHT_SMOOTHING_ITERATIONS: usize = 6;
const TOP_HEIGHT_SMOOTHING_AMOUNT: f32 = 0.50;
const DUPLICATE_SUPPORT_OFFSET_METRES: f32 = 0.01;
const ALPHA_AREA_EPSILON_METRES2: f32 = 1.0e-6;
const POINT_MERGE_EPSILON: f32 = 1.0e-7;

#[cfg(test)]
const TEST_GEOMETRY_EPSILON: f32 = 1.0e-18;

/// The two foliage streams for one spatial canopy patch.
///
/// LOD0 retains the LOD1 vertices at the beginning of its subdivided stream.
/// After smoothing, those corresponding vertices are synchronized so the two
/// levels share the rounded coarse silhouette while LOD0 keeps the additional
/// midpoint detail.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct ClusterFoliageMeshes {
    pub(crate) lod0: Mesh,
    pub(crate) lod1: Mesh,
    pub(crate) lod1_to_lod0: Vec<u32>,
}

/// One tree's trunk and terminal branch-ring support positions in a shared
/// coordinate space.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct FoliageCrown<'a> {
    pub(crate) trunk: Vec3,
    pub(crate) tips: &'a [Vec3],
}

#[derive(Clone, Copy, Debug)]
struct Support {
    position: Vec3,
    crown: usize,
}

#[derive(Clone, Copy, Debug)]
struct Sample {
    position: Vec3,
    support: usize,
}

#[derive(Clone, Copy, Debug)]
struct CrownShape {
    trunk: Vec3,
    radius: f32,
}

/// Generates the coarse control blob and derives the detailed foliage stream
/// from it.  The construction intentionally works entirely in normalized
/// island units; physical tuning constants above are converted at use sites.
pub(crate) fn generate_cluster_foliage(
    seed: u64,
    crowns: &[FoliageCrown<'_>],
) -> Result<ClusterFoliageMeshes, String> {
    validate_crowns(crowns)?;
    if crowns.is_empty() {
        return Ok(ClusterFoliageMeshes::default());
    }

    let supports = collect_supports(crowns);
    if supports.is_empty() {
        return Ok(ClusterFoliageMeshes::default());
    }

    let crown_shapes = crown_shapes(crowns, &supports);
    let samples = collect_samples(seed, &supports);
    if samples.len() < 3 {
        return Err("cluster foliage did not produce enough support samples".to_owned());
    }

    let points = delaunay_points(&samples);
    let delaunay = Mesh::delaunay(&points);
    let mut alpha_triangles = alpha_triangles(&delaunay, &samples);
    ensure_support_centres(&mut alpha_triangles, &samples, &supports);
    if alpha_triangles.is_empty() {
        return Err("cluster foliage support footprint has no valid triangles".to_owned());
    }

    let components = triangle_components(&alpha_triangles);
    let mut lod1 = Mesh::default();
    let mut support_vertices = Vec::new();
    for component in components {
        append_component(
            &mut lod1,
            seed,
            &component,
            &alpha_triangles,
            &samples,
            &supports,
            &crown_shapes,
            &mut support_vertices,
        )?;
    }
    if lod1.triangles.is_empty() {
        return Err("cluster foliage support footprint has no surface".to_owned());
    }
    support_vertices.sort_unstable();
    support_vertices.dedup();
    lod1.calculate_normals();

    let lod1_to_lod0: Vec<u32> = (0..lod1.vertices.len())
        .map(|vertex| {
            u32::try_from(vertex).map_err(|_| "cluster foliage exceeds u32 vertex capacity")
        })
        .collect::<Result<_, _>>()?;
    let tessellated = lod1.tessellated_attributed();
    let mut lod0 = tessellated.mesh;
    smooth_subdivision_vertices(&mut lod0, lod1.vertices.len(), &support_vertices);
    for (lod1_vertex, &lod0_vertex) in lod1_to_lod0.iter().enumerate() {
        lod1.vertices[lod1_vertex] = lod0.vertices[lod0_vertex as usize];
    }
    lod1.calculate_normals();
    lod0.calculate_normals();
    displace_subdivision_vertices(
        &mut lod0,
        seed,
        &tessellated.new_vertices,
        &support_vertices,
    );
    lod0.calculate_normals();

    Ok(ClusterFoliageMeshes {
        lod0,
        lod1,
        lod1_to_lod0,
    })
}

fn validate_crowns(crowns: &[FoliageCrown<'_>]) -> Result<(), String> {
    for (crown_index, crown) in crowns.iter().enumerate() {
        if !crown.trunk.is_finite() {
            return Err(format!(
                "foliage crown {crown_index} has a non-finite trunk"
            ));
        }
        for (tip_index, tip) in crown.tips.iter().enumerate() {
            if !tip.is_finite() {
                return Err(format!(
                    "foliage crown {crown_index} tip {tip_index} is non-finite"
                ));
            }
        }
    }
    Ok(())
}

fn collect_supports(crowns: &[FoliageCrown<'_>]) -> Vec<Support> {
    let mut supports = Vec::new();
    for (crown_index, crown) in crowns.iter().enumerate() {
        if crown.tips.is_empty() {
            supports.push(Support {
                position: crown.trunk,
                crown: crown_index,
            });
            continue;
        }
        supports.extend(crown.tips.iter().map(|&position| Support {
            position,
            crown: crown_index,
        }));
    }
    supports
}

fn crown_shapes(crowns: &[FoliageCrown<'_>], supports: &[Support]) -> Vec<CrownShape> {
    crowns
        .iter()
        .enumerate()
        .map(|(crown_index, crown)| {
            let spread = supports
                .iter()
                .filter(|support| support.crown == crown_index)
                .map(|support| support.position.truncate().distance(crown.trunk.truncate()))
                .fold(0.0_f32, f32::max);
            CrownShape {
                trunk: crown.trunk,
                radius: (spread + metres(1.5)).clamp(metres(2.0), metres(8.0)),
            }
        })
        .collect()
}

fn collect_samples(seed: u64, supports: &[Support]) -> Vec<Sample> {
    let mut samples = Vec::with_capacity(supports.len());

    // One projected control point per terminal branch ring is the topology
    // budget: the alpha surface is draped directly over branch tips instead
    // of first building a separate radial patch around every tip. Duplicate
    // XY projections receive a tiny deterministic offset so vertically
    // aligned tips remain independent Delaunay constraints.
    for (support_index, support) in supports.iter().enumerate() {
        let mut position = support.position;
        if let Some(previous) = samples.iter().find(|sample: &&Sample| {
            sample
                .position
                .truncate()
                .distance_squared(position.truncate())
                <= POINT_MERGE_EPSILON * POINT_MERGE_EPSILON
        }) {
            if (previous.position.z - position.z).abs() > POINT_MERGE_EPSILON {
                let angle =
                    hash_unit(seed ^ FOLIAGE_SEED_DOMAIN, support_index as u64, 0x44, 0) * TAU;
                let offset = metres(DUPLICATE_SUPPORT_OFFSET_METRES);
                position.x += offset * angle.cos();
                position.y += offset * angle.sin();
            } else {
                continue;
            }
        }
        samples.push(Sample {
            position,
            support: support_index,
        });
    }
    samples
}

/// Delaunay's fixed enclosing triangle is deliberately broad in normalized
/// island coordinates, so a tiny local canopy can lose precision against it.
/// Translate and isotropically scale the points into a compact local frame;
/// this preserves the triangulation while retaining the original samples for
/// all physical geometry and support constraints.
fn delaunay_points(samples: &[Sample]) -> Vec<Vec2> {
    let centroid = samples.iter().fold(Vec2::ZERO, |total, sample| {
        total + sample.position.truncate() * ISLAND_WORLD_METRES
    }) / samples.len() as f32;
    let extent = samples
        .iter()
        .map(|sample| (sample.position.truncate() * ISLAND_WORLD_METRES - centroid).length())
        .fold(1.0_f32, f32::max);
    samples
        .iter()
        .map(|sample| (sample.position.truncate() * ISLAND_WORLD_METRES - centroid) / extent)
        .collect()
}

fn alpha_triangles(delaunay: &Mesh, samples: &[Sample]) -> Vec<[usize; 3]> {
    let edge_limit = ALPHA_EDGE_MAX_METRES;
    let circumradius_limit = ALPHA_CIRCUMRADIUS_MAX_METRES;
    let mut triangles = Vec::new();
    for triangle in delaunay.triangles.chunks_exact(3) {
        let indices = [
            triangle[0] as usize,
            triangle[1] as usize,
            triangle[2] as usize,
        ];
        if indices.iter().any(|&index| index >= samples.len()) {
            continue;
        }
        let [a, b, c] =
            indices.map(|index| samples[index].position.truncate() * ISLAND_WORLD_METRES);
        let area_twice = (b - a).perp_dot(c - a);
        if area_twice.abs() <= ALPHA_AREA_EPSILON_METRES2 {
            continue;
        }
        let edge_lengths = [a.distance(b), b.distance(c), c.distance(a)];
        if edge_lengths.iter().any(|&length| length > edge_limit) {
            continue;
        }
        let circumradius =
            edge_lengths[0] * edge_lengths[1] * edge_lengths[2] / (2.0 * area_twice.abs());
        if circumradius > circumradius_limit {
            continue;
        }
        let oriented = if area_twice > 0.0 {
            indices
        } else {
            [indices[0], indices[2], indices[1]]
        };
        triangles.push(oriented);
    }
    triangles
}

/// Alpha filtering can remove every triangle incident to a support centre
/// while leaving that centre inside the retained footprint. Split the
/// containing alpha triangle so terminal-ring barycentres remain explicit
/// underside constraints.
fn ensure_support_centres(
    triangles: &mut Vec<[usize; 3]>,
    samples: &[Sample],
    supports: &[Support],
) {
    for (support_index, support) in supports.iter().enumerate() {
        let Some(centre) = samples.iter().position(|sample| {
            sample.support == support_index
                && sample
                    .position
                    .truncate()
                    .distance_squared(support.position.truncate())
                    <= POINT_MERGE_EPSILON * POINT_MERGE_EPSILON
        }) else {
            continue;
        };
        if triangles.iter().any(|triangle| triangle.contains(&centre)) {
            continue;
        }
        let Some(triangle_index) = triangles.iter().position(|triangle| {
            point_in_triangle(
                samples[centre].position.truncate(),
                [
                    samples[triangle[0]].position.truncate(),
                    samples[triangle[1]].position.truncate(),
                    samples[triangle[2]].position.truncate(),
                ],
            )
        }) else {
            continue;
        };
        let [a, b, c] = triangles.swap_remove(triangle_index);
        triangles.extend([[a, b, centre], [b, c, centre], [c, a, centre]]);
    }
}

fn point_in_triangle(point: Vec2, triangle: [Vec2; 3]) -> bool {
    let point = point * ISLAND_WORLD_METRES;
    let triangle = triangle.map(|vertex| vertex * ISLAND_WORLD_METRES);
    let signs = [
        (triangle[1] - triangle[0]).perp_dot(point - triangle[0]),
        (triangle[2] - triangle[1]).perp_dot(point - triangle[1]),
        (triangle[0] - triangle[2]).perp_dot(point - triangle[2]),
    ];
    let has_negative = signs.iter().any(|&sign| sign < -ALPHA_AREA_EPSILON_METRES2);
    let has_positive = signs.iter().any(|&sign| sign > ALPHA_AREA_EPSILON_METRES2);
    !(has_negative && has_positive)
}

fn triangle_components(triangles: &[[usize; 3]]) -> Vec<Vec<usize>> {
    let mut edge_to_triangles = BTreeMap::<(usize, usize), Vec<usize>>::new();
    for (triangle_index, triangle) in triangles.iter().enumerate() {
        for edge in triangle_edges(*triangle) {
            edge_to_triangles
                .entry(edge)
                .or_default()
                .push(triangle_index);
        }
    }

    let mut visited = vec![false; triangles.len()];
    let mut components = Vec::new();
    for first in 0..triangles.len() {
        if visited[first] {
            continue;
        }
        let mut stack = vec![first];
        let mut component = Vec::new();
        visited[first] = true;
        while let Some(triangle_index) = stack.pop() {
            component.push(triangle_index);
            for edge in triangle_edges(triangles[triangle_index]) {
                if let Some(neighbours) = edge_to_triangles.get(&edge) {
                    for &neighbour in neighbours {
                        if !visited[neighbour] {
                            visited[neighbour] = true;
                            stack.push(neighbour);
                        }
                    }
                }
            }
        }
        component.sort_unstable();
        components.push(component);
    }
    components
}

fn triangle_edges(triangle: [usize; 3]) -> [(usize, usize); 3] {
    [
        ordered_edge(triangle[0], triangle[1]),
        ordered_edge(triangle[1], triangle[2]),
        ordered_edge(triangle[2], triangle[0]),
    ]
}

fn ordered_edge(a: usize, b: usize) -> (usize, usize) {
    (a.min(b), a.max(b))
}

#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
fn append_component(
    mesh: &mut Mesh,
    seed: u64,
    component: &[usize],
    triangles: &[[usize; 3]],
    samples: &[Sample],
    supports: &[Support],
    crown_shapes: &[CrownShape],
    support_vertices: &mut Vec<usize>,
) -> Result<(), String> {
    let mut sample_indices = component
        .iter()
        .flat_map(|&triangle_index| triangles[triangle_index])
        .collect::<Vec<_>>();
    sample_indices.sort_unstable();
    sample_indices.dedup();
    if sample_indices.len() < 3 {
        return Ok(());
    }

    let base_index = mesh.vertices.len();
    u32::try_from(base_index)
        .map_err(|_| "cluster foliage exceeds u32 vertex capacity".to_owned())?;
    let mut local_index = BTreeMap::new();
    for (local, &sample_index) in sample_indices.iter().enumerate() {
        local_index.insert(sample_index, local);
    }

    let component_centroid = sample_indices.iter().fold(Vec2::ZERO, |total, &index| {
        total + samples[index].position.truncate()
    }) / sample_indices.len() as f32;
    let component_spread = sample_indices
        .iter()
        .map(|&index| {
            samples[index]
                .position
                .truncate()
                .distance(component_centroid)
        })
        .fold(0.0_f32, f32::max);
    let height = (MINIMUM_HEIGHT_METRES
        + (component_spread * ISLAND_WORLD_METRES * HEIGHT_PER_SPREAD_METRES)
            .min(MAX_SPREAD_HEIGHT_METRES))
        / ISLAND_WORLD_METRES;

    let mut component_triangles = Vec::with_capacity(component.len());
    for &triangle_index in component {
        let triangle = triangles[triangle_index];
        let mapped = [
            local_index[&triangle[0]],
            local_index[&triangle[1]],
            local_index[&triangle[2]],
        ];
        component_triangles.push(mapped);
    }

    let boundary = boundary_edges(&component_triangles);
    if boundary.is_empty() {
        return Ok(());
    }

    let adjacency = triangle_adjacency(sample_indices.len(), &component_triangles);
    let support_centres = sample_indices
        .iter()
        .map(|&sample_index| {
            let sample = samples[sample_index];
            supports[sample.support]
                .position
                .distance_squared(sample.position)
                <= POINT_MERGE_EPSILON * POINT_MERGE_EPSILON
        })
        .collect::<Vec<_>>();
    let mut bottom_positions = sample_indices
        .iter()
        .map(|&sample_index| samples[sample_index].position)
        .collect::<Vec<_>>();
    // Keep every branch tip inside the volume rather than pinning a tall tip
    // directly to the underside and turning the canopy into a cone. XY remains
    // attached to the projected tip, while the underside may relax downward.
    let unpinned = vec![false; sample_indices.len()];
    smooth_control_heights(
        &mut bottom_positions,
        &adjacency,
        &unpinned,
        CONTROL_HEIGHT_SMOOTHING_ITERATIONS,
        CONTROL_HEIGHT_SMOOTHING_AMOUNT,
    );
    for (local, position) in bottom_positions.iter_mut().enumerate() {
        let support_height = samples[sample_indices[local]].position.z;
        position.z = position.z.min(support_height);
    }
    let boundary_outward = boundary_outward_directions(&bottom_positions, &boundary);

    let mut top_positions = Vec::with_capacity(sample_indices.len());
    for local in 0..sample_indices.len() {
        let sample_index = sample_indices[local];
        let base = bottom_positions[local];
        let support = samples[sample_index].position;
        let radial = base.truncate() - component_centroid;
        let irregular_scale = 1.0
            + (hash_unit(seed ^ FOLIAGE_SEED_DOMAIN, sample_index as u64, 0x71, 0) - 0.5) * 0.10;
        let upper_xy = component_centroid + radial * TOP_RING_SCALE * irregular_scale;
        let peak = crown_peak_weight(base.truncate(), crown_shapes);
        let support_peak = hash_unit(seed ^ FOLIAGE_SEED_DOMAIN, sample_index as u64, 0x72, 0);
        let lift = height * (0.66 + 0.18 * peak + 0.03 * (support_peak - 0.5));
        let top_height = (base.z + lift).max(support.z + metres(TOP_TIP_CLEARANCE_METRES));
        top_positions.push(Vec3::new(upper_xy.x, upper_xy.y, top_height));
    }
    smooth_control_heights(
        &mut top_positions,
        &adjacency,
        &unpinned,
        TOP_HEIGHT_SMOOTHING_ITERATIONS,
        TOP_HEIGHT_SMOOTHING_AMOUNT,
    );
    for (local, position) in top_positions.iter_mut().enumerate() {
        let support_height = samples[sample_indices[local]].position.z;
        position.z = position
            .z
            .max(support_height + metres(TOP_TIP_CLEARANCE_METRES));
    }
    raise_top_shoulders(
        &mut top_positions,
        &adjacency,
        TOP_SHOULDER_PROPAGATION_ITERATIONS,
        TOP_MAXIMUM_SLOPE,
    );

    let mut middle_index = BTreeMap::new();
    let mut top_index = BTreeMap::new();
    for (local, &is_support_centre) in support_centres.iter().enumerate() {
        let position = bottom_positions[local];
        mesh.vertices.push(position);
        if is_support_centre {
            support_vertices.push(base_index + local);
        }
    }
    let top_base = mesh.vertices.len();
    mesh.vertices.extend(top_positions.iter().copied());
    for (local, &is_support_centre) in support_centres.iter().enumerate() {
        top_index.insert(local, top_base + local);
        if is_support_centre {
            // A top vertex is not a support constraint, but retaining the
            // index in the set lets the displacement pass keep the peak seam
            // quiet when it lies on a support centre.
            support_vertices.push(top_base + local);
        }
    }
    for &(a, b, _) in &boundary {
        for local in [a, b] {
            if middle_index.contains_key(&local) {
                continue;
            }
            let sample_index = sample_indices[local];
            let position = bottom_positions[local];
            let radial = position.truncate() - component_centroid;
            let jitter =
                (hash_unit(seed ^ FOLIAGE_SEED_DOMAIN, sample_index as u64, 0x73, 0) - 0.5) * 0.08;
            let middle_xy = component_centroid
                + radial * (MID_RING_SCALE + jitter)
                + boundary_outward[local] * metres(BOUNDARY_EXPANSION_METRES);
            let peak = crown_peak_weight(position.truncate(), crown_shapes);
            let lift = height * (0.18 + 0.08 * peak);
            let index = mesh.vertices.len();
            mesh.vertices
                .push(Vec3::new(middle_xy.x, middle_xy.y, position.z + lift));
            middle_index.insert(local, index);
        }
    }

    for triangle in &component_triangles {
        let a = u32::try_from(base_index + triangle[0])
            .map_err(|_| "cluster foliage exceeds u32 triangle capacity".to_owned())?;
        let b = u32::try_from(base_index + triangle[1])
            .map_err(|_| "cluster foliage exceeds u32 triangle capacity".to_owned())?;
        let c = u32::try_from(base_index + triangle[2])
            .map_err(|_| "cluster foliage exceeds u32 triangle capacity".to_owned())?;
        // Delaunay triangles are oriented upward; the underside is the
        // opposite winding and the upper cap retains the upward winding.
        mesh.triangles.extend([a, c, b]);
        mesh.triangles.extend([
            u32::try_from(top_base + triangle[0])
                .map_err(|_| "cluster foliage exceeds u32 triangle capacity".to_owned())?,
            u32::try_from(top_base + triangle[1])
                .map_err(|_| "cluster foliage exceeds u32 triangle capacity".to_owned())?,
            u32::try_from(top_base + triangle[2])
                .map_err(|_| "cluster foliage exceeds u32 triangle capacity".to_owned())?,
        ]);
    }

    for &(a, b, _) in &boundary {
        let bottom_a = u32::try_from(base_index + a)
            .map_err(|_| "cluster foliage exceeds u32 triangle capacity".to_owned())?;
        let bottom_b = u32::try_from(base_index + b)
            .map_err(|_| "cluster foliage exceeds u32 triangle capacity".to_owned())?;
        let top_a = u32::try_from(top_index[&a])
            .map_err(|_| "cluster foliage exceeds u32 triangle capacity".to_owned())?;
        let top_b = u32::try_from(top_index[&b])
            .map_err(|_| "cluster foliage exceeds u32 triangle capacity".to_owned())?;
        let middle_a = u32::try_from(middle_index[&a])
            .map_err(|_| "cluster foliage exceeds u32 triangle capacity".to_owned())?;
        let middle_b = u32::try_from(middle_index[&b])
            .map_err(|_| "cluster foliage exceeds u32 triangle capacity".to_owned())?;
        append_quad(&mut mesh.triangles, bottom_a, bottom_b, middle_b, middle_a);
        append_quad(&mut mesh.triangles, middle_a, middle_b, top_b, top_a);
    }
    Ok(())
}

fn triangle_adjacency(vertex_count: usize, triangles: &[[usize; 3]]) -> Vec<Vec<usize>> {
    let mut adjacency = vec![Vec::new(); vertex_count];
    for triangle in triangles {
        for (a, b) in [
            (triangle[0], triangle[1]),
            (triangle[1], triangle[2]),
            (triangle[2], triangle[0]),
        ] {
            adjacency[a].push(b);
            adjacency[b].push(a);
        }
    }
    for neighbours in &mut adjacency {
        neighbours.sort_unstable();
        neighbours.dedup();
    }
    adjacency
}

fn smooth_control_heights(
    positions: &mut [Vec3],
    adjacency: &[Vec<usize>],
    pinned: &[bool],
    iterations: usize,
    amount: f32,
) {
    debug_assert_eq!(positions.len(), adjacency.len());
    debug_assert_eq!(positions.len(), pinned.len());
    for _ in 0..iterations {
        // A separate height generation keeps the relaxation independent of
        // vertex order; XY is deliberately unchanged so the alpha footprint
        // and its closed side wall remain stable.
        let previous = positions
            .iter()
            .map(|position| position.z)
            .collect::<Vec<_>>();
        for (vertex, position) in positions.iter_mut().enumerate() {
            let neighbours = &adjacency[vertex];
            if pinned[vertex] || neighbours.is_empty() {
                continue;
            }
            let average = neighbours
                .iter()
                .map(|&neighbour| previous[neighbour])
                .sum::<f32>()
                / neighbours.len() as f32;
            position.z = (average - previous[vertex]).mul_add(amount, previous[vertex]);
        }
    }
}

fn raise_top_shoulders(
    positions: &mut [Vec3],
    adjacency: &[Vec<usize>],
    iterations: usize,
    maximum_slope: f32,
) {
    debug_assert_eq!(positions.len(), adjacency.len());
    for _ in 0..iterations {
        let previous = positions.to_vec();
        for (vertex, position) in positions.iter_mut().enumerate() {
            let required_height = adjacency[vertex]
                .iter()
                .map(|&neighbour| {
                    let horizontal_distance = previous[vertex]
                        .truncate()
                        .distance(previous[neighbour].truncate());
                    previous[neighbour].z - horizontal_distance * maximum_slope
                })
                .fold(position.z, f32::max);
            position.z = position.z.max(required_height);
        }
    }
}

fn boundary_outward_directions(
    positions: &[Vec3],
    boundary: &[(usize, usize, usize)],
) -> Vec<Vec2> {
    let mut outward_directions = vec![Vec2::ZERO; positions.len()];
    for &(a, b, _) in boundary {
        let edge = positions[b].truncate() - positions[a].truncate();
        let outward = Vec2::new(edge.y, -edge.x).normalize_or_zero();
        outward_directions[a] += outward;
        outward_directions[b] += outward;
    }
    for outward in &mut outward_directions {
        *outward = outward.normalize_or_zero();
    }
    outward_directions
}

fn boundary_edges(triangles: &[[usize; 3]]) -> Vec<(usize, usize, usize)> {
    let mut edges = BTreeMap::<(usize, usize), (usize, usize, usize)>::new();
    for triangle in triangles {
        for (a, b) in [
            (triangle[0], triangle[1]),
            (triangle[1], triangle[2]),
            (triangle[2], triangle[0]),
        ] {
            let key = ordered_edge(a, b);
            edges
                .entry(key)
                .and_modify(|entry| entry.2 += 1)
                .or_insert((a, b, 1));
        }
    }
    edges.into_values().filter(|entry| entry.2 == 1).collect()
}

fn append_quad(triangles: &mut Vec<u32>, a: u32, b: u32, c: u32, d: u32) {
    triangles.extend([a, b, c, a, c, d]);
}

fn crown_peak_weight(position: Vec2, crowns: &[CrownShape]) -> f32 {
    crowns
        .iter()
        .map(|crown| {
            let distance = position.distance(crown.trunk.truncate());
            let sigma = crown.radius * 0.58;
            (-distance * distance / (2.0 * sigma * sigma)).exp()
        })
        .fold(0.0_f32, f32::max)
}

fn smooth_subdivision_vertices(
    mesh: &mut Mesh,
    coarse_vertex_count: usize,
    support_vertices: &[usize],
) {
    if mesh.vertices.len() <= coarse_vertex_count {
        return;
    }
    let adjacency = mesh.adjacency();
    for _ in 0..LOD0_SMOOTHING_ITERATIONS {
        let previous = mesh.vertices.clone();
        for vertex in 0..mesh.vertices.len() {
            if support_vertices.binary_search(&vertex).is_ok() {
                continue;
            }
            let neighbours = &adjacency[vertex];
            if neighbours.is_empty() {
                continue;
            }
            let average = neighbours
                .iter()
                .fold(Vec3::ZERO, |total, &neighbour| total + previous[neighbour])
                / neighbours.len() as f32;
            let amount = if vertex < coarse_vertex_count {
                LOD0_COARSE_SMOOTHING_AMOUNT
            } else {
                LOD0_SUBDIVISION_SMOOTHING_AMOUNT
            };
            mesh.vertices[vertex] = previous[vertex].lerp(average, amount);
        }
    }
}

fn displace_subdivision_vertices(
    mesh: &mut Mesh,
    seed: u64,
    new_vertices: &[crate::mesh::NewVertexStencil],
    support_vertices: &[usize],
) {
    if new_vertices.is_empty() {
        return;
    }
    for stencil in new_vertices {
        let vertex = stencil.vertex as usize;
        if support_vertices.binary_search(&vertex).is_ok() || vertex >= mesh.vertices.len() {
            continue;
        }
        let position = mesh.vertices[vertex];
        let normal = mesh.normals.get(vertex).copied().unwrap_or(Vec3::Z);
        let coherent = coherent_value(seed, position);
        let underside_weight = if normal.z < -0.25 { 0.20 } else { 1.0 };
        let top_billow = metres(LOD0_BILLOW_METRES) * normal.z.max(0.0);
        let displacement =
            metres(LOD0_DISPLACEMENT_METRES) * coherent * underside_weight + top_billow;
        mesh.vertices[vertex] = position + normal * displacement;
    }
}

fn coherent_value(seed: u64, position: Vec3) -> f32 {
    const DETAIL_SCALE_METRES: f32 = 0.75;
    let point = position * (ISLAND_WORLD_METRES / DETAIL_SCALE_METRES);
    let domain_seed = seed ^ FOLIAGE_SEED_DOMAIN;
    let xy = noise::fractal(domain_seed, point.x, point.y, 3);
    let yz = noise::fractal(domain_seed.rotate_left(17), point.y, point.z, 3);
    let zx = noise::fractal(domain_seed.rotate_left(41), point.z, point.x, 3);
    (xy + yz + zx) / 3.0
}

fn metres(value: f32) -> f32 {
    value / ISLAND_WORLD_METRES
}

fn hash_unit(seed: u64, a: u64, b: u64, c: u64) -> f32 {
    let mut value = seed
        ^ a.wrapping_mul(0x9e37_79b9_7f4a_7c15)
        ^ b.rotate_left(21)
        ^ c.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value ^= value >> 30;
    value = value.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value ^= value >> 27;
    value = value.wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^= value >> 31;
    (value as f64 / u64::MAX as f64) as f32
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;

    fn m(value: f32) -> f32 {
        metres(value)
    }

    fn sample_crowns() -> Vec<FoliageCrown<'static>> {
        let tips_a = Box::leak(
            vec![
                Vec3::new(m(-2.0), m(0.0), m(8.0)),
                Vec3::new(m(0.0), m(1.0), m(8.4)),
                Vec3::new(m(1.5), m(-0.5), m(8.1)),
            ]
            .into_boxed_slice(),
        );
        let tips_b = Box::leak(
            vec![
                Vec3::new(m(3.0), m(0.2), m(8.2)),
                Vec3::new(m(4.5), m(1.1), m(8.0)),
                Vec3::new(m(5.2), m(-0.7), m(8.3)),
            ]
            .into_boxed_slice(),
        );
        vec![
            FoliageCrown {
                trunk: Vec3::new(m(0.0), m(0.0), m(4.0)),
                tips: tips_a,
            },
            FoliageCrown {
                trunk: Vec3::new(m(3.0), m(0.0), m(4.1)),
                tips: tips_b,
            },
        ]
    }

    fn assert_valid_mesh(mesh: &Mesh) {
        assert_eq!(mesh.vertices.len(), mesh.normals.len());
        assert!(mesh.triangles.len().is_multiple_of(3));
        assert!(mesh.vertices.iter().all(|vertex| vertex.is_finite()));
        assert!(mesh.normals.iter().all(|normal| normal.is_finite()));
        for triangle in mesh.triangles.chunks_exact(3) {
            assert!(
                triangle
                    .iter()
                    .all(|&index| (index as usize) < mesh.vertices.len())
            );
            let [a, b, c] =
                [triangle[0], triangle[1], triangle[2]].map(|index| mesh.vertices[index as usize]);
            assert!((b - a).cross(c - a).length_squared() > TEST_GEOMETRY_EPSILON);
        }
    }

    fn edge_use_counts(mesh: &Mesh) -> BTreeMap<(u32, u32), usize> {
        let mut counts = BTreeMap::new();
        for triangle in mesh.triangles.chunks_exact(3) {
            for edge in [
                ordered_edge_u32(triangle[0], triangle[1]),
                ordered_edge_u32(triangle[1], triangle[2]),
                ordered_edge_u32(triangle[2], triangle[0]),
            ] {
                *counts.entry(edge).or_insert(0) += 1;
            }
        }
        counts
    }

    fn ordered_edge_u32(a: u32, b: u32) -> (u32, u32) {
        (a.min(b), a.max(b))
    }

    #[test]
    fn generation_is_deterministic() {
        let crowns = sample_crowns();
        let first = generate_cluster_foliage(42, &crowns).expect("valid foliage");
        let second = generate_cluster_foliage(42, &crowns).expect("valid foliage");
        assert_eq!(first, second);
        let changed = generate_cluster_foliage(43, &crowns).expect("valid foliage");
        assert_ne!(first, changed);
    }

    #[test]
    fn branch_tips_are_the_only_projected_control_samples() {
        let crowns = sample_crowns();
        let supports = collect_supports(&crowns);
        let samples = collect_samples(42, &supports);
        assert_eq!(
            supports.len(),
            crowns.iter().map(|crown| crown.tips.len()).sum::<usize>()
        );
        assert_eq!(samples.len(), supports.len());

        let foliage = generate_cluster_foliage(42, &crowns).expect("valid foliage");
        assert!(foliage.lod1.triangles.len() / 3 <= supports.len() * 8);
    }

    #[test]
    fn meshes_have_finite_geometry_and_are_closed() {
        let foliage = generate_cluster_foliage(7, &sample_crowns()).expect("valid foliage");
        for mesh in [&foliage.lod1, &foliage.lod0] {
            assert_valid_mesh(mesh);
            assert!(edge_use_counts(mesh).values().all(|&count| count == 2));
        }
    }

    #[test]
    fn control_height_smoothing_softens_spikes_without_moving_pinned_supports() {
        let mut positions = vec![
            Vec3::new(0.0, 0.0, 4.0),
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
            Vec3::new(-1.0, 0.0, 0.0),
        ];
        let adjacency = vec![vec![1, 2, 3], vec![0, 2], vec![0, 1, 3], vec![0, 2]];
        smooth_control_heights(
            &mut positions,
            &adjacency,
            &[true, false, false, false],
            3,
            0.45,
        );

        assert_eq!(positions[0].z.to_bits(), 4.0_f32.to_bits());
        assert!(positions[1..].iter().all(|position| position.z > 0.0));
        assert!(positions[1..].iter().all(|position| position.z < 4.0));
    }

    #[test]
    fn tall_tips_raise_broad_shoulders_instead_of_narrow_spikes() {
        let mut positions = vec![
            Vec3::new(0.0, 0.0, m(8.0)),
            Vec3::new(m(1.0), 0.0, m(2.0)),
            Vec3::new(0.0, m(1.0), m(2.0)),
            Vec3::new(m(-1.0), 0.0, m(2.0)),
        ];
        let adjacency = vec![vec![1, 2, 3], vec![0, 2], vec![0, 1, 3], vec![0, 2]];
        raise_top_shoulders(
            &mut positions,
            &adjacency,
            TOP_SHOULDER_PROPAGATION_ITERATIONS,
            TOP_MAXIMUM_SLOPE,
        );

        for (vertex, neighbours) in adjacency.iter().enumerate() {
            for &neighbour in neighbours {
                let horizontal_distance = positions[vertex]
                    .truncate()
                    .distance(positions[neighbour].truncate());
                let height_difference = (positions[vertex].z - positions[neighbour].z).abs();
                assert!(
                    height_difference <= horizontal_distance * TOP_MAXIMUM_SLOPE + metres(0.001)
                );
            }
        }
    }

    #[test]
    fn canopy_billows_well_beyond_and_above_outer_branch_tips() {
        let crowns = sample_crowns();
        let outermost_tip = crowns
            .iter()
            .flat_map(|crown| crown.tips)
            .map(|tip| tip.x)
            .fold(f32::MIN, f32::max);
        let foliage = generate_cluster_foliage(18, &crowns).expect("valid foliage");
        let outermost_canopy = foliage
            .lod1
            .vertices
            .iter()
            .map(|vertex| vertex.x)
            .fold(f32::MIN, f32::max);
        let highest_tip = crowns
            .iter()
            .flat_map(|crown| crown.tips)
            .map(|tip| tip.z)
            .fold(f32::MIN, f32::max);
        let highest_canopy = foliage
            .lod1
            .vertices
            .iter()
            .map(|vertex| vertex.z)
            .fold(f32::MIN, f32::max);

        assert!(
            outermost_canopy > outermost_tip + metres(1.0),
            "canopy extends only {:.2}m beyond its outer tip",
            (outermost_canopy - outermost_tip) * ISLAND_WORLD_METRES
        );
        assert!(
            highest_canopy > highest_tip + metres(1.5),
            "canopy rises only {:.2}m above its highest tip",
            (highest_canopy - highest_tip) * ISLAND_WORLD_METRES
        );
    }

    #[test]
    fn lod0_is_subdivided_and_shares_its_smoothed_coarse_vertices() {
        let foliage = generate_cluster_foliage(12, &sample_crowns()).expect("valid foliage");
        assert!(foliage.lod0.triangles.len() > foliage.lod1.triangles.len());
        assert_eq!(foliage.lod1_to_lod0.len(), foliage.lod1.vertices.len());
        for (lod1_vertex, &lod0_vertex) in foliage.lod1_to_lod0.iter().enumerate() {
            assert_eq!(lod0_vertex as usize, lod1_vertex);
            assert_eq!(
                foliage.lod1.vertices[lod1_vertex],
                foliage.lod0.vertices[lod0_vertex as usize]
            );
        }
    }

    #[test]
    fn subdivision_smoothing_rounds_coarse_and_new_vertices_but_pins_supports() {
        let mut mesh = Mesh {
            vertices: vec![
                Vec3::new(0.0, 0.0, 2.0),
                Vec3::new(-1.0, -1.0, 0.0),
                Vec3::new(1.0, -1.0, 0.0),
                Vec3::new(0.0, 1.0, 0.0),
                Vec3::new(0.0, 0.0, -1.0),
            ],
            triangles: vec![0, 1, 4, 1, 2, 4, 2, 3, 4, 3, 0, 4],
            ..Mesh::default()
        };
        let original = mesh.vertices.clone();

        smooth_subdivision_vertices(&mut mesh, 4, &[0]);

        assert_eq!(mesh.vertices[0], original[0]);
        assert_ne!(mesh.vertices[1], original[1]);
        assert_ne!(mesh.vertices[4], original[4]);
    }

    #[test]
    fn distant_support_groups_remain_disconnected() {
        let tips_a = [
            Vec3::new(m(-21.0), m(-1.0), m(5.0)),
            Vec3::new(m(-19.0), m(-1.0), m(5.2)),
            Vec3::new(m(-20.0), m(1.0), m(5.1)),
        ];
        let tips_b = [
            Vec3::new(m(19.0), m(-1.0), m(5.0)),
            Vec3::new(m(21.0), m(-1.0), m(5.2)),
            Vec3::new(m(20.0), m(1.0), m(5.1)),
        ];
        let crowns = [
            FoliageCrown {
                trunk: Vec3::new(m(-20.0), m(0.0), m(3.0)),
                tips: &tips_a,
            },
            FoliageCrown {
                trunk: Vec3::new(m(20.0), m(0.0), m(3.0)),
                tips: &tips_b,
            },
        ];
        let foliage = generate_cluster_foliage(4, &crowns).expect("valid foliage");
        let mut neighbours = vec![Vec::new(); foliage.lod1.vertices.len()];
        for triangle in foliage.lod1.triangles.chunks_exact(3) {
            for (a, b) in [
                (triangle[0] as usize, triangle[1] as usize),
                (triangle[1] as usize, triangle[2] as usize),
                (triangle[2] as usize, triangle[0] as usize),
            ] {
                neighbours[a].push(b);
                neighbours[b].push(a);
            }
        }
        let mut components = 0;
        let mut visited = vec![false; neighbours.len()];
        for first in 0..neighbours.len() {
            if visited[first] {
                continue;
            }
            components += 1;
            let mut stack = vec![first];
            visited[first] = true;
            while let Some(vertex) = stack.pop() {
                for &neighbour in &neighbours[vertex] {
                    if !visited[neighbour] {
                        visited[neighbour] = true;
                        stack.push(neighbour);
                    }
                }
            }
        }
        assert_eq!(components, 2);
    }

    #[test]
    fn branch_tip_supports_do_not_protrude_below_the_underside() {
        let crowns = sample_crowns();
        let foliage = generate_cluster_foliage(18, &crowns).expect("valid foliage");
        for crown in crowns {
            for tip in crown.tips {
                let underside_near_tip = foliage
                    .lod1
                    .triangles
                    .chunks_exact(3)
                    .filter_map(|triangle| {
                        let vertices = [triangle[0], triangle[1], triangle[2]]
                            .map(|vertex| foliage.lod1.vertices[vertex as usize]);
                        let normal = (vertices[1] - vertices[0]).cross(vertices[2] - vertices[0]);
                        if normal.z >= -TEST_GEOMETRY_EPSILON
                            || !point_in_triangle(tip.truncate(), vertices.map(Vec3::truncate))
                        {
                            return None;
                        }
                        let offset = *tip - vertices[0];
                        Some(vertices[0].z - (normal.x * offset.x + normal.y * offset.y) / normal.z)
                    })
                    .any(|underside_height| underside_height <= tip.z + metres(0.001));
                assert!(
                    underside_near_tip,
                    "branch tip {tip:?} protrudes below the canopy"
                );
            }
        }
    }

    #[test]
    fn empty_and_invalid_inputs_are_safe() {
        let empty = generate_cluster_foliage(1, &[]).expect("empty foliage");
        assert!(empty.lod0.vertices.is_empty());
        let tips = [Vec3::new(f32::NAN, 0.0, 0.0)];
        let invalid = generate_cluster_foliage(
            1,
            &[FoliageCrown {
                trunk: Vec3::ZERO,
                tips: &tips,
            }],
        );
        assert!(invalid.is_err());
    }
}
