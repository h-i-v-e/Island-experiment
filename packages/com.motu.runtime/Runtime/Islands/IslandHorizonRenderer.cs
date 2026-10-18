using System;
using Motu.Interop;
using Motu.World;
using UnityEngine;
using UnityEngine.Rendering;
using static Motu.Rendering.UnityObjectLifetime;

namespace Motu.Islands
{
    // Owns the distant silhouette and its land footprint independently of details.
    internal sealed class IslandHorizonRenderer : MonoBehaviour
    {
        private Mesh mesh;
        private Material material;
        private MeshRenderer meshRenderer;
        private float maximumHeight;
        private RenderTexture shelterLand;
        private OceanIslandShelterField shelterField;

        internal void Initialize(IslandPreparedMesh prepared, float height)
        {
            maximumHeight = height;
            if (prepared == null) return;
            var shader = Resources.Load<Shader>("IslandHorizon")
                ?? throw new InvalidOperationException("The island horizon shader is unavailable.");
            mesh = IslandMeshInterop.CreateGeneratedMesh(prepared);
            mesh.name = "Disk-cached island LOD3";
            mesh.UploadMeshData(true);
            RegisterShelterLand();
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
            if (shelterField != null) shelterField.Unregister(this);
            if (shelterLand != null)
            {
                shelterLand.Release();
                DestroyUnityObject(shelterLand);
            }
            DestroyUnityObject(mesh);
            DestroyUnityObject(material);
        }

        private void RegisterShelterLand()
        {
            // World installation precedes horizon installation. Resolve once;
            // visual LOD, terrain residency and renderer visibility do not own shelter.
            var ocean = FindFirstObjectByType<OceanSurfaceController>();
            shelterField = ocean != null ? ocean.GetComponent<OceanIslandShelterField>() : null;
            if (shelterField == null) return;
            var shader = Resources.Load<Shader>("IslandShelterLand")
                ?? throw new InvalidOperationException("The island shelter land shader is unavailable.");
            var rasterMaterial = new Material(shader) { hideFlags = HideFlags.HideAndDontSave };
            var previous = RenderTexture.active;
            try
            {
                shelterLand = new RenderTexture(128, 128, 0,
                    RenderTextureFormat.ARGB32, RenderTextureReadWrite.Linear)
                {
                    name = "Island horizon shelter land",
                    wrapMode = TextureWrapMode.Clamp,
                    filterMode = FilterMode.Bilinear,
                    useMipMap = false,
                    autoGenerateMips = false,
                    hideFlags = HideFlags.DontSave,
                };
                if (!shelterLand.Create())
                    throw new InvalidOperationException("Could not create the island shelter land texture.");
                Graphics.SetRenderTarget(shelterLand);
                GL.Clear(false, true, new Color(0f, 1f, 0f, 1f));
                rasterMaterial.SetFloat("_IslandWorldSize", IslandWorldManager.IslandSizeMetres);
                if (!rasterMaterial.SetPass(0))
                    throw new InvalidOperationException("Could not render the island shelter land texture.");
                // Draw the original root mesh, before its visual child is sunk
                // below the horizon. Local elevation zero is the island sea level.
                Graphics.DrawMeshNow(mesh, Matrix4x4.identity);
            }
            finally
            {
                RenderTexture.active = previous;
                DestroyUnityObject(rasterMaterial);
            }
            shelterField.Register(this, shelterLand, transform, IslandWorldManager.IslandSizeMetres);
        }
    }
}
