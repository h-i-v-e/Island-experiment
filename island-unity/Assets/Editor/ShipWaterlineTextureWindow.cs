using System;
using System.Collections.Generic;
using System.IO;
using Motu.Gameplay;
using Motu.Rendering;
using UnityEditor;
using UnityEditor.SceneManagement;
using UnityEngine;

namespace Motu.Editor
{
    public sealed class ShipWaterlineTextureWindow : EditorWindow
    {
        [SerializeField] private Transform shipRoot;
        [SerializeField] private MeshFilter sourceMesh;
        [SerializeField] private float waterlineWorldY;
        [SerializeField] private Vector3 localBowDirection = Vector3.forward;
        [SerializeField] private ShipWaterlineTextureBuilder.Settings settings = new ShipWaterlineTextureBuilder.Settings();
        private ShipWaterlineTextureBuilder.Result preview;
        private List<ShipWaterlineTextureBuilder.Segment> segments;
        private Vector3 previewForward;
        private Matrix4x4 previewRootMatrix;
        private string message;
        private Texture2D savedTexture;
        private Vector2 scroll;

        [MenuItem("Island/Waterline Texture Generator")]
        public static void Open()
        {
            var window = GetWindow<ShipWaterlineTextureWindow>("Waterline Texture");
            window.minSize = new Vector2(400, 560);
            window.UseSelection();
            window.Show();
        }

        private void OnEnable() => SceneView.duringSceneGui += DrawScenePlane;
        private void OnDisable()
        {
            SceneView.duringSceneGui -= DrawScenePlane;
            ClearPreview();
        }

        private void Update()
        {
            if (preview != null && (shipRoot == null || shipRoot.localToWorldMatrix != previewRootMatrix))
            { ClearPreview(); Repaint(); }
        }

        private void UseSelection()
        {
            var selected = Selection.activeTransform;
            if (selected == null) return;
            var body = selected.GetComponentInParent<Rigidbody>();
            shipRoot = body != null ? body.transform : selected;
            sourceMesh = null;
            var clamp = shipRoot.GetComponent<OceanDeckWaveClamp>();
            if (clamp != null)
            {
                clamp.GetFootprint(out _, out var forward);
                localBowDirection = shipRoot.InverseTransformDirection(forward).normalized;
            }
            else localBowDirection = Vector3.forward;
            waterlineWorldY = EstimateWaterline(shipRoot);
            ClearPreview();
        }

        internal static float EstimateWaterline(Transform root)
        {
            var buoyancy = root.GetComponentInChildren<OceanBuoyancy>();
            if (buoyancy != null)
            {
                var probes = new SerializedObject(buoyancy).FindProperty("probes");
                if (probes.arraySize > 0)
                {
                    var y = 0f;
                    for (var i = 0; i < probes.arraySize; i++)
                        y += buoyancy.transform.TransformPoint(probes.GetArrayElementAtIndex(i).vector3Value).y;
                    return y / probes.arraySize + buoyancy.Draft;
                }
            }
            return 0; // The authored world ocean plane when no buoyancy reference exists.
        }

        private void OnGUI()
        {
            scroll = EditorGUILayout.BeginScrollView(scroll);
            EditorGUILayout.HelpBox("Slice the ship's meshes at its resting waterline. Black clears wave crests over the hull; grey is unchanged water; white adds a speed-driven bow ridge. The texture top points towards the bow.", MessageType.Info);
            if (GUILayout.Button("Use Selected Ship")) UseSelection();
            EditorGUI.BeginChangeCheck();
            shipRoot = (Transform)EditorGUILayout.ObjectField("Ship Root", shipRoot, typeof(Transform), true);
            sourceMesh = (MeshFilter)EditorGUILayout.ObjectField(new GUIContent("Source Mesh (optional)", "Leave empty to slice active MeshFilters and skinned meshes under Ship Root."), sourceMesh, typeof(MeshFilter), true);
            localBowDirection = EditorGUILayout.Vector3Field("Local Bow Direction", localBowDirection);
            waterlineWorldY = EditorGUILayout.FloatField("Waterline World Y", waterlineWorldY);
            using (new EditorGUI.DisabledScope(shipRoot == null))
                if (GUILayout.Button("Estimate Waterline From Buoyancy")) waterlineWorldY = EstimateWaterline(shipRoot);
            settings.resolution = EditorGUILayout.IntPopup("Longest Texture Edge", settings.resolution, new[] { "128", "256", "512", "1024" }, new[] { 128, 256, 512, 1024 });
            settings.clearance = Mathf.Max(0, EditorGUILayout.FloatField("Deck Clearance (m)", settings.clearance));
            settings.blend = Mathf.Max(.01f, EditorGUILayout.FloatField("Edge Blend (m)", settings.blend));
            settings.gapClosure = Mathf.Max(0, EditorGUILayout.FloatField(new GUIContent("Gap Closure (m)", "Seal small mesh cracks before filling. Larger values can merge nearby contours."), settings.gapClosure));
            settings.largestRegionOnly = EditorGUILayout.Toggle(new GUIContent("Largest Region Only", "Ignore small disconnected fittings. Turn off for multiple hulls."), settings.largestRegionOnly);
            settings.bowWave = EditorGUILayout.Toggle("Generate Bow Ridge", settings.bowWave);
            if (settings.bowWave)
            {
                settings.bowOffset = Mathf.Max(0, EditorGUILayout.FloatField("Bow Ridge Offset (m)", settings.bowOffset));
                settings.bowWidth = Mathf.Max(.1f, EditorGUILayout.FloatField("Bow Ridge Width (m)", settings.bowWidth));
            }
            if (EditorGUI.EndChangeCheck()) { ClearPreview(); SceneView.RepaintAll(); }
            if (EditorApplication.isPlaying) EditorGUILayout.HelpBox("Exit Play Mode to author and save the resting hull texture.", MessageType.Info);
            using (new EditorGUI.DisabledScope(shipRoot == null || EditorApplication.isPlaying))
                if (GUILayout.Button("Generate Preview")) GeneratePreview();
            if (!string.IsNullOrEmpty(message)) EditorGUILayout.HelpBox(message, MessageType.Info);
            if (preview != null)
            {
                var rect = GUILayoutUtility.GetRect(1, 260, GUILayout.ExpandWidth(true));
                EditorGUI.DrawPreviewTexture(rect, preview.Texture, null, ScaleMode.ScaleToFit);
                EditorGUILayout.LabelField($"{preview.Texture.width} x {preview.Texture.height} pixels · {preview.TextureBounds.width:F1} x {preview.TextureBounds.height:F1} m");
                EditorGUILayout.LabelField($"{segments.Count} slice segments · {preview.Regions} enclosed region(s)");
                if (GUILayout.Button("Save New Texture & Assign To Ship")) SaveAndAssign();
            }
            if (savedTexture != null) EditorGUILayout.ObjectField("Saved Texture", savedTexture, typeof(Texture2D), false);
            EditorGUILayout.EndScrollView();
        }

        private void GeneratePreview()
        {
            ClearPreview();
            try
            {
                if (sourceMesh != null && sourceMesh.transform != shipRoot && !sourceMesh.transform.IsChildOf(shipRoot))
                    throw new InvalidOperationException("Source Mesh must belong to Ship Root.");
                previewForward = Vector3.ProjectOnPlane(shipRoot.TransformDirection(localBowDirection), Vector3.up).normalized;
                if (previewForward.sqrMagnitude < .01f) throw new InvalidOperationException("Choose a bow direction along the waterline, rather than vertically.");
                EditorUtility.DisplayProgressBar("Ship Waterline Texture", "Slicing mesh triangles and filling the waterline outline…", .3f);
                segments = ShipWaterlineTextureBuilder.Slice(shipRoot, sourceMesh, waterlineWorldY, previewForward);
                preview = ShipWaterlineTextureBuilder.Build(segments, settings);
                previewRootMatrix = shipRoot.localToWorldMatrix;
                message = "Preview ready. Saving fits the texture position, direction and size to this slice. Existing wake texture and wave-height tuning are retained.";
                SceneView.RepaintAll();
            }
            catch (Exception exception) { message = exception.Message; }
            finally { EditorUtility.ClearProgressBar(); }
        }

        private void SaveAndAssign()
        {
            try
            {
                savedTexture = SaveAndAssign(shipRoot, preview, previewForward, waterlineWorldY);
                message = "Saved " + AssetDatabase.GetAssetPath(savedTexture) + ". The scene is marked modified; save it to keep the assignment.";
                EditorGUIUtility.PingObject(savedTexture);
            }
            catch (Exception exception) { message = exception.Message; }
        }

        internal static Texture2D SaveAndAssign(Transform root, ShipWaterlineTextureBuilder.Result result, Vector3 forward, float worldY)
        {
            const string folder = "Assets/Textures";
            if (!AssetDatabase.IsValidFolder(folder)) AssetDatabase.CreateFolder("Assets", "Textures");
            var name = root.name;
            foreach (var invalid in Path.GetInvalidFileNameChars()) name = name.Replace(invalid, '_');
            name = name.Replace('/', '_').Replace('\\', '_');
            var path = AssetDatabase.GenerateUniqueAssetPath($"{folder}/{name} Waterline.png");
            File.WriteAllBytes(path, result.Texture.EncodeToPNG());
            AssetDatabase.ImportAsset(path, ImportAssetOptions.ForceSynchronousImport);
            var texture = ShipDeckWaveClampSetup.LoadLinearTexture(path);
            var component = root.GetComponent<OceanDeckWaveClamp>() ?? Undo.AddComponent<OceanDeckWaveClamp>(root.gameObject);
            Undo.RecordObject(component, "Assign generated waterline texture");
            var centre2D = result.TextureBounds.center;
            var centre = root.position + Vector3.Cross(Vector3.up, forward)*centre2D.x + forward*centre2D.y;
            centre.y = worldY;
            component.FitGeneratedHullTexture(texture, centre, forward, result.SectionBounds.size, result.TextureBounds.size);
            EditorUtility.SetDirty(component);
            PrefabUtility.RecordPrefabInstancePropertyModifications(component);
            EditorSceneManager.MarkSceneDirty(root.gameObject.scene);
            return texture;
        }

        private void ClearPreview()
        {
            if (preview?.Texture != null) DestroyImmediate(preview.Texture);
            preview = null;
            segments = null;
            message = null;
        }

        private void DrawScenePlane(SceneView view)
        {
            if (shipRoot == null || EditorApplication.isPlaying) return;
            var centre = shipRoot.position;
            centre.y = waterlineWorldY;
            var span = 15f;
            foreach (var renderer in shipRoot.GetComponentsInChildren<Renderer>())
                span = Mathf.Max(span, renderer.bounds.extents.magnitude);
            Handles.color = new Color(0, .8f, 1, .7f);
            var a = centre+new Vector3(-span, 0, -span);
            var b = centre+new Vector3(span, 0, -span);
            var c = centre+new Vector3(span, 0, span);
            var d = centre+new Vector3(-span, 0, span);
            Handles.DrawAAPolyLine(a,b,c,d,a);
            Handles.Label(centre, $"Waterline {waterlineWorldY:F2} m");
            EditorGUI.BeginChangeCheck();
            var moved = Handles.Slider(centre, Vector3.up, HandleUtility.GetHandleSize(centre), Handles.ArrowHandleCap, .05f);
            if (EditorGUI.EndChangeCheck()) { waterlineWorldY = moved.y; ClearPreview(); Repaint(); }
            if (segments == null) return;
            var origin = shipRoot.position;
            origin.y = waterlineWorldY;
            var right = Vector3.Cross(Vector3.up, previewForward);
            Handles.color = Color.yellow;
            foreach (var segment in segments)
                Handles.DrawLine(origin+right*segment.A.x+previewForward*segment.A.y,
                    origin+right*segment.B.x+previewForward*segment.B.y);
        }
    }
}
