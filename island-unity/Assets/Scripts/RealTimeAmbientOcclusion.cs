using UnityEngine;

[ExecuteAlways]
[RequireComponent(typeof(Camera))]
[DisallowMultipleComponent]
[ImageEffectOpaque]
public sealed class RealTimeAmbientOcclusion : MonoBehaviour
{
    public const string ShaderName = "Hidden/Motu/Real-Time Ambient Occlusion";

    public enum Quality
    {
        Performance,
        Balanced,
        Quality,
    }

    [SerializeField] private Shader ambientOcclusionShader;
    [SerializeField] private Quality quality = Quality.Quality;
    [SerializeField, Range(0f, 2.5f)] private float intensity = 0.6f;
    [SerializeField, Min(0.05f)] private float radius = 1.5f;
    [SerializeField] private bool halfResolution = true;

    private static readonly int AmbientOcclusionParametersId =
        Shader.PropertyToID("_AOParams");
    private static readonly int OcclusionTextureId =
        Shader.PropertyToID("_OcclusionTexture");

    private Camera targetCamera;
    private Material material;

    private void OnEnable()
    {
        targetCamera = GetComponent<Camera>();
        RequestDepthNormals();
        EnsureMaterial();
    }

    private void OnDisable()
    {
        DestroyMaterial();
    }

    private void OnValidate()
    {
        radius = Mathf.Max(0.05f, radius);
        RequestDepthNormals();
    }

    private void OnRenderImage(RenderTexture source, RenderTexture destination)
    {
        if (intensity <= 0f || !EnsureMaterial())
        {
            Graphics.Blit(source, destination);
            return;
        }

        RequestDepthNormals();
        ConfigureMaterial();

        var divisor = halfResolution ? 2 : 1;
        var width = Mathf.Max(1, source.width / divisor);
        var height = Mathf.Max(1, source.height / divisor);
        const RenderTextureFormat format = RenderTextureFormat.ARGB32;
        var occlusion = RenderTexture.GetTemporary(
            width,
            height,
            0,
            format,
            RenderTextureReadWrite.Linear);
        var blurred = RenderTexture.GetTemporary(
            width,
            height,
            0,
            format,
            RenderTextureReadWrite.Linear);
        occlusion.filterMode = FilterMode.Bilinear;
        occlusion.wrapMode = TextureWrapMode.Clamp;
        blurred.filterMode = FilterMode.Bilinear;
        blurred.wrapMode = TextureWrapMode.Clamp;

        try
        {
            Graphics.Blit(source, occlusion, material, 0);
            Graphics.Blit(occlusion, blurred, material, 1);
            Graphics.Blit(blurred, occlusion, material, 2);

            material.SetTexture(OcclusionTextureId, occlusion);
            Graphics.Blit(source, destination, material, 3);
        }
        finally
        {
            RenderTexture.ReleaseTemporary(blurred);
            RenderTexture.ReleaseTemporary(occlusion);
        }
    }

    private void RequestDepthNormals()
    {
        if (targetCamera == null)
        {
            targetCamera = GetComponent<Camera>();
        }
        if (targetCamera != null)
        {
            targetCamera.depthTextureMode |=
                DepthTextureMode.Depth | DepthTextureMode.DepthNormals;
        }
    }

    private bool EnsureMaterial()
    {
        if (ambientOcclusionShader == null)
        {
            ambientOcclusionShader = Shader.Find(ShaderName);
        }
        if (ambientOcclusionShader == null || !ambientOcclusionShader.isSupported)
        {
            DestroyMaterial();
            return false;
        }
        if (material != null && material.shader == ambientOcclusionShader)
        {
            return true;
        }

        DestroyMaterial();
        material = new Material(ambientOcclusionShader)
        {
            name = "Motu Real-Time Ambient Occlusion",
            hideFlags = HideFlags.HideAndDontSave,
        };
        return true;
    }

    private void ConfigureMaterial()
    {
        material.SetVector(
            AmbientOcclusionParametersId,
            new Vector4(
                intensity,
                radius,
                halfResolution ? 0.5f : 1f,
                SampleCountFor(quality)));
    }

    private static int SampleCountFor(Quality selectedQuality)
    {
        return selectedQuality switch
        {
            Quality.Performance => 6,
            Quality.Quality => 12,
            _ => 10,
        };
    }

    private void DestroyMaterial()
    {
        if (material == null)
        {
            return;
        }

        if (Application.isPlaying)
        {
            Destroy(material);
        }
        else
        {
            DestroyImmediate(material);
        }
        material = null;
    }
}
