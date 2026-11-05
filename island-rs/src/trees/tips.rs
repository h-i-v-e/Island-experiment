//! Close grown LOD0 rims with a short continuation without moving their wood.
use crate::{ISLAND_WORLD_METRES, Mesh, Vec3};

pub(super) fn append_pointed_tip(
    mesh: &mut Mesh,
    rim: &[u32],
    direction: Vec3,
    last_span: f32,
) -> u32 {
    let direction = direction.normalize_or(Vec3::Z);
    let count = f32::from(u16::try_from(rim.len()).expect("tree tip rim fits u16"));
    let centre = rim
        .iter()
        .map(|&vertex| mesh.vertices[vertex as usize])
        .sum::<Vec3>()
        / count;
    let radius = rim
        .iter()
        .map(|&vertex| mesh.vertices[vertex as usize].distance(centre))
        .sum::<f32>()
        / count;
    let length = (radius * 5.0)
        .clamp(0.12 / ISLAND_WORLD_METRES, 0.8 / ISLAND_WORLD_METRES)
        .min(last_span * 0.5);
    assert!(length.is_finite() && length > f32::EPSILON);
    let apex = u32::try_from(mesh.vertices.len()).expect("tree tip index fits u32");
    mesh.vertices.push(centre + direction * length);
    mesh.uv.push(super::encode_bark_axis(direction));
    for side in 0..rim.len() {
        mesh.triangles
            .extend([rim[side], rim[(side + 1) % rim.len()], apex]);
    }
    apex
}
