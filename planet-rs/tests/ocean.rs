use planet_rs::{OceanStage, PlanetOptions, TerrainOptions, generate_terrain};

#[test]
fn only_lod2_and_lod1_connect_seas_and_later_disconnections_are_valid() {
    let options = PlanetOptions {
        seed: 42,
        vertex_count: 256,
        ..PlanetOptions::default()
    };
    let terrain = generate_terrain(options, TerrainOptions::default()).unwrap();
    for stage in [OceanStage::Lod2, OceanStage::Lod1] {
        let report = terrain
            .ocean_passes
            .iter()
            .find(|p| p.stage == stage)
            .unwrap();
        assert!(report.seas_before >= 1);
        assert_eq!(report.seas_after, 1);
    }
    assert_eq!(terrain.ocean_passes.len(), 2);
    assert!(terrain.stats().sea_components > 1);
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
