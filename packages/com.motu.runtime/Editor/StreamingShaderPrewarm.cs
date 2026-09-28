using Motu.World;
using UnityEditor;
using UnityEngine;

namespace Motu.Editor
{
    // Editor Game view compiles unseen variants on demand and draws them cyan
    // in the meantime. Compile the coarse/detail transition variants before
    // Play starts, while no floating bodies are being simulated.
    [InitializeOnLoad]
    internal static class StreamingShaderPrewarm
    {
        private const string VariantPath =
            "Packages/com.motu.runtime/Runtime/Resources/Motu/Variants/";

        static StreamingShaderPrewarm()
        {
            EditorApplication.playModeStateChanged += OnPlayModeStateChanged;
        }

        private static void OnPlayModeStateChanged(PlayModeStateChange state)
        {
            if (state != PlayModeStateChange.ExitingEditMode
                || Object.FindAnyObjectByType<IslandWorldManager>() == null)
            {
                return;
            }

            Compile(AssetDatabase.LoadAssetAtPath<Material>(VariantPath + "TerrainLod1.mat"));
            Compile(AssetDatabase.LoadAssetAtPath<Material>(VariantPath + "TerrainLod2.mat"));
            Compile(AssetDatabase.LoadAssetAtPath<Material>(VariantPath + "TreeWoodLod1.mat"));

            // These first appear after LOD 1, often while the boat is moving.
            // Compile their visible passes before physics starts as well.
            CompileShader("Motu/Terrain Unified");
            CompileShader("Motu/Terrain Grass");
            CompileShader("Motu/Tree Wood");
            CompileShader("Motu/Tree Foliage");
            CompileShader("Motu/Tree Foliage Distant");
            CompileShader("Motu/Riverbank Reeds");
            CompileShader("Motu/Forest Ferns");
            CompileShader("Motu/Rock Decoration");
            CompileShader("Motu/River Water");
            CompileShader("Motu/Planar Reflection Simplified", allPasses: true);
            CompileShader("Hidden/Motu/Reflection Fog");
        }

        private static void CompileShader(string name, bool allPasses = false)
        {
            var shader = Shader.Find(name);
            if (shader == null) return;
            var material = new Material(shader);
            try
            {
                var passCount = allPasses ? material.passCount : 1;
                for (var pass = 0; pass < passCount; pass++) Compile(material, pass);
            }
            finally { Object.DestroyImmediate(material); }
        }

        private static void Compile(Material material, int pass = 0)
        {
            if (material != null && material.shader != null
                && !ShaderUtil.IsPassCompiled(material, pass))
            {
                ShaderUtil.CompilePass(material, pass, true);
            }
        }
    }
}
