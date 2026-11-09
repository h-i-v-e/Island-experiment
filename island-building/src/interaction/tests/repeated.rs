//! Headless lifecycle and CPU scheduling evidence; this does not measure a GPU.

use super::super::{BuildingInstance, BuildingObserver, BuildingRuntimePlugin, DoorLeaf};
use crate::{
    BuildingRecipe, generate,
    render::{compile_building, spawn_building},
};
use bevy::{prelude::*, transform::TransformPlugin};
use std::{sync::Arc, time::Instant};

#[test]
fn sixty_four_placements_share_assets_keep_state_and_clean_up() {
    let started = Instant::now();
    let source = Arc::new(generate(&BuildingRecipe::hut()).unwrap());
    let generated = started.elapsed();
    let mut meshes = Assets::default();
    let mut materials = Assets::default();
    let mut images = Assets::default();
    let started = Instant::now();
    let compiled = compile_building(source, &mut meshes, &mut materials, &mut images);
    let compile_time = started.elapsed();
    let assets = (meshes.len(), materials.len(), images.len());
    let image_payload: usize = images
        .iter()
        .map(|(_, image)| image.data.as_ref().map_or(0, Vec::len))
        .sum();
    let triangles = compiled
        .source
        .lods
        .each_ref()
        .map(crate::BuildingLod::triangle_count);

    let mut app = App::new();
    app.add_plugins((MinimalPlugins, TransformPlugin, BuildingRuntimePlugin));
    app.insert_resource(meshes)
        .insert_resource(materials)
        .insert_resource(images);
    let observer = app
        .world_mut()
        .spawn((
            BuildingObserver::default(),
            Transform::from_xyz(0.0, 0.0, 60.0),
            GlobalTransform::from_translation(Vec3::Z * 60.0),
        ))
        .id();
    let baseline = app.world().entity_count();
    let first = spawn_building(
        &mut app.world_mut().commands(),
        Arc::clone(&compiled),
        "batch-0",
        Transform::IDENTITY,
    );
    app.world_mut().flush();
    let mut roots = Vec::with_capacity(64);
    roots.push(first);
    app.update();
    app.update();
    let one = sample_updates(&mut app);
    {
        let mut instance = app.world_mut().get_mut::<BuildingInstance>(first).unwrap();
        let state = instance.doors.get_mut("entrance").unwrap();
        state.angle = 0.8;
        state.target = 0.8;
    }
    spawn_remaining(&mut app, &compiled, &mut roots);
    app.world_mut().flush();
    app.update();
    app.update();
    assert_eq!(asset_counts(app.world()), assets);
    exercise_observer(&mut app, observer, first);
    let transform = Transform::from_xyz(0.0, 0.0, 60.0);
    app.world_mut()
        .entity_mut(observer)
        .insert((transform, GlobalTransform::from(transform)));
    app.update();
    for root in &roots[1..] {
        let instance = app.world().get::<BuildingInstance>(*root).unwrap();
        assert!(Arc::ptr_eq(&instance.prototype, &compiled));
        assert!(instance.doors["entrance"].angle.abs() < f32::EPSILON);
    }
    let sixty_four = sample_updates(&mut app);
    let mut lods = [0; 3];
    for root in &roots {
        lods[app
            .world()
            .get::<BuildingInstance>(*root)
            .unwrap()
            .current_lod] += 1;
    }
    assert_eq!(asset_counts(app.world()), assets);
    for root in roots {
        app.world_mut().despawn(root);
    }
    app.update();
    assert_eq!(app.world().entity_count(), baseline);
    assert_eq!(
        app.world_mut()
            .query::<&DoorLeaf>()
            .iter(app.world())
            .count(),
        0
    );
    assert_eq!(asset_counts(app.world()), assets);
    one.report("one placement");
    sixty_four.report("64 placements");
    eprintln!(
        "HEADLESS BUILDING ACCEPTANCE: generation={generated:?}, compile={compile_time:?}, assets(mesh/material/image)={assets:?}, image CPU payload={image_payload} bytes, triangles by LOD={triangles:?}; LOD distribution={lods:?}; cleanup entities={baseline}. Component payload excludes heap allocations/capacity/resources; timings include MinimalPlugins + transform propagation + building runtime, no rendering/GPU."
    );
}

fn spawn_remaining(
    app: &mut App,
    compiled: &Arc<crate::render::CompiledBuildingPrototype>,
    roots: &mut Vec<Entity>,
) {
    for index in 1_u16..64 {
        let transform = Transform::from_xyz(
            f32::from(index % 8) * 12.0,
            0.0,
            -f32::from(index / 8) * 12.0,
        );
        roots.push(spawn_building(
            &mut app.world_mut().commands(),
            Arc::clone(compiled),
            format!("batch-{index}"),
            transform,
        ));
    }
}

fn exercise_observer(app: &mut App, observer: Entity, first: Entity) {
    for (distance, expected) in [(5.0, 0), (60.0, 1), (300.0, 2)] {
        let transform = Transform::from_xyz(0.0, 0.0, distance);
        app.world_mut()
            .entity_mut(observer)
            .insert((transform, GlobalTransform::from(transform)));
        app.update();
        let instance = app.world().get::<BuildingInstance>(first).unwrap();
        assert_eq!(instance.current_lod, expected);
        assert!((instance.doors["entrance"].angle - 0.8).abs() < f32::EPSILON);
    }
}

struct Sample {
    entities: u32,
    component_payload: usize,
    mean_update_micros: f64,
}

impl Sample {
    fn report(&self, label: &str) {
        eprintln!(
            "HEADLESS {label}: {} entities, {} bytes component payload, {:.2} microseconds mean/update (200 samples after 10 warmups)",
            self.entities, self.component_payload, self.mean_update_micros
        );
    }
}

fn sample_updates(app: &mut App) -> Sample {
    for _ in 0..10 {
        app.update();
    }
    let started = Instant::now();
    for _ in 0..200 {
        app.update();
    }
    let mean_update_micros = started.elapsed().as_secs_f64() * 1_000_000.0 / 200.0;
    let world = app.world();
    let component_payload = world
        .archetypes()
        .iter()
        .map(|archetype| {
            let row: usize = archetype
                .components()
                .iter()
                .map(|id| world.components().get_info(*id).unwrap().layout().size())
                .sum();
            row * archetype.len() as usize
        })
        .sum();
    Sample {
        entities: world.entity_count(),
        component_payload,
        mean_update_micros,
    }
}

fn asset_counts(world: &World) -> (usize, usize, usize) {
    (
        world.resource::<Assets<Mesh>>().len(),
        world.resource::<Assets<StandardMaterial>>().len(),
        world.resource::<Assets<Image>>().len(),
    )
}
