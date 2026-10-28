use super::*;
use bevy::time::TimeUpdateStrategy;
use std::time::{Duration, Instant};

fn map(value: u8) -> CoastMap {
    CoastMap {
        size: 32,
        pixels: vec![value; 32 * 32 * 24],
    }
}
fn fixture() -> (App, Entity) {
    let mut scene = crate::collision::test_scene();
    std::sync::Arc::get_mut(&mut scene.terrain)
        .unwrap()
        .topology
        .radius = 40.0;
    scene.no_ocean = false;
    let mut world = OceanWorld::default();
    world.install_maps(map(200));
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(
            20,
        )))
        .insert_resource(scene)
        .insert_resource(world)
        .insert_resource(OceanSettings {
            fine_radius: 16.0,
            ..default()
        })
        .init_resource::<TerrainTextures>()
        .init_resource::<Assets<Mesh>>()
        .init_resource::<Assets<Image>>()
        .init_resource::<Assets<OceanMaterial>>()
        .add_systems(Update, update);
    let camera = app
        .world_mut()
        .spawn((
            Orbit {
                yaw: 0.0,
                pitch: 0.0,
                distance: 2.0,
                focus: Vec3::ZERO,
                frame: Quat::IDENTITY,
                minimum: 1.0,
                maximum: 10.0,
            },
            Transform::from_translation(Vec3::Y * 2.0),
        ))
        .id();
    (app, camera)
}
fn settle(app: &mut App) {
    let started = Instant::now();
    loop {
        app.update();
        let world = app.world().resource::<OceanWorld>();
        if world.settled() && world.entity.is_some() {
            return;
        }
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "{}",
            world.status
        );
        std::thread::yield_now();
    }
}

#[test]
fn moving_player_rotates_one_mesh_and_maps_replace_without_rebuilding_geometry() {
    let (mut app, camera) = fixture();
    settle(&mut app);
    let world = app.world().resource::<OceanWorld>();
    let entity = world.entity.unwrap();
    let mesh = world.mesh.clone().unwrap();
    let old_map = world.map.clone().unwrap();
    let material = world.material.clone().unwrap();
    for up in [Vec3::X, -Vec3::Y, -Vec3::Z] {
        app.world_mut()
            .get_mut::<Transform>(camera)
            .unwrap()
            .translation = up * 1.001;
        app.update();
        let transform = app.world().get::<Transform>(entity).unwrap();
        assert!((transform.rotation * Vec3::Y).distance(up) < 1e-5);
        let world = app.world().resource::<OceanWorld>();
        assert_eq!(world.mesh.as_ref(), Some(&mesh));
        assert_eq!(world.entity, Some(entity));
    }
    app.world_mut()
        .resource_mut::<OceanWorld>()
        .install_maps(map(80));
    app.update();
    let world = app.world().resource::<OceanWorld>();
    assert_eq!(world.mesh.as_ref(), Some(&mesh));
    let replacement = world.map.clone().unwrap();
    assert_ne!(replacement, old_map);
    assert!(
        app.world()
            .resource::<Assets<Image>>()
            .get(&old_map)
            .is_none()
    );
    assert_eq!(
        app.world()
            .resource::<Assets<OceanMaterial>>()
            .get(&material)
            .unwrap()
            .extension
            .coast,
        replacement
    );
}

#[test]
fn visibility_mode_changes_and_rejected_grid_keep_a_recoverable_surface() {
    let (mut app, _) = fixture();
    settle(&mut app);
    let entity = app.world().resource::<OceanWorld>().entity.unwrap();
    let mesh = app.world().resource::<OceanWorld>().mesh.clone();
    app.world_mut().resource_mut::<Scene>().no_ocean = true;
    app.update();
    assert_eq!(
        *app.world().get::<Visibility>(entity).unwrap(),
        Visibility::Hidden
    );
    app.world_mut().resource_mut::<Scene>().generation = GenerationMode::Sphere;
    app.update();
    assert!(app.world().resource::<OceanWorld>().settled());
    app.world_mut().resource_mut::<Scene>().generation = GenerationMode::Terrain;
    app.world_mut().resource_mut::<Scene>().no_ocean = false;
    app.world_mut().resource_mut::<OceanSettings>().spacing = 0.0;
    settle(&mut app);
    let world = app.world().resource::<OceanWorld>();
    assert!(world.status.contains("invalid"));
    assert_eq!(world.mesh, mesh);
    assert_eq!(
        *app.world().get::<Visibility>(entity).unwrap(),
        Visibility::Visible
    );
    app.world_mut().resource_mut::<OceanSettings>().spacing = 1.0;
    settle(&mut app);
    assert_eq!(app.world().resource::<OceanWorld>().status, "Ocean ready");
}

#[test]
fn pause_and_speed_preserve_integrated_animation_phase() {
    let (mut app, _) = fixture();
    settle(&mut app);
    app.world_mut().resource_mut::<OceanSettings>().paused = true;
    app.update();
    let phase = app.world().resource::<OceanWorld>().seconds;
    let sources = app
        .world()
        .resource::<OceanWorld>()
        .sources
        .as_ref()
        .unwrap()
        .packed();
    app.update();
    assert_eq!(
        app.world().resource::<OceanWorld>().seconds.to_bits(),
        phase.to_bits()
    );
    assert_eq!(
        app.world()
            .resource::<OceanWorld>()
            .sources
            .as_ref()
            .unwrap()
            .packed(),
        sources
    );
    {
        let mut settings = app.world_mut().resource_mut::<OceanSettings>();
        settings.paused = false;
        settings.speed = 2.0;
    }
    app.update();
    assert!((app.world().resource::<OceanWorld>().seconds - phase - 0.04).abs() < 1e-5);
    assert_ne!(
        app.world()
            .resource::<OceanWorld>()
            .sources
            .as_ref()
            .unwrap()
            .packed(),
        sources
    );
}

#[test]
fn planet_seed_replaces_wave_sources_without_rebuilding_the_ocean_mesh() {
    let (mut app, _) = fixture();
    settle(&mut app);
    app.world_mut().resource_mut::<OceanSettings>().paused = true;
    app.update();
    let mesh = app.world().resource::<OceanWorld>().mesh.clone();
    app.world_mut().resource_mut::<Scene>().seed = 43;
    app.update();
    let world = app.world().resource::<OceanWorld>();
    let expected = sources::WaveSources::new(43).packed();
    assert_eq!(world.sources.as_ref().unwrap().packed(), expected);
    assert_eq!(world.mesh, mesh);
    let material = world.material.as_ref().unwrap();
    let uniforms = &app
        .world()
        .resource::<Assets<OceanMaterial>>()
        .get(material)
        .unwrap()
        .extension
        .settings;
    assert_eq!(uniforms.sources, expected);
}
