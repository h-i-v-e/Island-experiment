use super::{
    ExtensionLayout, atlas,
    clip::{self, Vertex},
    node::{self, Node, Placement},
    validation,
};
use crate::{
    BakedAtlas, BuildingError, BuildingLod, BuildingMesh, BuildingPrototype, BuildingRecipe,
    MaterialSlot, OpeningKind, collision::BoxCollider, geometry::Builder,
};
use std::{borrow::Cow, path::Path};

struct Component {
    lods: [BuildingLod; 3],
    atlas: BakedAtlas,
    colliders: Vec<BoxCollider>,
    supports: Vec<BoxCollider>,
}

pub(super) fn generate(
    recipe: &BuildingRecipe,
    base: &Path,
) -> Result<BuildingPrototype, BuildingError> {
    let mut parent = recipe.clone();
    parent
        .openings
        .retain(|o| o.kind != OpeningKind::Window || !validation::covered(recipe, o));
    let root = crate::generate_standard(Cow::Owned(parent), base)?;
    validation::connections(&root.recipe)?;
    let resolved = root.recipe.clone();
    let bottom = crate::geometry::foundation::bounds(&resolved).min[2];
    let header = crate::geometry::ceiling_height(&crate::storeys::recipe_for_storey(&resolved, 0))
        - resolved.walls.beam_width;
    let mut nodes = vec![Node::new(
        root.recipe.clone(),
        Placement::identity(),
        bottom,
        header,
    )];
    let BuildingPrototype {
        lods,
        atlas,
        colliders,
        support_colliders,
        stair_colliders,
        mut doors,
        materials,
        mut layout,
        ..
    } = root;
    let mut components = vec![Component {
        lods,
        atlas,
        colliders,
        supports: support_colliders,
    }];
    for extension in &resolved.extensions {
        let (child, placement) = node::child(&resolved, extension);
        let node = Node::new(child, placement, bottom, header);
        if node.room_bounds[0].max[2] < 2. {
            return Err(BuildingError::new(
                &extension.id,
                "Raise the extension walls to provide at least 2 m of interior headroom.",
            ));
        }
        components.push(component(&node, &materials, base, bottom)?);
        nodes.push(node);
    }
    for (node, component) in nodes.iter_mut().zip(&components) {
        node.set_surfaces(&component.lods, bottom);
    }
    let mut lods = merge(&components, &nodes, resolved.atlas_size);
    let mut colliders = Vec::new();
    let mut supports = Vec::new();
    for (index, component) in components.iter().enumerate() {
        colliders.extend(collision(&component.colliders, &nodes, index));
        supports.extend_from_slice(&component.supports);
    }
    let connections =
        super::rooms::generate(&resolved, &nodes, &mut lods[0], &mut colliders, &mut doors)?;
    crate::validation::validate_door_swings(&doors, &colliders)?;
    extend_layout(&mut layout, &resolved, &nodes, &components, &connections);
    let atlases: Vec<_> = components.into_iter().map(|c| c.atlas).collect();
    let atlas = atlas::pack(&atlases, resolved.atlas_size);
    let hash = crate::materials::prototype_hash(&resolved, &materials)?;
    Ok(BuildingPrototype {
        recipe: resolved,
        layout,
        lods,
        doors,
        colliders,
        support_colliders: supports,
        stair_colliders,
        atlas,
        materials,
        hash,
    })
}

fn extend_layout(
    layout: &mut crate::layout::BuildingLayout,
    recipe: &BuildingRecipe,
    nodes: &[Node],
    components: &[Component],
    connections: &[super::rooms::Connection],
) {
    layout.occupied_bounds.push(BoxCollider {
        min: layout.bounds_min,
        max: layout.bounds_max,
    });
    for (index, node) in nodes.iter().enumerate().skip(1) {
        let bounds = mesh_bounds(&components[index].lods[0]);
        layout.occupied_bounds.push(bounds);
        for axis in 0..3 {
            layout.bounds_min[axis] = layout.bounds_min[axis].min(bounds.min[axis]);
            layout.bounds_max[axis] = layout.bounds_max[axis].max(bounds.max[axis]);
        }
        let e = &recipe.extensions[index - 1];
        layout.extensions.push(ExtensionLayout {
            id: e.id.clone(),
            kind: e.kind,
            facade: e.facade,
            bounds,
            room_bounds: node.room_bounds[0],
            connection: connection(recipe, e, node.room_bounds[0].max[2]),
            door_id: connections[index - 1].id.clone(),
            doorway: Some(connections[index - 1].bounds),
            window_centres: node
                .recipe
                .openings
                .iter()
                .filter(|o| o.kind == OpeningKind::Window)
                .map(|o| {
                    node.placement.point(o.facade.point(
                        &node.recipe,
                        o.offset,
                        o.sill + o.height * 0.5,
                        0.,
                    ))
                })
                .collect(),
        });
    }
    layout.room_count += recipe.extensions.len();
    let footprint = super::footprint(recipe);
    layout.footprint = [
        footprint.max[0] - footprint.min[0],
        footprint.max[1] - footprint.min[1],
    ];
}

fn component(
    node: &Node,
    materials: &[crate::MaterialTexture],
    base: &Path,
    bottom: f32,
) -> Result<Component, BuildingError> {
    let r = &node.recipe;
    let pitch = if r.roof.covering == crate::RoofCovering::Slate {
        crate::units::MaterialGrid::load(r, base, MaterialSlot::Roof)?.metres[0]
    } else {
        0.25
    };
    let (near, _, source_colliders) =
        crate::geometry::generate_attachment(r, pitch, node.placement.lean);
    let mut lods = [
        near,
        crate::lod::middle_attachment(r, node.placement.lean),
        crate::lod::distant_attachment(r, node.placement.lean),
    ];
    super::junction::reshape(&mut lods, node);
    let old_bottom = crate::geometry::foundation::bounds(r).min[2];
    for lod in &mut lods {
        for part in &mut lod.parts {
            for index in 0..part.mesh.positions.len() {
                if part.material == MaterialSlot::Foundation
                    && (part.mesh.positions[index][2] - old_bottom).abs() < 0.0001
                {
                    part.mesh.positions[index][2] = bottom;
                    if part.mesh.normals[index][2].abs() < 0.5 {
                        part.mesh.uvs[index][1] = bottom * 0.5;
                    }
                }
                part.mesh.positions[index] = node.placement.point(part.mesh.positions[index]);
                part.mesh.normals[index] = node.placement.vector(part.mesh.normals[index]);
            }
        }
    }
    let colliders = source_colliders
        .into_iter()
        .map(|mut b| {
            if (b.min[2] - old_bottom).abs() < 0.0001 {
                b.min[2] = bottom;
            }
            node.placement.transform_box(b)
        })
        .collect();
    let mut supports = Vec::new();
    let mut floor = crate::geometry::foundation::bounds(r);
    floor.min[2] = -r.floor_thickness;
    supports.push(node.placement.transform_box(floor));
    Ok(Component {
        lods,
        atlas: crate::bake::bake(r, materials),
        colliders,
        supports,
    })
}

fn merge(components: &[Component], nodes: &[Node], size: u32) -> [BuildingLod; 3] {
    std::array::from_fn(|level| {
        let mut builder = Builder::default();
        for (index, component) in components.iter().enumerate() {
            let mut lod = BuildingLod {
                parts: component.lods[level]
                    .parts
                    .iter()
                    .map(|part| crate::MeshPart {
                        material: part.material,
                        mesh: mesh(&part.mesh, part.material, nodes, index, level),
                    })
                    .collect(),
            };
            if level == 2 {
                atlas::remap(&mut lod, index, nodes.len(), size);
            }
            builder.append(lod, 0.);
        }
        let mut lod = builder.finish();
        crate::finalize::lod(&mut lod);
        lod
    })
}

fn mesh(
    source: &BuildingMesh,
    slot: MaterialSlot,
    nodes: &[Node],
    owner: usize,
    level: usize,
) -> BuildingMesh {
    let mut result = BuildingMesh::default();
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
        let roof = super::junction::roof_surface(slot, &vertices, &nodes[owner].recipe);
        let underside = roof && vertices[0].normal[2] < -0.01;
        if slot == MaterialSlot::Interior
            && !underside
            && !nodes[owner].inner_wall(vertices[0].point, vertices[0].normal)
        {
            clip::append(&mut result, &vertices);
            continue;
        }
        let bounds = BoxCollider {
            min: std::array::from_fn(|axis| {
                vertices
                    .iter()
                    .map(|v| v.point[axis])
                    .fold(f32::INFINITY, f32::min)
            }),
            max: std::array::from_fn(|axis| {
                vertices
                    .iter()
                    .map(|v| v.point[axis])
                    .fold(f32::NEG_INFINITY, f32::max)
            }),
        };
        let mut pieces = vec![vertices];
        if underside {
            for node in nodes {
                pieces = pieces
                    .into_iter()
                    .flat_map(|p| clip::subtract(p, &node.wall_volume, false))
                    .collect();
            }
        }
        for (index, node) in nodes.iter().enumerate() {
            // Outside every occupied wall footprint, a roof's underside is
            // exposed. A neighbour's ground-to-roof envelope also contains air
            // below its eaves, so it cannot be used to remove these soffits.
            if index == owner || underside {
                continue;
            }
            let volumes = volumes(node, slot, owner, level, bounds, roof);
            for volume in volumes {
                pieces = pieces
                    .into_iter()
                    .flat_map(|p| clip::subtract(p, volume, owner < index))
                    .collect();
                if pieces.is_empty() {
                    break;
                }
            }
            if pieces.is_empty() {
                break;
            }
        }
        if slot == MaterialSlot::Foundation && source.normals[tri[0] as usize][2] > 0.5 {
            for node in nodes {
                for volume in &node.floors {
                    pieces = pieces
                        .into_iter()
                        .flat_map(|p| clip::subtract(p, volume, false))
                        .collect();
                }
            }
        }
        for polygon in pieces {
            clip::append(&mut result, &polygon);
        }
    }
    result
}

fn volumes(
    node: &Node,
    slot: MaterialSlot,
    owner: usize,
    level: usize,
    bounds: BoxCollider,
    roof: bool,
) -> super::index::Query<'_> {
    use super::index::Query;
    if roof {
        return node.roof[level].query(bounds);
    }
    match slot {
        // Open the shared wall only through the occupied room. Retain its
        // loft infill behind separate roof layers and raised thatch ridge caps.
        MaterialSlot::Wall | MaterialSlot::Atlas if owner == 0 => Query::all(&node.rooms, bounds),
        MaterialSlot::Floor => Query::all(&node.floors, bounds),
        MaterialSlot::Ceiling | MaterialSlot::Interior | MaterialSlot::Trim => {
            Query::all(&node.rooms, bounds)
        }
        MaterialSlot::Timber if owner == 0 => Query::all(&node.below_header, bounds),
        MaterialSlot::Timber => Query::all(&node.timber_rooms, bounds),
        MaterialSlot::Foundation => Query::all(std::slice::from_ref(&node.foundation), bounds),
        _ => node.outer[level].query(bounds),
    }
}

fn collision(source: &[BoxCollider], nodes: &[Node], owner: usize) -> Vec<BoxCollider> {
    source
        .iter()
        .flat_map(|&shape| {
            if shape.max[2] <= 0.0001 {
                return vec![shape];
            }
            let mut pieces = vec![shape];
            for (index, node) in nodes.iter().enumerate() {
                if index == owner {
                    continue;
                }
                for &room in &node.room_bounds {
                    pieces = pieces
                        .into_iter()
                        .flat_map(|b| clip::box_difference(b, room))
                        .collect();
                }
            }
            pieces
        })
        .collect()
}

fn mesh_bounds(lod: &BuildingLod) -> BoxCollider {
    let mut bounds = BoxCollider {
        min: [f32::INFINITY; 3],
        max: [f32::NEG_INFINITY; 3],
    };
    for point in lod.parts.iter().flat_map(|p| &p.mesh.positions) {
        for (axis, &value) in point.iter().enumerate() {
            bounds.min[axis] = bounds.min[axis].min(value);
            bounds.max[axis] = bounds.max[axis].max(value);
        }
    }
    bounds
}

fn connection(parent: &BuildingRecipe, e: &super::ExtensionRecipe, height: f32) -> BoxCollider {
    let half = e.width * 0.5 - parent.wall_thickness;
    let a = e
        .facade
        .point(parent, e.offset - half, 0., parent.wall_thickness + 0.04);
    let b = e.facade.point(parent, e.offset + half, height, -0.04);
    BoxCollider {
        min: std::array::from_fn(|i| a[i].min(b[i])),
        max: std::array::from_fn(|i| a[i].max(b[i])),
    }
}
