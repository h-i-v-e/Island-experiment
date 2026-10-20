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
public static class ExplorerRowboatLaunchValidation
{
    const string Running = "ExplorerRowboatLaunchValidation.Running", PreviousStart = "ExplorerRowboatLaunchValidation.PreviousStart";
    const string Report = "/tmp/explorer-rowboat-launch-play.txt";
    static ShipRowboatLaunch[] launchers;
    static PlayerController player;
    static BoatBoarding boarding;
    static int side, phase, inputFrame;
    static float started, hipError, handError;
    static bool background;
    static InputSettings.BackgroundBehavior inputBackground;
    static GameObject blocker;

    static ExplorerRowboatLaunchValidation() => EditorApplication.playModeStateChanged += StateChanged;

    [MenuItem("Island/Boat Tests/Run Explorer Deck Rowboat Launch Play Check")]
    public static void Run()
    {
        if (EditorApplication.isPlayingOrWillChangePlaymode) throw new InvalidOperationException("Stop Play mode first.");
        Require(UnityEngine.Object.FindObjectsByType<ShipRowboatLaunch>(FindObjectsSortMode.None).Length == 2, "Open an Explorer scene with both launchers installed.");
        SessionState.SetString(PreviousStart, AssetDatabase.GetAssetPath(EditorSceneManager.playModeStartScene));
        EditorSceneManager.playModeStartScene = null;
        SessionState.SetBool(Running, true);
        File.WriteAllText(Report, "Testing the actual E/use action on both Explorer deck rowboats, anchor and clearance gates, crew and aft passenger docking.\n");
        EditorApplication.isPaused = false;
        EditorApplication.isPlaying = true;
    }

    static void StateChanged(PlayModeStateChange state)
    {
        if (!SessionState.GetBool(Running, false)) return;
        if (state == PlayModeStateChange.EnteredPlayMode)
        {
            try
            {
                background = Application.runInBackground;
                inputBackground = InputSystem.settings.backgroundBehavior;
                Application.runInBackground = true;
                InputSystem.settings.backgroundBehavior = InputSettings.BackgroundBehavior.IgnoreFocus;
                launchers = UnityEngine.Object.FindObjectsByType<ShipRowboatLaunch>(FindObjectsSortMode.None).OrderBy(l => l.PortSide ? 0 : 1).ToArray();
                player = UnityEngine.Object.FindObjectsByType<PlayerController>(FindObjectsSortMode.None).First(p => p.UsesPlayerInput);
                boarding = player.GetComponent<BoatBoarding>();
                Require(player.enabled && Keyboard.current != null, "Real player and keyboard actions are enabled.");
                foreach (var wheel in UnityEngine.Object.FindObjectsByType<ShipsWheel>(FindObjectsSortMode.None)) wheel.enabled = false;
                player.IKConstraint = null;
                foreach (var controller in UnityEngine.Object.FindObjectsByType<ShipController>(FindObjectsSortMode.None)) controller.ReadPlayerInput = false;
                side = phase = 0; started = Time.time;
                EditorApplication.update += Tick;
            }
            catch (Exception error) { Finish("FAIL: " + error); }
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

    static void PlaceOnDeck(ShipRowboatLaunch launch)
    {
        boarding.CancelBoarding();
        var capsule = player.GetComponent<CharacterController>();
        capsule.enabled = false;
        var local = launch.ShipFrame.InverseTransformPoint(launch.transform.position);
        local.x = Mathf.MoveTowards(local.x, 0, 1.3f);
        player.transform.SetPositionAndRotation(launch.ShipFrame.TransformPoint(local), Quaternion.Euler(0, launch.ShipFrame.eulerAngles.y, 0));
        player.ResetBoatSupport();
        capsule.enabled = true;
        Physics.SyncTransforms();
    }

    static void PressUse()
    {
        InputSystem.QueueStateEvent(Keyboard.current, new KeyboardState(Key.E));
        inputFrame = Time.frameCount;
    }
    static void ReleaseUse() => InputSystem.QueueStateEvent(Keyboard.current, new KeyboardState());

    static void Tick()
    {
        if (!EditorApplication.isPlaying) return;
        try
        {
            var launch = launchers[side];
            if (phase == 0)
            {
                Require(Time.time - started < 30, "Ocean and ship initialize in time.");
                var ocean = launch.Ship.GetComponent<OceanBuoyancy>().Ocean;
                if (Time.time - started < 3 || ocean == null || ocean.SurfaceTransform == null) return;
                launch.Ship.SailsRaised = true;
                PlaceOnDeck(launch);
                Require(ShipRowboatLaunch.Nearest(player) == launch, "Nearest deck hull selects the appropriate side.");
                PressUse(); phase = 1;
            }
            else if (phase == 1 && Time.frameCount > inputFrame + 2)
            {
                ReleaseUse();
                Require(launch.LaunchedBoat == null && boarding.State == BoatBoarding.BoardingState.Idle && launch.LastFailure != null,
                    "Use refuses launch while anchor is raised and leaves player on deck.");
                launch.Ship.SailsRaised = false;
                Require(launch.AnchorDeployed, "Furling sails drops the real ship anchor.");
                PlaceOnDeck(launch);
                var pose = launch.WaterPose(launch.Ship.GetComponent<OceanBuoyancy>().Ocean);
                blocker = GameObject.CreatePrimitive(PrimitiveType.Cube);
                blocker.name = "Temporary launch obstruction";
                blocker.transform.position = pose.position;
                blocker.transform.localScale = new Vector3(3, 3, 6);
                blocker.transform.rotation = pose.rotation;
                Physics.SyncTransforms();
                Require(!launch.TryLaunch(player) && launch.LaunchedBoat == null, "Blocked water rejects launch without spawning crew or moving player.");
                blocker.SetActive(false); UnityEngine.Object.Destroy(blocker);
                phase = 2; inputFrame = Time.frameCount;
            }
            else if (phase == 2 && Time.frameCount > inputFrame + 2)
            {
                PlaceOnDeck(launch);
                PressUse(); phase = 3; started = Time.time;
            }
            else if (phase == 3 && Time.frameCount > inputFrame + 2)
            {
                ReleaseUse();
                Require(launch.LaunchedBoat != null, "Use launches a boat: " + launch.LastFailure + " Blocker: " + launch.LaunchObstruction?.name + " " + launch.LaunchObstruction?.GetType().Name);
                Require(boarding.State == BoatBoarding.BoardingState.Seated && boarding.CurrentSeat.name == "Passenger aft",
                    "Player is immediately in the aft passenger seat; the same use did not trigger exit.");
                Require(!launch.gameObject.activeSelf, "Launched deck hull is hidden.");
                Require(Camera.main == player.GetComponentInChildren<Camera>(true), "Main view follows the teleported player.");
                var crew = launch.LaunchedBoat.GetComponent<RowboatCrew>();
                var boatPhysical = crew.Hull.GetComponent<Collider>();
                var shipPhysical = launch.Ship.GetComponent<Collider>();
                Require(!Physics.GetIgnoreCollision(boatPhysical, shipPhysical), "The physical ship and rowboat hulls still collide.");
                foreach (var walking in launch.ShipFrame.GetComponentsInChildren<Collider>().Where(c => c is MeshCollider && c.attachedRigidbody == null))
                    Require(Physics.GetIgnoreCollision(boatPhysical, walking), "Duplicate ship walking hull does not collide with the rowboat physics body.");
                Require(crew.Rowers.Length == 2 && crew.Rowers.All(n => !n.GetComponent<PlayerController>().UsesPlayerInput
                    && n.Boarding.State == BoatBoarding.BoardingState.Seated && n.Boarding.CurrentSeat.Role == BoatSeat.SeatRole.Rower), "Both NPC rowers are seated.");
                var right = Vector3.Cross(Vector3.up, Vector3.ProjectOnPlane(launch.ShipFrame.forward, Vector3.up).normalized);
                float lateral = Vector3.Dot(launch.LaunchedBoat.transform.position - launch.ShipFrame.position, right);
                Require(launch.PortSide ? lateral < -3 : lateral > 3, $"Boat is on matching side: port={launch.PortSide}, lateral={lateral:F3}.");
                var horizontal = Vector3.ProjectOnPlane(launch.LaunchedBoat.transform.position - launch.LastLaunchPose.position, Vector3.up);
                Require(horizontal.magnitude < .4f, "Boat spawns immediately beside its ship-relative launch point.");
                Require(!launch.TryLaunch(player), "Repeated use cannot duplicate this boat.");
                hipError = handError = 0;
                phase = 4; started = Time.time;
            }
            else if (phase == 4)
            {
                Require(boarding.State == BoatBoarding.BoardingState.Seated, "Player remains seated after launch.");
                hipError = Mathf.Max(hipError, Vector3.Distance(player.GetComponent<Animator>().GetBoneTransform(HumanBodyBones.Hips).position, boarding.CurrentSeat.transform.position));
                foreach (var rower in launch.LaunchedBoat.GetComponent<RowboatCrew>().Rowers)
                {
                    var animator = rower.GetComponent<Animator>();
                    var seat = rower.Boarding.CurrentSeat;
                    handError = Mathf.Max(handError, Vector3.Distance(animator.GetBoneTransform(HumanBodyBones.LeftHand).position, seat.LeftHandOar.HandGrip.position),
                        Vector3.Distance(animator.GetBoneTransform(HumanBodyBones.RightHand).position, seat.RightHandOar.HandGrip.position));
                }
                if (Time.time - started > 3)
                {
                    Require(hipError < .02f && handError < .03f, "Player and crew stay docked while the boat floats on the real ocean.");
                    File.AppendAllText(Report, $"PASS: {(launch.PortSide ? "port" : "starboard")} actual use; anchor-up and obstruction rejected, two rowers, aft passenger, correct side, no duplicate/instant exit. Floating hips {hipError * 1000:F1} mm, grips {handError * 1000:F1} mm.\n");
                    if (++side < launchers.Length) { phase = 0; started = Time.time - 3; PlaceOnDeck(launchers[side]); }
                    else Finish("PASS: Both Explorer deck rowboat launches completed; editing scene restored after Play.");
                }
            }
        }
        catch (Exception error) { Finish("FAIL: " + error); }
    }

    static void Require(bool condition, string message) { if (!condition) throw new InvalidOperationException(message); }
    static void Finish(string message)
    {
        EditorApplication.update -= Tick;
        ReleaseUse();
        if (blocker != null) UnityEngine.Object.Destroy(blocker);
        File.AppendAllText(Report, message + "\n");
        Debug.Log(File.ReadAllText(Report));
        EditorApplication.isPlaying = false;
    }
}
