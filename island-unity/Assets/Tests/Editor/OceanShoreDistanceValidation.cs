using System;
using UnityEditor;
using UnityEngine;
using UnityEngine.Rendering;
using Motu.Settings;
using Motu.World;

namespace Motu.Editor
{
    public static class OceanShoreDistanceValidation
    {
        public static void BatchValidateShoreDistance()
        {
            Require(SystemInfo.graphicsDeviceType != GraphicsDeviceType.Null,
                "Shore-distance validation requires a graphics device.");
            foreach (var resolution in new[] { 256, 512 })
            foreach (var coverage in new[] { 512f, 1024f })
                ValidateCoast(resolution, coverage, false);
            ValidateCoast(512, 1024f, true);
            ValidateDepthProtection();
            ValidateBreakerApproach();
            ValidateCoastalWaveCrossfade();
            Debug.Log("Ocean shore distance validation passed: waves beyond 100 m, 128 m cutoff, "
                + "physical wavelength, resolution/coverage independence, and river suppression.");
        }

        private static void ValidateCoastalWaveCrossfade()
        {
            var material = new Material(Shader.Find("Hidden/Motu/Ocean Shore Distance Probe"));
            var mask = new Texture2D(1, 1, TextureFormat.RGBAFloat, false, true);
            var onshore = new Texture2D(1, 1, TextureFormat.RGBAFloat, false, true);
            var target = RenderTexture.GetTemporary(1, 1, 0, RenderTextureFormat.ARGBFloat,
                RenderTextureReadWrite.Linear);
            var oldWind = Shader.GetGlobalVector("_MotuWeatherWind");
            var oldNoise = Shader.GetGlobalTexture("_MotuWindNoise");
            try
            {
                Shader.SetGlobalVector("_MotuWeatherWind", new Vector4(1, 0, 9, 1));
                Shader.SetGlobalTexture("_MotuWindNoise", Texture2D.grayTexture);
                material.SetFloat("_ProbeFullSurface", 1);
                material.SetFloat("_GeometricWaves", 1);
                material.SetFloat("_WaveFadeStart", 100);
                material.SetFloat("_WaveFadeEnd", 200);
                material.SetTexture("_WaveAttenuationTex", mask);
                material.SetTexture("_WaveOnshoreTex", onshore);
                material.SetVector("_WaveAttenuationWorldRect", new Vector4(0, 0, 1, 1));
                const float wavelength = 12f;
                var phase = Mathf.PI * .25f;
                material.SetVector("_OceanWave0", new Vector4(1, 0, wavelength, 1));
                material.SetVector("_OceanWaveFrom0", new Vector4(1, 0, wavelength, phase));
                material.SetFloat("_OnshoreWavePhase", phase);
                foreach (var depth in new[] { 5f, 7.5f, 10f })
                foreach (var distanceInfluence in new[] { 0f, .5f, 1f })
                foreach (var enabled in new[] { false, true })
                foreach (var shoreAmplitude in new[] { 0f, 2f })
                {
                    material.SetFloat("_OnshoreWaveEnabled", enabled ? 1 : 0);
                    material.SetVector("_OnshoreWaveParameters", new Vector4(wavelength, shoreAmplitude, 0, 0));
                    mask.SetPixel(0, 0, new Color(.2f, 1, depth / 10, 1));
                    mask.Apply();
                    // Ordinary waves travel on X; coastal waves on Z. Their
                    // independent slopes expose mismatched geometry/normal blends.
                    onshore.SetPixel(0, 0, new Color(.5f, 1, distanceInfluence, 0));
                    onshore.Apply();
                    Graphics.Blit(Texture2D.whiteTexture, target, material);
                    var actual = Read(target)[0];
                    var coastal = enabled && shoreAmplitude > 0
                        ? distanceInfluence * (1 - Mathf.SmoothStep(0, 1, (depth - 5) / 5)) : 0;
                    var ordinary = (enabled && shoreAmplitude > 0 ? 1 : .2f) * (1 - coastal);
                    var expectedHeight = Mathf.Sin(phase) * (ordinary + shoreAmplitude * coastal);
                    var slope = 2 * Mathf.PI / wavelength * Mathf.Cos(phase);
                    var expectedNormal = new Vector3(-slope * ordinary, 1, -slope * shoreAmplitude * coastal).normalized;
                    Require(Mathf.Abs(actual.r - expectedHeight) < .002f,
                        $"Coastal/swell height blend failed at depth {depth}, influence {distanceInfluence}, enabled {enabled}, amplitude {shoreAmplitude}.");
                    Require(Vector3.Distance(new Vector3(actual.g, actual.b, actual.a), expectedNormal) < .002f,
                        "Normals must use the same coastal/swell blend as displacement.");
                }
                Require(!ShaderUtil.ShaderHasError(material.shader), "Coastal crossfade shader failed to compile.");
                Debug.Log("Coastal crossfade passed: pure coastal, partial blend, full depth, outer distance, disabled/zero-amplitude coastal waves, and matching normals.");
            }
            finally
            {
                Shader.SetGlobalVector("_MotuWeatherWind", oldWind);
                Shader.SetGlobalTexture("_MotuWindNoise", oldNoise);
                RenderTexture.ReleaseTemporary(target);
                UnityEngine.Object.DestroyImmediate(material);
                UnityEngine.Object.DestroyImmediate(mask);
                UnityEngine.Object.DestroyImmediate(onshore);
            }
        }

        private static void ValidateBreakerApproach()
        {
            var profile = AssetDatabase.LoadAssetAtPath<OceanWaveProfile>("Assets/Settings/OceanWaveProfile.asset");
            Require(profile != null, "Missing active ocean profile for breaker validation.");
            var settings = profile.ToRuntimeSettings();
            Require(settings.Weather.OnshoreWaveBreakingStartDepthMetres == 5f
                && settings.Weather.OnshoreWaveBreakingFullDepthMetres == 4f
                && OceanWaveWeatherSettings.Default.OnshoreWaveBreakingFullDepthMetres == 3.5f,
                "Authored and fallback weather must use the earlier full-breaking depth.");
            Require(settings.OnshoreWaveSharpeningDistanceMetres == 96f
                && OceanWaveWeatherSettings.Default.OnshoreWaveSharpeningDistanceMetres == 96f,
                "The offshore breaker region must retain its 96 m default.");
            var weather = settings.Weather;
            weather.OnshoreWaveSharpeningDistanceMetres = 128f;
            Require(settings.WithWeather(weather).OnshoreWaveSharpeningDistanceMetres == 128f,
                "Runtime weather must support the full 128 m breaker region.");
            var material = new Material(Shader.Find("Hidden/Motu/Ocean Shore Distance Probe"));
            var onshore = new Texture2D(1, 1, TextureFormat.RGBAFloat, false, true);
            var mask = new Texture2D(1, 1, TextureFormat.RGBAFloat, false, true);
            var target = RenderTexture.GetTemporary(256, 1, 0, RenderTextureFormat.ARGBFloat,
                RenderTextureReadWrite.Linear);
            var previousWind = Shader.GetGlobalVector("_MotuWeatherWind");
            var previousNoise = Shader.GetGlobalTexture("_MotuWindNoise");
            try
            {
                Shader.SetGlobalVector("_MotuWeatherWind", new Vector4(1, 0, 9, 1));
                Shader.SetGlobalTexture("_MotuWindNoise", Texture2D.whiteTexture);
                material.SetVector("_OnshoreWaveParameters", new Vector4(12f, 12f, 0f, 0f));
                material.SetFloat("_GeometricWaves", 1f);
                material.SetFloat("_OnshoreWaveEnabled", 1f);
                material.SetFloat("_WhitecapHeightThreshold", .98f);
                material.SetFloat("_WhitecapSlopeThreshold", .12f);
                material.SetFloat("_WhitecapCoverage", 1f);
                material.SetFloat("_WhitecapStrength", 1f);
                material.SetTexture("_WaveAttenuationTex", mask);
                material.SetTexture("_WaveOnshoreTex", onshore);
                material.SetVector("_WaveAttenuationWorldRect", new Vector4(0, 0, 1, 1));
                onshore.SetPixel(0, 0, new Color(0f, .5f, 1f, 32f / 128f));
                onshore.Apply();
                float previousRatio = 1f, previousFoam = 0f, previousHeight = float.MaxValue;
                foreach (var depth in new[] { 5f, 3f, 1f, .25f, 0f })
                {
                    material.SetVector("_OnshoreWaveBreaking", new Vector4(.95f, 96f, 5f, .25f));
                    material.SetFloat("_ProbeBreakerShape", 1f);
                    material.SetFloat("_ProbeCoastDistance", 32f);
                    material.SetFloat("_ProbeWaterDepth", depth);
                    mask.SetPixel(0, 0, new Color(1f, 1f, depth / 10f, 1f));
                    mask.Apply();
                    Graphics.Blit(Texture2D.whiteTexture, target, material);
                    var shape = Read(target);
                    float frontSlope = 0f, rearSlope = 0f, foam = 0f;
                    var foamPixel = -1;
                    for (var pixel = 0; pixel < shape.Length; pixel++)
                    {
                        frontSlope = Mathf.Max(frontSlope, shape[pixel].g);
                        rearSlope = Mathf.Max(rearSlope, -shape[pixel].g);
                        foam = Mathf.Max(foam, shape[pixel].b);
                        if (shape[pixel].g > 0f && shape[pixel].r > .3f && shape[pixel].r < .8f
                            && (foamPixel < 0 || shape[pixel].b > shape[foamPixel].b))
                            foamPixel = pixel;
                    }
                    if (depth == 5f)
                    {
                        Require(foam < .0001f && Mathf.Abs(frontSlope - rearSlope) < .01f,
                            "Deep water must retain rounded waves without shallow-water breaking.");
                        continue;
                    }
                    Require(foamPixel >= 0, "Missing upper-front-face sample.");
                    var ratio = frontSlope / rearSlope;
                    Require(ratio >= previousRatio - .01f,
                        "The front must become steeper relative to the rear as depth decreases.");
                    previousRatio = ratio;
                    material.SetFloat("_ProbeBreakerShape", 0f);
                    material.SetFloat("_ProbeFullSurface", 2f);
                    material.SetFloat("_OnshoreWavePhase", (foamPixel + .5f) / 256f * 2f * Mathf.PI
                        - 32f / 12f * 2f * Mathf.PI);
                    Graphics.Blit(Texture2D.whiteTexture, target, material);
                    var breaking = Read(target)[0];
                    Require(Mathf.Abs(breaking.r) <= depth + .001f,
                        "Depth-driven breaking must retain seabed protection.");
                    if (depth > 0f)
                    {
                        Require((depth >= 1f ? breaking.g > previousFoam : breaking.g < previousFoam)
                            && Mathf.Abs(breaking.r) < previousHeight,
                            $"Foam must build as the wave shrinks: depth={depth}, foam={breaking.g}, height={breaking.r}.");
                        previousFoam = breaking.g;
                        previousHeight = Mathf.Abs(breaking.r);
                    }
                    else Require(breaking.g < .001f && Mathf.Abs(breaking.r) < .001f,
                        "Foam and displacement must disappear on dry land.");
                }
                // Move full breaking from 25 cm to 3.5 m at the same location.
                material.SetFloat("_ProbeWaterDepth", 3.5f);
                material.SetFloat("_ProbeBreakerShape", 1f);
                Graphics.Blit(Texture2D.whiteTexture, target, material);
                var lateBreaking = Read(target);
                material.SetVector("_OnshoreWaveBreaking", new Vector4(.95f, 96f, 5f, 3.5f));
                Graphics.Blit(Texture2D.whiteTexture, target, material);
                var earlyBreaking = Read(target);
                float lateSlope = 0f, earlySlope = 0f, lateFoam = 0f, earlyFoam = 0f;
                var upperFace = -1;
                for (var pixel = 0; pixel < earlyBreaking.Length; pixel++)
                {
                    lateSlope = Mathf.Max(lateSlope, lateBreaking[pixel].g);
                    earlySlope = Mathf.Max(earlySlope, earlyBreaking[pixel].g);
                    lateFoam = Mathf.Max(lateFoam, lateBreaking[pixel].b);
                    earlyFoam = Mathf.Max(earlyFoam, earlyBreaking[pixel].b);
                    if (earlyBreaking[pixel].r > .3f && earlyBreaking[pixel].r < .8f
                        && earlyBreaking[pixel].g > 0f)
                        upperFace = pixel;
                }
                Require(earlySlope > lateSlope * 2f && earlyFoam > lateFoam * 2f && upperFace >= 0,
                    "Increasing full-breaking depth must steepen the wave and increase foam in deeper water.");
                mask.SetPixel(0, 0, new Color(1, 1, 3.5f / 10f, 1));
                mask.Apply();
                material.SetFloat("_ProbeBreakerShape", 0f);
                material.SetFloat("_OnshoreWavePhase", (upperFace + .5f) / 256f * 2f * Mathf.PI
                    - 32f / 12f * 2f * Mathf.PI);
                Graphics.Blit(Texture2D.whiteTexture, target, material);
                var earlySurface = Read(target)[0];
                Require(earlySurface.r > 1f && earlySurface.r <= 3.5f && earlySurface.g > .5f,
                    "Configured early breaking must reach the rendered wave while it still has height.");

                // With no incoming wave, shallow water alone must not make foam.
                mask.SetPixel(0, 0, new Color(1, 1, .2f, 1));
                mask.Apply();
                Shader.SetGlobalVector("_MotuWeatherWind", new Vector4(1, 0, 0, 0));
                Graphics.Blit(Texture2D.whiteTexture, target, material);
                Require(Read(target)[0].g < .001f, "Calm water must not generate breaker foam.");
                Shader.SetGlobalVector("_MotuWeatherWind", new Vector4(1, 0, 9, 1));
                onshore.SetPixel(0, 0, new Color(0, .5f, 0, .25f));
                onshore.Apply();
                Graphics.Blit(Texture2D.whiteTexture, target, material);
                Require(Read(target)[0].g < .001f, "Suppressed river waves must not generate breaker foam.");
                Require(!ShaderUtil.ShaderHasError(material.shader), "Breaker shader failed to compile.");
                Debug.Log("Depth-driven breaker validation passed: stronger face compression and foam as "
                    + "depth decreases, shrinking protected waves, and no foam on land/in rivers/in calm water.");
            }
            finally
            {
                Shader.SetGlobalVector("_MotuWeatherWind", previousWind);
                Shader.SetGlobalTexture("_MotuWindNoise", previousNoise);
                RenderTexture.ReleaseTemporary(target);
                UnityEngine.Object.DestroyImmediate(onshore);
                UnityEngine.Object.DestroyImmediate(mask);
                UnityEngine.Object.DestroyImmediate(material);
            }
        }

        private static void ValidateDepthProtection()
        {
            var root = new GameObject("Ocean amplitude validation");
            var ocean = root.AddComponent<OceanSurfaceController>();
            var material = new Material(Shader.Find("Motu/Sea Water"));
            var probe = new Material(Shader.Find("Hidden/Motu/Ocean Shore Distance Probe"));
            var mask = new Texture2D(1, 1, TextureFormat.RGBAFloat, false, true);
            var onshore = new Texture2D(1, 1, TextureFormat.RGBAFloat, false, true);
            var target = RenderTexture.GetTemporary(1, 1, 0, RenderTextureFormat.ARGBFloat,
                RenderTextureReadWrite.Linear);
            var previousWind = Shader.GetGlobalVector("_MotuWeatherWind");
            var previousNoise = Shader.GetGlobalTexture("_MotuWindNoise");
            try
            {
                Shader.SetGlobalVector("_MotuWeatherWind", new Vector4(1, 0, 9, 1));
                Shader.SetGlobalTexture("_MotuWindNoise", Texture2D.grayTexture);
                ocean.Install(material, 128f, true);
                onshore.SetPixel(0, 0, new Color(0f, .5f, 1f, 0f));
                onshore.Apply();
                foreach (var shore in new[] { false, true })
                foreach (var amplitude in new[] { 12f, 24f })
                foreach (var depth in new[] { 0f, .1f, .2f, .5f, 1f })
                foreach (var phase in new[] { Mathf.PI * .25f, Mathf.PI * 1.5f })
                {
                    var weather = ocean.Weather;
                    weather.Enabled = true;
                    weather.AmplitudeVariation = 0f;
                    weather.DomainWarpMetres = 0f;
                    weather.Wave0 = new OceanWaveComponent(Vector2.right, 12f, shore ? 0f : amplitude, 0f);
                    weather.Wave1.AmplitudeMetres = 0f;
                    weather.Wave2.AmplitudeMetres = 0f;
                    weather.Wave3.AmplitudeMetres = 0f;
                    weather.OnshoreWaveEnabled = shore;
                    weather.OnshoreWaveAmplitudeMetres = amplitude;
                    weather.OnshoreWaveWavelengthMetres = 12f;
                    weather.OnshoreWaveChoppiness = 0f;
                    weather.OnshoreWaveLeadingEdgeSharpness = 0f;
                    ocean.ApplyWaveWeather(weather);
                    Require(ocean.Weather.OnshoreWaveAmplitudeMetres == amplitude,
                        "Runtime settings capped an onshore amplitude above 4 m.");
                    material.SetVector("_OceanWaveFrom0", new Vector4(1, 0, 12, phase));
                    material.SetVector("_OceanWaveTo0", new Vector4(1, 0, 12, phase));
                    material.SetFloat("_OnshoreWavePhase", phase);
                    mask.SetPixel(0, 0, new Color(1f, 1f, depth, 1f));
                    mask.Apply();
                    material.SetTexture("_WaveAttenuationTex", mask);
                    material.SetTexture("_WaveOnshoreTex", onshore);
                    material.SetVector("_WaveAttenuationWorldRect", new Vector4(0, 0, 1, 1));
                    probe.CopyPropertiesFromMaterial(material);
                    probe.SetFloat("_ProbeFullSurface", 1f);
                    Graphics.Blit(Texture2D.whiteTexture, target, probe);
                    var actual = Read(target)[0];
                    var depthScale = Mathf.Clamp01(depth * 10f / amplitude);
                    var coastalBlend = shore ? 1f - Mathf.SmoothStep(0f, 1f, Mathf.InverseLerp(5f, 10f, depth * 10f)) : 1f;
                    var expectedHeight = amplitude * Mathf.Sin(phase) * depthScale * coastalBlend;
                    var slope = amplitude * (2f * Mathf.PI / 12f) * Mathf.Cos(phase) * depthScale * coastalBlend;
                    var expectedNormal = new Vector3(slope, 1f, 0f).normalized;
                    Require(Mathf.Abs(actual.r - expectedHeight) < .005f,
                        $"Depth protection did not scale {amplitude} m waves (shore={shore}, depth={depth}).");
                    Require(actual.r >= -depth * 10f - .001f,
                        "The wave trough descended below the depth-map seabed.");
                    Require(Mathf.Abs(Mathf.Abs(actual.g) - Mathf.Abs(expectedNormal.x)) < .001f
                        && Mathf.Abs(actual.b - expectedNormal.y) < .001f,
                        "Ocean normals do not match depth-limited displacement.");
                    Require(ocean.SurfaceMesh.bounds.extents.y >= expectedHeight,
                        "Culling bounds do not include the larger wave height.");
                }
                Debug.Log("Ocean depth protection passed: 12/24 m ordinary and onshore waves, "
                    + "crests/troughs in 0-10 m depth, matching normals, and finite culling bounds.");
            }
            finally
            {
                Shader.SetGlobalVector("_MotuWeatherWind", previousWind);
                Shader.SetGlobalTexture("_MotuWindNoise", previousNoise);
                RenderTexture.ReleaseTemporary(target);
                UnityEngine.Object.DestroyImmediate(mask);
                UnityEngine.Object.DestroyImmediate(onshore);
                UnityEngine.Object.DestroyImmediate(probe);
                UnityEngine.Object.DestroyImmediate(root);
            }
        }

        private static void ValidateCoast(int resolution, float coverage, bool river)
        {
            var attenuation = new Material(Shader.Find("Hidden/Motu/Ocean Wave Attenuation"));
            var direction = new Material(Shader.Find("Hidden/Motu/Ocean Onshore Direction"));
            var probe = new Material(Shader.Find("Hidden/Motu/Ocean Shore Distance Probe"));
            var seaMask = new Texture2D(resolution, 8, TextureFormat.RGBA32, false, true)
            { wrapMode = TextureWrapMode.Clamp, filterMode = FilterMode.Bilinear };
            var composed = RenderTexture.GetTemporary(resolution, 8, 0,
                RenderTextureFormat.ARGB32, RenderTextureReadWrite.Linear);
            var onshore = RenderTexture.GetTemporary(resolution, 8, 0,
                RenderTextureFormat.ARGB32, RenderTextureReadWrite.Linear);
            var heights = RenderTexture.GetTemporary(resolution, 8, 0,
                RenderTextureFormat.ARGBFloat, RenderTextureReadWrite.Linear);
            var previous = RenderTexture.active;
            var previousWind = Shader.GetGlobalVector("_MotuWeatherWind");
            try
            {
                composed.wrapMode = onshore.wrapMode = TextureWrapMode.Clamp;
                var pixels = new Color[resolution * 8];
                for (var y = 0; y < 8; y++)
                for (var x = 0; x < resolution; x++)
                {
                    // Straight coast 32 m from the left edge; five-metre water offshore.
                    var distance = (x + .5f) * coverage / resolution - 32f;
                    pixels[y * resolution + x] = new Color(distance <= 0f ? 1f : .5f,
                        Mathf.Clamp01(distance / 128f), river ? 1f : 0f, 1f);
                }
                seaMask.SetPixels(pixels);
                seaMask.Apply();
                var rectangle = new Vector4(-coverage / 2f, -coverage / 2f, coverage, coverage);
                attenuation.SetTexture("_SeaMask", seaMask);
                attenuation.SetMatrix("_IslandWorldToLocal", Matrix4x4.identity);
                attenuation.SetFloat("_IslandWorldSize", coverage);
                attenuation.SetVector("_CompositionWorldRect", rectangle);
                // Non-default exponents must not distort the shore-wave phase.
                attenuation.SetFloat("_DepthAllowancePower", 1.35f);
                attenuation.SetFloat("_DistanceAllowancePower", 2f);
                RenderTexture.active = composed;
                GL.Clear(false, true, Color.white);
                Graphics.Blit(Texture2D.whiteTexture, composed, attenuation);
                direction.SetVector("_CompositionWorldRect", rectangle);
                Graphics.Blit(composed, onshore, direction);
                var coast = Read(onshore);
                var mask = Read(composed);
                int Pixel(float distance) => Mathf.FloorToInt((distance + 32f) * resolution / coverage);
                if (river)
                {
                    Require(coast[Pixel(100)].b < .001f && mask[Pixel(100)].r < .001f,
                        "Carved rivers must suppress both shore waves and ordinary swell.");
                    return;
                }
                Require(coast[Pixel(100)].b > .8f && coast[Pixel(112)].b > .3f
                    && coast[Pixel(120)].b > .02f,
                    $"Shore waves disappeared beyond 100 m ({resolution}, {coverage}).");
                Require(coast[Pixel(132)].b < .001f && coast[Pixel(-8)].b < .001f,
                    "Shore waves must fade out beyond 128 m and on land.");
                Require(coast[Pixel(8)].b > .99f,
                    "Coastal waves must keep ownership in the final metres approaching shore.");
                Require(coast[Pixel(64)].r < .01f && Mathf.Abs(coast[Pixel(64)].g - .5f) < .01f,
                    "Waves must travel towards the straight shoreline.");
                Require(mask[Pixel(32)].r > .99f,
                    "Extending the distance field must retain the ordinary swell attenuation range.");

                Shader.SetGlobalVector("_MotuWeatherWind", new Vector4(1f, 0f, 9f, 1f));
                probe.SetTexture("_WaveOnshoreTex", onshore);
                probe.SetTexture("_WaveAttenuationTex", composed);
                probe.SetVector("_WaveAttenuationWorldRect", new Vector4(0f, 0f, 1f, 1f));
                probe.SetFloat("_OnshoreWaveEnabled", 1f);
                probe.SetVector("_OnshoreWaveParameters", new Vector4(12f, 1f, 0f, 0f));
                probe.SetVector("_OnshoreWaveBreaking", Vector4.zero);
                probe.SetFloat("_OnshoreWavePhase", .3f);
                Graphics.Blit(Texture2D.whiteTexture, heights, probe);
                var wave = Read(heights);
                foreach (var distance in new[] { 32f, 64f, 100f, 112f })
                {
                    var pixel = Pixel(distance);
                    var actualDistance = (pixel + .5f) * coverage / resolution - 32f;
                    Require(Mathf.Abs(coast[pixel].a * 128f - actualDistance) < .6f,
                        "The composed coast coordinate must preserve linear distance in metres.");
                    var expected = Mathf.Sin(coast[pixel].a * 128f * 2f * Mathf.PI / 12f + .3f)
                        * coast[pixel].b;
                    Require(Mathf.Abs(wave[pixel].r - expected) < .003f,
                        "The ocean shader stretched the authored 12 m wavelength.");
                }
                foreach (var material in new[] { attenuation, direction, probe })
                    Require(!ShaderUtil.ShaderHasError(material.shader), "A coastal shader failed to compile.");
            }
            finally
            {
                Shader.SetGlobalVector("_MotuWeatherWind", previousWind);
                RenderTexture.active = previous;
                RenderTexture.ReleaseTemporary(composed);
                RenderTexture.ReleaseTemporary(onshore);
                RenderTexture.ReleaseTemporary(heights);
                UnityEngine.Object.DestroyImmediate(seaMask);
                UnityEngine.Object.DestroyImmediate(attenuation);
                UnityEngine.Object.DestroyImmediate(direction);
                UnityEngine.Object.DestroyImmediate(probe);
            }
        }

        private static Color[] Read(RenderTexture source)
        {
            var previous = RenderTexture.active;
            var texture = new Texture2D(source.width, source.height, TextureFormat.RGBAFloat, false, true);
            try
            {
                RenderTexture.active = source;
                texture.ReadPixels(new Rect(0, 0, source.width, source.height), 0, 0);
                texture.Apply();
                return texture.GetPixels();
            }
            finally
            {
                RenderTexture.active = previous;
                UnityEngine.Object.DestroyImmediate(texture);
            }
        }

        private static void Require(bool success, string message)
        {
            if (!success) throw new InvalidOperationException(message);
        }
    }
}
