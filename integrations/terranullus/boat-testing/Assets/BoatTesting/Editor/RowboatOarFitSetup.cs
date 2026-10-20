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

public static class RowboatOarFitSetup
{
    [MenuItem("Island/Boat Tests/Calibrate Rowboat Grips And Blade Dip")]
    public static void Apply()
    {
        if (EditorApplication.isPlayingOrWillChangePlaymode) throw new InvalidOperationException("Stop Play mode first.");
        var preview = EditorSceneManager.NewPreviewScene();
        var graph = PlayableGraph.Create("Rowboat wrist calibration");
        var report = new StringBuilder();
        try
        {
            var avatar = (GameObject)PrefabUtility.InstantiatePrefab(AssetDatabase.LoadAssetAtPath<GameObject>("Assets/Prefabs/Player.prefab"), preview);
            foreach (var behaviour in avatar.GetComponentsInChildren<MonoBehaviour>()) behaviour.enabled = false;
            var animator = avatar.GetComponent<Animator>();
            animator.runtimeAnimatorController = null;
            animator.applyRootMotion = false;
            animator.cullingMode = AnimatorCullingMode.AlwaysAnimate;
            animator.Rebind();
            avatar.transform.SetPositionAndRotation(Vector3.zero, Quaternion.identity);
            var clip = AssetDatabase.LoadAllAssetsAtPath("Assets/Small_Boat_Anim_Set/Animations/Boat_DoublePaddle_IdleOarInHand.fbx")
                .OfType<AnimationClip>().First(c => !c.name.StartsWith("__"));
            graph.SetTimeUpdateMode(DirectorUpdateMode.Manual);
            var playable = AnimationClipPlayable.Create(graph, clip);
            AnimationPlayableOutput.Create(graph, "Idle wrists", animator).SetSourcePlayable(playable);
            graph.Play(); graph.Evaluate(0);
            var left = animator.GetBoneTransform(HumanBodyBones.LeftHand);
            var right = animator.GetBoneTransform(HumanBodyBones.RightHand);
            var leftPose = new Pose(left.position, left.rotation);
            var rightPose = new Pose(right.position, right.rotation);
            var prefab = PrefabUtility.LoadPrefabContents(BoatTestSceneSetup.BoatPrefab);
            try
            {
                Fit(prefab, animator.humanScale, leftPose, rightPose, false, report);
                PrefabUtility.SaveAsPrefabAsset(prefab, BoatTestSceneSetup.BoatPrefab);
            }
            finally { PrefabUtility.UnloadPrefabContents(prefab); }
            for (int i = 0; i < SceneManager.sceneCount; i++)
            {
                var scene = SceneManager.GetSceneAt(i);
                if (!scene.isLoaded) continue;
                foreach (var boat in scene.GetRootGameObjects().SelectMany(r => r.GetComponentsInChildren<RowboatController>(true)))
                {
                    Fit(boat.gameObject, animator.humanScale, leftPose, rightPose, true, report);
                    EditorSceneManager.MarkSceneDirty(scene);
                }
            }
        }
        finally { graph.Destroy(); EditorSceneManager.ClosePreviewScene(preview); }
        report.AppendLine("Updated wrist rotations and blade-dip settings only. Open scene edits retained and left unsaved.");
        File.WriteAllText("/tmp/rowboat-oar-fit-setup.txt", report.ToString());
        Debug.Log(report.ToString());
    }

    private static void Fit(GameObject boat, float scale, Pose left, Pose right, bool undo, StringBuilder report)
    {
        foreach (var seat in boat.GetComponentsInChildren<BoatSeat>(true).Where(s => s.Role == BoatSeat.SeatRole.Rower))
        {
            void Grip(BoatOar oar, Pose hand)
            {
                if (oar == null || !oar.IsValid || oar.RowlockPin == null)
                    throw new InvalidOperationException("Each rower needs valid oars and rowlock pins.");
                if (undo) Undo.RecordObjects(new UnityEngine.Object[] { oar.HandGrip, oar }, "Calibrate rowboat wrist and dip");
                var desired = BoatBoardingProfile.Multiply(seat.SeatedRoot(scale), hand);
                var aimed = Quaternion.FromToRotation(oar.HandGrip.position - oar.RowlockPin.position,
                    desired.position - oar.transform.position) * oar.Model.rotation;
                // Keep grip position, shaft, pin and rowlock where the designer
                // placed them. Calibrate before extra dip so the wrist follows
                // that additional oar pitch naturally during the stroke.
                oar.HandGrip.localRotation = Quaternion.Inverse(aimed) * desired.rotation;
                EditorUtility.SetDirty(oar.HandGrip);
                EditorUtility.SetDirty(oar);
                if (undo)
                {
                    PrefabUtility.RecordPrefabInstancePropertyModifications(oar.HandGrip);
                    PrefabUtility.RecordPrefabInstancePropertyModifications(oar);
                }
                report.AppendLine($"{boat.name}/{oar.name}: recalibrated wrist, dip {oar.BladeDipDegrees:F1} degrees.");
            }
            Grip(seat.LeftHandOar, left);
            Grip(seat.RightHandOar, right);
        }
    }
}
