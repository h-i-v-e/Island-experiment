using System;
using UnityEngine;
using UnityEngine.Rendering;
using UnityEngine.Rendering.Universal;

namespace Motu.Editor
{
    internal static class UrpTestCamera
    {
        internal static void Render(Camera camera)
        {
            if (camera.targetTexture == null) throw new InvalidOperationException("Render tests require an explicit target.");
            camera.GetUniversalAdditionalCameraData();
            RenderPipeline.SubmitRenderRequest(camera,
                new UniversalRenderPipeline.SingleCameraRequest { destination = camera.targetTexture });
        }
    }
}
