using System;
using System.IO;
using System.Linq;
using System.Text;
using UnityEditor;
using UnityEditor.SceneManagement;
using UnityEngine;
using UnityEngine.Animations;
using UnityEngine.Playables;
using UnityEngine.SceneManagement;

public static class RowingBoatSeatSetup
{
    const string ProfilePath = "Assets/BoatTesting/RowingBoatBoarding.asset";
    const string PlayerPath = "Assets/Prefabs/Player.prefab";
    const string Animations = "Assets/Small_Boat_Anim_Set/Animations/";

    [MenuItem("Island/Boat Tests/Show Rowboat Seat Targets")]
    public static void ShowTargets()
    {
        var seats = UnityEngine.Object.FindObjectsByType<BoatSeat>();
        if (seats.Length == 0) throw new InvalidOperationException("Open the boat scene and set up its seats first.");
        var bounds = new Bounds(seats[0].transform.position, Vector3.zero);
        foreach (var seat in seats)
        {
            bounds.Encapsulate(seat.transform.position);
            if (seat.LeftApproach != null) bounds.Encapsulate(seat.LeftApproach.position);
            if (seat.RightApproach != null) bounds.Encapsulate(seat.RightApproach.position);
        }
        Selection.activeGameObject = seats.FirstOrDefault(s => s.name == "Rower aft")?.gameObject ?? seats[0].gameObject;
        var view = EditorWindow.GetWindow<SceneView>();
        view.drawGizmos = true;
        view.LookAt(bounds.center, Quaternion.Euler(55, -25, 0), bounds.extents.magnitude * 1.25f);
        view.Focus();
    }

    [MenuItem("Island/Boat Tests/Set Up Rowboat Seats And Boarding Targets")]
    public static void Apply()
    {
        if (EditorApplication.isPlayingOrWillChangePlaymode) throw new InvalidOperationException("Stop Play mode first.");
        var report = new StringBuilder();
        var profile = AssetDatabase.LoadAssetAtPath<BoatBoardingProfile>(ProfilePath);
        if (profile == null) { profile = ScriptableObject.CreateInstance<BoatBoardingProfile>(); AssetDatabase.CreateAsset(profile, ProfilePath); }
        Bake(profile, report);
        ConfigurePassenger();
        TheOneBoatAnimationSetup.ConfigureController();
        var boat = PrefabUtility.LoadPrefabContents(BoatTestSceneSetup.BoatPrefab);
        try
        {
            ConfigureBoat(boat, profile, report);
            PrefabUtility.SaveAsPrefabAsset(boat, BoatTestSceneSetup.BoatPrefab);
        }
        finally { PrefabUtility.UnloadPrefabContents(boat); }
        var player = PrefabUtility.LoadPrefabContents(PlayerPath);
        try
        {
            if (player.GetComponent<BoatBoarding>() == null) player.AddComponent<BoatBoarding>();
            PrefabUtility.SaveAsPrefabAsset(player, PlayerPath);
        }
        finally { PrefabUtility.UnloadPrefabContents(player); }
        var scene = SceneManager.GetSceneByPath(BoatTestSceneSetup.RowingScene);
        if (scene.IsValid() && scene.isLoaded)
        {
            foreach (var root in scene.GetRootGameObjects())
                foreach (var instance in root.GetComponentsInChildren<FloatingShip>(true))
                    if (PrefabUtility.GetPrefabAssetPathOfNearestInstanceRoot(instance.gameObject) == BoatTestSceneSetup.BoatPrefab)
                    {
                        Undo.RegisterFullObjectHierarchyUndo(instance.gameObject, "Add boat seats and boarding targets");
                        ConfigureBoat(instance.gameObject, profile, report);
                    }
            EditorSceneManager.MarkSceneDirty(scene);
        }
        EditorUtility.SetDirty(profile);
        AssetDatabase.SaveAssetIfDirty(profile);
        report.AppendLine("Two rowers and two passengers installed; open scene left unsaved, with other overrides preserved.");
        File.WriteAllText("/tmp/rowboat-seat-setup.txt", report.ToString());
        Debug.Log(report.ToString());
    }

    static void ConfigurePassenger()
    {
        var importer = (ModelImporter)AssetImporter.GetAtPath(Animations + "Boat_Idle_Rower.fbx");
        var clips = importer.clipAnimations;
        foreach (var clip in clips)
        {
            clip.rotationOffset = 90;
            clip.lockRootRotation = clip.lockRootPositionXZ = clip.lockRootHeightY = true;
            clip.keepOriginalOrientation = true;
            clip.keepOriginalPositionXZ = false;
            clip.loopTime = true;
        }
        importer.clipAnimations = clips;
        importer.SaveAndReimport();
    }

    static void ConfigureBoat(GameObject boat, BoatBoardingProfile profile, StringBuilder report)
    {
        var group = boat.transform.Find("Seats");
        if (group == null) { group = new GameObject("Seats").transform; group.SetParent(boat.transform, false); }
        var walk = boat.GetComponentsInChildren<MeshCollider>(true).First(c => !c.convex);
        Physics.SyncTransforms();
        void Seat(string name, float station, BoatSeat.SeatRole role, float yaw)
        {
            var marker = group.Find(name);
            if (marker == null)
            {
                marker = new GameObject(name).transform;
                marker.SetParent(group, false);
                var ray = new Ray(boat.transform.TransformPoint(new Vector3(0, 4, station)), -boat.transform.up);
                if (!walk.Raycast(ray, out var hit, 8)) throw new InvalidOperationException("No seat surface at station " + station);
                var local = boat.transform.InverseTransformPoint(hit.point);
                local.y += .06f;
                marker.localPosition = local;
                marker.localRotation = Quaternion.Euler(0, yaw, 0);
            }
            var seat = marker.GetComponent<BoatSeat>();
            if (seat == null) seat = marker.gameObject.AddComponent<BoatSeat>();
            seat.Configure(role, profile);
            // Preserve authored adjustments when re-running setup.
            if (seat.LeftApproach == null || seat.RightApproach == null) seat.GenerateApproachPoints();
            report.AppendLine($"{name}: hips in boat space {boat.transform.InverseTransformPoint(marker.position):F3}; left {boat.transform.InverseTransformPoint(seat.LeftApproach.position):F3}; right {boat.transform.InverseTransformPoint(seat.RightApproach.position):F3}.");
        }
        Seat("Rower aft", -.55f, BoatSeat.SeatRole.Rower, 180);
        Seat("Rower forward", .55f, BoatSeat.SeatRole.Rower, 180);
        Seat("Passenger aft", -1.65f, BoatSeat.SeatRole.Passenger, 0);
        Seat("Passenger forward", 1.65f, BoatSeat.SeatRole.Passenger, 180);
    }

    public static void Bake(BoatBoardingProfile profile, StringBuilder report)
    {
        var scene = EditorSceneManager.NewPreviewScene();
        var avatar = (GameObject)PrefabUtility.InstantiatePrefab(AssetDatabase.LoadAssetAtPath<GameObject>(PlayerPath), scene);
        foreach (var behaviour in avatar.GetComponentsInChildren<MonoBehaviour>()) behaviour.enabled = false;
        var animator = avatar.GetComponent<Animator>();
        animator.enabled = true;
        animator.runtimeAnimatorController = null;
        animator.applyRootMotion = true;
        animator.cullingMode = AnimatorCullingMode.AlwaysAnimate;
        animator.Rebind();
        profile.referenceHumanScale = animator.humanScale;
        var graph = PlayableGraph.Create("Bake boat root motion");
        graph.SetTimeUpdateMode(DirectorUpdateMode.Manual);
        var output = AnimationPlayableOutput.Create(graph, "Motion", animator);
        AnimationClip Clip(string name) => AssetDatabase.LoadAllAssetsAtPath(Animations + name + ".fbx")
            .OfType<AnimationClip>().First(c => !c.name.StartsWith("__"));
        try
        {
            var idle = AnimationClipPlayable.Create(graph, Clip("Boat_DoublePaddle_IdleOarInHand"));
            output.SetSourcePlayable(idle);
            avatar.transform.SetPositionAndRotation(Vector3.zero, Quaternion.identity);
            graph.Play(); graph.Evaluate(0);
            profile.seatedHips = avatar.transform.InverseTransformPoint(animator.GetBoneTransform(HumanBodyBones.Hips).position) / animator.humanScale;
            graph.DestroyPlayable(idle);
            BoatBoardingProfile.Motion Motion(string name, string state)
            {
                var clip = Clip(name);
                var playable = AnimationClipPlayable.Create(graph, clip);
                playable.SetApplyFootIK(false);
                output.SetSourcePlayable(playable);
                avatar.transform.SetPositionAndRotation(Vector3.zero, Quaternion.identity);
                graph.Evaluate(0);
                avatar.transform.SetPositionAndRotation(Vector3.zero, Quaternion.identity);
                int count = Mathf.CeilToInt(clip.length * 60);
                var motion = new BoatBoardingProfile.Motion { clip = clip, state = "Base Layer." + state,
                    positions = new Vector3[count + 1], rotations = new Quaternion[count + 1] };
                motion.rotations[0] = Quaternion.identity;
                var pose = new Pose(Vector3.zero, Quaternion.identity);
                for (int i = 1; i <= count; i++)
                {
                    graph.Evaluate(clip.length / count);
                    pose.position += animator.deltaPosition;
                    pose.rotation *= animator.deltaRotation;
                    avatar.transform.SetPositionAndRotation(pose.position, pose.rotation);
                    motion.positions[i] = pose.position / animator.humanScale;
                    motion.rotations[i] = pose.rotation;
                }
                graph.DestroyPlayable(playable);
                if (pose.position.magnitude < .3f || pose.position.magnitude > 4)
                    throw new InvalidOperationException("Root-motion bake is invalid for " + name + ": " + pose.position);
                report.AppendLine($"{state}: root travel {pose.position:F3}, rotation {pose.rotation.eulerAngles:F2}, samples {count + 1}.");
                return motion;
            }
            profile.shoreLeft = Motion("Boat_Enter_FromLeftSide", "ToBoatLeft");
            profile.shoreRight = Motion("Boat_Enter_FromRightSide", "ToBoatRight");
            profile.waterLeft = Motion("Boat_Enter_ClimbUpFromLeftSide", "ClimbToBoatLeft");
            profile.waterRight = Motion("Boat_Enter_ClimbUpFromRightSide", "ClimbToBoatRight");
            profile.exitLeft = Motion("Boat_Exit_ToLeftSide", "FromBoatLeft");
            profile.exitRight = Motion("Boat_Exit_ToRightSide", "FromBoatRight");
        }
        finally { graph.Destroy(); EditorSceneManager.ClosePreviewScene(scene); }
    }
}
