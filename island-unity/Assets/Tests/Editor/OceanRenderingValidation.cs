using System;
using System.Linq;
using System.Reflection;
using UnityEditor;
using UnityEditor.SceneManagement;
using UnityEngine;
using Motu.Gameplay;
using Motu.Islands;
using Motu.Rendering;
using Motu.Settings;
using Motu.World;

namespace Motu.Editor
{
    internal static class OceanRenderingValidation
    {
        private const string SandboxScenePath = "Assets/Scenes/IslandRuntimeSandbox.unity";
        internal static void ValidateOpenSeaEnvironmentAnchoring()
        {
            var anchor = WorldEnvironmentController.SnapAnchor(
                new Vector3(37f, 912f, -38f),
                4.5f,
                25f);
            if (anchor != new Vector3(25f, 4.5f, -50f))
            {
                throw new InvalidOperationException(
                    "The open-sea environment anchor is not snapped in XZ at sea level.");
            }
            var nearbyAnchor = WorldEnvironmentController.SnapAnchor(
                new Vector3(37.4f, -120f, -38.2f),
                4.5f,
                25f);
            if (nearbyAnchor != anchor)
            {
                throw new InvalidOperationException(
                    "Small player movements should not move the open-sea environment.");
            }

            ValidateOpenSeaEnvironmentOwnership();
            ValidateCoastalOverlayIsolation();
        }
        internal static void ValidateOceanWaveSystem()
        {
            var calmScale = WorldEnvironmentController.EvaluateWaveHeightScale(0f);
            var referenceScale = WorldEnvironmentController.EvaluateWaveHeightScale(
                WorldEnvironmentController.ReferenceWindSpeedMetresPerSecond);
            var strongScale = WorldEnvironmentController.EvaluateWaveHeightScale(36f);
            if (!Mathf.Approximately(calmScale, 0f)
                || !Mathf.Approximately(referenceScale, 1f)
                || !Mathf.Approximately(strongScale, 2f))
            {
                throw new InvalidOperationException(
                    "Global wind speed does not produce the expected ocean-wave height scale.");
            }
            var settings = OceanWaveRuntimeSettings.Default;
            var coordinates = OceanClipmapMeshBuilder.BuildAxisCoordinates(
                10000f,
                settings);
            var mesh = OceanClipmapMeshBuilder.Build(
                20000f,
                settings,
                markNoLongerReadable: false);
            try
            {
                var expectedVertices = coordinates.Count * coordinates.Count;
                var expectedTriangles = (coordinates.Count - 1)
                    * (coordinates.Count - 1)
                    * 2;
                var snapSteps = settings.MaskAnchorSnapMetres
                    / settings.FineVertexSpacingMetres;
                if (!settings.Enabled
                    || Mathf.Abs(settings.FineVertexSpacingMetres - 1f) > 1.0e-5f
                    || settings.DisplacementFadeStartMetres
                        >= settings.DisplacementFadeEndMetres
                    || settings.DisplacementFadeEndMetres
                        > settings.FineRadiusMetres
                    || Mathf.Abs(snapSteps - Mathf.Round(snapSteps)) > 1.0e-5f
                    || !float.IsFinite(settings.MaximumVerticalDisplacement)
                    || settings.MaximumVerticalDisplacement <= 0f
                    || expectedVertices > OceanClipmapMeshBuilder.MaximumVertexCount
                    || expectedTriangles > OceanClipmapMeshBuilder.MaximumTriangleCount
                    || mesh.vertexCount != expectedVertices
                    || mesh.GetIndexCount(0) != (uint)(expectedTriangles * 3)
                    || mesh.bounds.extents.x < 10000f
                    || mesh.bounds.extents.z < 10000f
                    || mesh.bounds.extents.y
                        < settings.MaximumVerticalDisplacement)
                {
                    throw new InvalidOperationException(
                        "The ocean clipmap does not satisfy its deterministic topology or bounds contract.");
                }
                if (Mathf.Abs(coordinates[0] + 10000f) > 1.0e-5f
                    || Mathf.Abs(coordinates[coordinates.Count - 1] - 10000f)
                        > 1.0e-5f)
                {
                    throw new InvalidOperationException(
                        "The ocean clipmap does not reach its configured outer extent exactly.");
                }
                for (var index = 1; index < coordinates.Count; index++)
                {
                    if (!float.IsFinite(coordinates[index])
                        || coordinates[index] <= coordinates[index - 1])
                    {
                        throw new InvalidOperationException(
                            "The ocean clipmap contains a non-finite or non-increasing axis coordinate.");
                    }
                }
                var vertices = mesh.vertices;
                var normals = mesh.normals;
                var uv = mesh.uv;
                var triangles = mesh.triangles;
                if (vertices.Length != expectedVertices
                    || normals.Length != expectedVertices
                    || uv.Length != expectedVertices
                    || triangles.Length != expectedTriangles * 3)
                {
                    throw new InvalidOperationException(
                        "The ocean clipmap vertex attributes are incomplete.");
                }
                for (var index = 0; index < vertices.Length; index++)
                {
                    var vertex = vertices[index];
                    var normal = normals[index];
                    var textureCoordinate = uv[index];
                    if (!float.IsFinite(vertex.x)
                        || !float.IsFinite(vertex.y)
                        || !float.IsFinite(vertex.z)
                        || normal != Vector3.up
                        || !float.IsFinite(textureCoordinate.x)
                        || !float.IsFinite(textureCoordinate.y))
                    {
                        throw new InvalidOperationException(
                            "The ocean clipmap contains a non-finite vertex attribute or invalid normal.");
                    }
                }
                for (var index = 0; index < triangles.Length; index += 3)
                {
                    var first = vertices[triangles[index]];
                    var second = vertices[triangles[index + 1]];
                    var third = vertices[triangles[index + 2]];
                    if (Vector3.Cross(second - first, third - first).y <= 1.0e-6f)
                    {
                        throw new InvalidOperationException(
                            "The ocean clipmap contains a degenerate or downward-facing triangle.");
                    }
                }
            }
            finally
            {
                UnityEngine.Object.DestroyImmediate(mesh);
            }

            var seaShader = Shader.Find("Motu/Sea Water");
            var attenuationShader = Shader.Find("Hidden/Motu/Ocean Wave Attenuation");
            var onshoreShader = Shader.Find("Hidden/Motu/Ocean Onshore Direction");
            if (seaShader == null
                || attenuationShader == null
                || onshoreShader == null
                || !seaShader.isSupported
                || !attenuationShader.isSupported
                || !onshoreShader.isSupported
                || ShaderUtil.ShaderHasError(seaShader)
                || ShaderUtil.ShaderHasError(attenuationShader)
                || ShaderUtil.ShaderHasError(onshoreShader))
            {
                throw new InvalidOperationException(
                    "The geometric ocean-wave shaders are missing, unsupported, or contain errors.");
            }
            var seaMaterial = new Material(seaShader);
            var attenuationMaterial = new Material(attenuationShader);
            var onshoreMaterial = new Material(onshoreShader);
            try
            {
                if (!seaMaterial.HasProperty("_GeometricWaves")
                    || !seaMaterial.HasProperty("_WaveAttenuationTex")
                    || !seaMaterial.HasProperty("_WaveOnshoreTex")
                    || !seaMaterial.HasProperty("_WaveAttenuationWorldRect")
                    || !seaMaterial.HasProperty("_WaveFadeStart")
                    || !seaMaterial.HasProperty("_WaveFadeEnd")
                    || !seaMaterial.HasProperty("_OceanWave0")
                    || !seaMaterial.HasProperty("_OceanWaveSpeeds")
                    || !seaMaterial.HasProperty("_WaveNoiseWorldSize")
                    || !seaMaterial.HasProperty("_WaveDomainWarp")
                    || !seaMaterial.HasProperty("_WaveAmplitudeVariation")
                    || !seaMaterial.HasProperty("_WhitecapColour")
                    || !seaMaterial.HasProperty("_WhitecapStrength")
                    || !seaMaterial.HasProperty("_WhitecapHeightThreshold")
                    || !seaMaterial.HasProperty("_WhitecapSlopeThreshold")
                    || !seaMaterial.HasProperty("_WhitecapCoverage")
                    || !seaMaterial.HasProperty("_WhitecapNoiseWorldSize")
                    || !seaMaterial.HasProperty("_WhitecapDistortionScale")
                    || !seaMaterial.HasProperty("_WhitecapDistortionSpeed")
                    || !seaMaterial.HasProperty("_OnshoreWaveEnabled")
                    || !seaMaterial.HasProperty("_OnshoreWaveParameters")
                    || !seaMaterial.HasProperty("_OnshoreWaveBreaking")
                    || seaMaterial.HasProperty("_SeaMask")
                    || !attenuationMaterial.HasProperty("_SeaMask")
                    || !attenuationMaterial.HasProperty("_IslandWorldSize")
                    || !attenuationMaterial.HasProperty("_CompositionWorldRect")
                    || !attenuationMaterial.HasProperty("_DepthAllowancePower")
                    || !attenuationMaterial.HasProperty("_DistanceAllowancePower")
                    || !onshoreMaterial.HasProperty("_MainTex"))
                {
                    throw new InvalidOperationException(
                        "The global ocean or attenuation composer violates its shader-property contract.");
                }
            }
            finally
            {
                UnityEngine.Object.DestroyImmediate(seaMaterial);
                UnityEngine.Object.DestroyImmediate(attenuationMaterial);
                UnityEngine.Object.DestroyImmediate(onshoreMaterial);
            }
        }
        internal static void ValidateCoastalOverlayIsolation()
        {
            var deepOceanShader = Shader.Find("Motu/Sea Water");
            var coastalShader = Shader.Find("Motu/Coastal Water Overlay");
            if (deepOceanShader == null || coastalShader == null)
            {
                throw new InvalidOperationException(
                    "The deep-ocean or coastal-overlay shader is unavailable.");
            }
            var deepOcean = new Material(deepOceanShader);
            var firstCoast = new Material(coastalShader);
            var secondCoast = new Material(coastalShader);
            var firstMask = new Texture2D(1, 1, TextureFormat.RGBA32, false, true);
            var secondMask = new Texture2D(1, 1, TextureFormat.RGBA32, false, true);
            try
            {
                var firstMatrix = Matrix4x4.Translate(new Vector3(-2000f, 0f, 750f));
                var secondMatrix = Matrix4x4.Translate(new Vector3(3200f, 0f, -900f));
                firstCoast.SetMatrix("_IslandWorldToLocal", firstMatrix);
                secondCoast.SetMatrix("_IslandWorldToLocal", secondMatrix);
                firstCoast.SetTexture("_SeaMask", firstMask);
                secondCoast.SetTexture("_SeaMask", secondMask);
                if (deepOcean.HasProperty("_SeaMask")
                    || deepOcean.HasProperty("_WorldSize")
                    || firstCoast.GetMatrix("_IslandWorldToLocal")
                        == secondCoast.GetMatrix("_IslandWorldToLocal")
                    || firstCoast.GetTexture("_SeaMask")
                        == secondCoast.GetTexture("_SeaMask")
                    || firstCoast.renderQueue <= deepOcean.renderQueue)
                {
                    throw new InvalidOperationException(
                        "Mock islands did not retain isolated coastal masks, transforms, and ordering.");
                }
            }
            finally
            {
                UnityEngine.Object.DestroyImmediate(firstMask);
                UnityEngine.Object.DestroyImmediate(secondMask);
                UnityEngine.Object.DestroyImmediate(firstCoast);
                UnityEngine.Object.DestroyImmediate(secondCoast);
                UnityEngine.Object.DestroyImmediate(deepOcean);
            }
        }
        internal static void ValidateOpenSeaEnvironmentOwnership()
        {
            var root = new GameObject("Open Sea Environment Validation");
            var target = new GameObject("Open Sea Follow Target");
            try
            {
                target.transform.position = new Vector3(37f, 120f, -38f);
                var controller = root.AddComponent<WorldEnvironmentController>();
                controller.SetFollowTarget(target.transform);
                var firstSkyMaterial = new Material(Shader.Find("Motu/Sky Dome"));
                var firstSeaMaterial = new Material(Shader.Find("Motu/Sea Water"));
                var firstWeather = new Texture2D(1, 1, TextureFormat.RGBA32, false, true);
                var firstOceanNoise = new Texture2D(1, 1, TextureFormat.R8, false, true);
                controller.Install(
                    firstSkyMaterial,
                    firstSeaMaterial,
                    firstWeather,
                    firstOceanNoise,
                    2000f,
                    4200f,
                    0f,
                    true,
                    null);
                var retainedSkyMesh = controller.SkyMesh;
                if (retainedSkyMesh == null
                    || controller.OceanTransform == null
                    || controller.MoonLight == null
                    || controller.AnchorPosition != new Vector3(25f, 0f, -50f))
                {
                    throw new InvalidOperationException(
                        "The global sky, ocean, moon, or player-relative anchor was not installed.");
                }

                var secondSkyMaterial = new Material(Shader.Find("Motu/Sky Dome"));
                var secondSeaMaterial = new Material(Shader.Find("Motu/Sea Water"));
                var secondWeather = new Texture2D(1, 1, TextureFormat.RGBA32, false, true);
                var secondOceanNoise = new Texture2D(1, 1, TextureFormat.R8, false, true);
                controller.Install(
                    secondSkyMaterial,
                    secondSeaMaterial,
                    secondWeather,
                    secondOceanNoise,
                    2000f,
                    4200f,
                    0f,
                    true,
                    null);
                if (controller.SkyMesh != retainedSkyMesh
                    || firstSkyMaterial != null
                    || firstSeaMaterial != null
                    || firstWeather != null
                    || firstOceanNoise != null)
                {
                    throw new InvalidOperationException(
                        "Environment replacement did not reuse global geometry or release old resources.");
                }
            }
            finally
            {
                UnityEngine.Object.DestroyImmediate(target);
                UnityEngine.Object.DestroyImmediate(root);
            }
        }
    }
}
