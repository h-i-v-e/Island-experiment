//! Reconstruct cloud area and in-cloud density without changing conserved cells.
use super::{
    WeatherSimulation,
    moisture::{self, CloudOptics},
};
impl WeatherSimulation {
    pub(super) fn reconstructed_clouds(&self, index: usize) -> [CloudOptics; 2] {
        std::array::from_fn(|layer| {
            let sample = |i: usize| moisture::cloud_optics(self.cells[i].layers[layer], layer);
            let centre = sample(index);
            let mut area = centre.fraction * 0.6;
            let mut optical = centre.fraction * centre.density * 0.6;
            for &i in &self.grid.optical_ring[index] {
                let cloud = sample(i);
                area += cloud.fraction * 0.05;
                optical += cloud.fraction * cloud.density * 0.05;
            }
            CloudOptics {
                fraction: area,
                density: if area > 1e-9 { optical / area } else { 0.0 },
            }
        })
    }
}
