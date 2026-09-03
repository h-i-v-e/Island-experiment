using System;
using UnityEngine;
using UnityEngine.Rendering;

[DefaultExecutionOrder(1000)]
[DisallowMultipleComponent]
public sealed class WorldEnvironmentController : MonoBehaviour
{
    private static readonly int EnvironmentWorldOffsetId = Shader.PropertyToID(
        "_MotuEnvironmentWorldOffset");

    [Tooltip("Distance between player-relative environment anchor positions.")]
    [Min(0.25f)]
    [SerializeField] private float anchorSnapMetres = 25f;

    private Transform followTarget;
    private GameObject skyDomeObject;
    private Mesh skyDomeMesh;
    private Material skyDomeMaterial;
    private Texture2D cloudWeatherTexture;
    private Texture2D ownedOceanNoiseTexture;
    private GameObject moonLightObject;
    private Light moonLight;
    private OceanSurfaceController ocean;
    private float seaLevel;
    private float skyDomeWorldSize;

    public Vector3 AnchorPosition => transform.position;
    public Mesh SkyMesh => skyDomeMesh;
    public Material SkyMaterial => skyDomeMaterial;
    public Material SeaMaterial => ocean != null ? ocean.SurfaceMaterial : null;
    public Light MoonLight => moonLight;
    public Transform OceanTransform => ocean != null ? ocean.SurfaceTransform : null;

    public static WorldEnvironmentController FindOrCreate()
    {
        var existing = FindAnyObjectByType<WorldEnvironmentController>(
            FindObjectsInactive.Include);
        if (existing != null)
        {
            return existing;
        }

        var root = new GameObject("Open Sea World Environment");
        return root.AddComponent<WorldEnvironmentController>();
    }

    public void SetFollowTarget(Transform target)
    {
        if (followTarget == target)
        {
            return;
        }
        followTarget = target;
        UpdateAnchor(true);
    }

    public void Install(
        Material newSkyMaterial,
        Material newSeaMaterial,
        Texture2D newCloudWeatherTexture,
        Texture2D newOwnedOceanNoiseTexture,
        float newSkyDomeWorldSize,
        float environmentDiameterMetres,
        float globalSeaLevel,
        bool showSea,
        Light sunlightTemplate)
    {
        Install(
            newSkyMaterial,
            newSeaMaterial,
            newCloudWeatherTexture,
            newOwnedOceanNoiseTexture,
            newSkyDomeWorldSize,
            environmentDiameterMetres,
            globalSeaLevel,
            showSea,
            OceanWaveRuntimeSettings.Default,
            sunlightTemplate);
    }

    public void Install(
        Material newSkyMaterial,
        Material newSeaMaterial,
        Texture2D newCloudWeatherTexture,
        Texture2D newOwnedOceanNoiseTexture,
        float newSkyDomeWorldSize,
        float environmentDiameterMetres,
        float globalSeaLevel,
        bool showSea,
        OceanWaveRuntimeSettings oceanWaves,
        Light sunlightTemplate)
    {
        if (newSkyMaterial == null)
        {
            throw new ArgumentNullException(nameof(newSkyMaterial));
        }
        if (newSeaMaterial == null)
        {
            throw new ArgumentNullException(nameof(newSeaMaterial));
        }

        var previousMaterial = skyDomeMaterial;
        var previousWeather = cloudWeatherTexture;
        var previousOceanNoise = ownedOceanNoiseTexture;
        EnsureSkyDome(newSkyDomeWorldSize, newSkyMaterial);
        skyDomeMaterial = newSkyMaterial;
        cloudWeatherTexture = newCloudWeatherTexture;
        ownedOceanNoiseTexture = newOwnedOceanNoiseTexture;
        seaLevel = globalSeaLevel;
        Shader.SetGlobalVector(
            EnvironmentWorldOffsetId,
            new Vector4(0f, -seaLevel, 0f, 0f));

        EnsureOcean();
        ocean.Install(
            newSeaMaterial,
            environmentDiameterMetres,
            showSea,
            oceanWaves);
        EnsureMoonLight(sunlightTemplate);
        UpdateAnchor(true);
        BindExistingReflectionCameras();

        if (previousMaterial != null && previousMaterial != skyDomeMaterial)
        {
            DestroyUnityObject(previousMaterial);
        }
        if (previousWeather != null && previousWeather != cloudWeatherTexture)
        {
            DestroyUnityObject(previousWeather);
        }
        if (previousOceanNoise != null
            && previousOceanNoise != ownedOceanNoiseTexture)
        {
            DestroyUnityObject(previousOceanNoise);
        }
    }

    public void SetSeaVisible(bool visible)
    {
        ocean?.SetVisible(visible);
    }

    internal void RegisterCoastalWaveMask(
        IslandRuntime owner,
        Texture mask,
        Transform islandTransform,
        float worldSize)
    {
        EnsureOcean();
        ocean.RegisterCoastalWaveMask(owner, mask, islandTransform, worldSize);
    }

    internal void UnregisterCoastalWaveMask(IslandRuntime owner)
    {
        ocean?.UnregisterCoastalWaveMask(owner);
    }

    public void BindReflectionCamera(Camera camera)
    {
        if (camera == null || OceanTransform == null)
        {
            return;
        }
        camera.GetComponent<PlanarWaterReflection>()?.Configure(OceanTransform);
    }

    public static Vector3 SnapAnchor(
        Vector3 targetPosition,
        float globalSeaLevel,
        float snapMetres)
    {
        snapMetres = Mathf.Max(snapMetres, 0.25f);
        return new Vector3(
            Mathf.Floor(targetPosition.x / snapMetres + 0.5f) * snapMetres,
            globalSeaLevel,
            Mathf.Floor(targetPosition.z / snapMetres + 0.5f) * snapMetres);
    }

    private void Awake()
    {
        EnsureOcean();
        Shader.SetGlobalVector(EnvironmentWorldOffsetId, Vector4.zero);
    }

    private void LateUpdate()
    {
        UpdateAnchor(false);
    }

    private void OnValidate()
    {
        anchorSnapMetres = Mathf.Max(anchorSnapMetres, 0.25f);
    }

    private void OnDestroy()
    {
        DestroyUnityObject(skyDomeMaterial);
        DestroyUnityObject(skyDomeMesh);
        DestroyUnityObject(cloudWeatherTexture);
        DestroyUnityObject(ownedOceanNoiseTexture);
        skyDomeMaterial = null;
        skyDomeMesh = null;
        cloudWeatherTexture = null;
        ownedOceanNoiseTexture = null;
    }

    private GameObject CreateSkyDomeObject(Mesh mesh, Material material)
    {
        var result = new GameObject("Player-Relative Sky Dome");
        result.transform.SetParent(transform, false);
        result.AddComponent<MeshFilter>().sharedMesh = mesh;
        var renderer = result.AddComponent<MeshRenderer>();
        renderer.sharedMaterial = material;
        renderer.shadowCastingMode = ShadowCastingMode.Off;
        renderer.receiveShadows = false;
        renderer.lightProbeUsage = LightProbeUsage.Off;
        renderer.reflectionProbeUsage = ReflectionProbeUsage.Off;
        renderer.motionVectorGenerationMode = MotionVectorGenerationMode.ForceNoMotion;
        renderer.allowOcclusionWhenDynamic = false;
        return result;
    }

    private void EnsureSkyDome(float worldSize, Material material)
    {
        worldSize = Mathf.Max(worldSize, 1f);
        if (skyDomeObject != null
            && skyDomeMesh != null
            && Mathf.Approximately(skyDomeWorldSize, worldSize))
        {
            skyDomeObject.GetComponent<MeshRenderer>().sharedMaterial = material;
            return;
        }

        var source = IslandGenerator.PrepareSkyDome(worldSize);
        var replacementMesh = IslandGenerator.CreateGeneratedMesh(source);
        replacementMesh.name = "Rust Generated Open-Sea Sky Dome";
        var replacementObject = CreateSkyDomeObject(replacementMesh, material);
        var previousObject = skyDomeObject;
        var previousMesh = skyDomeMesh;
        skyDomeObject = replacementObject;
        skyDomeMesh = replacementMesh;
        skyDomeWorldSize = worldSize;
        DestroyUnityObject(previousObject);
        DestroyUnityObject(previousMesh);
    }

    private void EnsureOcean()
    {
        if (ocean == null)
        {
            ocean = GetComponent<OceanSurfaceController>()
                ?? gameObject.AddComponent<OceanSurfaceController>();
        }
    }

    private void EnsureMoonLight(Light sunlightTemplate)
    {
        if (moonLight == null)
        {
            moonLightObject = new GameObject("Moon Light");
            moonLightObject.transform.SetParent(transform, false);
            moonLight = moonLightObject.AddComponent<Light>();
            moonLight.type = LightType.Directional;
            moonLight.renderMode = LightRenderMode.ForcePixel;
            moonLight.color = new Color(0.48f, 0.62f, 0.90f, 1f);
            moonLight.intensity = 0f;
            moonLight.shadows = LightShadows.Soft;
            moonLight.enabled = false;
        }
        if (sunlightTemplate == null)
        {
            return;
        }
        moonLight.cullingMask = sunlightTemplate.cullingMask;
        moonLight.shadowStrength = sunlightTemplate.shadowStrength;
        moonLight.shadowBias = sunlightTemplate.shadowBias;
        moonLight.shadowNormalBias = sunlightTemplate.shadowNormalBias;
        moonLight.shadowNearPlane = sunlightTemplate.shadowNearPlane;
        moonLight.shadowResolution = sunlightTemplate.shadowResolution;
    }

    private void BindExistingReflectionCameras()
    {
        if (OceanTransform == null)
        {
            return;
        }
        foreach (var reflection in FindObjectsByType<PlanarWaterReflection>(
            FindObjectsInactive.Include))
        {
            reflection.Configure(OceanTransform);
        }
    }

    private void UpdateAnchor(bool force)
    {
        var target = followTarget;
        if (target == null && Camera.main != null)
        {
            target = Camera.main.transform;
        }
        var targetPosition = target != null ? target.position : transform.position;
        var anchor = SnapAnchor(targetPosition, seaLevel, anchorSnapMetres);
        if (force || transform.position != anchor || transform.rotation != Quaternion.identity)
        {
            transform.SetPositionAndRotation(anchor, Quaternion.identity);
        }
    }

    private static void DestroyUnityObject(UnityEngine.Object value)
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
