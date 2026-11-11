//! Room-facing caps share the outer frame; solid corner posts are emitted once.
use super::{Builder, BuildingRecipe, Facade, MaterialSlot, Member, side};

pub(super) fn generate(
    builder: &mut Builder,
    recipe: &BuildingRecipe,
    facade: Facade,
    outer: &[Member],
) {
    let back = recipe.wall_thickness + recipe.walls.frame_projection;
    for member in outer.iter().filter(|m| !m.corner) {
        let polygon = &member.polygon;
        for i in 1..polygon.len() - 1 {
            // Reverse each facade so its cap faces into the room.
            let points = [polygon[0], polygon[i + 1], polygon[i]];
            if side(points[0], points[1], points[2]).abs() < 0.000_001 {
                continue;
            }
            builder.triangle(
                MaterialSlot::Timber,
                points.map(|[u, z]| facade.point(recipe, u, z, back)),
                points.map(|p| member.uv(p).map(|v| v * 0.5)),
            );
        }
    }
}
