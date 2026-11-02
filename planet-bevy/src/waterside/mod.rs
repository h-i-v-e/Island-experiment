//! Independent, bounded waterside instances sharing Island's vegetation material.
#![allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
mod placement;
use crate::{
    InitialOrbit, Orbit, Scene,
    collision::Ground,
    command::{GenerationMode, View},
    explore::Explorer,
    forest::{ForestMaterial, ForestSettings, create_material},
    terrain_material::TerrainTextures,
};
use bevy::{
    asset::RenderAssetUsages,
    mesh::{Indices, PrimitiveTopology},
    prelude::*,
    tasks::{AsyncComputeTaskPool, Task, block_on, poll_once},
};
use std::{
    collections::{HashMap, HashSet},
    time::{Duration, Instant},
};
#[derive(Resource, Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Settings {
    pub reeds: bool,
    pub rushes: bool,
    pub density: f32,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            reeds: true,
            rushes: true,
            density: 1.0,
        }
    }
}
impl Settings {
    pub fn validate(self) -> Result<(), String> {
        if self.density.is_finite() && (0.0..=3.0).contains(&self.density) {
            Ok(())
        } else {
            Err("reed density must be finite and 0..3".into())
        }
    }
}
struct Plant {
    root: Vec3,
    transform: Transform,
    outward: Vec3,
    rush: bool,
    above_sea: bool,
    variant: usize,
}
type PlacementJob = (Key, Task<(Vec<Plant>, Ground)>);
type Key = (usize, u64, u32, crate::command::BedView);
#[derive(Resource, Default)]
pub struct World {
    key: Option<Key>,
    request: Option<(Key, Instant)>,
    job: Option<PlacementJob>,
    plants: Vec<Plant>,
    cells: HashMap<[i32; 3], Vec<usize>>,
    meshes: Vec<Handle<Mesh>>,
    materials: Vec<Handle<ForestMaterial>>,
    live: HashMap<usize, Entity>,
    wanted: Vec<usize>,
    eye: Option<Vec3>,
    toggles: (bool, bool),
    pending: bool,
    pub inspect: bool,
    pub reeds: usize,
    pub rushes: usize,
    pub visible: usize,
}
impl World {
    pub fn with_view(view: View) -> Self {
        Self {
            inspect: view == View::Reeds,
            ..default()
        }
    }
    pub fn settled(&self) -> bool {
        self.job.is_none() && !self.pending && !self.inspect
    }
    fn hide(&mut self, commands: &mut Commands) {
        for (_, entity) in self.live.drain() {
            commands.entity(entity).despawn();
        }
        self.eye = None;
        self.wanted.clear();
        self.visible = 0;
    }
    fn clear(
        &mut self,
        commands: &mut Commands,
        meshes: &mut Assets<Mesh>,
        materials: &mut Assets<ForestMaterial>,
    ) {
        self.hide(commands);
        for handle in self.meshes.drain(..) {
            meshes.remove(handle.id());
        }
        for handle in self.materials.drain(..) {
            materials.remove(handle.id());
        }
        self.plants.clear();
        self.cells.clear();
        self.key = None;
        self.reeds = 0;
        self.rushes = 0;
    }
    fn upload(
        &mut self,
        plants: Vec<Plant>,
        radius: f32,
        textures: &TerrainTextures,
        meshes: &mut Assets<Mesh>,
        materials: &mut Assets<ForestMaterial>,
    ) {
        self.plants = plants;
        self.meshes = (0..8)
            .map(|variant| {
                let rush = variant >= 4;
                let source = motu::generate_reed(9173 + variant, rush);
                let data = vec![
                    [
                        f32::from(u8::from(rush)),
                        0.35 + (variant % 4) as f32 * 0.15,
                        if rush { 0.75 } else { 0.45 },
                        variant as f32 * 0.117
                    ];
                    source.vertices.len()
                ];
                meshes.add(
                    Mesh::new(
                        PrimitiveTopology::TriangleList,
                        RenderAssetUsages::RENDER_WORLD,
                    )
                    .with_inserted_attribute(
                        Mesh::ATTRIBUTE_POSITION,
                        source
                            .vertices
                            .into_iter()
                            .map(|p| p.to_array())
                            .collect::<Vec<_>>(),
                    )
                    .with_inserted_attribute(
                        Mesh::ATTRIBUTE_NORMAL,
                        source
                            .normals
                            .into_iter()
                            .map(|p| p.to_array())
                            .collect::<Vec<_>>(),
                    )
                    .with_inserted_attribute(
                        Mesh::ATTRIBUTE_UV_0,
                        source
                            .uv
                            .into_iter()
                            .map(|p| p.to_array())
                            .collect::<Vec<_>>(),
                    )
                    .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, data)
                    .with_inserted_indices(Indices::U32(source.triangles)),
                )
            })
            .collect();
        self.materials = [6, 7]
            .map(|kind| create_material(radius, kind, textures, materials))
            .into();
        for (id, plant) in self.plants.iter().enumerate() {
            self.cells
                .entry((plant.root * radius / 24.0).floor().as_ivec3().to_array())
                .or_default()
                .push(id);
            if plant.rush {
                self.rushes += 1;
            } else {
                self.reeds += 1;
            }
        }
    }
    fn draw(
        &mut self,
        eye: Vec3,
        radius: f32,
        settings: Settings,
        commands: &mut Commands,
        materials: &mut Assets<ForestMaterial>,
    ) {
        if self.meshes.is_empty() {
            return;
        }
        let toggles = (settings.reeds, settings.rushes);
        if self.eye.is_none_or(|old| old.distance(eye) * radius > 1.5) || self.toggles != toggles {
            self.eye = Some(eye);
            self.toggles = toggles;
            let cell = (eye * radius / 24.0).floor().as_ivec3();
            let mut wanted = Vec::new();
            for x in -3..=3 {
                for y in -3..=3 {
                    for z in -3..=3 {
                        if let Some(ids) = self.cells.get(&(cell + IVec3::new(x, y, z)).to_array())
                        {
                            wanted.extend(ids.iter().filter_map(|&id| {
                                let plant = &self.plants[id];
                                let distance = plant.root.distance(eye) * radius;
                                (distance < 60.0
                                    && if plant.rush {
                                        settings.rushes
                                    } else {
                                        settings.reeds
                                    })
                                .then_some((distance, id))
                            }));
                        }
                    }
                }
            }
            wanted.sort_unstable_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
            self.wanted = wanted.into_iter().take(768).map(|(_, id)| id).collect();
            let wanted: HashSet<_> = self.wanted.iter().copied().collect();
            self.live.retain(|id, entity| {
                if wanted.contains(id) {
                    true
                } else {
                    commands.entity(*entity).despawn();
                    false
                }
            });
            for handle in &self.materials {
                if let Some(mut material) = materials.get_mut(handle) {
                    material.extension.settings.viewer = eye.extend(0.0);
                }
            }
        }
        let mut uploaded = 0;
        for &id in &self.wanted {
            if !self.live.contains_key(&id) && uploaded < 16 {
                let plant = &self.plants[id];
                let kind = usize::from(plant.rush);
                let entity = commands
                    .spawn((
                        Mesh3d(self.meshes[plant.variant + kind * 4].clone()),
                        MeshMaterial3d(self.materials[kind].clone()),
                        plant.transform,
                    ))
                    .id();
                self.live.insert(id, entity);
                uploaded += 1;
            }
        }
        self.visible = self.live.len();
        self.pending = self.visible < self.wanted.len();
    }
    fn inspection(&self, radius: f32) -> Option<Orbit> {
        let plant = self.plants.iter().max_by(|a, b| {
            let score = |p: &Plant| {
                p.root
                    .normalize()
                    .dot(Vec3::new(-3.0, 4.0, 5.0).normalize())
            };
            (a.above_sea && score(a) > 0.3)
                .cmp(&(b.above_sea && score(b) > 0.3))
                .then_with(|| score(a).total_cmp(&score(b)))
        })?;
        let frame = Quat::from_rotation_arc(Vec3::Y, plant.root.normalize());
        let side = frame.inverse() * plant.outward;
        Some(Orbit {
            frame,
            focus: plant.root + plant.root.normalize() * (0.55 / radius),
            distance: 6.0 / radius,
            yaw: side.x.atan2(side.z),
            pitch: 0.18,
            minimum: 0.3 / radius,
            maximum: 0.5,
        })
    }
}
#[allow(clippy::too_many_arguments, clippy::too_many_lines)] // Worker ownership and bounded render handoff.
pub fn update(
    mut commands: Commands,
    scene: Res<Scene>,
    settings: Res<Settings>,
    forest: Res<ForestSettings>,
    mut state: ResMut<World>,
    mut explorer: ResMut<Explorer>,
    mut cameras: Query<(&mut Orbit, &mut Transform)>,
    mut initial: ResMut<InitialOrbit>,
    textures: Res<TerrainTextures>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ForestMaterial>>,
) {
    if scene.generation != GenerationMode::Terrain || scene.terrain.is_moon() {
        state.job = None;
        state.clear(&mut commands, &mut meshes, &mut materials);
        state.pending = false;
        state.inspect = false;
        return;
    }
    if !settings.reeds && !settings.rushes {
        state.job = None;
        state.hide(&mut commands);
        state.pending = false;
        state.inspect = false;
        return;
    }
    let Ok((mut orbit, mut camera)) = cameras.single_mut() else {
        return;
    };
    let key = (
        scene.terrain.elevations.as_ptr() as usize,
        scene.seed,
        settings.density.to_bits(),
        scene.bed,
    );
    if state.request.is_none_or(|(old, _)| old != key) {
        state.request = Some((key, Instant::now()));
    }
    if state
        .key
        .is_some_and(|old| old.0 != key.0 || old.3 != key.3)
    {
        state.clear(&mut commands, &mut meshes, &mut materials);
    }
    if let Some((_, job)) = &mut state.job
        && let Some((plants, ground)) = block_on(poll_once(job))
    {
        let (source, _) = state.job.take().unwrap();
        if source == key {
            state.clear(&mut commands, &mut meshes, &mut materials);
            explorer.adopt_ground(&scene, ground);
            state.upload(
                plants,
                scene.terrain.topology.radius as f32,
                &textures,
                &mut meshes,
                &mut materials,
            );
            state.key = Some(key);
        }
    }
    if state.key != Some(key) {
        state.pending = true;
        if state.job.is_none()
            && state
                .request
                .is_some_and(|(_, when)| when.elapsed() >= Duration::from_millis(300))
        {
            let query = explorer.shared_ground(&scene);
            if query.is_some() || !forest.enabled {
                let source = scene.as_ref().clone();
                let options = *settings;
                state.job = Some((
                    key,
                    AsyncComputeTaskPool::get().spawn(async move {
                        let ground = query.unwrap_or_else(|| Ground::new(&source));
                        let plants = placement::build(&source, &ground, options);
                        (plants, ground)
                    }),
                ));
            }
        }
        return;
    }
    let radius = scene.terrain.topology.radius as f32;
    if state.inspect {
        state.inspect = false;
        if let Some(view) = state.inspection(radius) {
            *orbit = view;
            *camera = crate::orbit_transform(&orbit);
            initial.0 = *orbit;
            explorer.return_to_orbit = explorer.navigating();
        }
    }
    state.draw(
        camera.translation,
        radius,
        *settings,
        &mut commands,
        &mut materials,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn streamed_clumps_reuse_eight_meshes_and_obey_species_toggles() {
        let mut app = App::new();
        let mut meshes = Assets::<Mesh>::default();
        let mut materials = Assets::<ForestMaterial>::default();
        let mut world = World::default();
        let plants = (0..1200)
            .map(|i| Plant {
                root: Vec3::Y,
                transform: Transform::from_translation(Vec3::Y),
                outward: Vec3::X,
                rush: i % 2 == 0,
                above_sea: true,
                variant: i % 4,
            })
            .collect();
        world.upload(
            plants,
            2000.0,
            &TerrainTextures::default(),
            &mut meshes,
            &mut materials,
        );
        assert_eq!(meshes.len(), 8);
        assert_eq!(materials.len(), 2);
        for _ in 0..50 {
            let mut commands = app.world_mut().commands();
            world.draw(
                Vec3::Y,
                2000.0,
                Settings::default(),
                &mut commands,
                &mut materials,
            );
            app.world_mut().flush();
        }
        assert_eq!(world.visible, 768);
        assert!(!world.pending);
        let mut commands = app.world_mut().commands();
        world.draw(
            Vec3::Y,
            2000.0,
            Settings {
                reeds: false,
                ..default()
            },
            &mut commands,
            &mut materials,
        );
        assert!(world.live.keys().all(|&id| world.plants[id].rush));
        world.clear(&mut commands, &mut meshes, &mut materials);
        app.world_mut().flush();
        assert!(meshes.is_empty() && materials.is_empty() && world.live.is_empty());
    }
}
