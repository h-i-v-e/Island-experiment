//! Swept, closed passage walls with a single arched opening at the throat.
//! No exterior terrain or roof geometry is generated here.
use super::{Cave, CaveOptions, field, stitch};
use crate::{ISLAND_WORLD_METRES, Mesh, Vec2, Vec3};

pub(crate) fn section(width: f32, height: f32) -> Vec<Vec2> {
    (0..=8)
        .map(|i| Vec2::new(width * (i as f32 / 8.0 - 0.5), 0.0))
        .chain((0..=48).map(|i| {
            let a = i as f32 * std::f32::consts::PI / 48.0;
            Vec2::new(a.cos() * width * 0.5, height * (0.35 + 0.65 * a.sin()))
        }))
        .collect()
}
pub(crate) fn build(cave: &Cave, o: &CaveOptions) -> Mesh {
    let mut mesh = Mesh::default();
    let mut ring_count = 0;
    let sides = cave.portal.section.len();
    for (segment, pair) in cave.nodes[1..].windows(2).enumerate() {
        let count = (pair[0].floor.distance(pair[1].floor) / o.voxel_size).ceil() as usize;
        for i in 0..=count {
            if segment > 0 && i == 0 {
                continue;
            }
            let t = i as f32 / count.max(1) as f32;
            let floor = if ring_count == 0 {
                cave.portal.throat()
            } else {
                pair[0].floor.lerp(pair[1].floor, t)
            };
            let direction = if ring_count == 0 {
                cave.inward
            } else {
                (pair[1].floor - pair[0].floor).truncate().normalize()
            };
            let right = Vec3::new(-direction.y, direction.x, 0.0);
            let width = pair[0].width + (pair[1].width - pair[0].width) * t;
            let height = pair[0].height + (pair[1].height - pair[0].height) * t;
            let fade = ((floor - cave.portal.throat()).length() / 3.0).clamp(0.0, 1.0);
            for (side, q) in section(width, height).into_iter().enumerate() {
                let mut p = floor + right * q.x + Vec3::Z * q.y;
                let rough = (field::noise3(cave.id, p / o.broad_period) * o.broad_amplitude
                    + field::noise3(cave.id ^ 0x4649_4e45, p / o.fine_period) * o.fine_amplitude)
                    * fade;
                if side <= 8 {
                    p.z += field::noise3(cave.id ^ 0x464c_4f4f, p / 3.0) * o.floor_roughness * fade;
                } else {
                    let radial =
                        (right * q.x + Vec3::Z * (q.y - height * 0.35)).normalize_or_zero();
                    p += radial * rough * (q.y / 1.0).clamp(0.0, 1.0);
                }
                mesh.vertices.push(p / ISLAND_WORLD_METRES);
                mesh.uv.push(p.truncate() / ISLAND_WORLD_METRES);
                mesh.normals.push(Vec3::ZERO);
            }
            if ring_count > 0 {
                let base = (ring_count * sides) as u32;
                let prev = base - sides as u32;
                for side in 0..sides {
                    let a = side as u32;
                    let b = ((side + 1) % sides) as u32;
                    mesh.triangles.extend([
                        prev + a,
                        base + a,
                        base + b,
                        prev + a,
                        base + b,
                        prev + b,
                    ]);
                }
            }
            ring_count += 1;
        }
    }
    let base = ((ring_count - 1) * sides) as u32;
    // Close beyond the chamber's standing destination. Use the section's
    // geometric centre, independent of how finely the arch is sampled.
    let last = cave.nodes.last().unwrap();
    let direction = (last.floor - cave.nodes[cave.nodes.len() - 2].floor).normalize();
    let centre = (last.floor + Vec3::Z * (last.height * 0.35) + direction * (last.width * 0.5))
        / ISLAND_WORLD_METRES;
    let centre_id = mesh.vertices.len() as u32;
    mesh.vertices.push(centre);
    mesh.uv.push(centre.truncate());
    mesh.normals.push(Vec3::ZERO);
    for side in 0..sides {
        mesh.triangles.extend([
            centre_id,
            base + ((side + 1) % sides) as u32,
            base + side as u32,
        ]);
    }
    mesh.calculate_normals();
    // The lip and passage must agree at their shared ring, even when the
    // following route widens or bends and changes the averaged mesh normals.
    for side in 0..sides {
        mesh.normals[side] = cave
            .portal
            .passage_normal(mesh.vertices[side] * ISLAND_WORLD_METRES, side <= 8);
    }
    let (min, max) = cave.portal.throat_bounds();
    stitch::repair(
        &mut mesh,
        min,
        max,
        cave.portal.throat_anchors(&cave.entrance_collider.vertices),
    );
    mesh
}
