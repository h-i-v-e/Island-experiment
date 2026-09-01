use crate::{IslandOptions, Vec2, noise};

#[derive(Clone, Copy, Debug)]
pub(crate) struct TerrainNoiseSettings {
    continental_frequency: f32,
    continental_strength: f32,
    detail_frequency: f32,
    detail_strength: f32,
}

impl From<IslandOptions> for TerrainNoiseSettings {
    fn from(options: IslandOptions) -> Self {
        Self {
            continental_frequency: options.continental_noise_frequency,
            continental_strength: options.continental_noise_strength,
            detail_frequency: options.detail_noise_frequency,
            detail_strength: options.detail_noise_strength,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct TerrainNoiseSample {
    pub continental: f32,
    pub detail: f32,
}

#[must_use]
pub(crate) fn terrain_noise(
    seed: u64,
    position: Vec2,
    settings: TerrainNoiseSettings,
) -> TerrainNoiseSample {
    TerrainNoiseSample {
        continental: noise::fractal(
            seed ^ 0x243f_6a88_85a3_08d3,
            position.x * settings.continental_frequency,
            position.y * settings.continental_frequency,
            5,
        ),
        detail: noise::fractal(
            seed ^ 0x1319_8a2e_0370_7344,
            position.x * settings.detail_frequency,
            position.y * settings.detail_frequency,
            4,
        ),
    }
}

impl TerrainNoiseSample {
    #[must_use]
    pub(crate) fn height_component(self, settings: TerrainNoiseSettings) -> f32 {
        self.continental.mul_add(
            settings.continental_strength,
            self.detail * settings.detail_strength,
        )
    }

    fn material_component(self, settings: TerrainNoiseSettings) -> f32 {
        self.height_component(settings)
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct GeologyField {
    seed: u64,
    noise: TerrainNoiseSettings,
    low_raw_hardness: f32,
    inverse_raw_range: f32,
}

impl GeologyField {
    #[must_use]
    pub(crate) fn calibrated(
        seed: u64,
        noise: TerrainNoiseSettings,
        positions: impl Iterator<Item = Vec2>,
    ) -> Self {
        let mut samples: Vec<f32> = positions
            .map(|position| terrain_noise(seed, position, noise).material_component(noise))
            .collect();
        samples.sort_unstable_by(f32::total_cmp);
        let last = samples.len().saturating_sub(1);
        let low = samples.get(last * 8 / 100).copied().unwrap_or(-1.0);
        let high = samples.get(last * 92 / 100).copied().unwrap_or(1.0);
        Self {
            seed,
            noise,
            low_raw_hardness: low,
            inverse_raw_range: 1.0 / (high - low).max(1.0e-5),
        }
    }

    #[must_use]
    pub(crate) fn hardness(self, position: Vec2) -> f32 {
        let normalized = ((terrain_noise(self.seed, position, self.noise)
            .material_component(self.noise)
            - self.low_raw_hardness)
            * self.inverse_raw_range)
            .clamp(0.0, 1.0);
        let hardness = normalized * normalized * (3.0 - 2.0 * normalized);
        hardness * hardness * (3.0 - 2.0 * hardness)
    }
}

#[cfg(test)]
mod tests {
    use super::{GeologyField, TerrainNoiseSettings, terrain_noise};
    use crate::{IslandOptions, Vec2, noise};

    #[test]
    fn default_settings_preserve_the_original_height_field() {
        let seed = 42;
        let position = Vec2::new(0.31, 0.67);
        let settings = TerrainNoiseSettings::from(IslandOptions::default());
        let sample = terrain_noise(seed, position, settings);
        let expected = noise::fractal(
            seed ^ 0x243f_6a88_85a3_08d3,
            position.x * 2.2,
            position.y * 2.2,
            5,
        ) * 0.78
            + noise::fractal(
                seed ^ 0x1319_8a2e_0370_7344,
                position.x * 12.0,
                position.y * 12.0,
                4,
            ) * 0.22;

        assert!((sample.height_component(settings) - expected).abs() < f32::EPSILON);
    }

    #[test]
    fn zero_strength_noise_produces_finite_uniform_hardness() {
        let options = IslandOptions {
            continental_noise_strength: 0.0,
            detail_noise_strength: 0.0,
            ..IslandOptions::default()
        };
        let settings = TerrainNoiseSettings::from(options);
        let positions = [
            Vec2::new(0.1, 0.1),
            Vec2::new(0.9, 0.1),
            Vec2::new(0.1, 0.9),
            Vec2::new(0.9, 0.9),
        ];
        let geology = GeologyField::calibrated(42, settings, positions.into_iter());

        assert!(positions.into_iter().all(|position| {
            let hardness = geology.hardness(position);
            hardness.is_finite() && hardness.abs() < f32::EPSILON
        }));
    }
}
