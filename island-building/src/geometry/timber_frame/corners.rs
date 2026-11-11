//! One closed rectangular post per corner, shared by both adjoining walls.
use super::{
    Builder, BuildingRecipe, Facade, MaterialSlot, Member, Point, clip, edges, roof_overlap,
};
use crate::{RoofKind, collision::BoxCollider};

fn reach(recipe: &BuildingRecipe, facade: Facade, sign: f32) -> f32 {
    let half = facade.length(recipe) * 0.5;
    let desired = recipe.wall_thickness + recipe.walls.beam_width;
    // Fit the complete post beside the nearest casing, even on a narrow facade.
    recipe
        .openings
        .iter()
        .filter(|o| o.facade == facade)
        .fold(desired, |width, o| {
            width.min(half - sign * o.offset - o.width * 0.5 - 0.075)
        })
}

fn height(recipe: &BuildingRecipe, x: f32) -> f32 {
    let roof = match recipe.roof.kind {
        RoofKind::Gable => recipe.wall_height,
        RoofKind::Shed => {
            crate::geometry::chimney::roof_height(recipe, x) - recipe.roof.effective_thickness()
        }
    };
    roof + roof_overlap(recipe)
}

pub(super) fn layout(recipe: &BuildingRecipe, facade: Facade, outline: &[Point]) -> Vec<Member> {
    let half = facade.length(recipe) * 0.5;
    [-1., 1.]
        .into_iter()
        .filter_map(|sign| {
            let end = sign * half;
            let inner = sign * (half - reach(recipe, facade, sign));
            let [left, right] = [end.min(inner), end.max(inner)];
            let top = |u| {
                height(
                    recipe,
                    facade.point(recipe, u, 0., -recipe.walls.frame_projection)[0],
                )
            };
            let polygon = vec![
                [left, 0.],
                [right, 0.],
                [right, top(right)],
                [left, top(left)],
            ];
            let polygon = edges(outline).fold(polygon, |p, (a, b)| clip(&p, a, b, true));
            (super::area(&polygon) > 0.00001).then(|| {
                let mut member = Member::new(polygon, [end, 0.], [0., 1.]);
                member.corner = true;
                member
            })
        })
        .collect()
}

pub(super) fn bounds(recipe: &BuildingRecipe) -> [BoxCollider; 4] {
    [(-1., -1.), (1., -1.), (1., 1.), (-1., 1.)].map(|(x_sign, y_sign)| {
        let gable = if y_sign < 0. {
            Facade::South
        } else {
            Facade::North
        };
        let side = if x_sign > 0. {
            Facade::East
        } else {
            Facade::West
        };
        let x_reach = reach(recipe, gable, -x_sign * y_sign);
        let y_reach = reach(recipe, side, x_sign * y_sign);
        let span = |half: f32, sign: f32, width: f32| {
            let outer = sign * (half + recipe.walls.frame_projection);
            let inner = sign * (half - width);
            [outer.min(inner), outer.max(inner)]
        };
        let x = span(recipe.width * 0.5, x_sign, x_reach);
        let y = span(recipe.depth * 0.5, y_sign, y_reach);
        BoxCollider {
            min: [x[0], y[0], 0.],
            max: [x[1], y[1], height(recipe, x[0]).max(height(recipe, x[1]))],
        }
    })
}

pub(super) fn generate(builder: &mut Builder, recipe: &BuildingRecipe) {
    for post in bounds(recipe) {
        let [left, back, _] = post.min;
        let [right, front, _] = post.max;
        let bottom = [
            [left, back, 0.],
            [right, back, 0.],
            [right, front, 0.],
            [left, front, 0.],
        ];
        let top = bottom.map(|[x, y, _]| [x, y, height(recipe, x)]);
        for edge in 0..4 {
            let next = (edge + 1) % 4;
            let points = [bottom[edge], bottom[next], top[next], top[edge]];
            builder.quad(
                MaterialSlot::Timber,
                points,
                points.map(|[x, y, z]| [(x + y) * 0.5, z * 0.5]),
            );
        }
        for points in [[bottom[3], bottom[2], bottom[1], bottom[0]], top] {
            builder.quad(
                MaterialSlot::Timber,
                points,
                points.map(|[x, y, _]| [x * 0.5, y * 0.5]),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    #[test]
    fn four_corner_posts_are_closed_solids_with_no_diagonal_join_faces() {
        for roof in [RoofKind::Gable, RoofKind::Shed] {
            for thickness in [0.08, 0.2, 0.6] {
                let mut recipe = BuildingRecipe::hut();
                recipe.roof.kind = roof;
                recipe.wall_thickness = thickness;
                let mut builder = Builder::default();
                generate(&mut builder, &recipe);
                let mesh = &builder.finish().parts.remove(0).mesh;
                assert_eq!(mesh.positions.len(), 4 * 24);
                assert_eq!(mesh.indices.len(), 4 * 36);
                let mut edges = BTreeMap::new();
                for tri in mesh.indices.chunks_exact(3) {
                    for i in 0..3 {
                        let mut pair = [
                            mesh.positions[tri[i] as usize].map(f32::to_bits),
                            mesh.positions[tri[(i + 1) % 3] as usize].map(f32::to_bits),
                        ];
                        pair.sort_unstable();
                        *edges.entry(pair).or_insert(0) += 1;
                    }
                }
                assert!(edges.values().all(|&count| count == 2), "watertight posts");
                for normal in &mesh.normals {
                    assert!(normal.iter().all(|v| v.is_finite()));
                    assert!(
                        normal[2].abs() > 0.9 || normal[0].abs() > 0.99 || normal[1].abs() > 0.99,
                        "only outer rectangular sides and end caps; no internal miter planes"
                    );
                }
            }
        }
    }

    #[test]
    fn posts_fit_near_corner_casings_without_notches_or_overlapping_frame_caps() {
        let mut recipe = BuildingRecipe::hut();
        recipe.walls.style = crate::WallStyle::WattleAndDaub;
        recipe.walls.beam_width = 0.3;
        for thickness in [0.08, 0.2, 0.6] {
            recipe.wall_thickness = thickness;
            for opening in &mut recipe.openings {
                if opening.kind == crate::OpeningKind::Window {
                    opening.offset = recipe.depth * 0.5 - thickness - 0.12 - opening.width * 0.5;
                }
            }
            for facade in Facade::ALL {
                let members = super::super::layout(&recipe, facade);
                assert_eq!(members.iter().filter(|m| m.corner).count(), 2);
                for member in &members {
                    let centre = std::array::from_fn(|i| {
                        member.polygon.iter().map(|p| p[i]).sum::<f32>()
                            / f32::from(u16::try_from(member.polygon.len()).unwrap())
                    });
                    assert_eq!(
                        members
                            .iter()
                            .filter(|m| m.sample(centre).is_some())
                            .count(),
                        1
                    );
                    for opening in recipe.openings.iter().filter(|o| o.facade == facade) {
                        let [bottom, top] = super::super::opening_vertical_bounds(opening);
                        assert!(
                            (centre[0] - opening.offset).abs() >= opening.width * 0.5 + 0.074
                                || centre[1] <= bottom + 0.001
                                || centre[1] >= top - 0.001
                        );
                    }
                }
            }
        }
    }
}
