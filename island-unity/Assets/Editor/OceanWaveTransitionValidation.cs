using System;
using System.Reflection;
using UnityEditor;
using UnityEngine;
using UnityEngine.Rendering;

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
            weather.WhitecapCounterflowSpeed = 1.5f;
            ocean.ApplyWaveWeather(weather);
            ocean.ApplyWeatherWindScale(2f);
            Advance(ocean, 0f);
            Require(material.GetVector("_OceanWaveFrom0").w == phase
                && material.GetFloat("_OnshoreWavePhase") == shorePhase
                && material.GetVector("_OceanFoamTravel") == foam,
                "Speed or wind-strength changes rewrote elapsed wave/foam travel.");
            Advance(ocean, .25f);
            Require(PhaseError(material.GetVector("_OceanWaveFrom0").w,
                    phase + .25f * 7f * 2f * 2f * Mathf.PI / 30f) < .00001f,
                "Directional phase did not integrate the new speed over delta time.");
            Require(PhaseError(material.GetFloat("_OnshoreWavePhase"),
                    shorePhase + .25f * 9f * 2f * 2f * Mathf.PI / weather.OnshoreWaveWavelengthMetres) < .00001f,
                "Onshore phase did not integrate the new speed over delta time.");
            var newFoam = material.GetVector("_OceanFoamTravel");
            Require(Mathf.Abs(newFoam.x - foam.x + 3.5f) < .01f
                && Mathf.Abs(newFoam.z - foam.z - 5.25f) < .01f,
                "Foam advection did not integrate speed and counterflow continuously.");
            var stoppedPhase = material.GetVector("_OceanWaveFrom0").w;
            ocean.ApplyWeatherWindScale(0f);
            Advance(ocean, 5f);
            Require(material.GetVector("_OceanWaveFrom0").w == stoppedPhase,
                "Calm weather must stop wave travel without resetting phase.");
            var fieldBeforeRecentering = Render(probe, material, origins[1]);
            root.transform.position = new Vector3(5000, 0, -5000);
            Advance(ocean, 0f);
            Require(ocean.SurfaceMesh == mesh
                && MaxDifference(fieldBeforeRecentering, Render(probe, material, origins[1])) < .00001f,
                "Recentering the ocean mesh changed the world-space wave field.");
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

    private static void Advance(OceanSurfaceController ocean, float deltaTime)
    {
        typeof(OceanSurfaceController).GetMethod("AdvanceWaveAnimation", BindingFlags.NonPublic | BindingFlags.Instance)
            .Invoke(ocean, new object[] { deltaTime });
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
