using UnityEngine;

[DisallowMultipleComponent]
[RequireComponent(typeof(OceanSurfaceController))]
public sealed class OceanWaveSandboxController : MonoBehaviour
{
    private const string Controls =
        "Ocean Wave Sandbox\n"
        + "WASD move | Q/E descend/ascend | Shift fast\n"
        + "Mouse look | Escape release/capture cursor\n"
        + "Edit Assets/Settings/OceanWaveProfile and restart Play mode to tune";

    private static readonly int NoiseTextureId = Shader.PropertyToID("_NoiseTex");

    [SerializeField] private OceanWaveProfile profile;
    [SerializeField] private Material seaMaterialTemplate;
    [SerializeField] private Transform followTarget;
    [Min(100f)] [SerializeField] private float oceanDiameterMetres = 16000f;
    [Min(0.25f)] [SerializeField] private float anchorSnapMetres = 16f;
    [Min(1f)] [SerializeField] private float moveSpeedMetresPerSecond = 24f;
    [Min(1f)] [SerializeField] private float fastMoveMultiplier = 3f;
    [Min(0.01f)] [SerializeField] private float mouseSensitivity = 0.12f;

    private OceanSurfaceController ocean;
    private Texture2D noiseTexture;
    private float yaw;
    private float pitch;
    private bool cursorCaptured;

    public OceanWaveProfile Profile => profile;
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
        noiseTexture = CreateNoiseTexture();
        material.SetTexture(NoiseTextureId, noiseTexture);
        var settings = profile != null
            ? profile.ToRuntimeSettings()
            : OceanWaveRuntimeSettings.Default;
        anchorSnapMetres = settings.MaskAnchorSnapMetres;
        ocean.Install(material, oceanDiameterMetres, true, settings);

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
        const float width = 480f;
        const float height = 92f;
        GUI.Box(new Rect(12f, 12f, width, height), GUIContent.none);
        GUI.Label(new Rect(24f, 20f, width - 24f, height - 16f), Controls);
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

    private static Texture2D CreateNoiseTexture()
    {
        const int size = 64;
        var texture = new Texture2D(
            size,
            size,
            TextureFormat.RGBA32,
            true,
            true)
        {
            name = "Ocean Wave Sandbox Noise",
            wrapMode = TextureWrapMode.Repeat,
            filterMode = FilterMode.Trilinear,
        };
        var colours = new Color[size * size];
        for (var y = 0; y < size; y++)
        {
            for (var x = 0; x < size; x++)
            {
                var broad = Mathf.PerlinNoise(x / 13f, y / 13f);
                var fine = Mathf.PerlinNoise(x / 4.5f + 17f, y / 4.5f + 31f);
                var value = Mathf.Clamp01(broad * 0.7f + fine * 0.3f);
                colours[y * size + x] = new Color(value, fine, broad, 1f);
            }
        }
        texture.SetPixels(colours);
        texture.Apply(true, true);
        return texture;
    }
}
