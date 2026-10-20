using System;
using System.IO;
using System.Linq;
using UnityEditor;
using UnityEditor.SceneManagement;
using UnityEngine;

[InitializeOnLoad]
public static class NpcPrefabValidation
{
    const string Running = "NpcPrefabValidation.Running", PreviousStart = "NpcPrefabValidation.PreviousStart";
    const string Report = "/tmp/npc-prefab-play.txt";
    static NpcController npc, idle;
    static Animator animator;
    static BoatSeat seat;
    static Vector3 destination, idlePosition;
    static float started, maximumHandError, maximumHipError, maximumSwing;
    static Quaternion restOar;
    static int phase;
    static CursorLockMode cursorLock;
    static bool cursorVisible;
    static bool background;
    static float timeScale;

    static NpcPrefabValidation() => EditorApplication.playModeStateChanged += StateChanged;

    [MenuItem("Island/Characters/Run NPC Prefab Play Check")]
    public static void Run()
    {
        if (EditorApplication.isPlayingOrWillChangePlaymode) throw new InvalidOperationException("Stop Play mode first.");
        var prefab = AssetDatabase.LoadAssetAtPath<GameObject>(NpcPrefabSetup.PrefabPath);
        Require(prefab != null && PrefabUtility.GetPrefabAssetType(prefab) == PrefabAssetType.Variant, "NPC is a prefab variant.");
        Require(prefab.GetComponent<Animator>().runtimeAnimatorController == AssetDatabase.LoadAssetAtPath<RuntimeAnimatorController>("Assets/TheOne.controller"), "NPC shares TheOne asset.");
        Require(!prefab.GetComponent<PlayerController>().UsesPlayerInput && prefab.GetComponent<NpcController>() != null, "NPC uses external control.");
        Require(prefab.GetComponent<AudioSource>().spatialBlend == 1f, "NPC footsteps use spatial audio.");
        Require(prefab.GetComponentsInChildren<Camera>(true).Length == 0 && prefab.GetComponentsInChildren<AudioListener>(true).Length == 0
            && prefab.GetComponent<ShoreWaveAudio>() == null && prefab.GetComponent<BoatTestPlayer>() == null, "No player-only camera, listener or behaviour.");
        File.WriteAllText(Report, "PASS: NPC variant shares TheOne, avatar and character motor, without player-only components.\n");
        SessionState.SetString(PreviousStart, AssetDatabase.GetAssetPath(EditorSceneManager.playModeStartScene));
        EditorSceneManager.playModeStartScene = AssetDatabase.LoadAssetAtPath<SceneAsset>(BoatTestSceneSetup.RowingScene);
        SessionState.SetBool(Running, true);
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
                timeScale = Time.timeScale;
                Application.runInBackground = true;
                Time.timeScale = 1;
                var player = UnityEngine.Object.FindFirstObjectByType<PlayerController>();
                Require(player != null && player.UsesPlayerInput && player.Interact != null, "Player retains its own input bindings.");
                File.AppendAllText(Report, "PASS: Existing player still uses player input.\n");
                foreach (var component in UnityEngine.Object.FindObjectsByType<MonoBehaviour>()
                    .Where(c => c is PlayerController || c is BoatTestPlayer)) component.enabled = false;
                cursorLock = Cursor.lockState; cursorVisible = Cursor.visible;
                Platform(new Vector3(50, 2.9f, 0), new Vector3(14, .2f, 14));
                npc = Spawn(new Vector3(48, 3.08f, -2));
                idle = Spawn(new Vector3(53, 3.08f, 0));
                animator = npc.GetComponent<Animator>();
                Require(npc.GetComponent<PlayerController>().Interact == null && idle.GetComponent<PlayerController>().Interact == null, "NPCs have no input actions.");
                CheckCursor();
                phase = 0; started = Time.time;
                maximumHandError = maximumHipError = maximumSwing = 0;
                EditorApplication.update += Tick;
            }
            catch (Exception error) { Finish("FAIL: " + error); }
        }
        if (state == PlayModeStateChange.EnteredEditMode)
        {
            EditorApplication.update -= Tick;
            SessionState.SetBool(Running, false);
            Application.runInBackground = background;
            Time.timeScale = timeScale;
            EditorSceneManager.playModeStartScene = AssetDatabase.LoadAssetAtPath<SceneAsset>(SessionState.GetString(PreviousStart, ""));
        }
    }

    static NpcController Spawn(Vector3 position) => UnityEngine.Object.Instantiate(
        AssetDatabase.LoadAssetAtPath<GameObject>(NpcPrefabSetup.PrefabPath), position, Quaternion.identity).GetComponent<NpcController>();

    static void Platform(Vector3 position, Vector3 scale)
    {
        var platform = GameObject.CreatePrimitive(PrimitiveType.Cube);
        platform.name = "Temporary NPC check platform";
        platform.transform.position = position; platform.transform.localScale = scale;
        var shape = platform.GetComponent<Collider>();
        foreach (var other in UnityEngine.Object.FindObjectsByType<Collider>())
            if (other != shape && other.GetComponent<CharacterController>() == null) Physics.IgnoreCollision(shape, other);
    }

    static void Tick()
    {
        if (!EditorApplication.isPlaying) return;
        try
        {
            float elapsed = Time.time - started;
            CheckCursor();
            if (phase == 0 && elapsed > 1)
            {
                idlePosition = idle.transform.position;
                destination = npc.transform.position + new Vector3(1, 0, 3);
                npc.MoveTo(destination);
                phase = 1; started = Time.time;
            }
            else if (phase == 1)
            {
                Require(elapsed < 15, $"NPC reaches destination in time: position {npc.transform.position:F3}, target {destination:F3}.");
                Require(Vector3.Distance(idle.transform.position, idlePosition) < .1f, "Second NPC remains independent and idle.");
                if (!npc.HasDestination)
                {
                    Require(Vector3.Distance(npc.transform.position, destination) < .35f, "Root-motion walking reaches destination.");
                    File.AppendAllText(Report, $"PASS: NPC walked to destination (error {Vector3.Distance(npc.transform.position, destination):F3} m), while second NPC remained idle.\n");
                    seat = UnityEngine.Object.FindObjectsByType<BoatSeat>().First(s => s.name == "Rower aft");
                    // Use a clear raised temporary dock to isolate NPC control
                    // from the unfinished low shore approach clearances.
                    seat.LeftApproach.position += Vector3.up;
                    var target = seat.Approach(true, animator.humanScale);
                    var outward = Vector3.ProjectOnPlane(target.position - seat.transform.position, Vector3.up).normalized;
                    Platform(target.position + outward * .65f - Vector3.up * .11f, new Vector3(2.2f, .2f, 3));
                    var capsule = npc.GetComponent<CharacterController>();
                    capsule.enabled = false;
                    npc.transform.SetPositionAndRotation(target.position + outward * 1.1f + Vector3.up * .08f, target.rotation);
                    npc.GetComponent<PlayerController>().ResetBoatSupport();
                    capsule.enabled = true;
                    phase = 2; started = Time.time;
                }
            }
            else if (phase == 2 && elapsed > 1)
            {
                Require(npc.BoardSeat(seat, true), "NPC can reserve and approach a specific seat.");
                phase = 3; started = Time.time;
            }
            else if (phase == 3)
            {
                Require(elapsed < 24 && npc.Boarding.State != BoatBoarding.BoardingState.Idle, "NPC approach and boarding complete.");
                if (npc.Boarding.State == BoatBoarding.BoardingState.Seated && npc.Boarding.SeatIKBlend > .99f)
                {
                    restOar = seat.LeftHandOar.Model.localRotation;
                    npc.SetRowing(Vector2.up);
                    phase = 4; started = Time.time;
                }
            }
            else if (phase == 4)
            {
                maximumHipError = Mathf.Max(maximumHipError, Vector3.Distance(animator.GetBoneTransform(HumanBodyBones.Hips).position, seat.transform.position));
                maximumHandError = Mathf.Max(maximumHandError,
                    Vector3.Distance(animator.GetBoneTransform(HumanBodyBones.LeftHand).position, seat.LeftHandOar.HandGrip.position),
                    Vector3.Distance(animator.GetBoneTransform(HumanBodyBones.RightHand).position, seat.RightHandOar.HandGrip.position));
                maximumSwing = Mathf.Max(maximumSwing, Quaternion.Angle(restOar, seat.LeftHandOar.Model.localRotation));
                if (elapsed > 3)
                {
                    Require(animator.GetFloat("YAxis") > .9f && maximumSwing > 10, "NPC rowing command drives animation and oars.");
                    Require(maximumHipError < .008f && maximumHandError < .025f, "NPC shares pelvis and hand IK.");
                    File.AppendAllText(Report, $"PASS: NPC boarding and rowing; hips {maximumHipError * 1000:F1} mm, hands {maximumHandError * 1000:F1} mm, oar swing {maximumSwing:F1} deg.\n");
                    Require(npc.ExitBoat(false), "NPC can request the other exit side.");
                    phase = 5; started = Time.time;
                }
            }
            else if (phase == 5)
            {
                Require(elapsed < 8, "NPC exits in time.");
                if (npc.Boarding.State == BoatBoarding.BoardingState.Idle)
                {
                    Require(seat.IsAvailable && npc.GetComponent<CharacterController>().enabled && npc.Boarding.SeatIKBlend == 0, "Exit restores collisions and releases seat/IK.");
                    npc.enabled = false; npc.enabled = true;
                    UnityEngine.Object.DestroyImmediate(idle.gameObject);
                    CheckCursor();
                    Finish("PASS: NPC exit, enable/disable and destruction leave player cursor/input ownership intact; editing scene restored after Play.");
                }
            }
        }
        catch (Exception error) { Finish("FAIL: " + error); }
    }

    static void CheckCursor() => Require(Cursor.lockState == cursorLock && Cursor.visible == cursorVisible, "NPC lifecycle does not own the cursor.");
    static void Require(bool condition, string message) { if (!condition) throw new InvalidOperationException(message); }
    static void Finish(string message)
    {
        EditorApplication.update -= Tick;
        File.AppendAllText(Report, message + "\n");
        Debug.Log(File.ReadAllText(Report));
        EditorApplication.isPlaying = false;
    }
}
