using System;
using System.Buffers;
using System.Collections.Generic;
using Unity.Profiling;
using UnityEngine;
using UnityEngine.Rendering;

namespace Motu.Streaming
{
    // Planet's 96/12 budgets apply to the entire view, never separately to each island.
    internal static class PlanetForestWorld
    {
        internal const int DetailedLimit = 96, ShellLimit = 12;
        internal const float FineWoodDistanceSquared = 30f * 30f;
        private const int BatchLimit = 256;
        private static readonly List<InstancedForestRenderer> Forests = new List<InstancedForestRenderer>();
        private static readonly Dictionary<Camera, View> Views = new Dictionary<Camera, View>();
        private static readonly List<Camera> DeadCameras = new List<Camera>();
        private static readonly ProfilerMarker SubmitMarker = new ProfilerMarker("Motu.Forest.Submit");
        private static readonly ProfilerMarker SelectionMarker = new ProfilerMarker("Motu.Forest.SelectNearest96");
        private static readonly Dictionary<(PlanetForestModels, int, int, int, bool), Group> Groups = new Dictionary<(PlanetForestModels, int, int, int, bool), Group>();
        private static readonly List<(PlanetForestModels, int, int, int, bool)> Forgotten = new List<(PlanetForestModels, int, int, int, bool)>();
        private static int revision;
        private readonly struct Tree : IEquatable<Tree>
        {
            internal readonly InstancedForestRenderer forest;
            internal readonly int index;
            internal readonly float distance;
            internal Tree(InstancedForestRenderer forest, int index, float distance) { this.forest = forest; this.index = index; this.distance = distance; }
            public bool Equals(Tree other) => ReferenceEquals(forest, other.forest) && index == other.index;
            public override bool Equals(object value) => value is Tree other && Equals(other);
            public override int GetHashCode() => forest.GetHashCode() * 397 ^ index;
        }
        private sealed class View
        {
            internal int revision = -1, count;
            internal Vector3 eye;
            internal int mask;
            internal readonly Tree[] nearest = new Tree[DetailedLimit];
            internal readonly HashSet<Tree> selected = new HashSet<Tree>();
            internal readonly Plane[] planes = new Plane[6];
            internal readonly MaterialPropertyBlock properties = new MaterialPropertyBlock();
            internal readonly MaterialPropertyBlock farProperties = new MaterialPropertyBlock();
            internal Vector4 palmDistances;
        }
        private sealed class Group
        {
            internal PlanetForestModels models;
            internal int prototype, kind, count, layer;
            internal bool fullPalmFar;
            internal Matrix4x4[] matrices;
            internal Bounds bounds;
        }
        internal static void Register(InstancedForestRenderer forest)
        {
            if (Forests.Contains(forest)) return;
            if (Forests.Count == 0) RenderPipelineManager.beginCameraRendering += RenderCamera;
            Forests.Add(forest);
            ++revision;
        }
        internal static void Unregister(InstancedForestRenderer forest)
        {
            var index = Forests.FindIndex(item => ReferenceEquals(item, forest));
            if (index < 0) return;
            Forests.RemoveAt(index);
            ++revision;
            if (Forests.Count == 0)
            {
                RenderPipelineManager.beginCameraRendering -= RenderCamera;
                Views.Clear();
                Groups.Clear();
            }
        }
        internal static void Forget(PlanetForestModels models)
        {
            Forgotten.Clear();
            foreach (var item in Groups) if (ReferenceEquals(item.Value.models, models)) Forgotten.Add(item.Key);
            foreach (var key in Forgotten) Groups.Remove(key);
        }
        private static void RenderCamera(ScriptableRenderContext context, Camera camera) => Submit(camera);
        internal static void Submit(Camera camera)
        {
            using var marker = SubmitMarker.Auto();
            if (camera == null) return;
            for (var i = Forests.Count - 1; i >= 0; --i)
                if (Forests[i] == null) Forests[i].Release();
            foreach (var forest in Forests)
            {
                forest.SubmittedBatches = forest.SubmittedDetailedCrowns = forest.SubmittedFarTrees = forest.SubmittedShellTrees = 0;
                forest.SubmittedTwigCardTrees = 0;
                forest.SubmittedShadowBatches = 0;
                forest.LastPineFoliageKind = -1;
                forest.LastPineFoliageLod = -1;
                forest.LastPalmFoliageLod = -1;
                forest.SubmittedPalmFineTrees = forest.SubmittedPalmMiddleTrees = forest.SubmittedPalmFarTrees = 0;
                if (forest.EnsureTransform()) ++revision;
            }
            if (!Views.TryGetValue(camera, out var view))
            {
                DeadCameras.Clear();
                foreach (var key in Views.Keys) if (key == null) DeadCameras.Add(key);
                foreach (var key in DeadCameras) Views.Remove(key);
                Views.Add(camera, view = new View());
            }
            var eye = camera.transform.position;
            if (view.revision != revision || view.mask != camera.cullingMask || (view.eye - eye).sqrMagnitude > 9f)
                Select(view, camera);
            ConfigurePalmLod(view, eye);
            GeometryUtility.CalculateFrustumPlanes(camera, view.planes);
            view.properties.SetVector("_MotuForestViewer", eye);
            try
            {
                // Distant crowns remain individual and do not cast shadows, just as in Planet.
                foreach (var forest in Forests)
                {
                    if ((camera.cullingMask & (1 << forest.gameObject.layer)) == 0
                        || !GeometryUtility.TestPlanesAABB(view.planes, forest.WorldBounds)) continue;
                    foreach (var batch in forest.batches)
                    {
                        if (!GeometryUtility.TestPlanesAABB(view.planes, batch.worldBounds)) continue;
                        foreach (var index in batch.indices)
                        {
                            if (view.selected.Contains(new Tree(forest, index, 0))) continue;
                            Add(forest, index, 4, batch.worldBounds, camera, forest.Models.IsPalm(batch.prototype));
                            ++forest.SubmittedFarTrees;
                            if (forest.Models.IsPine(batch.prototype))
                            {
                                forest.LastPineFoliageKind = 3;
                                forest.LastPineFoliageLod = 2;
                            }
                            if (forest.Models.IsPalm(batch.prototype))
                            {
                                forest.LastPalmFoliageLod = 2;
                                ++forest.SubmittedPalmFarTrees;
                            }
                        }
                    }
                }
                for (var rank = 0; rank < view.count; ++rank)
                {
                    var tree = view.nearest[rank];
                    var forest = tree.forest;
                    var p = forest.Prototypes[tree.index];
                    var pine = forest.Models.IsPine(p);
                    var palm = forest.Models.IsPalm(p);
                    var root = (Vector3)forest.World[tree.index].GetColumn(3);
                    var distance = (root - eye).sqrMagnitude;
                    var bounds = InstancedForestRenderer.TransformBounds(forest.World[tree.index], forest.Models.bounds[p]);
                    bounds.Expand(1f);
                    if (palm)
                    {
                        AddPalm(forest, tree.index, Mathf.Sqrt(distance), bounds, camera, view.palmDistances);
                        continue;
                    }
                    var fineWood = distance < FineWoodDistanceSquared;
                    Add(forest, tree.index, fineWood ? 0 : 1, bounds, camera);
                    if (fineWood && forest.Models.meshes[p][5] != null)
                    {
                        Add(forest, tree.index, 5, bounds, camera);
                        ++forest.SubmittedTwigCardTrees;
                    }
                    Add(forest, tree.index, pine && !fineWood ? 6 : 2, bounds, camera);
                    ++forest.SubmittedDetailedCrowns;
                    if (pine)
                    {
                        forest.LastPineFoliageKind = 1;
                        forest.LastPineFoliageLod = fineWood ? 0 : 1;
                    }
                    var shellRange = pine ? 32f : 38f;
                    if (rank < ShellLimit && (!pine || fineWood) && distance < shellRange * shellRange && forest.Models.meshes[p][3] != null)
                    {
                        Add(forest, tree.index, 3, bounds, camera);
                        ++forest.SubmittedShellTrees;
                    }
                }
                foreach (var group in Groups.Values) Flush(group, camera);
            }
            finally
            {
                foreach (var group in Groups.Values)
                {
                    if (group.matrices != null) ArrayPool<Matrix4x4>.Shared.Return(group.matrices);
                    group.matrices = null;
                    group.count = 0;
                }
            }
        }
        private static void ConfigurePalmLod(View view, Vector3 eye)
        {
            // The radius follows the current roots even while the nearest selection is cached.
            var radius = 240f;
            if (view.count == DetailedLimit)
            {
                radius = 0;
                for (var rank = 0; rank < view.count; ++rank)
                {
                    var tree = view.nearest[rank];
                    var root = (Vector3)tree.forest.World[tree.index].GetColumn(3);
                    radius = Mathf.Max(radius, Vector3.Distance(root, eye));
                }
                radius = Mathf.Min(radius, 240f);
            }
            view.palmDistances = PalmLodDistances(radius);
            view.properties.SetVector("_PalmLodDistances", view.palmDistances);
            view.properties.SetFloat("_PalmLodEnabled", 1);
            // Unselected far trees always have full coverage, including tied and cached roots.
            view.farProperties.SetFloat("_PalmLodEnabled", 0);
            view.farProperties.SetVector("_MotuForestViewer", eye);
        }
        internal static Vector4 PalmLodDistances(float radius)
        {
            var end = Mathf.Max(radius, 0.001f);
            // Narrow dense stands can overlap the two fades; the stage weights still sum to one.
            return new Vector4(24, 30, Mathf.Max(0, end - 12), end);
        }
        internal static Vector3 PalmLodWeights(float distance, Vector4 limits)
        {
            var middle = Mathf.SmoothStep(0, 1, Mathf.InverseLerp(limits.x, limits.y, distance));
            var far = Mathf.SmoothStep(0, 1, Mathf.InverseLerp(limits.z, limits.w, distance));
            return new Vector3((1-middle)*(1-far), middle*(1-far), far);
        }
        private static void AddPalm(InstancedForestRenderer forest, int index, float distance,
            Bounds bounds, Camera camera, Vector4 limits)
        {
            var weights = PalmLodWeights(distance, limits);
            if (weights.x > 0)
            {
                Add(forest, index, 0, bounds, camera);
                Add(forest, index, 2, bounds, camera);
                ++forest.SubmittedPalmFineTrees;
            }
            if (weights.y > 0)
            {
                Add(forest, index, 1, bounds, camera);
                Add(forest, index, 6, bounds, camera);
                ++forest.SubmittedPalmMiddleTrees;
            }
            if (weights.x + weights.y > 0) ++forest.SubmittedDetailedCrowns;
            if (weights.z > 0)
            {
                Add(forest, index, 4, bounds, camera);
                ++forest.SubmittedPalmFarTrees;
                ++forest.SubmittedFarTrees;
            }
            forest.LastPalmFoliageLod = weights.x > 0 ? 0 : weights.y > 0 ? 1 : 2;
        }
        private static void Select(View view, Camera camera)
        {
            using var marker = SelectionMarker.Auto();
            view.count = 0;
            view.selected.Clear();
            var eye = camera.transform.position;
            foreach (var forest in Forests)
            {
                if ((camera.cullingMask & (1 << forest.gameObject.layer)) == 0 || forest.WorldBounds.SqrDistance(eye) >= 240f * 240f) continue;
                foreach (var batch in forest.batches)
                {
                    if (batch.worldBounds.SqrDistance(eye) >= 240f * 240f) continue;
                    foreach (var index in batch.indices)
                    {
                        var distance = ((Vector3)forest.World[index].GetColumn(3) - eye).sqrMagnitude;
                        if (distance >= 240f * 240f || (view.count == DetailedLimit && distance >= view.nearest[view.count - 1].distance)) continue;
                        var position = view.count;
                        while (position > 0 && view.nearest[position - 1].distance > distance) --position;
                        var count = Math.Min(view.count, DetailedLimit - 1) - position;
                        if (count > 0) Array.Copy(view.nearest, position, view.nearest, position + 1, count);
                        view.nearest[position] = new Tree(forest, index, distance);
                        view.count = Math.Min(view.count + 1, DetailedLimit);
                    }
                }
            }
            for (var i = 0; i < view.count; ++i) view.selected.Add(view.nearest[i]);
            // Release references to islands which left the selected set.
            Array.Clear(view.nearest, view.count, DetailedLimit - view.count);
            view.eye = eye;
            view.mask = camera.cullingMask;
            view.revision = revision;
        }
        private static void Add(InstancedForestRenderer forest, int index, int kind, Bounds bounds, Camera camera, bool fullPalmFar = false)
        {
            var prototype = forest.Prototypes[index];
            var key = (forest.Models, prototype, kind, forest.gameObject.layer, fullPalmFar);
            if (!Groups.TryGetValue(key, out var group))
                Groups.Add(key, group = new Group { models = forest.Models, prototype = prototype, kind = kind, layer = forest.gameObject.layer, fullPalmFar = fullPalmFar });
            if (group.models.meshes[prototype][kind] == null) return;
            group.matrices ??= ArrayPool<Matrix4x4>.Shared.Rent(BatchLimit);
            if (group.count == 0) group.bounds = bounds; else group.bounds.Encapsulate(bounds);
            group.matrices[group.count++] = forest.World[index];
            // This is an attribution counter; actual draws are globally packed across islands.
            if (group.count == 1) ++forest.SubmittedBatches;
            if (group.count == BatchLimit) Flush(group, camera);
        }
        private static void Flush(Group group, Camera camera)
        {
            if (group.count == 0) return;
            var shadows = group.kind == 4 || (group.kind == 3 && group.prototype < group.models.broadleafCount)
                ? ShadowCastingMode.Off : ShadowCastingMode.On;
            var properties = group.fullPalmFar ? Views[camera].farProperties : Views[camera].properties;
            var material = group.models.Material(group.kind);
            var mesh = group.models.meshes[group.prototype][group.kind];
            if (SystemInfo.supportsInstancing)
            {
                var parameters = new RenderParams(material) { camera = camera, worldBounds = group.bounds,
                    shadowCastingMode = shadows, receiveShadows = true, matProps = properties, layer = group.layer, lightProbeUsage = LightProbeUsage.Off };
                Graphics.RenderMeshInstanced(parameters, mesh, 0, group.matrices, group.count);
            }
            else for (var i = 0; i < group.count; ++i)
                Graphics.DrawMesh(mesh, group.matrices[i], material, group.layer, camera, 0, properties, shadows, true, null, LightProbeUsage.Off);
            group.count = 0;
        }
    }
}
