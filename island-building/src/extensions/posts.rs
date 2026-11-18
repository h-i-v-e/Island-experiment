//! Trim the visible corners of both sides of an attachment.
use super::{ExtensionRecipe, node::Node};
use crate::{
    BuildingRecipe, Facade, WallStyle,
    collision::BoxCollider,
    geometry::{
        Builder, chimney,
        interior_wall::{self, Doorway},
        timber_frame::junction::{CASING_MARGIN, Post, opening_clearance, xy_dot},
    },
};

pub(super) fn exterior(recipe: &BuildingRecipe, nodes: &[Node]) -> Vec<Post> {
    if recipe.walls.style != WallStyle::WattleAndDaub {
        return Vec::new();
    }
    let host = &nodes[0];
    recipe
        .extensions
        .iter()
        .zip(&nodes[1..])
        .flat_map(|(extension, node)| {
            [-1., 1.].into_iter().filter_map(move |sign| {
                let along = extension.offset + sign * extension.width * 0.5;
                let corner = extension.facade.point(recipe, along, 0., 0.);
                let outward = extension.facade.normal();
                let side = extension.facade.tangent().map(|v| v * sign);
                let run = side_clearance(node, corner, outward, side);
                let height = post_head(host, node, corner);
                let span = opening_clearance(recipe, extension.facade, corner, side, height)
                    .min(neighbour_clearance(recipe, extension, along, sign));
                let reach =
                    std::array::from_fn(|i| outward[i] * run.max(0.) + side[i] * span.max(0.));
                Post::at(recipe, corner, reach, |point| post_head(host, node, point))
            })
        })
        .collect()
}

pub(super) fn interior(
    builder: &mut Builder,
    recipe: &BuildingRecipe,
    extension: &ExtensionRecipe,
    node: &Node,
    bounds: BoxCollider,
    door: Doorway,
) {
    if recipe.walls.style != WallStyle::WattleAndDaub {
        return;
    }
    let axis = interior_wall::axis(bounds);
    let outward = extension.facade.normal();
    for sign in [-1., 1.] {
        let along = extension.offset + sign * (extension.width * 0.5 - recipe.wall_thickness);
        let corner = extension.facade.point(recipe, along, 0., 0.);
        let side = extension.facade.tangent().map(|v| v * sign);
        let run = side_clearance(node, corner, outward, side);
        let margin = (corner[axis] - door.centre).abs() - door.width * 0.5 - CASING_MARGIN;
        let span = recipe.walls.beam_width.min(margin);
        let reach = std::array::from_fn(|i| outward[i] * run.max(0.) - side[i] * span.max(0.));
        if let Some(post) = Post::at(recipe, corner, reach, |_| bounds.max[2] + 0.008) {
            post.append(builder);
        }
    }
}

fn neighbour_clearance(
    recipe: &BuildingRecipe,
    extension: &ExtensionRecipe,
    along: f32,
    sign: f32,
) -> f32 {
    recipe
        .extensions
        .iter()
        .filter(|other| other.id != extension.id && other.facade == extension.facade)
        .map(|other| (other.offset - along) * sign - other.width * 0.5)
        .filter(|gap| *gap > 0.)
        .fold(recipe.walls.beam_width, |reach, gap| {
            reach.min(gap * 0.5 - 0.002)
        })
}

fn side_clearance(node: &Node, corner: [f32; 3], outward: [f32; 3], side: [f32; 3]) -> f32 {
    let local_side = node.placement.local_vector(side);
    let facade = Facade::ALL
        .into_iter()
        .find(|f| xy_dot(f.normal(), local_side) > 0.5)
        .expect("attachment side is axis aligned");
    opening_clearance(
        &node.recipe,
        facade,
        node.placement.local(corner),
        node.placement.local_vector(outward),
        node.recipe.wall_height + node.recipe.roof.rise,
    )
}

fn post_head(host: &Node, node: &Node, point: [f32; 3]) -> f32 {
    let query = BoxCollider {
        min: point,
        max: [point[0], point[1], 1000.],
    };
    let soffit = |node: &Node| {
        node.roof[0]
            .query(query)
            .filter_map(|volume| volume.lower_at(point))
            .min_by(f32::total_cmp)
    };
    // An overhang provides the actual bearing surface. If a corner extends
    // past a short eave, use the height where the roof meets the wall.
    let child = &node.recipe;
    let x = node.placement.local(point)[0].clamp(-child.width * 0.5, child.width * 0.5);
    let attachment = soffit(node)
        .unwrap_or_else(|| chimney::roof_height(child, x) - child.roof.effective_thickness());
    soffit(host).map_or(attachment, |height| height.min(attachment)) + 0.008
}
