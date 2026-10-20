using System;
using System.IO;
using Motu.Gameplay;
using Motu.World;
using UnityEditor;
using UnityEditor.SceneManagement;
using UnityEngine;
using UnityEngine.InputSystem;
using UnityEngine.InputSystem.LowLevel;
using Object = UnityEngine.Object;

[InitializeOnLoad]
public static class BoatPushTestSceneValidation
{
    const string Running = "BoatPushTestSceneValidation";
    public const string Report = "/tmp/boat-push-level-play.txt";
    static BoatPushTestScenario scenario;
    static BoatPushing actor;
    static Keyboard keyboard;
    static float began;
    static Vector3 boatStart;
    static int phase, pushes;
    static bool sawAnimation;
    static Camera overview;
    static RenderTexture overviewTexture;
    static double deadline;
    static BoatPushTestSceneValidation() => EditorApplication.playModeStateChanged += Changed;
    public static void RunBatch()
    {
        if (!Application.isBatchMode || !File.Exists(".boat-exit-isolated-project")) throw new InvalidOperationException("Isolated project only.");
        BoatPushTestSceneSetup.CreateBatch();
        EditorSceneManager.OpenScene(BoatPushTestSceneSetup.ScenePath);
        BoatPushSetup.Apply();
        InputSystem.actions = AssetDatabase.LoadAssetAtPath<InputActionAsset>("Assets/InputSystem_Actions.inputactions");
        File.WriteAllText(Report, "Saved calm-sea sloping-beach level, real E/R input, native collision, actual buoyancy and push animation.\n");
        SessionState.SetBool(Running, true); EditorApplication.isPlaying = true;
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
            keyboard = InputSystem.AddDevice<Keyboard>("PushBeachTestKeyboard");
            scenario = Object.FindAnyObjectByType<BoatPushTestScenario>();
            actor = scenario.Player.GetComponent<BoatPushing>();
            began = Time.time; phase = pushes = 0; deadline = EditorApplication.timeSinceStartup + 180;
            EditorApplication.update += Tick;
        }
        if (change == PlayModeStateChange.EnteredEditMode)
        {
            EditorApplication.update -= Tick; SessionState.SetBool(Running, false);
            EditorApplication.Exit(SessionState.GetInt(Running + ".result", 1));
        }
    }
    static void Check(bool condition, string message)
    {
        if (!condition) throw new InvalidOperationException(message);
        File.AppendAllText(Report, "PASS: " + message + "\n");
    }
    static void Key(params Key[] keys) => InputSystem.QueueStateEvent(keyboard, new KeyboardState(keys));
    static void Tick()
    {
        EditorApplication.QueuePlayerLoopUpdate(); InputSystem.Update();
        try
        {
            if (EditorApplication.timeSinceStartup > deadline) throw new TimeoutException();
            float elapsed = Time.time - began;
            var boat = scenario.Boat; var body = boat.Body;
            if (phase == 0 && elapsed > 5)
            {
                var wind = Shader.GetGlobalVector("_MotuWeatherWind");
                Check(wind.z < .001f && body.GetComponent<OceanBuoyancy>().WaterCurrent == Vector3.zero, "Sea has no wind or current");
                Check(boat.IsGrounded && !body.isKinematic && scenario.Player.GetComponent<CharacterController>().isGrounded,
                    $"Boat rests on the sloping collider and player stands ashore (boat={body.position:F3}, player={scenario.Player.transform.position:F3})");
                Check(scenario.Player.GetComponent<BoatBoarding>().State == BoatBoarding.BoardingState.Idle, "Player starts outside the boat");
                boatStart = body.position; Key(UnityEngine.InputSystem.Key.W); began = Time.time; phase = 10;
            }
            else if (phase == 10)
            {
                if (boat.GetComponent<BoatPushable>().IsInFrontOf(scenario.Player.transform.position, scenario.Player.transform.forward, 1.35f, out float gap) && gap < .45f)
                { Key(); began = Time.time; phase = 11; }
                else if (elapsed > 5) throw new Exception("Could not walk within palm reach.");
            }
            else if (phase == 11 && elapsed > .15f)
            { Key(UnityEngine.InputSystem.Key.E); began = Time.time; phase = 1; }
            else if (phase == 1 && elapsed > .12f)
            {
                Key(); Check(actor.IsActive, $"E starts pushing from the player position without a standing marker (state={actor.State}, boarding={scenario.Player.GetComponent<BoatBoarding>().State}, position={scenario.Player.transform.position:F3}, feet={scenario.Player.GetComponent<CharacterController>().isGrounded}, supported={scenario.Player.IsSupportedBy(boat.transform)}, available={boat.GetComponent<BoatPushable>().IsAvailable})");
                pushes++; sawAnimation = false; began = Time.time; phase = 2;
            }
            else if (phase == 2)
            {
                sawAnimation |= actor.IsAnimating;
                if (!actor.IsActive)
                {
                    float travel = body.position.z - boatStart.z;
                    File.AppendAllText(Report, $"Finished: animation={sawAnimation}, forceSteps={actor.ForceSteps}, left={actor.LeftForceSteps}, right={actor.RightForceSteps}, player={scenario.Player.transform.position:F3}.\n");
                    Check(sawAnimation && actor.ForceSteps > 0 && travel > .1f,
                        $"Push {pushes} plays animation and moves boat seawards ({travel:F3} m, grounded={boat.IsGrounded})");
                    began = Time.time; phase = 3;
                }
                else if (elapsed > 20) throw new Exception("Push did not finish: " + actor.State);
            }
            else if (phase == 3 && elapsed > 1)
            {
                if (boat.IsGrounded && pushes < 8)
                { Key(UnityEngine.InputSystem.Key.W); began = Time.time; phase = 10; }
                else
                {
                    Check(!boat.IsGrounded && body.GetComponent<OceanBuoyancy>().SubmergedFraction > .05f, "Boat clears the beach and floats in the calm sea");
                    var boarding = scenario.Player.GetComponent<BoatBoarding>();
                    Check(boarding.State == BoatBoarding.BoardingState.Seated && boarding.CurrentSeat == boat.CommandSeat
                        && boat.CommandSeat.Occupant == boarding && !actor.IsActive && !actor.IsAwaitingLaunch
                        && scenario.Player.IKConstraint == null && !scenario.Player.GetComponent<CharacterController>().enabled,
                        "Successful push launch automatically seats the player at the controls and releases pushing");
                    Key(UnityEngine.InputSystem.Key.W); began = Time.time; phase = 20;
                }
            }
            else if (phase == 20 && elapsed > .75f)
            {
                var boarding = scenario.Player.GetComponent<BoatBoarding>();
                var seated = boat.CommandSeat.SeatedRoot(scenario.Player.GetComponent<Animator>().humanScale);
                Check(boarding.State == BoatBoarding.BoardingState.Seated && boat.Input.y > .5f
                    && Vector3.Distance(scenario.Player.transform.position, seated.position) < .1f,
                    "Player stays attached to the moving control seat and W reaches the boat controls");
                Key(UnityEngine.InputSystem.Key.R); began = Time.time; phase = 4;
            }
            else if (phase == 4 && elapsed > .15f) { Key(); began = Time.time; phase = 5; }
            else if (phase == 5 && elapsed > 2)
            {
                Check(boat.IsGrounded && Vector3.Distance(body.position, boatStart) < .6f
                    && scenario.Player.GetComponent<BoatBoarding>().State == BoatBoarding.BoardingState.Idle,
                    "R restores the grounded boat and shore player for another test");
                overview = new GameObject("Test level overview").AddComponent<Camera>();
                overview.CopyFrom(scenario.Player.GetComponentInChildren<Camera>());
                overviewTexture = new RenderTexture(1024, 768, 24); overview.targetTexture = overviewTexture;
                overview.transform.position = new Vector3(-6, 3.8f, -9);
                overview.transform.LookAt(body.position + Vector3.up * .6f); overview.enabled = true;
                began = Time.time; phase = 6;
            }
            else if (phase == 6 && elapsed > .25f)
            {
                var previous = RenderTexture.active; RenderTexture.active = overviewTexture;
                var image = new Texture2D(1024, 768, TextureFormat.RGB24, false);
                image.ReadPixels(new Rect(0, 0, 1024, 768), 0, 0); image.Apply();
                File.WriteAllBytes("/tmp/boat-push-level-overview.png", image.EncodeToPNG()); RenderTexture.active = previous;
                Object.Destroy(image); Object.Destroy(overview.gameObject); Object.Destroy(overviewTexture); Finish(0);
            }
        }
        catch (Exception error) { File.AppendAllText(Report, "FAIL: " + error + "\n"); Debug.LogException(error); Finish(1); }
    }
    static void Finish(int result)
    { EditorApplication.update -= Tick; SessionState.SetInt(Running + ".result", result); EditorApplication.isPlaying = false; }
}
