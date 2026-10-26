//! Streamed, depth-clipped waterfall plunge mist. Proxy cubes never write depth.
use super::WaterSettings;
use crate::{
    Orbit, Scene,
    command::{GenerationMode, RiverDisplay},
};
use bevy::{
    light::NotShadowCaster,
    mesh::MeshVertexBufferLayoutRef,
    pbr::{Material, MaterialPipeline, MaterialPipelineKey},
    prelude::*,
    render::render_resource::{
        AsBindGroup, CompareFunction, Face, RenderPipelineDescriptor, ShaderType,
        SpecializedMeshPipelineError,
    },
    shader::ShaderRef,
};
#[derive(Clone, Copy, Debug, Default, Reflect, ShaderType)]
pub struct MistUniform {
    pub shape: Vec4,
    pub timing: Vec4,
}
#[derive(Asset, AsBindGroup, Clone, Debug, Reflect)]
pub struct MistMaterial {
    #[uniform(0)]
    pub settings: MistUniform,
    #[texture(1, dimension = "3d")]
    #[sampler(2)]
    pub noise: Handle<Image>,
}
impl Material for MistMaterial {
    fn fragment_shader() -> ShaderRef {
        "embedded://planet_bevy/water/mist.wgsl".into()
    }
    fn alpha_mode(&self) -> AlphaMode {
        AlphaMode::Blend
    }
    fn enable_prepass() -> bool {
        false
    }
    fn enable_shadows() -> bool {
        false
    }
    fn specialize(
        _pipeline: &MaterialPipeline,
        descriptor: &mut RenderPipelineDescriptor,
        _layout: &MeshVertexBufferLayoutRef,
        _key: MaterialPipelineKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        descriptor.primitive.cull_mode = Some(Face::Front);
        if let Some(depth) = &mut descriptor.depth_stencil {
            depth.depth_compare = Some(CompareFunction::Always);
            depth.depth_write_enabled = Some(false);
        }
        Ok(())
    }
}
#[derive(Resource, Default)]
struct MistWorld {
    key: usize,
    indices: Vec<usize>,
    entities: Vec<Entity>,
    materials: Vec<Handle<MistMaterial>>,
    mesh: Option<Handle<Mesh>>,
    seconds: f32,
}
pub struct MistPlugin;
impl Plugin for MistPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(MaterialPlugin::<MistMaterial>::default())
            .init_resource::<MistWorld>()
            .add_systems(Update, update.after(super::update));
    }
}
#[allow(clippy::too_many_arguments, clippy::cast_possible_truncation)]
fn update(
    mut commands: Commands,
    scene: Res<Scene>,
    settings: Res<WaterSettings>,
    time: Res<Time>,
    ocean: Res<crate::ocean::OceanSettings>,
    textures: Res<crate::terrain_material::TerrainTextures>,
    chunks: Res<crate::streaming::ChunkWorld>,
    cameras: Query<&Transform, With<Orbit>>,
    mut explorer: ResMut<crate::explore::Explorer>,
    mut world: ResMut<MistWorld>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<MistMaterial>>,
) {
    let Ok(camera) = cameras.single() else {
        return;
    };
    if !ocean.paused {
        world.seconds += time.delta_secs() * ocean.speed;
    }
    let key = scene.terrain.elevations.as_ptr() as usize;
    if world.key != key {
        clear(&mut world, &mut commands, &mut materials);
        world.key = key;
    }
    if world.mesh.is_none() {
        world.mesh = Some(meshes.add(Cuboid::new(1.0, 1.0, 1.0)));
    }
    for handle in &world.materials {
        if let Some(mut material) = materials.get_mut(handle) {
            material.settings.timing.x = world.seconds;
            material.settings.shape.y = settings.mist * 1.45;
        }
    }
    if scene.generation != GenerationMode::Terrain
        || scene.river_display != RiverDisplay::Show
        || settings.mist <= 0.0
    {
        clear(&mut world, &mut commands, &mut materials);
        return;
    }
    let locator =
        planet_rs::chunks::CellLocator::new(chunks.partition_level()).expect("valid chunk level");
    let radius = scene.terrain.topology.radius as f32;
    let mut feet: Vec<_> = scene
        .terrain
        .waterfalls
        .iter()
        .enumerate()
        .filter_map(|(i, fall)| {
            let centre = impact_position(fall)?;
            let up = centre.normalize();
            let flow = crate::surface::vector(fall.direction);
            let distance = centre.distance(camera.translation) * radius;
            (distance < 180.0
                && locator
                    .owner(fall.lip)
                    .is_some_and(|id| chunks.fine_visible(id)))
            .then_some((i, centre, up, flow, distance))
        })
        .collect();
    feet.sort_unstable_by(|a, b| a.4.total_cmp(&b.4));
    feet.truncate(32);
    let mut indices: Vec<_> = feet.iter().map(|f| f.0).collect();
    indices.sort_unstable();
    if indices == world.indices {
        return;
    }
    clear(&mut world, &mut commands, &mut materials);
    world.indices = indices;
    let ground = explorer.terrain_ground(&scene);
    for (i, foot, up, flow, _) in feet.into_iter().take(32) {
        let fall = &scene.terrain.waterfalls[i];
        let width = (fall.half_width as f32 * radius * 2.0).max(1.0);
        let drop = ((fall.upper_surface - fall.lower_surface) as f32 * radius).max(0.1);
        let length = (width * 1.1 + drop * 0.2).clamp(2.0, 18.0);
        let height = (drop * 0.18 + width * 0.35).clamp(0.7, 6.0);
        let right = up.cross(flow).normalize();
        let forward = right.cross(up).normalize();
        let rotation = Quat::from_mat3(&Mat3::from_cols(right, up, forward));
        // Keep the impact inside the dense part of the volume, overlapping the
        // face rather than putting the shader's feathered upstream edge there.
        let downstream = forward * (length * 0.30) / radius;
        let support = |point: Vec3| {
            ground
                .water_support(&scene.terrain, point.normalize())
                .map(|(terrain, water)| terrain.max(water.unwrap_or(terrain)))
        };
        let foot_radius = support(foot).unwrap_or(foot.length()).max(1.0);
        let centre_radius = support(foot + downstream).unwrap_or(foot_radius).max(1.0);
        let centre = mist_centre(foot, downstream, height, radius, foot_radius, centre_radius);
        let handle = materials.add(MistMaterial {
            settings: MistUniform {
                shape: Vec4::new(radius, settings.mist * 1.45, width, height),
                timing: Vec4::new(world.seconds, length, 0.0, 0.0),
            },
            noise: textures.noise.clone(),
        });
        let entity = commands
            .spawn((
                Mesh3d(world.mesh.as_ref().unwrap().clone()),
                MeshMaterial3d(handle.clone()),
                Transform::from_translation(centre)
                    .with_rotation(rotation)
                    .with_scale(Vec3::new(width * 1.25, height, length) / radius),
                NotShadowCaster,
            ))
            .id();
        world.entities.push(entity);
        world.materials.push(handle);
    }
}

fn mist_centre(
    planned_foot: Vec3,
    downstream: Vec3,
    height: f32,
    radius: f32,
    foot_surface: f32,
    downstream_surface: f32,
) -> Vec3 {
    let up = planned_foot.normalize();
    // Planned terrace levels precede final bank grading and water binding.
    // Raise the dense base above both the rendered impact and the landing,
    // including the small curvature rise away from this tangent plane.
    let centre_up = (planned_foot + downstream).normalize();
    let landing = (centre_up * downstream_surface).dot(up);
    let base = foot_surface.max(landing);
    up * (base + (height * 0.5 + 0.08) / radius) + downstream
}

fn impact_position(fall: &planet_rs::terrain::Waterfall) -> Option<Vec3> {
    if fall.upper_surface <= 0.0 {
        return None;
    }
    // Despite its name, `lip` is the downstream end of the shaped face:
    // surface_at(lip) == lower_surface; the crest is face_run upstream.
    // A submerged foot hits the ocean earlier. Invert the same smoothstep
    // profile used by the water mesh to locate that sea-level intersection.
    let direction = if fall.lower_surface < 0.0 {
        let target = fall.upper_surface / (fall.upper_surface - fall.lower_surface);
        let (mut lo, mut hi) = (0.0, 1.0);
        for _ in 0..24 {
            let progress = (lo + hi) * 0.5;
            if progress * progress * (3.0 - 2.0 * progress) < target {
                lo = progress;
            } else {
                hi = progress;
            }
        }
        // The profile measures the tangent projection, so use a sine rather
        // than normalizing lip + tangent (which would shorten the face run).
        let along = -fall.face_run * (1.0 - (lo + hi) * 0.5);
        fall.lip * (1.0 - along * along).sqrt() + fall.direction * along
    } else {
        fall.lip
    };
    Some(crate::surface::vector(
        direction * (1.0 + fall.lower_surface.max(0.0)),
    ))
}

fn clear(world: &mut MistWorld, commands: &mut Commands, materials: &mut Assets<MistMaterial>) {
    for entity in world.entities.drain(..) {
        commands.entity(entity).despawn();
    }
    for material in world.materials.drain(..) {
        materials.remove(material.id());
    }
    world.indices.clear();
}

#[cfg(test)]
mod tests {
    use super::*;
    use planet_rs::{DVec3, terrain::Waterfall};

    fn waterfall(lip: DVec3, direction: DVec3) -> Waterfall {
        Waterfall {
            upper_vertex: 0,
            lower_vertex: 1,
            upstream_vertex: 2,
            lip,
            direction,
            half_width: 0.003,
            upper_surface: 0.025,
            lower_surface: 0.020,
            face_run: 0.0025,
        }
    }

    #[test]
    fn inland_mist_hits_the_foot_without_an_extra_face_run() {
        for (up, flow) in [
            (DVec3::X, DVec3::Y),
            (-DVec3::Y, DVec3::Z),
            (DVec3::Z, -DVec3::X),
        ] {
            let fall = waterfall(up, flow);
            let impact = impact_position(&fall).unwrap();
            let foot = crate::surface::vector(up * 1.020);
            assert!(impact.distance(foot) * 1000.0 < 0.001);
        }
    }

    #[test]
    fn sea_mist_hits_the_shaped_face_at_sea_level() {
        let mut fall = waterfall(DVec3::Y, DVec3::X);
        fall.lower_surface = -0.010;
        let impact = impact_position(&fall).unwrap();
        assert!((impact.length() - 1.0).abs() < 1.0e-6);
        let along = f64::from(impact.x);
        assert!((-fall.face_run..0.0).contains(&along));
        let progress = (along + fall.face_run) / fall.face_run;
        let surface = fall.upper_surface
            + (fall.lower_surface - fall.upper_surface)
                * progress
                * progress
                * (3.0 - 2.0 * progress);
        assert!(surface.abs() * 1000.0 < 0.001);
    }

    #[test]
    fn submerged_waterfalls_have_no_airborne_mist() {
        let mut fall = waterfall(DVec3::Y, DVec3::X);
        fall.upper_surface = -0.001;
        fall.lower_surface = -0.010;
        assert!(impact_position(&fall).is_none());
    }

    #[test]
    fn dense_mist_base_clears_final_raised_water_and_landing() {
        let height = 2.0;
        let radius = 1000.0;
        for (up, flow) in [(Vec3::Y, Vec3::Z), (-Vec3::Z, Vec3::X)] {
            let planned = up * 1.020;
            let downstream = flow * 0.003;
            // Final support half a metre above the original terrace, with a
            // further rise across the mist footprint.
            let foot_surface = 1.02055;
            let landing_surface = 1.021;
            let centre = mist_centre(
                planned,
                downstream,
                height,
                radius,
                foot_surface,
                landing_surface,
            );
            let base = centre - up * (height * 0.5 / radius);
            let landing = (planned + downstream).normalize() * landing_surface;
            assert!((base.dot(up) - foot_surface) * radius >= 0.079);
            assert!((base - landing).dot(up) * radius >= 0.079);
            // Keep the repaired contact overlap; never add another face run.
            assert!((centre.dot(flow) - downstream.dot(flow)).abs() < 1.0e-7);
        }
    }
}
