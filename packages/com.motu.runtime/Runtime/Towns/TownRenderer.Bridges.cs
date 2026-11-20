using System;
using System.Collections.Generic;
using Motu.Islands;
using UnityEngine;

namespace Motu.Towns
{
    /// <summary>World-space landings and the reason a nominated crossing was rejected.</summary>
    public readonly struct TownBridgeRejection
    {
        /// <summary>Index in the planner pass that nominated this crossing.</summary>
        public readonly int Crossing;
        public readonly string Kind, Surface, Reason;
        public readonly Vector3 Start, End;

        internal TownBridgeRejection(TownBridgeDiagnostic value, Vector3 start, Vector3 end)
        {
            Crossing = value.crossing;
            Kind = value.kind;
            Surface = value.surface;
            Reason = value.reason;
            Start = start;
            End = end;
        }
    }

    public sealed partial class TownRenderer
    {
        private readonly List<MeshCollider> bridgeColliders = new List<MeshCollider>();
        private TownBridgeDiagnostic[] bridgeDiagnostics = Array.Empty<TownBridgeDiagnostic>();

        public int BridgeCount => bridgeColliders.Count;
        public int BridgeDiagnosticCount => bridgeDiagnostics.Length;

        public bool TryGetBridgeDiagnostic(int index, out TownBridgeRejection rejection)
        {
            rejection = default;
            if ((uint)index >= (uint)bridgeDiagnostics.Length) return false;
            var value = bridgeDiagnostics[index];
            rejection = new TownBridgeRejection(value, transform.TransformPoint(Position(value.start, NativeWorldSizeMetres)),
                transform.TransformPoint(Position(value.end, NativeWorldSizeMetres)));
            return true;
        }

        private void BuildBridges(TownBridgePlan plan, Material churchStone)
        {
            bridgeDiagnostics = plan?.diagnostics ?? Array.Empty<TownBridgeDiagnostic>();
            if ((plan?.bridges?.Length ?? 0) == 0) return;

            var stone = churchStone ?? CreateMaterial(TownNative.ChurchStone, "bridge stone");
            foreach (var bridge in plan.bridges) BuildBridge(bridge, stone);
        }

        private void BuildBridge(TownBridge bridge, Material stone)
        {
            var root = new GameObject(bridge.id);
            root.transform.SetParent(transform, false);
            var origin = (Vector(bridge.start) + Vector(bridge.end)) * .5f;
            root.transform.localPosition = origin - new Vector3(NativeWorldSizeMetres * .5f, 0f, NativeWorldSizeMetres * .5f);

            Renderer Surface(string label, params TownMesh[] parts)
            {
                // Church wall UVs are scaled to half the native metre coordinates.
                var mesh = CreateMesh(bridge.id + " " + label, origin, 0.5f, parts);
                return AddMesh(root.transform, mesh, stone, label).GetComponent<MeshRenderer>();
            }

            var group = root.AddComponent<LODGroup>();
            group.SetLODs(new[] {
                new LOD(.17f, new[] { Surface("Stonework", bridge.masonry, bridge.dressing, bridge.deck) }),
                new LOD(0f, new[] { Surface("Distant stonework", bridge.coarse_masonry, bridge.coarse_deck) }),
            });
            group.RecalculateBounds();
            var collider = root.AddComponent<MeshCollider>();
            collider.sharedMesh = CreateMesh(bridge.collision, bridge.id + " collision", origin);
            bridgeColliders.Add(collider);
        }

        internal IslandPreparedMesh[] CopyTownNavigationMeshes(Transform navigationSpace)
        {
            var pier = CapitalPierBerth;
            var meshes = new IslandPreparedMesh[bridgeColliders.Count + (pier != null ? 1 : 0)];
            for (var i = 0; i < bridgeColliders.Count; i++)
            {
                var collider = bridgeColliders[i];
                meshes[i] = CopyNavigationMesh(collider.sharedMesh, collider.transform, navigationSpace);
            }
            if (pier != null) meshes[bridgeColliders.Count] = CopyNavigationMesh(pierNavigation, pier.transform, navigationSpace);
            return meshes;
        }

        private static IslandPreparedMesh CopyNavigationMesh(Mesh mesh, Transform source, Transform navigationSpace)
        {
            var local = navigationSpace.worldToLocalMatrix * source.localToWorldMatrix;
            var vertices = mesh.vertices;
            for (var v = 0; v < vertices.Length; v++) vertices[v] = local.MultiplyPoint3x4(vertices[v]);
            return new IslandPreparedMesh(vertices, Array.Empty<Vector3>(), mesh.triangles,
                Array.Empty<Vector2>(), Array.Empty<Color>(), Array.Empty<Vector2>());
        }
    }
}
