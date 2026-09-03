using UnityEngine;
using UnityEngine.Rendering;

[DisallowMultipleComponent]
public sealed class OceanSurfaceController : MonoBehaviour
{
    private static readonly int GeometricWavesId = Shader.PropertyToID("_GeometricWaves");
    private static readonly int WaveFadeStartId = Shader.PropertyToID("_WaveFadeStart");
    private static readonly int WaveFadeEndId = Shader.PropertyToID("_WaveFadeEnd");
    private static readonly int Wave0Id = Shader.PropertyToID("_OceanWave0");
    private static readonly int Wave1Id = Shader.PropertyToID("_OceanWave1");
    private static readonly int Wave2Id = Shader.PropertyToID("_OceanWave2");
    private static readonly int Wave3Id = Shader.PropertyToID("_OceanWave3");
    private static readonly int WaveSpeedsId = Shader.PropertyToID("_OceanWaveSpeeds");
    private static readonly int WaveChoppinessId = Shader.PropertyToID("_OceanWaveChoppiness");
    private static readonly int WaveAttenuationTextureId = Shader.PropertyToID(
        "_WaveAttenuationTex");
    private static readonly int WaveAttenuationWorldRectId = Shader.PropertyToID(
        "_WaveAttenuationWorldRect");

    private GameObject surfaceObject;
    private Material surfaceMaterial;
    private Mesh surfaceMesh;
    private OceanWaveMaskComposer maskComposer;
    private OceanWaveRuntimeSettings waveSettings;

    public Transform SurfaceTransform => surfaceObject != null
        ? surfaceObject.transform
        : null;

    public Material SurfaceMaterial => surfaceMaterial;
    public Mesh SurfaceMesh => surfaceMesh;

    public void Install(
        Material material,
        float diameterMetres,
        bool visible)
    {
        Install(
            material,
            diameterMetres,
            visible,
            OceanWaveRuntimeSettings.Default);
    }

    public void Install(
        Material material,
        float diameterMetres,
        bool visible,
        OceanWaveRuntimeSettings settings)
    {
        if (material == null)
        {
            throw new System.ArgumentNullException(nameof(material));
        }

        var previousMaterial = surfaceMaterial;
        surfaceMaterial = material;
        waveSettings = settings;
        EnsureSurfaceObject();
        EnsureMaskComposer();
        surfaceObject.transform.localPosition = Vector3.zero;
        surfaceObject.transform.localRotation = Quaternion.identity;
        surfaceObject.transform.localScale = Vector3.one;
        ReplaceSurfaceMesh(Mathf.Max(diameterMetres, 1f));
        ConfigureWaveMaterial();
        maskComposer.Configure(this, waveSettings);
        surfaceObject.GetComponent<MeshRenderer>().sharedMaterial = surfaceMaterial;
        surfaceObject.SetActive(visible);

        if (previousMaterial != null && previousMaterial != surfaceMaterial)
        {
            DestroyUnityObject(previousMaterial);
        }
    }

    public void SetVisible(bool visible)
    {
        surfaceObject?.SetActive(visible);
    }

    internal void SetWaveAttenuation(Texture texture, Vector4 worldRect)
    {
        if (surfaceMaterial == null)
        {
            return;
        }
        surfaceMaterial.SetTexture(
            WaveAttenuationTextureId,
            texture != null ? texture : Texture2D.whiteTexture);
        surfaceMaterial.SetVector(WaveAttenuationWorldRectId, worldRect);
    }

    internal void RegisterCoastalWaveMask(
        IslandRuntime owner,
        Texture mask,
        Transform islandTransform,
        float worldSize)
    {
        EnsureMaskComposer();
        maskComposer.Register(owner, mask, islandTransform, worldSize);
    }

    internal void UnregisterCoastalWaveMask(IslandRuntime owner)
    {
        maskComposer?.Unregister(owner);
    }

    private void EnsureSurfaceObject()
    {
        if (surfaceObject != null)
        {
            return;
        }

        surfaceObject = new GameObject("Player-Relative Deep Ocean");
        surfaceObject.transform.SetParent(transform, false);
        surfaceObject.AddComponent<MeshFilter>();
        var renderer = surfaceObject.AddComponent<MeshRenderer>();
        renderer.shadowCastingMode = ShadowCastingMode.Off;
        renderer.receiveShadows = true;
        renderer.lightProbeUsage = LightProbeUsage.Off;
        renderer.reflectionProbeUsage = ReflectionProbeUsage.Off;
        renderer.allowOcclusionWhenDynamic = false;
        renderer.motionVectorGenerationMode = MotionVectorGenerationMode.ForceNoMotion;
        var waterLayer = LayerMask.NameToLayer("Water");
        if (waterLayer >= 0)
        {
            surfaceObject.layer = waterLayer;
        }
    }

    private void EnsureMaskComposer()
    {
        if (maskComposer == null)
        {
            maskComposer = GetComponent<OceanWaveMaskComposer>()
                ?? gameObject.AddComponent<OceanWaveMaskComposer>();
        }
    }

    private void ReplaceSurfaceMesh(float diameterMetres)
    {
        var replacement = OceanClipmapMeshBuilder.Build(diameterMetres, waveSettings);
        var previous = surfaceMesh;
        surfaceMesh = replacement;
        surfaceObject.GetComponent<MeshFilter>().sharedMesh = surfaceMesh;
        DestroyUnityObject(previous);
    }

    private void ConfigureWaveMaterial()
    {
        surfaceMaterial.SetFloat(GeometricWavesId, waveSettings.Enabled ? 1f : 0f);
        surfaceMaterial.SetFloat(
            WaveFadeStartId,
            waveSettings.DisplacementFadeStartMetres);
        surfaceMaterial.SetFloat(
            WaveFadeEndId,
            waveSettings.DisplacementFadeEndMetres);
        surfaceMaterial.SetVector(Wave0Id, WaveVector(waveSettings.Wave0));
        surfaceMaterial.SetVector(Wave1Id, WaveVector(waveSettings.Wave1));
        surfaceMaterial.SetVector(Wave2Id, WaveVector(waveSettings.Wave2));
        surfaceMaterial.SetVector(Wave3Id, WaveVector(waveSettings.Wave3));
        surfaceMaterial.SetVector(WaveSpeedsId, new Vector4(
            waveSettings.Wave0.SpeedMetresPerSecond,
            waveSettings.Wave1.SpeedMetresPerSecond,
            waveSettings.Wave2.SpeedMetresPerSecond,
            waveSettings.Wave3.SpeedMetresPerSecond));
        surfaceMaterial.SetVector(WaveChoppinessId, new Vector4(
            waveSettings.Wave0.Choppiness,
            waveSettings.Wave1.Choppiness,
            waveSettings.Wave2.Choppiness,
            waveSettings.Wave3.Choppiness));
        SetWaveAttenuation(
            Texture2D.whiteTexture,
            new Vector4(-1f, -1f, 0.5f, 0.5f));
    }

    private static Vector4 WaveVector(OceanWaveComponent wave)
    {
        var direction = wave.Direction;
        return new Vector4(
            direction.x,
            direction.y,
            wave.WavelengthMetres,
            wave.AmplitudeMetres);
    }

    private void OnDestroy()
    {
        DestroyUnityObject(surfaceMaterial);
        DestroyUnityObject(surfaceMesh);
        surfaceMaterial = null;
        surfaceMesh = null;
    }

    private static void DestroyUnityObject(Object value)
    {
        if (value == null)
        {
            return;
        }
        if (Application.isPlaying)
        {
            Destroy(value);
        }
        else
        {
            DestroyImmediate(value);
        }
    }
}
