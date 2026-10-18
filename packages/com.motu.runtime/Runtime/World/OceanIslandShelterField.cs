using System;
using System.Collections.Generic;
using UnityEngine;

namespace Motu.World
{
    /// <summary>Low-resolution, world-space lee shelter shared by the ocean and its physics queries.</summary>
    [DefaultExecutionOrder(1200)]
    [DisallowMultipleComponent]
    [AddComponentMenu("Motu/Ocean Island Shelter Field")]
    public sealed class OceanIslandShelterField : MonoBehaviour
    {
        private sealed class Source
        {
            internal UnityEngine.Object Owner;
            internal Texture Mask;
            internal Transform Transform;
            internal float Size;
        }

        [SerializeField] private bool shelterEnabled = true;
        [Range(32, 512), SerializeField] private int resolution = 256;
        [Min(256), SerializeField] private float coverageMetres = 4096;
        [Tooltip("Distance downwind over which an island provides shelter. The first 65 percent forms the calm anchorage core.")]
        [Min(32), SerializeField] private float reachMetres = 1800;
        [Min(0), SerializeField] private float edgeSoftnessMetres = 40;
        [Tooltip("Time constant for sea-state changes after a wind turn. The field stays fixed to the islands as the ocean recentres.")]
        [Min(.1f), SerializeField] private float responseSeconds = 12;
        [Tooltip("Fraction of normal wave amplitude in the fully sheltered core, including shore breakers. One third gives gentler waves; 1 retains full-sized waves. Installed from the Ocean Wave Profile.")]
        [Range(0, 1), SerializeField] private float shelteredWaveFraction = 1f / 3f;

        private readonly List<Source> sources = new List<Source>();
        private OceanSurfaceController ocean;
        private Material generator;
        private RenderTexture target, history, next;
        private Vector4 worldRect;
        private Vector2 targetCentre, targetDirection;
        private bool dirty = true, historyValid;
        private float sinceComposition = 1;
        private int targetCompositionCount;

        private static readonly int TextureId = Shader.PropertyToID("_OceanShelterTex");
        private static readonly int RectId = Shader.PropertyToID("_OceanShelterWorldRect");
        private static readonly int ParametersId = Shader.PropertyToID("_OceanShelterParameters");
        private static readonly int TexelSizeId = Shader.PropertyToID("_OceanShelterTex_TexelSize");

        public bool Enabled { get => shelterEnabled; set { shelterEnabled = value; dirty = true; if (!value) BindOpenWater(); } }
        public int Resolution { get => resolution; set { resolution = Mathf.Clamp(value, 32, 512); dirty = true; } }
        public float CoverageMetres { get => coverageMetres; set { coverageMetres = FiniteAtLeast(value, 256); dirty = true; } }
        public float ReachMetres { get => reachMetres; set { reachMetres = FiniteAtLeast(value, 32); dirty = true; } }
        public float ResponseSeconds { get => responseSeconds; set => responseSeconds = FiniteAtLeast(value, .1f); }
        public float ShelteredWaveFraction { get => shelteredWaveFraction; set => shelteredWaveFraction = Mathf.Clamp01(FiniteAtLeast(value, 0)); }
        public RenderTexture Texture => history;
        /// <summary>World minimum XZ and inverse world extent.</summary>
        public Vector4 WorldRect => worldRect;
        public int SourceCount => sources.Count;
        public int TargetCompositionCount => targetCompositionCount;

        public void Configure(OceanSurfaceController owner)
        {
            if (owner == null) throw new ArgumentNullException(nameof(owner));
            if (ocean != null && ocean != owner) BindOpenWater();
            ocean = owner;
            dirty = true;
            if (isActiveAndEnabled && shelterEnabled) EnsureResources();
            else BindOpenWater();
        }

        public void Register(UnityEngine.Object owner, Texture mask, Transform island, float worldSize)
        {
            if (owner == null || mask == null || island == null) throw new ArgumentException("Shelter needs its owner, sea mask and island transform.");
            float size = FiniteAtLeast(worldSize, 1);
            foreach (var source in sources)
            {
                if (source.Owner != owner) continue;
                source.Mask = mask; source.Transform = island; source.Size = size;
                dirty = true;
                return;
            }
            if (sources.Count == 0) historyValid = false;
            sources.Add(new Source { Owner = owner, Mask = mask, Transform = island, Size = size });
            dirty = true;
        }

        public void Unregister(UnityEngine.Object owner)
        {
            for (int i = sources.Count - 1; i >= 0; i--)
                if (ReferenceEquals(sources[i].Owner, owner)) { sources.RemoveAt(i); dirty = true; }
        }

        /// <summary>Explicit world teardown. Ordinary ocean reinstallations preserve live island registrations.</summary>
        public void Clear()
        {
            sources.Clear();
            ResetHistory();
        }

        public void ResetHistory()
        {
            BindOpenWater();
            historyValid = false;
            dirty = true;
            ReleaseTextures();
        }

        private void LateUpdate()
        {
            if (ocean == null || ocean.SurfaceTransform == null) return;
            var p = ocean.SurfaceTransform.position;
            var wind = Shader.GetGlobalVector("_MotuWeatherWind");
            UpdateField(new Vector2(p.x, p.z), new Vector2(wind.x, wind.y), Time.deltaTime);
        }

        public void UpdateField(Vector2 centre, Vector2 windDirection, float deltaTime, bool forceTarget = false)
        {
            if (!float.IsFinite(centre.x) || !float.IsFinite(centre.y) || !float.IsFinite(windDirection.x)
                || !float.IsFinite(windDirection.y) || !float.IsFinite(deltaTime) || deltaTime < 0)
                throw new ArgumentException("Shelter position, wind and nonnegative elapsed time must be finite.");
            if (!isActiveAndEnabled || !shelterEnabled || ocean == null || !ocean.isActiveAndEnabled || ocean.SurfaceMaterial == null)
            { BindOpenWater(); return; }
            EnsureResources();
            for (int i = sources.Count - 1; i >= 0; i--)
            {
                var source = sources[i];
                if (source.Owner == null || source.Mask == null || source.Transform == null)
                { sources.RemoveAt(i); dirty = true; }
            }
            if (windDirection.sqrMagnitude < .000001f) windDirection = targetDirection.sqrMagnitude > .5f ? targetDirection : Vector2.right;
            else windDirection.Normalize();
            // Whole-texel shifts preserve stable island silhouettes. Reprojection
            // handles the history overlap, rather than translating the calm water.
            float snap = coverageMetres / resolution * 4;
            centre = new Vector2(Mathf.Round(centre.x / snap) * snap, Mathf.Round(centre.y / snap) * snap);
            sinceComposition += deltaTime;
            if (centre != targetCentre || Vector2.Dot(windDirection, targetDirection) < .9994f) dirty = true;
            var minimum = centre - Vector2.one * (coverageMetres * .5f);
            var compositionRect = new Vector4(minimum.x, minimum.y, coverageMetres, coverageMetres);
            var samplingRect = new Vector4(minimum.x, minimum.y, 1 / coverageMetres, 1 / coverageMetres);
            var previous = RenderTexture.active;
            try
            {
                if (dirty || forceTarget || sinceComposition >= .5f)
                {
                    ComposeTarget(compositionRect, windDirection);
                    targetCentre = centre; targetDirection = windDirection;
                    dirty = false; sinceComposition = 0; targetCompositionCount++;
                }
                generator.SetVector("_CompositionWorldRect", compositionRect);
                generator.SetTexture("_ShelterHistoryTex", history);
                generator.SetVector("_ShelterHistoryWorldRect", worldRect);
                generator.SetFloat("_ShelterHistoryValid", historyValid ? 1 : 0);
                generator.SetFloat("_ShelterBlend", 1 - Mathf.Exp(-deltaTime / responseSeconds));
                Graphics.Blit(target, next, generator, 1);
                (history, next) = (next, history);
                historyValid = true;
                worldRect = samplingRect;
            }
            finally { RenderTexture.active = previous; }
            var material = ocean.SurfaceMaterial;
            material.SetTexture(TextureId, history);
            material.SetVector(RectId, worldRect);
            material.SetVector(TexelSizeId, new Vector4(1f / resolution, 1f / resolution, resolution, resolution));
            material.SetVector(ParametersId, new Vector4(shelteredWaveFraction, 0, 1, 0));
        }

        private void ComposeTarget(Vector4 rect, Vector2 direction)
        {
            RenderTexture.active = target;
            GL.Clear(false, true, Color.clear);
            generator.SetVector("_CompositionWorldRect", rect);
            generator.SetVector("_ShelterWindDirection", new Vector4(direction.x, direction.y, 0, 0));
            generator.SetVector("_ShelterGenerationParameters", new Vector4(reachMetres, .65f, edgeSoftnessMetres, 1));
            foreach (var source in sources)
            {
                var position = source.Transform.position;
                var scale = source.Transform.lossyScale;
                float radius = source.Size * .75f * Mathf.Max(Mathf.Abs(scale.x), Mathf.Abs(scale.z)) + reachMetres + edgeSoftnessMetres;
                if (position.x + radius < rect.x || position.z + radius < rect.y
                    || position.x - radius > rect.x + rect.z || position.z - radius > rect.y + rect.w) continue;
                generator.SetTexture("_SeaMask", source.Mask);
                generator.SetMatrix("_IslandWorldToLocal", source.Transform.worldToLocalMatrix);
                generator.SetFloat("_IslandWorldSize", source.Size);
                Graphics.Blit(UnityEngine.Texture2D.blackTexture, target, generator, 0);
            }
        }

        private void EnsureResources()
        {
            if (generator == null)
            {
                var shader = Resources.Load<Shader>("OceanIslandShelter");
                if (shader == null || !shader.isSupported) throw new NotSupportedException("Island shelter shader is unavailable.");
                generator = new Material(shader) { hideFlags = HideFlags.HideAndDontSave };
            }
            if (target != null && target.width == resolution && target.IsCreated()) return;
            BindOpenWater();
            ReleaseTextures();
            target = MakeTexture("Island shelter target");
            history = MakeTexture("Island shelter history");
            next = MakeTexture("Island shelter blend");
            historyValid = false; dirty = true;
        }

        private RenderTexture MakeTexture(string label)
        {
            // Float history is necessary: even half precision rounds away small
            // per-frame changes near one and can prevent the lee becoming calm.
            var texture = new RenderTexture(resolution, resolution, 0, RenderTextureFormat.RFloat, RenderTextureReadWrite.Linear)
            { name = label, filterMode = FilterMode.Bilinear, wrapMode = TextureWrapMode.Clamp, hideFlags = HideFlags.HideAndDontSave };
            if (!texture.Create()) { Motu.Rendering.UnityObjectLifetime.DestroyUnityObject(texture); throw new InvalidOperationException("Could not allocate island shelter field."); }
            var previous = RenderTexture.active;
            RenderTexture.active = texture; GL.Clear(false, true, Color.clear); RenderTexture.active = previous;
            return texture;
        }

        private void BindOpenWater()
        {
            if (ocean == null || ocean.SurfaceMaterial == null) return;
            ocean.SurfaceMaterial.SetTexture(TextureId, UnityEngine.Texture2D.blackTexture);
            ocean.SurfaceMaterial.SetVector(ParametersId, Vector4.zero);
        }

        private void ReleaseTextures()
        {
            foreach (var texture in new[] { target, history, next })
            {
                if (texture == null) continue;
                texture.Release();
                Motu.Rendering.UnityObjectLifetime.DestroyUnityObject(texture);
            }
            target = history = next = null;
        }

        private void OnEnable() { dirty = true; }
        private void OnDisable() { BindOpenWater(); historyValid = false; }
        private void OnDestroy()
        {
            Clear();
            Motu.Rendering.UnityObjectLifetime.DestroyUnityObject(generator);
            generator = null;
        }
        private void OnValidate()
        {
            resolution = Mathf.Clamp(resolution, 32, 512);
            coverageMetres = FiniteAtLeast(coverageMetres, 256);
            reachMetres = FiniteAtLeast(reachMetres, 32);
            responseSeconds = FiniteAtLeast(responseSeconds, .1f);
            edgeSoftnessMetres = FiniteAtLeast(edgeSoftnessMetres, 0);
            shelteredWaveFraction = Mathf.Clamp01(FiniteAtLeast(shelteredWaveFraction, 0));
            dirty = true;
        }
        private static float FiniteAtLeast(float value, float minimum)
        {
            if (!float.IsFinite(value)) throw new ArgumentException("Shelter settings must be finite.");
            return Mathf.Max(value, minimum);
        }
    }
}
