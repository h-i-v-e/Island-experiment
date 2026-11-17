//! Physical segment contact with a local, distance-bounded terrain connector.
//! Index accepted trunks only; stale entries are checked against current owners
//! and receiver edges after downhill captures. No all-river pair scan is needed.
use super::{Contact, ContactEnvironment, Owner, Search, Vec2};
use std::{
    cmp::Ordering,
    collections::{BinaryHeap, HashMap},
};

const NEAR_CHANNEL_MARGIN: f32 = 1.0 / super::super::ISLAND_WORLD_METRES;

#[derive(Clone, Copy)]
struct Segment {
    from: usize,
    to: usize,
    river: usize,
    half_width: f32,
}

pub(super) struct SegmentIndex {
    cell_size: f32,
    segments: Vec<Segment>,
    by_start: HashMap<usize, usize>,
    cells: HashMap<(i32, i32), Vec<usize>>,
}
impl SegmentIndex {
    pub(super) fn new(maximum_width: f32) -> Self {
        Self {
            cell_size: maximum_width.max(NEAR_CHANNEL_MARGIN * 2.),
            segments: Vec::new(),
            by_start: HashMap::new(),
            cells: HashMap::new(),
        }
    }
    pub(super) fn insert(
        &mut self,
        environment: &ContactEnvironment<'_>,
        owners: &[Option<Owner>],
        downstream: &[usize],
        from: usize,
    ) {
        let to = downstream[from];
        let (Some(owner), Some(end)) = (owners[from], owners[to]) else {
            return;
        };
        if from == to || environment.ocean[from] || environment.ocean[to] {
            return;
        }
        let half_width = owner.half_width.max(end.half_width);
        if let Some(&index) = self.by_start.get(&from) {
            let previous = self.segments[index];
            if previous.to == to
                && previous.river == owner.river
                && previous.half_width >= half_width
            {
                return;
            }
        }
        let segment = Segment {
            from,
            to,
            river: owner.river,
            half_width,
        };
        let index = if let Some(&index) = self.by_start.get(&from) {
            self.segments[index] = segment;
            index
        } else {
            let index = self.segments.len();
            self.segments.push(segment);
            self.by_start.insert(from, index);
            index
        };
        let a = environment.mesh.vertices[from].truncate();
        let b = environment.mesh.vertices[to].truncate();
        let radius = half_width + NEAR_CHANNEL_MARGIN;
        let (min, max) = self.bounds(a, b, radius);
        for y in min.1..=max.1 {
            for x in min.0..=max.0 {
                self.cells.entry((x, y)).or_default().push(index);
            }
        }
    }
    fn bounds(&self, a: Vec2, b: Vec2, radius: f32) -> ((i32, i32), (i32, i32)) {
        let min = ((a.min(b) - Vec2::splat(radius)) / self.cell_size)
            .floor()
            .as_ivec2();
        let max = ((a.max(b) + Vec2::splat(radius)) / self.cell_size)
            .floor()
            .as_ivec2();
        ((min.x, min.y), (max.x, max.y))
    }
    pub(super) fn find_contact(
        &self,
        environment: &ContactEnvironment<'_>,
        search: &Search<'_>,
    ) -> Option<Contact> {
        if environment.blocked(search.current, search) {
            return None;
        }
        let next = search.downstream[search.current];
        let a = environment.mesh.vertices[search.current].truncate();
        let b = environment.mesh.vertices[next].truncate();
        let (min, max) = self.bounds(a, b, search.half_width);
        let mut candidates = Vec::new();
        for y in min.1..=max.1 {
            for x in min.0..=max.0 {
                if let Some(indices) = self.cells.get(&(x, y)) {
                    candidates.extend_from_slice(indices);
                }
            }
        }
        candidates.sort_unstable();
        candidates.dedup();
        let mut best = None;
        for index in candidates {
            Self::consider_segment(environment, search, self.segments[index], a, b, &mut best);
        }
        best
    }
    fn consider_segment(
        environment: &ContactEnvironment<'_>,
        search: &Search<'_>,
        segment: Segment,
        a: Vec2,
        b: Vec2,
        best: &mut Option<Contact>,
    ) {
        if search.downstream[segment.from] != segment.to {
            return;
        }
        let (Some(first), Some(last)) = (search.owners[segment.from], search.owners[segment.to])
        else {
            return;
        };
        if first.river != segment.river {
            return;
        }
        let c = environment.mesh.vertices[segment.from].truncate();
        let d = environment.mesh.vertices[segment.to].truncate();
        let (incoming, receiving, distance) = closest_parameters(a, b, c, d);
        let width = first.half_width.max(last.half_width) + search.half_width + NEAR_CHANNEL_MARGIN;
        if distance > width {
            return;
        }
        let next_level = environment.mesh.vertices[search.downstream[search.current]].z;
        let incoming_level = (next_level - search.level).mul_add(incoming, search.level);
        let receiving_level = (last.level - first.level).mul_add(receiving, first.level);
        // A higher established segment must turn into the lower new channel;
        // redirect its upstream end rather than allowing it to remain parallel.
        let target = if receiving_level > incoming_level + 1e-6 {
            segment.from
        } else {
            segment.to
        };
        let Some(owner) = search.owners[target] else {
            return;
        };
        if environment.blocked(target, search) || search.prefix[target] == search.stamp {
            return;
        }
        let Some(path) = connector(environment, search, target, owner, width) else {
            return;
        };
        let distance = path
            .windows(2)
            .map(|edge| environment.distance(edge[0], edge[1]))
            .sum();
        environment.consider(
            search,
            Contact {
                nodes: path.len(),
                path,
                owner,
                distance,
            },
            best,
        );
    }
}

fn project(point: Vec2, start: Vec2, end: Vec2) -> (f32, f32) {
    let delta = end - start;
    let t = ((point - start).dot(delta) / delta.length_squared().max(1e-20)).clamp(0., 1.);
    (t, point.distance(start + delta * t))
}
#[allow(clippy::many_single_char_names)] // Standard endpoint notation for two line segments.
fn closest_parameters(a: Vec2, b: Vec2, c: Vec2, d: Vec2) -> (f32, f32, f32) {
    let ab = b - a;
    let cd = d - c;
    let cross = ab.perp_dot(cd);
    if cross.abs() > 1e-20 {
        let t = (c - a).perp_dot(cd) / cross;
        let u = (c - a).perp_dot(ab) / cross;
        if (0. ..=1.).contains(&t) && (0. ..=1.).contains(&u) {
            return (t, u, 0.);
        }
    }
    let (u, dist) = project(a, c, d);
    let mut best = (0., u, dist);
    let (u, dist) = project(b, c, d);
    if dist < best.2 {
        best = (1., u, dist);
    }
    let (t, dist) = project(c, a, b);
    if dist < best.2 {
        best = (t, 0., dist);
    }
    let (t, dist) = project(d, a, b);
    if dist < best.2 {
        best = (t, 1., dist);
    }
    best
}

#[derive(Clone, Copy)]
struct Pending {
    vertex: usize,
    cost: f32,
    estimate: f32,
}
impl PartialEq for Pending {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}
impl Eq for Pending {}
impl PartialOrd for Pending {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for Pending {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .estimate
            .total_cmp(&self.estimate)
            .then_with(|| other.cost.total_cmp(&self.cost))
            .then_with(|| other.vertex.cmp(&self.vertex))
    }
}
#[derive(Clone, Copy)]
struct Visit {
    cost: f32,
    previous: usize,
}

fn connector(
    environment: &ContactEnvironment<'_>,
    search: &Search<'_>,
    goal: usize,
    owner: Owner,
    width: f32,
) -> Option<Vec<usize>> {
    let start = search.current;
    let start_position = environment.mesh.vertices[start].truncate();
    let goal_position = environment.mesh.vertices[goal].truncate();
    let direct = start_position.distance(goal_position);
    let maximum_distance = direct * 1.75 + width;
    let donor_head = search.level.max(owner.level);
    let reverse = owner.level > search.level + 1e-6;
    let mut pending = BinaryHeap::from([Pending {
        vertex: start,
        cost: 0.,
        estimate: direct,
    }]);
    let mut visited = HashMap::from([(
        start,
        Visit {
            cost: 0.,
            previous: start,
        },
    )]);
    while let Some(current) = pending.pop() {
        if current.cost > visited[&current.vertex].cost {
            continue;
        }
        if current.vertex == goal {
            return Some(reconstruct(&visited, start, goal));
        }
        for &next in &environment.adjacency[current.vertex] {
            let position = environment.mesh.vertices[next].truncate();
            let cost = current.cost + environment.distance(current.vertex, next);
            let estimate = cost + position.distance(goal_position);
            let (from, to) = if reverse {
                (next, current.vertex)
            } else {
                (current.vertex, next)
            };
            if environment.blocked(next, search)
                || (search.prefix[next] == search.stamp && next != start)
                || (next != goal && search.owners[next].is_some())
                || (next != goal && environment.mesh.vertices[next].z > donor_head + 1e-6)
                || (search.committed[from] && search.downstream[from] != to)
                || estimate > maximum_distance
                || project(position, start_position, goal_position).1 > width
                || visited.get(&next).is_some_and(|old| old.cost <= cost)
            {
                continue;
            }
            visited.insert(
                next,
                Visit {
                    cost,
                    previous: current.vertex,
                },
            );
            pending.push(Pending {
                vertex: next,
                cost,
                estimate,
            });
        }
    }
    None
}
fn reconstruct(visited: &HashMap<usize, Visit>, start: usize, goal: usize) -> Vec<usize> {
    let mut path = vec![goal];
    let mut current = goal;
    while current != start {
        current = visited[&current].previous;
        path.push(current);
    }
    path.reverse();
    path
}
