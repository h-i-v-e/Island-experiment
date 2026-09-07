using System;
using System.IO;
using System.Linq;
using UnityEditor;
using UnityEditor.Build.Reporting;
using UnityEngine;

namespace Motu.Editor
{
    public static class UnityProjectBuild
    {
        public static void BuildPlayer()
        {
            var output = Environment.GetEnvironmentVariable("MOTU_PLAYER_OUTPUT");
            if (string.IsNullOrWhiteSpace(output)) output = "Builds/Motu.app";
            Directory.CreateDirectory(Path.GetDirectoryName(Path.GetFullPath(output)));
            var report = BuildPipeline.BuildPlayer(new BuildPlayerOptions
            {
                scenes = EditorBuildSettings.scenes.Where(scene => scene.enabled).Select(scene => scene.path).ToArray(),
                locationPathName = output,
                target = BuildTarget.StandaloneOSX,
                options = BuildOptions.Development,
            });
            if (report.summary.result != BuildResult.Succeeded)
                throw new InvalidOperationException("Motu player build failed: " + report.summary.result);
            Debug.Log("MOTU PLAYER BUILD PASSED: " + output);
        }
    }
}
