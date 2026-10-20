using System;
using System.IO;
using System.Linq;
using UnityEditor;
using UnityEditor.SceneManagement;
using UnityEngine;
using UnityEngine.InputSystem;
using UnityEngine.InputSystem.LowLevel;

[InitializeOnLoad]
public static class RowboatControlValidation
{
    const string Running = "RowboatControlValidation.Running", PreviousStart = "RowboatControlValidation.PreviousStart";
    const string Report = "/tmp/rowboat-controls-play.txt";
    static GameObject boat;
    static PlayerController player;
    static BoatBoarding boarding;
    static RowboatController controls;
    static NpcController[] crew;
    static BoatOar[] oars;
    static Rigidbody body;
    static int phase;
    static float started, maximumPinError, maximumHandError, heading, startHeight;
    static Vector3 startPosition;
    static bool background;
    static InputSettings.BackgroundBehavior inputBackground;

    static RowboatControlValidation() => EditorApplication.playModeStateChanged += StateChanged;

    [MenuItem("Island/Boat Tests/Run Rowboat Controls And Exit Gravity Play Check")]
    public static void Run()
    {
        if (EditorApplication.isPlayingOrWillChangePlaymode) throw new InvalidOperationException("Stop Play mode first.");
        var prefab = AssetDatabase.LoadAssetAtPath<GameObject>(BoatTestSceneSetup.BoatPrefab);
        Require(prefab.GetComponent<RowboatController>() != null && prefab.GetComponentsInChildren<BoatOar>(true).All(o => o.RowlockPin != null), "Run rowing control setup first.");
        SessionState.SetString(PreviousStart, AssetDatabase.GetAssetPath(EditorSceneManager.playModeStartScene));
        EditorSceneManager.playModeStartScene = AssetDatabase.LoadAssetAtPath<SceneAsset>(BoatTestSceneSetup.EmptyScene);
        SessionState.SetBool(Running, true);
        File.WriteAllText(Report, "Testing actual W/A/D passenger commands, NPC rowing, free floating propulsion/steering, fixed shaft pins, idle and unsupported exit gravity on the real avatar.\n");
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
                var inspection = UnityEngine.Object.FindFirstObjectByType<BoatTestPlayer>();
                inspection.SetFlying(false); inspection.enabled = false;
                player = UnityEngine.Object.FindFirstObjectByType<PlayerController>();
                boarding = player.GetComponent<BoatBoarding>();
                Require(Keyboard.current != null && player.enabled, "Real player keyboard control is active.");
                Keys();
                var prefab = AssetDatabase.LoadAssetAtPath<GameObject>(BoatTestSceneSetup.BoatPrefab);
                boat = UnityEngine.Object.Instantiate(prefab, prefab.transform.position, prefab.transform.rotation);
                controls = boat.GetComponent<RowboatController>();
                body = boat.GetComponent<FloatingShip>().Hull;
                var rowSeats = boat.GetComponentsInChildren<BoatSeat>().Where(s => s.Role == BoatSeat.SeatRole.Rower).ToArray();
                crew = new NpcController[2];
                boat.AddComponent<RowboatCrew>().Configure(body, crew);
                for (int i = 0; i < 2; i++)
                {
                    crew[i] = UnityEngine.Object.Instantiate(AssetDatabase.LoadAssetAtPath<GameObject>(NpcPrefabSetup.PrefabPath), rowSeats[i].transform.position, Quaternion.identity).GetComponent<NpcController>();
                    Require(crew[i].Boarding.TrySeatImmediately(rowSeats[i], true), "NPC takes its rower seat.");
                }
                Require(boarding.TrySeatImmediately(controls.CommandSeat, true), "Player takes the aft command/passenger seat.");
                PlayerView.Activate(player);
                oars = boat.GetComponentsInChildren<BoatOar>();
                maximumPinError = maximumHandError = 0;
                phase = 0; started = Time.time;
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

    static void Keys(params Key[] pressed)
    { if (Keyboard.current != null) InputSystem.QueueStateEvent(Keyboard.current, new KeyboardState(pressed)); }

    static void Tick()
    {
        if (!EditorApplication.isPlaying) return;
        try
        {
            float elapsed = Time.time - started;
            // Skip the initial activation frame, before all animators have run.
            if (elapsed > .1f)
            {
                foreach (var oar in oars) maximumPinError = Mathf.Max(maximumPinError, Vector3.Distance(oar.RowlockPin.position, oar.transform.position));
                foreach (var npc in crew)
                {
                    var seat = npc.Boarding.CurrentSeat;
                    var animator = npc.GetComponent<Animator>();
                    maximumHandError = Mathf.Max(maximumHandError, Vector3.Distance(animator.GetBoneTransform(HumanBodyBones.LeftHand).position, seat.LeftHandOar.HandGrip.position),
                        Vector3.Distance(animator.GetBoneTransform(HumanBodyBones.RightHand).position, seat.RightHandOar.HandGrip.position));
                }
            }
            if (phase == 0 && elapsed > 3)
            {
                Require(!body.isKinematic && controls.ActiveRowers == 2, "Two rowers drive a free floating hull.");
                // Move a rowlock target without moving the shaft pin. This must
                // keep the same physical shaft point attached, including idle.
                oars[0].transform.localPosition += new Vector3(.04f, .03f, 0);
                startPosition = body.position;
                Keys(Key.W); phase = 1; started = Time.time;
            }
            else if (phase == 1 && elapsed > 6)
            {
                float speed = Vector3.Dot(body.linearVelocity, Vector3.ProjectOnPlane(boat.transform.forward, Vector3.up).normalized);
                float distance = Vector3.Dot(body.position - startPosition, Vector3.ProjectOnPlane(boat.transform.forward, Vector3.up).normalized);
                Require(speed > .65f && distance > 2, $"Forward rowing accelerates and moves the boat: speed {speed:F2}, distance {distance:F2}.");
                Require(controls.Input.y > .95f && crew.All(n => n.GetComponent<Animator>().GetFloat("YAxis") > .9f), "Passenger W drives both rower animations.");
                File.AppendAllText(Report, $"PASS: W moves free floating boat {distance:F2} m, speed {speed:F2} m/s; both NPCs row forward.\n");
                heading = boat.transform.eulerAngles.y;
                Keys(Key.D); phase = 2; started = Time.time;
            }
            else if (phase == 2 && elapsed > 3)
            {
                float turn = Mathf.DeltaAngle(heading, boat.transform.eulerAngles.y);
                Require(turn > 8 && crew.All(n => n.GetComponent<Animator>().GetFloat("XAxis") > .9f), $"D turns right with right-rowing animation: {turn:F1} deg.");
                File.AppendAllText(Report, $"PASS: D turns right {turn:F1} deg with matching rowing blend.\n");
                heading = boat.transform.eulerAngles.y;
                Keys(Key.A); phase = 3; started = Time.time;
            }
            else if (phase == 3 && elapsed > 3)
            {
                float turn = Mathf.DeltaAngle(heading, boat.transform.eulerAngles.y);
                Require(turn < -8 && crew.All(n => n.GetComponent<Animator>().GetFloat("XAxis") < -.9f), $"A turns left: {turn:F1} deg, input={controls.Input}, yaw rate={body.angularVelocity.y * Mathf.Rad2Deg:F1}, row axes={string.Join(",", crew.Select(n => n.GetComponent<Animator>().GetFloat("XAxis").ToString("F2")))}, rotation={boat.transform.eulerAngles:F1}.");
                File.AppendAllText(Report, $"PASS: A turns left {turn:F1} deg with matching rowing blend.\n");
                Keys(); phase = 4; started = Time.time;
            }
            else if (phase == 4 && elapsed > 2)
            {
                Require(controls.Input.sqrMagnitude < .001f && crew.All(n => Mathf.Abs(n.GetComponent<Animator>().GetFloat("XAxis")) < .02f
                    && n.GetComponent<Animator>().GetFloat("YAxis") < .02f), "Releasing controls returns crew to idle.");
                Require(maximumPinError < .001f && maximumHandError < .03f, $"Fixed pins/contact throughout strokes: pins {maximumPinError:F5}, hands {maximumHandError:F5}.");
                File.AppendAllText(Report, $"PASS: Release idles crew; maximum shaft pin error {maximumPinError * 1000:F2} mm and hand error {maximumHandError * 1000:F2} mm during idle, forward and both turns.\n");
                // Raise and hold the boat like a launch platform, with a real
                // floor underneath. The unchanged exit animation must release
                // the character to gravity instead of leaving it in mid-air.
                body.constraints = RigidbodyConstraints.FreezeAll;
                body.linearVelocity = body.angularVelocity = Vector3.zero;
                body.position += Vector3.up * 3;
                body.transform.position = body.position;
                var floor = GameObject.CreatePrimitive(PrimitiveType.Cube);
                floor.name = "Temporary exit landing floor";
                floor.transform.position = new Vector3(body.position.x, -.1f, body.position.z);
                floor.transform.localScale = new Vector3(30, .2f, 30);
                Physics.SyncTransforms();
                phase = 5; started = Time.time;
            }
            else if (phase == 5 && elapsed > .2f)
            {
                Require(boarding.RequestExit(true), "Player requests exit from the raised boat.");
                phase = 6; started = Time.time;
            }
            else if (phase == 6)
            {
                Require(elapsed < 9, "Exit animation releases the character in time.");
                if (boarding.State == BoatBoarding.BoardingState.Idle)
                {
                    startHeight = player.transform.position.y;
                    Require(startHeight > 1, "Exit begins a measurable unsupported fall above the floor.");
                    phase = 7; started = Time.time;
                }
            }
            else if (phase == 7 && elapsed > 2)
            {
                Require(player.GetComponent<CharacterController>().isGrounded && Mathf.Abs(player.transform.position.y) < .15f,
                    $"Exit falls to the actual floor: start {startHeight:F3}, now {player.transform.position.y:F3}.");
                Require(Vector3.Dot(player.transform.up, Vector3.up) > .999f, "Character stands upright after leaving a tilted boat.");
                Require(controls.Input == Vector2.zero && boarding.SeatIKBlend == 0, "Leaving the command seat stops crew orders and releases seat IK.");
                File.AppendAllText(Report, $"PASS: Exit releases gravity; fell from {startHeight:F2} m to floor at {player.transform.position.y:F3} m and stands upright.\n");
                startPosition = player.transform.position;
                Keys(Key.W); phase = 8; started = Time.time;
            }
            else if (phase == 8 && elapsed > 1.5f)
            {
                Require(Vector3.ProjectOnPlane(player.transform.position - startPosition, Vector3.up).magnitude > .3f, "Normal player walking works after landing.");
                Finish("PASS: Passenger control, free floating propulsion, steering, rowing blends, rowlock pins, exit gravity and post-exit walking passed; editing scene restored after Play.");
            }
        }
        catch (Exception error) { Finish("FAIL: " + error); }
    }

    static void Require(bool condition, string message) { if (!condition) throw new InvalidOperationException(message); }
    static void Finish(string message)
    {
        EditorApplication.update -= Tick;
        Keys();
        File.AppendAllText(Report, message + "\n");
        Debug.Log(File.ReadAllText(Report));
        EditorApplication.isPlaying = false;
    }
}
