//! Shared Island geometry, with bounded local instances rather than global draws.
#![allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
mod placement;
use super::{
    ForestSettings,
    material::{ForestMaterial, create_material},
};
use crate::{Orbit, terrain_material::TerrainTextures};
use bevy::{
    asset::RenderAssetUsages,
    mesh::{Indices, PrimitiveTopology},
    prelude::*,
};
use std::collections::{HashMap, HashSet};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Fern,
    Log,
}
struct Placed {
    root: Vec3,
    transform: Transform,
    kind: Kind,
    variant: usize,
}
pub(super) struct Built {
    props: Vec<Placed>,
    fern_models: Vec<motu::Mesh>,
    log_models: Vec<motu::FallenLogPrototype>,
}
pub(super) use placement::build;
struct Model {
    length: f32,
    radius: f32,
    lods: Vec<Handle<Mesh>>,
}
struct Live {
    entity: Entity,
    lod: usize,
}
#[derive(Default)]
pub(crate) struct World {
    props: Vec<Placed>,
    cells: HashMap<[i32; 3], Vec<usize>>,
    models: Vec<Model>,
    materials: Option<[Handle<ForestMaterial>; 2]>,
    live: HashMap<usize, Live>,
    wanted: Vec<usize>,
    eye: Option<Vec3>,
    toggles: (bool, bool),
    pending: bool,
    collision_dirty: bool,
    pub inspect: bool,
    pub ferns: usize,
    pub logs: usize,
    pub visible_ferns: usize,
    pub visible_logs: usize,
}
impl World {
    pub fn settled(&self) -> bool {
        !self.pending && !self.inspect
    }
    pub fn hide(&mut self, commands: &mut Commands) {
        for (_, live) in self.live.drain() {
            commands.entity(live.entity).despawn();
        }
        self.wanted.clear();
        self.eye = None;
        self.pending = false;
        self.visible_ferns = 0;
        self.visible_logs = 0;
        self.collision_dirty = true;
    }
    pub fn clear(
        &mut self,
        commands: &mut Commands,
        meshes: &mut Assets<Mesh>,
        materials: &mut Assets<ForestMaterial>,
    ) {
        self.hide(commands);
        for model in self.models.drain(..) {
            for handle in model.lods {
                meshes.remove(handle.id());
            }
        }
        if let Some(handles) = self.materials.take() {
            for handle in handles {
                materials.remove(handle.id());
            }
        }
        self.props.clear();
        self.cells.clear();
        self.ferns = 0;
        self.logs = 0;
    }
    pub(super) fn upload(
        &mut self,
        built: Built,
        radius: f32,
        textures: &TerrainTextures,
        meshes: &mut Assets<Mesh>,
        materials: &mut Assets<ForestMaterial>,
    ) {
        self.props = built.props;
        self.models = built
            .fern_models
            .into_iter()
            .enumerate()
            .map(|(i, source)| Model {
                length: 0.0,
                radius: 0.0,
                lods: vec![meshes.add(upload_mesh(source, None, i))],
            })
            .collect();
        self.models
            .extend(built.log_models.into_iter().map(|model| {
                Model {
                    length: model.length_metres,
                    radius: model.radius_metres,
                    lods: model
                        .meshes
                        .into_iter()
                        .enumerate()
                        .map(|(lod, source)| {
                            meshes.add(upload_mesh(source, Some(model.bark_vertices[lod]), 0))
                        })
                        .collect(),
                }
            }));
        for (id, prop) in self.props.iter().enumerate() {
            self.cells
                .entry((prop.root * radius / 32.0).floor().as_ivec3().to_array())
                .or_default()
                .push(id);
            match prop.kind {
                Kind::Fern => self.ferns += 1,
                Kind::Log => self.logs += 1,
            }
        }
        self.materials = Some([
            create_material(radius, 4, textures, materials),
            create_material(radius, 5, textures, materials),
        ]);
        info!(
            "Planet undergrowth: {} ferns, {} fitted fallen logs; {} shared models",
            self.ferns,
            self.logs,
            self.models.len()
        );
    }
    pub fn colliders(&mut self) -> Option<Vec<crate::collision::LogCollider>> {
        if !std::mem::take(&mut self.collision_dirty) {
            return None;
        }
        Some(
            self.live
                .keys()
                .filter_map(|&id| {
                    let prop = &self.props[id];
                    if prop.kind != Kind::Log {
                        return None;
                    }
                    let model = &self.models[4 + prop.variant];
                    let point = |x| {
                        prop.transform.transform_point(
                            Vec3::new(x, 0.0, model.radius) / motu::ISLAND_WORLD_METRES,
                        )
                    };
                    Some(crate::collision::LogCollider {
                        a: point(-model.length * 0.5),
                        b: point(model.length * 0.5),
                        radius: model.radius * prop.transform.scale.x / motu::ISLAND_WORLD_METRES
                            * 1.10,
                    })
                })
                .collect(),
        )
    }
    pub(super) fn inspection(
        &self,
        radius: f32,
        trees: &[super::placement::Plant],
    ) -> Option<Orbit> {
        let prop = self
            .props
            .iter()
            .filter(|p| p.kind == Kind::Log)
            .max_by(|a, b| {
                let score = |p: &Placed| {
                    let cell = (p.root * radius / 32.0).floor().as_ivec3();
                    let mut ferns = 0;
                    for x in -1..=1 {
                        for y in -1..=1 {
                            for z in -1..=1 {
                                if let Some(ids) =
                                    self.cells.get(&(cell + IVec3::new(x, y, z)).to_array())
                                {
                                    ferns += ids
                                        .iter()
                                        .filter(|&&i| {
                                            self.props[i].kind == Kind::Fern
                                                && self.props[i].root.distance(p.root) * radius
                                                    < 5.0
                                        })
                                        .count();
                                }
                            }
                        }
                    }
                    p.root
                        .normalize()
                        .dot(Vec3::new(-3.0, 4.0, 5.0).normalize())
                        + ferns.min(6) as f32 * 0.08
                };
                score(a).total_cmp(&score(b))
            })
            .or_else(|| self.props.first())?;
        let up = prop.root.normalize();
        let frame = Quat::from_rotation_arc(Vec3::Y, up);
        let side = frame.inverse() * (prop.transform.rotation * Vec3::Y);
        let base = side.x.atan2(side.z);
        (0..8)
            .map(|i| Orbit {
                frame,
                focus: prop.root + up * (0.35 / radius),
                distance: 8.0 / radius,
                yaw: base + i as f32 * std::f32::consts::FRAC_PI_4,
                pitch: 0.24,
                minimum: 0.5 / radius,
                maximum: 0.5,
            })
            .min_by_key(|orbit| {
                let eye = crate::orbit_transform(orbit).translation;
                let ray = orbit.focus - eye;
                let tangent = ray - up * ray.dot(up);
                trees
                    .iter()
                    .filter(|tree| {
                        let delta = tree.root - eye;
                        let t = (delta.dot(tangent) / tangent.length_squared()).clamp(0.0, 1.0);
                        let lateral = delta - tangent * t;
                        let lateral = lateral - up * lateral.dot(up);
                        lateral.length() * radius < 1.2 && delta.length() * radius < 15.0
                    })
                    .count()
            })
    }

    pub fn update(
        &mut self,
        eye: Vec3,
        radius: f32,
        settings: &ForestSettings,
        commands: &mut Commands,
        materials: &mut Assets<ForestMaterial>,
    ) {
        if self.materials.is_none() {
            return;
        }
        let toggles = (settings.ferns, settings.fallen_logs);
        let moved = self.eye.is_none_or(|old| old.distance(eye) * radius > 2.0);
        if moved || toggles != self.toggles {
            self.collision_dirty = true;
            self.eye = Some(eye);
            self.toggles = toggles;
            let cell = (eye * radius / 32.0).floor().as_ivec3();
            let mut candidates = Vec::new();
            for x in -5..=5 {
                for y in -5..=5 {
                    for z in -5..=5 {
                        if let Some(ids) = self.cells.get(&(cell + IVec3::new(x, y, z)).to_array())
                        {
                            candidates.extend(ids.iter().filter_map(|&id| {
                                let p = &self.props[id];
                                let distance = p.root.distance(eye) * radius;
                                let eligible = match p.kind {
                                    Kind::Fern => settings.ferns && distance < 50.0,
                                    Kind::Log => settings.fallen_logs && distance < 160.0,
                                };
                                eligible.then_some((distance, id))
                            }));
                        }
                    }
                }
            }
            candidates.sort_unstable_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
            let mut counts = [0; 2];
            self.wanted = candidates
                .into_iter()
                .filter_map(|(_, id)| {
                    let kind = usize::from(self.props[id].kind == Kind::Log);
                    counts[kind] += 1;
                    (counts[kind] <= [512, 96][kind]).then_some(id)
                })
                .collect();
            let wanted: HashSet<_> = self.wanted.iter().copied().collect();
            self.live.retain(|id, live| {
                if wanted.contains(id) {
                    true
                } else {
                    commands.entity(live.entity).despawn();
                    false
                }
            });
            for handle in self.materials.as_ref().unwrap() {
                if let Some(mut material) = materials.get_mut(handle) {
                    material.extension.settings.viewer = eye.extend(0.0);
                }
            }
        }
        let mut uploaded = 0;
        for &id in &self.wanted {
            let prop = &self.props[id];
            let model = &self.models[prop.variant + if prop.kind == Kind::Log { 4 } else { 0 }];
            let distance = prop.root.distance(eye) * radius;
            let lod = if prop.kind == Kind::Fern || distance < 35.0 {
                0
            } else if distance < 80.0 {
                1
            } else {
                2
            };
            if let Some(live) = self.live.get_mut(&id) {
                let change = lod_changes(live.lod, lod, distance);
                if change {
                    live.lod = lod;
                    commands
                        .entity(live.entity)
                        .insert(Mesh3d(model.lods[lod].clone()));
                }
            } else if uploaded < 12 {
                let kind = usize::from(prop.kind == Kind::Log);
                let entity = commands
                    .spawn((
                        Mesh3d(model.lods[lod].clone()),
                        MeshMaterial3d(self.materials.as_ref().unwrap()[kind].clone()),
                        prop.transform,
                    ))
                    .id();
                self.collision_dirty = true;
                self.live.insert(id, Live { entity, lod });
                uploaded += 1;
            }
        }
        self.pending = self.live.len() < self.wanted.len();
        self.visible_ferns = self
            .live
            .keys()
            .filter(|&&id| self.props[id].kind == Kind::Fern)
            .count();
        self.visible_logs = self.live.len() - self.visible_ferns;
    }
}
// Separate enter/leave distances keep log LODs stable at their boundaries.
fn lod_changes(previous: usize, desired: usize, distance: f32) -> bool {
    previous != desired
        && match (previous, desired) {
            (0, _) => distance > 38.0,
            (1, 0) => distance < 32.0,
            (1, 2) => distance > 85.0,
            (2, _) => distance < 75.0,
            _ => true,
        }
}

fn upload_mesh(source: motu::Mesh, bark: Option<usize>, variant: usize) -> Mesh {
    let colours: Vec<_> = (0..source.vertices.len())
        .map(|i| {
            bark.map_or(
                [variant as f32 / 4.0, 0.4 + variant as f32 * 0.12, 0.8, 0.3],
                |count| [0.0, f32::from(u8::from(i >= count)), 0.0, 1.0],
            )
        })
        .collect();
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD,
    )
    .with_inserted_attribute(
        Mesh::ATTRIBUTE_POSITION,
        source
            .vertices
            .into_iter()
            .map(|point| point.to_array())
            .collect::<Vec<_>>(),
    )
    .with_inserted_attribute(
        Mesh::ATTRIBUTE_NORMAL,
        source
            .normals
            .into_iter()
            .map(|normal| normal.to_array())
            .collect::<Vec<_>>(),
    )
    .with_inserted_attribute(
        Mesh::ATTRIBUTE_UV_0,
        source
            .uv
            .into_iter()
            .map(|uv| uv.to_array())
            .collect::<Vec<_>>(),
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, colours)
    .with_inserted_indices(Indices::U32(source.triangles))
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::world::CommandQueue;

    fn models(props: Vec<Placed>) -> Built {
        Built {
            props,
            fern_models: (0..4).map(motu::generate_fern).collect(),
            log_models: (0..8).map(motu::generate_fallen_log).collect(),
        }
    }
    #[test]
    fn island_models_are_deterministic_nonempty_and_have_successively_cheaper_logs() {
        for seed in 0..8 {
            let fern = motu::generate_fern(seed);
            assert_eq!(fern, motu::generate_fern(seed));
            assert!((60..=90).contains(&(fern.triangles.len() / 3)));
            assert!(fern.vertices.iter().all(|p| p.is_finite()));
            assert!(fern.normals.iter().all(|p| p.is_finite()));
            assert_eq!(fern.vertices.len(), fern.uv.len());
            let log = motu::generate_fallen_log(seed);
            assert_eq!(log.meshes, motu::generate_fallen_log(seed).meshes);
            assert!(
                log.meshes
                    .windows(2)
                    .all(|pair| pair[0].triangles.len() > pair[1].triangles.len())
            );
            for (lod, mesh) in log.meshes.iter().enumerate() {
                assert!(log.bark_vertices[lod] > 0 && log.bark_vertices[lod] < mesh.vertices.len());
                assert_eq!(mesh.vertices.len(), mesh.normals.len());
                assert_eq!(mesh.vertices.len(), mesh.uv.len());
                assert!(
                    mesh.triangles
                        .iter()
                        .all(|&i| (i as usize) < mesh.vertices.len())
                );
                assert!(
                    mesh.vertices
                        .iter()
                        .chain(&mesh.normals)
                        .all(|p| p.is_finite())
                );
            }
        }
    }
    fn tick(
        world: &mut World,
        ecs: &mut bevy::prelude::World,
        materials: &mut Assets<ForestMaterial>,
        eye: Vec3,
        settings: &ForestSettings,
    ) {
        let mut queue = CommandQueue::default();
        world.update(
            eye,
            2000.0,
            settings,
            &mut Commands::new(&mut queue, ecs),
            materials,
        );
        queue.apply(ecs);
    }
    #[test]
    fn nearby_instances_are_capped_reuse_assets_and_clear_on_toggles_or_regeneration() {
        let mut meshes = Assets::<Mesh>::default();
        let mut materials = Assets::<ForestMaterial>::default();
        let root = Vec3::Y * 1.01;
        let props = (0..720)
            .map(|i| Placed {
                root,
                transform: Transform::from_translation(root).with_scale(Vec3::splat(0.5)),
                kind: if i < 600 { Kind::Fern } else { Kind::Log },
                variant: 0,
            })
            .collect();
        let mut world = World::default();
        world.upload(
            models(props),
            2000.0,
            &TerrainTextures::default(),
            &mut meshes,
            &mut materials,
        );
        assert_eq!(meshes.len(), 28);
        let mut ecs = bevy::prelude::World::new();
        let mut settings = ForestSettings::default();
        for _ in 0..60 {
            tick(
                &mut world,
                &mut ecs,
                &mut materials,
                root + Vec3::Y * 0.001,
                &settings,
            );
        }
        assert!(world.settled());
        assert_eq!((world.visible_ferns, world.visible_logs), (512, 96));
        assert_eq!(world.colliders().unwrap().len(), 96);
        assert!(
            world.colliders().is_none(),
            "stationary frames do not reallocate collision"
        );
        assert_eq!((meshes.len(), materials.len()), (28, 2));
        settings.ferns = false;
        tick(&mut world, &mut ecs, &mut materials, root, &settings);
        assert_eq!((world.visible_ferns, world.visible_logs), (0, 96));
        tick(
            &mut world,
            &mut ecs,
            &mut materials,
            root + Vec3::Y * 0.03,
            &settings,
        );
        assert!(world.live.values().all(|live| live.lod == 1));
        tick(
            &mut world,
            &mut ecs,
            &mut materials,
            root + Vec3::Y * 0.06,
            &settings,
        );
        assert!(world.live.values().all(|live| live.lod == 2));
        assert_eq!(meshes.len(), 28, "log LOD changes reuse the shared assets");
        settings.fallen_logs = false;
        tick(&mut world, &mut ecs, &mut materials, root, &settings);
        assert!(world.colliders().unwrap().is_empty());
        settings = ForestSettings::default();
        tick(
            &mut world,
            &mut ecs,
            &mut materials,
            Vec3::Y * 2.5,
            &settings,
        );
        assert!(
            world.live.is_empty(),
            "ground props are absent at orbital distance"
        );
        let mut queue = CommandQueue::default();
        world.clear(
            &mut Commands::new(&mut queue, &ecs),
            &mut meshes,
            &mut materials,
        );
        queue.apply(&mut ecs);
        assert!(meshes.is_empty() && materials.is_empty() && world.props.is_empty());
    }
    #[test]
    fn radial_placement_repeats_and_log_shafts_follow_the_sampled_ground() {
        let mut scene = crate::collision::test_scene();
        scene.terrain = planet_rs::generate_terrain(
            planet_rs::PlanetOptions {
                vertex_count: 1024,
                ..planet_rs::PlanetOptions::default()
            },
            planet_rs::TerrainOptions {
                refinement_passes: 2,
                ..planet_rs::TerrainOptions::default()
            },
        )
        .unwrap()
        .into();
        let built = crate::forest::placement::build(scene.clone(), None, 2.0);
        let first = &built.undergrowth;
        let second = build(&scene, &built.ground, &built.plants);
        assert_eq!(first.props.len(), second.props.len());
        assert!(first.props.iter().any(|p| p.kind == Kind::Fern));
        assert!(first.props.iter().any(|p| p.kind == Kind::Log));
        assert!(
            first
                .props
                .iter()
                .zip(&second.props)
                .all(|(a, b)| a.kind == b.kind
                    && a.variant == b.variant
                    && a.transform == b.transform)
        );
        let radius = scene.terrain.topology.radius as f32;
        for prop in &first.props {
            if prop.kind == Kind::Fern {
                let (point, _, material) = built
                    .ground
                    .forest_sample(&scene.terrain, prop.root.normalize(), false)
                    .unwrap();
                assert!(point.distance(prop.root) * radius < 0.10);
                assert!(material[2] < 0.025);
            } else {
                let model = &first.log_models[prop.variant];
                for step in 0..=8 {
                    let t = step as f32 / 8.0;
                    let bottom = Vec3::new(
                        (t - 0.5) * model.length_metres,
                        0.0,
                        model.radius_metres * 0.22 * t,
                    ) / motu::ISLAND_WORLD_METRES;
                    let point = prop.transform.transform_point(bottom);
                    let (support, _, _) = built
                        .ground
                        .forest_sample(&scene.terrain, point.normalize(), false)
                        .unwrap();
                    let gap = (point - support).dot(prop.transform.rotation * Vec3::Z) * radius;
                    assert!(
                        (-model.radius_metres * 0.29 - 0.01..0.25).contains(&gap),
                        "shaft support gap {gap}"
                    );
                }
            }
        }
    }
}
