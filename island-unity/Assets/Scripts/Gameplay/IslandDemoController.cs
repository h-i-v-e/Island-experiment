using UnityEngine;
using Motu.Islands;
using Motu.World;

namespace Motu.Gameplay
{
    [UnityEngine.Scripting.APIUpdating.MovedFrom(true, sourceNamespace: "", sourceAssembly: "Assembly-CSharp", sourceClassName: "IslandDemoController")]
    public sealed class IslandDemoController : MonoBehaviour
    {
        private const float ClickDragTolerance = 6f;
        private const float FrameRateSampleSeconds = 0.25f;
        private const int MinimapRadiusCells = 16;
        private const int MinimapDiameterCells = MinimapRadiusCells * 2 + 1;
        private const float MinimapCellPixels = 7f;
        private const float MinimapTexturePixels = MinimapDiameterCells * MinimapCellPixels;
        private const float MinimapPanelWidth = MinimapTexturePixels + 20f;
        private const float MinimapPanelHeight = MinimapTexturePixels + 74f;
        private static readonly Rect PanelRect = new Rect(16f, 16f, 600f, 250f);
        private static readonly Color32 MinimapSeaColour = new Color32(18, 63, 105, 255);
        private static readonly Color32 MinimapIslandColour = new Color32(79, 139, 61, 255);
        private static readonly Color MinimapPlayerColour = new Color(1f, 0.82f, 0.18f, 1f);

        [SerializeField] private IslandWorldManager worldManager;
        [SerializeField] private Camera viewerCamera;
        [SerializeField] private OrbitCamera orbitCamera;
        [SerializeField] private FirstPersonController firstPersonController;
        [SerializeField] private ShipController shipController;
        [Header("Play Mode Start")]
        [SerializeField] private bool startInFlyMode;
        [SerializeField] private Vector3 flyStartPosition = new Vector3(0f, 4f, -1800f);
        [SerializeField] private float flyStartYawDegrees;
        [SerializeField] private float flyStartPitchDegrees;
        [Header("World Minimap")]
        [SerializeField] private bool showMinimap = true;

        private bool clickCandidate;
        private Vector2 clickStart;
        private float frameRateSampleTime;
        private int frameRateSampleFrames;
        private string frameRateText = "FPS: --";
        internal Texture2D minimapTexture;
        private Color32[] minimapPixels;
        private IIslandGenerationRequestFactory minimapFactory;
        internal Vector2Int minimapCentreCell;
        internal bool hasMinimapCentre;
        private bool minimapClickCandidate;
        private bool minimapClickDragged;
        private Vector2 minimapClickStart;
        private Vector2Int minimapClickCell;

        public bool ShowMinimap
        {
            get => showMinimap;
            set
            {
                showMinimap = value;
                hasMinimapCentre = false;
            }
        }

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

        public void ConfigureShipStart(ShipController controller, Camera camera)
        {
            shipController = controller;
            viewerCamera = camera;
            startInFlyMode = false;
        }

        private void Awake()
        {
            if (worldManager == null || viewerCamera == null || orbitCamera == null)
            {
                enabled = false;
                Debug.LogWarning("IslandDemoController is missing its scene references.", this);
                return;
            }
            if (shipController != null)
            {
                orbitCamera.enabled = false;
                if (firstPersonController != null) firstPersonController.enabled = false;
                worldManager.SetStreamingTarget(viewerCamera.transform);
                worldManager.SetFirstPersonViewActive(true);
                return;
            }
            orbitCamera.Configure(
                IslandWorldManager.CellCentre(Vector2Int.zero, 60f),
                IslandWorldManager.IslandSizeMetres * 1.15f);
            firstPersonController?.Configure(
                orbitCamera,
                worldManager);
            worldManager.SetStreamingTarget(viewerCamera.transform);
        }

        private void Start()
        {
            if (shipController == null && startInFlyMode && firstPersonController != null)
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
            UpdateMinimap();
            var mousePosition = Input.mousePosition;
            var guiPosition = new Vector2(mousePosition.x, Screen.height - mousePosition.y);
            var cursorAvailable = Cursor.lockState != CursorLockMode.Locked;
            if (orbitCamera != null)
            {
                orbitCamera.PointerInputBlocked = cursorAvailable
                    && (minimapClickCandidate || IsOverMinimap(guiPosition));
            }
            if (HandleMinimapPointer(guiPosition, Input.GetMouseButtonDown(0),
                Input.GetMouseButtonUp(0), cursorAvailable))
            {
                clickCandidate = false;
                return;
            }
            var island = worldManager.FocusedIsland;
            if (shipController != null || firstPersonController == null
                || firstPersonController.IsActive
                || island == null
                || viewerCamera == null)
            {
                return;
            }

            if (Input.GetMouseButtonDown(0))
            {
                clickStart = Input.mousePosition;
                clickCandidate = !PanelRect.Contains(guiPosition) && !IsOverMinimap(guiPosition);
            }
            if (!Input.GetMouseButtonUp(0) || !clickCandidate)
            {
                return;
            }

            clickCandidate = false;
            var releasedAt = (Vector2)Input.mousePosition;
            if (IsOverMinimap(guiPosition) || PanelRect.Contains(guiPosition)
                || (releasedAt - clickStart).sqrMagnitude
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
            var minimapPanel = DrawMinimap();
            var island = worldManager.FocusedIsland;
            if (island != null && island.DebugSettings.ShowFrameRate)
            {
                var frameRateY = minimapPanel.height > 0f
                    ? minimapPanel.yMax + 8f
                    : 16f;
                GUI.Label(
                    new Rect(Mathf.Max(16f, Screen.width - 116f), frameRateY, 100f, 28f),
                    frameRateText,
                    GUI.skin.box);
            }
            if (shipController != null)
            {
                GUILayout.BeginArea(new Rect(16f, 16f, 480f, 126f), GUI.skin.box);
                GUILayout.Label("Ship helm: W/S forward/reverse | A/D rudder | Space brake");
                GUILayout.Label("Mouse: look | Tab: mouse look/cursor | Escape: release cursor");
                GUILayout.Label($"Speed: {shipController.SpeedMetresPerSecond * 1.943844f:0.0} knots");
                DrawWorldStatus();
                GUILayout.EndArea();
                return;
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

        private void UpdateMinimap()
        {
            if (!showMinimap || worldManager == null)
            {
                return;
            }
            var factory = worldManager.IslandGenerationRequestFactory;
            if (factory == null)
            {
                return;
            }

            var logicalPosition = worldManager.LogicalPlayerPosition;
            var centreCell = IslandWorldManager.WorldToCell(
                new Vector3(logicalPosition.x, 0f, logicalPosition.y));
            var factoryChanged = !ReferenceEquals(minimapFactory, factory);
            if (!factoryChanged
                && hasMinimapCentre
                && minimapCentreCell == centreCell)
            {
                return;
            }

            EnsureMinimapTexture();
            for (var z = -MinimapRadiusCells; z <= MinimapRadiusCells; z++)
            {
                for (var x = -MinimapRadiusCells; x <= MinimapRadiusCells; x++)
                {
                    var cell = centreCell + new Vector2Int(x, z);
                    var pixelIndex = (z + MinimapRadiusCells) * MinimapDiameterCells
                        + x + MinimapRadiusCells;
                    minimapPixels[pixelIndex] = worldManager.HasIsland(cell)
                        ? MinimapIslandColour
                        : MinimapSeaColour;
                }
            }
            minimapTexture.SetPixels32(minimapPixels);
            minimapTexture.Apply(false, false);
            minimapFactory = factory;
            minimapCentreCell = centreCell;
            hasMinimapCentre = true;
        }

        private void EnsureMinimapTexture()
        {
            if (minimapTexture != null)
            {
                return;
            }
            minimapTexture = new Texture2D(
                MinimapDiameterCells,
                MinimapDiameterCells,
                TextureFormat.RGBA32,
                false,
                true)
            {
                name = "Island occupancy minimap",
                filterMode = FilterMode.Point,
                wrapMode = TextureWrapMode.Clamp,
                hideFlags = HideFlags.HideAndDontSave,
            };
            minimapPixels = new Color32[MinimapDiameterCells * MinimapDiameterCells];
        }

        private Rect DrawMinimap()
        {
            if (!showMinimap || minimapTexture == null || !hasMinimapCentre)
            {
                return default;
            }

            var panel = MinimapPanelRect();
            GUI.Box(panel, GUIContent.none);
            GUI.Label(
                new Rect(panel.x + 10f, panel.y + 5f, panel.width - 20f, 20f),
                $"Island map ±16 cells | centre {minimapCentreCell.x}, {minimapCentreCell.y}");
            var map = MinimapMapRect();
            GUI.DrawTexture(map, minimapTexture, ScaleMode.StretchToFill, false);
            DrawMinimapCentre(map);
            GUI.Label(
                new Rect(panel.x + 10f, map.yMax + 3f, panel.width - 20f, 20f),
                "N ↑    green: island    blue: open sea");
            GUI.Label(
                new Rect(panel.x + 10f, map.yMax + 23f, panel.width - 20f, 20f),
                Cursor.lockState == CursorLockMode.Locked
                    ? "Tab: release cursor to teleport"
                    : shipController != null ? "Click a square: teleport ship" : "Click a square: teleport (fly mode)");
            return panel;
        }

        private static Rect MinimapPanelRect()
        {
            return new Rect(Mathf.Max(8f, Screen.width - MinimapPanelWidth - 16f),
                16f, MinimapPanelWidth, MinimapPanelHeight);
        }

        internal static Rect MinimapMapRect()
        {
            var panel = MinimapPanelRect();
            return new Rect(panel.x + 10f, panel.y + 26f, MinimapTexturePixels, MinimapTexturePixels);
        }

        private bool IsOverMinimap(Vector2 guiPosition)
        {
            return showMinimap && minimapTexture != null && hasMinimapCentre
                && MinimapPanelRect().Contains(guiPosition);
        }

        internal bool TryGetMinimapCell(Vector2 guiPosition, out Vector2Int cell)
        {
            cell = default;
            var map = MinimapMapRect();
            if (!IsOverMinimap(guiPosition) || !map.Contains(guiPosition))
                return false;
            var column = Mathf.FloorToInt((guiPosition.x - map.x) / MinimapCellPixels);
            var row = Mathf.FloorToInt((guiPosition.y - map.y) / MinimapCellPixels);
            // GUI Y points down; the texture's positive world Z points north/up.
            cell = minimapCentreCell + new Vector2Int(
                column - MinimapRadiusCells, MinimapRadiusCells - row);
            return true;
        }

        internal bool HandleMinimapPointer(Vector2 position, bool pressed, bool released, bool cursorAvailable)
        {
            if (!cursorAvailable || (firstPersonController == null && shipController == null))
            {
                minimapClickCandidate = false;
                return false;
            }
            if (pressed)
            {
                minimapClickCandidate = TryGetMinimapCell(position, out minimapClickCell);
                minimapClickDragged = false;
                minimapClickStart = position;
            }
            if (!minimapClickCandidate)
                return false;
            minimapClickDragged |= (position - minimapClickStart).sqrMagnitude
                > ClickDragTolerance * ClickDragTolerance;
            if (released)
            {
                minimapClickCandidate = false;
                if (!minimapClickDragged
                    && TryGetMinimapCell(position, out var cell) && cell == minimapClickCell)
                {
                    if (shipController != null) shipController.Teleport(IslandWorldManager.CellCentre(cell));
                    else firstPersonController.Teleport(IslandWorldManager.CellCentre(cell));
                    UpdateMinimap();
                }
            }
            return true;
        }

        private void OnDisable()
        {
            minimapClickCandidate = false;
            clickCandidate = false;
            if (orbitCamera != null)
                orbitCamera.PointerInputBlocked = false;
        }

        private static void DrawMinimapCentre(Rect map)
        {
            var centre = new Rect(
                map.x + MinimapRadiusCells * MinimapCellPixels,
                map.y + MinimapRadiusCells * MinimapCellPixels,
                MinimapCellPixels,
                MinimapCellPixels);
            var previousColour = GUI.color;
            GUI.color = MinimapPlayerColour;
            GUI.DrawTexture(
                new Rect(centre.x, centre.y, centre.width, 1f),
                Texture2D.whiteTexture);
            GUI.DrawTexture(
                new Rect(centre.x, centre.yMax - 1f, centre.width, 1f),
                Texture2D.whiteTexture);
            GUI.DrawTexture(
                new Rect(centre.x, centre.y, 1f, centre.height),
                Texture2D.whiteTexture);
            GUI.DrawTexture(
                new Rect(centre.xMax - 1f, centre.y, 1f, centre.height),
                Texture2D.whiteTexture);
            GUI.color = previousColour;
        }

        private void OnDestroy()
        {
            if (minimapTexture == null)
            {
                return;
            }
            if (Application.isPlaying)
            {
                Destroy(minimapTexture);
            }
            else
            {
                DestroyImmediate(minimapTexture);
            }
            minimapTexture = null;
            minimapPixels = null;
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
                $"Position {logicalPosition.x:0}, {logicalPosition.y:0} m | focus: {focused}");
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
}
