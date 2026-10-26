use planet_rs::{PlanetOptions, TerrainOptions};
use std::{path::PathBuf, str::FromStr};

pub const HELP: &str = "Planet experiment — radial terrain, erosion, oceans and rivers

cargo run -p planet-bevy -- [options]
  --seed N                  Deterministic seed (42)
  --vertices N              Surface points, 4..8192 (4096)
  --radius R                Sea-level radius in metres (1000 = 1 km)
  --sweep-axis x|y|z         Sphere construction plane normal (y)
  --relief F                Noise amplitude / sea radius (0.16)
  --land-bias F             Signed elevation offset / radius (0)
  --continental-frequency F Continental noise frequency (2.2)
  --detail-frequency F      Detail noise frequency (12)
  --erosion-passes N        Sequential hydraulic sweeps per stage, 0..128 (1)
  --erosion-strength F      Hydraulic stage strength, 0..4 (1)
  --thermal-passes N        Thermal passes, 0..128 (8)
  --river-catchment F       Rainfall area / whole sphere area (0.0015)
  --river-depth F           Channel depth / radius (0.0015)
  --refinements N           Broad/adaptive tessellation stages, 0..6 (5)
  --smoothings N            Smoothing passes per stage, 0..12 (3)
  --river-refinements N     Local channel tessellation stages, 0..3 (3)
  --river-width F           Base angular channel half-width (0.003)
  --no-rivers               Hide river meshes/textures to inspect carved beds
  --uncarved                Inspect terrain before the final channel carve
  --sphere-only             Original undeformed triangulation
  --separate-seas           Disable ocean-connecting channels for comparison
  --no-ocean                Start with sea hidden to inspect the seabed
  --hide-controls           Start with the parameter panel hidden (F1 toggles)
  --wireframe               Start with terrain triangle edges visible
  --chunk-level N           Icosahedron split depth, 0..3 (2 = 320 cells)
  --chunk-colours           Colour chunks to inspect boundaries
  --no-horizon              Disable planet horizon culling for comparison
  --lod auto|0|1|2|3        Automatic or forced chunk detail (auto; 0 finest)
  --chunk-budget-mb N       Fine-mesh memory budget, 1..512 MiB (192)
  --lod-error F             Screen-space error target, 0.25..16 pixels (2)
  --lod-surface-maps on|off Baked normal and occlusion maps for LOD1–3 (on)
  --surface-view shaded|normals|occlusion  Inspect the baked surface maps
  --no-shadows              Disable sun shadow maps
  --shadow-distance M       Surface shadow range, 20..3000 m (300)
  --shadow-bias M           Shadow depth bias in metres, 0..0.25 (0.02)
  --no-ssao                 Disable final-depth ambient occlusion
  --ao-radius M             AO radius in metres, 0.01..20 (0.05)
  --ao-intensity F          AO strength, 0..2.5 (0.3)
  --ao-samples N            Hemisphere samples: 6, 10 or 12 (12)
  --ao-half                 Half-resolution AO with bilateral upsampling
  --ao-only                 Show the screen-space AO visibility
  --ocean-amplitude F       Ocean wave height multiplier, 0..3 (1)
  --ocean-speed F           Ocean animation multiplier, 0..4 (1)
  --ocean-drift F           Source wandering speed, 0..2 m/s (0.35)
  --ocean-refraction F      Screen-space refraction, 0..1 (0.8)
  --ocean-view MODE         shaded, depth, distance or sources
  --ocean-paused            Freeze ocean animation for inspection
  --no-ocean-waves          Disable geometric displacement
  --water-distortion F     Independent surface distortion, 0..1.5 (1)
  --water-reflections F    Screen-space reflection weight, 0..1 (1)
  --water-absorption F     RGB absorption multiplier, 0..4 (1)
  --river-whitewater F     River whitewater strength, 0..1 (0.9)
  --waterfall-mist F       Plunge mist density multiplier, 0..2 (1)
  --water-foam F           Persistent ocean foam strength, 0..2 (0.8)
  --no-underwater          Disable underwater absorption and haze
  --view front|back|river|waterfall|mouth|grass|ocean|shore|underwater   Inspection camera (front)
  --export PATH.obj         Export terrain mesh in physical units
  --validate-only           Generate/validate/export without a GPU or window
  --screenshot PATH.png     Render offscreen to a new PNG and exit
  -h, --help                Show help

Click the planet: first person. Drag: orbit; scroll: zoom.
On foot: WASD move; mouse look; Shift run; Space jump; Tab release mouse;
Esc return to orbit. In orbit: W edges; S shading; O sea; R rivers; B beds.
Sea level stays at the original radius. No forced water ratio, island falloff,
or perimeter ocean. Rivers end at the first submerged vertex.
Small random point sets can fail whole-sphere coverage.";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum GenerationMode {
    Sphere,
    #[default]
    Terrain,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum View {
    #[default]
    Front,
    Back,
    River,
    Waterfall,
    Mouth,
    Grass,
    Ocean,
    Shore,
    Underwater,
}

impl View {
    fn parse(value: &str) -> Result<Self, String> {
        Ok(match value {
            "front" => Self::Front,
            "back" => Self::Back,
            "river" => Self::River,
            "waterfall" => Self::Waterfall,
            "mouth" => Self::Mouth,
            "grass" => Self::Grass,
            "ocean" => Self::Ocean,
            "shore" => Self::Shore,
            "underwater" => Self::Underwater,
            _ => {
                return Err(String::from(
                    "view must be front, back, river, waterfall, mouth, grass, ocean, shore or underwater",
                ));
            }
        })
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BedView {
    #[default]
    Carved,
    Original,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RiverDisplay {
    #[default]
    Show,
    Hide,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PanelVisibility {
    #[default]
    Show,
    Hidden,
}

pub struct Command {
    pub options: PlanetOptions,
    pub terrain: TerrainOptions,
    pub generation: GenerationMode,
    pub bed: BedView,
    pub rivers: RiverDisplay,
    pub no_ocean: bool,
    pub view: View,
    pub wireframe: bool,
    pub panel: PanelVisibility,
    pub export: Option<PathBuf>,
    pub screenshot: Option<PathBuf>,
    pub validate_only: bool,
    pub streaming: crate::streaming::StreamingSettings,
    pub lighting: crate::lighting::LightingSettings,
    pub ocean: crate::ocean::OceanSettings,
    pub water: crate::water::WaterSettings,
}

impl Default for Command {
    fn default() -> Self {
        Self {
            options: PlanetOptions {
                vertex_count: 4096,
                ..PlanetOptions::default()
            },
            terrain: TerrainOptions::default(),
            generation: GenerationMode::Terrain,
            bed: BedView::Carved,
            rivers: RiverDisplay::Show,
            no_ocean: false,
            view: View::Front,
            wireframe: false,
            panel: PanelVisibility::Show,
            export: None,
            screenshot: None,
            validate_only: false,
            streaming: crate::streaming::StreamingSettings::default(),
            lighting: crate::lighting::LightingSettings::default(),
            ocean: crate::ocean::OceanSettings::default(),
            water: crate::water::WaterSettings::default(),
        }
    }
}

impl Command {
    pub fn parse(args: impl IntoIterator<Item = String>) -> Result<Option<Self>, String> {
        let mut command = Self::default();
        let mut args = args.into_iter();
        while let Some(flag) = args.next() {
            match flag.as_str() {
                "-h" | "--help" => return Ok(None),
                "--ocean-paused" => command.ocean.paused = true,
                "--no-ocean-waves" => command.ocean.waves = false,
                "--no-underwater" => command.water.underwater = false,
                "--no-shadows" => command.lighting.shadows = false,
                "--no-ssao" => command.lighting.ao = false,
                "--ao-only" => command.lighting.ao_only = true,
                "--ao-half" => command.lighting.half_resolution = true,
                "--wireframe" => command.wireframe = true,
                "--chunk-colours" => command.streaming.colours = true,
                "--no-horizon" => command.streaming.horizon = false,
                "--hide-controls" => command.panel = PanelVisibility::Hidden,
                "--validate-only" => command.validate_only = true,
                "--sphere-only" => command.generation = GenerationMode::Sphere,
                "--separate-seas" => command.terrain.connect_seas = false,
                "--no-ocean" => command.no_ocean = true,
                "--no-rivers" => command.rivers = RiverDisplay::Hide,
                "--uncarved" => command.bed = BedView::Original,
                _ => {
                    let value = args.next().ok_or_else(|| {
                        format!("{flag} requires a value or is not a supported option")
                    })?;
                    command.apply_value(&flag, &value)?;
                }
            }
        }
        command.lighting.validate()?;
        command.ocean.validate()?;
        command.water.validate()?;
        command
            .terrain
            .validate()
            .map_err(|error| error.to_string())?;
        if command.streaming.level > 3 {
            return Err(String::from("chunk level must be 0..3"));
        }
        if !(1..=512).contains(&command.streaming.budget_mb)
            || !command.streaming.target_pixels.is_finite()
            || !(0.25..=16.0).contains(&command.streaming.target_pixels)
        {
            return Err(String::from(
                "chunk budget must be 1..512 MiB and LOD error 0.25..16 pixels",
            ));
        }
        if command.validate_only && command.screenshot.is_some() {
            return Err(String::from(
                "--validate-only cannot be combined with --screenshot",
            ));
        }
        if let Some(path) = &command.screenshot {
            if path
                .extension()
                .is_none_or(|ext| !ext.eq_ignore_ascii_case("png"))
            {
                return Err(String::from("screenshot path must end in .png"));
            }
            if path.exists() {
                return Err(format!("screenshot already exists: {}", path.display()));
            }
        }
        Ok(Some(command))
    }

    fn apply_value(&mut self, flag: &str, value: &str) -> Result<(), String> {
        match flag {
            "--water-distortion" => self.water.distortion = number(value, flag)?,
            "--water-reflections" => self.water.reflections = number(value, flag)?,
            "--water-absorption" => self.water.absorption = number(value, flag)?,
            "--river-whitewater" => self.water.whitewater = number(value, flag)?,
            "--waterfall-mist" => self.water.mist = number(value, flag)?,
            "--water-foam" => self.water.foam_strength = number(value, flag)?,
            "--ocean-amplitude" => self.ocean.amplitude = number(value, flag)?,
            "--ocean-speed" => self.ocean.speed = number(value, flag)?,
            "--ocean-drift" => self.ocean.source_drift = number(value, flag)?,
            "--ocean-refraction" => self.ocean.refraction = number(value, flag)?,
            "--ocean-view" => {
                self.ocean.debug = match value {
                    "shaded" => 0,
                    "depth" => 1,
                    "distance" => 2,
                    "sources" => 3,
                    _ => return Err("ocean view must be shaded, depth, distance or sources".into()),
                }
            }
            "--shadow-bias" => self.lighting.shadow_bias = number(value, flag)?,
            "--ao-radius" => self.lighting.ao_radius = number(value, flag)?,
            "--ao-intensity" => self.lighting.ao_intensity = number(value, flag)?,
            "--ao-samples" => self.lighting.ao_samples = number(value, flag)?,
            "--shadow-distance" => self.lighting.shadow_distance = number(value, flag)?,
            "--seed" => self.options.seed = number(value, flag)?,
            "--chunk-level" => self.streaming.level = number(value, flag)?,
            "--chunk-budget-mb" => self.streaming.budget_mb = number(value, flag)?,
            "--lod-error" => self.streaming.target_pixels = number(value, flag)?,
            "--lod-surface-maps" => {
                self.streaming.surface_maps = match value {
                    "on" => true,
                    "off" => false,
                    _ => return Err(String::from("LOD surface maps must be on or off")),
                }
            }
            "--surface-view" => {
                self.streaming.surface_debug = match value {
                    "shaded" => 0,
                    "normals" => 1,
                    "occlusion" => 2,
                    _ => {
                        return Err(String::from(
                            "surface view must be shaded, normals, or occlusion",
                        ));
                    }
                }
            }
            "--lod" => {
                self.streaming.forced_lod = match value {
                    "auto" => None,
                    "0" => Some(0),
                    "1" => Some(1),
                    "2" => Some(2),
                    "3" => Some(3),
                    _ => return Err(String::from("LOD must be auto, 0, 1, 2, or 3")),
                }
            }
            "--vertices" => self.options.vertex_count = number(value, flag)?,
            "--radius" => self.options.radius = number(value, flag)?,
            "--relief" => self.terrain.relief = number(value, flag)?,
            "--land-bias" => self.terrain.land_bias = number(value, flag)?,
            "--continental-frequency" => self.terrain.continental_frequency = number(value, flag)?,
            "--detail-frequency" => self.terrain.detail_frequency = number(value, flag)?,
            "--erosion-strength" => self.terrain.erosion_strength = number(value, flag)?,
            "--erosion-passes" => self.terrain.hydraulic_passes = number(value, flag)?,
            "--thermal-passes" => self.terrain.thermal_passes = number(value, flag)?,
            "--river-catchment" => self.terrain.river_catchment = number(value, flag)?,
            "--river-depth" => self.terrain.river_depth = number(value, flag)?,
            "--refinements" => self.terrain.refinement_passes = number(value, flag)?,
            "--smoothings" => self.terrain.smoothing_passes = number(value, flag)?,
            "--river-refinements" => self.terrain.river_refinement_passes = number(value, flag)?,
            "--river-width" => self.terrain.river_width = number(value, flag)?,
            "--sweep-axis" => {
                self.options.sweep_direction = match value {
                    "x" => planet_rs::DVec3::X,
                    "y" => planet_rs::DVec3::Y,
                    "z" => planet_rs::DVec3::Z,
                    _ => return Err(String::from("sweep axis must be x, y, or z")),
                }
            }
            "--view" => self.view = View::parse(value)?,
            "--export" => self.export = Some(value.into()),
            "--screenshot" => self.screenshot = Some(value.into()),
            _ => return Err(format!("unknown option: {flag}")),
        }
        Ok(())
    }
}

fn number<T: FromStr>(value: &str, flag: &str) -> Result<T, String> {
    value
        .parse()
        .map_err(|_| format!("invalid numeric value for {flag}: {value}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn parse(args: &[&str]) -> Result<Option<Command>, String> {
        Command::parse(args.iter().map(|arg| String::from(*arg)))
    }
    #[test]
    fn invalid_and_conflicting_flags_fail() {
        for args in [
            vec!["--seed"],
            vec!["--vertices", "many"],
            vec!["--sweep-axis", "q"],
            vec!["--validate-only", "--screenshot", "test.png"],
            vec!["--unknown"],
            vec!["--screenshot", "test.jpg"],
            vec!["--relief", "NaN"],
            vec!["--erosion-passes", "129"],
            vec!["--river-catchment", "0"],
            vec!["--chunk-level", "4"],
            vec!["--lod", "4"],
            vec!["--lod-surface-maps", "maybe"],
            vec!["--surface-view", "invalid"],
        ] {
            assert!(parse(&args).is_err(), "{args:?}");
        }
    }
    #[test]
    fn configuration_and_help_parse() {
        let command = parse(&[
            "--seed",
            "123",
            "--vertices",
            "256",
            "--radius",
            "10",
            "--wireframe",
            "--sweep-axis",
            "z",
            "--relief",
            "0.1",
            "--land-bias",
            "-0.02",
            "--view",
            "back",
            "--no-ocean",
        ])
        .unwrap()
        .unwrap();
        assert_eq!(command.options.seed, 123);
        assert_eq!(command.options.vertex_count, 256);
        assert_eq!(command.options.radius.to_bits(), 10.0_f64.to_bits());
        assert_eq!(command.options.sweep_direction, planet_rs::DVec3::Z);
        assert!(command.wireframe && command.view == View::Back && command.no_ocean);
        assert!((command.terrain.land_bias + 0.02).abs() < f64::EPSILON);
        assert!(parse(&["--help"]).unwrap().is_none());
        assert_eq!(
            parse(&["--view", "mouth"]).unwrap().unwrap().view,
            View::Mouth
        );
    }

    #[test]
    fn streaming_controls_parse_without_changing_generation() {
        let command = parse(&[
            "--chunk-level",
            "3",
            "--chunk-colours",
            "--no-horizon",
            "--lod",
            "1",
            "--lod-surface-maps",
            "off",
            "--surface-view",
            "occlusion",
        ])
        .unwrap()
        .unwrap();
        assert_eq!(command.streaming.level, 3);
        assert!(command.streaming.colours && !command.streaming.horizon);
        assert_eq!(command.streaming.forced_lod, Some(1));
        assert!(!command.streaming.surface_maps);
        assert_eq!(command.streaming.surface_debug, 2);
        assert_eq!(command.terrain, TerrainOptions::default());
    }
    #[test]
    fn ocean_controls_keep_generation_fixed_and_reject_invalid_values() {
        let command = parse(&[
            "--view",
            "ocean",
            "--ocean-paused",
            "--ocean-amplitude",
            "1.5",
            "--ocean-speed",
            "0.5",
            "--ocean-drift",
            "0.75",
            "--ocean-view",
            "distance",
            "--no-ocean-waves",
        ])
        .unwrap()
        .unwrap();
        assert_eq!(command.view, View::Ocean);
        assert!(command.ocean.paused && !command.ocean.waves);
        assert_eq!(command.ocean.debug, 2);
        assert!((command.ocean.source_drift - 0.75).abs() < f32::EPSILON);
        assert_eq!(
            parse(&["--ocean-view", "sources"])
                .unwrap()
                .unwrap()
                .ocean
                .debug,
            3
        );
        assert_eq!(command.terrain, TerrainOptions::default());
        for args in [
            ["--ocean-amplitude", "NaN"],
            ["--ocean-speed", "-1"],
            ["--ocean-drift", "NaN"],
            ["--ocean-drift", "2.1"],
            ["--ocean-view", "bad"],
            ["--ocean-refraction", "2"],
        ] {
            assert!(parse(&args).is_err(), "{args:?}");
        }
    }
    #[test]
    fn water_effect_controls_validate_and_keep_hydrology_unchanged() {
        let command = parse(&[
            "--water-distortion",
            "0.5",
            "--water-reflections",
            "0.75",
            "--river-whitewater",
            "0.8",
            "--waterfall-mist",
            "1.2",
            "--water-absorption",
            "2",
            "--water-foam",
            "0.4",
            "--no-underwater",
            "--view",
            "underwater",
        ])
        .unwrap()
        .unwrap();
        assert!(!command.water.underwater && command.view == View::Underwater);
        assert_eq!(command.terrain, TerrainOptions::default());
        assert!((command.water.absorption - 2.0).abs() < f32::EPSILON);
        for args in [
            ["--water-distortion", "NaN"],
            ["--water-reflections", "2"],
            ["--river-whitewater", "-1"],
            ["--waterfall-mist", "3"],
            ["--water-absorption", "inf"],
            ["--water-foam", "5"],
        ] {
            assert!(parse(&args).is_err());
        }
    }
    #[test]
    fn lighting_controls_parse_and_reject_invalid_physical_parameters() {
        let command = parse(&[
            "--no-shadows",
            "--shadow-distance",
            "120",
            "--ao-radius",
            "1.5",
            "--ao-intensity",
            "0.6",
            "--ao-samples",
            "10",
            "--ao-half",
            "--ao-only",
        ])
        .unwrap()
        .unwrap();
        assert!(
            !command.lighting.shadows
                && command.lighting.half_resolution
                && command.lighting.ao_only
        );
        assert!((command.lighting.ao_radius - 1.5).abs() < f32::EPSILON);
        assert_eq!(command.lighting.ao_samples, 10);
        for flags in [
            ["--ao-radius", "NaN"],
            ["--ao-radius", "0"],
            ["--ao-intensity", "-1"],
            ["--ao-samples", "11"],
            ["--shadow-distance", "0"],
        ] {
            assert!(parse(&flags).is_err());
        }
    }
}
