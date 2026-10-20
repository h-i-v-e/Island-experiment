using System;
using System.IO;
using System.Reflection;
using UnityEditor;
using UnityEditor.SceneManagement;
using UnityEngine;
using UnityEngine.InputSystem;

// Run only in an isolated copy; keeps the user's open Unity project untouched.
[InitializeOnLoad]
public static class BoatExitGravityValidation
{
    const string Running = "BoatExitGravityValidation.Running";
    const string Report = "/tmp/boat-exit-gravity-play.txt";
    static PlayerController player;
    static BoatBoarding boarding;
    static CharacterController capsule;
    static Animator animator;
    static BoatSeat seat;
    static int phase;
    static float started, releasedHeight;
    static double wallStarted;

    static BoatExitGravityValidation() => EditorApplication.playModeStateChanged += Changed;

    public static void RunBatch()
    {
        if (!Application.isBatchMode || !File.Exists(".boat-exit-isolated-project"))
            throw new InvalidOperationException("Run in the isolated boat exit validation project only.");
        File.WriteAllText(Report, "Real player avatar/controller, actual boat seats and exit animations; no ocean collider or terrain below the first exit.\n");
        EditorSceneManager.NewScene(NewSceneSetup.EmptyScene, NewSceneMode.Single);
        InputSystem.actions = AssetDatabase.LoadAssetAtPath<InputActionAsset>("Assets/InputSystem_Actions.inputactions");
        SessionState.SetBool(Running, true);
        EditorApplication.isPlaying = true;
    }

    static void Changed(PlayModeStateChange change)
    {
        if (!SessionState.GetBool(Running, false)) return;
        if (change == PlayModeStateChange.EnteredPlayMode)
        {
            try
            {
                Application.runInBackground = true;
                InputSystem.settings.backgroundBehavior = InputSettings.BackgroundBehavior.IgnoreFocus;
                var boat = UnityEngine.Object.Instantiate(AssetDatabase.LoadAssetAtPath<GameObject>("Assets/BoatTesting/RowingBoatTest.prefab"), new Vector3(0, 3, 0), Quaternion.identity);
                seat = boat.GetComponent<RowboatController>().CommandSeat;
                player = UnityEngine.Object.Instantiate(AssetDatabase.LoadAssetAtPath<GameObject>("Assets/Prefabs/Player.prefab"), new Vector3(0, 8, 0), Quaternion.identity).GetComponent<PlayerController>();
                animator = player.GetComponent<Animator>();
                capsule = player.GetComponent<CharacterController>();
                boarding = player.GetComponent<BoatBoarding>();
                // Exercise the immediate-seat path with a capsule temporarily
                // disabled for placement; an actual exit must restore walking.
                capsule.enabled = false;
                Require(boarding.TrySeatImmediately(seat, true), "Player takes the aft seat.");
                phase = 0; started = Time.time; wallStarted = EditorApplication.timeSinceStartup;
                EditorApplication.update += Tick;
            }
            catch (Exception error) { Finish("FAIL: " + error, 1); }
        }
        if (change == PlayModeStateChange.EnteredEditMode)
        {
            SessionState.SetBool(Running, false);
            EditorApplication.Exit(SessionState.GetInt("BoatExitGravityValidation.Result", 1));
        }
    }

    static float FootWeight(string field) => (float)typeof(PlayerController).GetField(field, BindingFlags.Instance | BindingFlags.NonPublic).GetValue(player);

    static void Tick()
    {
        if (!EditorApplication.isPlaying) return;
        EditorApplication.QueuePlayerLoopUpdate();
        try
        {
            Require(EditorApplication.timeSinceStartup - wallStarted < 60, "Physics check completes within one minute.");
            float elapsed = Time.time - started;
            if (phase == 0 && elapsed > .5f)
            {
                Require(boarding.RequestExit(true), "Left exit starts.");
                phase = 1; started = Time.time;
            }
            else if (phase == 1 || phase == 4)
            {
                Require(elapsed < 12, "Exit animation releases the character.");
                if (boarding.State == BoatBoarding.BoardingState.Idle)
                {
                    Require(capsule.enabled, "Completed exit enables the walking capsule.");
                    Require(boarding.SeatIKBlend == 0 && seat.Occupant == null, "Exit releases seat IK and reservation.");
                    releasedHeight = player.transform.position.y;
                    File.AppendAllText(Report, $"Release: position={player.transform.position:F3}, grounded={capsule.isGrounded}, capsule={capsule.enabled}, animation={animator.GetCurrentAnimatorStateInfo(0).fullPathHash}.\n");
                    phase++; started = Time.time;
                }
            }
            else if (phase == 2 && elapsed > 2.2f)
            {
                Require(!capsule.isGrounded && player.transform.position.y < 0 && player.transform.position.y < releasedHeight - 2,
                    $"Unsupported exit falls below sea level: release {releasedHeight:F3}, now {player.transform.position.y:F3}, grounded={capsule.isGrounded}, speed={FootWeight("verticalSpeed"):F3}, elapsed={elapsed:F3}.");
                File.AppendAllText(Report, $"PASS: Left exit re-enables a previously disabled capsule, releases IK and falls from {releasedHeight:F3} to {player.transform.position.y:F3} m without support.\n");
                var floor = GameObject.CreatePrimitive(PrimitiveType.Cube);
                floor.name = "Actual walkable landing surface";
                floor.transform.position = new Vector3(0, -.1f, 0);
                floor.transform.localScale = new Vector3(30, .2f, 30);
                Physics.SyncTransforms();
                Require(boarding.TrySeatImmediately(seat, false), "Player returns to seat for right exit.");
                phase = 3; started = Time.time;
            }
            else if (phase == 3 && elapsed > .5f)
            { Require(boarding.RequestExit(false), "Right exit starts."); phase = 4; started = Time.time; }
            else if (phase == 5 && elapsed > 3)
            {
                float left = FootWeight("leftFootWeight"), right = FootWeight("rightFootWeight");
                Require(capsule.isGrounded && Mathf.Abs(player.transform.position.y) < .15f, $"Right exit lands on floor at {player.transform.position.y:F3} m.");
                Require(left > .5f && right > .5f, $"Foot IK resumes on landing: {left:F3}, {right:F3}.");
                File.AppendAllText(Report, $"PASS: Right exit falls from {releasedHeight:F3} m to floor at {player.transform.position.y:F3} m. Foot IK weights L={left:F3}, R={right:F3}.\n");
                animator.enabled = false;
                capsule.enabled = false;
                player.transform.position += Vector3.up * 3;
                capsule.enabled = true;
                player.ResumeFromBoat(Vector3.zero);
                releasedHeight = player.transform.position.y;
                phase = 6; started = Time.time;
            }
            else if (phase == 6 && elapsed > 1.3f)
            {
                Require(player.transform.position.y < releasedHeight - 1, "Gravity runs even without animation callbacks.");
                File.AppendAllText(Report, $"PASS: With Animator disabled, gravity still moves the character from {releasedHeight:F3} to {player.transform.position.y:F3} m.\n");
                animator.enabled = true;
                phase = 7; started = Time.time;
            }
            else if (phase == 7 && elapsed > 1)
            {
                Require(capsule.isGrounded && FootWeight("leftFootWeight") > .5f && FootWeight("rightFootWeight") > .5f, "Walking contact and foot IK recover after animation resumes.");
                Finish("PASS: Unsupported sea-side exit, both animation sides, capsule restoration, landing, foot IK and gravity independent of animation passed.", 0);
            }
        }
        catch (Exception error) { Finish("FAIL: " + error, 1); }
    }

    static void Require(bool condition, string message) { if (!condition) throw new InvalidOperationException(message); }
    static void Finish(string message, int result)
    {
        EditorApplication.update -= Tick;
        File.AppendAllText(Report, message + "\n"); Debug.Log(File.ReadAllText(Report));
        SessionState.SetInt("BoatExitGravityValidation.Result", result);
        EditorApplication.isPlaying = false;
    }
}
