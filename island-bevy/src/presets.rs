//! The showcase presets as one table.
//!
//! Each entry is a seed and the parameters that go with it, found by generating
//! candidates and looking at them rather than by reasoning about the generator.
//! Together they are ten islands the generator makes that do not look like one
//! another: what the parameter panel can be dragged to, already arrived at.
//!
//! `terrain_size` is deliberately not part of a preset. It is the one parameter
//! that costs minutes rather than changing what the island is, and the viewer is
//! usually opened at whatever size the machine and the moment call for, so
//! [`Preset::options`] takes it from the caller instead.

use motu::IslandOptions;

/// The generator's own defaults, spelled out because [`Default::default`] is not
/// a const function and the table has to be one. `presets_start_from_the_
/// generator_defaults` holds the two together.
const DEFAULTS: IslandOptions = IslandOptions {
    max_height: 0.2,
    water_ratio: 0.6,
    slope_multiplier: 1.3,
    coastal_slope_multiplier: 1.0,
    hydraulic_erosion_strength: 1.0,
    hydraulic_deposition_strength: 1.5,
    hydraulic_deposition_slope_degrees: 12.0,
    river_source_catchment_hectares: 0.05,
    river_source_steep_multiplier: 4.0,
    river_source_elevation_boost: 9.0,
    river_source_width_metres: 2.0,
    river_maximum_width_metres: 14.0,
    river_source_depth_metres: 0.35,
    river_maximum_depth_metres: 2.0,
    // Replaced by whatever the viewer is already running at; see
    // [`Preset::options`].
    terrain_size: 1024,
};

/// One curated island.
pub struct Preset {
    /// What it is called in the panel, and what its capture is named.
    pub name: &'static str,
    /// One line on what the generator did here, shown as the button's tooltip.
    pub character: &'static str,
    pub seed: u64,
    /// Every parameter but `terrain_size`, which [`Preset::options`] replaces.
    options: IslandOptions,
}

impl Preset {
    /// The preset's parameters at the size the viewer is already running.
    #[must_use]
    pub const fn options(&self, terrain_size: u32) -> IslandOptions {
        IslandOptions {
            terrain_size,
            ..self.options
        }
    }
}

/// Ten islands, in panel order, which runs from the tallest relief to the
/// flattest.
pub const PRESETS: [Preset; 10] = [
    // The slope multiplier at its ceiling over the gentlest coastal falloff the
    // panel offers: the interior stands up as bare rock and the shore lies
    // almost flat, which is what leaves the spires with nothing around them.
    Preset {
        name: "The Spires",
        character: "a field of bare rock spires out of a low green skirt, with \
                    almost nothing standing between them",
        seed: 123,
        options: IslandOptions {
            max_height: 0.2,
            water_ratio: 0.9,
            slope_multiplier: 4.0,
            coastal_slope_multiplier: 0.05,
            ..DEFAULTS
        },
    },
    // A steep interior over a steep coast, which is the pair that leaves a cone
    // a whole island to stand on rather than a rim.
    Preset {
        name: "Lone Cone",
        character: "one cone over the whole island, drowned valleys and lagoons \
                    cut into the skirt all the way round it",
        seed: 3,
        options: IslandOptions {
            max_height: 0.28,
            water_ratio: 0.78,
            slope_multiplier: 3.6,
            coastal_slope_multiplier: 2.0,
            ..DEFAULTS
        },
    },
    Preset {
        name: "Stone Tower",
        character: "a single flat-topped tower with near-vertical sides, standing \
                    off-centre on a narrow green shelf",
        seed: 77,
        options: IslandOptions {
            max_height: 0.25,
            water_ratio: 0.88,
            slope_multiplier: 3.0,
            hydraulic_erosion_strength: 2.5,
            ..DEFAULTS
        },
    },
    // Moderate height and a gentle coast, with the erosion turned up enough to
    // cut the flanks and the deposition to fill the aprons under them.
    Preset {
        name: "Snowcap Massif",
        character: "the most land in the set: a snow-capped summit over deeply \
                    gullied flanks and a wide green apron",
        seed: 200,
        options: IslandOptions {
            max_height: 0.22,
            water_ratio: 0.65,
            slope_multiplier: 1.8,
            coastal_slope_multiplier: 0.6,
            hydraulic_erosion_strength: 4.0,
            hydraulic_deposition_strength: 2.0,
            river_source_catchment_hectares: 0.03,
            ..DEFAULTS
        },
    },
    // Erosion at its ceiling with deposition off and the deposition angle at its
    // floor: nothing that is carved is filled back in.
    Preset {
        name: "Gullied Ridges",
        character: "two ridges rather than a peak, carved by the strongest \
                    hydraulic erosion the generator takes with nothing settling",
        seed: 8,
        options: IslandOptions {
            max_height: 0.27,
            water_ratio: 0.82,
            slope_multiplier: 4.0,
            coastal_slope_multiplier: 1.5,
            hydraulic_erosion_strength: 8.0,
            hydraulic_deposition_strength: 0.0,
            hydraulic_deposition_slope_degrees: 2.0,
            ..DEFAULTS
        },
    },
    // The opposite of the ridges: no hydraulic erosion at all and deposition at
    // its ceiling, which leaves a surface nothing has cut.
    Preset {
        name: "Uncut Dome",
        character: "a dome no hydraulic pass has touched — erosion off, \
                    deposition at its ceiling — over a scalloped shore",
        seed: 64,
        options: IslandOptions {
            max_height: 0.1,
            water_ratio: 0.82,
            slope_multiplier: 0.5,
            coastal_slope_multiplier: 0.15,
            hydraulic_erosion_strength: 0.0,
            hydraulic_deposition_strength: 4.0,
            ..DEFAULTS
        },
    },
    // Every river parameter pushed at once: the smallest catchment a source is
    // allowed, no penalty for a steep edge, the largest low-ground boost, and
    // channels three to five times the default width and depth.
    Preset {
        name: "River Country",
        character: "every river parameter at its extreme: the smallest catchment \
                    a source needs, channels 45 m across and 5 m deep, braided to the sea",
        seed: 31,
        options: IslandOptions {
            max_height: 0.09,
            water_ratio: 0.7,
            slope_multiplier: 1.2,
            coastal_slope_multiplier: 0.6,
            hydraulic_erosion_strength: 3.5,
            hydraulic_deposition_strength: 2.0,
            river_source_catchment_hectares: 0.01,
            river_source_steep_multiplier: 1.0,
            river_source_elevation_boost: 20.0,
            river_source_width_metres: 8.0,
            river_maximum_width_metres: 45.0,
            river_source_depth_metres: 1.2,
            river_maximum_depth_metres: 5.0,
            ..DEFAULTS
        },
    },
    // Heavy erosion carrying sediment that settles on everything under 45
    // degrees, which is all of a low island: what is cut inland is laid back
    // down at the shore.
    Preset {
        name: "Silted Shore",
        character: "a low island inside the broadest sand fringe in the set, from \
                    deposition that settles on any slope at all",
        seed: 27,
        options: IslandOptions {
            max_height: 0.12,
            water_ratio: 0.75,
            hydraulic_erosion_strength: 6.0,
            hydraulic_deposition_strength: 4.0,
            hydraulic_deposition_slope_degrees: 45.0,
            river_source_catchment_hectares: 0.02,
            river_maximum_width_metres: 55.0,
            river_maximum_depth_metres: 6.0,
            ..DEFAULTS
        },
    },
    // The flattest island the panel can ask for: the height near its floor and
    // both slope multipliers near theirs. Below a water ratio of about 0.8 the
    // shallow bank around it reaches the edge of the terrain square and is cut
    // off square, which is what sets the water here.
    Preset {
        name: "Tidal Flats",
        character: "a plain with nothing on it 60 m above the sea, lagoon-cut and \
                    bleached pale, inside the widest shallow bank in the set",
        seed: 250,
        options: IslandOptions {
            max_height: 0.03,
            water_ratio: 0.82,
            slope_multiplier: 0.12,
            coastal_slope_multiplier: 0.05,
            hydraulic_erosion_strength: 0.0,
            hydraulic_deposition_strength: 4.0,
            ..DEFAULTS
        },
    },
    // The least land in the set: a height near the panel's floor under the most
    // water it offers.
    Preset {
        name: "Bare Atoll",
        character: "the least land in the set — a ninth of the island Snowcap \
                    Massif is — bare low ground inside a sand ring",
        seed: 21,
        options: IslandOptions {
            max_height: 0.035,
            water_ratio: 0.93,
            slope_multiplier: 0.6,
            coastal_slope_multiplier: 0.35,
            hydraulic_erosion_strength: 0.5,
            hydraulic_deposition_strength: 3.0,
            ..DEFAULTS
        },
    },
];

#[cfg(test)]
mod tests {
    use motu::{Island, IslandOptions};

    use super::{DEFAULTS, PRESETS};

    /// The table is written against the generator's defaults, so a default that
    /// moves has to be brought here rather than silently changing ten islands.
    #[test]
    fn presets_start_from_the_generator_defaults() {
        assert_eq!(DEFAULTS, IslandOptions::default());
    }

    #[test]
    fn every_preset_is_named_once() {
        for (index, preset) in PRESETS.iter().enumerate() {
            assert!(!preset.name.is_empty());
            assert!(!preset.character.is_empty());
            assert!(
                PRESETS[..index]
                    .iter()
                    .all(|earlier| earlier.name != preset.name),
                "{} is listed twice",
                preset.name
            );
        }
    }

    /// A preset the generator rejects would put the panel into its failure
    /// state on a click, so every one of them is generated here. Sixteen seed
    /// points is the smallest island `Island::generate` accepts, which runs the
    /// whole validation and the whole pipeline in milliseconds.
    #[test]
    fn every_preset_generates() {
        for preset in &PRESETS {
            let options = preset.options(16);
            assert_eq!(options.terrain_size, 16);
            if let Err(error) = Island::generate(preset.seed, options) {
                panic!("{}: {error}", preset.name);
            }
        }
    }

    /// The size a preset is opened at is the caller's, never the table's.
    #[test]
    fn a_preset_takes_the_callers_terrain_size() {
        for preset in &PRESETS {
            for size in [16_u32, 256, 1024] {
                assert_eq!(preset.options(size).terrain_size, size);
            }
        }
    }
}
