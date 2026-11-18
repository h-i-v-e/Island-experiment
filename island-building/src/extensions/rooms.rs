//! Every attachment has one clean dividing wall and a retained internal door.
use super::{
    ExtensionRecipe,
    clip::{self, Plane, Vertex, Volume},
    node::Node,
};
use crate::{
    BuildingError, BuildingLod, BuildingRecipe, DoorDescriptor, Facade, HingeSide, MaterialSlot,
    OpeningKind, OpeningRecipe, StaircaseLayout, WallStyle,
    collision::BoxCollider,
    geometry::{
        self, Builder,
        interior_wall::{self, Doorway},
        timber_frame::junction::CASING_MARGIN,
    },
};

pub(super) struct Connection {
    pub id: String,
    pub bounds: BoxCollider,
}

pub(super) fn generate(
    recipe: &BuildingRecipe,
    nodes: &[Node],
    stairs: Option<&StaircaseLayout>,
    near: &mut BuildingLod,
    colliders: &mut Vec<BoxCollider>,
    doors: &mut Vec<DoorDescriptor>,
) -> Result<Vec<Connection>, BuildingError> {
    let mut builder = Builder::default();
    builder.append(std::mem::take(near), 0.);
    let mut connections = Vec::with_capacity(recipe.extensions.len());
    let parent = crate::storeys::recipe_for_storey(recipe, 0);
    let header = geometry::ceiling_height(&parent) - recipe.walls.beam_width;
    let stair_space = stairs.map(|stairs| {
        stairs
            .stairwell
            .as_ref()
            .map_or(stairs.opening, |room| room.bounds)
    });
    for (extension, node) in recipe.extensions.iter().zip(nodes.iter().skip(1)) {
        let bounds = wall_bounds(recipe, extension, node.room_bounds[0].max[2]);
        let (opening, mut leaf) = select_door(recipe, extension, bounds, colliders, stair_space)?;
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
        clear_floor_band(&mut lod, recipe, extension.facade, bounds);
        builder.append(lod, 0.);
        super::posts::interior(&mut builder, recipe, extension, node, bounds, doorway);
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
    stair_space: Option<BoxCollider>,
) -> Result<(OpeningRecipe, DoorDescriptor), BuildingError> {
    let size = recipe.opening_defaults.door;
    let margin = (extension.width - 2. * recipe.wall_thickness - size.width) * 0.5 - 0.09;
    if margin < 0. || size.height + 0.09 > bounds.max[2] {
        return Err(BuildingError::new(
            &extension.id,
            "The shared door dimensions must fit the extension wall below its ceiling; widen or raise the extension, or adjust the building-wide door dimensions.",
        ));
    }
    // Prefer a doorway into the main room. Where the staircase leaves no
    // separate opening, its empty space can still be used if the full walk
    // and swing clear every real obstacle.
    if let Some(stair_space) = stair_space
        && let Some(door) = find_door(
            recipe,
            extension,
            bounds,
            colliders,
            margin,
            Some(stair_space),
        )
    {
        return Ok(door);
    }
    find_door(recipe, extension, bounds, colliders, margin, None).ok_or_else(|| BuildingError::new(
        &extension.id,
        "No clear door fits between the corners of this extension. Widen or shift the extension, or move the nearby stairs, fireplace or partition.",
    ))
}

fn find_door(
    recipe: &BuildingRecipe,
    extension: &ExtensionRecipe,
    bounds: BoxCollider,
    colliders: &[BoxCollider],
    margin: f32,
    stair_space: Option<BoxCollider>,
) -> Option<(OpeningRecipe, DoorDescriptor)> {
    let size = recipe.opening_defaults.door;
    let axis = interior_wall::axis(bounds);
    for offset in door_offsets(extension.offset, margin) {
        if !fireplace_clear(recipe, extension.facade, offset, size.width)
            || !stair_clear(recipe, extension.facade, offset, size.width, stair_space)
            || !approach_clear(
                recipe,
                extension.facade,
                offset,
                colliders.iter().chain(&stair_space),
            )
        {
            continue;
        }
        for hinge in [HingeSide::Left, HingeSide::Right] {
            let opening = OpeningRecipe {
                id: format!("extension-door-{}", extension.id),
                storey: 0,
                facade: extension.facade,
                kind: OpeningKind::Door,
                offset,
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
            {
                return Some((opening, leaf));
            }
        }
    }
    None
}

fn door_offsets(centre: f32, margin: f32) -> impl Iterator<Item = f32> {
    // Try the centre first and then move outwards, including both corners.
    // The 30 m maximum attachment width keeps the search under 600 steps.
    let steps = (1_u16..)
        .map(|step| f32::from(step) * 0.025)
        .take_while(move |distance| *distance < margin)
        .chain((margin > 0.).then_some(margin));
    std::iter::once(centre).chain(steps.flat_map(move |d| [centre - d, centre + d]))
}

fn fireplace_clear(recipe: &BuildingRecipe, facade: Facade, offset: f32, width: f32) -> bool {
    recipe.chimney.as_ref().is_none_or(|chimney| {
        // Leave both the projecting mantel and the doorway casing intact.
        chimney.facade != facade
            || (offset - chimney.offset).abs()
                >= (chimney.width + width) * 0.5 + 0.1 + CASING_MARGIN
    })
}

fn stair_clear(
    recipe: &BuildingRecipe,
    facade: Facade,
    offset: f32,
    width: f32,
    stair_space: Option<BoxCollider>,
) -> bool {
    stair_space.is_none_or(|stairs| {
        // A stairwell that reuses the exterior wall may have no collider to
        // block the opening there. Keep the entire casing out of that space.
        let half = width * 0.5 + CASING_MARGIN;
        let a = facade.point(recipe, offset - half, 0., recipe.wall_thickness + 0.08);
        let b = facade.point(recipe, offset + half, 0., recipe.wall_thickness);
        (0..2).any(|i| stairs.min[i] >= a[i].max(b[i]) || stairs.max[i] <= a[i].min(b[i]))
    })
}

fn approach_clear<'a>(
    recipe: &BuildingRecipe,
    facade: Facade,
    offset: f32,
    mut colliders: impl Iterator<Item = &'a BoxCollider>,
) -> bool {
    let axis = usize::from(facade.normal()[1].abs() > 0.5);
    let inside = facade.point(recipe, offset, 0.025, recipe.wall_thickness + 0.4);
    let outside = facade.point(recipe, offset, 0.025, -0.4);
    let low = inside[axis].min(outside[axis]);
    let high = inside[axis].max(outside[axis]);
    // The closest point of the straight walk to each collider also tests
    // obstructions between the two ends, including stairwell wall returns.
    !colliders.any(|bounds| {
        let mut feet = inside;
        feet[axis] = bounds.min[axis].clamp(low, high);
        bounds.intersects_capsule(feet, 0.28, 1.8)
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
        for vertices in triangles(&part.mesh) {
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

fn clear_floor_band(
    lod: &mut BuildingLod,
    recipe: &BuildingRecipe,
    facade: Facade,
    mut bounds: BoxCollider,
) {
    if recipe.storeys < 2
        || recipe.walls.style != WallStyle::WattleAndDaub
        || bounds.max[2] <= recipe.wall_height
    {
        return;
    }
    // The host's exposed floor edge already provides the timber finish on
    // the wing side. Keep the divider solid, but leave off its plaster there.
    let axis = 1 - interior_wall::axis(bounds);
    let normal = facade.normal()[axis];
    bounds.min[2] = recipe.wall_height;
    bounds.max[2] = recipe.floor_height(1);
    let band = Volume::bounds(bounds);
    for part in lod
        .parts
        .iter_mut()
        .filter(|p| p.material == MaterialSlot::Interior)
    {
        let mut mesh = crate::BuildingMesh::default();
        for vertices in triangles(&part.mesh) {
            if vertices[0].normal[axis] * normal > 0.99 {
                for polygon in clip::subtract(vertices.to_vec(), &band, false) {
                    clip::append(&mut mesh, &polygon);
                }
            } else {
                clip::append(&mut mesh, &vertices);
            }
        }
        part.mesh = mesh;
    }
}

fn triangles(mesh: &crate::BuildingMesh) -> impl Iterator<Item = [Vertex; 3]> + '_ {
    mesh.indices.chunks_exact(3).map(|tri| {
        [tri[0], tri[1], tri[2]].map(|i| {
            let i = i as usize;
            Vertex {
                point: mesh.positions[i],
                normal: mesh.normals[i],
                uv: mesh.uvs[i],
            }
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn narrow_clearance_between_the_approach_points_can_move_the_door() {
        let mut recipe = BuildingRecipe::hut();
        recipe.chimney = None;
        let extension = ExtensionRecipe {
            facade: Facade::East,
            width: 4.,
            ..ExtensionRecipe::default()
        };
        let bounds = wall_bounds(&recipe, &extension, 2.8);
        let obstacles = [
            BoxCollider {
                min: [2.86, -2., 0.],
                max: [2.91, 0.62, 2.4],
            },
            BoxCollider {
                min: [2.86, 1.28, 0.],
                max: [2.91, 2., 2.4],
            },
        ];
        let (opening, _) = select_door(&recipe, &extension, bounds, &obstacles, None).unwrap();
        assert!((0.899..=1.001).contains(&opening.offset), "{opening:?}");
    }
}
