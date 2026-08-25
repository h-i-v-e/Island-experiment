//! The cloud layer and the shadow it lays on the ground, which are one field
//! read two ways.
//!
//! The field is a tiling value-noise sum built here, once per look, into a
//! single-channel image. That image is handed to the sun as a
//! [`DirectionalLightTexture`](bevy::light::DirectionalLightTexture), which
//! Bevy projects along the light and multiplies into the direct term of every
//! lit fragment in the scene — terrain, rock, foliage, both waters and the
//! spray, without a line of shader in any of them. The same image is sampled
//! again by the layer in the sky, through the same projection, so a cloud and
//! the shadow under it are the same feature of the same field and cannot drift
//! apart.
//!
//! That projection is the whole trick. A directional light texture is mapped by
//! the inverse of the light's own transform, so its local XY plane is
//! perpendicular to the sunlight and every point on one sun ray lands on the
//! same texel. Registering the sky layer with the ground shadow therefore needs
//! no altitude arithmetic at all: the layer reads the field at its own world
//! position through the same matrix, and the ground below it along the ray
//! reads the identical texel. Moving the sun's translation sideways moves both
//! at once, which is how the layer drifts — `weather::drift_clouds` writes it
//! from the same clock `--screenshot` freezes.
//!
//! What the image cannot carry is detail: the shadow of a cloud a kilometre and
//! a half up has lost its fine edges to the sun's own angular size long before
//! it reaches the ground. So the sky layer adds a finer noise of its own in the
//! fragment stage, where the ground shadow stays the smooth field. The two
//! disagreeing at that scale is the physical answer, not a compromise.
//!
//! Everything here is deterministic: the field is hashed the same way the rest
//! of the crate hashes, and there are no asset files.

use bevy::{
    asset::{RenderAssetUsages, embedded_asset},
    ecs::system::SystemParam,
    image::{ImageAddressMode, ImageFilterMode, ImageSampler, ImageSamplerDescriptor},
    light::{DirectionalLightTexture, NotShadowCaster},
    pbr::{ExtendedMaterial, MaterialExtension, MaterialPlugin},
    prelude::*,
    render::render_resource::{
        AsBindGroup, Extent3d, ShaderType, TextureDimension, TextureFormat,
    },
    shader::ShaderRef,
};

use crate::{
    capture::DebugView,
    hash::{mix, unit},
    lighting::Sun,
    weather::{CloudLook, Weather},
};

/// The layer in the sky.
pub type CloudMaterial = ExtendedMaterial<StandardMaterial, CloudExtension>;

/// Texels along each edge of the field image. At the tile size below that is a
/// texel every twenty metres, which is finer than a cloud shadow's own edge and
/// costs a quarter of a megabyte.
const FIELD_RESOLUTION: u32 = 512;
/// Metres of ground the field repeats over. Ten kilometres against a
/// two-kilometre island means no view of the island can see the tile repeat,
/// and the layer that reaches out to the horizon repeats eight times across
/// itself — far enough out that the distance fade has taken most of it.
const TILE_METRES: f32 = 10_000.0;
/// Lattice cells across the tile in the first octave, and how many octaves
/// follow it. Eight cells over ten kilometres puts the largest cloud feature at
/// about 1.2 km and the smallest at 150 m, which is the range a trade cumulus
/// field actually occupies.
const FIELD_CELLS: u32 = 8;
const FIELD_OCTAVES: u32 = 4;
/// Bins the coverage threshold is found in. The field's own distribution is
/// nothing like uniform, so the threshold is read off the histogram of the
/// samples themselves rather than assumed: a look asking for a third of the sky
/// gets a third of the sky.
const COVERAGE_BINS: usize = 1_024;
/// Field units one unit of a look's softness stands for. The sum lands well
/// inside the middle of its range, so an edge drawn out by a quarter of that
/// range is already a very soft one.
const SOFTNESS_SCALE: f32 = 0.22;

/// Distinguishes the cloud field from the crate's other hashed values.
const FIELD_SALT: u64 = 0x0c1d_7a63_b482_e95f;
/// Separates one octave's lattice from another's. Without it two octaves would
/// hash the same cell coordinates to the same value and sum into one layer.
const OCTAVE_SALT: u64 = 0x3f19_5c88_d704_6ab3;

/// Metres across the layer's own mesh. The camera never leaves the island, so a
/// fixed plane this wide is always overhead and never has to follow — which is
/// what keeps the temporal resolve from reading a layer that tracks the camera
/// as a layer sliding across the sky.
const LAYER_EXTENT: f32 = 8.0e4;
/// Metres of horizontal distance the layer starts fading out at, and the run it
/// fades over. It reaches the end of the fade well inside its own mesh, so the
/// edge of the plane is never in a frame and what the eye finds towards the
/// horizon is cloud thinning into sky rather than a rim.
const LAYER_FADE_START: f32 = 8.0e3;
const LAYER_FADE_RANGE: f32 = 1.8e4;
/// Metres of the finer noise the fragment stage adds over the sampled field.
const DETAIL_METRES: f32 = 240.0;

pub struct CloudPlugin;

impl Plugin for CloudPlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "cloud.wgsl");
        app.add_plugins(MaterialPlugin::<CloudMaterial>::default())
            .add_systems(Update, (sync_layer, write_settings).chain());
    }
}

/// Tags the layer's own entity, so a look with no cloud can take it away again.
#[derive(Component)]
struct CloudLayer;

/// Builds the field for the current look and puts it on both readers: the sun,
/// as the light texture Bevy multiplies its direct term by, and the layer in
/// the sky. A look with no cloud takes the texture off the sun and despawns the
/// layer, leaving the scene exactly as it was before either existed.
///
/// Both readers are set here rather than one here and one in `weather`, because
/// they have to be handed the same image on the same frame or the layer and its
/// shadow would be one look apart.
fn sync_layer(
    mut commands: Commands,
    weather: Res<Weather>,
    mut applied: Local<Option<Weather>>,
    mut built: Built,
    layers: Query<Entity, With<CloudLayer>>,
    mut suns: Query<(Entity, &mut Transform), With<Sun>>,
) {
    if *applied == Some(*weather) {
        return;
    }
    let Ok((sun, mut sun_transform)) = suns.single_mut() else {
        return;
    };
    let look = weather.look();
    for layer in &layers {
        commands.entity(layer).despawn();
    }
    *applied = Some(*weather);

    if !look.has_clouds() {
        commands.entity(sun).remove::<DirectionalLightTexture>();
        return;
    }
    // The light texture's local space runs from -1 to 1 across one tile, and
    // the only thing that scales it is the light's own transform. A directional
    // light takes nothing else from its scale.
    sun_transform.scale = Vec3::splat(TILE_METRES * 0.5);
    let image = built.images.add(field_image(&look.clouds));
    commands.entity(sun).insert(DirectionalLightTexture {
        image: image.clone(),
        tiled: true,
    });
    let mesh = built
        .meshes
        .add(Plane3d::default().mesh().size(LAYER_EXTENT, LAYER_EXTENT));
    let material = built.materials.add(CloudMaterial {
        // The extension writes the whole fragment, so what the base material
        // still decides is only how the layer is drawn: blended, from either
        // side, after the sky.
        base: StandardMaterial {
            alpha_mode: AlphaMode::Blend,
            double_sided: true,
            cull_mode: None,
            ..default()
        },
        extension: CloudExtension::new(image, &look.clouds),
    });
    commands.spawn((
        Name::new("Cloud layer"),
        CloudLayer,
        Mesh3d(mesh),
        MeshMaterial3d(material),
        Transform::from_xyz(0.0, look.clouds.altitude, 0.0),
        // A layer that cast a shadow would print itself into the cascades on
        // top of the light texture that already carries it.
        NotShadowCaster,
    ));
}

/// The three asset stores the layer is built out of, collected as one parameter
/// so the system stays inside the argument count the rest of the crate keeps to.
#[derive(SystemParam)]
struct Built<'w> {
    images: ResMut<'w, Assets<Image>>,
    meshes: ResMut<'w, Assets<Mesh>>,
    materials: ResMut<'w, Assets<CloudMaterial>>,
}

/// Writes the projection the sun is currently at, and the diagnostic channel,
/// into the layer's material every frame.
///
/// The sun drifts, so the basis the layer reads the field through moves with
/// it, and the two are only registered while they agree. Written without asking
/// whether anything moved, for the same reason `capture` writes the water clock
/// that way: the layer is spawned frames after the sun exists and change
/// detection would leave it on whatever the first frame held.
fn write_settings(
    view: Res<DebugView>,
    weather: Res<Weather>,
    suns: Query<&GlobalTransform, With<Sun>>,
    mut materials: ResMut<Assets<CloudMaterial>>,
) {
    let Ok(sun) = suns.single() else {
        return;
    };
    let look = weather.look();
    let (right, up) = (sun.right(), sun.up());
    for (_, material) in materials.iter_mut() {
        let settings = &mut material.extension.settings;
        settings.light_right = *right;
        settings.light_up = *up;
        settings.light_origin = sun.translation();
        settings.sun_direction = look.sun_direction.normalize_or(Vec3::NEG_Y);
        settings.sun_illuminance = look.illuminance();
        settings.debug_view = view.flag();
    }
}

/// The field image for one look: the share of direct sunlight that reaches the
/// ground, which is what Bevy multiplies the sun by, and what the layer inverts
/// to recover the cloud that took it away.
///
/// Four channels for a value that only ever needs one, and the other three are
/// deliberately zero. A directional light texture is registered in the same
/// buffer clustered decals live in, and 0.19.1's GPU clustering takes the
/// length of that whole buffer as its decal count — so the sun's own light
/// texture is also rasterised into the clusters and composited as a base-colour
/// decal by every fragment that goes through the stock standard material. As a
/// single-channel image that arrives as opaque red and every tree on the island
/// turns with it. Composited alpha is what the decal path blends by and the
/// light path never reads, so an alpha of zero makes that pass a no-op and
/// leaves the red channel doing its own job untouched. The extensions in
/// `surface` and the layer below write their own fragment and never took the
/// decal in the first place.
fn field_image(look: &CloudLook) -> Image {
    let span = FIELD_RESOLUTION as usize;
    let mut samples = Vec::with_capacity(span * span);
    for row in 0..span {
        for column in 0..span {
            #[allow(clippy::cast_precision_loss)]
            let (u, v) = (column as f32 / span as f32, row as f32 / span as f32);
            samples.push(field(u, v));
        }
    }
    let threshold = coverage_threshold(&samples, look.coverage);
    let softness = (look.softness * SOFTNESS_SCALE).max(1.0e-3);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let data: Vec<u8> = samples
        .iter()
        .flat_map(|&sample| {
            let cover = smoothstep(threshold - softness, threshold + softness, sample);
            let sunlight = 1.0 - look.shadow * cover;
            [(sunlight.clamp(0.0, 1.0) * 255.0).round() as u8, 0, 0, 0]
        })
        .collect();

    let mut image = Image::new(
        Extent3d {
            width: FIELD_RESOLUTION,
            height: FIELD_RESOLUTION,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8Unorm,
        RenderAssetUsages::RENDER_WORLD,
    );
    // The field tiles, and both readers wrap into it, so the sampler has to
    // wrap as well or the seam would show as a one-texel band at every repeat.
    image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::Repeat,
        address_mode_v: ImageAddressMode::Repeat,
        mag_filter: ImageFilterMode::Linear,
        min_filter: ImageFilterMode::Linear,
        mipmap_filter: ImageFilterMode::Linear,
        ..default()
    });
    image
}

/// The field value the given share of the sky stands above.
///
/// Read off a histogram of the samples rather than assumed, because a sum of
/// four octaves of value noise is banked hard around the middle of its range: a
/// threshold of `1 - coverage` would leave a look asking for a third of the sky
/// with a tenth of it. Zero coverage answers above every sample, so nothing is
/// covered at all.
fn coverage_threshold(samples: &[f32], coverage: f32) -> f32 {
    if coverage <= 0.0 {
        return 2.0;
    }
    if coverage >= 1.0 {
        return -1.0;
    }
    let mut bins = [0_u32; COVERAGE_BINS];
    #[allow(clippy::cast_precision_loss)]
    let last = (COVERAGE_BINS - 1) as f32;
    for &sample in samples {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let bin = (sample.clamp(0.0, 1.0) * last) as usize;
        bins[bin] += 1;
    }
    #[allow(clippy::cast_precision_loss)]
    let wanted = f64::from(1.0 - coverage) * samples.len() as f64;
    let mut seen = 0.0_f64;
    for (bin, &count) in bins.iter().enumerate() {
        seen += f64::from(count);
        if seen >= wanted {
            #[allow(clippy::cast_precision_loss)]
            return bin as f32 / (COVERAGE_BINS - 1) as f32;
        }
    }
    1.0
}

/// The tiling field itself, over the unit tile.
fn field(u: f32, v: f32) -> f32 {
    let mut total = 0.0;
    let mut normalization = 0.0;
    let mut amplitude = 1.0;
    let mut period = FIELD_CELLS;
    for octave in 0..FIELD_OCTAVES {
        total += amplitude * value_noise(u, v, period, mix(u64::from(octave), OCTAVE_SALT));
        normalization += amplitude;
        amplitude *= 0.5;
        period *= 2;
    }
    total / normalization
}

/// Value noise on a lattice that wraps at `period` cells, quintic-interpolated
/// so the tile has no lattice creases in it and no seam at its own edge.
fn value_noise(u: f32, v: f32, period: u32, salt: u64) -> f32 {
    #[allow(clippy::cast_precision_loss)]
    let cells = period as f32;
    let (x, y) = (u * cells, v * cells);
    let (column, row) = (x.floor(), y.floor());
    let (across, down) = (quintic(x - column), quintic(y - row));
    #[allow(clippy::cast_possible_truncation)]
    let (column, row) = (column as i64, row as i64);
    let corner = |column: i64, row: i64| lattice(column, row, period, salt);
    let near = corner(column, row).lerp(corner(column + 1, row), across);
    let far = corner(column, row + 1).lerp(corner(column + 1, row + 1), across);
    near.lerp(far, down)
}

/// One lattice corner, wrapped into the tile.
fn lattice(column: i64, row: i64, period: u32, salt: u64) -> f32 {
    let period = i64::from(period);
    let column = column.rem_euclid(period).cast_unsigned();
    let row = row.rem_euclid(period).cast_unsigned();
    let cell = column.wrapping_mul(0x9e37_79b9_7f4a_7c15) ^ row.wrapping_mul(0xc2b2_ae3d_27d4_eb4f);
    unit(mix(cell, FIELD_SALT ^ salt))
}

fn quintic(t: f32) -> f32 {
    t * t * t * (t * (t * 6.0 - 15.0) + 10.0)
}

/// The same curve WGSL's `smoothstep` is, so the field baked here and the shape
/// the layer derives from it answer alike.
fn smoothstep(low: f32, high: f32, value: f32) -> f32 {
    let span = high - low;
    if span.abs() <= f32::EPSILON {
        return f32::from(value >= high);
    }
    let progress = ((value - low) / span).clamp(0.0, 1.0);
    progress * progress * (3.0 - 2.0 * progress)
}

/// Uniform block shared with `cloud.wgsl`. The three light vectors together are
/// the same `local_from_world` Bevy projects the light texture by, written out
/// as a basis so the layer can walk it without a matrix of its own.
#[derive(Clone, Copy, Debug, Default, Reflect, ShaderType)]
pub struct CloudSettings {
    pub light_right: Vec3,
    /// The reciprocal of the light texture's own half tile, which is what one
    /// unit of its local space is worth in metres.
    pub tile_scale: f32,
    pub light_up: Vec3,
    /// The share of direct sun the image already has taken out of a fully
    /// covered patch, which is what the layer divides back out to recover the
    /// cloud that took it.
    pub shadow: f32,
    pub light_origin: Vec3,
    /// How dark the underside of a fully thick cloud goes.
    pub thickness: f32,
    /// The direction the sunlight travels.
    pub sun_direction: Vec3,
    pub sun_illuminance: f32,
    pub fade_start: f32,
    pub fade_range: f32,
    pub detail_metres: f32,
    /// The diagnostic channel in force, or zero. The layer carries no channel
    /// of its own and answers every one of them by leaving the frame.
    pub debug_view: u32,
}

/// Cloud extension bindings: the field, and the projection to read it through.
#[derive(Asset, AsBindGroup, Clone, Debug, Reflect)]
pub struct CloudExtension {
    #[uniform(100)]
    pub settings: CloudSettings,
    #[texture(101)]
    #[sampler(102)]
    pub field: Handle<Image>,
}

impl CloudExtension {
    #[must_use]
    pub fn new(field: Handle<Image>, look: &CloudLook) -> Self {
        Self {
            settings: CloudSettings {
                tile_scale: 2.0 / TILE_METRES,
                shadow: look.shadow.max(1.0e-3),
                thickness: look.thickness,
                fade_start: LAYER_FADE_START,
                fade_range: LAYER_FADE_RANGE,
                detail_metres: DETAIL_METRES,
                ..CloudSettings::default()
            },
            field,
        }
    }
}

impl MaterialExtension for CloudExtension {
    fn fragment_shader() -> ShaderRef {
        "embedded://island_bevy/cloud.wgsl".into()
    }
}

#[cfg(test)]
mod tests {
    use super::{FIELD_RESOLUTION, coverage_threshold, field, value_noise};

    fn samples() -> Vec<f32> {
        let span = FIELD_RESOLUTION as usize;
        let mut samples = Vec::with_capacity(span * span);
        for row in 0..span {
            for column in 0..span {
                #[allow(clippy::cast_precision_loss)]
                let (u, v) = (column as f32 / span as f32, row as f32 / span as f32);
                samples.push(field(u, v));
            }
        }
        samples
    }

    /// The field is read by two things that must agree, and one of them wraps
    /// into it eight times across a frame, so the tile has to join itself
    /// exactly at both edges.
    #[test]
    fn the_field_tiles_without_a_seam() {
        for step in 0..64_u8 {
            let along = f32::from(step) / 64.0;
            assert!(
                (field(0.0, along) - field(1.0, along)).abs() < 1.0e-5,
                "the vertical seam at {along}"
            );
            assert!(
                (field(along, 0.0) - field(along, 1.0)).abs() < 1.0e-5,
                "the horizontal seam at {along}"
            );
        }
        // And one lattice on its own, at the period the first octave uses.
        assert!((value_noise(0.0, 0.3, 8, 7) - value_noise(1.0, 0.3, 8, 7)).abs() < 1.0e-6);
    }

    /// A look asking for a third of the sky has to get a third of the sky. The
    /// sum of four octaves is banked hard around the middle of its range, so a
    /// threshold taken as `1 - coverage` would deliver a fraction of it; this
    /// is the one thing the histogram is for.
    #[test]
    fn coverage_is_the_share_of_sky_it_says_it_is() {
        let samples = samples();
        for wanted in [0.16_f32, 0.36, 0.5, 0.93] {
            let threshold = coverage_threshold(&samples, wanted);
            #[allow(clippy::cast_precision_loss)]
            let covered = samples.iter().filter(|&&value| value >= threshold).count() as f32
                / samples.len() as f32;
            assert!(
                (covered - wanted).abs() < 0.02,
                "asked for {wanted} of the sky and covered {covered}"
            );
        }
        // No cloud at all has to answer above every sample there is.
        let clear = coverage_threshold(&samples, 0.0);
        assert!(samples.iter().all(|&value| value < clear));
    }

    /// The field has to use its range: one banked so tightly that a soft edge
    /// spans the whole of it would give every look the same sky.
    #[test]
    fn the_field_spreads_across_its_range() {
        let samples = samples();
        let low = samples.iter().copied().fold(f32::MAX, f32::min);
        let high = samples.iter().copied().fold(f32::MIN, f32::max);
        assert!(low < 0.35, "the field bottoms out at {low}");
        assert!(high > 0.65, "the field tops out at {high}");
    }
}
