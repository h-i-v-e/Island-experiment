//! File-output helpers for generated procedural texture sets.
//!
//! The generator itself works with typed, linear image buffers.  This module
//! is deliberately the only place that knows about PNG colour types, packed
//! Unity masks, output filenames, and the filesystem.  Keeping that boundary
//! small means an in-process renderer can use the same buffers without
//! pulling in an image encoder.

use std::{
    borrow::Cow,
    collections::BTreeSet,
    fmt,
    fs::{self, File, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    process,
    time::{SystemTime, UNIX_EPOCH},
};

use serde::Serialize;
use sha2::{Digest, Sha256};

/// Version of the file-output contract.  This is separate from the
/// generator's algorithm version: changing PNG naming or manifest fields is a
/// compatibility change even when the source field stays the same.
pub const OUTPUT_FORMAT_VERSION: &str = "2";

/// The profiles currently supported by the standalone baker.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputProfile {
    /// Write one PNG per generated map.
    Separate,
    /// Write the separate maps and the packed mask consumed by the terrain
    /// shader (`R=height`, `G=occlusion`, `B=0`, `A=255`).
    MotuUnityTerrain,
}

impl OutputProfile {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Separate => "separate",
            Self::MotuUnityTerrain => "motu_unity_terrain",
        }
    }
}

impl std::str::FromStr for OutputProfile {
    type Err = OutputError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "separate" => Ok(Self::Separate),
            "motu_unity_terrain" => Ok(Self::MotuUnityTerrain),
            _ => Err(OutputError::InvalidInput(format!(
                "unknown output profile {value:?}; expected separate or motu_unity_terrain"
            ))),
        }
    }
}

/// Options controlling an output transaction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutputOptions {
    /// Selects separate maps or the Unity packed mask in addition to them.
    pub profile: OutputProfile,
    /// Permit replacing files that belong to a previous generated set.
    pub force: bool,
}

impl Default for OutputOptions {
    fn default() -> Self {
        Self {
            profile: OutputProfile::Separate,
            force: false,
        }
    }
}

/// Dimensions shared by all maps in one texture set.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct OutputDimensions {
    pub width: u32,
    pub height: u32,
}

impl OutputDimensions {
    /// Return the checked number of pixels.
    ///
    /// # Errors
    ///
    /// Returns [`OutputError::InvalidInput`] when the dimensions overflow the
    /// platform's addressable pixel count.
    pub fn pixel_count(self) -> Result<usize, OutputError> {
        usize::try_from(self.width)
            .ok()
            .and_then(|width| {
                usize::try_from(self.height)
                    .ok()
                    .and_then(|height| width.checked_mul(height))
            })
            .ok_or_else(|| OutputError::InvalidInput("image dimensions overflow usize".into()))
    }

    fn byte_count(self, channels: usize) -> Result<usize, OutputError> {
        self.pixel_count()?
            .checked_mul(channels)
            .ok_or_else(|| OutputError::InvalidInput("image byte count overflows usize".into()))
    }
}

/// Metadata copied into the manifest by the caller that owns the procedural
/// recipe.  All values are explicit so a manifest can be understood without
/// loading a Rust type or the original recipe file.
#[derive(Debug, Clone, Serialize)]
pub struct TextureMetadata {
    pub generator_algorithm_version: String,
    pub recipe_hash: String,
    pub parameter_hash: String,
    pub seed: u64,
    pub physical_tile_width_m: f32,
    pub physical_tile_height_m: f32,
    pub height_min_m: f32,
    pub height_max_m: f32,
    pub neutral_height_m: f32,
    pub displacement_map: bool,
    pub normal_convention: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub overrides: Option<serde_json::Value>,
}

impl Default for TextureMetadata {
    fn default() -> Self {
        Self {
            generator_algorithm_version: "1".into(),
            recipe_hash: String::new(),
            parameter_hash: String::new(),
            seed: 0,
            physical_tile_width_m: 1.0,
            physical_tile_height_m: 1.0,
            height_min_m: 0.0,
            height_max_m: 1.0,
            neutral_height_m: 0.0,
            displacement_map: true,
            normal_convention: "open_gl".into(),
            overrides: None,
        }
    }
}

/// Borrowed image buffers ready for encoding.  RGB and RGBA data are tightly
/// packed bytes in row-major order; the height map is kept as host-endian
/// `u16` until PNG encoding, where it is written in the required network byte
/// order.
#[derive(Debug)]
pub struct TextureSetImages<'a> {
    pub name: Cow<'a, str>,
    pub dimensions: OutputDimensions,
    pub albedo_rgb8: Cow<'a, [u8]>,
    pub height_gray16: Cow<'a, [u16]>,
    pub normal_rgb8: Cow<'a, [u8]>,
    pub occlusion_gray8: Cow<'a, [u8]>,
    pub metadata: TextureMetadata,
}

impl<'a> TextureSetImages<'a> {
    /// Build an image view and defer all validation until writing.
    #[must_use]
    pub fn new(
        name: impl Into<Cow<'a, str>>,
        dimensions: OutputDimensions,
        albedo_rgb8: &'a [u8],
        height_gray16: &'a [u16],
        normal_rgb8: &'a [u8],
        occlusion_gray8: &'a [u8],
        metadata: TextureMetadata,
    ) -> Self {
        Self {
            name: name.into(),
            dimensions,
            albedo_rgb8: Cow::Borrowed(albedo_rgb8),
            height_gray16: Cow::Borrowed(height_gray16),
            normal_rgb8: Cow::Borrowed(normal_rgb8),
            occlusion_gray8: Cow::Borrowed(occlusion_gray8),
            metadata,
        }
    }

    /// Adapt the core engine-neutral texture set to the file-output boundary.
    ///
    /// RGB pixels are flattened into owned byte buffers because the core image
    /// uses typed `[u8; 3]` pixels while PNG encoders consume tightly packed
    /// bytes. Height and occlusion remain borrowed from the owned set.
    #[must_use]
    pub fn from_texture_set(textures: &'a super::image::TextureSet) -> Self {
        let core_metadata = &textures.metadata;
        let normal_convention = match core_metadata.normal_convention {
            super::image::NormalConvention::OpenGl => "open_gl",
            super::image::NormalConvention::DirectX => "direct_x",
        };
        Self {
            name: Cow::Borrowed(&core_metadata.name),
            dimensions: OutputDimensions {
                width: textures.dimensions.width,
                height: textures.dimensions.height,
            },
            albedo_rgb8: Cow::Owned(flatten_rgb8(textures.albedo.pixels())),
            height_gray16: Cow::Borrowed(textures.height.pixels()),
            normal_rgb8: Cow::Owned(flatten_rgb8(textures.normal.pixels())),
            occlusion_gray8: Cow::Borrowed(textures.occlusion.pixels()),
            metadata: TextureMetadata {
                generator_algorithm_version: core_metadata.algorithm_version.to_string(),
                recipe_hash: core_metadata.recipe_hash.clone(),
                parameter_hash: core_metadata.parameter_hash.clone(),
                seed: core_metadata.seed,
                physical_tile_width_m: core_metadata.physical_tile_size_m[0],
                physical_tile_height_m: core_metadata.physical_tile_size_m[1],
                height_min_m: core_metadata.minimum_height_m,
                height_max_m: core_metadata.maximum_height_m,
                neutral_height_m: core_metadata.base_height_m,
                displacement_map: core_metadata.displacement,
                normal_convention: normal_convention.into(),
                overrides: None,
            },
        }
    }

    fn validate(&self) -> Result<(), OutputError> {
        let dimensions = self.dimensions;
        let pixel_count = dimensions.pixel_count()?;
        let expected_rgb = dimensions.byte_count(3)?;
        if self.albedo_rgb8.len() != expected_rgb {
            return Err(OutputError::InvalidInput(format!(
                "albedo buffer has {} bytes; expected {expected_rgb}",
                self.albedo_rgb8.len()
            )));
        }
        if self.normal_rgb8.len() != expected_rgb {
            return Err(OutputError::InvalidInput(format!(
                "normal buffer has {} bytes; expected {expected_rgb}",
                self.normal_rgb8.len()
            )));
        }
        if self.height_gray16.len() != pixel_count {
            return Err(OutputError::InvalidInput(format!(
                "height buffer has {} pixels; expected {pixel_count}",
                self.height_gray16.len()
            )));
        }
        if self.occlusion_gray8.len() != pixel_count {
            return Err(OutputError::InvalidInput(format!(
                "occlusion buffer has {} pixels; expected {pixel_count}",
                self.occlusion_gray8.len()
            )));
        }
        validate_name(&self.name)
    }
}

fn flatten_rgb8(pixels: &[[u8; 3]]) -> Vec<u8> {
    pixels
        .iter()
        .flat_map(|pixel| pixel.iter().copied())
        .collect()
}

/// One encoded map's manifest entry.
#[derive(Debug, Clone, Serialize)]
pub struct ManifestMap {
    pub file: String,
    pub format: String,
    pub color_space: String,
    pub width: u32,
    pub height: u32,
    pub sha256: String,
}

/// The completed texture-set manifest.  The manifest is written last; its
/// presence therefore means all listed files were successfully renamed into
/// place.
#[derive(Debug, Clone, Serialize)]
pub struct OutputManifest {
    pub output_format_version: &'static str,
    pub profile: &'static str,
    pub name: String,
    pub dimensions: OutputDimensions,
    pub metadata: TextureMetadata,
    pub maps: Vec<ManifestMap>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub packed_channels: Option<BTreeSet<String>>,
}

/// A low-level image representation useful to adapters that already have
/// their own map naming.  [`write_png_bytes`] is intentionally independent of
/// the core generator types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PixelFormat {
    Rgb8,
    Gray16,
    Gray8,
    Rgba8,
}

impl PixelFormat {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Rgb8 => "RGB8",
            Self::Gray16 => "Gray16",
            Self::Gray8 => "Gray8",
            Self::Rgba8 => "RGBA8",
        }
    }
}

/// Errors from image validation, encoding, or output transactions.
#[derive(Debug)]
pub enum OutputError {
    InvalidInput(String),
    Io(io::Error),
    Png(png::EncodingError),
    Json(serde_json::Error),
}

impl fmt::Display for OutputError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidInput(message) => formatter.write_str(message),
            Self::Io(error) => write!(formatter, "I/O error: {error}"),
            Self::Png(error) => write!(formatter, "PNG encoding error: {error}"),
            Self::Json(error) => write!(formatter, "manifest JSON error: {error}"),
        }
    }
}

impl std::error::Error for OutputError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::Png(error) => Some(error),
            Self::Json(error) => Some(error),
            Self::InvalidInput(_) => None,
        }
    }
}

impl From<io::Error> for OutputError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<png::EncodingError> for OutputError {
    fn from(error: png::EncodingError) -> Self {
        Self::Png(error)
    }
}

impl From<serde_json::Error> for OutputError {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error)
    }
}

/// Encode a tightly packed image to PNG bytes.
///
/// # Errors
///
/// Returns an error when the buffer size does not match the dimensions or the
/// PNG encoder rejects the supplied image.
pub fn encode_png_bytes(
    dimensions: OutputDimensions,
    format: PixelFormat,
    pixels: &[u8],
) -> Result<Vec<u8>, OutputError> {
    let (color, depth, channels) = match format {
        PixelFormat::Rgb8 => (png::ColorType::Rgb, png::BitDepth::Eight, 3),
        PixelFormat::Gray16 => (png::ColorType::Grayscale, png::BitDepth::Sixteen, 2),
        PixelFormat::Gray8 => (png::ColorType::Grayscale, png::BitDepth::Eight, 1),
        PixelFormat::Rgba8 => (png::ColorType::Rgba, png::BitDepth::Eight, 4),
    };
    let expected = dimensions.byte_count(channels)?;
    if pixels.len() != expected {
        return Err(OutputError::InvalidInput(format!(
            "{} image has {} bytes; expected {expected}",
            format.as_str(),
            pixels.len()
        )));
    }

    let mut encoded = Vec::new();
    {
        let mut png_encoder = png::Encoder::new(&mut encoded, dimensions.width, dimensions.height);
        png_encoder.set_color(color);
        png_encoder.set_depth(depth);
        let mut writer = png_encoder.write_header()?;
        writer.write_image_data(pixels)?;
    }
    Ok(encoded)
}

/// Encode a host-endian grayscale buffer as a standards-compliant Gray16 PNG.
///
/// # Errors
///
/// Returns an error when the pixel count does not match the dimensions or the
/// PNG encoder rejects the image.
pub fn encode_gray16_png_bytes(
    dimensions: OutputDimensions,
    pixels: &[u16],
) -> Result<Vec<u8>, OutputError> {
    let expected = dimensions.pixel_count()?;
    if pixels.len() != expected {
        return Err(OutputError::InvalidInput(format!(
            "Gray16 image has {} pixels; expected {expected}",
            pixels.len()
        )));
    }
    let mut network_order = Vec::with_capacity(dimensions.byte_count(2)?);
    for pixel in pixels {
        network_order.extend_from_slice(&pixel.to_be_bytes());
    }
    encode_png_bytes(dimensions, PixelFormat::Gray16, &network_order)
}

/// Write a single encoded PNG through a unique sibling temporary file and a
/// rename.  The caller must have checked whether replacing `path` is allowed.
///
/// # Errors
///
/// Returns an error when the temporary file cannot be created, written,
/// synchronized, or renamed into place.
pub fn write_png_bytes(path: &Path, bytes: &[u8]) -> Result<(), OutputError> {
    let (temporary, mut file) = temporary_sibling(path)?;
    let write_result = (|| {
        file.write_all(bytes)?;
        file.sync_all()?;
        fs::rename(&temporary, path)?;
        Ok::<(), io::Error>(())
    })();
    if write_result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    write_result.map_err(OutputError::from)
}

/// Write all maps and the final manifest for one generated set.
///
/// # Errors
///
/// Returns an error for invalid image buffers, unsafe destination contents,
/// encoding failures, or filesystem failures. The manifest is written last.
pub fn write_texture_set(
    images: &TextureSetImages<'_>,
    destination: &Path,
    options: &OutputOptions,
) -> Result<OutputManifest, OutputError> {
    images.validate()?;
    let filenames = output_filenames(&images.name, options.profile);
    prepare_destination(
        destination,
        filenames.iter().map(String::as_str),
        options.force,
    )?;

    let dimensions = images.dimensions;
    let albedo = encode_png_bytes(dimensions, PixelFormat::Rgb8, &images.albedo_rgb8)?;
    let height = encode_gray16_png_bytes(dimensions, &images.height_gray16)?;
    let normal = encode_png_bytes(dimensions, PixelFormat::Rgb8, &images.normal_rgb8)?;
    let occlusion = encode_png_bytes(dimensions, PixelFormat::Gray8, &images.occlusion_gray8)?;
    let mut maps = Vec::with_capacity(filenames.len());
    let mut output_maps = vec![
        (&filenames[0], PixelFormat::Rgb8, "sRGB", albedo),
        (&filenames[1], PixelFormat::Gray16, "linear", height),
        (&filenames[2], PixelFormat::Rgb8, "linear", normal),
        (&filenames[3], PixelFormat::Gray8, "linear", occlusion),
    ];
    for (filename, format, color_space, bytes) in &output_maps {
        maps.push(ManifestMap {
            file: (*filename).clone(),
            format: format.as_str().into(),
            color_space: (*color_space).into(),
            width: dimensions.width,
            height: dimensions.height,
            sha256: sha256_hex(bytes),
        });
    }

    let packed_channels = if options.profile == OutputProfile::MotuUnityTerrain {
        let packed = pack_unity_mask(&images.height_gray16, &images.occlusion_gray8);
        let bytes = encode_png_bytes(dimensions, PixelFormat::Rgba8, &packed)?;
        let filename = &filenames[4];
        output_maps.push((filename, PixelFormat::Rgba8, "linear", bytes));
        maps.push(ManifestMap {
            file: filename.clone(),
            format: PixelFormat::Rgba8.as_str().into(),
            color_space: "linear".into(),
            width: dimensions.width,
            height: dimensions.height,
            sha256: sha256_hex(&output_maps[4].3),
        });
        Some(BTreeSet::from([
            "R=height_8bit".into(),
            "G=occlusion".into(),
            "B=unused_zero".into(),
            "A=opaque_one".into(),
        ]))
    } else {
        None
    };

    let manifest = OutputManifest {
        output_format_version: OUTPUT_FORMAT_VERSION,
        profile: options.profile.as_str(),
        name: images.name.to_string(),
        dimensions,
        metadata: images.metadata.clone(),
        maps,
        packed_channels,
    };
    let manifest_name = filenames.last().ok_or_else(|| {
        OutputError::InvalidInput("texture output did not produce a manifest filename".into())
    })?;
    let mut manifest_bytes = serde_json::to_vec_pretty(&manifest)?;
    manifest_bytes.push(b'\n');
    let staging = temporary_directory(destination_parent(destination), "texture-set-staging")?;
    let write_result = (|| {
        for (filename, _, _, bytes) in &output_maps {
            write_png_bytes(&staging.join(filename), bytes)?;
        }
        write_png_bytes(&staging.join(manifest_name), &manifest_bytes)?;
        preserve_expected_sidecars(destination, &staging, &filenames)?;
        commit_staged_set(destination, &staging, &filenames)
    })();
    if let Err(error) = write_result {
        let _ = fs::remove_dir_all(&staging);
        return Err(error);
    }
    Ok(manifest)
}

/// Pack the Unity terrain mask contract.  Height is intentionally reduced
/// from Gray16 only at this downstream boundary; the full Gray16 map remains
/// available to other consumers.
#[must_use]
pub fn pack_unity_mask(height: &[u16], occlusion: &[u8]) -> Vec<u8> {
    debug_assert_eq!(height.len(), occlusion.len());
    let mut packed = Vec::with_capacity(height.len().saturating_mul(4));
    height.iter().zip(occlusion).for_each(|(&height, &ao)| {
        packed.extend_from_slice(&[(height >> 8) as u8, ao, 0, u8::MAX]);
    });
    packed
}

/// Compute a lowercase SHA-256 digest for an output file.
#[must_use]
pub fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest
        .iter()
        .fold(String::with_capacity(64), |mut output, byte| {
            use std::fmt::Write as _;
            write!(&mut output, "{byte:02x}").expect("writing to String cannot fail");
            output
        })
}

/// Serialize a recipe using its serde representation and hash those bytes.
/// Struct serialization supplies a stable field order while allowing the
/// recipe module to own normalization/default handling.
///
/// # Errors
///
/// Returns an error when the recipe cannot be serialized to normalized JSON.
pub fn normalized_recipe_hash<T: Serialize>(recipe: &T) -> Result<String, OutputError> {
    Ok(sha256_hex(&serde_json::to_vec(recipe)?))
}

fn validate_name(name: &str) -> Result<(), OutputError> {
    if name.is_empty()
        || name == "."
        || name == ".."
        || name.contains('/')
        || name.contains('\\')
        || name.bytes().any(|byte| byte.is_ascii_control())
    {
        return Err(OutputError::InvalidInput(format!(
            "texture set name {name:?} is not a safe filename stem"
        )));
    }
    Ok(())
}

fn output_filenames(name: &str, profile: OutputProfile) -> Vec<String> {
    let mut filenames = vec![
        format!("{name}_albedo.png"),
        format!("{name}_height.png"),
        format!("{name}_normal.png"),
        format!("{name}_occlusion.png"),
    ];
    if profile == OutputProfile::MotuUnityTerrain {
        filenames.push(format!("{name}_mask.png"));
    }
    filenames.push(format!("{name}.texture-set.json"));
    filenames
}

fn prepare_destination<'a>(
    destination: &Path,
    expected_names: impl Iterator<Item = &'a str>,
    force: bool,
) -> Result<(), OutputError> {
    let expected: BTreeSet<&str> = expected_names.collect();
    match fs::symlink_metadata(destination) {
        Ok(metadata) if metadata.file_type().is_dir() => {}
        Ok(_) => {
            return Err(OutputError::Io(io::Error::new(
                io::ErrorKind::AlreadyExists,
                format!("output path {} is not a directory", destination.display()),
            )));
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            fs::create_dir_all(destination_parent(destination))?;
        }
        Err(error) => return Err(error.into()),
    }

    let mut existing = BTreeSet::new();
    if destination.is_dir() {
        for entry in fs::read_dir(destination)? {
            let entry = entry?;
            let file_type = entry.file_type()?;
            if !file_type.is_file() {
                return Err(OutputError::InvalidInput(format!(
                    "output directory {} contains a non-file entry {}; refusing to write",
                    destination.display(),
                    entry.file_name().to_string_lossy()
                )));
            }
            let name = entry.file_name();
            let name = name.to_string_lossy();
            let is_expected_output = expected.contains(name.as_ref());
            let is_expected_sidecar = name
                .strip_suffix(".meta")
                .is_some_and(|output_name| expected.contains(output_name));
            if !is_expected_output && !is_expected_sidecar {
                return Err(OutputError::InvalidInput(format!(
                    "output directory {} contains unrelated file {}; refusing to write",
                    destination.display(),
                    name
                )));
            }
            if is_expected_output {
                existing.insert(name.into_owned());
            }
        }
    }
    if !existing.is_empty() && !force {
        return Err(OutputError::InvalidInput(format!(
            "output directory {} already contains generated files; pass --force to replace them",
            destination.display()
        )));
    }
    Ok(())
}

/// Carry engine-owned metadata for this generated set through the atomic
/// directory swap. Only sidecars whose basename is one of the exact expected
/// outputs pass `prepare_destination`; unrelated files remain a hard error.
fn preserve_expected_sidecars(
    destination: &Path,
    staging: &Path,
    expected_names: &[String],
) -> Result<(), OutputError> {
    if !destination.is_dir() {
        return Ok(());
    }
    for filename in expected_names {
        let sidecar_name = format!("{filename}.meta");
        let source = destination.join(&sidecar_name);
        match fs::symlink_metadata(&source) {
            Ok(metadata) if metadata.file_type().is_file() => {
                fs::copy(&source, staging.join(sidecar_name))?;
            }
            Ok(_) => {
                return Err(OutputError::InvalidInput(format!(
                    "generated sidecar {} is not a regular file",
                    source.display()
                )));
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}

fn destination_parent(destination: &Path) -> &Path {
    destination
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."))
}

/// Commit a completely staged set without ever renaming over an existing
/// path.  Moving the old directory aside first makes replacement work on
/// platforms where `rename` refuses to overwrite an existing directory, and
/// retaining the backup until the new directory is in place gives us a
/// rollback point if the second rename fails.
fn commit_staged_set(
    destination: &Path,
    staging: &Path,
    expected_names: &[String],
) -> Result<(), OutputError> {
    for filename in expected_names {
        let path = staging.join(filename);
        let metadata = fs::symlink_metadata(&path).map_err(|error| {
            io::Error::new(
                error.kind(),
                format!("staged output {} is missing: {error}", path.display()),
            )
        })?;
        if !metadata.file_type().is_file() {
            return Err(OutputError::InvalidInput(format!(
                "staged output {} is not a regular file",
                path.display()
            )));
        }
    }

    if !destination.exists() {
        fs::rename(staging, destination)?;
        return Ok(());
    }

    let backup = temporary_path(destination_parent(destination), "texture-set-backup")?;
    fs::rename(destination, &backup)?;

    match fs::rename(staging, destination) {
        Ok(()) => {
            // The old directory is no longer needed.  This is intentionally
            // done only after the replacement directory has been installed.
            fs::remove_dir_all(&backup)?;
            Ok(())
        }
        Err(error) => {
            let rollback = fs::rename(&backup, destination);
            match rollback {
                Ok(()) => Err(error.into()),
                Err(rollback_error) => Err(OutputError::Io(io::Error::other(format!(
                    "could not install staged output: {error}; rollback also failed: {rollback_error}"
                )))),
            }
        }
    }
}

fn temporary_sibling(path: &Path) -> Result<(PathBuf, File), io::Error> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let stem = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("texture-output");
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    for attempt in 0..32_u32 {
        let candidate = parent.join(format!(
            ".{stem}.tmp-{}-{timestamp}-{attempt}",
            process::id()
        ));
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&candidate)
        {
            Ok(file) => {
                return Ok((candidate, file));
            }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error),
        }
    }
    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        format!(
            "could not allocate a temporary sibling for {}",
            path.display()
        ),
    ))
}

fn temporary_directory(parent: &Path, stem: &str) -> Result<PathBuf, io::Error> {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    for attempt in 0..32_u32 {
        let candidate = parent.join(format!(".{stem}-{}-{timestamp}-{attempt}", process::id()));
        match fs::create_dir(&candidate) {
            Ok(()) => return Ok(candidate),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error),
        }
    }
    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        format!(
            "could not allocate a temporary directory in {}",
            parent.display()
        ),
    ))
}

fn temporary_path(parent: &Path, stem: &str) -> Result<PathBuf, io::Error> {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    for attempt in 0..32_u32 {
        let candidate = parent.join(format!(".{stem}-{}-{timestamp}-{attempt}", process::id()));
        if !candidate.exists() {
            return Ok(candidate);
        }
    }
    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        format!(
            "could not allocate a temporary path in {}",
            parent.display()
        ),
    ))
}

#[cfg(test)]
mod tests {
    use std::{fs, path::PathBuf};

    use super::*;

    #[test]
    fn png_formats_have_expected_headers() {
        let dimensions = OutputDimensions {
            width: 1,
            height: 1,
        };
        let rgb = encode_png_bytes(dimensions, PixelFormat::Rgb8, &[1, 2, 3]).unwrap();
        let gray8 = encode_png_bytes(dimensions, PixelFormat::Gray8, &[4]).unwrap();
        let rgba = encode_png_bytes(dimensions, PixelFormat::Rgba8, &[5, 6, 7, 8]).unwrap();
        let gray16 = encode_gray16_png_bytes(dimensions, &[0x1234]).unwrap();
        for png in [rgb, gray8, rgba, gray16] {
            assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
        }
    }

    #[test]
    fn gray16_is_network_order() {
        let bytes = encode_gray16_png_bytes(
            OutputDimensions {
                width: 1,
                height: 1,
            },
            &[0x1234],
        )
        .unwrap();
        let decoder = png::Decoder::new(bytes.as_slice());
        let mut reader = decoder.read_info().unwrap();
        let mut output = vec![0; reader.output_buffer_size()];
        let info = reader.next_frame(&mut output).unwrap();
        assert_eq!(info.bit_depth, png::BitDepth::Sixteen);
        assert_eq!(&output[..2], &[0x12, 0x34]);
    }

    #[test]
    fn packed_mask_matches_unity_contract() {
        assert_eq!(
            pack_unity_mask(&[0xabcd, 0x0123], &[7, 255]),
            vec![0xab, 7, 0, 255, 1, 255, 0, 255]
        );
    }

    #[test]
    fn output_is_atomic_and_manifest_is_last() {
        let root = unique_test_dir("texture-output");
        let images = TextureSetImages::new(
            "stone",
            OutputDimensions {
                width: 1,
                height: 1,
            },
            &[1, 2, 3],
            &[0x1234],
            &[128, 128, 255],
            &[255],
            TextureMetadata::default(),
        );
        let manifest = write_texture_set(&images, &root, &OutputOptions::default()).unwrap();
        assert_eq!(manifest.maps.len(), 4);
        assert!(root.join("stone.texture-set.json").is_file());
        assert!(fs::read_dir(&root).unwrap().all(|entry| {
            !entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .contains(".tmp-")
        }));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn force_is_required_for_existing_generated_files() {
        let root = unique_test_dir("texture-force");
        fs::write(root.join("stone_albedo.png"), b"old").unwrap();
        fs::write(root.join("stone_albedo.png.meta"), b"stable-guid").unwrap();
        let images = TextureSetImages::new(
            "stone",
            OutputDimensions {
                width: 1,
                height: 1,
            },
            &[1, 2, 3],
            &[0x1234],
            &[128, 128, 255],
            &[255],
            TextureMetadata::default(),
        );
        let error = write_texture_set(&images, &root, &OutputOptions::default()).unwrap_err();
        assert!(error.to_string().contains("--force"));
        let options = OutputOptions {
            force: true,
            ..OutputOptions::default()
        };
        write_texture_set(&images, &root, &options).unwrap();
        assert_eq!(
            fs::read(root.join("stone_albedo.png.meta")).unwrap(),
            b"stable-guid"
        );
        let first_albedo = fs::read(root.join("stone_albedo.png")).unwrap();
        assert!(fs::read_dir(&root).unwrap().all(|entry| {
            let name = entry.unwrap().file_name();
            !name.to_string_lossy().contains("texture-set-")
        }));

        let changed_images = TextureSetImages::new(
            "stone",
            OutputDimensions {
                width: 1,
                height: 1,
            },
            &[4, 5, 6],
            &[0x1234],
            &[128, 128, 255],
            &[255],
            TextureMetadata::default(),
        );
        write_texture_set(&changed_images, &root, &options).unwrap();
        assert_ne!(
            first_albedo,
            fs::read(root.join("stone_albedo.png")).unwrap()
        );
        assert!(fs::read_dir(&root).unwrap().all(|entry| {
            let name = entry.unwrap().file_name();
            !name.to_string_lossy().contains("texture-set-")
        }));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn force_still_rejects_an_unrelated_sidecar() {
        let root = unique_test_dir("texture-unrelated-sidecar");
        fs::write(
            root.join("notes.txt.meta"),
            b"not generated output metadata",
        )
        .unwrap();
        let images = TextureSetImages::new(
            "stone",
            OutputDimensions {
                width: 1,
                height: 1,
            },
            &[1, 2, 3],
            &[0x1234],
            &[128, 128, 255],
            &[255],
            TextureMetadata::default(),
        );
        let error = write_texture_set(
            &images,
            &root,
            &OutputOptions {
                force: true,
                ..OutputOptions::default()
            },
        )
        .unwrap_err();
        assert!(error.to_string().contains("notes.txt.meta"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn staged_set_preflight_preserves_destination_when_a_file_is_missing() {
        let root = unique_test_dir("texture-transaction");
        let destination = root.join("destination");
        let staging = root.join("staging");
        fs::create_dir_all(&destination).unwrap();
        fs::create_dir_all(&staging).unwrap();
        fs::write(destination.join("stone_albedo.png"), b"old").unwrap();
        fs::write(staging.join("stone_albedo.png"), b"new").unwrap();
        let expected = vec![
            "stone_albedo.png".to_string(),
            "stone.texture-set.json".to_string(),
        ];

        let error = commit_staged_set(&destination, &staging, &expected).unwrap_err();
        assert!(error.to_string().contains("stone.texture-set.json"));
        assert_eq!(
            fs::read(destination.join("stone_albedo.png")).unwrap(),
            b"old"
        );
        assert!(staging.is_dir());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn output_profile_parser_accepts_only_documented_names() {
        assert_eq!(
            "separate".parse::<OutputProfile>().unwrap(),
            OutputProfile::Separate
        );
        assert_eq!(
            "motu_unity_terrain".parse::<OutputProfile>().unwrap(),
            OutputProfile::MotuUnityTerrain
        );
        for alias in ["motu-unity-terrain", "unity"] {
            assert!(alias.parse::<OutputProfile>().is_err());
        }
    }

    fn unique_test_dir(stem: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("island-rs-{stem}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        root
    }
}
