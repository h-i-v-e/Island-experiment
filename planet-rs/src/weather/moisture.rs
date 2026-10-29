//! Local water cycle and sub-grid cloud distribution. All transfers are explicit:
//! phase changes conserve atmospheric water; evaporation/rain cross its boundary.
use super::{Layer, WeatherSimulation};
use crate::DVec3;

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct SurfaceState {
    pub temperature: f64,
    /// Liquid water per column area, already weighted by land fraction.
    pub water: f64,
}
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct WaterBudget {
    pub evaporated: f64,
    pub precipitated: f64,
    pub runoff: f64,
}
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct CloudOptics {
    pub fraction: f64,
    pub density: f64,
}

/// A uniform unresolved total-water distribution around the resolved mean.
/// Integrating its saturated tail gives both area and condensate independently.
pub(super) fn cloud_distribution(total: f64, saturated: f64, layer: usize) -> (f64, f64) {
    let spread = saturated * if layer == 0 { 0.28 } else { 0.20 };
    let excess = total - saturated;
    if excess <= -spread {
        (0.0, 0.0)
    } else if excess >= spread {
        (excess, 1.0)
    } else {
        let wet_tail = excess + spread;
        (
            wet_tail * wet_tail / (4.0 * spread),
            wet_tail / (2.0 * spread),
        )
    }
}
pub(super) fn cloud_optics(air: Layer, layer: usize) -> CloudOptics {
    let saturated =
        super::physics::saturation(air.temperature, if layer == 0 { 850.0 } else { 450.0 });
    let (_, fraction) = cloud_distribution(air.vapour + air.cloud, saturated, layer);
    let fraction = fraction.min(air.cloud.max(0.0) / 0.000_001);
    let in_cloud_water = air.cloud.max(0.0) / fraction.max(0.01);
    CloudOptics {
        fraction,
        density: 1.0 - (-in_cloud_water / 0.0005).exp(),
    }
}

/// Relax towards fractional saturation with latent-heat feedback. Rain has no
/// protected condensate floor, so even old thin clouds eventually fall out.
pub(super) fn cloud_step(air: &mut Layer, layer: usize, dt: f64) -> f64 {
    let saturated =
        super::physics::saturation(air.temperature, if layer == 0 { 850.0 } else { 450.0 });
    let (target, fraction) = cloud_distribution(air.vapour + air.cloud, saturated, layer);
    let difference = target - air.cloud;
    let thermal_feedback = 1.0 + 2490.0 * saturated * 0.065 * fraction;
    let phase =
        (difference / thermal_feedback * (1.0 - (-dt / 450.0).exp())).clamp(-air.cloud, air.vapour);
    air.cloud += phase;
    air.vapour -= phase;
    air.temperature += phase * 2490.0;
    let rate = 1.0 / if layer == 0 { 18_000.0 } else { 12_000.0 }
        + air.cloud / (air.cloud + 0.0003) / if layer == 0 { 2400.0 } else { 3600.0 };
    let rain = air.cloud * (1.0 - (-dt * rate).exp());
    air.cloud -= rain;
    rain * air.mass
}

impl WeatherSimulation {
    pub(super) fn convergence(&mut self) {
        self.lift.fill(0.0);
        for edge in &self.grid.edges {
            let velocity = (self.cells[edge.a].layers[0].wind + self.cells[edge.b].layers[0].wind)
                .dot(edge.tangent)
                * 0.5;
            let flow = velocity * edge.width * 1500.0 / self.settings.effective_radius;
            self.lift[edge.a] -= flow / self.grid.cells[edge.a].area;
            self.lift[edge.b] += flow / self.grid.cells[edge.b].area;
        }
    }
    pub(super) fn evaporate(&mut self, i: usize, equilibrium: f64, sunlight: f64, dt: f64) {
        let land = self.surface.land_fraction[i];
        let ground = &mut self.ground[i];
        let target = equilibrium + 2.0 + land * (6.0 * sunlight - 3.0);
        ground.temperature +=
            (target - ground.temperature) * (1.0 - (-dt / (100_000.0 - 90_000.0 * land)).exp());
        let air = &mut self.next[i].layers[0];
        let surface_vapour =
            super::physics::saturation(ground.temperature, 1013.25) * self.settings.humidity;
        let rate = 0.000_018 + air.wind.length() * 0.000_002;
        let potential =
            (surface_vapour - air.vapour).max(0.0) * (1.0 - (-dt * rate).exp()) * air.mass;
        let wetness = ground.water / (ground.water + 0.002 * land).max(1e-12);
        let from_land = (potential * land * wetness).min(ground.water);
        let amount = potential * (1.0 - land) + from_land;
        ground.water -= from_land;
        ground.temperature -= amount * 2490.0 / (40.0 - 35.0 * land);
        air.vapour += amount / air.mass;
        self.budget.evaporated += amount * self.grid.cells[i].area;
    }
    pub(super) fn rain_to_surface(&mut self, i: usize, rain: f64) {
        let land = self.surface.land_fraction[i];
        self.ground[i].water += rain * land;
        let runoff = (self.ground[i].water - 0.02 * land).max(0.0);
        self.ground[i].water -= runoff;
        self.budget.precipitated += rain * self.grid.cells[i].area;
        self.budget.runoff += runoff * self.grid.cells[i].area;
    }
    pub(super) fn exchange_moisture(&mut self, i: usize, dt: f64, insolation: f64) {
        let [low, high] = self.next[i].layers;
        let instability = ((low.temperature - high.temperature - 30.0) / 5.0).clamp(0.0, 1.0);
        let buoyancy = (self.ground[i].temperature - low.temperature - 2.0).max(0.0);
        let convection = buoyancy * insolation * instability * 0.000_001;
        let rate = 0.000_000_2 + self.lift[i].abs() / 2500.0 + convection;
        let exchanged = low.mass.min(high.mass) * (1.0 - (-dt * rate).exp()).min(0.04);
        for layer in 0..2 {
            let (source, other) = if layer == 0 { (low, high) } else { (high, low) };
            let output = &mut self.next[i].layers[layer];
            let fraction = exchanged / output.mass;
            output.vapour += (other.vapour - source.vapour) * fraction;
            output.cloud += (other.cloud - source.cloud) * fraction;
            // Exchange potential temperature: descending upper air compresses
            // and warms instead of spuriously cooling the lower layer.
            let other_temperature = other.temperature + if layer == 0 { 35.0 } else { -35.0 };
            output.temperature += (other_temperature - source.temperature) * fraction;
        }
    }
    pub(super) fn lift_at(&self, i: usize, terrain_gradient: DVec3) -> f64 {
        (self.lift[i] + self.cells[i].layers[0].wind.dot(terrain_gradient)).clamp(-0.08, 0.08)
    }
}
