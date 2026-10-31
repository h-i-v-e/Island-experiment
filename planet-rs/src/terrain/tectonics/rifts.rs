//! Finite fault-basin networks entering continental crust from extensional
//! margins. A continuous source lets refinement retain earlier erosion cuts.
use super::{Boundary, Plate, TectonicOptions};
use crate::{
    DVec3,
    terrain::{feature_index::FeatureIndex, random::Random},
};

pub(super) struct RiftField {
    segments: Vec<Segment>,
    index: FeatureIndex,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Segment {
    start: DVec3,
    tangent: DVec3,
    normal: DVec3,
    length: f64,
    depths: [f64; 2],
    widths: [f64; 2],
    centre: DVec3,
    radius: f64,
    cap_cos: f64,
}

impl Segment {
    fn new(a: DVec3, b: DVec3, depths: [f64; 2], widths: [f64; 2]) -> Self {
        let cross = a.cross(b);
        let length = cross.length().atan2(a.dot(b));
        let normal = cross.normalize();
        let centre = (a + b).normalize();
        let radius = length * 0.5 + widths[0].max(widths[1]);
        Self {
            start: a,
            tangent: normal.cross(a),
            normal,
            length,
            depths,
            widths,
            centre,
            radius,
            cap_cos: radius.cos(),
        }
    }

    fn point(self, t: f64) -> DVec3 {
        self.start * (self.length * t).cos() + self.tangent * (self.length * t).sin()
    }

    fn depth(self, t: f64) -> f64 {
        self.depths[0] + (self.depths[1] - self.depths[0]) * t
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
        let distance = side.hypot(end) / width;
        if distance >= 1.0 {
            0.0
        } else {
            self.depth(t) * (1.0 - distance * distance).powi(2)
        }
    }
}

#[derive(Clone, Copy)]
struct Branch {
    start: DVec3,
    direction: DVec3,
    length: f64,
    width: f64,
    depth: f64,
    remaining: u32,
}

impl RiftField {
    pub fn new(
        boundaries: &[Boundary],
        plates: &[Plate],
        spacing: f64,
        seed: u64,
        options: TectonicOptions,
    ) -> Self {
        let mut segments = Vec::new();
        if options.rift_strength > 0.0
            && options.rift_branching > 0
            && options.drift > 0.0
            && options.deformation > 0.0
        {
            for (id, edge) in boundaries.iter().enumerate() {
                seed_roots(
                    &mut segments,
                    edge,
                    plates,
                    spacing,
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

fn continental_at(plates: &[Plate], point: DVec3) -> bool {
    plates
        .iter()
        .max_by(|a, b| a.centre.dot(point).total_cmp(&b.centre.dot(point)))
        .is_some_and(|plate| plate.continental)
}

fn seed_roots(
    segments: &mut Vec<Segment>,
    edge: &Boundary,
    plates: &[Plate],
    spacing: f64,
    seed: u64,
    options: TectonicOptions,
) {
    // Three independent slots per finite edge, rather than repeated narrow
    // grooves along every force sample. Short edges produce just one basin.
    let slots: u32 = if edge.length > spacing * 0.8 { 3 } else { 1 };
    for slot in 0..slots {
        let root_seed =
            seed ^ 0x79ce_5a63_d8e2_047b ^ u64::from(slot).wrapping_mul(0x9e37_79b9_7f4a_7c15);
        let mut random = Random::new(root_seed);
        let t = (f64::from(slot) + 0.2 + random.unit() * 0.6) / f64::from(slots);
        let strain = edge.force(t);
        if strain.extension <= 0.01 || strain.extension <= strain.compression * 1.25 {
            continue;
        }
        let root = edge.point(edge.length * t);
        let depth =
            (strain.extension * options.deformation * 5.0).tanh() * options.rift_strength * 0.35;
        for side in [-1.0, 1.0] {
            let inland = edge.normal * side;
            let probe =
                (root * (spacing * 0.12).cos() + inland * (spacing * 0.12).sin()).normalize();
            if !continental_at(plates, probe) {
                continue;
            }
            let side_seed = root_seed ^ if side > 0.0 { 0x8bd9_0741 } else { 0x3ca5_62e7 };
            let mut random = Random::new(side_seed);
            let width = spacing * (0.025 + random.unit() * 0.025);
            let start = (root * (width * 0.8).cos() - inland * (width * 0.8).sin()).normalize();
            let direction = (inland - start * inland.dot(start)).normalize();
            grow(
                segments,
                Branch {
                    start,
                    direction,
                    length: spacing * (0.3 + random.unit() * 0.3),
                    width,
                    depth: depth.min(0.35),
                    remaining: options.rift_branching,
                },
                side_seed,
            );
        }
    }
}

fn grow(segments: &mut Vec<Segment>, branch: Branch, seed: u64) {
    let mut random = Random::new(seed ^ 0x6e53_b8ac_412f_9d70);
    let mut point = branch.start;
    let mut direction = branch.direction;
    let mut width = branch.width;
    let mut depth = branch.depth;
    let path: [Segment; 4] = std::array::from_fn(|i| {
        let normal = point.cross(direction);
        let bend = (random.unit() - 0.5) * 0.65;
        let forward = direction * bend.cos() + normal * bend.sin();
        let angle = branch.length * 0.25;
        let end = (point * angle.cos() + forward * angle.sin()).normalize();
        let progress = f64::from(u32::try_from(i + 1).expect("four basin segments")) * 0.25;
        let next_depth = branch.depth * (1.0 - progress).sqrt();
        let next_width = branch.width * (0.65 + random.unit() * 0.6) * (1.0 - progress * 0.5);
        let segment = Segment::new(point, end, [depth, next_depth], [width, next_width]);
        direction = (forward - end * forward.dot(end)).normalize();
        point = end;
        width = next_width;
        depth = next_depth;
        segment
    });
    segments.extend(path);
    if branch.remaining <= 1 {
        return;
    }
    for (i, side) in [(1_usize, -1.0), (2, 1.0)] {
        let parent = path[i];
        let start = parent.point(0.5);
        let tangent = parent.normal.cross(start);
        let heading = (tangent * 0.5 + parent.normal * side * 0.866_025_403_784_438_6).normalize();
        grow(
            segments,
            Branch {
                start,
                direction: heading,
                length: branch.length * 0.48,
                width: branch.width * 0.65,
                depth: parent.depth(0.5),
                remaining: branch.remaining - 1,
            },
            seed ^ u64::try_from(i)
                .expect("basin child index")
                .wrapping_mul(0xa24b_aed4_963e_e407),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terrain::tectonics::{Peak, Strain};

    fn fixture(extension: f64, compression: f64) -> (Boundary, [Plate; 2]) {
        let angle = 0.6_f64;
        let start = DVec3::new(0.0, -angle.sin(), angle.cos());
        let normal = DVec3::X;
        (
            Boundary {
                start,
                tangent: normal.cross(start),
                normal,
                length: 1.2,
                uplift_offset: 0.0,
                envelope_sin: 0.5,
                strain: [Strain {
                    extension,
                    compression,
                    shear: 0.0,
                }; 9],
                peaks: vec![
                    Peak {
                        height: 1.0,
                        bend: 0.0
                    };
                    2
                ],
            },
            [
                Plate {
                    centre: DVec3::new(-0.4, 0.0, 1.0).normalize(),
                    omega: DVec3::ZERO,
                    continental: false,
                },
                Plate {
                    centre: DVec3::new(0.4, 0.0, 1.0).normalize(),
                    omega: DVec3::ZERO,
                    continental: true,
                },
            ],
        )
    }

    #[test]
    fn broad_basins_enter_the_continent_and_have_finite_support() {
        let (edge, plates) = fixture(0.3, 0.0);
        let field = RiftField::new(&[edge], &plates, 0.8, 42, TectonicOptions::default());
        assert!(!field.segments.is_empty());
        assert!(field.sample(-DVec3::Z).abs() < f64::EPSILON);
        let root = field.segments[0];
        let p = root.point(0.4);
        let width = root.widths[0].min(root.widths[1]);
        let beside = (p * (width * 0.4).cos() + root.normal * (width * 0.4).sin()).normalize();
        assert!(field.sample(p) > 0.1);
        assert!(field.sample(beside) > field.sample(p) * 0.5);
        assert!(
            field
                .segments
                .iter()
                .all(|s| s.depths.iter().all(|&d| (0.0..=0.35).contains(&d)))
        );
        for segment in &field.segments {
            for t in [0.0, 0.25, 0.5, 0.75, 1.0] {
                let p = segment.point(t);
                let expected = field
                    .segments
                    .iter()
                    .map(|s| s.sample(p))
                    .fold(0.0, f64::max);
                assert!(
                    (field.sample(p) - expected).abs() < 1e-12,
                    "index lost a basin"
                );
            }
        }
    }

    #[test]
    fn hierarchy_is_deterministic_and_does_not_change_existing_basin_geometry() {
        let (edge, plates) = fixture(0.3, 0.0);
        let boundaries = [edge];
        let options = TectonicOptions {
            rift_branching: 1,
            ..TectonicOptions::default()
        };
        let shallow = RiftField::new(&boundaries, &plates, 0.8, 42, options);
        let deep_options = TectonicOptions {
            rift_branching: 4,
            ..options
        };
        let deep = RiftField::new(&boundaries, &plates, 0.8, 42, deep_options);
        let repeat = RiftField::new(&boundaries, &plates, 0.8, 42, deep_options);
        assert_eq!(deep.segments, repeat.segments);
        assert_eq!(deep.segments.len(), shallow.segments.len() * 15);
        assert!(shallow.segments.iter().all(|s| deep.segments.contains(s)));
        assert!(deep.segments.len() <= 180);
        assert_ne!(
            deep.segments,
            RiftField::new(&boundaries, &plates, 0.8, 43, deep_options).segments
        );
        // Branch roots retain their parent's depth: no artificial sill appears
        // where a smaller fault joins the main basin.
        let root_path = &deep.segments[..4];
        let child = deep.segments[4];
        let attachment = root_path[1].point(0.5);
        assert!(child.start.distance(attachment) < 1.0e-12);
        assert!((child.depths[0] - root_path[1].depth(0.5)).abs() < 1.0e-12);
    }

    #[test]
    fn compression_oceanic_crust_and_zero_controls_do_not_create_fault_basins() {
        let options = TectonicOptions::default();
        let (edge, plates) = fixture(0.0, 0.3);
        assert!(
            RiftField::new(&[edge], &plates, 0.8, 42, options)
                .segments
                .is_empty()
        );
        let (edge, mut plates) = fixture(0.3, 0.0);
        let boundaries = [edge];
        for disabled in [
            TectonicOptions {
                rift_strength: 0.0,
                ..options
            },
            TectonicOptions {
                rift_branching: 0,
                ..options
            },
            TectonicOptions {
                drift: 0.0,
                ..options
            },
            TectonicOptions {
                deformation: 0.0,
                ..options
            },
        ] {
            assert!(
                RiftField::new(&boundaries, &plates, 0.8, 42, disabled)
                    .segments
                    .is_empty()
            );
        }
        plates[1].continental = false;
        assert!(
            RiftField::new(&boundaries, &plates, 0.8, 42, options)
                .segments
                .is_empty()
        );
    }
}
