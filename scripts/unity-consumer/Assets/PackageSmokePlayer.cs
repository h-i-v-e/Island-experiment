using System;
using System.IO;
using System.Linq;
using System.Threading.Tasks;
using Motu.Islands;
using Motu.Rendering;
using Motu.Settings;
using Motu.Streaming;
using Motu.World;
using UnityEngine;
using UnityEngine.SceneManagement;

// Only included in the standalone consumer fixture, never the reusable runtime.
public sealed class PackageSmokePlayer : MonoBehaviour
{
    private async void Start()
    {
        try
        {
            var camera = new GameObject("Host camera").AddComponent<Camera>();
            camera.transform.position = new Vector3(0, 600, -1100);
            camera.transform.LookAt(Vector3.zero);
            camera.farClipPlane = 5000;
            camera.clearFlags = CameraClearFlags.SolidColor;
            camera.backgroundColor = Color.gray;
            var sun = new GameObject("Host sun").AddComponent<Light>();
            sun.type = LightType.Directional;
            sun.transform.rotation = Quaternion.Euler(40, -35, 0);
            RenderSettings.sun = sun;
            RenderSettings.ambientLight = Color.gray;
            var host = new GameObject("Island");
            var island = host.AddComponent<SingleIsland>();
            island.GenerateOnStart = false;
            island.GenerationSettings.Seed = 17;
            island.GenerationSettings.WaterRatio = .85f;
            island.GenerationSettings.MaximumHeightMetres = 80;
            island.GenerationSettings.UseSnapshotCache = false;
            island.NavigationSettings.Enabled = Environment.GetEnvironmentVariable("MOTU_WITH_NAVIGATION") == "1";
            JsonUtility.FromJsonOverwrite("{\"materialTextureResolution\":16}", island.RenderingSettings);
            JsonUtility.FromJsonOverwrite("{\"forestPrototypeCount\":1}", island.ForestSettings);
            if (!await island.GenerateAsync()) throw new Exception(island.Generator.Status);
            if (island.NavigationSettings.Enabled)
            {
                var navigation = island.Generator.Runtime.Navigation;
                if (navigation == null) throw new Exception("Navigation package did not install its extension.");
                await navigation.BuildCompletion;
                if (!navigation.IsReady || !navigation.IsRegistered) throw new Exception("Navigation did not become ready.");
                Debug.Log("MOTU CONSUMER NAVIGATION PASSED: full-resolution extension bake ready and registered.");
            }
            if (RenderSettings.sun != sun || Camera.allCamerasCount != 1)
                throw new Exception("The island changed host scene ownership.");
            await ValidateTravelStreaming(island);
            var target = new RenderTexture(320, 180, 24);
            camera.targetTexture = target;
            camera.Render();
            var previous = RenderTexture.active;
            RenderTexture.active = target;
            var capture = new Texture2D(320, 180, TextureFormat.RGBA32, false);
            capture.ReadPixels(new Rect(0, 0, 320, 180), 0, 0);
            capture.Apply();
            var output = Environment.GetEnvironmentVariable("MOTU_CAPTURE_PATH");
            if (!string.IsNullOrEmpty(output)) File.WriteAllBytes(output, capture.EncodeToPNG());
            var pixels = capture.GetPixels32();
            var colours = pixels.Distinct().Count();
            RenderTexture.active = previous;
            camera.targetTexture = null;
            Destroy(capture); Destroy(target);
            if (colours < 100) throw new Exception("The consumer camera did not render a detailed island: " + colours);
            var renderers = host.GetComponentsInChildren<Renderer>(true);
            foreach (var material in renderers.SelectMany(r => r.sharedMaterials).Where(m => m != null))
                if (!material.shader.isSupported || material.shader.name == "Hidden/InternalErrorShader")
                    throw new Exception("Unsupported player shader: " + material.name);
            island.Clear();
            await ValidateEnvironmentTeardown(camera, sun);
            Debug.Log("MOTU CONSUMER PLAYER PASSED: terrain rendered, " + colours + " colours; shaders, native generation, host ownership, unload.");
            Application.Quit(0);
        }
        catch (Exception error) { Debug.LogException(error); Application.Quit(1); }
    }

    private static async Task ValidateTravelStreaming(SingleIsland island)
    {
        var streamer = island.GetComponentInChildren<TerrainTileStreamer>();
        if (streamer == null) throw new Exception("Terrain streaming was not installed.");
        var firstFrame = Time.frameCount;
        island.Generator.PrepareStreamingAt(Vector3.zero);
        var deadline = Time.realtimeSinceStartupAsDouble + 120;
        while (true)
        {
            var groups = streamer.GetComponentsInChildren<Transform>();
            if (groups.Count(t => t.name.StartsWith("LOD 1 group ", StringComparison.Ordinal)) == 9
                && groups.Count(t => t.name.StartsWith("LOD 0 group ", StringComparison.Ordinal)) == 9)
                break;
            if (Time.realtimeSinceStartupAsDouble >= deadline)
                throw new Exception("Terrain neighbourhoods did not finish streaming.");
            await Task.Yield();
        }
        if (Time.frameCount <= firstFrame)
            throw new Exception("Initial terrain streaming blocked without advancing a frame.");
        Debug.Log("MOTU CONSUMER STREAMING PASSED: LOD 1 and LOD 0 installed while frames advanced.");
    }

    private static async Task ValidateEnvironmentTeardown(Camera camera, Light sun)
    {
        var hostScene = SceneManager.GetActiveScene();
        var scene = SceneManager.CreateScene("Optional Motu environment");
        var root = new GameObject("World environment");
        SceneManager.MoveGameObjectToScene(root, scene);
        RenderSettings.fog = true;
        RenderSettings.fogMode = FogMode.Linear;
        RenderSettings.fogColor = Color.magenta;
        var background = camera.backgroundColor;
        var depth = camera.depthTextureMode;
        var intensity = sun.intensity;
        var wind = Shader.GetGlobalVector("_MotuWeatherWind");
        var environment = root.AddComponent<WorldEnvironmentController>();
        environment.Initialize(new WorldEnvironmentSettings(), new IslandCloudSettings(), 64, 128, null);
        environment.BindReflectionCamera(camera);
        var sky = environment.SkyMaterial;
        var sea = environment.SeaMaterial;
        environment.enabled = false;
        if (!RenderSettings.fog || RenderSettings.fogMode != FogMode.Linear
            || RenderSettings.fogColor != Color.magenta || sun.intensity != intensity
            || RenderSettings.sun != sun || camera.backgroundColor != background || camera.depthTextureMode != depth
            || Shader.GetGlobalVector("_MotuWeatherWind") != wind)
            throw new Exception("Environment disable did not restore its host state.");
        environment.enabled = true;
        environment.BindReflectionCamera(camera);
        await Task.Yield();
        await Task.Yield();
        environment.BindReflectionCamera(camera);
        if (camera.GetComponent<OceanUnderwaterView>() == null)
            throw new Exception("Environment re-enable did not restore underwater rendering.");
        var unload = SceneManager.UnloadSceneAsync(scene);
        while (!unload.isDone) await Task.Yield();
        await Task.Yield();
        if (sky != null || sea != null || camera.GetComponent<OceanUnderwaterView>() != null
            || RenderSettings.fogColor != Color.magenta || camera.depthTextureMode != depth
            || SceneManager.GetActiveScene() != hostScene)
            throw new Exception("Additive environment unload left owned resources or changed the host scene.");
        Debug.Log("MOTU CONSUMER ENVIRONMENT PASSED: disable, same-frame re-enable, host restoration and additive unload.");
    }
}
