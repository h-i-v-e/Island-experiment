using System;
using System.IO;
using System.Linq;
using System.Text;
using Motu.Gameplay;
using Motu.World;
using UnityEditor;
using UnityEditor.SceneManagement;
using UnityEngine;
using UnityEngine.SceneManagement;

// Uses Unity's Play-mode start-scene override; the user's editing scenes are restored on exit.
[InitializeOnLoad]
public static class BoatTestPlayValidation
{
    private const string Running = "BoatTestValidation.Running";
    private const string PreviousStart = "BoatTestValidation.PreviousStart";
    private const string TestScene = "BoatTestValidation.Scene";
    private const string ReportPath = "/tmp/terranullus-boat-play.txt";
    private static double started;
    private static int supportedFrames, sampledFrames;
    private static bool inspectionChecked;
    private static bool draftChanged;
    private static float initialDraft, shallowDraftTotal, deepDraftTotal;
    private static int shallowDraftCount, deepDraftCount;
    private static readonly StringBuilder report = new StringBuilder();
    private static float minimumY = float.MaxValue, maximumY = float.MinValue;

    static BoatTestPlayValidation()
    {
        EditorApplication.playModeStateChanged += StateChanged;
        if (SessionState.GetBool(Running, false) && EditorApplication.isPlaying) Begin();
    }

    [MenuItem("Island/Boat Tests/Run Rowing Boat Play Check")]
    public static void Run() => RunScene(BoatTestSceneSetup.RowingScene);

    [MenuItem("Island/Boat Tests/Run Empty Testbed Play Check")]
    public static void RunEmpty() => RunScene(BoatTestSceneSetup.EmptyScene);

    private static void RunScene(string path)
    {
        if (EditorApplication.isPlayingOrWillChangePlaymode) throw new InvalidOperationException("Stop Play mode first.");
        SessionState.SetString(PreviousStart, AssetDatabase.GetAssetPath(EditorSceneManager.playModeStartScene));
        SessionState.SetBool(Running, true);
        SessionState.SetString(TestScene, path);
        File.AppendAllText(ReportPath, "Starting isolated play check: " + path + "\n");
        EditorSceneManager.playModeStartScene = AssetDatabase.LoadAssetAtPath<SceneAsset>(path);
        EditorApplication.isPlaying = true;
    }

    private static void StateChanged(PlayModeStateChange state)
    {
        if (!SessionState.GetBool(Running, false)) return;
        if (state == PlayModeStateChange.EnteredPlayMode) Begin();
        if (state == PlayModeStateChange.EnteredEditMode)
        {
            EditorSceneManager.playModeStartScene = AssetDatabase.LoadAssetAtPath<SceneAsset>(SessionState.GetString(PreviousStart, ""));
            SessionState.SetBool(Running, false);
            EditorApplication.update -= Tick;
        }
    }

    private static void Begin()
    {
        EditorApplication.update -= Tick;
        started = EditorApplication.timeSinceStartup;
        report.Clear();
        supportedFrames = sampledFrames = 0;
        inspectionChecked = false;
        draftChanged = false;
        initialDraft = shallowDraftTotal = deepDraftTotal = 0f;
        shallowDraftCount = deepDraftCount = 0;
        minimumY = float.MaxValue;
        maximumY = float.MinValue;
        EditorApplication.update += Tick;
    }

    private static void Tick()
    {
        if (!EditorApplication.isPlaying) return;
        try
        {
            var scene = SceneManager.GetActiveScene();
            if (scene.path != SessionState.GetString(TestScene, ""))
            {
                if (EditorApplication.timeSinceStartup - started > 45) throw new InvalidOperationException("Test scene did not start.");
                return;
            }
            if (scene.path == BoatTestSceneSetup.EmptyScene)
            {
                if (EditorApplication.timeSinceStartup - started < 5) return;
                var observer = UnityEngine.Object.FindAnyObjectByType<BoatTestPlayer>();
                Check(UnityEngine.Object.FindAnyObjectByType<FloatingShip>() == null, "empty testbed has no boat");
                Check(UnityEngine.Object.FindAnyObjectByType<IslandWorldManager>() == null, "empty testbed has no island world");
                Check(UnityEngine.Object.FindAnyObjectByType<WorldEnvironmentController>().IsInstalled, "empty testbed creates its sea and sky");
                Check(!observer.GetComponent<CharacterController>().enabled && !observer.GetComponent<Animator>().enabled,
                    "empty player starts in inspection flight");
                Check(Mathf.Abs(observer.transform.position.y - 2f) < .01f, "empty player remains above the sea");
                observer.ResetPlayer();
                Check(!observer.GetComponent<CharacterController>().enabled, "empty reset restores flight without falling into the sea");
                report.AppendLine("SUCCESS: Empty testbed runtime checks passed.");
                Finish();
                return;
            }
            var boat = UnityEngine.Object.FindAnyObjectByType<FloatingShip>();
            var buoyancy = UnityEngine.Object.FindAnyObjectByType<OceanBuoyancy>();
            var controls = UnityEngine.Object.FindAnyObjectByType<BoatTestPlayer>();
            var player = controls.GetComponents<MonoBehaviour>().First(c => c.GetType().Name == "PlayerController");
            double elapsed = EditorApplication.timeSinceStartup - started;
            if (elapsed < 4) return;
            var hullBody = buoyancy.GetComponent<Rigidbody>();
            Check(buoyancy.GetComponent<HullDraftLoading>() != null, "rowboat has draft-controlled loading");
            if (buoyancy.TryWaterHeight(hullBody.position, out float surface, out _))
            {
                float measuredDraft = surface - hullBody.position.y;
                if (elapsed < 7.5) { shallowDraftTotal += measuredDraft; shallowDraftCount++; }
                if (elapsed > 14) { deepDraftTotal += measuredDraft; deepDraftCount++; }
            }
            if (elapsed >= 8 && !draftChanged)
            {
                initialDraft = buoyancy.Draft;
                Vector3 previousPosition = hullBody.position;
                buoyancy.Draft += .2f;
                Check(hullBody.position == previousPosition, "changing draft does not teleport the boat");
                draftChanged = true;
            }
            Check(UnityEngine.Object.FindObjectsByType<WorldEnvironmentController>().Length == 1, "one active standalone environment");
            Check(UnityEngine.Object.FindAnyObjectByType<IslandWorldManager>() == null, "play scene has no island world");
            Check(buoyancy.Ocean != null && buoyancy.Ocean.SurfaceMaterial != null, "real ocean bound to rowboat");
            Check(float.IsFinite(boat.transform.position.y) && Mathf.Abs(boat.transform.position.y) < 2f, "boat remains near sea level");
            if (buoyancy.SubmergedFraction > .01f) sampledFrames++;
            if ((bool)player.GetType().GetMethod("IsSupportedBy").Invoke(player, new object[] { boat.transform })) supportedFrames++;
            minimumY = Mathf.Min(minimumY, boat.transform.position.y);
            maximumY = Mathf.Max(maximumY, boat.transform.position.y);
            if (!inspectionChecked)
            {
                var view = player.GetComponentInChildren<Camera>();
                var originalParent = view.transform.parent;
                controls.SetFlying(true);
                Check(!player.enabled && !player.GetComponent<CharacterController>().enabled && !player.GetComponent<Animator>().enabled,
                    "inspection disables character movement and animation");
                Check(view.transform.parent == player.transform, "inspection camera follows freely");
                controls.ResetPlayer();
                Check(player.enabled && player.GetComponent<CharacterController>().enabled && view.transform.parent == originalParent,
                    "reset restores walking and camera hierarchy");
                inspectionChecked = true;
            }
            if (EditorApplication.timeSinceStartup - started < 18) return;
            Check(sampledFrames > 20, "GPU wave samples keep the hull afloat");
            Check(supportedFrames > 20, "real player acquires and retains moving-boat support");
            var body = buoyancy.GetComponent<Rigidbody>();
            Check(!body.isKinematic && body.useGravity && (body.constraints & RigidbodyConstraints.FreezePositionY) == 0,
                "launch hold released into dynamic flotation");
            Check(Vector3.Dot(boat.transform.up, Vector3.up) > .9f, "rowboat remains upright");
            Check(shallowDraftCount > 20 && deepDraftCount > 20, "measured waterline before and after the live draft change");
            float shallowDraft = shallowDraftTotal / shallowDraftCount;
            float deepDraft = deepDraftTotal / deepDraftCount;
            Check(Mathf.Abs(deepDraft - shallowDraft - .2f) < .06f, "increasing draft by 20 cm makes the live boat settle 20 cm deeper");
            report.AppendLine($"Measured mean draft {shallowDraft:F3} -> {deepDraft:F3} m; target {initialDraft:F2} -> {buoyancy.Draft:F2} m; mass {body.mass:F1} kg");
            report.AppendLine($"Sampled frames={sampledFrames}; supported frames={supportedFrames}; hull Y range={minimumY:F3}..{maximumY:F3} m");
            report.AppendLine("SUCCESS: Rowing scene runtime flotation, boarding and inspection checks passed.");
            Finish();
        }
        catch (Exception error) { report.AppendLine("FAIL: " + error); Finish(); }
    }

    private static void Check(bool condition, string message)
    {
        if (!condition) throw new InvalidOperationException(message);
        string line = "PASS: " + message;
        if (!report.ToString().Contains(line)) report.AppendLine(line);
    }

    private static void Finish()
    {
        EditorApplication.update -= Tick;
        File.AppendAllText(ReportPath, report.ToString());
        Debug.Log(report.ToString());
        EditorApplication.isPlaying = false;
    }
}
