//! Borrow the generated radial terrain when creating owned GPU buffers.
use bevy::prelude::*;
use planet_rs::{DVec3, PlanetTerrain};

#[must_use]
pub fn vector(point: DVec3) -> Vec3 {
    Vec3::from_array(point.as_vec3().to_array())
}

/// Match Unity's COLOR contract: hardness, loose cover, river bed, sea proximity.
/// All distances are converted to metres before choosing material coverage.
#[allow(clippy::cast_possible_truncation)]
pub(crate) fn terrain_material(terrain: &PlanetTerrain, i: usize, uncarved: bool) -> [f32; 4] {
    let radius = terrain.topology.radius;
    let soil_metres = terrain.soil_depth[i] * radius;
    let elevation = (surface_height(terrain, i, uncarved)) * radius;
    [
        terrain.bedrock_hardness[i].clamp(0.0, 1.0) as f32,
        blend(soil_metres, 0.0, 4.0),
        if uncarved {
            0.0
        } else {
            blend(terrain.channel_depths[i] * radius, 0.02, 0.3)
        },
        1.0 - blend(elevation.max(0.0), 2.0, 20.0),
    ]
}

fn surface_height(source: &PlanetTerrain, vertex: usize, uncarved: bool) -> f64 {
    source.elevations[vertex]
        + if uncarved {
            source.channel_depths[vertex] - source.bank_lifts[vertex]
        } else {
            0.0
        }
}

#[allow(clippy::cast_possible_truncation)]
fn blend(value: f64, low: f64, high: f64) -> f32 {
    let t = ((value - low) / (high - low)).clamp(0.0, 1.0);
    (t * t * (3.0 - 2.0 * t)) as f32
}

pub fn displayed_position(source: &PlanetTerrain, vertex: usize, uncarved: bool) -> DVec3 {
    source.topology.directions[vertex]
        * (1.0
            + source.elevations[vertex]
            + if uncarved {
                source.channel_depths[vertex] - source.bank_lifts[vertex]
            } else {
                0.0
            })
}

#[cfg(test)]
mod tests {
    use super::*;
    use planet_rs::{PlanetOptions, TerrainOptions, generate_terrain};

    #[test]
    fn material_coverage_uses_metres_and_radial_height_on_both_hemispheres() {
        let mut source = generate_terrain(
            PlanetOptions {
                vertex_count: 16,
                ..default()
            },
            TerrainOptions {
                relief: 0.0,
                hydraulic_passes: 0,
                thermal_passes: 0,
                refinement_passes: 0,
                river_refinement_passes: 0,
                smoothing_passes: 0,
                connect_seas: false,
                ..default()
            },
        )
        .unwrap();
        let mut reference = None;
        for radius in [1000.0, 2000.0] {
            source.topology.radius = radius;
            source.elevations[0] = 5.0 / radius;
            source.soil_depth[0] = 2.0 / radius;
            source.channel_depths[0] = 0.2 / radius;
            source.bank_lifts[0] = 0.1 / radius;
            source.bedrock_hardness[0] = 0.6;
            for direction in [DVec3::Y, -DVec3::Y, DVec3::Z, -DVec3::X] {
                source.topology.directions[0] = direction;
                let coverage = terrain_material(&source, 0, false);
                if let Some(expected) = reference {
                    assert_eq!(coverage.map(f32::to_bits), expected);
                } else {
                    reference = Some(coverage.map(f32::to_bits));
                }
                assert_eq!(
                    terrain_material(&source, 0, true)[2].to_bits(),
                    0.0_f32.to_bits(),
                    "uncarved inspection has no river-bed overlay"
                );
            }
        }
    }
}
