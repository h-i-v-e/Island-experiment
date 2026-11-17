//! Every attachment has one clean dividing wall and a retained internal door.
use super::{
    ExtensionRecipe,
    clip::{self, Plane, Vertex},
    node::Node,
};
use crate::{
    BuildingError, BuildingLod, BuildingRecipe, DoorDescriptor, HingeSide, MaterialSlot,
    OpeningKind, OpeningRecipe,
    collision::BoxCollider,
    geometry::{
        self, Builder,
        interior_wall::{self, Doorway},
    },
};

pub(super) struct Connection {
    pub id: String,
    pub bounds: BoxCollider,
}

pub(super) fn generate(
    recipe: &BuildingRecipe,
    nodes: &[Node],
    near: &mut BuildingLod,
    colliders: &mut Vec<BoxCollider>,
    doors: &mut Vec<DoorDescriptor>,
) -> Result<Vec<Connection>, BuildingError> {
    let mut builder = Builder::default();
    builder.append(std::mem::take(near), 0.);
    let mut connections = Vec::with_capacity(recipe.extensions.len());
    let parent = crate::storeys::recipe_for_storey(recipe, 0);
    let header = geometry::ceiling_height(&parent) - recipe.walls.beam_width;
    for (extension, node) in recipe.extensions.iter().zip(nodes.iter().skip(1)) {
        let bounds = wall_bounds(recipe, extension, node.room_bounds[0].max[2]);
        let (opening, mut leaf) = select_door(recipe, extension, bounds, colliders)?;
        let axis = interior_wall::axis(bounds);
        let point = extension.facade.point(recipe, opening.offset, 0., 0.);
        let doorway = Doorway {
            centre: point[axis],
            width: opening.width,
            height: opening.height,
        };
        let mut wall = Builder::default();
        interior_wall::generate(&mut wall, recipe, bounds, Some(doorway), colliders);
        geometry::frame(&mut wall, recipe, &opening, false);
        let mut lod = wall.finish();
        trim_header(&mut lod, header);
        builder.append(lod, 0.);
        crate::finalize::door(&mut leaf);
        let mut aperture = bounds;
        aperture.min[axis] = doorway.centre - doorway.width * 0.5;
        aperture.max[axis] = doorway.centre + doorway.width * 0.5;
        aperture.max[2] = doorway.height;
        connections.push(Connection {
            id: leaf.id.clone(),
            bounds: aperture,
        });
        doors.push(leaf);
    }
    *near = builder.finish();
    crate::finalize::lod(near);
    Ok(connections)
}

fn wall_bounds(recipe: &BuildingRecipe, extension: &ExtensionRecipe, height: f32) -> BoxCollider {
    let half = extension.width * 0.5 - recipe.wall_thickness;
    let a = extension
        .facade
        .point(recipe, extension.offset - half, 0., 0.);
    let b = extension.facade.point(
        recipe,
        extension.offset + half,
        height,
        recipe.wall_thickness,
    );
    BoxCollider {
        min: std::array::from_fn(|i| a[i].min(b[i])),
        max: std::array::from_fn(|i| a[i].max(b[i])),
    }
}

fn select_door(
    recipe: &BuildingRecipe,
    extension: &ExtensionRecipe,
    bounds: BoxCollider,
    colliders: &[BoxCollider],
) -> Result<(OpeningRecipe, DoorDescriptor), BuildingError> {
    let size = recipe.opening_defaults.door;
    let margin = (extension.width - 2. * recipe.wall_thickness - size.width) * 0.5 - 0.09;
    if margin < 0. || size.height + 0.09 > bounds.max[2] {
        return Err(BuildingError::new(
            &extension.id,
            "The shared door dimensions must fit the extension wall below its ceiling; widen or raise the extension, or adjust the building-wide door dimensions.",
        ));
    }
    for fraction in [0., -0.5, 0.5, -1., 1., -0.25, 0.25, -0.75, 0.75] {
        for hinge in [HingeSide::Left, HingeSide::Right] {
            let opening = OpeningRecipe {
                id: format!("extension-door-{}", extension.id),
                storey: 0,
                facade: extension.facade,
                kind: OpeningKind::Door,
                offset: extension.offset + margin * fraction,
                width: size.width,
                height: size.height,
                sill: 0.,
                hinge,
                swing_degrees: 90.,
                door_style: extension.door_style,
                use_default_dimensions: false,
                window_panes: None,
            };
            let leaf = door(recipe, &opening);
            let axis = interior_wall::axis(bounds);
            let centre = opening.facade.point(recipe, opening.offset, 0., 0.)[axis];
            let walls = interior_wall::solids(
                bounds,
                Some(Doorway {
                    centre,
                    width: size.width,
                    height: size.height,
                }),
            );
            if crate::validation::validate_door_swings(std::slice::from_ref(&leaf), colliders)
                .is_ok()
                && crate::validation::validate_door_swings(std::slice::from_ref(&leaf), &walls)
                    .is_ok()
                && approach_clear(recipe, &opening, colliders)
            {
                return Ok((opening, leaf));
            }
        }
    }
    Err(BuildingError::new(
        &extension.id,
        "No clear internal door and approach fit this extension; move nearby partitions, stairs or windows, or widen the attachment.",
    ))
}

fn approach_clear(
    recipe: &BuildingRecipe,
    opening: &OpeningRecipe,
    colliders: &[BoxCollider],
) -> bool {
    [recipe.wall_thickness + 0.4, -0.4]
        .into_iter()
        .all(|inward| {
            let feet = opening.facade.point(recipe, opening.offset, 0.025, inward);
            !colliders
                .iter()
                .any(|b| b.intersects_capsule(feet, 0.28, 1.8))
        })
}

fn door(recipe: &BuildingRecipe, opening: &OpeningRecipe) -> DoorDescriptor {
    let right = opening.hinge == HingeSide::Right;
    let tangent = opening.facade.tangent();
    let edge = opening.width * 0.5 - 0.003;
    let hinge = opening.facade.point(
        recipe,
        opening.offset + if right { edge } else { -edge },
        0.006,
        -0.001,
    );
    geometry::door_leaf_at(
        opening.id.clone(),
        [opening.width, opening.height],
        hinge,
        [
            tangent[1].atan2(tangent[0]) + if right { std::f32::consts::PI } else { 0. },
            if right {
                std::f32::consts::FRAC_PI_2
            } else {
                -std::f32::consts::FRAC_PI_2
            },
        ],
        opening.door_style,
        false,
        if right { -0.5 } else { 0.5 },
    )
}

fn trim_header(lod: &mut BuildingLod, header: f32) {
    for part in lod
        .parts
        .iter_mut()
        .filter(|p| p.material == MaterialSlot::Timber)
    {
        let mut mesh = crate::BuildingMesh::default();
        for tri in part.mesh.indices.chunks_exact(3) {
            let vertices = [tri[0], tri[1], tri[2]].map(|i| {
                let i = i as usize;
                Vertex {
                    point: part.mesh.positions[i],
                    normal: part.mesh.normals[i],
                    uv: part.mesh.uvs[i],
                }
            });
            let (below, _) = clip::split(
                &vertices,
                Plane {
                    normal: [0., 0., 1.],
                    distance: header,
                },
            );
            if below.len() >= 3 {
                clip::append(&mut mesh, &below);
            }
        }
        part.mesh = mesh;
    }
}
