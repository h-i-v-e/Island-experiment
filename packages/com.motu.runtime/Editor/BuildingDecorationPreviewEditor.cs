using System;
using System.Text.RegularExpressions;
using Motu.Towns;
using UnityEditor;
using UnityEditor.SceneManagement;
using UnityEngine;
using UnityEngine.Rendering;

namespace Motu.Editor
{
    [CustomEditor(typeof(BuildingDecorationPreview))]
    public sealed class BuildingDecorationPreviewEditor : UnityEditor.Editor
    {
        private int anchorIndex;
        private int variantIndex;
        private GameObject trialPrefab;
        private bool tryAxes;
        private TownDecorationAxis facing, forward;
        private string status;
        private MessageType statusType;

        [MenuItem("Motu/Towns/New Building Decoration Preview")]
        public static void NewScene()
        {
            if (EditorApplication.isPlayingOrWillChangePlaymode) return;
            if (!Application.isBatchMode && !EditorSceneManager.SaveCurrentModifiedScenesIfUserWantsTo()) return;
            EditorSceneManager.NewScene(NewSceneSetup.EmptyScene, NewSceneMode.Single);
            var preview = new GameObject("Building Decoration Preview", typeof(BuildingDecorationPreview));

            var camera = new GameObject("Preview Camera", typeof(Camera)).GetComponent<Camera>();
            camera.transform.SetParent(preview.transform, false);
            camera.clearFlags = CameraClearFlags.SolidColor;
            camera.backgroundColor = new Color(.32f, .38f, .43f);
            camera.farClipPlane = 250f;
            PositionCamera(camera, new Bounds(Vector3.up * 2f, new Vector3(8f, 5f, 8f)));

            var floor = GameObject.CreatePrimitive(PrimitiveType.Plane);
            floor.name = "Preview Floor";
            floor.transform.position = Vector3.down * .03f;
            floor.transform.localScale = new Vector3(8f, 1f, 8f);
            DestroyImmediate(floor.GetComponent<Collider>());

            var light = new GameObject("Sun", typeof(Light)).GetComponent<Light>();
            light.type = LightType.Directional;
            light.intensity = 1.25f;
            light.shadows = LightShadows.Soft;
            light.transform.rotation = Quaternion.Euler(47f, -38f, 0f);
            RenderSettings.sun = light;
            RenderSettings.ambientMode = AmbientMode.Flat;
            RenderSettings.ambientLight = new Color(.50f, .54f, .58f);
            Selection.activeGameObject = preview;
        }

        public override void OnInspectorGUI()
        {
            var preview = (BuildingDecorationPreview)target;
            using (new EditorGUI.DisabledScope(EditorApplication.isPlayingOrWillChangePlaymode))
            {
                bool settingsChanged = DrawDefaultInspector();
                using (new EditorGUILayout.HorizontalScope())
                {
                    using (new EditorGUI.DisabledScope(preview.Recipe == null))
                        if (GUILayout.Button("Build Preview")) Build(preview);
                    using (new EditorGUI.DisabledScope(!preview.HasBuilding))
                    {
                        if (GUILayout.Button("Frame Building")) FrameBuilding(preview);
                        if (GUILayout.Button("Clear"))
                        {
                            preview.Clear();
                            status = null;
                            SceneView.RepaintAll();
                        }
                    }
                }
                if (preview.Anchors.Count > 0) DrawDecorations(preview, settingsChanged);
            }
            if (!string.IsNullOrEmpty(status)) EditorGUILayout.HelpBox(status, statusType);
        }

        internal static void BuildRecipe(BuildingDecorationPreview preview)
        {
            var bindings = preview.MaterialCatalogue;
            if (bindings == null) bindings = Resources.Load<TownRecipeCatalogue>("Towns/DefaultTownCatalogue");
            // All detailed surfaces and anchors still come from the recipe. Keep the unused distant bake small.
            var json = Regex.Replace(preview.Recipe.text, "(\"atlas_size\"\\s*:\\s*)[0-9]+", "${1}128");
            var request = bindings.CaptureBuildingPreview(json, out var dependencies);
            var data = TownNative.Generate(request, TownPrefabBaker.PreviewInput(42), dependencies);
            if (data.prototypes.Length != 1)
                throw new InvalidOperationException("Unity could not build a preview from this recipe.");
            preview.SetBuilding(data.prototypes[0]);
        }

        private void Build(BuildingDecorationPreview preview)
        {
            try
            {
                BuildRecipe(preview);
                anchorIndex = 0;
                ResetTrial();
                RefreshDecorations(preview);
                FrameBuilding(preview);
            }
            catch (Exception error)
            {
                status = error.Message;
                statusType = MessageType.Error;
            }
        }

        private void DrawDecorations(BuildingDecorationPreview preview, bool settingsChanged)
        {
            var anchors = preview.Anchors;
            anchorIndex = Mathf.Clamp(anchorIndex, 0, anchors.Count - 1);
            var labels = new string[anchors.Count];
            for (var index = 0; index < labels.Length; index++)
                labels[index] = $"{anchors[index].Label} ({anchors[index].Id})";
            EditorGUI.BeginChangeCheck();
            int nextAnchor = EditorGUILayout.Popup("Anchor", anchorIndex, labels);
            if (nextAnchor != anchorIndex)
            {
                anchorIndex = nextAnchor;
                ResetTrial();
            }
            DrawVariant(preview.Catalogue, anchors[anchorIndex].Label);
            trialPrefab = (GameObject)EditorGUILayout.ObjectField("Try Prefab", trialPrefab, typeof(GameObject), false);
            tryAxes = EditorGUILayout.Toggle("Try Axes", tryAxes);
            if (tryAxes)
            {
                facing = (TownDecorationAxis)EditorGUILayout.EnumPopup("Facing Axis", facing);
                forward = (TownDecorationAxis)EditorGUILayout.EnumPopup("Forward Axis", forward);
            }
            bool changed = EditorGUI.EndChangeCheck();
            using (new EditorGUILayout.HorizontalScope())
            {
                if (GUILayout.Button("Refresh Decorations") || changed || settingsChanged) RefreshDecorations(preview);
                if (GUILayout.Button("Frame Anchor")) FrameAnchor(preview.AnchorPose(anchorIndex));
            }
        }

        private void DrawVariant(TownDecorationCatalogue catalogue, string label)
        {
            int count = catalogue != null ? catalogue.CountPrefabs(label) : 0;
            if (count == 0) return;
            var variants = new string[count];
            for (var index = 0; index < count; index++)
                variants[index] = catalogue.SelectPrefab(label, (uint)index).prefab.name;
            variantIndex = EditorGUILayout.Popup("Catalogue Variant", Mathf.Clamp(variantIndex, 0, count - 1), variants);
        }

        private void RefreshDecorations(BuildingDecorationPreview preview)
        {
            Quaternion? axes = tryAxes ? TownDecorationPrefabAxes.RotationOffset(facing, forward) : null;
            int count = preview.ShowDecorations(anchorIndex, (uint)variantIndex, trialPrefab, axes);
            status = preview.Anchors.Count == 0
                ? "Add a decoration anchor in Building Studio, then rebuild the preview."
                : count == 0 ? "Choose a prefab to try or add a matching label to the catalogue."
                    : $"Showing {count} of {preview.Anchors.Count} anchors. Trial settings apply to this preview.";
            statusType = MessageType.Info;
            SceneView.RepaintAll();
        }

        private void ResetTrial()
        {
            variantIndex = 0;
            trialPrefab = null;
            tryAxes = false;
            facing = forward = TownDecorationAxis.Default;
        }

        private static void FrameBuilding(BuildingDecorationPreview preview)
        {
            var bounds = preview.BuildingBounds();
            var camera = preview.GetComponentInChildren<Camera>();
            if (camera != null) PositionCamera(camera, bounds);
            SceneView.lastActiveSceneView?.Frame(bounds, false);
        }

        private static void PositionCamera(Camera camera, Bounds bounds)
        {
            var direction = new Vector3(.75f, .42f, -1f).normalized;
            var position = bounds.center + direction * (Mathf.Max(2f, bounds.extents.magnitude) * 3f);
            camera.transform.SetPositionAndRotation(position, Quaternion.LookRotation(bounds.center - position));
        }

        private static void FrameAnchor(Pose pose)
        {
            var normal = pose.rotation * Vector3.up;
            var forwardDirection = pose.rotation * Vector3.forward;
            SceneView.lastActiveSceneView?.LookAt(pose.position,
                Quaternion.LookRotation(-normal, forwardDirection), 1.6f, false);
        }

        private void OnSceneGUI()
        {
            var preview = (BuildingDecorationPreview)target;
            if ((uint)anchorIndex >= (uint)preview.Anchors.Count || Event.current.type != EventType.Repaint) return;
            var pose = preview.AnchorPose(anchorIndex);
            float size = HandleUtility.GetHandleSize(pose.position) * .22f;
            using (new Handles.DrawingScope(new Color(.24f, .84f, 1f)))
            {
                var direction = Quaternion.LookRotation(pose.rotation * Vector3.up, pose.rotation * Vector3.forward);
                Handles.ArrowHandleCap(0, pose.position, direction, size, EventType.Repaint);
                Handles.Label(pose.position + pose.rotation * Vector3.forward * size, preview.Anchors[anchorIndex].Label);
            }
            using (new Handles.DrawingScope(new Color(1f, .69f, .25f)))
                Handles.ArrowHandleCap(0, pose.position, pose.rotation, size, EventType.Repaint);
        }
    }
}
