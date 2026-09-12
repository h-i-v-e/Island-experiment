using static Motu.Rendering.UnityObjectLifetime;
using System;
using System.Threading;
using System.Threading.Tasks;
using UnityEngine;
using Debug = UnityEngine.Debug;
using Motu.Interop;
using Motu.Rendering;
using Motu.Settings;
using Motu.Streaming;
using Motu.World;

namespace Motu.Islands
{
    [UnityEngine.Scripting.APIUpdating.MovedFrom(true, sourceNamespace: "", sourceAssembly: "Assembly-CSharp", sourceClassName: "IslandGenerator")]
    public sealed partial class IslandGenerator : MonoBehaviour, IWorldSurfaceQuery
    {
        private const float SeaHeight = 0f;
        private const float RockPatchNoiseDetailScale = 8f;
        private static readonly int IslandWorldToLocalId = Shader.PropertyToID(
            "_IslandWorldToLocal");
        private static readonly int WaterSkyExposureId = Shader.PropertyToID(
            "_WaterSkyExposure");
        private static readonly int NightStrengthId = Shader.PropertyToID(
            "_MotuNightStrength");

        [Header("Streaming")]
        [SerializeField] private IslandStreamingSettings streaming = new IslandStreamingSettings();

        [HideInInspector] [SerializeField]
        private IslandCloudSettings clouds = new IslandCloudSettings();

        private NativeIslandHandle islandHandle;
        private IslandRuntime islandRuntime;
        private TerrainTileStreamer terrainStreamer;
        private GameObject runtimeRoot;
        private WorldEnvironmentController worldEnvironment;
        private Material terrainMaterial;
        private Material terrainLod1Material;
        private Material terrainLod2Material;
        private Material grassMaterial;
        private Texture2D terrainNormalTexture;
        private Texture2D terrainOcclusionTexture;
        private Texture2D seaMaskTexture;
        private Texture3D cliffNoiseTexture;
        private Texture2D riverNoiseTexture;
        private Texture2D grassPatchNoiseTexture;
        private TerrainMaterialTextureArrays terrainMaterialTextures;
        private Material rockMaterial;
        private Material riverMaterial;
        private Material seaMaterial;
        private Material meshEdgeMaterial;
        internal Material treeWoodMaterial;
        private Material treeLod1WoodMaterial;
        private Texture2D treeBarkAlbedoTexture;
        private Texture2D treeBarkHeightTexture;
        private Texture2D treeBarkNormalTexture;
        private Texture2D treeBarkOcclusionTexture;
        private Material treeFoliageMaterial;
        private Material treeLod0FoliageMaterial;
        private Material reedMaterial;
        private Material fernMaterial;
        private string status = "Ready";
        private readonly IslandGenerationLifecycle generationLifecycle =
            new IslandGenerationLifecycle();
        private IslandGenerationProfile activeProfile;
        private IslandRuntimeLoop runtimeLoop;
        private bool ownsCliffNoiseTexture;
        private bool ownsRiverNoiseTexture;
        private bool? appliedShowRivers;
        private bool? appliedShowGrass;
        private bool? appliedShowRocks;
        private bool? appliedShowForests;
        private bool? appliedShowReeds;
        private Color? appliedReedBaseColour;
        private Color? appliedReedTipColour;
        private bool? appliedShowFerns;
        private Color? appliedFernBaseColour;
        private Color? appliedFernTipColour;
        private bool? appliedShowMeshEdges;
        private bool? appliedShowTreeMeshEdges;
        private bool? appliedWaterfallDebug;
        private Color? appliedGrassColourA;
        private Color? appliedGrassColourB;
        private float appliedGrassColourNoiseWorldSize = float.NaN;
        private float currentSkyExposure = 1f;
        private float currentNightStrength;
        private Matrix4x4 appliedWorldToLocal;
        private bool hasAppliedWorldToLocal;

        public bool IsGenerating => generationLifecycle.IsGenerating;
        public bool HasActiveRuntime => islandRuntime != null
            && islandRuntime.State == IslandRuntimeState.Active;
        public bool HasRuntime => islandRuntime != null
            && islandRuntime.State != IslandRuntimeState.Disposed;
        internal bool HasInstalledWorldEnvironment => worldEnvironment != null
            && worldEnvironment.SkyMaterial != null
            && worldEnvironment.SeaMaterial != null;
        internal IslandRuntime Runtime => islandRuntime;
        public string Status => status;
        public float WorldSizeMetres => Generation.WorldSizeMetres;
        private IslandGenerationProfile Profile =>
            activeProfile ?? throw new InvalidOperationException(
                "IslandGenerator requires a complete IslandGenerationRequest before use.");
        public IslandGenerationSettings Generation => Profile.Generation;
        public IslandRiverSettings Rivers => Profile.Rivers;
        public IslandForestSettings Forest => Profile.Forest;
        public IslandReedSettings Reeds => Profile.Reeds;
        public IslandFernSettings Ferns => Profile.Ferns;
        public IslandCaveSettings Caves => Profile.Caves;
        public IslandStreamingSettings Streaming => streaming;
        public IslandRenderingSettings Rendering => Profile.Rendering;
        public IslandDebugSettings DebugSettings => Profile.DebugSettings;
        private IslandRuntimeLoop RuntimeLoop =>
            runtimeLoop ??= new IslandRuntimeLoop(this);

        private void OnEnable()
        {
            RuntimeLoop.Enable();
        }

        private void OnDisable()
        {
            RuntimeLoop.Disable();
        }

        internal static void EnsureCameraDepthTexture(Camera camera)
        {
            if (camera != null && !PlanarWaterReflection.IsReflectionCamera(camera))
            {
                camera.depthTextureMode |= DepthTextureMode.Depth;
            }
        }

        private void Update()
        {
            RuntimeLoop.Update();
        }

        private void OnValidate()
        {
            if (!generationLifecycle.IsGenerating && islandRuntime == null)
            {
                activeProfile = null;
            }
            if (!HasSupportedTransform())
            {
                Debug.LogWarning(
                    "IslandGenerator currently requires a unit scale and rotation around the Y axis only.",
                    this);
            }
        }

        private void OnDestroy()
        {
            generationLifecycle.MarkDestroyed();
            ClearGeneratedContent();
            DestroyRuntimeMaterials();
        }

        private bool HasSupportedTransform()
        {
            var scale = transform.lossyScale;
            return Mathf.Approximately(scale.x, 1f)
                && Mathf.Approximately(scale.y, 1f)
                && Mathf.Approximately(scale.z, 1f)
                && Vector3.Dot(transform.up, Vector3.up) > 0.99999f;
        }

        internal async Task<bool> GenerateAsync(
            IslandGenerationRequest request,
            CancellationToken externalCancellation,
            float installationFrameBudgetMilliseconds = 4f)
        {
            if (request == null) throw new ArgumentNullException(nameof(request));
            if (generationLifecycle.IsGenerating)
            {
                return false;
            }
            if (!HasSupportedTransform())
            {
                status = "Island transform must use unit scale and Y-axis rotation only.";
                Debug.LogError(status, this);
                return false;
            }

            request.ApplyProfileTo(this);
            if (!generationLifecycle.TryBegin(externalCancellation, out var cancellation))
            {
                return false;
            }
            status = "Generating island on CPU in background...";
            IslandPreparedData prepared = null;
            var installationStarted = false;
            var islandSeed = request.RandomSeed;
            var worldSize = request.WorldSizeMetres;
            var descriptor = request.Descriptor;
            var installationBudget = new UnityFrameBudget(
                installationFrameBudgetMilliseconds);

            try
            {
                prepared = await IslandGenerationWorker.GenerateAsync(
                    request,
                    cancellation.Token);
                cancellation.Token.ThrowIfCancellationRequested();
                if (generationLifecycle.IsDestroyed || !isActiveAndEnabled)
                {
                    return false;
                }

                status = "Uploading generated island...";
                installationStarted = true;
                var runtimeInstaller = new IslandRuntimeInstaller(this);
                await runtimeInstaller.InstallAsync(
                    prepared,
                    descriptor,
                    worldSize,
                    cancellation.Token,
                    installationBudget);

                generationLifecycle.StopTimer();
                status = IslandGenerationStatus.Installed(terrainStreamer, generationLifecycle.Elapsed,
                    prepared.materialTextures.loadedFromCache, worldSize, prepared.loadedFromSnapshot, islandSeed,
                    prepared.caves.caves.Length, prepared.caves.branchCount);
                return true;
            }
            catch (OperationCanceledException)
            {
                if (installationStarted)
                {
                    ClearGeneratedContent();
                    DestroyRuntimeMaterials();
                }
                if (!generationLifecycle.IsDestroyed)
                {
                    status = "Generation cancelled.";
                }
                return false;
            }
            catch (Exception exception)
            {
                status = exception.Message;
                Debug.LogException(exception);
                if (installationStarted)
                {
                    islandRuntime?.MarkFailed();
                    ClearGeneratedContent();
                    DestroyRuntimeMaterials();
                }
                return false;
            }
            finally
            {
                prepared?.Dispose();
                generationLifecycle.End(cancellation);
            }
        }

        public void Clear()
        {
            generationLifecycle.Cancel();
            ClearGeneratedContent();
            status = "Cleared";
        }

        public void SetStreamingTarget(Transform target)
        {
            Streaming.Target = target;
            if (terrainStreamer != null && target != null)
            {
                terrainStreamer.SetPlayerPosition(target.position);
            }
            else if (target == null)
            {
                terrainStreamer?.ClearPlayerFocus();
            }
        }

        public void PrepareStreamingAt(Vector3 worldPosition)
        {
            terrainStreamer?.SetPlayerPosition(worldPosition);
        }

        public void ClearStreamingFocus()
        {
            terrainStreamer?.ClearPlayerFocus();
        }

        internal void ConfigureWorldManagement(WorldEnvironmentController environment)
        {
            if (generationLifecycle.IsGenerating || islandRuntime != null)
            {
                throw new InvalidOperationException(
                    "World management must be configured before island generation starts.");
            }
            worldEnvironment = environment
                ?? throw new ArgumentNullException(nameof(environment));
        }

        internal void ApplyRequestProfile(IslandGenerationProfile profile)
        {
            if (profile == null) throw new ArgumentNullException(nameof(profile));
            if (generationLifecycle.IsGenerating || islandRuntime != null)
            {
                throw new InvalidOperationException(
                    "An island request profile must be applied before generation starts.");
            }
            activeProfile = profile.Clone();
        }

        public void Configure(
            IslandGenerationRequest request,
            Transform streamingTarget = null)
        {
            if (request == null) throw new ArgumentNullException(nameof(request));
            if (generationLifecycle.IsGenerating || islandRuntime != null)
            {
                throw new InvalidOperationException(
                    "An island request must be selected before generation starts.");
            }
            request.ApplyProfileTo(this);
            Streaming.Target = streamingTarget;
        }

        internal void SetRuntimeDormant(bool dormant)
        {
            if (islandRuntime == null
                || islandRuntime.State == IslandRuntimeState.Disposed
                || islandRuntime.State == IslandRuntimeState.Installing)
            {
                return;
            }
            if (dormant)
            {
                terrainStreamer?.ClearPlayerFocus();
                Streaming.Target = null;
            }
            islandRuntime.SetDormant(dormant);
        }


        internal void SyncSharedWorldLighting(WorldEnvironmentController environment)
        {
            if (environment == null)
            {
                return;
            }
            currentSkyExposure = environment.SkyExposure;
            currentNightStrength = environment.NightStrength;
            riverMaterial?.SetFloat(WaterSkyExposureId, currentSkyExposure);
            treeFoliageMaterial?.SetFloat(NightStrengthId, currentNightStrength);
            var light = RenderSettings.sun;
            var direction = light != null ? -light.transform.forward : Vector3.down;
            var colour = light != null ? light.color * light.intensity : Color.black;
            grassMaterial?.SetVector("_GrassLightDirection", direction);
            grassMaterial?.SetColor("_GrassLightColor", colour);
            grassMaterial?.SetColor("_GrassAmbientColor", RenderSettings.ambientLight);
        }

        public bool TryRaycastOverview(Ray worldRay, out Vector3 worldPoint)
        {
            if (terrainStreamer != null)
            {
                return terrainStreamer.TryRaycastOverview(worldRay, out worldPoint);
            }
            worldPoint = default;
            return false;
        }

        public bool TrySnapToTerrain(Vector3 approximateWorldPoint, out Vector3 worldPoint)
        {
            if (terrainStreamer != null)
            {
                return terrainStreamer.TrySnapToCurrentCollider(
                    approximateWorldPoint,
                    out worldPoint);
            }
            worldPoint = approximateWorldPoint;
            return false;
        }

        public float GetTerrainOrSeaHeight(Vector3 approximateWorldPoint)
        {
            var surfaceHeight = transform.TransformPoint(Vector3.up * SeaHeight).y;
            if (TrySnapToTerrain(approximateWorldPoint, out var terrainPoint))
            {
                surfaceHeight = Mathf.Max(surfaceHeight, terrainPoint.y);
            }
            return surfaceHeight;
        }

        public void ConfigureSceneReferences(
            Transform streamingTarget,
            Material terrainTemplate,
            Material grassTemplate,
            Material riverTemplate,
            Material seaTemplate,
            Material rockTemplate,
            Material treeFoliageTemplate = null)
        {
            Streaming.Target = streamingTarget;
            Rendering.AssignMaterialTemplates(
                terrainTemplate,
                grassTemplate,
                riverTemplate,
                seaTemplate,
                rockTemplate,
                treeFoliageTemplate);
        }

        private void ClearGeneratedContent()
        {
            if (islandRuntime != null)
            {
                islandRuntime.Dispose();
                islandRuntime = null;
                ClearIslandRuntimeAliases();
                ResetAppliedLiveSettings();
                return;
            }
            if (terrainStreamer != null)
            {
                terrainStreamer.Dispose();
                DestroyUnityObject(terrainStreamer.gameObject);
                terrainStreamer = null;
            }
            DestroyUnityObject(runtimeRoot);
            runtimeRoot = null;
            terrainMaterial?.SetTexture("_WorldNormal", null);
            terrainMaterial?.SetTexture("_Occlusion", null);
            terrainLod1Material?.SetTexture("_WorldNormal", null);
            terrainLod1Material?.SetTexture("_Occlusion", null);
            terrainLod2Material?.SetTexture("_WorldNormal", null);
            terrainLod2Material?.SetTexture("_Occlusion", null);
            DestroyUnityObject(terrainNormalTexture);
            DestroyUnityObject(terrainOcclusionTexture);
            DestroyUnityObject(seaMaskTexture);
            terrainNormalTexture = null;
            terrainOcclusionTexture = null;
            seaMaskTexture = null;

            islandHandle?.Dispose();
            islandHandle = null;
            ResetAppliedLiveSettings();
        }

        private void ClearIslandRuntimeAliases()
        {
            islandHandle = null;
            terrainStreamer = null;
            runtimeRoot = null;
            terrainMaterialTextures = null;
            terrainMaterial = null;
            terrainLod1Material = null;
            terrainLod2Material = null;
            grassMaterial = null;
            rockMaterial = null;
            treeWoodMaterial = null;
            treeLod1WoodMaterial = null;
            treeBarkAlbedoTexture = null;
            treeBarkHeightTexture = null;
            treeBarkNormalTexture = null;
            treeBarkOcclusionTexture = null;
            treeFoliageMaterial = null;
            treeLod0FoliageMaterial = null;
            reedMaterial = null;
            fernMaterial = null;
            riverMaterial = null;
            meshEdgeMaterial = null;
            terrainNormalTexture = null;
            terrainOcclusionTexture = null;
            seaMaskTexture = null;
            cliffNoiseTexture = null;
            riverNoiseTexture = null;
            grassPatchNoiseTexture = null;
            ownsCliffNoiseTexture = false;
            ownsRiverNoiseTexture = false;
        }


    }
}
