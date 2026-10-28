//! Player-centred, globally closed spherical ocean and fixed coastal cube maps.
mod cache;
mod foam;
mod sources;
pub(crate) use foam::OceanFrame;
#[cfg(test)]
mod tests;
use crate::{Orbit, Scene, command::GenerationMode, terrain_material::TerrainTextures};
use bevy::{
    asset::{RenderAssetUsages, embedded_asset},
    camera::primitives::Aabb,
    image::{ImageAddressMode, ImageFilterMode, ImageSampler, ImageSamplerDescriptor},
    light::NotShadowCaster,
    mesh::{Indices, PrimitiveTopology},
    pbr::{ExtendedMaterial, MaterialExtension},
    prelude::*,
    render::render_resource::{
        AsBindGroup, Extent3d, ShaderType, TextureDimension, TextureViewDescriptor,
        TextureViewDimension,
    },
    render::storage::ShaderBuffer,
    shader::ShaderRef,
    tasks::{AsyncComputeTaskPool, Task, block_on, poll_once},
};
pub(crate) use cache::prepare as prepare_maps;
use planet_rs::ocean_surface::{CoastMap, OceanGrid, OceanGridOptions};

#[derive(Resource, Clone, Copy, Debug, PartialEq)]
pub struct OceanSettings {
    pub waves: bool,
    pub amplitude: f32,
    pub speed: f32,
    pub source_drift: f32,
    pub choppiness: f32,
    pub foam: f32,
    pub roughness: f32,
    pub refraction: f32,
    pub fine_radius: f32,
    pub spacing: f32,
    pub debug: u32,
    pub paused: bool,
}
impl Default for OceanSettings {
    fn default() -> Self {
        Self {
            waves: true,
            amplitude: 1.0,
            speed: 1.0,
            source_drift: 0.35,
            choppiness: 0.18,
            foam: 0.85,
            roughness: 0.16,
            refraction: 0.8,
            fine_radius: 256.0,
            spacing: 1.0,
            debug: 0,
            paused: false,
        }
    }
}
impl OceanSettings {
    pub fn validate(&self) -> Result<(), String> {
        if !self.amplitude.is_finite()
            || !(0.0..=3.0).contains(&self.amplitude)
            || !self.speed.is_finite()
            || !(0.0..=4.0).contains(&self.speed)
            || !self.source_drift.is_finite()
            || !(0.0..=2.0).contains(&self.source_drift)
            || !self.refraction.is_finite()
            || !(0.0..=1.0).contains(&self.refraction)
            || self.debug > 3
        {
            return Err(
                "ocean amplitude must be 0..3, speed 0..4, source drift 0..2, refraction 0..1 and a valid diagnostic"
                    .into(),
            );
        }
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, Default, Reflect, ShaderType)]
pub struct OceanUniform {
    pub shape: Vec4,
    pub waves: Vec4,
    pub shading: Vec4,
    pub viewer: Vec4,
    pub sources: [Vec4; 4],
}
#[derive(Asset, AsBindGroup, Clone, Debug, Reflect)]
pub struct OceanExtension {
    #[uniform(100)]
    pub settings: OceanUniform,
    #[texture(101, dimension = "cube")]
    #[sampler(102)]
    pub coast: Handle<Image>,
    #[texture(103, dimension = "3d")]
    #[sampler(104)]
    pub noise: Handle<Image>,
    #[uniform(105)]
    pub optics: crate::water::WaterUniform,
    #[texture(106)]
    #[sampler(107)]
    pub foam: Handle<Image>,
    #[uniform(108)]
    pub foam_frame: foam::FoamFrame,
    #[uniform(109)]
    pub impacts: crate::water::ImpactUniform,
    #[storage(123, read_only)]
    pub environment: Handle<ShaderBuffer>,
    #[texture(126, dimension = "cube")]
    pub environment_sky: Handle<Image>,
    #[texture(127, dimension = "cube")]
    pub environment_shadows: Handle<Image>,
}
pub type OceanMaterial = ExtendedMaterial<StandardMaterial, OceanExtension>;
impl MaterialExtension for OceanExtension {
    fn vertex_shader() -> ShaderRef {
        "embedded://planet_bevy/ocean/ocean.wgsl".into()
    }
    fn fragment_shader() -> ShaderRef {
        Self::vertex_shader()
    }
    fn enable_prepass() -> bool {
        false
    }
    fn enable_shadows() -> bool {
        false
    }
}
pub struct OceanPlugin;
impl Plugin for OceanPlugin {
    fn build(&self, app: &mut App) {
        bevy::shader::load_shader_library!(app, "waves.wgsl");
        bevy::shader::load_shader_library!(app, "impacts.wgsl");
        embedded_asset!(app, "ocean.wgsl");
        app.add_plugins(MaterialPlugin::<OceanMaterial>::default())
            .add_plugins(foam::FoamPlugin)
            .init_resource::<OceanWorld>()
            .init_resource::<OceanSettings>();
    }
}
#[derive(Component)]
pub struct OceanSurface;
#[derive(Resource, Default)]
pub struct OceanWorld {
    pending: Option<CoastMap>,
    map: Option<Handle<Image>>,
    material: Option<Handle<OceanMaterial>>,
    mesh: Option<Handle<Mesh>>,
    entity: Option<Entity>,
    mesh_options: Option<(u64, OceanGridOptions)>,
    job: Option<Task<Result<OceanGrid, String>>>,
    job_options: Option<(u64, OceanGridOptions)>,
    seconds: f32,
    sources: Option<sources::WaveSources>,
    pub vertices: usize,
    pub triangles: usize,
    pub rings: usize,
    pub map_size: u32,
    pub status: String,
}
impl OceanWorld {
    pub fn install_maps(&mut self, map: CoastMap) {
        self.pending = Some(map);
    }
    pub fn settled(&self) -> bool {
        self.pending.is_none() && self.job.is_none()
    }
}
fn cube_image(map: CoastMap) -> Image {
    let mut image = Image::new(
        Extent3d {
            width: map.size,
            height: map.size,
            depth_or_array_layers: 6,
        },
        TextureDimension::D2,
        map.pixels,
        bevy::render::render_resource::TextureFormat::Rgba8Unorm,
        RenderAssetUsages::RENDER_WORLD,
    );
    image.texture_view_descriptor = Some(TextureViewDescriptor {
        dimension: Some(TextureViewDimension::Cube),
        ..default()
    });
    image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::ClampToEdge,
        address_mode_v: ImageAddressMode::ClampToEdge,
        address_mode_w: ImageAddressMode::ClampToEdge,
        mag_filter: ImageFilterMode::Linear,
        min_filter: ImageFilterMode::Linear,
        ..default()
    });
    image
}
fn mesh(grid: OceanGrid) -> Mesh {
    let normals: Vec<_> = grid
        .positions
        .iter()
        .map(|p| p.as_vec3().to_array())
        .collect();
    let positions: Vec<_> = grid
        .positions
        .iter()
        .map(|p| p.as_vec3().to_array())
        .collect();
    let mut result = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD,
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
    .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, grid.sampling);
    result.insert_indices(Indices::U32(grid.triangles.into_iter().flatten().collect()));
    result
}
#[allow(
    clippy::too_many_arguments,
    clippy::too_many_lines,
    clippy::cast_possible_truncation
)]
pub fn update(
    mut commands: Commands,
    time: Res<Time>,
    scene: Res<Scene>,
    settings: Res<OceanSettings>,
    textures: Res<TerrainTextures>,
    mut world: ResMut<OceanWorld>,
    cameras: Query<&Transform, (With<Orbit>, Without<OceanSurface>)>,
    mut surfaces: Query<(&mut Transform, &mut Visibility), With<OceanSurface>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<OceanMaterial>>,
) {
    let Ok(camera) = cameras.single() else {
        return;
    };
    if world
        .sources
        .as_ref()
        .is_none_or(|sources| sources.seed != scene.seed)
    {
        world.sources = Some(sources::WaveSources::new(scene.seed));
    }
    if !settings.paused {
        let seconds = time.delta_secs_f64() * f64::from(settings.speed);
        world.seconds += seconds as f32;
        world.sources.as_mut().unwrap().advance(
            seconds,
            f64::from(settings.source_drift),
            scene.terrain.topology.radius,
        );
    }
    if let Some(map) = world.pending.take() {
        world.map_size = map.size;
        let handle = images.add(cube_image(map));
        if let Some(material) = world.material.as_ref().and_then(|h| materials.get_mut(h)) {
            let mut material = material;
            material.extension.coast = handle.clone();
        }
        if let Some(old) = world.map.replace(handle) {
            images.remove(old.id());
        }
    }
    if scene.generation != GenerationMode::Terrain || world.map.is_none() {
        world.job = None;
        world.job_options = None;
        if let Some(entity) = world.entity
            && let Ok((_, mut visible)) = surfaces.get_mut(entity)
        {
            *visible = Visibility::Hidden;
        }
        return;
    }
    let radius = scene.terrain.topology.radius;
    let options = OceanGridOptions {
        spacing: f64::from(settings.spacing),
        fine_radius: f64::from(settings.fine_radius),
        rows_per_level: 8,
    };
    let key = (radius.to_bits(), options);
    if world.mesh_options != Some(key) && world.job.is_none() {
        world.job = Some(
            AsyncComputeTaskPool::get().spawn(async move { OceanGrid::build(radius, options) }),
        );
        world.job_options = Some(key);
        world.status = "Building spherical ocean rings…".into();
    }
    if let Some(job) = &mut world.job
        && let Some(result) = block_on(poll_once(job))
    {
        world.job = None;
        let matching = world.job_options == Some(key);
        world.job_options = None;
        if matching {
            match result {
                Ok(grid) => {
                    world.vertices = grid.positions.len();
                    world.triangles = grid.triangles.len();
                    world.rings = grid.rings;
                    let handle = meshes.add(mesh(grid));
                    if let Some(entity) = world.entity {
                        commands.entity(entity).insert(Mesh3d(handle.clone()));
                    }
                    if let Some(old) = world.mesh.replace(handle) {
                        meshes.remove(old.id());
                    }
                    world.mesh_options = Some(key);
                    world.status = "Ocean ready".into();
                }
                Err(error) => {
                    world.mesh_options = Some(key);
                    world.status = error;
                }
            }
        }
    }
    if world.material.is_none() {
        world.material = Some(materials.add(OceanMaterial {
            base: StandardMaterial {
                base_color: Color::srgb(0.03, 0.28, 0.55),
                perceptual_roughness: 0.16,
                reflectance: 0.35,
                specular_transmission: 0.8,
                ior: 1.333,
                thickness: 0.002,
                attenuation_distance: 5.0 / radius as f32,
                attenuation_color: Color::linear_rgb(0.1054, 0.5488, 0.7596),
                cull_mode: None,
                double_sided: true,
                ..default()
            },
            extension: OceanExtension {
                settings: default(),
                coast: world.map.as_ref().unwrap().clone(),
                noise: textures.noise.clone(),
                optics: default(),
                foam: default(),
                foam_frame: default(),
                impacts: default(),
                environment: textures.environment.clone(),
                environment_sky: textures.environment_sky.clone(),
                environment_shadows: textures.environment_shadows.clone(),
            },
        }));
    }
    if world.entity.is_none()
        && let Some(mesh) = &world.mesh
    {
        world.entity = Some(
            commands
                .spawn((
                    OceanSurface,
                    Mesh3d(mesh.clone()),
                    MeshMaterial3d(world.material.as_ref().unwrap().clone()),
                    Transform::from_rotation(Quat::from_rotation_arc(
                        Vec3::Y,
                        camera.translation.normalize_or_zero(),
                    )),
                    Visibility::Visible,
                    NotShadowCaster,
                    Aabb::from_min_max(Vec3::splat(-1.02), Vec3::splat(1.02)),
                ))
                .id(),
        );
    }
    let anchor = camera.translation.normalize_or_zero();
    let frame = Quat::from_rotation_arc(Vec3::Y, anchor);
    if let Some(entity) = world.entity
        && let Ok((mut transform, mut visible)) = surfaces.get_mut(entity)
    {
        if transform.rotation != frame {
            transform.rotation = frame;
        }
        *visible = if scene.no_ocean {
            Visibility::Hidden
        } else {
            Visibility::Visible
        };
    }
    if let Some(handle) = &world.material
        && let Some(mut material) = materials.get_mut(handle)
    {
        material.extension.settings = OceanUniform {
            shape: Vec4::new(
                radius as f32,
                settings.fine_radius * 0.75,
                settings.fine_radius,
                if settings.waves { 1.0 } else { 0.0 },
            ),
            waves: Vec4::new(
                settings.amplitude,
                settings.speed,
                settings.choppiness,
                settings.foam,
            ),
            shading: Vec4::new(
                settings.roughness,
                settings.refraction,
                f32::from(u16::try_from(settings.debug).expect("bounded diagnostic")),
                f32::from(u16::try_from(world.map_size).expect("bounded map size")),
            ),
            viewer: anchor.extend(world.seconds),
            sources: world.sources.as_ref().unwrap().packed(),
        };
        material.base.specular_transmission = settings.refraction;
        material.base.attenuation_distance = 5.0 / radius as f32;
    }
}

#[allow(clippy::cast_possible_truncation)]
pub fn inspection_orbit(
    view: crate::command::View,
    terrain: &planet_rs::PlanetTerrain,
) -> Option<Orbit> {
    use crate::command::View;
    if !matches!(view, View::Ocean | View::Shore | View::Underwater) {
        return None;
    }
    let light = planet_rs::DVec3::new(-3.0, 4.0, 5.0).normalize();
    let target = if view == View::Shore { 0.55 } else { 0.32 };
    let i = (0..terrain.elevations.len())
        .filter(|&i| {
            let depth = -terrain.elevations[i] * terrain.topology.radius;
            if view == View::Shore {
                (0.5..=4.0).contains(&depth)
            } else if view == View::Underwater {
                (4.0..=8.0).contains(&depth)
            } else {
                (12.0..=35.0).contains(&depth)
            }
        })
        .min_by(|&a, &b| {
            (terrain.topology.directions[a].dot(light) - target)
                .abs()
                .total_cmp(&(terrain.topology.directions[b].dot(light) - target).abs())
        })?;
    let up = crate::surface::vector(terrain.topology.directions[i]);
    let frame = Quat::from_rotation_arc(Vec3::Y, up);
    let direction = if view == View::Shore {
        let normal = crate::surface::vector(terrain.normals[i]);
        frame.inverse() * (normal - up * normal.dot(up))
    } else {
        frame.inverse() * -crate::surface::vector(light)
    };
    let mut orbit = Orbit {
        focus: up,
        frame,
        yaw: direction.x.atan2(direction.z) + if view == View::Ocean { 0.2 } else { 0.0 },
        pitch: if view == View::Shore {
            0.35
        } else if view == View::Underwater {
            0.65
        } else {
            0.18
        },
        distance: if view == View::Shore { 20.0 } else { 10.0 } / terrain.topology.radius as f32,
        minimum: 0.5 / terrain.topology.radius as f32,
        maximum: 0.5,
    };
    if view == View::Underwater {
        // Keep the eye directly above the sampled seabed vertex; a horizontal
        // camera offset can otherwise place a shallow-water inspection in land.
        orbit.focus += up * (1.0 - 1.0 / terrain.topology.radius as f32)
            - crate::orbit_transform(&orbit).translation;
    }
    Some(orbit)
}
