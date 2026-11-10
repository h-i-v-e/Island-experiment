using UnityEngine;
using UnityEngine.Experimental.Rendering;
using UnityEngine.Rendering;
using UnityEngine.Rendering.RenderGraphModule;
using UnityEngine.Rendering.Universal;
using UnityEngine.Rendering.Universal.Internal;

namespace Motu.Rendering
{
    public sealed partial class MotuRendererFeature
    {
        private sealed class SquallVolumePass : ScriptableRenderPass
        {
            private static readonly ShaderTagId CloudTag = new ShaderTagId("MotuSquallCloud");
            private static readonly ShaderTagId DetailTag = new ShaderTagId("MotuSquallVolume");
            private static readonly int SizeId = Shader.PropertyToID("_MotuSquallRenderSize");
            private readonly CopyDepthPass depthAfterWater;
            private readonly Material composite;

            private sealed class VolumeData { internal RendererListHandle list; internal Vector4 size; }
            private sealed class CompositeData { internal TextureHandle volume; internal Material material; internal Vector4 size; }
            private sealed class DetailData { internal RendererListHandle list; }

            internal SquallVolumePass()
            {
                renderPassEvent = RenderPassEvent.AfterRenderingTransparents;
                depthAfterWater = new CopyDepthPass(renderPassEvent, Shader.Find("Hidden/Universal Render Pipeline/CopyDepth"));
                var shader = Resources.Load<Shader>("SquallComposite");
                if (shader != null) composite = CoreUtils.CreateEngineMaterial(shader);
                ConfigureInput(ScriptableRenderPassInput.Depth);
                requiresIntermediateTexture = true;
            }

            internal void Dispose() { depthAfterWater.Dispose(); CoreUtils.Destroy(composite); }

            public override void RecordRenderGraph(RenderGraph graph, ContextContainer frame)
            {
                var resources = frame.Get<UniversalResourceData>();
                if (resources.isActiveTargetBackBuffer || composite == null) return;
                // Retain the real displaced-water depth and hull occlusion.
                depthAfterWater.Render(graph, frame, resources.cameraDepthTexture,
                    resources.activeDepthTexture, true, "Motu depth after water");
                var descriptor = resources.activeColorTexture.GetDescriptor(graph);
                if (SquallVisualRenderer.VolumeResolutionDivisor(frame.Get<UniversalCameraData>().camera) == 1)
                {
                    DrawVolume(graph, frame, resources, resources.activeColorTexture,
                        new Vector4(descriptor.width, descriptor.height, 1f / descriptor.width, 1f / descriptor.height));
                    DrawDetails(graph, frame, resources);
                    return;
                }
                descriptor.width = Mathf.Max(1, (descriptor.width + 1) / 2);
                descriptor.height = Mathf.Max(1, (descriptor.height + 1) / 2);
                descriptor.depthBufferBits = DepthBits.None;
                descriptor.msaaSamples = MSAASamples.None;
                descriptor.colorFormat = GraphicsFormat.R16G16B16A16_SFloat;
                descriptor.clearBuffer = true; descriptor.clearColor = Color.clear;
                descriptor.filterMode = FilterMode.Bilinear;
                descriptor.name = "Motu half-resolution squall volume";
                var volume = graph.CreateTexture(descriptor);
                var size = new Vector4(descriptor.width, descriptor.height, 1f / descriptor.width, 1f / descriptor.height);
                DrawVolume(graph, frame, resources, volume, size);
                CompositeVolume(graph, resources, volume, size);
                DrawDetails(graph, frame, resources);
            }

            private static RendererListHandle Renderers(RenderGraph graph, ContextContainer frame, ShaderTagId tag)
            {
                var camera = frame.Get<UniversalCameraData>();
                var rendering = frame.Get<UniversalRenderingData>();
                var drawing = RenderingUtils.CreateDrawingSettings(tag, rendering, camera,
                    frame.Get<UniversalLightData>(), SortingCriteria.CommonTransparent);
                var filtering = new FilteringSettings(RenderQueueRange.transparent, camera.camera.cullingMask);
                return graph.CreateRendererList(new RendererListParams(rendering.cullResults, drawing, filtering));
            }

            private static void DrawVolume(RenderGraph graph, ContextContainer frame,
                UniversalResourceData resources, TextureHandle volume, Vector4 size)
            {
                using var builder = graph.AddRasterRenderPass<VolumeData>("Motu squall volume", out var data);
                data.list = Renderers(graph, frame, CloudTag); data.size = size;
                builder.UseRendererList(data.list);
                builder.UseTexture(resources.cameraDepthTexture, AccessFlags.Read);
                builder.SetRenderAttachment(volume, 0, AccessFlags.ReadWrite);
                builder.AllowGlobalStateModification(true);
                builder.SetRenderFunc(static (VolumeData pass, RasterGraphContext context) =>
                {
                    context.cmd.SetGlobalVector(SizeId, pass.size);
                    context.cmd.DrawRendererList(pass.list);
                });
            }

            private void CompositeVolume(RenderGraph graph, UniversalResourceData resources, TextureHandle volume, Vector4 size)
            {
                using var builder = graph.AddRasterRenderPass<CompositeData>("Motu depth-aware squall upsample", out var data);
                data.volume = volume; data.material = composite; data.size = size;
                builder.UseTexture(volume, AccessFlags.Read);
                builder.UseTexture(resources.cameraDepthTexture, AccessFlags.Read);
                builder.SetRenderAttachment(resources.activeColorTexture, 0, AccessFlags.ReadWrite);
                builder.AllowGlobalStateModification(true);
                builder.SetRenderFunc(static (CompositeData pass, RasterGraphContext context) =>
                {
                    context.cmd.SetGlobalVector(SizeId, pass.size);
                    Blitter.BlitTexture(context.cmd, (RTHandle)pass.volume, new Vector4(1, 1, 0, 0), pass.material, 0);
                });
            }

            private static void DrawDetails(RenderGraph graph, ContextContainer frame, UniversalResourceData resources)
            {
                using var builder = graph.AddRasterRenderPass<DetailData>("Motu sharp rain and lightning", out var data);
                data.list = Renderers(graph, frame, DetailTag);
                builder.UseRendererList(data.list);
                builder.SetRenderAttachment(resources.activeColorTexture, 0, AccessFlags.ReadWrite);
                builder.SetRenderAttachmentDepth(resources.activeDepthTexture, AccessFlags.Read);
                builder.SetRenderFunc(static (DetailData pass, RasterGraphContext context) => context.cmd.DrawRendererList(pass.list));
            }
        }
    }
}
