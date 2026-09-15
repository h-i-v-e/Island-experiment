using UnityEditor;
using UnityEngine;
using Motu.Islands;
using Motu.Settings;
using Motu.World;

namespace Motu.Editor
{
    [CustomEditor(typeof(IslandGenerator))]
    public sealed class IslandGeneratorEditor : UnityEditor.Editor
    {
        public override void OnInspectorGUI()
        {
            serializedObject.Update();
            DrawPropertiesExcluding(serializedObject, "m_Script");
            serializedObject.ApplyModifiedProperties();
            var island = (IslandGenerator)target;
            EditorGUILayout.Space();
            EditorGUILayout.HelpBox(
                "IslandGenerator is a runtime consumer. Island parameters and per-cell customization belong to the assigned IIslandGenerationRequestFactory.",
                MessageType.Info);
            EditorGUILayout.LabelField("Runtime Status", island.Status);
        }
    }

    [CustomEditor(typeof(OceanWaveProfile))]
    public sealed class OceanWaveProfileEditor : UnityEditor.Editor
    {
        public override void OnInspectorGUI()
        {
            serializedObject.Update();

            EditorGUILayout.HelpBox(
                "Tune the broad swells first, then use Wave Shape Noise to break up "
                + "regular crest lines. Fine vertex spacing affects performance; the "
                + "directional wave and noise controls affect appearance.",
                MessageType.Info);

            DrawPropertiesExcluding(serializedObject, "m_Script");
            serializedObject.ApplyModifiedProperties();

            var profile = (OceanWaveProfile)target;
            var settings = profile.ToRuntimeSettings();
            EditorGUILayout.Space();
            EditorGUILayout.LabelField("Calculated Limits", EditorStyles.boldLabel);
            using (new EditorGUI.DisabledScope(true))
            {
                EditorGUILayout.FloatField(
                    "Max Height From Mean",
                    settings.MaximumVerticalDisplacement);
                EditorGUILayout.FloatField(
                    "Max Crest To Trough",
                    settings.MaximumVerticalDisplacement * 2f);
                EditorGUILayout.FloatField(
                    "Maximum Horizontal Shift",
                    settings.MaximumHorizontalDisplacement);
            }

            EditorGUILayout.Space();
            using (new EditorGUI.DisabledScope(!Application.isPlaying))
            {
                if (GUILayout.Button("Apply To Running Ocean"))
                {
                    ApplyToRunningOceans(settings);
                }
            }
            if (!Application.isPlaying)
            {
                EditorGUILayout.HelpBox(
                    "Enter Play mode to apply changes immediately. Otherwise the "
                    + "profile is read the next time the ocean is installed.",
                    MessageType.None);
            }
        }

        private static void ApplyToRunningOceans(OceanWaveRuntimeSettings settings)
        {
            var oceans = Object.FindObjectsByType<OceanSurfaceController>(
                FindObjectsInactive.Include);
            var applied = 0;
            foreach (var ocean in oceans)
            {
                if (ocean.SurfaceMaterial == null)
                {
                    continue;
                }
                ocean.ApplyWaveSettings(settings);
                applied++;
            }

            Debug.Log($"Applied ocean-wave profile to {applied} running ocean surface(s).");
        }
    }
}
