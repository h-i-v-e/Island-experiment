using System.Collections.Generic;
using UnityEngine;
using UnityEngine.Rendering;

namespace Motu.Rendering
{
    // Compose stamps once into a local world-space field, avoiding a texture
    // sample per wake section at every ocean vertex and fragment.
    internal static class OceanShipWaveField
    {
        private const float Span = 512;
        private static RenderTexture field;
        private static Material material;
        private static Mesh quad;
        private static CommandBuffer commands;
        private static readonly MaterialPropertyBlock Properties = new MaterialPropertyBlock();
        private static readonly int TextureId = Shader.PropertyToID("_StampTexture");
        private static readonly int StrengthId = Shader.PropertyToID("_StampStrength");
        private static readonly int RectId = Shader.PropertyToID("_MotuShipWaveRect");
        private static readonly int FieldId = Shader.PropertyToID("_MotuShipWaveField");
        private static readonly int EnabledId = Shader.PropertyToID("_MotuShipWaveEnabled");

        internal readonly struct StampWriter
        {
            private readonly Vector3 centre;
            internal StampWriter(Vector3 centre) => this.centre = centre;

            internal void Draw(Texture2D texture, Vector3 position, Vector3 forward, Vector2 size,
                float clamp, float height, float foam)
            {
                var reach = size.magnitude * .5f;
                if (Mathf.Abs(position.x - centre.x) > Span * .5f + reach ||
                    Mathf.Abs(position.z - centre.z) > Span * .5f + reach) return;
                Properties.Clear();
                Properties.SetTexture(TextureId, texture);
                Properties.SetVector(StrengthId, new Vector4(clamp, height, height * foam, 0));
                commands.DrawMesh(quad, Matrix4x4.TRS(position, Quaternion.LookRotation(forward),
                    new Vector3(Mathf.Max(.1f, size.x), 1, Mathf.Max(.1f, size.y))), material, 0, 0, Properties);
            }
        }

        internal static void Render(IReadOnlyList<OceanDeckWaveClamp> ships, Vector3 centre, float now)
        {
            var anyTextures = false;
            foreach (var ship in ships)
                if (ship != null && ship.isActiveAndEnabled && ship.HasHullTexture) { anyTextures = true; break; }
            if (!anyTextures) { Shader.SetGlobalFloat(EnabledId, 0); return; }
            if (field == null)
            {
                var shader = Resources.Load<Shader>("OceanShipWaveStamp");
                if (shader == null) { Shader.SetGlobalFloat(EnabledId, 0); return; }
                material = new Material(shader) { hideFlags = HideFlags.HideAndDontSave };
                field = new RenderTexture(1024, 1024, 0, RenderTextureFormat.ARGBHalf, RenderTextureReadWrite.Linear)
                {
                    name = "Ship wave displacement field", hideFlags = HideFlags.HideAndDontSave,
                    filterMode = FilterMode.Bilinear, wrapMode = TextureWrapMode.Clamp,
                };
                field.Create();
                quad = new Mesh { name = "Ship wave stamp quad", hideFlags = HideFlags.HideAndDontSave };
                quad.vertices = new[] { new Vector3(-.5f, 0, -.5f), new Vector3(.5f, 0, -.5f), new Vector3(.5f, 0, .5f), new Vector3(-.5f, 0, .5f) };
                quad.uv = new[] { Vector2.zero, Vector2.right, Vector2.one, Vector2.up };
                quad.triangles = new[] { 0, 1, 2, 0, 2, 3 };
                commands = new CommandBuffer { name = "Ship bow waves and wake" };
            }
            centre.x = Mathf.Round(centre.x / 8) * 8;
            centre.z = Mathf.Round(centre.z / 8) * 8;
            var rect = new Vector4(centre.x - Span * .5f, centre.z - Span * .5f, 1 / Span, 1 / Span);
            commands.Clear();
            commands.SetRenderTarget(field);
            commands.SetViewport(new Rect(0, 0, field.width, field.height));
            commands.ClearRenderTarget(false, true, Color.clear);
            commands.SetGlobalVector(RectId, rect);
            var writer = new StampWriter(centre);
            foreach (var ship in ships)
                if (ship != null && ship.isActiveAndEnabled) ship.DrawStamps(writer, now);
            // Immediate rendering must restore the caller's target (including reflection cameras).
            var previousTarget = RenderTexture.active;
            Graphics.ExecuteCommandBuffer(commands);
            RenderTexture.active = previousTarget;
            Shader.SetGlobalTexture(FieldId, field);
            Shader.SetGlobalVector(RectId, rect);
            Shader.SetGlobalFloat(EnabledId, 1);
        }

        internal static void Release()
        {
            Shader.SetGlobalFloat(EnabledId, 0);
            Shader.SetGlobalTexture(FieldId, Texture2D.blackTexture);
            commands?.Release();
            commands = null;
            UnityObjectLifetime.DestroyUnityObject(field);
            UnityObjectLifetime.DestroyUnityObject(material);
            UnityObjectLifetime.DestroyUnityObject(quad);
            field = null;
            material = null;
            quad = null;
        }
    }
}
