//! Spherical cloud volumes evaluated at reduced resolution against final scene depth.
//! Render-owned textures and the small weather uniform persist while the viewport is stable.
use super::settings::{EnvironmentFrame, EnvironmentUniform};
use bevy::{
    asset::embedded_asset,
    core_pipeline::{Core3d, Core3dSystems, FullscreenShader},
    prelude::*,
    render::{
        Render, RenderApp, RenderStartup, RenderSystems,
        camera::ExtractedCamera,
        render_asset::RenderAssets,
        render_resource::{
            BindGroupEntries, BindGroupLayoutDescriptor, BindGroupLayoutEntries,
            CachedRenderPipelineId, ColorTargetState, ColorWrites, Extent3d, FilterMode,
            FragmentState, Operations, PipelineCache, RenderPassColorAttachment,
            RenderPassDescriptor, RenderPipelineDescriptor, Sampler, SamplerBindingType,
            SamplerDescriptor, ShaderStages, ShaderType, TextureDescriptor, TextureDimension,
            TextureFormat, TextureSampleType, TextureUsages, UniformBuffer, VertexState,
            binding_types::{
                sampler, texture_2d, texture_3d, texture_cube_array, texture_depth_2d,
                uniform_buffer,
            },
        },
        renderer::{RenderContext, RenderDevice, RenderQueue, ViewQuery},
        texture::{CachedTexture, GpuImage, TextureCache},
        view::{
            ExtractedView, ViewDepthTexture, ViewTarget, ViewUniform, ViewUniformOffset,
            ViewUniforms,
        },
    },
};
use std::collections::HashMap;

pub struct CloudPlugin;

/// Clouds use completed opaque/transmissive depth, after AO and before submersion.
#[derive(SystemSet, Debug, Hash, PartialEq, Eq, Clone)]
pub struct CloudsPost;

impl Plugin for CloudPlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "clouds.wgsl");
        if let Some(render) = app.get_sub_app_mut(RenderApp) {
            render
                .add_systems(RenderStartup, initialize)
                .add_systems(Render, prepare.in_set(RenderSystems::PrepareResources))
                .add_systems(
                    Core3d,
                    draw.in_set(Core3dSystems::EarlyPostProcess)
                        .in_set(CloudsPost)
                        .before(crate::water::WaterPost),
                );
        }
    }
}

#[derive(Clone, Copy)]
struct CameraFrame {
    projection: Mat4,
    clip_from_world: Mat4,
    position: Vec3,
    forward: Vec3,
    viewport: UVec4,
    wind: Vec4,
    simulation: Vec4,
    exposure: f32,
    revision: u64,
    seconds: f32,
}
impl CameraFrame {
    fn new(camera: &ExtractedCamera, view: &ExtractedView, weather: &EnvironmentFrame) -> Self {
        let world = view.world_from_view.to_matrix();
        Self {
            projection: view.clip_from_view,
            clip_from_world: view
                .clip_from_world
                .unwrap_or_else(|| view.clip_from_view * world.inverse()),
            position: view.world_from_view.translation(),
            forward: *view.world_from_view.forward(),
            viewport: view.viewport,
            wind: weather.uniform.wind,
            simulation: weather.uniform.simulation,
            exposure: camera.exposure,
            revision: weather.revision,
            seconds: weather.uniform.air.w,
        }
    }
    fn compatible(self, next: Self, radius: f32) -> bool {
        self.revision == next.revision
            && (self.simulation.x > 0.5) == (next.simulation.x > 0.5)
            && next.simulation.y >= self.simulation.y
            && (self.simulation.x < 0.5 || next.simulation.y - self.simulation.y <= 600.0)
            && self.viewport == next.viewport
            && self.projection.abs_diff_eq(next.projection, 0.00001)
            && self.position.distance(next.position) * radius <= 50.0
            && self.forward.dot(next.forward) > 0.2
            && next.seconds >= self.seconds
            && next.seconds - self.seconds <= 1.0
    }
}
#[derive(Clone, Copy, Default, ShaderType)]
struct TemporalUniform {
    previous_clip_from_world: Mat4,
    previous_eye: Vec4,
    previous_viewport: Vec4,
    previous_wind: Vec4,
    /// Valid history, jitter phase, enabled, maximum history blend.
    controls: Vec4,
}
impl TemporalUniform {
    fn new(previous: CameraFrame, valid: bool, phase: u32, enabled: bool) -> Self {
        Self {
            previous_clip_from_world: previous.clip_from_world,
            previous_eye: previous.position.extend(previous.exposure),
            previous_viewport: previous.viewport.as_vec4(),
            // Simulation reprojection uses its previous climate clock; legacy
            // procedural weather retains the inverse rotation quaternion.
            previous_wind: if previous.simulation.x > 0.5 {
                Vec4::new(previous.simulation.y, 0.0, 0.0, 0.0)
            } else {
                previous.wind
            },
            controls: Vec4::new(
                f32::from(u8::from(valid)),
                f32::from(u8::try_from(phase % 8).unwrap()),
                f32::from(u8::from(enabled)),
                0.85,
            ),
        }
    }
}
struct HistoryTextures {
    colour: CachedTexture,
    depth: CachedTexture,
}
#[derive(Component)]
struct Targets {
    size: Extent3d,
    colour: CachedTexture,
    depth: CachedTexture,
    history: [HistoryTextures; 2],
    previous: Option<CameraFrame>,
    current: CameraFrame,
    temporal: UniformBuffer<TemporalUniform>,
    index: usize,
    phase: u32,
}

#[derive(Resource, Default)]
struct WeatherUniform(UniformBuffer<EnvironmentUniform>);

fn reduced_size(viewport: UVec2, quality: u32) -> Extent3d {
    let divisor = if quality == 0 { 4 } else { 2 };
    Extent3d {
        width: viewport.x.div_ceil(divisor).max(1),
        height: viewport.y.div_ceil(divisor).max(1),
        depth_or_array_layers: 1,
    }
}

#[allow(clippy::type_complexity)] // Render-world views and their persistent cloud history.
fn prepare(
    mut commands: Commands,
    frame: Res<EnvironmentFrame>,
    device: Res<RenderDevice>,
    queue: Res<RenderQueue>,
    mut uniform: ResMut<WeatherUniform>,
    mut cache: ResMut<TextureCache>,
    mut views: Query<
        (
            Entity,
            &ExtractedCamera,
            &ExtractedView,
            Option<&mut Targets>,
        ),
        (With<ViewDepthTexture>, With<Camera3d>),
    >,
) {
    let enabled = frame.enabled && frame.uniform.weather.w > 0.0;
    if enabled {
        uniform.0.set(frame.uniform);
        uniform.0.write_buffer(&device, &queue);
    }
    for (entity, camera, view, targets) in &mut views {
        if !enabled {
            if targets.is_some() {
                commands.entity(entity).remove::<Targets>();
            }
            continue;
        }
        let Some(viewport) = camera.physical_viewport_size else {
            continue;
        };
        let size = reduced_size(viewport, frame.quality);
        let current = CameraFrame::new(camera, view, &frame);
        if let Some(mut targets) = targets.filter(|old| old.size == size) {
            let previous = targets.previous.unwrap_or(current);
            let valid = frame.temporal
                && targets.previous.is_some()
                && previous.compatible(current, frame.uniform.shape.x);
            let phase = if frame.temporal && current.seconds.to_bits() != previous.seconds.to_bits()
            {
                targets.phase
            } else {
                0
            };
            targets
                .temporal
                .set(TemporalUniform::new(previous, valid, phase, frame.temporal));
            targets.temporal.write_buffer(&device, &queue);
            targets.current = current;
            if !frame.temporal {
                targets.previous = None;
            }
            continue;
        }
        let mut texture = |label, format| {
            cache.get(
                &device,
                TextureDescriptor {
                    label: Some(label),
                    size,
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: TextureDimension::D2,
                    format,
                    usage: TextureUsages::TEXTURE_BINDING | TextureUsages::RENDER_ATTACHMENT,
                    view_formats: &[],
                },
            )
        };
        let mut temporal =
            UniformBuffer::from(TemporalUniform::new(current, false, 0, frame.temporal));
        temporal.write_buffer(&device, &queue);
        commands.entity(entity).insert(Targets {
            size,
            colour: texture("planet_cloud_colour", TextureFormat::Rgba16Float),
            depth: texture("planet_cloud_scene_distance", TextureFormat::Rg32Float),
            history: std::array::from_fn(|_| HistoryTextures {
                colour: texture("planet_cloud_history_colour", TextureFormat::Rgba16Float),
                depth: texture("planet_cloud_history_distance", TextureFormat::Rg32Float),
            }),
            previous: None,
            current,
            temporal,
            index: 0,
            phase: 0,
        });
    }
}

#[derive(Resource)]
struct Pipelines {
    layout: BindGroupLayoutDescriptor,
    sampler: Sampler,
    estimate: CachedRenderPipelineId,
    temporal: CachedRenderPipelineId,
    shader: Handle<Shader>,
    vertex: VertexState,
}

impl Pipelines {
    fn composite(
        &self,
        format: TextureFormat,
        cache: &PipelineCache,
        composites: &mut HashMap<TextureFormat, CachedRenderPipelineId>,
    ) -> CachedRenderPipelineId {
        *composites.entry(format).or_insert_with(|| {
            cache.queue_render_pipeline(descriptor(
                &self.layout,
                &self.vertex,
                self.shader.clone(),
                "composite",
                &[format],
            ))
        })
    }
}

fn descriptor(
    layout: &BindGroupLayoutDescriptor,
    vertex: &VertexState,
    shader: Handle<Shader>,
    entry: &'static str,
    formats: &[TextureFormat],
) -> RenderPipelineDescriptor {
    RenderPipelineDescriptor {
        label: Some(format!("planet_clouds_{entry}").into()),
        layout: vec![layout.clone()],
        vertex: vertex.clone(),
        fragment: Some(FragmentState {
            shader,
            entry_point: Some(entry.into()),
            targets: formats
                .iter()
                .map(|format| {
                    Some(ColorTargetState {
                        format: *format,
                        blend: None,
                        write_mask: ColorWrites::ALL,
                    })
                })
                .collect(),
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
        "planet_clouds_layout",
        &BindGroupLayoutEntries::sequential(
            ShaderStages::FRAGMENT,
            (
                texture_2d(TextureSampleType::Float { filterable: true }),
                sampler(SamplerBindingType::Filtering),
                texture_depth_2d(),
                uniform_buffer::<ViewUniform>(true),
                uniform_buffer::<EnvironmentUniform>(false),
                texture_3d(TextureSampleType::Float { filterable: true }),
                texture_2d(TextureSampleType::Float { filterable: true }),
                texture_2d(TextureSampleType::Float { filterable: false }),
                uniform_buffer::<TemporalUniform>(false),
                texture_2d(TextureSampleType::Float { filterable: true }),
                texture_2d(TextureSampleType::Float { filterable: false }),
                texture_cube_array(TextureSampleType::Float { filterable: true }),
            ),
        ),
    );
    let sampler = device.create_sampler(&SamplerDescriptor {
        mag_filter: FilterMode::Linear,
        min_filter: FilterMode::Linear,
        address_mode_u: bevy::render::render_resource::AddressMode::Repeat,
        address_mode_v: bevy::render::render_resource::AddressMode::Repeat,
        address_mode_w: bevy::render::render_resource::AddressMode::Repeat,
        ..default()
    });
    let shader = server.load("embedded://planet_bevy/environment/clouds.wgsl");
    let vertex = fullscreen.to_vertex_state();
    let estimate = cache.queue_render_pipeline(descriptor(
        &layout,
        &vertex,
        shader.clone(),
        "estimate",
        &[TextureFormat::Rgba16Float, TextureFormat::Rg32Float],
    ));
    let temporal = cache.queue_render_pipeline(descriptor(
        &layout,
        &vertex,
        shader.clone(),
        "temporal",
        &[TextureFormat::Rgba16Float, TextureFormat::Rg32Float],
    ));
    commands.insert_resource(Pipelines {
        layout,
        sampler,
        estimate,
        temporal,
        shader,
        vertex,
    });
    commands.init_resource::<WeatherUniform>();
}

fn cloud_attachment(
    view: &bevy::render::render_resource::TextureView,
) -> RenderPassColorAttachment<'_> {
    RenderPassColorAttachment {
        view,
        depth_slice: None,
        resolve_target: None,
        ops: Operations::default(),
    }
}
fn cloud_pass(
    ctx: &mut RenderContext,
    label: &'static str,
    pipeline: &bevy::render::render_resource::RenderPipeline,
    group: &bevy::render::render_resource::BindGroup,
    offset: u32,
    colour: &bevy::render::render_resource::TextureView,
    guide: Option<&bevy::render::render_resource::TextureView>,
) {
    let attachments = [Some(cloud_attachment(colour)), guide.map(cloud_attachment)];
    let mut pass = ctx
        .command_encoder()
        .begin_render_pass(&RenderPassDescriptor {
            label: Some(label),
            color_attachments: &attachments[..if guide.is_some() { 2 } else { 1 }],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
    pass.set_pipeline(pipeline);
    pass.set_bind_group(0, group, &[offset]);
    pass.draw(0..3, 0..1);
}
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn draw(
    view: ViewQuery<(
        &ViewTarget,
        &ViewDepthTexture,
        &ViewUniformOffset,
        &mut Targets,
    )>,
    pipelines: Option<Res<Pipelines>>,
    cache: Res<PipelineCache>,
    weather: Res<WeatherUniform>,
    frame: Res<EnvironmentFrame>,
    images: Res<RenderAssets<GpuImage>>,
    views: Res<ViewUniforms>,
    mut composites: Local<HashMap<TextureFormat, CachedRenderPipelineId>>,
    mut ctx: RenderContext,
) {
    if !frame.enabled || frame.uniform.weather.w <= 0.0 {
        return;
    }
    let Some(pipelines) = pipelines else {
        return;
    };
    let (target, depth, index, mut textures) = view.into_inner();
    let composite = pipelines.composite(target.main_texture_format(), &cache, &mut composites);
    let (Some(estimate), Some(composite), Some(temporal), Some(noise), Some(global_weather)) = (
        cache.get_render_pipeline(pipelines.estimate),
        cache.get_render_pipeline(composite),
        cache.get_render_pipeline(pipelines.temporal),
        images.get(&frame.noise),
        images.get(&frame.weather),
    ) else {
        return;
    };
    let (Some(view_binding), Some(weather_binding), Some(temporal_binding)) = (
        views.uniforms.binding(),
        weather.0.binding(),
        textures.temporal.binding(),
    ) else {
        return;
    };
    let post = target.post_process_write();
    let group = |colour: &bevy::render::render_resource::TextureView,
                 distance: &bevy::render::render_resource::TextureView,
                 history_colour: &bevy::render::render_resource::TextureView,
                 history_distance: &bevy::render::render_resource::TextureView| {
        ctx.render_device().create_bind_group(
            "planet_clouds_group",
            &cache.get_bind_group_layout(&pipelines.layout),
            &BindGroupEntries::sequential((
                post.source,
                &pipelines.sampler,
                depth.view(),
                view_binding.clone(),
                weather_binding.clone(),
                &noise.texture_view,
                colour,
                distance,
                temporal_binding.clone(),
                history_colour,
                history_distance,
                &global_weather.texture_view,
            )),
        )
    };
    let estimate_group = group(post.source, post.source, post.source, post.source);
    let read = textures.index;
    let write = 1 - read;
    let temporal_group = group(
        &textures.colour.default_view,
        &textures.depth.default_view,
        &textures.history[read].colour.default_view,
        &textures.history[read].depth.default_view,
    );
    let resolved = if frame.temporal {
        (
            &textures.history[write].colour.default_view,
            &textures.history[write].depth.default_view,
        )
    } else {
        (&textures.colour.default_view, &textures.depth.default_view)
    };
    let composite_group = group(resolved.0, resolved.1, post.source, post.source);
    cloud_pass(
        &mut ctx,
        "planet_clouds_estimate",
        estimate,
        &estimate_group,
        index.offset,
        &textures.colour.default_view,
        Some(&textures.depth.default_view),
    );
    if frame.temporal {
        cloud_pass(
            &mut ctx,
            "planet_clouds_temporal",
            temporal,
            &temporal_group,
            index.offset,
            &textures.history[write].colour.default_view,
            Some(&textures.history[write].depth.default_view),
        );
    }
    cloud_pass(
        &mut ctx,
        "planet_clouds_composite",
        composite,
        &composite_group,
        index.offset,
        post.destination,
        None,
    );
    if frame.temporal {
        textures.index = write;
        textures.phase = textures.phase.wrapping_add(1);
        textures.previous = Some(textures.current);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn history_frame() -> CameraFrame {
        CameraFrame {
            projection: Mat4::perspective_infinite_reverse_rh(1.0, 1.6, 0.00002),
            clip_from_world: Mat4::IDENTITY,
            position: Vec3::Y * 1.01,
            forward: Vec3::Z,
            viewport: UVec4::new(690, 0, 1870, 1600),
            wind: Vec4::W,
            simulation: Vec4::ZERO,
            exposure: 0.001,
            revision: 3,
            seconds: 1.0,
        }
    }
    #[test]
    fn simulation_history_rejects_mode_changes_clock_rewinds_and_large_steps() {
        let mut previous = history_frame();
        previous.simulation = Vec4::new(1.0, 1200.0, 6_371_000.0, 0.0);
        let mut next = previous;
        next.simulation.y += 120.0;
        next.simulation.x = 1.4;
        assert!(previous.compatible(next, 1000.0));
        for clock in [1100.0, 1900.0] {
            next.simulation.y = clock;
            assert!(!previous.compatible(next, 1000.0));
        }
        next = previous;
        next.simulation.x = 0.0;
        assert!(!previous.compatible(next, 1000.0));
    }
    #[test]
    fn history_resets_for_teleports_projection_viewport_and_world_changes() {
        let previous = history_frame();
        let mut next = previous;
        next.position += Vec3::X * 0.001;
        next.seconds += 0.016;
        assert!(previous.compatible(next, 1000.0));
        for invalid in [
            CameraFrame {
                position: previous.position + Vec3::X * 0.06,
                ..previous
            },
            CameraFrame {
                viewport: UVec4::new(0, 0, 1870, 1600),
                ..previous
            },
            CameraFrame {
                viewport: UVec4::new(690, 0, 1700, 1600),
                ..previous
            },
            CameraFrame {
                projection: Mat4::perspective_infinite_reverse_rh(1.1, 1.6, 0.00002),
                ..previous
            },
            CameraFrame {
                revision: 4,
                ..previous
            },
            CameraFrame {
                seconds: 3.0,
                ..previous
            },
            CameraFrame {
                seconds: 0.0,
                ..previous
            },
            CameraFrame {
                forward: -previous.forward,
                ..previous
            },
        ] {
            assert!(!previous.compatible(invalid, 1000.0));
        }
        // Exposure changes are rescaled and wind is reprojected on the GPU.
        next.exposure = 0.002;
        next.wind = Vec4::from_array(Quat::from_rotation_y(0.01).to_array());
        assert!(previous.compatible(next, 1000.0));
    }

    #[test]
    fn reduced_targets_follow_the_viewport_and_bound_even_tiny_sizes() {
        assert_eq!(
            reduced_size(UVec2::new(1870, 1600), 1),
            Extent3d {
                width: 935,
                height: 800,
                depth_or_array_layers: 1,
            }
        );
        assert_eq!(
            reduced_size(UVec2::new(1871, 1601), 0),
            Extent3d {
                width: 468,
                height: 401,
                depth_or_array_layers: 1,
            }
        );
        assert_eq!(
            reduced_size(UVec2::ZERO, 2),
            Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            }
        );
    }
}
