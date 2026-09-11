using System;
using Motu.Rendering;
using UnityEngine;

namespace Motu.World
{
    // Per-ocean history, updated once per frame rather than once per camera.
    // Rect reprojection preserves world positions when the ocean follows a viewer.
    internal sealed class OceanFoamHistory : IDisposable
    {
        internal const int Resolution = 256;
        internal const float Span = 256;
        private Material material;
        private RenderTexture current, scratch;
        private Vector4 previousRect;
        private bool valid;
        private int lastFrame = -1;
        internal RenderTexture Texture => current;
        internal Vector4 WorldRect => previousRect;

        internal void Update(Material ocean, Vector3 centre, float deltaTime)
        {
            if (ocean == null || deltaTime <= 0 || lastFrame == Time.frameCount) return;
            lastFrame = Time.frameCount;
            if (!SystemInfo.SupportsRenderTextureFormat(RenderTextureFormat.ARGBHalf)) return;
            if (material == null)
            {
                var shader = Resources.Load<Shader>("OceanFoamHistory");
                if (shader == null || !shader.isSupported) return;
                material = new Material(shader) { hideFlags = HideFlags.HideAndDontSave };
                current = CreateTexture();
                scratch = CreateTexture();
            }
            var rect = new Vector4(Mathf.Floor(centre.x / 4) * 4 - Span * .5f,
                Mathf.Floor(centre.z / 4) * 4 - Span * .5f, 1 / Span, 1 / Span);
            if (deltaTime > .25f || Mathf.Abs(rect.x - previousRect.x) >= Span || Mathf.Abs(rect.y - previousRect.y) >= Span)
                valid = false;
            material.CopyPropertiesFromMaterial(ocean);
            material.SetTexture("_MotuWindNoise", Shader.GetGlobalTexture("_MotuWindNoise") ?? Texture2D.grayTexture);
            var wind = Shader.GetGlobalVector("_MotuWeatherWind");
            material.SetVector("_MotuWeatherWind", wind);
            material.SetVector("_MotuWindOffset", Shader.GetGlobalVector("_MotuWindOffset"));
            material.SetVector("_FoamWindDrift", new Vector4(wind.x, wind.y, 0, 0) * Mathf.Min(Mathf.Max(wind.z, 0), 20) * .02f);
            material.SetVector("_FoamPreviousRect", previousRect);
            material.SetVector("_FoamCurrentRect", rect);
            material.SetFloat("_FoamDeltaTime", Mathf.Min(deltaTime, .25f));
            material.SetFloat("_FoamHistoryValid", valid ? 1 : 0);
            // Bind explicitly: the ocean material has a different main texture.
            // Blit's automatic _MainTex binding is unsuitable after copying it.
            material.SetTexture("_FoamPrevious", current);
            var oldTarget = RenderTexture.active;
            try { Graphics.Blit(Texture2D.blackTexture, scratch, material); }
            finally { RenderTexture.active = oldTarget; }
            (current, scratch) = (scratch, current);
            previousRect = rect;
            valid = true;
            ocean.SetTexture("_OceanFoamHistory", current);
            ocean.SetVector("_OceanFoamHistoryRect", rect);
        }

        private static RenderTexture CreateTexture()
        {
            var texture = new RenderTexture(Resolution, Resolution, 0, RenderTextureFormat.ARGBHalf, RenderTextureReadWrite.Linear)
            { name = "Ocean foam history", filterMode = FilterMode.Bilinear, wrapMode = TextureWrapMode.Clamp, hideFlags = HideFlags.HideAndDontSave };
            texture.Create();
            return texture;
        }

        public void Dispose()
        {
            UnityObjectLifetime.DestroyUnityObject(current);
            UnityObjectLifetime.DestroyUnityObject(scratch);
            UnityObjectLifetime.DestroyUnityObject(material);
            current = scratch = null;
            material = null;
            valid = false;
            lastFrame = -1;
        }
    }
}
