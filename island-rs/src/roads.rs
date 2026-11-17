//! Deterministic, cut-only road grading in native XY metres and Z-up heights.

mod grading;
mod mesh;

pub use grading::grade_roads;
pub use mesh::RoadMesh;

use crate::Vec2;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

pub const MAX_ROAD_EDGES: usize = 4096;

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
    /// Radius of a circular junction or the largest rectangular half-extent.
    pub radius: f32,
    /// Unit vector along one edge. The serialized name accepts older square profiles.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub square_axis: Option<Vec2>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rectangle_half_extents: Option<Vec2>,
}

impl RoadJunction {
    #[must_use]
    pub fn rectangle(&self) -> Option<(Vec2, Vec2)> {
        self.square_axis.map(|axis| {
            (
                axis,
                self.rectangle_half_extents
                    .unwrap_or(Vec2::splat(self.radius)),
            )
        })
    }

    #[must_use]
    pub fn distance_to_core(&self, point: Vec2) -> f32 {
        let offset = point - self.point;
        if let Some((axis, half)) = self.rectangle() {
            let local = junction_axes(axis, offset);
            return (local.abs() - half).max(Vec2::ZERO).length();
        }
        (offset.length() - self.radius).max(0.)
    }

    /// Nearest forward intersection with the paved boundary, a little inset if requested.
    #[must_use]
    pub fn ray_entry(&self, from: Vec2, direction: Vec2, inset: f32) -> Option<Vec2> {
        let radius = self.radius - inset;
        if radius <= 0. {
            return None;
        }
        let offset = from - self.point;
        let distance = if let Some((axis, half)) = self.rectangle() {
            rectangle_ray_entry(
                junction_axes(axis, offset),
                junction_axes(axis, direction),
                half - Vec2::splat(inset),
            )?
        } else {
            let chord_squared = radius.powi(2) - offset.perp_dot(direction).powi(2);
            -offset.dot(direction) - chord_squared.sqrt()
        };
        (distance > 0.).then(|| from + direction * distance)
    }

    fn seam_offset(&self, direction: Vec2, half_width: f32) -> f32 {
        if let Some(size) = self.seam_half_extents(direction) {
            size.x
        } else {
            (self.radius * self.radius - half_width * half_width)
                .max(0.)
                .sqrt()
        }
    }

    // The first extent runs along the outgoing strip, the second spans its mouth.
    fn seam_half_extents(&self, direction: Vec2) -> Option<Vec2> {
        self.rectangle().map(|(axis, half)| {
            if axis.dot(direction).abs() > 0.5 {
                half
            } else {
                Vec2::new(half.y, half.x)
            }
        })
    }
}

fn junction_axes(axis: Vec2, point: Vec2) -> Vec2 {
    Vec2::new(axis.dot(point), axis.perp_dot(point))
}

fn rectangle_ray_entry(origin: Vec2, direction: Vec2, half: Vec2) -> Option<f32> {
    if half.min_element() <= 0. {
        return None;
    }
    let mut enter = 0_f32;
    let mut leave = f32::INFINITY;
    for ((from, delta), radius) in origin
        .to_array()
        .into_iter()
        .zip(direction.to_array())
        .zip(half.to_array())
    {
        if delta.abs() < 0.00001 {
            if from.abs() > radius {
                return None;
            }
        } else {
            let a = (-radius - from) / delta;
            let b = (radius - from) / delta;
            enter = enter.max(a.min(b));
            leave = leave.min(a.max(b));
        }
    }
    (enter <= leave).then_some(enter)
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
                    .filter(|junction| junction.distance_to_core(point) <= 0.0001)
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
            let distance = junction.distance_to_core(point);
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
                let extent = junction
                    .rectangle()
                    .map_or(Vec2::splat(junction.radius), |(axis, half)| {
                        axis.abs() * half.x + axis.perp().abs() * half.y
                    });
                let margin = extent + Vec2::splat(self.shoulder);
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
            || self.strips.len() > MAX_ROAD_EDGES
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
                || junction.square_axis.is_some_and(|axis| {
                    !axis.is_finite() || (axis.length_squared() - 1.).abs() > 0.0001
                })
                || junction.rectangle_half_extents.is_some_and(|half| {
                    junction.square_axis.is_none()
                        || !half.is_finite()
                        || half.min_element() <= 0.
                        || half.max_element() > junction.radius
                })
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
                    let half_width = junction
                        .seam_half_extents(outward)
                        .map_or(junction.radius, |size| size.y);
                    if half_width < strip.half_width {
                        return Err("road strip exceeds its junction width".into());
                    }
                    if junction
                        .square_axis
                        .is_some_and(|axis| !axis_aligned(axis, outward))
                    {
                        return Err("road strip is angled across a rectangular junction".into());
                    }
                    RoadStation {
                        point: junction.point
                            + outward * junction.seam_offset(outward, strip.half_width),
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

fn axis_aligned(axis: Vec2, direction: Vec2) -> bool {
    axis.dot(direction)
        .abs()
        .min(axis.perp_dot(direction).abs())
        < 0.001
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
