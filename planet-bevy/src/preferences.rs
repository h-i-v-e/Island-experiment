//! User configuration only: geometry and simulation snapshots never enter the
//! settings file. A debounced worker owns the path while an atomic save runs.
use crate::{
    Scene,
    command::{BedView, Command, PanelVisibility, RiverDisplay, View},
    details::DetailSettings,
    environment::EnvironmentSettings,
    forest::ForestSettings,
    lighting::LightingSettings,
    ocean::OceanSettings,
    panel::{Panel, Settings},
    solar::SolarControls,
    streaming::StreamingSettings,
    water::WaterSettings,
    weather::WeatherControls,
};
use bevy::{
    ecs::system::SystemParam,
    prelude::*,
    tasks::{IoTaskPool, Task, block_on, poll_once},
};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    io::{self, BufReader, BufWriter, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};
const VERSION: u32 = 1;
const DEBOUNCE: Duration = Duration::from_millis(600);
static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
#[allow(clippy::struct_excessive_bools)] // Independent remembered display switches.
struct Display {
    bed: BedView,
    rivers: RiverDisplay,
    no_ocean: bool,
    wireframe: bool,
    smooth: bool,
    hidden: bool,
    view: View,
}
impl Default for Display {
    fn default() -> Self {
        Self {
            bed: BedView::Carved,
            rivers: RiverDisplay::Show,
            no_ocean: false,
            wireframe: false,
            smooth: true,
            hidden: false,
            view: View::Front,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
struct RenderSettings {
    streaming: StreamingSettings,
    details: DetailSettings,
    forest: ForestSettings,
    lighting: LightingSettings,
    ocean: OceanSettings,
    water: WaterSettings,
    environment: EnvironmentSettings,
    weather: WeatherControls,
    solar: SolarControls,
}
impl Default for RenderSettings {
    fn default() -> Self {
        Self::from_command(&Command::default())
    }
}
impl RenderSettings {
    fn from_command(c: &Command) -> Self {
        Self {
            streaming: c.streaming,
            details: c.details,
            forest: c.forest,
            lighting: c.lighting,
            ocean: c.ocean,
            water: c.water,
            environment: c.environment,
            weather: c.weather,
            solar: c.solar,
        }
        .canonical()
    }
    fn canonical(mut self) -> Self {
        self.weather.step = false;
        self.weather.reset = 0;
        self.solar.revision = 0;
        self
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
struct Snapshot {
    version: u32,
    /// Only a successfully installed planet becomes the startup world.
    applied: Settings,
    /// Keep valid, unapplied edits available in the controls after reopening.
    draft: Settings,
    display: Display,
    render: RenderSettings,
}
impl Default for Snapshot {
    fn default() -> Self {
        let c = Command::default();
        let settings = Settings::from(&c);
        Self {
            version: VERSION,
            applied: settings,
            draft: settings,
            display: Display::default(),
            render: RenderSettings::from_command(&c),
        }
    }
}
impl Snapshot {
    fn from_command(c: &Command, draft: &Settings) -> Self {
        Self {
            version: VERSION,
            applied: Settings::from(c),
            draft: *draft,
            display: Display {
                bed: c.bed,
                rivers: c.rivers,
                no_ocean: c.no_ocean,
                wireframe: c.wireframe,
                smooth: c
                    .smooth
                    .unwrap_or(c.generation == crate::command::GenerationMode::Terrain),
                hidden: c.panel == PanelVisibility::Hidden,
                view: c.view,
            },
            render: RenderSettings::from_command(c),
        }
    }
    fn command(&self, generation: &Settings) -> Command {
        Command {
            options: generation.planet,
            terrain: generation.terrain,
            generation: generation.mode,
            bed: self.display.bed,
            rivers: self.display.rivers,
            no_ocean: self.display.no_ocean,
            wireframe: self.display.wireframe,
            smooth: Some(self.display.smooth),
            view: self.display.view,
            panel: if self.display.hidden {
                PanelVisibility::Hidden
            } else {
                PanelVisibility::Show
            },
            streaming: self.render.streaming,
            details: self.render.details,
            forest: self.render.forest,
            lighting: self.render.lighting,
            ocean: self.render.ocean,
            water: self.render.water,
            environment: self.render.environment,
            weather: self.render.weather,
            solar: self.render.solar,
            ..Command::default()
        }
    }
    fn validate(&self) -> Result<(), String> {
        if self.version != VERSION {
            return Err(format!("unsupported settings version {}", self.version));
        }
        self.command(&self.applied).validate_settings()?;
        self.draft.validate()?;
        self.render.solar.draft.validate()?;
        Ok(())
    }
}
fn default_path() -> Option<PathBuf> {
    if cfg!(target_os = "macos") {
        std::env::var_os("HOME").map(|home| {
            PathBuf::from(home).join("Library/Application Support/Planet experiment/settings.json")
        })
    } else if cfg!(target_os = "windows") {
        std::env::var_os("APPDATA")
            .map(|root| PathBuf::from(root).join("Planet experiment/settings.json"))
    } else {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
            .map(|root| root.join("planet-experiment/settings.json"))
    }
}
fn read(path: &Path) -> Result<Option<Snapshot>, String> {
    let file = match File::open(path) {
        Ok(file) => file,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.to_string()),
    };
    if file.metadata().map_err(|e| e.to_string())?.len() > 256 * 1024 {
        return Err("settings file is too large".into());
    }
    let mut settings: Snapshot =
        serde_json::from_reader(BufReader::new(file)).map_err(|e| e.to_string())?;
    settings.render = settings.render.canonical();
    settings.validate()?;
    Ok(Some(settings))
}
fn write(path: &Path, settings: &Snapshot) -> io::Result<()> {
    settings.validate().map_err(io::Error::other)?;
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    fs::create_dir_all(parent)?;
    let temporary = path.with_extension(format!(
        "tmp-{}-{}",
        std::process::id(),
        NEXT_TEMP.fetch_add(1, Ordering::Relaxed)
    ));
    let result = (|| {
        let mut file = BufWriter::new(
            OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temporary)?,
        );
        serde_json::to_writer_pretty(&mut file, settings).map_err(io::Error::other)?;
        file.write_all(b"\n")?;
        file.flush()?;
        file.get_ref().sync_all()?;
        drop(file);
        fs::rename(&temporary, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

pub(super) struct Startup {
    pub command: Command,
    pub draft: Settings,
    pub store: Option<PreferencesStore>,
}
/// CLI arguments are retained for two borrowed parses: applied settings and
/// pending draft settings. This avoids copying argument strings for the merge.
pub(super) fn startup(args: impl IntoIterator<Item = String>) -> Result<Option<Startup>, String> {
    let mut args = args.into_iter();
    let mut flags = Vec::new();
    let mut path = None;
    let mut disabled = false;
    while let Some(flag) = args.next() {
        match flag.as_str() {
            "--no-settings" => disabled = true,
            "--settings-file" => {
                path = Some(PathBuf::from(
                    args.next().ok_or("--settings-file requires a path")?,
                ));
            }
            _ => flags.push(flag),
        }
    }
    if flags.iter().any(|s| matches!(s.as_str(), "-h" | "--help")) {
        return Ok(None);
    }
    let headless = flags
        .iter()
        .any(|s| matches!(s.as_str(), "--screenshot" | "--validate-only"));
    let load = !disabled && (!headless || path.is_some());
    let path = path.or_else(default_path);
    let (saved, invalid) = if load {
        match path.as_deref().map_or(Ok(None), read) {
            Ok(saved) => (saved, false),
            Err(error) => {
                eprintln!(
                    "Ignoring saved settings: {error}. Use --no-settings to start independently."
                );
                (None, true)
            }
        }
    } else {
        (None, false)
    };
    let parsed = if let Some(saved) = &saved {
        Command::parse_from(saved.command(&saved.applied), &flags)
    } else {
        Command::parse(&flags)
    };
    let Some(command) = parsed? else {
        return Ok(None);
    };
    let applied = Settings::from(&command);
    let draft = saved
        .as_ref()
        .and_then(|s| {
            Command::parse_from(s.command(&s.draft), &flags)
                .ok()
                .flatten()
        })
        .map_or(applied, |c| Settings::from(&c));
    let initial = Snapshot::from_command(&command, &draft);
    let baseline = saved.or_else(|| invalid.then_some(initial));
    let store = (!disabled && !headless)
        .then(|| path.map(|path| PreferencesStore::new(path, &initial, baseline.as_ref())))
        .flatten();
    Ok(Some(Startup {
        command,
        draft,
        store,
    }))
}

struct FinishedWrite {
    path: PathBuf,
    snapshot: Snapshot,
    result: Result<(), String>,
}
#[derive(Resource)]
pub(super) struct PreferencesStore {
    // Ownership moves to the single save task and returns on completion.
    path: Option<PathBuf>,
    armed: bool,
    current: Snapshot,
    saved: Option<Snapshot>,
    changed: Instant,
    retry: Option<Instant>,
    job: Option<Task<FinishedWrite>>,
    pub error: Option<String>,
}
impl PreferencesStore {
    fn new(path: PathBuf, current: &Snapshot, saved: Option<&Snapshot>) -> Self {
        Self {
            path: Some(path),
            armed: false,
            current: *current,
            saved: saved.copied(),
            changed: Instant::now(),
            retry: None,
            job: None,
            error: None,
        }
    }
    fn dirty(&self) -> bool {
        self.saved.as_ref() != Some(&self.current)
    }
    fn complete(&mut self, finished: FinishedWrite) {
        self.path = Some(finished.path);
        match finished.result {
            Ok(()) => {
                self.saved = Some(finished.snapshot);
                self.error = None;
                self.retry = None;
            }
            Err(error) => {
                warn!("Could not save settings: {error}");
                self.error = Some(error);
                self.retry = Some(Instant::now() + Duration::from_secs(5));
            }
        }
    }
    fn observe(&mut self, snapshot: &Snapshot) {
        self.armed = true;
        if self.current != *snapshot {
            self.current = *snapshot;
            self.changed = Instant::now();
            self.retry = None;
        }
        if let Some(job) = self.job.as_mut()
            && let Some(result) = block_on(poll_once(job))
        {
            self.job = None;
            self.complete(result);
        }
        if !self.dirty()
            || self.job.is_some()
            || self.changed.elapsed() < DEBOUNCE
            || self.retry.is_some_and(|t| Instant::now() < t)
        {
            return;
        }
        let Some(path) = self.path.take() else {
            return;
        };
        let snapshot = self.current;
        self.job = Some(IoTaskPool::get().spawn(async move {
            let result = write(&path, &snapshot).map_err(|e| e.to_string());
            FinishedWrite {
                path,
                snapshot,
                result,
            }
        }));
    }
}
impl Drop for PreferencesStore {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
        if let Some(job) = self.job.take() {
            self.complete(block_on(job));
        }
        // The final debounced edit is flushed during shutdown, after rendering.
        if self.dirty()
            && let Some(path) = self.path.as_deref()
            && let Err(error) = write(path, &self.current)
        {
            warn!("Could not save settings on exit: {error}");
        }
    }
}
#[derive(SystemParam)]
pub(super) struct LiveSettings<'w> {
    panel: Res<'w, Panel>,
    scene: Res<'w, Scene>,
    streaming: Res<'w, StreamingSettings>,
    details: Res<'w, DetailSettings>,
    forest: Res<'w, ForestSettings>,
    lighting: Res<'w, LightingSettings>,
    ocean: Res<'w, OceanSettings>,
    water: Res<'w, WaterSettings>,
    environment: Res<'w, EnvironmentSettings>,
    weather: Res<'w, WeatherControls>,
    solar: Res<'w, SolarControls>,
}
impl LiveSettings<'_> {
    fn snapshot(&self, previous: &Snapshot) -> Snapshot {
        let (applied, draft, hidden, view) = self.panel.remembered_generation();
        Snapshot {
            version: VERSION,
            applied,
            draft: draft.unwrap_or(previous.draft),
            display: Display {
                bed: self.scene.bed,
                rivers: self.scene.river_display,
                no_ocean: self.scene.no_ocean,
                wireframe: self.scene.wireframe,
                smooth: self.scene.smooth,
                hidden,
                view,
            },
            render: RenderSettings {
                streaming: *self.streaming,
                details: *self.details,
                forest: *self.forest,
                lighting: *self.lighting,
                ocean: *self.ocean,
                water: *self.water,
                environment: *self.environment,
                weather: *self.weather,
                solar: *self.solar,
            }
            .canonical(),
        }
    }
}
pub(super) fn remember(mut store: Option<ResMut<PreferencesStore>>, settings: LiveSettings) {
    if let Some(store) = store.as_mut() {
        let snapshot = settings.snapshot(&store.current);
        store.observe(&snapshot);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Folder(PathBuf);
    impl Folder {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "planet-prefs-{}-{}",
                std::process::id(),
                NEXT_TEMP.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }
        fn file(&self) -> PathBuf {
            self.0.join("settings.json")
        }
        fn args(&self) -> [String; 2] {
            [
                "--settings-file".into(),
                self.file().to_string_lossy().into_owned(),
            ]
        }
    }
    impl Drop for Folder {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    fn edited() -> Snapshot {
        let mut s = Snapshot::default();
        s.applied.planet.seed = 73;
        s.applied.planet.vertex_count = 512;
        s.applied.terrain.source = planet_rs::TerrainSource::Tectonics;
        s.applied.terrain.erosion_strength = 8.0;
        s.applied.terrain.refinement_passes = 3;
        s.applied.terrain.noise_layers[2].weight = 0.333_333_333_333_333_3;
        s.applied.terrain.tectonics.fragmentation = 0.9;
        s.applied.terrain.river_taper = 0.45;
        s.applied.terrain.tectonics.fragment_scale = 0.8;
        s.applied.terrain.tectonics.rift_strength = 1.2;
        s.applied.terrain.tectonics.rift_branching = 3;
        s.draft = s.applied;
        s.draft.planet.seed = 74;
        s.draft.terrain.tectonics.branch_depth = 4;
        s.draft.terrain.river_taper = 0.9;
        s.draft.terrain.tectonics.fragmentation = 0.4;
        s.draft.terrain.tectonics.fragment_scale = 1.6;
        s.draft.terrain.tectonics.rift_strength = 0.0;
        s.draft.terrain.tectonics.rift_branching = 0;
        s.display.wireframe = true;
        s.display.hidden = true;
        s.display.view = View::Mountains;
        s.render.details.density = 17.0;
        s.render.details.grass = false;
        s.render.forest.distance = 190.0;
        s.render.streaming.forced_lod = Some(2);
        s.render.lighting.ao_radius = 0.12;
        s.render.ocean.foam = 2.0;
        s.render.water.mist = 1.4;
        s.render.environment.coverage = 0.61;
        s.render.weather.model.humidity = 0.7;
        s.render.solar.model.axial_tilt = 0.37;
        s.render.solar.draft = s.render.solar.model;
        s
    }
    #[test]
    fn file_roundtrip_preserves_every_group_and_pending_edits_without_float_drift() {
        let folder = Folder::new();
        let expected = edited();
        write(&folder.file(), &expected).unwrap();
        assert_eq!(read(&folder.file()).unwrap(), Some(expected));
        let loaded = startup(folder.args()).unwrap().unwrap();
        assert_eq!(Settings::from(&loaded.command), expected.applied);
        assert_eq!(loaded.draft, expected.draft);
        assert_eq!(loaded.command.details, expected.render.details);
        assert_eq!(loaded.command.streaming, expected.render.streaming);
        assert_eq!(loaded.command.solar.draft, expected.render.solar.draft);
        assert_eq!(loaded.command.view, View::Mountains);
        drop(loaded);
        assert_eq!(read(&folder.file()).unwrap(), Some(expected));
    }
    #[test]
    fn legacy_preferences_default_new_coast_controls_without_losing_existing_edits() {
        let folder = Folder::new();
        // A version-1 file written before the four coastal controls existed.
        let legacy = r#"{
            "version":1,
            "applied":{
                "planet":{"seed":73,"vertex_count":512},
                "terrain":{"source":"Tectonics","tectonics":{"plates":24,"drift":0.4}}
            },
            "draft":{
                "planet":{"seed":74,"vertex_count":512},
                "terrain":{"source":"Tectonics","tectonics":{"plates":32,"drift":0.7}}
            }
        }"#;
        fs::write(folder.file(), legacy).unwrap();
        let loaded = startup(folder.args()).unwrap().unwrap();
        let mut expected = Settings::default();
        expected.planet.seed = 73;
        expected.planet.vertex_count = 512;
        expected.terrain.source = planet_rs::TerrainSource::Tectonics;
        expected.terrain.tectonics.plates = 24;
        expected.terrain.tectonics.drift = 0.4;
        assert_eq!(Settings::from(&loaded.command), expected);
        expected.planet.seed = 74;
        expected.terrain.tectonics.plates = 32;
        expected.terrain.tectonics.drift = 0.7;
        assert_eq!(loaded.draft, expected);
        drop(loaded);
        assert_eq!(fs::read_to_string(folder.file()).unwrap(), legacy);
    }
    #[test]
    fn cli_overrides_both_applied_and_pending_values_and_tools_never_write() {
        let folder = Folder::new();
        let original = edited();
        write(&folder.file(), &original).unwrap();
        let mut args = folder.args().to_vec();
        args.extend(
            [
                "--seed",
                "99",
                "--erosion-strength",
                "6",
                "--solar-tilt",
                "12",
            ]
            .map(str::to_owned),
        );
        // Validation mode explicitly reads this fixture, and disables all saving.
        args.push("--validate-only".into());
        let loaded = startup(args).unwrap().unwrap();
        assert!(loaded.store.is_none());
        assert_eq!(loaded.command.options.seed, 99);
        assert_eq!(loaded.draft.planet.seed, 99);
        assert!((loaded.command.terrain.erosion_strength - 6.0).abs() < f64::EPSILON);
        assert!((loaded.draft.terrain.erosion_strength - 6.0).abs() < f64::EPSILON);
        assert_eq!(loaded.command.solar.model, loaded.command.solar.draft);
        assert_eq!(read(&folder.file()).unwrap(), Some(original));
        let mut args = folder.args().to_vec();
        args.push("--no-settings".into());
        let fresh = startup(args).unwrap().unwrap();
        assert!(fresh.store.is_none());
        assert_eq!(fresh.command.options, Command::default().options);
        assert_eq!(read(&folder.file()).unwrap(), Some(original));
    }
    #[test]
    fn corrupt_or_future_settings_are_preserved_until_the_user_changes_a_valid_setting() {
        let folder = Folder::new();
        for contents in [
            "broken json",
            r#"{"version":99}"#,
            r#"{"applied":{"terrain":{"erosion_strength":9}}}"#,
        ] {
            fs::write(folder.file(), contents).unwrap();
            assert!(read(&folder.file()).is_err());
            let loaded = startup(folder.args()).unwrap().unwrap();
            assert_eq!(Settings::from(&loaded.command), Settings::default());
            drop(loaded);
            assert_eq!(fs::read_to_string(folder.file()).unwrap(), contents);
        }
    }
    #[test]
    fn unused_startup_settings_do_not_overwrite_the_last_working_world() {
        let folder = Folder::new();
        let original = edited();
        write(&folder.file(), &original).unwrap();
        let mut args = folder.args().to_vec();
        args.extend(["--vertices", "4"].map(str::to_owned));
        let pending = startup(args).unwrap().unwrap();
        assert_eq!(pending.command.options.vertex_count, 4);
        // A startup generation error occurs before the viewer observes a world.
        drop(pending);
        assert_eq!(read(&folder.file()).unwrap(), Some(original));
    }

    #[test]
    fn normal_exit_flushes_pending_edits_even_before_the_debounce_expires() {
        let folder = Folder::new();
        let expected = edited();
        let mut store = PreferencesStore::new(folder.file(), &Snapshot::default(), None);
        store.observe(&expected);
        assert!(!folder.file().exists());
        drop(store);
        assert_eq!(read(&folder.file()).unwrap(), Some(expected));
        let newest = Snapshot {
            draft: Settings {
                planet: planet_rs::PlanetOptions {
                    seed: 107,
                    ..expected.draft.planet
                },
                ..expected.draft
            },
            ..expected
        };
        write(&folder.file(), &newest).unwrap();
        assert_eq!(read(&folder.file()).unwrap(), Some(newest));
        assert_eq!(
            fs::read_dir(&folder.0).unwrap().count(),
            1,
            "atomic saves should leave no temporary file"
        );
    }
    #[test]
    fn live_resources_and_draft_edits_are_captured_and_restored_across_app_recreation() {
        use bevy::tasks::TaskPool;
        IoTaskPool::get_or_init(TaskPool::new);
        let folder = Folder::new();
        let expected = edited();
        let command = expected.command(&expected.applied);
        let mut app = App::new();
        let mut panel = Panel::new(&expected.applied, expected.display.hidden);
        panel.restore_draft(&expected.draft, expected.display.view);
        let terrain = planet_rs::generate_terrain(
            planet_rs::PlanetOptions {
                vertex_count: 32,
                ..Default::default()
            },
            planet_rs::TerrainOptions {
                relief: 0.0,
                refinement_passes: 0,
                river_refinement_passes: 0,
                hydraulic_passes: 0,
                thermal_passes: 0,
                ..Default::default()
            },
        )
        .unwrap();
        app.insert_resource(panel)
            .insert_resource(Scene {
                terrain: std::sync::Arc::new(terrain),
                seed: expected.applied.planet.seed,
                generation: expected.applied.mode,
                bed: expected.display.bed,
                river_display: expected.display.rivers,
                no_ocean: expected.display.no_ocean,
                wireframe: expected.display.wireframe,
                smooth: expected.display.smooth,
            })
            .insert_resource(command.streaming)
            .insert_resource(command.details)
            .insert_resource(command.forest)
            .insert_resource(command.lighting)
            .insert_resource(command.ocean)
            .insert_resource(command.water)
            .insert_resource(command.environment)
            .insert_resource(command.weather)
            .insert_resource(command.solar)
            .insert_resource(PreferencesStore::new(
                folder.file(),
                &Snapshot::default(),
                None,
            ))
            .add_systems(Last, remember);
        app.update();
        // Let a worker own the path, then change a setting and close while it is
        // in flight; the latest snapshot must still win at shutdown.
        app.world_mut().resource_mut::<PreferencesStore>().changed =
            Instant::now().checked_sub(Duration::from_secs(1)).unwrap();
        app.update();
        app.world_mut()
            .resource_mut::<EnvironmentSettings>()
            .coverage = 0.72;
        app.update();
        drop(app);
        let loaded = startup(folder.args()).unwrap().unwrap();
        assert_eq!(loaded.command.options.seed, 73);
        assert_eq!(loaded.draft.planet.seed, 74);
        assert!((loaded.command.environment.coverage - 0.72).abs() < f32::EPSILON);
        assert_eq!(loaded.command.details, expected.render.details);
        assert_eq!(loaded.draft.terrain.tectonics.branch_depth, 4);
    }
}
