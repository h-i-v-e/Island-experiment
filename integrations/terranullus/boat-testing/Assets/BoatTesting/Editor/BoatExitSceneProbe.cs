using System;
using System.IO;
using UnityEditor;
using UnityEditor.SceneManagement;
using UnityEngine;

[InitializeOnLoad]
public static class BoatExitSceneProbe
{
    const string Running = "BoatExitSceneProbe.Running", PreviousStart = "BoatExitSceneProbe.PreviousStart";
    const string Report = "/tmp/boat-exit-current-scene.txt";
    static RowingBoatTestScenario scenario;
    static BoatBoarding player;
    static float started, releasedY;
    static int phase;
    static bool rightSide;
    static double deadline;

    static BoatExitSceneProbe()
    {
        EditorApplication.playModeStateChanged += Changed;
    }

    [MenuItem("Island/Boat Tests/Inspect Exit Support In Current Crewed Scene")]
    public static void Run()
    {
        if (EditorApplication.isPlayingOrWillChangePlaymode) throw new InvalidOperationException("Stop Play mode first.");
        if (UnityEngine.Object.FindFirstObjectByType<RowingBoatTestScenario>() == null)
            throw new InvalidOperationException("Open the crewed rowboat scene first.");
        File.WriteAllText(Report, "Testing the CURRENT editing scene, including its unsaved overrides and real ocean.\n");
        SessionState.SetString(PreviousStart, AssetDatabase.GetAssetPath(EditorSceneManager.playModeStartScene));
        SessionState.SetBool("BoatExitSceneProbe.Background", Application.runInBackground);
        EditorSceneManager.playModeStartScene = null;
        SessionState.SetBool(Running, true);
        EditorApplication.isPaused = false;
        EditorApplication.isPlaying = true;
    }

    static void Changed(PlayModeStateChange change)
    {
        if (!SessionState.GetBool(Running, false)) return;
        if (change == PlayModeStateChange.EnteredPlayMode)
        {
            scenario = UnityEngine.Object.FindFirstObjectByType<RowingBoatTestScenario>();
            player = scenario.Player.GetComponent<BoatBoarding>();
            rightSide = false;
            phase = 0; started = Time.time; deadline = EditorApplication.timeSinceStartup + 60;
            Application.runInBackground = true;
            EditorApplication.update += Tick;
        }
        if (change == PlayModeStateChange.EnteredEditMode)
        {
            EditorApplication.update -= Tick;
            Application.runInBackground = SessionState.GetBool("BoatExitSceneProbe.Background", false);
            EditorSceneManager.playModeStartScene = AssetDatabase.LoadAssetAtPath<SceneAsset>(SessionState.GetString(PreviousStart, ""));
            SessionState.SetBool(Running, false);
            File.AppendAllText(Report, "Restored the user's editing scene and previous Play start scene.\n");
        }
    }

    static void Snapshot(string label)
    {
        label = (rightSide ? "right-" : "left-") + label;
        string path = "/tmp/boat-support-" + label + ".txt";
        BoatSupportDiagnostics.Capture(path);
        File.AppendAllText(Report, label + "\n" + File.ReadAllText(path) + "\n");
    }

    static void Tick()
    {
        if (!EditorApplication.isPlaying) return;
        try
        {
            if (EditorApplication.timeSinceStartup > deadline) throw new TimeoutException("Current scene probe timed out.");
            float elapsed = Time.time - started;
            if (phase == 0 && elapsed > 3)
            {
                Snapshot("before-exit");
                if (!player.RequestExit(true)) throw new InvalidOperationException("Player was not seated and ready to exit.");
                phase = 1; started = Time.time;
            }
            else if (phase == 1 && player.State == BoatBoarding.BoardingState.Idle)
            {
                releasedY = player.transform.position.y;
                Snapshot("released");
                phase = 2; started = Time.time;
            }
            else if (phase == 2 && elapsed > .5f)
            { Snapshot("half-second"); phase = 3; }
            else if (phase == 3 && elapsed > 2)
            {
                Snapshot("two-seconds");
                float y = player.transform.position.y;
                bool fell = y < releasedY - 1 && !player.GetComponent<CharacterController>().isGrounded;
                File.AppendAllText(Report, $"{(fell ? "PASS" : "FAIL")}: {(rightSide ? "right" : "left")} exit root height: released {releasedY:F3}, after two seconds {y:F3}.\n");
                if (!fell) throw new InvalidOperationException("The current scene still supports the player after this exit; see collider snapshots above.");
                if (rightSide) Finish();
                else
                {
                    scenario.ReturnAboard();
                    rightSide = true;
                    phase = 4; started = Time.time;
                }
            }
            else if (phase == 4 && elapsed > .5f)
            {
                Snapshot("before-exit");
                if (!player.RequestExit(false)) throw new InvalidOperationException("Right exit could not start.");
                phase = 1; started = Time.time;
            }
        }
        catch (Exception error)
        { File.AppendAllText(Report, "FAIL: " + error + "\n"); Finish(); }
    }

    static void Finish()
    {
        EditorApplication.update -= Tick;
        Debug.Log("Current-scene boat exit support report: " + Report);
        EditorApplication.isPlaying = false;
    }
}
