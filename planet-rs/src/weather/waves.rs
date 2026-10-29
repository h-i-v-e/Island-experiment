use super::{
    Swell, WeatherSimulation,
    grid::{basis, parallel_transport, tangent},
};
use crate::DVec3;
pub const DIRECTIONS: usize = 8;
pub const WAVELENGTHS: [f64; 3] = [10.0, 30.0, 80.0];
fn heading(normal: DVec3, bin: usize) -> DVec3 {
    let (east, north) = basis(normal);
    let angle = std::f64::consts::TAU * (bin % DIRECTIONS) as f64 / DIRECTIONS as f64;
    east * angle.cos() + north * angle.sin()
}
fn direction_bin(normal: DVec3, vector: DVec3) -> usize {
    let (east, north) = basis(normal);
    let angle = vector
        .dot(north)
        .atan2(vector.dot(east))
        .rem_euclid(std::f64::consts::TAU);
    ((angle * DIRECTIONS as f64 / std::f64::consts::TAU).round() as usize) % DIRECTIONS
}
fn wind_target(normal: DVec3, wind: DVec3, bin: usize, band: usize) -> f64 {
    let speed = wind.length();
    if speed < 0.5 {
        return 0.0;
    }
    let alignment = (wind.dot(heading(normal, bin)) / speed).max(0.0).powi(4);
    let peak = (std::f64::consts::TAU * speed * speed / 9.81).max(2.0);
    let frequency = (-((WAVELENGTHS[band] / peak).ln()).powi(2) / 2.0).exp();
    (0.045 * (speed / 8.0).powi(4) * frequency * alignment).min(1.5)
}
impl WeatherSimulation {
    pub(super) fn initialize_waves(&mut self) {
        for (i, cell) in self.grid.cells.iter().enumerate() {
            if self.surface.land_fraction[i] >= 0.5 {
                continue;
            }
            for bin in 0..24 {
                self.waves[i][bin] = wind_target(
                    cell.direction,
                    self.cells[i].layers[0].wind,
                    bin % DIRECTIONS,
                    bin / DIRECTIONS,
                ) * 0.5;
            }
        }
    }
    pub(super) fn wave_step(&mut self, dt: f64) {
        self.wave_next.copy_from_slice(&self.waves);
        for (i, cell) in self.grid.cells.iter().enumerate() {
            if self.surface.land_fraction[i] >= 0.5 {
                self.wave_next[i].fill(0.0);
                continue;
            }
            let depth = (-self.surface.heights[i]).max(0.1);
            for bin in 0..24 {
                let energy = self.waves[i][bin];
                if energy < 1.0e-12 {
                    continue;
                }
                let band = bin / DIRECTIONS;
                let direction = heading(cell.direction, bin);
                let k = std::f64::consts::TAU / WAVELENGTHS[band];
                let kh = (k * depth).min(20.0);
                let phase = (9.81 / k * kh.tanh()).sqrt();
                let group = phase
                    * 0.5
                    * (1.0
                        + if kh < 10.0 {
                            2.0 * kh / (2.0 * kh).sinh()
                        } else {
                            0.0
                        });
                let fractions = cell.neighbours.map(|j| {
                    let other = &self.grid.cells[j];
                    let outgoing = tangent(other.direction, cell.direction).normalize();
                    let width = cell.area.min(other.area).sqrt();
                    direction.dot(outgoing).max(0.0) * group * dt / self.surface.radius * width
                        / cell.area
                });
                let total: f64 = fractions.iter().sum();
                let limiter = if total > 0.35 { 0.35 / total } else { 1.0 };
                for (&j, fraction) in cell.neighbours.iter().zip(fractions) {
                    let amount = energy * fraction * limiter;
                    self.wave_next[i][bin] -= amount;
                    // A land cell absorbs incoming wave action. No energy is
                    // forwarded through the continent to a sheltered sea.
                    if self.surface.land_fraction[j] >= 0.5 {
                        continue;
                    }
                    let transported =
                        parallel_transport(direction, cell.direction, self.grid.cells[j].direction);
                    let destination = direction_bin(self.grid.cells[j].direction, transported)
                        + band * DIRECTIONS;
                    self.wave_next[j][destination] += amount * cell.area / self.grid.cells[j].area;
                }
            }
        }
        for (i, cell) in self.grid.cells.iter().enumerate() {
            if self.surface.land_fraction[i] >= 0.5 {
                self.wave_next[i].fill(0.0);
                continue;
            }
            let wind = self.cells[i].layers[0].wind;
            let limit = (-self.surface.heights[i]).max(0.0) * 0.18;
            let energy_limit = (limit * limit * 0.5).min(1.5);
            for bin in 0..24 {
                let target = wind_target(cell.direction, wind, bin % DIRECTIONS, bin / DIRECTIONS)
                    .min(energy_limit);
                let energy = self.wave_next[i][bin].max(0.0);
                let time = if target > energy { 45.0 } else { 600.0 };
                let evolved = energy + (target - energy) * (1.0 - (-dt / time).exp());
                // Bottom friction and depth-limited breaking dissipate the
                // excess gently; no instant replacement when wind changes.
                self.wave_next[i][bin] = evolved.min(energy_limit).max(0.0);
            }
        }
        std::mem::swap(&mut self.waves, &mut self.wave_next);
    }
    pub(super) fn dominant_swells(&self, i: usize) -> [Swell; 2] {
        let energies = &self.waves[i];
        let mut order = [0; 2];
        let mut values = [0.0; 2];
        for (bin, &energy) in energies.iter().enumerate() {
            if energy > values[0] {
                order[1] = order[0];
                values[1] = values[0];
                order[0] = bin;
                values[0] = energy;
            } else if energy > values[1] {
                order[1] = bin;
                values[1] = energy;
            }
        }
        std::array::from_fn(|j| {
            let bin = order[j];
            Swell {
                wave_vector: heading(self.grid.cells[i].direction, bin) * std::f64::consts::TAU
                    / WAVELENGTHS[bin / DIRECTIONS],
                amplitude: (values[j] * 2.0).sqrt(),
            }
        })
    }
}
