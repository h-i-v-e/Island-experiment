using UnityEngine;

public sealed class IslandDemoController : MonoBehaviour
{
    private const float ClickDragTolerance = 6f;
    private const float FrameRateSampleSeconds = 0.25f;
    private static readonly Rect PanelRect = new Rect(16f, 16f, 540f, 210f);

    [SerializeField] private IslandGenerator island;
    [SerializeField] private Camera viewerCamera;
    [SerializeField] private OrbitCamera orbitCamera;
    [SerializeField] private FirstPersonController firstPersonController;
    [Header("Play Mode Start")]
    [SerializeField] private bool startInFlyMode;
    [SerializeField] private Vector3 flyStartPosition = new Vector3(0f, 4f, -1800f);
    [SerializeField] private float flyStartYawDegrees;
    [SerializeField] private float flyStartPitchDegrees;

    private IslandWorldManager worldManager;
    private bool clickCandidate;
    private Vector2 clickStart;
    private float frameRateSampleTime;
    private int frameRateSampleFrames;
    private string frameRateText = "FPS: --";

    public void Configure(
        IslandGenerator islandGenerator,
        Camera camera,
        OrbitCamera orbit,
        FirstPersonController firstPerson)
    {
        island = islandGenerator;
        viewerCamera = camera;
        orbitCamera = orbit;
        firstPersonController = firstPerson;
    }

    public void ConfigureFlyStart(
        bool enabled,
        Vector3 position,
        float yawDegrees = 0f,
        float pitchDegrees = 0f)
    {
        startInFlyMode = enabled;
        flyStartPosition = position;
        flyStartYawDegrees = yawDegrees;
        flyStartPitchDegrees = pitchDegrees;
    }

    private void Awake()
    {
        if (island == null || viewerCamera == null || orbitCamera == null)
        {
            enabled = false;
            Debug.LogWarning("IslandDemoController is missing its scene references.", this);
            return;
        }
        var target = island.transform.TransformPoint(
            new Vector3(0f, island.Generation.MaximumHeightMetres * 0.3f, 0f));
        orbitCamera.Configure(target, island.WorldSizeMetres * 1.15f);
        worldManager = FindAnyObjectByType<IslandWorldManager>();
        firstPersonController?.Configure(
            orbitCamera,
            worldManager != null
                ? (IWorldSurfaceQuery)worldManager
                : island);
        island.SetStreamingTarget(null);
    }

    private void Start()
    {
        if (startInFlyMode && firstPersonController != null)
        {
            firstPersonController.BeginFlying(
                flyStartPosition,
                flyStartYawDegrees,
                flyStartPitchDegrees);
        }
    }

    private void Update()
    {
        UpdateFrameRate();
        if (firstPersonController == null
            || firstPersonController.IsActive
            || island == null
            || viewerCamera == null)
        {
            return;
        }

        if (Input.GetMouseButtonDown(0))
        {
            clickStart = Input.mousePosition;
            var guiPosition = new Vector2(clickStart.x, Screen.height - clickStart.y);
            clickCandidate = !PanelRect.Contains(guiPosition);
        }
        if (!Input.GetMouseButtonUp(0) || !clickCandidate)
        {
            return;
        }

        clickCandidate = false;
        var releasedAt = (Vector2)Input.mousePosition;
        if ((releasedAt - clickStart).sqrMagnitude
            > ClickDragTolerance * ClickDragTolerance)
        {
            return;
        }
        if (island.TryRaycastOverview(
            viewerCamera.ScreenPointToRay(releasedAt),
            out var groundPoint))
        {
            firstPersonController.Enter(groundPoint);
        }
    }

    private void UpdateFrameRate()
    {
        var deltaTime = Time.unscaledDeltaTime;
        if (deltaTime <= 0f)
        {
            return;
        }

        frameRateSampleTime += deltaTime;
        frameRateSampleFrames++;
        if (frameRateSampleTime < FrameRateSampleSeconds)
        {
            return;
        }

        frameRateText = $"FPS: {Mathf.RoundToInt(frameRateSampleFrames / frameRateSampleTime)}";
        frameRateSampleTime = 0f;
        frameRateSampleFrames = 0;
    }

    private void OnGUI()
    {
        if (island == null)
        {
            return;
        }
        if (island.DebugSettings.ShowFrameRate)
        {
            GUI.Label(
                new Rect(Mathf.Max(16f, Screen.width - 116f), 16f, 100f, 28f),
                frameRateText,
                GUI.skin.box);
        }
        if (firstPersonController != null && firstPersonController.IsActive)
        {
            GUILayout.BeginArea(PanelRect, GUI.skin.box);
            GUILayout.Label("First person: WASD move | Shift run/fly boost | Space jump | Mouse look");
            GUILayout.Label(
                $"{firstPersonController.ToggleFlyModeKey}: fly "
                + $"{firstPersonController.FlySpeedMetresPerSecond:0.#} m/s at "
                + $"{firstPersonController.FlyClearanceMetres:0.#} m clearance"
                + (firstPersonController.IsFlyMode ? " (ACTIVE)" : string.Empty));
            GUILayout.Label(
                $"{island.DebugSettings.ToggleMeshEdgesKey}: mesh edges | "
                + $"{island.DebugSettings.ToggleTreeMeshEdgesKey}: tree wireframe | "
                + $"{island.DebugSettings.ToggleFrameRateKey}: frame rate");
            GUILayout.Label("Tab: release cursor | Escape: overview");
            DrawWorldStatus();
            GUILayout.Label(island.Status);
            GUILayout.EndArea();
            GUI.Label(
                new Rect(Screen.width * 0.5f - 5f, Screen.height * 0.5f - 10f, 20f, 20f),
                "+");
            return;
        }

        GUILayout.BeginArea(PanelRect, GUI.skin.box);
        GUILayout.Label("Procedural Island Sandbox");
        GUILayout.Label("Generation method: CPU");
        DrawWorldStatus();
        GUILayout.Label(island.Status);
        GUILayout.BeginHorizontal();
        GUI.enabled = !island.IsGenerating;
        if (GUILayout.Button("Generate")) island.Generate();
        if (GUILayout.Button("Clear")) island.Clear();
        GUI.enabled = true;
        GUILayout.EndHorizontal();
        GUILayout.Label(
            "Click terrain: walk | Drag: orbit | Wheel: zoom");
        GUILayout.Label(
            $"{island.DebugSettings.ToggleMeshEdgesKey}: mesh edges | "
            + $"{island.DebugSettings.ToggleTreeMeshEdgesKey}: tree wireframe | "
            + $"{island.DebugSettings.ToggleFrameRateKey}: frame rate");
        GUILayout.EndArea();
    }

    private void DrawWorldStatus()
    {
        if (worldManager == null)
        {
            return;
        }
        var focused = worldManager.FocusedIsland != null
            ? worldManager.FocusedIsland.name
            : "open sea";
        GUILayout.Label(
            $"Islands: {worldManager.LoadedIslandCount}/{worldManager.AuthoredIslandCount} loaded | focus: {focused}");
    }
}
