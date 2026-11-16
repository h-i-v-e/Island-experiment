//! A buried rear roof transition avoids an exposed gable ending on the main roof.
use super::{
    clip::{self, Plane, Vertex},
    node::Node,
};
use crate::{BuildingLod, BuildingMesh, MaterialSlot};

struct Tail {
    axis: usize,
    interface: f32,
    back: f32,
    drop: f32,
}

pub(super) fn reshape(lods: &mut [BuildingLod; 3], node: &Node) {
    let axis = usize::from(!node.placement.lean);
    let overlap = node.recipe.wall_thickness * 2. + 0.04;
    let interface = node.placement.normal_depth * 0.5 - overlap;
    for lod in lods {
        let back = lod
            .parts
            .iter()
            .filter(|p| roof_slot(p.material))
            .flat_map(|p| &p.mesh.positions)
            .map(|p| p[axis])
            .fold(interface, f32::max);
        let top = lod
            .parts
            .iter()
            .filter(|p| roof_slot(p.material))
            .flat_map(|p| &p.mesh.positions)
            .map(|p| p[2])
            .fold(node.recipe.wall_height, f32::max);
        let tail = Tail {
            axis,
            interface,
            back,
            drop: top - node.recipe.wall_height,
        };
        for part in &mut lod.parts {
            part.mesh = reshape_mesh(&part.mesh, part.material, node, &tail);
        }
    }
}

fn roof_slot(slot: MaterialSlot) -> bool {
    matches!(
        slot,
        MaterialSlot::Roof
            | MaterialSlot::ThatchEnd
            | MaterialSlot::ThatchShell1
            | MaterialSlot::ThatchShell2
            | MaterialSlot::ThatchShell3
            | MaterialSlot::ThatchShell4
            | MaterialSlot::Atlas
    )
}

fn reshape_mesh(
    source: &BuildingMesh,
    slot: MaterialSlot,
    node: &Node,
    tail: &Tail,
) -> BuildingMesh {
    let mut output = BuildingMesh::default();
    let Tail {
        axis,
        interface,
        back,
        ..
    } = *tail;
    let half = node.placement.normal_depth * 0.5;
    for tri in source.indices.chunks_exact(3) {
        let vertices: Vec<_> = tri
            .iter()
            .map(|&i| {
                let i = i as usize;
                Vertex {
                    point: source.positions[i],
                    normal: source.normals[i],
                    uv: source.uvs[i],
                }
            })
            .collect();
        let n = vertices[0].normal;
        let back_face = matches!(
            slot,
            MaterialSlot::Wall | MaterialSlot::Interior | MaterialSlot::Atlas
        ) && n[axis].abs() > 0.99
            && [half, half - node.recipe.wall_thickness]
                .iter()
                .any(|edge| {
                    vertices
                        .iter()
                        .all(|v| (v.point[axis] - edge).abs() < 0.0001)
                });
        if back_face {
            continue;
        }
        let roof = roof_slot(slot) && (slot != MaterialSlot::Atlas || n[2].abs() > 0.1)
            || (slot == MaterialSlot::Interior
                && n[2] < -0.01
                && vertices
                    .iter()
                    .any(|v| v.point[2] >= node.recipe.wall_height - 0.01));
        if !roof {
            clip::append(&mut output, &vertices);
            continue;
        }
        let mut normal = [0.; 3];
        normal[axis] = 1.;
        let (front, mut rear) = clip::split(
            &vertices,
            Plane {
                normal,
                distance: interface,
            },
        );
        if front.len() >= 3 {
            clip::append(&mut output, &front);
        }
        if rear.len() < 3 {
            continue;
        }
        for vertex in &mut rear {
            let original = vertex.point[axis];
            let progress = ((original - interface) / (back - interface).max(0.0001)).clamp(0., 1.);
            vertex.point[axis] = interface + progress * node.placement.roof_reach;
            // One affine shear preserves contacts between roof layers and the
            // ridge cap. Scaling height by progress would warp their separately
            // triangulated surfaces differently and open cracks between them.
            vertex.point[2] -= tail.drop * progress;
            if slot == MaterialSlot::Roof {
                vertex.uv[usize::from(axis == 1)] += (vertex.point[axis] - original) * 0.5;
            }
        }
        append_warped(&mut output, &rear);
    }
    output
}

fn append_warped(output: &mut BuildingMesh, polygon: &[Vertex]) {
    for index in 1..polygon.len() - 1 {
        let mut tri = [polygon[0], polygon[index], polygon[index + 1]];
        let a = std::array::from_fn::<_, 3, _>(|i| tri[1].point[i] - tri[0].point[i]);
        let b = std::array::from_fn::<_, 3, _>(|i| tri[2].point[i] - tri[0].point[i]);
        let normal = [
            a[1] * b[2] - a[2] * b[1],
            a[2] * b[0] - a[0] * b[2],
            a[0] * b[1] - a[1] * b[0],
        ];
        let length = normal.iter().map(|v| v * v).sum::<f32>().sqrt();
        if length < 0.00001 {
            continue;
        }
        for vertex in &mut tri {
            vertex.normal = normal.map(|v| v / length);
        }
        clip::append(output, &tri);
    }
}
