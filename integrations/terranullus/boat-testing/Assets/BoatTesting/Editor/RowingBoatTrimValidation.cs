using System;
using System.IO;
using System.Linq;
using Motu.Gameplay;
using UnityEditor;
using UnityEditor.SceneManagement;
using UnityEngine;

// Validate the current unsaved boat pose without replacing the user's scene.
[InitializeOnLoad]
public static class RowingBoatTrimValidation
{
    private const string Running = "RowingBoatTrimValidation.Running";
    private const string PreviousStart = "RowingBoatTrimValidation.PreviousStart";
    private const string Report = "/tmp/rowing-boat-trim-play.txt";
    private static float started, initialDraft;
    private static bool changed;
    private static int shallowCount, deepCount, supported;
    private static Vector2 shallow, deep;

    static RowingBoatTrimValidation()
    {
        EditorApplication.playModeStateChanged += StateChanged;
        if (SessionState.GetBool(Running, false) && EditorApplication.isPlaying) Begin();
    }

    [MenuItem("Island/Boat Tests/Validate Corrected Rowboat Trim In Current Scene")]
    public static void Run()
    {
        if (EditorApplication.isPlayingOrWillChangePlaymode) throw new InvalidOperationException("Stop Play mode first.");
        if (UnityEngine.SceneManagement.SceneManager.GetActiveScene().path != BoatTestSceneSetup.RowingScene)
            throw new InvalidOperationException("Open RowingBoatTest first.");
        SessionState.SetString(PreviousStart, AssetDatabase.GetAssetPath(EditorSceneManager.playModeStartScene));
        EditorSceneManager.playModeStartScene = null;
        SessionState.SetBool(Running, true);
        File.WriteAllText(Report, "Checking current unsaved rowboat pose in the real ocean...\n");
        EditorApplication.isPlaying = true;
    }

    private static void StateChanged(PlayModeStateChange state)
    {
        if (!SessionState.GetBool(Running, false)) return;
        if (state == PlayModeStateChange.EnteredPlayMode) Begin();
        if (state == PlayModeStateChange.EnteredEditMode)
        {
            SessionState.SetBool(Running, false);
            EditorApplication.update -= Tick;
            EditorSceneManager.playModeStartScene = AssetDatabase.LoadAssetAtPath<SceneAsset>(SessionState.GetString(PreviousStart, ""));
        }
    }

    private static void Begin()
    {
        EditorApplication.update -= Tick;
        started = Time.time;
        changed = false;
        shallowCount = deepCount = supported = 0;
        shallow = deep = Vector2.zero;
        initialDraft = UnityEngine.Object.FindAnyObjectByType<OceanBuoyancy>().Draft;
        EditorApplication.update += Tick;
    }

    private static void Tick()
    {
        if (!EditorApplication.isPlaying) return;
        try
        {
            float elapsed = Time.time - started;
            if (elapsed < 10) return;
            var buoyancy = UnityEngine.Object.FindAnyObjectByType<OceanBuoyancy>();
            var body = buoyancy.GetComponent<Rigidbody>();
            var loading = buoyancy.GetComponent<HullDraftLoading>();
            var boat = UnityEngine.Object.FindAnyObjectByType<FloatingShip>();
            var controls = UnityEngine.Object.FindAnyObjectByType<BoatTestPlayer>();
            var player = controls.GetComponents<MonoBehaviour>().First(c => c.GetType().Name == "PlayerController");
            if ((bool)player.GetType().GetMethod("IsSupportedBy").Invoke(player, new object[] { boat.transform })) supported++;
            Require(buoyancy.SubmergedFraction > .01f && float.IsFinite(body.position.y) && Mathf.Abs(body.position.y) < 2,
                "Boat must remain afloat on GPU-sampled water.");
            Require(!body.isKinematic && body.useGravity && body.constraints == RigidbodyConstraints.None,
                "Boat must remain fully dynamic.");
            Require(HullDraftLoading.TryCalculateLoading(buoyancy.HullGeometry, body.transform.lossyScale,
                buoyancy.Draft, buoyancy.WaterDensity, loading.ReferenceUp, out var reference), "Valid reference loading.");
            if (buoyancy.TryWaterHeight(body.position, out float height, out _))
            {
                var sample = new Vector2(height - body.position.y - reference.MinimumHeight,
                    Mathf.DeltaAngle(0, body.rotation.eulerAngles.x));
                if (elapsed < 20) { shallow += sample; shallowCount++; }
                if (elapsed >= 32) { deep += sample; deepCount++; }
            }
            if (elapsed >= 20 && !changed)
            {
                var position = body.position;
                buoyancy.Draft = initialDraft + .2f;
                Require(Vector3.Distance(position, body.position) < .00001f, "Draft change must not teleport the boat.");
                changed = true;
            }
            if (elapsed < 42) return;
            Require(shallowCount > 20 && deepCount > 20 && supported > 20, "Waterline samples and player support must remain available.");
            shallow /= shallowCount;
            deep /= deepCount;
            float pitch = Mathf.DeltaAngle(0, loading.ReferenceRotation.eulerAngles.x);
            string measurement = $"Reference pitch {pitch:F3} deg; mean pitch {shallow.y:F3} -> {deep.y:F3} deg.\n" +
                $"Target draft {initialDraft:F3} -> {buoyancy.Draft:F3} m; measured {shallow.x:F3} -> {deep.x:F3} m.\n" +
                $"Samples {shallowCount}/{deepCount}; player supported frames {supported}; final mass {body.mass:F1} kg.\n";
            File.AppendAllText(Report, measurement);
            Require(Mathf.Abs(shallow.y - pitch) < .75f && Mathf.Abs(deep.y - pitch) < .75f, "Mean pitch must match the corrected pose.");
            Require(Mathf.Abs(shallow.x - initialDraft) < .06f && Mathf.Abs(deep.x - buoyancy.Draft) < .06f,
                "Live draft must match the configured depth.");
            Finish("PASS: Corrected trim, draft changes, dynamic flotation and player support.\n");
        }
        catch (Exception error) { Finish("FAIL: " + error + "\n"); }
    }

    private static void Require(bool condition, string message)
    {
        if (!condition) throw new InvalidOperationException(message);
    }

    private static void Finish(string message)
    {
        EditorApplication.update -= Tick;
        File.AppendAllText(Report, message);
        Debug.Log(File.ReadAllText(Report));
        EditorApplication.isPlaying = false;
    }
}
