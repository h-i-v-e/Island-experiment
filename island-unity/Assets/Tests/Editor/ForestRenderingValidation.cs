using System;
using System.Collections;
using System.Collections.Generic;
using UnityEngine;
using UnityEngine.Rendering;
using Motu.Interop;
using Motu.Islands;
using Motu.World;

using Motu.Streaming;
using Motu.Rendering;
using static UnityEngine.Object;
using static Motu.Streaming.ForestTileStreamer;
namespace Motu.Editor
{
    internal static class ForestRenderingValidation
    {
        internal static void ValidateLowPolyCanopyShadowProxy(
            Material material,
            Material lod0Material)
        {
            var parent = new GameObject("Canopy shadow proxy validation");
            var streamer = new ForestTileStreamer();
            try
            {
                var lowPolyCanopy = new IslandPreparedMesh(
                    new[]
                    {
                        Vector3.zero,
                        Vector3.right,
                        Vector3.forward,
                    },
                    new[]
                    {
                        Vector3.up,
                        Vector3.up,
                        Vector3.up,
                    },
                    new[] { 0, 1, 2 },
                    Array.Empty<Vector2>(),
                    Array.Empty<Color>(),
                    Array.Empty<Vector2>());
                var lod2 = new IslandPreparedMesh[Lod2TileCount];
                lod2[0] = lowPolyCanopy;
                var lod1 = new IslandPreparedMesh[Lod1TileCount];
                lod1[0] = lowPolyCanopy;
                var lod0 = new IslandPreparedMesh[Lod1TileCount];
                lod0[0] = lowPolyCanopy;
                var lod0Colliders = new IslandPreparedTreeCollider[Lod1TileCount][];
                lod0Colliders[0] = new[]
                {
                    new IslandPreparedTreeCollider(Vector3.zero, Vector3.up * 2f, 0.25f),
                };
                var prepared = new IslandPreparedForestData(
                    lod2,
                    lod2,
                    lod1,
                    new IslandPreparedMesh[Lod1TileCount],
                    lod0,
                    new IslandPreparedMesh[Lod1TileCount],
                    lod0Colliders);
                streamer.Initialize(
                    parent.transform,
                    material,
                    lod0Material,
                    material,
                    material,
                    material,
                    prepared,
                    true);
                var renderers = streamer.Root.GetComponentsInChildren<MeshRenderer>(true);
                var foliageRenderer = Array.Find(
                    renderers,
                    renderer => renderer.gameObject.name == "Forest LOD 2 tile 0,0 foliage");
                var woodRenderer = Array.Find(
                    renderers,
                    renderer => renderer.gameObject.name == "Forest LOD 2 tile 0,0 wood");
                var shadowRenderer = Array.Find(
                    renderers,
                    renderer => renderer.gameObject.name == "Forest canopy shadow tile 0,0");
                if (renderers.Length != 3
                    || foliageRenderer == null
                    || woodRenderer == null
                    || shadowRenderer == null
                    || Array.FindAll(
                        renderers,
                        renderer => renderer.shadowCastingMode == ShadowCastingMode.Off).Length != 1
                    || Array.FindAll(
                        renderers,
                        renderer => renderer.shadowCastingMode == ShadowCastingMode.ShadowsOnly).Length
                        != 1
                    || Array.FindAll(
                        renderers,
                        renderer => renderer.shadowCastingMode == ShadowCastingMode.On).Length != 1
                    || foliageRenderer.GetComponent<MeshFilter>().sharedMesh
                        != shadowRenderer.GetComponent<MeshFilter>().sharedMesh)
                {
                    throw new InvalidOperationException(
                        "The forest did not create one shared low-poly canopy shadow proxy.");
                }
                streamer.UpdateLod1Neighborhood(Vector2Int.zero);
                streamer.UpdateLod0Neighborhood(Vector2Int.zero);
                var capsule = streamer.Root.GetComponentInChildren<CapsuleCollider>(true);
                if (streamer.ActiveTrunkColliderCount != 1
                    || capsule == null
                    || !Mathf.Approximately(capsule.radius, 0.25f)
                    || !Mathf.Approximately(capsule.height, 2f)
                    || capsule.direction != 1
                    || capsule.transform.localPosition != Vector3.up)
                {
                    throw new InvalidOperationException(
                        "The forest did not create its LOD0 trunk capsule collider.");
                }
                var lod0Renderer = Array.Find(
                    streamer.Root.GetComponentsInChildren<MeshRenderer>(true),
                    renderer => renderer.gameObject.name == "Forest LOD 0 tile 0,0 foliage");
                if (lod0Renderer == null || lod0Renderer.sharedMaterial != lod0Material)
                {
                    throw new InvalidOperationException(
                        "The forest did not assign its double-sided material to LOD0 foliage.");
                }
                streamer.UpdateLod0Neighborhood(
                    new Vector2Int(Lod1Resolution - 1, Lod1Resolution - 1));
                if (streamer.ActiveTrunkColliderCount != 0)
                {
                    throw new InvalidOperationException(
                        "The forest retained a trunk collider after its LOD0 tile retired.");
                }
            }
            finally
            {
                streamer.Dispose();
                DestroyImmediate(parent);
            }
        }
    }
}
