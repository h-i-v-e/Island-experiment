use island_building::{
    BuildingRecipe, Facade, MaterialSlot, RoofCovering, WallStyle, generate, generate_with_base,
    units::MaterialGrid,
};
use std::{fs, path::PathBuf};

#[test]
fn facade_charts_start_at_each_left_corner_and_keep_courses_continuous_across_openings() {
    let mut recipe = BuildingRecipe::hut();
    recipe.atlas_size = 128;
    let prototype = generate(&recipe).unwrap();
    for lod in &prototype.lods[..2] {
        let wall = &lod
            .parts
            .iter()
            .find(|p| p.material == MaterialSlot::Wall)
            .unwrap()
            .mesh;
        for ((point, normal), uv) in wall.positions.iter().zip(&wall.normals).zip(&wall.uvs) {
            let facade = Facade::ALL
                .into_iter()
                .find(|f| {
                    f.normal()
                        .iter()
                        .zip(normal)
                        .map(|(a, b)| a * b)
                        .sum::<f32>()
                        > 0.99
                })
                .unwrap();
            let along = point[0] * facade.tangent()[0] + point[1] * facade.tangent()[1];
            assert!((uv[0] * 2. - along - facade.length(&recipe) * 0.5).abs() < 0.00001);
            assert!((uv[1] * 2. - point[2]).abs() < 0.00001);
        }
    }
    for lod in &prototype.lods {
        let foundation = &lod
            .parts
            .iter()
            .find(|p| p.material == MaterialSlot::Foundation)
            .unwrap()
            .mesh;
        for ((point, normal), uv) in foundation
            .positions
            .iter()
            .zip(&foundation.normals)
            .zip(&foundation.uvs)
        {
            if normal[2].abs() < 0.1 {
                assert!((uv[1] * 2. - point[2]).abs() < 0.00001);
            }
        }
    }
}

#[test]
fn custom_recipe_grid_and_slate_relief_use_the_same_course_size() {
    let dir: PathBuf =
        std::env::temp_dir().join(format!("building-uv-units-{}", std::process::id()));
    fs::create_dir(&dir).unwrap();
    let mut material: serde_json::Value = serde_json::from_str(include_str!(
        "../../island-rs/texture-recipes/slate-roof.json"
    ))
    .unwrap();
    material["physical_tile_width_m"] = 4.0.into();
    let path = dir.join("slate.json");
    fs::write(&path, serde_json::to_vec(&material).unwrap()).unwrap();
    let mut recipe = BuildingRecipe::hut();
    recipe.atlas_size = 128;
    recipe.chimney = None;
    recipe.walls.style = WallStyle::Plain;
    recipe.roof.covering = RoofCovering::Slate;
    recipe.materials.roof_recipe = Some("slate.json".into());
    let grid = MaterialGrid::load(&recipe, &dir, MaterialSlot::Roof).unwrap();
    assert!((grid.metres[0] - 0.5).abs() < 0.00001);
    let prototype = generate_with_base(&recipe, &dir).unwrap();
    let roof = &prototype.lods[0]
        .parts
        .iter()
        .find(|p| p.material == MaterialSlot::Roof)
        .unwrap()
        .mesh;
    let mut lips = 0;
    for ((point, normal), uv) in roof.positions.iter().zip(&roof.normals).zip(&roof.uvs) {
        let base = recipe.wall_height
            + recipe.roof.rise * (1. - point[0].abs() / (recipe.width * 0.5))
            + recipe.roof.thickness;
        if normal[2] > 0.1 && (point[2] - base - 0.025).abs() < 0.0001 {
            let course = uv[0] * 2. / grid.metres[0];
            assert!(
                (course - course.round()).abs() < 0.0001,
                "relief lips match texture courses"
            );
            lips += 1;
        }
    }
    assert!(lips > 10);
    material["physical_tile_width_m"] = 2.0.into();
    fs::write(&path, serde_json::to_vec(&material).unwrap()).unwrap();
    assert!(
        (MaterialGrid::load(&recipe, &dir, MaterialSlot::Roof)
            .unwrap()
            .metres[0]
            - 0.25)
            .abs()
            < 0.00001
    );
    fs::write(&path, b"invalid json").unwrap();
    assert!(MaterialGrid::load(&recipe, &dir, MaterialSlot::Roof).is_err());
    fs::remove_dir_all(dir).unwrap();
}
