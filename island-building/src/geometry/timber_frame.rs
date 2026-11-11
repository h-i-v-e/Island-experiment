//! Shared half-timber layout: real near/middle relief and matching distant charts.
use super::Builder;
use crate::{
    BuildingRecipe, Facade, MaterialSlot, OpeningKind, OpeningRecipe, RoofCovering, RoofKind,
    WallStyle,
};

type Point = [f32; 2];

pub(crate) struct Member {
    polygon: Vec<Point>,
    origin: Point,
    pub(crate) direction: Point,
    bounds: [Point; 2],
}

impl Member {
    pub(crate) fn uv(&self, point: Point) -> Point {
        let delta = [point[0] - self.origin[0], point[1] - self.origin[1]];
        [
            delta[0] * self.direction[1] - delta[1] * self.direction[0],
            delta[0] * self.direction[0] + delta[1] * self.direction[1],
        ]
    }

    #[cfg(test)]
    fn sample(&self, point: Point) -> Option<(Point, Point)> {
        if (0..2).any(|i| point[i] < self.bounds[0][i] || point[i] > self.bounds[1][i]) {
            return None;
        }
        edges(&self.polygon)
            .all(|(a, b)| side(a, b, point) >= -0.000_000_1)
            .then(|| (self.uv(point), self.direction))
    }

    pub(crate) fn coverage(&self, point: Point, footprint: Point) -> f32 {
        if (0..2).any(|i| {
            point[i] < self.bounds[0][i] - footprint[i] * 0.5
                || point[i] > self.bounds[1][i] + footprint[i] * 0.5
        }) {
            return 0.;
        }
        edges(&self.polygon).fold(1., |coverage, (a, b)| {
            let edge = [b[0] - a[0], b[1] - a[1]];
            let span = edge[1].abs() * footprint[0] + edge[0].abs() * footprint[1];
            coverage.min((side(a, b, point) / span.max(0.000_001) + 0.5).clamp(0., 1.))
        })
    }
}

// Validated facades are at most 40 m and bays at least 0.75 m: at most 54 bays.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
pub(crate) fn layout(recipe: &BuildingRecipe, facade: Facade) -> Vec<Member> {
    if recipe.walls.style != WallStyle::WattleAndDaub {
        return Vec::new();
    }
    let width = recipe.walls.beam_width;
    let half = facade.length(recipe) * 0.5;
    let height = recipe.wall_height;
    let count = ((half * 2. - width) / recipe.walls.bay_width).ceil();
    let count = count as u16;
    let mut posts: Vec<_> = (0..=count)
        .map(|i| -half + width * 0.5 + (half * 2. - width) * f32::from(i) / f32::from(count))
        .collect();
    for opening in recipe.openings.iter().filter(|o| o.facade == facade) {
        posts.extend([
            opening.offset - opening.width * 0.5 - 0.075 - width * 0.5,
            opening.offset + opening.width * 0.5 + 0.075 + width * 0.5,
        ]);
    }
    posts.sort_by(f32::total_cmp);
    posts.dedup_by(|a, b| (*a - *b).abs() < width);
    // A nearby opening jamb may merge with a corner station; keep its outer edge.
    if let Some(last) = posts.last_mut() {
        *last = half - width * 0.5;
    }
    let profile = profile(recipe, facade);
    let mut members = Vec::new();
    opening_crosspieces(&mut members, recipe, facade, &profile, &posts);
    for &along in &posts {
        add(
            &mut members,
            recipe,
            facade,
            &profile,
            [along, 0.],
            [along, height + recipe.roof.rise],
        );
    }
    let middle = height * 0.38;
    for z in [
        width * 0.5,
        middle,
        height + roof_overlap(recipe) - width * 0.5,
    ] {
        add(
            &mut members,
            recipe,
            facade,
            &profile,
            [-half, z],
            [half, z],
        );
    }
    braces(&mut members, recipe, facade, &profile, &posts, middle);
    roof_members(&mut members, recipe, facade, &profile, half);
    members
}

fn opening_crosspieces(
    members: &mut Vec<Member>,
    recipe: &BuildingRecipe,
    facade: Facade,
    profile: &[Point],
    posts: &[f32],
) {
    let half_width = recipe.walls.beam_width * 0.5;
    let half = facade.length(recipe) * 0.5;
    for opening in recipe.openings.iter().filter(|o| o.facade == facade) {
        let a = opening.offset - opening.width * 0.5 - 0.075;
        let b = opening.offset + opening.width * 0.5 + 0.075;
        let left = posts
            .iter()
            .rev()
            .find(|&&u| u <= a)
            .map_or(-half, |u| (u - half_width).max(-half));
        let right = posts
            .iter()
            .find(|&&u| u >= b)
            .map_or(half, |u| (u + half_width).min(half));
        let [bottom, top] = opening_vertical_bounds(opening);
        add(
            members,
            recipe,
            facade,
            profile,
            [left, top + half_width],
            [right, top + half_width],
        );
        if opening.kind == OpeningKind::Window {
            add(
                members,
                recipe,
                facade,
                profile,
                [left, bottom - half_width],
                [right, bottom - half_width],
            );
        }
    }
}

fn opening_vertical_bounds(opening: &OpeningRecipe) -> Point {
    let [below, above] = match opening.kind {
        OpeningKind::Window => [super::windows::SILL_DROP, super::windows::CASING_WIDTH],
        OpeningKind::Door => [0.05, 0.07],
    };
    [opening.sill - below, opening.sill + opening.height + above]
}

fn braces(
    members: &mut Vec<Member>,
    recipe: &BuildingRecipe,
    facade: Facade,
    profile: &[Point],
    posts: &[f32],
    middle: f32,
) {
    let width = recipe.walls.beam_width;
    for (index, bay) in posts.windows(2).enumerate() {
        if bay[1] - bay[0] < width * 3. {
            continue;
        }
        for (bottom, top) in [
            (width, middle - width * 0.5),
            (middle + width * 0.5, recipe.wall_height - width),
        ] {
            let [a, b] = if index % 2 == 0 {
                [bottom, top]
            } else {
                [top, bottom]
            };
            add(members, recipe, facade, profile, [bay[0], a], [bay[1], b]);
        }
    }
}

fn roof_members(
    members: &mut Vec<Member>,
    recipe: &BuildingRecipe,
    facade: Facade,
    profile: &[Point],
    half: f32,
) {
    if recipe.roof.kind == RoofKind::Gable {
        if matches!(facade, Facade::South | Facade::North) {
            let slope = recipe.roof.rise / half;
            let z = recipe.wall_height + roof_overlap(recipe)
                - recipe.walls.beam_width * 0.5 * (1. + slope * slope).sqrt();
            let peak = z + recipe.roof.rise;
            add(members, recipe, facade, profile, [-half, z], [0., peak]);
            add(members, recipe, facade, profile, [0., peak], [half, z]);
            add(members, recipe, facade, profile, [0., z], [0., peak]);
        }
    } else {
        let top = |u| {
            let point = facade.point(recipe, u, 0., 0.);
            let slope = if matches!(facade, Facade::South | Facade::North) {
                recipe.roof.rise / recipe.width
            } else {
                0.
            };
            super::chimney::roof_height(recipe, point[0]) - recipe.roof.effective_thickness()
                + roof_overlap(recipe)
                - recipe.walls.beam_width * 0.5 * (1. + slope * slope).sqrt()
        };
        add(
            members,
            recipe,
            facade,
            profile,
            [-half, top(-half)],
            [half, top(half)],
        );
    }
}

fn roof_overlap(recipe: &BuildingRecipe) -> f32 {
    if recipe.roof.covering == RoofCovering::Thatch {
        0.10
    } else {
        0.
    }
}

fn profile(recipe: &BuildingRecipe, facade: Facade) -> Vec<Point> {
    let half = facade.length(recipe) * 0.5;
    let height = recipe.wall_height + roof_overlap(recipe);
    let mut polygon = vec![[-half, 0.], [half, 0.]];
    if recipe.roof.kind == RoofKind::Gable {
        polygon.push([half, height]);
        if matches!(facade, Facade::South | Facade::North) {
            polygon.push([0., height + recipe.roof.rise]);
        }
        polygon.push([-half, height]);
    } else {
        for u in [half, -half] {
            let x = facade.point(recipe, u, 0., 0.)[0];
            polygon.push([
                u,
                super::chimney::roof_height(recipe, x) - recipe.roof.effective_thickness()
                    + roof_overlap(recipe),
            ]);
        }
    }
    polygon
}

fn add(
    members: &mut Vec<Member>,
    recipe: &BuildingRecipe,
    facade: Facade,
    profile: &[Point],
    start: Point,
    end: Point,
) {
    let length = (end[0] - start[0]).hypot(end[1] - start[1]);
    if length < 0.001 {
        return;
    }
    let direction = [(end[0] - start[0]) / length, (end[1] - start[1]) / length];
    let width = recipe.walls.beam_width * 0.5;
    let offset = [direction[1] * width, -direction[0] * width];
    let polygon = vec![
        [start[0] - offset[0], start[1] - offset[1]],
        [start[0] + offset[0], start[1] + offset[1]],
        [end[0] + offset[0], end[1] + offset[1]],
        [end[0] - offset[0], end[1] - offset[1]],
    ];
    let polygon = edges(profile).fold(polygon, |p, (a, b)| clip(&p, a, b, true));
    let mut pieces = vec![polygon];
    // Leave the existing casing and sill unobstructed, including their trim border.
    for opening in recipe.openings.iter().filter(|o| o.facade == facade) {
        let a = opening.offset - opening.width * 0.5 - 0.075;
        let b = opening.offset + opening.width * 0.5 + 0.075;
        let [bottom, top] = opening_vertical_bounds(opening);
        let cut = [[a, bottom], [b, bottom], [b, top], [a, top]];
        pieces = pieces.into_iter().flat_map(|p| subtract(p, &cut)).collect();
    }
    // Butt joints partition the visible front: no overlapping timber polygons.
    for previous in members.iter() {
        pieces = pieces
            .into_iter()
            .flat_map(|p| subtract(p, &previous.polygon))
            .collect();
    }
    for polygon in pieces.into_iter().filter(|p| area(p) > 0.00001) {
        let bounds = [
            std::array::from_fn(|i| polygon.iter().map(|p| p[i]).fold(f32::INFINITY, f32::min)),
            std::array::from_fn(|i| {
                polygon
                    .iter()
                    .map(|p| p[i])
                    .fold(f32::NEG_INFINITY, f32::max)
            }),
        ];
        members.push(Member {
            polygon,
            origin: grain_origin(recipe, facade, start, offset, direction),
            direction,
            bounds,
        });
    }
}

fn grain_origin(
    recipe: &BuildingRecipe,
    facade: Facade,
    start: Point,
    offset: Point,
    direction: Point,
) -> Point {
    let half = facade.length(recipe) * 0.5;
    if direction[0].abs() < 0.00001
        && (start[0] - (half - recipe.walls.beam_width * 0.5)).abs() < 0.00001
    {
        // Both faces of a corner meet at the same grain phase and vertical height.
        [half, start[1]]
    } else {
        [start[0] - offset[0], start[1] - offset[1]]
    }
}

fn point(recipe: &BuildingRecipe, facade: Facade, mut along: f32, z: f32, inward: f32) -> [f32; 3] {
    let half = facade.length(recipe) * 0.5;
    if (along.abs() - half).abs() < 0.0001 {
        along -= along.signum() * inward;
    }
    facade.point(recipe, along, z, inward)
}

fn edges(polygon: &[Point]) -> impl Iterator<Item = (Point, Point)> + '_ {
    polygon
        .iter()
        .copied()
        .zip(polygon.iter().copied().cycle().skip(1))
}

fn side(a: Point, b: Point, p: Point) -> f32 {
    (b[0] - a[0]) * (p[1] - a[1]) - (b[1] - a[1]) * (p[0] - a[0])
}

fn area(polygon: &[Point]) -> f32 {
    edges(polygon)
        .map(|(a, b)| a[0] * b[1] - a[1] * b[0])
        .sum::<f32>()
        * 0.5
}

fn clip(polygon: &[Point], a: Point, b: Point, inside: bool) -> Vec<Point> {
    let mut result = Vec::new();
    for (p, q) in edges(polygon) {
        let d = side(a, b, p);
        let e = side(a, b, q);
        let keep_p = (d >= 0.) == inside;
        let keep_q = (e >= 0.) == inside;
        if keep_p {
            result.push(p);
        }
        if keep_p != keep_q {
            let t = d / (d - e);
            result.push([p[0] + (q[0] - p[0]) * t, p[1] + (q[1] - p[1]) * t]);
        }
    }
    clean(result)
}

fn clean(mut result: Vec<Point>) -> Vec<Point> {
    result.dedup_by(|a, b| (a[0] - b[0]).hypot(a[1] - b[1]) < 0.0001);
    if result.len() > 1
        && (result[0][0] - result[result.len() - 1][0])
            .hypot(result[0][1] - result[result.len() - 1][1])
            < 0.0001
    {
        result.pop();
    }
    let mut i = 0;
    while result.len() >= 3 && i < result.len() {
        let previous = result[(i + result.len() - 1) % result.len()];
        let next = result[(i + 1) % result.len()];
        if side(previous, result[i], next).abs() < 0.000_001 {
            result.remove(i);
            i = 0;
        } else {
            i += 1;
        }
    }
    result
}

fn subtract(polygon: Vec<Point>, cut: &[Point]) -> Vec<Vec<Point>> {
    if polygon.is_empty() {
        return Vec::new();
    }
    let mut remainder = polygon;
    let mut result = Vec::new();
    for (a, b) in edges(cut) {
        let outside = clip(&remainder, a, b, false);
        if area(&outside) > 0.00001 {
            result.push(outside);
        }
        remainder = clip(&remainder, a, b, true);
        if area(&remainder) <= 0.00001 {
            break;
        }
    }
    result
}

pub(crate) fn generate(builder: &mut Builder, recipe: &BuildingRecipe, detailed: bool) {
    for facade in Facade::ALL {
        for member in layout(recipe, facade) {
            let polygon = &member.polygon;
            let front = -recipe.walls.frame_projection;
            for i in 1..polygon.len() - 1 {
                let points = [polygon[0], polygon[i], polygon[i + 1]];
                if side(points[0], points[1], points[2]).abs() < 0.000_001 {
                    continue;
                }
                builder.triangle(
                    MaterialSlot::Timber,
                    points.map(|[u, z]| point(recipe, facade, u, z, front)),
                    points.map(|p| member.uv(p).map(|v| v * 0.5)),
                );
            }
            if detailed {
                for (a, b) in edges(polygon) {
                    builder.face(
                        MaterialSlot::Timber,
                        [
                            point(recipe, facade, b[0], b[1], front),
                            point(recipe, facade, a[0], a[1], front),
                            point(recipe, facade, a[0], a[1], 0.006),
                            point(recipe, facade, b[0], b[1], 0.006),
                        ],
                    );
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opening_crosspieces_keep_horizontal_grain_across_the_opening() {
        let mut recipe = BuildingRecipe::hut();
        recipe.walls.style = WallStyle::WattleAndDaub;
        for opening in &recipe.openings {
            let members = layout(&recipe, opening.facade);
            let [bottom, top] = opening_vertical_bounds(opening);
            let heights = [
                Some(top + recipe.walls.beam_width * 0.5),
                (opening.kind == OpeningKind::Window)
                    .then_some(bottom - recipe.walls.beam_width * 0.5),
            ];
            for z in heights.into_iter().flatten() {
                for fraction in [-0.45, 0., 0.45] {
                    let point = [opening.offset + opening.width * fraction, z];
                    let member = members
                        .iter()
                        .find(|m| m.sample(point).is_some())
                        .expect("full crosspiece at opening");
                    assert!(
                        member.direction[1].abs() < 0.00001,
                        "vertical posts butt into the horizontal crosspiece"
                    );
                }
            }
        }
    }

    #[test]
    fn adjacent_corner_faces_share_the_same_edge_and_grain_phase() {
        let mut recipe = BuildingRecipe::hut();
        recipe.walls.style = WallStyle::WattleAndDaub;
        for opening in &mut recipe.openings {
            if opening.kind == crate::OpeningKind::Window {
                opening.offset =
                    recipe.depth * 0.5 - recipe.wall_thickness - 0.12 - opening.width * 0.5;
            }
        }
        let p = -recipe.walls.frame_projection;
        for (first, second) in [
            (Facade::South, Facade::East),
            (Facade::East, Facade::North),
            (Facade::North, Facade::West),
            (Facade::West, Facade::South),
        ] {
            let a = first.length(&recipe) * 0.5;
            let b = -second.length(&recipe) * 0.5;
            let first_layout = layout(&recipe, first);
            let second_layout = layout(&recipe, second);
            let first_post = first_layout
                .iter()
                .find(|m| m.sample([a, 0.5]).is_some())
                .unwrap();
            let second_post = second_layout
                .iter()
                .find(|m| m.sample([b, 0.5]).is_some())
                .unwrap();
            assert!(
                point(&recipe, first, a, 0.5, p)
                    .into_iter()
                    .zip(point(&recipe, second, b, 0.5, p))
                    .all(|(a, b)| (a - b).abs() < 0.000_001)
            );
            assert!(
                first_post
                    .uv([a, 0.5])
                    .into_iter()
                    .zip(second_post.uv([b, 0.5]))
                    .all(|(a, b)| (a - b).abs() < 0.000_001)
            );
        }
    }

    #[test]
    fn clipped_members_respect_profiles_openings_and_non_overlapping_fronts_at_both_scales() {
        for size in [3., 40.] {
            for roof in [RoofKind::Gable, RoofKind::Shed] {
                let mut recipe = BuildingRecipe::hut();
                recipe.width = size;
                recipe.depth = size;
                recipe.wall_height = if size < 4. { 2.4 } else { 8. };
                recipe.roof.kind = roof;
                recipe.roof.rise = if size < 4. { 0.1 } else { 6. };
                recipe.walls.style = WallStyle::WattleAndDaub;
                for facade in Facade::ALL {
                    let members = layout(&recipe, facade);
                    assert!(!members.is_empty());
                    let outline = profile(&recipe, facade);
                    for member in &members {
                        assert!(area(&member.polygon) > 0.00001);
                        assert!(
                            member
                                .polygon
                                .iter()
                                .all(|p| edges(&outline).all(|(a, b)| side(a, b, *p) > -0.0001))
                        );
                        let centre = std::array::from_fn(|i| {
                            member.polygon.iter().map(|p| p[i]).sum::<f32>()
                                / f32::from(u16::try_from(member.polygon.len()).unwrap())
                        });
                        assert_eq!(
                            members
                                .iter()
                                .filter(|m| m.sample(centre).is_some())
                                .count(),
                            1,
                            "timber faces must not compete at joints"
                        );
                        for opening in recipe.openings.iter().filter(|o| o.facade == facade) {
                            assert!(
                                (centre[0] - opening.offset).abs() >= opening.width * 0.5 + 0.074
                                    || centre[1] <= opening_vertical_bounds(opening)[0] + 0.001
                                    || centre[1] >= opening_vertical_bounds(opening)[1] - 0.001
                            );
                        }
                    }
                }
            }
        }
    }
}
