using System;
using System.Collections;
using System.IO;
using System.Reflection;
using Motu.Rendering;
using Motu.Settings;
using Motu.World;
using NUnit.Framework;
using UnityEngine;
using UnityEngine.Rendering;
using UnityEngine.Rendering.Universal;
using UnityEngine.TestTools;
using Object = UnityEngine.Object;

namespace Motu.Tests
{
    public sealed class WorldMultipleSquallTests
    {
        private GameObject root, target;
        private WorldEnvironmentController environment;
        private EnvironmentHostState baseline;

        public sealed class ControlledDriver : WorldWeatherDriver
        {
            private readonly CoherentSquallScheduler scheduler = new CoherentSquallScheduler();
            public int SpawnUpdates;
            public override bool ControlsSquallSpawning => true;
            public override float GetSquallActivity(WorldWeatherState weather) =>
                CoherentSquallScheduler.WeatherActivity(weather, .45f);
            public override void UpdateWeather(ref WorldWeatherState weather, float deltaTime) { }
            public override void UpdateSquallSpawning(WorldEnvironmentController owner, float deltaTime)
            {
                SpawnUpdates++;
                scheduler.Advance(owner, deltaTime, .45f);
            }
        }

        [SetUp]
        public void SetUp()
        {
            baseline = new EnvironmentHostState();
            target = new GameObject("Multiple squall target");
            root = new GameObject("Multiple squall environment");
            environment = root.AddComponent<WorldEnvironmentController>();
            environment.Initialize(new WorldEnvironmentSettings().WithSeed(7129), new IslandCloudSettings(), 64, 128, target.transform);
            environment.SquallSpawnTarget = target.transform;
            var weather = environment.Weather;
            weather.WindDirection = Vector2.right;
            weather.WindSpeedMetresPerSecond = 9;
            weather.Clouds.Coverage = 1;
            var settings = SquallWeatherSettings.Default;
            settings.AutomaticSpawning = false;
            settings.FadeSeconds = 0;
            settings.MinimumSpeedMultiplier = settings.MaximumSpeedMultiplier = 1;
            settings.WindMultiplier = 3;
            weather.Squalls = settings;
            environment.ApplyWeather(weather);
        }

        [TearDown]
        public void TearDown()
        {
            Object.DestroyImmediate(root);
            Object.DestroyImmediate(target);
            baseline.Restore();
        }

        [Test]
        public void ThreeCellsKeepTheirIdentityAndOverlapUsesStrongestGain()
        {
            Assert.That(environment.SpawnSquall(Vector3.zero, 300), Is.True);
            Assert.That(environment.SpawnSquall(Vector3.right * 100, 300), Is.True);
            Assert.That(environment.SpawnSquall(Vector3.right * 600, 300), Is.True);
            Assert.That(environment.ActiveSquallCount, Is.EqualTo(3));
            var first = environment.GetSquall(0);
            Assert.That(environment.SpawnSquall(Vector3.zero, 500), Is.False);
            Assert.That(environment.GetSquall(0).Seed, Is.EqualTo(first.Seed));
            Assert.That(environment.WindSpeedAt(Vector3.zero), Is.EqualTo(27f));
            Assert.That(environment.WindSpeedAt(Vector3.right * 600), Is.EqualTo(27f));
            Assert.That(environment.WindSpeedAt(Vector3.back * 1000), Is.EqualTo(9f));
            Assert.That(environment.SquallWindMultiplierAt(Vector2.right * 50), Is.EqualTo(3f), "Overlaps must not produce 3x3 wind.");
            for (var slot = 0; slot < 3; slot++)
            {
                Assert.That(Shader.GetGlobalVector(WorldSquallShader.BoundsIds[slot]), Is.EqualTo(environment.GetSquall(slot).Field.ShaderBounds));
                Assert.That(Shader.GetGlobalVector(WorldSquallShader.WindIds[slot]), Is.EqualTo(environment.GetSquall(slot).Field.ShaderWind));
            }
            Assert.Throws<ArgumentOutOfRangeException>(() => environment.GetSquall(3));
        }

        [Test]
        public void OneCellsRetirementDoesNotRemoveEncounterableNeighbours()
        {
            var weather = environment.Weather;
            weather.WindSpeedMetresPerSecond = 0;
            var settings = weather.Squalls;
            settings.LifetimeSeconds = 5;
            weather.Squalls = settings;
            environment.ApplyWeather(weather);
            environment.SpawnSquall(Vector3.zero, 300);
            environment.SpawnSquall(Vector3.right * 12000, 300);
            environment.SpawnSquall(Vector3.left * 12000, 300);
            var first = environment.GetSquall(0);
            environment.AdvanceSqualls(5);
            Assert.That(environment.ActiveSquallCount, Is.EqualTo(1));
            Assert.That(environment.GetSquall(0).Seed, Is.EqualTo(first.Seed));
            Assert.That(Shader.GetGlobalVector(WorldSquallShader.BoundsIds[1]), Is.EqualTo(Vector4.zero));
            Assert.That(Shader.GetGlobalVector(WorldSquallShader.BoundsIds[2]), Is.EqualTo(Vector4.zero));
            Assert.That(environment.SpawnSquall(Vector3.right * 500, 200, out var replacement), Is.True);
            Assert.That(environment.GetSquall(1).Seed, Is.EqualTo(replacement.Seed));
        }

        [Test]
        public void RetiringTheFirstSlotKeepsLegacyAndGpuFieldsOnTheRemainingCell()
        {
            var weather = environment.Weather;
            weather.WindSpeedMetresPerSecond = 0;
            var settings = weather.Squalls;
            settings.LifetimeSeconds = 5;
            weather.Squalls = settings;
            environment.ApplyWeather(weather);
            environment.SpawnSquall(Vector3.right * 12000, 300);
            environment.SpawnSquall(Vector3.zero, 200, out var survivor);
            environment.AdvanceSqualls(5);
            Assert.That(environment.ActiveSquallCount, Is.EqualTo(1));
            Assert.That(environment.Squall.Seed, Is.EqualTo(survivor.Seed));
            Assert.That(Shader.GetGlobalVector(WorldSquallShader.BoundsIds[0]), Is.EqualTo(environment.Squall.Field.ShaderBounds));
            Assert.That(Shader.GetGlobalVector(WorldSquallShader.BoundsIds[1]), Is.EqualTo(Vector4.zero));
        }

        [Test]
        public void LoweringTheLimitAndDisablingWeatherClearUnusedFields()
        {
            for (var i = 0; i < 3; i++) environment.SpawnSquall(Vector3.right * (i * 100), 300);
            var weather = environment.Weather;
            var settings = weather.Squalls;
            settings.MaximumActiveSqualls = 1;
            weather.Squalls = settings;
            environment.ApplyWeather(weather);
            Assert.That(environment.ActiveSquallCount, Is.EqualTo(1));
            Assert.That(Shader.GetGlobalVector(WorldSquallShader.WindIds[1]), Is.EqualTo(Vector4.zero));
            Assert.That(Shader.GetGlobalVector(WorldSquallShader.WindIds[2]), Is.EqualTo(Vector4.zero));
            settings.Enabled = false;
            weather.Squalls = settings;
            environment.ApplyWeather(weather);
            Assert.That(environment.ActiveSquallCount, Is.Zero);
            for (var slot = 0; slot < 3; slot++)
                Assert.That(Shader.GetGlobalVector(WorldSquallShader.BoundsIds[slot]), Is.EqualTo(Vector4.zero));
        }

        [Test]
        public void CoherentOwnerPausesInFairWeatherAndFillsTheLimitInStormWeather()
        {
            var driver = root.AddComponent<ControlledDriver>();
            environment.WeatherDriver = driver;
            var weather = environment.Weather;
            var settings = weather.Squalls;
            settings.AutomaticSpawning = true;
            // Test formation timing independently of room for large rain cores.
            settings.MinimumDiameterMetres = settings.MaximumDiameterMetres = 200;
            settings.MinimumSpawnIntervalSeconds = settings.MaximumSpawnIntervalSeconds = .25f;
            weather.Squalls = settings;
            weather.Clouds.Coverage = 0;
            weather.WindSpeedMetresPerSecond = 0;
            environment.ApplyWeather(weather);
            driver.enabled = false;
            environment.ClearSquall();
            environment.AdvanceSqualls(100);
            Assert.That(environment.ActiveSquallCount, Is.Zero, "A disabled coherent owner must not fall back to the standalone timer.");
            driver.enabled = true;
            environment.AdvanceSqualls(100);
            Assert.That(environment.ActiveSquallCount, Is.Zero, "Fair coherent weather pauses new formation.");
            weather.Clouds.Coverage = 1;
            weather.WindSpeedMetresPerSecond = 25;
            environment.ApplyWeather(weather);
            for (var i = 0; i < 3; i++) environment.AdvanceSqualls(.26f);
            Assert.That(environment.ActiveSquallCount, Is.EqualTo(3));
            Assert.That(driver.SpawnUpdates, Is.EqualTo(4));
            environment.AdvanceSqualls(1);
            Assert.That(environment.ActiveSquallCount, Is.EqualTo(3));
        }

        [Test]
        public void ShorterLiveCoherentIntervalsApplyWithoutAnOldLongWait()
        {
            var driver = root.AddComponent<ControlledDriver>();
            environment.WeatherDriver = driver;
            var weather = environment.Weather;
            var settings = weather.Squalls;
            settings.AutomaticSpawning = true;
            settings.MinimumSpawnIntervalSeconds = settings.MaximumSpawnIntervalSeconds = 100;
            weather.Squalls = settings;
            environment.ApplyWeather(weather);
            environment.AdvanceSqualls(1);
            settings.MinimumSpawnIntervalSeconds = settings.MaximumSpawnIntervalSeconds = .1f;
            weather.Squalls = settings;
            environment.ApplyWeather(weather);
            environment.AdvanceSqualls(.11f);
            Assert.That(environment.ActiveSquallCount, Is.EqualTo(1));
        }

        [Test]
        public void ClearingWeatherFadesNearbyCloudsAndWindBeforeRemovingTheirFields()
        {
            environment.WeatherDriver = root.AddComponent<ControlledDriver>();
            var weather = environment.Weather;
            weather.Clouds.Coverage = 1;
            weather.WindSpeedMetresPerSecond = 25;
            weather.Squalls.FadeSeconds = 2;
            environment.ApplyWeather(weather);
            for (var i = 0; i < 3; i++) environment.SpawnSquall(Vector3.right * (i * 200), 300);
            environment.AdvanceSqualls(2);
            var first = environment.Squall;
            Assert.That(first.Strength, Is.EqualTo(1));
            Assert.That(environment.SquallWindMultiplierAt(new Vector2(first.Center.x, first.Center.z)), Is.EqualTo(3));
            target.transform.position = first.Center;
            weather.Clouds.Coverage = 0;
            weather.WindSpeedMetresPerSecond = 1;
            environment.ApplyWeather(weather);
            environment.AdvanceSqualls(0);
            Assert.That(environment.Squall.Strength, Is.EqualTo(1), "Weather changes must not pop visible clouds.");
            environment.AdvanceSqualls(1);
            var fading = environment.Squall;
            Assert.That(environment.ActiveSquallCount, Is.EqualTo(3));
            Assert.That(fading.Strength, Is.EqualTo(.5f).Within(.00001f));
            Assert.That(fading.Seed, Is.EqualTo(first.Seed));
            Assert.That(environment.WindSpeedAt(fading.Center), Is.EqualTo(2f).Within(.00001f));
            Assert.That(Shader.GetGlobalVector(WorldSquallShader.WindIds[0]).x, Is.EqualTo(1f).Within(.00001f));
            environment.AdvanceSqualls(1);
            Assert.That(environment.ActiveSquallCount, Is.Zero, "Weather clearing dissolves even encounterable cells.");
            for (var slot = 0; slot < 3; slot++)
                Assert.That(Shader.GetGlobalVector(WorldSquallShader.WindIds[slot]), Is.EqualTo(Vector4.zero));
        }

        [Test]
        public void ImprovingWeatherGraduallyReducesPopulationAndExistingCellStrength()
        {
            environment.WeatherDriver = root.AddComponent<ControlledDriver>();
            var weather = environment.Weather;
            weather.Clouds.Coverage = 1;
            weather.WindSpeedMetresPerSecond = 25;
            weather.Squalls.FadeSeconds = 2;
            environment.ApplyWeather(weather);
            for (var i = 0; i < 3; i++) environment.SpawnSquall(Vector3.right * (i * 200), 300);
            environment.AdvanceSqualls(2);
            var seed = environment.Squall.Seed;
            weather.Clouds.Coverage = .9f;
            weather.WindSpeedMetresPerSecond = 2;
            weather.WindDirection = Vector2.left;
            environment.ApplyWeather(weather);
            environment.AdvanceSqualls(1);
            Assert.That(environment.ActiveSquallCount, Is.EqualTo(3), "Excess cells fade instead of vanishing on a count change.");
            environment.AdvanceSqualls(1);
            Assert.That(environment.ActiveSquallCount, Is.EqualTo(1));
            var survivor = environment.Squall;
            Assert.That(survivor.Seed, Is.EqualTo(seed));
            Assert.That(survivor.Strength, Is.InRange(.01f, .5f));
            Assert.That(survivor.Field.Strength, Is.EqualTo(survivor.Strength));
            Assert.That(Vector2.Angle(survivor.Field.Velocity, Vector2.left), Is.LessThanOrEqualTo(45.001f));
            Assert.That(survivor.Field.Velocity.magnitude, Is.EqualTo(2).Within(.00001f));
            var center = survivor.Center;
            environment.AdvanceSqualls(1);
            var drift = environment.Squall.Center - center;
            Assert.That(drift.x, Is.LessThan(-1.4f));
            Assert.That(drift.magnitude, Is.EqualTo(2).Within(.001f));
        }

        [Test]
        public void FormationUsesLiveWeatherRateAndDiscardsProgressDuringFairWeather()
        {
            environment.WeatherDriver = root.AddComponent<ControlledDriver>();
            var weather = environment.Weather;
            weather.Squalls.AutomaticSpawning = true;
            weather.Squalls.MinimumSpawnIntervalSeconds = weather.Squalls.MaximumSpawnIntervalSeconds = 1;
            weather.Clouds.Coverage = 1;
            weather.WindSpeedMetresPerSecond = 25;
            environment.ApplyWeather(weather);
            environment.AdvanceSqualls(.5f);
            weather.Clouds.Coverage = 0;
            weather.WindSpeedMetresPerSecond = 0;
            environment.ApplyWeather(weather);
            environment.AdvanceSqualls(100);
            weather.Clouds.Coverage = 1;
            weather.WindSpeedMetresPerSecond = 25;
            environment.ApplyWeather(weather);
            environment.AdvanceSqualls(.5f);
            Assert.That(environment.ActiveSquallCount, Is.Zero, "An earlier front must not bank formation for the next front.");
            environment.AdvanceSqualls(.5f);
            Assert.That(environment.ActiveSquallCount, Is.EqualTo(1));
            weather.Clouds.Coverage = .9f;
            weather.WindSpeedMetresPerSecond = 2;
            environment.ApplyWeather(weather);
            for (var i = 0; i < 10; i++) environment.AdvanceSqualls(2);
            Assert.That(environment.ActiveSquallCount, Is.EqualTo(1), "Moderate weather must not fill all three slots over time.");
        }

        [Test]
        public void ALongFrameSpawnsAtMostOneCellAndLegacySettingsGainTheThreeCellLimit()
        {
            var driver = root.AddComponent<ControlledDriver>();
            environment.WeatherDriver = driver;
            var weather = environment.Weather;
            var settings = weather.Squalls;
            settings.AutomaticSpawning = true;
            settings.MinimumSpawnIntervalSeconds = settings.MaximumSpawnIntervalSeconds = .1f;
            weather.Squalls = settings;
            environment.ApplyWeather(weather);
            environment.AdvanceSqualls(100);
            Assert.That(environment.ActiveSquallCount, Is.EqualTo(1));
            var legacy = SquallWeatherSettings.Default;
            legacy.Version = 1;
            legacy.MaximumActiveSqualls = 0;
            legacy.CloudFringeFraction = .2f;
            legacy = legacy.Validated();
            Assert.That(legacy.MaximumActiveSqualls, Is.EqualTo(3));
            Assert.That(legacy.CloudFringeFraction, Is.EqualTo(.2f));
        }

        [Test]
        public void ExplicitLimitOnAnOlderSettingsVersionIsPreserved()
        {
            var settings = SquallWeatherSettings.Default;
            settings.Version = 1;
            settings.MaximumActiveSqualls = 2;
            Assert.That(settings.Validated().MaximumActiveSqualls, Is.EqualTo(2));
        }

        [Test]
        public void HostReleaseAndResumeRestoreAllThreeShaderFields()
        {
            InvokeLifecycle("ReleaseEnvironment");
            var savedBounds = new Vector4[3];
            var savedWind = new Vector4[3];
            for (var slot = 0; slot < 3; slot++)
            {
                savedBounds[slot] = new Vector4(100 + slot, 200, 30, 40);
                savedWind[slot] = new Vector4(1 + slot, 2, 3, 0);
                Shader.SetGlobalVector(WorldSquallShader.BoundsIds[slot], savedBounds[slot]);
                Shader.SetGlobalVector(WorldSquallShader.WindIds[slot], savedWind[slot]);
            }
            InvokeLifecycle("ResumeEnvironment");
            for (var i = 0; i < 3; i++) environment.SpawnSquall(Vector3.right * (i * 100), 300);
            InvokeLifecycle("ReleaseEnvironment");
            for (var slot = 0; slot < 3; slot++)
            {
                Assert.That(Shader.GetGlobalVector(WorldSquallShader.BoundsIds[slot]), Is.EqualTo(savedBounds[slot]));
                Assert.That(Shader.GetGlobalVector(WorldSquallShader.WindIds[slot]), Is.EqualTo(savedWind[slot]));
            }
            InvokeLifecycle("ResumeEnvironment");
            Assert.That(environment.ActiveSquallCount, Is.EqualTo(3));
            for (var slot = 0; slot < 3; slot++)
                Assert.That(Shader.GetGlobalVector(WorldSquallShader.BoundsIds[slot]), Is.EqualTo(environment.GetSquall(slot).Field.ShaderBounds));
        }

        [UnityTest]
        public IEnumerator ActualCameraRendersThreeDistinctVolumetricCells()
        {
            if (!(GraphicsSettings.currentRenderPipeline is UniversalRenderPipelineAsset)) Assert.Ignore("URP is required.");
            var cameraObject = new GameObject("Three squall render camera");
            var texture = new RenderTexture(768, 432, 24);
            var image = new Texture2D(768, 432, TextureFormat.RGB24, false);
            var previous = RenderTexture.active;
            try
            {
                var camera = cameraObject.AddComponent<Camera>();
                camera.enabled = false;
                camera.clearFlags = CameraClearFlags.SolidColor;
                camera.backgroundColor = new Color(.1f, .3f, .5f);
                camera.nearClipPlane = .3f; camera.farClipPlane = 5000; camera.fieldOfView = 55;
                camera.allowHDR = false; camera.allowMSAA = false;
                camera.cullingMask = 1 << 31;
                camera.targetTexture = texture;
                camera.GetUniversalAdditionalCameraData().renderPostProcessing = false;
                camera.transform.position = new Vector3(0, 90, -400);
                camera.transform.LookAt(new Vector3(0, 90, 1000));
                target.transform.position = camera.transform.position;
                void Render()
                {
                    RenderPipeline.SubmitRenderRequest(camera,
                        new UniversalRenderPipeline.SingleCameraRequest { destination = texture });
                    RenderTexture.active = texture;
                    image.ReadPixels(new Rect(0, 0, image.width, image.height), 0, 0);
                    image.Apply();
                }
                Render();
                var clear = image.GetPixels32();
                for (var slot = 0; slot < 3; slot++)
                    Assert.That(environment.SpawnSquall(new Vector3((slot - 1) * 350, 0, 1000), 200), Is.True);
                foreach (var visuals in root.GetComponentsInChildren<MeshRenderer>(true))
                    if (visuals.transform.parent != null && visuals.transform.parent.name == "Travelling squall")
                        visuals.gameObject.layer = 31;
                // NeedsRender uses the root layer as well as the mesh bounds.
                foreach (Transform child in root.transform)
                    if (child.name == "Travelling squall") child.gameObject.layer = 31;
                yield return null;
                Assert.That(SquallVisualRenderer.NeedsRender(camera), Is.True);
                Render();
                var pixels = image.GetPixels32();
                for (var slot = 0; slot < 3; slot++)
                {
                    var centre = camera.WorldToScreenPoint(environment.GetSquall(slot).Center + Vector3.up * 110);
                    var changed = 0;
                    for (var y = Mathf.Max(0, (int)centre.y - 35); y < Mathf.Min(image.height, (int)centre.y + 35); y++)
                    for (var x = Mathf.Max(0, (int)centre.x - 45); x < Mathf.Min(image.width, (int)centre.x + 45); x++)
                    {
                        var i = y * image.width + x;
                        var a = (Color)pixels[i]; var b = (Color)clear[i];
                        if (Mathf.Abs(a.r - b.r) + Mathf.Abs(a.g - b.g) + Mathf.Abs(a.b - b.b) > .04f) changed++;
                        Assert.That(a.r > .9f && a.g < .1f && a.b > .9f, Is.False, "Cloud error shader detected.");
                    }
                    Assert.That(changed, Is.GreaterThan(30), "Missing volumetric cloud " + slot);
                }
                var directory = "/tmp/coherent-weather-linked-squalls-20261108";
                Directory.CreateDirectory(directory);
                File.WriteAllBytes(Path.Combine(directory, "three-squalls.png"), image.EncodeToPNG());
                environment.WeatherDriver = root.AddComponent<ControlledDriver>();
                var weather = environment.Weather;
                weather.Clouds.Coverage = 1;
                weather.WindSpeedMetresPerSecond = 25;
                weather.Squalls.FadeSeconds = 2;
                weather.Squalls.LightningBrightness = 0;
                environment.ApplyWeather(weather);
                environment.AdvanceSqualls(2);
                void UpdateVisuals() => typeof(WorldEnvironmentController)
                    .GetMethod("UpdateSquallVisuals", BindingFlags.Instance | BindingFlags.NonPublic)
                    .Invoke(environment, new object[] { 0f });
                float CloudResponse()
                {
                    var current = image.GetPixels32();
                    var difference = 0f;
                    for (var i = 0; i < current.Length; i++) difference += Mathf.Abs(current[i].r - clear[i].r)
                        + Mathf.Abs(current[i].g - clear[i].g) + Mathf.Abs(current[i].b - clear[i].b);
                    return difference;
                }
                UpdateVisuals();
                Render();
                var stormResponse = CloudResponse();
                File.WriteAllBytes(Path.Combine(directory, "storm-weather.png"), image.EncodeToPNG());
                weather.Clouds.Coverage = 0;
                weather.WindSpeedMetresPerSecond = 0;
                environment.ApplyWeather(weather);
                environment.AdvanceSqualls(1);
                UpdateVisuals();
                Render();
                var fadingResponse = CloudResponse();
                Assert.That(environment.Squall.Strength, Is.EqualTo(.5f));
                Assert.That(fadingResponse, Is.InRange(stormResponse * .1f, stormResponse * .98f),
                    "The actual volumetric cloud/rain must visibly fade with improving weather.");
                File.WriteAllBytes(Path.Combine(directory, "clearing-weather.png"), image.EncodeToPNG());
                environment.AdvanceSqualls(1);
                UpdateVisuals();
                Render();
                var clearResponse = CloudResponse();
                Assert.That(environment.ActiveSquallCount, Is.Zero);
                Assert.That(clearResponse, Is.LessThan(stormResponse * .01f));
                File.WriteAllBytes(Path.Combine(directory, "fair-weather.png"), image.EncodeToPNG());
                File.WriteAllText(Path.Combine(directory, "render-response.txt"),
                    $"Storm: {stormResponse}\nClearing: {fadingResponse}\nFair: {clearResponse}\n");
            }
            finally
            {
                RenderTexture.active = previous;
                Object.DestroyImmediate(cameraObject); Object.DestroyImmediate(texture); Object.DestroyImmediate(image);
            }
        }

        private void InvokeLifecycle(string method) => typeof(WorldEnvironmentController)
            .GetMethod(method, BindingFlags.Instance | BindingFlags.NonPublic).Invoke(environment, null);

        [Test]
        public void AnOffCameraLastCellDoesNotHideOtherVolumes()
        {
            var cameraObject = new GameObject("Multiple squall culling camera");
            try
            {
                var camera = cameraObject.AddComponent<Camera>();
                camera.enabled = false;
                camera.farClipPlane = 5000;
                environment.SpawnSquall(Vector3.forward * 1000, 300);
                environment.SpawnSquall(new Vector3(200, 0, 1000), 300);
                environment.SpawnSquall(Vector3.back * 1000, 300);
                Assert.That(SquallVisualRenderer.NeedsRender(camera), Is.True);
                camera.transform.rotation = Quaternion.Euler(0, 90, 0);
                Assert.That(SquallVisualRenderer.NeedsRender(camera), Is.False);
                environment.ClearSquall();
                camera.transform.rotation = Quaternion.identity;
                Assert.That(SquallVisualRenderer.NeedsRender(camera), Is.False);
            }
            finally { Object.DestroyImmediate(cameraObject); }
        }
    }
}
