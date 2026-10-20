using System;
using System.Reflection;
using Motu.Gameplay;
using NUnit.Framework;
using UnityEngine;
using Object = UnityEngine.Object;

namespace Motu.Editor
{
    public sealed class OceanRollTests
    {
        private const BindingFlags Fields = BindingFlags.Instance | BindingFlags.NonPublic;

        private sealed class Hull : IDisposable
        {
            internal readonly GameObject Root = new GameObject("Wave roll fixture");
            internal readonly Rigidbody Body;
            internal readonly OceanBuoyancy Buoyancy;
            internal readonly ShipHullGeometry Geometry;
            internal readonly Vector3[] Probes = {
                new Vector3(-2, 0, -4), new Vector3(2, 0, -4),
                new Vector3(-2, 0, 0), new Vector3(2, 0, 0),
                new Vector3(-2, 0, 4), new Vector3(2, 0, 4)
            };

            internal Hull()
            {
                Root.AddComponent<BoxCollider>().size = new Vector3(4, 4, 10);
                Body = Root.AddComponent<Rigidbody>();
                Body.mass = 82000;
                Body.centerOfMass = new Vector3(0, -.75f, 0);
                Buoyancy = Root.AddComponent<OceanBuoyancy>();
                Buoyancy.enabled = false;
                typeof(OceanBuoyancy).GetMethod("Awake", Fields).Invoke(Buoyancy, null);
                Geometry = ScriptableObject.CreateInstance<ShipHullGeometry>();
                typeof(ShipHullGeometry).GetField("sections", Fields).SetValue(Geometry,
                    new[] { new ShipHullGeometry.Section {
                        station = 0, length = 10,
                        outline = new[] { new Vector2(-2,-2), new Vector2(2,-2), new Vector2(2,2), new Vector2(-2,2) }
                    }});
                typeof(OceanBuoyancy).GetField("hullGeometry", Fields).SetValue(Buoyancy, Geometry);
                typeof(OceanBuoyancy).GetField("probes", Fields).SetValue(Buoyancy, Probes);
            }

            internal void Samples(Func<Vector3, float> surface, Func<int, float> confidence = null)
            {
                var samples = new OceanBuoyancy.WaterSampleFilter[Probes.Length];
                for (int i = 0; i < samples.Length; i++)
                    samples[i].Update(surface(Buoyancy.PhysicsProbePosition(Probes[i])),
                        confidence?.Invoke(i) ?? 1, .02f, 0);
                typeof(OceanBuoyancy).GetField("waterSamples", Fields).SetValue(Buoyancy, samples);
            }

            internal void Forces() => typeof(OceanBuoyancy).GetMethod("ApplyHullForces", Fields)
                .Invoke(Buoyancy, new object[] { 9.81f });

            public void Dispose() { Object.DestroyImmediate(Root); Object.DestroyImmediate(Geometry); }
        }

        [TestCase(0f, 0f)]
        [TestCase(8192f, 0f)]
        [TestCase(-8192f, 37f)]
        public void PlaneFitRetainsWaveSlopeWithTranslatedAndRotatedHull(float origin, float yaw)
        {
            using var hull = new Hull();
            Vector3 centre = new Vector3(origin, 0, -origin);
            hull.Root.transform.SetPositionAndRotation(centre, Quaternion.Euler(11, yaw, -12));
            hull.Body.position = centre; hull.Body.rotation = hull.Root.transform.rotation;
            Physics.SyncTransforms();
            hull.Samples(p => 1.25f + .15f * (p.x - centre.x) - .06f * (p.z - centre.z));
            Vector3 query = centre + new Vector3(2, 3, -2);
            Assert.IsTrue(hull.Buoyancy.TryWaterPlane(query, out var height, out var normal, out var confidence));
            Assert.That(height, Is.EqualTo(1.67f).Within(.001f));
            Assert.That(Vector3.Angle(normal, new Vector3(-.15f, 1, .06f).normalized), Is.LessThan(.03f));
            Assert.That(confidence, Is.EqualTo(1).Within(.00001f));
        }

        [Test]
        public void MissingSideSamplesDoNotInventAnUnconstrainedCrossBeamSlope()
        {
            using var hull = new Hull();
            hull.Samples(p => 1 + .2f * p.z, i => i % 2 == 0 ? 1 : 0);
            Assert.IsTrue(hull.Buoyancy.TryWaterPlane(Vector3.zero, out var height, out var normal, out _));
            Assert.That(height, Is.EqualTo(1).Within(.00001f));
            Assert.That(normal.x, Is.Zero.Within(.00001f));
            Assert.That(Vector3.Angle(normal, new Vector3(0, 1, -.2f).normalized), Is.LessThan(.02f));
            hull.Samples(_ => 100, _ => 0);
            Assert.IsFalse(hull.Buoyancy.TryWaterPlane(Vector3.zero, out _, out _, out _));
        }

        [TestCase(.25f)]
        [TestCase(-.25f)]
        public void BeamWaveMovesBuoyancyToTheHigherSideAndExcitesRoll(float slope)
        {
            using var hull = new Hull();
            hull.Samples(p => slope * p.x);
            hull.Forces();
            Assert.That(hull.Buoyancy.DisplacedVolume, Is.EqualTo(80).Within(.001f));
            Assert.That(hull.Buoyancy.CentreOfBuoyancy.x * Mathf.Sign(slope), Is.GreaterThan(.15f));
            Assert.That(hull.Body.GetAccumulatedTorque().z * Mathf.Sign(slope), Is.GreaterThan(10000));
        }

        private static float RollTorque(float forwardSpeed, float waterCurrent, float rollRate, float boost = 1)
        {
            using var hull = new Hull();
            hull.Samples(_ => 0);
            hull.Body.linearVelocity = Vector3.forward * forwardSpeed;
            hull.Body.angularVelocity = Vector3.forward * rollRate;
            hull.Buoyancy.WaterCurrent = Vector3.forward * waterCurrent;
            hull.Buoyancy.ForwardSpeedMultiplier = boost;
            hull.Forces();
            return hull.Body.GetAccumulatedTorque().z;
        }

        [TestCase(.2f)]
        [TestCase(-.2f)]
        public void KeelDampingOpposesRollAndGrowsWithActualThroughWaterSpeed(float rollRate)
        {
            float stopped = RollTorque(0, 0, rollRate);
            float slow = RollTorque(2, 0, rollRate);
            float fast = RollTorque(8, 0, rollRate);
            Assert.That(stopped * rollRate, Is.LessThan(0));
            Assert.That(slow * rollRate, Is.LessThan(stopped * rollRate));
            Assert.That(fast * rollRate, Is.LessThan(slow * rollRate));
            Assert.That(RollTorque(-8, 0, rollRate), Is.EqualTo(fast).Within(.01f));
            Assert.That(RollTorque(8, 8, rollRate), Is.EqualTo(stopped).Within(.01f));
            Assert.That(RollTorque(8, 0, rollRate, 4), Is.EqualTo(fast).Within(.01f));
        }

        [Test]
        public void ForwardFlowDoesNotCreateAnUprightingTorqueOnANonRollingHull()
        {
            Assert.That(RollTorque(8, 0, 0), Is.Zero.Within(.001f));
            using var hull = new Hull();
            hull.Samples(_ => -10);
            hull.Body.angularVelocity = Vector3.forward;
            hull.Body.linearVelocity = Vector3.forward * 8;
            hull.Forces();
            Assert.That(hull.Body.GetAccumulatedTorque(), Is.EqualTo(Vector3.zero));
        }
    }
}
