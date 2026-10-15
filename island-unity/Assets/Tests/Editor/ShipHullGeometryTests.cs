using Motu.Gameplay;
using NUnit.Framework;
using UnityEngine;

namespace Motu.Editor
{
    public sealed class ShipHullGeometryTests
    {
        private static ShipHullGeometry.Section Box => new ShipHullGeometry.Section
        {
            station = 0f, length = 10f,
            outline = new[] { new Vector2(-2, -2), new Vector2(2, -2), new Vector2(2, 2), new Vector2(-2, 2) },
        };

        [Test]
        public void HalfSubmergedBoxExcludesWaterCapFromWettedPerimeter()
        {
            var wet = ShipHullGeometry.Immerse(Box, Vector3.one, Vector3.up, 0, new Vector2[8]);
            Assert.That(wet.area * Box.length, Is.EqualTo(80).Within(.0001f));
            Assert.That(wet.wettedPerimeter * Box.length, Is.EqualTo(80).Within(.0001f));
            Assert.That(wet.waterlineWidth, Is.EqualTo(4).Within(.0001f));
            Assert.That(wet.centre.y, Is.EqualTo(-1).Within(.0001f));
        }

        [TestCase(-75f)] [TestCase(-45f)] [TestCase(45f)] [TestCase(75f)]
        public void HeelMovesBuoyancyCentreWhilePreservingSymmetricHalfVolume(float degrees)
        {
            var normal = Quaternion.Inverse(Quaternion.Euler(0, 0, degrees)) * Vector3.up;
            var wet = ShipHullGeometry.Immerse(Box, Vector3.one, normal, 0, new Vector2[8]);
            Assert.That(wet.area, Is.EqualTo(8).Within(.0001f));
            Assert.That(Vector3.Dot(wet.centre, normal), Is.LessThan(0));
            Assert.That(wet.centre.x, Is.Not.EqualTo(0).Within(.01f));
        }

        [Test]
        public void DraftAndScaleChangeDisplacementAndResistanceGeometry()
        {
            var shallow = ShipHullGeometry.Immerse(Box, Vector3.one, Vector3.up, -1, new Vector2[8]);
            var deep = ShipHullGeometry.Immerse(Box, Vector3.one, Vector3.up, 1, new Vector2[8]);
            Assert.That(deep.area, Is.GreaterThan(shallow.area));
            Assert.That(deep.wettedPerimeter, Is.GreaterThan(shallow.wettedPerimeter));
            var scaled = ShipHullGeometry.Immerse(Box, Vector3.one * 2, Vector3.up, 0, new Vector2[8]);
            Assert.That(scaled.area, Is.EqualTo(32).Within(.0001f));
            Assert.That(scaled.wettedPerimeter, Is.EqualTo(16).Within(.0001f));
            var dry = ShipHullGeometry.Immerse(Box, Vector3.one, Vector3.up, -3, new Vector2[8]);
            Assert.That(dry.area, Is.Zero);
        }

        [Test]
        public void FrictionUsesActualReynoldsNumberAndHasFiniteLowSpeedLimit()
        {
            // 5 m/s, 40 m, viscosity 1e-6 m2/s -> Re=2e8, ITTC C_F ~0.001889.
            Assert.That(ShipHullGeometry.SkinFrictionCoefficient(5, 40, .000001f), Is.EqualTo(.001889f).Within(.000002f));
            Assert.That(ShipHullGeometry.SkinFrictionCoefficient(0, 40, .000001f), Is.GreaterThan(0));
            Assert.That(ShipHullGeometry.SkinFrictionCoefficient(10, 40, .000001f),
                Is.LessThan(ShipHullGeometry.SkinFrictionCoefficient(5, 40, .000001f)));
        }
    }
}
