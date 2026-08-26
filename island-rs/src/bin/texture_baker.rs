//! Standalone procedural texture baker and editor protocol endpoint.

#![allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]

use std::{
    env,
    error::Error,
    fs, io,
    path::{Component, Path, PathBuf},
    process,
    time::Instant,
};

use motu::procedural_textures::{
    EditorEnvelope, NormalConvention, OutputOptions, OutputProfile, TextureRecipe,
    editor_protocol::{self, Diagnostic},
    encoding::{
        OutputDimensions, PixelFormat, TextureSetImages, encode_png_bytes,
        write_texture_set as write_encoded_texture_set,
    },
    evaluate_material, generate_preview, generate_texture_set, layer_preview_maps,
    preview::{LayerPreviewMaps, PreviewSettings},
};
use serde_json::{Value, json};

const VERSION: &str = env!("CARGO_PKG_VERSION");

fn main() {
    match run() {
        Ok(()) => {}
        Err(error) => {
            eprintln!("island-texture-baker: {error}");
            process::exit(2);
        }
    }
}

fn run() -> Result<(), Box<dyn Error>> {
    match parse(env::args().skip(1))? {
        ParseResult::Help => {
            print_help();
            Ok(())
        }
        ParseResult::Version => {
            println!("island-texture-baker {VERSION}");
            Ok(())
        }
        ParseResult::Bake(command) => run_bake(&command),
        ParseResult::Schema => print_editor_envelope(&schema_envelope()),
        ParseResult::Validate { recipe } => print_editor_envelope(&validate_command(&recipe)),
        ParseResult::Preview {
            recipe,
            output,
            size,
            normal_convention,
        } => print_editor_envelope(&preview_command(&recipe, &output, size, normal_convention)),
    }
}

#[derive(Debug)]
struct BakeCommand {
    recipe: PathBuf,
    output: PathBuf,
    profile: OutputProfile,
    force: bool,
    seed: Option<u64>,
    width: Option<u32>,
    height: Option<u32>,
    normal_convention: NormalConvention,
}

#[derive(Debug)]
enum ParseResult {
    Help,
    Version,
    Bake(BakeCommand),
    Schema,
    Validate {
        recipe: PathBuf,
    },
    Preview {
        recipe: PathBuf,
        output: PathBuf,
        size: u32,
        normal_convention: NormalConvention,
    },
}

fn parse(mut arguments: impl Iterator<Item = String>) -> Result<ParseResult, String> {
    let Some(first) = arguments.next() else {
        return Err("--recipe is required; use --help for usage".into());
    };
    if first == "-h" || first == "--help" {
        return Ok(ParseResult::Help);
    }
    if first == "-V" || first == "--version" {
        return Ok(ParseResult::Version);
    }
    match first.as_str() {
        "schema" => parse_schema(arguments),
        "validate" => parse_validate(arguments),
        "preview" => parse_preview(arguments),
        "bake" => parse_bake(arguments),
        _ => parse_bake(std::iter::once(first).chain(arguments)),
    }
}

fn parse_schema(arguments: impl Iterator<Item = String>) -> Result<ParseResult, String> {
    for argument in arguments {
        if argument != "--json" && argument != "-h" && argument != "--help" {
            return Err(format!("unknown schema option {argument:?}"));
        }
        if argument == "-h" || argument == "--help" {
            return Ok(ParseResult::Help);
        }
    }
    Ok(ParseResult::Schema)
}

fn parse_validate(mut arguments: impl Iterator<Item = String>) -> Result<ParseResult, String> {
    let mut recipe = None;
    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "--recipe" => recipe = Some(next_value(&mut arguments, "--recipe")?),
            "--json" => {}
            "-h" | "--help" => return Ok(ParseResult::Help),
            _ => return Err(format!("unknown validate option {argument:?}")),
        }
    }
    Ok(ParseResult::Validate {
        recipe: PathBuf::from(recipe.ok_or("--recipe is required")?),
    })
}

fn parse_preview(mut arguments: impl Iterator<Item = String>) -> Result<ParseResult, String> {
    let mut recipe = None;
    let mut output = None;
    let mut size = 256;
    let mut normal_convention = None;
    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "--recipe" => recipe = Some(next_value(&mut arguments, "--recipe")?),
            "--output" | "-o" => output = Some(next_value(&mut arguments, "--output")?),
            "--size" => size = parse_value("--size", &next_value(&mut arguments, "--size")?)?,
            "--normal-convention" => {
                normal_convention = Some(parse_normal_convention(&next_value(
                    &mut arguments,
                    "--normal-convention",
                )?)?);
            }
            "--json" => {}
            "-h" | "--help" => return Ok(ParseResult::Help),
            _ => return Err(format!("unknown preview option {argument:?}")),
        }
    }
    if size == 0 {
        return Err("--size must be greater than zero".into());
    }
    Ok(ParseResult::Preview {
        recipe: PathBuf::from(recipe.ok_or("--recipe is required")?),
        output: PathBuf::from(output.ok_or("--output is required")?),
        size,
        normal_convention: normal_convention.ok_or("--normal-convention is required")?,
    })
}

fn parse_bake(mut arguments: impl Iterator<Item = String>) -> Result<ParseResult, String> {
    let mut recipe = None;
    let mut output = None;
    let mut profile = OutputProfile::Separate;
    let mut force = false;
    let mut seed = None;
    let mut width = None;
    let mut height = None;
    let mut normal_convention = None;
    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "--recipe" => recipe = Some(next_value(&mut arguments, "--recipe")?),
            "--output" | "-o" => output = Some(next_value(&mut arguments, "--output")?),
            "--profile" => {
                profile = next_value(&mut arguments, "--profile")?
                    .parse::<OutputProfile>()
                    .map_err(|error| error.to_string())?;
            }
            "--force" => force = true,
            "--seed" => {
                seed = Some(parse_value(
                    "--seed",
                    &next_value(&mut arguments, "--seed")?,
                )?);
            }
            "--width" => {
                width = Some(parse_value(
                    "--width",
                    &next_value(&mut arguments, "--width")?,
                )?);
            }
            "--height" => {
                height = Some(parse_value(
                    "--height",
                    &next_value(&mut arguments, "--height")?,
                )?);
            }
            "--normal-convention" => {
                normal_convention = Some(parse_normal_convention(&next_value(
                    &mut arguments,
                    "--normal-convention",
                )?)?);
            }
            "-h" | "--help" => return Ok(ParseResult::Help),
            _ => return Err(format!("unknown option {argument:?}; use --help for usage")),
        }
    }
    if width == Some(0) || height == Some(0) {
        return Err("image width and height must be greater than zero".into());
    }
    Ok(ParseResult::Bake(BakeCommand {
        recipe: PathBuf::from(recipe.ok_or("--recipe is required")?),
        output: PathBuf::from(output.ok_or("--output is required")?),
        profile,
        force,
        seed,
        width,
        height,
        normal_convention: normal_convention.ok_or("--normal-convention is required")?,
    }))
}

fn parse_normal_convention(value: &str) -> Result<NormalConvention, String> {
    match value {
        "open-gl" | "open_gl" => Ok(NormalConvention::OpenGl),
        "direct-x" | "direct_x" => Ok(NormalConvention::DirectX),
        _ => Err(format!(
            "invalid value for --normal-convention: {value}; expected open-gl or direct-x"
        )),
    }
}

fn next_value(
    arguments: &mut impl Iterator<Item = String>,
    option: &str,
) -> Result<String, String> {
    arguments
        .next()
        .ok_or_else(|| format!("{option} requires a value"))
}

fn parse_value<T>(option: &str, value: &str) -> Result<T, String>
where
    T: std::str::FromStr,
    T::Err: std::fmt::Display,
{
    value
        .parse()
        .map_err(|error| format!("invalid value for {option}: {error}"))
}

fn run_bake(command: &BakeCommand) -> Result<(), Box<dyn Error>> {
    let bytes = fs::read(&command.recipe).map_err(|error| {
        format!(
            "could not read recipe {}: {error}",
            command.recipe.display()
        )
    })?;
    let mut document: Value = serde_json::from_slice(&bytes)?;
    if let Some(seed) = command.seed {
        document["seed"] = Value::from(seed);
    }
    if let Some(width) = command.width {
        document["width"] = Value::from(width);
    }
    if let Some(height) = command.height {
        document["height"] = Value::from(height);
    }
    let recipe: TextureRecipe = serde_json::from_value(document)?;
    let started = Instant::now();
    let textures = generate_texture_set(&recipe, command.normal_convention)?;
    let images = TextureSetImages::from_texture_set(&textures);
    let manifest = write_encoded_texture_set(
        &images,
        &command.output,
        &OutputOptions {
            profile: command.profile,
            force: command.force,
        },
    )?;
    println!(
        "baked {} ({}x{}, seed {}) in {:.2?}",
        manifest.name,
        manifest.dimensions.width,
        manifest.dimensions.height,
        manifest.metadata.seed,
        started.elapsed()
    );
    for map in &manifest.maps {
        println!("  {}  sha256={}", map.file, map.sha256);
    }
    Ok(())
}

fn read_recipe(path: &Path) -> Result<TextureRecipe, Diagnostic> {
    let bytes = fs::read(path).map_err(|error| Diagnostic {
        pointer: String::new(),
        severity: "error",
        code: "recipe.read",
        message: format!("could not read recipe {}: {error}", path.display()),
    })?;
    serde_json::from_slice(&bytes).map_err(|error| Diagnostic {
        pointer: String::new(),
        severity: "error",
        code: "recipe.parse",
        message: error.to_string(),
    })
}

fn schema_envelope() -> EditorEnvelope {
    let mut envelope = EditorEnvelope::success();
    envelope.schema = Some(editor_protocol::schema_document());
    envelope
}

fn validate_command(path: &Path) -> EditorEnvelope {
    let recipe = match read_recipe(path) {
        Ok(recipe) => recipe,
        Err(diagnostic) => {
            return EditorEnvelope {
                success: false,
                diagnostics: vec![diagnostic],
                ..EditorEnvelope::default()
            };
        }
    };
    let mut envelope = EditorEnvelope::success();
    envelope.recipe_hash = editor_protocol::recipe_hash(&recipe).ok();
    envelope.diagnostics = editor_protocol::validate_diagnostics(&recipe);
    envelope.success = envelope.diagnostics.is_empty();
    envelope
}

fn preview_command(
    path: &Path,
    output: &Path,
    size: u32,
    normal_convention: NormalConvention,
) -> EditorEnvelope {
    let recipe = match read_recipe(path) {
        Ok(recipe) => recipe,
        Err(diagnostic) => {
            return EditorEnvelope {
                success: false,
                diagnostics: vec![diagnostic],
                ..EditorEnvelope::default()
            };
        }
    };
    let mut preview_recipe = recipe.clone();
    preview_recipe.width = size;
    preview_recipe.height = size;
    let source_recipe_hash = editor_protocol::recipe_hash(&recipe).ok();
    let diagnostics = editor_protocol::validate_diagnostics(&preview_recipe);
    if !diagnostics.is_empty() {
        return EditorEnvelope {
            success: false,
            diagnostics,
            recipe_hash: source_recipe_hash.clone(),
            ..EditorEnvelope::default()
        };
    }
    if let Err(error) = clear_previous_preview(output, &preview_recipe.name) {
        return EditorEnvelope::failure("preview.cleanup", error.to_string());
    }
    let started = Instant::now();
    let preview = match generate_preview(
        &preview_recipe,
        &PreviewSettings {
            normal_convention,
            selected_layer_id: None,
        },
    ) {
        Ok(preview) => preview,
        Err(error) => return EditorEnvelope::failure("preview.generate", error.to_string()),
    };
    // The CLI historically emits diagnostics for every layer. Keep that
    // protocol contract while the in-process preview result retains only the
    // selected layer to avoid allocating all diagnostic maps for UI callers.
    let evaluation = match evaluate_material(&preview_recipe) {
        Ok(evaluation) => evaluation,
        Err(error) => return EditorEnvelope::failure("preview.layer_maps", error.to_string()),
    };
    let layer_maps = match layer_preview_maps(&evaluation) {
        Ok(layer_maps) => layer_maps,
        Err(error) => return EditorEnvelope::failure("preview.layer_maps", error.to_string()),
    };
    let manifest = match write_encoded_texture_set(
        &TextureSetImages::from_texture_set(&preview.textures),
        output,
        &OutputOptions {
            profile: OutputProfile::MotuUnityTerrain,
            force: true,
        },
    ) {
        Ok(manifest) => manifest,
        Err(error) => return EditorEnvelope::failure("preview.write", error.to_string()),
    };
    let mut generated_maps = manifest
        .maps
        .iter()
        .map(|map| serde_json::to_value(map).unwrap_or_else(|_| json!({})))
        .collect::<Vec<_>>();
    if let Err(error) = write_layer_maps(output, &preview_recipe, &layer_maps, &mut generated_maps)
    {
        return EditorEnvelope::failure("preview.layer_maps", error.to_string());
    }
    if let Err(error) = write_raw_height(
        output,
        &preview_recipe,
        &preview.textures,
        &manifest.metadata.recipe_hash,
        &mut generated_maps,
    ) {
        return EditorEnvelope::failure("preview.raw_height", error.to_string());
    }
    if let Err(error) = write_preview_manifest(output, &preview_recipe, &manifest, &generated_maps)
    {
        return EditorEnvelope::failure("preview.manifest", error.to_string());
    }
    let mut envelope = EditorEnvelope::success();
    envelope.recipe_hash = source_recipe_hash;
    envelope.generated_maps = generated_maps;
    envelope
        .timings_ms
        .insert("evaluate".into(), preview.timings_ms.evaluate_ms);
    envelope.timings_ms.insert(
        "write".into(),
        started.elapsed().as_secs_f64() * 1000.0 - preview.timings_ms.evaluate_ms,
    );
    envelope.timings = envelope.timings_ms.clone();
    envelope
}

fn write_preview_manifest(
    output: &Path,
    recipe: &TextureRecipe,
    manifest: &motu::procedural_textures::OutputManifest,
    generated_maps: &[Value],
) -> Result<(), io::Error> {
    let bytes = serde_json::to_vec_pretty(&json!({
        "kind": "procedural_material_preview",
        "complete": true,
        "recipe_hash": manifest.metadata.recipe_hash,
        "width": recipe.width,
        "height": recipe.height,
        "maps": generated_maps,
    }))
    .map_err(|error| io::Error::other(error.to_string()))?;
    atomic_write(&output.join("preview.manifest.json"), &bytes)
}

fn clear_previous_preview(output: &Path, name: &str) -> Result<(), io::Error> {
    let marker = output.join("preview.manifest.json");
    if !marker.is_file() {
        return Ok(());
    }
    let previous: Value = serde_json::from_slice(&fs::read(&marker)?)
        .map_err(|error| io::Error::other(error.to_string()))?;
    let mut paths = Vec::new();
    if let Some(maps) = previous.get("maps").and_then(Value::as_array) {
        for map in maps {
            if let Some(file) = map.get("file").and_then(Value::as_str) {
                paths.push(preview_manifest_path(output, file)?);
            }
            if let Some(metadata) = map.get("metadata").and_then(Value::as_str) {
                paths.push(preview_manifest_path(output, metadata)?);
            }
        }
    }
    paths.push(preview_manifest_path(
        output,
        &format!("{name}.texture-set.json"),
    )?);
    for path in paths {
        if path.is_file() {
            fs::remove_file(path)?;
        }
    }
    fs::remove_file(marker)
}

fn preview_manifest_path(output: &Path, relative: &str) -> Result<PathBuf, io::Error> {
    let relative_path = Path::new(relative);
    if relative_path.as_os_str().is_empty()
        || relative_path.is_absolute()
        || relative_path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("preview manifest path {relative:?} is not relative to the output directory"),
        ));
    }

    let output_root = fs::canonicalize(output)?;
    let candidate = output.join(relative_path);
    let contained_path = match fs::canonicalize(&candidate) {
        Ok(path) => path,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            let parent = candidate.parent().unwrap_or(output);
            fs::canonicalize(parent)?
        }
        Err(error) => return Err(error),
    };
    if !contained_path.starts_with(&output_root) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("preview manifest path {relative:?} resolves outside the output directory"),
        ));
    }
    Ok(candidate)
}

fn write_layer_maps(
    output: &Path,
    recipe: &TextureRecipe,
    layer_maps: &[(String, LayerPreviewMaps)],
    generated_maps: &mut Vec<Value>,
) -> Result<(), io::Error> {
    let dimensions = OutputDimensions {
        width: recipe.width,
        height: recipe.height,
    };
    for (id, layer_maps) in layer_maps {
        for (suffix, values) in [
            ("raw", layer_maps.raw.pixels()),
            ("remapped", layer_maps.remapped.pixels()),
            ("mask", layer_maps.mask.pixels()),
        ] {
            let pixels = normalize_scalar(values);
            let filename = format!("{}_layer_{}_{}.png", recipe.name, id, suffix);
            let bytes = encode_png_bytes(dimensions, PixelFormat::Gray8, &pixels)
                .map_err(|error| io::Error::other(error.to_string()))?;
            atomic_write(&output.join(&filename), &bytes)?;
            generated_maps.push(json!({
                "file": filename,
                "format": "Gray8",
                "kind": format!("layer_{suffix}"),
                "width": recipe.width,
                "height": recipe.height,
            }));
        }
    }
    Ok(())
}

fn write_raw_height(
    output: &Path,
    recipe: &TextureRecipe,
    textures: &motu::procedural_textures::TextureSet,
    recipe_hash: &str,
    generated_maps: &mut Vec<Value>,
) -> Result<(), io::Error> {
    let range = motu::procedural_textures::packing::HeightRange::new(
        recipe.displacement.minimum_m,
        recipe.displacement.maximum_m,
        recipe.displacement.base_m,
    )
    .map_err(|error| io::Error::other(format!("{error:?}")))?;
    let pixels = &textures.height;
    let mut bytes = Vec::with_capacity(pixels.pixels().len() * 2);
    for pixel in pixels.pixels() {
        bytes.extend_from_slice(&pixel.to_le_bytes());
    }
    let filename = format!("{}_preview_height.r16", recipe.name);
    atomic_write(&output.join(&filename), &bytes)?;
    let metadata_name = format!("{}_preview_height.json", recipe.name);
    let metadata = serde_json::to_vec_pretty(&json!({
        "file": filename,
        "width": recipe.width,
        "height": recipe.height,
        "endianness": "little",
        "row_order": "top_to_bottom",
        "minimum_m": range.minimum,
        "maximum_m": range.maximum,
        "base_m": range.neutral,
        "recipe_hash": recipe_hash,
    }))
    .map_err(|error| io::Error::other(error.to_string()))?;
    atomic_write(&output.join(&metadata_name), &metadata)?;
    generated_maps.push(json!({
        "file": filename,
        "metadata": metadata_name,
        "format": "R16",
        "kind": "raw_height",
    }));
    Ok(())
}

fn normalize_scalar(values: &[f32]) -> Vec<u8> {
    let (minimum, maximum) = values.iter().fold(
        (f32::INFINITY, f32::NEG_INFINITY),
        |(minimum, maximum), value| (minimum.min(*value), maximum.max(*value)),
    );
    let span = (maximum - minimum).max(f32::EPSILON);
    values
        .iter()
        .map(|value| (((*value - minimum) / span).clamp(0.0, 1.0) * 255.0).round() as u8)
        .collect()
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), io::Error> {
    let temporary = path.with_extension(format!(
        "{}.tmp-{}",
        path.extension()
            .and_then(|extension| extension.to_str())
            .unwrap_or("bin"),
        process::id()
    ));
    fs::write(&temporary, bytes)?;
    fs::rename(temporary, path)?;
    Ok(())
}

fn print_editor_envelope(envelope: &EditorEnvelope) -> Result<(), Box<dyn Error>> {
    println!("{}", envelope.to_json()?);
    Ok(())
}

fn print_help() {
    println!(
        "island-texture-baker {VERSION}\n\n\
         Bake: island-texture-baker --recipe <FILE> --output <DIR> [OPTIONS]\n\
         Editor: island-texture-baker schema --json\n\
                 island-texture-baker validate --recipe <FILE> --json\n\
                 island-texture-baker preview --recipe <FILE> --output <DIR> --size 256 --normal-convention <CONVENTION>\n\n\
         Bake options:\n\
           --profile <PROFILE>   separate (default) or motu_unity_terrain\n\
           --seed <N>            Override recipe seed\n\
           --width <PX>          Override recipe width\n\
           --height <PX>         Override recipe height\n\
           --normal-convention <CONVENTION>\n\
                                 open-gl or direct-x (required for bake and preview)\n\
           --force               Replace an existing generated set\n\
           -h, --help            Print help\n\
           -V, --version         Print version"
    );
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    #[test]
    fn parses_editor_commands() {
        assert!(matches!(
            parse(["schema", "--json"].into_iter().map(String::from)),
            Ok(ParseResult::Schema)
        ));
        assert!(matches!(
            parse(
                ["validate", "--recipe", "stone.json", "--json"]
                    .into_iter()
                    .map(String::from)
            ),
            Ok(ParseResult::Validate { .. })
        ));
        assert!(matches!(
            parse(
                [
                    "preview",
                    "--recipe",
                    "stone.json",
                    "--output",
                    "out",
                    "--size",
                    "128",
                    "--normal-convention",
                    "open-gl",
                    "--json"
                ]
                .into_iter()
                .map(String::from)
            ),
            Ok(ParseResult::Preview { size: 128, .. })
        ));
    }

    #[test]
    fn parses_direct_bake_invocation_without_subcommand() {
        let ParseResult::Bake(command) = parse(
            [
                "--recipe",
                "stone.json",
                "--output",
                "out",
                "--seed",
                "42",
                "--normal-convention",
                "direct-x",
            ]
            .into_iter()
            .map(String::from),
        )
        .unwrap() else {
            panic!("expected bake command");
        };
        assert_eq!(command.seed, Some(42));
        assert_eq!(command.normal_convention, NormalConvention::DirectX);
    }

    #[test]
    fn generation_commands_require_the_callers_normal_convention() {
        assert_eq!(
            parse(
                ["--recipe", "stone.json", "--output", "out"]
                    .into_iter()
                    .map(String::from)
            )
            .unwrap_err(),
            "--normal-convention is required"
        );
        assert_eq!(
            parse(
                ["preview", "--recipe", "stone.json", "--output", "out"]
                    .into_iter()
                    .map(String::from)
            )
            .unwrap_err(),
            "--normal-convention is required"
        );
    }

    #[test]
    fn scalar_normalization_is_finite() {
        assert_eq!(normalize_scalar(&[2.0, 2.0]), vec![0, 0]);
        assert_eq!(normalize_scalar(&[-1.0, 1.0]), vec![0, 255]);
    }

    #[test]
    fn preview_cleanup_rejects_parent_paths_before_deleting_anything() {
        let output = unique_preview_dir("parent");
        let outside = output
            .parent()
            .expect("temporary output has a parent")
            .join(format!("island-preview-outside-{}", process::id()));
        fs::write(output.join("safe.png"), b"safe").unwrap();
        fs::write(&outside, b"outside").unwrap();
        fs::write(
            output.join("preview.manifest.json"),
            serde_json::to_vec(&json!({
                "maps": [
                    {"file": "safe.png"},
                    {"file": format!("../{}", outside.file_name().unwrap().to_string_lossy())}
                ]
            }))
            .unwrap(),
        )
        .unwrap();

        let error = clear_previous_preview(&output, "stone").unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
        assert!(output.join("safe.png").is_file());
        assert!(outside.is_file());

        let _ = fs::remove_file(outside);
        let _ = fs::remove_dir_all(output);
    }

    #[test]
    fn preview_cleanup_rejects_absolute_paths() {
        let output = unique_preview_dir("absolute");
        let outside = output
            .parent()
            .expect("temporary output has a parent")
            .join(format!("island-preview-absolute-{}.png", process::id()));
        fs::write(&outside, b"outside").unwrap();
        fs::write(
            output.join("preview.manifest.json"),
            serde_json::to_vec(&json!({"maps": [{"file": outside}]})).unwrap(),
        )
        .unwrap();

        let error = clear_previous_preview(&output, "stone").unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
        assert!(outside.is_file());

        let _ = fs::remove_file(outside);
        let _ = fs::remove_dir_all(output);
    }

    #[cfg(unix)]
    #[test]
    fn preview_cleanup_rejects_symlink_paths_outside_output() {
        use std::os::unix::fs::symlink;

        let output = unique_preview_dir("symlink");
        let outside = output
            .parent()
            .expect("temporary output has a parent")
            .join(format!("island-preview-symlink-target-{}", process::id()));
        fs::create_dir_all(&outside).unwrap();
        fs::write(outside.join("escape.png"), b"outside").unwrap();
        symlink(&outside, output.join("linked")).unwrap();
        fs::write(
            output.join("preview.manifest.json"),
            serde_json::to_vec(&json!({"maps": [{"file": "linked/escape.png"}]})).unwrap(),
        )
        .unwrap();

        let error = clear_previous_preview(&output, "stone").unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
        assert!(outside.join("escape.png").is_file());

        let _ = fs::remove_dir_all(output);
        let _ = fs::remove_dir_all(outside);
    }

    fn unique_preview_dir(stem: &str) -> PathBuf {
        let output = env::temp_dir().join(format!("island-preview-{stem}-{}", process::id()));
        let _ = fs::remove_dir_all(&output);
        fs::create_dir_all(&output).unwrap();
        output
    }
}
