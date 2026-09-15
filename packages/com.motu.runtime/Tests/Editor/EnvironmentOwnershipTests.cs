using System;
using System.Collections;
using Motu.Rendering;
using Motu.Settings;
using Motu.World;
using NUnit.Framework;
using UnityEditor.SceneManagement;
using UnityEngine;
using UnityEngine.Rendering;
using UnityEngine.SceneManagement;
using UnityEngine.TestTools;
using Object = UnityEngine.Object;

namespace Motu.Tests
{
    public sealed class EnvironmentOwnershipTests
    {
        [UnityTest, Timeout(180000)]
        public IEnumerator EnvironmentRestoresHostAcrossDisableReenableAndAdditiveUnload()
        {
            EditorSceneManager.NewScene(NewSceneSetup.EmptyScene);
            yield return new EnterPlayMode();
            var baseline = new EnvironmentHostState();
            var hostScene = SceneManager.GetActiveScene();
            var environmentScene = SceneManager.CreateScene("Motu ownership fixture");
            var root = new GameObject("Temporary environment");
            SceneManager.MoveGameObjectToScene(root, environmentScene);
            var lightObject = new GameObject("Host light");
            var cameraObject = new GameObject("Host camera");
            var otherCameraObject = new GameObject("Host camera with existing effects");
            var priorPlane = new GameObject("Host reflection plane");
            var priorOcean = new GameObject("Host ocean").AddComponent<OceanSurfaceController>();
            try
            {
                var sun = lightObject.AddComponent<Light>();
                sun.type = LightType.Directional; sun.intensity = .37f; sun.color = Color.cyan;
                sun.transform.SetPositionAndRotation(new Vector3(1, 2, 3), Quaternion.Euler(21, 32, 43));
                var rotation = sun.transform.rotation;
                RenderSettings.sun = sun;
                RenderSettings.fog = true; RenderSettings.fogMode = FogMode.Linear;
                RenderSettings.fogColor = Color.magenta; RenderSettings.fogDensity = .023f;
                RenderSettings.ambientMode = AmbientMode.Trilight; RenderSettings.ambientLight = Color.red;
                Shader.SetGlobalVector("_MotuWeatherWind", new Vector4(2, 3, 4, 5));
                Shader.SetGlobalFloat("_MotuCloudEnabled", .25f);
                Shader.SetGlobalColor("_MotuIslandHorizonColour", Color.yellow);
                Shader.SetGlobalTexture("_MotuWindNoise", Texture2D.whiteTexture);
                var camera = cameraObject.AddComponent<Camera>();
                camera.enabled = false; camera.backgroundColor = Color.green;
                camera.depthTextureMode = DepthTextureMode.None;
                var borrowedCamera = otherCameraObject.AddComponent<Camera>();
                borrowedCamera.enabled = false;
                var reflection = otherCameraObject.AddComponent<PlanarWaterReflection>();
                reflection.enabled = false; reflection.Configure(priorPlane.transform);
                var borrowedUnderwater = otherCameraObject.AddComponent<OceanUnderwaterView>();
                borrowedUnderwater.Configure(priorOcean); borrowedUnderwater.enabled = false;
                var environment = root.AddComponent<WorldEnvironmentController>();
                Assert.IsTrue(RenderSettings.fog, "Adding an uninitialized component changed host fog.");
                Assert.AreEqual(new Vector4(2, 3, 4, 5), Shader.GetGlobalVector("_MotuWeatherWind"));
                environment.Initialize(new WorldEnvironmentSettings(), new IslandCloudSettings(), 64, 128, null);
                environment.SetFirstPersonViewActive(true);
                environment.BindReflectionCamera(camera);
                environment.BindReflectionCamera(borrowedCamera);
                Assert.AreEqual(environment.OceanTransform, reflection.ReflectionPlane);
                Assert.AreNotEqual(priorOcean, borrowedUnderwater.Surface);
                var installedSky = environment.SkyMaterial;
                var installedSea = environment.SeaMaterial;
                var addedEffect = camera.GetComponent<OceanUnderwaterView>();
                Assert.IsNotNull(addedEffect);
                var contenderRoot = new GameObject("Second environment");
                try
                {
                    var contender = contenderRoot.AddComponent<WorldEnvironmentController>();
                    var wind = Shader.GetGlobalVector("_MotuWeatherWind");
                    Assert.Throws<InvalidOperationException>(() => contender.Initialize(
                        new WorldEnvironmentSettings(), new IslandCloudSettings(), 64, 128, null));
                    Assert.IsFalse(contender.IsInstalled);
                    Assert.AreEqual(wind, Shader.GetGlobalVector("_MotuWeatherWind"));
                }
                finally { Object.Destroy(contenderRoot); }

                environment.enabled = false;
                Assert.IsTrue(RenderSettings.fog);
                Assert.AreEqual(FogMode.Linear, RenderSettings.fogMode);
                Assert.AreEqual(Color.magenta, RenderSettings.fogColor);
                Assert.AreEqual(.023f, RenderSettings.fogDensity);
                Assert.AreEqual(AmbientMode.Trilight, RenderSettings.ambientMode);
                Assert.AreEqual(Color.red, RenderSettings.ambientLight);
                Assert.AreSame(sun, RenderSettings.sun);
                Assert.AreEqual(.37f, sun.intensity); Assert.AreEqual(Color.cyan, sun.color);
                Assert.AreEqual(new Vector3(1, 2, 3), sun.transform.position);
                Assert.Less(Quaternion.Angle(rotation, sun.transform.rotation), .001f);
                Assert.AreEqual(Color.green, camera.backgroundColor);
                Assert.AreEqual(DepthTextureMode.None, camera.depthTextureMode);
                Assert.AreSame(priorPlane.transform, reflection.ReflectionPlane);
                Assert.AreSame(priorOcean, borrowedUnderwater.Surface);
                Assert.IsFalse(borrowedUnderwater.enabled);
                Assert.AreEqual(new Vector4(2, 3, 4, 5), Shader.GetGlobalVector("_MotuWeatherWind"));
                Assert.AreEqual(.25f, Shader.GetGlobalFloat("_MotuCloudEnabled"));
                Assert.AreEqual(Color.yellow, Shader.GetGlobalColor("_MotuIslandHorizonColour"));
                Assert.AreSame(Texture2D.whiteTexture, Shader.GetGlobalTexture("_MotuWindNoise"));
                Assert.IsFalse(environment.OceanTransform.gameObject.activeSelf);

                // Exercise the deferred-Destroy race before the next frame.
                environment.enabled = true;
                environment.BindReflectionCamera(camera);
                yield return null;
                environment.BindReflectionCamera(camera);
                Assert.IsTrue(addedEffect == null);
                Assert.IsNotNull(camera.GetComponent<OceanUnderwaterView>());
                Assert.IsTrue(environment.OceanTransform.gameObject.activeSelf);
                environment.enabled = false;
                yield return null;
                Assert.IsNull(camera.GetComponent<OceanUnderwaterView>());
                RenderSettings.fogColor = Color.blue;
                environment.enabled = true;
                environment.BindReflectionCamera(camera);
                Assert.AreEqual(hostScene, SceneManager.GetActiveScene());
                yield return SceneManager.UnloadSceneAsync(environmentScene);
                yield return null;
                Assert.IsTrue(installedSky == null && installedSea == null, "Unloading leaked environment materials.");
                Assert.AreEqual(Color.blue, RenderSettings.fogColor, "Re-enable did not capture the new host state.");
                Assert.IsNull(camera.GetComponent<OceanUnderwaterView>());
                Assert.IsTrue(borrowedUnderwater != null);
                Assert.AreSame(priorOcean, borrowedUnderwater.Surface);
                Assert.AreSame(priorPlane.transform, reflection.ReflectionPlane);

                // Unexpected host switches must not copy the old scene's lighting
                // into the incoming scene. Normal use disables before switching.
                var incomingScene = SceneManager.CreateScene("Incoming host scene");
                SceneManager.SetActiveScene(incomingScene);
                RenderSettings.fogColor = Color.cyan;
                SceneManager.SetActiveScene(hostScene);
                root = new GameObject("Scene-switch environment");
                environment = root.AddComponent<WorldEnvironmentController>();
                environment.Initialize(new WorldEnvironmentSettings(), new IslandCloudSettings(), 64, 128, null);
                LogAssert.Expect(LogType.Warning, "Disable the Motu environment before changing the active scene. The environment has been disabled to preserve the incoming scene's lighting.");
                SceneManager.SetActiveScene(incomingScene);
                Assert.IsFalse(environment.enabled);
                Assert.AreEqual(Color.cyan, RenderSettings.fogColor);
                SceneManager.SetActiveScene(hostScene);
                yield return SceneManager.UnloadSceneAsync(incomingScene);
            }
            finally
            {
                Object.Destroy(root); Object.Destroy(lightObject); Object.Destroy(cameraObject);
                Object.Destroy(otherCameraObject); Object.Destroy(priorPlane); Object.Destroy(priorOcean.gameObject);
                baseline.Restore();
            }
            yield return null;
            yield return new ExitPlayMode();
        }
    }
}
