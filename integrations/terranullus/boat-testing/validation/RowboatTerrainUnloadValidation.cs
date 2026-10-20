using System;
using System.IO;
using System.Linq;
using System.Reflection;
using Motu.Islands;
using Motu.World;
using UnityEngine.InputSystem.LowLevel;
using UnityEditor;
using UnityEditor.SceneManagement;
using UnityEngine;
using UnityEngine.InputSystem;

[InitializeOnLoad]
public static class RowboatTerrainUnloadValidation
{
    const string Running = "RowboatTerrainUnloadValidation.Running";
    static RowboatController controls;
    static BoatBoarding player;
    static Rigidbody body;
    static float started;
    static int phase;
    static Vector3 start;
    static float windError;
    static double deadline;
    static GameObject shore;
    static bool baseline;
    static GameObject worldFixture;
    static Vector3 heldPosition;
    static readonly BindingFlags Private = BindingFlags.Instance | BindingFlags.NonPublic;
    sealed class IslandSource : IIslandGenerationRequestFactory
    {
        public bool HasIsland(Vector2Int cell) => true;
        public IslandGenerationRequest CreateIslandGenerationRequest(Vector2Int cell) => throw new InvalidOperationException("Read-only fixture");
    }
    static void Set(object target, string name, object value) => target.GetType().GetField(name, Private).SetValue(target, value);
    static Keyboard keyboard;
    static RowboatTerrainUnloadValidation() => EditorApplication.playModeStateChanged += Changed;
    public static void Run()
    {
        if (!Application.isBatchMode || !File.Exists(".boat-exit-isolated-project")) throw new InvalidOperationException("Isolated project only.");
        EditorSceneManager.OpenScene("Assets/Scenes/RowingBoatCrewTest.unity");
        InputSystem.actions = AssetDatabase.LoadAssetAtPath<InputActionAsset>("Assets/InputSystem_Actions.inputactions");
        SessionState.SetBool(Running, true);
        File.WriteAllText("/tmp/rowboat-terrain-unload-play.txt", "Saved crewed scene, real ocean, native shore collider and actual passenger climb-out.\n");
        EditorApplication.isPlaying = true;
    }
    static void Changed(PlayModeStateChange change)
    {
        if (!SessionState.GetBool(Running, false)) return;
        if (change == PlayModeStateChange.EnteredPlayMode)
        {
            Application.runInBackground = true;
            InputSystem.settings.updateMode = InputSettings.UpdateMode.ProcessEventsManually;
            InputSystem.settings.editorInputBehaviorInPlayMode = InputSettings.EditorInputBehaviorInPlayMode.AllDeviceInputAlwaysGoesToGameView;
            InputSystem.settings.backgroundBehavior = InputSettings.BackgroundBehavior.IgnoreFocus;
            keyboard = InputSystem.AddDevice<Keyboard>("RowboatValidationKeyboard");
            controls = UnityEngine.Object.FindFirstObjectByType<RowboatController>();
            body = controls.GetComponent<FloatingShip>().Hull;
            baseline = Environment.GetCommandLineArgs().Contains("-beachBaseline");
            if (baseline) { var settings = new SerializedObject(controls); settings.FindProperty("holdOnShore").boolValue = false; settings.ApplyModifiedPropertiesWithoutUndo(); }
            player = UnityEngine.Object.FindFirstObjectByType<RowingBoatTestScenario>().Player.GetComponent<BoatBoarding>();
            started = Time.time; phase = 0; deadline = EditorApplication.timeSinceStartup + 240;
            EditorApplication.update += Tick;
        }
        if (change == PlayModeStateChange.EnteredEditMode)
        {
            SessionState.SetBool(Running, false);
            EditorApplication.Exit(SessionState.GetInt(Running + ".result", 1));
        }
    }
    static void Tick()
    {
        EditorApplication.QueuePlayerLoopUpdate();
        InputSystem.Update();
        try
        {
            if (EditorApplication.timeSinceStartup > deadline) throw new TimeoutException();
            float elapsed = Time.time - started;
            if (phase == 0 && elapsed > 4)
            {
                if (body.isKinematic) throw new Exception("Hull has not launched.");
                body.GetComponent<Motu.Gameplay.OceanBuoyancy>().WaterCurrent = Vector3.zero;
                shore = GameObject.CreatePrimitive(PrimitiveType.Cube);
                shore.name = "Sloping beach";
                float bottom = body.GetComponentsInChildren<Collider>().Where(c => !c.isTrigger && c.attachedRigidbody == body).Min(c => c.bounds.min.y);
                shore.transform.position = new Vector3(body.position.x, bottom - .45f, body.position.z);
                shore.transform.localScale = new Vector3(30, 1, 30);
                body.linearVelocity = Vector3.zero;
                body.angularVelocity = Vector3.zero;
                Physics.SyncTransforms();
                if (!player.RequestExit(true)) throw new Exception("Cannot start actual exit.");
                start = body.position; started = Time.time; phase = 1;
            }
            else if (phase == 1 && elapsed > 12)
            {
                var held = typeof(RowboatController).GetProperty("IsShoreHeld");
                File.AppendAllText("/tmp/rowboat-terrain-unload-play.txt", $"After exit: state={player.State}, input={controls.Input}, displacement={body.position-start:F4}, velocity={body.linearVelocity:F4}, shore-held={held?.GetValue(controls)}\n");
                if (controls.Input != Vector2.zero || player.State != BoatBoarding.BoardingState.Idle) throw new Exception("Exit must stop orders and finish.");
                if (baseline) { Finish(0); return; }
                if (!controls.IsShoreHeld || Vector3.ProjectOnPlane(body.position-start, Vector3.up).magnitude > .3f)
                    throw new Exception("Beached boat must remain within 30 cm of its disembark position.");
                worldFixture = new GameObject("Unloaded shoreline fixture");
                worldFixture.SetActive(false);
                var world = worldFixture.AddComponent<IslandWorldManager>();
                world.IslandGenerationRequestFactory = new IslandSource();
                var floating = controls.GetComponent<FloatingShip>();
                Set(floating, "world", world);
                Set(floating, "nextWorldSearch", float.PositiveInfinity);
                Set(floating, "players", UnityEngine.Object.FindObjectsByType<PlayerController>(FindObjectsInactive.Exclude));
                var motor = player.GetComponent<PlayerController>();
                motor.enabled = false;
                player.GetComponent<CharacterController>().enabled = false;
                player.transform.position = body.position + Vector3.right * 150;
                heldPosition = body.position;
                shore.SetActive(false); // Same loss of collision residency as the terrain streamer.
                started = Time.time; phase = 10;
            }
            else if (phase == 10 && elapsed > 2)
            {
                File.AppendAllText("/tmp/rowboat-terrain-unload-play.txt", $"Terrain unloaded with only NPC rowers aboard: kinematic={body.isKinematic}, shore-held={controls.IsShoreHeld}, displacement={body.position-heldPosition:F4}.\n");
                if (!body.isKinematic || !controls.IsShoreHeld || Vector3.Distance(body.position, heldPosition) > .01f)
                    throw new Exception("An unattended beached boat must preserve its shore hold and suspend when the terrain unloads; NPC rowers cannot bypass collision readiness.");
                File.AppendAllText("/tmp/rowboat-terrain-unload-play.txt", "PASS: Terrain unload preserves the beached boat's position and shore hold, with NPCs still aboard.\n");
                UnityEngine.Object.Destroy(shore); // Also exercise destruction, rather than temporary deactivation.
                player.GetComponent<PlayerController>().enabled = true;
                player.GetComponent<CharacterController>().enabled = true;
                if (!player.TrySeatImmediately(controls.CommandSeat, true)) throw new Exception("Cannot reboard.");
                started = Time.time; phase = 11;
            }
            else if (phase == 11 && elapsed > 2)
            {
                if (body.isKinematic || !controls.IsShoreHeld || Vector3.ProjectOnPlane(body.position-heldPosition, Vector3.up).magnitude > .3f)
                    throw new Exception("Reboarding must resume physics without dropping the saved beach hold when the old collider is destroyed.");
                File.AppendAllText("/tmp/rowboat-terrain-unload-play.txt", "PASS: Reboarding resumes buoyancy and retains the beach position after collider destruction.\n");
                Set(controls.GetComponent<FloatingShip>(), "world", null);
                UnityEngine.Object.Destroy(worldFixture);
                InputSystem.QueueStateEvent(keyboard, new KeyboardState(Key.W));
                started = Time.time; phase = 2;
            }
            else if (phase == 2 && elapsed > 1)
            {
                if (controls.IsShoreHeld || controls.Input.y < .9f || controls.ActiveRowers != 2)
                    throw new Exception($"W must release hold/drive crew: held={controls.IsShoreHeld}, input={controls.Input}, rowers={controls.ActiveRowers}, state={player.State}, playerEnabled={player.GetComponent<PlayerController>().enabled}, movement={player.GetComponent<PlayerController>().MovementInput}, keyboard={keyboard.wKey.isPressed}.");
                File.AppendAllText("/tmp/rowboat-terrain-unload-play.txt", "PASS: Reboard and real W input release the shore hold and command both rowers.\n");
                UnityEngine.Object.Destroy(shore);
                start = body.position; started = Time.time; phase = 3;
            }
            else if (phase == 3 && elapsed > 3)
            {
                if (body.position.z - start.z < .3f) throw new Exception("Boat must row forward after release.");
                File.AppendAllText("/tmp/rowboat-terrain-unload-play.txt", $"PASS: Released boat rows {body.position.z-start.z:F3} m forward in open water.\n");
                InputSystem.QueueStateEvent(keyboard, new KeyboardState());
                player.CancelBoarding();
                started = Time.time; phase = 4;
            }
            else if (phase == 4 && elapsed > 1)
            {
                if (controls.IsShoreHeld) throw new Exception("Open-water exit cannot hold the boat.");
                start = body.position; body.linearVelocity = new Vector3(0, body.linearVelocity.y, -1);
                started = Time.time; phase = 5;
            }
            else if (phase == 5 && elapsed > 1)
            {
                if (controls.IsShoreHeld || start.z - body.position.z < .3f) throw new Exception("Open-water drift must remain free.");
                File.AppendAllText("/tmp/rowboat-terrain-unload-play.txt", $"PASS: Open-water unoccupied boat coasts freely {start.z-body.position.z:F3} m sternward; no hold.\n");
                var wind = Shader.GetGlobalVector("_MotuWeatherWind");
                windError = Vector3.Angle(Vector3.ProjectOnPlane(body.rotation * Vector3.forward, Vector3.up), new Vector3(-wind.x, 0, -wind.y));
                started = Time.time; phase = 6;
            }
            else if (phase == 6 && elapsed > 18)
            {
                var wind = Shader.GetGlobalVector("_MotuWeatherWind");
                var downwind = new Vector3(wind.x, 0, wind.y).normalized;
                float error = Vector3.Angle(Vector3.ProjectOnPlane(body.rotation * Vector3.forward, Vector3.up), -downwind);
                float drift = Vector3.Dot(body.linearVelocity, downwind);
                File.AppendAllText("/tmp/rowboat-terrain-unload-play.txt", $"Wind alignment error {windError:F1} -> {error:F1} degrees; downwind speed {drift:F3} m/s; input={controls.Input}, submerged={body.GetComponent<Motu.Gameplay.OceanBuoyancy>().SubmergedFraction:F2}.\n");
                if (error > 20 || drift < .01f) throw new Exception("Idle boat must face upwind and drift downwind.");
                Finish(0);
            }
        }
        catch (Exception error) { File.AppendAllText("/tmp/rowboat-terrain-unload-play.txt", "FAIL " + error); Finish(1); }
    }
    static void Finish(int code)
    {
        EditorApplication.update -= Tick;
        SessionState.SetInt(Running + ".result", code);
        EditorApplication.isPlaying = false;
    }
}
