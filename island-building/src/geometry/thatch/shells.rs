//! Four offsets along the cut reeds, independent of smooth lighting normals.
use super::{Builder, Coat, Vertex};
use crate::MaterialSlot;

pub(super) fn emit(builder: &mut Builder, coat: &Coat<'_>, vertices: &[Vertex]) {
    for (layer, slot) in MaterialSlot::THATCH_SHELLS.into_iter().enumerate() {
        let distance = (f32::from(u8::try_from(layer).expect("four shells")) + 1.) * 0.005;
        let vertices: Vec<_> = vertices
            .iter()
            .copied()
            .map(|mut vertex| {
                vertex.point =
                    std::array::from_fn(|i| vertex.point[i] + vertex.strand[i] * distance);
                vertex
            })
            .collect();
        coat.emit_vertices(builder, &vertices, slot);
    }
}
