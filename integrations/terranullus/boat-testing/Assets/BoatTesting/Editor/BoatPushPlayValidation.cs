using System;
using System.IO;
using System.Linq;
using Motu.Gameplay;
using UnityEditor;
using UnityEditor.SceneManagement;
using UnityEngine;
using UnityEngine.InputSystem;
using UnityEngine.InputSystem.LowLevel;
using Object = UnityEngine.Object;

[InitializeOnLoad]
public static class BoatPushPlayValidation
{
    const string Running = "BoatPushPlayValidation";
    public const string Report = "/tmp/boat-push-play.txt";
    static BoatPushing actor;
    static BoatBoarding boarding;
    static RowboatController boat;
    static BoatPushable pushable;
    static Rigidbody body;
    static CharacterController capsule;
    static MeshCollider hull;
    static Keyboard keyboard;
    static GameObject shore;
    static Vector3 home, pushStart, playerStart, direction;
    static Quaternion homeRotation;
    static float began, maxHandError;
    static int phase, sample;
    static bool sawContact;
    static Camera overview;
    static RenderTexture picture;
    static bool captured;
    static double deadline;
    static readonly string[] Samples = { "Port aft", "Starboard forward", "Oblique port bow", "Stern", "Palms miss", "Cancel E", "Cancel movement" };
    static BoatPushPlayValidation() => EditorApplication.playModeStateChanged += Changed;
    public static void RunBatch()
    {
        if (!Application.isBatchMode || !File.Exists(".boat-exit-isolated-project")) throw new InvalidOperationException("Isolated project only.");
        EditorSceneManager.OpenScene("Assets/Scenes/RowingBoatCrewTest.unity");
        BoatPushSetup.Apply();
        InputSystem.actions = AssetDatabase.LoadAssetAtPath<InputActionAsset>("Assets/InputSystem_Actions.inputactions");
        File.WriteAllText(Report, "Real E input and animated palm hull collisions, with no fixed standing or hand markers.\n");
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
            keyboard = InputSystem.AddDevice<Keyboard>("PalmContactValidationKeyboard");
            boat = Object.FindAnyObjectByType<RowboatController>(); body = boat.Body; pushable = boat.GetComponent<BoatPushable>();
            hull = boat.GetComponentsInChildren<MeshCollider>().First(c => !c.convex);
            boarding = Object.FindAnyObjectByType<RowingBoatTestScenario>().Player.GetComponent<BoatBoarding>();
            actor = boarding.GetComponent<BoatPushing>(); capsule = actor.GetComponent<CharacterController>();
            phase = sample = 0; began = Time.time; deadline = EditorApplication.timeSinceStartup + 240;
            EditorApplication.update += Tick;
        }
        if (change == PlayModeStateChange.EnteredEditMode)
        {
            EditorApplication.update -= Tick; SessionState.SetBool(Running, false);
            EditorApplication.Exit(SessionState.GetInt(Running + ".result", 1));
        }
    }
    static void Key(params Key[] keys) => InputSystem.QueueStateEvent(keyboard, new KeyboardState(keys));
    static void Check(bool success, string message)
    {
        if (!success) throw new InvalidOperationException(message);
        File.AppendAllText(Report, "PASS: " + message + "\n");
    }
    static void Prepare()
    {
        actor.CancelPush(); boat.BeginShorePush();
        body.position = home; body.rotation = homeRotation; body.linearVelocity = body.angularVelocity = Vector3.zero;
        body.transform.SetPositionAndRotation(home, homeRotation); boat.transform.SetPositionAndRotation(home, homeRotation);
        boat.EndShorePush(); Physics.SyncTransforms();
        var origin = sample == 0 || sample >= 4 ? new Vector3(-4, .55f, -1.1f)
            : sample == 1 ? new Vector3(4, .55f, .9f) : sample == 2 ? new Vector3(-4, .55f, 1.8f) : new Vector3(0, .8f, -5);
        direction = sample == 0 || sample >= 4 ? Vector3.right : sample == 1 ? Vector3.left
            : sample == 2 ? new Vector3(1, 0, -.35f).normalized : Vector3.forward;
        var worldOrigin = boat.transform.TransformPoint(origin);
        if (!hull.Raycast(new Ray(worldOrigin, direction), out var hit, 10)) throw new Exception("No test hull intersection.");
        playerStart = hit.point - direction * (sample == 4 ? 3.3f : sample == 3 ? .45f : .68f);
        playerStart.y = shore.GetComponent<Collider>().bounds.max.y + capsule.skinWidth + .02f;
        capsule.enabled = false;
        float yaw = actor.transform.eulerAngles.y;
        actor.transform.SetPositionAndRotation(playerStart, Quaternion.LookRotation(direction));
        actor.GetComponent<PlayerController>().CarryBoatFacing(Mathf.DeltaAngle(yaw, actor.transform.eulerAngles.y));
        actor.GetComponent<PlayerController>().ResetBoatSupport(); capsule.enabled = true; Physics.SyncTransforms();
        var settings = new SerializedObject(actor); settings.FindProperty("interactionRange").floatValue = sample == 4 ? 4f : 1.35f;
        settings.ApplyModifiedPropertiesWithoutUndo();
        if (sample == 0)
        {
            overview = new GameObject("Palm contact overview").AddComponent<Camera>();
            overview.CopyFrom(actor.GetComponentInChildren<Camera>());
            picture = new RenderTexture(1024, 768, 24); overview.targetTexture = picture;
            overview.transform.position = playerStart + new Vector3(-2, 1.7f, -2.5f);
            overview.transform.LookAt(playerStart + direction * .65f + Vector3.up * .9f);
            overview.enabled = true; captured = false;
        }
        pushStart = body.position; sawContact = false; maxHandError = 0; began = Time.time; phase = 1;
    }
    static Vector3 Palm(Animator animator, HumanBodyBones wrist, HumanBodyBones finger) =>
        Vector3.Lerp(animator.GetBoneTransform(wrist).position, animator.GetBoneTransform(finger).position, .65f);
    static void Tick()
    {
        EditorApplication.QueuePlayerLoopUpdate(); InputSystem.Update();
        try
        {
            if (EditorApplication.timeSinceStartup > deadline) throw new TimeoutException();
            float elapsed = Time.time - began;
            if (phase == 0 && elapsed > 4)
            {
                Check(boat.transform.Find("Push spots") == null, "Boat has no fixed push spots or authored hand targets");
                Check(!actor.TryPushNearest() && boarding.State == BoatBoarding.BoardingState.Seated, "Aboard player cannot push");
                boarding.CancelBoarding(); Check(!pushable.IsAvailable, "Afloat boat is not pushable");
                Check(boarding.TryBoard(boat.CommandSeat, true), "Afloat boat still allows climbing aboard");
                boarding.CancelBoarding();
                home = body.position; homeRotation = Quaternion.Euler(-1.768f, 0, 0);
                shore = GameObject.CreatePrimitive(PrimitiveType.Cube); shore.name = "Contact test shore";
                float bottom = body.GetComponentsInChildren<Collider>().Where(c => !c.isTrigger && c.attachedRigidbody == body).Min(c => c.bounds.min.y);
                shore.transform.position = new Vector3(home.x, bottom - .45f, home.z); shore.transform.localScale = new Vector3(40, 1, 40);
                Prepare();
            }
            else if (phase == 1 && elapsed > .7f)
            {
                Check(boat.IsGrounded && capsule.isGrounded, Samples[sample] + " has real shore support");
                if (sample == 0)
                {
                    Check(!boarding.TryBoardNearest() && !boarding.TryBoard(boat.CommandSeat, true)
                        && !boarding.TryBoard(boat.CommandSeat, false) && !boarding.TrySeatImmediately(boat.CommandSeat, true)
                        && boarding.State == BoatBoarding.BoardingState.Idle,
                        "Grounded boat rejects nearest, both climb sides and immediate boarding");
                    var ground = shore.GetComponent<Collider>(); ground.enabled = false;
                    Check(boat.IsShoreHeld && !boat.IsGrounded && !boarding.TryBoard(boat.CommandSeat, true)
                        && !boarding.TryBoardNearest(), "Beach-held boat still rejects boarding when its shore collider unloads");
                    ground.enabled = true; Physics.SyncTransforms();
                }
                Key(UnityEngine.InputSystem.Key.E); began = Time.time; phase = 2;
            }
            else if (phase == 2 && elapsed > .12f)
            {
                Key(); Check(actor.IsAnimating && boarding.State == BoatBoarding.BoardingState.Idle, "E plays push directly at " + Samples[sample]);
                Check(Vector3.ProjectOnPlane(actor.transform.position - playerStart, Vector3.up).magnitude < .12f, "No relocation to a standing spot");
                began = Time.time; phase = 3;
            }
            else if (phase == 3)
            {
                if (sample == 0 && actor.ForceSteps > 10 && !captured)
                {
                    var previous = RenderTexture.active; RenderTexture.active = picture;
                    var image = new Texture2D(1024, 768, TextureFormat.RGB24, false);
                    image.ReadPixels(new Rect(0, 0, 1024, 768), 0, 0); image.Apply();
                    File.WriteAllBytes("/tmp/boat-palm-contact.png", image.EncodeToPNG());
                    RenderTexture.active = previous; Object.Destroy(image);
                    Object.Destroy(overview.gameObject); Object.Destroy(picture); captured = true;
                }
                if (sample >= 5 && elapsed > .45f)
                { Key(sample == 5 ? UnityEngine.InputSystem.Key.E : UnityEngine.InputSystem.Key.W); began = Time.time; phase = 4; return; }
                if (actor.LeftContact.IsValid || actor.RightContact.IsValid)
                {
                    sawContact = true;
                    if (actor.ContactWeight > .95f)
                    {
                        var animator = actor.GetComponent<Animator>();
                        if (actor.LeftContact.IsValid) maxHandError = Mathf.Max(maxHandError, Vector3.Distance(Palm(animator, HumanBodyBones.LeftHand, HumanBodyBones.LeftMiddleProximal), actor.LeftContact.Position));
                        if (actor.RightContact.IsValid) maxHandError = Mathf.Max(maxHandError, Vector3.Distance(Palm(animator, HumanBodyBones.RightHand, HumanBodyBones.RightMiddleProximal), actor.RightContact.Position));
                    }
                }
                if (!actor.IsActive)
                {
                    float travel = Vector3.Dot(body.position - pushStart, direction);
                    File.AppendAllText(Report, $"{Samples[sample]}: contacts={sawContact}, palm-error={maxHandError:F3}m, left-force={actor.LeftForceSteps}, right-force={actor.RightForceSteps}, travel={travel:F3}m, player-travel={Vector3.Distance(actor.transform.position, playerStart):F3}m.\n");
                    if (sample == 4) Check(!sawContact && actor.ForceSteps == 0, "An animation whose palms miss the boat applies no force");
                    else Check(sawContact && actor.ForceSteps > 0 && travel > .015f && maxHandError < .15f,
                        Samples[sample] + " uses detected palm contact, IK and force at the actual hull");
                    Check(actor.GetComponent<PlayerController>().IKConstraint == null && capsule.enabled
                        && boarding.State == BoatBoarding.BoardingState.Idle, "Grounded or missed push leaves player ashore");
                    sample++;
                    if (sample == Samples.Length) { Finish(0); return; }
                    Prepare();
                }
                else if (elapsed > 10) throw new Exception("Push did not finish.");
            }
            else if (phase == 4 && elapsed > .25f)
            {
                Key();
                Check(!actor.IsActive && actor.GetComponent<PlayerController>().IKConstraint == null
                    && !actor.IsAwaitingLaunch && !actor.LeftContact.IsValid && !actor.RightContact.IsValid && pushable.IsAvailable,
                    Samples[sample] + " releases both palms, boat reservation and player control");
                sample++;
                if (sample == Samples.Length) { Finish(0); return; }
                Prepare();
            }
        }
        catch (Exception error) { File.AppendAllText(Report, "FAIL: " + error + "\n"); Debug.LogException(error); Finish(1); }
    }
    static void Finish(int code)
    { EditorApplication.update -= Tick; SessionState.SetInt(Running + ".result", code); EditorApplication.isPlaying = false; }
}
