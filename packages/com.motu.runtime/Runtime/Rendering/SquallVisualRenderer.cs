using System;
using System.Collections.Generic;
using Motu.Islands;
using Motu.World;
using UnityEngine;
using UnityEngine.Rendering;
using static Motu.Rendering.UnityObjectLifetime;

namespace Motu.Rendering
{
    // A single moving weather footprint owns these visuals. Horizon sinking is
    // applied only to this root; the weather field retains its true position.
    internal sealed class SquallVisualRenderer : IDisposable
    {
        private const int BoltSegments = 32, MainSegments = 16;
        private const float RainFallSpeed = 38f;
        private readonly GameObject root;
        private readonly Transform cloud;
        private readonly MeshRenderer cloudRenderer, boltRenderer;
        private readonly Mesh cloudMesh, boltMesh;
        private readonly Material cloudMaterial, dropMaterial, boltMaterial;
        private readonly MaterialPropertyBlock cloudProperties = new MaterialPropertyBlock();
        private readonly MaterialPropertyBlock boltProperties = new MaterialPropertyBlock();
        private readonly ParticleSystem drops;
        private readonly ParticleSystemRenderer dropRenderer;
        private readonly Light flashLight;
        private readonly Vector3[] mainPoints = new Vector3[MainSegments + 1];
        private readonly Segment[] segments = new Segment[BoltSegments];
        private readonly Vector3[] boltVertices = new Vector3[BoltSegments * 4];
        private readonly WorldSquallLightning lightning = new WorldSquallLightning();
        private int seed, emitted;
        private float previousAge = -1, dropBudget, distanceFade = 1;
        private bool disposed;
        private static readonly int TimeId = Shader.PropertyToID("_SquallTime");
        private static readonly int SeedId = Shader.PropertyToID("_SquallSeed");
        private static readonly int StrengthId = Shader.PropertyToID("_SquallStrength");
        private static readonly int FlashId = Shader.PropertyToID("_SquallFlash");
        private static readonly List<SquallVisualRenderer> visibleOwners = new List<SquallVisualRenderer>(3);
        private bool registered;
        private static readonly Plane[] frustumPlanes = new Plane[6];
        private static readonly int OriginId = Shader.PropertyToID("_SquallOrigin");
        private static readonly int BoundsId = Shader.PropertyToID("_SquallBounds");
        private static readonly int LayerId = Shader.PropertyToID("_SquallLayer");
        private static readonly int FlowId = Shader.PropertyToID("_SquallFlow");
        private static readonly int DarknessId = Shader.PropertyToID("_SquallDarkness");
        private static readonly int DistanceOpacityId = Shader.PropertyToID("_SquallDistanceOpacity");
        private static readonly int SeaLevelId = Shader.PropertyToID("_SquallSeaLevel");
        private static readonly int RainVelocityId = Shader.PropertyToID("_SquallRainVelocity");

        private struct Segment { internal Vector3 a, b; internal float width; }
        internal GameObject Root => root;
        internal bool Visible => !disposed && root != null && root.activeSelf;
        internal int RainParticleCount => drops != null ? drops.particleCount : 0;
        internal float FlashStrength { get; private set; }
        internal float HorizonSubmersion { get; private set; }
        internal float LightningStartAgeSeconds { get; private set; }

        // One depth copy serves every visible squall, including when another
        // cell is off camera. Each volume keeps its own material properties.
        internal static bool NeedsRender(Camera camera)
        {
            if (camera == null || visibleOwners.Count == 0) return false;
            GeometryUtility.CalculateFrustumPlanes(camera, frustumPlanes);
            foreach (var owner in visibleOwners)
                if (owner.Visible && owner.root.activeInHierarchy
                    && (camera.cullingMask & (1 << owner.root.layer)) != 0
                    && (GeometryUtility.TestPlanesAABB(frustumPlanes, owner.cloudRenderer.bounds)
                        || owner.RainParticleCount > 0 && GeometryUtility.TestPlanesAABB(frustumPlanes, owner.dropRenderer.bounds))) return true;
            return false;
        }

        internal SquallVisualRenderer(Transform parent)
        {
            if (parent == null) throw new ArgumentNullException(nameof(parent));
            try
            {
                root = new GameObject("Travelling squall") { layer = parent.gameObject.layer };
                root.transform.SetParent(parent, false);
                cloudMaterial = CreateMaterial("SquallCloud");
                dropMaterial = CreateMaterial("SquallRain");
                boltMaterial = CreateMaterial("SquallLightning");
                cloudMesh = CloudBox(); boltMesh = LightningMesh();
                cloudRenderer = Surface("Volumetric squall cloud and rain", cloudMesh, cloudMaterial); cloud = cloudRenderer.transform;
                boltRenderer = Surface("Branching lightning", boltMesh, boltMaterial);
                drops = CreateDrops();
                dropRenderer = drops.GetComponent<ParticleSystemRenderer>();
                flashLight = new GameObject("Squall lightning light").AddComponent<Light>();
                flashLight.transform.SetParent(root.transform, false);
                flashLight.type = LightType.Point; flashLight.color = new Color(.72f, .84f, 1);
                flashLight.shadows = LightShadows.None; flashLight.enabled = false;
                CloudNoiseTextures.EnsureBound();
                root.SetActive(false);
            }
            catch { Dispose(); throw; }
        }

        internal void UpdateVisuals(WorldSquallState state, SquallWeatherSettings settings, Vector3 viewer, float deltaTime,
            SquallThunderAudio thunder = null, Vector2 windVelocity = default)
        {
            if (disposed) throw new ObjectDisposedException(nameof(SquallVisualRenderer));
            if (!state.Active) { Hide(); lightning.Reset(); return; }
            if (!lightning.IsCurrent(state)) ResetAnimation(state.Seed);
            float diameter = Mathf.Clamp(state.DiameterMetres, SquallWeatherSettings.MinimumSupportedDiameterMetres,
                SquallWeatherSettings.MaximumSupportedDiameterMetres);
            float thickness = Mathf.Max(10, settings.CloudThicknessMetres) * Mathf.Clamp(Mathf.Sqrt(diameter / 250), .65f, 1.45f);
            float cloudBase = Mathf.Max(15, settings.CloudBaseMetres);
            if (lightning.Advance(state, settings))
            {
                BuildLightning(lightning.StrikeSeed, diameter, cloudBase + thickness * .55f);
                if (settings.LightningBrightness > 0)
                    thunder?.QueueStrike(state, settings, state.Center + mainPoints[MainSegments], viewer,
                        lightning.StrikeSeed, state.AgeSeconds - lightning.StrikeAge);
            }
            LightningStartAgeSeconds = lightning.StrikeAge;
            if (state.Strength <= .001f) { Hide(); return; }
            float distance = new Vector2(viewer.x - state.Center.x, viewer.z - state.Center.z).magnitude;
            float start = Mathf.Max(1, settings.HorizonStartMetres);
            float far = Mathf.Max(start + 1, settings.VisibilityDistanceMetres);
            float scaledDistance = start + (distance - start) * IslandRuntime.Lod3SubmersionRangeMetres / (far - start);
            HorizonSubmersion = IslandRuntime.Lod3Submersion(scaledDistance, start);
            if (distance >= far) { Hide(); return; }
            float rangeFraction = Mathf.InverseLerp(start, far, distance);
            float haze = Mathf.SmoothStep(0, 1, Mathf.InverseLerp(Mathf.Min(250f, far * .2f), far * .85f, distance));
            distanceFade = Mathf.Lerp(1f, settings.DistantOpacity, haze)
                * (1 - Mathf.SmoothStep(0, 1, Mathf.InverseLerp(.75f, 1, rangeFraction)));
            // Let the rain base sink towards the horizon while retaining the
            // tall cloud silhouette as a navigable warning in the distance.
            float sinking = cloudBase * .6f * HorizonSubmersion;
            root.SetActive(true);
            root.transform.SetPositionAndRotation(state.Center - Vector3.up * sinking, Quaternion.identity);
            if (!registered) { visibleOwners.Add(this); registered = true; }
            UpdateLightning(state, settings, viewer, diameter);
            var rainDrift = Vector2.ClampMagnitude(windVelocity * .85f, 40f);
            ConfigureVolume(state, settings, diameter, cloudBase, thickness, rainDrift);
            UpdateDrops(state, settings, viewer, diameter, cloudBase, rainDrift);
        }

        private void ConfigureVolume(WorldSquallState state, SquallWeatherSettings settings,
            float diameter, float cloudBase, float thickness, Vector2 rainDrift)
        {
            const float bottom = -24;
            float volumeHeight = cloudBase + thickness - bottom;
            float cloudDiameter = diameter * (1f + settings.CloudFringeFraction);
            // Enclose both the unchanged cloud and its downwind-swept rain,
            // including narrow fringes, tall cloud bases and strong winds.
            var sweep = rainDrift * ((cloudBase - bottom) / RainFallSpeed);
            var minimum = Vector2.Min(Vector2.one * (-cloudDiameter * .5f),
                Vector2.one * (-diameter * .5f) + Vector2.Min(Vector2.zero, sweep));
            var maximum = Vector2.Max(Vector2.one * (cloudDiameter * .5f),
                Vector2.one * (diameter * .5f) + Vector2.Max(Vector2.zero, sweep));
            var centre = (minimum + maximum) * .5f;
            cloud.localPosition = new Vector3(centre.x, bottom + volumeHeight * .5f, centre.y);
            cloud.localScale = new Vector3(maximum.x - minimum.x, volumeHeight, maximum.y - minimum.y);
            SetProperties(cloudProperties, state, settings.CloudDarkness);
            cloudProperties.SetVector(OriginId, root.transform.position);
            cloudProperties.SetVector(BoundsId, new Vector4(cloudDiameter, volumeHeight, bottom, diameter));
            float activity = state.SpeedMultiplier / Mathf.Max(.001f, settings.MaximumSpeedMultiplier);
            cloudProperties.SetVector(LayerId, new Vector4(cloudBase, thickness,
                Mathf.Max(0, settings.RainIntensity) * Mathf.Lerp(.4f, 1f, activity), 20));
            cloudProperties.SetFloat(DistanceOpacityId, distanceFade);
            cloudProperties.SetFloat(SeaLevelId, state.Center.y);
            cloudProperties.SetVector(RainVelocityId, new Vector4(rainDrift.x, rainDrift.y, RainFallSpeed, 0));
            // Internal billow advection follows ambient flow. Multiplying age
            // by a changing cell speed would make old storms churn ever faster.
            var noiseFlow = state.SpeedMultiplier > .001f ? state.Field.Velocity / state.SpeedMultiplier : Vector2.zero;
            cloudProperties.SetVector(FlowId, new Vector4(noiseFlow.x, noiseFlow.y, 0, 0));
            cloudRenderer.SetPropertyBlock(cloudProperties);
        }

        private void SetProperties(MaterialPropertyBlock properties, WorldSquallState state, float darkness)
        {
            properties.SetFloat(TimeId, state.AgeSeconds);
            properties.SetFloat(SeedId, Unit(Hash(unchecked((uint)state.Seed))) * 97);
            properties.SetFloat(StrengthId, state.Strength);
            properties.SetFloat(FlashId, FlashStrength);
            properties.SetFloat(DarknessId, Mathf.Clamp01(darkness));
        }

        private void UpdateLightning(WorldSquallState state, SquallWeatherSettings settings, Vector3 viewer, float diameter)
        {
            float age = state.AgeSeconds - LightningStartAgeSeconds;
            float pulse = age >= 0 && age < .075f ? 1 : age >= .11f && age < .155f ? .75f
                : age >= .215f && age < .26f ? .4f : 0;
            FlashStrength = pulse * Mathf.Max(0, settings.LightningBrightness) * state.Strength * distanceFade;
            boltRenderer.enabled = FlashStrength > 0;
            flashLight.enabled = FlashStrength > 0 && Vector3.Distance(viewer, state.Center) < 800;
            flashLight.range = Mathf.Clamp(diameter * .9f, 120, 450);
            flashLight.intensity = FlashStrength * 8;
            flashLight.transform.localPosition = mainPoints[0];
            if (!boltRenderer.enabled) return;
            WriteLightning(viewer);
            boltProperties.SetFloat(FlashId, FlashStrength);
            boltRenderer.SetPropertyBlock(boltProperties);
        }

        private void BuildLightning(uint hash, float diameter, float height)
        {
            Vector3 top = new Vector3((Unit(Hash(hash + 1)) - .5f) * diameter * .55f, height,
                (Unit(Hash(hash + 2)) - .5f) * diameter * .55f);
            Vector3 bottom = top + new Vector3((Unit(Hash(hash + 3)) - .5f) * diameter * .12f, -height - 4, 0);
            float jitter = Mathf.Clamp(diameter * .025f, 3, 11);
            for (int i = 0; i <= MainSegments; i++)
            {
                float t = i / (float)MainSegments;
                mainPoints[i] = Vector3.Lerp(top, bottom, t) + Jitter(hash, i) * (jitter * Mathf.Sin(t * Mathf.PI));
                if (i > 0) segments[i - 1] = new Segment { a = mainPoints[i - 1], b = mainPoints[i], width = 1 };
            }
            int count = MainSegments;
            Branch(hash + 21, mainPoints[5], 7, diameter, jitter, ref count);
            Branch(hash + 41, mainPoints[9], 5, diameter, jitter, ref count);
            Branch(hash + 61, mainPoints[12], 4, diameter, jitter, ref count);
        }

        private void Branch(uint hash, Vector3 start, int length, float diameter, float jitter, ref int count)
        {
            float angle = Unit(Hash(hash)) * Mathf.PI * 2;
            Vector3 end = start + new Vector3(Mathf.Cos(angle) * diameter * .15f, -start.y * .7f, Mathf.Sin(angle) * diameter * .15f);
            Vector3 previous = start;
            for (int i = 1; i <= length; i++)
            {
                float t = i / (float)length;
                Vector3 point = Vector3.Lerp(start, end, t) + Jitter(hash, i) * (jitter * .45f * Mathf.Sin(t * Mathf.PI));
                segments[count++] = new Segment { a = previous, b = point, width = .55f };
                previous = point;
            }
        }

        private void WriteLightning(Vector3 viewer)
        {
            Vector3 localViewer = root.transform.InverseTransformPoint(viewer);
            for (int i = 0; i < segments.Length; i++)
            {
                var segment = segments[i]; Vector3 direction = (segment.b - segment.a).normalized;
                Vector3 midpoint = (segment.a + segment.b) * .5f;
                Vector3 side = Vector3.Cross(direction, localViewer - midpoint).normalized;
                if (side.sqrMagnitude < .1f) side = Vector3.right;
                float width = Mathf.Max(.65f, Vector3.Distance(localViewer, midpoint) * .0015f) * segment.width;
                Vector3 extend = direction * (width * .25f); side *= width * .5f;
                int v = i * 4;
                boltVertices[v] = segment.a - extend - side; boltVertices[v + 1] = segment.a - extend + side;
                boltVertices[v + 2] = segment.b + extend - side; boltVertices[v + 3] = segment.b + extend + side;
            }
            boltMesh.vertices = boltVertices; boltMesh.RecalculateBounds();
        }

        private void UpdateDrops(WorldSquallState state, SquallWeatherSettings settings, Vector3 viewer,
            float diameter, float cloudBase, Vector2 rainDrift)
        {
            float advance = previousAge < 0 ? 0 : Mathf.Clamp(state.AgeSeconds - previousAge, 0, .1f);
            previousAge = state.AgeSeconds;
            Vector2 delta = new Vector2(viewer.x - state.Center.x, viewer.z - state.Center.z);
            delta -= rainDrift * (Mathf.Max(0, root.transform.position.y + cloudBase - viewer.y) / RainFallSpeed);
            // Match the distant rain volume: gentle at the rim, heaviest at the centre.
            float coverage = Mathf.SmoothStep(0, 1, 1 - delta.magnitude / (diameter * .5f));
            float localRain = coverage * state.Strength * Mathf.Max(0, settings.RainIntensity);
            localRain *= Mathf.Lerp(.4f, 1f, state.SpeedMultiplier / Mathf.Max(.001f, settings.MaximumSpeedMultiplier));
            drops.transform.position = viewer;
            if (advance <= 0) return;
            if (localRain > 0) EmitDrops(viewer, advance, localRain, rainDrift);
            else dropBudget = 0;
            // Stop emitting outside the cell, but let airborne world-space drops
            // finish falling rather than erasing them as the storm passes.
            drops.Simulate(advance, false, false, false);
            drops.Pause(false);
            dropMaterial.SetFloat(FlashId, FlashStrength);
        }

        private void EmitDrops(Vector3 viewer, float advance, float localRain, Vector2 drift)
        {
            dropBudget = Mathf.Min(dropBudget + advance * 1300 * localRain, 1800);
            int count = Mathf.Min(Mathf.FloorToInt(dropBudget), 1800 - drops.particleCount);
            // Discard whole emissions when the pool is full; resuming should
            // continue the rain rather than replaying a deferred burst.
            dropBudget -= Mathf.Floor(dropBudget);
            for (int i = 0; i < count; i++)
            {
                uint hash = Hash(unchecked((uint)seed) + unchecked((uint)++emitted) * 0x85ebca6bu);
                float angle = Unit(hash) * Mathf.PI * 2, radius = Mathf.Sqrt(Unit(Hash(hash + 1))) * 18;
                float height = 24 + Unit(Hash(hash + 2)) * 12;
                float fallSpeed = 34 + Unit(Hash(hash + 3)) * 10;
                // Emit upwind so slanting drops cross the viewer's vicinity
                // instead of blowing out of the nearby pool before arriving.
                var upwind = drift * (height / fallSpeed);
                var drop = new ParticleSystem.EmitParams
                {
                    position = viewer + new Vector3(Mathf.Cos(angle) * radius - upwind.x, height,
                        Mathf.Sin(angle) * radius - upwind.y),
                    velocity = new Vector3(drift.x, -fallSpeed, drift.y),
                    startLifetime = 1.35f, startSize = .035f + Unit(Hash(hash + 4)) * .025f,
                    startColor = new Color(.55f, .62f, .68f, .7f), randomSeed = hash,
                };
                drops.Emit(drop, 1);
            }
        }

        private ParticleSystem CreateDrops()
        {
            var child = new GameObject("Nearby heavy rain") { layer = root.layer };
            child.transform.SetParent(root.transform, false);
            var system = child.AddComponent<ParticleSystem>(); system.Stop(true, ParticleSystemStopBehavior.StopEmittingAndClear);
            var main = system.main; main.playOnAwake = false; main.loop = false; main.maxParticles = 1800;
            main.simulationSpace = ParticleSystemSimulationSpace.World; main.startSpeed = 0;
            main.cullingMode = ParticleSystemCullingMode.AlwaysSimulate;
            var emission = system.emission; emission.enabled = false;
            var shape = system.shape; shape.enabled = false;
            var colour = system.colorOverLifetime; colour.enabled = true;
            var fade = new Gradient();
            fade.SetKeys(new[] { new GradientColorKey(Color.white, 0), new GradientColorKey(Color.white, 1) },
                new[] { new GradientAlphaKey(0, 0), new GradientAlphaKey(1, .05f),
                    new GradientAlphaKey(1, .8f), new GradientAlphaKey(0, 1) });
            colour.color = fade;
            var renderer = system.GetComponent<ParticleSystemRenderer>(); renderer.sharedMaterial = dropMaterial;
            renderer.renderMode = ParticleSystemRenderMode.Stretch; renderer.velocityScale = .035f; renderer.lengthScale = 4;
            renderer.shadowCastingMode = ShadowCastingMode.Off; renderer.receiveShadows = false;
            return system;
        }

        private void ResetAnimation(int value)
        {
            seed = value; emitted = 0; dropBudget = 0; previousAge = -1;
            lightning.Reset();
            drops.Stop(true, ParticleSystemStopBehavior.StopEmittingAndClear);
            drops.useAutoRandomSeed = false; drops.randomSeed = unchecked((uint)value) | 1u;
            drops.Simulate(0, false, true, false);
            drops.Pause(false);
        }

        private void Unregister()
        {
            if (!registered) return;
            visibleOwners.Remove(this);
            registered = false;
        }

        private void Hide()
        {
            Unregister();
            root.SetActive(false); FlashStrength = 0; previousAge = -1; dropBudget = 0;
            drops.Stop(true, ParticleSystemStopBehavior.StopEmittingAndClear); flashLight.enabled = false;
        }

        private Material CreateMaterial(string resource) => new Material(Resources.Load<Shader>(resource)
            ?? throw new InvalidOperationException("Missing squall shader: " + resource))
            { name = resource, hideFlags = HideFlags.DontSave };

        private MeshRenderer Surface(string name, Mesh mesh, Material material)
        {
            var surface = new GameObject(name) { layer = root.layer }; surface.transform.SetParent(root.transform, false);
            surface.AddComponent<MeshFilter>().sharedMesh = mesh;
            var renderer = surface.AddComponent<MeshRenderer>(); renderer.sharedMaterial = material;
            renderer.shadowCastingMode = ShadowCastingMode.Off; renderer.receiveShadows = false;
            renderer.lightProbeUsage = LightProbeUsage.Off; renderer.reflectionProbeUsage = ReflectionProbeUsage.Off;
            renderer.motionVectorGenerationMode = MotionVectorGenerationMode.ForceNoMotion;
            return renderer;
        }

        private static Mesh CloudBox()
        {
            var mesh = new Mesh { name = "Squall cloud proxy", hideFlags = HideFlags.DontSave };
            mesh.vertices = new[] { new Vector3(-.5f,-.5f,-.5f), new Vector3(.5f,-.5f,-.5f), new Vector3(.5f,.5f,-.5f), new Vector3(-.5f,.5f,-.5f),
                new Vector3(-.5f,-.5f,.5f), new Vector3(.5f,-.5f,.5f), new Vector3(.5f,.5f,.5f), new Vector3(-.5f,.5f,.5f) };
            mesh.triangles = new[] { 0,2,1,0,3,2,4,5,6,4,6,7,0,4,7,0,7,3,1,2,6,1,6,5,0,1,5,0,5,4,3,7,6,3,6,2 };
            mesh.RecalculateBounds(); return mesh;
        }

        private Mesh LightningMesh()
        {
            var indices = new int[BoltSegments * 6]; var uv = new Vector2[boltVertices.Length];
            for (int i = 0; i < BoltSegments; i++)
            {
                int v = i * 4, t = i * 6;
                uv[v] = Vector2.zero; uv[v + 1] = Vector2.right; uv[v + 2] = Vector2.up; uv[v + 3] = Vector2.one;
                indices[t] = v; indices[t + 1] = v + 1; indices[t + 2] = v + 2;
                indices[t + 3] = v + 2; indices[t + 4] = v + 1; indices[t + 5] = v + 3;
            }
            var mesh = new Mesh { name = "Squall lightning forks", hideFlags = HideFlags.DontSave };
            mesh.MarkDynamic(); mesh.vertices = boltVertices; mesh.uv = uv; mesh.triangles = indices; return mesh;
        }

        private static Vector3 Jitter(uint seed, int index) => new Vector3(Unit(Hash(seed + (uint)index * 3)) - .5f,
            0, Unit(Hash(seed + (uint)index * 3 + 1)) - .5f);
        private static float Unit(uint value) => (value & 0xffffffu) / 16777216f;
        private static uint Hash(uint value)
        {
            unchecked { value ^= value >> 16; value *= 0x7feb352du; value ^= value >> 15; value *= 0x846ca68bu; return value ^ (value >> 16); }
        }

        public void Dispose()
        {
            if (disposed) return;
            disposed = true;
            Unregister();
            DestroyUnityObject(root); DestroyUnityObject(cloudMesh); DestroyUnityObject(boltMesh);
            DestroyUnityObject(cloudMaterial); DestroyUnityObject(dropMaterial); DestroyUnityObject(boltMaterial);
        }
    }
}
