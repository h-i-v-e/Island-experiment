using Motu.Gameplay;
using NUnit.Framework;
using UnityEngine;

namespace Motu.Tests
{
    public sealed class SailingNavigationTests
    {
        [Test]
        public void SettingsHandleZeroInitializationAndInvalidValues()
        {
            var defaults = default(SailingNavigationSettings).Validated();
            Assert.That(defaults.TackWidthMetres, Is.EqualTo(SailingNavigationSettings.Default.TackWidthMetres));
            Assert.That(defaults.ForwardSpeedMultiplier, Is.EqualTo(SailingNavigationSettings.Default.ForwardSpeedMultiplier));
            defaults.MinimumWindAngleDegrees = float.NaN;
            defaults.TackWidthMetres = -5f;
            defaults.WindSpeedRatio = float.PositiveInfinity;
            var validated = defaults.Validated();
            Assert.That(validated.MinimumWindAngleDegrees, Is.EqualTo(45f));
            Assert.That(validated.TackWidthMetres, Is.EqualTo(1f));
            Assert.That(validated.WindSpeedRatio, Is.EqualTo(.25f));
        }

        [Test]
        public void PolarRespectsWindDirectionAndSpeed()
        {
            var settings = SailingNavigationSettings.Default;
            var wind = new Vector2(0f, -8f);
            Assert.That(SailingNavigation.SpeedAtHeading(0f, wind, settings), Is.Zero);
            Assert.That(SailingNavigation.SpeedAtHeading(45f, wind, settings), Is.EqualTo(0f).Within(.0001f));
            float beam = SailingNavigation.SpeedAtHeading(90f, wind, settings);
            Assert.That(beam, Is.GreaterThan(0f));
            Assert.That(SailingNavigation.SpeedAtHeading(90f, wind * 2f, settings), Is.EqualTo(beam * 2f).Within(.001f));
            Assert.That(SailingNavigation.SpeedAtHeading(90f, Vector2.zero, settings), Is.Zero);
        }

        [TestCase(0f, 5000f, 0f, -9f)]
        [TestCase(1200f, 5000f, 0f, -9f)]
        [TestCase(-2500f, 0f, 9f, 0f)]
        [TestCase(3000f, -2000f, 0f, -9f)]
        public void VirtualShipReachesDestinationWithWindDependentHeading(float x, float z, float wx, float wz)
        {
            var settings = SailingNavigationSettings.Default;
            var state = new SailingNavigationState(Vector2.zero, 90f);
            var destination = new Vector2(x, z);
            var wind = new Vector2(wx, wz);
            int tacks = 0;
            int previousTack = 0;
            for (int second = 0; second < 20000 && !state.Arrived; second++)
            {
                float heading = SailingNavigation.DesiredHeading(ref state, destination, wind, 1f, settings);
                if (state.TackSide != 0 && previousTack != 0 && state.TackSide != previousTack) tacks++;
                if (state.TackSide != 0) previousTack = state.TackSide;
                SailingNavigation.AdvanceVirtual(ref state, heading, wind, 1f, settings);
            }
            Assert.That(state.Arrived, Is.True, $"Position {state.Position}, destination {destination}");
            if (x == 0f && z > 0f) Assert.That(tacks, Is.GreaterThan(0));
        }

        [Test]
        public void VoyageStillConvergesAfterWindShifts()
        {
            var settings = SailingNavigationSettings.Default;
            var state = new SailingNavigationState(Vector2.zero, 90f);
            var destination = Vector2.up * 3000f;
            for (int second = 0; second < 20000 && !state.Arrived; second++)
            {
                var wind = second < 500 ? Vector2.down * 9f : Vector2.left * 9f;
                float heading = SailingNavigation.DesiredHeading(ref state, destination, wind, 1f, settings);
                SailingNavigation.AdvanceVirtual(ref state, heading, wind, 1f, settings);
            }
            Assert.That(state.Arrived, Is.True, $"Position {state.Position}, destination {destination}");
        }

        [Test]
        public void CalmStopsAccelerationAndDestinationChangesResetArrivalAndRoute()
        {
            var settings = SailingNavigationSettings.Default;
            var state = new SailingNavigationState(Vector2.zero, 0f);
            SailingNavigation.DesiredHeading(ref state, Vector2.zero, Vector2.down, 1f, settings);
            Assert.That(state.Arrived, Is.True);
            state.Position = Vector2.up * 100f;
            SailingNavigation.DesiredHeading(ref state, Vector2.zero, Vector2.down, 1f, settings);
            Assert.That(state.Arrived, Is.True, "Anchor drift does not restart a completed voyage.");
            state.Position = Vector2.zero;
            float heading = SailingNavigation.DesiredHeading(ref state, Vector2.up * 1000f, Vector2.zero, 1f, settings);
            SailingNavigation.AdvanceVirtual(ref state, heading, Vector2.zero, 10f, settings);
            Assert.That(state.Arrived, Is.False);
            Assert.That(state.Position, Is.EqualTo(Vector2.zero));
            Assert.That(state.RouteDestination, Is.EqualTo(Vector2.up * 1000f));
        }

        [Test]
        public void TackHoldsThroughSmallChangesAndRespondsToWindShift()
        {
            var settings = SailingNavigationSettings.Default;
            var state = new SailingNavigationState(Vector2.zero, 0f);
            var destination = Vector2.up * 1000f;
            float first = SailingNavigation.DesiredHeading(ref state, destination, Vector2.down * 9f, 1f, settings);
            int side = state.TackSide;
            float next = SailingNavigation.DesiredHeading(ref state, destination, new Vector2(.1f, -9f), 1f, settings);
            Assert.That(state.TackSide, Is.EqualTo(side));
            Assert.That(Mathf.Abs(Mathf.DeltaAngle(first, next)), Is.LessThan(2f));
            float shifted = SailingNavigation.DesiredHeading(ref state, destination, Vector2.right * 9f, 1f, settings);
            Assert.That(shifted, Is.EqualTo(0f).Within(.001f));
            Assert.That(state.TackSide, Is.Zero);
        }
    }
}
