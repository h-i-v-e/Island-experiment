//! Reduced, coupled planetary weather and wave-energy simulation.
//!
//! Atmospheric dynamics use an effective climate radius and accelerated clock;
//! wind is in m/s and wave transport uses the actual small planet's metres.
//! This is a deliberately low-order game model, not a forecasting GCM.
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss
)]

mod grid;
mod moisture;
mod optics;
mod physics;
#[cfg(test)]
mod tests;
mod waves;

use crate::{DVec3, PlanetTerrain, noise};
use grid::{Grid, basis, tangent};
pub use waves::{DIRECTIONS, WAVELENGTHS};

pub const LOWER: usize = 0;
pub const UPPER: usize = 1;
pub const CLIMATE: usize = 2;
pub const SWELL_PRIMARY: usize = 3;
pub const SWELL_SECONDARY: usize = 4;
pub const CLOUD_OPTICS: usize = 5;
pub const LAYERS: usize = 6;
const STEP: f64 = 0.25;

#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct WeatherSettings {
    pub face_size: u32,
    /// Atmospheric seconds per real second. Wave groups still travel in real metres/seconds.
    pub time_scale: f64,
    pub effective_radius: f64,
    pub rotation_period: f64,
    pub humidity: f64,
    pub solar_heating: f64,
}
impl Default for WeatherSettings {
    fn default() -> Self {
        Self {
            face_size: 64,
            time_scale: 600.0,
            effective_radius: 6_371_000.0,
            rotation_period: 86_400.0,
            humidity: 1.0,
            solar_heating: 1.0,
        }
    }
}
impl WeatherSettings {
    /// # Errors
    /// Rejects unbounded grids, non-finite parameters and unstable forcing scales.
    pub fn validate(self) -> Result<(), String> {
        if !(4..=128).contains(&self.face_size) {
            return Err("weather face size must be 4..128".into());
        }
        for (name, value, low, high) in [
            ("weather time scale", self.time_scale, 0.0, 3600.0),
            (
                "effective radius",
                self.effective_radius,
                1_000_000.0,
                20_000_000.0,
            ),
            ("rotation period", self.rotation_period, 3600.0, 1_000_000.0),
            ("humidity", self.humidity, 0.0, 2.0),
            ("solar heating", self.solar_heating, 0.0, 3.0),
        ] {
            if !value.is_finite() || !(low..=high).contains(&value) {
                return Err(format!("{name} must be {low}..{high}"));
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct WeatherSurface {
    pub size: u32,
    pub radius: f64,
    pub heights: Vec<f64>,
    /// Conservative local terrain ceiling for cloud placement, in metres.
    pub cloud_heights: Vec<f64>,
    pub land_fraction: Vec<f64>,
}
impl WeatherSurface {
    #[must_use]
    pub fn uniform(size: u32, height_metres: f64) -> Self {
        // Surface builders run before WeatherSimulation::new validates controls.
        // Reject invalid dimensions without attempting an unbounded allocation;
        // the resulting empty surface is rejected by the simulation boundary.
        if !(4..=128).contains(&size) {
            return Self {
                size,
                radius: 1000.0,
                heights: Vec::new(),
                cloud_heights: Vec::new(),
                land_fraction: Vec::new(),
            };
        }
        let count = 6 * size as usize * size as usize;
        Self {
            size,
            radius: 1000.0,
            heights: vec![height_metres; count],
            cloud_heights: vec![height_metres.max(0.0); count],
            land_fraction: vec![f64::from(u8::from(height_metres >= 0.0)); count],
        }
    }
    /// Coarse topography/land-fraction forcing, binned once on the worker.
    /// Fine shore attenuation remains the renderer's existing coastal map.
    #[must_use]
    pub fn from_terrain(terrain: &PlanetTerrain, size: u32) -> Self {
        let mut result = Self::uniform(size, -10.0);
        result.radius = terrain.topology.radius;
        if result.heights.is_empty() {
            return result;
        }
        let mut counts = vec![0_u32; result.heights.len()];
        result.heights.fill(0.0);
        result.land_fraction.fill(0.0);
        for (&direction, &height) in terrain.topology.directions.iter().zip(&terrain.elevations) {
            let i = Grid::index_for(direction, size);
            result.heights[i] += height * result.radius;
            result.cloud_heights[i] = result.cloud_heights[i].max(height * result.radius);
            result.land_fraction[i] += f64::from(u8::from(height >= 0.0));
            counts[i] += 1;
        }
        for (i, &count) in counts.iter().enumerate() {
            if count > 0 {
                result.heights[i] /= f64::from(count);
                result.land_fraction[i] /= f64::from(count);
            }
        }
        let grid = Grid::new(size);
        for _ in 0..size {
            let mut updates = Vec::new();
            for (i, &count) in counts.iter().enumerate() {
                if count > 0 {
                    continue;
                }
                let mut height = 0.0;
                let mut land = 0.0;
                let mut n = 0.0;
                for &j in &grid.cells[i].neighbours {
                    if counts[j] > 0 {
                        height += result.heights[j];
                        land += result.land_fraction[j];
                        n += 1.0;
                    }
                }
                if n > 0.0 {
                    updates.push((i, height / n, land / n));
                }
            }
            if updates.is_empty() {
                break;
            }
            for (i, height, land) in updates {
                result.heights[i] = height;
                result.land_fraction[i] = land;
                counts[i] = 1;
            }
        }
        // Cover bilinear cube-filter footprints on both sides of a peak.
        // This is render-only: the mean heights driving wind remain unchanged.
        for _ in 0..2 {
            result.cloud_heights = grid
                .cells
                .iter()
                .enumerate()
                .map(|(i, cell)| {
                    cell.neighbours
                        .iter()
                        .map(|&j| result.cloud_heights[j])
                        .fold(result.cloud_heights[i], f64::max)
                })
                .collect();
        }
        result
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct WeatherStats {
    pub minimum_temperature: f64,
    pub maximum_temperature: f64,
    pub mean_wind: f64,
    pub maximum_wind: f64,
    /// Mean per-layer area; `cloud_coverage` estimates their combined column area.
    pub mean_cloud: f64,
    pub mean_rain: f64,
    /// Independent layer overlap: 1 - (1 - lower) * (1 - upper).
    pub cloud_coverage: f64,
    pub clear_area: f64,
    pub overcast_area: f64,
    pub surface_water: f64,
    pub evaporated_water: f64,
    pub precipitated_water: f64,
    pub runoff_water: f64,
    pub maximum_wave_height: f64,
    pub total_air_mass: f64,
    /// Atmospheric water; its change equals cumulative evaporation minus rainfall.
    pub total_water: f64,
}
#[derive(Clone, Copy, Debug, Default)]
pub struct Swell {
    pub wave_vector: DVec3,
    pub amplitude: f64,
}
#[derive(Clone, Copy, Debug, Default)]
pub struct WeatherSample {
    pub lower_wind: DVec3,
    pub upper_wind: DVec3,
    pub temperature: f64,
    pub pressure: f64,
    pub humidity: f64,
    pub clouds: [f64; 2],
    pub rain: f64,
    pub swells: [Swell; 2],
}
#[derive(Clone, Debug)]
pub struct WeatherSnapshot {
    pub size: u32,
    pub seconds: f64,
    pub revision: u64,
    pub pixels: Vec<[f32; 4]>,
    pub stats: WeatherStats,
}

#[derive(Clone, Copy, Debug, Default)]
struct Layer {
    mass: f64,
    temperature: f64,
    vapour: f64,
    cloud: f64,
    wind: DVec3,
}
#[derive(Clone, Copy, Debug, Default)]
struct Column {
    layers: [Layer; 2],
    rain: f64,
}
pub struct WeatherSimulation {
    settings: WeatherSettings,
    surface: WeatherSurface,
    grid: Grid,
    cells: Vec<Column>,
    next: Vec<Column>,
    ground: Vec<moisture::SurfaceState>,
    lift: Vec<f64>,
    budget: moisture::WaterBudget,
    waves: Vec<[f64; 24]>,
    wave_next: Vec<[f64; 24]>,
    seconds: f64,
    remainder: f64,
    ticks: u64,
    solar_flux: f64,
}
impl WeatherSimulation {
    /// # Errors
    /// Surface dimensions, finite heights and the settings must be valid.
    pub fn new(
        seed: u64,
        settings: WeatherSettings,
        surface: WeatherSurface,
    ) -> Result<Self, String> {
        settings.validate()?;
        let count = 6 * settings.face_size as usize * settings.face_size as usize;
        if surface.size != settings.face_size
            || surface.heights.len() != count
            || surface.cloud_heights.len() != count
            || surface.land_fraction.len() != count
            || !surface.radius.is_finite()
            || surface.radius <= 0.0
            || surface.heights.iter().any(|h| !h.is_finite())
            || surface
                .cloud_heights
                .iter()
                .any(|h| !h.is_finite() || *h < 0.0)
            || surface
                .land_fraction
                .iter()
                .any(|v| !v.is_finite() || !(0.0..=1.0).contains(v))
        {
            return Err(
                "weather surface dimensions and finite samples must match the settings".into(),
            );
        }
        let grid = Grid::new(settings.face_size);
        let mut cells = Vec::with_capacity(count);
        for (i, cell) in grid.cells.iter().enumerate() {
            let n = cell.direction;
            let perturbation = noise::fractal(seed, n * 4.0, 3);
            let moisture = noise::fractal(seed.wrapping_add(31), n * 5.0, 3);
            let (east, north) = basis(n);
            let latitude = n.y.asin();
            let lower_temperature =
                293.0 - 30.0 * n.y * n.y + perturbation * 5.0 - surface.heights[i].max(0.0) * 0.09;
            let mut column = Column::default();
            for layer in 0..2 {
                let mass = 1.0 + perturbation * 0.012;
                let temperature = lower_temperature - 35.0 * layer as f64;
                let pressure = if layer == 0 { 850.0 } else { 450.0 };
                let saturated = physics::saturation(temperature, pressure);
                let humidity = (if layer == 0 {
                    0.80 + moisture * 0.65 + 0.10 * (1.0 - n.y * n.y)
                } else {
                    0.45 + moisture * 0.50 + 0.12 * (1.0 - n.y * n.y)
                }) * settings.humidity;
                let total = saturated * humidity.max(0.0);
                let (condensate, _) = moisture::cloud_distribution(total, saturated, layer);
                let wind =
                    east * (if layer == 0 {
                        -8.0 * (3.0 * latitude).cos()
                    } else {
                        18.0 * (3.0 * latitude).cos()
                    }) + north * (perturbation * 6.0 * latitude.cos());
                column.layers[layer] = Layer {
                    mass,
                    temperature,
                    vapour: total - condensate,
                    cloud: condensate,
                    wind: tangent(wind, n),
                };
            }
            cells.push(column);
        }
        let ground = cells
            .iter()
            .enumerate()
            .map(|(i, c)| moisture::SurfaceState {
                temperature: c.layers[0].temperature + 2.0,
                water: 0.004 * surface.land_fraction[i] * settings.humidity,
            })
            .collect();
        let mut result = Self {
            settings,
            surface,
            grid,
            next: cells.clone(),
            ground,
            lift: vec![0.0; count],
            budget: moisture::WaterBudget::default(),
            cells,
            waves: vec![[0.0; 24]; count],
            wave_next: vec![[0.0; 24]; count],
            seconds: 0.0,
            remainder: 0.0,
            ticks: 0,
            solar_flux: 1.0,
        };
        result.initialize_waves();
        Ok(result)
    }
    /// Fixed real-time stepping is independent of rendering frame cadence.
    /// Returns the number of fixed steps completed. The caller bounds worker batches.
    pub fn advance(&mut self, real_seconds: f64, sun_direction: DVec3) -> u32 {
        let sun = sun_direction.try_normalize().unwrap_or(DVec3::Y);
        self.advance_forcing(real_seconds, |_| (sun, 1.0))
    }
    /// Uses orbital position and spin at every atmosphere substep, including warm-up.
    pub fn advance_with_solar(
        &mut self,
        real_seconds: f64,
        solar: &crate::solar::SolarSystem,
    ) -> u32 {
        self.advance_forcing(real_seconds, |seconds| {
            let state = solar.state(seconds);
            (state.sun_direction, state.irradiance_ratio)
        })
    }
    fn advance_forcing(&mut self, real_seconds: f64, forcing: impl Fn(f64) -> (DVec3, f64)) -> u32 {
        if !real_seconds.is_finite() || real_seconds <= 0.0 || self.settings.time_scale == 0.0 {
            return 0;
        }
        self.remainder += real_seconds;
        let mut steps = 0;
        while self.remainder + 1.0e-10 >= STEP {
            self.remainder = (self.remainder - STEP).max(0.0);
            let dt = STEP * self.settings.time_scale;
            let minimum = self
                .grid
                .cells
                .iter()
                .map(|c| c.area.sqrt())
                .fold(f64::INFINITY, f64::min)
                * self.settings.effective_radius;
            let substeps = (dt / (minimum / 400.0).max(1.0)).ceil().max(1.0) as u32;
            for substep in 0..substeps {
                let (sun, flux) =
                    forcing(self.seconds + dt * (f64::from(substep) + 0.5) / f64::from(substeps));
                self.solar_flux = flux;
                self.atmosphere_step(dt / f64::from(substeps), sun);
            }
            self.wave_step(STEP);
            self.seconds += dt;
            self.ticks += 1;
            steps += 1;
        }
        self.solar_flux = 1.0;
        steps
    }
    #[must_use]
    pub fn sample(&self, direction: DVec3) -> WeatherSample {
        let i = self
            .grid
            .index(direction.try_normalize().unwrap_or(DVec3::Y));
        self.sample_cell(i)
    }
    fn sample_cell(&self, i: usize) -> WeatherSample {
        let c = self.cells[i];
        let l = c.layers[0];
        WeatherSample {
            lower_wind: l.wind,
            upper_wind: c.layers[1].wind,
            temperature: l.temperature,
            pressure: 1013.25 * (c.layers[0].mass + c.layers[1].mass) * 0.5,
            humidity: (l.vapour / physics::saturation(l.temperature, 850.0)).clamp(0.0, 1.5),
            clouds: std::array::from_fn(|layer| {
                moisture::cloud_optics(c.layers[layer], layer).fraction
            }),
            rain: c.rain,
            swells: self.dominant_swells(i),
        }
    }
    #[must_use]
    pub fn snapshot(&self) -> WeatherSnapshot {
        let count = self.cells.len();
        let mut pixels = vec![[0.0; 4]; count * LAYERS];
        let mut stats = WeatherStats {
            minimum_temperature: f64::INFINITY,
            maximum_temperature: f64::NEG_INFINITY,
            evaporated_water: self.budget.evaporated,
            precipitated_water: self.budget.precipitated,
            runoff_water: self.budget.runoff,
            ..WeatherStats::default()
        };
        let total_area = std::f64::consts::TAU * 2.0;
        for (i, cell) in self.grid.cells.iter().enumerate() {
            let s = self.sample_cell(i);
            let optical_clouds = self.reconstructed_clouds(i);
            pixels[CLOUD_OPTICS * count + i] = [
                optical_clouds[0].density as f32,
                optical_clouds[1].density as f32,
                self.surface.cloud_heights[i] as f32,
                // Reference height of the lower-air temperature. Keep the
                // mean orography separate from the conservative cloud ceiling
                // so fine terrain can derive a local freezing altitude.
                self.surface.heights[i].max(0.0) as f32,
            ];
            let weight = cell.area / total_area;
            for (layer, wind) in [s.lower_wind, s.upper_wind].into_iter().enumerate() {
                pixels[layer * count + i] = [
                    wind.x as f32,
                    wind.y as f32,
                    wind.z as f32,
                    optical_clouds[layer].fraction as f32,
                ];
            }
            pixels[CLIMATE * count + i] = [
                s.temperature as f32,
                s.pressure as f32,
                s.humidity.min(1.0) as f32,
                s.rain as f32,
            ];
            for (layer, swell) in s.swells.into_iter().enumerate() {
                pixels[(SWELL_PRIMARY + layer) * count + i] = [
                    swell.wave_vector.x as f32,
                    swell.wave_vector.y as f32,
                    swell.wave_vector.z as f32,
                    swell.amplitude as f32,
                ];
            }
            stats.minimum_temperature = stats.minimum_temperature.min(s.temperature);
            stats.maximum_temperature = stats.maximum_temperature.max(s.temperature);
            stats.mean_wind += s.lower_wind.length() * weight;
            stats.maximum_wind = stats.maximum_wind.max(s.lower_wind.length());
            stats.mean_cloud += (s.clouds[0] + s.clouds[1]) * 0.5 * weight;
            stats.mean_rain += s.rain * weight;
            let combined = 1.0 - (1.0 - s.clouds[0]) * (1.0 - s.clouds[1]);
            stats.cloud_coverage += combined * weight;
            if combined < 0.15 {
                stats.clear_area += weight;
            }
            if combined > 0.85 {
                stats.overcast_area += weight;
            }
            stats.surface_water += self.ground[i].water * cell.area;
            stats.maximum_wave_height = stats.maximum_wave_height.max(
                2.0 * s
                    .swells
                    .iter()
                    .map(|v| v.amplitude * v.amplitude)
                    .sum::<f64>()
                    .sqrt(),
            );
            for layer in self.cells[i].layers {
                stats.total_air_mass += layer.mass * cell.area;
                stats.total_water += layer.mass * (layer.vapour + layer.cloud) * cell.area;
            }
        }
        WeatherSnapshot {
            size: self.grid.size,
            seconds: self.seconds,
            revision: self.ticks,
            pixels,
            stats,
        }
    }
}
