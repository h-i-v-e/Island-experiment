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

public static class TheOneBoatAnimationSetup
{
    const string ControllerPath = "Assets/TheOne.controller";
    const string Clips = "Assets/Small_Boat_Anim_Set/Animations/";

    [MenuItem("Island/Boat Tests/Align TheOne Boat Clip Root Motion")]
    public static void AlignImports()
    {
        if (EditorApplication.isPlayingOrWillChangePlaymode) throw new InvalidOperationException("Stop Play mode first.");
        foreach (string name in new[] { "Boat_Enter_FromLeftSide", "Boat_Enter_FromRightSide",
            "Boat_Exit_ToLeftSide", "Boat_Exit_ToRightSide", "Boat_Enter_ClimbUpFromLeftSide", "Boat_Enter_ClimbUpFromRightSide" })
        {
            var importer = (ModelImporter)AssetImporter.GetAtPath(Clips + name + ".fbx");
            var clips = importer.clipAnimations;
            foreach (var clip in clips)
            {
                clip.loopTime = false;
                clip.loopPose = false;
                clip.lockRootPositionXZ = false;
                clip.lockRootRotation = false;
                clip.keepOriginalOrientation = true;
                clip.keepOriginalPositionXZ = false;
                clip.lockRootHeightY = true;
                clip.keepOriginalPositionY = true;
                clip.heightFromFeet = false;
                clip.rotationOffset = name.Contains("Left") ? 90 : -90;
            }
            importer.clipAnimations = clips;
            importer.SaveAndReimport();
        }
        // Extracted rotation is relative to the clip's initial root. Match that
        // reference to the seated rowing frame rather than retaining a side-on
        // standing reference from the entry take.
        var reference = Pose("Boat_DoublePaddle_IdleOarInHand", false);
        foreach (string name in new[] { "Boat_Enter_FromLeftSide", "Boat_Enter_FromRightSide",
            "Boat_Enter_ClimbUpFromLeftSide", "Boat_Enter_ClimbUpFromRightSide" })
        {
            var pose = Pose(name, true);
            var importer = (ModelImporter)AssetImporter.GetAtPath(Clips + name + ".fbx");
            var clips = importer.clipAnimations;
            foreach (var clip in clips)
                clip.rotationOffset += Mathf.DeltaAngle(reference.rotation.eulerAngles.y, pose.rotation.eulerAngles.y);
            importer.clipAnimations = clips;
            importer.SaveAndReimport();
        }
        ConfigureController();
        Inspect();
    }

    static AnimationClip Clip(string name) => AssetDatabase.LoadAllAssetsAtPath(Clips + name + ".fbx")
        .OfType<AnimationClip>().First(c => !c.name.StartsWith("__"));

    static (Vector3 position, Quaternion rotation) Pose(string name, bool end)
    {
        var scene = EditorSceneManager.NewPreviewScene();
        var avatar = (GameObject)PrefabUtility.InstantiatePrefab(AssetDatabase.LoadAssetAtPath<GameObject>("Assets/Prefabs/Player.prefab"), scene);
        foreach (var behaviour in avatar.GetComponentsInChildren<MonoBehaviour>()) behaviour.enabled = false;
        var animator = avatar.GetComponent<Animator>();
        animator.enabled = true;
        animator.runtimeAnimatorController = null;
        animator.cullingMode = AnimatorCullingMode.AlwaysAnimate;
        animator.applyRootMotion = false;
        animator.Rebind();
        avatar.transform.SetPositionAndRotation(Vector3.zero, Quaternion.identity);
        var graph = PlayableGraph.Create("Calibrate boat seated frame");
        graph.SetTimeUpdateMode(DirectorUpdateMode.Manual);
        try
        {
            var clip = Clip(name);
            var playable = AnimationClipPlayable.Create(graph, clip);
            playable.SetApplyFootIK(false);
            AnimationPlayableOutput.Create(graph, "Pose", animator).SetSourcePlayable(playable);
            graph.Play();
            playable.SetTime(end ? clip.length - 1d / clip.frameRate : 0);
            graph.Evaluate(.00001f);
            var hips = animator.GetBoneTransform(HumanBodyBones.Hips);
            return (hips.position, hips.rotation);
        }
        finally { graph.Destroy(); EditorSceneManager.ClosePreviewScene(scene); }
    }

    public static void ConfigureController()
    {
        var controller = AssetDatabase.LoadAssetAtPath<AnimatorController>(ControllerPath);
        Undo.RegisterCompleteObjectUndo(AssetDatabase.LoadAllAssetsAtPath(ControllerPath), "Align boat animation transitions");
        var layers = controller.layers;
        layers[0].iKPass = true;
        controller.layers = layers;
        var machine = controller.layers[0].stateMachine;
        AnimatorState State(string name) => machine.states.First(s => s.state.name == name).state;
        var walking = State("Blend Tree");
        var rowingMachine = machine.stateMachines.First(s => s.stateMachine.name == "Rowing").stateMachine;
        var rowing = rowingMachine.states.First(s => s.state.name == "rowing").state;
        var passenger = rowingMachine.states.FirstOrDefault(s => s.state.name == "passenger").state;
        if (passenger == null) passenger = rowingMachine.AddState("passenger", new Vector3(400, 250));
        passenger.motion = Clip("Boat_Idle_Rower");
        passenger.tag = "Boat";
        passenger.iKOnFeet = false;
        if (!controller.parameters.Any(p => p.name == "Passenger"))
            controller.AddParameter("Passenger", AnimatorControllerParameterType.Bool);
        if (!controller.parameters.Any(p => p.name == "ClimbFromWater"))
            controller.AddParameter("ClimbFromWater", AnimatorControllerParameterType.Bool);
        foreach (var transition in machine.GetStateMachineTransitions(rowingMachine))
            machine.RemoveStateMachineTransition(rowingMachine, transition);
        foreach (var transition in rowing.transitions) rowing.RemoveTransition(transition);
        foreach (var transition in passenger.transitions) passenger.RemoveTransition(transition);
        foreach (bool port in new[] { true, false })
        {
            string side = port ? "Left" : "Right";
            var enter = State("ToBoat" + side);
            var exit = State("FromBoat" + side);
            string climbName = "ClimbToBoat" + side;
            var climb = machine.states.FirstOrDefault(s => s.state.name == climbName).state;
            if (climb == null) climb = machine.AddState(climbName, new Vector3(port ? 250 : 500, 550));
            climb.motion = Clip("Boat_Enter_ClimbUpFrom" + side + "Side");
            foreach (var state in new[] { enter, exit, climb, rowing }) { state.tag = "Boat"; state.iKOnFeet = false; }
            // Replace only boat routing; leave locomotion, jump and land intact.
            foreach (var transition in walking.transitions.Where(t => t.destinationState == enter || t.destinationState == climb).ToArray())
                walking.RemoveTransition(transition);
            void Route(AnimatorState destination, bool water)
            {
                var t = walking.AddTransition(destination); Configure(t, false, .12f);
                t.AddCondition(AnimatorConditionMode.Equals, 1, "Locamotion");
                t.AddCondition(port ? AnimatorConditionMode.If : AnimatorConditionMode.IfNot, 0, "PortSide");
                t.AddCondition(water ? AnimatorConditionMode.If : AnimatorConditionMode.IfNot, 0, "ClimbFromWater");
            }
            Route(enter, false); Route(climb, true);
            foreach (var entry in new[] { enter, climb })
            {
                foreach (var transition in entry.transitions) entry.RemoveTransition(transition);
                var toRower = entry.AddTransition(rowing); Configure(toRower, true, .16f);
                toRower.AddCondition(AnimatorConditionMode.IfNot, 0, "Passenger");
                var toPassenger = entry.AddTransition(passenger); Configure(toPassenger, true, .16f);
                toPassenger.AddCondition(AnimatorConditionMode.If, 0, "Passenger");
            }
            foreach (var transition in exit.transitions) exit.RemoveTransition(transition);
            Configure(exit.AddTransition(walking), true, .18f);
            var leave = rowing.AddTransition(exit); Configure(leave, false, .16f);
            leave.AddCondition(AnimatorConditionMode.Equals, 0, "Locamotion");
            leave.AddCondition(port ? AnimatorConditionMode.If : AnimatorConditionMode.IfNot, 0, "PortSide");
            var passengerLeave = passenger.AddTransition(exit); Configure(passengerLeave, false, .16f);
            passengerLeave.AddCondition(AnimatorConditionMode.Equals, 0, "Locamotion");
            passengerLeave.AddCondition(port ? AnimatorConditionMode.If : AnimatorConditionMode.IfNot, 0, "PortSide");
        }
        var becomePassenger = rowing.AddTransition(passenger); Configure(becomePassenger, false, .16f);
        becomePassenger.AddCondition(AnimatorConditionMode.Equals, 1, "Locamotion");
        becomePassenger.AddCondition(AnimatorConditionMode.If, 0, "Passenger");
        var becomeRower = passenger.AddTransition(rowing); Configure(becomeRower, false, .16f);
        becomeRower.AddCondition(AnimatorConditionMode.Equals, 1, "Locamotion");
        becomeRower.AddCondition(AnimatorConditionMode.IfNot, 0, "Passenger");
        foreach (var transition in machine.anyStateTransitions.Where(t => t.conditions.Any(c => c.parameter == "Jump")))
        {
            foreach (var condition in transition.conditions.Where(c => c.parameter == "Locamotion").ToArray()) transition.RemoveCondition(condition);
            transition.AddCondition(AnimatorConditionMode.Equals, 0, "Locamotion");
        }
        EditorUtility.SetDirty(controller);
        AssetDatabase.SaveAssetIfDirty(controller);
    }

    static void Configure(AnimatorStateTransition transition, bool exitTime, float seconds)
    {
        transition.hasExitTime = exitTime;
        transition.exitTime = 1f;
        transition.hasFixedDuration = true;
        transition.duration = seconds;
        transition.offset = 0f;
        transition.interruptionSource = TransitionInterruptionSource.None;
        transition.canTransitionToSelf = false;
    }

    [MenuItem("Island/Boat Tests/Inspect TheOne Boat Animations")]
    public static void Inspect()
    {
        var report = new StringBuilder();
        var controller = AssetDatabase.LoadAssetAtPath<AnimatorController>(ControllerPath);
        report.AppendLine("Controller dirty=" + EditorUtility.IsDirty(controller));
        foreach (var layer in controller.layers)
            foreach (var entry in layer.stateMachine.states)
                report.AppendLine($"{layer.name}/{entry.state.name}: {entry.state.motion?.name}; transitions={entry.state.transitions.Length}");
        var scene = EditorSceneManager.NewPreviewScene();
        var avatar = (GameObject)PrefabUtility.InstantiatePrefab(AssetDatabase.LoadAssetAtPath<GameObject>("Assets/Prefabs/Player.prefab"), scene);
        foreach (var behaviour in avatar.GetComponentsInChildren<MonoBehaviour>()) behaviour.enabled = false;
        var animator = avatar.GetComponent<Animator>();
        avatar.SetActive(true);
        animator.enabled = true;
        animator.cullingMode = AnimatorCullingMode.AlwaysAnimate;
        animator.Rebind();
        animator.runtimeAnimatorController = null;
        animator.applyRootMotion = false;
        var graph = PlayableGraph.Create("Inspect boat clips");
        graph.SetTimeUpdateMode(DirectorUpdateMode.Manual);
        var output = AnimationPlayableOutput.Create(graph, "Pose", animator);
        try
        {
            foreach (string path in Directory.GetFiles(Clips, "Boat_*.fbx").OrderBy(p => p))
            {
                var clip = AssetDatabase.LoadAllAssetsAtPath(path).OfType<AnimationClip>().First(c => !c.name.StartsWith("__"));
                var playable = AnimationClipPlayable.Create(graph, clip);
                playable.SetApplyFootIK(false);
                output.SetSourcePlayable(playable);
                graph.Play();
                report.AppendLine($"{clip.name}: length={clip.length:F3}, loop={clip.isLooping}, scale={animator.humanScale:F4}");
                foreach (double time in new[] { 0d, (double)clip.length - 1d / clip.frameRate })
                {
                    avatar.transform.SetPositionAndRotation(Vector3.zero, Quaternion.identity);
                    playable.SetTime(time); graph.Evaluate(.00001f);
                    var hips = animator.GetBoneTransform(HumanBodyBones.Hips);
                    var foot = animator.GetBoneTransform(HumanBodyBones.LeftFoot);
                    report.AppendLine($"  t={time:F3}: root={avatar.transform.position:F4}, hips={hips.position:F4}, hipsRotation={hips.eulerAngles:F2}, foot={foot.position:F4}, body={animator.bodyPosition:F4}, bodyRotation={animator.bodyRotation.eulerAngles:F2}");
                }
                graph.DestroyPlayable(playable);
            }
        }
        finally { graph.Destroy(); EditorSceneManager.ClosePreviewScene(scene); }
        File.WriteAllText("/tmp/theone-boat-animation-inspection.txt", report.ToString());
        Debug.Log(report.ToString());
    }
}
