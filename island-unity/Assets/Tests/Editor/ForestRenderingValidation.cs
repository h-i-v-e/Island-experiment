using System;
using System.Collections;
using Motu.Interop;
using Motu.Islands;
using Motu.Streaming;
using UnityEngine;
using static UnityEngine.Object;

namespace Motu.Editor
{
    internal static class ForestRenderingValidation
    {
        // Fixtures supply completed preparation; draining their upload routines
        // does not perform a native export or wait for a worker.
        internal static void RunPreparedTransition(IEnumerator routine)
        {
            try
            {
                while (routine.MoveNext())
                    if (routine.Current is IEnumerator nested) RunPreparedTransition(nested);
            }
            finally { (routine as IDisposable)?.Dispose(); }
        }

        internal static HybridForestData SharedCatalogFixture()
        {
            var mesh = new IslandPreparedMesh(new[] { Vector3.zero, Vector3.right, Vector3.up * 2 },
                new[] { Vector3.forward, Vector3.forward, Vector3.forward }, new[] { 0, 1, 2 },
                new[] { Vector2.zero, Vector2.right, Vector2.up },
                new[] { new Color(.5f, .5f, 0, .5f), new Color(.5f, .5f, 0, .5f), new Color(.5f, .5f, 0, .5f) },
                new[] { Vector2.zero, Vector2.zero, Vector2.zero });
            var prototypes = new IslandPreparedMesh[3][];
            for (var species = 0; species < prototypes.Length; species++)
                prototypes[species] = new[] { mesh, mesh, mesh, mesh, mesh, mesh, mesh, mesh };
            return new HybridForestData(prototypes, new[] { new HybridForestNative.Instance
                { anchor = new Vector3(.5f, .5f, 0), scale = 1, prototype = 0 } }, 1, 2000);
        }

        internal static void ValidateSharedTreeCatalogAndCollision()
        {
            var parent = new GameObject("Shared tree catalog and collision validation");
            var streamer = new ForestTileStreamer();
            try
            {
                var trunks = new IslandPreparedTreeCollider[ForestTileStreamer.Lod0TileCount][];
                trunks[0] = new[] { new IslandPreparedTreeCollider(Vector3.zero, Vector3.up * 2, .25f) };
                var prepared = new IslandPreparedForestData(SharedCatalogFixture(), trunks);
                streamer.Initialize(parent.transform, prepared, true);
                var trees = streamer.Root.GetComponentsInChildren<InstancedForestRenderer>(true);
                if (trees.Length != 1 || trees[0].InstanceCount != 1 || trees[0].PrototypeCount != 3
                    || streamer.Root.GetComponentsInChildren<MeshRenderer>(true).Length != 0)
                    throw new InvalidOperationException("Standing trees must use one three-species instanced catalog without mesh renderers.");
                for (var kind = 0; kind < 5; kind++)
                {
                    var material = trees[0].Models.Material(kind);
                    if (!material.enableInstancing || material.shader.name != "Motu/Planet Forest"
                        || material.FindPass("ShadowCaster") < 0)
                        throw new InvalidOperationException("Shared trees must retain their own instanced shadow-casting materials.");
                }
                RunPreparedTransition(streamer.UpdateLod0NeighborhoodIncremental(Vector2Int.zero, () => true));
                var capsule = streamer.Root.GetComponentInChildren<CapsuleCollider>(true);
                if (streamer.ActiveTrunkColliderCount != 1 || capsule == null
                    || !Mathf.Approximately(capsule.radius, .25f) || !Mathf.Approximately(capsule.height, 2)
                    || capsule.direction != 1 || capsule.transform.localPosition != Vector3.up)
                    throw new InvalidOperationException("The forest did not retain its nearby trunk capsule geometry.");
                RunPreparedTransition(streamer.UpdateLod0NeighborhoodIncremental(
                    new Vector2Int(ForestTileStreamer.Lod0Resolution - 1, ForestTileStreamer.Lod0Resolution - 1), () => true));
                if (streamer.ActiveTrunkColliderCount != 0 || trees[0].InstanceCount != 1)
                    throw new InvalidOperationException("Retiring nearby collision must preserve shared visual tree placements.");
            }
            finally { streamer.Dispose(); DestroyImmediate(parent); }
        }
    }
}
