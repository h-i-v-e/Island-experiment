//! Deterministic, cut-only road grading in native XY metres and Z-up heights.

mod grading;
mod mesh;

pub use grading::grade_roads;
pub use mesh::RoadMesh;

use crate::Vec2;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RoadEdge {
    pub start: usize,
    pub end: usize,
    pub half_width: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RoadGradeOptions {
    pub min_height: f32,
    pub max_grade: f32,
    pub max_cut: f32,
    pub shoulder: f32,
    pub sample_spacing: f32,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RoadStation {
    pub point: Vec2,
    pub height: f32,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RoadStrip {
    /// Index in the requested edge list.
    pub source: usize,
    pub start: usize,
    pub end: usize,
    pub half_width: f32,
    /// In order from start to end, beginning and ending at the junction seams.
    pub stations: Vec<RoadStation>,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RoadJunction {
    pub node: usize,
    pub point: Vec2,
    pub height: f32,
    pub radius: f32,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RoadNetwork {
    pub strips: Vec<RoadStrip>,
    pub junctions: Vec<RoadJunction>,
    pub shoulder: f32,
}

impl RoadStrip {
    fn sample(&self, point: Vec2, reach: f32) -> Option<(f32, f32)> {
        let [first, .., last] = self.stations.as_slice() else {
            return None;
        };
        let bounds_expansion = self.half_width + reach;
        if outside_bounds(point, first.point, last.point, bounds_expansion) {
            return None;
        }
        let delta = last.point - first.point;
        let length = delta.length();
        if length <= 0. {
            return None;
        }
        let direction = delta / length;
        let offset = point - first.point;
        let distance_along = offset.dot(direction);
        let along = distance_along.clamp(0., length);
        let across = offset.perp_dot(direction).abs();
        let outside = Vec2::new(
            (across - self.half_width).max(0.),
            (distance_along - along).abs(),
        )
        .length();
        if outside > reach + 0.0001 {
            return None;
        }
        let after = self
            .stations
            .partition_point(|s| (s.point - first.point).dot(direction) < along)
            .clamp(1, self.stations.len() - 1);
        let a = self.stations[after - 1];
        let b = self.stations[after];
        let t = ((along - (a.point - first.point).dot(direction))
            / (b.point - a.point).dot(direction))
        .clamp(0., 1.);
        Some((a.height + (b.height - a.height) * t, outside))
    }

    #[must_use]
    pub fn surface_height(&self, point: Vec2) -> Option<f32> {
        self.sample(point, 0.).map(|(height, _)| height)
    }
}

impl RoadNetwork {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.strips.is_empty()
    }

    /// Keep a subset of solved strips and discard junctions without any roads.
    pub fn retain_strips(&mut self, retain: impl FnMut(&RoadStrip) -> bool) {
        self.strips.retain(retain);
        let used: HashSet<_> = self
            .strips
            .iter()
            .flat_map(|strip| [strip.start, strip.end])
            .collect();
        self.junctions
            .retain(|junction| used.contains(&junction.node));
    }

    /// The paved surface height, excluding earthwork shoulders.
    #[must_use]
    pub fn surface_height(&self, point: Vec2) -> Option<f32> {
        self.strips
            .iter()
            .filter_map(|strip| strip.surface_height(point))
            .reduce(f32::min)
            .or_else(|| {
                self.junctions
                    .iter()
                    .filter(|junction| {
                        junction.point.distance_squared(point) <= junction.radius * junction.radius
                    })
                    .map(|junction| junction.height)
                    .reduce(f32::min)
            })
    }

    /// Cut the core to the road height and feather the shoulders into natural ground.
    #[must_use]
    pub fn graded_height(&self, point: Vec2, natural_height: f32) -> f32 {
        self.terrain_support_height(point, natural_height, 0.)
    }

    /// Conform finer terrain to the road without raising it beyond the given
    /// allowance for differences between the planning grid and the native mesh.
    #[must_use]
    pub fn terrain_support_height(
        &self,
        point: Vec2,
        natural_height: f32,
        max_subgrid_raise: f32,
    ) -> f32 {
        let maximum = natural_height + max_subgrid_raise.max(0.);
        let mut core = f32::INFINITY;
        let mut blend = ShoulderBlend::default();
        for strip in &self.strips {
            if let Some((target, distance)) = strip.sample(point, self.shoulder) {
                if distance <= 0.0001 {
                    core = core.min(target);
                } else {
                    blend.add(
                        target.min(maximum) - natural_height,
                        distance,
                        self.shoulder,
                    );
                }
            }
        }
        if core.is_finite() {
            return maximum.min(core);
        }
        for junction in &self.junctions {
            let distance = (junction.point.distance(point) - junction.radius).max(0.);
            if distance <= 0.0001 {
                core = core.min(junction.height);
            } else {
                blend.add(
                    junction.height.min(maximum) - natural_height,
                    distance,
                    self.shoulder,
                );
            }
        }
        if core.is_finite() {
            maximum.min(core)
        } else {
            natural_height + blend.delta()
        }
    }

    /// Axis-aligned bounds including the earthwork shoulders.
    #[must_use]
    pub fn bounds(&self) -> Option<(Vec2, Vec2)> {
        self.strips
            .iter()
            .filter_map(|strip| {
                let a = strip.stations.first()?.point;
                let b = strip.stations.last()?.point;
                let margin = Vec2::splat(strip.half_width + self.shoulder);
                Some((a.min(b) - margin, a.max(b) + margin))
            })
            .chain(self.junctions.iter().map(|junction| {
                let margin = Vec2::splat(junction.radius + self.shoulder);
                (junction.point - margin, junction.point + margin)
            }))
            .reduce(|(min, max), (a, b)| (min.min(a), max.max(b)))
    }

    /// Check bounded geometry at input boundaries before meshing or terrain edits.
    ///
    /// # Errors
    /// Returns an error for malformed, nonfinite or excessively large profiles.
    pub fn validate(&self) -> Result<(), String> {
        if !self.shoulder.is_finite()
            || !(0. ..=50.).contains(&self.shoulder)
            || self.strips.len() > 4096
            || self.junctions.len() > 8192
            || self
                .strips
                .iter()
                .map(|strip| strip.stations.len())
                .sum::<usize>()
                > 65_536
        {
            return Err("road network exceeds geometry or shoulder limits".into());
        }
        let mut junctions = HashMap::with_capacity(self.junctions.len());
        for junction in &self.junctions {
            if !junction.point.is_finite()
                || !junction.height.is_finite()
                || !junction.radius.is_finite()
                || !(0. ..=100.).contains(&junction.radius)
                || junctions.insert(junction.node, junction).is_some()
            {
                return Err("invalid road junction".into());
            }
        }
        let mut endpoints = HashMap::<usize, RoadStation>::new();
        for strip in &self.strips {
            if !valid_strip(strip) {
                return Err("invalid road strip".into());
            }
            let [first, .., last] = strip.stations.as_slice() else {
                return Err("road strip requires both endpoints".into());
            };
            let direction = (last.point - first.point).normalize();
            for (node, station, outward) in [
                (strip.start, first, direction),
                (strip.end, last, -direction),
            ] {
                let expected = if let Some(junction) = junctions.get(&node) {
                    if junction.radius < strip.half_width {
                        return Err("road strip exceeds its junction width".into());
                    }
                    RoadStation {
                        point: junction.point
                            + outward * grading::seam_offset(junction.radius, strip.half_width),
                        height: junction.height,
                    }
                } else {
                    *endpoints.entry(node).or_insert(*station)
                };
                if station.point.distance(expected.point) > 0.003
                    || (station.height - expected.height).abs() > 0.0001
                {
                    return Err("road strip is disconnected from its junction".into());
                }
            }
        }
        Ok(())
    }
}

fn valid_strip(strip: &RoadStrip) -> bool {
    let [first, .., last] = strip.stations.as_slice() else {
        return false;
    };
    let delta = last.point - first.point;
    let direction = delta.normalize_or_zero();
    strip.start != strip.end
        && strip.half_width.is_finite()
        && (0.05..=50.).contains(&strip.half_width)
        && delta.is_finite()
        && delta.length() >= 0.01
        && strip.stations.iter().all(|station| {
            station.point.is_finite()
                && station.height.is_finite()
                && (station.point - first.point).perp_dot(direction).abs() <= 0.002
        })
        && strip
            .stations
            .windows(2)
            .all(|span| (span[1].point - span[0].point).dot(direction) > 0.)
}

fn outside_bounds(point: Vec2, a: Vec2, b: Vec2, margin: f32) -> bool {
    let margin = Vec2::splat(margin);
    point.cmplt(a.min(b) - margin).any() || point.cmpgt(a.max(b) + margin).any()
}

#[derive(Default)]
struct ShoulderBlend {
    total_influence: f32,
    weighted_delta: f32,
}

impl ShoulderBlend {
    fn add(&mut self, delta: f32, distance: f32, shoulder: f32) {
        let falloff = shoulder_weight(distance, shoulder);
        let influence = falloff / (distance * distance);
        self.total_influence += influence;
        self.weighted_delta += delta * falloff * influence;
    }

    fn delta(&self) -> f32 {
        if self.total_influence > 0. {
            // Near an edge, the adjacent shoulder dominates the weighted
            // average. Farther overlapping shoulders taper out independently.
            self.weighted_delta / self.total_influence
        } else {
            0.
        }
    }
}

fn shoulder_weight(distance: f32, shoulder: f32) -> f32 {
    if distance <= 0. {
        return 1.;
    }
    if distance >= shoulder {
        return 0.;
    }
    let t = distance / shoulder;
    (1. - t) * (1. - t) * (1. + 2. * t)
}

#[cfg(test)]
mod tests;
