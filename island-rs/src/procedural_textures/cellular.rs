//! Periodic jittered-cell (Voronoi/Worley-style) source fields.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::float_cmp,
    clippy::must_use_candidate
)]

use super::periodic::{Period2D, hash_2d, hash_to_signed, hash_to_unit, mix64};

const FEATURE_X_DOMAIN: u64 = 0x3c79_ac49_2ba7_b653;
const FEATURE_Y_DOMAIN: u64 = 0x1c69_b3f7_4ac4_ae35;
const CELL_VALUE_DOMAIN: u64 = 0x94d0_49bb_1331_11eb;

/// The cellular quantity requested by a recipe layer.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum CellularMetric {
    /// Euclidean distance to the nearest feature point.
    Distance,
    /// Half the gap between the two nearest feature-point distances, an
    /// inexpensive approximation of distance to a Voronoi cell edge.
    DistanceToEdge,
    /// Stable random value assigned to the nearest cell.
    CellValue,
}

/// The nearest-cell attributes retained for later material passes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CellularSample {
    /// Stable identifier for the nearest periodic cell.
    pub cell_id: u64,
    /// The nearest cell's integer x coordinate before periodic wrapping.
    pub cell_x: i64,
    /// The nearest cell's integer y coordinate before periodic wrapping.
    pub cell_y: i64,
    /// Distance to the nearest feature point.
    pub nearest_distance: f32,
    /// Distance to the second-nearest feature point.
    pub second_nearest_distance: f32,
    /// Approximate distance to the nearest Voronoi edge.
    pub edge_distance: f32,
    /// Stable signed value associated with the nearest cell.
    pub cell_value: f32,
}

impl CellularSample {
    /// Returns the selected scalar metric.
    #[inline]
    pub fn metric(self, metric: CellularMetric) -> f32 {
        match metric {
            CellularMetric::Distance => self.nearest_distance,
            CellularMetric::DistanceToEdge => self.edge_distance,
            CellularMetric::CellValue => self.cell_value,
        }
    }
}

/// Samples a periodic jittered feature-point field in lattice coordinates.
///
/// One feature point is generated for every integer cell.  The bounded
/// 3-by-3 neighbour stencil is sufficient for a jitter of at most one cell,
/// and wrapping the candidate cell coordinates before hashing makes cells
/// crossing either image edge continue on the opposite edge.
pub fn sample(seed: u64, position: [f32; 2], period: Period2D, jitter: f32) -> CellularSample {
    if !position[0].is_finite() || !position[1].is_finite() {
        return CellularSample {
            cell_id: 0,
            cell_x: 0,
            cell_y: 0,
            nearest_distance: 0.0,
            second_nearest_distance: 0.0,
            edge_distance: 0.0,
            cell_value: 0.0,
        };
    }

    let jitter = if jitter.is_finite() {
        jitter.clamp(0.0, 1.0)
    } else {
        0.0
    };
    // Work in one canonical tile copy.  Besides making the periodic contract
    // explicit, this avoids one-ulp differences from subtracting large but
    // equivalent absolute feature coordinates at opposite borders.
    let wrapped_position = [
        position[0].rem_euclid(period.x as f32),
        position[1].rem_euclid(period.y as f32),
    ];
    let base_x = wrapped_position[0].floor() as i64;
    let base_y = wrapped_position[1].floor() as i64;
    let mut nearest = Candidate::empty();
    let mut second = Candidate::empty();

    for offset_y in -1_i64..=1 {
        for offset_x in -1_i64..=1 {
            let cell_x = base_x + offset_x;
            let cell_y = base_y + offset_y;
            let feature = feature_point(seed, cell_x, cell_y, period, jitter);
            let dx = feature.x - wrapped_position[0];
            let dy = feature.y - wrapped_position[1];
            let distance_squared = dx.mul_add(dx, dy * dy);
            let candidate = Candidate {
                distance_squared,
                cell_x,
                cell_y,
                cell_id: feature.cell_id,
                cell_value: feature.cell_value,
            };

            if candidate.is_nearer_than(nearest) {
                second = nearest;
                nearest = candidate;
            } else if candidate.is_nearer_than(second) {
                second = candidate;
            }
        }
    }

    let nearest_distance = nearest.distance_squared.sqrt();
    let second_nearest_distance = second.distance_squared.sqrt();
    let edge_distance = ((second_nearest_distance - nearest_distance) * 0.5).max(0.0);
    CellularSample {
        cell_id: nearest.cell_id,
        cell_x: nearest.cell_x,
        cell_y: nearest.cell_y,
        nearest_distance,
        second_nearest_distance,
        edge_distance,
        cell_value: nearest.cell_value,
    }
}

/// Samples a cellular metric after scaling normalized tile coordinates by an
/// integer lattice frequency.
pub fn sample_with_frequency(
    seed: u64,
    position: [f32; 2],
    period: Period2D,
    frequency: f32,
    jitter: f32,
) -> CellularSample {
    let frequency = if frequency.is_finite() && frequency > 0.0 {
        frequency.round().max(1.0).min(u32::MAX as f32) as u32
    } else {
        1
    };
    let sample_period = period.checked_mul(frequency).unwrap_or(period);
    sample(
        seed,
        [
            position[0] * frequency as f32,
            position[1] * frequency as f32,
        ],
        sample_period,
        jitter,
    )
}

/// Returns nearest-feature distance for a periodic cellular field.
#[inline]
pub fn distance(seed: u64, position: [f32; 2], period: Period2D, jitter: f32) -> f32 {
    sample(seed, position, period, jitter).nearest_distance
}

/// Returns approximate distance to the nearest periodic cell edge.
#[inline]
pub fn distance_to_edge(seed: u64, position: [f32; 2], period: Period2D, jitter: f32) -> f32 {
    sample(seed, position, period, jitter).edge_distance
}

/// Returns the nearest periodic cell's stable random value.
#[inline]
pub fn cell_value(seed: u64, position: [f32; 2], period: Period2D, jitter: f32) -> f32 {
    sample(seed, position, period, jitter).cell_value
}

#[derive(Clone, Copy)]
struct FeaturePoint {
    x: f32,
    y: f32,
    cell_id: u64,
    cell_value: f32,
}

#[derive(Clone, Copy)]
struct Candidate {
    distance_squared: f32,
    cell_x: i64,
    cell_y: i64,
    cell_id: u64,
    cell_value: f32,
}

impl Candidate {
    const fn empty() -> Self {
        Self {
            distance_squared: f32::INFINITY,
            cell_x: 0,
            cell_y: 0,
            cell_id: 0,
            cell_value: 0.0,
        }
    }

    #[inline]
    fn is_nearer_than(self, other: Self) -> bool {
        self.distance_squared < other.distance_squared
            || (self.distance_squared == other.distance_squared && self.cell_id < other.cell_id)
    }
}

fn feature_point(
    seed: u64,
    cell_x: i64,
    cell_y: i64,
    period: Period2D,
    jitter: f32,
) -> FeaturePoint {
    let cell_hash = hash_2d(seed, cell_x, cell_y, period);
    let x_hash = mix64(cell_hash ^ FEATURE_X_DOMAIN);
    let y_hash = mix64(cell_hash ^ FEATURE_Y_DOMAIN);
    let x_offset = 0.5 + (hash_to_unit(x_hash) - 0.5) * jitter;
    let y_offset = 0.5 + (hash_to_unit(y_hash) - 0.5) * jitter;
    let value_hash = mix64(cell_hash ^ CELL_VALUE_DOMAIN);
    FeaturePoint {
        x: cell_x as f32 + x_offset,
        y: cell_y as f32 + y_offset,
        cell_id: cell_hash,
        cell_value: hash_to_signed(value_hash),
    }
}

#[cfg(test)]
mod tests {
    use super::{CellularMetric, cell_value, distance, distance_to_edge, sample};
    use crate::procedural_textures::periodic::Period2D;

    #[test]
    fn cellular_sample_wraps_across_both_axes() {
        let period = Period2D::new(17, 13).expect("non-empty period");
        let point = [3.125, 8.75];
        let original = sample(1234, point, period, 0.8);
        let wrapped_x = sample(1234, [point[0] + 17.0, point[1]], period, 0.8);
        let wrapped_y = sample(1234, [point[0], point[1] + 13.0], period, 0.8);
        assert_eq!(original.cell_id, wrapped_x.cell_id);
        assert_eq!(original.cell_id, wrapped_y.cell_id);
        assert_eq!(original.nearest_distance, wrapped_x.nearest_distance);
        assert_eq!(original.edge_distance, wrapped_y.edge_distance);
        assert_eq!(original.cell_value, wrapped_x.cell_value);
    }

    #[test]
    fn metrics_are_finite_and_bounded() {
        let period = Period2D::new(9, 11).expect("non-empty period");
        let sample = sample(99, [0.1, 10.9], period, 1.0);
        assert!(sample.nearest_distance.is_finite());
        assert!(sample.second_nearest_distance >= sample.nearest_distance);
        assert!(sample.edge_distance >= 0.0);
        assert!((-1.0..=1.0).contains(&sample.cell_value));
        assert_eq!(
            sample.metric(CellularMetric::Distance),
            sample.nearest_distance
        );
    }

    #[test]
    fn convenience_metrics_share_the_same_field() {
        let period = Period2D::new(8, 8).expect("non-empty period");
        let point = [2.75, 1.5];
        let sample = sample(7, point, period, 0.4);
        assert_eq!(distance(7, point, period, 0.4), sample.nearest_distance);
        assert_eq!(
            distance_to_edge(7, point, period, 0.4),
            sample.edge_distance
        );
        assert_eq!(cell_value(7, point, period, 0.4), sample.cell_value);
    }
}
