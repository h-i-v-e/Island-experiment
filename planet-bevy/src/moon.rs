//! Dry lunar preview controls and the independently rendered companion body.
use crate::{
    Orbit, Scene,
    lod_material::LodMaterial,
    solar::{SolarControls, SolarFrame},
    terrain_material::TerrainTextures,
};
use bevy::{
    asset::RenderAssetUsages,
    mesh::Indices,
    prelude::*,
    render::render_resource::PrimitiveTopology,
    tasks::{AsyncComputeTaskPool, Task, block_on, poll_once},
};
use planet_rs::{
    PlanetOptions,
    solar::moon::MoonOrbit,
    terrain::moon::{MoonMesh, MoonOptions, generate_moon},
};
use std::time::{Duration, Instant};

pub fn controls(ui: &mut bevy_egui::egui::Ui, recipe: &mut planet_rs::terrain::moon::MoonOptions) {
    recipe_sliders(ui, recipe);
    ui.small("Dry impact geology: overlapping craters, raised rims, terraced walls, central peaks and pale ejecta rays.");
    ui.small("Generate to apply. Basin floors stay dry; atmospheric weather and vegetation are disabled for this preview.");
}
fn recipe_sliders(
    ui: &mut bevy_egui::egui::Ui,
    recipe: &mut planet_rs::terrain::moon::MoonOptions,
) {
    use bevy_egui::egui;
    ui.add(egui::Slider::new(&mut recipe.relief, 0.0..=2.0).text("Moon relief"));
    ui.add(egui::Slider::new(&mut recipe.crater_density, 0.0..=3.0).text("Crater density"));
    ui.add(egui::Slider::new(&mut recipe.maria, 0.0..=1.0).text("Basalt-flooded basins"));
    ui.add(egui::Slider::new(&mut recipe.weathering, 0.0..=1.0).text("Old crater softening"));
    ui.add(egui::Slider::new(&mut recipe.refinements, 0..=5).text("Moon mesh refinements"));
}

#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct CompanionSettings {
    pub enabled: bool,
    pub orbit: MoonOrbit,
    pub recipe: MoonOptions,
    pub light: crate::moonlight::MoonlightSettings,
}
impl Default for CompanionSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            orbit: MoonOrbit::default(),
            recipe: MoonOptions::default(),
            light: crate::moonlight::MoonlightSettings::default(),
        }
    }
}
impl CompanionSettings {
    pub fn validate(self) -> Result<(), String> {
        self.light.validate()?;
        self.orbit.validate()?;
        self.recipe.validate().map_err(|e| e.to_string())
    }
}
#[derive(Component)]
struct MoonBody;
type RecipeKey = (u64, MoonOptions);
struct PreparedMoon {
    key: RecipeKey,
    meshes: Vec<MoonMesh>,
}
#[derive(Resource, Default)]
pub struct MoonWorld {
    requested: Option<RecipeKey>,
    installed: Option<RecipeKey>,
    changed: Option<Instant>,
    job: Option<Task<Result<PreparedMoon, String>>>,
    entity: Option<Entity>,
    meshes: Vec<Handle<Mesh>>,
    material: Option<Handle<LodMaterial>>,
    active: bool,
    pub error: Option<String>,
    pub position: Vec3,
    pub lod: usize,
    inspecting: bool,
    radius: f32,
}
impl MoonWorld {
    pub fn visible_body(&self) -> bool {
        self.active && self.entity.is_some()
    }

    pub fn nearby_surface(&self, eye: Vec3) -> Option<f32> {
        if !self.active || self.entity.is_none() || self.radius <= 0.0 {
            return None;
        }
        let distance = eye.distance(self.position);
        (distance < self.radius * 6.0 && distance - self.radius < (eye.length() - 1.0).max(0.0))
            .then_some(distance / self.radius)
    }

    pub fn settled(&self) -> bool {
        !self.active
            || (self.installed == self.requested && self.job.is_none() && self.entity.is_some())
    }
}
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct MoonUpdate;
pub struct CompanionPlugin;
impl Plugin for CompanionPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(crate::moonlight::MoonlightPlugin)
            .init_resource::<MoonWorld>()
            .add_systems(
                Update,
                update
                    .in_set(MoonUpdate)
                    .after(crate::solar::SolarUpdate)
                    .after(crate::controls)
                    .after(crate::explore::update),
            );
    }
}
fn mesh(source: MoonMesh) -> Mesh {
    let positions: Vec<_> = source
        .topology
        .directions
        .iter()
        .zip(source.heights)
        .map(|(&p, h)| (p * (1.0 + h)).as_vec3().to_array())
        .collect();
    let normals: Vec<_> = source
        .normals
        .into_iter()
        .map(|n| n.as_vec3().to_array())
        .collect();
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD,
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
    .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, source.albedo)
    .with_inserted_indices(Indices::U32(
        source.topology.triangles.into_iter().flatten().collect(),
    ))
}
#[allow(clippy::cast_possible_truncation)] // Metre-based f64 state converted to planet-local rendering.
fn transform(settings: &SolarControls, frame: &SolarFrame, radius: f64) -> Transform {
    let state =
        settings
            .companion
            .orbit
            .state(frame.orbital_seconds, radius, settings.model, frame.state);
    Transform {
        translation: crate::surface::vector(state.position / radius),
        rotation: Quat::from_array(state.rotation.as_quat().to_array()),
        scale: Vec3::splat((state.radius / radius) as f32),
    }
}

#[allow(
    clippy::too_many_arguments,
    clippy::too_many_lines,
    clippy::cast_possible_truncation
)] // One owned worker/asset transition and its live transform.
fn update(
    mut commands: Commands,
    scene: Res<Scene>,
    settings: Res<SolarControls>,
    frame: Res<SolarFrame>,
    panel: Res<crate::panel::Panel>,
    explorer: Res<crate::explore::Explorer>,
    textures: Res<TerrainTextures>,
    mut world: ResMut<MoonWorld>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<LodMaterial>>,
    mut bodies: Query<(&mut Transform, &mut Mesh3d, &mut Visibility), With<MoonBody>>,
    mut cameras: Query<(&mut Orbit, &mut Transform), Without<MoonBody>>,
) {
    world.active = settings.companion.enabled && !scene.terrain.is_moon();
    if !world.active {
        if let Some(id) = world.entity
            && let Ok((_, _, mut v)) = bodies.get_mut(id)
        {
            *v = Visibility::Hidden;
        }
        return;
    }
    let radius = scene.terrain.topology.radius;
    let next = transform(&settings, &frame, radius);
    world.position = next.translation;
    world.radius = next.scale.x;
    let key = (scene.seed, settings.companion.recipe);
    if world.requested != Some(key) {
        let initial = world.requested.is_none();
        world.requested = Some(key);
        world.changed = if initial { None } else { Some(Instant::now()) };
        world.error = None;
    }
    if let Some(job) = world.job.as_mut()
        && let Some(result) = block_on(poll_once(job))
    {
        world.job = None;
        match result {
            Ok(prepared) if Some(prepared.key) == world.requested => {
                if let Some(id) = world.entity.take() {
                    commands.entity(id).despawn();
                }
                for handle in world.meshes.drain(..) {
                    meshes.remove(handle.id());
                }
                if let Some(handle) = world.material.take() {
                    materials.remove(handle.id());
                }
                world.meshes = prepared
                    .meshes
                    .into_iter()
                    .map(|source| meshes.add(mesh(source)))
                    .collect();
                let mut extension =
                    textures.extension((radius * settings.companion.orbit.radius_ratio) as f32);
                extension.terrain.padding = 1.0;
                extension.settings.terrain = 5;
                let material = materials.add(LodMaterial {
                    base: StandardMaterial {
                        perceptual_roughness: 1.0,
                        reflectance: 0.03,
                        ..default()
                    },
                    extension,
                });
                world.entity = Some(
                    commands
                        .spawn((
                            MoonBody,
                            Visibility::Visible,
                            Mesh3d(world.meshes[0].clone()),
                            MeshMaterial3d(material.clone()),
                            next,
                        ))
                        .id(),
                );
                world.material = Some(material);
                world.installed = Some(prepared.key);
            }
            Ok(_) => {}
            Err(error) => {
                error!("Moon generation failed: {error}");
                world.error = Some(error);
            }
        }
    }
    if world.job.is_none()
        && world.installed != world.requested
        && world.error.is_none()
        && world
            .changed
            .is_none_or(|t| t.elapsed() >= Duration::from_millis(300))
    {
        world.job = Some(AsyncComputeTaskPool::get().spawn(async move {
            let mut result = Vec::with_capacity(3);
            let levels = [
                key.1.refinements.min(1),
                key.1.refinements.min(2),
                key.1.refinements,
            ];
            for refinements in levels {
                result.push(
                    generate_moon(
                        PlanetOptions {
                            seed: key.0 ^ 0x729a_d01b_631f_540e,
                            vertex_count: 1024,
                            radius: 1.0,
                            ..default()
                        },
                        key.1,
                        refinements,
                    )
                    .map_err(|e| e.to_string())?,
                );
            }
            Ok(PreparedMoon {
                key,
                meshes: result,
            })
        }));
    }
    if panel.inspecting_moon() && !explorer.navigating() {
        for (mut orbit, mut camera) in &mut cameras {
            if !world.inspecting {
                orbit.distance = settings.companion.orbit.radius_ratio as f32 * 3.5;
            }
            orbit.focus = next.translation;
            orbit.minimum = settings.companion.orbit.radius_ratio as f32 * 1.08;
            *camera = crate::orbit_transform(&orbit);
        }
    }
    world.inspecting = panel.inspecting_moon() && !explorer.navigating();
    let distance = cameras
        .iter()
        .next()
        .map_or(10.0, |(_, c)| c.translation.distance(next.translation));
    let relative = distance / settings.companion.orbit.radius_ratio as f32;
    // Hysteresis avoids toggling meshes around a threshold as the player moves.
    world.lod = match world.lod {
        0 if relative < 12.0 => 1,
        1 if relative > 16.0 => 0,
        1 if relative < 4.5 => 2,
        2 if relative > 6.0 => 1,
        other => other,
    };
    if let Some(id) = world.entity
        && let Ok((mut t, mut body, mut visible)) = bodies.get_mut(id)
    {
        *t = next;
        *visible = Visibility::Visible;
        if body.0 != world.meshes[world.lod] {
            body.0 = world.meshes[world.lod].clone();
        }
    }
    if let Some(handle) = &world.material
        && let Some(mut material) = materials.get_mut(handle)
    {
        let moon_radius = (radius * settings.companion.orbit.radius_ratio) as f32;
        if material.extension.terrain.radius.to_bits() != moon_radius.to_bits() {
            material.extension.terrain.radius = moon_radius;
        }
    }
}
#[allow(clippy::cast_possible_truncation)]
pub fn inspection_orbit(radius: f64) -> Orbit {
    let settings = SolarControls::default();
    let planet = planet_rs::solar::SolarSystem::new(settings.model, radius)
        .unwrap()
        .state(0.0);
    let moon = settings
        .companion
        .orbit
        .state(0.0, radius, settings.model, Some(planet));
    Orbit {
        focus: crate::surface::vector(moon.position / radius),
        distance: settings.companion.orbit.radius_ratio as f32 * 3.5,
        yaw: 0.45,
        pitch: 0.25,
        frame: Quat::IDENTITY,
        minimum: settings.companion.orbit.radius_ratio as f32 * 1.08,
        maximum: 12.0,
    }
}

pub fn companion_controls(
    ui: &mut bevy_egui::egui::Ui,
    settings: &mut CompanionSettings,
    radius: f64,
    solar: planet_rs::solar::SolarSettings,
) {
    use bevy_egui::egui;
    egui::CollapsingHeader::new("Companion moon").show(ui, |ui| {
        ui.checkbox(&mut settings.enabled, "Show companion moon");
        crate::moonlight::controls(ui,&mut settings.light);
        let orbit = &mut settings.orbit;
        ui.add(egui::Slider::new(&mut orbit.radius_ratio, 0.05..=0.4).text("Moon / planet radius"));
        let max_distance = (0.45 * solar.hill_radius(radius) / radius - 1e-6).clamp(2.0, 10.0);
        ui.add(
            egui::Slider::new(&mut orbit.distance_ratio, 2.0..=max_distance)
                .text("Moon distance / planet radius"),
        );
        let minimum = MoonOrbit { distance_ratio: 2.0, ..*orbit }.orbital_period(radius) / 60.0;
        let maximum = MoonOrbit { distance_ratio: max_distance, ..*orbit }.orbital_period(radius) / 60.0;
        let mut minutes = orbit.orbital_period(radius) / 60.0;
        if ui.add(egui::Slider::new(&mut minutes, minimum..=maximum).text("Moon orbit (physical minutes)")).changed() {
            orbit.distance_ratio = orbit.distance_for_period(radius, minutes * 60.0);
        }
        ui.small(format!("Gravity-derived orbit: {:.2} physical minutes · {:.1} m/s", orbit.orbital_period(radius) / 60.0, orbit.orbital_speed(radius)));
        let mut inclination = orbit.inclination.to_degrees();
        ui.add(egui::Slider::new(&mut inclination, 0.0..=60.0).text("Moon inclination (degrees)"));
        orbit.inclination = inclination.to_radians();
        let mut angle=orbit.initial_angle.to_degrees().rem_euclid(360.0);
        ui.add(egui::Slider::new(&mut angle,0.0..=360.0).text("Starting orbit angle"));
        orbit.initial_angle=angle.to_radians();
        ui.small(format!(
            "Radius {:.0} m · centre distance {:.0} m · surface gap {:.0} m",
            radius * orbit.radius_ratio,
            radius * orbit.distance_ratio,
            radius * (orbit.distance_ratio - 1.0 - orbit.radius_ratio)
        ));
        ui.small("Tidally locked. Moon camera follows it; System shows both bodies.");
        recipe_sliders(ui, &mut settings.recipe);
        ui.small("Recipe edits rebuild this moon in the background after dragging stops. Orbit controls apply live.");
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    fn wait(app: &mut App) {
        for _ in 0..3000 {
            app.update();
            if app.world().resource::<MoonWorld>().settled() {
                return;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        panic!(
            "moon worker did not complete: {:?}",
            app.world().resource::<MoonWorld>().error
        );
    }
    fn light_lux(app: &App) -> f64 {
        app.world()
            .resource::<crate::moonlight::MoonlightFrame>()
            .lux
    }
    fn lunar_fixture() -> planet_rs::PlanetTerrain {
        planet_rs::generate_terrain(
            PlanetOptions {
                vertex_count: 16,
                ..default()
            },
            planet_rs::TerrainOptions {
                source: planet_rs::TerrainSource::Moon,
                moon: MoonOptions {
                    refinements: 0,
                    ..default()
                },
                ..default()
            },
        )
        .unwrap()
    }
    #[test]
    fn motion_and_scale_reuse_assets_and_the_standalone_recipe_hides_the_companion() {
        let mut settings = SolarControls::default();
        settings.companion.recipe.refinements = 0;
        let scene = crate::collision::test_scene();
        let orbit = crate::initial_orbit(crate::command::View::Front, &scene.terrain);
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(scene)
            .insert_resource(settings)
            .init_resource::<SolarFrame>()
            .insert_resource(crate::panel::Panel::new(
                &crate::panel::Settings::default(),
                false,
            ))
            .init_resource::<crate::explore::Explorer>()
            .init_resource::<TerrainTextures>()
            .init_resource::<MoonWorld>()
            .init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<LodMaterial>>()
            .init_resource::<crate::environment::EnvironmentSettings>()
            .init_resource::<crate::lighting::LightingSettings>()
            .add_plugins(crate::moonlight::MoonlightPlugin)
            .add_systems(Update, update.in_set(MoonUpdate));
        app.world_mut()
            .spawn((orbit, crate::orbit_transform(&orbit)));
        wait(&mut app);
        app.update();
        assert!(light_lux(&app) > 0.0);
        app.world_mut()
            .resource_mut::<SolarControls>()
            .companion
            .light
            .strength = 0.0;
        app.update();
        assert!(light_lux(&app).abs() < 1e-9);
        let entity = app.world().resource::<MoonWorld>().entity.unwrap();
        let position = app.world().get::<Transform>(entity).unwrap().translation;
        let handles = app.world().resource::<MoonWorld>().meshes.clone();
        let material = app.world().resource::<MoonWorld>().material.clone();
        app.world_mut().resource_mut::<SolarFrame>().orbital_seconds = 100.0;
        app.world_mut()
            .resource_mut::<SolarControls>()
            .companion
            .light
            .strength = 3.0;
        app.world_mut()
            .resource_mut::<SolarControls>()
            .companion
            .orbit
            .radius_ratio = 0.25;
        app.update();
        assert!(
            app.world()
                .get::<Transform>(entity)
                .unwrap()
                .translation
                .distance(position)
                > 0.1
        );
        assert!((app.world().get::<Transform>(entity).unwrap().scale.x - 0.25).abs() < 1e-6);
        let world = app.world().resource::<MoonWorld>();
        assert!(world.job.is_none());
        let eye = world.position + Vec3::Y * world.radius * 3.5;
        assert!((world.nearby_surface(eye).unwrap() - 3.5).abs() < 1e-5);
        assert!(world.nearby_surface(Vec3::Y * 1.001).is_none());
        assert_eq!(world.meshes, handles);
        assert_eq!(world.material, material);
        assert_eq!(app.world().resource::<Assets<LodMaterial>>().len(), 1);
        app.world_mut()
            .resource_mut::<SolarControls>()
            .companion
            .enabled = false;
        app.update();
        assert!(light_lux(&app).abs() < 1e-9);
        assert_eq!(
            *app.world().get::<Visibility>(entity).unwrap(),
            Visibility::Hidden
        );
        app.world_mut()
            .resource_mut::<SolarControls>()
            .companion
            .enabled = true;
        app.world_mut().resource_mut::<Scene>().terrain = std::sync::Arc::new(lunar_fixture());
        app.update();
        assert_eq!(
            *app.world().get::<Visibility>(entity).unwrap(),
            Visibility::Hidden
        );
        assert!(app.world().resource::<MoonWorld>().settled());
    }
}
