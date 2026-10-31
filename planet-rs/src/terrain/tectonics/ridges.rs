//! A descending, recursive spur network attached to compressional crests.
//! The hierarchy is generated once; erosion sees the same continuous source
//! when subsequent terrain stages resolve its narrower branches.
use super::{Boundary, TectonicOptions, belt_profile};
use crate::{
    DVec3,
    terrain::{feature_index::FeatureIndex, random::Random},
};

pub(super) struct RidgeField {
    segments: Vec<Segment>,
    index: FeatureIndex,
}
#[derive(Clone, Copy, Debug, PartialEq)]
struct Segment {
    start: DVec3,
    tangent: DVec3,
    normal: DVec3,
    length: f64,
    heights: [f64; 2],
    widths: [f64; 2],
    centre: DVec3,
    radius: f64,
    cap_cos: f64,
}
impl Segment {
    fn new(a: DVec3, b: DVec3, heights: [f64; 2], widths: [f64; 2]) -> Self {
        let cross = a.cross(b);
        let length = cross.length().atan2(a.dot(b));
        let normal = cross.normalize();
        let centre = (a + b).normalize();
        let radius = length * 0.5 + widths[0].max(widths[1]) * 4.0;
        Self {
            start: a,
            tangent: normal.cross(a),
            normal,
            length,
            heights,
            widths,
            centre,
            radius,
            cap_cos: radius.cos(),
        }
    }
    fn point(self, t: f64) -> DVec3 {
        self.start * (self.length * t).cos() + self.tangent * (self.length * t).sin()
    }
    fn sample(self, p: DVec3) -> f64 {
        if self.centre.dot(p) < self.cap_cos {
            return 0.0;
        }
        let along = p.dot(self.tangent).atan2(p.dot(self.start));
        let t = (along / self.length).clamp(0.0, 1.0);
        let end = along - t * self.length;
        let side = p.dot(self.normal).clamp(-1.0, 1.0).asin();
        let width = self.widths[0] + (self.widths[1] - self.widths[0]) * t;
        let height = self.heights[0] + (self.heights[1] - self.heights[0]) * t;
        height * belt_profile(side.hypot(end) / width)
    }
}
/// Forward and outward directions in the tangent plane at the attachment.
/// Incoming/outgoing tangents share that plane, removing radial curvature.
struct ConvexFrame {
    forward: DVec3,
    outward: DVec3,
}
fn convex_frame(before: DVec3, point: DVec3, after: DVec3) -> Option<ConvexFrame> {
    let incoming = (point * before.dot(point) - before).normalize_or_zero();
    let outgoing = (after - point * after.dot(point)).normalize_or_zero();
    let turn = point.dot(incoming.cross(outgoing));
    if incoming.length_squared() < 0.5 || outgoing.length_squared() < 0.5 || turn.abs() < 1.0e-6 {
        return None;
    }
    let forward = (incoming + outgoing).normalize_or_zero();
    (forward.length_squared() > 0.5).then(|| ConvexFrame {
        forward,
        outward: point.cross(forward) * -turn.signum(),
    })
}
fn crest_point(edge: &Boundary, width: f64, options: TectonicOptions, t: f64) -> DVec3 {
    let offset = edge.uplift_offset + width * options.ruggedness * edge.shape(t).bend;
    (edge.point(edge.length * t) * offset.cos() + edge.normal * offset.sin()).normalize()
}
fn root_frame(
    edge: &Boundary,
    width: f64,
    options: TectonicOptions,
    t: f64,
) -> Option<ConvexFrame> {
    let half_interval =
        0.5 / f64::from(u32::try_from(edge.peaks.len() - 1).expect("bounded crest knots"));
    let step = (width * 0.5 / edge.length)
        .min(half_interval)
        .min(t)
        .min(1.0 - t);
    convex_frame(
        crest_point(edge, width, options, t - step),
        crest_point(edge, width, options, t),
        crest_point(edge, width, options, t + step),
    )
}
fn path_point(path: &[Segment], progress: f64) -> DVec3 {
    let t = progress.clamp(0.0, 1.0) * 3.0;
    if t <= 1.0 {
        path[0].point(t)
    } else if t <= 2.0 {
        path[1].point(t - 1.0)
    } else {
        path[2].point(t - 2.0)
    }
}
fn child_frame(path: &[Segment], progress: f64) -> Option<ConvexFrame> {
    convex_frame(
        path_point(path, progress - 1.0 / 3.0),
        path_point(path, progress),
        path_point(path, progress + 1.0 / 3.0),
    )
}

#[derive(Clone, Copy)]
struct Branch {
    start: DVec3,
    direction: DVec3,
    length: f64,
    width: f64,
    height: f64,
    height_taper: f64,
    remaining: u32,
}
impl RidgeField {
    pub fn new(boundaries: &[Boundary], width: f64, seed: u64, options: TectonicOptions) -> Self {
        let mut segments = Vec::new();
        if options.branching > 0.0 && options.branch_depth > 0 {
            for (id, edge) in boundaries.iter().enumerate() {
                seed_roots(
                    &mut segments,
                    edge,
                    width,
                    seed ^ u64::try_from(id).expect("boundary index"),
                    options,
                );
            }
        }
        let index = FeatureIndex::new(segments.iter().map(|s| (s.centre, s.radius)));
        Self { segments, index }
    }
    pub fn sample(&self, p: DVec3) -> f64 {
        self.index
            .at(p)
            .iter()
            .map(|&i| self.segments[i].sample(p))
            .fold(0.0, f64::max)
    }
    pub fn features(&self) -> impl Iterator<Item = ([DVec3; 3], f64)> + '_ {
        self.segments
            .iter()
            .map(|s| ([s.start, s.centre, s.point(1.0)], s.radius))
    }
}
/// Every interior extremum of the lateral wobble is a mandatory bend apex.
/// The fixed knot IDs keep coarse roots stable when recursion depth changes.
fn seed_roots(
    segments: &mut Vec<Segment>,
    edge: &Boundary,
    width: f64,
    seed: u64,
    options: TectonicOptions,
) {
    if options.ruggedness == 0.0 {
        return;
    }
    let intervals = f64::from(u32::try_from(edge.peaks.len() - 1).expect("bounded crest knots"));
    for (index, knots) in edge.peaks.windows(3).enumerate() {
        if (knots[1].bend - knots[0].bend) * (knots[2].bend - knots[1].bend) >= 0.0 {
            continue;
        }
        let knot = u32::try_from(index + 1).expect("bounded apex index");
        let t = f64::from(knot) / intervals;
        let Some(frame) = root_frame(edge, width, options, t) else {
            continue;
        };
        let strain = edge.force(t);
        let crest = (strain.compression * 5.0 * options.deformation).tanh()
            * 0.85
            * (1.0 - options.ruggedness + options.ruggedness * edge.shape(t).height);
        if crest <= 1.0e-12 {
            continue;
        }
        let mut random =
            Random::new(seed ^ 0x6f49_b7d1_c382_a05e ^ u64::from(knot).wrapping_mul(0x9e37_79b9));
        let slant = (random.unit() - 0.5) * 0.8;
        let branch = Branch {
            start: crest_point(edge, width, options, t),
            direction: (frame.outward + frame.forward * slant).normalize(),
            length: width * (4.0 + random.unit() * 2.5),
            width: width * (0.42 + random.unit() * 0.12),
            height: crest,
            // Strength changes the descending profile, never the seam height.
            height_taper: 1.15 / options.branching.max(0.001),
            remaining: options.branch_depth,
        };
        grow(segments, branch, &mut Random::new(random.bits()));
    }
}

fn grow(segments: &mut Vec<Segment>, branch: Branch, random: &mut Random) {
    let first = segments.len();
    let mut point = branch.start;
    let mut direction = branch.direction;
    for step in 0..3 {
        let turn = (random.unit() - 0.5) * 0.5;
        direction = (direction * turn.cos() + point.cross(direction) * turn.sin()).normalize();
        let angle = branch.length / 3.0;
        let end = (point * angle.cos() + direction * angle.sin()).normalize();
        let a = f64::from(step) / 3.0;
        let b = f64::from(step + 1) / 3.0;
        let height = |t: f64| branch.height * (1.0 - t).powf(branch.height_taper);
        let width = |t: f64| branch.width * (1.0 - t * 0.55);
        segments.push(Segment::new(
            point,
            end,
            [height(a), height(b)],
            [width(a), width(b)],
        ));
        direction = (-point * angle.sin() + direction * angle.cos()).normalize();
        point = end;
    }
    if branch.remaining <= 1 {
        return;
    }
    // Both internal joints are actual bend apices of the parent's polyline.
    // Match the shared apex height exactly before tapering towards lowland.
    for apex in [1, 2] {
        let parent = segments[first + apex - 1];
        let progress = if apex == 1 { 1.0 / 3.0 } else { 2.0 / 3.0 };
        let Some(frame) = child_frame(&segments[first..first + 3], progress) else {
            continue;
        };
        if parent.heights[1] <= 1.0e-12 {
            continue;
        }
        let angle = 0.55 + random.unit() * 0.55;
        let child = Branch {
            start: parent.point(1.0),
            direction: (frame.forward * angle.cos() + frame.outward * angle.sin()).normalize(),
            height: parent.heights[1],
            height_taper: branch.height_taper,
            length: branch.length * (0.48 + random.unit() * 0.14),
            width: parent.widths[1] * 0.53,
            remaining: branch.remaining - 1,
        };
        grow(segments, child, &mut Random::new(random.bits()));
    }
}

#[cfg(test)]
mod tests {
    use super::super::{Peak, Strain};
    use super::*;
    #[test]
    fn convex_side_follows_both_s_curve_lobes_and_ignores_radial_curvature() {
        let curve = |x: f64| DVec3::new(x, 0.08 * (8.0 * x).sin(), 1.0).normalize();
        let rotation = glam::DQuat::from_axis_angle(DVec3::new(1.0, 2.0, -3.0).normalize(), 1.2);
        for x in [-0.2, 0.2] {
            let (before, p, after) = (curve(x - 0.02), curve(x), curve(x + 0.02));
            let frame = convex_frame(before, p, after).unwrap();
            let bulge = p - (before + after) * 0.5;
            let bulge = (bulge - p * bulge.dot(p)).normalize();
            assert!(
                frame.outward.dot(bulge) > 0.99,
                "branch aimed into the concave side"
            );
            assert!(
                frame.outward.y * x > 0.0,
                "S lobes should choose opposite sides"
            );
            assert!(frame.outward.dot(p).abs() < 1.0e-12);
            assert!(frame.outward.dot(frame.forward).abs() < 1.0e-12);
            let rotated = convex_frame(rotation * before, rotation * p, rotation * after).unwrap();
            assert!(rotated.outward.distance(rotation * frame.outward) < 1.0e-10);
        }
        assert!(convex_frame(curve(-0.02), curve(0.0), curve(0.02)).is_none());
        let circle = |a: f64| DVec3::new(a.sin(), 0.0, a.cos());
        assert!(convex_frame(circle(-0.2), DVec3::Z, circle(0.2)).is_none());
    }

    fn curved_boundary() -> Boundary {
        let normal = -DVec3::X;
        let start = DVec3::new(0.0, -0.5_f64.sin(), 0.5_f64.cos());
        Boundary {
            start,
            tangent: normal.cross(start),
            normal,
            length: 1.0,
            uplift_offset: 0.0,
            envelope_sin: 0.2,
            strain: [Strain {
                compression: 0.25,
                ..Strain::default()
            }; 9],
            peaks: vec![
                Peak {
                    height: 1.0,
                    bend: -0.6,
                },
                Peak {
                    height: 1.0,
                    bend: 0.6,
                },
                Peak {
                    height: 1.0,
                    bend: -0.6,
                },
                Peak {
                    height: 1.0,
                    bend: 0.6,
                },
                Peak {
                    height: 1.0,
                    bend: -0.6,
                },
            ],
        }
    }

    #[test]
    fn every_primary_attachment_leaves_the_convex_side_of_the_wobbled_crest() {
        let edge = curved_boundary();
        let normal = edge.normal;
        let options = TectonicOptions {
            ruggedness: 1.0,
            branch_depth: 1,
            ..TectonicOptions::default()
        };
        let field = RidgeField::new(std::slice::from_ref(&edge), 0.03, 42, options);
        assert_eq!(field.segments.len(), 9);
        let (mut positive, mut negative) = (false, false);
        for path in field.segments.chunks_exact(3) {
            let (_, _, t) = edge.coordinates(path[0].start);
            let frame = root_frame(&edge, 0.03, options, t).unwrap();
            assert!(path[0].tangent.dot(frame.outward) > 0.0);
            positive |= frame.outward.dot(normal) > 0.0;
            negative |= frame.outward.dot(normal) < 0.0;
        }
        assert!(
            positive && negative,
            "convex selection must follow the alternating bends"
        );
    }

    #[test]
    fn every_bend_apex_is_anchored_at_full_parent_height_for_all_seeds_and_strengths() {
        let expected_height = 0.85 * 1.25_f64.tanh();
        for length in [1.0, 0.01] {
            let mut edge = curved_boundary();
            edge.length = length;
            for seed in 0..8 {
                for strength in [0.2, 0.8, 1.0] {
                    let options = TectonicOptions {
                        ruggedness: 1.0,
                        branch_depth: 1,
                        branching: strength,
                        ..TectonicOptions::default()
                    };
                    let field = RidgeField::new(std::slice::from_ref(&edge), 0.03, seed, options);
                    assert_eq!(field.segments.len(), 9);
                    for (i, path) in field.segments.chunks_exact(3).enumerate() {
                        let t = f64::from(u32::try_from(i + 1).unwrap()) / 4.0;
                        let apex = crest_point(&edge, 0.03, options, t);
                        assert!(
                            path[0].start.distance(apex) < 1.0e-12,
                            "missing mandatory apex"
                        );
                        assert!(
                            (path[0].sample(apex) - expected_height).abs() < 1.0e-12,
                            "join does not match the parent crest"
                        );
                        let sign = if i == 1 { -1.0 } else { 1.0 };
                        assert!(
                            path[0].tangent.dot(edge.normal) * sign > 0.0,
                            "tight bend selected the concave side"
                        );
                        assert!(path[0].heights[1] < expected_height);
                        assert!(path[2].heights[1].abs() < 1.0e-12);
                    }
                }
            }
        }
        let edge = curved_boundary();
        let make = |branching| {
            RidgeField::new(
                std::slice::from_ref(&edge),
                0.03,
                42,
                TectonicOptions {
                    ruggedness: 1.0,
                    branch_depth: 1,
                    branching,
                    ..TectonicOptions::default()
                },
            )
        };
        let weak = make(0.2);
        let strong = make(1.0);
        for (weak, strong) in weak
            .segments
            .chunks_exact(3)
            .zip(strong.segments.chunks_exact(3))
        {
            assert_eq!(weak[0].heights[0].to_bits(), strong[0].heights[0].to_bits());
            assert!(
                weak[0].heights[1] < strong[0].heights[1],
                "strength should change the descending profile"
            );
        }
    }

    #[test]
    fn recursive_spurs_shrink_descend_and_are_continuous_at_joints() {
        let root = Branch {
            start: DVec3::Z,
            direction: DVec3::X,
            length: 0.2,
            width: 0.02,
            height: 0.6,
            height_taper: 1.15,
            remaining: 3,
        };
        let mut segments = Vec::new();
        grow(&mut segments, root, &mut Random::new(42));
        assert_eq!(segments.len(), 21);
        for segment in &segments {
            assert!(segment.heights[1] < segment.heights[0]);
            assert!(segment.widths[1] < segment.widths[0]);
            assert!(segment.length <= root.length / 3.0 + 1.0e-12);
            assert!(segment.widths[0] <= root.width);
            assert!(segment.heights[0] <= root.height);
            assert!(segment.sample(-segment.centre).abs() < 1.0e-12);
            assert!((segment.sample(segment.start) - segment.heights[0]).abs() < 1.0e-12);
            assert!((segment.sample(segment.point(1.0)) - segment.heights[1]).abs() < 1.0e-12);
        }
        for pair in segments[..3].windows(2) {
            assert!(pair[0].point(1.0).distance(pair[1].start) < 1.0e-12);
            assert!((pair[0].heights[1] - pair[1].heights[0]).abs() < 1.0e-12);
        }
        for (parent, child, offset) in [
            (0, 3, 0),
            (0, 12, 1),
            (3, 6, 0),
            (3, 9, 1),
            (12, 15, 0),
            (12, 18, 1),
        ] {
            let parent_path = &segments[parent..parent + 3];
            let first_child = segments[child];
            let segment = parent_path[offset];
            assert!(
                first_child.start.distance(segment.point(1.0)) < 1.0e-12,
                "attachment missed the bend apex"
            );
            assert!(
                (first_child.heights[0] - segment.heights[1]).abs() < 1.0e-12,
                "child starts below its parent"
            );
            assert!(
                (first_child.sample(first_child.start) - segment.sample(first_child.start)).abs()
                    < 1.0e-12
            );
            let angle = first_child
                .start
                .dot(segment.tangent)
                .atan2(first_child.start.dot(segment.start));
            let t = angle / segment.length;
            let progress = if offset == 0 {
                t / 3.0
            } else {
                (1.0 + t) / 3.0
            };
            let frame = child_frame(parent_path, progress).unwrap();
            assert!(
                first_child.tangent.dot(frame.outward) > 0.0,
                "recursive branch points into its parent's concavity"
            );
        }
        assert!(segments[3].widths[0] < segments[0].widths[0]);
        assert!(segments[3].heights[0] < segments[0].heights[0]);
        let mut again = Vec::new();
        grow(&mut again, root, &mut Random::new(42));
        assert_eq!(segments, again);
        let mut shallow = Vec::new();
        grow(
            &mut shallow,
            Branch {
                remaining: 2,
                ..root
            },
            &mut Random::new(42),
        );
        assert!(
            shallow.iter().all(|segment| segments.contains(segment)),
            "more levels must preserve the existing coarse ridge network"
        );
        let field = RidgeField {
            index: FeatureIndex::new(segments.iter().map(|s| (s.centre, s.radius))),
            segments,
        };
        for segment in &field.segments {
            for t in [0.0, 0.25, 0.5, 0.75, 1.0] {
                let p = segment.point(t);
                let exhaustive = field
                    .segments
                    .iter()
                    .map(|s| s.sample(p))
                    .fold(0.0, f64::max);
                assert!((field.sample(p) - exhaustive).abs() < 1.0e-12);
            }
        }
    }
}
