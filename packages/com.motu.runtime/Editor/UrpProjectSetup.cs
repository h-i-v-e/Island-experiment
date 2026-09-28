using System;
using Motu.Rendering;
using UnityEditor;
using UnityEditor.Rendering;
using UnityEditor.Rendering.Universal;
using UnityEngine;
using UnityEngine.Rendering;
using UnityEngine.Rendering.Universal;

namespace Motu.Editor
{
    public static class UrpProjectSetup
    {
        public const string PipelinePath = "Assets/Settings/MotuURP.asset";

        [MenuItem("Motu/Rendering/Configure URP")]
        public static void Configure()
        {
            if (!AssetDatabase.IsValidFolder("Assets/Settings")) AssetDatabase.CreateFolder("Assets", "Settings");
            var main = Renderer("MotuRenderer", false);
            var reflection = Renderer("MotuReflectionRenderer", true);
            var pipeline = AssetDatabase.LoadAssetAtPath<UniversalRenderPipelineAsset>(PipelinePath);
            if (pipeline == null)
            {
                pipeline = UniversalRenderPipelineAsset.Create(main);
                AssetDatabase.CreateAsset(pipeline, PipelinePath);
            }
            pipeline.supportsCameraDepthTexture = true;
            pipeline.supportsCameraOpaqueTexture = true;
            pipeline.msaaSampleCount = 2;
            pipeline.supportsHDR = true;
            pipeline.shadowDistance = 150;
            pipeline.shadowCascadeCount = 4;
            var serialized = new SerializedObject(pipeline);
            serialized.FindProperty("m_MainLightShadowsSupported").boolValue = true;
            serialized.FindProperty("m_AdditionalLightShadowsSupported").boolValue = true;
            serialized.FindProperty("m_SoftShadowsSupported").boolValue = true;
            var renderers = serialized.FindProperty("m_RendererDataList");
            renderers.arraySize = 2;
            renderers.GetArrayElementAtIndex(0).objectReferenceValue = main;
            renderers.GetArrayElementAtIndex(1).objectReferenceValue = reflection;
            serialized.FindProperty("m_OpaqueDownsampling").intValue = 0;
            serialized.FindProperty("m_AdditionalLightsPerObjectLimit").intValue = 8;
            serialized.ApplyModifiedPropertiesWithoutUndo();
            GraphicsSettings.defaultRenderPipeline = pipeline;
            var quality = QualitySettings.GetQualityLevel();
            for (var index = 0; index < QualitySettings.names.Length; index++)
            {
                QualitySettings.SetQualityLevel(index, false);
                QualitySettings.renderPipeline = pipeline;
            }
            QualitySettings.SetQualityLevel(quality, false);
            // Preserve texture, normal, metallic, smoothness, emission and transparency through Unity's upgrader.
            foreach (var guid in AssetDatabase.FindAssets("t:Material", new[] { "Assets" }))
            {
                var material = AssetDatabase.LoadAssetAtPath<Material>(AssetDatabase.GUIDToAssetPath(guid));
                if (material.shader.name is "Standard" or "Standard (Specular setup)")
                {
                    new StandardUpgrader(material.shader.name).Upgrade(material, MaterialUpgrader.UpgradeFlags.None);
                    EditorUtility.SetDirty(material);
                }
            }
            EditorUtility.SetDirty(pipeline);
            AssetDatabase.SaveAssets();
            Debug.Log("MOTU URP CONFIGURED");
        }

        private static UniversalRendererData Renderer(string name, bool reflection)
        {
            var path = $"Assets/Settings/{name}.asset";
            var renderer = AssetDatabase.LoadAssetAtPath<UniversalRendererData>(path);
            if (renderer == null)
            {
                renderer = ScriptableObject.CreateInstance<UniversalRendererData>();
                AssetDatabase.CreateAsset(renderer, path);
                var feature = ScriptableObject.CreateInstance<MotuRendererFeature>();
                feature.name = "Motu Rendering";
                var featureSettings = new SerializedObject(feature);
                featureSettings.FindProperty("simplifiedReflections").boolValue = reflection;
                featureSettings.ApplyModifiedPropertiesWithoutUndo();
                AssetDatabase.AddObjectToAsset(feature, renderer);
                renderer.rendererFeatures.Add(feature);
            }
            renderer.renderingMode = RenderingMode.Forward;
            renderer.copyDepthMode = CopyDepthMode.AfterOpaques;
            renderer.intermediateTextureMode = IntermediateTextureMode.Always;
            renderer.opaqueLayerMask = reflection ? 0 : ~0;
            renderer.SetDirty();
            EditorUtility.SetDirty(renderer);
            return renderer;
        }
    }
}
