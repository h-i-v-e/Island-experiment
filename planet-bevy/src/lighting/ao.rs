//! Independent depth-buffer SSAO implementation. Samples the completed main-pass
//! depth, including alpha-clipped grass, then bilateral-filters and composites it.
use super::LightingSettings;
use bevy::{
    asset::embedded_asset,
    core_pipeline::{Core3d, Core3dSystems, FullscreenShader},
    prelude::*,
    render::{
        Render, RenderApp, RenderStartup, RenderSystems,
        camera::ExtractedCamera,
        extract_component::{
            ComponentUniforms, DynamicUniformIndex, ExtractComponent, ExtractComponentPlugin,
            UniformComponentPlugin,
        },
        render_resource::{
            BindGroupEntries, BindGroupLayoutDescriptor, BindGroupLayoutEntries,
            CachedRenderPipelineId, ColorTargetState, ColorWrites, Extent3d, FilterMode,
            FragmentState, Operations, PipelineCache, RenderPassColorAttachment,
            RenderPassDescriptor, RenderPipelineDescriptor, Sampler, SamplerBindingType,
            SamplerDescriptor, ShaderStages, ShaderType, TextureDescriptor, TextureDimension,
            TextureFormat, TextureSampleType, TextureUsages, VertexState,
            binding_types::{sampler, texture_2d, texture_depth_2d, uniform_buffer},
        },
        renderer::{RenderContext, RenderDevice, ViewQuery},
        texture::{CachedTexture, TextureCache},
        view::{ViewDepthTexture, ViewTarget, ViewUniform, ViewUniformOffset, ViewUniforms},
    },
};
use std::collections::HashMap;

#[derive(Component, Clone, Copy, Debug, PartialEq, ExtractComponent, ShaderType)]
pub struct AoSettings {
    pub parameters: Vec4,
    pub scale: Vec4,
}
impl AoSettings {
    pub fn new(settings: &LightingSettings, radius: f32) -> Self {
        Self {
            parameters: Vec4::new(
                settings.ao_radius,
                if settings.ao {
                    settings.ao_intensity
                } else {
                    0.0
                },
                f32::from(u16::try_from(settings.ao_samples).expect("validated AO sample count")),
                if settings.ao_only { 1.0 } else { 0.0 },
            ),
            scale: Vec4::new(
                radius,
                if settings.half_resolution { 2.0 } else { 1.0 },
                0.0,
                0.0,
            ),
        }
    }
}
pub struct AmbientOcclusionPlugin;
impl Plugin for AmbientOcclusionPlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "ao.wgsl");
        app.add_plugins((
            ExtractComponentPlugin::<AoSettings>::default(),
            UniformComponentPlugin::<AoSettings>::default(),
        ));
        if let Some(render_app) = app.get_sub_app_mut(RenderApp) {
            render_app
                .add_systems(RenderStartup, initialize)
                .add_systems(Render, prepare.in_set(RenderSystems::PrepareResources))
                .add_systems(
                    Core3d,
                    draw.in_set(Core3dSystems::EarlyPostProcess)
                        .before(crate::environment::CloudsPost)
                        .before(crate::water::WaterPost),
                );
        }
    }
}
#[derive(Component)]
struct Targets {
    first: CachedTexture,
    second: CachedTexture,
}
fn prepare(
    mut commands: Commands,
    mut cache: ResMut<TextureCache>,
    device: Res<RenderDevice>,
    views: Query<(Entity, &ExtractedCamera, &AoSettings)>,
) {
    for (entity, camera, settings) in &views {
        if settings.parameters.y <= 0.0 {
            commands.entity(entity).remove::<Targets>();
            continue;
        }
        let Some(size) = camera.physical_target_size else {
            continue;
        };
        let divisor = if settings.scale.y > 1.0 { 2 } else { 1 };
        let size = Extent3d {
            width: size.x.div_ceil(divisor).max(1),
            height: size.y.div_ceil(divisor).max(1),
            depth_or_array_layers: 1,
        };
        let mut texture = |label| {
            cache.get(
                &device,
                TextureDescriptor {
                    label: Some(label),
                    size,
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: TextureDimension::D2,
                    format: TextureFormat::Rgba16Float,
                    usage: TextureUsages::TEXTURE_BINDING | TextureUsages::RENDER_ATTACHMENT,
                    view_formats: &[],
                },
            )
        };
        commands.entity(entity).insert(Targets {
            first: texture("planet_ao_first"),
            second: texture("planet_ao_second"),
        });
    }
}
#[derive(Resource)]
struct Pipelines {
    layout: BindGroupLayoutDescriptor,
    sampler: Sampler,
    passes: [CachedRenderPipelineId; 3],
    shader: Handle<Shader>,
    vertex: VertexState,
}
fn descriptor(
    layout: &BindGroupLayoutDescriptor,
    vertex: &VertexState,
    shader: Handle<Shader>,
    entry: &'static str,
    format: TextureFormat,
) -> RenderPipelineDescriptor {
    RenderPipelineDescriptor {
        label: Some(format!("planet_ao_{entry}").into()),
        layout: vec![layout.clone()],
        vertex: vertex.clone(),
        fragment: Some(FragmentState {
            shader,
            entry_point: Some(entry.into()),
            targets: vec![Some(ColorTargetState {
                format,
                blend: None,
                write_mask: ColorWrites::ALL,
            })],
            ..default()
        }),
        ..default()
    }
}
fn initialize(
    mut commands: Commands,
    device: Res<RenderDevice>,
    server: Res<AssetServer>,
    fullscreen: Res<FullscreenShader>,
    cache: Res<PipelineCache>,
) {
    let layout = BindGroupLayoutDescriptor::new(
        "planet_ao_layout",
        &BindGroupLayoutEntries::sequential(
            ShaderStages::FRAGMENT,
            (
                texture_2d(TextureSampleType::Float { filterable: true }),
                sampler(SamplerBindingType::Filtering),
                texture_depth_2d(),
                uniform_buffer::<ViewUniform>(true),
                uniform_buffer::<AoSettings>(true),
                texture_2d(TextureSampleType::Float { filterable: true }),
            ),
        ),
    );
    let sampler = device.create_sampler(&SamplerDescriptor {
        mag_filter: FilterMode::Linear,
        min_filter: FilterMode::Linear,
        ..default()
    });
    let shader = server.load("embedded://planet_bevy/lighting/ao.wgsl");
    let vertex = fullscreen.to_vertex_state();
    let passes = ["estimate", "horizontal", "vertical"].map(|entry| {
        cache.queue_render_pipeline(descriptor(
            &layout,
            &vertex,
            shader.clone(),
            entry,
            TextureFormat::Rgba16Float,
        ))
    });
    commands.insert_resource(Pipelines {
        layout,
        sampler,
        passes,
        shader,
        vertex,
    });
}
#[allow(clippy::too_many_arguments, clippy::type_complexity)] // Per-view render-system resources.
fn draw(
    view: ViewQuery<(
        &ViewTarget,
        &ViewDepthTexture,
        &ViewUniformOffset,
        &AoSettings,
        &DynamicUniformIndex<AoSettings>,
        &Targets,
    )>,
    pipelines: Option<Res<Pipelines>>,
    cache: Res<PipelineCache>,
    uniforms: Res<ComponentUniforms<AoSettings>>,
    view_uniforms: Res<ViewUniforms>,
    mut composites: Local<HashMap<TextureFormat, CachedRenderPipelineId>>,
    mut ctx: RenderContext,
) {
    let Some(pipelines) = pipelines else {
        return;
    };
    let (target, depth, view_index, settings, settings_index, textures) = view.into_inner();
    if settings.parameters.y <= 0.0 {
        return;
    }
    let composite = *composites
        .entry(target.main_texture_format())
        .or_insert_with(|| {
            cache.queue_render_pipeline(descriptor(
                &pipelines.layout,
                &pipelines.vertex,
                pipelines.shader.clone(),
                "composite",
                target.main_texture_format(),
            ))
        });
    let Some(estimate) = cache.get_render_pipeline(pipelines.passes[0]) else {
        return;
    };
    let Some(horizontal) = cache.get_render_pipeline(pipelines.passes[1]) else {
        return;
    };
    let Some(vertical) = cache.get_render_pipeline(pipelines.passes[2]) else {
        return;
    };
    let Some(composite) = cache.get_render_pipeline(composite) else {
        return;
    };
    let Some(settings_binding) = uniforms.uniforms().binding() else {
        return;
    };
    let Some(view_binding) = view_uniforms.uniforms.binding() else {
        return;
    };
    let post = target.post_process_write();
    let offsets = [view_index.offset, settings_index.index()];
    for (pipeline, input, output) in [
        (
            estimate,
            &textures.second.default_view,
            &textures.first.default_view,
        ),
        (
            horizontal,
            &textures.first.default_view,
            &textures.second.default_view,
        ),
        (
            vertical,
            &textures.second.default_view,
            &textures.first.default_view,
        ),
        (composite, &textures.first.default_view, post.destination),
    ] {
        let group = ctx.render_device().create_bind_group(
            "planet_ao_group",
            &cache.get_bind_group_layout(&pipelines.layout),
            &BindGroupEntries::sequential((
                post.source,
                &pipelines.sampler,
                depth.view(),
                view_binding.clone(),
                settings_binding.clone(),
                input,
            )),
        );
        let mut pass = ctx
            .command_encoder()
            .begin_render_pass(&RenderPassDescriptor {
                label: Some("planet_ao_pass"),
                color_attachments: &[Some(RenderPassColorAttachment {
                    view: output,
                    depth_slice: None,
                    resolve_target: None,
                    ops: Operations::default(),
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, &group, &offsets);
        pass.draw(0..3, 0..1);
    }
}
