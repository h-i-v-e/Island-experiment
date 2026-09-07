using System;
using System.Threading;
using UnityEngine;
using UnityEditor;
using UnityEditor.SceneManagement;
using Motu.Islands;
using Motu.Settings;

namespace Motu.Editor
{
    internal static class IslandRequestValidation
    {
        internal static void ValidateSoilAndSnapshots()
        {
            var settings = new IslandGenerationSettings();
            var rivers = new IslandRiverSettings();
            foreach (var invalid in new[] { -1f, float.NaN, float.PositiveInfinity })
            {
                settings.InitialSoilDepthMetres = invalid;
                Require(settings.ToNativeOptions(rivers).initialSoilDepthMetres == 0f,
                    "Invalid soil depth reached native generation.");
            }
            var profile = new IslandGenerationProfile(settings, rivers, new IslandForestSettings(),
                new IslandReedSettings(), new IslandFernSettings(), new IslandRenderingSettings(), new IslandDebugSettings());
            var bare = new IslandGenerationRequest(42, Vector2Int.zero, profile, default);
            profile.Generation.InitialSoilDepthMetres = 2.5f;
            var soil = new IslandGenerationRequest(42, Vector2Int.zero, profile, default);
            Require(soil.Options.initialSoilDepthMetres == 2.5f && bare.Options.initialSoilDepthMetres == 0f
                && soil.SnapshotPath != bare.SnapshotPath, "Soil depth was lost in options, request isolation or cache identity.");
            profile.Generation.InitialSoilDepthMetres = 8f;
            Require(soil.Generation.InitialSoilDepthMetres == 2.5f, "Authored changes mutated an existing request.");

            EditorSceneManager.OpenScene("Assets/Scenes/OpenSeaWorld.unity");
            var factory = UnityEngine.Object.FindFirstObjectByType<CoherentIslandFactory>();
            Require(factory != null, "The open sea scene has no coherent factory.");
            var checkedCells = 0;
            for (var y = -9; y <= 9; y += 2)
            for (var x = -9; x <= 9; x += 2)
            {
                var cell = new Vector2Int(x, y);
                if (!factory.HasIsland(cell)) continue;
                var request = factory.CreateIslandGenerationRequest(cell);
                var repeated = factory.CreateIslandGenerationRequest(cell);
                var heightSample = Mathf.InverseLerp(50f, 500f, request.Generation.MaximumHeightMetres);
                Require(Mathf.Abs(request.Generation.InitialSoilDepthMetres - (1f - heightSample) * 5f) < 0.00001f,
                    "Coherent soil blanket no longer follows its height sample.");
                Require(request.SnapshotPath == repeated.SnapshotPath, "Coherent requests are not deterministic.");
                checkedCells++;
            }
            Require(checkedCells > 0, "Coherent soil fixture contains no islands.");
            EditorSceneManager.NewScene(NewSceneSetup.EmptyScene);
            Debug.Log("Soil policy, request isolation, deterministic factory requests and cache separation passed.");
        }

        internal static void ValidateCancellation()
        {
            using var external = new CancellationTokenSource();
            using var lifecycle = new IslandGenerationLifecycle();
            Require(lifecycle.TryBegin(external.Token, out var first), "Generation could not begin.");
            Require(!lifecycle.TryBegin(CancellationToken.None, out _), "Overlapping generation was accepted.");
            external.Cancel();
            Require(first.IsCancellationRequested, "External cancellation was not forwarded.");
            lifecycle.End(first);
            Require(lifecycle.TryBegin(CancellationToken.None, out var second), "Generation could not restart after cancellation.");
            lifecycle.MarkDestroyed();
            Require(second.IsCancellationRequested, "Teardown did not cancel generation.");
            lifecycle.End(second);
            Require(!lifecycle.TryBegin(CancellationToken.None, out _), "A destroyed lifecycle accepted generation.");
            Debug.Log("Generation cancellation, restart and teardown passed.");
        }

        private static void Require(bool condition, string message)
        {
            if (!condition) throw new InvalidOperationException(message);
        }
    }
}
