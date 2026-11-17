use super::{
    MAX_ROAD_EDGES, RoadEdge, RoadGradeOptions, RoadJunction, RoadNetwork, RoadStation, RoadStrip,
    axis_aligned, shoulder_weight,
};
use crate::Vec2;
use std::cmp::Ordering;
use std::collections::BinaryHeap;

#[derive(Clone, Copy)]
struct HeightBounds {
    low: f32,
    high: f32,
}

impl HeightBounds {
    fn open(min_height: f32) -> Self {
        Self {
            low: min_height,
            high: f32::INFINITY,
        }
    }

    fn intersect(&mut self, other: Self) {
        self.low = self.low.max(other.low);
        self.high = self.high.min(other.high);
    }

    fn probe(
        &mut self,
        options: RoadGradeOptions,
        terrain: &impl Fn(Vec2) -> Option<f32>,
        point: Vec2,
        outside: f32,
    ) -> Option<()> {
        let height = terrain(point).filter(|h| h.is_finite() && *h >= options.min_height)?;
        let weight = shoulder_weight(outside, options.shoulder);
        if weight > 0. {
            self.intersect(Self {
                low: height - options.max_cut / weight,
                high: if outside <= 0. { height } else { f32::INFINITY },
            });
        }
        self.is_feasible().then_some(())
    }

    fn is_feasible(self) -> bool {
        self.low <= self.high + 0.0001
    }
}

struct Profile {
    source: usize,
    edge: RoadEdge,
    start: Vec2,
    end: Vec2,
    bounds: Vec<HeightBounds>,
}

impl Profile {
    fn point(&self, index: usize) -> Vec2 {
        self.start
            .lerp(self.end, fraction(index, self.bounds.len() - 1))
    }
}

#[derive(Clone, Copy)]
struct Ray {
    source: usize,
    direction: Vec2,
    half_width: f32,
}

/// Grade a planar graph whose crossings are split into shared nodes.
///
/// The terrain callback supplies natural, dry heights and may reject protected
/// locations. Sample spacing controls the smallest feature the solver can
/// resolve. Infeasible edges are removed whole, so the caller can submit short
/// edges and keep only the connected part of the result.
pub fn grade_roads(
    nodes: &[Vec2],
    edges: &[RoadEdge],
    options: RoadGradeOptions,
    terrain: impl Fn(Vec2) -> Option<f32>,
) -> RoadNetwork {
    if !valid_options(options) || nodes.len() > 8192 || edges.len() > MAX_ROAD_EDGES {
        return RoadNetwork::default();
    }

    let mut active: Vec<_> = edges.iter().map(|edge| valid_edge(nodes, *edge)).collect();
    while active.iter().any(|&a| a) {
        let Some((junctions, bounds, profiles)) =
            build_profiles(nodes, edges, &mut active, options, &terrain)
        else {
            continue;
        };
        match solve(&bounds, &profiles, options) {
            Ok((heights, stations)) => {
                return build_network(junctions, &heights, profiles, stations, options);
            }
            Err(source) => active[source] = false,
        }
    }
    RoadNetwork::default()
}

fn valid_options(o: RoadGradeOptions) -> bool {
    o.min_height.is_finite()
        && o.max_grade.is_finite()
        && (0. ..=1.).contains(&o.max_grade)
        && o.max_cut.is_finite()
        && (0. ..=100.).contains(&o.max_cut)
        && o.shoulder.is_finite()
        && (0. ..=50.).contains(&o.shoulder)
        && o.sample_spacing.is_finite()
        && (0.05..=10.).contains(&o.sample_spacing)
}

fn valid_edge(nodes: &[Vec2], edge: RoadEdge) -> bool {
    let (Some(a), Some(b)) = (nodes.get(edge.start), nodes.get(edge.end)) else {
        return false;
    };
    a.is_finite()
        && b.is_finite()
        && (0.05..=4000.).contains(&a.distance(*b))
        && (0.05..=50.).contains(&edge.half_width)
}

type Profiles = (Vec<RoadJunction>, Vec<HeightBounds>, Vec<Profile>);

fn build_profiles(
    nodes: &[Vec2],
    edges: &[RoadEdge],
    active: &mut [bool],
    options: RoadGradeOptions,
    terrain: &impl Fn(Vec2) -> Option<f32>,
) -> Option<Profiles> {
    let incident = incident_rays(nodes, edges, active);
    let junctions: Vec<_> = incident
        .iter()
        .zip(nodes)
        .enumerate()
        .map(|(node, (rays, &point))| junction_shape(node, point, rays))
        .collect();
    let mut changed = false;
    let mut bounds = vec![HeightBounds::open(options.min_height); nodes.len()];
    for (node, rays) in incident.iter().enumerate() {
        let Some(last) = rays.last() else {
            continue;
        };
        if let Some(interval) = junction_bounds(junctions[node], options, terrain) {
            bounds[node] = interval;
        } else {
            active[last.source] = false;
            changed = true;
        }
    }
    if changed {
        return None;
    }
    let mut profiles = Vec::new();
    let mut stations = 0;
    for (source, &edge) in edges.iter().enumerate().filter(|(i, _)| active[*i]) {
        if let Some(profile) = profile(nodes, edge, source, &junctions, options, terrain) {
            stations += profile.bounds.len();
            profiles.push(profile);
        } else {
            changed = true;
        }
    }
    if changed {
        let mut valid = profiles.iter().map(|profile| profile.source).peekable();
        for (source, kept) in active.iter_mut().enumerate() {
            *kept = valid.next_if_eq(&source).is_some();
        }
        return None;
    }
    if stations > 65_536 {
        active.fill(false);
        return None;
    }
    Some((junctions, bounds, profiles))
}

fn incident_rays(nodes: &[Vec2], edges: &[RoadEdge], active: &[bool]) -> Vec<Vec<Ray>> {
    let mut incident = vec![Vec::new(); nodes.len()];
    for (source, edge) in edges.iter().enumerate().filter(|(i, _)| active[*i]) {
        let direction = (nodes[edge.end] - nodes[edge.start]).normalize();
        incident[edge.start].push(Ray {
            source,
            direction,
            half_width: edge.half_width,
        });
        incident[edge.end].push(Ray {
            source,
            direction: -direction,
            half_width: edge.half_width,
        });
    }
    incident
}

fn junction_shape(node: usize, point: Vec2, rays: &[Ray]) -> RoadJunction {
    let mut junction = RoadJunction {
        node,
        point,
        height: 0.,
        radius: 0.,
        square_axis: None,
        rectangle_half_extents: None,
    };
    let [first, rest @ ..] = rays else {
        return junction;
    };
    if rest.is_empty() {
        return junction; // Single strips have a straight end across their full width.
    }
    if let [a, b] = rays
        && a.direction.dot(b.direction) < -0.999_999
        && (a.half_width - b.half_width).abs() < 0.0001
    {
        return junction;
    }
    let axis = first.direction;
    if rays.len() <= 4
        && rest.iter().enumerate().all(|(i, ray)| {
            axis_aligned(axis, ray.direction)
                && rays[..=i]
                    .iter()
                    .all(|other| other.direction.dot(ray.direction) < 0.999_999)
        })
    {
        junction.square_axis = Some(axis);
        let mut half = Vec2::ZERO;
        for ray in rays {
            if axis.dot(ray.direction).abs() > 0.5 {
                half.y = half.y.max(ray.half_width);
            } else {
                half.x = half.x.max(ray.half_width);
            }
        }
        junction.radius = half.max_element();
        // A straight change of width still needs a short transition along the road.
        if half.x == 0. {
            half.x = half.y;
        }
        junction.rectangle_half_extents = Some(half);
    } else {
        junction.radius = circular_radius(rays);
    }
    junction
}

fn circular_radius(rays: &[Ray]) -> f32 {
    rays.iter().enumerate().fold(0., |radius, (index, a)| {
        rays[..index]
            .iter()
            .fold(radius.max(a.half_width), |radius, b| {
                let sine_half_angle = (0.5 * (1. - a.direction.dot(b.direction))).sqrt();
                radius.max((a.half_width.max(b.half_width) + 0.001) / sine_half_angle)
            })
    })
}

fn junction_bounds(
    junction: RoadJunction,
    options: RoadGradeOptions,
    terrain: &impl Fn(Vec2) -> Option<f32>,
) -> Option<HeightBounds> {
    let RoadJunction {
        point: centre,
        radius,
        ..
    } = junction;
    if radius > 100. {
        return None;
    }
    let mut bounds = HeightBounds::open(options.min_height);
    bounds.probe(options, terrain, centre, 0.)?;
    if let Some((axis, half)) = junction.rectangle() {
        let across = rectangle_samples(half.y, options);
        for x in rectangle_samples(half.x, options) {
            for y in across.clone() {
                let outside = (Vec2::new(x, y).abs() - half).max(Vec2::ZERO).length();
                if outside <= options.shoulder + 0.0001 {
                    let point = centre + axis * x + axis.perp() * y;
                    bounds.probe(options, terrain, point, outside)?;
                }
            }
        }
        return Some(bounds);
    }
    for (start, span) in [(0., radius), (radius, options.shoulder)] {
        if span == 0. {
            continue;
        }
        let rings = steps(span, options.sample_spacing);
        for ring in 1..=rings {
            let depth = span * fraction(ring, rings);
            let r = start + depth;
            let samples = steps(std::f32::consts::TAU * r, options.sample_spacing).max(8);
            for index in 0..samples {
                let angle = std::f32::consts::TAU * fraction(index, samples);
                let offset = r * Vec2::from_angle(angle);
                bounds.probe(options, terrain, centre + offset, (r - radius).max(0.))?;
            }
        }
    }
    Some(bounds)
}

fn rectangle_samples(radius: f32, options: RoadGradeOptions) -> impl Iterator<Item = f32> + Clone {
    let core = steps(2. * radius, options.sample_spacing);
    let shoulder = steps(options.shoulder, options.sample_spacing);
    (0..=core)
        .map(move |i| -radius + 2. * radius * fraction(i, core))
        .chain((1..=shoulder).flat_map(move |i| {
            let outer = radius + options.shoulder * fraction(i, shoulder);
            [-outer, outer]
        }))
}

fn profile(
    nodes: &[Vec2],
    edge: RoadEdge,
    source: usize,
    junctions: &[RoadJunction],
    options: RoadGradeOptions,
    terrain: &impl Fn(Vec2) -> Option<f32>,
) -> Option<Profile> {
    let a = nodes[edge.start];
    let b = nodes[edge.end];
    let direction = (b - a).normalize();
    let start_trim = junctions[edge.start].seam_offset(direction, edge.half_width);
    let end_trim = junctions[edge.end].seam_offset(-direction, edge.half_width);
    let length = b.distance(a) - start_trim - end_trim;
    if length < 0.05 {
        return None;
    }
    let start = a + direction * start_trim;
    let end = b - direction * end_trim;
    let spans = steps(length, options.sample_spacing);
    let mut bounds = Vec::with_capacity(spans + 1);
    for index in 0..=spans {
        bounds.push(section_bounds(
            start.lerp(end, fraction(index, spans)),
            direction,
            edge.half_width,
            options,
            terrain,
        )?);
    }
    for index in 0..spans {
        let middle = start.lerp(
            end,
            (fraction(index, spans) + fraction(index + 1, spans)) * 0.5,
        );
        let interval = section_bounds(middle, direction, edge.half_width, options, terrain)?;
        // The interpolated surface at this probe is bounded by both adjacent
        // station heights. Constraining both prevents interpolating over a dip.
        bounds[index].intersect(interval);
        bounds[index + 1].intersect(interval);
    }
    bounds
        .iter()
        .all(|bound| bound.is_feasible())
        .then_some(Profile {
            source,
            edge,
            start,
            end,
            bounds,
        })
}

fn section_bounds(
    point: Vec2,
    direction: Vec2,
    half_width: f32,
    options: RoadGradeOptions,
    terrain: &impl Fn(Vec2) -> Option<f32>,
) -> Option<HeightBounds> {
    let side = direction.perp();
    let mut bounds = HeightBounds::open(options.min_height);
    bounds.probe(options, terrain, point, 0.)?;
    for (start, span) in [(0., half_width), (half_width, options.shoulder)] {
        if span == 0. {
            continue;
        }
        let count = steps(span, options.sample_spacing);
        for index in 1..=count {
            let offset = start + span * fraction(index, count);
            for sign in [-1., 1.] {
                bounds.probe(
                    options,
                    terrain,
                    point + side * offset * sign,
                    (offset - half_width).max(0.),
                )?;
            }
        }
    }
    Some(bounds)
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn steps(distance: f32, spacing: f32) -> usize {
    (distance / spacing).ceil().max(1.) as usize
}

#[allow(clippy::cast_precision_loss)]
fn fraction(index: usize, count: usize) -> f32 {
    index as f32 / count as f32
}

#[derive(Clone, Copy)]
struct Candidate {
    height: f32,
    vertex: usize,
}

impl Eq for Candidate {}

impl PartialEq for Candidate {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other).is_eq()
    }
}

impl PartialOrd for Candidate {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Candidate {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .height
            .total_cmp(&self.height)
            .then_with(|| other.vertex.cmp(&self.vertex))
    }
}

type Solved = (Vec<f32>, Vec<Vec<usize>>);

fn solve(
    junctions: &[HeightBounds],
    profiles: &[Profile],
    options: RoadGradeOptions,
) -> Result<Solved, usize> {
    let mut bounds = junctions.to_vec();
    let mut incidence = vec![None; junctions.len()];
    let mut links = vec![Vec::new(); junctions.len()];
    let mut stations = Vec::with_capacity(profiles.len());
    for profile in profiles {
        let edge = profile.edge;
        let mut indices = Vec::with_capacity(profile.bounds.len());
        for (index, &interval) in profile.bounds.iter().enumerate() {
            let vertex = if index == 0 {
                edge.start
            } else if index + 1 == profile.bounds.len() {
                edge.end
            } else {
                let vertex = bounds.len();
                bounds.push(interval);
                incidence.push(Some(profile.source));
                links.push(Vec::new());
                vertex
            };
            bounds[vertex].intersect(interval);
            incidence[vertex] = Some(profile.source);
            indices.push(vertex);
        }
        for (index, span) in indices.windows(2).enumerate() {
            let run = profile.point(index).distance(profile.point(index + 1));
            let rise = options.max_grade * run;
            links[span[0]].push((span[1], rise));
            links[span[1]].push((span[0], rise));
        }
        stations.push(indices);
    }

    let heights = upper_envelope(&bounds, &links);
    if let Some(source) = bounds
        .iter()
        .zip(&heights)
        .zip(incidence)
        .filter_map(|((interval, &height), source)| {
            (height + 0.0001 < interval.low).then_some(source).flatten()
        })
        .max()
    {
        return Err(source);
    }
    Ok((heights, stations))
}

fn upper_envelope(bounds: &[HeightBounds], links: &[Vec<(usize, f32)>]) -> Vec<f32> {
    // Every upper bound is a source. The least upper bound propagated through
    // the gradient constraints is the highest feasible, least-cut solution.
    let mut queue = BinaryHeap::new();
    let mut heights: Vec<_> = bounds
        .iter()
        .enumerate()
        .map(|(vertex, bounds)| {
            if bounds.high.is_finite() {
                queue.push(Candidate {
                    height: bounds.high,
                    vertex,
                });
            }
            bounds.high
        })
        .collect();

    while let Some(Candidate { height, vertex }) = queue.pop() {
        if height > heights[vertex] {
            continue;
        }
        for &(neighbour, rise) in &links[vertex] {
            let mut cap = height + rise;
            if cap - height > rise {
                cap = cap.next_down();
            }
            if cap < heights[neighbour] {
                heights[neighbour] = cap;
                queue.push(Candidate {
                    height: cap,
                    vertex: neighbour,
                });
            }
        }
    }
    heights
}

fn build_network(
    junctions: Vec<RoadJunction>,
    heights: &[f32],
    profiles: Vec<Profile>,
    stations: Vec<Vec<usize>>,
    options: RoadGradeOptions,
) -> RoadNetwork {
    let junctions = junctions
        .into_iter()
        .zip(heights)
        .filter_map(|(mut junction, &height)| {
            junction.height = height;
            (junction.radius > 0. && height.is_finite()).then_some(junction)
        })
        .collect();
    let strips = profiles
        .into_iter()
        .zip(stations)
        .map(|(profile, indices)| RoadStrip {
            source: profile.source,
            start: profile.edge.start,
            end: profile.edge.end,
            half_width: profile.edge.half_width,
            stations: indices
                .iter()
                .enumerate()
                .map(|(index, &station)| RoadStation {
                    point: profile.point(index),
                    height: heights[station],
                })
                .collect(),
        })
        .collect();
    RoadNetwork {
        strips,
        junctions,
        shoulder: options.shoulder,
    }
}
