using Motu.Rendering;
using Motu.World;
using NUnit.Framework;
using UnityEditor;
using UnityEngine;

namespace Motu.Tests
{
    public sealed class SquallWetnessTests
    {
        [Test]
        public void WetnessBuildsWithRainAndNeverDriesWhileEvenLightRainContinues()
        {
            var state = new SquallWetnessState();
            state.Advance(1, 3, 6, 40);
            Assert.That(state.Amount, Is.EqualTo(.5f).Within(.0001f));
            state.Advance(1, 3, 6, 40);
            Assert.That(state.Amount, Is.EqualTo(1).Within(.0001f));
            state.Advance(.001f, 1000, 6, 40);
            Assert.That(state.Amount, Is.EqualTo(1), "The fading edge of a storm must hold accumulated wetness.");
            state.Advance(0, WorldSquallRain.ParticleLifetime, 6, 40);
            Assert.That(state.Amount, Is.EqualTo(1), "Airborne drops must finish before drying.");
            state.Advance(0, 20, 6, 40);
            Assert.That(state.Amount, Is.EqualTo(.5f).Within(.0001f));
            state.Advance(0, 20, 6, 40);
            Assert.That(state.Amount, Is.Zero);
        }

        [Test]
        public void DryingIsFrameIndependentAndNewRainInterruptsIt()
        {
            var whole = new SquallWetnessState(); var sliced = new SquallWetnessState();
            whole.Advance(1, 6, 6, 40); sliced.Advance(1, 6, 6, 40);
            whole.Advance(0, 13, 6, 40);
            for (var i = 0; i < 130; ++i) sliced.Advance(0, .1f, 6, 40);
            Assert.That(sliced.Amount, Is.EqualTo(whole.Amount).Within(.00001f));
            var before = sliced.Amount;
            sliced.Advance(.05f, 10, 6, 40);
            Assert.That(sliced.Amount, Is.GreaterThan(before), "Fresh rain must add moisture rather than only holding it.");
            sliced.Advance(1, 6, 6, 40);
            Assert.That(sliced.Amount, Is.EqualTo(1));
            sliced.Advance(0, 0, 6, 40);
            Assert.That(sliced.Amount, Is.EqualTo(1), "Pausing must not dry surfaces.");
        }

        [Test]
        public void SustainedLightRainEventuallySoaksSurfacesCompletely()
        {
            var state = new SquallWetnessState();
            state.Advance(.1f, 30, 6, 40);
            Assert.That(state.Amount, Is.EqualTo(.5f).Within(.0001f));
            state.Advance(.1f, 30, 6, 40);
            Assert.That(state.Amount, Is.EqualTo(1).Within(.0001f));
        }

        [Test]
        public void RainSamplingFollowsTheSlantedFootprintAndStopsAboveTheCloudBase()
        {
            var settings = SquallWeatherSettings.Default;
            var field = new WorldSquallField(Vector2.zero, 100, .25f, 2, 1, Vector2.zero);
            var cell = new WorldSquallState(Vector3.zero, 200, 20, 1, 7, field, settings.MaximumSpeedMultiplier);
            var drift = WorldSquallRain.Drift(Vector2.right * 20);
            var landing = drift * (60 / WorldSquallRain.FallSpeed);
            Assert.That(WorldSquallRain.IntensityAt(cell, settings, new Vector3(landing.x, 0, 0), drift, 60), Is.EqualTo(1).Within(.0001f));
            Assert.That(WorldSquallRain.IntensityAt(cell, settings, new Vector3(-100, 0, 0), drift, 60), Is.Zero);
            Assert.That(WorldSquallRain.IntensityAt(cell, settings, new Vector3(0, 61, 0), drift, 60), Is.Zero);
            settings.RainIntensity = 0;
            Assert.That(WorldSquallRain.IntensityAt(cell, settings, Vector3.zero, drift, 60), Is.Zero);
        }

        [Test]
        public void OldWeatherSettingsGainWetnessWithoutResettingAuthoredStormHeadings()
        {
            var settings = SquallWeatherSettings.Default;
            settings.Version = 3;
            settings.SurfaceWetness = settings.WettingSeconds = settings.DryingSeconds = 0;
            settings.MaximumHeadingDeviationDegrees = 17;
            settings.HeadingVariationSeconds = 23;
            settings = settings.Validated();
            Assert.That(settings.Version, Is.EqualTo(SquallWeatherSettings.Default.Version));
            Assert.That(settings.SurfaceWetness, Is.EqualTo(.75f));
            Assert.That(settings.WettingSeconds, Is.EqualTo(6));
            Assert.That(settings.DryingSeconds, Is.EqualTo(40));
            Assert.That(settings.MaximumHeadingDeviationDegrees, Is.EqualTo(17));
            Assert.That(settings.HeadingVariationSeconds, Is.EqualTo(23));
            settings.SurfaceWetness = 0;
            Assert.That(settings.Validated().SurfaceWetness, Is.Zero, "An explicit wetness opt-out must remain off.");
        }

        [Test]
        public void WetSurfaceShaderIsPackagedForPlayersAndCompiles()
        {
            var shader = Resources.Load<Shader>("SquallSurfaceWetness");
            Assert.That(shader, Is.Not.Null);
            Assert.That(ShaderUtil.ShaderHasError(shader), Is.False);
        }
    }
}
