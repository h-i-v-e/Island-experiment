using UnityEngine;

[DisallowMultipleComponent]
[RequireComponent(typeof(OceanSurfaceController))]
public sealed class OceanWaveSandboxController : MonoBehaviour
{
    private const string Controls =
        "Ocean Wave Sandbox\n"
        + "WASD move | Q/E descend/ascend | Shift fast\n"
        + "Mouse look | Escape release/capture cursor\n"
        + "Assign a Weather Driver for runtime wind and wave control";

    private static readonly int NoiseTextureId = Shader.PropertyToID("_NoiseTex");

    [SerializeField] private OceanWaveProfile profile;
    [Tooltip("Optional scene component derived from WorldWeatherDriver for runtime wind and wave control.")]
    [SerializeField] private WorldWeatherDriver weatherDriver;
    [SerializeField] private Material seaMaterialTemplate;
    [SerializeField] private Transform followTarget;
    [Tooltip("Global wind direction in world X/Z coordinates for this sea-only test scene.")]
    [SerializeField] private Vector2 windDirection = new Vector2(1f, 0.25f);
    [Tooltip("Global wind speed in metres per second for this sea-only test scene.")]
    [Range(0f, 40f)] [SerializeField] private float windSpeedMetresPerSecond = 9f;
    [Min(100f)] [SerializeField] private float oceanDiameterMetres = 16000f;
    [Min(0.25f)] [SerializeField] private float anchorSnapMetres = 16f;
    [Min(1f)] [SerializeField] private float moveSpeedMetresPerSecond = 24f;
    [Min(1f)] [SerializeField] private float fastMoveMultiplier = 3f;
    [Min(0.01f)] [SerializeField] private float mouseSensitivity = 0.12f;

    private OceanSurfaceController ocean;
    private WorldWeatherState weather;
    private Vector2 windTravelOffset;
    private Texture2D noiseTexture;
    private float yaw;
    private float pitch;
    private bool cursorCaptured;
    private string statistics = string.Empty;
    private float nextStatisticsUpdate;
    private float smoothedFrameTime = 1f / 60f;

    public OceanWaveProfile Profile => profile;
    public WorldWeatherDriver WeatherDriver { get => weatherDriver; set => weatherDriver = value; }
    public WorldWeatherState Weather => weather;

    public void ApplyWeather(WorldWeatherState value)
    {
        weather = value.Validated();
        ocean.ApplyWaveWeather(weather.Waves);
        ApplyWeatherWind();
    }

    public Transform FollowTarget => followTarget;
    public float OceanDiameterMetres => oceanDiameterMetres;

    public void Configure(
        OceanWaveProfile value,
        Material materialTemplate,
        Transform target,
        float diameterMetres)
    {
        profile = value;
        seaMaterialTemplate = materialTemplate;
        followTarget = target;
        oceanDiameterMetres = Mathf.Max(diameterMetres, 100f);
    }

    private void Awake()
    {
        ocean = GetComponent<OceanSurfaceController>();
        var shader = Shader.Find("Motu/Sea Water")
            ?? throw new System.InvalidOperationException(
                "Could not find shader 'Motu/Sea Water'.");
        var material = seaMaterialTemplate != null
            ? new Material(seaMaterialTemplate)
            : new Material(shader);
        material.name = "Ocean Wave Sandbox Sea (Runtime)";
        noiseTexture = IslandGenerator.CreateWeatherNoiseTexture();
        material.SetTexture(NoiseTextureId, noiseTexture);
        WorldEnvironmentController.ApplyWeatherWindNoise(noiseTexture);
        var settings = profile != null
            ? profile.ToRuntimeSettings()
            : OceanWaveRuntimeSettings.Default;
        anchorSnapMetres = settings.MaskAnchorSnapMetres;
        ocean.Install(material, oceanDiameterMetres, true, settings);
        weather = WorldWeatherState.FromEnvironment(new WorldEnvironmentSettings());
        weather.WindDirection = windDirection;
        weather.WindSpeedMetresPerSecond = windSpeedMetresPerSecond;
        weather.Waves = settings.Weather;
        ApplyWeather(weather);

        if (followTarget == null && Camera.main != null)
        {
            followTarget = Camera.main.transform;
        }
        var reflection = followTarget != null
            ? followTarget.GetComponent<PlanarWaterReflection>()
            : null;
        reflection?.Configure(ocean.SurfaceTransform);
        if (followTarget != null)
        {
            var euler = followTarget.eulerAngles;
            yaw = euler.y;
            pitch = NormalizeAngle(euler.x);
        }
        SetCursorCaptured(true);
        UpdateAnchor();
    }

    private void Update()
    {
        if (weatherDriver != null && weatherDriver.isActiveAndEnabled)
        {
            var updated = weather;
            weatherDriver.UpdateWeather(ref updated, Time.unscaledDeltaTime);
            ApplyWeather(updated);
        }
        windTravelOffset += weather.WindDirection
            * (weather.WindSpeedMetresPerSecond * Time.unscaledDeltaTime);
        ApplyWeatherWind();
        smoothedFrameTime = Mathf.Lerp(
            smoothedFrameTime,
            Mathf.Max(Time.unscaledDeltaTime, 1.0e-5f),
            0.08f);
        if (followTarget == null)
        {
            return;
        }
        if (Input.GetKeyDown(KeyCode.Escape))
        {
            SetCursorCaptured(!cursorCaptured);
        }
        if (cursorCaptured)
        {
            yaw += Input.GetAxisRaw("Mouse X") * mouseSensitivity * 10f;
            pitch = Mathf.Clamp(
                pitch - Input.GetAxisRaw("Mouse Y") * mouseSensitivity * 10f,
                -89f,
                89f);
            followTarget.rotation = Quaternion.Euler(pitch, yaw, 0f);
        }

        var localMovement = new Vector3(
            KeyAxis(KeyCode.D, KeyCode.A),
            KeyAxis(KeyCode.E, KeyCode.Q),
            KeyAxis(KeyCode.W, KeyCode.S));
        if (localMovement.sqrMagnitude <= 0f)
        {
            return;
        }
        localMovement.Normalize();
        var speed = moveSpeedMetresPerSecond;
        if (Input.GetKey(KeyCode.LeftShift) || Input.GetKey(KeyCode.RightShift))
        {
            speed *= fastMoveMultiplier;
        }
        followTarget.position += followTarget.TransformDirection(localMovement)
            * (speed * Time.deltaTime);
    }

    private void LateUpdate()
    {
        UpdateAnchor();
    }

    private void OnDestroy()
    {
        if (noiseTexture == null)
        {
            return;
        }
        if (Application.isPlaying)
        {
            Destroy(noiseTexture);
        }
        else
        {
            DestroyImmediate(noiseTexture);
        }
        noiseTexture = null;
    }

    private void OnGUI()
    {
        const float width = 620f;
        const float height = 122f;
        UpdateStatistics();
        GUI.Box(new Rect(12f, 12f, width, height), GUIContent.none);
        GUI.Label(new Rect(24f, 20f, width - 24f, 62f), Controls);
        GUI.Label(new Rect(24f, 86f, width - 24f, 28f), statistics);
    }

    private void UpdateStatistics()
    {
        if (ocean == null || Time.unscaledTime < nextStatisticsUpdate)
        {
            return;
        }
        nextStatisticsUpdate = Time.unscaledTime + 0.5f;
        statistics = string.Format(
            System.Globalization.CultureInfo.InvariantCulture,
            "{0:F0} fps | {1:N0} vertices | {2:N0} triangles | mask {3:F2} ms ({4} rebuilds, {5}/{6} coasts)",
            1f / smoothedFrameTime,
            ocean.MeshVertexCount,
            ocean.MeshTriangleCount,
            ocean.LastWaveMaskCompositionMilliseconds,
            ocean.WaveMaskCompositionCount,
            ocean.LastOverlappingCoastalBindingCount,
            ocean.CoastalWaveBindingCount);
    }

    private void UpdateAnchor()
    {
        if (followTarget == null)
        {
            return;
        }
        var snap = Mathf.Max(anchorSnapMetres, 0.25f);
        transform.position = new Vector3(
            Mathf.Round(followTarget.position.x / snap) * snap,
            0f,
            Mathf.Round(followTarget.position.z / snap) * snap);
    }

    private void ApplyWeatherWind()
    {
        var waveScale = WorldEnvironmentController.ApplyWeatherWindGlobals(
            weather.WindDirection,
            weather.WindSpeedMetresPerSecond,
            weather.VegetationWindStrengthMetres,
            weather.WindGustSizeMetres,
            weather.VegetationWindNormalStrength,
            weather.TreeWindStrengthMultiplier,
            weather.TreeWindBasePinHeightMetres,
            weather.TreeWindFullBendHeightMetres,
            weather.ReedWindStrengthMultiplier,
            weather.FernWindStrengthMultiplier);
        WorldEnvironmentController.ApplyWeatherWindOffset(windTravelOffset);
        ocean?.ApplyWeatherWindScale(waveScale);
    }

    private void SetCursorCaptured(bool captured)
    {
        cursorCaptured = captured;
        Cursor.lockState = captured ? CursorLockMode.Locked : CursorLockMode.None;
        Cursor.visible = !captured;
    }

    private static float KeyAxis(KeyCode positive, KeyCode negative)
    {
        return (Input.GetKey(positive) ? 1f : 0f)
            - (Input.GetKey(negative) ? 1f : 0f);
    }

    private static float NormalizeAngle(float angle)
    {
        return angle > 180f ? angle - 360f : angle;
    }

}
