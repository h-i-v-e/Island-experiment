using System;
using UnityEditor;
using UnityEditor.Build;
using UnityEditor.Build.Reporting;
using UnityEditor.SceneManagement;
using UnityEngine;
public static class BuildConsumer
{
    public static void Run()
    {
        EditorSceneManager.NewScene(NewSceneSetup.EmptyScene);
        new GameObject("Consumer smoke test").AddComponent<PackageSmokePlayer>();
        EditorSceneManager.SaveScene(UnityEngine.SceneManagement.SceneManager.GetActiveScene(), "Assets/Consumer.unity");
        PlayerSettings.productName = "MotuConsumer";
        UnityEditor.OSXStandalone.UserBuildSettings.architecture = OSArchitecture.ARM64;
        var backend = Environment.GetEnvironmentVariable("MOTU_IL2CPP") == "1"
            ? ScriptingImplementation.IL2CPP : ScriptingImplementation.Mono2x;
        PlayerSettings.SetScriptingBackend(NamedBuildTarget.Standalone, backend);
        var report = BuildPipeline.BuildPlayer(new BuildPlayerOptions {
            scenes = new[] { "Assets/Consumer.unity" }, locationPathName = "Build/MotuConsumer.app",
            target = BuildTarget.StandaloneOSX, options = BuildOptions.Development
        });
        if (report.summary.result != BuildResult.Succeeded) throw new Exception("Consumer build failed: " + report.summary.result);
        if (System.IO.Directory.GetFiles("Build/MotuConsumer.app/Contents", "libmotu.dylib", System.IO.SearchOption.AllDirectories).Length != 1)
            throw new Exception("Consumer build omitted the Motu native plugin.");
        foreach (var guid in AssetDatabase.FindAssets("t:Shader", new[] { "Packages/com.motu.runtime" }))
        {
            var shader = AssetDatabase.LoadAssetAtPath<Shader>(AssetDatabase.GUIDToAssetPath(guid));
            if (ShaderUtil.ShaderHasError(shader)) throw new Exception("Player shader failed: " + shader.name);
        }
        Debug.Log("MOTU CONSUMER BUILD PASSED: " + backend);
    }
}
