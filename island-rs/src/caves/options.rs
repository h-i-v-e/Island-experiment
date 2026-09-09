use serde::{Deserialize, Serialize};

/// Additive C ABI block. Dimensions are metres; angles are degrees.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[repr(C)]
pub struct CaveOptions {
    pub enabled: u32,
    pub seed_offset: u32,
    pub maximum_caves: u32,
    pub candidate_limit: u32,
    pub spacing: f32,
    pub entrance_width: f32,
    pub entrance_height: f32,
    pub minimum_face_slope: f32,
    pub minimum_face_height: f32,
    pub approach_slope: f32,
    pub approach_length: f32,
    pub side_margin: f32,
    pub approach_step: f32,
    pub route_radius: f32,
    pub roof_cover: f32,
    pub side_cover: f32,
    pub transition_length: f32,
    pub length_min: f32,
    pub length_max: f32,
    pub width_min: f32,
    pub width_max: f32,
    pub height_min: f32,
    pub height_max: f32,
    pub floor_slope: f32,
    pub chamber_width: f32,
    pub chamber_height: f32,
    pub broad_amplitude: f32,
    pub broad_period: f32,
    pub fine_amplitude: f32,
    pub fine_period: f32,
    pub floor_roughness: f32,
    pub sea_clearance: f32,
    pub voxel_size: f32,
}

impl Default for CaveOptions {
    fn default() -> Self {
        Self {
            enabled: 0,
            seed_offset: 0,
            maximum_caves: 1,
            candidate_limit: 2048,
            spacing: 100.0,
            entrance_width: 4.0,
            entrance_height: 3.0,
            minimum_face_slope: 60.0,
            minimum_face_height: 6.0,
            approach_slope: 15.0,
            approach_length: 6.0,
            side_margin: 1.0,
            approach_step: 0.2,
            route_radius: 20.0,
            roof_cover: 3.0,
            side_cover: 3.0,
            transition_length: 6.0,
            length_min: 30.0,
            length_max: 60.0,
            width_min: 3.0,
            width_max: 5.0,
            height_min: 3.0,
            height_max: 4.0,
            floor_slope: 12.0,
            chamber_width: 10.0,
            chamber_height: 6.0,
            broad_amplitude: 0.5,
            broad_period: 8.0,
            fine_amplitude: 0.15,
            fine_period: 2.0,
            floor_roughness: 0.08,
            sea_clearance: 5.0,
            voxel_size: 0.5,
        }
    }
}

impl CaveOptions {
    /// # Errors
    /// Rejects non-finite, inconsistent or unbounded generation settings.
    pub fn validate(self) -> Result<Self, String> {
        let positive = [
            self.spacing,
            self.entrance_width,
            self.entrance_height,
            self.minimum_face_height,
            self.approach_length,
            self.route_radius,
            self.roof_cover,
            self.side_cover,
            self.transition_length,
            self.length_min,
            self.length_max,
            self.width_min,
            self.width_max,
            self.height_min,
            self.height_max,
            self.chamber_width,
            self.chamber_height,
            self.broad_period,
            self.fine_period,
            self.voxel_size,
        ];
        let nonnegative = [
            self.side_margin,
            self.approach_step,
            self.broad_amplitude,
            self.fine_amplitude,
            self.floor_roughness,
            self.sea_clearance,
        ];
        if positive
            .iter()
            .any(|v| !v.is_finite() || *v <= 0.0 || *v > 1000.0)
            || nonnegative
                .iter()
                .any(|v| !v.is_finite() || *v < 0.0 || *v > 100.0)
            || [
                self.minimum_face_slope,
                self.approach_slope,
                self.floor_slope,
            ]
            .iter()
            .any(|v| !v.is_finite() || *v <= 0.0 || *v >= 89.0)
            || self.enabled > 1
            || self.maximum_caves > 4
            || self.candidate_limit == 0
            || self.candidate_limit > 4096
            || !(0.25..=1.0).contains(&self.voxel_size)
            || self.roof_cover > 32.0
            || self.side_cover > 32.0
            || self.route_radius > 64.0
            || self.approach_length > 32.0
            || self.side_margin > 8.0
            || self.chamber_width > 24.0
            || self.chamber_height > 20.0
            || self.entrance_width > 12.0
            || self.entrance_height > 12.0
            || self.transition_length >= self.length_min
            || self.broad_amplitude + self.fine_amplitude > 2.0
            || self.floor_roughness > 0.15
            || self.length_min > self.length_max
            || self.length_max > 120.0
            || self.width_min > self.width_max
            || self.height_min > self.height_max
            || self.width_min < 1.5
            || self.height_min < 2.0
            || self.entrance_width < 1.5
            || self.entrance_height < 2.0
            || self.chamber_width < self.width_max
            || self.chamber_height < self.height_max
            || self.broad_period < self.voxel_size * 4.0
            || self.fine_period < self.voxel_size * 4.0
            || self.floor_roughness > self.approach_step
        {
            return Err("invalid cave settings or generation limits".into());
        }
        Ok(self)
    }
}
