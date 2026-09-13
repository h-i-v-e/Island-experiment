using System;
using Motu.Interop;
using UnityEngine;
using UnityEngine.Rendering;

namespace Motu.Islands
{
    public sealed partial class IslandRuntime
    {
        public const float Lod3DistanceMetres = 2000f;
        public const float Lod3SubmergedDistanceMetres = 4000f;
        private GameObject lod3Object;
        private Mesh lod3Mesh;
        private bool lod3Installed;
        private float lod3MaximumHeightMetres;
        private bool beyondLod3Distance;
        public bool IsLod3Visible => lod3Object != null && lod3Object.activeInHierarchy;
        public int Lod3TriangleCount => lod3Mesh != null ? (int)lod3Mesh.GetIndexCount(0) / 3 : 0;

        internal void InstallLod3(IslandPreparedMesh prepared, float maximumHeightMetres)
        {
            RequireInstalling();
            lod3MaximumHeightMetres = maximumHeightMetres;
            if (prepared == null)
            {
                // Fully submerged cells have no horizon surface, but can still finish installation.
                lod3Installed = true;
                return;
            }
            var shader = Resources.Load<Shader>("IslandHorizon")
                ?? throw new InvalidOperationException("The island horizon shader is unavailable.");
            var material = new Material(shader) { name = "Island LOD3 Horizon" };
            OwnMaterial(material);
            lod3Mesh = IslandMeshInterop.CreateGeneratedMesh(prepared);
            lod3Mesh.name = "Island LOD3 (Unsliced)";
            lod3Object = new GameObject("Island LOD3 Horizon");
            lod3Object.transform.SetParent(transform, false);
            lod3Object.AddComponent<MeshFilter>().sharedMesh = lod3Mesh;
            var renderer = lod3Object.AddComponent<MeshRenderer>();
            renderer.sharedMaterial = material;
            renderer.shadowCastingMode = ShadowCastingMode.Off;
            renderer.receiveShadows = false;
            renderer.lightProbeUsage = LightProbeUsage.Off;
            renderer.reflectionProbeUsage = ReflectionProbeUsage.Off;
            renderer.motionVectorGenerationMode = MotionVectorGenerationMode.ForceNoMotion;
            lod3Object.SetActive(false);
            lod3Installed = true;
        }

        public void SetViewPosition(Vector3 worldPosition)
        {
            var distance = Vector3.Distance(worldPosition, transform.position);
            if (lod3Object != null)
            {
                var submersion = Mathf.SmoothStep(0f, 1f,
                    Mathf.InverseLerp(Lod3DistanceMetres, Lod3SubmergedDistanceMetres, distance));
                // Move only the silhouette; residency and wave masks retain the true island centre.
                lod3Object.transform.position = transform.position
                    - Vector3.up * (lod3MaximumHeightMetres * submersion);
            }
            var distant = distance > Lod3DistanceMetres;
            if (distant == beyondLod3Distance) return;
            beyondLod3Distance = distant;
            if (State == IslandRuntimeState.Active || State == IslandRuntimeState.Dormant)
                ApplyLodVisibility();
        }

        private void ApplyLodVisibility()
        {
            var far = beyondLod3Distance || State == IslandRuntimeState.Dormant;
            // Cancel pending tile transitions before deactivating their coroutine owner.
            if (far && TerrainStreamer != null && TerrainStreamer.gameObject.activeSelf)
                TerrainStreamer.ClearPlayerFocus();
            TerrainStreamer?.gameObject.SetActive(!far);
            Caves?.gameObject.SetActive(!far);
            lod3Object?.SetActive(far);
        }

        private void ReleaseLod3()
        {
            lod3Installed = false;
            DestroyUnityObject(lod3Mesh);
            lod3Mesh = null;
            DestroyUnityObject(lod3Object);
            lod3Object = null;
        }
    }
}
