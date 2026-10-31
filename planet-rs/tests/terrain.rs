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
        assert!(
            terrain.ocean_passes.iter().all(|p| p.seas_after <= 1),
            "seed {seed}"
        );
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
fn entirely_deep_ocean_skips_all_requested_terrain_refinement_stages() {
    let options = planet(42);
    let terrain = generate_terrain(
        options,
        TerrainOptions {
            relief: 0.0,
            land_bias: -0.1,
            hydraulic_passes: 0,
            thermal_passes: 0,
            refinement_passes: 6,
            smoothing_passes: 12,
            ..TerrainOptions::default()
        },
    )
    .unwrap();
    assert_eq!(terrain.topology.directions.len(), options.vertex_count);
    assert_eq!(
        terrain.topology.directions,
        generate(options).unwrap().directions
    );
    assert!(
        terrain
            .elevations
            .iter()
            .all(|&h| h.to_bits() == (-0.1_f64).to_bits())
    );
    terrain.validate().unwrap();
}

#[test]
fn raising_relief_does_not_drain_seas_without_fixed_metre_basin_filling() {
    // Control-area coverage avoids counting adaptive land vertices as extra
    // land area: deeper water intentionally has far fewer vertices.
    fn submerged_fraction(t: &PlanetTerrain) -> f64 {
        t.topology
            .triangles
            .iter()
            .map(|face| {
                let [a, b, c] = face.map(|i| t.topology.directions[i as usize]);
                let area = 2.0
                    * a.dot(b.cross(c))
                        .abs()
                        .atan2(1.0 + a.dot(b) + b.dot(c) + c.dot(a));
                let submerged: u32 = face
                    .iter()
                    .map(|&i| u32::from(t.elevations[i as usize] < 0.0))
                    .sum();
                area * f64::from(submerged) / 3.0
            })
            .sum::<f64>()
            / (4.0 * std::f64::consts::PI)
    }
    let terrain = |relief| {
        generate_terrain(
            PlanetOptions {
                vertex_count: 1024,
                ..PlanetOptions::default()
            },
            TerrainOptions {
                relief,
                land_bias: 0.0,
                noise_layers: planet_rs::NoiseLayer::plates(),
                refinement_passes: 3,
                river_refinement_passes: 0,
                river_catchment: 1.0,
                river_depth: 0.0,
                // Isolate the source/refinement sea-area contract. Basin fill
                // deliberately removes inland water with a fixed metre floor.
                connect_seas: false,
                ..TerrainOptions::default()
            },
        )
        .unwrap()
    };
    let low = submerged_fraction(&terrain(0.08));
    let high = submerged_fraction(&terrain(0.32));
    assert!(low > 0.3 && high < 0.5);
    assert!(
        high >= low - 0.005,
        "raising relief removed sea area: low={low:.4}, high={high:.4}"
    );
}

#[test]
fn radial_results_without_fixed_metre_basin_fill_are_repeatable_and_scale_with_radius() {
    let settings = TerrainOptions {
        connect_seas: false,
        ..TerrainOptions::default()
    };
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
            noise_layers: [planet_rs::NoiseLayer {
                frequency: f64::INFINITY,
                ..planet_rs::NoiseLayer::classic()[0]
            }; 3],
            ..TerrainOptions::default()
        },
        TerrainOptions {
            erosion_strength: 8.001,
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

#[test]
fn plate_layers_survive_refinement_drainage_and_radial_validation() {
    for seed in [1, 42, 123] {
        let terrain = generate_terrain(
            planet(seed),
            TerrainOptions {
                noise_layers: planet_rs::NoiseLayer::plates(),
                ..TerrainOptions::default()
            },
        )
        .unwrap_or_else(|error| panic!("plate seed {seed}: {error}"));
        terrain.validate().unwrap();
        assert!(terrain.elevations.iter().any(|h| *h > 0.0));
        assert!(terrain.elevations.iter().any(|h| *h < 0.0));
        assert!(
            terrain
                .rivers
                .iter()
                .all(|r| terrain.elevations[r.from] > terrain.elevations[r.to])
        );
    }
}

#[test]
fn disabling_every_noise_layer_also_disables_refinement_noise() {
    let mut layers = planet_rs::NoiseLayer::classic();
    for layer in &mut layers {
        layer.weight = 0.0;
    }
    let terrain = generate_terrain(
        planet(42),
        TerrainOptions {
            noise_layers: layers,
            land_bias: 0.02,
            refinement_passes: 1,
            smoothing_passes: 0,
            river_refinement_passes: 0,
            hydraulic_passes: 0,
            thermal_passes: 0,
            river_depth: 0.0,
            connect_seas: false,
            ..TerrainOptions::default()
        },
    )
    .unwrap();
    assert!(terrain.topology.directions.len() > 256);
    assert!(terrain.elevations.iter().all(|h| (*h - 0.02).abs() < 1e-12));
}

#[test]
fn tectonic_worlds_refine_erode_and_keep_sea_at_the_original_radius() {
    for (seed, relief) in [(7, 0.16), (42, 0.08), (42, 0.32), (123, 0.16)] {
        let t = generate_terrain(
            PlanetOptions {
                seed,
                vertex_count: 1024,
                ..PlanetOptions::default()
            },
            TerrainOptions {
                source: planet_rs::TerrainSource::Tectonics,
                relief,
                refinement_passes: 4,
                river_refinement_passes: 2,
                ..TerrainOptions::default()
            },
        )
        .unwrap();
        t.validate().unwrap();
        let stats = t.stats();
        assert!(stats.land_vertices > 0 && stats.submerged_vertices > 0);
        assert!(
            (0.4..0.95).contains(&stats.submerged_area_fraction),
            "unexpected sea coverage for seed {seed}, relief {relief}: {}",
            stats.submerged_area_fraction
        );
        assert!(!t.rivers.is_empty());
        assert!(
            t.bedrock_hardness
                .iter()
                .all(|h| h.is_finite() && (0.05..=0.95).contains(h))
        );
        assert_eq!(t.topology.radius.to_bits(), 1000.0_f64.to_bits());
    }
}

#[test]
fn tectonic_height_and_hardness_follow_drift_but_do_not_depend_on_detail_noise() {
    let options = PlanetOptions {
        vertex_count: 512,
        ..PlanetOptions::default()
    };
    let settings = TerrainOptions {
        source: planet_rs::TerrainSource::Tectonics,
        hydraulic_passes: 0,
        thermal_passes: 0,
        smoothing_passes: 0,
        refinement_passes: 0,
        river_refinement_passes: 0,
        connect_seas: false,
        river_catchment: 1.0,
        river_depth: 0.0,
        tectonics: planet_rs::TectonicOptions {
            detail: 0.0,
            ..planet_rs::TectonicOptions::default()
        },
        ..TerrainOptions::default()
    };
    let a = generate_terrain(options, settings).unwrap();
    assert_eq!(a, generate_terrain(options, settings).unwrap());
    let still = generate_terrain(
        options,
        TerrainOptions {
            tectonics: planet_rs::TectonicOptions {
                drift: 0.0,
                ..settings.tectonics
            },
            ..settings
        },
    )
    .unwrap();
    assert!(
        a.elevations
            .iter()
            .zip(&still.elevations)
            .any(|(a, b)| (a - b).abs() > 0.001)
    );
    assert!(
        a.bedrock_hardness
            .iter()
            .zip(&still.bedrock_hardness)
            .any(|(a, b)| (a - b).abs() > 0.01)
    );
    let noisy = generate_terrain(
        options,
        TerrainOptions {
            tectonics: planet_rs::TectonicOptions {
                detail: 0.25,
                ..settings.tectonics
            },
            ..settings
        },
    )
    .unwrap();
    assert_eq!(a.bedrock_hardness, noisy.bedrock_hardness);
    assert_ne!(a.elevations, noisy.elevations);
    let wet = a.elevations.iter().filter(|&&h| h < 0.0).count();
    assert!(wet > options.vertex_count / 2 && wet < options.vertex_count * 9 / 10);
}

#[test]
fn sea_coverage_measures_area_instead_of_counting_nonuniform_vertices() {
    let mut t = generate_terrain(
        PlanetOptions {
            vertex_count: 128,
            ..PlanetOptions::default()
        },
        TerrainOptions {
            relief: 0.0,
            land_bias: -0.1,
            refinement_passes: 0,
            river_refinement_passes: 0,
            hydraulic_passes: 0,
            thermal_passes: 0,
            smoothing_passes: 0,
            ..TerrainOptions::default()
        },
    )
    .unwrap();
    assert!((t.sea_area_fraction() - 1.0).abs() < 1.0e-12);
    t.elevations.fill(0.1);
    assert!(t.sea_area_fraction().abs() < f64::EPSILON);
    t.elevations.fill(0.0);
    assert!(t.sea_area_fraction().abs() < f64::EPSILON);
    for (h, p) in t.elevations.iter_mut().zip(&t.topology.directions) {
        *h = p.y * 0.1;
    }
    assert!((t.sea_area_fraction() - 0.5).abs() < 1.0e-12);
}

#[test]
fn impacts_and_volcanic_chains_survive_the_complete_terrain_and_river_pipeline() {
    for seed in [7, 42, 91] {
        let terrain = generate_terrain(
            PlanetOptions {
                seed,
                vertex_count: 512,
                radius: 1000.0,
                ..PlanetOptions::default()
            },
            TerrainOptions {
                source: planet_rs::TerrainSource::Tectonics,
                impacts: planet_rs::ImpactOptions {
                    enabled: true,
                    per_stage: 3,
                    ..Default::default()
                },
                refinement_passes: 3,
                river_refinement_passes: 3,
                ..TerrainOptions::default()
            },
        )
        .unwrap_or_else(|error| panic!("seed {seed}: {error}"));
        terrain.validate().unwrap();
        assert!(!terrain.volcanoes.is_empty());
        assert_eq!(terrain.impact_craters.len(), 12);
        assert_eq!(
            terrain.impact_craters.iter().map(|c| c.stage).max(),
            Some(3)
        );
        assert!(
            terrain
                .elevations
                .iter()
                .all(|h| h.is_finite() && *h > -0.8 && *h < 0.8)
        );
        assert!(terrain.stats().submerged_area_fraction > 0.25);
        assert!(!terrain.rivers.is_empty());
    }
}
