using System;
using System.Collections;
using System.Reflection;
using System.Threading;
using System.Threading.Tasks;
using Motu.Interop;
using Motu.Islands;
using Motu.Settings;
using Motu.Streaming;
using Motu.World;
using NUnit.Framework;
using UnityEngine;
using UnityEngine.TestTools;
using Object = UnityEngine.Object;
namespace Motu.Editor
{
    public sealed class EmptySnapshotReproTests
    {
        [UnityTest, Timeout(600000)]
        public IEnumerator CachedSubmergedCellPreparesInstallsAndUnloads()
        {
            const string snapshot = "/Users/jeromejohnson/Library/Application Support/DefaultCompany/island-unity/GeneratedIslandCache/a17f0861699fccacfbbf8e735825d2f2d1609717cc9b741ff59ed5ebedc320a4.motusnapshot";
            var world = new GameObject("Cached submerged world");
            var host = new GameObject("Cached submerged island");
            IslandPreparedData prepared = null;
            var handles = NativeIslandHandle.ActiveCount;
            try
            {
                var environment = world.AddComponent<WorldEnvironmentController>();
                environment.Initialize(new WorldEnvironmentSettings(), new IslandCloudSettings(), 64f, 128f, null);
                var generation = new IslandGenerationSettings { Seed = 17, MaximumHeightMetres = 80f, UseSnapshotCache = false };
                var rendering = new IslandRenderingSettings();
                JsonUtility.FromJsonOverwrite("{\"materialTextureResolution\":16}", rendering);
                var request = new IslandGenerationRequest(17, Vector2Int.zero, generation, new IslandRiverSettings(),
                    new IslandForestSettings(), new IslandReedSettings(), new IslandFernSettings(), rendering, default);
                var pending = Task.Run(() => IslandPreparationPipeline.PrepareIsland(17, request.Options, request.ForestOptions,
                    request.ReedOptions, request.FernOptions, request.WorldSizeMetres, request.MaterialColours,
                    request.MaterialTextureResolution, CancellationToken.None, snapshot));
                while (!pending.IsCompleted) yield return null;
                prepared = pending.GetAwaiter().GetResult();
                Assert.IsTrue(prepared.loadedFromSnapshot);
                Assert.IsNull(prepared.lod3);
                Assert.That(Array.TrueForAll(prepared.overviewTiles, tile => tile == null), Is.True);
                var generator = host.AddComponent<IslandGenerator>();
                generator.ConfigureWorldManagement(environment);
                generator.ApplyRequestProfile(request.Profile);
                var installerType = typeof(IslandGenerator).GetNestedType("IslandRuntimeInstaller", BindingFlags.NonPublic);
                var installer = Activator.CreateInstance(installerType, BindingFlags.Instance | BindingFlags.NonPublic,
                    null, new object[] { generator }, null);
                var installed = (Task)installerType.GetMethod("InstallAsync", BindingFlags.Instance | BindingFlags.NonPublic)
                    .Invoke(installer, new object[] { prepared, request.Descriptor, request.WorldSizeMetres,
                        CancellationToken.None, new UnityFrameBudget(.5f) });
                while (!installed.IsCompleted) yield return null;
                installed.GetAwaiter().GetResult();
                Assert.IsTrue(generator.HasActiveRuntime);
                generator.Runtime.SetViewPosition(Vector3.right * 4000f);
                Assert.IsFalse(generator.Runtime.IsLod3Visible);
                Assert.That(generator.Runtime.Lod3TriangleCount, Is.Zero);
                generator.Runtime.SetDormant(true);
                generator.Runtime.SetDormant(false);
                generator.Runtime.SetViewPosition(Vector3.zero);
                Assert.IsTrue(generator.HasActiveRuntime);
                generator.Clear();
                Assert.That(NativeIslandHandle.ActiveCount, Is.EqualTo(handles));
            }
            finally { prepared?.Dispose(); Object.DestroyImmediate(host); Object.DestroyImmediate(world); }
        }
    }
}
