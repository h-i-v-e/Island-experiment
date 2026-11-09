using System;
using System.Collections.Generic;
using Motu.Interop;
using UnityEngine;

namespace Motu.Streaming
{
    // Per-island placements and collision ownership. Rendering/budgets are world-wide.
    internal sealed class InstancedForestRenderer : MonoBehaviour
    {
        internal sealed class Batch
        {
            internal Vector2Int cell;
            internal int prototype;
            internal int[] indices;
            internal Matrix4x4[] local;
            internal Bounds localBounds, worldBounds;
        }
        internal readonly List<Batch> batches = new List<Batch>();
        internal PlanetForestModels Models { get; private set; }
        internal Matrix4x4[] World { get; private set; }
        internal int[] Prototypes { get; private set; }
        internal Transform island;
        internal Bounds WorldBounds { get; private set; }
        private Matrix4x4 previousMatrix;
        private bool hasMatrix;
        internal int InstanceCount => World?.Length ?? 0;
        internal int PrototypeCount => Models?.meshes.Length ?? 0;
        internal int SharedVertexCount => Models?.vertexCount ?? 0;
        internal int SubmittedBatches { get; set; }
        internal int SubmittedDetailedCrowns { get; set; }
        internal int SubmittedShellTrees { get; set; }
        internal int SubmittedTwigCardTrees { get; set; }
        internal int SubmittedFarTrees { get; set; }
        internal int SubmittedFarShadowTrees { get; set; }
        internal int SubmittedShadowOnlyTrees { get; set; }
        internal int LastPineFoliageKind { get; set; } = -1;
        internal int LastPineFoliageLod { get; set; } = -1;
        internal int SubmittedPalmFineTrees { get; set; }
        internal int SubmittedPalmMiddleTrees { get; set; }
        internal int SubmittedPalmFarTrees { get; set; }
        internal int LastPalmFoliageLod { get; set; } = -1;

        internal void Initialize(Transform island, HybridForestData data, Texture detailNoise = null)
        {
            if (Models != null) throw new InvalidOperationException("Forest is already initialized.");
            this.island = island;
            try
            {
                Models = PlanetForestModels.Acquire(data, detailNoise);
                World = new Matrix4x4[data.instances.Length];
                Prototypes = new int[World.Length];
                var groups = new Dictionary<(Vector2Int, int), List<(int, Matrix4x4)>>();
                for (var i = 0; i < data.instances.Length; ++i)
                {
                    var tree = data.instances[i];
                    Prototypes[i] = (int)tree.prototype;
                    var cell = Cell(tree.anchor);
                    var key = (new Vector2Int(cell.x / 8, cell.y / 8), Prototypes[i]);
                    var root = new Vector3((tree.anchor.x - .5f) * data.worldSize,
                        tree.anchor.z * data.worldSize, (tree.anchor.y - .5f) * data.worldSize);
                    var matrix = Matrix4x4.TRS(root, Quaternion.Euler(0, -tree.yawRadians * Mathf.Rad2Deg, 0), Vector3.one * tree.scale);
                    if (!groups.TryGetValue(key, out var list)) groups.Add(key, list = new List<(int, Matrix4x4)>());
                    list.Add((i, matrix));
                }
                foreach (var group in groups)
                {
                    var batch = new Batch { cell = group.Key.Item1, prototype = group.Key.Item2,
                        indices = new int[group.Value.Count], local = new Matrix4x4[group.Value.Count] };
                    for (var i = 0; i < batch.indices.Length; ++i)
                    {
                        batch.indices[i] = group.Value[i].Item1;
                        batch.local[i] = group.Value[i].Item2;
                        var bounds = TransformBounds(batch.local[i], Models.bounds[batch.prototype]);
                        if (i == 0) batch.localBounds = bounds; else batch.localBounds.Encapsulate(bounds);
                    }
                    batch.localBounds.Expand(1f);
                    batches.Add(batch);
                }
                EnsureTransform();
                if (isActiveAndEnabled) PlanetForestWorld.Register(this);
                Debug.Log($"Planet forest: {InstanceCount} trees, {PrototypeCount} globally shared prototypes, {batches.Count} regions.");
            }
            catch { Release(); throw; }
        }
        internal static Vector2Int Cell(Vector3 anchor) => new Vector2Int(
            Mathf.Clamp(Mathf.FloorToInt(anchor.x * 64), 0, 63), Mathf.Clamp(Mathf.FloorToInt(anchor.y * 64), 0, 63));
        internal bool EnsureTransform()
        {
            if (Models == null || island == null) return false;
            var matrix = island.localToWorldMatrix;
            if (hasMatrix && previousMatrix == matrix) return false;
            var first = true;
            foreach (var batch in batches)
            {
                for (var i = 0; i < batch.indices.Length; ++i) World[batch.indices[i]] = matrix * batch.local[i];
                batch.worldBounds = TransformBounds(matrix, batch.localBounds);
                if (first) { WorldBounds = batch.worldBounds; first = false; }
                else { var bounds = WorldBounds; bounds.Encapsulate(batch.worldBounds); WorldBounds = bounds; }
            }
            previousMatrix = matrix;
            hasMatrix = true;
            return true;
        }
        internal static Bounds TransformBounds(Matrix4x4 matrix, Bounds bounds)
        {
            var x = matrix.MultiplyVector(new Vector3(bounds.extents.x, 0, 0));
            var y = matrix.MultiplyVector(new Vector3(0, bounds.extents.y, 0));
            var z = matrix.MultiplyVector(new Vector3(0, 0, bounds.extents.z));
            var extent = new Vector3(Mathf.Abs(x.x) + Mathf.Abs(y.x) + Mathf.Abs(z.x),
                Mathf.Abs(x.y) + Mathf.Abs(y.y) + Mathf.Abs(z.y), Mathf.Abs(x.z) + Mathf.Abs(y.z) + Mathf.Abs(z.z));
            return new Bounds(matrix.MultiplyPoint3x4(bounds.center), extent * 2f);
        }
        private void OnEnable() { if (Models != null) PlanetForestWorld.Register(this); }
        private void OnDisable() => PlanetForestWorld.Unregister(this);
        internal void Submit(Camera camera) => PlanetForestWorld.Submit(camera);
        private void OnDestroy() => Release();
        internal void Release()
        {
            PlanetForestWorld.Unregister(this);
            Models?.Release();
            Models = null;
            World = null;
            batches.Clear();
        }
    }
}
