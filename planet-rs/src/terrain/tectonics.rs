//! Seeded spherical Voronoi plates with Euler-pole drift. This is a kinematic
//! terrain model: crust regions advect and boundary compression/extension
//! accumulates through time, without simulating mantle convection or slabs.
mod fragments;
mod ridges;
mod rifts;

use super::{TerrainOptions, initial_noise, random::Random};
use crate::{DVec3, PlanetError};
use glam::DQuat;
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum TerrainSource {
    #[default]
    Noise,
    Tectonics,
}

impl TerrainSource {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Noise => "Noise layers",
            Self::Tectonics => "Tectonic drift",
        }
    }
}

impl std::str::FromStr for TerrainSource {
    type Err = &'static str;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "noise" => Ok(Self::Noise),
            "tectonics" => Ok(Self::Tectonics),
            _ => Err("terrain source must be noise or tectonics"),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct TectonicOptions {
    pub plates: u32,
    pub steps: u32,
    /// Total drift in radians at unit speed, not a geological time in years.
    pub drift: f64,
    /// Fraction of plate seeds with continental crust, not a forced land area.
    pub continental_fraction: f64,
    /// Boundary influence width relative to typical plate spacing.
    pub boundary_width: f64,
    pub deformation: f64,
    /// Irregular linked peaks, saddles and bends along each mountain belt.
    pub ruggedness: f64,
    /// Descending fractal spurs attached to compressional mountain crests.
    pub branching: f64,
    pub branch_depth: u32,
    /// Small contribution from the existing noise layers, zero for plates only.
    pub detail: f64,
    /// Artistic volcanic chains where sideways shear dominates.
    pub volcanism: f64,
    pub volcano_spacing: f64,
    /// Deform plate-cell continents into advected crustal fragments, 0..1.
    pub fragmentation: f64,
    /// Characteristic fragment length relative to plate spacing, 0.35..2.
    pub fragment_scale: f64,
    /// Subsidence in branching extensional basins, independent of narrow rifts.
    pub rift_strength: f64,
    pub rift_branching: u32,
}

impl Default for TectonicOptions {
    fn default() -> Self {
        Self {
            plates: 16,
            steps: 48,
            drift: 0.25,
            continental_fraction: 0.375,
            boundary_width: 0.35,
            deformation: 1.0,
            ruggedness: 0.8,
            branching: 0.8,
            branch_depth: 3,
            detail: 0.08,
            volcanism: 1.0,
            volcano_spacing: 1.0,
            fragmentation: 0.75,
            fragment_scale: 1.0,
            rift_strength: 0.65,
            rift_branching: 2,
        }
    }
}

impl TectonicOptions {
    pub(super) fn validate(self) -> Result<(), PlanetError> {
        if !(4..=64).contains(&self.plates)
            || !(1..=128).contains(&self.steps)
            || self.branch_depth > 4
            || self.rift_branching > 4
        {
            return Err(PlanetError::InvalidOptions(
                "tectonics needs 4..64 plates, 1..128 drift steps and 0..4 ridge/rift branching levels",
            ));
        }
        for (value, min, max) in [
            (self.drift, 0.0, 1.0),
            (self.continental_fraction, 0.0, 1.0),
            (self.boundary_width, 0.05, 1.0),
            (self.deformation, 0.0, 3.0),
            (self.ruggedness, 0.0, 1.0),
            (self.branching, 0.0, 1.0),
            (self.detail, 0.0, 0.25),
            (self.volcanism, 0.0, 3.0),
            (self.volcano_spacing, 0.5, 3.0),
            (self.fragmentation, 0.0, 1.0),
            (self.fragment_scale, 0.35, 2.0),
            (self.rift_strength, 0.0, 2.0),
        ] {
            if !value.is_finite() || !(min..=max).contains(&value) {
                return Err(PlanetError::InvalidOptions(
                    "invalid tectonic drift, crust fraction, boundary width, deformation, ruggedness, detail or volcanism",
                ));
            }
        }
        Ok(())
    }
}

#[derive(Clone, Copy)]
struct Plate {
    centre: DVec3,
    omega: DVec3,
    continental: bool,
}

pub(super) struct TectonicField {
    plates: Vec<Plate>,
    boundaries: Vec<Boundary>,
    width: f64,
    crust_blend: f64,
    deformation: f64,
    ruggedness: f64,
    volcanoes: Vec<Volcano>,
    ridges: ridges::RidgeField,
    fragments: fragments::FragmentField,
    rifts: rifts::RiftField,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct Sample {
    pub height: f64,
    pub hardness: f64,
}

#[derive(Clone, Copy, Default)]
struct Strain {
    compression: f64,
    extension: f64,
    shear: f64,
}

struct Boundary {
    start: DVec3,
    tangent: DVec3,
    normal: DVec3,
    length: f64,
    uplift_offset: f64,
    envelope_sin: f64,
    strain: [Strain; 9],
    peaks: Vec<Peak>,
}

#[derive(Clone, Copy)]
struct Peak {
    height: f64,
    bend: f64,
}

impl TectonicField {
    /// History is reduced to force samples on the final Voronoi edges. Sampling
    /// those finite arcs keeps uplift narrow instead of smearing a moving edge
    /// over a wide stationary swath of terrain.
    pub fn new(seed: u64, options: TectonicOptions) -> Self {
        let mut history = drift_history(seed, options);
        let spacing = (4.0 * std::f64::consts::PI / f64::from(options.plates)).sqrt();
        let width = spacing * options.boundary_width * 0.12;
        let boundaries = boundaries(&history, width, seed);
        let volcanoes = volcanoes(&boundaries, width, seed, options);
        let ridges = ridges::RidgeField::new(&boundaries, width, seed, options);
        let fragments = fragments::FragmentField::new(&history[0], spacing, seed, options);
        let plates = history.pop().expect("initial plate frame");
        let rifts = rifts::RiftField::new(&boundaries, &plates, spacing, seed, options);
        Self {
            plates,
            boundaries,
            width,
            crust_blend: spacing * spacing * 0.025,
            deformation: options.deformation,
            ruggedness: options.ruggedness,
            volcanoes,
            ridges,
            fragments,
            rifts,
        }
    }

    pub fn sample(&self, point: DVec3) -> Sample {
        let mut uplift: f64 = 0.0;
        let mut rift: f64 = 0.0;
        let mut compression: f64 = 0.0;
        let mut extension: f64 = 0.0;
        for boundary in &self.boundaries {
            // Most edges are far away. Avoid trigonometry for those samples.
            if point.dot(boundary.normal).abs() > boundary.envelope_sin {
                continue;
            }
            let (signed, end_distance, t) = boundary.coordinates(point);
            let strain = boundary.force(t);
            let shape = boundary.shape(t);
            let height = 1.0 - self.ruggedness + self.ruggedness * shape.height;
            let width = self.width * (1.0 + self.ruggedness * (shape.height - 0.5) * 0.7);
            let bend = self.width * self.ruggedness * shape.bend;
            let peak =
                belt_profile((signed - boundary.uplift_offset - bend).hypot(end_distance) / width);
            let valley = belt_profile(signed.hypot(end_distance) / self.width);
            uplift = uplift
                .max((strain.compression * 5.0 * self.deformation).tanh() * 0.85 * peak * height);
            rift = rift.max((strain.extension * 4.0 * self.deformation).tanh() * 0.5 * valley);
            compression = compression.max(strain.compression * peak);
            extension = extension.max(strain.extension * valley);
        }
        let branch = self.ridges.sample(point);
        uplift = uplift.max(branch);
        compression = compression.max(branch * 0.4);
        let volcanic = self
            .volcanoes
            .iter()
            .map(|v| v.sample(point))
            .fold(0.0, f64::max);
        uplift = uplift.max(volcanic);
        let crust = self.continental_weight(point);
        let basins = self.rifts.sample(point);
        Sample {
            // Low continental interiors make relief come from the mountain
            // belts. Bounded belt amplitudes retain the signed-radius contract
            // without flattening a narrow summit through a final tanh clamp.
            height: -0.35 + crust * 0.43 + uplift - rift.max(basins),
            hardness: (0.45
                + crust * 0.18
                + self.deformation * (compression + volcanic * 0.3 - extension * 0.65)
                - basins * 0.6)
                .clamp(0.05, 0.95),
        }
    }

    pub fn initial_sample(&self, point: DVec3, seed: u64, settings: &TerrainOptions) -> Sample {
        let mut sample = self.sample(point);
        let detail = if settings.noise_layers.iter().any(|layer| layer.weight > 0.0) {
            settings.tectonics.detail
        } else {
            0.0
        };
        if detail > 0.0 {
            // Bend the height belts gently with a continuous Cartesian field;
            // keep geological hardness independent of the cosmetic detail.
            let bend = DVec3::new(
                crate::noise::fractal(seed ^ 0x31a5_73c9, point * 12.0, 3),
                crate::noise::fractal(seed ^ 0xa79c_53e1, point * 12.0, 3),
                crate::noise::fractal(seed ^ 0x4b61_d3f7, point * 12.0, 3),
            );
            let tangent = bend - point * bend.dot(point);
            let warped = (point + tangent * (self.width * detail * 4.0)).normalize();
            sample.height = (self.sample(warped).height
                + detail * initial_noise::sample(seed, point, &settings.noise_layers))
                / (1.0 + detail);
        }
        sample
    }

    pub fn into_volcanoes(self) -> Vec<Volcano> {
        self.volcanoes
    }

    pub fn features(
        &self,
        seed: u64,
        settings: &TerrainOptions,
    ) -> impl Iterator<Item = (DVec3, f64)> + '_ {
        let settings = *settings;
        self.volcanoes
            .iter()
            .filter(move |v| {
                let tangent = v.centre.any_orthonormal_vector();
                let bitangent = v.centre.cross(tangent);
                (0..12).any(|i| {
                    let angle = f64::from(i) * std::f64::consts::TAU / 12.0;
                    let radius = v.angular_radius * 0.18;
                    let p = v.centre * radius.cos()
                        + (tangent * angle.cos() + bitangent * angle.sin()) * radius.sin();
                    settings.land_bias
                        + settings.relief * self.initial_sample(p, seed, &settings).height
                        >= -0.01
                })
            })
            .map(|v| (v.centre, v.angular_radius))
            .chain(self.ridges.features().filter_map(move |(points, radius)| {
                points
                    .iter()
                    .any(|&p| {
                        settings.land_bias
                            + settings.relief * self.initial_sample(p, seed, &settings).height
                            >= -0.01
                    })
                    .then_some((points[1], radius))
            }))
            .chain(
                self.fragments
                    .features()
                    .chain(self.rifts.features())
                    .filter_map(move |(points, radius)| {
                        points
                            .iter()
                            .any(|&p| {
                                settings.land_bias
                                    + settings.relief
                                        * self.initial_sample(p, seed, &settings).height
                                    >= -0.01
                            })
                            .then_some((points[1], radius))
                    }),
            )
    }

    fn continental_weight(&self, point: DVec3) -> f64 {
        let nearest = self
            .plates
            .iter()
            .map(|p| p.centre.dot(point))
            .fold(f64::NEG_INFINITY, f64::max);
        let (continental, total) = self.plates.iter().fold((0.0, 0.0), |(c, t), plate| {
            let weight = ((plate.centre.dot(point) - nearest) / self.crust_blend).exp();
            (c + if plate.continental { weight } else { 0.0 }, t + weight)
        });
        self.fragments.weight(point, continental / total)
    }
}

/// A seeded cone on a finite sliding plate boundary. Angles are radians.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Volcano {
    pub centre: DVec3,
    pub angular_radius: f64,
    /// Summit uplift before Relief scaling and erosion.
    pub height: f64,
    base_cos: f64,
}
impl Volcano {
    fn sample(self, point: DVec3) -> f64 {
        let dot = self.centre.dot(point);
        if dot < self.base_cos {
            return 0.0;
        }
        let r = self.centre.cross(point).length().atan2(dot) / self.angular_radius;
        volcano_profile(r) * self.height
    }
}
fn volcano_profile(r: f64) -> f64 {
    if r >= 1.0 {
        0.0
    } else if r < 0.18 {
        0.82 + 0.18 * (r / 0.18).powi(2)
    } else {
        ((1.0 - r) / 0.82).powf(1.4)
    }
}
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // Bounded feature cells on finite arcs.
fn volcanoes(
    boundaries: &[Boundary],
    width: f64,
    seed: u64,
    options: TectonicOptions,
) -> Vec<Volcano> {
    if options.volcanism == 0.0 {
        return Vec::new();
    }
    let mut result = Vec::new();
    for (i, edge) in boundaries.iter().enumerate() {
        let mut random =
            Random::new(seed ^ 0xf6e1_49c2_73b8_a50d ^ u64::try_from(i).expect("boundary index"));
        let count = (edge.length / (width * 4.0 * options.volcano_spacing))
            .ceil()
            .clamp(1.0, 128.0) as u32;
        for j in 0..count {
            let t = (f64::from(j) + 0.2 + random.unit() * 0.6) / f64::from(count);
            let strain = edge.force(t);
            if strain.shear < 0.025
                || strain.shear < 1.2 * (strain.compression + strain.extension)
                || random.unit() < 0.18
            {
                continue;
            }
            let radius = width * (0.9 + random.unit() * 0.8);
            let bend = width * (random.unit() - 0.5) * 0.5;
            let centre =
                (edge.point(edge.length * t) * bend.cos() + edge.normal * bend.sin()).normalize();
            let height =
                (strain.shear * 8.0 * options.volcanism).tanh() * (0.32 * radius / width).min(0.85);
            result.push(Volcano {
                centre,
                angular_radius: radius,
                height,
                base_cos: radius.cos(),
            });
        }
    }
    result
}

fn belt_profile(distance: f64) -> f64 {
    if distance >= 4.0 {
        return 0.0;
    }
    let taper = (1.0 - distance * distance / 16.0).powi(2);
    taper * (0.78 * (-2.0 * distance * distance).exp() + 0.22 * (-0.2 * distance * distance).exp())
}

impl Boundary {
    fn point(&self, angle: f64) -> DVec3 {
        self.start * angle.cos() + self.tangent * angle.sin()
    }
    fn coordinates(&self, p: DVec3) -> (f64, f64, f64) {
        let signed = p.dot(self.normal).clamp(-1.0, 1.0).asin();
        let angle = p
            .dot(self.tangent)
            .atan2(p.dot(self.start))
            .rem_euclid(std::f64::consts::TAU);
        if angle <= self.length {
            return (signed, 0.0, angle / self.length);
        }
        let to_start = std::f64::consts::TAU - angle;
        let to_end = angle - self.length;
        if to_start < to_end {
            (signed, to_start, 0.0)
        } else {
            (signed, to_end, 1.0)
        }
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // Clamped to the eight finite intervals.
    fn force(&self, t: f64) -> Strain {
        let position = t.clamp(0.0, 1.0) * 8.0;
        let i = (position as usize).min(7);
        let fraction = position - f64::from(u32::try_from(i).expect("force sample index"));
        let a = self.strain[i];
        let b = self.strain[i + 1];
        Strain {
            compression: a.compression + (b.compression - a.compression) * fraction,
            extension: a.extension + (b.extension - a.extension) * fraction,
            shear: a.shear + (b.shear - a.shear) * fraction,
        }
    }

    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss
    )] // Bounded knot interval.
    fn shape(&self, progress: f64) -> Peak {
        let position = progress.clamp(0.0, 1.0) * (self.peaks.len() - 1) as f64;
        let index = (position as usize).min(self.peaks.len() - 2);
        let fraction = position - index as f64;
        let blend = fraction * fraction * (3.0 - 2.0 * fraction);
        let first = self.peaks[index];
        let next = self.peaks[index + 1];
        Peak {
            height: first.height + (next.height - first.height) * blend,
            bend: first.bend + (next.bend - first.bend) * blend,
        }
    }
}

fn drift_history(seed: u64, options: TectonicOptions) -> Vec<Vec<Plate>> {
    let mut random = Random::new(seed ^ 0x6a09_e667_f3bc_c909);
    let mut plates = seed_plates(&mut random, options);
    let mut history = Vec::with_capacity(options.steps as usize + 1);
    history.push(plates.clone());
    for _ in 0..options.steps {
        for plate in &mut plates {
            let angle = plate.omega.length() / f64::from(options.steps);
            if angle > 0.0 {
                plate.centre = DQuat::from_axis_angle(plate.omega.normalize(), angle)
                    .mul_vec3(plate.centre)
                    .normalize();
            }
        }
        history.push(plates.clone());
    }
    history
}

fn voronoi_owner(plates: &[Plate], p: DVec3, owner: usize) -> bool {
    let score = plates[owner].centre.dot(p) + 1.0e-10;
    plates.iter().all(|plate| plate.centre.dot(p) <= score)
}

fn boundaries(history: &[Vec<Plate>], width: f64, seed: u64) -> Vec<Boundary> {
    let plates = history.last().expect("plate frame");
    let mut corners: BTreeMap<(usize, usize), Vec<DVec3>> = BTreeMap::new();
    // A spherical Voronoi corner is the equal-dot intersection of three sites,
    // retained only if no other site is closer. This also handles hemispherical
    // site arrangements, where a convex-hull triangulator would reject input.
    for a in 0..plates.len() {
        for b in a + 1..plates.len() {
            for c in b + 1..plates.len() {
                let cross = (plates[a].centre - plates[b].centre)
                    .cross(plates[a].centre - plates[c].centre);
                if cross.length_squared() < 1.0e-20 {
                    continue;
                }
                let vertex = cross.normalize();
                for p in [vertex, -vertex] {
                    if !voronoi_owner(plates, p, a) {
                        continue;
                    }
                    for edge in [(a, b), (a, c), (b, c)] {
                        let list = corners.entry(edge).or_default();
                        if list.iter().all(|q| q.distance_squared(p) > 1.0e-16) {
                            list.push(p);
                        }
                    }
                }
            }
        }
    }
    corners
        .into_iter()
        .filter_map(|((a, b), points)| {
            if points.len() != 2 {
                return None;
            }
            let start = points[0];
            let end = points[1];
            let cross = start.cross(end);
            if cross.length_squared() < 1.0e-20 {
                return None;
            }
            let mut normal = cross.normalize();
            let mut length = cross.length().atan2(start.dot(end));
            if !voronoi_owner(plates, (start + end).normalize(), a) {
                normal = -normal;
                length = std::f64::consts::TAU - length;
            }
            let toward_a = (plates[a].centre - plates[b].centre).normalize();
            let uplift_offset = if plates[a].continental == plates[b].continental {
                0.0
            } else if plates[a].continental {
                width * 0.6 * normal.dot(toward_a).signum()
            } else {
                -width * 0.6 * normal.dot(toward_a).signum()
            };
            let mut edge = Boundary {
                start,
                tangent: normal.cross(start),
                normal,
                length,
                uplift_offset,
                envelope_sin: (6.2 * width + uplift_offset.abs()).sin(),
                strain: [Strain::default(); 9],
                peaks: peak_knots(seed, a, b, length, width),
            };
            for i in 0..9 {
                let p = edge
                    .point(length * f64::from(u32::try_from(i).expect("force sample index")) / 8.0);
                edge.strain[i] = boundary_history(history, a, b, p);
            }
            Some(edge)
        })
        .collect()
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // Bounded shape knots, independent of mesh resolution.
fn peak_knots(seed: u64, a: usize, b: usize, length: f64, width: f64) -> Vec<Peak> {
    let count = (length / (width * 1.8)).ceil().clamp(4.0, 96.0) as u32;
    let mut random = Random::new(
        seed ^ u64::try_from(a)
            .expect("plate index")
            .wrapping_mul(0x9e37_79b9)
            ^ u64::try_from(b)
                .expect("plate index")
                .wrapping_mul(0x85eb_ca6b)
            ^ 0x8e23_f19a_739c_b8d1,
    );
    (0..=count)
        .map(|_| Peak {
            height: 0.1 + 0.9 * random.unit().powi(2),
            bend: (random.unit() - 0.5) * 1.4,
        })
        .collect()
}

fn relative_closing(a: Plate, b: Plate, point: DVec3) -> f64 {
    let separation = b.centre - a.centre;
    let normal = (separation - point * separation.dot(point)).normalize_or_zero();
    (a.omega - b.omega).cross(point).dot(normal)
}

fn relative_shear(a: Plate, b: Plate, point: DVec3) -> f64 {
    let separation = b.centre - a.centre;
    let normal = (separation - point * separation.dot(point)).normalize_or_zero();
    (a.omega - b.omega)
        .cross(point)
        .dot(point.cross(normal))
        .abs()
}

fn boundary_history(history: &[Vec<Plate>], a: usize, b: usize, point: DVec3) -> Strain {
    let last = history.last().expect("plate frame");
    let omega = (last[a].omega + last[b].omega) * 0.5;
    let steps = f64::from(u32::try_from(history.len() - 1).expect("drift steps"));
    let mut result = Strain::default();
    for (i, frame) in history.iter().enumerate() {
        let remaining = 1.0 - f64::from(u32::try_from(i).expect("drift step index")) / steps;
        // Backtrack the edge sample with the pair's mean motion. History changes
        // belt strength, not its width on the final planet.
        let p = if omega.length_squared() > 0.0 {
            DQuat::from_axis_angle(omega.normalize(), -omega.length() * remaining).mul_vec3(point)
        } else {
            point
        };
        let closing = relative_closing(frame[a], frame[b], p);
        let weight = if i == 0 || i + 1 == history.len() {
            0.5
        } else {
            1.0
        };
        result.compression += closing.max(0.0) * weight / steps;
        result.extension += (-closing).max(0.0) * weight / steps;
        result.shear += relative_shear(frame[a], frame[b], p) * weight / steps;
    }
    let current = relative_closing(last[a], last[b], point);
    result.compression = (result.compression + current.max(0.0)) * 0.5;
    result.extension = (result.extension + (-current).max(0.0)) * 0.5;
    result.shear = (result.shear + relative_shear(last[a], last[b], point)) * 0.5;
    result
}

fn seed_plates(random: &mut Random, options: TectonicOptions) -> Vec<Plate> {
    let mut plates: Vec<Plate> = Vec::with_capacity(options.plates as usize);
    // Best-candidate random sites avoid tiny sliver plates and latitude bands.
    for i in 0..options.plates {
        let centre = (0..24)
            .map(|_| random.direction())
            .max_by(|a, b| {
                let nearest = |p: DVec3| {
                    plates
                        .iter()
                        .map(|plate| 1.0 - plate.centre.dot(p))
                        .fold(2.0, f64::min)
                };
                nearest(*a).total_cmp(&nearest(*b))
            })
            .expect("plate candidates");
        let bearing = random.direction();
        let tangent = (bearing - centre * bearing.dot(centre)).normalize_or_zero();
        let omega = centre.cross(tangent) * (options.drift * (0.5 + random.unit()));
        // An exact seed count avoids accidentally losing a whole crust type.
        let continental =
            f64::from(i) + 0.5 < f64::from(options.plates) * options.continental_fraction;
        plates.push(Plate {
            centre,
            omega,
            continental,
        });
    }
    // Best-candidate insertion spreads early seeds apart. Shuffle the crust
    // flags independently so adjacent continental plates can collide and form
    // connected landmasses, rather than every continent being isolated.
    for i in (1..plates.len()).rev() {
        let bound = u64::try_from(i + 1).expect("validated plate count");
        let j = usize::try_from(random.bits() % bound).expect("bounded plate index");
        let continental = plates[i].continental;
        plates[i].continental = plates[j].continental;
        plates[j].continental = continental;
    }
    plates
}

#[cfg(test)]
mod tests {
    use super::*;

    fn coast_metrics(mesh: &crate::PlanetMesh, field: &TectonicField) -> (f64, f64) {
        let heights: Vec<_> = mesh
            .directions
            .iter()
            .map(|&p| field.sample(p).height)
            .collect();
        let mut perimeter = 0.0;
        let mut land_area = 0.0;
        for face in &mesh.triangles {
            let mut polygon = Vec::with_capacity(4);
            let mut crossings = Vec::with_capacity(2);
            for edge in 0..3 {
                let a = face[edge] as usize;
                let b = face[(edge + 1) % 3] as usize;
                if heights[a] >= 0.0 {
                    polygon.push(mesh.directions[a]);
                }
                if (heights[a] >= 0.0) != (heights[b] >= 0.0) {
                    let t = heights[a] / (heights[a] - heights[b]);
                    let p = mesh.directions[a].lerp(mesh.directions[b], t).normalize();
                    polygon.push(p);
                    crossings.push(p);
                }
            }
            if let [a, b] = crossings.as_slice() {
                perimeter += a.cross(*b).length().atan2(a.dot(*b));
            }
            for i in 1..polygon.len().saturating_sub(1) {
                land_area +=
                    crate::terrain::graph::solid_angle(polygon[0], polygon[i], polygon[i + 1]);
            }
        }
        let total = 4.0 * std::f64::consts::PI;
        (
            perimeter / (land_area * (total - land_area)).sqrt(),
            1.0 - land_area / total,
        )
    }

    #[test]
    fn fragmentation_approaches_the_disabled_model_continuously() {
        let options = TectonicOptions {
            fragmentation: 0.0,
            rift_strength: 0.0,
            ..TectonicOptions::default()
        };
        let old = TectonicField::new(42, options);
        let near = TectonicField::new(
            42,
            TectonicOptions {
                fragmentation: 1e-9,
                ..options
            },
        );
        let mut random = Random::new(18);
        for _ in 0..2048 {
            let p = random.direction();
            let a = old.sample(p);
            let b = near.sample(p);
            assert!((a.height - b.height).abs() < 1e-6);
            assert!((a.hardness - b.hardness).abs() < 1e-6);
        }
    }

    #[test]
    fn fragmented_coasts_are_more_varied_without_changing_plate_or_mountain_geometry() {
        let mesh = crate::generate(crate::PlanetOptions {
            vertex_count: 8192,
            ..crate::PlanetOptions::default()
        })
        .unwrap();
        let mut before_score = 0.0;
        let mut after_score = 0.0;
        for seed in [7, 42, 123] {
            let options = TectonicOptions::default();
            let old = TectonicField::new(
                seed,
                TectonicOptions {
                    fragmentation: 0.0,
                    rift_strength: 0.0,
                    ..options
                },
            );
            let new = TectonicField::new(seed, options);
            assert_eq!(old.volcanoes, new.volcanoes);
            assert_eq!(old.boundaries.len(), new.boundaries.len());
            for (a, b) in old.boundaries.iter().zip(&new.boundaries) {
                assert_eq!(a.start, b.start);
                assert_eq!(a.normal, b.normal);
                assert_eq!(a.length.to_bits(), b.length.to_bits());
            }
            let (old_coast, old_sea) = coast_metrics(&mesh, &old);
            let (new_coast, new_sea) = coast_metrics(&mesh, &new);
            println!(
                "Coast seed {seed}: score {old_coast:.3} -> {new_coast:.3}; sea {old_sea:.3} -> {new_sea:.3}"
            );
            assert!((0.25..0.95).contains(&new_sea));
            before_score += old_coast;
            after_score += new_coast;
        }
        assert!(
            after_score > before_score * 1.1,
            "coast complexity {before_score} -> {after_score}"
        );
    }

    fn chain_fixture(closing: f64) -> TectonicField {
        let start = DVec3::new(0.0, -0.5_f64.sin(), 0.5_f64.cos());
        let normal = -DVec3::X;
        let plates = vec![
            Plate {
                centre: DVec3::new(-0.3, 0.0, 1.0).normalize(),
                omega: DVec3::ZERO,
                continental: true,
            },
            Plate {
                centre: DVec3::new(0.3, 0.0, 1.0).normalize(),
                omega: DVec3::ZERO,
                continental: true,
            },
        ];
        let strain = Strain {
            compression: closing.max(0.0),
            extension: (-closing).max(0.0),
            shear: 0.0,
        };
        TectonicField {
            plates,
            boundaries: vec![Boundary {
                start,
                tangent: normal.cross(start),
                normal,
                length: 1.0,
                uplift_offset: 0.0,
                envelope_sin: (4.0_f64 * 0.03).sin(),
                strain: [strain; 9],
                peaks: vec![
                    Peak {
                        height: 1.0,
                        bend: 0.0
                    };
                    2
                ],
            }],
            width: 0.03,
            crust_blend: 0.04,
            deformation: 1.0,
            ruggedness: 0.0,
            volcanoes: Vec::new(),
            ridges: ridges::RidgeField::new(&[], 0.03, 42, TectonicOptions::default()),
            fragments: fragments::FragmentField::new(&[], 1.0, 42, TectonicOptions::default()),
            rifts: rifts::RiftField::new(&[], &[], 1.0, 42, TectonicOptions::default()),
        }
    }

    #[test]
    fn sliding_motion_produces_seeded_cone_chains_with_calderas_and_can_be_disabled() {
        let mut field = chain_fixture(0.0);
        field.boundaries[0].strain = [Strain {
            shear: 0.25,
            ..Strain::default()
        }; 9];
        let options = TectonicOptions::default();
        let cones = volcanoes(&field.boundaries, field.width, 42, options);
        assert!(cones.len() > 3);
        assert_eq!(
            cones,
            volcanoes(&field.boundaries, field.width, 42, options)
        );
        assert_ne!(
            cones,
            volcanoes(&field.boundaries, field.width, 43, options)
        );
        for v in &cones {
            let (signed, end, t) = field.boundaries[0].coordinates(v.centre);
            assert!(signed.abs() < field.width * 0.26 && end == 0.0 && (0.0..1.0).contains(&t));
            let slope = v.centre.any_orthonormal_vector();
            let p = |r: f64| {
                v.centre * (v.angular_radius * r).cos() + slope * (v.angular_radius * r).sin()
            };
            assert!(v.sample(p(0.0)) < v.sample(p(0.18)));
            assert!(v.sample(p(0.18)) > v.sample(p(0.6)));
            assert!(v.sample(p(1.01)).abs() < 1.0e-12);
        }
        assert!(
            volcanoes(
                &field.boundaries,
                field.width,
                42,
                TectonicOptions {
                    volcanism: 0.0,
                    ..options
                }
            )
            .is_empty()
        );
        field.boundaries[0].strain = [Strain {
            compression: 0.25,
            ..Strain::default()
        }; 9];
        assert!(volcanoes(&field.boundaries, field.width, 42, options).is_empty());
        let default = TectonicField::new(42, options);
        assert!(!default.volcanoes.is_empty());
        assert!(
            TectonicField::new(
                42,
                TectonicOptions {
                    drift: 0.0,
                    ..options
                }
            )
            .volcanoes
            .is_empty()
        );
    }

    #[test]
    fn fractal_spurs_extend_off_the_main_range_without_raising_its_crest_or_distant_terrain() {
        let mut field = chain_fixture(0.25);
        field.ruggedness = 1.0;
        field.boundaries[0].peaks = vec![
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
        ];
        let options = TectonicOptions {
            ruggedness: 1.0,
            ..TectonicOptions::default()
        };
        let before = field.sample(DVec3::Z).height;
        field.ridges = ridges::RidgeField::new(&field.boundaries, field.width, 42, options);
        let count = field.ridges.features().count();
        assert_eq!(count, 21);
        assert!((field.sample(DVec3::Z).height - before).abs() < 1.0e-12);
        let mut maximum_spur: f64 = 0.0;
        for y in -30..=30 {
            for x in 16..=40 {
                let p = DVec3::new(-f64::from(x) * 0.005, f64::from(y) * 0.005, 1.0).normalize();
                maximum_spur = maximum_spur.max(field.ridges.sample(p));
            }
        }
        assert!(
            maximum_spur > 0.05,
            "no distinct relief beside the main belt"
        );
        assert!((field.sample(-DVec3::Z).height - 0.08).abs() < 1.0e-12);
        let inactive = |options| {
            ridges::RidgeField::new(&field.boundaries, field.width, 42, options)
                .features()
                .count()
        };
        assert_eq!(
            inactive(TectonicOptions {
                branching: 0.0,
                ..options
            }),
            0
        );
        assert_eq!(
            inactive(TectonicOptions {
                branch_depth: 0,
                ..options
            }),
            0
        );
        let shallow = inactive(TectonicOptions {
            branch_depth: 1,
            ..options
        });
        assert_eq!(count, shallow * 7);
        assert_eq!(
            inactive(TectonicOptions {
                deformation: 0.0,
                ..options
            }),
            0
        );
        let separating = chain_fixture(-0.25);
        assert_eq!(
            ridges::RidgeField::new(&separating.boundaries, field.width, 42, options)
                .features()
                .count(),
            0
        );
    }

    #[test]
    fn compression_forms_a_long_narrow_chain_with_foothills_and_no_opposite_ghost() {
        let field = chain_fixture(0.25);
        let cross_section = |angle: f64| {
            field
                .sample(DVec3::new(angle.sin(), 0.0, angle.cos()))
                .height
        };
        for angle in [-0.3_f64, -0.1, 0.1, 0.3] {
            let p = DVec3::new(0.0, angle.sin(), angle.cos());
            assert!(field.sample(p).height > 0.6, "crest lost along the chain");
        }
        assert!(cross_section(0.0) > cross_section(0.03));
        assert!(cross_section(0.03) > cross_section(0.06));
        assert!(
            cross_section(0.12) < 0.081,
            "mountain uplift spread into the interior"
        );
        assert!(
            field.sample(-DVec3::Z).height < 0.081,
            "finite edge became a whole great-circle ridge"
        );
    }

    #[test]
    fn ruggedness_creates_linked_summits_saddles_and_bends_instead_of_a_wall() {
        let mut field = chain_fixture(0.25);
        field.boundaries[0].peaks = vec![
            Peak {
                height: 1.0,
                bend: -0.5,
            },
            Peak {
                height: 0.1,
                bend: 0.5,
            },
            Peak {
                height: 1.0,
                bend: -0.5,
            },
        ];
        field.boundaries[0].envelope_sin = (field.width * 6.2).sin();
        let crest = |field: &TectonicField, t: f64| {
            let edge = &field.boundaries[0];
            let bend = field.width * field.ruggedness * edge.shape(t).bend;
            field
                .sample(edge.point(edge.length * t) * bend.cos() + edge.normal * bend.sin())
                .height
        };
        assert!((crest(&field, 0.1) - crest(&field, 0.5)).abs() < 1.0e-12);
        field.ruggedness = 1.0;
        assert!(crest(&field, 0.0) > crest(&field, 0.5) + 0.4);
        assert!(crest(&field, 1.0) > crest(&field, 0.5) + 0.4);
    }

    #[test]
    fn belts_cover_real_finite_voronoi_edges_across_plate_counts_and_drift() {
        for plates in [4, 16, 64] {
            for drift in [0.0, 0.25, 1.0] {
                let field = TectonicField::new(
                    42,
                    TectonicOptions {
                        plates,
                        drift,
                        ..TectonicOptions::default()
                    },
                );
                assert_eq!(field.boundaries.len(), 3 * plates as usize - 6);
                for edge in &field.boundaries {
                    let point = edge.point(edge.length * 0.5);
                    let mut scores: Vec<_> =
                        field.plates.iter().map(|p| p.centre.dot(point)).collect();
                    scores.sort_unstable_by(|a, b| b.total_cmp(a));
                    assert!((scores[0] - scores[1]).abs() < 1.0e-8);
                    assert!(edge.length > 0.0 && edge.length <= std::f64::consts::TAU);
                }
            }
        }
    }

    #[test]
    fn boundary_motion_has_the_correct_sign_and_ignores_parallel_slip() {
        let centre = |x| DVec3::new(x, 0.0, 1.0).normalize();
        let mut plates = [
            Plate {
                centre: centre(-0.3),
                omega: DVec3::Y,
                continental: true,
            },
            Plate {
                centre: centre(0.3),
                omega: -DVec3::Y,
                continental: false,
            },
        ];
        let compression = relative_closing(plates[0], plates[1], DVec3::Z);
        assert!(compression > 0.1);
        for plate in &mut plates {
            plate.omega = -plate.omega;
        }
        assert!((relative_closing(plates[0], plates[1], DVec3::Z) + compression).abs() < 1.0e-12);
        plates[0].omega = DVec3::X;
        plates[1].omega = -DVec3::X;
        assert!(relative_closing(plates[0], plates[1], DVec3::Z).abs() < 1.0e-12);
        assert!((relative_shear(plates[0], plates[1], DVec3::Z) - 2.0).abs() < 1.0e-12);
        plates[1].omega = plates[0].omega;
        assert!(relative_shear(plates[0], plates[1], DVec3::Z) < 1.0e-12);
        assert!(relative_closing(plates[0], plates[1], DVec3::Z).abs() < 1.0e-12);
    }

    #[test]
    fn drift_moves_crust_and_preserves_tangential_rigid_plate_velocities() {
        let options = TectonicOptions::default();
        let history = drift_history(42, options);
        for (first, last) in history[0].iter().zip(history.last().unwrap()) {
            assert!(first.centre.distance(last.centre) > 0.05);
            assert!(first.omega.cross(first.centre).dot(first.centre).abs() < 1.0e-12);
            assert!((last.centre.length() - 1.0).abs() < 1.0e-12);
        }
        let still = TectonicField::new(
            42,
            TectonicOptions {
                drift: 0.0,
                ..options
            },
        );
        assert!(
            history
                .last()
                .unwrap()
                .iter()
                .zip(&still.plates)
                .any(|(a, b)| a.centre.distance(b.centre) > 0.05)
        );
    }

    #[test]
    fn field_is_seeded_continuous_bounded_and_contains_both_crust_types() {
        let field = TectonicField::new(42, TectonicOptions::default());
        let again = TectonicField::new(42, TectonicOptions::default());
        let mut land = 0;
        let mut sea = 0;
        for i in 0..2048 {
            let y = 1.0 - (f64::from(i) + 0.5) / 1024.0;
            let phi = f64::from(i) * 2.399_963_229_728_653;
            let r = (1.0 - y * y).sqrt();
            let p = DVec3::new(r * phi.cos(), y, r * phi.sin());
            let sample = field.sample(p);
            assert_eq!(sample.height.to_bits(), again.sample(p).height.to_bits());
            assert!(sample.height.is_finite() && sample.height.abs() < 1.0);
            assert!((0.05..=0.95).contains(&sample.hardness));
            land += u32::from(sample.height > 0.0);
            sea += u32::from(sample.height < 0.0);
            let nearby = field.sample((p + DVec3::X * 1.0e-8).normalize());
            assert!((sample.height - nearby.height).abs() < 1.0e-5);
        }
        assert!(land > 250 && sea > 800, "land={land}, sea={sea}");
    }

    #[test]
    fn more_steps_refine_the_same_history_instead_of_accumulating_extra_uplift() {
        let coarse = TectonicField::new(
            42,
            TectonicOptions {
                steps: 12,
                ..TectonicOptions::default()
            },
        );
        let fine = TectonicField::new(
            42,
            TectonicOptions {
                steps: 96,
                ..TectonicOptions::default()
            },
        );
        for p in [DVec3::X, DVec3::Y, DVec3::Z, -DVec3::X, -DVec3::Z] {
            assert!((coarse.sample(p).height - fine.sample(p).height).abs() < 0.001);
            assert!((coarse.sample(p).hardness - fine.sample(p).hardness).abs() < 0.001);
        }
    }

    #[test]
    fn compression_raises_and_hardens_while_extension_lowers_and_weakens() {
        let mut plates = vec![
            Plate {
                centre: DVec3::new(-0.3, 0.0, 1.0).normalize(),
                omega: DVec3::Y,
                continental: true,
            },
            Plate {
                centre: DVec3::new(0.3, 0.0, 1.0).normalize(),
                omega: -DVec3::Y,
                continental: true,
            },
        ];
        let sample = |plates: &[Plate]| {
            let mut field = chain_fixture(relative_closing(plates[0], plates[1], DVec3::Z));
            field.plates = plates.to_vec();
            field.sample(DVec3::Z)
        };
        let pushed = sample(&plates);
        for plate in &mut plates {
            plate.omega = -plate.omega;
        }
        let pulled = sample(&plates);
        for plate in &mut plates {
            plate.omega = DVec3::ZERO;
        }
        let neutral = sample(&plates);
        assert!(pushed.height > neutral.height && neutral.height > pulled.height);
        assert!(pushed.hardness > neutral.hardness && neutral.hardness > pulled.hardness);
    }
}
