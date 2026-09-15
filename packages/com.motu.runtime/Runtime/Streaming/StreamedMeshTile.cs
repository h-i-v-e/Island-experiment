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

        private StreamedMeshTile(GameObject gameObject, Mesh mesh)
        {
            this.gameObject = gameObject;
            this.mesh = mesh;
        }

        internal static StreamedMeshTile Create(IslandPreparedMesh source, Transform parent, Material material, string name)
        {
            var mesh = IslandMeshInterop.CreateGeneratedMesh(source);
            GameObject root = null;
            try
            {
                mesh.name = name;
                root = new GameObject(name);
                root.transform.SetParent(parent, false);
                root.AddComponent<MeshFilter>().sharedMesh = mesh;
                var renderer = root.AddComponent<MeshRenderer>();
                renderer.sharedMaterial = material;
                renderer.shadowCastingMode = ShadowCastingMode.On;
                renderer.receiveShadows = true;
                return new StreamedMeshTile(root, mesh);
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
