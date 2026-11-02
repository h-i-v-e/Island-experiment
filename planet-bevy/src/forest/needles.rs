//! Shared near-pine needle volume with an irregular, branch-following fringe.
#![allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
use bevy::{
    asset::RenderAssetUsages,
    mesh::{Indices, PrimitiveTopology},
    prelude::*,
};
use motu::{Mesh as SourceMesh, Vec3 as Point};
use std::f32::consts::{PI, TAU};

pub(super) const LAYERS: usize = 4;
pub(super) const LENGTH: f32 = 0.12;

/// Borrow one fine crown; geometry and stable physical UVs are shared by every
/// instance of that variant. Fibres lean out and down along the branches.
pub(super) fn crown(source: &SourceMesh) -> Mesh {
    let mut points = Vec::new();
    let mut normals = Vec::new();
    let mut uv = Vec::new();
    let mut data = Vec::new();
    let mut indices = Vec::new();
    for layer in 1..=LAYERS {
        let fraction = layer as f32 / LAYERS as f32;
        for face in source.triangles.chunks_exact(3) {
            let vertices: [Point; 3] = std::array::from_fn(|i| source.vertices[face[i] as usize]);
            let angles = vertices.map(|p| p.y.atan2(p.x));
            let wrap = angles.iter().copied().fold(f32::NEG_INFINITY, f32::max)
                - angles.iter().copied().fold(f32::INFINITY, f32::min)
                > PI;
            let whorl = (face[0] as usize / 27) * 27;
            let radius = source.vertices[whorl + 9].truncate().length() * motu::ISLAND_WORLD_METRES;
            for (index, &id) in face.iter().enumerate() {
                let point = source.vertices[id as usize];
                let normal = source.normals[id as usize];
                let angle = angles[index]
                    + if wrap && angles[index] < 0.0 {
                        TAU
                    } else {
                        0.0
                    };
                indices.push(points.len() as u32);
                points.push(
                    (point
                        + needle_direction(point, normal)
                            * (fraction * LENGTH / motu::ISLAND_WORLD_METRES))
                        .to_array(),
                );
                normals.push(normal.to_array());
                uv.push([angle * radius, point.z * motu::ISLAND_WORLD_METRES]);
                data.push([fraction, 1.0, 0.0, 0.0]);
            }
        }
    }
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD,
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, points)
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
    .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uv)
    .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, data)
    .with_inserted_indices(Indices::U32(indices))
}
fn needle_direction(point: Point, normal: Point) -> Point {
    let radial = Point::new(point.x, point.y, 0.0).normalize_or_zero();
    // Follow the branch instead of projecting straight out from the crown.
    (normal * 0.60 + radial * 0.85 - Point::Z * 0.35).normalize()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shared_needle_volume_has_four_stable_layers_and_a_fixed_triangle_budget() {
        use bevy::mesh::VertexAttributeValues;
        for variant in 0..4 {
            let tree = super::super::models::conifer(variant);
            let detail = crown(&tree.lod0_foliage);
            assert_eq!(detail.indices().unwrap().len() / 3, 1152);
            let points = detail
                .attribute(Mesh::ATTRIBUTE_POSITION)
                .unwrap()
                .as_float3()
                .unwrap();
            let normal = detail
                .attribute(Mesh::ATTRIBUTE_NORMAL)
                .unwrap()
                .as_float3()
                .unwrap();
            assert!(points.iter().all(|p| Vec3::from_array(*p).is_finite()));
            assert!(
                normal
                    .iter()
                    .all(|p| (Vec3::from_array(*p).length() - 1.0).abs() < 0.001)
            );
            let VertexAttributeValues::Float32x4(data) =
                detail.attribute(Mesh::ATTRIBUTE_COLOR).unwrap()
            else {
                panic!("missing layer data")
            };
            let per_layer = tree.lod0_foliage.triangles.len();
            for layer in 0..LAYERS {
                let start = layer * per_layer;
                assert!(data[start..start + per_layer].iter().all(|d| d[0].to_bits()
                    == ((layer + 1) as f32 / LAYERS as f32).to_bits()
                    && d[3].abs() < 0.01));
                for (vertex, &id) in tree.lod0_foliage.triangles.iter().enumerate() {
                    let delta = Point::from_array(points[start + vertex])
                        - tree.lod0_foliage.vertices[id as usize];
                    assert!(
                        delta.normalize().dot(needle_direction(
                            tree.lod0_foliage.vertices[id as usize],
                            tree.lod0_foliage.normals[id as usize]
                        )) > 0.999
                    );
                    assert!(
                        (delta.length() * motu::ISLAND_WORLD_METRES
                            - (layer + 1) as f32 / LAYERS as f32 * LENGTH)
                            .abs()
                            < 0.001
                    );
                }
            }
        }
    }
}
