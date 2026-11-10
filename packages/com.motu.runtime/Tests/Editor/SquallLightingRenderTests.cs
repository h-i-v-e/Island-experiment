using System.Linq;
using System.Reflection;
using Motu.Rendering;
using NUnit.Framework;
using UnityEngine;
using UnityEngine.Rendering;
using UnityEngine.Rendering.Universal;

namespace Motu.Tests
{
    public sealed class SquallLightingRenderTests
    {
        [Test]
        public void StormShadowsDarkenOrdinaryUrpSurfacesAndClearingRestoresTheirLight()
        {
            using var fixture = new SquallWetnessRenderTests.Fixture();
            var sun = RenderSettings.sun;
            var original = sun.cookie;
            var weather = fixture.Environment.Weather;
            weather.Clouds.Enabled = false;
            weather.Squalls.SurfaceWetness = 0;
            fixture.Environment.ApplyWeather(weather);
            var clear = fixture.Capture("storm-shadow-clear");
            Assert.That(fixture.Environment.SpawnSquall(Vector3.zero, 2000), Is.True);
            Assert.That(sun.cookie, Is.Not.Null);
            var shaded = fixture.Capture("storm-shadow-on-ship-and-ground");
            Assert.That(Mean(clear) - Mean(shaded), Is.GreaterThan(.015f),
                "The directional cookie must shade ordinary URP Lit decks and stone, beyond custom Motu materials.");
            fixture.Environment.ClearSquall();
            Assert.That(sun.cookie, Is.SameAs(original));
            Assert.That(Difference(clear, fixture.Capture("storm-shadow-cleared")), Is.LessThan(.002f));
        }

        [Test]
        public void StormCookieFollowsThePrimaryMoonInsteadOfAnEnabledSecondarySun()
        {
            using var fixture = new SquallWetnessRenderTests.Fixture();
            var sun = RenderSettings.sun;
            var moon = fixture.Environment.MoonLight;
            var original = moon.cookie;
            moon.enabled = true; moon.intensity = 3;
            moon.transform.rotation = Quaternion.Euler(27, 180, 0);
            RenderSettings.sun = moon;
            fixture.Environment.SpawnSquall(Vector3.zero, 2000);
            Assert.That(moon.cookie, Is.Not.Null);
            Assert.That(sun.cookie, Is.Null);
            fixture.Environment.ClearSquall();
            Assert.That(moon.cookie, Is.SameAs(original));
        }

        [Test]
        public void CloudHeightLightningActuallyIlluminatesTheDeckAndGround()
        {
            using var fixture = new SquallWetnessRenderTests.Fixture();
            RenderSettings.sun.enabled = false;
            RenderSettings.ambientMode = AmbientMode.Flat; RenderSettings.ambientLight = Color.gray * .03f;
            var weather = fixture.Environment.Weather;
            weather.Squalls.SurfaceWetness = 0; weather.Squalls.LightningBrightness = 5;
            weather.WindSpeedMetresPerSecond = 9;
            fixture.Environment.ApplyWeather(weather);
            Assert.That(fixture.Environment.SpawnSquall(Vector3.zero, 2000), Is.True);
            var dark = fixture.Capture("lightning-dark-deck");
            var flash = fixture.Environment.GetComponentsInChildren<Light>(true)
                .Single(light => light.name == "Squall lightning light");
            for (var step = 0; step < 600 && !flash.enabled; ++step)
            {
                fixture.Environment.AdvanceSqualls(.01f);
                typeof(Motu.World.WorldEnvironmentController).GetMethod("UpdateSquallVisuals",
                    BindingFlags.Instance | BindingFlags.NonPublic).Invoke(fixture.Environment, new object[] { .01f });
            }
            Assert.That(flash.enabled, Is.True);
            Assert.That(flash.shadows, Is.EqualTo(LightShadows.None), "Brief flashes must not render six extra shadow views of the scene.");
            Assert.That(flash.range, Is.GreaterThan(1000));
            var lit = fixture.Capture("lightning-lit-deck");
            Assert.That(Mean(lit) - Mean(dark), Is.GreaterThan(.04f),
                "The flash must illuminate real opaque surfaces, not just brighten the cloud or bolt.");
            fixture.Environment.ClearSquall();
            Assert.That(flash.enabled, Is.False);
        }

        [Test]
        public void ReleasingTheOwnerRestoresAuthoredCookiesUvSettingsAndShadowGlobals()
        {
            var previous = Shader.GetGlobalVector("_MotuSquallCloud0");
            var sentinel = new Vector4(11, 22, 33, 44);
            var authored = new Texture2D(2, 2, TextureFormat.RGBA32, false);
            try
            {
                Shader.SetGlobalVector("_MotuSquallCloud0", sentinel);
                using var fixture = new SquallWetnessRenderTests.Fixture();
                var sun = RenderSettings.sun;
                var data = sun.GetUniversalAdditionalLightData();
                var size = new Vector2(40, 60); var offset = new Vector2(3, 6);
                data.lightCookieSize = size; data.lightCookieOffset = offset; sun.cookie = authored;
                fixture.Environment.SpawnSquall(Vector3.zero, 2000);
                Assert.That(sun.cookie, Is.Not.SameAs(authored));
                fixture.Environment.enabled = false;
                typeof(Motu.World.WorldEnvironmentController).GetMethod("OnDisable",
                    BindingFlags.Instance | BindingFlags.NonPublic).Invoke(fixture.Environment, null);
                Assert.That(sun.cookie, Is.SameAs(authored));
                Assert.That(data.lightCookieSize, Is.EqualTo(size));
                Assert.That(data.lightCookieOffset, Is.EqualTo(offset));
                Assert.That(Shader.GetGlobalVector("_MotuSquallCloud0"), Is.EqualTo(sentinel));
            }
            finally
            {
                Object.DestroyImmediate(authored);
                Shader.SetGlobalVector("_MotuSquallCloud0", previous);
            }
        }

        [Test]
        public void LightRainCreatesVisibleWetnessUnderAnOvercastSkyWithoutASunHighlight()
        {
            using var fixture = new SquallWetnessRenderTests.Fixture();
            RenderSettings.sun.enabled = false;
            RenderSettings.ambientMode = AmbientMode.Flat; RenderSettings.ambientLight = Color.gray * .3f;
            Shader.SetGlobalColor("_MotuCloudAmbientColor", new Color(.22f, .25f, .3f));
            var weather = fixture.Environment.Weather;
            weather.Squalls.RainIntensity = .15f;
            fixture.Environment.ApplyWeather(weather);
            fixture.Environment.SpawnSquall(Vector3.zero, 2000);
            var dry = fixture.Capture("overcast-dry-deck");
            var wetness = fixture.Camera.GetComponent<SquallSurfaceWetness>();
            wetness.Advance(90);
            Assert.That(wetness.Wetness, Is.EqualTo(1).Within(.001f));
            var wet = fixture.Capture("overcast-wet-deck");
            Assert.That(Difference(dry, wet), Is.GreaterThan(.01f),
                "The overcast wet sheen must be noticeable without a direct sun reflection.");
        }

        private static float Mean(Color[] image) => image.Average(pixel => pixel.grayscale);
        private static float Difference(Color[] first, Color[] second) =>
            first.Select((pixel, i) => Mathf.Abs(pixel.grayscale - second[i].grayscale)).Average();
    }
}
