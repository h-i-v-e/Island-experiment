use super::{
    Layer, WeatherSimulation,
    grid::{parallel_transport, tangent},
};
use crate::DVec3;

pub(super) fn saturation(temperature: f64, pressure_hpa: f64) -> f64 {
    let celsius = (temperature - 273.15).clamp(-70.0, 50.0);
    let vapour_pressure = 6.112 * (17.67 * celsius / (celsius + 243.5)).exp();
    (0.622 * vapour_pressure / (pressure_hpa - vapour_pressure).max(1.0)).clamp(0.00001, 0.1)
}
fn extensive_add(target: &mut Layer, source: Layer, amount: f64, momentum: DVec3) {
    target.mass += amount;
    target.temperature += source.temperature * amount;
    target.vapour += source.vapour * amount;
    target.cloud += source.cloud * amount;
    target.wind += momentum * amount;
}

impl WeatherSimulation {
    pub(super) fn transport_air(&mut self, dt: f64) {
        self.next.copy_from_slice(&self.cells);
        for column in &mut self.next {
            for layer in &mut column.layers {
                layer.temperature *= layer.mass;
                layer.vapour *= layer.mass;
                layer.cloud *= layer.mass;
                layer.wind *= layer.mass;
            }
        }
        for edge in &self.grid.edges {
            for layer in 0..2 {
                let a = self.cells[edge.a].layers[layer];
                let b = self.cells[edge.b].layers[layer];
                let velocity = (a.wind + b.wind).dot(edge.tangent) * 0.5;
                let advection = velocity * if velocity >= 0.0 { a.mass } else { b.mass };
                // Centred pressure gradients cannot see an alternating-cell
                // pressure mode on a colocated grid. A modest acoustic face
                // flux couples those cells even when their mean wind is zero.
                // Both fluxes transport the same extensive tracers; this is
                // conservative pressure stabilization, not a visual blur.
                let pressure = 0.5 * 35.0 * (a.mass - b.mass);
                for (flux, limit) in [(advection, 0.045), (pressure, 0.015)] {
                    let (donor, recipient) = if flux >= 0.0 {
                        (edge.a, edge.b)
                    } else {
                        (edge.b, edge.a)
                    };
                    let source = self.cells[donor].layers[layer];
                    let from = &self.grid.cells[donor];
                    let to = &self.grid.cells[recipient];
                    // Shared budgets and bounded outflow keep air and moisture
                    // positive even during unusually large catch-up steps.
                    let amount = (flux.abs() * edge.width * dt / self.settings.effective_radius)
                        .min(from.area * source.mass * limit);
                    extensive_add(
                        &mut self.next[donor].layers[layer],
                        source,
                        -amount / from.area,
                        source.wind,
                    );
                    let momentum = parallel_transport(source.wind, from.direction, to.direction);
                    extensive_add(
                        &mut self.next[recipient].layers[layer],
                        source,
                        amount / to.area,
                        momentum,
                    );
                }
            }
        }
        for (i, column) in self.next.iter_mut().enumerate() {
            for layer in &mut column.layers {
                let inverse = 1.0 / layer.mass;
                layer.temperature *= inverse;
                layer.vapour *= inverse;
                layer.cloud *= inverse;
                layer.wind = tangent(layer.wind * inverse, self.grid.cells[i].direction);
            }
        }
        std::mem::swap(&mut self.cells, &mut self.next);
    }
    pub(super) fn atmosphere_step(&mut self, dt: f64, sun: DVec3) {
        self.transport_air(dt);
        self.convergence();
        self.next.copy_from_slice(&self.cells);
        for i in 0..self.grid.cells.len() {
            let cell = &self.grid.cells[i];
            let n = cell.direction;
            let height = self.surface.heights[i].max(0.0) * 20.0;
            let land = self.surface.land_fraction[i];
            let insolation = n.dot(sun).max(0.0) * self.solar_flux;
            let equilibrium = 274.0
                + (18.0 * (1.0 - n.y * n.y) + 10.0 * insolation) * self.settings.solar_heating
                - height * 0.0065;
            let mut height_gradient = DVec3::ZERO;
            for &j in &cell.neighbours {
                let other = &self.grid.cells[j];
                let delta = tangent(other.direction, n);
                let distance = delta.length() * self.settings.effective_radius;
                height_gradient += delta.normalize()
                    * (self.surface.heights[j].max(0.0) * 20.0 - height)
                    / distance
                    * 0.5;
            }
            self.lift[i] = self.lift_at(i, height_gradient);
            for layer in 0..2 {
                let current = self.cells[i].layers[layer];
                let column_mass = self.cells[i].layers[0].mass + self.cells[i].layers[1].mass;
                let mut pressure_gradient = DVec3::ZERO;
                let mut temperature_gradient = DVec3::ZERO;
                for &j in &cell.neighbours {
                    let other = self.cells[j].layers[layer];
                    let other_mass = self.cells[j].layers[0].mass + self.cells[j].layers[1].mass;
                    let delta = tangent(self.grid.cells[j].direction, n);
                    let distance = delta.length() * self.settings.effective_radius;
                    pressure_gradient +=
                        delta.normalize() * (other_mass.ln() - column_mass.ln()) / distance * 0.5;
                    temperature_gradient +=
                        delta.normalize() * (other.temperature - current.temperature) / distance
                            * 0.5;
                }
                // The layers share column pressure. Aloft, a warm column's
                // hydrostatic thickness adds stronger thermal forcing, keeping
                // baroclinic shear rather than independently cancelling each
                // layer's heat gradient into a windless static equilibrium.
                let thermal_thickness = if layer == 0 { 35.0 } else { 287.0 };
                let acceleration = (-pressure_gradient * (287.0 * current.temperature)
                    - temperature_gradient * thermal_thickness)
                    .clamp_length_max(0.03);
                let accelerated = current.wind + acceleration * dt;
                let angle = -2.0 * std::f64::consts::TAU / self.settings.rotation_period * n.y * dt;
                let (sin, cos) = angle.sin_cos();
                let wind = (accelerated * cos + n.cross(accelerated) * sin)
                    * (-dt / if layer == 0 { 86_400.0 } else { 432_000.0 }).exp();
                let target = equilibrium - 35.0 * layer as f64;
                let thermal_time = if layer == 0 {
                    40000.0 - 30000.0 * land
                } else {
                    30000.0
                };
                let output = &mut self.next[i].layers[layer];
                output.wind = tangent(wind.clamp_length_max(65.0), n);
                output.temperature +=
                    (target - current.temperature) * (1.0 - (-dt / thermal_time).exp());
                output.temperature -=
                    self.lift[i] * dt * 0.0065 * if layer == 0 { 1.0 } else { 0.7 };
            }
            self.evaporate(i, equilibrium, insolation, dt);
            self.exchange_moisture(i, dt, insolation);
            let mut rain = 0.0;
            for (layer, output) in self.next[i].layers.iter_mut().enumerate() {
                rain += super::moisture::cloud_step(output, layer, dt);
            }
            self.rain_to_surface(i, rain);
            self.next[i].rain = self.cells[i].rain * (-dt / 1200.0).exp()
                + (rain / dt * 1_000_000.0).min(1.0) * (1.0 - (-dt / 1200.0).exp());
        }
        std::mem::swap(&mut self.cells, &mut self.next);
    }
}
