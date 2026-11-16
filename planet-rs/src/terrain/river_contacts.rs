//! Join touching channel cores through safe existing surface edges before
//! profiles freeze. Spatial proximity alone never paints a cross-bank ramp.
use super::{Drainage, RiverSegment, SurfaceGraph, TerrainOptions, arc};
use crate::{
    DVec3,
    terrain::{feature_index::FeatureIndex, river_widths},
};
use std::{cmp::Ordering, collections::BinaryHeap};

const UNOWNED: usize = usize::MAX;

/// One contact resolution owns its accepted junctions through all width/Q
/// rounds. Replanning can add earlier joins, but cannot undo a committed edge.
/// Every changing round commits a previously untouched vertex, so convergence
/// needs no arbitrary iteration or river-count limit.
pub(super) struct ContactPlan {
    accepted: Vec<Option<usize>>,
}
impl ContactPlan {
    pub(super) fn new(count: usize) -> Self {
        Self {
            accepted: vec![None; count],
        }
    }
    fn allows(&self, from: usize, to: usize) -> bool {
        self.accepted[from].is_none_or(|receiver| receiver == to)
    }
}

#[derive(Clone, Copy)]
struct Segment {
    from: usize,
    to: usize,
}

enum Contact {
    Incoming(Vec<usize>),
    Capture(Vec<usize>),
}

#[derive(Clone, Copy, PartialEq)]
struct State {
    distance: f64,
    vertex: usize,
}
impl Eq for State {}
impl Ord for State {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .distance
            .total_cmp(&self.distance)
            .then_with(|| other.vertex.cmp(&self.vertex))
    }
}
impl PartialOrd for State {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

struct Search {
    stamp: Vec<usize>,
    distance: Vec<f64>,
    previous: Vec<usize>,
    queue: BinaryHeap<State>,
    serial: usize,
}
impl Search {
    fn new(count: usize) -> Self {
        Self {
            stamp: vec![0; count],
            distance: vec![0.0; count],
            previous: vec![UNOWNED; count],
            queue: BinaryHeap::new(),
            serial: 0,
        }
    }
}

struct Planner<'a> {
    directions: &'a [DVec3],
    graph: &'a SurfaceGraph,
    heights: &'a [f64],
    receivers: &'a mut [Option<usize>],
    plan: &'a mut ContactPlan,
    grades: Vec<f64>,
    widths: Vec<f64>,
    offsets: Vec<f64>,
    owner: Vec<usize>,
    segments: Vec<Segment>,
    index: FeatureIndex,
    search: Search,
    maximum_width: f64,
    registered: Vec<Option<usize>>,
}

pub(super) fn join(
    directions: &[DVec3],
    graph: &SurfaceGraph,
    heights: &[f64],
    options: &TerrainOptions,
    drainage: &mut Drainage,
    rivers: &[RiverSegment],
    plan: &mut ContactPlan,
) -> usize {
    if rivers.is_empty() {
        return 0;
    }
    let count = directions.len();
    let widths = river_widths::on_directions(directions, drainage, rivers, options);
    let maximum_width = widths.iter().copied().fold(0.0_f64, f64::max);
    let threshold = options.river_catchment * 4.0 * std::f64::consts::PI;
    let offsets: Vec<_> = drainage
        .catchment
        .iter()
        .map(|&q| options.river_depth * (q / threshold).sqrt().clamp(1.0, 3.0).sqrt() * 0.35)
        .collect();
    let mut grades: Vec<_> = heights
        .iter()
        .zip(&offsets)
        .map(|(&h, &offset)| (h - offset).max(0.0))
        .collect();
    for &i in &drainage.order {
        if let Some(to) = drainage.receivers[i] {
            grades[i] = grades[i].max(grades[to]);
        }
    }
    // Freeze represented-channel membership while physical receivers change.
    // Abandoned suffixes must not become new fictitious source candidates.
    let mut original = vec![None; count];
    let mut candidates = Vec::with_capacity(rivers.len());
    for river in rivers {
        original[river.from] = Some(river.to);
        candidates.push(river.from);
    }
    candidates.sort_unstable_by(|&a, &b| {
        drainage.catchment[b]
            .total_cmp(&drainage.catchment[a])
            .then_with(|| a.cmp(&b))
    });
    let mut upstream = vec![None; count];
    for &from in &candidates {
        if let Some(to) = original[from] {
            upstream[to].get_or_insert(from);
        }
    }
    let mut consumed = vec![false; count];
    let mut planner = Planner {
        directions,
        graph,
        heights,
        receivers: &mut drainage.receivers,
        plan,
        grades,
        widths,
        offsets,
        owner: vec![UNOWNED; count],
        segments: Vec::new(),
        index: FeatureIndex::new(std::iter::empty()),
        search: Search::new(count),
        maximum_width,
        registered: vec![None; count],
    };
    let mut joins = 0;
    for (branch, candidate) in candidates.into_iter().enumerate() {
        if consumed[candidate] {
            continue;
        }
        let mut source = candidate;
        while let Some(next) = upstream[source].filter(|&next| !consumed[next]) {
            source = next;
        }
        let mut current = source;
        while !consumed[current] {
            consumed[current] = true;
            let Some(next) = original[current] else { break };
            current = next;
        }
        joins += usize::from(planner.trace(source, branch));
    }
    joins
}

impl Planner<'_> {
    fn trace(&mut self, source: usize, branch: usize) -> bool {
        let mut prefix = Vec::new();
        let mut current = source;
        let mut joined = false;
        while self.owner[current] == UNOWNED {
            if self.receivers[current].is_none() {
                break;
            }
            self.owner[current] = branch;
            prefix.push(current);
            if let Some(contact) = self.contact(current, branch) {
                match contact {
                    Contact::Incoming(path) => {
                        for pair in path.windows(2) {
                            self.set_receiver(pair[0], pair[1]);
                        }
                        for &vertex in path.iter().skip(1).take(path.len() - 2) {
                            self.owner[vertex] = branch;
                            self.widths[vertex] =
                                self.widths[current].min(self.widths[*path.last().unwrap()]);
                            prefix.push(vertex);
                        }
                        self.grade_path(&path);
                    }
                    Contact::Capture(path) => self.capture(&path),
                }
                joined = true;
                break;
            }
            current = self.receivers[current].unwrap();
        }
        for vertex in prefix {
            self.insert(vertex);
        }
        joined
    }

    fn set_receiver(&mut self, from: usize, to: usize) {
        if self.receivers[from] != Some(to) {
            debug_assert!(self.plan.allows(from, to));
            self.receivers[from] = Some(to);
            self.plan.accepted[from] = Some(to);
        }
    }

    fn insert(&mut self, from: usize) {
        let Some(to) = self.receivers[from] else {
            return;
        };
        if self.registered[from] == Some(to) {
            return;
        }
        if self.grades[from] <= 0.0 || self.grades[to] <= 0.0 {
            return;
        }
        let centre = (self.directions[from] + self.directions[to]).normalize();
        let radius = arc(self.directions[from], self.directions[to]) * 0.5
            + self.widths[from].max(self.widths[to])
            + self.maximum_width;
        let id = self.segments.len();
        self.segments.push(Segment { from, to });
        self.registered[from] = Some(to);
        self.index.insert(id, centre, radius);
    }

    fn sample_segment(&self, segment: Segment, point: DVec3) -> (f64, f64, f64) {
        let a = self.directions[segment.from];
        let b = self.directions[segment.to];
        let t = ((point - a).dot(b - a) / (b - a).length_squared()).clamp(0.0, 1.0);
        let distance = arc(point, a.lerp(b, t).normalize());
        let width =
            self.widths[segment.from] + (self.widths[segment.to] - self.widths[segment.from]) * t;
        let grade =
            self.grades[segment.from] + (self.grades[segment.to] - self.grades[segment.from]) * t;
        (distance, width, grade)
    }

    fn valid_segment(&self, segment: Segment, branch: usize) -> bool {
        self.owner[segment.from] != UNOWNED
            && self.owner[segment.from] != branch
            && self.receivers[segment.from] == Some(segment.to)
    }

    fn contact_segments(&mut self, current: usize, branch: usize) -> Vec<usize> {
        let mut found = Vec::new();
        let point = self.directions[current];
        for &id in self.index.at(point) {
            let segment = self.segments[id];
            let (distance, width, _) = self.sample_segment(segment, point);
            if self.valid_segment(segment, branch) && distance <= self.widths[current] + width {
                found.push(id);
            }
        }
        // The rendered wet boundary can touch across an incident mesh edge:
        // centre A--wet p--wet q--centre B needs three actual edges, not a
        // centreline-only radius test or a generic unbounded neighbourhood.
        self.search.serial += 1;
        let serial = self.search.serial;
        self.search.stamp[current] = serial;
        let mut core = vec![current];
        let mut cursor = 0;
        while cursor < core.len() {
            let q = core[cursor];
            cursor += 1;
            for p in std::iter::once(q).chain(self.graph.neighbours[q].iter().copied()) {
                for &id in self.index.at(self.directions[p]) {
                    let segment = self.segments[id];
                    if self.valid_segment(segment, branch) {
                        let (distance, width, _) = self.sample_segment(segment, self.directions[p]);
                        let incoming_margin = self.widths[current] - arc(point, self.directions[q]);
                        let recipient_margin = width - distance;
                        if recipient_margin >= 0.0
                            && arc(self.directions[q], self.directions[p])
                                <= incoming_margin + recipient_margin
                        {
                            found.push(id);
                        }
                    }
                }
            }
            for &next in &self.graph.neighbours[q] {
                if self.search.stamp[next] != serial
                    && self.receivers[next].is_some()
                    && arc(point, self.directions[next]) <= self.widths[current]
                {
                    self.search.stamp[next] = serial;
                    core.push(next);
                }
            }
        }
        found.sort_unstable();
        found.dedup();
        found
    }

    fn contact(&mut self, current: usize, branch: usize) -> Option<Contact> {
        if self.grades[current] <= 0.0 {
            return None;
        }
        let point = self.directions[current];
        let mut targets = Vec::new();
        for id in self.contact_segments(current, branch) {
            let segment = self.segments[id];
            let (_, width, grade) = self.sample_segment(segment, point);
            let capture = grade > self.grades[current] + 1e-12;
            for target in [segment.from, segment.to] {
                if self.owner[target] == UNOWNED
                    || self.owner[target] == branch
                    || self.grades[target] <= 0.0
                {
                    continue;
                }
                let allowed = if capture {
                    self.grades[target] + 1e-12 >= self.grades[current]
                } else {
                    self.grades[target] <= self.grades[current] + 1e-12
                };
                if allowed {
                    targets.push((
                        arc(point, self.directions[target]),
                        target,
                        2.0 * (self.widths[current] + width),
                        capture,
                    ));
                }
            }
        }
        targets.sort_unstable_by(|a, b| a.0.total_cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
        targets.dedup_by(|a, b| a.1 == b.1 && a.3 == b.3);
        for (_, target, limit, capture) in targets {
            let (from, to) = if capture {
                (target, current)
            } else {
                (current, target)
            };
            if let Some(path) = self.connector(from, to, branch, limit)
                && path
                    .windows(2)
                    .any(|pair| self.receivers[pair[0]] != Some(pair[1]))
                && !self.would_cycle(&path)
                && self.improves_route(&path)
            {
                return Some(if capture {
                    Contact::Capture(path)
                } else {
                    Contact::Incoming(path)
                });
            }
        }
        None
    }

    /// Compare the route after the donor, rather than letting the donor's
    /// own elevation dominate both alternatives. A contact may lower the
    /// required downstream spill head, or shorten a route at the same head.
    /// Compare the established donor, not incidental runoff from unowned
    /// connector cells; their old outlets must not veto a real channel join.
    /// Terrain costs remain independent of catchment-dependent depths/widths.
    fn improves_route(&self, path: &[usize]) -> bool {
        let donor = path[0];
        let Some(old_next) = self.receivers[donor] else {
            return false;
        };
        let (old_head, old_tail) = self.route_cost(old_next);
        let old_distance = arc(self.directions[donor], self.directions[old_next]) + old_tail;
        let (tail_head, tail_distance) = self.route_cost(*path.last().unwrap());
        let new_head = path
            .iter()
            .skip(1)
            .fold(tail_head, |head, &i| head.max(self.heights[i]));
        let new_distance = tail_distance
            + path
                .windows(2)
                .map(|pair| arc(self.directions[pair[0]], self.directions[pair[1]]))
                .sum::<f64>();
        new_head < old_head - 1e-12
            || ((new_head - old_head).abs() <= 1e-12 && new_distance < old_distance - 1e-12)
    }

    fn route_cost(&self, start: usize) -> (f64, f64) {
        let mut head = self.heights[start].max(0.0);
        let mut distance = 0.0;
        let mut current = start;
        while let Some(next) = self.receivers[current] {
            head = head.max(self.heights[next]);
            distance += arc(self.directions[current], self.directions[next]);
            current = next;
        }
        (head, distance)
    }

    fn would_cycle(&mut self, path: &[usize]) -> bool {
        self.search.serial += 1;
        let serial = self.search.serial;
        for &vertex in &path[..path.len() - 1] {
            self.search.stamp[vertex] = serial;
        }
        let mut current = Some(*path.last().unwrap());
        while let Some(vertex) = current {
            if self.search.stamp[vertex] == serial {
                return true;
            }
            current = self.receivers[vertex];
        }
        false
    }

    fn grade_path(&mut self, path: &[usize]) {
        for &vertex in path.iter().rev() {
            if let Some(next) = self.receivers[vertex] {
                self.grades[vertex] =
                    (self.heights[vertex] - self.offsets[vertex]).max(self.grades[next]);
            }
        }
    }

    fn capture(&mut self, path: &[usize]) {
        let donor = path[0];
        let priority = self.owner[donor];
        let mut suffix = Vec::new();
        let mut current = Some(*path.last().unwrap());
        self.search.serial += 1;
        let serial = self.search.serial;
        while let Some(vertex) = current {
            self.search.stamp[vertex] = serial;
            suffix.push(vertex);
            current = self.receivers[vertex];
        }
        let mut abandoned = self.receivers[donor];
        while let Some(vertex) = abandoned {
            if self.search.stamp[vertex] == serial {
                break;
            }
            if self.owner[vertex] == priority {
                self.owner[vertex] = UNOWNED;
            }
            abandoned = self.receivers[vertex];
        }
        for pair in path.windows(2) {
            self.set_receiver(pair[0], pair[1]);
        }
        for &vertex in path.iter().chain(&suffix) {
            self.owner[vertex] = self.owner[vertex].min(priority);
        }
        self.grade_path(&suffix);
        self.grade_path(path);
        for &vertex in path.iter().chain(&suffix) {
            self.insert(vertex);
        }
    }

    fn edge_is_land(&self, from: usize, to: usize) -> bool {
        let a = self.directions[from] * (1.0 + self.heights[from]);
        let b = self.directions[to] * (1.0 + self.heights[to]);
        let edge = b - a;
        let t = (-a.dot(edge) / edge.length_squared()).clamp(0.0, 1.0);
        (a + edge * t).length_squared() >= 1.0
    }

    fn connector(
        &mut self,
        from: usize,
        target: usize,
        branch: usize,
        limit: f64,
    ) -> Option<Vec<usize>> {
        let search = &mut self.search;
        search.serial += 1;
        search.queue.clear();
        search.stamp[from] = search.serial;
        search.distance[from] = 0.0;
        search.previous[from] = UNOWNED;
        search.queue.push(State {
            distance: 0.0,
            vertex: from,
        });
        let ceiling = self.grades[from] + self.offsets[from].max(self.offsets[target]);
        while let Some(state) = self.search.queue.pop() {
            let i = state.vertex;
            if state.distance > self.search.distance[i] {
                continue;
            }
            if i == target {
                let mut path = vec![i];
                let mut next = self.search.previous[i];
                while next != UNOWNED {
                    path.push(next);
                    next = self.search.previous[next];
                }
                path.reverse();
                return Some(path);
            }
            for &j in &self.graph.neighbours[i] {
                if !self.plan.allows(i, j) {
                    continue;
                }
                if j != target && (self.owner[j] != UNOWNED || self.receivers[j].is_none()) {
                    continue;
                }
                if (j != target && self.owner[j] == branch)
                    || self.heights[j] > ceiling + 1e-12
                    || !self.edge_is_land(i, j)
                {
                    continue;
                }
                let distance = state.distance + arc(self.directions[i], self.directions[j]);
                if distance > limit + 1e-12 {
                    continue;
                }
                if self.search.stamp[j] != self.search.serial || distance < self.search.distance[j]
                {
                    self.search.stamp[j] = self.search.serial;
                    self.search.distance[j] = distance;
                    self.search.previous[j] = i;
                    self.search.queue.push(State {
                        distance,
                        vertex: j,
                    });
                }
            }
        }
        None
    }
}

#[cfg(test)]
#[path = "river_contacts_tests.rs"]
mod tests;
