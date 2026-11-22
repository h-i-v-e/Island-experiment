use super::geometry::{segment_near, segments_within};
use crate::terrain::{cart_tracks::CartTrackRun, spatial_bins::SpatialBins};
use crate::{ISLAND_WORLD_METRES, Vec2};

const TRACK_RESERVE: f32 = 3.5;
const JUNCTION_NEAR: f32 = 3.75;
const JUNCTION_APPROACH: f32 = 13.;

pub(super) struct TrackNetwork {
    pub(super) routes: Vec<Route>,
    segments: SpatialBins<TrackSegment>,
}

impl TrackNetwork {
    pub(super) fn new(runs: &[CartTrackRun]) -> Self {
        let mut routes: Vec<_> = runs.iter().filter_map(Route::new).collect();
        let mut segments = SpatialBins::new();
        let radius = Vec2::splat(TRACK_RESERVE.max(JUNCTION_NEAR));
        for (index, route) in routes.iter().enumerate() {
            for segment in route.segments(index) {
                let [min, max] = segment.bounds();
                segments.insert(min - radius, max + radius, segment);
            }
        }
        for (index, route) in routes.iter_mut().enumerate() {
            route.junctions = junction_approaches(&segments, route.segments(index));
        }
        Self { routes, segments }
    }

    pub(super) fn track_overlaps(&self, quad: &[Vec2; 4], min: Vec2, max: Vec2) -> bool {
        self.segments
            .nearby(min, max)
            .any(|segment| segment_near(quad, segment.a.xy, segment.b.xy, TRACK_RESERVE))
    }

    pub(super) fn near_junction(&self, route: usize, start: f32, end: f32) -> bool {
        let approaches = &self.routes[route].junctions;
        let first = approaches.partition_point(|&(_, far)| far < start);
        approaches.get(first).is_some_and(|&(near, _)| near <= end)
    }
}

fn junction_approaches(
    index: &SpatialBins<TrackSegment>,
    segments: impl Iterator<Item = TrackSegment>,
) -> Vec<(f32, f32)> {
    let mut result = Vec::<(f32, f32)>::new();
    for segment in segments {
        let [min, max] = segment.bounds();
        if !index.nearby(min, max).any(|other| segment.joins(*other)) {
            continue;
        }
        let from = segment.a.distance - JUNCTION_APPROACH;
        let to = segment.b.distance + JUNCTION_APPROACH;
        if let Some((_, previous_to)) = result.last_mut().filter(|(_, end)| *end >= from) {
            *previous_to = to;
        } else {
            result.push((from, to));
        }
    }
    result
}

pub(super) struct Route {
    points: Vec<Station>,
    pub(super) key: u64,
    junctions: Vec<(f32, f32)>,
}

#[derive(Clone, Copy)]
struct Station {
    xy: Vec2,
    distance: f32,
}

impl Route {
    fn new(run: &CartTrackRun) -> Option<Self> {
        let mut points = Vec::with_capacity(run.points.len());
        let mut distance = 0.;
        for raw in &run.points {
            let xy = Vec2::new(raw[0], raw[1]);
            if !xy.is_finite()
                || xy.min_element() < -0.01
                || xy.max_element() > ISLAND_WORLD_METRES + 0.01
            {
                return None;
            }
            if let Some(previous) = points.last().map(|point: &Station| point.xy) {
                let increment = xy.distance(previous);
                if increment < 0.01 {
                    continue;
                }
                distance += increment;
            }
            points.push(Station { xy, distance });
        }
        let first = points.first()?.xy;
        let key = u64::from(first.x.to_bits()) << 32 | u64::from(first.y.to_bits());
        (points.len() >= 2).then_some(Self {
            points,
            key,
            junctions: Vec::new(),
        })
    }

    pub(super) fn length(&self) -> f32 {
        self.points[self.points.len() - 1].distance
    }

    pub(super) fn point(&self, distance: f32) -> Vec2 {
        let next = self
            .points
            .partition_point(|station| station.distance < distance);
        let next = next.clamp(1, self.points.len() - 1);
        let [a, b] = [self.points[next - 1], self.points[next]];
        let t = ((distance - a.distance) / (b.distance - a.distance)).clamp(0., 1.);
        a.xy.lerp(b.xy, t)
    }

    fn segments(&self, run: usize) -> impl Iterator<Item = TrackSegment> + '_ {
        self.points.windows(2).map(move |pair| TrackSegment {
            a: pair[0],
            b: pair[1],
            run,
        })
    }
}

#[derive(Clone, Copy)]
struct TrackSegment {
    a: Station,
    b: Station,
    run: usize,
}

impl TrackSegment {
    fn bounds(self) -> [Vec2; 2] {
        [self.a.xy.min(self.b.xy), self.a.xy.max(self.b.xy)]
    }

    fn joins(self, other: Self) -> bool {
        if self.run == other.run
            && self.a.distance - other.b.distance <= 12.
            && other.a.distance - self.b.distance <= 12.
        {
            return false;
        }
        segments_within(self.a.xy, self.b.xy, other.a.xy, other.b.xy, JUNCTION_NEAR)
    }
}
