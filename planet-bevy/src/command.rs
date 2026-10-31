use planet_rs::{NoiseLayer, PlanetOptions, TerrainOptions};
use std::{path::PathBuf, str::FromStr};

pub const HELP: &str = "Planet experiment — radial terrain, erosion, oceans and rivers

cargo run -p planet-bevy -- [options]
  --no-settings             Start from defaults without reading/writing preferences
  --settings-file PATH      Use a separate preferences file
  --seed N                  Deterministic seed (42)
  --vertices N              Surface points, 4..8192 (4096)
  --radius R                Sea-level radius in metres (1000 = 1 km)
  --sweep-axis x|y|z         Sphere construction plane normal (y)
  --relief F                Terrain amplitude / sea radius (0.16)
  --land-bias F             Signed elevation offset / radius (0)
  --terrain-source TYPE     noise or tectonics (noise)
  --tectonic-plates N       Large spherical Voronoi plates, 4..64 (16)
  --tectonic-steps N        Drift integration steps, 1..128 (48)
  --tectonic-drift F        Total angular drift, 0..1 radians (0.25)
  --tectonic-continents F   Continental plate fraction, 0..1 (0.375)
  --tectonic-fragmentation F  Irregular continental fragments, 0..1 (0.75)
  --tectonic-fragment-scale F  Continental fragment size, 0.35..2 (1)
  --tectonic-rifts F        Branching rift basin strength, 0..2 (0.65)
  --tectonic-rift-branches N  Rift branching levels, 0..4 (2)
  --tectonic-width F        Mountain belt width scale, 0.05..1 (0.35)
  --tectonic-deformation F  Compression/rift strength, 0..3 (1)
  --tectonic-branching F    Descending side-ridge strength, 0..1 (0.8)
  --tectonic-branch-depth N Recursive ridge levels, 0..4 (3)
  --tectonic-ruggedness F   Mountain peaks, saddles and bends, 0..1 (0.8)
  --tectonic-volcanism F    Sliding-boundary volcanic cone strength, 0..3 (1)
  --volcano-spacing F       Volcanic chain spacing, 0.5..3 (1)
  --impacts                 Add seeded crater batches before terrain stages
  --impact-count N          Craters in each batch, 0..24 (4)
  --impact-size F           Crater size multiplier, 0.25..3 (1)
  --impact-strength F       Bowl and raised rim strength, 0..3 (1)
  --tectonic-detail F       Extra noise contribution, 0..0.25 (0.08)
  --noise-preset classic|plates  Initial terrain blend (classic)
  --noise-N-type TYPE       Layer N=1..3: fractal, ridged or voronoi
  --noise-N-frequency F     Layer N frequency, 0.01..100 (2.2, 12, 3)
  --noise-N-weight F        Relative height weight, 0..2 (0.78, 0.22, 0)
  --continental-frequency F Alias for --noise-1-frequency
  --detail-frequency F      Alias for --noise-2-frequency
  --erosion-passes N        Cumulative hydraulic sweeps per stage, 0..128 (1)
  --erosion-strength F      Hydraulic stage strength, 0..8 (1)
  --thermal-passes N        Thermal passes, 0..128 (8)
  --river-catchment F       Rainfall area / whole sphere area (0.0015)
  --river-depth F           Channel depth / radius (0.0015)
  --refinements N           Broad/adaptive tessellation stages, 0..6 (5)
  --smoothings N            Smoothing passes per stage, 0..12 (3)
  --river-refinements N     Local channel tessellation stages, 0..3 (3)
  --river-width F           Base angular channel half-width (0.003)
  --river-taper F           Upstream narrowing, 0 legacy to 1 full taper (1)
  --no-rivers               Hide river meshes/textures to inspect carved beds
  --uncarved                Inspect terrain before the final channel carve
  --sphere-only             Original undeformed triangulation
  --keep-inland-seas        Disable isolated-sea filling (alias --separate-seas)
  --no-ocean                Start with sea hidden to inspect the seabed
  --hide-controls           Start with the parameter panel hidden (F1 toggles)
  --wireframe               Start with terrain triangle edges visible
  --flat | --smooth         Choose terrain shading
  --chunk-level N           Icosahedron split depth, 0..3 (2 = 320 cells)
  --chunk-colours           Colour chunks to inspect boundaries
  --no-horizon              Disable planet horizon culling for comparison
  --lod auto|0|1|2|3        Automatic or forced chunk detail (auto; 0 finest)
  --chunk-budget-mb N       Fine-mesh memory budget, 1..512 MiB (192)
  --lod-error F             Screen-space error target, 0.25..16 pixels (2)
  --lod-surface-maps on|off Baked normal and occlusion maps for LOD1–3 (on)
  --surface-view shaded|normals|occlusion  Inspect the baked surface maps
  --no-shadows              Disable sun shadow maps
  --no-forests              Disable procedural forests
  --forest-distance M       Detailed tree range, 0..400 m (240; 0 = all coarse)
  --shadow-distance M       Surface shadow range, 20..3000 m (300)
  --shadow-bias M           Shadow depth bias in metres, 0..0.25 (0.02)
  --no-ssao                 Disable final-depth ambient occlusion
  --ao-radius M             AO radius in metres, 0.01..20 (0.05)
  --ao-intensity F          AO strength, 0..2.5 (0.3)
  --ao-samples N            Hemisphere samples: 6, 10 or 12 (12)
  --ao-half                 Half-resolution AO with bilateral upsampling
  --ao-only                 Show the screen-space AO visibility
  --ocean-amplitude F       Ocean wave height multiplier, 0..3 (2)
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
  --no-solar               Use manual Sun direction instead of orbit/spin
  --solar-paused           Pause solar motion and weather
  --solar-day H            Sidereal spin period in hours (24)
  --solar-year D           Orbital period in days (365.256)
  --solar-tilt D           Axial tilt in degrees (23.44)
  --solar-eccentricity F   Orbital eccentricity (0)
  --solar-orbit R          Orbit semimajor axis / planet radius (23481)
  --solar-radius R         Sun radius / planet radius (109.2)
  --no-weather             Use the previous procedural weather and manual waves
  --weather-paused         Freeze simulated weather and wave animation
  --weather-hours H        Start with 0..240 simulated weather hours (0)
  --weather-speed F        Weather simulation speed, 0..20 (1)
  --weather-grid N         Weather cells per cube face edge (64)
  --weather-humidity F     Climate moisture multiplier (1)
  --weather-rotation S     Climate rotation period in seconds (86400)
  --weather-view MODE      shaded|pressure|wind|humidity|clouds|rain|waves|temperature
  --no-atmosphere           Disable atmospheric scattering
  --no-clouds               Disable volumetric clouds and cloud shadows
  --atmosphere-height M     Minimum atmosphere thickness, 20..2000 m (200)
  --atmosphere-haze F       Atmospheric optical density, 0..5 (1)
  --cloud-coverage F        Cloud cover, 0..1 (0.42)
  --cloud-density F         Cloud optical density, 0..8 (1.2)
  --cloud-base M            Minimum cloud base, 1..1500 m (45)
  --clouds-through-terrain  Allow clouds around mountain tops
  --cloud-thickness M       Cloud layer thickness, 5..1000 m (55)
  --cloud-scale M           Formation size, 10..2000 m (90)
  --cloud-wind F            Wind speed, 0..40 m/s (1.5)
  --cloud-shadow F          Surface cloud shadow strength, 0..1 (0.6)
  --cloud-quality low|balanced|high   Cloud quality (balanced)
  --no-cloud-temporal       Disable temporal cloud reconstruction
  --environment-paused      Freeze cloud weather for inspection
  --sun-azimuth D           Global sun azimuth, -180..180 degrees (-31)
  --sun-elevation D         Global sun elevation, -90..90 degrees (34.45)
  --view front|back|mountains|volcanoes|craters|river|waterfall|mouth|grass|ocean|shore|underwater|sky|clouds|forest   Inspection camera (front)
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

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum GenerationMode {
    Sphere,
    #[default]
    Terrain,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum View {
    #[default]
    Front,
    Back,
    Mountains,
    Volcanoes,
    Craters,
    River,
    Waterfall,
    Mouth,
    Grass,
    Ocean,
    Shore,
    Underwater,
    Sky,
    Clouds,
    Forest,
}

impl View {
    fn parse(value: &str) -> Result<Self, String> {
        Ok(match value {
            "front" => Self::Front,
            "back" => Self::Back,
            "mountains" => Self::Mountains,
            "volcanoes" => Self::Volcanoes,
            "craters" => Self::Craters,
            "river" => Self::River,
            "waterfall" => Self::Waterfall,
            "mouth" => Self::Mouth,
            "grass" => Self::Grass,
            "ocean" => Self::Ocean,
            "shore" => Self::Shore,
            "underwater" => Self::Underwater,
            "sky" => Self::Sky,
            "clouds" => Self::Clouds,
            "forest" => Self::Forest,
            _ => {
                return Err(String::from(
                    "view must be front, back, mountains, volcanoes, craters, river, waterfall, mouth, grass, ocean, shore, underwater, sky, clouds or forest",
                ));
            }
        })
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum BedView {
    #[default]
    Carved,
    Original,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
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
    pub smooth: Option<bool>,
    pub details: crate::details::DetailSettings,
    pub panel: PanelVisibility,
    pub export: Option<PathBuf>,
    pub screenshot: Option<PathBuf>,
    pub validate_only: bool,
    pub streaming: crate::streaming::StreamingSettings,
    pub lighting: crate::lighting::LightingSettings,
    pub ocean: crate::ocean::OceanSettings,
    pub water: crate::water::WaterSettings,
    pub environment: crate::environment::EnvironmentSettings,
    pub weather: crate::weather::WeatherControls,
    pub solar: crate::solar::SolarControls,
    pub forest: crate::forest::ForestSettings,
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
            smooth: None,
            details: crate::details::DetailSettings::default(),
            panel: PanelVisibility::Show,
            export: None,
            screenshot: None,
            validate_only: false,
            streaming: crate::streaming::StreamingSettings::default(),
            lighting: crate::lighting::LightingSettings::default(),
            ocean: crate::ocean::OceanSettings::default(),
            water: crate::water::WaterSettings::default(),
            environment: crate::environment::EnvironmentSettings::default(),
            weather: crate::weather::WeatherControls::default(),
            solar: crate::solar::SolarControls::default(),
            forest: crate::forest::ForestSettings::default(),
        }
    }
}

impl Command {
    pub fn parse(args: impl IntoIterator<Item = impl AsRef<str>>) -> Result<Option<Self>, String> {
        Self::parse_from(Self::default(), args)
    }
    pub(super) fn parse_from(
        mut command: Self,
        args: impl IntoIterator<Item = impl AsRef<str>>,
    ) -> Result<Option<Self>, String> {
        let mut args = args.into_iter();
        let mut solar_edited = false;
        while let Some(flag) = args.next() {
            let flag = flag.as_ref();
            solar_edited |= flag.starts_with("--solar-") && flag != "--solar-paused";
            match flag {
                "-h" | "--help" => return Ok(None),
                "--ocean-paused" => command.ocean.paused = true,
                "--no-atmosphere" => command.environment.atmosphere = false,
                "--no-solar" => command.solar.enabled = false,
                "--solar-paused" => command.solar.paused = true,
                "--no-weather" => command.weather.enabled = false,
                "--weather-paused" => command.weather.paused = true,
                "--no-clouds" => command.environment.clouds = false,
                "--no-cloud-temporal" => command.environment.temporal = false,
                "--clouds-through-terrain" => command.environment.clear_terrain = false,
                "--environment-paused" => command.environment.paused = true,
                "--no-ocean-waves" => command.ocean.waves = false,
                "--no-underwater" => command.water.underwater = false,
                "--no-shadows" => command.lighting.shadows = false,
                "--no-forests" => command.forest.enabled = false,
                "--no-ssao" => command.lighting.ao = false,
                "--ao-only" => command.lighting.ao_only = true,
                "--ao-half" => command.lighting.half_resolution = true,
                "--flat" => command.smooth = Some(false),
                "--smooth" => command.smooth = Some(true),
                "--wireframe" => command.wireframe = true,
                "--chunk-colours" => command.streaming.colours = true,
                "--no-horizon" => command.streaming.horizon = false,
                "--hide-controls" => command.panel = PanelVisibility::Hidden,
                "--validate-only" => command.validate_only = true,
                "--impacts" => command.terrain.impacts.enabled = true,
                "--no-impacts" => command.terrain.impacts.enabled = false,
                "--sphere-only" => {
                    command.generation = GenerationMode::Sphere;
                    command.smooth = None;
                }
                "--separate-seas" | "--keep-inland-seas" => command.terrain.connect_seas = false,
                "--no-ocean" => command.no_ocean = true,
                "--no-rivers" => command.rivers = RiverDisplay::Hide,
                "--uncarved" => command.bed = BedView::Original,
                _ => {
                    let value = args.next().ok_or_else(|| {
                        format!("{flag} requires a value or is not a supported option")
                    })?;
                    command.apply_value(flag, value.as_ref())?;
                }
            }
        }
        if solar_edited {
            command.solar.draft = command.solar.model;
        }
        command.validate_settings()?;
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

    pub(super) fn validate_settings(&self) -> Result<(), String> {
        crate::panel::Settings::from(self).validate()?;
        self.lighting.validate()?;
        if !self.forest.distance.is_finite() || !(0.0..=400.0).contains(&self.forest.distance) {
            return Err("detailed tree distance must be 0..400 m".into());
        }
        self.ocean.validate()?;
        self.water.validate()?;
        self.environment.validate()?;
        self.weather.model.validate()?;
        self.solar.model.validate()?;
        if !self.weather.warmup_hours.is_finite()
            || !(0.0..=240.0).contains(&self.weather.warmup_hours)
        {
            return Err("initial weather hours must be 0..240".into());
        }
        if !self.weather.speed.is_finite() || !(0.0..=20.0).contains(&self.weather.speed) {
            return Err("weather speed must be 0..20".into());
        }
        self.terrain.validate().map_err(|error| error.to_string())?;
        if self.streaming.level > 3 {
            return Err(String::from("chunk level must be 0..3"));
        }
        if !(1..=512).contains(&self.streaming.budget_mb)
            || !self.streaming.target_pixels.is_finite()
            || !(0.25..=16.0).contains(&self.streaming.target_pixels)
        {
            return Err(String::from(
                "chunk budget must be 1..512 MiB and LOD error 0.25..16 pixels",
            ));
        }
        if self.streaming.forced_lod.is_some_and(|lod| lod > 3)
            || self.streaming.surface_debug > 2
            || self.weather.debug > 7
        {
            return Err("invalid saved diagnostic or LOD selection".into());
        }
        for (name, value, lo, hi) in [
            ("grass height", self.details.height, 0.02, 0.5),
            ("grass density", self.details.density, 2.0, 32.0),
            ("rock density", self.details.rock_density, 0.2, 2.0),
            ("ocean choppiness", self.ocean.choppiness, 0.0, 1.0),
            ("ocean foam", self.ocean.foam, 0.0, 2.0),
            ("ocean roughness", self.ocean.roughness, 0.04, 0.8),
            ("fine ocean radius", self.ocean.fine_radius, 16.0, 256.0),
            ("ocean spacing", self.ocean.spacing, 0.5, 4.0),
        ] {
            if !value.is_finite() || !(lo..=hi).contains(&value) {
                return Err(format!("{name} must be {lo}..{hi}"));
            }
        }
        Ok(())
    }

    fn apply_environment_value(&mut self, flag: &str, value: &str) -> Result<bool, String> {
        match flag {
            "--solar-day" => self.solar.model.rotation_period=number::<f64>(value,flag)?*3600.0,
            "--solar-year" => self.solar.model.orbital_period=number::<f64>(value,flag)?*86400.0,
            "--solar-tilt" => self.solar.model.axial_tilt=number::<f64>(value,flag)?.to_radians(),
            "--solar-eccentricity" => self.solar.model.eccentricity=number(value,flag)?,
            "--solar-orbit" => self.solar.model.orbit_radius_ratio=number(value,flag)?,
            "--solar-radius" => self.solar.model.sun_radius_ratio=number(value,flag)?,
            "--weather-hours" => self.weather.warmup_hours = number(value, flag)?,
            "--weather-speed" => self.weather.speed = number(value, flag)?,
            "--weather-grid" => self.weather.model.face_size = number(value, flag)?,
            "--weather-humidity" => self.weather.model.humidity = number(value, flag)?,
            "--weather-rotation" => self.weather.model.rotation_period = number(value, flag)?,
            "--weather-view" => self.weather.debug = match value { "shaded"=>0,"pressure"=>1,"wind"=>2,"humidity"=>3,"clouds"=>4,"rain"=>5,"waves"=>6,"temperature"=>7,_=>return Err("weather view must be shaded, pressure, wind, humidity, clouds, rain, waves or temperature".into()) },
            "--atmosphere-height" => self.environment.atmosphere_height = number(value, flag)?,
            "--atmosphere-haze" => self.environment.haze = number(value, flag)?,
            "--cloud-coverage" => self.environment.coverage = number(value, flag)?,
            "--cloud-density" => self.environment.density = number(value, flag)?,
            "--cloud-base" => self.environment.cloud_base = number(value, flag)?,
            "--cloud-thickness" => self.environment.cloud_thickness = number(value, flag)?,
            "--cloud-scale" => self.environment.cloud_scale = number(value, flag)?,
            "--cloud-wind" => self.environment.wind_speed = number(value, flag)?,
            "--cloud-shadow" => self.environment.shadow_strength = number(value, flag)?,
            "--sun-azimuth" => self.environment.sun_azimuth = number(value, flag)?,
            "--sun-elevation" => self.environment.sun_elevation = number(value, flag)?,
            "--cloud-quality" => {
                self.environment.quality = match value {
                    "low" => 0,
                    "balanced" => 1,
                    "high" => 2,
                    _ => return Err("cloud quality must be low, balanced or high".into()),
                }
            }
            _ => return Ok(false),
        }
        Ok(true)
    }

    fn apply_noise_value(&mut self, flag: &str, value: &str) -> Result<bool, String> {
        if flag == "--noise-preset" {
            self.terrain.noise_layers = match value {
                "classic" => NoiseLayer::classic(),
                "plates" => NoiseLayer::plates(),
                _ => return Err("noise preset must be classic or plates".into()),
            };
            return Ok(true);
        }
        if let Some(layer_flag) = flag.strip_prefix("--noise-") {
            let (index, parameter) = layer_flag
                .split_once('-')
                .ok_or("noise flag must specify a layer 1..3 and type, frequency or weight")?;
            let index: usize = number(index, flag)?;
            let layer = index
                .checked_sub(1)
                .and_then(|i| self.terrain.noise_layers.get_mut(i))
                .ok_or("noise layer must be 1, 2 or 3")?;
            match parameter {
                "type" => layer.kind = value.parse().map_err(str::to_owned)?,
                "frequency" => layer.frequency = number(value, flag)?,
                "weight" => layer.weight = number(value, flag)?,
                _ => return Err("noise layer parameter must be type, frequency or weight".into()),
            }
            return Ok(true);
        }
        Ok(false)
    }

    fn apply_tectonic_value(&mut self, flag: &str, value: &str) -> Result<bool, String> {
        match flag {
            "--terrain-source" => self.terrain.source = value.parse()?,
            "--tectonic-plates" => self.terrain.tectonics.plates = number(value, flag)?,
            "--tectonic-steps" => self.terrain.tectonics.steps = number(value, flag)?,
            "--tectonic-drift" => self.terrain.tectonics.drift = number(value, flag)?,
            "--tectonic-continents" => {
                self.terrain.tectonics.continental_fraction = number(value, flag)?;
            }
            "--tectonic-fragmentation" => {
                self.terrain.tectonics.fragmentation = number(value, flag)?;
            }
            "--tectonic-fragment-scale" => {
                self.terrain.tectonics.fragment_scale = number(value, flag)?;
            }
            "--tectonic-rifts" => self.terrain.tectonics.rift_strength = number(value, flag)?,
            "--tectonic-rift-branches" => {
                self.terrain.tectonics.rift_branching = number(value, flag)?;
            }
            "--tectonic-width" => self.terrain.tectonics.boundary_width = number(value, flag)?,
            "--tectonic-deformation" => self.terrain.tectonics.deformation = number(value, flag)?,
            "--tectonic-branching" => self.terrain.tectonics.branching = number(value, flag)?,
            "--tectonic-branch-depth" => self.terrain.tectonics.branch_depth = number(value, flag)?,
            "--tectonic-ruggedness" => self.terrain.tectonics.ruggedness = number(value, flag)?,
            "--tectonic-volcanism" => self.terrain.tectonics.volcanism = number(value, flag)?,
            "--volcano-spacing" => self.terrain.tectonics.volcano_spacing = number(value, flag)?,
            "--impact-count" => self.terrain.impacts.per_stage = number(value, flag)?,
            "--impact-size" => self.terrain.impacts.size = number(value, flag)?,
            "--impact-strength" => self.terrain.impacts.strength = number(value, flag)?,
            "--tectonic-detail" => self.terrain.tectonics.detail = number(value, flag)?,
            _ => return Ok(false),
        }
        Ok(true)
    }

    fn apply_value(&mut self, flag: &str, value: &str) -> Result<(), String> {
        if self.apply_environment_value(flag, value)? {
            return Ok(());
        }
        if self.apply_noise_value(flag, value)? {
            return Ok(());
        }
        if self.apply_tectonic_value(flag, value)? {
            return Ok(());
        }
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
            "--forest-distance" => self.forest.distance = number(value, flag)?,
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
            "--continental-frequency" => {
                self.terrain.noise_layers[0].frequency = number(value, flag)?;
            }
            "--detail-frequency" => self.terrain.noise_layers[1].frequency = number(value, flag)?,
            "--erosion-strength" => self.terrain.erosion_strength = number(value, flag)?,
            "--erosion-passes" => self.terrain.hydraulic_passes = number(value, flag)?,
            "--thermal-passes" => self.terrain.thermal_passes = number(value, flag)?,
            "--river-catchment" => self.terrain.river_catchment = number(value, flag)?,
            "--river-depth" => self.terrain.river_depth = number(value, flag)?,
            "--refinements" => self.terrain.refinement_passes = number(value, flag)?,
            "--smoothings" => self.terrain.smoothing_passes = number(value, flag)?,
            "--river-refinements" => self.terrain.river_refinement_passes = number(value, flag)?,
            "--river-width" => self.terrain.river_width = number(value, flag)?,
            "--river-taper" => self.terrain.river_taper = number(value, flag)?,
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
    #[test]
    fn upstream_narrowing_parses_and_rejects_invalid_strengths() {
        for (value, expected) in [("0", 0.0), ("0.6", 0.6), ("1", 1.0)] {
            assert_eq!(
                Command::parse(["--river-taper", value])
                    .unwrap()
                    .unwrap()
                    .terrain
                    .river_taper
                    .to_bits(),
                f64::to_bits(expected)
            );
        }
        for value in ["-0.01", "1.01", "NaN", "inf"] {
            assert!(Command::parse(["--river-taper", value]).is_err());
        }
    }
    #[test]
    fn inland_sea_filling_is_enabled_by_default_and_both_opt_out_names_work() {
        assert!(Command::default().terrain.connect_seas);
        for flag in ["--keep-inland-seas", "--separate-seas"] {
            let command = Command::parse([flag]).unwrap().unwrap();
            assert!(!command.terrain.connect_seas);
        }
    }

    #[test]
    fn geological_features_parse_and_reject_invalid_settings() {
        let parsed = Command::parse(
            [
                "--impacts",
                "--impact-count",
                "7",
                "--impact-size",
                "1.5",
                "--impact-strength",
                "2",
                "--tectonic-volcanism",
                "0.5",
                "--volcano-spacing",
                "2",
                "--view",
                "craters",
            ]
            .map(str::to_owned),
        )
        .unwrap()
        .unwrap();
        assert!(parsed.terrain.impacts.enabled);
        assert_eq!(parsed.terrain.impacts.per_stage, 7);
        assert!((parsed.terrain.impacts.size - 1.5).abs() < f64::EPSILON);
        assert!((parsed.terrain.impacts.strength - 2.0).abs() < f64::EPSILON);
        assert!((parsed.terrain.tectonics.volcanism - 0.5).abs() < f64::EPSILON);
        assert!((parsed.terrain.tectonics.volcano_spacing - 2.0).abs() < f64::EPSILON);
        assert_eq!(parsed.view, View::Craters);
        for args in [
            ["--impact-count", "25"],
            ["--impact-size", "NaN"],
            ["--impact-strength", "-1"],
            ["--tectonic-volcanism", "4"],
            ["--volcano-spacing", "0"],
        ] {
            assert!(Command::parse(args.map(str::to_owned)).is_err());
        }
        assert_eq!(View::parse("volcanoes").unwrap(), View::Volcanoes);
    }

    #[test]
    fn hydraulic_strength_accepts_eight_and_rejects_values_above_it() {
        let command = parse(&["--erosion-strength", "8"]).unwrap().unwrap();
        assert!((command.terrain.erosion_strength - 8.0).abs() < f64::EPSILON);
        for value in ["8.001", "-1", "NaN", "inf"] {
            assert!(parse(&["--erosion-strength", value]).is_err());
        }
    }

    #[test]
    fn fractal_ridge_controls_parse_and_reject_unbounded_branching() {
        let options = parse(&[
            "--tectonic-branching",
            "0.65",
            "--tectonic-branch-depth",
            "4",
        ])
        .unwrap()
        .unwrap()
        .terrain
        .tectonics;
        assert!((options.branching - 0.65).abs() < f64::EPSILON);
        assert_eq!(options.branch_depth, 4);
        for args in [
            ["--tectonic-branching", "NaN"],
            ["--tectonic-branching", "1.1"],
            ["--tectonic-branching", "-1"],
            ["--tectonic-branch-depth", "5"],
        ] {
            assert!(parse(&args).is_err());
        }
    }

    #[test]
    fn coastal_geology_controls_parse_and_validate_finite_bounds() {
        let options = parse(&[
            "--terrain-source",
            "tectonics",
            "--tectonic-fragmentation",
            "0.9",
            "--tectonic-fragment-scale",
            "1.6",
            "--tectonic-rifts",
            "1.25",
            "--tectonic-rift-branches",
            "4",
        ])
        .unwrap()
        .unwrap()
        .terrain
        .tectonics;
        assert!((options.fragmentation - 0.9).abs() < f64::EPSILON);
        assert!((options.fragment_scale - 1.6).abs() < f64::EPSILON);
        assert!((options.rift_strength - 1.25).abs() < f64::EPSILON);
        assert_eq!(options.rift_branching, 4);
        let disabled = parse(&[
            "--tectonic-fragmentation",
            "0",
            "--tectonic-rifts",
            "0",
            "--tectonic-rift-branches",
            "0",
        ])
        .unwrap()
        .unwrap();
        assert!(disabled.terrain.tectonics.fragmentation.abs() < f64::EPSILON);
        assert!(disabled.terrain.tectonics.rift_strength.abs() < f64::EPSILON);
        assert_eq!(disabled.terrain.tectonics.rift_branching, 0);
        for args in [
            ["--tectonic-fragmentation", "NaN"],
            ["--tectonic-fragmentation", "1.01"],
            ["--tectonic-fragmentation", "-0.01"],
            ["--tectonic-fragment-scale", "0.34"],
            ["--tectonic-fragment-scale", "2.01"],
            ["--tectonic-fragment-scale", "inf"],
            ["--tectonic-rifts", "-0.01"],
            ["--tectonic-rifts", "2.01"],
            ["--tectonic-rifts", "NaN"],
            ["--tectonic-rift-branches", "5"],
            ["--tectonic-rift-branches", "-1"],
        ] {
            assert!(parse(&args).is_err(), "accepted {args:?}");
        }
    }

    #[test]
    fn tectonic_source_and_controls_parse_and_reject_invalid_models() {
        let parsed = parse(&[
            "--terrain-source",
            "tectonics",
            "--tectonic-plates",
            "24",
            "--tectonic-steps",
            "64",
            "--tectonic-drift",
            "0.4",
            "--tectonic-continents",
            "0.3",
            "--tectonic-width",
            "0.2",
            "--tectonic-deformation",
            "1.5",
            "--tectonic-detail",
            "0",
            "--tectonic-ruggedness",
            "0.7",
            "--view",
            "mountains",
        ])
        .unwrap()
        .unwrap();
        assert_eq!(parsed.terrain.source, planet_rs::TerrainSource::Tectonics);
        assert_eq!(parsed.view, View::Mountains);
        assert_eq!(parsed.terrain.tectonics.plates, 24);
        assert_eq!(parsed.terrain.tectonics.steps, 64);
        assert_eq!(parsed.terrain.tectonics.drift.to_bits(), 0.4_f64.to_bits());
        assert_eq!(
            parsed.terrain.tectonics.continental_fraction.to_bits(),
            0.3_f64.to_bits()
        );
        assert_eq!(
            parsed.terrain.tectonics.boundary_width.to_bits(),
            0.2_f64.to_bits()
        );
        assert_eq!(
            parsed.terrain.tectonics.deformation.to_bits(),
            1.5_f64.to_bits()
        );
        assert!(parsed.terrain.tectonics.detail.abs() < f64::EPSILON);
        assert_eq!(
            parsed.terrain.tectonics.ruggedness.to_bits(),
            0.7_f64.to_bits()
        );
        for flags in [
            ["--terrain-source", "unknown"],
            ["--tectonic-plates", "3"],
            ["--tectonic-plates", "65"],
            ["--tectonic-steps", "0"],
            ["--tectonic-drift", "NaN"],
            ["--tectonic-continents", "1.1"],
            ["--tectonic-width", "0"],
            ["--tectonic-deformation", "4"],
            ["--tectonic-detail", "-1"],
            ["--tectonic-ruggedness", "1.1"],
        ] {
            assert!(parse(&flags).is_err(), "accepted {flags:?}");
        }
    }

    #[test]
    fn noise_layers_parse_independently_and_keep_legacy_frequency_aliases() {
        let parsed = parse(&[
            "--noise-preset",
            "plates",
            "--noise-1-frequency",
            "1.4",
            "--noise-2-type",
            "ridged",
            "--noise-2-weight",
            "0.3",
            "--noise-3-frequency",
            "4.5",
            "--noise-3-weight",
            "0.6",
        ])
        .unwrap()
        .unwrap();
        assert_eq!(
            parsed.terrain.noise_layers[0].frequency.to_bits(),
            1.4_f64.to_bits()
        );
        assert_eq!(
            parsed.terrain.noise_layers[1].kind,
            planet_rs::NoiseKind::Ridged
        );
        assert_eq!(
            parsed.terrain.noise_layers[1].weight.to_bits(),
            0.3_f64.to_bits()
        );
        assert_eq!(
            parsed.terrain.noise_layers[2].kind,
            planet_rs::NoiseKind::Voronoi
        );
        assert_eq!(
            parsed.terrain.noise_layers[2].frequency.to_bits(),
            4.5_f64.to_bits()
        );
        assert_eq!(
            parsed.terrain.noise_layers[2].weight.to_bits(),
            0.6_f64.to_bits()
        );
        let alias = parse(&["--continental-frequency", "3", "--detail-frequency", "20"])
            .unwrap()
            .unwrap();
        assert_eq!(
            alias.terrain.noise_layers[0].frequency.to_bits(),
            3.0_f64.to_bits()
        );
        assert_eq!(
            alias.terrain.noise_layers[1].frequency.to_bits(),
            20.0_f64.to_bits()
        );
        for flags in [
            ["--noise-0-weight", "1"],
            ["--noise-4-frequency", "1"],
            ["--noise-1-type", "bad"],
            ["--noise-1-octaves", "2"],
            ["--noise-2-frequency", "NaN"],
            ["--noise-3-frequency", "101"],
            ["--noise-3-weight", "-1"],
            ["--noise-3-weight", "inf"],
            ["--noise-2-weight", "3"],
            ["--noise-preset", "bad"],
        ] {
            assert!(parse(&flags).is_err(), "accepted {flags:?}");
        }
    }
    #[test]
    fn forest_distance_can_force_coarse_trees_and_rejects_invalid_ranges() {
        for distance in ["0", "80", "240", "400"] {
            let command = super::Command::parse(["--forest-distance", distance].map(str::to_owned))
                .unwrap()
                .unwrap();
            assert_eq!(
                command.forest.distance.to_bits(),
                distance.parse::<f32>().unwrap().to_bits()
            );
            assert!(command.forest.enabled);
        }
        for distance in ["-1", "401", "NaN", "inf"] {
            assert!(
                super::Command::parse(["--forest-distance", distance].map(str::to_owned)).is_err()
            );
        }
    }
    #[test]
    fn solar_controls_parse_and_reject_invalid_orbits() {
        let command = Command::parse(
            [
                "--solar-day",
                "12",
                "--solar-year",
                "20",
                "--solar-tilt",
                "30",
                "--solar-eccentricity",
                "0.2",
                "--solar-paused",
            ]
            .map(str::to_owned),
        )
        .unwrap()
        .unwrap();
        assert!(command.solar.enabled && command.solar.paused);
        assert!((command.solar.model.rotation_period - 43200.0).abs() < 1e-10);
        assert!((command.solar.model.orbital_period - 20.0 * 86400.0).abs() < 1e-10);
        assert_eq!(command.solar.model, command.solar.draft);
        for args in [
            ["--solar-day", "0"],
            ["--solar-tilt", "NaN"],
            ["--solar-eccentricity", "1"],
            ["--solar-orbit", "100"],
        ] {
            assert!(Command::parse(args.map(str::to_owned)).is_err());
        }
        assert!(
            !Command::parse(["--no-solar".to_owned()])
                .unwrap()
                .unwrap()
                .solar
                .enabled
        );
    }
    #[test]
    fn global_weather_controls_parse_and_bound_worker_work() {
        let command = Command::parse(
            [
                "--weather-paused",
                "--weather-speed",
                "2",
                "--weather-grid",
                "16",
                "--weather-view",
                "wind",
                "--weather-hours",
                "6",
            ]
            .map(str::to_string),
        )
        .unwrap()
        .unwrap();
        assert!(command.weather.paused && command.weather.enabled);
        assert_eq!(command.weather.model.face_size, 16);
        assert_eq!(command.weather.debug, 2);
        assert!((command.weather.warmup_hours - 6.0).abs() < f64::EPSILON);
        for arguments in [
            ["--weather-grid", "0"],
            ["--weather-speed", "21"],
            ["--weather-hours", "241"],
            ["--weather-humidity", "NaN"],
            ["--weather-view", "bad"],
        ] {
            assert!(Command::parse(arguments.map(str::to_string)).is_err());
        }
    }

    #[test]
    fn atmosphere_and_cloud_controls_parse_and_reject_invalid_values() {
        let parsed = super::Command::parse(
            [
                "--no-cloud-temporal",
                "--clouds-through-terrain",
                "--cloud-coverage",
                "0.6",
                "--cloud-base",
                "75",
                "--cloud-quality",
                "high",
                "--sun-elevation",
                "-12",
                "--environment-paused",
                "--view",
                "clouds",
            ]
            .map(String::from),
        )
        .unwrap()
        .unwrap();
        assert!(!parsed.environment.temporal && !parsed.environment.clear_terrain);
        assert!(parsed.environment.paused && parsed.environment.quality == 2);
        assert_eq!(parsed.view, super::View::Clouds);
        for args in [
            ["--cloud-coverage", "1.1"],
            ["--cloud-density", "NaN"],
            ["--cloud-quality", "extreme"],
            ["--cloud-thickness", "0"],
        ] {
            assert!(super::Command::parse(args.map(String::from)).is_err());
        }
    }
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
