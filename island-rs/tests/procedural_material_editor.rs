use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
    sync::atomic::{AtomicUsize, Ordering},
};

use motu::procedural_textures::{
    LayerMask, MaterialLayer, NormalConvention, OutputOptions, OutputProfile, TextureRecipe,
    evaluate_material, generate_texture_set, validate_recipe, write_texture_set,
};
use serde_json::Value;

const BAKER: &str = env!("CARGO_BIN_EXE_island-texture-baker");
const GOLDEN_SIZE: u32 = 128;
static NEXT_TEST_DIRECTORY: AtomicUsize = AtomicUsize::new(0);

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn new(label: &str) -> Self {
        let unique = NEXT_TEST_DIRECTORY.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "motu-procedural-material-{label}-{}-{unique}",
            std::process::id()
        ));
        fs::create_dir_all(&path).expect("create test directory");
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TestDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn recipe_path(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("texture-recipes")
        .join(name)
}

fn load_recipe(name: &str) -> TextureRecipe {
    serde_json::from_slice(&fs::read(recipe_path(name)).expect("read committed recipe"))
        .expect("load current recipe shape")
}

#[test]
fn all_committed_recipes_use_the_current_valid_schema() {
    for recipe_name in [
        "Bark.json",
        "FallenStones.json",
        "ForestFloor.json",
        "PlateBark.json",
        "cracked-stone.json",
        "rounded-river-stones.json",
    ] {
        let recipe = load_recipe(recipe_name);
        validate_recipe(&recipe).unwrap_or_else(|error| panic!("{recipe_name}: {error}"));
    }
}

fn run_baker(arguments: &[&str]) -> Output {
    Command::new(BAKER)
        .args(arguments)
        .output()
        .expect("run texture baker")
}

fn json_stdout(output: &Output) -> Value {
    serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
        panic!(
            "baker stdout was not JSON: {error}; stdout={}; stderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    })
}

#[test]
fn committed_recipes_retain_the_locked_128_pixel_map_hashes() {
    let cases = [
        (
            "cracked-stone.json",
            [
                "6a83481118b0fd5c33b9b5e733b8f1315eeb9a49448f98a95eee73f47e4852d0",
                "bb7bdcb8e6b4fd1f39d0ff0028297eb89bdb66e9355acff94082f4823731ffcf",
                "647ec5d03b221e419d5ad13e58c0acc3a38acd81ed4e7890337057fb43326704",
                "24162171f56b2b122ba0def72a3e0676523ab72ff6d1231b990fa0b94b124ae9",
                "e16702dc73d773d06c290397c38c0b5d61e39dad281b71672f3e057d055eee8f",
            ],
        ),
        (
            "rounded-river-stones.json",
            [
                "e24789a54569d84ae21f4443281ef6eae5a8fed6c8e8f033f53b9f879b7231cb",
                "ae0e92cd4de04c32975ef2fa58ccf0b1656a0377fc0bbb1b4cee15a1b43dce09",
                "2e11eb58cd16aa7f0e7e50cc4a278eaf1363a6e93900c77e56699a03c190871c",
                "f735d58824b221134e2da0e1ca719cf75e019a79fd1276c8c15f8190c1538799",
                "5367b3a657d7dbfbe9fee568e4c045303fcddea0a95b6510550e5953391fa11d",
            ],
        ),
    ];

    for (recipe_name, expected_hashes) in cases {
        let mut recipe = load_recipe(recipe_name);
        recipe.width = GOLDEN_SIZE;
        recipe.height = GOLDEN_SIZE;
        let textures = generate_texture_set(&recipe, NormalConvention::OpenGl)
            .expect("generate committed recipe");
        let directory = TestDirectory::new(recipe_name.trim_end_matches(".json"));
        let manifest = write_texture_set(
            &textures,
            directory.path(),
            &OutputOptions {
                profile: OutputProfile::MotuUnityTerrain,
                force: false,
            },
        )
        .expect("write golden texture set");
        let actual_hashes = manifest
            .maps
            .iter()
            .map(|map| map.sha256.as_str())
            .collect::<Vec<_>>();
        assert_eq!(actual_hashes, expected_hashes, "{recipe_name} changed");
    }
}

#[test]
fn previous_height_mask_tracks_the_rounded_stone_base_field() {
    let mut recipe = load_recipe("rounded-river-stones.json");
    recipe.width = 64;
    recipe.height = 64;
    recipe.layers.clear();

    let base_evaluation = evaluate_material(&recipe).expect("evaluate rounded stones base field");
    let (minimum, maximum) = base_evaluation.layers.field.values().iter().copied().fold(
        (f32::INFINITY, f32::NEG_INFINITY),
        |(minimum, maximum), height| (minimum.min(height), maximum.max(height)),
    );
    let range = maximum - minimum;
    assert!(
        range > f32::EPSILON,
        "expected a non-flat stone height field"
    );
    let bottom_m = minimum + range * 0.25;
    let top_m = minimum + range * 0.75;

    recipe.layers = vec![MaterialLayer {
        id: "gap-colour".into(),
        name: "Gap colour".into(),
        mask: Some(LayerMask::PreviousHeight {
            bottom_m,
            top_m,
            invert: true,
        }),
        ..MaterialLayer::default()
    }];

    let evaluation = evaluate_material(&recipe).expect("evaluate rounded stones");
    let heights = evaluation.layers.field.values();
    let mask = &evaluation.layers.layers[0].mask;
    let low_count = heights
        .iter()
        .zip(mask)
        .filter(|(height, opacity)| **height <= bottom_m && **opacity >= 1.0 - f32::EPSILON)
        .count();
    let high_count = heights
        .iter()
        .zip(mask)
        .filter(|(height, opacity)| **height >= top_m && **opacity <= f32::EPSILON)
        .count();

    assert!(low_count > 0, "expected the inverted mask to select gaps");
    assert!(
        high_count > 0,
        "expected the inverted mask to reject stone tops"
    );
}

#[test]
fn editor_cli_schema_validation_preview_and_failure_envelopes_are_valid_json() {
    let schema = run_baker(&["schema", "--json"]);
    assert!(schema.status.success());
    let schema = json_stdout(&schema);
    assert_eq!(schema["success"], true);
    assert!(
        schema["schema"]["metadata"]
            .as_array()
            .is_some_and(|items| items.len() >= 100)
    );

    for recipe_name in ["cracked-stone.json", "rounded-river-stones.json"] {
        let recipe = recipe_path(recipe_name);
        let recipe = recipe.to_str().expect("UTF-8 recipe path");
        let validation = run_baker(&["validate", "--recipe", recipe, "--json"]);
        assert!(validation.status.success());
        let validation = json_stdout(&validation);
        assert_eq!(validation["success"], true);
        assert_eq!(validation["diagnostics"].as_array().map(Vec::len), Some(0));

        let preview_directory = TestDirectory::new("preview");
        let preview_path = preview_directory
            .path()
            .to_str()
            .expect("UTF-8 preview path");
        let preview = run_baker(&[
            "preview",
            "--recipe",
            recipe,
            "--output",
            preview_path,
            "--size",
            "64",
            "--normal-convention",
            "direct-x",
        ]);
        assert!(preview.status.success());
        let preview = json_stdout(&preview);
        assert_eq!(preview["success"], true);
        assert!(
            preview["generated_maps"]
                .as_array()
                .is_some_and(|maps| maps.len() >= 9)
        );
        assert!(
            preview_directory
                .path()
                .join("preview.manifest.json")
                .is_file()
        );
        let raw_height = preview["generated_maps"]
            .as_array()
            .and_then(|maps| maps.iter().find(|map| map["kind"] == "raw_height"))
            .expect("raw preview height map");
        let metadata_path = preview_directory.path().join(
            raw_height["metadata"]
                .as_str()
                .expect("raw preview height metadata path"),
        );
        let metadata: Value = serde_json::from_slice(
            &fs::read(metadata_path).expect("read raw preview height metadata"),
        )
        .expect("parse raw preview height metadata");
        assert_eq!(metadata["row_order"], "top_to_bottom");
    }

    let invalid_directory = TestDirectory::new("invalid");
    let invalid_recipe = invalid_directory.path().join("invalid.json");
    fs::write(&invalid_recipe, b"{ not valid JSON").expect("write invalid recipe fixture");
    let invalid = run_baker(&[
        "validate",
        "--recipe",
        invalid_recipe.to_str().expect("UTF-8 invalid recipe path"),
        "--json",
    ]);
    assert!(
        invalid.status.success(),
        "editor validation failures use a JSON envelope"
    );
    let invalid = json_stdout(&invalid);
    assert_eq!(invalid["success"], false);
    assert_eq!(invalid["diagnostics"][0]["code"], "recipe.parse");
}

#[test]
fn preview_maps_match_a_final_bake_at_the_same_resolution() {
    let recipe = recipe_path("cracked-stone.json");
    let recipe = recipe.to_str().expect("UTF-8 recipe path");
    let preview = TestDirectory::new("matching-preview");
    let bake = TestDirectory::new("matching-bake");
    let preview_output = run_baker(&[
        "preview",
        "--recipe",
        recipe,
        "--output",
        preview.path().to_str().expect("UTF-8 preview path"),
        "--size",
        "64",
        "--normal-convention",
        "open-gl",
    ]);
    assert!(preview_output.status.success());
    assert_eq!(json_stdout(&preview_output)["success"], true);
    let bake_output = run_baker(&[
        "--recipe",
        recipe,
        "--output",
        bake.path().to_str().expect("UTF-8 bake path"),
        "--profile",
        "motu_unity_terrain",
        "--width",
        "64",
        "--height",
        "64",
        "--normal-convention",
        "open-gl",
    ]);
    assert!(
        bake_output.status.success(),
        "{}",
        String::from_utf8_lossy(&bake_output.stderr)
    );

    for suffix in ["albedo", "height", "normal", "occlusion", "mask"] {
        let filename = format!("CrackedStone_{suffix}.png");
        assert_eq!(
            fs::read(preview.path().join(&filename)).expect("read preview map"),
            fs::read(bake.path().join(&filename)).expect("read final map"),
            "{suffix} preview differed from final bake"
        );
    }
}

#[test]
fn cli_colour_override_changes_only_albedo_derived_output() {
    let recipe = recipe_path("cracked-stone.json");
    let recipe = recipe.to_str().expect("UTF-8 recipe path");
    let defaults = TestDirectory::new("parameter-defaults");
    let overridden = TestDirectory::new("parameter-override");
    let bake = |output: &TestDirectory, extra: &[&str]| {
        let mut arguments = vec![
            "--recipe",
            recipe,
            "--output",
            output.path().to_str().expect("UTF-8 output path"),
            "--profile",
            "motu_unity_terrain",
            "--width",
            "64",
            "--height",
            "64",
            "--normal-convention",
            "open-gl",
        ];
        arguments.extend_from_slice(extra);
        let result = run_baker(&arguments);
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
    };
    bake(&defaults, &[]);
    bake(&overridden, &["--set-colour", "stone_colour=#805f40"]);

    assert_ne!(
        fs::read(defaults.path().join("CrackedStone_albedo.png")).unwrap(),
        fs::read(overridden.path().join("CrackedStone_albedo.png")).unwrap()
    );
    for suffix in ["height", "normal", "occlusion", "mask"] {
        let filename = format!("CrackedStone_{suffix}.png");
        assert_eq!(
            fs::read(defaults.path().join(&filename)).unwrap(),
            fs::read(overridden.path().join(&filename)).unwrap(),
            "{suffix} changed under a colour-only override"
        );
    }
}
