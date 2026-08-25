//! Authoritative, deterministic height-field evaluation for procedural textures.
//!
//! The material modules deliberately work on this small, engine-neutral field
//! type.  A field is sampled at pixel centres and is periodic in both axes;
//! all derived maps therefore use the same unquantized values and the same
//! wrapping rules.  The public constructors take plain scalar arguments so
//! callers that use the recipe/image layer do not need to know about a
//! renderer-specific image type.

#![allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
#![allow(
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::missing_errors_doc,
    clippy::missing_panics_doc
)]

use super::{cellular, cracked_stone, noise, periodic, recipe, rounded_stones};

/// A finite, periodic texture extent in pixels and physical metres.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FieldDimensions {
    pub width: u32,
    pub height: u32,
    pub tile_width: f32,
    pub tile_height: f32,
}

impl FieldDimensions {
    /// Creates dimensions after checking the arithmetic needed by an owned
    /// field.  Zero dimensions and non-positive/non-finite physical extents
    /// are rejected because they cannot describe a useful periodic domain.
    pub fn new(
        width: u32,
        height: u32,
        tile_width: f32,
        tile_height: f32,
    ) -> Result<Self, FieldError> {
        let pixel_count = usize::try_from(width)
            .ok()
            .and_then(|width| {
                usize::try_from(height)
                    .ok()
                    .and_then(|height| width.checked_mul(height))
            })
            .ok_or(FieldError::DimensionOverflow)?;
        if pixel_count == 0 {
            return Err(FieldError::ZeroDimensions);
        }
        if !tile_width.is_finite()
            || !tile_height.is_finite()
            || tile_width <= 0.0
            || tile_height <= 0.0
        {
            return Err(FieldError::InvalidTileSize);
        }
        Ok(Self {
            width,
            height,
            tile_width,
            tile_height,
        })
    }

    #[must_use]
    pub const fn pixel_count(self) -> usize {
        self.width as usize * self.height as usize
    }

    #[must_use]
    pub fn pixel_size(self) -> (f32, f32) {
        (
            self.tile_width / self.width as f32,
            self.tile_height / self.height as f32,
        )
    }

    /// Converts the pixel extent to the shared engine-neutral image type.
    #[must_use]
    pub fn texture_dimensions(self) -> super::image::TextureDimensions {
        super::image::TextureDimensions::new(self.width, self.height)
            .expect("field dimensions were validated")
    }
}

/// Errors raised while constructing or evaluating a periodic field.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FieldError {
    ZeroDimensions,
    DimensionOverflow,
    InvalidTileSize,
    NonFiniteParameter,
}

/// A fully owned linear height field in metres.
#[derive(Clone, Debug, PartialEq)]
pub struct HeightField {
    dimensions: FieldDimensions,
    values: Vec<f32>,
}

impl HeightField {
    pub fn new(dimensions: FieldDimensions, values: Vec<f32>) -> Result<Self, FieldError> {
        if values.len() != dimensions.pixel_count() {
            return Err(FieldError::DimensionOverflow);
        }
        if values.iter().any(|value| !value.is_finite()) {
            return Err(FieldError::NonFiniteParameter);
        }
        Ok(Self { dimensions, values })
    }

    #[must_use]
    pub const fn dimensions(&self) -> FieldDimensions {
        self.dimensions
    }

    #[must_use]
    pub fn values(&self) -> &[f32] {
        &self.values
    }

    #[must_use]
    pub fn values_mut(&mut self) -> &mut [f32] {
        &mut self.values
    }

    #[must_use]
    pub fn into_values(self) -> Vec<f32> {
        self.values
    }

    #[must_use]
    pub fn at(&self, x: u32, y: u32) -> f32 {
        let x = x % self.dimensions.width;
        let y = y % self.dimensions.height;
        self.values[y as usize * self.dimensions.width as usize + x as usize]
    }

    #[must_use]
    pub fn sample_wrapped(&self, x: i32, y: i32) -> f32 {
        self.at(
            wrap_index(x, self.dimensions.width),
            wrap_index(y, self.dimensions.height),
        )
    }

    /// Samples the field using periodic bilinear interpolation.  Coordinates
    /// are in pixel units, with integer coordinates referring to pixel
    /// centres.  This helper is useful to adapters and AO implementations;
    /// it is intentionally independent of any GPU or image crate.
    #[must_use]
    pub fn sample_bilinear_wrapped(&self, x: f32, y: f32) -> f32 {
        let x0 = x.floor() as i32;
        let y0 = y.floor() as i32;
        let tx = x - x.floor();
        let ty = y - y.floor();
        let top = lerp(
            self.sample_wrapped(x0, y0),
            self.sample_wrapped(x0 + 1, y0),
            tx,
        );
        let bottom = lerp(
            self.sample_wrapped(x0, y0 + 1),
            self.sample_wrapped(x0 + 1, y0 + 1),
            tx,
        );
        lerp(top, bottom, ty)
    }
}

/// The field pass selected by a texture recipe.
#[derive(Clone, Debug, PartialEq)]
pub enum FieldProgram {
    /// A small, general-purpose fBM field for material experiments.
    Layered(LayeredField),
    /// Connected, bevelled cellular slabs with branching fractures.
    CrackedStone(cracked_stone::CrackedStoneConfig),
    /// Separated, rounded river stones and a low sand/silt floor.
    RoundedStones(rounded_stones::RoundedStonesConfig),
}

impl FieldProgram {
    /// Evaluates one complete deterministic field pass.
    pub fn evaluate(
        &self,
        dimensions: FieldDimensions,
        seed: u64,
    ) -> Result<HeightField, FieldError> {
        let values = match self {
            Self::Layered(config) => evaluate_layered(*config, dimensions, seed),
            Self::CrackedStone(config) => {
                cracked_stone::generate_height_values(*config, dimensions, seed)
            }
            Self::RoundedStones(config) => {
                rounded_stones::generate_height_values(*config, dimensions, seed)
            }
        }?;
        HeightField::new(dimensions, values)
    }

    /// Builds and evaluates a field directly from the versioned recipe
    /// material variant, then folds its optional surface layers in order.
    /// Recipe validation remains the responsibility of the recipe boundary;
    /// this method still rejects non-finite or structurally unsafe values
    /// before allocating output pixels.
    pub fn evaluate_recipe(recipe: &recipe::TextureRecipe) -> Result<HeightField, FieldError> {
        let dimensions = FieldDimensions::new(
            recipe.width,
            recipe.height,
            recipe.physical_tile_width_m,
            recipe.physical_tile_height_m,
        )?;
        let program = match &recipe.material {
            recipe::MaterialModel::LayeredNoise {
                frequency,
                amplitude,
                octaves,
                lacunarity,
                gain,
                offset,
            } => Self::Layered(LayeredField {
                frequency: *frequency,
                amplitude: *amplitude,
                octaves: *octaves,
                lacunarity: *lacunarity,
                gain: *gain,
                offset: *offset,
            }),
            recipe::MaterialModel::CrackedStone {
                cells_x,
                cells_y,
                cell_jitter,
                warp_amplitude,
                crack_width,
                shoulder_width,
                crack_depth,
                slab_variation,
                fracture_probability,
                fracture_depth,
                surface_amplitude,
                broad_variation,
            } => Self::CrackedStone(cracked_stone::CrackedStoneConfig {
                cells_x: *cells_x,
                cells_y: *cells_y,
                cell_jitter: *cell_jitter,
                warp_amplitude: *warp_amplitude,
                crack_width: *crack_width,
                shoulder_width: *shoulder_width,
                crack_depth: *crack_depth,
                slab_variation: *slab_variation,
                fracture_probability: *fracture_probability,
                fracture_depth: *fracture_depth,
                surface_amplitude: *surface_amplitude,
                broad_variation: *broad_variation,
            }),
            recipe::MaterialModel::RoundedStones {
                cells_x,
                cells_y,
                stone_radius,
                cell_jitter,
                warp_amplitude,
                anisotropy,
                stone_height,
                stone_variation,
                gap_height,
                sand_amplitude,
                edge_softness,
            } => Self::RoundedStones(rounded_stones::RoundedStonesConfig {
                cells_x: *cells_x,
                cells_y: *cells_y,
                stone_radius: *stone_radius,
                cell_jitter: *cell_jitter,
                warp_amplitude: *warp_amplitude,
                anisotropy: *anisotropy,
                stone_height: *stone_height,
                stone_variation: *stone_variation,
                gap_height: *gap_height,
                sand_amplitude: *sand_amplitude,
                edge_softness: *edge_softness,
            }),
        };
        let mut field = program.evaluate(dimensions, recipe.seed)?;
        apply_surface_layers(&mut field, &recipe.surface_layers, recipe.seed)?;
        Ok(field)
    }
}

/// Parameters for [`FieldProgram::Layered`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LayeredField {
    pub frequency: f32,
    pub amplitude: f32,
    pub octaves: u8,
    pub lacunarity: f32,
    pub gain: f32,
    pub offset: f32,
}

impl Default for LayeredField {
    fn default() -> Self {
        Self {
            frequency: 1.0,
            amplitude: 1.0,
            octaves: 4,
            lacunarity: 2.0,
            gain: 0.5,
            offset: 0.0,
        }
    }
}

fn evaluate_layered(
    config: LayeredField,
    dimensions: FieldDimensions,
    seed: u64,
) -> Result<Vec<f32>, FieldError> {
    if !config.frequency.is_finite()
        || !config.amplitude.is_finite()
        || !config.lacunarity.is_finite()
        || !config.gain.is_finite()
        || !config.offset.is_finite()
        || config.frequency <= 0.0
        || config.lacunarity <= 0.0
        || config.octaves == 0
        || config.octaves > 16
    {
        return Err(FieldError::NonFiniteParameter);
    }
    let mut values = vec![0.0; dimensions.pixel_count()];
    for y in 0..dimensions.height {
        for x in 0..dimensions.width {
            // Pixel centres are expressed in tile-space; the integer period
            // is carried by the trigonometric lattice coordinates below.
            let u = (x as f32 + 0.5) / dimensions.width as f32;
            let v = (y as f32 + 0.5) / dimensions.height as f32;
            values[y as usize * dimensions.width as usize + x as usize] =
                fbm(seed, u, v, config) + config.offset;
        }
    }
    Ok(values)
}

/// Stable 64-bit integer mixer shared by the material passes.  It is kept
/// local to this module so generated bytes never depend on a hash-map or
/// platform random-number implementation.
#[must_use]
pub(crate) fn hash_u64(value: u64) -> u64 {
    periodic::mix64(value)
}

#[must_use]
pub(crate) fn hash_unit(seed: u64, x: i32, y: i32, domain: u64) -> f32 {
    let key = seed
        ^ domain.rotate_left(21)
        ^ u64::from(x.cast_unsigned()).wrapping_mul(0x9e37_79b9_7f4a_7c15)
        ^ u64::from(y.cast_unsigned()).wrapping_mul(0x6a09_e667_f3bc_c909);
    periodic::hash_to_unit(hash_u64(key))
}

#[must_use]
pub(crate) fn hash_signed(seed: u64, x: i32, y: i32, domain: u64) -> f32 {
    hash_unit(seed, x, y, domain) * 2.0 - 1.0
}

/// Periodic interpolated value noise. `period_x` and `period_y` are lattice
/// periods, not output pixel dimensions.
#[must_use]
pub(crate) fn periodic_value(
    seed: u64,
    x: f32,
    y: f32,
    period_x: i32,
    period_y: i32,
    domain: u64,
) -> f32 {
    let period = periodic::Period2D::new(period_x.max(1) as u32, period_y.max(1) as u32)
        .expect("positive lattice period");
    periodic::value_noise(seed ^ domain, [x, y], period)
}

#[must_use]
pub(crate) fn fbm(seed: u64, u: f32, v: f32, config: LayeredField) -> f32 {
    let frequency = config.frequency.round().max(1.0);
    let period = periodic::Period2D::new(frequency as u32, frequency as u32)
        .unwrap_or_else(|_| periodic::Period2D::new_unchecked(1, 1));
    noise::fbm(
        seed,
        [u * frequency, v * frequency],
        period,
        config.octaves,
        config.lacunarity,
        config.gain,
    ) * config.amplitude
}

#[must_use]
pub(crate) fn smoothstep(edge0: f32, edge1: f32, value: f32) -> f32 {
    if (edge1 - edge0).abs() <= f32::EPSILON {
        return f32::from(value >= edge1);
    }
    let t = ((value - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

#[must_use]
pub(crate) fn lerp(a: f32, b: f32, amount: f32) -> f32 {
    (b - a).mul_add(amount, a)
}

#[must_use]
pub(crate) fn wrap_index(value: i32, period: u32) -> u32 {
    debug_assert!(period > 0);
    value.rem_euclid(period as i32) as u32
}

fn apply_surface_layers(
    field: &mut HeightField,
    layers: &[recipe::NoiseLayer],
    seed: u64,
) -> Result<(), FieldError> {
    if layers.len() > recipe::MAX_SURFACE_LAYERS {
        return Err(FieldError::NonFiniteParameter);
    }
    let dimensions = field.dimensions();
    for layer in layers {
        if !layer.frequency.is_finite()
            || !layer.amplitude.is_finite()
            || layer.frequency <= 0.0
            || !layer.offset.iter().all(|value| value.is_finite())
        {
            return Err(FieldError::NonFiniteParameter);
        }
        let frequency = layer.frequency.round().max(1.0);
        let frequency =
            u32::try_from(frequency as u64).map_err(|_| FieldError::NonFiniteParameter)?;
        let period = periodic::Period2D::new(frequency, frequency)
            .map_err(|_| FieldError::NonFiniteParameter)?;
        for y in 0..dimensions.height {
            let v = (y as f32 + 0.5) / dimensions.height as f32;
            for x in 0..dimensions.width {
                let u = (x as f32 + 0.5) / dimensions.width as f32;
                let value = sample_noise_layer(layer, seed, [u, v], period)? * layer.amplitude;
                let index = y as usize * dimensions.width as usize + x as usize;
                let previous = field.values[index];
                field.values[index] = match &layer.blend {
                    recipe::BlendOperation::Replace => value,
                    recipe::BlendOperation::Add => previous + value,
                    recipe::BlendOperation::Subtract => previous - value,
                    recipe::BlendOperation::Multiply => previous * value,
                    recipe::BlendOperation::Minimum => previous.min(value),
                    recipe::BlendOperation::Maximum => previous.max(value),
                    recipe::BlendOperation::Lerp { amount } => {
                        if !amount.is_finite() {
                            return Err(FieldError::NonFiniteParameter);
                        }
                        lerp(previous, value, amount.clamp(0.0, 1.0))
                    }
                    recipe::BlendOperation::LerpByMask { mask } => {
                        let mask_value = sample_noise_layer(mask, seed, [u, v], period)?;
                        lerp(previous, value, mask_value.clamp(0.0, 1.0))
                    }
                };
            }
        }
    }
    if field.values.iter().any(|value| !value.is_finite()) {
        return Err(FieldError::NonFiniteParameter);
    }
    Ok(())
}

fn sample_noise_layer(
    layer: &recipe::NoiseLayer,
    seed: u64,
    uv: [f32; 2],
    period: periodic::Period2D,
) -> Result<f32, FieldError> {
    let frequency = layer.frequency.round().max(1.0);
    let mut position = [
        uv[0] * frequency + layer.offset[0] * frequency,
        uv[1] * frequency + layer.offset[1] * frequency,
    ];
    let layer_seed = seed ^ layer.seed_domain;
    if let Some(warp) = layer.domain_warp {
        if !warp.amplitude.is_finite()
            || !warp.frequency.is_finite()
            || !warp.lacunarity.is_finite()
            || !warp.gain.is_finite()
            || warp.frequency <= 0.0
        {
            return Err(FieldError::NonFiniteParameter);
        }
        position = noise::domain_warp(
            layer_seed ^ warp.seed_domain,
            position,
            period,
            warp.amplitude,
            warp.frequency,
            warp.octaves,
            warp.lacunarity,
            warp.gain,
        );
    }
    sample_noise_kind(&layer.kind, layer_seed, position, period, layer)
}

fn sample_noise_kind(
    kind: &recipe::NoiseKind,
    seed: u64,
    position: [f32; 2],
    period: periodic::Period2D,
    layer: &recipe::NoiseLayer,
) -> Result<f32, FieldError> {
    let value = match kind {
        recipe::NoiseKind::Value => noise::value(seed, position, period),
        recipe::NoiseKind::Fbm => noise::fbm(
            seed,
            position,
            period,
            layer.octaves,
            layer.lacunarity,
            layer.gain,
        ),
        recipe::NoiseKind::Billow => noise::billow(
            seed,
            position,
            period,
            layer.octaves,
            layer.lacunarity,
            layer.gain,
        ),
        recipe::NoiseKind::Ridged => noise::ridged(
            seed,
            position,
            period,
            layer.octaves,
            layer.lacunarity,
            layer.gain,
        ),
        recipe::NoiseKind::CellularDistance => {
            cellular::sample(seed, position, period, layer.cellular_jitter).nearest_distance
        }
        recipe::NoiseKind::CellularDistanceToEdge => {
            cellular::sample(seed, position, period, layer.cellular_jitter).edge_distance
        }
        recipe::NoiseKind::CellularValue => {
            cellular::sample(seed, position, period, layer.cellular_jitter).cell_value
        }
        recipe::NoiseKind::DomainWarp {
            source,
            warp,
            amplitude,
        } => {
            if !amplitude.is_finite() {
                return Err(FieldError::NonFiniteParameter);
            }
            let displacement =
                sample_noise_kind(warp, seed ^ 0x44_4f_4d_41_49_4e, position, period, layer)?;
            let warped = [
                position[0] + displacement * amplitude,
                position[1] + displacement * amplitude,
            ];
            sample_noise_kind(source, seed ^ 0x53_4f_55_52_43_45, warped, period, layer)?
        }
    };
    if value.is_finite() {
        Ok(value)
    } else {
        Err(FieldError::NonFiniteParameter)
    }
}

#[cfg(test)]
#[allow(clippy::float_cmp)]
mod tests {
    use super::*;

    #[test]
    fn dimensions_reject_invalid_domains() {
        assert_eq!(
            FieldDimensions::new(0, 4, 1.0, 1.0),
            Err(FieldError::ZeroDimensions)
        );
        assert_eq!(
            FieldDimensions::new(4, 4, 0.0, 1.0),
            Err(FieldError::InvalidTileSize)
        );
        assert_eq!(
            FieldDimensions::new(4, 4, f32::NAN, 1.0),
            Err(FieldError::InvalidTileSize)
        );
    }

    #[test]
    fn hash_is_stable_and_coordinate_sensitive() {
        assert_eq!(hash_u64(0), 0);
        assert_eq!(hash_u64(1), 0x5692_161d_100b_05e5);
        assert_ne!(hash_signed(9, 0, 0, 2), hash_signed(9, 1, 0, 2));
    }

    #[test]
    fn periodic_value_matches_at_lattice_period() {
        let a = periodic_value(44, 0.63, 1.17, 8, 7, 3);
        let b = periodic_value(44, 8.63, 8.17, 8, 7, 3);
        assert!((a - b).abs() < 1.0e-6);
    }

    #[test]
    fn layered_field_is_deterministic_and_finite() {
        let dimensions = FieldDimensions::new(17, 13, 4.0, 4.0).expect("valid dimensions");
        let program = FieldProgram::Layered(LayeredField::default());
        let a = program.evaluate(dimensions, 99).expect("field");
        let b = program.evaluate(dimensions, 99).expect("field");
        assert_eq!(a, b);
        assert!(a.values().iter().all(|value| value.is_finite()));
    }

    #[test]
    fn wrapped_field_sampling_uses_opposite_edges() {
        let dimensions = FieldDimensions::new(3, 2, 1.0, 1.0).expect("valid dimensions");
        let field =
            HeightField::new(dimensions, vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0]).expect("field");
        assert_eq!(field.sample_wrapped(-1, 0), 3.0);
        assert_eq!(field.sample_wrapped(0, -1), 4.0);
    }
}
