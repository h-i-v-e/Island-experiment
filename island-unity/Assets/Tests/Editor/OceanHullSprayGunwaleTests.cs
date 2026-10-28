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
    public sealed class OceanHullSprayGunwaleTests
    {
        const BindingFlags Hidden = BindingFlags.Instance | BindingFlags.NonPublic;
        static readonly Vector3[] Loop = { new Vector3(-4, 0, -10), new Vector3(4, 0, -10),
            new Vector3(4, 0, 10), new Vector3(-4, 0, 10) };
        static void Set(object target, string name, object value) => target.GetType().GetField(name, Hidden).SetValue(target, value);
        static void Invoke(object target, string name, params object[] args) => target.GetType().GetMethod(name, Hidden).Invoke(target, args);

        [Test]
        public void IndividualHeightsInterpolateAcrossEveryEdgeIncludingClosingSeam()
        {
            var ship = new GameObject("Individual rail profile");
            try
            {
                var spray = ship.AddComponent<OceanHullSpray>(); spray.SetWaterline(Loop, null);
                Set(spray, "gunwaleHeight", 2f);
                Assert.That(spray.HasGunwaleProfile, Is.False);
                Assert.That(spray.GunwaleHeightAlongEdge(3, .25f), Is.EqualTo(2));
                Set(spray, "gunwaleHeights", new[] { 1f, 3f, 5f, 7f });
                Assert.That(spray.HasGunwaleProfile, Is.True);
                Assert.That(spray.GetGunwaleHeight(0), Is.EqualTo(1));
                Assert.That(spray.GetGunwaleHeight(3), Is.EqualTo(7));
                Assert.That(spray.GunwaleHeightAlongEdge(0, .25f), Is.EqualTo(1.5f));
                Assert.That(spray.GunwaleHeightAlongEdge(1, .5f), Is.EqualTo(4));
                Assert.That(spray.GunwaleHeightAlongEdge(3, .25f), Is.EqualTo(5.5f), "The final edge interpolates back to point zero.");
                Set(spray, "gunwaleHeights", new[] { 9f, 10f });
                Assert.That(spray.HasGunwaleProfile, Is.False);
                Assert.That(spray.GetGunwaleHeight(0), Is.EqualTo(2), "A profile for another sample count cannot leave a partially applied rail.");
            }
            finally { Object.DestroyImmediate(ship); }
        }

        [TestCase(-35f, -12f)]
        [TestCase(0f, 0f)]
        [TestCase(35f, 12f)]
        public void OriginFollowsWaveUntilRailAndNeverRisesAboveIt(float heel, float pitch)
        {
            var rotation = Quaternion.Euler(pitch, 63, heel);
            var up = rotation * Vector3.up;
            var point = new Vector3(15, -2, -8) + rotation * new Vector3(-4, 0, 3);
            foreach (float rise in new[] { .5f, 1.999f, 2f, 3f, 100f })
            {
                float wave = point.y + up.y * rise;
                Assert.That(OceanHullSpray.TryProjectCappedEmissionOrigin(point, up, wave, 2, out var origin, out var over), Is.True);
                Assert.That(Vector3.Distance(origin, point + up * Mathf.Min(rise, 2)), Is.LessThan(.0001f));
                Assert.That(over, Is.EqualTo(rise >= 2));
                if (!over) Assert.That(origin.y, Is.EqualTo(wave).Within(.0001f));
            }
        }

        [TestCase(-90f)]
        [TestCase(90f)]
        [TestCase(180f)]
        public void CapsizedHullDoesNotCreateDistantDeckSpray(float heel)
        {
            Assert.That(OceanHullSpray.TryProjectCappedEmissionOrigin(Vector3.zero, Quaternion.Euler(0, 0, heel) * Vector3.up,
                8, 2, out _, out var over), Is.False);
            Assert.That(over, Is.False);
        }

        [TestCase(-35f, -12f, false, false)]
        [TestCase(35f, 12f, false, false)]
        [TestCase(35f, 12f, true, false)]
        [TestCase(-35f, -12f, false, true)]
        [TestCase(35f, 12f, false, true)]
        [TestCase(35f, 12f, true, true)]
        public void ActualEmissionsCapAtRailAndTurnInwardOnlyOnOvertoppedEdges(float heel, float pitch, bool imported, bool individual)
        {
            var ship = new GameObject("Capped hull spray fixture");
            var material = new Material(Shader.Find("Motu/Waterfall Spray Particle"));
            try
            {
                var import = imported ? Quaternion.Euler(-90, 0, 0) : Quaternion.identity;
                ship.transform.rotation = import; ship.transform.localScale = Vector3.one * (imported ? 1500 : 1);
                var local = new Vector3[4];
                for (int i = 0; i < 4; i++) local[i] = ship.transform.InverseTransformPoint(Loop[i]);
                var spray = ship.AddComponent<OceanHullSpray>(); spray.SetWaterline(local, material); spray.enabled = false;
                ship.transform.SetPositionAndRotation(new Vector3(13, -1, 19), Quaternion.Euler(pitch, 73, heel) * import);
                float localCap = 2f / ship.transform.localScale.y;
                Configure(spray, localCap);
                if (individual) Set(spray, "gunwaleHeights", new[] { localCap * .5f, localCap * 1.5f, localCap, localCap * 2 });
                var points = new Vector3[4]; var depths = new float[4];
                for (int i = 0; i < 4; i++) { points[i] = ship.transform.TransformPoint(local[i]); depths[i] = -points[i].y; }
                Set(spray, "worldPoints", points); Set(spray, "depths", depths);
                Set(spray, "waveVelocities", new Vector3[4]); Set(spray, "strengthMultipliers", new float[4]);
                Set(spray, "fractions", new float[4]); Set(spray, "valid", new[] { true, true, true, true });
                Set(spray, "segments", new OceanHullSpray.WetSegment[4]);
                Invoke(spray, "CreateParticles"); Invoke(spray, "EmitSegments", .05f);
                var snapshot = new ParticleSystem.Particle[2500]; int count = spray.Particles.GetParticles(snapshot);
                Assert.That(count, Is.GreaterThan(30));
                CheckFresh(spray, ship.transform, local, snapshot, count, out int inward, out int outward);
                Assert.That(inward, Is.GreaterThan(0)); Assert.That(outward, Is.GreaterThan(0));
                // A submerged rail remains the origin even for a much higher crest.
                for (int i = 0; i < 4; i++) depths[i] += 20;
                spray.Particles.Clear(); Invoke(spray, "EmitSegments", .05f);
                count = spray.Particles.GetParticles(snapshot);
                CheckFresh(spray, ship.transform, local, snapshot, count, out inward, out outward);
                Assert.That(inward, Is.EqualTo(count)); Assert.That(outward, Is.Zero);
            }
            finally { Object.DestroyImmediate(ship); Object.DestroyImmediate(material); }
        }
        static void Configure(OceanHullSpray spray, float height)
        {
            Set(spray, "gunwaleOverflow", true); Set(spray, "gunwaleHeight", height);
            Set(spray, "inwardLaunchAngle", 10f); Set(spray, "launchAngle", 20f); Set(spray, "directionSpread", 0f);
        }
        static void CheckFresh(OceanHullSpray spray, Transform ship, Vector3[] local, ParticleSystem.Particle[] snapshot,
            int count, out int inward, out int outward)
        {
            inward = outward = 0;
            var localUp = (Vector3)typeof(OceanHullSpray).GetField("waterlineUp", Hidden).GetValue(spray);
            var up = ship.TransformDirection(localUp).normalized;
            float tolerance = .0005f / ship.TransformVector(localUp).magnitude;
            for (int i = 0; i < count; i++)
            {
                var projected = ship.InverseTransformPoint(snapshot[i].position);
                float rise = Vector3.Dot(projected - local[0], localUp);
                projected -= localUp * rise;
                int nearest = -1; float nearestT = 0, error = float.PositiveInfinity;
                for (int edge = 0; edge < 4; edge++)
                {
                    var a = local[edge]; var b = local[(edge + 1) % 4];
                    float t = Mathf.Clamp01(Vector3.Dot(projected - a, b - a) / (b - a).sqrMagnitude);
                    float distance = ship.TransformVector(projected - Vector3.Lerp(a, b, t)).magnitude;
                    if (distance < error) { error = distance; nearest = edge; nearestT = t; }
                }
                Assert.That(error, Is.LessThan(.002f), "Origins retain their hull-relative outline.");
                var tangent = ship.TransformDirection(local[(nearest + 1) % 4] - local[nearest]).normalized;
                var normal = Vector3.Cross(up, tangent).normalized;
                float across = Vector3.Dot(snapshot[i].velocity, normal);
                float localCap = Mathf.Lerp(spray.GetGunwaleHeight(nearest), spray.GetGunwaleHeight((nearest + 1) % 4), nearestT);
                Assert.That(rise, Is.LessThanOrEqualTo(localCap + tolerance));
                if (Mathf.Abs(rise - localCap) < tolerance)
                { Assert.That(across, Is.LessThan(0), "Water at the gunwale travels across the deck."); inward++; }
                else
                { Assert.That(across, Is.GreaterThan(0), "Lower spray retains its outward direction."); outward++; }
            }
        }

        [UnityTest]
        public IEnumerator LiveWavesSwitchAcrossGunwaleWhileShipMovesAndHeels()
        {
            yield return new EnterPlayMode();
            var ship = new GameObject("Moving overtopping hull");
            var seaRoot = new GameObject("Flat native ocean");
            var material = new Material(Shader.Find("Motu/Waterfall Spray Particle"));
            try
            {
                var sea = seaRoot.AddComponent<OceanSurfaceController>();
                sea.Install(new Material(Shader.Find("Motu/Sea Water")), 256, true);
                var weather = sea.Weather; weather.Enabled = false; sea.ApplyWaveWeather(weather);
                var spray = ship.AddComponent<OceanHullSpray>(); spray.SetWaterline(Loop, material);
                Set(spray, "ocean", sea); Configure(spray, 2);
                Set(spray, "gunwaleHeights", new[] { 1.25f, 2.5f, 3.5f, 1.75f });
                var snapshot = new ParticleSystem.Particle[2500];
                int checkedParticles = 0, inwardFrames = 0, outwardFrames = 0;
                for (int frame = 0; frame < 120; frame++)
                {
                    ship.transform.SetPositionAndRotation(new Vector3(frame * .05f, -1, frame * .025f),
                        Quaternion.Euler(Mathf.Cos(frame * .05f) * 10, frame * .6f, Mathf.Sin(frame * .08f) * 38));
                    yield return null;
                    if (frame < 15) continue;
                    Invoke(spray, "LateUpdate"); spray.Particles.Clear(); Invoke(spray, "EmitSegments", .05f);
                    int count = spray.Particles.GetParticles(snapshot);
                    CheckFresh(spray, ship.transform, Loop, snapshot, count, out int inward, out int outward);
                    checkedParticles += count; if (inward > 0) inwardFrames++; if (outward > 0) outwardFrames++;
                }
                Assert.That(checkedParticles, Is.GreaterThan(500));
                Assert.That(inwardFrames, Is.GreaterThan(20)); Assert.That(outwardFrames, Is.GreaterThan(20));
                Debug.Log($"Gunwale spray: {checkedParticles} fresh emissions checked across 120 moving/rolling frames; inward={inwardFrames}, outward={outwardFrames}.");
                ship.transform.position = Vector3.up * 40; spray.Particles.Clear();
                for (int frame = 0; frame < 30; frame++) yield return null;
                Assert.That(spray.Particles.particleCount, Is.Zero, "A dry hull cannot keep pouring water over the rail.");
            }
            finally { Object.Destroy(ship); Object.Destroy(seaRoot); Object.Destroy(material); }
            yield return null; yield return new ExitPlayMode();
        }
    }
}
