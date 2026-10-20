using System;
using System.IO;
using System.Linq;
using Motu.Gameplay;
using UnityEditor;
using UnityEditor.SceneManagement;
using UnityEngine;
using UnityEngine.InputSystem;
using UnityEngine.InputSystem.LowLevel;

[InitializeOnLoad]
public static class RowboatOarFitValidation
{
    private const string Running = "RowboatOarFitValidation.Running", PreviousStart = "RowboatOarFitValidation.PreviousStart";
    private const string Report = "/tmp/rowboat-oar-fit-play.txt";
    private static RowingBoatTestScenario scenario;
    private static BoatOar[] oars;
    private static Vector3[] bladePoints;
    private static float[] minimumHeight, maximumHeight;
    private static float started, pinError, handError, wristError, minimumLowering;
    private static int phase;
    private static bool background;
    private static InputSettings.BackgroundBehavior inputBackground;

    static RowboatOarFitValidation() => EditorApplication.playModeStateChanged += StateChanged;

    [MenuItem("Island/Boat Tests/Run Current Crewed Boat Oar Fit Play Check")]
    public static void Run()
    {
        if (EditorApplication.isPlayingOrWillChangePlaymode) throw new InvalidOperationException("Stop Play mode first.");
        if (UnityEngine.Object.FindFirstObjectByType<RowingBoatTestScenario>() == null)
            throw new InvalidOperationException("Open RowingBoatCrewTest first. This check includes unsaved scene adjustments.");
        SessionState.SetString(PreviousStart, AssetDatabase.GetAssetPath(EditorSceneManager.playModeStartScene));
        EditorSceneManager.playModeStartScene = null;
        SessionState.SetBool(Running, true);
        File.WriteAllText(Report, "Checking the current scene's authored oars during idle, forward rowing and both turns.\n");
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
            oars = scenario.Boat.GetComponentsInChildren<BoatOar>();
            bladePoints = oars.Select(o => BladePoint(o)).ToArray();
            minimumHeight = Enumerable.Repeat(float.PositiveInfinity, oars.Length).ToArray();
            maximumHeight = Enumerable.Repeat(float.NegativeInfinity, oars.Length).ToArray();
            pinError = handError = wristError = 0;
            minimumLowering = float.PositiveInfinity;
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
        }
    }

    private static Vector3 BladePoint(BoatOar oar)
    {
        var bounds = oar.Model.GetComponent<MeshFilter>().sharedMesh.bounds;
        var e = bounds.extents;
        var axis = e.x >= e.y && e.x >= e.z ? Vector3.right * e.x : e.y >= e.z ? Vector3.up * e.y : Vector3.forward * e.z;
        var a = bounds.center - axis; var b = bounds.center + axis;
        return Vector3.Distance(oar.Model.TransformPoint(a), oar.HandGrip.position)
            > Vector3.Distance(oar.Model.TransformPoint(b), oar.HandGrip.position) ? a : b;
    }

    private static void Keys(params Key[] keys)
    { if (Keyboard.current != null) InputSystem.QueueStateEvent(Keyboard.current, new KeyboardState(keys)); }

    private static void Tick()
    {
        if (!EditorApplication.isPlaying) return;
        try
        {
            float elapsed = Time.time - started;
            if (phase > 0 || elapsed > 1)
            {
                var buoyancy = scenario.Boat.Body.GetComponent<OceanBuoyancy>();
                for (int i = 0; i < oars.Length; i++)
                {
                    var oar = oars[i];
                    pinError = Mathf.Max(pinError, Vector3.Distance(oar.RowlockPin.position, oar.transform.position));
                    var blade = oar.Model.TransformPoint(bladePoints[i]);
                    var axis = Vector3.Cross(oar.HandGrip.position - oar.transform.position, oar.transform.up).normalized;
                    var original = oar.transform.position + Quaternion.AngleAxis(-oar.BladeDipDegrees, axis) * (blade - oar.transform.position);
                    minimumLowering = Mathf.Min(minimumLowering, Vector3.Dot(original - blade, oar.transform.up));
                    if (phase == 1 && buoyancy.TryWaterHeight(blade, out var height, out var confidence) && confidence > .5f)
                    { minimumHeight[i] = Mathf.Min(minimumHeight[i], blade.y - height); maximumHeight[i] = Mathf.Max(maximumHeight[i], blade.y - height); }
                }
                foreach (var rower in scenario.Rowers)
                {
                    var seat = rower.Boarding.CurrentSeat;
                    var animator = rower.GetComponent<Animator>();
                    void Contact(HumanBodyBones bone, BoatOar oar)
                    {
                        var hand = animator.GetBoneTransform(bone);
                        handError = Mathf.Max(handError, Vector3.Distance(hand.position, oar.HandGrip.position));
                        wristError = Mathf.Max(wristError, Quaternion.Angle(hand.rotation, oar.HandGrip.rotation));
                    }
                    Contact(HumanBodyBones.LeftHand, seat.LeftHandOar);
                    Contact(HumanBodyBones.RightHand, seat.RightHandOar);
                }
            }
            if (phase == 0 && elapsed > 3) { Keys(Key.W); phase = 1; started = Time.time; }
            else if (phase == 1 && elapsed > 5) { Keys(Key.A); phase = 2; started = Time.time; }
            else if (phase == 2 && elapsed > 3) { Keys(Key.D); phase = 3; started = Time.time; }
            else if (phase == 3 && elapsed > 3)
            {
                File.AppendAllText(Report, $"Maximum pin error {pinError * 1000:F2} mm, hand error {handError * 1000:F2} mm, wrist angle error {wristError:F2} degrees. Minimum extra blade lowering {minimumLowering:F3} m.\n");
                for (int i = 0; i < oars.Length; i++)
                    File.AppendAllText(Report, $"{oars[i].name}: blade tip relative to sampled water during forward strokes {minimumHeight[i]:F3} to {maximumHeight[i]:F3} m.\n");
                Require(pinError < .001f, "Shaft contacts remain fixed.");
                // Humanoid retargeting can leave a few degrees of wrist error
                // after the arm solve. Check a five-degree fit, including turns,
                // rather than comparing imported bone axes with IK palm axes.
                Require(handError < .03f && wristError < 5f, "Both rowers' wrists follow the grip poses within five degrees, without a rotation-axis flip.");
                Require(minimumLowering > .1f, "All four blades dip farther below the original animation pose.");
                Require(minimumHeight.All(h => h < -.02f), "Every blade enters the sampled water during forward rowing.");
                Finish("PASS: Deeper blade immersion, fixed rowlocks, hand position and wrist orientation through idle, rowing and both turns. Editing scene restored.");
            }
        }
        catch (Exception error) { Finish("FAIL: " + error); }
    }

    private static void Require(bool condition, string message) { if (!condition) throw new InvalidOperationException(message); }
    private static void Finish(string message)
    {
        EditorApplication.update -= Tick;
        Keys(); File.AppendAllText(Report, message + "\n");
        Debug.Log(File.ReadAllText(Report));
        EditorApplication.isPlaying = false;
    }
}
