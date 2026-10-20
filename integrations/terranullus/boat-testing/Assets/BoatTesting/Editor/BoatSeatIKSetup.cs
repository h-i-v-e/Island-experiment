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
using UnityEngine.SceneManagement;

public static class BoatSeatIKSetup
{
    [MenuItem("Island/Boat Tests/Set Up Rowboat Seat And Oar IK")]
    public static void Apply()
    {
        if (EditorApplication.isPlayingOrWillChangePlaymode) throw new InvalidOperationException("Stop Play mode first.");
        var report = new StringBuilder();
        var controller = AssetDatabase.LoadAssetAtPath<AnimatorController>("Assets/TheOne.controller");
        Undo.RecordObject(controller, "Enable seated IK pass");
        var layers = controller.layers;
        layers[0].iKPass = true;
        controller.layers = layers;
        EditorUtility.SetDirty(controller);
        AssetDatabase.SaveAssetIfDirty(controller);
        var avatarScene = EditorSceneManager.NewPreviewScene();
        var avatar = (GameObject)PrefabUtility.InstantiatePrefab(AssetDatabase.LoadAssetAtPath<GameObject>("Assets/Prefabs/Player.prefab"), avatarScene);
        foreach (var behaviour in avatar.GetComponentsInChildren<MonoBehaviour>()) behaviour.enabled = false;
        var animator = avatar.GetComponent<Animator>();
        animator.enabled = true;
        animator.cullingMode = AnimatorCullingMode.AlwaysAnimate;
        animator.runtimeAnimatorController = null;
        animator.applyRootMotion = false;
        animator.Rebind();
        avatar.transform.SetPositionAndRotation(Vector3.zero, Quaternion.identity);
        var graph = PlayableGraph.Create("Calibrate oar grips");
        graph.SetTimeUpdateMode(DirectorUpdateMode.Manual);
        try
        {
            var clip = AssetDatabase.LoadAllAssetsAtPath("Assets/Small_Boat_Anim_Set/Animations/Boat_DoublePaddle_IdleOarInHand.fbx")
                .OfType<AnimationClip>().First(c => !c.name.StartsWith("__"));
            var playable = AnimationClipPlayable.Create(graph, clip);
            AnimationPlayableOutput.Create(graph, "Pose", animator).SetSourcePlayable(playable);
            graph.Play(); graph.Evaluate(0);
            var left = animator.GetBoneTransform(HumanBodyBones.LeftHand);
            var right = animator.GetBoneTransform(HumanBodyBones.RightHand);
            var leftPose = new Pose(left.position, left.rotation);
            var rightPose = new Pose(right.position, right.rotation);
            var prefab = PrefabUtility.LoadPrefabContents(BoatTestSceneSetup.BoatPrefab);
            try
            {
                Configure(prefab, animator.humanScale, leftPose, rightPose, report);
                PrefabUtility.SaveAsPrefabAsset(prefab, BoatTestSceneSetup.BoatPrefab);
            }
            finally { PrefabUtility.UnloadPrefabContents(prefab); }
            var scene = SceneManager.GetSceneByPath(BoatTestSceneSetup.RowingScene);
            if (scene.IsValid() && scene.isLoaded)
            {
                foreach (var root in scene.GetRootGameObjects())
                    foreach (var boat in root.GetComponentsInChildren<FloatingShip>(true))
                        if (PrefabUtility.GetPrefabAssetPathOfNearestInstanceRoot(boat.gameObject) == BoatTestSceneSetup.BoatPrefab)
                        {
                            Undo.RegisterFullObjectHierarchyUndo(boat.gameObject, "Set up seat and oar IK");
                            Configure(boat.gameObject, animator.humanScale, leftPose, rightPose, report);
                        }
                EditorSceneManager.MarkSceneDirty(scene);
            }
        }
        finally { graph.Destroy(); EditorSceneManager.ClosePreviewScene(avatarScene); }
        report.AppendLine("Pelvis IK enabled for all seats; hand IK and four animated oar rigs assigned to the two rowers. Open scene left unsaved.");
        File.WriteAllText("/tmp/boat-seat-ik-setup.txt", report.ToString());
        Debug.Log(report.ToString());
    }

    static void Configure(GameObject boat, float humanScale, Pose leftHand, Pose rightHand, StringBuilder report)
    {
        var group = boat.transform.Find("Oar IK targets");
        if (group == null) { group = new GameObject("Oar IK targets").transform; group.SetParent(boat.transform, false); }
        var available = boat.GetComponentsInChildren<MeshFilter>(true).Where(m => m.name.StartsWith("SM_Prop_Oar")).ToList();
        foreach (var seat in boat.GetComponentsInChildren<BoatSeat>(true).Where(s => s.Role == BoatSeat.SeatRole.Rower))
        {
            var pair = available.OrderBy(m => Mathf.Abs(boat.transform.InverseTransformPoint(m.transform.position).z
                - boat.transform.InverseTransformPoint(seat.transform.position).z)).Take(2).ToArray();
            if (pair.Length != 2) throw new InvalidOperationException("Each rower needs two oars.");
            BoatOar left = seat.LeftHandOar, right = seat.RightHandOar;
            foreach (var mesh in pair)
            {
                available.Remove(mesh);
                Endpoints(mesh, out var handle, out var blade);
                bool isLeft = seat.transform.InverseTransformPoint(handle).x < 0;
                var assigned = isLeft ? left : right;
                if (assigned != null && assigned.IsValid) { EnsureRowlockPin(assigned); continue; }
                var pivot = new GameObject(seat.name + (isLeft ? " left rowlock" : " right rowlock")).transform;
                pivot.SetParent(group, false);
                pivot.SetPositionAndRotation(mesh.transform.position, boat.transform.rotation);
                var grip = mesh.transform.Find("Hand grip");
                if (grip == null) { grip = new GameObject("Hand grip").transform; grip.SetParent(mesh.transform, false); }
                grip.position = handle + (blade - handle).normalized * .08f;
                var hand = isLeft ? leftHand : rightHand;
                var root = seat.SeatedRoot(humanScale);
                var desired = BoatBoardingProfile.Multiply(root, hand);
                var aimed = Quaternion.FromToRotation(grip.position - pivot.position, desired.position - pivot.position) * mesh.transform.rotation;
                grip.localRotation = Quaternion.Inverse(aimed) * desired.rotation;
                var rig = pivot.gameObject.AddComponent<BoatOar>();
                rig.Configure(mesh.transform, grip);
                EnsureRowlockPin(rig);
                if (isLeft) left = rig; else right = rig;
                report.AppendLine($"{seat.name} {(isLeft ? "left hand" : "right hand")}: {mesh.name}, rowlock {boat.transform.InverseTransformPoint(pivot.position):F3}, grip {boat.transform.InverseTransformPoint(grip.position):F3}.");
            }
            if (left == null || right == null) throw new InvalidOperationException("Could not assign both hand sides for " + seat.name);
            seat.ConfigureOars(left, right);
            if (PrefabUtility.IsPartOfPrefabInstance(seat)) PrefabUtility.RecordPrefabInstancePropertyModifications(seat);
        }
    }

    public static void EnsureRowlockPin(BoatOar rig)
    {
        if (!rig.IsValid || rig.RowlockPin != null) return;
        var pin = rig.Model.Find("Rowlock pin");
        if (pin == null)
        {
            pin = new GameObject("Rowlock pin").transform;
            pin.SetParent(rig.Model, false);
            pin.position = rig.transform.position;
        }
        rig.ConfigurePin(pin);
        if (PrefabUtility.IsPartOfPrefabInstance(rig)) PrefabUtility.RecordPrefabInstancePropertyModifications(rig);
    }

    [MenuItem("Island/Boat Tests/Inspect Rowboat Oar Geometry")]
    public static void Inspect()
    {
        var boat = UnityEngine.Object.FindAnyObjectByType<FloatingShip>();
        if (boat == null) throw new InvalidOperationException("Open the boat scene first.");
        var report = new StringBuilder();
        foreach (var mesh in boat.GetComponentsInChildren<MeshFilter>(true).Where(m => m.name.StartsWith("SM_Prop_Oar")))
        {
            Endpoints(mesh, out var handle, out var blade);
            report.AppendLine($"{mesh.name} local={boat.transform.InverseTransformPoint(mesh.transform.position):F4}, rotation={mesh.transform.localEulerAngles:F2}, scale={mesh.transform.lossyScale:F3}, meshBounds={mesh.sharedMesh.bounds}; handle={boat.transform.InverseTransformPoint(handle):F4}, blade={boat.transform.InverseTransformPoint(blade):F4}.");
        }
        File.WriteAllText("/tmp/boat-oar-inspection.txt", report.ToString());
        Debug.Log(report.ToString());
    }

    static void Endpoints(MeshFilter mesh, out Vector3 handle, out Vector3 blade)
    {
        var bounds = mesh.sharedMesh.bounds;
        var size = bounds.extents;
        var axis = size.x >= size.y && size.x >= size.z ? Vector3.right * size.x
            : size.y >= size.z ? Vector3.up * size.y : Vector3.forward * size.z;
        var a = mesh.transform.TransformPoint(bounds.center - axis);
        var b = mesh.transform.TransformPoint(bounds.center + axis);
        var boat = mesh.GetComponentInParent<FloatingShip>().transform;
        bool aInside = Mathf.Abs(boat.InverseTransformPoint(a).x) < Mathf.Abs(boat.InverseTransformPoint(b).x);
        handle = aInside ? a : b;
        blade = aInside ? b : a;
    }
}
