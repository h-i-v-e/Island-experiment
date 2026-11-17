//! Continue an attachment's roof pitch into the host, then clip their intersection.
use super::{
    clip::{self, Plane, Vertex},
    node::Node,
};
use crate::{BuildingLod, BuildingMesh, BuildingRecipe, MaterialSlot};

struct Tail {
    axis: usize,
    interface: f32,
    back: f32,
    slope: f32,
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
        let tail = Tail {
            axis,
            interface,
            back,
            slope: if node.placement.lean {
                node.recipe.roof.rise / node.recipe.width
            } else {
                0.
            },
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

/// Distant roofs share an atlas with walls and foundations. Chart ownership
/// also identifies their vertical eave/end faces, which have no upward normal.
pub(super) fn roof_surface(
    slot: MaterialSlot,
    vertices: &[Vertex],
    recipe: &BuildingRecipe,
) -> bool {
    if slot == MaterialSlot::Atlas {
        let row = if recipe.chimney.is_some() { 0.25 } else { 0.5 };
        return vertices
            .iter()
            .all(|v| (row..row * 2.).contains(&v.uv[1]) && (v.uv[0] < 0.5 || v.uv[0] >= 0.75));
    }
    roof_slot(slot)
        || (slot == MaterialSlot::Interior
            && vertices[0].normal[2] < -0.01
            && vertices
                .iter()
                .any(|v| v.point[2] >= recipe.eaves_height() - 0.01))
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
        let roof = roof_surface(slot, &vertices, &node.recipe);
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
            // Gabled wings keep their ridge level. A lean-to continues its
            // authored slope through the same horizontal extension. Applying
            // one transform to every layer preserves the closed roof section.
            vertex.point[2] += (vertex.point[axis] - original) * tail.slope;
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
