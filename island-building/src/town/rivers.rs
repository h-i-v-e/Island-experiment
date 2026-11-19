use super::{TownInput, TownMouth};

pub(super) fn distance(a: [f32; 2], b: [f32; 2]) -> f32 {
    (b[0] - a[0]).hypot(b[1] - a[1])
}

/// Project a point onto a segment, returning its parameter and distance.
pub(super) fn projection(point: [f32; 2], a: [f32; 2], b: [f32; 2]) -> (f32, f32) {
    let d = [b[0] - a[0], b[1] - a[1]];
    let squared = d[0] * d[0] + d[1] * d[1];
    let t = if squared > 1e-8 {
        ((point[0] - a[0]) * d[0] + (point[1] - a[1]) * d[1]) / squared
    } else {
        0.
    };
    let t = t.clamp(0., 1.);
    (t, distance(point, [a[0] + d[0] * t, a[1] + d[1] * t]))
}

#[derive(Clone, Copy)]
struct Route {
    length: f32,
    position: [f32; 2],
    upstream: [f32; 2],
    outlet: usize,
}

pub(super) fn longest_mouth(input: &TownInput) -> Option<TownMouth> {
    routes_to_sea(input)
        .into_iter()
        .enumerate()
        .filter_map(|(index, route)| route.map(|route| mouth(index, route)))
        .max_by(|a, b| {
            a.river_length
                .total_cmp(&b.river_length)
                .then_with(|| b.river_index.cmp(&a.river_index))
        })
}

/// Put the outlet nearest the midpoint of the occupied coastline first.
/// Villages follow in geographic order for stable priority.
pub(super) fn settlement_mouths(input: &TownInput) -> Vec<TownMouth> {
    let routes = routes_to_sea(input);
    let mut outlets = Vec::<TownMouth>::new();
    for (index, route) in routes.iter().enumerate() {
        let Some(route) = route.filter(|route| route.outlet == index) else {
            continue;
        };
        if outlets
            .iter()
            .all(|outlet| distance(outlet.position, route.position) > JOIN_TOLERANCE)
        {
            outlets.push(mouth(index, route));
        }
    }
    outlets.sort_by(|a, b| {
        a.position[0]
            .total_cmp(&b.position[0])
            .then_with(|| a.position[1].total_cmp(&b.position[1]))
            .then_with(|| a.river_index.cmp(&b.river_index))
    });
    if let [first, .., last] = outlets.as_slice() {
        let (min_y, max_y) = outlets
            .iter()
            .map(|mouth| mouth.position[1])
            .fold((f32::INFINITY, f32::NEG_INFINITY), |(lo, hi), y| {
                (lo.min(y), hi.max(y))
            });
        let (min_x, max_x) = (first.position[0], last.position[0]);
        let (axis, midpoint) = if max_y - min_y > (max_x - min_x) * 2. {
            (1, f32::midpoint(min_y, max_y))
        } else {
            (0, f32::midpoint(min_x, max_x))
        };
        let (capital, _) = outlets
            .iter()
            .map(|candidate| (candidate.position[axis] - midpoint).abs())
            .enumerate()
            .min_by(|(a, dist_a), (b, dist_b)| dist_a.total_cmp(dist_b).then(a.cmp(b)))
            .expect("two or more outlets");
        outlets[..=capital].rotate_right(1);
    }
    outlets
}

const JOIN_TOLERANCE: f32 = 0.25;

fn mouth(river_index: usize, route: Route) -> TownMouth {
    TownMouth {
        river_index,
        position: route.position,
        upstream: route.upstream,
        river_length: route.length,
    }
}

fn routes_to_sea(input: &TownInput) -> Vec<Option<Route>> {
    let mut routes = vec![None; input.rivers.len()];
    let mut visiting = vec![false; input.rivers.len()];
    let mut evaluated = vec![false; input.rivers.len()];
    for index in 0..input.rivers.len() {
        trace(input, index, &mut routes, &mut visiting, &mut evaluated);
    }
    routes
}

fn trace(
    input: &TownInput,
    index: usize,
    routes: &mut [Option<Route>],
    visiting: &mut [bool],
    evaluated: &mut [bool],
) -> Option<Route> {
    if evaluated[index] {
        return routes[index];
    }
    if visiting[index] {
        return None;
    }
    visiting[index] = true;
    let river = &input.rivers[index];
    let mut length = 0.;
    // Locate the first dry-to-sea crossing along this source-to-mouth line.
    // A submerged downstream tail cannot make one river artificially longer.
    for segment in river.points.windows(2) {
        let a = segment[0];
        let b = segment[1];
        let ha = input.terrain.sample(a[0], a[1]);
        let hb = input.terrain.sample(b[0], b[1]);
        if let (Some(ha), Some(hb)) = (ha, hb)
            && river.join.is_none()
            && ha > input.sea_level
            && hb <= input.sea_level + 0.02
        {
            let mut lower = 0.;
            let mut upper = 1.;
            for _ in 0..24 {
                let middle = (lower + upper) * 0.5;
                let p = [a[0] + (b[0] - a[0]) * middle, a[1] + (b[1] - a[1]) * middle];
                if input
                    .terrain
                    .sample(p[0], p[1])
                    .is_some_and(|height| height > input.sea_level)
                {
                    lower = middle;
                } else {
                    upper = middle;
                }
            }
            let t = upper;
            let mouth = [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t];
            let route = Route {
                length: length + distance(a, mouth),
                position: mouth,
                upstream: a,
                outlet: index,
            };
            routes[index] = Some(route);
            visiting[index] = false;
            evaluated[index] = true;
            return Some(route);
        }
        length += distance(a, b);
    }
    let endpoint = *river.points.last()?;
    let mut best: Option<Route> = None;
    // Tributaries contribute only their own length plus the remaining downstream
    // suffix. Never sum the sibling catchment or count a shared main stem twice.
    for (other_index, other) in input.rivers.iter().enumerate() {
        if other_index == index
            || visiting[other_index]
            || river.join.is_some_and(|join| join != other_index)
        {
            continue;
        }
        let nearest = river.join.and_then(|_| {
            other
                .points
                .windows(2)
                .enumerate()
                .min_by(|(_, a), (_, b)| {
                    projection(endpoint, a[0], a[1])
                        .1
                        .total_cmp(&projection(endpoint, b[0], b[1]).1)
                })
                .map(|(index, _)| index)
        });
        let mut before = 0.;
        for (segment_index, pair) in other.points.windows(2).enumerate() {
            let segment_length = distance(pair[0], pair[1]);
            let (t, separation) = projection(endpoint, pair[0], pair[1]);
            if (nearest == Some(segment_index)
                || (river.join.is_none() && separation <= JOIN_TOLERANCE))
                && let Some(downstream) = trace(input, other_index, routes, visiting, evaluated)
            {
                let remaining = downstream.length - before - segment_length * t;
                if remaining > 0.01 {
                    let route = Route {
                        length: length + remaining,
                        ..downstream
                    };
                    if best.is_none_or(|current| route.length > current.length) {
                        best = Some(route);
                    }
                }
            }
            before += segment_length;
        }
    }
    visiting[index] = false;
    evaluated[index] = true;
    routes[index] = best;
    best
}
