using Motu.World;
using UnityEngine;

namespace Motu.Rendering
{
    [ExecuteAlways]
    [DisallowMultipleComponent]
    [RequireComponent(typeof(Camera))]
    [AddComponentMenu("Motu/Ocean Underwater View")]
    public sealed class OceanUnderwaterView : MonoBehaviour
    {
        [SerializeField] private OceanSurfaceController ocean;
        [Tooltip("Additional suspended-particle haze per metre, independent of RGB absorption.")]
        [Min(0)] [SerializeField] private float hazeDensity = .045f;
        [Tooltip("Maximum underwater visibility in metres, including rays aimed at the sky.")]
        [Min(1)] [SerializeField] private float visibilityMetres = 80f;
        [Tooltip("Softness of the waterline in world metres. Pixel antialiasing is also applied.")]
        [Range(.001f, .05f)] [SerializeField] private float waterlineSoftness = .005f;

        private Camera view;
        private Material material;
        internal OceanSurfaceController Surface => ocean;
        internal bool PendingOwnerRemoval { get; set; }
        public void Configure(OceanSurfaceController surface) => ocean = surface;

        private void OnEnable()
        {
            view = GetComponent<Camera>();
            view.depthTextureMode |= DepthTextureMode.Depth;
        }

        private void OnDisable()
        {
            UnityObjectLifetime.DestroyUnityObject(material);
            material = null;
        }

        internal Material PrepareMaterial()
        {
            if (ocean == null || ocean.SurfaceMaterial == null || ocean.SurfaceMesh == null
                || ocean.SurfaceTransform == null || !ocean.SurfaceTransform.gameObject.activeInHierarchy
                || (view.cullingMask & (1 << ocean.SurfaceTransform.gameObject.layer)) == 0
                || PlanarWaterReflection.IsReflectionCamera(view))
            {
                return null;
            }
            if (material == null)
            {
                var shader = Resources.Load<Shader>("OceanUnderwater");
                if (shader == null || !shader.isSupported)
                {
                    return null;
                }
                material = new Material(shader) { hideFlags = HideFlags.HideAndDontSave };
            }
            material.CopyPropertiesFromMaterial(ocean.SurfaceMaterial);
            material.shaderKeywords = System.Array.Empty<string>();
            var projection = GL.GetGPUProjectionMatrix(view.projectionMatrix, true);
            // Blit UVs use viewport orientation; only mesh rasterization uses the GPU Y flip.
            material.SetMatrix("_UnderwaterInvProjection", view.projectionMatrix.inverse);
            material.SetMatrix("_UnderwaterCameraToWorld", view.worldToCameraMatrix.inverse);
            material.SetMatrix("_UnderwaterView", view.worldToCameraMatrix);
            material.SetMatrix("_UnderwaterVP", projection * view.worldToCameraMatrix);
            material.SetVector("_UnderwaterOrigin", ocean.SurfaceTransform.position);
            material.SetVector("_UnderwaterParams", new Vector4(
                Mathf.Max(hazeDensity, 0), Mathf.Max(visibilityMetres, 1),
                Mathf.Max(waterlineSoftness, .001f), view.nearClipPlane));
            material.SetFloat("_UnderwaterOrthographic", view.orthographic ? 1 : 0);
            // Fog scattering follows the world lighting; it must not glow at night.
            var illumination = RenderSettings.ambientLight.linear;
            var sun = RenderSettings.sun;
            if (sun != null && sun.enabled)
                illumination += sun.color.linear * (sun.intensity * Mathf.Clamp01(-sun.transform.forward.y));
            var tint = ocean.SurfaceMaterial.GetColor("_Color").linear;
            material.SetColor("_UnderwaterScatter", tint * illumination);

            return material;
        }
    }
}
