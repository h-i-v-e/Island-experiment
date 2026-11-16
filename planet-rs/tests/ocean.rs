use planet_rs::{
    ImpactOptions, OceanStage, PlanetOptions, TerrainOptions, TerrainSource, generate_terrain,
};

#[test]
fn inland_seas_are_filled_through_terrain_stages_before_final_river_carving() {
    let options = PlanetOptions {
        seed: 42,
        vertex_count: 256,
        ..PlanetOptions::default()
    };
    let terrain = generate_terrain(options, TerrainOptions::default()).unwrap();
    for stage in [
        OceanStage::Initial,
        OceanStage::InitialErosion,
        OceanStage::Lod2,
        OceanStage::Refinement(2),
        OceanStage::Lod1,
        OceanStage::Refinement(4),
        OceanStage::Refinement(5),
        OceanStage::FinalTerrain,
    ] {
        let report = terrain
            .ocean_passes
            .iter()
            .find(|p| p.stage == stage)
            .unwrap();
        assert!(report.seas_before >= 1);
        assert_eq!(report.seas_after, 1);
    }
    assert_eq!(terrain.ocean_passes.len(), 8);
    assert_eq!(
        terrain.ocean_passes.last().unwrap().stage,
        OceanStage::FinalTerrain
    );
    terrain.validate().unwrap();
    let separate = generate_terrain(
        options,
        TerrainOptions {
            connect_seas: false,
            ..TerrainOptions::default()
        },
    )
    .unwrap();
    assert!(separate.ocean_passes.is_empty());
    assert!(separate.stats().sea_components > 1);
}

#[test]
fn filled_basins_keep_wet_spill_channels_and_open_ocean_mouths() {
    let mesh = planet_rs::generate(PlanetOptions {
        vertex_count: 1024,
        ..PlanetOptions::default()
    })
    .unwrap();
    let basin: Vec<_> = mesh
        .directions
        .iter()
        .enumerate()
        .filter_map(|(i, p)| (p.x > 0.98).then_some(i))
        .collect();
    assert!(basin.len() >= 3);
    let heights = mesh
        .directions
        .iter()
        .map(|p| {
            if p.x < -0.4 || p.x > 0.98 {
                -0.02
            } else {
                0.025
            }
        })
        .collect();
    let original_ocean: Vec<_> = mesh
        .directions
        .iter()
        .enumerate()
        .filter_map(|(i, p)| (p.x < -0.4).then_some(i))
        .collect();
    let terrain = planet_rs::PlanetTerrain::from_elevations(
        mesh,
        heights,
        42,
        TerrainOptions {
            refinement_passes: 0,
            hydraulic_passes: 0,
            thermal_passes: 0,
            smoothing_passes: 0,
            river_catchment: 0.0005,
            river_depth: 0.002,
            ..TerrainOptions::default()
        },
    )
    .unwrap();
    assert_eq!(terrain.ocean_passes[0].filled_seas, 1);
    assert_eq!(
        terrain.ocean_passes.last().unwrap().stage,
        OceanStage::FinalTerrain
    );
    assert!(
        basin.iter().any(|&i| terrain.water_elevations[i] > 0.0
            && terrain.water_elevations[i] > terrain.elevations[i]),
        "filled basin must keep a wet supported spill channel"
    );
    assert!(
        original_ocean.iter().all(|&i| terrain.elevations[i] < 0.0),
        "spill support must never refill any captured ocean root"
    );
    assert!(
        terrain
            .rivers
            .iter()
            .any(|river| terrain.elevations[river.to] < 0.0
                && terrain.drainage.receivers[river.to].is_none()),
        "a real terminal river mouth must remain open to the ocean"
    );
    assert!(!terrain.river_surface.triangles.is_empty());
    terrain.validate().unwrap();
}

#[test]
fn impact_seas_are_filled_at_every_terrain_stage_including_without_refinement() {
    for (seed, source, refinements) in [
        (42, TerrainSource::Tectonics, 0),
        (42, TerrainSource::Tectonics, 3),
        (73, TerrainSource::Tectonics, 3),
    ] {
        let terrain = generate_terrain(
            PlanetOptions {
                seed,
                vertex_count: 256,
                ..PlanetOptions::default()
            },
            TerrainOptions {
                source,
                refinement_passes: refinements,
                river_catchment: if refinements == 0 { 1.0 } else { 0.0015 },
                impacts: ImpactOptions {
                    enabled: true,
                    per_stage: 8,
                    strength: 3.0,
                    ..ImpactOptions::default()
                },
                ..TerrainOptions::default()
            },
        )
        .unwrap();
        assert!(!terrain.impact_craters.is_empty());
        for stage in 1..=refinements {
            assert!(
                terrain
                    .ocean_passes
                    .iter()
                    .any(|p| p.stage == OceanStage::Impact(stage))
            );
        }
        assert!(terrain.ocean_passes.iter().all(|p| p.seas_after <= 1));
        assert_eq!(
            terrain.ocean_passes.last().unwrap().stage,
            OceanStage::FinalTerrain
        );
        terrain.validate().unwrap();
    }
}
