use planet_rs::{DVec3, PlanetOptions, PlanetTerrain, TerrainOptions, generate, generate_terrain};

fn planet(seed: u64) -> PlanetOptions {
    PlanetOptions {
        seed,
        vertex_count: 256,
        ..PlanetOptions::default()
    }
}

#[test]
fn twenty_seeded_worlds_have_valid_radial_terrain_and_downhill_rivers() {
    for seed in 0..20 {
        let terrain = generate_terrain(planet(seed), TerrainOptions::default()).unwrap();
        terrain.validate().unwrap();
        for i in 0..terrain.elevations.len() {
            assert!(
                (terrain.normalized_position(i).length() - 1.0 - terrain.elevations[i]).abs()
                    < 1.0e-12
            );
            assert!(terrain.down(i).dot(terrain.normals[i]) < 0.0);
        }
        for river in &terrain.rivers {
            assert!(terrain.elevations[river.to] < terrain.elevations[river.from]);
            let mut i = river.from;
            for _ in 0..terrain.elevations.len() {
                let Some(next) = terrain.drainage.receivers[i] else {
                    break;
                };
                i = next;
            }
            assert!(terrain.elevations[i] < 0.0);
        }
    }
}

#[test]
fn sea_level_stays_at_original_radius_for_dry_wet_and_zero_noise_worlds() {
    for bias in [-0.1, 0.0, 0.1] {
        let options = PlanetOptions {
            radius: 1000.0,
            ..planet(42)
        };
        let terrain = generate_terrain(
            options,
            TerrainOptions {
                relief: 0.0,
                land_bias: bias,
                hydraulic_passes: 0,
                thermal_passes: 0,
                ..TerrainOptions::default()
            },
        )
        .unwrap();
        assert_eq!(terrain.topology.radius.to_bits(), 1000.0_f64.to_bits());
        assert!(
            terrain
                .elevations
                .iter()
                .all(|&h| (h - bias).abs() < f64::EPSILON)
        );
        assert!(terrain.rivers.is_empty());
        for i in 0..terrain.elevations.len() {
            assert!((terrain.position(i).length() - 1000.0 * (1.0 + bias)).abs() < 1.0e-9);
        }
    }
}

#[test]
fn radial_results_are_repeatable_and_scale_with_the_original_radius() {
    let settings = TerrainOptions::default();
    let first = generate_terrain(planet(42), settings).unwrap();
    assert_eq!(first, generate_terrain(planet(42), settings).unwrap());
    let large = generate_terrain(
        PlanetOptions {
            radius: 6_371_000.0,
            ..planet(42)
        },
        settings,
    )
    .unwrap();
    assert_eq!(first.elevations, large.elevations);
    assert_eq!(first.drainage, large.drainage);
    assert_eq!(first.rivers, large.rivers);
    for i in 0..first.elevations.len() {
        assert!(
            (first.position(i) / first.topology.radius).distance(large.position(i) / 6_371_000.0)
                < 1.0e-12
        );
    }
}

#[test]
fn river_downhill_is_radial_and_can_increase_world_y() {
    let terrain = generate_terrain(
        PlanetOptions {
            vertex_count: 512,
            ..planet(42)
        },
        TerrainOptions::default(),
    )
    .unwrap();
    let stats = terrain.stats();
    assert!(stats.land_vertices > 0 && stats.submerged_vertices > 0);
    assert!(
        terrain
            .rivers
            .iter()
            .any(|river| terrain.position(river.to).y > terrain.position(river.from).y)
    );
    assert!(
        terrain
            .rivers
            .iter()
            .any(|river| terrain.position(river.to).y < terrain.position(river.from).y)
    );
}

#[test]
fn authored_noise_free_terrain_has_no_island_edge_or_perimeter_rule() {
    let mesh = generate(planet(1)).unwrap();
    // Same signed heights in both polar hemispheres: no hemisphere is a border.
    let heights = mesh.directions.iter().map(|p| 0.03 * p.x).collect();
    let terrain = PlanetTerrain::from_elevations(
        mesh,
        heights,
        1,
        TerrainOptions {
            hydraulic_passes: 0,
            thermal_passes: 0,
            river_depth: 0.0,
            ..TerrainOptions::default()
        },
    )
    .unwrap();
    assert!(
        terrain
            .topology
            .directions
            .iter()
            .enumerate()
            .any(|(i, p)| p.y > 0.7 && terrain.elevations[i] > 0.0)
    );
    assert!(
        terrain
            .topology
            .directions
            .iter()
            .enumerate()
            .any(|(i, p)| p.y < -0.7 && terrain.elevations[i] > 0.0)
    );
}

#[test]
fn invalid_terrain_parameters_and_damaged_routes_are_rejected() {
    for settings in [
        TerrainOptions {
            relief: f64::NAN,
            ..TerrainOptions::default()
        },
        TerrainOptions {
            relief: 0.5,
            ..TerrainOptions::default()
        },
        TerrainOptions {
            detail_frequency: f64::INFINITY,
            ..TerrainOptions::default()
        },
        TerrainOptions {
            hydraulic_passes: 129,
            ..TerrainOptions::default()
        },
        TerrainOptions {
            river_catchment: 0.0,
            ..TerrainOptions::default()
        },
    ] {
        assert!(generate_terrain(planet(1), settings).is_err());
    }
    let mut terrain = generate_terrain(planet(42), TerrainOptions::default()).unwrap();
    let land = terrain.elevations.iter().position(|&h| h > 0.0).unwrap();
    terrain.drainage.receivers[land] = Some(land);
    assert!(terrain.validate().is_err());
    let mut terrain = generate_terrain(planet(42), TerrainOptions::default()).unwrap();
    terrain.normals[0] = DVec3::ZERO;
    assert!(terrain.validate().is_err());
}
