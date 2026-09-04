using UnityEngine;

public sealed class IslandDemoController : MonoBehaviour
{
    private const float ClickDragTolerance = 6f;
    private const float FrameRateSampleSeconds = 0.25f;
    private static readonly Rect PanelRect = new Rect(16f, 16f, 600f, 250f);

    [SerializeField] private IslandWorldManager worldManager;
    [SerializeField] private Camera viewerCamera;
    [SerializeField] private OrbitCamera orbitCamera;
    [SerializeField] private FirstPersonController firstPersonController;
    [Header("Play Mode Start")]
    [SerializeField] private bool startInFlyMode;
    [SerializeField] private Vector3 flyStartPosition = new Vector3(0f, 4f, -1800f);
    [SerializeField] private float flyStartYawDegrees;
    [SerializeField] private float flyStartPitchDegrees;

    private bool clickCandidate;
    private Vector2 clickStart;
    private float frameRateSampleTime;
    private int frameRateSampleFrames;
    private string frameRateText = "FPS: --";

    public void Configure(
        IslandWorldManager manager,
        Camera camera,
        OrbitCamera orbit,
        FirstPersonController firstPerson)
    {
        worldManager = manager;
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
        if (worldManager == null || viewerCamera == null || orbitCamera == null)
        {
            enabled = false;
            Debug.LogWarning("IslandDemoController is missing its scene references.", this);
            return;
        }
        orbitCamera.Configure(
            IslandWorldManager.CellCentre(Vector2Int.zero, 60f),
            IslandWorldManager.IslandCellSizeMetres * 1.15f);
        firstPersonController?.Configure(
            orbitCamera,
            worldManager);
        worldManager.SetStreamingTarget(viewerCamera.transform);
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
        var island = worldManager.FocusedIsland;
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
        var island = worldManager.FocusedIsland;
        if (island != null && island.DebugSettings.ShowFrameRate)
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
            DrawDebugKeys(island);
            GUILayout.Label("Tab: release cursor | Escape: overview");
            DrawWorldStatus();
            DrawIslandStatus(island);
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
        DrawIslandStatus(island);
        GUILayout.Label(
            "Click terrain: walk | Drag: orbit | Wheel: zoom");
        DrawDebugKeys(island);
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
        var logicalPosition = worldManager.LogicalPlayerPosition;
        GUILayout.Label(
            $"World {worldManager.WorldSeed} | position "
            + $"{logicalPosition.x:0}, {logicalPosition.y:0} m | focus: {focused}");
        GUILayout.Label(
            $"Islands: {worldManager.LoadedIslandCount}/{worldManager.ResidentIslandLimit} resident"
            + $" | {worldManager.KnownIslandCount} known"
            + $" | {worldManager.QueuedIslandCount} queued"
            + $" | {worldManager.GeneratingIslandCount} generating"
            + $" | {worldManager.NativeHandleCount} native handles");
    }

    private static void DrawIslandStatus(IslandGenerator island)
    {
        GUILayout.Label(island != null ? island.Status : "Open sea");
    }

    private static void DrawDebugKeys(IslandGenerator island)
    {
        if (island == null)
        {
            return;
        }
        GUILayout.Label(
            $"{island.DebugSettings.ToggleMeshEdgesKey}: mesh edges | "
            + $"{island.DebugSettings.ToggleTreeMeshEdgesKey}: tree wireframe | "
            + $"{island.DebugSettings.ToggleFrameRateKey}: frame rate");
    }
}
