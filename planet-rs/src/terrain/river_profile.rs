//! Spherical port of Island's `form_river_profile` / `form_stepped_profile`
//! (`island-rs/src/rivers/carving.rs`) and local `WaterfallPatch` face planes.
use super::{Drainage, RiverSegment, TerrainOptions, graph::arc};
use crate::{DVec3, PlanetMesh};
use std::collections::BTreeMap;

struct Junction {
    point: DVec3,
    half_width: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Waterfall {
    pub upper_vertex: usize,
    pub lower_vertex: usize,
    pub upstream_vertex: usize,
    pub lip: DVec3,
    /// Flow tangent; radial down is always towards the sphere centre.
    pub direction: DVec3,
    pub half_width: f64,
    pub upper_surface: f64,
    pub lower_surface: f64,
    pub face_run: f64,
}

/// Island's terrain lift weight and complementary upstream smoothing weight.
/// These shape the support; final water binding uses inclination everywhere.
#[derive(Clone, Copy)]
pub(super) struct WaterfallBlend {
    pub normal: f64,
    pub smoothing: f64,
}

impl Default for WaterfallBlend {
    fn default() -> Self {
        Self {
            normal: 1.0,
            smoothing: 0.0,
        }
    }
}

impl Waterfall {
    /// Use one straight centreline and width across the drop. The face may
    /// slope downstream; its crest and foot must share the same lateral span.
    /// Fade into the natural channel on the existing repair/landing shoulders.
    pub(super) fn corridor_at(self, point: DVec3, distance: f64, width: f64) -> (f64, f64) {
        let along = (point - self.lip).dot(self.direction);
        let lateral = point
            .dot(self.lip.cross(self.direction))
            .abs()
            .clamp(0.0, 1.0)
            .asin();
        let weight = if along < -self.face_run {
            ((along + self.face_run * 2.0) / self.face_run).clamp(0.0, 1.0)
        } else if along > 0.0 {
            (1.0 - along / (self.half_width * 1.5 + 0.00075)).clamp(0.0, 1.0)
        } else {
            1.0
        };
        let weight = weight * weight * (3.0 - 2.0 * weight);
        let apron = (lateral / self.half_width - 1.5).clamp(0.0, 1.0);
        let weight = weight * (1.0 - apron * apron * (3.0 - 2.0 * apron));
        (
            distance + (lateral - distance) * weight,
            width + (self.half_width - width) * weight,
        )
    }

    pub(super) fn on_face(self, point: DVec3) -> bool {
        let along = (point - self.lip).dot(self.direction);
        let lateral = (point - self.lip).dot(self.lip.cross(self.direction)).abs();
        (-self.face_run..=0.0).contains(&along) && lateral <= self.half_width * 2.5
    }
    pub(super) fn blend(self, point: DVec3) -> WaterfallBlend {
        let along = (point - self.lip).dot(self.direction);
        if along < -self.face_run {
            let normal = ((-self.face_run - along) / self.face_run).clamp(0.0, 1.0);
            WaterfallBlend {
                normal,
                smoothing: 1.0 - normal,
            }
        } else if along <= 0.0 {
            WaterfallBlend {
                normal: 0.0,
                smoothing: 0.0,
            }
        } else {
            // The terrain lift mask ends here. Final water has no such switch:
            // its inclination blend continues over both sides of the foot.
            WaterfallBlend::default()
        }
    }

    pub(super) fn surface_at(self, point: DVec3) -> Option<f64> {
        if !self.footprint_contains(point, 0.0) {
            return None;
        }
        let along = (point - self.lip).dot(self.direction);
        let progress = ((along + self.face_run) / self.face_run).clamp(0.0, 1.0);
        let smooth = progress * progress * (3.0 - 2.0 * progress);
        Some(self.upper_surface + (self.lower_surface - self.upper_surface) * smooth)
    }

    /// Shared extent for placement and shaping, expanded by a junction's banks.
    fn footprint_contains(self, point: DVec3, margin: f64) -> bool {
        let along = (point - self.lip).dot(self.direction);
        let lateral = (point - self.lip).dot(self.lip.cross(self.direction)).abs();
        // Opposite hemispheres can have identical tangent-plane projections.
        point.dot(self.lip) > 0.0
            && along >= -self.face_run * 2.0 - margin
            && along <= self.half_width * 1.5 + 0.00075 + margin
            && lateral <= self.half_width * 2.5 + margin
    }
}

pub(super) struct Profile {
    pub levels: Vec<f64>,
    pub waterfalls: Vec<Waterfall>,
}

pub(super) fn build(
    mesh: &PlanetMesh,
    heights: &[f64],
    drainage: &Drainage,
    rivers: &[RiverSegment],
    settings: TerrainOptions,
) -> Profile {
    let mut active = vec![false; heights.len()];
    let mut main_upstream = vec![None; heights.len()];
    let mut incoming = BTreeMap::<usize, usize>::new();
    for river in rivers {
        active[river.from] = true;
        *incoming.entry(river.to).or_default() += 1;
        let upstream = &mut main_upstream[river.to];
        if upstream.is_none_or(|old| drainage.catchment[river.from] > drainage.catchment[old]) {
            *upstream = Some(river.from);
        }
    }
    let threshold = settings.river_catchment * 4.0 * std::f64::consts::PI;
    let depth = |i: usize| {
        settings.river_depth
            * (drainage.catchment[i] / threshold)
                .sqrt()
                .clamp(1.0, 3.0)
                .sqrt()
    };
    let width = |i: usize| {
        settings.river_width * (drainage.catchment[i] / threshold).sqrt().clamp(1.0, 3.0)
    };
    let junctions: Vec<_> = incoming
        .into_iter()
        .filter(|&(_, count)| count > 1)
        .map(|(vertex, _)| Junction {
            point: mesh.directions[vertex],
            half_width: width(vertex),
        })
        .collect();
    let mut levels: Vec<_> = heights
        .iter()
        .enumerate()
        .map(|(i, h)| (h - depth(i) * 0.35).max(0.0))
        .collect();
    let mut assigned = vec![false; heights.len()];
    let mut path = Vec::new();
    let mut natural = Vec::new();
    let mut gradients = Vec::new();
    let mut waterfalls = Vec::new();
    // Parent-before-child order gives main rivers authority at tributary joins.
    // Follow the strongest incoming branch, then process its remaining feeders.
    for &start in &drainage.order {
        if !active[start] || assigned[start] {
            continue;
        }
        path.clear();
        if let Some(to) = drainage.receivers[start] {
            path.push(to);
        }
        let mut current = Some(start);
        while let Some(i) = current {
            path.push(i);
            assigned[i] = true;
            current = main_upstream[i].filter(|&j| !assigned[j]);
        }
        path.reverse();
        natural.clear();
        natural.extend(path.iter().map(|&i| levels[i]));
        // Island's downstream-to-source monotone profile, frozen before carving.
        for i in (0..natural.len().saturating_sub(1)).rev() {
            natural[i] = natural[i].max(natural[i + 1]);
        }
        stepped(
            mesh,
            &path,
            &natural,
            &width,
            &mut levels,
            &mut gradients,
            &mut waterfalls,
            &junctions,
        );
    }
    Profile { levels, waterfalls }
}

#[allow(clippy::too_many_arguments)] // Shared borrowed network and reusable profile buffers.
fn stepped(
    mesh: &PlanetMesh,
    path: &[usize],
    natural: &[f64],
    width: &impl Fn(usize) -> f64,
    levels: &mut [f64],
    gradients: &mut Vec<f64>,
    waterfalls: &mut Vec<Waterfall>,
    junctions: &[Junction],
) {
    let end = path.len().saturating_sub(1);
    if end == 0 {
        return;
    }
    gradients.clear();
    // Same five-segment weighted gradient and terrace spacing as Island.
    for segment in 0..end {
        let start = segment.saturating_sub(2);
        let finish = (segment + 2).min(end - 1);
        let mut total = 0.0;
        let mut weights = 0.0;
        for neighbour in start..=finish {
            let distance = arc(
                mesh.directions[path[neighbour]],
                mesh.directions[path[neighbour + 1]],
            )
            .max(1.0e-12);
            let weight = 3.0 - f64::from(u32::try_from(segment.abs_diff(neighbour)).unwrap());
            total += (natural[neighbour] - natural[neighbour + 1]).max(0.0) / distance * weight;
            weights += weight;
        }
        gradients.push(total / weights);
    }
    let reference_height = 0.2; // Island's 200 m reference at the default 1 km radius.
    let mut level = levels[path[end]];
    let mut reach_length = 0.0;
    let mut half_width: f64 = 0.0;
    for index in (0..end).rev() {
        let from = path[index];
        let to = path[index + 1];
        let length = arc(mesh.directions[from], mesh.directions[to]);
        reach_length += length;
        half_width = half_width.max(width(from));
        let response = (gradients[index] / (reference_height * 2.25))
            .clamp(0.0, 1.0)
            .sqrt();
        let target_fall = reference_height * (0.0075 + (0.018 - 0.0075) * response);
        let required_length =
            (reference_height * 0.006 * (1.0 - response * 0.9)).max(0.00075 + half_width * 2.5);
        let natural_surface = natural[index].max(level);
        if index > 0 && natural_surface - level >= target_fall && reach_length >= required_length {
            let lip = mesh.directions[from];
            // The top belongs to the incoming reach. Using the outgoing
            // tangent at a bend can put it beside the actual feeding channel.
            let upstream = mesh.directions[path[index - 1]];
            let direction = (lip * lip.dot(upstream) - upstream).normalize();
            let fall = Waterfall {
                upper_vertex: from,
                lower_vertex: to,
                upstream_vertex: path[index - 1],
                lip,
                direction,
                half_width: width(from),
                upper_surface: level + target_fall,
                lower_surface: level,
                face_run: 0.0025,
            };
            // Defer the same terrace step farther upstream until the whole
            // support/landing footprint clears every merge's dry bank apron.
            // The lower run-in keeps its downstream profile while we search.
            if !junctions
                .iter()
                .any(|junction| fall.footprint_contains(junction.point, junction.half_width * 2.5))
            {
                waterfalls.push(fall);
                level += target_fall;
                reach_length = 0.0;
                half_width = 0.0;
            }
        }
        levels[from] = level.min(natural_surface);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn junction_moves_the_waterfall_upstream_and_keeps_its_landing_clear() {
        for pole in [1.0, -1.0] {
            let mut mesh = PlanetMesh {
                directions: (0..=40)
                    .map(|i| {
                        let angle = f64::from(i) * 0.002;
                        DVec3::new(angle.sin(), 0.0, angle.cos() * pole)
                    })
                    .collect(),
                triangles: Vec::new(),
                radius: 1000.0,
            };
            let junction = mesh.directions[40];
            mesh.directions.extend(
                (1..=4).map(|i| (junction + DVec3::Y * (f64::from(i) * 0.002)).normalize()),
            );
            let settings = TerrainOptions::default();
            let threshold = settings.river_catchment * 4.0 * std::f64::consts::PI;
            let mut rivers: Vec<_> = (0..40)
                .map(|from| RiverSegment {
                    from,
                    to: from + 1,
                    catchment: threshold * 2.0,
                })
                .collect();
            rivers.extend((41..=44).map(|from| RiverSegment {
                from,
                to: if from == 41 { 40 } else { from - 1 },
                catchment: threshold,
            }));
            let mut heights: Vec<_> = (0..=40)
                .map(|i| 0.01 + 0.05 * (1.0 - f64::from(i) / 40.0))
                .collect();
            heights.extend((1..=4).map(|i| 0.01 + f64::from(i) * 0.0025));
            let mut drainage = Drainage {
                receivers: vec![None; 45],
                catchment: vec![threshold * 2.0; 45],
                order: (0..=40).rev().chain(41..=44).collect(),
            };
            drainage.catchment[40] = threshold * 3.0;
            for river in &rivers {
                drainage.receivers[river.from] = Some(river.to);
                drainage.catchment[river.from] = river.catchment;
            }
            let original = build(&mesh, &heights, &drainage, &rivers[..40], settings);
            let moved = build(&mesh, &heights, &drainage, &rivers, settings);
            let first = moved
                .waterfalls
                .first()
                .expect("a long branch still has room for a fall");
            assert!(
                first.upper_vertex < original.waterfalls[0].upper_vertex,
                "defer the drop upstream instead of placing it beside the junction"
            );
            let junction_apron = settings.river_width * 3.0_f64.sqrt() * 2.5;
            for fall in &moved.waterfalls {
                let along = (mesh.directions[40] - fall.lip).dot(fall.direction);
                assert!(
                    along > fall.half_width * 1.5 + 0.00075 + junction_apron,
                    "leave room for the waterfall landing and the receiving river's bank apron"
                );
            }
            for i in first.upper_vertex + 1..=40 {
                assert!(
                    (moved.levels[i] - moved.levels[40]).abs() < 1.0e-12,
                    "keep a low run-in to the merge, rather than dragging the face downstream"
                );
            }
            assert!(
                rivers
                    .iter()
                    .all(|r| moved.levels[r.from] >= moved.levels[r.to])
            );
        }
    }

    #[test]
    fn upstream_terrain_blend_reaches_the_lip_and_ends_after_the_foot() {
        let fall = Waterfall {
            upper_vertex: 0,
            lower_vertex: 1,
            upstream_vertex: 2,
            lip: DVec3::Z,
            direction: DVec3::X,
            half_width: 0.003,
            upper_surface: 0.04,
            lower_surface: 0.036,
            face_run: 0.0025,
        };
        let point = |x: f64| DVec3::new(x, 0.0, (1.0 - x * x).sqrt());
        for (x, normal, smoothing) in [
            (-0.006, 1.0, 0.0),
            (-0.005, 1.0, 0.0),
            (-0.00375, 0.5, 0.5),
            (-0.0025, 0.0, 0.0),
            (-0.001, 0.0, 0.0),
            (0.0, 0.0, 0.0),
            (0.00001, 1.0, 0.0),
        ] {
            let blend = fall.blend(point(x));
            assert!((blend.normal - normal).abs() < 1.0e-12);
            assert!((blend.smoothing - smoothing).abs() < 1.0e-12);
        }
        let before_lip = fall.blend(point(-0.002_500_001));
        assert!(before_lip.normal < 1.0e-6 && before_lip.smoothing > 0.999);
    }

    #[test]
    fn placement_follows_the_incoming_channel_at_bends_without_shortening_the_drop_run() {
        for south in [false, true] {
            let mesh = PlanetMesh {
                directions: (0..41)
                    .map(|i| {
                        let i = f64::from(i);
                        let p = DVec3::new(i * 0.002, 0.003 * (i * 0.8).sin(), 1.0).normalize();
                        if south {
                            DVec3::new(p.x, -p.y, -p.z)
                        } else {
                            p
                        }
                    })
                    .collect(),
                triangles: Vec::new(),
                radius: 1000.0,
            };
            let path: Vec<_> = (0..41).collect();
            let natural: Vec<_> = (0..41)
                .map(|i| 0.05 * (1.0 - f64::from(i) / 40.0))
                .collect();
            let mut falls = Vec::new();
            stepped(
                &mesh,
                &path,
                &natural,
                &|_| 0.003,
                &mut vec![0.0; 41],
                &mut Vec::new(),
                &mut falls,
                &[],
            );
            assert!(falls.len() >= 4);
            for fall in falls {
                let across = fall.lip.cross(fall.direction);
                assert!(
                    mesh.directions[fall.upstream_vertex].dot(across).abs() < 1.0e-12,
                    "the crest must line up with the actual incoming reach"
                );
                assert!(
                    (fall.face_run - 0.0025).abs() < 1.0e-12,
                    "lateral alignment must not force a steeper waterfall"
                );
            }
        }
    }

    #[test]
    fn island_terraces_have_supported_spaced_falls_with_flat_cross_faces() {
        let mesh = PlanetMesh {
            directions: (0..41)
                .map(|i| {
                    let angle = f64::from(i) * 0.002;
                    DVec3::new(angle.sin(), 0.0, angle.cos())
                })
                .collect(),
            triangles: Vec::new(),
            radius: 1000.0,
        };
        let path: Vec<_> = (0..41).collect();
        let natural: Vec<_> = (0..41)
            .map(|i| 0.05 * (1.0 - f64::from(i) / 40.0))
            .collect();
        let mut levels = natural.clone();
        let mut falls = Vec::new();
        stepped(
            &mesh,
            &path,
            &natural,
            &|_| 0.003,
            &mut levels,
            &mut Vec::new(),
            &mut falls,
            &[],
        );
        assert!(falls.len() >= 4);
        for (i, &level) in levels.iter().enumerate() {
            assert!(level <= natural[i] + 1.0e-12);
            if i > 0 {
                assert!(level <= levels[i - 1]);
            }
        }
        for &fall in &falls {
            assert!(
                fall.upper_vertex > 0,
                "source cannot feed an unsupported curtain"
            );
            assert!(
                (0.0015 - 1.0e-12..=0.0036 + 1.0e-12)
                    .contains(&(fall.upper_surface - fall.lower_surface))
            );
            let upstream = (fall.lip - fall.direction * fall.face_run).normalize();
            assert!((fall.surface_at(upstream).unwrap() - fall.upper_surface).abs() < 1.0e-8);
            assert!((fall.surface_at(fall.lip).unwrap() - fall.lower_surface).abs() < 1.0e-12);
            let across = fall.lip.cross(fall.direction);
            let left =
                (fall.lip - fall.direction * fall.face_run * 0.5 + across * 0.002).normalize();
            let right =
                (fall.lip - fall.direction * fall.face_run * 0.5 - across * 0.002).normalize();
            assert!(
                (fall.surface_at(left).unwrap() - fall.surface_at(right).unwrap()).abs() < 1.0e-12
            );
        }
        for pair in falls.windows(2) {
            assert!(arc(pair[0].lip, pair[1].lip) >= 0.008);
        }
        // A remote merge on the opposite hemisphere shares the same tangent
        // projection, but must not suppress this river's ordinary waterfalls.
        let mut remote_levels = natural.clone();
        let mut remote_falls = Vec::new();
        stepped(
            &mesh,
            &path,
            &natural,
            &|_| 0.003,
            &mut remote_levels,
            &mut Vec::new(),
            &mut remote_falls,
            &[Junction {
                point: -mesh.directions[20],
                half_width: 0.009,
            }],
        );
        assert_eq!(levels, remote_levels);
        assert_eq!(falls, remote_falls);
    }
}
