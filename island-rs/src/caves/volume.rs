//! Sparse cube-edge contouring of the union of cave air volumes. Shared-face
//! decisions agree across cells; only the cavity is meshed, never the terrain roof.
use super::{Cave, CaveOptions, field, portal::Portal};
use crate::{ISLAND_WORLD_METRES, Mesh, Vec3};
use std::collections::{BTreeSet, HashMap};
const CELLS: i32 = 16;
const SIDE: usize = 17;
const CORNERS: [[i32; 3]; 8] = [
    [0, 0, 0],
    [1, 0, 0],
    [1, 1, 0],
    [0, 1, 0],
    [0, 0, 1],
    [1, 0, 1],
    [1, 1, 1],
    [0, 1, 1],
];
const EDGES: [[usize; 2]; 12] = [
    [0, 1],
    [1, 2],
    [2, 3],
    [3, 0],
    [4, 5],
    [5, 6],
    [6, 7],
    [7, 4],
    [0, 4],
    [1, 5],
    [2, 6],
    [3, 7],
];
// Face corners and matching edges use the same cyclic order.
const FACES: [([usize; 4], [usize; 4]); 6] = [
    ([0, 1, 2, 3], [0, 1, 2, 3]),
    ([4, 5, 6, 7], [4, 5, 6, 7]),
    ([0, 1, 5, 4], [0, 9, 4, 8]),
    ([3, 2, 6, 7], [2, 10, 6, 11]),
    ([0, 3, 7, 4], [3, 11, 7, 8]),
    ([1, 2, 6, 5], [1, 10, 5, 9]),
];

pub(super) const ENTRANCE_BLEND_LENGTH: f32 = 5.0;

fn index(x: usize, y: usize, z: usize) -> usize {
    (z * SIDE + y) * SIDE + x
}

pub(crate) fn build(cave: &mut Cave, o: &CaveOptions) -> Result<Vec<Mesh>, String> {
    let span = o.voxel_size * CELLS as f32;
    let mut active = BTreeSet::new();
    for path in cave.paths() {
        for pair in path.windows(2) {
            let radius = pair[0].width.max(pair[1].width) * 0.5
                + o.broad_amplitude
                + o.fine_amplitude
                + o.voxel_size * 2.0;
            let min = pair[0].floor.min(pair[1].floor) - Vec3::splat(radius);
            let max = pair[0].floor.max(pair[1].floor)
                + Vec3::new(radius, radius, pair[0].height.max(pair[1].height) + radius);
            for z in (min.z / span).floor() as i32..=(max.z / span).floor() as i32 {
                for y in (min.y / span).floor() as i32..=(max.y / span).floor() as i32 {
                    for x in (min.x / span).floor() as i32..=(max.x / span).floor() as i32 {
                        active.insert([x, y, z]);
                    }
                }
            }
        }
    }
    if active.len() > 8192 {
        return Err("cave volume exceeded emergency sampling safeguard".into());
    }
    let mut mesh = Mesh::default();
    for key in active {
        let part = chunk(cave, o, key)?;
        let offset = mesh.vertices.len() as u32;
        mesh.vertices.extend(part.vertices);
        mesh.normals.extend(part.normals);
        mesh.uv.extend(part.uv);
        mesh.triangles
            .extend(part.triangles.into_iter().map(|i| i + offset));
        if mesh.triangles.len() / 3 > super::MAX_TRIANGLES {
            return Err("cave volume exceeded emergency triangle safeguard".into());
        }
    }
    super::orientation::orient(&mut mesh)?;
    super::smoothing::round_interior(&mut mesh, cave, o);
    // Cut back through the closed vestibule cap, rounding into the exact existing
    // throat. The exterior terrain lip is untouched, including its grass/LOD data.
    let reverse = Portal {
        origin: cave.portal.throat() + cave.inward.extend(0.0) * ENTRANCE_BLEND_LENGTH,
        inward: -cave.inward,
        length: ENTRANCE_BLEND_LENGTH,
        floor_clearance: o.floor_roughness + 0.15,
        exterior: false,
        section: cave.portal.section.clone(),
    };
    mesh = reverse.join_volume(mesh, &cave.entrance_collider.vertices);
    let throat = cave.portal.throat();
    if !super::stitch::close_boundary_seams(&mut mesh, |p| {
        (p - throat).truncate().dot(cave.inward).abs() < 0.01
    }) {
        return Err("cave volume has an unmatched interior seam".into());
    }
    // Both render-LOD cutting and the entrance collider must see the additional
    // contour samples introduced by the volume's throat.
    let (min, max) = cave.portal.throat_bounds();
    super::stitch::repair(
        &mut cave.entrance_collider,
        min,
        max,
        cave.portal.throat_anchors(&mesh.vertices),
    );
    super::stitch::repair(
        &mut mesh,
        min,
        max,
        cave.portal.throat_anchors(&cave.entrance_collider.vertices),
    );
    let mut result = Vec::new();
    for triangles in mesh.triangles.chunks(10_000 * 3) {
        let mut part = Mesh::default();
        let mut indices = HashMap::new();
        for &index in triangles {
            let local = *indices.entry(index).or_insert_with(|| {
                let local = part.vertices.len() as u32;
                part.vertices.push(mesh.vertices[index as usize]);
                part.normals.push(mesh.normals[index as usize]);
                part.uv.push(mesh.uv[index as usize]);
                local
            });
            part.triangles.push(local);
        }
        result.push(part);
    }
    Ok(result)
}

// Positive is air. A maximum unions the paths, eliminating walls at crossings.
pub(super) fn density(cave: &Cave, o: &CaveOptions, p: Vec3) -> f32 {
    let floor = cave.entrance.z
        + field::noise3(cave.id ^ 0x464c_4f4f, p.truncate().extend(0.0) / 3.0) * o.floor_roughness;
    let rough = field::noise3(cave.id, p / o.broad_period) * o.broad_amplitude
        + field::noise3(cave.id ^ 0x4649_4e45, p / o.fine_period) * o.fine_amplitude;
    let mut air = f32::NEG_INFINITY;
    // The first two main nodes belong to the preserved entrance, not the volume.
    for path in
        std::iter::once(&cave.nodes[2..]).chain(cave.branches.iter().map(|b| b.nodes.as_slice()))
    {
        for (a, b) in path
            .windows(2)
            .map(|p| (p[0], p[1]))
            .chain(path.first().map(|n| (*n, *n)))
        {
            let edge = (b.floor - a.floor).truncate();
            let t = ((p - a.floor).truncate().dot(edge) / edge.length_squared().max(1.0e-8))
                .clamp(0.0, 1.0);
            let centre = a.floor.lerp(b.floor, t);
            let radius = (a.width + (b.width - a.width) * t) * 0.5;
            let height = a.height + (b.height - a.height) * t;
            let radial = (p - centre).truncate().length();
            let upper = ((p.z - centre.z - height * 0.35) / (height * 0.65)).max(0.0);
            let wall = (1.0 - ((radial / radius).powi(2) + upper * upper).sqrt())
                * radius.min(height * 0.65)
                + rough;
            air = air.max(wall);
        }
    }
    // Constant section behind the protected entrance, joined to the wandering
    // root. The front end is trimmed away before installation.
    let start = cave.portal.throat() + cave.inward.extend(0.0) * 2.0;
    let end = cave.nodes[2].floor;
    let edge = end - start;
    let t = ((p - start).dot(edge) / edge.length_squared()).clamp(0.0, 1.0);
    let centre = start.lerp(end, t);
    let radius = o.entrance_width * 0.5;
    let upper = ((p.z - centre.z - o.entrance_height * 0.35) / (o.entrance_height * 0.65)).max(0.0);
    air = air.max(
        (1.0 - (((p - centre).truncate().length() / radius).powi(2) + upper * upper).sqrt())
            * radius.min(o.entrance_height * 0.65)
            + rough,
    );
    air.min(p.z - floor)
}
fn normal(cave: &Cave, o: &CaveOptions, p: Vec3) -> Vec3 {
    let e = 0.01;
    Vec3::new(
        density(cave, o, p + Vec3::X * e) - density(cave, o, p - Vec3::X * e),
        density(cave, o, p + Vec3::Y * e) - density(cave, o, p - Vec3::Y * e),
        density(cave, o, p + Vec3::Z * e) - density(cave, o, p - Vec3::Z * e),
    )
    .normalize_or_zero()
}
#[allow(clippy::too_many_lines, clippy::cast_possible_wrap)]
fn chunk(cave: &Cave, options: &CaveOptions, key: [i32; 3]) -> Result<Mesh, String> {
    let step = options.voxel_size;

    let mut samples = vec![0.0; SIDE * SIDE * SIDE];
    for z in 0..SIDE {
        for y in 0..SIDE {
            for x in 0..SIDE {
                samples[index(x, y, z)] = density(
                    cave,
                    options,
                    (Vec3::new(
                        (key[0] * CELLS + x as i32) as f32,
                        (key[1] * CELLS + y as i32) as f32,
                        (key[2] * CELLS + z as i32) as f32,
                    ) + Vec3::Z * 0.173)
                        * step,
                );
            }
        }
    }
    let mut mesh = Mesh::default();
    let mut vertices = HashMap::<(usize, usize), u32>::new();
    for z in 0..CELLS as usize {
        for y in 0..CELLS as usize {
            for x in 0..CELLS as usize {
                let ids =
                    CORNERS.map(|c| index(x + c[0] as usize, y + c[1] as usize, z + c[2] as usize));
                let values = ids.map(|i| samples[i]);
                if values.iter().all(|v| *v >= 0.0) || values.iter().all(|v| *v < 0.0) {
                    continue;
                }
                let positions = CORNERS.map(|c| {
                    (Vec3::new(
                        (key[0] * CELLS + x as i32 + c[0]) as f32,
                        (key[1] * CELLS + y as i32 + c[1]) as f32,
                        (key[2] * CELLS + z as i32 + c[2]) as f32,
                    ) + Vec3::Z * 0.173)
                        * step
                });
                let mut vertex_ids = [u32::MAX; 12];
                for (edge, [a, b]) in EDGES.iter().copied().enumerate() {
                    if (values[a] >= 0.0) == (values[b] >= 0.0) {
                        continue;
                    }
                    let (a, b) = if ids[a] < ids[b] { (a, b) } else { (b, a) };
                    let key = (ids[a], ids[b]);
                    vertex_ids[edge] = *vertices.entry(key).or_insert_with(|| {
                        let t = values[a] / (values[a] - values[b]);
                        let p = positions[a].lerp(positions[b], t);
                        let id = mesh.vertices.len() as u32;
                        mesh.vertices.push(p / ISLAND_WORLD_METRES);
                        mesh.normals.push(normal(cave, options, p));
                        mesh.uv.push(p.truncate() / ISLAND_WORLD_METRES);
                        id
                    });
                }
                let mut adjacent = [[usize::MAX; 2]; 12];
                let mut degree = [0; 12];
                for (corners, edges) in FACES {
                    let crossed: Vec<_> = edges
                        .into_iter()
                        .filter(|&e| vertex_ids[e] != u32::MAX)
                        .collect();
                    let mut connect = |a: usize, b: usize| {
                        if degree[a] < 2 && degree[b] < 2 {
                            adjacent[a][degree[a]] = b;
                            degree[a] += 1;
                            adjacent[b][degree[b]] = a;
                            degree[b] += 1;
                        }
                    };
                    match crossed.as_slice() {
                        [a, b] => connect(*a, *b),
                        [_, _, _, _] => {
                            let q = values[corners[0]] * values[corners[2]]
                                - values[corners[1]] * values[corners[3]];
                            if q >= 0.0 {
                                connect(edges[0], edges[1]);
                                connect(edges[2], edges[3]);
                            } else {
                                connect(edges[0], edges[3]);
                                connect(edges[1], edges[2]);
                            }
                        }
                        [] => {}
                        _ => return Err("invalid cube face contour".into()),
                    }
                }
                let mut visited = [false; 12];
                for start in 0..12 {
                    if vertex_ids[start] == u32::MAX || visited[start] {
                        continue;
                    }
                    let mut ring = Vec::new();
                    let mut previous = usize::MAX;
                    let mut current = start;
                    loop {
                        if degree[current] != 2 {
                            return Err("open cube contour".into());
                        }
                        if visited[current] {
                            break;
                        }
                        visited[current] = true;
                        ring.push(vertex_ids[current]);
                        let next = adjacent[current]
                            .into_iter()
                            .find(|&n| n != previous)
                            .unwrap();
                        previous = current;
                        current = next;
                    }
                    if current != start {
                        return Err("self-intersecting cube contour".into());
                    }
                    // A three-edge contour is already a triangle. Adding a
                    // normalized-coordinate centroid can round onto its edge
                    // for tiny corner crossings and leave a pinhole.
                    if ring.len() == 3 {
                        let [a, b, c] = [ring[0], ring[1], ring[2]];
                        mesh.triangles.extend([a, b, c]);
                        continue;
                    }
                    let centre = ring
                        .iter()
                        .map(|&i| mesh.vertices[i as usize])
                        .sum::<Vec3>()
                        / ring.len() as f32;
                    let centre_id = mesh.vertices.len() as u32;
                    mesh.vertices.push(centre);
                    mesh.normals
                        .push(normal(cave, options, centre * ISLAND_WORLD_METRES));
                    mesh.uv.push(centre.truncate());
                    for i in 0..ring.len() {
                        let a = ring[i];
                        let b = ring[(i + 1) % ring.len()];
                        let normal = (mesh.vertices[a as usize] - centre)
                            .cross(mesh.vertices[b as usize] - centre);
                        if normal.length_squared() < 1.0e-24 {
                            continue;
                        }
                        mesh.triangles.extend([centre_id, a, b]);
                    }
                }
            }
        }
    }
    Ok(mesh)
}
