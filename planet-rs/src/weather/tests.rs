use super::*;

#[test]
fn cloud_clearance_covers_cube_edge_peaks_without_changing_mean_orography() {
    let mut terrain = crate::generate_terrain(
        crate::PlanetOptions {
            vertex_count: 128,
            ..crate::PlanetOptions::default()
        },
        crate::TerrainOptions {
            refinement_passes: 0,
            river_refinement_passes: 0,
            hydraulic_passes: 0,
            thermal_passes: 0,
            ..crate::TerrainOptions::default()
        },
    )
    .unwrap();
    let size = 8;
    let grid = Grid::new(size);
    let face_area = (size * size) as usize;
    let (peak, cell) = terrain
        .topology
        .directions
        .iter()
        .enumerate()
        .map(|(i, &n)| (i, grid.index(n)))
        .find(|&(_, i)| {
            grid.cells[i]
                .neighbours
                .iter()
                .any(|&j| i / face_area != j / face_area)
        })
        .unwrap();
    terrain.elevations.fill(0.0);
    terrain.elevations[peak] = 0.09;
    let surface = WeatherSurface::from_terrain(&terrain, size);
    let count = terrain
        .topology
        .directions
        .iter()
        .filter(|&&n| grid.index(n) == cell)
        .count();
    let height = terrain.topology.radius * 0.09;
    assert!((surface.heights[cell] - height / count as f64).abs() < 1e-10);
    for &neighbour in &grid.cells[cell].neighbours {
        assert!(surface.cloud_heights[neighbour] >= height);
        for &second in &grid.cells[neighbour].neighbours {
            assert!(surface.cloud_heights[second] >= height);
        }
    }
}

#[test]
fn cloud_clearance_metadata_does_not_change_weather_evolution() {
    let mut original = simulation(8);
    let mut elevated = simulation(8);
    elevated.surface.cloud_heights.fill(123.5);
    for _ in 0..20 {
        original.advance(0.25, DVec3::Y);
        elevated.advance(0.25, DVec3::Y);
    }
    let mut before = original.snapshot().pixels;
    let mut after = elevated.snapshot().pixels;
    let offset = CLOUD_OPTICS * original.cells.len();
    for (a, b) in before[offset..].iter_mut().zip(&mut after[offset..]) {
        assert_eq!(b[2].to_bits(), 123.5_f32.to_bits());
        a[2] = 0.0;
        b[2] = 0.0;
    }
    assert_eq!(before, after);
}

#[test]
fn face_pressure_flux_damps_stationary_checkerboard_without_losing_air_or_water() {
    let mut sim = simulation(16);
    let width = sim.grid.size as usize;
    for (i, column) in sim.cells.iter_mut().enumerate() {
        let parity = if (i % width + (i / width) % width).is_multiple_of(2) {
            1.0
        } else {
            -1.0
        };
        for layer in &mut column.layers {
            layer.mass = 1.0 + 0.03 * parity;
            layer.wind = DVec3::ZERO;
            layer.temperature = 280.0;
            layer.vapour = 0.004;
            layer.cloud = 0.0002;
        }
    }
    let amplitude = |simulation: &WeatherSimulation| {
        simulation
            .cells
            .iter()
            .zip(&simulation.grid.cells)
            .enumerate()
            .map(|(i, (column, cell))| {
                let parity = if (i % width + (i / width) % width).is_multiple_of(2) {
                    1.0
                } else {
                    -1.0
                };
                (column.layers[0].mass - 1.0) * parity * cell.area
            })
            .sum::<f64>()
            .abs()
    };
    let before = amplitude(&sim);
    let budgets = totals(&sim);
    for _ in 0..100 {
        sim.transport_air(150.0);
    }
    assert!(
        amplitude(&sim) < before * 0.2,
        "stationary pressure checkerboards must not survive the face flux"
    );
    let after = totals(&sim);
    assert!((after.0 - budgets.0).abs() < 1e-11);
    assert!((after.1 - budgets.1).abs() < 1e-12);
    assert!(
        sim.cells
            .iter()
            .flat_map(|c| c.layers)
            .all(|l| l.mass > 0.0 && l.vapour >= 0.0 && l.cloud >= 0.0)
    );
}

#[test]
fn optical_reconstruction_suppresses_cell_pattern_and_preserves_uniform_clouds() {
    let mut sim = simulation(16);
    let width = sim.grid.size as usize;
    for column in &mut sim.cells {
        for layer in &mut column.layers {
            layer.cloud = 0.0002;
            layer.vapour = 0.006;
            layer.temperature = 280.0;
        }
    }
    let uniform = moisture::cloud_optics(sim.cells[0].layers[0], 0).fraction;
    for i in 0..sim.cells.len() {
        assert!((sim.reconstructed_clouds(i)[0].fraction - uniform).abs() < 1e-12);
    }
    for (i, column) in sim.cells.iter_mut().enumerate() {
        column.layers[0].cloud = if (i % width + (i / width) % width).is_multiple_of(2) {
            0.0006
        } else {
            0.0
        };
    }
    let snapshot = sim.snapshot();
    let mut raw_pattern = 0.0;
    let mut filtered_pattern = 0.0;
    let mut raw_mean = 0.0;
    let mut filtered_mean = 0.0;
    for (i, cell) in sim.grid.cells.iter().enumerate() {
        let sign = if (i % width + (i / width) % width).is_multiple_of(2) {
            1.0
        } else {
            -1.0
        };
        raw_pattern += sim.sample_cell(i).clouds[0] * sign * cell.area;
        filtered_pattern += f64::from(snapshot.pixels[i][3]) * sign * cell.area;
        raw_mean += sim.sample_cell(i).clouds[0] * cell.area;
        filtered_mean += f64::from(snapshot.pixels[i][3]) * cell.area;
    }
    assert!(filtered_pattern.abs() < raw_pattern.abs() * 0.75);
    assert!(
        (filtered_mean - raw_mean).abs() / (4.0 * std::f64::consts::PI) < 0.005,
        "optical reconstruction must not turn clear gaps into extra mean cloud cover"
    );
    assert!(
        snapshot.pixels[..sim.cells.len()]
            .iter()
            .all(|p| (0.0..=1.0).contains(&p[3]))
    );
    assert!(
        sim.grid
            .optical_ring
            .iter()
            .enumerate()
            .any(|(i, ring)| ring
                .iter()
                .any(|&j| i / (width * width) != j / (width * width))),
        "optical footprints must reconstruct across cube faces"
    );
}

#[test]
fn optical_reconstruction_has_no_preferred_cube_axis_for_smooth_cloud_fields() {
    let mut sim = simulation(16);
    let mut means = Vec::new();
    for axis in [DVec3::X, DVec3::new(0.7, 0.3, 0.64).normalize()] {
        for (cell, column) in sim.grid.cells.iter().zip(&mut sim.cells) {
            column.layers[0].cloud = 0.0002 + 0.0001 * cell.direction.dot(axis);
            column.layers[0].temperature = 280.0;
            column.layers[0].vapour = 0.006;
        }
        means.push(
            sim.grid
                .cells
                .iter()
                .enumerate()
                .map(|(i, cell)| sim.reconstructed_clouds(i)[0].fraction * cell.area)
                .sum::<f64>()
                / (4.0 * std::f64::consts::PI),
        );
    }
    assert!(
        (means[0] - means[1]).abs() < 0.00005,
        "rotating a smooth physical cloud field must not expose cube axes: {means:?}"
    );
}

#[test]
fn pressure_stabilization_retains_a_resolved_global_gradient() {
    let mut sim = simulation(16);
    for (cell, column) in sim.grid.cells.iter().zip(&mut sim.cells) {
        for layer in &mut column.layers {
            layer.mass = 1.0 + 0.03 * cell.direction.x;
            layer.wind = DVec3::ZERO;
        }
    }
    let projection = |simulation: &WeatherSimulation| {
        simulation
            .cells
            .iter()
            .zip(&simulation.grid.cells)
            .map(|(column, cell)| (column.layers[0].mass - 1.0) * cell.direction.x * cell.area)
            .sum::<f64>()
    };
    let before = projection(&sim);
    for _ in 0..100 {
        sim.transport_air(150.0);
    }
    assert!(
        projection(&sim) > before * 0.98,
        "pressure stabilization must retain resolved circulation gradients"
    );
}

fn simulation(size: u32) -> WeatherSimulation {
    WeatherSimulation::new(
        42,
        WeatherSettings {
            face_size: size,
            ..WeatherSettings::default()
        },
        WeatherSurface::uniform(size, -30.0),
    )
    .unwrap()
}
fn totals(sim: &WeatherSimulation) -> (f64, f64) {
    sim.grid
        .cells
        .iter()
        .zip(&sim.cells)
        .fold((0.0, 0.0), |(mass, water), (cell, column)| {
            (
                mass + column
                    .layers
                    .iter()
                    .map(|l| l.mass * cell.area)
                    .sum::<f64>(),
                water
                    + column
                        .layers
                        .iter()
                        .map(|l| l.mass * (l.vapour + l.cloud) * cell.area)
                        .sum::<f64>(),
            )
        })
}

#[test]
fn cube_grid_covers_sphere_and_connects_all_face_edges() {
    for size in [4, 8, 16] {
        let grid = Grid::new(size);
        assert!(
            (grid.cells.iter().map(|c| c.area).sum::<f64>() - 4.0 * std::f64::consts::PI).abs()
                < 1.0e-10
        );
        let face_pixels = (size * size) as usize;
        let crossing = grid
            .edges
            .iter()
            .filter(|e| e.a / face_pixels != e.b / face_pixels)
            .count();
        assert_eq!(crossing, 12 * size as usize);
        for cell in &grid.cells {
            assert!((cell.direction.length() - 1.0).abs() < 1.0e-12);
            for &n in &cell.neighbours {
                assert!(grid.cells[n].direction.dot(cell.direction) > 0.8);
            }
        }
    }
}
#[test]
fn atmospheric_transport_conserves_area_weighted_air_and_water() {
    let mut sim = simulation(12);
    let before = totals(&sim);
    for _ in 0..100 {
        sim.transport_air(150.0);
    }
    let after = totals(&sim);
    assert!((before.0 - after.0).abs() < 1.0e-11);
    assert!((before.1 - after.1).abs() < 1.0e-12);
    for column in &sim.cells {
        for layer in column.layers {
            assert!(layer.mass > 0.0 && layer.vapour >= 0.0 && layer.cloud >= 0.0);
        }
    }
}
#[test]
fn seeded_weather_is_fixed_step_frame_rate_independent_and_tangent() {
    let mut fine = simulation(8);
    let mut coarse = simulation(8);
    for _ in 0..600 {
        fine.advance(1.0 / 60.0, DVec3::Y);
    }
    for _ in 0..40 {
        coarse.advance(0.25, DVec3::Y);
    }
    assert_eq!(fine.snapshot().pixels, coarse.snapshot().pixels);
    assert_eq!(fine.snapshot().revision, 40);
    for (cell, column) in fine.grid.cells.iter().zip(&fine.cells) {
        for layer in column.layers {
            assert!(layer.wind.dot(cell.direction).abs() < 1.0e-10);
        }
    }
    let different = WeatherSimulation::new(
        43,
        WeatherSettings {
            face_size: 8,
            ..WeatherSettings::default()
        },
        WeatherSurface::uniform(8, -30.0),
    )
    .unwrap();
    assert_ne!(different.snapshot().pixels, simulation(8).snapshot().pixels);
}
#[test]
fn long_weather_run_stays_finite_positive_and_spatially_varied() {
    let mut sim = simulation(8);
    for i in 0..400 {
        let angle = f64::from(i) * 0.01;
        sim.advance(0.25, DVec3::new(angle.sin(), 0.4, angle.cos()));
    }
    let result = sim.snapshot();
    assert!(result.pixels.iter().flatten().all(|v| v.is_finite()));
    assert!(result.stats.maximum_wind <= 65.0 + 1e-10);
    assert!(result.stats.minimum_temperature > 180.0 && result.stats.maximum_temperature < 340.0);
    assert!(result.stats.mean_cloud > 0.0 && result.stats.mean_cloud < 0.95);
    for column in &sim.cells {
        for l in column.layers {
            assert!(l.mass > 0.0 && l.vapour >= 0.0 && l.cloud >= 0.0);
        }
    }
    let initial = simulation(8).snapshot().stats;
    assert!((initial.total_air_mass - result.stats.total_air_mass).abs() < 1.0e-9);
}
#[test]
fn zero_humidity_clears_clouds_and_land_stays_wave_free() {
    let settings = WeatherSettings {
        face_size: 8,
        humidity: 0.0,
        ..WeatherSettings::default()
    };
    let mut sim = WeatherSimulation::new(42, settings, WeatherSurface::uniform(8, 20.0)).unwrap();
    sim.advance(5.0, DVec3::Y);
    let snapshot = sim.snapshot();
    assert!(snapshot.stats.mean_cloud.abs() < f64::EPSILON);
    assert!(snapshot.stats.maximum_wave_height.abs() < f64::EPSILON);
}
#[test]
fn waves_grow_with_wind_and_keep_swell_when_the_wind_stops() {
    let mut sim = simulation(8);
    for (cell, column) in sim.grid.cells.iter().zip(&mut sim.cells) {
        column.layers[0].wind = basis(cell.direction).0 * 15.0;
    }
    sim.waves.fill([0.0; 24]);
    for _ in 0..160 {
        sim.wave_step(0.25);
    }
    let energy = sim.waves.iter().flatten().sum::<f64>();
    assert!(energy > 1.0);
    for column in &mut sim.cells {
        column.layers[0].wind = DVec3::ZERO;
    }
    sim.wave_step(0.25);
    let calm = sim.waves.iter().flatten().sum::<f64>();
    assert!(calm > energy * 0.95 && calm < energy * 1.02);
    for _ in 0..400 {
        sim.wave_step(0.25);
    }
    assert!(sim.waves.iter().flatten().sum::<f64>() < calm);
}

#[test]
fn land_barriers_do_not_transport_swell_between_disconnected_seas() {
    let mut sim = simulation(8);
    sim.surface.land_fraction.fill(1.0);
    sim.surface.land_fraction[0] = 0.0;
    sim.surface.land_fraction[100] = 0.0;
    sim.waves.fill([0.0; 24]);
    sim.waves[0][0] = 0.25;
    for column in &mut sim.cells {
        column.layers[0].wind = DVec3::ZERO;
    }
    for _ in 0..100 {
        sim.wave_step(0.25);
    }
    assert!(sim.waves[0][0] > 0.0);
    assert!(
        sim.waves[100]
            .iter()
            .all(|energy| energy.abs() < f64::EPSILON)
    );
    for (i, energies) in sim.waves.iter().enumerate() {
        if i != 0 {
            assert!(energies.iter().all(|energy| energy.abs() < f64::EPSILON));
        }
    }
}

#[test]
fn full_default_grid_produces_bounded_worker_snapshots() {
    let mut sim = simulation(64);
    let started = std::time::Instant::now();
    sim.advance(0.25, DVec3::new(-0.4, 0.6, 0.7));
    let snapshot = sim.snapshot();
    println!(
        "64-face weather step and snapshot: {:?}; {:?}",
        started.elapsed(),
        snapshot.stats
    );
    assert_eq!(snapshot.pixels.len(), 64 * 64 * 6 * LAYERS);
    assert!(snapshot.pixels.iter().flatten().all(|v| v.is_finite()));
    assert!(snapshot.stats.mean_cloud > 0.01 && snapshot.stats.mean_cloud < 0.9);
}

#[test]
fn coupled_heating_sustains_multiday_circulation_and_variable_clouds() {
    let mut sim = simulation(8);
    let initial_mass = sim.snapshot().stats.total_air_mass;
    for day in 0..3 {
        for tick in 0..576 {
            let angle = std::f64::consts::TAU * f64::from(tick) / 576.0;
            sim.advance(0.25, DVec3::new(angle.sin() * 0.9, 0.4, angle.cos() * 0.9));
        }
        let snapshot = sim.snapshot();
        let stats = snapshot.stats;
        assert!(
            stats.mean_wind > 1.0,
            "day {day}: independent layer pressures must not cancel heating into a windless equilibrium"
        );
        assert!(stats.maximum_wind <= 65.0 + 1e-10);
        assert!((stats.total_air_mass - initial_mass).abs() < 1e-9);
        assert!(stats.minimum_temperature > 240.0 && stats.maximum_temperature < 330.0);
        assert!(stats.total_water > 0.01 && stats.total_water < 1.0);
        assert!(stats.mean_rain > 0.0 && stats.mean_cloud > 0.01 && stats.mean_cloud < 0.9);
        let count = sim.cells.len();
        let cover = &snapshot.pixels[..count];
        let min = cover.iter().map(|p| p[3]).fold(f32::INFINITY, f32::min);
        let max = cover.iter().map(|p| p[3]).fold(f32::NEG_INFINITY, f32::max);
        assert!(
            max - min > 0.1,
            "clouds must remain spatially varied after spin-up"
        );
        assert!(snapshot.pixels.iter().flatten().all(|v| v.is_finite()));
    }
}

#[test]
fn no_solar_forcing_and_no_humidity_do_not_invent_cloud_water() {
    let mut sim = WeatherSimulation::new(
        42,
        WeatherSettings {
            face_size: 8,
            humidity: 0.0,
            solar_heating: 0.0,
            ..WeatherSettings::default()
        },
        WeatherSurface::uniform(8, -30.0),
    )
    .unwrap();
    for _ in 0..1152 {
        sim.advance(0.25, DVec3::ZERO);
    }
    let snapshot = sim.snapshot();
    assert!(snapshot.stats.total_water.abs() < f64::EPSILON);
    assert!(snapshot.stats.mean_cloud.abs() < f64::EPSILON);
    assert!(snapshot.stats.mean_rain.abs() < f64::EPSILON);
    assert!(
        snapshot.stats.maximum_temperature - snapshot.stats.minimum_temperature < 5.0,
        "without solar heating the imposed equator-pole thermal gradient must decay"
    );
    assert!(snapshot.pixels.iter().flatten().all(|v| v.is_finite()));
}
#[test]
fn cube_snapshot_packs_every_layer_and_clouds_form_from_saturation() {
    let mut sim = simulation(8);
    let i = sim.grid.index(DVec3::Y);
    sim.cells[i].layers[0].vapour = 0.04;
    sim.cells[i].layers[0].cloud = 0.0;
    sim.atmosphere_step(30.0, DVec3::Y);
    assert!(sim.cells[i].layers[0].cloud > 0.0);
    let s = sim.snapshot();
    let count = 6 * 8 * 8;
    assert_eq!(s.pixels.len(), LAYERS * count);
    assert_eq!(
        s.pixels[CLIMATE * count + i][0].to_bits(),
        (sim.cells[i].layers[0].temperature as f32).to_bits()
    );
    assert_eq!(
        s.pixels[LOWER * count + i][3].to_bits(),
        (sim.reconstructed_clouds(i)[0].fraction as f32).to_bits()
    );
}

#[test]
fn snow_temperature_reference_uses_mean_land_height_without_the_cloud_ceiling_or_seabed() {
    let settings = WeatherSettings {
        face_size: 4,
        ..WeatherSettings::default()
    };
    let mut surface = WeatherSurface::uniform(4, 37.5);
    surface.heights[0] = -40.0;
    surface.cloud_heights.fill(250.0);
    let simulation = WeatherSimulation::new(42, settings, surface).unwrap();
    let snapshot = simulation.snapshot();
    let count = 6 * 4 * 4;
    for i in 0..count {
        let fields = snapshot.pixels[CLOUD_OPTICS * count + i];
        assert_eq!(fields[2].to_bits(), 250.0_f32.to_bits());
        assert_eq!(
            fields[3].to_bits(),
            if i == 0 { 0.0_f32 } else { 37.5_f32 }.to_bits()
        );
    }
}
#[test]
fn weather_rejects_invalid_controls_and_surface_mismatches() {
    for size in [0, 3, 129, u32::MAX] {
        assert!(
            WeatherSurface::uniform(size, -10.0).heights.is_empty(),
            "invalid surface dimensions must not allocate before validation"
        );
    }
    for settings in [
        WeatherSettings {
            face_size: 0,
            ..WeatherSettings::default()
        },
        WeatherSettings {
            humidity: f64::NAN,
            ..WeatherSettings::default()
        },
        WeatherSettings {
            time_scale: -1.0,
            ..WeatherSettings::default()
        },
    ] {
        assert!(settings.validate().is_err());
    }
    assert!(
        WeatherSimulation::new(
            42,
            WeatherSettings::default(),
            WeatherSurface::uniform(8, -10.0)
        )
        .is_err()
    );
}

#[test]
fn orbital_weather_forcing_is_independent_of_render_frame_cadence() {
    let solar =
        crate::solar::SolarSystem::new(crate::solar::SolarSettings::default(), 1000.0).unwrap();
    let mut batched = simulation(4);
    let mut split = simulation(4);
    assert_eq!(batched.advance_with_solar(1.0, &solar), 4);
    for _ in 0..10 {
        split.advance_with_solar(0.1, &solar);
    }
    assert_eq!(batched.snapshot().pixels, split.snapshot().pixels);
    assert!((batched.seconds - 600.0).abs() < 1e-10);
}

#[test]
fn orbital_warmup_uses_changing_sun_and_inverse_square_heating() {
    let mut orbiting = simulation(4);
    let mut fixed = simulation(4);
    let solar = crate::solar::SolarSystem::new(
        crate::solar::SolarSettings {
            rotation_period: 4.0 * 3600.0,
            eccentricity: 0.2,
            ..crate::solar::SolarSettings::default()
        },
        1000.0,
    )
    .unwrap();
    orbiting.advance_with_solar(36.0, &solar);
    fixed.advance(36.0, solar.sun_direction(0.0));
    let thermal_difference: f64 = orbiting
        .cells
        .iter()
        .zip(&fixed.cells)
        .map(|(a, b)| (a.layers[0].temperature - b.layers[0].temperature).abs())
        .sum();
    assert!(
        thermal_difference > 1.0,
        "warm-up must not use one frozen Sun direction"
    );
    assert!(
        orbiting
            .snapshot()
            .pixels
            .iter()
            .flatten()
            .all(|x| x.is_finite())
    );
    assert!((totals(&orbiting).0 - totals(&fixed).0).abs() < 1e-10);
}

#[test]
fn phase_changes_and_fallout_preserve_an_explicit_water_and_heat_budget() {
    for (vapour, cloud) in [(0.02, 0.0), (0.001, 0.0002), (0.006, 0.00001)] {
        let mut air = Layer {
            mass: 1.2,
            temperature: 280.0,
            vapour,
            cloud,
            wind: DVec3::ZERO,
        };
        let before = air;
        let rain = moisture::cloud_step(&mut air, 0, 150.0);
        assert!(
            (air.mass * (air.vapour + air.cloud) + rain - before.mass * (vapour + cloud)).abs()
                < 1e-14
        );
        let condensed = air.cloud + rain / air.mass - before.cloud;
        assert!((air.temperature - before.temperature - condensed * 2490.0).abs() < 1e-12);
        assert!(air.cloud >= 0.0 && air.vapour >= 0.0 && rain >= 0.0);
    }
}
#[test]
fn thin_clouds_have_no_protected_residue_and_evaporate_in_dry_air() {
    let mut air = Layer {
        mass: 1.0,
        temperature: 280.0,
        cloud: 0.0002,
        vapour: 0.001,
        wind: DVec3::ZERO,
    };
    let total = air.cloud + air.vapour;
    let mut rain = 0.0;
    for _ in 0..144 {
        rain += moisture::cloud_step(&mut air, 0, 150.0);
    }
    assert!(rain > 0.0, "thin clouds must still be able to precipitate");
    assert!(air.cloud < 1e-10);
    assert!((air.vapour + air.cloud + rain - total).abs() < 1e-14);
    assert!(moisture::cloud_optics(air, 0).fraction < 0.001);
}
#[test]
fn cloud_occupancy_and_in_cloud_density_are_independent() {
    let total = physics::saturation(280.0, 850.0);
    let make = |cloud| Layer {
        mass: 1.0,
        temperature: 280.0,
        vapour: total - cloud,
        cloud,
        wind: DVec3::ZERO,
    };
    let thin = moisture::cloud_optics(make(0.00002), 0);
    let thick = moisture::cloud_optics(make(0.0004), 0);
    assert!((thin.fraction - thick.fraction).abs() < 1e-10);
    assert!(thick.density > thin.density * 3.0);
    assert!(thin.fraction > 0.1 && thin.fraction < 0.9);
}
#[test]
fn ten_solar_days_keep_clear_regions_and_close_water_budgets() {
    let solar =
        crate::solar::SolarSystem::new(crate::solar::SolarSettings::default(), 1000.0).unwrap();
    for seed in [7, 42, 113] {
        let mut sim = WeatherSimulation::new(
            seed,
            WeatherSettings {
                face_size: 8,
                ..WeatherSettings::default()
            },
            WeatherSurface::uniform(8, -30.0),
        )
        .unwrap();
        let initial = sim.snapshot().stats;
        let mut late = Vec::new();
        for day in 1..=10 {
            sim.advance_with_solar(144.0, &solar);
            let snapshot = sim.snapshot();
            let stats = snapshot.stats;
            println!(
                "seed={seed} day={day} cover={:.3} clear={:.3} wind={:.2}",
                stats.cloud_coverage, stats.clear_area, stats.mean_wind
            );
            assert!((stats.total_air_mass - initial.total_air_mass).abs() < 1e-9);
            assert!(
                (stats.total_water + stats.precipitated_water
                    - stats.evaporated_water
                    - initial.total_water)
                    .abs()
                    < 1e-10
            );
            assert!(snapshot.pixels.iter().flatten().all(|v| v.is_finite()));
            if day >= 3 {
                assert!(
                    (0.2..0.7).contains(&stats.cloud_coverage),
                    "global cloud blanket returned"
                );
                assert!(stats.clear_area > 0.03, "clear regions disappeared");
                let mut extrema = (1.0_f32, 0.0_f32);
                for p in &snapshot.pixels[..sim.cells.len()] {
                    extrema.0 = extrema.0.min(p[3]);
                    extrema.1 = extrema.1.max(p[3]);
                }
                assert!(
                    extrema.1 - extrema.0 > 0.20,
                    "regional cloud contrast disappeared"
                );
                late.push(stats.cloud_coverage);
            }
        }
        assert!(
            (late[late.len() - 1] - late[0]).abs() < 0.15,
            "coverage keeps drifting instead of equilibrating"
        );
    }
}
#[test]
fn land_evaporation_uses_a_finite_rain_replenished_reservoir() {
    let solar =
        crate::solar::SolarSystem::new(crate::solar::SolarSettings::default(), 1000.0).unwrap();
    let mut sim = WeatherSimulation::new(
        42,
        WeatherSettings {
            face_size: 8,
            ..WeatherSettings::default()
        },
        WeatherSurface::uniform(8, 5.0),
    )
    .unwrap();
    let initial = sim.snapshot().stats;
    sim.advance_with_solar(144.0 * 10.0, &solar);
    let stats = sim.snapshot().stats;
    assert!(
        (stats.total_water + stats.surface_water + stats.runoff_water
            - initial.total_water
            - initial.surface_water)
            .abs()
            < 1e-10
    );
    assert!(stats.surface_water >= 0.0);
    assert!(stats.evaporated_water > 0.0 && stats.precipitated_water > 0.0);
    assert!(stats.cloud_coverage < 0.75);
}
