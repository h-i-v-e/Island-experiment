using System;
using System.IO;
using System.Linq;
using System.Reflection;
using UnityEditor;
using UnityEditor.SceneManagement;
using UnityEngine;

// Exercises the real PlayerController, CharacterController and animator in Play.
[InitializeOnLoad]
public static class TheOneBoatAnimationPlayValidation
{
    const string Running = "TheOneBoatAnimationPlayValidation.Running";
    const string PreviousStart = "TheOneBoatAnimationPlayValidation.PreviousStart";
    const string Report = "/tmp/theone-boat-animation-play.txt";
    static GameObject actor, floor;
    static Animator animator;
    static MonoBehaviour movement;
    static Camera view;
    static float started;
    static int phase, side;
    static Vector3 entryStart;

    static TheOneBoatAnimationPlayValidation()
    {
        EditorApplication.playModeStateChanged += State;
    }

    [MenuItem("Island/Boat Tests/Run TheOne Boat Animation Play Check")]
    public static void Run()
    {
        if (EditorApplication.isPlayingOrWillChangePlaymode) throw new InvalidOperationException("Stop Play mode first.");
        SessionState.SetString(PreviousStart, AssetDatabase.GetAssetPath(EditorSceneManager.playModeStartScene));
        EditorSceneManager.playModeStartScene = null;
        SessionState.SetBool(Running, true);
        File.WriteAllText(Report, "Testing real player root motion and boat-pose overrides on an isolated platform in Play.\n");
        EditorApplication.isPlaying = true;
    }

    static void State(PlayModeStateChange state)
    {
        if (!SessionState.GetBool(Running, false)) return;
        if (state == PlayModeStateChange.EnteredPlayMode)
        {
            try
            {
                view = Camera.main;
                foreach (var item in UnityEngine.Object.FindObjectsByType<MonoBehaviour>(FindObjectsSortMode.None)
                    .Where(m => m.GetType().Name == "PlayerController" || m is BoatTestPlayer)) item.enabled = false;
                view.transform.SetParent(null, true);
                floor = GameObject.CreatePrimitive(PrimitiveType.Cube);
                floor.name = "Temporary animation validation platform";
                floor.transform.position = new Vector3(1000, 4.5f, 0);
                floor.transform.localScale = new Vector3(20, 1, 20);
                side = 0;
                BeginActor();
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

    static void BeginActor()
    {
        actor = UnityEngine.Object.Instantiate(AssetDatabase.LoadAssetAtPath<GameObject>("Assets/Prefabs/Player.prefab"), new Vector3(1000, 5.1f, 0), Quaternion.identity);
        actor.name = side == 0 ? "Validate port boarding" : "Validate starboard boarding";
        foreach (var camera in actor.GetComponentsInChildren<Camera>()) camera.enabled = false;
        foreach (var listener in actor.GetComponentsInChildren<AudioListener>()) listener.enabled = false;
        movement = actor.GetComponents<MonoBehaviour>().First(m => m.GetType().Name == "PlayerController");
        animator = actor.GetComponent<Animator>();
        animator.SetInteger("Locamotion", 0);
        animator.SetBool("PortSide", side == 0);
        animator.SetBool("ClimbFromWater", false);
        started = Time.time;
        phase = 0;
        view.transform.position = new Vector3(1003, 7, 4);
        view.transform.LookAt(actor.transform.position + Vector3.up * .7f);
    }

    static void Tick()
    {
        if (!EditorApplication.isPlaying) return;
        try
        {
            float elapsed = Time.time - started;
            if (phase == 0 && elapsed > 1.5f)
            {
                Require(animator.GetCurrentAnimatorStateInfo(0).IsName("Base Layer.Blend Tree"), "Player remains walking before boarding.");
                entryStart = actor.transform.position;
                animator.SetInteger("Locamotion", 1);
                phase = 1;
            }
            if (phase == 1 && elapsed > 7.2f)
            {
                Require(animator.GetCurrentAnimatorStateInfo(0).IsName("Base Layer.Rowing.rowing"), "Player reaches rowing.");
                float distance = Vector3.Distance(entryStart, actor.transform.position);
                Require(distance > .5f && distance < 3f, "Entry root motion physically moves the player into the boat frame.");
                // Force a stale walking overlay; the next actual Update must clear it.
                animator.SetLayerWeight(1, 1);
                phase = 2;
            }
            if (phase == 2 && elapsed > 7.5f)
            {
                Require(animator.GetLayerWeight(1) < .001f, "Walking overlay is suppressed while rowing.");
                const BindingFlags fields = BindingFlags.Instance | BindingFlags.NonPublic;
                Require((float)movement.GetType().GetField("leftFootWeight", fields).GetValue(movement) == 0f
                    && (float)movement.GetType().GetField("rightFootWeight", fields).GetValue(movement) == 0f,
                    "Standing foot IK does not alter the seated pose.");
                animator.SetInteger("Locamotion", 0);
                phase = 3;
            }
            if (phase == 3 && elapsed > 13.2f)
            {
                Require(animator.GetCurrentAnimatorStateInfo(0).IsName("Base Layer.Blend Tree"), "Player exits back to walking.");
                float returnDistance = Vector3.Distance(entryStart, actor.transform.position);
                Require(returnDistance < .25f, "Entry and exit root motion return to the boarding point.");
                File.AppendAllText(Report, $"PASS: {(side == 0 ? "port" : "starboard")} entry -> rowing -> exit -> walking; return error {returnDistance:F3} m; walking overlay and foot IK suppressed aboard.\n");
                UnityEngine.Object.Destroy(actor);
                if (++side < 2) BeginActor();
                else Finish("PASS: Both live player paths completed without scene asset changes.");
            }
        }
        catch (Exception error) { Finish("FAIL: " + error); }
    }

    static void Require(bool condition, string message)
    { if (!condition) throw new InvalidOperationException(message); }

    static void Finish(string message)
    {
        EditorApplication.update -= Tick;
        File.AppendAllText(Report, message + "\n");
        Debug.Log(File.ReadAllText(Report));
        EditorApplication.isPlaying = false;
    }
}
