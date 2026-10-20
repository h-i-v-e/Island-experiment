using System;
using System.IO;
using System.Linq;
using System.Text;
using UnityEditor;
using UnityEditor.Animations;
using UnityEditor.SceneManagement;
using UnityEngine;
using UnityEngine.Animations;
using UnityEngine.Playables;

public static class TheOneBoatAnimationValidation
{
    [MenuItem("Island/Boat Tests/Validate TheOne Boat Animation Paths")]
    public static void Validate()
    {
        var report = new StringBuilder();
        try
        {
            foreach (bool port in new[] { true, false })
                foreach (bool water in new[] { false, true }) CheckPath(port, water, report);
            report.AppendLine("PASS: All four entry paths, seated rowing and both exit paths completed on the player avatar.");
        }
        catch (Exception error) { report.AppendLine("FAIL: " + error); }
        File.WriteAllText("/tmp/theone-boat-animation-validation.txt", report.ToString());
        Debug.Log(report.ToString());
    }

    static void CheckPath(bool port, bool water, StringBuilder report)
    {
        var scene = EditorSceneManager.NewPreviewScene();
        var actor = (GameObject)PrefabUtility.InstantiatePrefab(AssetDatabase.LoadAssetAtPath<GameObject>("Assets/Prefabs/Player.prefab"), scene);
        foreach (var behaviour in actor.GetComponentsInChildren<MonoBehaviour>()) behaviour.enabled = false;
        var animator = actor.GetComponent<Animator>();
        animator.enabled = true;
        animator.cullingMode = AnimatorCullingMode.AlwaysAnimate;
        animator.runtimeAnimatorController = null;
        animator.applyRootMotion = false;
        animator.Rebind();
        actor.transform.SetPositionAndRotation(Vector3.zero, Quaternion.identity);
        var graph = PlayableGraph.Create("Validate boat animation paths");
        graph.SetTimeUpdateMode(DirectorUpdateMode.Manual);
        try
        {
            var controller = AssetDatabase.LoadAssetAtPath<AnimatorController>("Assets/TheOne.controller");
            var playable = AnimatorControllerPlayable.Create(graph, controller);
            AnimationPlayableOutput.Create(graph, "Pose", animator).SetSourcePlayable(playable);
            graph.Play();
            var hips = animator.GetBoneTransform(HumanBodyBones.Hips);
            void Step(float seconds) { for (int i = 0; i < Mathf.CeilToInt(seconds / .02f); i++) graph.Evaluate(.02f); }
            bool In(string name) => playable.GetCurrentAnimatorStateInfo(0).IsName(name);
            Step(1);
            Require(In("Base Layer.Blend Tree"), "Walking remains active with Locamotion=0.");
            playable.SetBool("PortSide", port);
            playable.SetBool("ClimbFromWater", water);
            playable.SetInteger("Locamotion", 1);
            Step(.24f);
            string side = port ? "Left" : "Right";
            Require(In("Base Layer." + (water ? "ClimbToBoat" : "ToBoat") + side), "Correct side and entry variant selected.");
            Require(!playable.GetCurrentAnimatorClipInfo(0)[0].clip.isLooping, "Entry is a one-shot.");
            float largestJoinStep = 0;
            Vector3 previous = hips.position;
            for (int i = 0; i < 280; i++)
            {
                bool joining = playable.IsInTransition(0) && playable.GetNextAnimatorStateInfo(0).IsTag("Boat");
                graph.Evaluate(.02f);
                if (joining) largestJoinStep = Mathf.Max(largestJoinStep, Vector3.Distance(previous, hips.position));
                previous = hips.position;
            }
            Require(In("Base Layer.Rowing.rowing"), "Entry completes into rowing.");
            Require(largestJoinStep < .04f, "Entry-to-rowing hip movement stays continuous.");
            Vector3 seated = hips.position;
            Require(seated.y > .2f && seated.y < .35f, "Rowing remains seated.");
            if (port && !water) Capture(actor, "rowing");
            foreach (var input in new[] { new Vector2(0,1), new Vector2(-1,0), new Vector2(1,0), Vector2.zero })
            {
                playable.SetFloat("XAxis", input.x); playable.SetFloat("YAxis", input.y);
                Step(5);
                Require(In("Base Layer.Rowing.rowing"), "Rowing blend stays active until exit is requested.");
                Require(Mathf.Abs(hips.position.x) < .08f && Mathf.Abs(hips.position.z) < .24f,
                    "Forward and turning strokes retain the seat frame.");
            }
            playable.SetInteger("Locamotion", 0);
            Step(.24f);
            Require(In("Base Layer.FromBoat" + side), "Exit begins promptly on the selected side.");
            Require(!playable.GetCurrentAnimatorClipInfo(0)[0].clip.isLooping, "Exit is a one-shot.");
            Vector3 exitStart = hips.position;
            Require(Vector3.Distance(seated, exitStart) < .08f, "Exit begins at the rowing seat.");
            if (!water) Capture(actor, "exit-" + side);
            Step(5.3f);
            Require(In("Base Layer.Blend Tree"), "Exit completes into walking without re-entry.");
            Step(2);
            Require(In("Base Layer.Blend Tree"), "Walking remains active after disembarking.");
            report.AppendLine($"PASS: {(water ? "water climb" : "shore entry")} {side}; largest entry join hip step {largestJoinStep * 1000:F1} mm; exit seat difference {Vector3.Distance(seated, exitStart) * 1000:F1} mm.");
        }
        finally { graph.Destroy(); EditorSceneManager.ClosePreviewScene(scene); }
    }

    static void Capture(GameObject actor, string name)
    {
        var preview = new PreviewRenderUtility();
        try
        {
            // Render the actual skinned player mesh, leaving all editing scenes alone.
            var snapshot = UnityEngine.Object.Instantiate(actor);
            snapshot.GetComponent<Animator>().enabled = false;
            preview.AddSingleGO(snapshot);
            preview.BeginStaticPreview(new Rect(0, 0, 720, 720));
            preview.camera.fieldOfView = 35;
            preview.camera.transform.position = new Vector3(2.5f, 1.4f, 3f);
            preview.camera.transform.LookAt(new Vector3(0, .55f, 0));
            preview.camera.nearClipPlane = .01f;
            preview.camera.farClipPlane = 20;
            preview.camera.clearFlags = CameraClearFlags.SolidColor;
            preview.camera.backgroundColor = new Color(.18f, .2f, .23f);
            preview.lights[0].intensity = 1.2f;
            preview.lights[0].transform.rotation = Quaternion.Euler(40, 30, 0);
            preview.lights[1].intensity = .8f;
            preview.ambientColor = Color.gray;
            preview.Render(true);
            var texture = preview.EndStaticPreview();
            File.WriteAllBytes("/tmp/theone-boat-" + name + ".png", texture.EncodeToPNG());
            UnityEngine.Object.DestroyImmediate(texture);
        }
        finally { preview.Cleanup(); }
    }

    static void Require(bool condition, string message)
    {
        if (!condition) throw new InvalidOperationException(message);
    }
}
