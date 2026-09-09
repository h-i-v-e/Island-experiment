use super::terrain_height;
use crate::{ISLAND_WORLD_METRES, Vec2, terrain::Terrain};
use serde::{Deserialize, Serialize};

/// Shared surface lattice at the replacement boundary. The voxel spacing is
/// rounded to a collider-cell multiple so Unity's hole mask covers exactly it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SurfaceGrid {
    pub minimum: Vec2,
    pub step: f32,
    pub width_count: usize,
    pub height_count: usize,
    pub heights: Vec<f32>,
}

impl SurfaceGrid {
    pub(crate) fn new(
        terrain: &Terrain,
        minimum: Vec2,
        maximum: Vec2,
        voxel: f32,
    ) -> Result<Self, String> {
        let cell = ISLAND_WORLD_METRES / 8192.0;
        let step = (voxel / cell).round().max(1.0) * cell;
        let minimum = (minimum / step).floor() * step;
        let maximum = (maximum / step).ceil() * step;
        let size = ((maximum - minimum) / step).round();
        let width_count = size.x as usize + 1;
        let height_count = size.y as usize + 1;
        if width_count > 1024 || height_count > 1024 || width_count * height_count > 131_072 {
            return Err("cave replacement surface exceeds sampling budget".into());
        }
        let heights = (0..height_count)
            .flat_map(|y| {
                (0..width_count).map(move |x| {
                    terrain_height(terrain, minimum + Vec2::new(x as f32, y as f32) * step)
                })
            })
            .collect();
        Ok(Self {
            minimum,
            step,
            width_count,
            height_count,
            heights,
        })
    }

    pub(crate) fn maximum(&self) -> Vec2 {
        self.minimum
            + Vec2::new(
                (self.width_count - 1) as f32,
                (self.height_count - 1) as f32,
            ) * self.step
    }

    pub(crate) fn validate(&self) -> Result<(), String> {
        if !self.minimum.is_finite()
            || !self.step.is_finite()
            || self.step < 0.2
            || self.step > 1.1
            || self.width_count < 2
            || self.height_count < 2
            || self.width_count > 1024
            || self.height_count > 1024
            || self.width_count * self.height_count != self.heights.len()
            || self.heights.len() > 131_072
            || self.heights.iter().any(|h| !h.is_finite())
        {
            return Err("invalid serialized cave surface lattice".into());
        }
        Ok(())
    }

    #[allow(clippy::many_single_char_names)]
    pub(crate) fn height(&self, p: Vec2) -> f32 {
        let q = ((p - self.minimum) / self.step).clamp(
            Vec2::ZERO,
            Vec2::new(
                (self.width_count - 1) as f32,
                (self.height_count - 1) as f32,
            ),
        );
        let x = (q.x as usize).min(self.width_count - 2);
        let y = (q.y as usize).min(self.height_count - 2);
        let tx = q.x - x as f32;
        let ty = q.y - y as f32;
        let a = self.heights[y * self.width_count + x];
        let b = self.heights[y * self.width_count + x + 1];
        let c = self.heights[(y + 1) * self.width_count + x];
        let d = self.heights[(y + 1) * self.width_count + x + 1];
        (a + (b - a) * tx) * (1.0 - ty) + (c + (d - c) * tx) * ty
    }
}
