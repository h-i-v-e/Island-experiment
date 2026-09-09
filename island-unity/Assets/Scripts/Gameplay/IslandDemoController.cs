using UnityEngine;
using Motu.Islands;
using Motu.World;

namespace Motu.Gameplay
{
    [UnityEngine.Scripting.APIUpdating.MovedFrom(true, sourceNamespace: "", sourceAssembly: "Assembly-CSharp", sourceClassName: "IslandDemoController")]
    public sealed partial class IslandDemoController : MonoBehaviour
    {
        private const float ClickDragTolerance = 6f;
        private const float FrameRateSampleSeconds = 0.25f;
        private const int MinimapRadiusCells = 16;
        private const int MinimapDiameterCells = MinimapRadiusCells * 2 + 1;
        private const float MinimapCellPixels = 7f;
        private const float MinimapTexturePixels = MinimapDiameterCells * MinimapCellPixels;
        private const float MinimapPanelWidth = MinimapTexturePixels + 20f;
        private const float MinimapPanelHeight = MinimapTexturePixels + 74f;
        private static readonly Rect PanelRect = new Rect(16f, 16f, 600f, 340f);
        private static readonly Color32 MinimapSeaColour = new Color32(18, 63, 105, 255);
        private static readonly Color32 MinimapIslandColour = new Color32(79, 139, 61, 255);
        private static readonly Color MinimapPlayerColour = new Color(1f, 0.82f, 0.18f, 1f);

        [SerializeField] private IslandWorldManager worldManager;
        [SerializeField] private Camera viewerCamera;
        [SerializeField] private OrbitCamera orbitCamera;
        [SerializeField] private FirstPersonController firstPersonController;
        [SerializeField] private ShipController shipController;
        [Tooltip("Switch between the ship helm and the flying camera.")]
        [SerializeField] private KeyCode switchCameraKey = KeyCode.F;
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
            awayFromHelm = false;
            viewerCamera = camera;
            startInFlyMode = false;
        }

        /// <summary>Debug traversal of an installed cave; normal startup remains at the helm.</summary>
        public bool VisitCave(Motu.Streaming.CaveStreamer caves, int index, bool chamber = false)
        {
            if (!Application.isPlaying || caves == null || firstPersonController == null
                || worldManager == null
                || !caves.TryGetEntrance(index, out var target, out var inward)) return false;
            if (chamber && !caves.TryGetChamber(index, out target)) return false;
            worldManager.PrepareStreamingAt(target);
            if (!caves.TryFindGround(target, 2f, 3f, out var ground)) return false;
            if (!UseExplorationCamera()) return false;
            firstPersonController.transform.rotation = Quaternion.LookRotation(inward);
            firstPersonController.EnterPreparedGround(ground);
            return true;
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
            if (switchCameraKey != KeyCode.None && Input.GetKeyDown(switchCameraKey))
                ToggleShipCamera();
            UpdateFrameRate();
            UpdateMinimap();
            var mousePosition = Input.mousePosition;
            var guiPosition = new Vector2(mousePosition.x, Screen.height - mousePosition.y);
            var cursorAvailable = Cursor.lockState != CursorLockMode.Locked;
            if (orbitCamera != null)
            {
                orbitCamera.PointerInputBlocked = cursorAvailable
                    && (minimapClickCandidate || IsOverMinimap(guiPosition) || PanelRect.Contains(guiPosition));
            }
            if (HandleMinimapPointer(guiPosition, Input.GetMouseButtonDown(0),
                Input.GetMouseButtonUp(0), cursorAvailable))
            {
                clickCandidate = false;
                return;
            }
            var island = worldManager.FocusedIsland;
            if (IsAtHelm || firstPersonController == null
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
            if (IsAtHelm)
            {
                GUILayout.BeginArea(PanelRect, GUI.skin.box);
                GUILayout.Label("Ship helm: W/S forward/reverse | A/D rudder | Space brake");
                GUILayout.Label($"{switchCameraKey}: switch to flying camera");
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
                if (firstPersonController.IsFlyMode && !firstPersonController.FollowsTerrainInFlyMode)
                    GUILayout.Label("Free flight: Q/E down/up | Shift: boost");
                else GUILayout.Label(
                    $"{firstPersonController.ToggleFlyModeKey}: fly "
                    + $"{firstPersonController.FlySpeedMetresPerSecond:0.#} m/s at "
                    + $"{firstPersonController.FlyClearanceMetres:0.#} m clearance"
                    + (firstPersonController.IsFlyMode ? " (ACTIVE)" : string.Empty));
                if (shipController != null) GUILayout.Label($"{switchCameraKey}: return to ship helm");
                DrawDebugKeys(island);
                GUILayout.Label($"{firstPersonController.ToggleTorchKey}: torch "
                    + (firstPersonController.IsTorchOn ? "ON" : "OFF")
                    + " | Tab: release cursor | Escape: overview");
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
            if (shipController != null) GUILayout.Label($"{switchCameraKey}: return to ship helm");
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
                    : "Click a square: teleport player (fly mode)");
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
                    TeleportPlayer(IslandWorldManager.CellCentre(cell));
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
            DrawCaveControls();
        }

        private void DrawCaveControls()
        {
            var position = viewerCamera != null ? viewerCamera.transform.position : transform.position;
            var total = worldManager.GetCaveAvailability(position, out var caves, out var entranceIndex);
            var current = worldManager.FocusedIsland;
            var currentCaves = current != null ? current.Runtime?.Caves : null;
            var currentCount = currentCaves != null ? currentCaves.CaveCount : 0;
            var currentStatus = current != null && current.IsGenerating ? "generating"
                : current != null && !current.Caves.Enabled ? "disabled" : currentCount.ToString();
            var status = current != null
                ? $"Caves: {currentStatus} on this island | {total} on loaded islands"
                : $"Caves: {total} on loaded islands";
            GUILayout.Label(new GUIContent(status, currentCaves != null ? currentCaves.Diagnostics : string.Empty));
            if (caves == null) return;

            caves.TryGetEntrance(entranceIndex, out var mouth, out _);
            var cell = IslandWorldManager.WorldToCell(mouth);
            var caption = new GUIContent("Teleport to cave mouth",
                $"Nearest cave: island square {cell.x}, {cell.y}; {Vector3.Distance(position, mouth):0} m away");
            var previousEnabled = GUI.enabled;
            GUI.enabled = previousEnabled && Application.isPlaying && firstPersonController != null
                && Cursor.lockState != CursorLockMode.Locked;
            if (GUILayout.Button(caption, GUILayout.Height(26f)) && !VisitCave(caves, entranceIndex))
                Debug.LogWarning("Could not enter the cave: its ground collision is not ready.", caves);
            GUI.enabled = previousEnabled;
            if (Cursor.lockState == CursorLockMode.Locked)
                GUILayout.Label("Tab: release cursor to use cave teleport");
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
