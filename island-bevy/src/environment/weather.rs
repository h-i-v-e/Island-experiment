//! Named weather looks: one coherent set of sun, air, cloud, mist and grading
//! values each, chosen by `--weather` and recorded beside every capture.
//!
//! A look is a place rather than a slider. Each one names where the sun stands,
//! how much haze the air carries, what cloud is over the island and how hard it
//! shades the ground, where mist collects, and the restrained grade the frame
//! is finished with. Nothing here is animated except the cloud drift, and that
//! runs on the same clock the water does, so a capture of a look answers the
//! same way twice.
//!
//! [`Weather::Clear`] is the renderer as it stood before this module: no cloud
//! layer, no fog volumes, an unmodified earth medium and a neutral grade. It is
//! the default, so every capture taken before weather existed is still the
//! baseline the rest are read against.
//!
//! The table is the only authority. `lighting` asks it where the sun goes and
//! what the air is made of, `clouds` and `mist` ask it what to build, `camera`
//! carries the grade it hands over, and `screenshot` writes its name into the
//! sidecar.

use bevy::{
    light::{
        Atmosphere, DirectionalLight, VolumetricFog, VolumetricLight, atmosphere::ScatteringMedium,
        light_consts::lux,
    },
    math::DVec3,
    pbr::AtmosphereSettings,
    prelude::*,
    render::view::{ColorGrading, ColorGradingGlobal, ColorGradingSection},
};

use crate::{
    camera::FlyCamera,
    capture::WaterClock,
    clouds::DRIFT_WRAP_METRES,
    lighting::{MEDIUM_RESOLUTION, Sun},
};

/// Sampling resolution of the earth medium's Mie term, which is the one term a
/// look scales. Rayleigh is the sky's blue and ozone its evening band; what
/// separates a maritime haze, a valley morning and an overcast day from a clear
/// one is how much aerosol is in the air, and that is Mie alone.
const MIE_TERM: usize = 1;

/// One coherent look. Not `Copy`: [`ColorGrading`] carries the midtone range as
/// a `Range<f32>`, and the table is read by reference anyway.
pub struct Look {
    /// The name `--weather` spells this look with.
    pub name: &'static str,
    /// Where the sun stands, as the direction its light travels.
    pub sun_direction: Vec3,
    /// The share of raw sunlight that reaches the top of the atmosphere. The
    /// atmosphere itself applies the tint and the loss along the path, so this
    /// is only ever 1 unless a look means something else to be in the way.
    pub sun_scale: f32,
    /// What the earth medium's Mie term is multiplied by: 1 is the clear
    /// standard atmosphere, higher is more aerosol and so more haze, whiter sky
    /// and stronger aerial perspective over the same distance.
    pub mie_scale: f32,
    /// How far out the aerial-perspective lookup is distributed. Bevy's default
    /// spreads 32 slices over 32 km, which gives a two-kilometre island two of
    /// them; a shorter run resolves the gradient across the island instead.
    pub aerial_distance: f32,
    pub clouds: CloudLook,
    pub mist: MistLook,
    pub grading: ColorGrading,
}

/// The cloud layer and the shadow it lays on the ground, which are one field
/// read two ways.
pub struct CloudLook {
    /// The share of sky the layer covers. Zero is no layer at all: no mesh, no
    /// light texture, nothing sampled.
    pub coverage: f32,
    /// How far a cloud edge is drawn out, as a share of the field's range.
    pub softness: f32,
    /// Metres above the sea the layer sits at.
    pub altitude: f32,
    /// The share of direct sun a fully covered patch of ground loses. Skylight
    /// is untouched, which is what makes a shadowed slope soft rather than
    /// black.
    pub shadow: f32,
    /// How dark the underside of a thick cloud goes, against its lit edge.
    pub thickness: f32,
    /// Metres per second the layer drifts across the ground, and so how fast
    /// the shadows travel with it.
    pub wind: Vec2,
}

/// Where mist collects and how thick it is. Both densities are the
/// `FogVolume::density_factor` the volumes are built with.
pub struct MistLook {
    /// Mist pooled in the island's own drainage. Zero places none.
    pub valley: f32,
    /// Mist standing at the foot of every fall, scaled by the fall's strength.
    pub fall: f32,
    /// What the mist is lit as.
    pub colour: Color,
    /// The share of its own physical in-scattering the mist returns. See
    /// `mist::light_intensity` for why a share rather than all of it.
    pub glow: f32,
}

/// The named looks, in the order `--weather`, the help text and the HUD list
/// them. The first entry is the default.
pub const LOOKS: [Look; 4] = [
    // Mid-morning, standard air, empty sky: the renderer as it stood before
    // this module, and so the baseline every other look is read against.
    Look {
        name: "clear",
        sun_direction: Vec3::new(-0.48, -0.62, -0.62),
        sun_scale: 1.0,
        mie_scale: 1.0,
        aerial_distance: 3.2e4,
        clouds: CloudLook {
            coverage: 0.0,
            softness: 0.0,
            altitude: 0.0,
            shadow: 0.0,
            thickness: 0.0,
            wind: Vec2::ZERO,
        },
        mist: MistLook {
            valley: 0.0,
            fall: 0.0,
            colour: Color::WHITE,
            glow: 0.0,
        },
        grading: NEUTRAL,
    },
    // Open ocean air over a working sun: aerosol enough that the far coast
    // stands back from the near one, scattered trade cumulus with the shadows
    // to match, and the grade the review asks for — greens and skylight pulled
    // apart, highlights held off the top so snow and pale rock keep detail.
    Look {
        name: "maritime",
        sun_direction: Vec3::new(-0.30, -0.55, -0.78),
        sun_scale: 1.0,
        mie_scale: 1.9,
        aerial_distance: 1.6e4,
        clouds: CloudLook {
            coverage: 0.36,
            softness: 0.16,
            altitude: 1_600.0,
            shadow: 0.62,
            thickness: 0.72,
            wind: Vec2::new(7.5, -3.0),
        },
        mist: MistLook {
            valley: 0.0,
            fall: 0.012,
            colour: Color::srgb(0.94, 0.96, 1.0),
            glow: 0.0026,
        },
        grading: ColorGrading {
            global: ColorGradingGlobal {
                temperature: 0.03,
                tint: -0.03,
                post_saturation: 1.04,
                ..NEUTRAL_GLOBAL
            },
            shadows: ColorGradingSection {
                saturation: 1.12,
                contrast: 1.03,
                lift: 0.006,
                ..NEUTRAL_SECTION
            },
            midtones: ColorGradingSection {
                saturation: 1.04,
                ..NEUTRAL_SECTION
            },
            highlights: ColorGradingSection {
                saturation: 0.92,
                gain: 0.95,
                gamma: 1.05,
                ..NEUTRAL_SECTION
            },
        },
    },
    // Early morning: the sun fifteen degrees up and raking across the island,
    // mist lying in every hollow the drainage cut, and a thin remnant of the
    // night's cloud still overhead. The grade is warm through the midtones and
    // cool in the shadows, which is what the light is actually doing.
    Look {
        name: "valley-mist",
        sun_direction: Vec3::new(0.86, -0.26, -0.44),
        sun_scale: 1.0,
        mie_scale: 2.2,
        aerial_distance: 1.6e4,
        clouds: CloudLook {
            coverage: 0.16,
            softness: 0.22,
            altitude: 2_200.0,
            shadow: 0.34,
            thickness: 0.45,
            wind: Vec2::new(3.0, 1.5),
        },
        mist: MistLook {
            valley: 0.020,
            fall: 0.018,
            colour: Color::srgb(1.0, 0.97, 0.92),
            glow: 0.0030,
        },
        grading: ColorGrading {
            global: ColorGradingGlobal {
                exposure: 0.45,
                temperature: 0.04,
                post_saturation: 0.94,
                ..NEUTRAL_GLOBAL
            },
            shadows: ColorGradingSection {
                saturation: 1.06,
                contrast: 1.02,
                lift: 0.014,
                ..NEUTRAL_SECTION
            },
            midtones: ColorGradingSection {
                saturation: 1.02,
                gain: 1.02,
                ..NEUTRAL_SECTION
            },
            highlights: ColorGradingSection {
                saturation: 0.88,
                gain: 0.93,
                gamma: 1.06,
                ..NEUTRAL_SECTION
            },
        },
    },
    // Overcast. The sun is high and undimmed, and the deck over it is what
    // takes the direct light away almost everywhere, so what is left lighting
    // the island is the sky itself — which is exactly what soft light is. The
    // grade opens the shadows and pulls the saturation back rather than
    // pretending the sun is still out.
    Look {
        name: "overcast",
        sun_direction: Vec3::new(-0.18, -0.90, -0.40),
        sun_scale: 1.0,
        mie_scale: 3.2,
        aerial_distance: 1.6e4,
        clouds: CloudLook {
            coverage: 0.96,
            softness: 0.34,
            altitude: 1_400.0,
            shadow: 0.90,
            thickness: 0.85,
            wind: Vec2::new(5.0, 2.0),
        },
        mist: MistLook {
            valley: 0.009,
            fall: 0.011,
            colour: Color::srgb(0.96, 0.97, 1.0),
            glow: 0.0022,
        },
        grading: ColorGrading {
            global: ColorGradingGlobal {
                exposure: 0.40,
                temperature: 0.0,
                tint: 0.0,
                post_saturation: 0.92,
                ..NEUTRAL_GLOBAL
            },
            shadows: ColorGradingSection {
                saturation: 1.06,
                contrast: 0.97,
                lift: 0.014,
                ..NEUTRAL_SECTION
            },
            midtones: ColorGradingSection {
                saturation: 0.98,
                gain: 1.03,
                ..NEUTRAL_SECTION
            },
            highlights: ColorGradingSection {
                saturation: 0.90,
                gain: 0.92,
                gamma: 1.05,
                ..NEUTRAL_SECTION
            },
        },
    },
];

/// `ColorGrading::default()` written out, because the table is a `const` and
/// `Default::default()` is not available in one.
const NEUTRAL_GLOBAL: ColorGradingGlobal = ColorGradingGlobal {
    exposure: 0.0,
    temperature: 0.0,
    tint: 0.0,
    hue: 0.0,
    post_saturation: 1.0,
    midtones_range: 0.2..0.7,
};
const NEUTRAL_SECTION: ColorGradingSection = ColorGradingSection {
    saturation: 1.0,
    contrast: 1.0,
    gamma: 1.0,
    gain: 1.0,
    lift: 0.0,
};
const NEUTRAL: ColorGrading = ColorGrading {
    global: NEUTRAL_GLOBAL,
    shadows: NEUTRAL_SECTION,
    midtones: NEUTRAL_SECTION,
    highlights: NEUTRAL_SECTION,
};

/// The look `--weather` selects, as an index into [`LOOKS`].
///
/// A resource rather than a plain value because the HUD switches it at runtime
/// and everything that answers to a look watches it for changes. The default is
/// the first entry of the table, which is `clear`.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Weather(usize);

impl Weather {
    /// Every look in table order, for the HUD's list.
    pub fn all() -> impl Iterator<Item = Self> {
        (0..LOOKS.len()).map(Self)
    }

    /// The look itself.
    #[must_use]
    pub fn look(self) -> &'static Look {
        &LOOKS[self.0.min(LOOKS.len() - 1)]
    }

    /// The name `--weather` spells this look with.
    #[must_use]
    pub fn label(self) -> &'static str {
        self.look().name
    }

    /// Looks a `--weather` name up. Rejected at parse time with the valid names
    /// listed, because a capture that quietly carried the default look would be
    /// read as the look it was asked for.
    pub fn named(name: &str) -> Result<Self, String> {
        Self::all()
            .find(|weather| weather.label() == name)
            .ok_or_else(|| {
                format!(
                    "unknown weather {name:?}; expected one of {}",
                    Self::names()
                )
            })
    }

    /// The look names in table order, for help text and parse errors.
    #[must_use]
    pub fn names() -> String {
        LOOKS
            .iter()
            .map(|look| look.name)
            .collect::<Vec<_>>()
            .join(", ")
    }
}

impl Look {
    /// The sun's illuminance under this look, in lux at the top of the
    /// atmosphere.
    #[must_use]
    pub fn illuminance(&self) -> f32 {
        lux::RAW_SUNLIGHT * self.sun_scale
    }

    /// Whether this look has a cloud layer at all.
    #[must_use]
    pub fn has_clouds(&self) -> bool {
        self.clouds.coverage > 0.0
    }

    /// Whether this look places any fog volume.
    #[must_use]
    pub fn has_mist(&self) -> bool {
        self.mist.valley > 0.0 || self.mist.fall > 0.0
    }

    /// The air this look is seen through: the earth medium with its aerosol
    /// term scaled. Both halves of the Mie term move together, because what a
    /// haze does is scatter and absorb in the same proportion the standard
    /// atmosphere already gives it.
    #[must_use]
    pub fn medium(&self) -> ScatteringMedium {
        let mut medium = ScatteringMedium::earth(MEDIUM_RESOLUTION, MEDIUM_RESOLUTION);
        if let Some(mie) = medium.terms.get_mut(MIE_TERM) {
            mie.scattering *= self.mie_scale;
            mie.absorption *= self.mie_scale;
        }
        medium
    }

    /// The image-stack entries this look adds, which a capture's own metadata
    /// names beside the ones the camera always carries. Empty under `clear`, so
    /// its sidecar still reports exactly the stack it did before.
    #[must_use]
    pub fn features(&self) -> Vec<&'static str> {
        let mut features = Vec::new();
        if self.has_clouds() {
            features.push("clouds");
            features.push("cloud-shadows");
        }
        if self.has_mist() {
            features.push("volumetric-fog");
        }
        if !is_neutral(&self.grading) {
            features.push("colour-grading");
        }
        features
    }
}

/// Whether a grade is the neutral one. `ColorGrading` carries no equality of
/// its own, and everything compared here is a constant written out in this
/// table, so the values are compared as written.
#[must_use]
pub fn is_neutral(grading: &ColorGrading) -> bool {
    let (global, neutral) = (&grading.global, &NEUTRAL.global);
    (global.exposure - neutral.exposure).abs() < f32::EPSILON
        && (global.temperature - neutral.temperature).abs() < f32::EPSILON
        && (global.tint - neutral.tint).abs() < f32::EPSILON
        && (global.hue - neutral.hue).abs() < f32::EPSILON
        && (global.post_saturation - neutral.post_saturation).abs() < f32::EPSILON
        && grading.shadows == NEUTRAL_SECTION
        && grading.midtones == NEUTRAL_SECTION
        && grading.highlights == NEUTRAL_SECTION
}

pub struct WeatherPlugin;

impl Plugin for WeatherPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (apply_look, drift_clouds).chain());
    }
}

/// Puts the current look on the sun, the sky and the camera. Guarded on the
/// look rather than run every frame: re-inserting a camera component would have
/// the render world extract it again for nothing, and replacing the scattering
/// medium would rebuild both atmosphere lookup tables.
///
/// The guard starts empty rather than at the default look, so the first frame
/// applies whatever `--weather` asked for even when that is `clear`.
fn apply_look(
    mut commands: Commands,
    weather: Res<Weather>,
    mut applied: Local<Option<Weather>>,
    mut mediums: ResMut<Assets<ScatteringMedium>>,
    mut suns: Query<(Entity, &mut DirectionalLight, &mut Transform), With<Sun>>,
    mut skies: Query<&mut Atmosphere>,
    mut cameras: Query<(Entity, &mut AtmosphereSettings), With<FlyCamera>>,
) {
    if *applied == Some(*weather) {
        return;
    }
    let look = weather.look();
    let Ok((sun, mut light, mut transform)) = suns.single_mut() else {
        return;
    };
    let Ok((camera, mut atmosphere_settings)) = cameras.single_mut() else {
        return;
    };

    light.illuminance = look.illuminance();
    transform.rotation = Transform::default()
        .looking_to(look.sun_direction, Vec3::Y)
        .rotation;
    let mut sun = commands.entity(sun);
    if look.has_mist() {
        sun.insert(VolumetricLight);
    } else {
        sun.remove::<VolumetricLight>();
    }

    for mut sky in &mut skies {
        sky.medium = mediums.add(look.medium());
    }

    atmosphere_settings.aerial_view_lut_max_distance = look.aerial_distance;
    let mut camera = commands.entity(camera);
    camera.insert(look.grading.clone());
    if look.has_mist() {
        camera.insert(VolumetricFog {
            // Left at nothing on purpose. The pass adds this term over the
            // whole of a volume's box without asking the density texture
            // anything, so it is the one thing that can draw a volume's own
            // silhouette across the terrain; what lights the mist is the sun,
            // through `mist::light_intensity`.
            ambient_intensity: 0.0,
            // The temporal resolve is what turns a jittered ray start back into
            // a smooth volume, and this camera always has it.
            jitter: MIST_JITTER,
            step_count: MIST_STEPS,
            ..default()
        });
    } else {
        camera.remove::<VolumetricFog>();
    }

    *applied = Some(*weather);
    info!("weather: {}", look.name);
}

/// How far the fog raymarch may start off its own ray, and how many steps it
/// takes. Sixty-four is the component's own default and more than a mist this
/// thin needs; the jitter is what the temporal resolve trades the difference
/// against.
const MIST_JITTER: f32 = 0.7;
const MIST_STEPS: u32 = 40;

/// Drifts the cloud layer downwind.
///
/// The whole field — the layer in the sky and the shadow on the ground — is one
/// pattern projected along the sun, and the sun's own translation is where that
/// projection is centred. A directional light takes nothing else from its
/// position: the cascades are built from its rotation alone. World-space wind
/// is projected onto the light's right/up plane and each coordinate is wrapped
/// over the shared field/detail period before that origin is reconstructed.
/// The bounded translation therefore moves clouds and shadows together without
/// a reset at the water shader clock's own wrap.
///
/// The clock is the one `--screenshot` freezes, so a capture catches the layer
/// where the last capture of the same command left it.
fn drift_clouds(
    weather: Res<Weather>,
    clock: Res<WaterClock>,
    mut suns: Query<&mut Transform, With<Sun>>,
) {
    let look = weather.look();
    let Ok(mut transform) = suns.single_mut() else {
        return;
    };
    let drift = if look.has_clouds() {
        cloud_drift(
            look.clouds.wind,
            clock.elapsed_seconds(),
            *transform.right(),
            *transform.up(),
        )
    } else {
        Vec3::ZERO
    };
    if transform.translation != drift {
        transform.translation = drift;
    }
}

fn centred_wrap(value: f64, period: f64) -> f64 {
    (value + period * 0.5).rem_euclid(period) - period * 0.5
}

/// The bounded light-plane origin equivalent to a world-space wind journey.
#[allow(
    clippy::cast_possible_truncation,
    reason = "each projected offset is bounded to thirty kilometres before conversion"
)]
fn cloud_drift(wind: Vec2, seconds: f64, right: Vec3, up: Vec3) -> Vec3 {
    let travelled = DVec3::new(
        f64::from(wind.x) * seconds,
        0.0,
        f64::from(wind.y) * seconds,
    );
    let period = f64::from(DRIFT_WRAP_METRES);
    let along_right = centred_wrap(travelled.dot(right.as_dvec3()), period) as f32;
    let along_up = centred_wrap(travelled.dot(up.as_dvec3()), period) as f32;
    right * along_right + up * along_up
}

#[cfg(test)]
mod tests {
    use bevy::prelude::{Transform, Vec2, Vec3};

    use super::{DRIFT_WRAP_METRES, LOOKS, Weather, centred_wrap, cloud_drift, is_neutral};

    #[test]
    fn cloud_drift_is_bounded_in_the_rotated_projection_plane() {
        let rotation = Transform::default()
            .looking_to(Vec3::new(-0.3, -0.55, -0.78), Vec3::Y)
            .rotation;
        let (right, up) = (rotation * Vec3::X, rotation * Vec3::Y);
        let drift = cloud_drift(Vec2::new(7.5, -3.0), 123_456.75, right, up);
        let half_period = DRIFT_WRAP_METRES * 0.5;
        assert!(drift.dot(right).abs() <= half_period + 0.01);
        assert!(drift.dot(up).abs() <= half_period + 0.01);

        let period = f64::from(DRIFT_WRAP_METRES);
        assert!(
            (centred_wrap(12.5 + period * 7.0, period) - centred_wrap(12.5, period)).abs()
                < f64::EPSILON
        );
    }

    /// The names are what the flag, the help text, the HUD and the capture
    /// sidecar all spell a look with, so a duplicate would quietly merge two of
    /// them, and every name has to come back as the look it names.
    #[test]
    fn every_look_is_listed_once() {
        for (index, look) in LOOKS.iter().enumerate() {
            assert!(
                LOOKS[..index]
                    .iter()
                    .all(|earlier| earlier.name != look.name),
                "{} is listed twice",
                look.name
            );
            assert_eq!(Weather::named(look.name).unwrap().label(), look.name);
        }
        assert_eq!(Weather::all().count(), LOOKS.len());
    }

    /// A misspelled look has to stop the run with the valid ones listed. A
    /// capture that quietly carried the default instead would be read as the
    /// look it was asked for.
    #[test]
    fn rejects_an_unknown_look() {
        let error = Weather::named("stormy").unwrap_err();
        assert!(error.contains("unknown weather"), "{error}");
        for look in &LOOKS {
            assert!(error.contains(look.name), "{error}");
        }
    }

    /// `clear` is the baseline every capture taken before weather existed is
    /// still read against, so it has to add nothing at all: no cloud layer, no
    /// fog volume, standard air and a neutral grade.
    #[test]
    fn the_default_look_is_the_renderer_without_weather() {
        let weather = Weather::default();
        assert_eq!(weather.label(), "clear");
        let look = weather.look();
        assert!(!look.has_clouds());
        assert!(!look.has_mist());
        assert!((look.mie_scale - 1.0).abs() < f32::EPSILON);
        assert!((look.sun_scale - 1.0).abs() < f32::EPSILON);
        assert!((look.aerial_distance - 3.2e4).abs() < f32::EPSILON);
        assert!(is_neutral(&look.grading));
        assert!(look.features().is_empty());
    }

    /// Every look away from the default has to actually be one: a sun somewhere
    /// else, air of its own, and something the image stack reports.
    #[test]
    fn every_named_look_is_a_place_of_its_own() {
        let clear = Weather::default().look();
        for look in LOOKS.iter().skip(1) {
            assert!(
                look.sun_direction.distance(clear.sun_direction) > 0.05,
                "{} stands the sun where clear does",
                look.name
            );
            assert!(look.mie_scale > clear.mie_scale, "{}", look.name);
            assert!(!look.features().is_empty(), "{}", look.name);
            // A direction is only ever used to look along, so it must have one.
            assert!(look.sun_direction.length() > 0.5, "{}", look.name);
        }
    }
}
