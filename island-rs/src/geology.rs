use crate::{Vec2, noise};

#[derive(Clone, Copy, Debug)]
pub(crate) struct TerrainNoiseSample {
    pub continental: f32,
    pub detail: f32,
}

#[must_use]
pub(crate) fn terrain_noise(seed: u64, position: Vec2) -> TerrainNoiseSample {
    TerrainNoiseSample {
        continental: noise::fractal(
            seed ^ 0x243f_6a88_85a3_08d3,
            position.x * 2.2,
            position.y * 2.2,
            5,
        ),
        detail: noise::fractal(
            seed ^ 0x1319_8a2e_0370_7344,
            position.x * 12.0,
            position.y * 12.0,
            4,
        ),
    }
}

impl TerrainNoiseSample {
    #[must_use]
    pub(crate) fn height_component(self) -> f32 {
        self.continental.mul_add(0.78, self.detail * 0.22)
    }

    fn material_component(self) -> f32 {
        self.height_component()
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct GeologyField {
    seed: u64,
    low_raw_hardness: f32,
    inverse_raw_range: f32,
}

impl GeologyField {
    #[must_use]
    pub(crate) fn calibrated(seed: u64, positions: impl Iterator<Item = Vec2>) -> Self {
        let mut samples: Vec<f32> = positions
            .map(|position| terrain_noise(seed, position).material_component())
            .collect();
        samples.sort_unstable_by(f32::total_cmp);
        let last = samples.len().saturating_sub(1);
        let low = samples.get(last * 8 / 100).copied().unwrap_or(-1.0);
        let high = samples.get(last * 92 / 100).copied().unwrap_or(1.0);
        Self {
            seed,
            low_raw_hardness: low,
            inverse_raw_range: 1.0 / (high - low).max(1.0e-5),
        }
    }

    #[must_use]
    pub(crate) fn hardness(self, position: Vec2) -> f32 {
        let normalized = ((terrain_noise(self.seed, position).material_component()
            - self.low_raw_hardness)
            * self.inverse_raw_range)
            .clamp(0.0, 1.0);
        let hardness = normalized * normalized * (3.0 - 2.0 * normalized);
        hardness * hardness * (3.0 - 2.0 * hardness)
    }
}
