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
    private static readonly int WaveNoiseWorldSizeId = Shader.PropertyToID(
        "_WaveNoiseWorldSize");
    private static readonly int WaveDomainWarpId = Shader.PropertyToID(
        "_WaveDomainWarp");
    private static readonly int WaveAmplitudeVariationId = Shader.PropertyToID(
        "_WaveAmplitudeVariation");
    private static readonly int WhitecapColourId = Shader.PropertyToID(
        "_WhitecapColour");
    private static readonly int WhitecapStrengthId = Shader.PropertyToID(
        "_WhitecapStrength");
    private static readonly int WhitecapHeightThresholdId = Shader.PropertyToID(
        "_WhitecapHeightThreshold");
    private static readonly int WhitecapSlopeThresholdId = Shader.PropertyToID(
        "_WhitecapSlopeThreshold");
    private static readonly int WhitecapCoverageId = Shader.PropertyToID(
        "_WhitecapCoverage");
    private static readonly int WhitecapNoiseWorldSizeId = Shader.PropertyToID(
        "_WhitecapNoiseWorldSize");
    private static readonly int WhitecapFineNoiseScaleId = Shader.PropertyToID(
        "_WhitecapFineNoiseScale");
    private static readonly int WhitecapCounterflowSpeedId = Shader.PropertyToID(
        "_WhitecapCounterflowSpeed");
    private static readonly int OnshoreWaveEnabledId = Shader.PropertyToID(
        "_OnshoreWaveEnabled");
    private static readonly int OnshoreWaveParametersId = Shader.PropertyToID(
        "_OnshoreWaveParameters");
    private static readonly int OnshoreWaveBreakingId = Shader.PropertyToID(
        "_OnshoreWaveBreaking");
    private static readonly int WaveAttenuationTextureId = Shader.PropertyToID(
        "_WaveAttenuationTex");
    private static readonly int WaveOnshoreTextureId = Shader.PropertyToID(
        "_WaveOnshoreTex");
    private static readonly int WaveAttenuationWorldRectId = Shader.PropertyToID(
        "_WaveAttenuationWorldRect");

    private GameObject surfaceObject;
    private Material surfaceMaterial;
    private Mesh surfaceMesh;
    private OceanWaveMaskComposer maskComposer;
    private OceanWaveRuntimeSettings waveSettings;
    private float surfaceDiameterMetres;

    public Transform SurfaceTransform => surfaceObject != null
        ? surfaceObject.transform
        : null;

    public Material SurfaceMaterial => surfaceMaterial;
    public Mesh SurfaceMesh => surfaceMesh;
    public int MeshVertexCount => surfaceMesh != null ? surfaceMesh.vertexCount : 0;
    public int MeshTriangleCount => surfaceMesh != null
        ? checked((int)(surfaceMesh.GetIndexCount(0) / 3))
        : 0;
    public int WaveMaskCompositionCount => maskComposer != null
        ? maskComposer.CompositionCount
        : 0;
    public int CoastalWaveBindingCount => maskComposer != null
        ? maskComposer.BindingCount
        : 0;
    public int LastOverlappingCoastalBindingCount => maskComposer != null
        ? maskComposer.LastOverlappingBindingCount
        : 0;
    public double LastWaveMaskCompositionMilliseconds => maskComposer != null
        ? maskComposer.LastCompositionMilliseconds
        : 0.0;

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
        surfaceDiameterMetres = Mathf.Max(diameterMetres, 1f);
        EnsureSurfaceObject();
        EnsureMaskComposer();
        surfaceObject.transform.localPosition = Vector3.zero;
        surfaceObject.transform.localRotation = Quaternion.identity;
        surfaceObject.transform.localScale = Vector3.one;
        ReplaceSurfaceMesh(surfaceDiameterMetres);
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

    public void ApplyWaveSettings(OceanWaveRuntimeSettings settings)
    {
        waveSettings = settings;
        if (surfaceMaterial == null || surfaceDiameterMetres <= 0f)
        {
            return;
        }

        ReplaceSurfaceMesh(surfaceDiameterMetres);
        ConfigureWaveMaterial();
        EnsureMaskComposer();
        maskComposer.Configure(this, waveSettings);
    }

    internal void SetWaveAttenuation(
        Texture attenuation,
        Texture onshore,
        Vector4 worldRect)
    {
        if (surfaceMaterial == null)
        {
            return;
        }
        surfaceMaterial.SetTexture(
            WaveAttenuationTextureId,
            attenuation != null ? attenuation : Texture2D.whiteTexture);
        surfaceMaterial.SetTexture(
            WaveOnshoreTextureId,
            onshore != null ? onshore : Texture2D.blackTexture);
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
        surfaceMaterial.SetFloat(
            WaveNoiseWorldSizeId,
            waveSettings.NoiseWorldSizeMetres);
        surfaceMaterial.SetFloat(WaveDomainWarpId, waveSettings.DomainWarpMetres);
        surfaceMaterial.SetFloat(
            WaveAmplitudeVariationId,
            waveSettings.AmplitudeVariation);
        surfaceMaterial.SetColor(WhitecapColourId, waveSettings.WhitecapColour);
        surfaceMaterial.SetFloat(
            WhitecapStrengthId,
            waveSettings.WhitecapStrength);
        surfaceMaterial.SetFloat(
            WhitecapHeightThresholdId,
            waveSettings.WhitecapHeightThreshold);
        surfaceMaterial.SetFloat(
            WhitecapSlopeThresholdId,
            waveSettings.WhitecapSlopeThreshold);
        surfaceMaterial.SetFloat(WhitecapCoverageId, waveSettings.WhitecapCoverage);
        surfaceMaterial.SetFloat(
            WhitecapNoiseWorldSizeId,
            waveSettings.WhitecapNoiseWorldSizeMetres);
        surfaceMaterial.SetFloat(
            WhitecapFineNoiseScaleId,
            waveSettings.WhitecapFineNoiseScale);
        surfaceMaterial.SetFloat(
            WhitecapCounterflowSpeedId,
            waveSettings.WhitecapCounterflowSpeed);
        surfaceMaterial.SetFloat(
            OnshoreWaveEnabledId,
            waveSettings.OnshoreWaveEnabled ? 1f : 0f);
        surfaceMaterial.SetVector(OnshoreWaveParametersId, new Vector4(
            waveSettings.OnshoreWaveWavelengthMetres,
            waveSettings.OnshoreWaveAmplitudeMetres,
            waveSettings.OnshoreWaveSpeedMetresPerSecond,
            waveSettings.OnshoreWaveChoppiness));
        surfaceMaterial.SetVector(OnshoreWaveBreakingId, new Vector4(
            waveSettings.OnshoreWaveLeadingEdgeSharpness,
            waveSettings.OnshoreWaveSharpeningDistanceMetres,
            0f,
            0f));
        SetWaveAttenuation(
            Texture2D.whiteTexture,
            Texture2D.blackTexture,
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
