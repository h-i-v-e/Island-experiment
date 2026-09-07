using System;
using UnityEditor;
using UnityEngine;

public static class VegetationWindValidation
{
    public static void BatchValidateSharedWind()
    {
        WorldWeatherValidation.BatchValidateRuntimeWeather();
        var root = new GameObject("Shared vegetation wind GPU validation");
        var environment = root.AddComponent<WorldEnvironmentController>();
        var probe = new Material(Shader.Find("Hidden/Motu/Vegetation Wind Probe"));
        var target = new RenderTexture(1, 1, 0, RenderTextureFormat.ARGBFloat, RenderTextureReadWrite.Linear);
        var readback = new Texture2D(1, 1, TextureFormat.RGBAFloat, false, true);
        var previous = RenderTexture.active;
        try
        {
            environment.Initialize(new WorldEnvironmentSettings(), new IslandCloudSettings(), 64f, 128f, null);
            probe.SetFloat("_WorldSize", 2000f);
            var weather = environment.Weather;
            foreach (var noise in new[] { Texture2D.blackTexture, Texture2D.grayTexture, Texture2D.whiteTexture })
            {
                Shader.SetGlobalTexture("_MotuWindNoise", noise);
                foreach (var direction in new[] { Vector2.right, Vector2.up, Vector2.left, Vector2.down, new Vector2(-1, 1).normalized })
                {
                    float reference = 0f;
                    foreach (var speed in new[] { 0f, 9f, 36f })
                    {
                        weather.WindDirection = direction;
                        weather.WindSpeedMetresPerSecond = speed;
                        environment.ApplyWeather(weather);
                        var sample = Read(0, new Vector2(123, -456));
                        var common = new Vector2(sample.r, sample.g);
                        var tree = new Vector2(sample.b, sample.a);
                        Require((common - tree).magnitude < .0001f,
                            "Trees and the common vegetation field use different wind strength or direction.");
                        if (speed == 0f)
                            Require(common.magnitude < .00001f, "Vegetation still moves in calm weather.");
                        else
                        {
                            Require(Vector2.Dot(common.normalized, direction) > .9999f,
                                "GPU vegetation displacement disagrees with the configured world X/Z wind direction.");
                            if (speed == 9f) reference = common.magnitude;
                            else Require(Mathf.Abs(common.magnitude - reference * 2f) < .0001f,
                                "Vegetation does not share the speed-derived wind response.");
                        }
                        var pinned = Read(3, Vector2.zero);
                        Require(Mathf.Abs(pinned.r) + Mathf.Abs(pinned.g) + Mathf.Abs(pinned.b) < .00001f,
                            "Shared wind moved a pinned tree root.");
                    }
                }
            }
            var origin = new Vector2(743, -851);
            var travel = new Vector2(180, -90);
            Shader.SetGlobalVector("_MotuCloudWindOffset", Vector4.zero);
            var cloudBefore = Read(1, origin);
            var broadBefore = Read(2, origin);
            Shader.SetGlobalVector("_MotuCloudWindOffset", new Vector4(travel.x, travel.y, travel.x * .18f, travel.y * .18f));
            Require(Difference(cloudBefore, Read(1, origin + travel)) < .00001f,
                "Cloud texture moves against the shared wind.");
            Require(Difference(broadBefore, Read(2, origin + travel * .18f)) < .00001f,
                "Broad clouds move against the shared wind.");
            foreach (var name in new[] { "Motu/Terrain Grass", "Motu/Terrain Unified", "Motu/Tree Wood", "Motu/Tree Foliage", "Motu/Tree Foliage Distant", "Motu/Riverbank Reeds", "Motu/Forest Ferns", "Motu/Planar Reflection Simplified" })
            {
                var shader = Shader.Find(name);
                Require(shader != null && shader.isSupported, $"Wind receiver shader is unavailable: {name}.");
                var receiver = new Material(shader);
                try
                {
                    for (var pass = 0; pass < receiver.passCount; pass++) receiver.SetPass(pass);
                    Require(!ShaderUtil.ShaderHasError(shader), $"Wind receiver shader failed: {name}.");
                }
                finally
                {
                    UnityEngine.Object.DestroyImmediate(receiver);
                }
            }
            Debug.Log("Shared vegetation wind GPU validation passed: cardinal/diagonal directions, gusts, calm, speed response, tree strength/root pinning, and both cloud advection scales.");
        }
        finally
        {
            RenderTexture.active = previous;
            UnityEngine.Object.DestroyImmediate(root);
            UnityEngine.Object.DestroyImmediate(probe);
            UnityEngine.Object.DestroyImmediate(target);
            UnityEngine.Object.DestroyImmediate(readback);
        }

        Color Read(float mode, Vector2 position)
        {
            probe.SetFloat("_ProbeMode", mode);
            probe.SetVector("_ProbePosition", new Vector4(position.x, position.y, 0, 0));
            Graphics.Blit(Texture2D.blackTexture, target, probe);
            RenderTexture.active = target;
            readback.ReadPixels(new Rect(0, 0, 1, 1), 0, 0);
            readback.Apply();
            return readback.GetPixel(0, 0);
        }
    }

    private static float Difference(Color a, Color b) =>
        Mathf.Max(Mathf.Abs(a.r - b.r), Mathf.Abs(a.g - b.g), Mathf.Abs(a.b - b.b), Mathf.Abs(a.a - b.a));

    private static void Require(bool condition, string message)
    {
        if (!condition) throw new InvalidOperationException(message);
    }
}
