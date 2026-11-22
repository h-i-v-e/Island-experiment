using System.Collections;
using System.Threading;
using Motu.Interop;
using Motu.Islands;
using Motu.Navigation;
using Motu.Settings;
using Motu.Towns;
using NUnit.Framework;
using UnityEngine;
using UnityEngine.AI;
using UnityEngine.TestTools;

namespace Motu.Editor
{
    public sealed class RoadsideFieldNavigationTests
    {
        [UnityTest]
        public IEnumerator ClosedFieldGateAndWallsBlockNavigation()
        {
            var host = new GameObject("Roadside field navigation fixture");
            try
            {
                host.transform.SetPositionAndRotation(new Vector3(32f, 5f, -20f), Quaternion.Euler(0, 55f, 0));
                var runtime = host.AddComponent<IslandRuntime>();
                var plan = new TownRoadsideFieldPlan {
                    fields = new[] { new TownRoadsideField { id = 1, outline = Points(-8, -4, 8, -4, 8, 9, -8, 9) } },
                    walls = new[] { new TownRoadsideWall { points = Points(.92f, -4, 8, -4, 8, 9, -8, 9, -8, -4, -.92f, -4) } },
                    gates = new[] { new TownRoadsideGate { field_id = 1, left = Points(-.92f, -4), right = Points(.92f, -4) } },
                };
                var town = TownRenderer.Install(host.transform, new TownPreparedData { roadsideFields = plan }, 2000f);
                runtime.SetTownRenderer(town);
                Assert.That(town.RoadsideFieldCount, Is.EqualTo(1));
                Assert.That(town.RoadsideBoundaryNavigation, Has.Count.EqualTo(6));

                var ground = IslandNavigationTests.Ground();
                for (var n = 0; n < ground.vertices.Length; n++) ground.vertices[n].y = 2f;
                var root = new GameObject("Island Navigation");
                root.transform.SetParent(host.transform, false);
                var navigation = root.AddComponent<IslandNavigation>();
                var task = navigation.BuildAsync(ground, null, null, IslandPreparedCaves.Empty,
                    new IslandNavigationSettings { LogBuildStatistics = false }, CancellationToken.None);
                while (!task.IsCompleted) yield return null;
                task.GetAwaiter().GetResult();

                var filter = new NavMeshQueryFilter { agentTypeID = navigation.AgentTypeId, areaMask = NavMesh.AllAreas };
                Vector3 World(float x, float z) => host.transform.TransformPoint(new Vector3(x, 2f, z));
                Assert.That(NavMesh.SamplePosition(World(4, -4), out _, .2f, filter), Is.False);
                Assert.That(NavMesh.SamplePosition(World(0, -4), out _, .2f, filter), Is.False);
                var path = new NavMeshPath();
                NavMesh.CalculatePath(World(4, -8), World(4, 3), filter, path);
                Assert.That(path.status, Is.Not.EqualTo(NavMeshPathStatus.PathComplete));
            }
            finally { UnityEngine.Object.DestroyImmediate(host); }
        }

        private static float[] Points(params float[] xy)
        {
            var points = new float[xy.Length / 2 * 3];
            for (int source = 0, target = 0; source < xy.Length; source += 2, target += 3)
            {
                points[target] = 1000f + xy[source];
                points[target + 1] = 1000f + xy[source + 1];
                points[target + 2] = 2f;
            }
            return points;
        }
    }
}
