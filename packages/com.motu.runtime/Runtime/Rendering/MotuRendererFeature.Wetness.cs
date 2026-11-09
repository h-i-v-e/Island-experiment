using UnityEngine;
using UnityEngine.Rendering;
using UnityEngine.Rendering.RenderGraphModule;
using UnityEngine.Rendering.Universal;
using UnityEngine.Rendering.Universal.Internal;

namespace Motu.Rendering
{
    public sealed partial class MotuRendererFeature
    {
        private sealed class SquallWetnessPass : ScriptableRenderPass
        {
            private readonly CopyDepthPass surfaceDepth;
            private sealed class Data
            {
                internal TextureHandle source, depth, destination;
                internal Material material;
            }

            internal SquallWetnessPass()
            {
                // After shell geometry/AO, before sky, water, rain and URP's
                // opaque-colour copy used by water refraction.
                renderPassEvent = (RenderPassEvent)((int)RenderPassEvent.AfterRenderingSkybox - 1);
                surfaceDepth = new CopyDepthPass(renderPassEvent, Shader.Find("Hidden/Universal Render Pipeline/CopyDepth"));
                ConfigureInput(ScriptableRenderPassInput.Depth);
                requiresIntermediateTexture = true;
            }

            internal void Dispose() => surfaceDepth.Dispose();

            public override void RecordRenderGraph(RenderGraph graph, ContextContainer frame)
            {
                var camera = frame.Get<UniversalCameraData>().camera;
                if (!camera.TryGetComponent<SquallSurfaceWetness>(out var wetness)) return;
                var material = wetness.PrepareMaterial();
                var resources = frame.Get<UniversalResourceData>();
                if (material == null || resources.isActiveTargetBackBuffer) return;
                surfaceDepth.Render(graph, frame, resources.cameraDepthTexture,
                    resources.activeDepthTexture, true, "Motu wet surface depth");
                var descriptor = resources.activeColorTexture.GetDescriptor(graph);
                descriptor.depthBufferBits = DepthBits.None;
                descriptor.clearBuffer = false;
                descriptor.name = "Motu wet surfaces";
                var destination = graph.CreateTexture(descriptor);
                using var builder = graph.AddUnsafePass<Data>(descriptor.name, out var data);
                data.source = resources.activeColorTexture;
                data.depth = resources.cameraDepthTexture;
                data.destination = destination;
                data.material = material;
                builder.UseTexture(data.source, AccessFlags.Read);
                builder.UseTexture(data.depth, AccessFlags.Read);
                if (resources.mainShadowsTexture.IsValid()) builder.UseTexture(resources.mainShadowsTexture, AccessFlags.Read);
                builder.UseTexture(data.destination, AccessFlags.WriteAll);
                builder.AllowGlobalStateModification(true);
                builder.SetRenderFunc(static (Data pass, UnsafeGraphContext context) =>
                {
                    var cmd = CommandBufferHelpers.GetNativeCommandBuffer(context.cmd);
                    cmd.SetGlobalTexture("_CameraDepthTexture", (RTHandle)pass.depth);
                    Blitter.BlitCameraTexture(cmd, (RTHandle)pass.source, (RTHandle)pass.destination, pass.material, 0);
                });
                resources.cameraColor = destination;
            }
        }
    }
}
