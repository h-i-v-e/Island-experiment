//! The generator's parameters as one table.
//!
//! Every `IslandOptions` field is listed once, with the `--flag` it is spelled
//! with, the range the HUD offers it over and an accessor for the field itself.
//! The argument parser, the help text, the HUD panel, the command line the HUD
//! reports and the cache key all walk this table, so a parameter can only be
//! named one way and a field added to `IslandOptions` is added here once.

use std::fmt::Write as _;

use motu::IslandOptions;

/// Where a parameter appears in the HUD. Table order is `IslandOptions`
/// declaration order, which the cache key depends on; the group is what the
/// panel lays the sliders out by.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Group {
    Terrain,
    Hydraulics,
    Rivers,
}

impl Group {
    /// In panel order.
    pub const ALL: [Self; 3] = [Self::Terrain, Self::Hydraulics, Self::Rivers];

    #[must_use]
    pub const fn title(self) -> &'static str {
        match self {
            Self::Terrain => "Terrain",
            Self::Hydraulics => "Hydraulics",
            Self::Rivers => "Rivers",
        }
    }
}

/// One scalar parameter.
pub struct Parameter {
    pub flag: &'static str,
    pub group: Group,
    /// The bounds the HUD's slider spans. Where `Island::generate` validates the
    /// field these are its own limits; the rest are working ranges, and neither
    /// the parser nor the generator holds a value to them.
    pub minimum: f32,
    pub maximum: f32,
    /// True for a field whose useful values span orders of magnitude, so the
    /// slider spaces them evenly instead of crowding them at one end.
    pub logarithmic: bool,
    pub field: fn(&mut IslandOptions) -> &mut f32,
}

impl Parameter {
    /// The flag without its dashes, which is what the HUD labels the slider
    /// with: the control and the flag that reproduces it read the same.
    #[must_use]
    pub fn label(&self) -> &'static str {
        &self.flag[2..]
    }
}

/// Every `f32` field of `IslandOptions`, in declaration order.
///
/// `terrain_size` is the one field that is not an `f32` and is handled beside
/// the table, under [`TERRAIN_SIZE_FLAG`].
pub const PARAMETERS: [Parameter; 14] = [
    // Validated only as finite and above zero. Past about 0.5 the massif
    // reaches heights the river and coastal passes were never framed against.
    Parameter {
        flag: "--max-height",
        group: Group::Terrain,
        minimum: 0.02,
        maximum: 0.6,
        logarithmic: false,
        field: |options| &mut options.max_height,
    },
    // The range the Unity viewer constrains water coverage to. The generator
    // does not hold it to anything, but under about 0.6 the land reaches the
    // edges of the terrain square and is cut off by them rather than by a
    // coast, so the slider does not go there. The flag still will.
    Parameter {
        flag: "--water-ratio",
        group: Group::Terrain,
        minimum: 0.6,
        maximum: 0.95,
        logarithmic: false,
        field: |options| &mut options.water_ratio,
    },
    Parameter {
        flag: "--slope-multiplier",
        group: Group::Terrain,
        minimum: 0.1,
        maximum: 4.0,
        logarithmic: false,
        field: |options| &mut options.slope_multiplier,
    },
    // The `eroded` variant runs this at 0.25, so the range reaches well below it.
    Parameter {
        flag: "--coastal-slope-multiplier",
        group: Group::Terrain,
        minimum: 0.05,
        maximum: 4.0,
        logarithmic: false,
        field: |options| &mut options.coastal_slope_multiplier,
    },
    // Validated: 0 to 8.
    Parameter {
        flag: "--hydraulic-erosion-strength",
        group: Group::Hydraulics,
        minimum: 0.0,
        maximum: 8.0,
        logarithmic: false,
        field: |options| &mut options.hydraulic_erosion_strength,
    },
    // Validated: 0 to 4.
    Parameter {
        flag: "--hydraulic-deposition-strength",
        group: Group::Hydraulics,
        minimum: 0.0,
        maximum: 4.0,
        logarithmic: false,
        field: |options| &mut options.hydraulic_deposition_strength,
    },
    // Validated: 1 to 45 degrees.
    Parameter {
        flag: "--hydraulic-deposition-slope-degrees",
        group: Group::Hydraulics,
        minimum: 1.0,
        maximum: 45.0,
        logarithmic: false,
        field: |options| &mut options.hydraulic_deposition_slope_degrees,
    },
    // The range the Unity viewer constrains the catchment to. Its default of
    // 0.05 ha sits in the bottom half per cent of it, so the slider is
    // logarithmic or every useful value lands on the first pixel.
    Parameter {
        flag: "--river-source-catchment-hectares",
        group: Group::Rivers,
        minimum: 0.01,
        maximum: 10.0,
        logarithmic: true,
        field: |options| &mut options.river_source_catchment_hectares,
    },
    // The range the Unity viewer constrains the steep-slope multiplier to.
    Parameter {
        flag: "--river-source-steep-multiplier",
        group: Group::Rivers,
        minimum: 1.0,
        maximum: 8.0,
        logarithmic: false,
        field: |options| &mut options.river_source_steep_multiplier,
    },
    // The range the Unity viewer constrains the elevation boost to.
    Parameter {
        flag: "--river-source-elevation-boost",
        group: Group::Rivers,
        minimum: 0.0,
        maximum: 20.0,
        logarithmic: false,
        field: |options| &mut options.river_source_elevation_boost,
    },
    Parameter {
        flag: "--river-source-width-metres",
        group: Group::Rivers,
        minimum: 0.25,
        maximum: 30.0,
        logarithmic: false,
        field: |options| &mut options.river_source_width_metres,
    },
    Parameter {
        flag: "--river-maximum-width-metres",
        group: Group::Rivers,
        minimum: 0.25,
        maximum: 60.0,
        logarithmic: false,
        field: |options| &mut options.river_maximum_width_metres,
    },
    Parameter {
        flag: "--river-source-depth-metres",
        group: Group::Rivers,
        minimum: 0.05,
        maximum: 5.0,
        logarithmic: false,
        field: |options| &mut options.river_source_depth_metres,
    },
    Parameter {
        flag: "--river-maximum-depth-metres",
        group: Group::Rivers,
        minimum: 0.05,
        maximum: 10.0,
        logarithmic: false,
        field: |options| &mut options.river_maximum_depth_metres,
    },
];

pub const SEED_FLAG: &str = "--seed";
pub const TERRAIN_SIZE_FLAG: &str = "--terrain-size";
/// The seed-point count `Island::generate` validates against.
pub const TERRAIN_SIZE_RANGE: std::ops::RangeInclusive<u32> = 16..=4096;

/// The table entry one `--flag` names, if it names one. A value written through
/// it is taken as written: the table's bounds are the HUD's sliders, and
/// `Island::generate` is the only thing that rejects a parameter.
#[must_use]
pub fn parameter(flag: &str) -> Option<&'static Parameter> {
    PARAMETERS.iter().find(|entry| entry.flag == flag)
}

/// Raises the two maxima to their own source values, which is the one
/// constraint `Island::generate` enforces between fields rather than within
/// one. Sliders move independently, so without this a maximum dragged under its
/// source would only fail once the generator ran.
pub fn reconcile(options: &mut IslandOptions) {
    options.river_maximum_width_metres = options
        .river_maximum_width_metres
        .max(options.river_source_width_metres);
    options.river_maximum_depth_metres = options
        .river_maximum_depth_metres
        .max(options.river_source_depth_metres);
}

/// The full argument list one island is reproduced from, in table order. Every
/// parameter is spelled out rather than only those away from their defaults, so
/// the line stands on its own however the defaults later move.
#[must_use]
pub fn command_line(seed: u64, options: &IslandOptions) -> String {
    let mut options = *options;
    let mut line = format!(
        "{SEED_FLAG} {seed} {TERRAIN_SIZE_FLAG} {}",
        options.terrain_size
    );
    for parameter in &PARAMETERS {
        let value = *(parameter.field)(&mut options);
        // `{}` on an f32 is the shortest form that parses back to the same
        // value, so the line round-trips through the parser exactly.
        let _ = write!(line, " {} {value}", parameter.flag);
    }
    line
}

/// One `--flag <RANGE>` line per parameter, for the help text.
#[must_use]
pub fn help_lines() -> String {
    let mut text = String::new();
    for group in Group::ALL {
        let _ = writeln!(text, "\n  {}:", group.title());
        for parameter in PARAMETERS.iter().filter(|entry| entry.group == group) {
            // Padding applies to the whole argument, so the flag and its
            // placeholder are one string before the column is measured.
            let flag = format!("{} <VALUE>", parameter.flag);
            let _ = writeln!(
                text,
                "    {flag:<46}{} to {}",
                parameter.minimum, parameter.maximum
            );
        }
    }
    text
}

#[cfg(test)]
mod tests {
    use motu::IslandOptions;

    use super::{PARAMETERS, command_line, parameter, reconcile};

    /// The table is what the cache key, the parser and the HUD all read, so a
    /// duplicate flag or a field listed twice would quietly merge two
    /// parameters into one.
    #[test]
    fn every_parameter_is_listed_once() {
        for (index, parameter) in PARAMETERS.iter().enumerate() {
            assert!(parameter.flag.starts_with("--"), "{}", parameter.flag);
            assert!(
                PARAMETERS[..index]
                    .iter()
                    .all(|earlier| earlier.flag != parameter.flag),
                "{} is listed twice",
                parameter.flag
            );
            // Writing through one accessor must move that field and no other.
            let mut options = IslandOptions::default();
            let mut defaults = IslandOptions::default();
            *(parameter.field)(&mut options) = 12.5;
            for other in PARAMETERS
                .iter()
                .filter(|entry| entry.flag != parameter.flag)
            {
                assert!(
                    (*(other.field)(&mut options) - *(other.field)(&mut defaults)).abs()
                        < f32::EPSILON,
                    "{} and {} name the same field",
                    parameter.flag,
                    other.flag
                );
            }
        }
    }

    #[test]
    fn ranges_hold_the_defaults() {
        let mut defaults = IslandOptions::default();
        for parameter in &PARAMETERS {
            let value = *(parameter.field)(&mut defaults);
            assert!(
                (parameter.minimum..=parameter.maximum).contains(&value),
                "{} defaults to {value}, outside {} to {}",
                parameter.flag,
                parameter.minimum,
                parameter.maximum
            );
        }
    }

    #[test]
    fn looks_a_flag_up_by_name() {
        let mut options = IslandOptions::default();
        let found = parameter("--max-height").expect("--max-height is in the table");
        *(found.field)(&mut options) = 0.35;
        assert!((options.max_height - 0.35).abs() < f32::EPSILON);
        assert!(parameter("--not-a-parameter").is_none());
    }

    #[test]
    fn a_maximum_never_stays_under_its_source() {
        let mut options = IslandOptions {
            river_source_width_metres: 20.0,
            river_maximum_width_metres: 14.0,
            river_source_depth_metres: 3.0,
            river_maximum_depth_metres: 2.0,
            ..IslandOptions::default()
        };
        reconcile(&mut options);
        assert!((options.river_maximum_width_metres - 20.0).abs() < f32::EPSILON);
        assert!((options.river_maximum_depth_metres - 3.0).abs() < f32::EPSILON);
    }

    /// The reported line has to name every parameter, or an island found in the
    /// HUD could not be opened again from the command line.
    #[test]
    fn the_command_line_names_every_parameter() {
        let line = command_line(666, &IslandOptions::default());
        assert!(line.starts_with("--seed 666 --terrain-size 1024"));
        for parameter in &PARAMETERS {
            assert!(
                line.contains(parameter.flag),
                "{} is missing",
                parameter.flag
            );
        }
    }
}
