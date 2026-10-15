using System;
using Motu.Interop;
using UnityEngine;
using UnityEngine.Rendering;
using static Motu.Rendering.UnityObjectLifetime;

namespace Motu.Islands
{
    // Owns only the distant silhouette. Detailed runtime disposal cannot remove it.
    internal sealed class IslandHorizonRenderer : MonoBehaviour
    {
        private Mesh mesh;
        private Material material;
        private MeshRenderer meshRenderer;
        private float maximumHeight;

        internal void Initialize(IslandPreparedMesh prepared, float height)
        {
            maximumHeight = height;
            if (prepared == null) return;
            var shader = Resources.Load<Shader>("IslandHorizon")
                ?? throw new InvalidOperationException("The island horizon shader is unavailable.");
            mesh = IslandMeshInterop.CreateGeneratedMesh(prepared);
            mesh.name = "Disk-cached island LOD3";
            mesh.UploadMeshData(true);
            material = new Material(shader) { name = "Island horizon residency" };
            var surface = new GameObject("LOD3 silhouette");
            surface.transform.SetParent(transform, false);
            surface.AddComponent<MeshFilter>().sharedMesh = mesh;
            meshRenderer = surface.AddComponent<MeshRenderer>();
            meshRenderer.sharedMaterial = material;
            meshRenderer.shadowCastingMode = ShadowCastingMode.Off;
            meshRenderer.receiveShadows = false;
            meshRenderer.lightProbeUsage = LightProbeUsage.Off;
            meshRenderer.reflectionProbeUsage = ReflectionProbeUsage.Off;
            meshRenderer.motionVectorGenerationMode = MotionVectorGenerationMode.ForceNoMotion;
        }

        internal void SetViewPosition(Vector3 position, bool visible, float lod3DistanceMetres)
        {
            if (meshRenderer == null) return;
            var distance = Vector3.Distance(position, transform.position);
            var submersion = IslandRuntime.Lod3Submersion(distance, lod3DistanceMetres);
            meshRenderer.transform.localPosition = Vector3.down * (maximumHeight * submersion);
            meshRenderer.enabled = visible;
        }

        private void OnDestroy()
        {
            DestroyUnityObject(mesh);
            DestroyUnityObject(material);
        }
    }
}
