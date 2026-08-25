use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
    sync::atomic::{AtomicUsize, Ordering},
};

use motu::procedural_textures::{
    OutputOptions, OutputProfile, TextureRecipe, generate_texture_set, write_texture_set,
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
                "d547f69e724b20fca0b4c520a8f8de8b3cb9dda37e0a521c244bf5459347d5f0",
                "e73d6600817563f29760ca087477799078fdb91c04e4156bf800de3823c4ab2b",
                "34168a0c6ef4f81049881116639d5f5e4059d2bfc85cf0fe0d64e2694241d91e",
                "11f0734a36eed8a8a4b5215cb27c209cd61268db9ced8e3ccc2584a132bbf8a2",
                "16abd2939b9f00e2de48d8ad155661111b164c5b66c26c9d338cdb282030aee1",
            ],
        ),
    ];

    for (recipe_name, expected_hashes) in cases {
        let mut recipe = load_recipe(recipe_name);
        recipe.width = GOLDEN_SIZE;
        recipe.height = GOLDEN_SIZE;
        let textures = generate_texture_set(&recipe).expect("generate committed recipe");
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
