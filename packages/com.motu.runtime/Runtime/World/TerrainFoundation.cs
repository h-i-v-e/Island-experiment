using System.Collections;
using Motu.Islands;
using UnityEngine;
using static Motu.Rendering.UnityObjectLifetime;

namespace Motu.World
{
    public enum TerrainFoundationStatus
    {
        NotBuilt, Ready, WaitingForTerrain, MissingTerrainSource, MissingBounds,
        UnsuitableTerrain, QueryFailed, InvalidConfiguration, MissingMaterial,
    }

    /// <summary>A level, bounds-sized stone base fitted to the complete terrain footprint.</summary>
    [DisallowMultipleComponent]
    [AddComponentMenu("Motu/Placement/Terrain Foundation")]
    public sealed partial class TerrainFoundation : MonoBehaviour
    {
        [Tooltip("Building or prop-group root. Defaults to this object; all active child meshes are included.")]
        [SerializeField] private Transform footprintRoot;
        [Tooltip("An IslandWorldManager, IslandGenerator, or another ITerrainFootprintQuery provider. Auto-detected when empty.")]
        [SerializeField] private MonoBehaviour terrainSource;
        [SerializeField] private bool buildOnEnable = true;
        [Min(0f)] [SerializeField] private float terrainWaitSeconds = 30f;
        [Min(0f)] [SerializeField] private float paddingMetres = .25f;
        [Min(.01f)] [SerializeField] private float minimumThicknessMetres = .1f;
        [Min(0f)] [SerializeField] private float embedmentMetres = .05f;
        [SerializeField] private bool alignObjectToPlatform = true;
        [SerializeField] private bool requireDryLand = true;
        [SerializeField] private bool createCollider = true;
        [Min(1)] [SerializeField] private int maximumCellTests = 262144;
        [Tooltip("Optional override. Defaults to the bundled Rust Studio stonework material.")]
        [SerializeField] private Material stoneMaterial;
        [Tooltip("Physical dimensions of one texture repeat, in metres, on every face.")]
        [SerializeField] private Vector2 textureTileSizeMetres = new Vector2(2.4f, 1.2f);
        [SerializeField, HideInInspector] private GameObject platform;
        [SerializeField, HideInInspector] private Mesh platformMesh;
        private Coroutine buildRoutine;

        public Transform FootprintRoot { get => footprintRoot != null ? footprintRoot : transform; set => footprintRoot = value; }
        public MonoBehaviour TerrainSource { get => terrainSource; set => terrainSource = value; }
        public Material StoneMaterial { get => stoneMaterial; set => stoneMaterial = value; }
        public float PaddingMetres { get => paddingMetres; set => paddingMetres = value; }
        public float MinimumThicknessMetres { get => minimumThicknessMetres; set => minimumThicknessMetres = value; }
        public float EmbedmentMetres { get => embedmentMetres; set => embedmentMetres = value; }
        public bool AlignObjectToPlatform { get => alignObjectToPlatform; set => alignObjectToPlatform = value; }
        public bool RequireDryLand { get => requireDryLand; set => requireDryLand = value; }
        public Vector2 TextureTileSizeMetres { get => textureTileSizeMetres; set => textureTileSizeMetres = value; }
        public TerrainFoundationStatus Status { get; private set; }
        public TerrainFootprint Footprint { get; private set; }
        public TerrainFootprintSample GroundSample { get; private set; }
        public GameObject Platform => platform;
        public bool IsBuilt => platform != null && platformMesh != null;

        private void OnEnable()
        {
            if (Application.isPlaying && buildOnEnable && !IsBuilt)
                buildRoutine = StartCoroutine(BuildWhenTerrainIsReady());
        }

        private void OnDisable()
        {
            if (buildRoutine != null) StopCoroutine(buildRoutine);
            buildRoutine = null;
        }

        private IEnumerator BuildWhenTerrainIsReady()
        {
            // Let the caller finish configuring a freshly instantiated prefab.
            yield return null;
            if (IsBuilt) { buildRoutine = null; yield break; }
            float deadline = Time.realtimeSinceStartup + Mathf.Max(0, terrainWaitSeconds);
            do
            {
                if (TryBuild()) { buildRoutine = null; yield break; }
                if (Status != TerrainFoundationStatus.WaitingForTerrain
                    && Status != TerrainFoundationStatus.MissingTerrainSource) break;
                yield return new WaitForSecondsRealtime(.5f);
            } while (Time.realtimeSinceStartup < deadline);
            buildRoutine = null;
            Debug.LogWarning($"Foundation for {FootprintRoot.name} was not built: {Status}. Call TryBuild() when placement is ready.", this);
        }

        /// <summary>Build or refit once, without requesting terrain generation or changing streaming focus.</summary>
        public bool TryBuild() => TryBuild(FindTerrainSource());

        /// <summary>Use an explicit query provider, for runtime placement systems and custom terrain.</summary>
        public bool TryBuild(ITerrainFootprintQuery source)
        {
            if (!ConfigurationIsValid() || !TryGetDepthRepeat(out var depthRepeat))
                return Failed(TerrainFoundationStatus.InvalidConfiguration);
            if (!TryMeasureContent(out var bounds, out var rotation)) return Failed(TerrainFoundationStatus.MissingBounds);
            Footprint = new TerrainFootprint(new Vector2(bounds.center.x, bounds.center.z),
                new Vector2(RoundUpToRepeat(bounds.size.x + paddingMetres * 2, textureTileSizeMetres.x),
                    RoundUpToRepeat(bounds.size.z + paddingMetres * 2, depthRepeat)), rotation.eulerAngles.y);
            if (source == null || source is Object owner && owner == null)
                return Failed(TerrainFoundationStatus.MissingTerrainSource);
            var query = source.QueryTerrainFootprint(Footprint, out var ground, maximumCellTests);
            GroundSample = ground;
            if (query == TerrainFootprintQueryStatus.Incomplete) return Failed(TerrainFoundationStatus.WaitingForTerrain);
            if (query != TerrainFootprintQueryStatus.Complete) return Failed(TerrainFoundationStatus.QueryFailed);
            if (!CanSupportFoundation(ground)) return Failed(TerrainFoundationStatus.UnsuitableTerrain);
            var material = stoneMaterial != null ? stoneMaterial : Resources.Load<Material>("Motu/FoundationStonework");
            if (material == null) return Failed(TerrainFoundationStatus.MissingMaterial);
            if (!FitPlatform(bounds.min.y, rotation, material)) return Failed(TerrainFoundationStatus.InvalidConfiguration);
            Status = TerrainFoundationStatus.Ready;
            return true;
        }

        private bool CanSupportFoundation(TerrainFootprintSample ground) => ground.IsComplete && ground.HasTerrain
            && ground.OpenSeaArea == 0d && ground.UnknownArea == 0d
            && Finite(ground.MinimumHeight) && Finite(ground.MaximumHeight)
            && Finite(ground.HeightRange)
            && ground.MaximumHeight >= ground.MinimumHeight && (!requireDryLand || ground.IsDryLand);

        private bool ConfigurationIsValid() => Finite(paddingMetres) && paddingMetres >= 0
            && Finite(minimumThicknessMetres) && minimumThicknessMetres >= .01f
            && Finite(embedmentMetres) && embedmentMetres >= 0 && maximumCellTests > 0
            && Finite(textureTileSizeMetres.x) && textureTileSizeMetres.x > 0
            && Finite(textureTileSizeMetres.y) && textureTileSizeMetres.y > 0
            && Finite(FootprintRoot.localToWorldMatrix.determinant)
            && FootprintRoot.localToWorldMatrix.determinant != 0f;

        private ITerrainFootprintQuery FindTerrainSource()
        {
            if (terrainSource != null) return terrainSource as ITerrainFootprintQuery;
            foreach (var candidate in FootprintRoot.GetComponentsInParent<MonoBehaviour>())
                if (candidate is ITerrainFootprintQuery query) return query;
            return FindAnyObjectByType<IslandWorldManager>() as ITerrainFootprintQuery
                ?? FindAnyObjectByType<IslandGenerator>();
        }

        private bool FitPlatform(float contentBottom, Quaternion rotation, Material material)
        {
            var top = GroundSample.MaximumHeight;
            var requiredDepth = Mathf.Max(top - GroundSample.MinimumHeight + embedmentMetres, minimumThicknessMetres);
            var bottom = top - RoundUpToRepeat(requiredDepth, textureTileSizeMetres.y);
            if (!Finite(bottom)) return false;
            if (alignObjectToPlatform) FootprintRoot.position += Vector3.up * (top - contentBottom);
            EnsurePlatform();
            WritePlatformMesh(rotation, bottom, top);
            platform.GetComponent<MeshRenderer>().sharedMaterial = material;
            var collision = platform.GetComponent<MeshCollider>();
            if (createCollider)
            {
                if (collision == null) collision = platform.AddComponent<MeshCollider>();
                collision.convex = FootprintRoot.GetComponentInParent<Rigidbody>() != null;
                collision.sharedMesh = null;
                collision.sharedMesh = platformMesh;
            }
            if (collision != null) collision.enabled = createCollider;
            return true;
        }

        private static float RoundUpToRepeat(float size, float repeat) =>
            (float)(System.Math.Ceiling((double)size / repeat) * repeat);

        private bool TryGetDepthRepeat(out float repeat)
        {
            repeat = 0;
            try
            {
                // Depth is U on the end walls and V on the caps. Its shortest
                // shared repeat keeps both mappings whole without stretching.
                decimal width = (decimal)textureTileSizeMetres.x, height = (decimal)textureTileSizeMetres.y;
                if (width <= 0 || height <= 0) return false;
                decimal a = width, b = height;
                while (b != 0) { decimal remainder = a % b; a = b; b = remainder; }
                repeat = (float)(width / a * height);
                return Finite(repeat) && repeat > 0;
            }
            catch (System.OverflowException) { return false; }
        }

        private void EnsurePlatform()
        {
            if (platform == null)
            {
                platform = new GameObject("Stone foundation") { layer = FootprintRoot.gameObject.layer };
                platform.transform.SetParent(FootprintRoot, false);
                platform.AddComponent<MeshFilter>();
                platform.AddComponent<MeshRenderer>();
            }
            else if (platform.transform.parent != FootprintRoot)
                platform.transform.SetParent(FootprintRoot, false);
            if (platformMesh == null) platformMesh = new Mesh { name = "Terrain-fitted stone foundation" };
            platform.GetComponent<MeshFilter>().sharedMesh = platformMesh;
        }

        private bool Failed(TerrainFoundationStatus status) { Status = status; return false; }
        private static bool Finite(float value) => float.IsFinite(value);

        private void OnDestroy()
        {
            DestroyUnityObject(platform);
            DestroyUnityObject(platformMesh);
        }
    }
}
