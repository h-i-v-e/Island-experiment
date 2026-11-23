using System;
using UnityEngine;
using UnityEngine.Rendering;
using Motu.Interop;
using Motu.Islands;
using static Motu.Rendering.UnityObjectLifetime;

namespace Motu.Streaming
{
    // Owns one uploaded mesh and its renderer; species streamers choose its lifetime.
    internal sealed class StreamedMeshTile : IDisposable
    {
        private readonly GameObject gameObject;
        private readonly Mesh mesh;
        private bool disposed;

        private StreamedMeshTile(GameObject gameObject, Mesh mesh, MeshRenderer renderer)
        {
            this.gameObject = gameObject;
            this.mesh = mesh;
            Renderer = renderer;
        }

        internal MeshRenderer Renderer { get; }

        internal static StreamedMeshTile Create(IslandPreparedMesh source, Transform parent, Material material, string name) =>
            Create(IslandMeshInterop.CreateGeneratedMesh(source), parent, material, name);

        internal static StreamedMeshTile Create(Mesh mesh, Transform parent, Material material, string name,
            Vector3 localPosition = default)
        {
            GameObject root = null;
            try
            {
                mesh.name = name;
                root = new GameObject(name) { layer = parent.gameObject.layer };
                root.transform.SetParent(parent, false);
                root.transform.localPosition = localPosition;
                root.AddComponent<MeshFilter>().sharedMesh = mesh;
                var renderer = root.AddComponent<MeshRenderer>();
                renderer.sharedMaterial = material;
                renderer.shadowCastingMode = ShadowCastingMode.On;
                renderer.receiveShadows = true;
                return new StreamedMeshTile(root, mesh, renderer);
            }
            catch
            {
                DestroyUnityObject(root);
                DestroyUnityObject(mesh);
                throw;
            }
        }

        public void Dispose()
        {
            if (disposed) return;
            disposed = true;
            DestroyUnityObject(gameObject);
            DestroyUnityObject(mesh);
        }
    }
}
