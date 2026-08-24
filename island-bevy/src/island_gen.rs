//! Off-thread island generation and the handoff resource every renderer reads.

use bevy::{
    prelude::*,
    tasks::{AsyncComputeTaskPool, Task, block_on, poll_once},
};
use motu::{Island, IslandOptions};

/// Parameters the generator runs with, inserted before the app starts.
#[derive(Resource, Clone, Copy, Debug)]
pub struct GenerationSettings {
    pub seed: u64,
    pub options: IslandOptions,
}

/// The finished island. Renderer plugins spawn their geometry on the frame this
/// resource is added.
#[derive(Resource)]
pub struct GeneratedIsland(pub Island);

#[derive(Component)]
struct GenerationTask(Task<Result<Island, String>>);

#[derive(Component)]
struct LoadingNotice;

pub struct IslandGenPlugin;

impl Plugin for IslandGenPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, (start_generation, spawn_loading_notice))
            .add_systems(PreUpdate, poll_generation);
    }
}

fn start_generation(mut commands: Commands, settings: Res<GenerationSettings>) {
    let seed = settings.seed;
    let options = settings.options;
    info!(
        "generating island: seed {seed}, terrain size {}",
        options.terrain_size
    );
    let task = AsyncComputeTaskPool::get().spawn(async move { Island::generate(seed, options) });
    commands.spawn((Name::new("Island generation"), GenerationTask(task)));
}

fn spawn_loading_notice(mut commands: Commands) {
    commands.spawn((
        Name::new("Loading notice"),
        LoadingNotice,
        Text::new("Generating island..."),
        TextFont::from_font_size(26.0),
        TextColor(Color::WHITE),
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(24.0),
            top: Val::Px(24.0),
            ..default()
        },
    ));
}

/// Generation runs in `PreUpdate` so the resource insertion is flushed before
/// the renderer plugins look for it in `Update`.
fn poll_generation(
    mut commands: Commands,
    mut tasks: Query<(Entity, &mut GenerationTask)>,
    notices: Query<Entity, With<LoadingNotice>>,
    mut exit: MessageWriter<AppExit>,
) {
    for (entity, mut task) in &mut tasks {
        let Some(result) = block_on(poll_once(&mut task.0)) else {
            continue;
        };
        commands.entity(entity).despawn();
        for notice in &notices {
            commands.entity(notice).despawn();
        }
        match result {
            Ok(island) => {
                info!(
                    "island ready: {} terrain vertices, {} rivers, {} trees, {} bushes",
                    island.lod(0).map_or(0, |mesh| mesh.vertices.len()),
                    island.rivers().len(),
                    island.decorations().trees().len(),
                    island.decorations().bushes().len()
                );
                commands.insert_resource(GeneratedIsland(island));
            }
            Err(error) => {
                error!("island generation failed: {error}");
                exit.write(AppExit::error());
            }
        }
    }
}
