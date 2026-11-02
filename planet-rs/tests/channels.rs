use planet_rs::{PlanetOptions, TerrainOptions, generate_terrain};

#[test]
fn refined_channels_have_excavated_beds_and_water_above_actual_terrain_faces() {
    let options = PlanetOptions {
        seed: 42,
        vertex_count: 512,
        ..PlanetOptions::default()
    };
    let settings = TerrainOptions::default();
    let terrain = generate_terrain(options, settings).unwrap();
    assert!(terrain.refinement_vertices.len() >= 4);
    assert_eq!(terrain.refinement_vertices[0], 512);
    assert!(terrain.topology.directions.len() > 512 * 16);
    assert!(terrain.stats().carved_vertices > 100);
    assert!(terrain.stats().maximum_channel_depth > settings.river_depth * 0.5);
    assert!(!terrain.river_surface.triangles.is_empty());
    // Whole-river smoothing can remove all material initially added at the
    // banks, so net lift may be zero. Actual bank lifting/pinning is exercised
    // independently by the authored low-bank and waterfall-support fixtures.
    assert!(terrain.bank_added_material.is_finite() && terrain.bank_added_material >= 0.0);
    assert!(
        terrain
            .bank_lifts
            .iter()
            .zip(&terrain.channel_depths)
            .all(|(&lift, &cut)| lift >= 0.0 && cut >= 0.0 && lift.min(cut) <= f64::EPSILON)
    );
    assert!(
        !terrain.waterfalls.is_empty(),
        "stepped reaches must produce waterfalls"
    );
    assert!(
        terrain.river_surface.triangles.iter().any(|face| {
            let [a, b, c] = face.map(|i| terrain.river_surface.positions[i as usize]);
            let normal = (b - a).cross(c - a).normalize();
            normal.dot((a + b + c).normalize()) < 0.8
        }),
        "waterfall drops must exist in the actual water mesh, not only in metadata"
    );
    terrain
        .river_surface
        .validate(&terrain.topology, &terrain.elevations)
        .unwrap();
    let wet_centres = terrain
        .rivers
        .iter()
        .filter(|r| {
            terrain.elevations[r.from] > 0.005
                && terrain.channel_depths[r.from] > settings.river_depth * 0.3
                && terrain.water_elevations[r.from] - terrain.elevations[r.from]
                    > settings.river_depth * 0.2
        })
        .count();
    assert!(
        wet_centres > 5,
        "must have real water clearance over carved centre vertices"
    );
    for (face, &support) in terrain
        .river_surface
        .triangles
        .iter()
        .zip(&terrain.river_surface.terrain_faces)
    {
        let [a, b, c] =
            terrain.topology.triangles[support].map(|i| terrain.normalized_position(i as usize));
        let normal = (b - a).cross(c - a).normalize();
        for &i in face {
            assert!(normal.dot(terrain.river_surface.positions[i as usize] - a) >= -1.0e-9);
        }
    }
}

#[test]
fn disabling_refinement_keeps_seed_topology_and_options_are_bounded() {
    let options = PlanetOptions {
        vertex_count: 128,
        ..PlanetOptions::default()
    };
    let terrain = generate_terrain(
        options,
        TerrainOptions {
            refinement_passes: 0,
            river_refinement_passes: 0,
            ..TerrainOptions::default()
        },
    )
    .unwrap();
    assert_eq!(terrain.topology.directions.len(), 128);
    for settings in [
        TerrainOptions {
            refinement_passes: 7,
            ..TerrainOptions::default()
        },
        TerrainOptions {
            river_width: 0.0,
            ..TerrainOptions::default()
        },
        TerrainOptions {
            river_refinement_passes: 7,
            ..TerrainOptions::default()
        },
    ] {
        assert!(generate_terrain(options, settings).is_err());
    }
}

#[test]
fn inland_channel_keeps_its_water_when_excavation_crosses_zero_height() {
    use planet_rs::{DVec3, ImpactOptions, OceanStage, TectonicOptions};
    // Regression from seed 44: the channel crosses zero near this inland
    // location, while its original land is 14 m high and water is 4.5 m high.
    let terrain = generate_terrain(
        PlanetOptions {
            seed: 44,
            vertex_count: 1024,
            ..PlanetOptions::default()
        },
        TerrainOptions {
            relief: 0.2,
            erosion_strength: 6.0,
            hydraulic_passes: 2,
            thermal_passes: 4,
            refinement_passes: 3,
            river_refinement_passes: 3,
            tectonics: TectonicOptions {
                steps: 32,
                boundary_width: 1.0,
                branching: 1.0,
                detail: 0.25,
                ..TectonicOptions::default()
            },
            impacts: ImpactOptions {
                enabled: true,
                per_stage: 8,
                ..ImpactOptions::default()
            },
            ..TerrainOptions::default()
        },
    )
    .unwrap();
    let location = DVec3::new(-0.985_134, -0.066_730, -0.158_300).normalize();
    let radius = terrain.topology.radius;
    let (i, distance) = terrain
        .topology
        .directions
        .iter()
        .enumerate()
        .filter(|&(i, _)| {
            terrain.elevations[i] < 0.0
                && (terrain.elevations[i] + terrain.channel_depths[i] - terrain.bank_lifts[i])
                    * radius
                    > 10.0
        })
        .map(|(i, &p)| (i, p.distance(location) * radius))
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .expect("excavated inland channel");
    assert!(
        distance < 5.0,
        "reproduce the affected section, distance {distance}"
    );
    assert!(
        terrain.water_elevations[i] * radius > 2.0,
        "crossing sea-level in the bed must not remove the inland river water"
    );
    assert!(
        terrain.pre_carve_sea_proximity[i] < 1e-4,
        "the pre-carve map must retain this inland channel instead of treating its excavated bed as ocean"
    );
    assert_eq!(
        terrain.ocean_passes.last().unwrap().stage,
        OceanStage::FinalTerrain,
        "inland-sea filling must finish before the final channel excavation"
    );
    terrain.validate().unwrap();
}
