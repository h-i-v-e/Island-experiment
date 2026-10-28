using System.Collections;
using System.Reflection;
using Motu.Rendering;
using Motu.World;
using NUnit.Framework;
using UnityEngine;
using UnityEngine.TestTools;
using Object = UnityEngine.Object;
namespace Motu.Editor
{
    public sealed class OceanHullSprayHeelTests
    {
        private const BindingFlags Hidden = BindingFlags.Instance | BindingFlags.NonPublic;
        private static void Set(object target, string name, object value)
            => target.GetType().GetField(name, Hidden).SetValue(target, value);

        [TestCase(90f)]
        [TestCase(-90f)]
        [TestCase(180f)]
        public void SidewaysOrCapsizedHullsDoNotProjectDistantSpray(float heel)
        {
            var up = Quaternion.Euler(0, 0, heel) * Vector3.up;
            Assert.That(OceanHullSpray.TryProjectEmissionOrigin(new Vector3(4, -2, 8), up, 0, out _), Is.False);
        }

        [TestCase(0f, 0f, 0f, false)]
        [TestCase(35f, 12f, 0f, false)]
        [TestCase(-35f, -12f, 0f, false)]
        [TestCase(55f, 18f, 4f, false)]
        [TestCase(35f, 12f, 0f, true)]
        public void EmittedOriginsMeetSeaPlaneWithoutMovingAcrossHull(float heel, float pitch, float seaLevel, bool imported)
        {
            var ship = new GameObject("Hull spray heel fixture");
            var oceanRoot = new GameObject("Offset ocean fixture");
            var sprayMaterial = new Material(Shader.Find("Motu/Waterfall Spray Particle"));
            try
            {
                var import = imported ? Quaternion.Euler(-90, 0, 0) : Quaternion.identity;
                ship.transform.rotation = import;
                ship.transform.localScale = Vector3.one * (imported ? 1500 : 1);
                var worldLoop = new[] { new Vector3(-4, 0, -10), new Vector3(4, 0, -10),
                    new Vector3(4, 0, 10), new Vector3(-4, 0, 10) };
                var local = new Vector3[worldLoop.Length];
                for (int i = 0; i < local.Length; i++) local[i] = ship.transform.InverseTransformPoint(worldLoop[i]);
                var spray = ship.AddComponent<OceanHullSpray>();
                spray.SetWaterline(local, sprayMaterial); spray.enabled = false;
                var localUp = ship.transform.InverseTransformDirection(Vector3.up);
                ship.transform.SetPositionAndRotation(new Vector3(13, -2, 19), Quaternion.Euler(pitch, 73, heel) * import);
                oceanRoot.transform.position = Vector3.up * seaLevel;
                var ocean = oceanRoot.AddComponent<OceanSurfaceController>();
                ocean.Install(new Material(Shader.Find("Motu/Sea Water")), 128, true);
                Set(spray, "ocean", ocean);
                var points = new Vector3[local.Length];
                for (int i = 0; i < local.Length; i++) points[i] = ship.transform.TransformPoint(local[i]);
                Set(spray, "worldPoints", points);
                Set(spray, "depths", new[] { 1f, 1f, 1f, 1f });
                Set(spray, "waveVelocities", new Vector3[4]);
                Set(spray, "strengthMultipliers", new float[4]);
                Set(spray, "fractions", new float[4]);
                Set(spray, "valid", new[] { true, true, true, true });
                Set(spray, "segments", new OceanHullSpray.WetSegment[4]);
                Set(spray, "velocityPerMetre", 0f);
                typeof(OceanHullSpray).GetMethod("CreateParticles", Hidden).Invoke(spray, null);
                typeof(OceanHullSpray).GetMethod("EmitSegments", Hidden).Invoke(spray, new object[] { .05f });
                var snapshot = new ParticleSystem.Particle[2500];
                int count = spray.Particles.GetParticles(snapshot);
                Assert.That(count, Is.GreaterThan(10));
                for (int i = 0; i < count; i++)
                {
                    Assert.That(snapshot[i].position.y, Is.EqualTo(seaLevel).Within(.001f));
                    var projected = ship.transform.InverseTransformPoint(snapshot[i].position);
                    projected -= localUp * Vector3.Dot(projected - local[0], localUp);
                    float distance = float.PositiveInfinity;
                    for (int edge = 0; edge < local.Length; edge++)
                    {
                        var a = local[edge]; var b = local[(edge + 1) % local.Length];
                        var t = Mathf.Clamp01(Vector3.Dot(projected - a, b - a) / (b - a).sqrMagnitude);
                        distance = Mathf.Min(distance, ship.transform.TransformVector(projected - Vector3.Lerp(a, b, t)).magnitude);
                    }
                    Assert.That(distance, Is.LessThan(.002f), "The emitter must retain its hull-local position along the authored up axis.");
                }
            }
            finally
            {
                Object.DestroyImmediate(ship);
                Object.DestroyImmediate(oceanRoot);
                Object.DestroyImmediate(sprayMaterial);
            }
        }

        [UnityTest]
        public IEnumerator LiveGpuSprayRemainsOnHullWhileMovingThroughBothHeelDirections()
        {
            yield return new EnterPlayMode();
            var ship = new GameObject("Moving heeled spray fixture");
            var oceanRoot = new GameObject("Raised flat sea fixture");
            var material = new Material(Shader.Find("Motu/Waterfall Spray Particle"));
            try
            {
                const float seaLevel = 4f;
                oceanRoot.transform.position = Vector3.up * seaLevel;
                var ocean = oceanRoot.AddComponent<OceanSurfaceController>();
                ocean.Install(new Material(Shader.Find("Motu/Sea Water")), 128, true);
                var weather = ocean.Weather;
                weather.Enabled = false;
                ocean.ApplyWaveWeather(weather);
                var body = ship.AddComponent<Rigidbody>(); body.isKinematic = true;
                var spray = ship.AddComponent<OceanHullSpray>();
                var local = new[] { new Vector3(-4, 0, -10), new Vector3(4, 0, -10),
                    new Vector3(4, 0, 10), new Vector3(-4, 0, 10) };
                spray.SetWaterline(local, material);
                Set(spray, "ocean", ocean);
                Set(spray, "maximumSpeed", 0f);
                var snapshot = new ParticleSystem.Particle[2500];
                int checkedParticles = 0, positiveFrames = 0, negativeFrames = 0;
                for (int frame = 0; frame < 120; frame++)
                {
                    float heel = Mathf.Sin(frame * Mathf.PI / 50f) * 40f;
                    float pitch = Mathf.Cos(frame * Mathf.PI / 60f) * 10f;
                    ship.transform.SetPositionAndRotation(new Vector3(frame * .06f, seaLevel - 2f, frame * .03f),
                        Quaternion.Euler(pitch, frame * .6f, heel));
                    yield return null;
                    if (frame < 20) continue;
                    // Refresh through the actual sampler, then inspect fresh
                    // emissions before gravity or later hull movement acts.
                    typeof(OceanHullSpray).GetMethod("LateUpdate", Hidden).Invoke(spray, null);
                    spray.Particles.Clear();
                    typeof(OceanHullSpray).GetMethod("EmitSegments", Hidden).Invoke(spray, new object[] { .05f });
                    int count = spray.Particles.GetParticles(snapshot);
                    if (count == 0) continue;
                    positiveFrames += heel > 10f ? 1 : 0;
                    negativeFrames += heel < -10f ? 1 : 0;
                    for (int i = 0; i < count; i++)
                    {
                        Assert.That(snapshot[i].position.y, Is.EqualTo(seaLevel).Within(.001f));
                        var point = ship.transform.InverseTransformPoint(snapshot[i].position);
                        float edgeError = Mathf.Min(Mathf.Abs(Mathf.Abs(point.x) - 4f), Mathf.Abs(Mathf.Abs(point.z) - 10f));
                        Assert.That(edgeError, Is.LessThan(.002f), "Fresh spray remains on the moving hull's sides.");
                    }
                    checkedParticles += count;
                }
                Assert.That(checkedParticles, Is.GreaterThan(500));
                Assert.That(positiveFrames, Is.GreaterThan(10));
                Assert.That(negativeFrames, Is.GreaterThan(10));
                Debug.Log($"Moving heel spray: {checkedParticles} fresh origins verified across 120 frames, both heel directions, pitch and translation.");
            }
            finally
            {
                Object.Destroy(ship); Object.Destroy(oceanRoot); Object.Destroy(material);
            }
            yield return null;
            yield return new ExitPlayMode();
        }
    }
}
