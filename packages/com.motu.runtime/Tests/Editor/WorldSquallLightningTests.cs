using Motu.World;
using Motu.Rendering;
using NUnit.Framework;
using UnityEngine;
using Object = UnityEngine.Object;

namespace Motu.Tests
{
    public sealed class WorldSquallLightningTests
    {
        private static WorldSquallState State(int seed, float age, SquallWeatherSettings settings, bool moving = true)
        {
            var speed = WorldSquallMotion.Speed(seed, age, settings);
            var velocity = moving ? Vector2.right * (speed * 9) : Vector2.zero;
            var field = new WorldSquallField(Vector2.zero, settings.MaximumDiameterMetres * .5f, .25f, 4, 1, velocity);
            return new WorldSquallState(Vector3.zero, settings.MaximumDiameterMetres, age, 1, seed, field, speed);
        }

        private static int CountStrikes(WorldSquallLightning clock, int seed, SquallWeatherSettings settings, bool moving = true)
        {
            var count = 0;
            for (var i = 0; i <= 4000; i++)
                if (clock.Advance(State(seed, i * .01f, settings, moving), settings)) count++;
            return count;
        }

        [Test]
        public void TopSpeedKeepsTheExistingRateAndHalfSpeedRoughlyHalvesIt()
        {
            var settings = SquallWeatherSettings.Default;
            settings.MinimumSpeedMultiplier = settings.MaximumSpeedMultiplier;
            var fast = CountStrikes(new WorldSquallLightning(), 7129, settings);
            Assert.That(fast, Is.EqualTo(16), "The existing top-speed interval is 2.5 seconds.");
            settings.MinimumSpeedMultiplier = 0;
            settings.SpeedVariationSeconds = 600;
            var seed = 0;
            while (WorldSquallMotion.Speed(seed, 0, settings) < 3.92f
                || WorldSquallMotion.Speed(seed, 0, settings) > 4.08f) seed++;
            var slower = CountStrikes(new WorldSquallLightning(), seed, settings);
            Assert.That(slower, Is.InRange(7, 9));
        }

        [Test]
        public void ZeroSpeedAndZeroAmbientWindNeverStrikeAndPausingDoesNotRepeatStrikes()
        {
            var settings = SquallWeatherSettings.Default;
            settings.MinimumSpeedMultiplier = settings.MaximumSpeedMultiplier = 0;
            Assert.That(CountStrikes(new WorldSquallLightning(), 7129, settings), Is.Zero);
            settings.MinimumSpeedMultiplier = settings.MaximumSpeedMultiplier = 4;
            Assert.That(CountStrikes(new WorldSquallLightning(), 7129, settings, false), Is.Zero);
            var clock = new WorldSquallLightning();
            clock.Advance(State(7129, 0, settings), settings);
            Assert.That(clock.Advance(State(7129, 2, settings), settings), Is.True);
            for (var i = 0; i < 50; i++) Assert.That(clock.Advance(State(7129, 2, settings), settings), Is.False);
        }

        [Test]
        public void LiveIntervalChangesPreserveClockPhaseAndLongFramesDoNotReplayABurst()
        {
            var settings = SquallWeatherSettings.Default;
            settings.MinimumSpeedMultiplier = settings.MaximumSpeedMultiplier = 4;
            var clock = new WorldSquallLightning();
            clock.Advance(State(7129, 0, settings), settings);
            clock.Advance(State(7129, 10, settings), settings);
            settings.LightningIntervalSeconds = .2f;
            Assert.That(clock.Advance(State(7129, 10, settings), settings), Is.False);
            Assert.That(clock.Advance(State(7129, 100, settings), settings), Is.True);
            Assert.That(clock.Advance(State(7129, 100, settings), settings), Is.False);
            Assert.That(clock.StrikeAge, Is.InRange(99.7f, 100f));
        }

        [Test]
        public void StoppingAStormDoesNotFlashAndTwoThousandMetreCloudsUseTheirFullFootprint()
        {
            var root = new GameObject("Stationary storm fixture");
            using var visuals = new SquallVisualRenderer(root.transform);
            try
            {
                var settings = SquallWeatherSettings.Default;
                settings.MinimumSpeedMultiplier = settings.MaximumSpeedMultiplier = 0;
                for (var age = 0; age < 20; age++) visuals.UpdateVisuals(State(7129, age, settings, false), settings, Vector3.zero, 1);
                Assert.That(visuals.FlashStrength, Is.Zero);
                Assert.That(settings.MaximumDiameterMetres, Is.EqualTo(2000));
                var volume = visuals.Root.transform.Find("Volumetric squall cloud and rain");
                Assert.That(volume.localScale.x, Is.EqualTo(2000 * (1 + settings.CloudFringeFraction)).Within(.001f));
            }
            finally { Object.DestroyImmediate(root); }
        }
    }
}
