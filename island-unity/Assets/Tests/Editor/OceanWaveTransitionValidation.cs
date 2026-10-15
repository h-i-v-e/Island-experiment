using System;
using System.Reflection;
using UnityEditor;
using UnityEngine;
using UnityEngine.Rendering;
using Motu.Settings;
using Motu.World;

namespace Motu.Editor
{
    public static class OceanWaveTransitionValidation
    {
        public static void BatchValidateWaveTransitions()
        {
            Require(SystemInfo.graphicsDeviceType != GraphicsDeviceType.Null,
                "Wave transition validation requires a graphics device.");
            var root = new GameObject("Ocean wave transition validation");
            var ocean = root.AddComponent<OceanSurfaceController>();
            var seaShader = Shader.Find("Motu/Sea Water");
            var probeShader = Shader.Find("Hidden/Motu/Ocean Wave Transition Probe");
            Require(seaShader != null && probeShader != null, "Missing wave validation shaders.");
            var material = new Material(seaShader);
            var probe = new Material(probeShader);
            var previousNoise = Shader.GetGlobalTexture("_MotuWindNoise");
            var previousWind = Shader.GetGlobalVector("_MotuWeatherWind");
            var previousOffset = Shader.GetGlobalVector("_MotuWindOffset");
            try
            {
                Shader.SetGlobalTexture("_MotuWindNoise", Texture2D.grayTexture);
                Shader.SetGlobalVector("_MotuWeatherWind", new Vector4(1, 0, 9, 1));
                Shader.SetGlobalVector("_MotuWindOffset", Vector4.zero);
                ocean.Install(material, 128f, true);
                var weather = ocean.Weather;
                weather.Wave0 = new OceanWaveComponent(Vector2.right, 30f, 1f, 0f);
                weather.Wave1 = new OceanWaveComponent(Vector2.up, 15f, 0f, 0f);
                weather.Wave2 = new OceanWaveComponent(Vector2.left, 7.5f, 0f, 0f);
                weather.Wave3 = new OceanWaveComponent(Vector2.down, 4f, 0f, 0f);
                weather.DomainWarpMetres = 0f;
                weather.AmplitudeVariation = 0f;
                weather.OnshoreWaveSpeedMetresPerSecond = 0f;
                ocean.ApplyWaveWeather(weather);
                ocean.ApplyWeatherWindScale(1f);
                Advance(ocean, 0f);
                var mesh = ocean.SurfaceMesh;
                var origins = new[] { Vector2.zero, new Vector2(10000, -20000), new Vector2(1000000, 1000000) };
                var baseline = new Color[origins.Length][];
                for (var index = 0; index < origins.Length; index++)
                    baseline[index] = Render(probe, material, origins[index]);

                // Wind direction alone must not rotate any wave bank or GPU field.
                Shader.SetGlobalVector("_MotuWeatherWind", new Vector4(0, 1, 9, 1));
                Advance(ocean, 4f);
                for (var index = 0; index < origins.Length; index++)
                    Require(MaxDifference(baseline[index], Render(probe, material, origins[index])) < .00001f,
                        "Wind direction rotated the wave field.");
                Require(material.GetFloat("_OceanWaveTransition") == 0f,
                    "Wind direction started an unwanted wave transition.");
                weather.Wave0.Direction = Vector2.up;
                ocean.ApplyWaveWeather(weather);
                Advance(ocean, 0f);
                for (var index = 0; index < origins.Length; index++)
                    Require(MaxDifference(baseline[index], Render(probe, material, origins[index])) < 0.00001f,
                        "Changing a scripted wave direction jumped the field before its transition.");
                var outgoing = material.GetVector("_OceanWaveFrom0");
                var incoming = material.GetVector("_OceanWaveTo0");
                Require(outgoing.x == 1f && outgoing.y == 0f && incoming.x == 0f && incoming.y == 1f,
                    "The wave banks must capture fixed old and new world directions.");

                Advance(ocean, .1f);
                var blend = material.GetFloat("_OceanWaveTransition");
                Require(blend > 0f && blend < .01f, "Missing smooth direction transition.");
                float greatestHeightChange = 0f;
                for (var index = 0; index < origins.Length; index++)
                {
                    var actual = Render(probe, material, origins[index]);
                    var destination = Render(probe, material, origins[index], 1f);
                    for (var pixel = 0; pixel < actual.Length; pixel++)
                    {
                        var expected = Color.Lerp(baseline[index][pixel], destination[pixel], blend);
                        for (var channel = 0; channel < 3; channel++)
                            Require(Mathf.Abs(actual[pixel][channel] - expected[channel]) < .0001f,
                                "Geometry or normals do not use the same spatially uniform transition.");
                        var heightChange = Mathf.Abs(actual[pixel].r - baseline[index][pixel].r);
                        greatestHeightChange = Mathf.Max(greatestHeightChange, heightChange);
                        Require(heightChange <= 2f * blend + .0001f,
                            "Wave direction changes grew beyond the amplitude-based bound with distance.");
                    }
                }

                var beforeRetarget = Render(probe, material, origins[2]);
                weather.Wave0.Direction = Vector2.left;
                ocean.ApplyWaveWeather(weather);
                Advance(ocean, 0f);
                Require(material.GetVector("_OceanWaveFrom0") == outgoing
                    && material.GetVector("_OceanWaveTo0") == incoming
                    && MaxDifference(beforeRetarget, Render(probe, material, origins[2])) < .00001f,
                    "A changing driver rotated or reset an in-progress transition.");
                var destinationField = Render(probe, material, origins[2], 1f);
                Advance(ocean, 3.9f);
                Require(MaxDifference(destinationField, Render(probe, material, origins[2])) < .00001f,
                    "Completing a wave transition caused a phase discontinuity.");
                Advance(ocean, 0f);
                Require(material.GetVector("_OceanWaveFrom0").y == 1f
                    && material.GetVector("_OceanWaveTo0").x == -1f
                    && material.GetFloat("_OceanWaveTransition") == 0f,
                    "The next transition did not pick up the latest requested direction.");
                Advance(ocean, 4f);

                weather.Wave0.SpeedMetresPerSecond = 3f;
                weather.OnshoreWaveSpeedMetresPerSecond = 2f;
                ocean.ApplyWaveWeather(weather);
                Advance(ocean, 10000.125f);
                var phase = material.GetVector("_OceanWaveFrom0").w;
                var shorePhase = material.GetFloat("_OnshoreWavePhase");
                var foam = material.GetVector("_OceanFoamTravel");
                weather.Wave0.SpeedMetresPerSecond = 7f;
                weather.OnshoreWaveSpeedMetresPerSecond = 9f;
                weather.WhitecapDistortionSpeed = 1.5f;
                ocean.ApplyWaveWeather(weather);
                ocean.ApplyWeatherWindScale(2f);
                Advance(ocean, 0f);
                Require(material.GetVector("_OceanWaveFrom0").w == phase
                    && material.GetFloat("_OnshoreWavePhase") == shorePhase
                    && material.GetVector("_OceanFoamTravel") == foam,
                    "Speed or wind-strength changes rewrote elapsed wave/foam travel.");
                Advance(ocean, .25f);
                Require(PhaseError(material.GetVector("_OceanWaveFrom0").w,
                        phase - .25f * 7f * 2f * 2f * Mathf.PI / 30f) < .00001f,
                    "Directional phase did not integrate the new speed over delta time.");
                Require(PhaseError(material.GetFloat("_OnshoreWavePhase"),
                        shorePhase + .25f * 9f * 2f * 2f * Mathf.PI / weather.OnshoreWaveWavelengthMetres) < .00001f,
                    "Onshore phase did not integrate the new speed over delta time.");
                var newFoam = material.GetVector("_OceanFoamTravel");
                Require(Mathf.Abs(newFoam.x - foam.x + .525f) < .01f
                    && PhaseError(newFoam.z, foam.z + .75f) < .00001f,
                    "Foam must drift with crests at 15% speed and integrate distortion continuously.");
                var stoppedFoam = material.GetVector("_OceanFoamTravel");
                var stoppedPhase = material.GetVector("_OceanWaveFrom0").w;
                ocean.ApplyWeatherWindScale(0f);
                Advance(ocean, 5f);
                Require(material.GetVector("_OceanWaveFrom0").w == stoppedPhase
                    && material.GetVector("_OceanFoamTravel") == stoppedFoam,
                    "Calm weather must stop wave travel without resetting phase.");
                var fieldBeforeRecentering = Render(probe, material, origins[1]);
                root.transform.position = new Vector3(5000, 0, -5000);
                Advance(ocean, 0f);
                Require(ocean.SurfaceMesh == mesh
                    && MaxDifference(fieldBeforeRecentering, Render(probe, material, origins[1])) < .00001f,
                    "Recentering the ocean mesh changed the world-space wave field.");
                ValidateTravelDirection(ocean, material, probe);
                BatchValidateDistantWaveTransitionAmplitude();
                var compiled = false;
                for (var pass = 0; pass < material.passCount; pass++) compiled |= material.SetPass(pass);
                Require(compiled && !ShaderUtil.ShaderHasError(seaShader) && !ShaderUtil.ShaderHasError(probeShader),
                    "Ocean transition shaders failed to compile.");
                Debug.Log($"Ocean wave transitions passed GPU validation at 0, 10km and 1000km: max initial height change={greatestHeightChange}, bound={2f * blend}; normals, retargets, phase integration, foam, calm wind and mesh recentering passed.");
            }
            finally
            {
                Shader.SetGlobalTexture("_MotuWindNoise", previousNoise);
                Shader.SetGlobalVector("_MotuWeatherWind", previousWind);
                Shader.SetGlobalVector("_MotuWindOffset", previousOffset);
                UnityEngine.Object.DestroyImmediate(probe);
                UnityEngine.Object.DestroyImmediate(root);
            }
        }

        private static float PhaseError(float actual, float expected)
        {
            return Mathf.Abs(Mathf.DeltaAngle(actual * Mathf.Rad2Deg, expected * Mathf.Rad2Deg)) * Mathf.Deg2Rad;
        }

        public static void BatchValidateDistantWaveTransitionAmplitude()
        {
            var previousNoise = Shader.GetGlobalTexture("_MotuWindNoise");
            var previousWind = Shader.GetGlobalVector("_MotuWeatherWind");
            var previousOffset = Shader.GetGlobalVector("_MotuWindOffset");
            var probe = new Material(Shader.Find("Hidden/Motu/Ocean Wave Transition Probe"));
            try
            {
                Shader.SetGlobalTexture("_MotuWindNoise", Texture2D.grayTexture);
                Shader.SetGlobalVector("_MotuWeatherWind", new Vector4(0, 1, 9, 1));
                Shader.SetGlobalVector("_MotuWindOffset", Vector4.zero);
                foreach (var distance in new[] { 0f, 4096f, -4096f, 8192f, 16384f })
                {
                    var root = new GameObject("Distant wave transition");
                    var ocean = root.AddComponent<OceanSurfaceController>();
                    var material = new Material(Shader.Find("Motu/Sea Water"));
                    try
                    {
                        root.transform.position = new Vector3(distance, 0, 0);
                        ocean.Install(material, 512, true);
                        var weather = ocean.Weather;
                        weather.Wave0 = new OceanWaveComponent(Vector2.up, 30, 1, 0);
                        weather.Wave1 = new OceanWaveComponent(Vector2.right, 15, 0, 0);
                        weather.Wave2 = new OceanWaveComponent(Vector2.left, 8, 0, 0);
                        weather.Wave3 = new OceanWaveComponent(Vector2.down, 4, 0, 0);
                        weather.DomainWarpMetres = 0;
                        weather.AmplitudeVariation = 0;
                        weather.OnshoreWaveEnabled = false;
                        ocean.ApplyWaveWeather(weather);
                        ocean.ApplyWeatherWindScale(1);
                        Advance(ocean, 0);
                        var origin = new Vector2(distance, 0);
                        var baseline = Render(probe, material, origin);
                        // A small turn puts origin-aligned banks half a cycle apart here.
                        float angle = 30f / (2f * Mathf.Max(Mathf.Abs(distance), 4096f));
                        weather.Wave0.Direction = new Vector2(Mathf.Sin(angle), Mathf.Cos(angle));
                        ocean.ApplyWaveWeather(weather);
                        Advance(ocean, 0);
                        Require(MaxDifference(baseline, Render(probe, material, origin)) < .00001f,
                            "Starting a direction transition must preserve the current field.");
                        var from = material.GetVector("_OceanWaveFrom0");
                        var to = material.GetVector("_OceanWaveTo0");
                        float baselineEnergy = HeightEnergy(baseline);
                        foreach (var blend in new[] { .25f, .5f, .75f })
                        {
                            float energy = HeightEnergy(Render(probe, material, origin, blend));
                            Require(energy / baselineEnergy > .95f,
                                $"Small wind shifts must not cancel distant waves: distance={distance}, blend={blend}, energy ratio={energy / baselineEnergy}.");
                        }
                        // Moving the ocean mesh must not rewrite a transition already underway.
                        var beforeMove = Render(probe, material, origin, .5f);
                        root.transform.position += new Vector3(16, 0, 16);
                        Advance(ocean, 0);
                        Require(material.GetVector("_OceanWaveFrom0") == from
                            && material.GetVector("_OceanWaveTo0") == to
                            && MaxDifference(beforeMove, Render(probe, material, origin, .5f)) < .00001f,
                            "Recentering must preserve captured wave phases.");
                        var destination = Render(probe, material, origin, 1);
                        Advance(ocean, 4);
                        Require(MaxDifference(destination, Render(probe, material, origin)) < .00001f,
                            "Completing a distant transition must preserve its destination phase.");
                        Debug.Log($"Distant wave transition retains amplitude at {distance} metres.");
                    }
                    finally { UnityEngine.Object.DestroyImmediate(root); }
                }
            }
            finally
            {
                UnityEngine.Object.DestroyImmediate(probe);
                Shader.SetGlobalTexture("_MotuWindNoise", previousNoise);
                Shader.SetGlobalVector("_MotuWeatherWind", previousWind);
                Shader.SetGlobalVector("_MotuWindOffset", previousOffset);
            }
        }

        private static float HeightEnergy(Color[] field)
        {
            float energy = 0;
            foreach (var pixel in field) energy += pixel.r * pixel.r;
            return energy / field.Length;
        }

        private static void ValidateTravelDirection(OceanSurfaceController ocean, Material material, Material probe)
        {
            var weather = ocean.Weather;
            weather.Wave0.SpeedMetresPerSecond = 3f;
            weather.Wave0.Choppiness = 0f;
            weather.OnshoreWaveEnabled = false;
            ocean.ApplyWeatherWindScale(1f);
            foreach (var direction in new[] { Vector2.right, Vector2.up, Vector2.left,
                Vector2.down, new Vector2(1f, 1f).normalized })
            {
                weather.Wave0.Direction = direction;
                ocean.ApplyWaveWeather(weather);
                Advance(ocean, 4f);
                var expected = Render(probe, material, -direction * .75f);
                var foam = material.GetVector("_OceanFoamTravel");
                Advance(ocean, .25f);
                Require(MaxDifference(expected, Render(probe, material, Vector2.zero)) < .00002f,
                    "GPU wave heights and normals must travel towards the authored direction.");
                var drift = material.GetVector("_OceanFoamTravel") - foam;
                Require(Vector2.Distance(new Vector2(drift.x, drift.y), direction * .1125f) < .001f,
                    "Foam must drift towards the same direction as the wave crests.");
            }
            ValidateQueryVelocity(ocean, material, probe);
        }

        private static void ValidateQueryVelocity(OceanSurfaceController ocean, Material material, Material probe)
        {
            var queryShader = Shader.Find("Hidden/Motu/Ocean Surface Query");
            Require(queryShader != null, "Missing ocean surface query shader.");
            var query = new Material(queryShader);
            var positions = new Texture2D(1, 1, TextureFormat.RGBAFloat, false, true);
            var target = RenderTexture.GetTemporary(1, 2, 0, RenderTextureFormat.ARGBFloat, RenderTextureReadWrite.Linear);
            var output = new Texture2D(1, 2, TextureFormat.RGBAFloat, false, true);
            var previous = RenderTexture.active;
            var from = material.GetVector("_OceanWaveFrom0");
            var to = material.GetVector("_OceanWaveTo0");
            try
            {
                var delta = .02f * ocean.Weather.Wave0.SpeedMetresPerSecond * 2f * Mathf.PI / from.z;
                material.SetVector("_OceanWaveFrom0", from + new Vector4(0, 0, 0, delta));
                material.SetVector("_OceanWaveTo0", to + new Vector4(0, 0, 0, delta));
                var before = SampleHeight(probe, material);
                material.SetVector("_OceanWaveFrom0", from - new Vector4(0, 0, 0, delta));
                material.SetVector("_OceanWaveTo0", to - new Vector4(0, 0, 0, delta));
                var after = SampleHeight(probe, material);
                material.SetVector("_OceanWaveFrom0", from);
                material.SetVector("_OceanWaveTo0", to);
                query.CopyPropertiesFromMaterial(material);
                positions.SetPixel(0, 0, Color.clear);
                positions.Apply();
                query.SetTexture("_QueryPositions", positions);
                query.SetVector("_QueryOceanOrigin", Vector4.zero);
                query.SetFloat("_QueryWaveTimeScale", 1f);
                query.EnableKeyword("MOTU_QUERY_VELOCITY");
                Graphics.Blit(Texture2D.blackTexture, target, query);
                RenderTexture.active = target;
                output.ReadPixels(new Rect(0, 0, 1, 2), 0, 0);
                output.Apply();
                Require(Mathf.Abs(output.GetPixel(0, 1).g - (after - before) / .04f) < .001f,
                    "GPU physics-query velocity must predict the visible wave's travel direction.");
                Require(!ShaderUtil.ShaderHasError(queryShader), "Ocean surface query shader failed to compile.");
            }
            finally
            {
                material.SetVector("_OceanWaveFrom0", from);
                material.SetVector("_OceanWaveTo0", to);
                RenderTexture.active = previous;
                RenderTexture.ReleaseTemporary(target);
                UnityEngine.Object.DestroyImmediate(output);
                UnityEngine.Object.DestroyImmediate(positions);
                UnityEngine.Object.DestroyImmediate(query);
            }
        }

        private static float SampleHeight(Material probe, Material material)
        {
            probe.CopyPropertiesFromMaterial(material);
            probe.SetVector("_ProbeWorldRect", Vector4.zero);
            var target = RenderTexture.GetTemporary(1, 1, 0, RenderTextureFormat.ARGBFloat, RenderTextureReadWrite.Linear);
            var output = new Texture2D(1, 1, TextureFormat.RGBAFloat, false, true);
            var previous = RenderTexture.active;
            try
            {
                Graphics.Blit(Texture2D.blackTexture, target, probe);
                RenderTexture.active = target;
                output.ReadPixels(new Rect(0, 0, 1, 1), 0, 0);
                output.Apply();
                return output.GetPixel(0, 0).r;
            }
            finally
            {
                RenderTexture.active = previous;
                RenderTexture.ReleaseTemporary(target);
                UnityEngine.Object.DestroyImmediate(output);
            }
        }

        private static void Advance(OceanSurfaceController ocean, float deltaTime)
        {
            ocean.AdvanceWaveAnimation(deltaTime);
        }

        private static Color[] Render(Material probe, Material material, Vector2 origin, float? blend = null)
        {
            probe.CopyPropertiesFromMaterial(material);
            probe.SetVector("_ProbeWorldRect", new Vector4(origin.x, origin.y, 123f, 117f));
            if (blend.HasValue) probe.SetFloat("_OceanWaveTransition", blend.Value);
            var target = RenderTexture.GetTemporary(64, 64, 0, RenderTextureFormat.ARGBFloat, RenderTextureReadWrite.Linear);
            var readback = new Texture2D(64, 64, TextureFormat.RGBAFloat, false, true);
            var previous = RenderTexture.active;
            try
            {
                Graphics.Blit(Texture2D.blackTexture, target, probe);
                RenderTexture.active = target;
                readback.ReadPixels(new Rect(0, 0, 64, 64), 0, 0);
                readback.Apply();
                return readback.GetPixels();
            }
            finally
            {
                RenderTexture.active = previous;
                RenderTexture.ReleaseTemporary(target);
                UnityEngine.Object.DestroyImmediate(readback);
            }
        }

        private static float MaxDifference(Color[] first, Color[] second)
        {
            float difference = 0f;
            for (var pixel = 0; pixel < first.Length; pixel++)
                for (var channel = 0; channel < 3; channel++)
                    difference = Mathf.Max(difference, Mathf.Abs(first[pixel][channel] - second[pixel][channel]));
            return difference;
        }

        private static void Require(bool success, string message)
        {
            if (!success) throw new InvalidOperationException(message);
        }
    }
}
