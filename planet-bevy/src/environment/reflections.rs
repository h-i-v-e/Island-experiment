//! A persistent low-resolution reflection cube. One face is refreshed per frame,
//! so water fragments only sample one texture and cloud work stays bounded.
use super::settings::{EnvironmentFrame, EnvironmentUniform};
use bevy::{
    asset::{RenderAssetUsages, embedded_asset},
    core_pipeline::{Core3d, Core3dSystems},
    image::{ImageFilterMode, ImageSampler, ImageSamplerDescriptor},
    prelude::*,
    render::{
        Render, RenderApp, RenderStartup, RenderSystems,
        extract_resource::{ExtractResource, ExtractResourcePlugin},
        render_asset::RenderAssets,
        render_resource::{
            BindGroup, BindGroupEntries, BindGroupLayoutDescriptor, BindGroupLayoutEntries,
            CachedComputePipelineId, ComputePipelineDescriptor, Extent3d, FilterMode,
            PipelineCache, Sampler, SamplerBindingType, SamplerDescriptor, ShaderStages,
            ShaderType, StorageTextureAccess, TextureDimension, TextureFormat, TextureSampleType,
            TextureUsages, TextureViewDescriptor, TextureViewDimension, UniformBuffer,
            binding_types::{sampler, texture_3d, texture_storage_2d_array, uniform_buffer},
        },
        renderer::{RenderContext, RenderDevice, RenderQueue},
        texture::GpuImage,
    },
};

const SIZE: u32 = 64;
#[derive(Resource, Clone, Default, ExtractResource)]
pub struct SkyReflectionAssets {
    pub cube: Handle<Image>,
}
#[derive(Clone, Copy, Default, ShaderType)]
struct ReflectionStep {
    environment: EnvironmentUniform,
    face: UVec4,
}
pub struct SkyReflectionPlugin;
impl Plugin for SkyReflectionPlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "reflections.wgsl");
        app.init_resource::<SkyReflectionAssets>()
            .add_plugins(ExtractResourcePlugin::<SkyReflectionAssets>::default())
            .add_systems(Startup, setup.before(crate::terrain_material::setup));
        if let Some(render) = app.get_sub_app_mut(RenderApp) {
            render
                .init_resource::<ReflectionWorld>()
                .add_systems(RenderStartup, initialize)
                .add_systems(Render, prepare.in_set(RenderSystems::PrepareBindGroups))
                .add_systems(Core3d, compute.before(Core3dSystems::MainPass));
        }
    }
}
fn cube_image() -> Image {
    let mut image = Image::new_fill(
        Extent3d {
            width: SIZE,
            height: SIZE,
            depth_or_array_layers: 6,
        },
        TextureDimension::D2,
        &[0; 8],
        TextureFormat::Rgba16Float,
        RenderAssetUsages::RENDER_WORLD,
    );
    image.texture_descriptor.usage =
        TextureUsages::TEXTURE_BINDING | TextureUsages::STORAGE_BINDING | TextureUsages::COPY_DST;
    image.texture_view_descriptor = Some(TextureViewDescriptor {
        dimension: Some(TextureViewDimension::Cube),
        ..default()
    });
    image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        mag_filter: ImageFilterMode::Linear,
        min_filter: ImageFilterMode::Linear,
        ..default()
    });
    image
}
fn setup(mut assets: ResMut<SkyReflectionAssets>, mut images: ResMut<Assets<Image>>) {
    assets.cube = images.add(cube_image());
}
#[derive(Resource)]
struct Pipeline {
    layout: BindGroupLayoutDescriptor,
    id: CachedComputePipelineId,
    sampler: Sampler,
}
#[derive(Resource, Default)]
struct ReflectionWorld {
    settings: UniformBuffer<ReflectionStep>,
    group: Option<BindGroup>,
    key: Option<(AssetId<Image>, AssetId<Image>)>,
    face: u32,
    faces: u32,
    revision: Option<u64>,
    viewer: Vec3,
    reset_pending: bool,
}
fn initialize(
    mut commands: Commands,
    device: Res<RenderDevice>,
    server: Res<AssetServer>,
    cache: Res<PipelineCache>,
) {
    let layout = BindGroupLayoutDescriptor::new(
        "planet_sky_reflection",
        &BindGroupLayoutEntries::sequential(
            ShaderStages::COMPUTE,
            (
                texture_3d(TextureSampleType::Float { filterable: true }),
                sampler(SamplerBindingType::Filtering),
                texture_storage_2d_array(
                    TextureFormat::Rgba16Float,
                    StorageTextureAccess::WriteOnly,
                ),
                uniform_buffer::<ReflectionStep>(false),
            ),
        ),
    );
    let id = cache.queue_compute_pipeline(ComputePipelineDescriptor {
        label: Some("planet_sky_reflection".into()),
        layout: vec![layout.clone()],
        shader: server.load("embedded://planet_bevy/environment/reflections.wgsl"),
        entry_point: Some("update".into()),
        ..default()
    });
    commands.insert_resource(Pipeline {
        layout,
        id,
        sampler: device.create_sampler(&SamplerDescriptor {
            mag_filter: FilterMode::Linear,
            min_filter: FilterMode::Linear,
            ..default()
        }),
    });
}
#[allow(clippy::too_many_arguments)] // Render-world resources share one persistent cube/bind group.
fn prepare(
    frame: Res<EnvironmentFrame>,
    assets: Res<SkyReflectionAssets>,
    images: Res<RenderAssets<GpuImage>>,
    pipeline: Res<Pipeline>,
    device: Res<RenderDevice>,
    queue: Res<RenderQueue>,
    cache: Res<PipelineCache>,
    mut world: ResMut<ReflectionWorld>,
) {
    let (Some(cube), Some(noise)) = (images.get(&assets.cube), images.get(&frame.noise)) else {
        return;
    };
    let viewer = frame.uniform.viewer.truncate();
    let reset = world.revision != Some(frame.revision)
        || world.viewer.distance(viewer) * frame.uniform.shape.x > 40.0;
    world.reset_pending |= reset;
    let face = if world.reset_pending { 0 } else { world.face };
    world.faces = if world.reset_pending { 6 } else { 1 };
    let faces = world.faces;
    world.revision = Some(frame.revision);
    world.viewer = viewer;
    world.settings.set(ReflectionStep {
        environment: frame.uniform,
        face: UVec4::new(face, faces, 0, 0),
    });
    world.settings.write_buffer(&device, &queue);
    world.face = (face + 1) % 6;
    let key = (assets.cube.id(), frame.noise.id());
    if world.key != Some(key) {
        let storage = cube.texture.create_view(&TextureViewDescriptor {
            dimension: Some(TextureViewDimension::D2Array),
            ..default()
        });
        world.group = Some(device.create_bind_group(
            "planet_sky_reflection",
            &cache.get_bind_group_layout(&pipeline.layout),
            &BindGroupEntries::sequential((
                &noise.texture_view,
                &pipeline.sampler,
                &storage,
                world.settings.binding().unwrap(),
            )),
        ));
        world.key = Some(key);
    }
}
fn compute(
    pipeline: Option<Res<Pipeline>>,
    mut world: ResMut<ReflectionWorld>,
    cache: Res<PipelineCache>,
    mut context: RenderContext,
) {
    let Some(pipeline) = pipeline else {
        return;
    };
    let (Some(compiled), Some(group)) = (
        cache.get_compute_pipeline(pipeline.id),
        world.group.as_ref(),
    ) else {
        return;
    };
    let mut pass = context.command_encoder().begin_compute_pass(&default());
    pass.set_pipeline(compiled);
    pass.set_bind_group(0, group, &[]);
    pass.dispatch_workgroups(SIZE / 8, SIZE / 8, world.faces);
    drop(pass);
    world.reset_pending = false;
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reflection_cube_keeps_one_small_persistent_storage_allocation() {
        let image = cube_image();
        assert_eq!(image.texture_descriptor.size.depth_or_array_layers, 6);
        assert_eq!(
            image.texture_view_descriptor.unwrap().dimension,
            Some(TextureViewDimension::Cube)
        );
        assert_eq!(image.data.unwrap().len(), 64 * 64 * 6 * 8);
        assert!(
            image
                .texture_descriptor
                .usage
                .contains(TextureUsages::STORAGE_BINDING | TextureUsages::TEXTURE_BINDING)
        );
    }
}
