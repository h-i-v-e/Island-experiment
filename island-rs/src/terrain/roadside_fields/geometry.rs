use crate::Vec2;

const CONTACT_TOLERANCE: f32 = 0.002;

pub(super) fn edges(points: &[Vec2]) -> impl Iterator<Item = (Vec2, Vec2)> + '_ {
    points
        .iter()
        .copied()
        .zip(points.iter().copied().cycle().skip(1))
}

pub(super) fn bounds(points: &[Vec2]) -> [Vec2; 2] {
    points.iter().fold(
        [Vec2::splat(f32::INFINITY), Vec2::splat(f32::NEG_INFINITY)],
        |[min, max], point| [min.min(*point), max.max(*point)],
    )
}

pub(super) fn contains_convex(polygon: &[Vec2], point: Vec2) -> bool {
    let turns = || edges(polygon).map(|(a, b)| (b - a).perp_dot(point - a));
    turns().all(|turn| turn >= 0.) || turns().all(|turn| turn <= 0.)
}

/// Compare convex interiors. Coincident edges are allowed for shared dividers.
pub(super) fn overlaps(a: &[Vec2], b: &[Vec2]) -> bool {
    let origin = a[0];
    let along = |polygon: &[Vec2], axis| {
        polygon
            .iter()
            .fold([f32::INFINITY, f32::NEG_INFINITY], |[min, max], point| {
                let projection = (*point - origin).dot(axis);
                [min.min(projection), max.max(projection)]
            })
    };
    edges(a).chain(edges(b)).all(|(start, end)| {
        let Some(axis) = (end - start).perp().try_normalize() else {
            return true;
        };
        let [a_min, a_max] = along(a, axis);
        let [b_min, b_max] = along(b, axis);
        a_max.min(b_max) - a_min.max(b_min) > CONTACT_TOLERANCE
    })
}

pub(super) fn convex(quad: &[Vec2; 4], orientation: f32) -> bool {
    (0..4).all(|i| {
        let a = quad[(i + 1) % 4] - quad[i];
        let b = quad[(i + 2) % 4] - quad[(i + 1) % 4];
        a.perp_dot(b) * orientation > 0.025
    })
}

pub(super) fn intersects_circle(polygon: &[Vec2], centre: Vec2, radius: f32) -> bool {
    contains_convex(polygon, centre)
        || edges(polygon)
            .any(|(a, b)| point_segment_distance_squared(centre, a, b) <= radius * radius)
}

pub(super) fn segment_near(polygon: &[Vec2], a: Vec2, b: Vec2, radius: f32) -> bool {
    contains_convex(polygon, a)
        || contains_convex(polygon, b)
        || edges(polygon).any(|(c, d)| segments_within(a, b, c, d, radius))
}

pub(super) fn segments_within(a: Vec2, b: Vec2, c: Vec2, d: Vec2, radius: f32) -> bool {
    let first = b - a;
    let second = d - c;
    let across = c - a;
    let det = first.perp_dot(second);
    if det != 0.0
        && (0. ..=1.).contains(&(across.perp_dot(second) / det))
        && (0. ..=1.).contains(&(across.perp_dot(first) / det))
    {
        return true;
    }
    let limit = radius * radius;
    point_segment_distance_squared(a, c, d) <= limit
        || point_segment_distance_squared(b, c, d) <= limit
        || point_segment_distance_squared(c, a, b) <= limit
        || point_segment_distance_squared(d, a, b) <= limit
}

pub(super) fn point_segment_distance_squared(point: Vec2, a: Vec2, b: Vec2) -> f32 {
    let span = b - a;
    let fraction = if span == Vec2::ZERO {
        0.
    } else {
        ((point - a).dot(span) / span.length_squared()).clamp(0., 1.)
    };
    point.distance_squared(a + span * fraction)
}

pub(super) fn intersects_marked_triangle(
    quad: &[Vec2; 4],
    triangle: [Vec2; 3],
    margins: [f32; 3],
) -> bool {
    if margins.iter().all(|&margin| margin <= 0.) {
        return false;
    }
    if margins.iter().all(|&margin| margin >= 0.) {
        return overlaps(quad, &triangle);
    }
    // Thresholds are affine on each face. Clip only the affected part of it.
    let mut polygon = [Vec2::ZERO; 4];
    let mut count = 0;
    for (a, b) in [(0, 1), (1, 2), (2, 0)] {
        if margins[a] >= 0. {
            polygon[count] = triangle[a];
            count += 1;
        }
        if (margins[a] >= 0.) != (margins[b] >= 0.) {
            let t = margins[a] / (margins[a] - margins[b]);
            polygon[count] = triangle[a].lerp(triangle[b], t);
            count += 1;
        }
    }
    count >= 3 && overlaps(quad, &polygon[..count])
}
