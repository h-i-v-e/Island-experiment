using System;
using System.IO;
using System.Linq;
using UnityEditor;
using UnityEditor.SceneManagement;
using UnityEngine;

[InitializeOnLoad]
public static class BoatBoardingPlayValidation
{
    const string Running = "BoatBoardingPlayValidation.Running";
    const string PreviousStart = "BoatBoardingPlayValidation.PreviousStart";
    const string FirstTest = "BoatBoardingPlayValidation.FirstTest";
    const string Report = "/tmp/boat-boarding-play.txt";
    static GameObject actor, contender, dock;
    static BoatBoarding boarding;
    static BoatSeat seat;
    static Animator animator;
    static Camera view;
    static int test, phase;
    static float started, stageStarted, maximumHipError, minimumBoatY, maximumBoatY;
    static Vector3 startPosition;
    static bool sawApproach, sawEntry;

    static BoatBoardingPlayValidation() => EditorApplication.playModeStateChanged += StateChanged;

    [MenuItem("Island/Boat Tests/Run Seat Approach And Boarding Play Check")]
    public static void Run() => RunFrom(0);

    [MenuItem("Island/Boat Tests/Run Forward Rower IK Play Check")]
    public static void RunForwardIK() => RunFrom(2);

    static void RunFrom(int first)
    {
        if (EditorApplication.isPlayingOrWillChangePlaymode) throw new InvalidOperationException("Stop Play mode first.");
        SessionState.SetString(PreviousStart, AssetDatabase.GetAssetPath(EditorSceneManager.playModeStartScene));
        EditorSceneManager.playModeStartScene = null;
        SessionState.SetBool(Running, true);
        SessionState.SetInt(FirstTest, first);
        File.WriteAllText(Report, "Checking generated seat targets, normal walking approach and animation docking on the real floating boat.\n");
        EditorApplication.isPlaying = true;
    }

    static void StateChanged(PlayModeStateChange state)
    {
        if (!SessionState.GetBool(Running, false)) return;
        if (state == PlayModeStateChange.EnteredPlayMode)
        {
            try
            {
                view = Camera.main;
                foreach (var component in UnityEngine.Object.FindObjectsByType<MonoBehaviour>()
                    .Where(c => c.GetType().Name == "PlayerController" || c is BoatTestPlayer)) component.enabled = false;
                view.transform.SetParent(null, true);
                var seats = UnityEngine.Object.FindObjectsByType<BoatSeat>();
                Require(seats.Count(s => s.Role == BoatSeat.SeatRole.Rower) == 2 && seats.Count(s => s.Role == BoatSeat.SeatRole.Passenger) == 2,
                    "Two rowers and two passengers are present.");
                foreach (var item in seats)
                {
                    Require(item.LeftApproach != null && item.RightApproach != null && item.IsAvailable, "Both generated approach points are valid and the seat is free.");
                    foreach (bool port in new[] { true, false })
                    {
                        var start = item.AnimationStart(port, item.Profile.referenceHumanScale);
                        var end = BoatBoardingProfile.Multiply(start, item.Profile.Entry(port, item.WaterClimb).Evaluate(1, item.Profile.referenceHumanScale));
                        var target = item.SeatedRoot(item.Profile.referenceHumanScale);
                        Require(Vector3.Distance(end.position, target.position) < .0001f && Quaternion.Angle(end.rotation, target.rotation) < .02f,
                            "Inverting entry travel reaches the seated target at either side.");
                    }
                }
                test = SessionState.GetInt(FirstTest, 0);
                Begin();
                EditorApplication.update += Tick;
            }
            catch (Exception error) { Finish("FAIL: " + error); }
        }
        if (state == PlayModeStateChange.EnteredEditMode)
        {
            EditorApplication.update -= Tick;
            SessionState.SetBool(Running, false);
            EditorSceneManager.playModeStartScene = AssetDatabase.LoadAssetAtPath<SceneAsset>(SessionState.GetString(PreviousStart, ""));
        }
    }

    static GameObject Character(Vector3 position)
    {
        var instance = UnityEngine.Object.Instantiate(AssetDatabase.LoadAssetAtPath<GameObject>("Assets/Prefabs/Player.prefab"), position, Quaternion.identity);
        foreach (var camera in instance.GetComponentsInChildren<Camera>()) camera.enabled = false;
        foreach (var listener in instance.GetComponentsInChildren<AudioListener>()) listener.enabled = false;
        return instance;
    }

    static void Begin()
    {
        seat = UnityEngine.Object.FindObjectsByType<BoatSeat>()
            .First(s => s.name == (test == 0 ? "Rower aft" : test == 1 ? "Passenger forward" : "Rower forward"));
        bool port = test != 1;
        if (test == 2)
        {
            // The forward seat's generated shore start is below the gunwale.
            // Use a raised temporary dock/approach for this additional IK case;
            // authored scene targets are restored on leaving Play mode.
            seat.LeftApproach.position += Vector3.up;
            File.AppendAllText(Report, "Forward-rower IK case uses a temporary approach/dock raised 1 m for hull clearance.\n");
        }
        var target = seat.Approach(port, seat.Profile.referenceHumanScale);
        var outward = Vector3.ProjectOnPlane(target.position - seat.transform.position, Vector3.up).normalized;
        startPosition = target.position + outward * 1.1f;
        dock = GameObject.CreatePrimitive(PrimitiveType.Cube);
        dock.name = "Temporary boarding dock";
        dock.transform.position = target.position + outward * .65f - Vector3.up * .11f;
        dock.transform.localScale = new Vector3(2.2f, .2f, 3f);
        // The temporary dock supports only the character, without pushing the boat.
        var dockCollider = dock.GetComponent<Collider>();
        foreach (var collider in UnityEngine.Object.FindObjectsByType<Collider>())
            if (collider != dockCollider && collider.GetComponent<CharacterController>() == null) Physics.IgnoreCollision(dockCollider, collider);
        actor = Character(startPosition + Vector3.up * .08f);
        boarding = actor.GetComponent<BoatBoarding>();
        animator = actor.GetComponent<Animator>();
        contender = Character(startPosition + outward * .6f + Vector3.up * .08f);
        Physics.IgnoreCollision(actor.GetComponent<CharacterController>(), contender.GetComponent<CharacterController>());
        view.transform.position = seat.transform.position + new Vector3(4, 2.5f, 4);
        view.transform.LookAt(seat.transform.position);
        started = Time.time;
        phase = 0;
        maximumHipError = 0;
        minimumBoatY = float.MaxValue;
        maximumBoatY = float.MinValue;
        sawApproach = sawEntry = false;
    }

    static void Tick()
    {
        if (!EditorApplication.isPlaying) return;
        try
        {
            bool port = test != 1;
            if (phase == 0 && Time.time - started > 1)
            {
                Require(boarding.TryBoard(seat, port), "Character reserves a free seat.");
                Require(!contender.GetComponent<BoatBoarding>().TryBoard(seat, !port), "Second character cannot reserve an occupied target.");
                boarding.CancelBoarding();
                Require(seat.IsAvailable && actor.GetComponent<CharacterController>().enabled, "Cancelling the approach releases the seat and preserves walking.");
                Require(boarding.TryBoard(seat, port), "Cancelled seat can be reserved again.");
                phase = 1;
                stageStarted = Time.time;
            }
            if (phase == 1)
            {
                if (boarding.State == BoatBoarding.BoardingState.Approaching)
                {
                    sawApproach = true;
                    Require(animator.GetInteger("Locamotion") == 0 && actor.GetComponent<CharacterController>().enabled,
                        "Character walks toward the point before entry starts.");
                }
                if (boarding.State == BoatBoarding.BoardingState.Entering) sawEntry = true;
                Require(boarding.State != BoatBoarding.BoardingState.Idle,
                    $"Approach unexpectedly cancelled: actor {actor.transform.position:F3}, target {seat.Approach(port, animator.humanScale).position:F3}.");
                Require(Time.time - stageStarted < 23, "Approach and entry finish in time.");
                if (boarding.State == BoatBoarding.BoardingState.Seated)
                {
                    Require(sawApproach && sawEntry && Vector3.Distance(startPosition, actor.transform.position) > .5f, "Approach movement and entry both occurred.");
                    Require(animator.GetCurrentAnimatorStateInfo(0).IsName(seat.Role == BoatSeat.SeatRole.Rower ? "Base Layer.Rowing.rowing" : "Base Layer.Rowing.passenger"), "Seat role selects rower or passenger pose.");
                    phase = 2;
                    stageStarted = Time.time;
                }
            }
            if (phase == 2)
            {
                float error = Vector3.Distance(animator.GetBoneTransform(HumanBodyBones.Hips).position, seat.transform.position);
                maximumHipError = Mathf.Max(maximumHipError, error);
                minimumBoatY = Mathf.Min(minimumBoatY, seat.transform.position.y);
                maximumBoatY = Mathf.Max(maximumBoatY, seat.transform.position.y);
                Require(error < .04f, "Seated hips follow the moving seat marker.");
                Require(!actor.GetComponent<CharacterController>().enabled && !seat.IsAvailable, "Seat stays reserved with character collisions suspended aboard.");
                if (Time.time - stageStarted > 3)
                {
                    if (seat.Role == BoatSeat.SeatRole.Rower)
                    {
                        Require(seat.LeftHandOar != null && seat.RightHandOar != null, "Both rower hands have assigned oars.");
                        ValidateSeatIK();
                    }
                    Require(maximumBoatY - minimumBoatY > .005f, "Seat tracking was measured while the real boat moved on waves.");
                    Require(boarding.RequestExit(port), "Seated character can request the chosen exit side.");
                    phase = 3;
                    stageStarted = Time.time;
                }
            }
            if (phase == 3)
            {
                Require(Time.time - stageStarted < 8, "Exit completes in time.");
                if (boarding.State == BoatBoarding.BoardingState.Idle)
                {
                    Require(seat.IsAvailable && actor.GetComponent<CharacterController>().enabled && boarding.SeatIKBlend == 0, "Exit releases the seat and IK, and restores walking collisions.");
                    Require(Vector3.Distance(actor.transform.position, seat.Approach(port, animator.humanScale).position) < .08f,
                        "Exit restores walking at the approach marker, clear of the hull.");
                    File.AppendAllText(Report, $"PASS: {seat.name}, {(port ? "left" : "right")} approach -> entry -> {seat.Role} -> exit; maximum hips error {maximumHipError * 1000:F1} mm, seat heave {(maximumBoatY - minimumBoatY) * 1000:F1} mm; reservation and cancellation passed.\n");
                    UnityEngine.Object.Destroy(actor); UnityEngine.Object.Destroy(contender); UnityEngine.Object.Destroy(dock);
                    if (++test < 3) Begin(); else Finish("PASS: Seat targets, real player approach, moving-boat seating and exit checks passed.");
                }
            }
        }
        catch (Exception error) { Finish("FAIL: " + error); }
    }

    static void Require(bool condition, string message) { if (!condition) throw new InvalidOperationException(message); }

    static void ValidateSeatIK()
    {
        float hipsError = 0, handsError = 0, swing = 0;
        var original = seat.LeftHandOar.Model.localRotation;
        foreach (var input in new[] { Vector2.zero, Vector2.up, Vector2.left, Vector2.right })
        {
            animator.SetFloat("XAxis", input.x);
            animator.SetFloat("YAxis", input.y);
            for (int i = 0; i < 140; i++)
            {
                // Tick the real animator/PlayerController IK callback manually,
                // allowing deterministic stroke inputs without injecting keyboard events.
                animator.Update(.02f);
                float hip = Vector3.Distance(animator.GetBoneTransform(HumanBodyBones.Hips).position, seat.transform.position);
                float left = Vector3.Distance(animator.GetBoneTransform(HumanBodyBones.LeftHand).position, seat.LeftHandOar.HandGrip.position);
                float right = Vector3.Distance(animator.GetBoneTransform(HumanBodyBones.RightHand).position, seat.RightHandOar.HandGrip.position);
                hipsError = Mathf.Max(hipsError, hip);
                handsError = Mathf.Max(handsError, left, right);
                swing = Mathf.Max(swing, Quaternion.Angle(original, seat.LeftHandOar.Model.localRotation));
            }
        }
        File.AppendAllText(Report, $"IK strokes: maximum hips error {hipsError * 1000:F1} mm, hand-to-grip error {handsError * 1000:F1} mm; oar swing {swing:F1} deg.\n");
        Require(boarding.SeatIKBlend > .99f, "Seated IK has blended fully in.");
        Require(hipsError < .008f, "Pelvis stays fixed to the bench through all stroke blends.");
        Require(handsError < .025f, "Both hands remain on their oar grips through idle, rowing and turns.");
        Require(swing > 10f, "The oars animate with the rowing strokes.");
    }
    static void Finish(string message)
    {
        EditorApplication.update -= Tick;
        File.AppendAllText(Report, message + "\n");
        Debug.Log(File.ReadAllText(Report));
        EditorApplication.isPlaying = false;
    }
}
