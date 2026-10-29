//! Viewer-centred foam history, reprojected in fixed spherical coordinates.
use super::{OceanMaterial, OceanSettings, OceanUniform, OceanWorld, Orbit};
use crate::environment::EnvironmentUniform;
use bevy::asset::{RenderAssetUsages, embedded_asset};
use bevy::image::{ImageFilterMode, ImageSampler, ImageSamplerDescriptor};
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, ShaderType, TextureDimension};
use bevy::{
    core_pipeline::{Core3d, Core3dSystems},
    render::{
        Render, RenderApp, RenderStartup, RenderSystems,
        extract_resource::{ExtractResource, ExtractResourcePlugin},
        render_asset::RenderAssets,
        render_resource::SamplerDescriptor,
        render_resource::{
            BindGroup, BindGroupEntries, BindGroupLayoutDescriptor, BindGroupLayoutEntries,
            CachedComputePipelineId, ComputePipelineDescriptor, PipelineCache, SamplerBindingType,
            ShaderStages, StorageTextureAccess, TextureFormat, TextureSampleType, TextureUsages,
            UniformBuffer,
            binding_types::{
                sampler, storage_buffer_read_only, texture_2d, texture_3d, texture_cube,
                texture_cube_array, texture_storage_2d, uniform_buffer,
            },
        },
        renderer::{RenderContext, RenderDevice, RenderQueue},
        storage::{GpuShaderBuffer, ShaderBuffer},
        texture::GpuImage,
    },
    shader::ShaderDefVal,
};

const SIZE: u32 = 512;
#[derive(Clone, Copy, Debug, Default, Reflect, ShaderType)]
pub struct FoamFrame {
    pub origin: Vec4,
    pub tangent: Vec4,
    pub bitangent: Vec4,
}
impl FoamFrame {
    fn at(anchor: Vec3, radius: f32) -> Self {
        let q = Quat::from_rotation_arc(Vec3::Y, anchor);
        Self {
            origin: anchor.extend(128.0),
            tangent: (q * Vec3::X).extend(radius),
            bitangent: (q * Vec3::Z).extend(0.0),
        }
    }
}
#[derive(Clone, Copy, Default, ShaderType)]
struct Step {
    current: FoamFrame,
    previous: FoamFrame,
    parameters: Vec4,
}
#[derive(Resource, Clone, Default, ExtractResource)]
pub(crate) struct OceanFrame {
    active: bool,
    pub ocean: OceanUniform,
    pub coast: Handle<Image>,
    pub noise: Handle<Image>,
    pub environment: Handle<ShaderBuffer>,
    pub weather: Handle<Image>,
    previous: Handle<Image>,
    next: Handle<Image>,
    step: Step,
}
#[derive(Resource, Default)]
struct History {
    images: Option<[Handle<Image>; 2]>,
    frame: FoamFrame,
    key: Option<bevy::asset::AssetId<Image>>,
    index: usize,
}
pub struct FoamPlugin;
impl Plugin for FoamPlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "foam.wgsl");
        app.init_resource::<History>()
            .init_resource::<OceanFrame>()
            .add_plugins(ExtractResourcePlugin::<OceanFrame>::default())
            .add_systems(
                Update,
                update.after(super::update).before(crate::water::update),
            );
        if let Some(render) = app.get_sub_app_mut(RenderApp) {
            render
                .add_systems(RenderStartup, initialize)
                .add_systems(Render, prepare.in_set(RenderSystems::PrepareBindGroups))
                .add_systems(Core3d, compute.before(Core3dSystems::MainPass));
        }
    }
}
fn texture() -> Image {
    let mut image = Image::new_fill(
        Extent3d {
            width: SIZE,
            height: SIZE,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        &[0; 8],
        TextureFormat::Rgba16Float,
        RenderAssetUsages::RENDER_WORLD,
    );
    image.texture_descriptor.usage =
        TextureUsages::TEXTURE_BINDING | TextureUsages::STORAGE_BINDING | TextureUsages::COPY_DST;
    image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        mag_filter: ImageFilterMode::Linear,
        min_filter: ImageFilterMode::Linear,
        ..default()
    });
    image
}
#[allow(clippy::too_many_arguments, clippy::cast_possible_truncation)]
fn update(
    time: Res<Time>,
    settings: Res<crate::water::WaterSettings>,
    ocean_settings: Res<OceanSettings>,
    world: Res<OceanWorld>,
    cameras: Query<&Transform, With<Orbit>>,
    mut history: ResMut<History>,
    mut record: ResMut<OceanFrame>,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<OceanMaterial>>,
    scene: Res<crate::Scene>,
    weather: Option<Res<crate::weather::WeatherControls>>,
    environment: Option<Res<crate::environment::EnvironmentSettings>>,
    solar: Option<Res<crate::solar::SolarFrame>>,
) {
    let (Some(handle), Ok(camera)) = (&world.material, cameras.single()) else {
        return;
    };
    let Some(mut material) = materials.get_mut(handle) else {
        return;
    };
    let altitude = (camera.translation.length() - 1.0) * material.extension.settings.shape.x;
    if altitude > 200.0
        || scene.no_ocean
        || scene.generation != crate::command::GenerationMode::Terrain
    {
        history.key = None;
        record.active = false;
        material.extension.foam_frame = default();
        return;
    }
    let pair = history
        .images
        .get_or_insert_with(|| [images.add(texture()), images.add(texture())])
        .clone();
    let frame = FoamFrame::at(
        camera.translation.normalize(),
        material.extension.settings.shape.x,
    );
    let reset = history.key != Some(material.extension.coast.id());
    let weather_paused = weather
        .as_ref()
        .is_some_and(|weather| weather.enabled && weather.paused)
        || environment
            .as_ref()
            .is_some_and(|environment| environment.paused);
    let delta =
        if ocean_settings.paused || weather_paused || solar.as_ref().is_some_and(|s| s.paused) {
            0.0
        } else {
            time.delta_secs() * ocean_settings.speed
        };
    let previous = history.index;
    let next = 1 - previous;
    record.ocean = material.extension.settings;
    record.active = true;
    record.coast = material.extension.coast.clone();
    record.noise = material.extension.noise.clone();
    record.environment = material.extension.environment.clone();
    record.weather = material.extension.global_weather.clone();
    record.previous = pair[previous].clone();
    record.next = pair[next].clone();
    record.step = Step {
        current: frame,
        previous: history.frame,
        parameters: Vec4::new(
            delta,
            settings.foam_lifetime,
            2.0,
            if reset { 1.0 } else { 0.0 },
        ),
    };
    material.extension.foam = pair[next].clone();
    material.extension.foam_frame = frame;
    history.index = next;
    history.frame = frame;
    history.key = Some(material.extension.coast.id());
}
#[derive(Resource)]
struct Pipeline {
    layout: BindGroupLayoutDescriptor,
    id: CachedComputePipelineId,
    sampler: bevy::render::render_resource::Sampler,
}
#[derive(Resource)]
struct Prepared {
    group: BindGroup,
}
fn initialize(
    mut commands: Commands,
    device: Res<RenderDevice>,
    server: Res<AssetServer>,
    cache: Res<PipelineCache>,
) {
    let entries = BindGroupLayoutEntries::with_indices(
        ShaderStages::COMPUTE,
        (
            (0, texture_2d(TextureSampleType::Float { filterable: true })),
            (1, sampler(SamplerBindingType::Filtering)),
            (
                2,
                texture_storage_2d(TextureFormat::Rgba16Float, StorageTextureAccess::WriteOnly),
            ),
            (3, uniform_buffer::<Step>(false)),
            (100, uniform_buffer::<OceanUniform>(false)),
            (
                101,
                texture_cube(TextureSampleType::Float { filterable: true }),
            ),
            (102, sampler(SamplerBindingType::Filtering)),
            (
                103,
                texture_3d(TextureSampleType::Float { filterable: true }),
            ),
            (104, sampler(SamplerBindingType::Filtering)),
            (123, storage_buffer_read_only::<EnvironmentUniform>(false)),
            (
                129,
                texture_cube_array(TextureSampleType::Float { filterable: true }),
            ),
        ),
    );
    let layout = BindGroupLayoutDescriptor::new("planet_foam_history", &entries);
    let id = cache.queue_compute_pipeline(ComputePipelineDescriptor {
        label: Some("planet_foam_history".into()),
        layout: vec![layout.clone()],
        shader: server.load("embedded://planet_bevy/ocean/foam.wgsl"),
        shader_defs: vec![ShaderDefVal::UInt("MATERIAL_BIND_GROUP".into(), 0)],
        entry_point: Some("update".into()),
        ..default()
    });
    commands.insert_resource(Pipeline {
        layout,
        id,
        sampler: device.create_sampler(&SamplerDescriptor {
            mag_filter: bevy::render::render_resource::FilterMode::Linear,
            min_filter: bevy::render::render_resource::FilterMode::Linear,
            ..default()
        }),
    });
}
#[allow(clippy::too_many_arguments)] // Shared material buffers and images are borrowed from render assets.
fn prepare(
    mut commands: Commands,
    record: Res<OceanFrame>,
    images: Res<RenderAssets<GpuImage>>,
    buffers: Res<RenderAssets<GpuShaderBuffer>>,
    pipeline: Res<Pipeline>,
    device: Res<RenderDevice>,
    queue: Res<RenderQueue>,
    cache: Res<PipelineCache>,
) {
    if !record.active || record.next == Handle::default() {
        commands.remove_resource::<Prepared>();
        return;
    }
    let (Some(previous), Some(next), Some(coast), Some(noise), Some(weather), Some(environment)) = (
        images.get(&record.previous),
        images.get(&record.next),
        images.get(&record.coast),
        images.get(&record.noise),
        images.get(&record.weather),
        buffers.get(&record.environment),
    ) else {
        commands.remove_resource::<Prepared>();
        return;
    };
    let mut settings = UniformBuffer::from(record.step);
    settings.write_buffer(&device, &queue);
    let mut ocean = UniformBuffer::from(record.ocean);
    ocean.write_buffer(&device, &queue);
    let group = device.create_bind_group(
        "planet_foam_history",
        &cache.get_bind_group_layout(&pipeline.layout),
        &BindGroupEntries::with_indices((
            (0, &previous.texture_view),
            (1, &pipeline.sampler),
            (2, &next.texture_view),
            (3, settings.binding().unwrap()),
            (100, ocean.binding().unwrap()),
            (101, &coast.texture_view),
            (102, &coast.sampler),
            (103, &noise.texture_view),
            (104, &noise.sampler),
            (123, environment.buffer.as_entire_binding()),
            (129, &weather.texture_view),
        )),
    );
    commands.insert_resource(Prepared { group });
}
fn compute(
    pipeline: Option<Res<Pipeline>>,
    prepared: Option<Res<Prepared>>,
    cache: Res<PipelineCache>,
    mut ctx: RenderContext,
) {
    let (Some(pipeline), Some(prepared)) = (pipeline, prepared) else {
        return;
    };
    let Some(compiled) = cache.get_compute_pipeline(pipeline.id) else {
        return;
    };
    let mut pass = ctx.command_encoder().begin_compute_pass(&default());
    pass.set_pipeline(compiled);
    pass.set_bind_group(0, &prepared.group, &[]);
    pass.dispatch_workgroups(SIZE / 8, SIZE / 8, 1);
}
