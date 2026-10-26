//! Pixel waterline and underwater extinction against the rendered surface/depth.
use super::WaterSettings;
use crate::{
    Orbit, Scene,
    ocean::{OceanFrame, OceanUniform},
};
use bevy::{
    asset::embedded_asset,
    core_pipeline::{Core3d, Core3dSystems, FullscreenShader},
    prelude::*,
    render::{
        RenderApp, RenderStartup,
        extract_component::{
            ComponentUniforms, DynamicUniformIndex, ExtractComponent, ExtractComponentPlugin,
            UniformComponentPlugin,
        },
        render_asset::RenderAssets,
        render_resource::{
            BindGroupEntries, BindGroupLayoutDescriptor, BindGroupLayoutEntries,
            CachedRenderPipelineId, ColorTargetState, ColorWrites, FilterMode, FragmentState,
            Operations, PipelineCache, RenderPassColorAttachment, RenderPassDescriptor,
            RenderPipelineDescriptor, Sampler, SamplerBindingType, SamplerDescriptor, ShaderStages,
            ShaderType, TextureFormat, TextureSampleType, UniformBuffer, VertexState,
            binding_types::{
                sampler, texture_2d, texture_3d, texture_cube, texture_depth_2d, uniform_buffer,
            },
        },
        renderer::{RenderContext, RenderDevice, RenderQueue, ViewQuery},
        texture::GpuImage,
        view::{ViewDepthTexture, ViewTarget, ViewUniform, ViewUniformOffset, ViewUniforms},
    },
    shader::ShaderDefVal,
};
use std::collections::HashMap;
#[derive(Component, Clone, Copy, ExtractComponent, ShaderType)]
struct Submerge {
    parameters: Vec4,
    absorption: Vec4,
    scatter: Vec4,
}
pub struct UnderwaterPlugin;
impl Plugin for UnderwaterPlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "underwater.wgsl");
        app.add_plugins((
            ExtractComponentPlugin::<Submerge>::default(),
            UniformComponentPlugin::<Submerge>::default(),
        ))
        .add_systems(Update, settings.after(super::update));
        if let Some(render) = app.get_sub_app_mut(RenderApp) {
            render.add_systems(RenderStartup, initialize).add_systems(
                Core3d,
                draw.in_set(Core3dSystems::EarlyPostProcess)
                    .in_set(super::WaterPost),
            );
        }
    }
}
#[allow(clippy::cast_possible_truncation)]
fn settings(
    mut commands: Commands,
    scene: Res<Scene>,
    options: Res<WaterSettings>,
    ocean: Res<crate::ocean::OceanSettings>,
    cameras: Query<(Entity, &Transform), With<Orbit>>,
) {
    for (entity, camera) in &cameras {
        let sun = Vec3::new(-3.0, 4.0, 5.0).normalize();
        let up = camera.translation.normalize();
        let day = (up.dot(sun) * 2.0 + 0.2).clamp(0.03, 1.0);
        commands.entity(entity).insert(Submerge {
            parameters: Vec4::new(
                scene.terrain.topology.radius as f32,
                if options.underwater
                    && !scene.no_ocean
                    && scene.generation == crate::command::GenerationMode::Terrain
                    && (camera.translation.length() - 1.0) * scene.terrain.topology.radius as f32
                        <= ocean.amplitude * 0.81 + 0.1
                {
                    1.0
                } else {
                    0.0
                },
                0.045,
                80.0,
            ),
            absorption: Vec4::new(0.45, 0.12, 0.055, options.absorption),
            scatter: Vec4::new(0.02 * day, 0.16 * day, 0.23 * day, 0.005),
        });
    }
}
#[derive(Resource)]
struct Pipeline {
    layout: BindGroupLayoutDescriptor,
    sampler: Sampler,
    shader: Handle<Shader>,
    vertex: VertexState,
}
fn initialize(
    mut commands: Commands,
    device: Res<RenderDevice>,
    server: Res<AssetServer>,
    fullscreen: Res<FullscreenShader>,
) {
    let layout = BindGroupLayoutDescriptor::new(
        "planet_underwater",
        &BindGroupLayoutEntries::with_indices(
            ShaderStages::FRAGMENT,
            (
                (0, texture_2d(TextureSampleType::Float { filterable: true })),
                (1, sampler(SamplerBindingType::Filtering)),
                (2, texture_depth_2d()),
                (3, uniform_buffer::<ViewUniform>(true)),
                (4, uniform_buffer::<Submerge>(true)),
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
            ),
        ),
    );
    commands.insert_resource(Pipeline {
        layout,
        sampler: device.create_sampler(&SamplerDescriptor {
            mag_filter: FilterMode::Linear,
            min_filter: FilterMode::Linear,
            ..default()
        }),
        shader: server.load("embedded://planet_bevy/water/underwater.wgsl"),
        vertex: fullscreen.to_vertex_state(),
    });
}
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn draw(
    view: ViewQuery<(
        &ViewTarget,
        &ViewDepthTexture,
        &ViewUniformOffset,
        &Submerge,
        &DynamicUniformIndex<Submerge>,
    )>,
    pipeline: Option<Res<Pipeline>>,
    record: Option<Res<OceanFrame>>,
    images: Res<RenderAssets<GpuImage>>,
    cache: Res<PipelineCache>,
    uniforms: Res<ComponentUniforms<Submerge>>,
    views: Res<ViewUniforms>,
    device: Res<RenderDevice>,
    queue: Res<RenderQueue>,
    mut pipelines: Local<HashMap<TextureFormat, CachedRenderPipelineId>>,
    mut ctx: RenderContext,
) {
    let (target, depth, view_index, settings, index) = view.into_inner();
    if settings.parameters.y < 0.5 {
        return;
    }
    let (Some(pipeline), Some(record)) = (pipeline, record) else {
        return;
    };
    if record.coast == Handle::default() {
        return;
    }
    let (Some(coast), Some(noise)) = (images.get(&record.coast), images.get(&record.noise)) else {
        return;
    };
    let id = *pipelines
        .entry(target.main_texture_format())
        .or_insert_with(|| {
            cache.queue_render_pipeline(RenderPipelineDescriptor {
                label: Some("planet_underwater".into()),
                layout: vec![pipeline.layout.clone()],
                vertex: pipeline.vertex.clone(),
                fragment: Some(FragmentState {
                    shader: pipeline.shader.clone(),
                    shader_defs: vec![ShaderDefVal::UInt("MATERIAL_BIND_GROUP".into(), 0)],
                    entry_point: Some("composite".into()),
                    targets: vec![Some(ColorTargetState {
                        format: target.main_texture_format(),
                        blend: None,
                        write_mask: ColorWrites::ALL,
                    })],
                }),
                ..default()
            })
        });
    let Some(compiled) = cache.get_render_pipeline(id) else {
        return;
    };
    let (Some(view_binding), Some(settings_binding)) =
        (views.uniforms.binding(), uniforms.uniforms().binding())
    else {
        return;
    };
    let mut ocean = UniformBuffer::from(record.ocean);
    ocean.write_buffer(&device, &queue);
    let post = target.post_process_write();
    let group = device.create_bind_group(
        "planet_underwater",
        &cache.get_bind_group_layout(&pipeline.layout),
        &BindGroupEntries::with_indices((
            (0, post.source),
            (1, &pipeline.sampler),
            (2, depth.view()),
            (3, view_binding),
            (4, settings_binding),
            (100, ocean.binding().unwrap()),
            (101, &coast.texture_view),
            (102, &coast.sampler),
            (103, &noise.texture_view),
            (104, &noise.sampler),
        )),
    );
    let mut pass = ctx
        .command_encoder()
        .begin_render_pass(&RenderPassDescriptor {
            label: Some("planet_underwater"),
            color_attachments: &[Some(RenderPassColorAttachment {
                view: post.destination,
                depth_slice: None,
                resolve_target: None,
                ops: Operations::default(),
            })],
            ..default()
        });
    pass.set_pipeline(compiled);
    pass.set_bind_group(0, &group, &[view_index.offset, index.index()]);
    pass.draw(0..3, 0..1);
}
