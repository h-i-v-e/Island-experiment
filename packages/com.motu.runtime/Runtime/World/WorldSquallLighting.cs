using System;
using System.Collections.Generic;
using Motu.Rendering;
using UnityEngine;
using UnityEngine.Experimental.Rendering;
using UnityEngine.Rendering.Universal;

namespace Motu.World
{
    // A projected light cookie lets ordinary URP ship/building materials receive
    // the same storm shadows as the custom terrain and ocean shaders.
    internal sealed class WorldSquallLighting : IDisposable
    {
        private static readonly int[] Clouds = { Shader.PropertyToID("_MotuSquallCloud0"),
            Shader.PropertyToID("_MotuSquallCloud1"), Shader.PropertyToID("_MotuSquallCloud2") };
        private static readonly int[] Shades = { Shader.PropertyToID("_MotuSquallShade0"),
            Shader.PropertyToID("_MotuSquallShade1"), Shader.PropertyToID("_MotuSquallShade2") };
        private readonly Dictionary<Light, Binding> bindings = new Dictionary<Light, Binding>();
        private Binding current;
        private RenderTexture cookie;
        private Material material;

        internal void Update(WorldSquallState[] cells, SquallWeatherSettings settings, Light source, Vector3 anchor)
        {
            var active = Publish(cells, settings);
            if (!active || source == null || !source.enabled || source.intensity <= 0)
            {
                current?.Restore(cookie); current = null;
                return;
            }
            if (!EnsureResources()) return;
            if (current == null || current.Light != source)
            {
                current?.Restore(cookie);
                if (!bindings.TryGetValue(source, out current))
                {
                    current = new Binding(source);
                    bindings.Add(source, current);
                }
            }
            DrawCookie(settings, source, anchor);
        }

        private static bool Publish(WorldSquallState[] cells, SquallWeatherSettings settings)
        {
            var active = false;
            for (var i = 0; i < Clouds.Length; ++i)
            {
                var cell = cells[i];
                var enabled = settings.Enabled && cell.Active && cell.Strength > .001f;
                var radius = enabled ? cell.DiameterMetres * .5f : 0;
                var thickness = Mathf.Max(10, settings.CloudThicknessMetres)
                    * Mathf.Clamp(Mathf.Sqrt(cell.DiameterMetres / 250), .65f, 1.45f);
                Shader.SetGlobalVector(Clouds[i], enabled ? new Vector4(cell.Center.x, cell.Center.z,
                    cell.Center.y + Mathf.Max(15, settings.CloudBaseMetres) + thickness * .35f, radius) : Vector4.zero);
                Shader.SetGlobalVector(Shades[i], enabled ? new Vector4(settings.CloudFringeFraction,
                    cell.Strength * settings.CloudDarkness, cell.AgeSeconds, (cell.Seed & 0xffff) / 655.35f) : Vector4.zero);
                active |= enabled && settings.CloudDarkness > 0;
            }
            return active;
        }

        private bool EnsureResources()
        {
            if (material != null) return true;
            var shader = Resources.Load<Shader>("SquallShadowCookie");
            if (shader == null || !shader.isSupported) return false;
            material = new Material(shader) { name = "Motu squall shadow cookie", hideFlags = HideFlags.HideAndDontSave };
            cookie = new RenderTexture(512, 512, 0, RenderTextureFormat.ARGB32, RenderTextureReadWrite.Linear)
            { name = "Motu moving storm shadow", wrapMode = TextureWrapMode.Clamp,
                filterMode = FilterMode.Bilinear, hideFlags = HideFlags.HideAndDontSave };
            cookie.Create();
            return true;
        }

        private void DrawCookie(SquallWeatherSettings settings, Light source, Vector3 anchor)
        {
            var span = Mathf.Max(4000, settings.VisibilityDistanceMetres * 2);
            var local = source.transform.InverseTransformPoint(anchor);
            var offset = new Vector2(local.x, local.y);
            var origin = source.transform.TransformPoint(new Vector3(offset.x, offset.y, 0));
            material.SetVector("_CookieOrigin", origin);
            material.SetVector("_CookieRight", source.transform.right * span);
            material.SetVector("_CookieUp", source.transform.up * span);
            material.SetVector("_CookieLightDirection", -source.transform.forward);
            material.SetFloat("_CookieSeaLevel", anchor.y);
            material.SetTexture("_PreviousCookie", current.Cookie != null ? current.Cookie : Texture2D.whiteTexture);
            material.SetFloat("_PreviousChannel", CookieChannel(current.Cookie));
            material.SetVector("_PreviousTransform", new Vector4(span / current.Size.x, span / current.Size.y,
                (offset.x - current.Offset.x) / current.Size.x + .5f, (offset.y - current.Offset.y) / current.Size.y + .5f));
            var previous = RenderTexture.active;
            try { Graphics.Blit(Texture2D.whiteTexture, cookie, material); }
            finally { RenderTexture.active = previous; }
            current.Data.lightCookieSize = Vector2.one * span;
            current.Data.lightCookieOffset = offset;
            source.cookie = cookie;
        }

        private static int CookieChannel(Texture texture)
        {
            if (texture == null) return 0;
            var format = texture.graphicsFormat;
            if ((int)format == 54 || (int)format == 55) return 1; // URP's legacy alpha-only cookie formats.
            return GraphicsFormatUtility.GetComponentCount(format) == 1 ? 2 : 0;
        }

        public void Dispose()
        {
            foreach (var binding in bindings.Values) binding.Dispose(cookie);
            bindings.Clear(); current = null;
            if (cookie != null) cookie.Release();
            UnityObjectLifetime.DestroyUnityObject(cookie);
            UnityObjectLifetime.DestroyUnityObject(material);
            cookie = null; material = null;
        }

        private sealed class Binding
        {
            internal readonly Light Light;
            internal readonly UniversalAdditionalLightData Data;
            internal readonly Texture Cookie;
            internal readonly Vector2 Size, Offset;
            private readonly bool addedData;

            internal Binding(Light light)
            {
                Light = light; Cookie = light.cookie;
                Data = light.GetComponent<UniversalAdditionalLightData>();
                addedData = Data == null;
                if (addedData) Data = light.gameObject.AddComponent<UniversalAdditionalLightData>();
                Size = new Vector2(Mathf.Max(.001f, Data.lightCookieSize.x), Mathf.Max(.001f, Data.lightCookieSize.y));
                Offset = Data.lightCookieOffset;
            }

            internal void Restore(Texture ownedCookie)
            {
                if (Light == null || Light.cookie != ownedCookie) return;
                Light.cookie = Cookie;
                if (Data == null) return;
                Data.lightCookieSize = Size; Data.lightCookieOffset = Offset;
            }

            internal void Dispose(Texture ownedCookie)
            {
                Restore(ownedCookie);
                // URP itself lazily adds this metadata in players. Keep it
                // available for a same-frame owner handoff; Play-mode scene
                // teardown removes it. Edit-mode audits restore exact ownership.
                if (addedData && Data != null && !Application.isPlaying)
                    UnityObjectLifetime.DestroyUnityObject(Data);
            }
        }
    }
}
