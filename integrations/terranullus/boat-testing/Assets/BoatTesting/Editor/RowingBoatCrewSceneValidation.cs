using System;
using System.IO;
using System.Linq;
using Motu.World;
using UnityEditor;
using UnityEditor.SceneManagement;
using UnityEngine;
using UnityEngine.InputSystem;
using UnityEngine.InputSystem.LowLevel;
using UnityEngine.SceneManagement;

[InitializeOnLoad]
public static class RowingBoatCrewSceneValidation
{
    private const string Running = "RowingBoatCrewSceneValidation.Running";
    private const string PreviousStart = "RowingBoatCrewSceneValidation.PreviousStart";
    private const string Report = "/tmp/crewed-rowboat-scene-play.txt";
    private static RowingBoatTestScenario scenario;
    private static int phase;
    private static float started;
    private static Vector3 startPosition;
    private static bool background;
    private static InputSettings.BackgroundBehavior inputBackground;

    static RowingBoatCrewSceneValidation() => EditorApplication.playModeStateChanged += StateChanged;

    [MenuItem("Island/Boat Tests/Run Crewed Rowboat Scene Play Check")]
    public static void Run()
    {
        RowingBoatCrewSceneSetup.Create();
        SessionState.SetString(PreviousStart, AssetDatabase.GetAssetPath(EditorSceneManager.playModeStartScene));
        EditorSceneManager.playModeStartScene = AssetDatabase.LoadAssetAtPath<SceneAsset>(RowingBoatCrewSceneSetup.ScenePath);
        SessionState.SetBool(Running, true);
        File.WriteAllText(Report, "Testing the saved crewed scene without spawning additional boats or characters.\n");
        EditorApplication.isPaused = false;
        EditorApplication.isPlaying = true;
    }

    private static void StateChanged(PlayModeStateChange state)
    {
        if (!SessionState.GetBool(Running, false)) return;
        if (state == PlayModeStateChange.EnteredPlayMode)
        {
            background = Application.runInBackground;
            inputBackground = InputSystem.settings.backgroundBehavior;
            Application.runInBackground = true;
            InputSystem.settings.backgroundBehavior = InputSettings.BackgroundBehavior.IgnoreFocus;
            scenario = UnityEngine.Object.FindFirstObjectByType<RowingBoatTestScenario>();
            Keys(); phase = 0; started = Time.time;
            EditorApplication.update += Tick;
        }
        if (state == PlayModeStateChange.EnteredEditMode)
        {
            EditorApplication.update -= Tick;
            SessionState.SetBool(Running, false);
            Application.runInBackground = background;
            InputSystem.settings.backgroundBehavior = inputBackground;
            EditorSceneManager.playModeStartScene = AssetDatabase.LoadAssetAtPath<SceneAsset>(SessionState.GetString(PreviousStart, ""));
            bool dirty = Enumerable.Range(0, SceneManager.sceneCount).Any(i => SceneManager.GetSceneAt(i).isDirty);
            File.AppendAllText(Report, "Editing scenes restored; unsaved changes present: " + dirty + ".\n");
        }
    }

    private static void Keys(params Key[] keys)
    { if (Keyboard.current != null) InputSystem.QueueStateEvent(Keyboard.current, new KeyboardState(keys)); }

    private static void Tick()
    {
        if (!EditorApplication.isPlaying) return;
        try
        {
            float elapsed = Time.time - started;
            Require(scenario != null, "Saved scene contains the crewed test scenario.");
            if (phase == 0 && elapsed > 3)
            {
                Require(UnityEngine.Object.FindObjectsByType<FloatingShip>(FindObjectsSortMode.None).Length == 1, "Exactly one floating boat.");
                Require(UnityEngine.Object.FindObjectsByType<NpcController>(FindObjectsSortMode.None).Length == 2, "Exactly two NPC rowers.");
                Require(UnityEngine.Object.FindObjectsByType<PlayerController>(FindObjectsSortMode.None).Count(p => p.UsesPlayerInput) == 1, "Exactly one controlled player.");
                Require(UnityEngine.Object.FindObjectsByType<Camera>(FindObjectsSortMode.None).Count(c => c.enabled && c.CompareTag("MainCamera")) == 1
                    && UnityEngine.Object.FindObjectsByType<AudioListener>(FindObjectsSortMode.None).Count(a => a.enabled) == 1, "One active player view/listener.");
                Require(UnityEngine.Object.FindFirstObjectByType<IslandWorldManager>() == null && UnityEngine.Object.FindFirstObjectByType<TerrainCollider>() == null,
                    "Open water without terrain or island generation.");
                Require(scenario.Player.GetComponent<BoatBoarding>().CurrentSeat == scenario.Boat.CommandSeat
                    && scenario.Player.GetComponent<BoatBoarding>().State == BoatBoarding.BoardingState.Seated, "Player starts seated aft.");
                Require(scenario.Rowers.All(n => n.Boarding.CanRow) && scenario.Boat.ActiveRowers == 2, "Both NPCs start in their rowing seats.");
                Require(!scenario.Boat.Body.isKinematic && scenario.Boat.Body.useGravity, "Hull is floating dynamically after ocean readback.");
                File.AppendAllText(Report, "PASS: Saved scene starts in open water with two seated NPC rowers, the player in the command seat, one player view/listener, and a dynamic floating hull.\n");
                startPosition = scenario.Boat.Body.position;
                Keys(Key.W); phase = 1; started = Time.time;
            }
            else if (phase == 1 && elapsed > 4)
            {
                float distance = Vector3.ProjectOnPlane(scenario.Boat.Body.position - startPosition, Vector3.up).magnitude;
                Require(distance > 1 && scenario.Boat.Input.y > .9f
                    && scenario.Rowers.All(n => n.GetComponent<Animator>().GetFloat("YAxis") > .9f), "W moves the boat and animates both rowers.");
                File.AppendAllText(Report, $"PASS: Actual W input moves the saved boat {distance:F2} m in four seconds with both rowing animations.\n");
                Keys();
                scenario.Player.GetComponent<BoatBoarding>().CancelBoarding();
                phase = 2; started = Time.time;
            }
            else if (phase == 2 && elapsed > .3f)
            { Keys(Key.R); phase = 3; started = Time.time; }
            else if (phase == 3 && elapsed > .3f)
            {
                Require(scenario.Player.GetComponent<BoatBoarding>().State == BoatBoarding.BoardingState.Seated
                    && scenario.Boat.CommandSeat.Occupant == scenario.Player.GetComponent<BoatBoarding>(), "R returns the player to the aft seat.");
                Require(scenario.Rowers.All(n => n.Boarding.CanRow), "Crew remains ready after recovery.");
                Finish("PASS: R recovers the player aboard; the saved crewed scene is ready for manual testing.");
            }
        }
        catch (Exception error) { Finish("FAIL: " + error); }
    }

    private static void Require(bool condition, string message)
    { if (!condition) throw new InvalidOperationException(message); }

    private static void Finish(string message)
    {
        EditorApplication.update -= Tick;
        Keys();
        File.AppendAllText(Report, message + "\n");
        Debug.Log(File.ReadAllText(Report));
        EditorApplication.isPlaying = false;
    }
}
