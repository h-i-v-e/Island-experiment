using UnityEngine;
using UnityEngine.Experimental.Rendering;
using UnityEngine.Rendering;
using UnityEngine.Rendering.RenderGraphModule;
using UnityEngine.Rendering.Universal;
using UnityEngine.Rendering.Universal.Internal;

namespace Motu.Rendering
{
    /// <summary>Procedural shell layers, simplified reflections and camera effects for URP Render Graph.</summary>
    public sealed class MotuRendererFeature : ScriptableRendererFeature
    {
        [SerializeField] private bool simplifiedReflections;
        private GeometryPass[] geometry;
        private EffectPass ambientOcclusion, underwater;
        private ReflectionFogPass reflectionFog;

        public override void Create()
        {
            if (simplifiedReflections)
            {
                geometry = new[] { new GeometryPass("Motu reflection", "MotuReflection", true) };
                reflectionFog = new ReflectionFogPass();
            }
            else
            {
                geometry = new GeometryPass[15];
                for (var index = 0; index < geometry.Length; index++)
                    geometry[index] = new GeometryPass($"Motu shell {index + 1}", $"MotuShell{index + 1}", false);
            }
            ambientOcclusion = new EffectPass(false);
            underwater = new EffectPass(true);
        }

        public override void AddRenderPasses(ScriptableRenderer renderer, ref RenderingData renderingData)
        {
            foreach (var pass in geometry) renderer.EnqueuePass(pass);
            var camera = renderingData.cameraData.camera;
            if (PlanarWaterReflection.IsReflectionCamera(camera))
            {
                if (reflectionFog != null && RenderSettings.fog)
                    renderer.EnqueuePass(reflectionFog);
                return;
            }
            if (camera.TryGetComponent<RealTimeAmbientOcclusion>(out var ao) && ao.isActiveAndEnabled)
                renderer.EnqueuePass(ambientOcclusion);
            if (camera.TryGetComponent<OceanUnderwaterView>(out var view) && view.isActiveAndEnabled)
                renderer.EnqueuePass(underwater);
        }

        protected override void Dispose(bool disposing)
        {
            ambientOcclusion?.Dispose();
            reflectionFog?.Dispose();
            reflectionFog = null;
        }

        private sealed class ReflectionFogPass : ScriptableRenderPass
        {
            private readonly Material material;
            private sealed class Data
            {
                public TextureHandle source, depth, destination;
                public Material material;
            }

            public ReflectionFogPass()
            {
                var shader = MotuShaders.Find("Hidden/Motu/Reflection Fog");
                if (shader != null) material = CoreUtils.CreateEngineMaterial(shader);
                renderPassEvent = (RenderPassEvent)((int)RenderPassEvent.AfterRenderingSkybox + 1);
                ConfigureInput(ScriptableRenderPassInput.Depth);
                requiresIntermediateTexture = true;
            }

            public void Dispose()
            {
                CoreUtils.Destroy(material);
            }

            public override void RecordRenderGraph(RenderGraph graph, ContextContainer frame)
            {
                if (material == null) return;
                var resources = frame.Get<UniversalResourceData>();
                if (resources.isActiveTargetBackBuffer) return;
                var descriptor = resources.activeColorTexture.GetDescriptor(graph);
                descriptor.depthBufferBits = DepthBits.None;
                descriptor.clearBuffer = false;
                descriptor.name = "Motu reflection fog";
                var destination = graph.CreateTexture(descriptor);
                using var builder = graph.AddUnsafePass<Data>(descriptor.name, out var data);
                data.source = resources.activeColorTexture;
                data.depth = resources.cameraDepthTexture;
                data.destination = destination;
                data.material = material;
                builder.UseTexture(data.source, AccessFlags.Read);
                builder.UseTexture(data.depth, AccessFlags.Read);
                builder.UseTexture(data.destination, AccessFlags.WriteAll);
                builder.AllowGlobalStateModification(true);
                builder.SetRenderFunc(static (Data pass, UnsafeGraphContext context) =>
                {
                    var cmd = CommandBufferHelpers.GetNativeCommandBuffer(context.cmd);
                    cmd.SetGlobalTexture("_CameraDepthTexture", (RTHandle)pass.depth);
                    Blitter.BlitCameraTexture(cmd, (RTHandle)pass.source,
                        (RTHandle)pass.destination, pass.material, 0);
                });
                resources.cameraColor = destination;
            }
        }

        private sealed class GeometryPass : ScriptableRenderPass
        {
            private readonly string label;
            private readonly ShaderTagId tag;
            private readonly bool reflection;
            private sealed class Data { public RendererListHandle list; }
            public GeometryPass(string label, string tag, bool reflection)
            {
                this.label = label;
                this.tag = new ShaderTagId(tag);
                this.reflection = reflection;
                renderPassEvent = reflection ? RenderPassEvent.BeforeRenderingOpaques : RenderPassEvent.AfterRenderingOpaques;
            }
            public override void RecordRenderGraph(RenderGraph graph, ContextContainer frame)
            {
                var resources = frame.Get<UniversalResourceData>();
                var camera = frame.Get<UniversalCameraData>();
                var rendering = frame.Get<UniversalRenderingData>();
                var drawing = RenderingUtils.CreateDrawingSettings(tag, rendering, camera,
                    frame.Get<UniversalLightData>(), camera.defaultOpaqueSortFlags);
                if (reflection)
                {
                    // Materials without a simplified pass (including imported ships) keep their normal pass.
                    drawing.SetShaderPassName(1, new ShaderTagId("UniversalForwardOnly"));
                    drawing.SetShaderPassName(2, new ShaderTagId("UniversalForward"));
                    drawing.SetShaderPassName(3, new ShaderTagId("SRPDefaultUnlit"));
                }
                var filtering = new FilteringSettings(RenderQueueRange.opaque, camera.camera.cullingMask);
                using var builder = graph.AddRasterRenderPass<Data>(label, out var data);
                data.list = graph.CreateRendererList(new RendererListParams(rendering.cullResults, drawing, filtering));
                builder.UseRendererList(data.list);
                builder.SetRenderAttachment(resources.activeColorTexture, 0);
                builder.SetRenderAttachmentDepth(resources.activeDepthTexture, AccessFlags.ReadWrite);
                builder.SetRenderFunc(static (Data pass, RasterGraphContext context) => context.cmd.DrawRendererList(pass.list));
            }
        }

        private sealed class EffectPass : ScriptableRenderPass
        {
            private readonly bool underwater;
            private readonly CopyDepthPass depthAfterShells;
            private sealed class Data
            {
                public TextureHandle source, destination, first, second, depth, surfaceDepth;
                public Material material;
                public Mesh mesh;
                public Matrix4x4 matrix;
                public bool underwater;
            }
            public EffectPass(bool underwater)
            {
                this.underwater = underwater;
                if (!underwater)
                    depthAfterShells = new CopyDepthPass(RenderPassEvent.AfterRenderingOpaques,
                        Shader.Find("Hidden/Universal Render Pipeline/CopyDepth"));
                // AO must precede URP's opaque colour copy, so refracted terrain includes occlusion.
                renderPassEvent = underwater ? RenderPassEvent.BeforeRenderingPostProcessing
                    : (RenderPassEvent)((int)RenderPassEvent.AfterRenderingSkybox - 1);
                ConfigureInput(ScriptableRenderPassInput.Depth);
                requiresIntermediateTexture = true;
            }

            public void Dispose() => depthAfterShells?.Dispose();

            private static void Blit(CommandBuffer cmd, RTHandle source, RTHandle destination, Material material, int pass)
            {
                cmd.SetGlobalVector("_BlitTexture_TexelSize", new Vector4(1f/source.rt.width, 1f/source.rt.height, source.rt.width, source.rt.height));
                Blitter.BlitCameraTexture(cmd, source, destination, material, pass);
            }

            public override void RecordRenderGraph(RenderGraph graph, ContextContainer frame)
            {
                var resources = frame.Get<UniversalResourceData>();
                var camera = frame.Get<UniversalCameraData>().camera;
                if (resources.isActiveTargetBackBuffer) return;
                Material material;
                OceanUnderwaterView view = null;
                var divisor = 1;
                if (underwater)
                {
                    if (!camera.TryGetComponent(out view) || !view.isActiveAndEnabled) return;
                    material = view.PrepareMaterial();
                }
                else
                {
                    if (!camera.TryGetComponent<RealTimeAmbientOcclusion>(out var ao) || !ao.isActiveAndEnabled) return;
                    material = ao.PrepareMaterial();
                    divisor = ao.ResolutionDivisor;
                }
                if (material == null) return;
                if (!underwater && depthAfterShells != null)
                {
                    // URP may have copied _CameraDepthTexture before the custom
                    // grass shells. Resolve the current depth attachment now so
                    // AO and later depth-sampling water see their clipped pixels.
                    depthAfterShells.Render(graph, frame, resources.cameraDepthTexture,
                        resources.activeDepthTexture, true, "Motu depth after grass shells");
                }
                var descriptor = resources.activeColorTexture.GetDescriptor(graph);
                descriptor.depthBufferBits = DepthBits.None;
                descriptor.clearBuffer = false;
                descriptor.name = underwater ? "Motu underwater colour" : "Motu occluded colour";
                // Before transparents the colour attachment must retain the camera depth attachment's MSAA.
                var destination = graph.CreateTexture(descriptor);
                descriptor.msaaSamples = MSAASamples.None;
                TextureHandle first, second, surfaceDepth = default;
                if (underwater)
                {
                    descriptor.colorFormat = GraphicsFormat.R32G32_SFloat;
                    descriptor.name = "Motu underwater interface";
                    second = graph.CreateTexture(descriptor);
                    descriptor.colorFormat = GraphicsFormat.None;
                    descriptor.depthBufferBits = DepthBits.Depth24;
                    descriptor.name = "Motu underwater interface depth";
                    surfaceDepth = graph.CreateTexture(descriptor);
                    descriptor.depthBufferBits = DepthBits.None;
                    descriptor.colorFormat = GraphicsFormat.R32_SFloat;
                    descriptor.width = descriptor.height = 32;
                    descriptor.filterMode = FilterMode.Bilinear;
                    descriptor.name = "Motu underwater near plane";
                    first = graph.CreateTexture(descriptor);
                }
                else
                {
                    descriptor.width = Mathf.Max(1, descriptor.width / divisor);
                    descriptor.height = Mathf.Max(1, descriptor.height / divisor);
                    descriptor.colorFormat = GraphicsFormat.R8G8B8A8_UNorm;
                    descriptor.filterMode = FilterMode.Bilinear;
                    descriptor.name = "Motu AO";
                    first = graph.CreateTexture(descriptor);
                    descriptor.name = "Motu AO blur";
                    second = graph.CreateTexture(descriptor);
                }
                using (var builder = graph.AddUnsafePass<Data>(descriptor.name, out var data))
                {
                    data.source = resources.activeColorTexture;
                    data.destination = destination;
                    data.first = first; data.second = second;
                    data.depth = resources.cameraDepthTexture;
                    data.surfaceDepth = surfaceDepth;
                    data.material = material; data.underwater = underwater;
                    if (view != null)
                    {
                        data.mesh = view.Surface.SurfaceMesh;
                        data.matrix = view.Surface.SurfaceTransform.localToWorldMatrix;
                    }
                    builder.UseTexture(data.source, AccessFlags.Read);
                    builder.UseTexture(data.depth, AccessFlags.Read);
                    builder.UseTexture(destination, AccessFlags.WriteAll);
                    builder.UseTexture(first, AccessFlags.ReadWrite);
                    builder.UseTexture(second, AccessFlags.ReadWrite);
                    if (underwater) builder.UseTexture(surfaceDepth, AccessFlags.WriteAll);
                    builder.AllowGlobalStateModification(true);
                    builder.SetRenderFunc(static (Data pass, UnsafeGraphContext context) =>
                    {
                        var cmd = CommandBufferHelpers.GetNativeCommandBuffer(context.cmd);
                        RTHandle source = pass.source, destination = pass.destination, first = pass.first, second = pass.second;
                        cmd.SetGlobalTexture("_CameraDepthTexture", (RTHandle)pass.depth);
                        if (pass.underwater)
                        {
                            Blit(cmd, source, first, pass.material, 0);
                            cmd.SetRenderTarget(second, (RTHandle)pass.surfaceDepth);
                            cmd.SetViewport(new Rect(0, 0, second.rt.width, second.rt.height));
                            cmd.ClearRenderTarget(true, true, Color.clear);
                            cmd.DrawMesh(pass.mesh, pass.matrix, pass.material, 0, 1);
                            cmd.SetGlobalTexture("_UnderwaterNear", first);
                            cmd.SetGlobalTexture("_UnderwaterSurface", second);
                            Blit(cmd, source, destination, pass.material, 2);
                        }
                        else
                        {
                            Blit(cmd, source, first, pass.material, 0);
                            Blit(cmd, first, second, pass.material, 1);
                            Blit(cmd, second, first, pass.material, 2);
                            cmd.SetGlobalTexture("_OcclusionTexture", first);
                            cmd.SetGlobalVector("_OcclusionTexture_TexelSize", new Vector4(1f/first.rt.width, 1f/first.rt.height, first.rt.width, first.rt.height));
                            Blit(cmd, source, destination, pass.material, 3);
                        }
                    });
                }
                resources.cameraColor = destination;
            }
        }
    }
}
