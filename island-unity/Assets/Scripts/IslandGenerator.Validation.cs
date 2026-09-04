#if UNITY_EDITOR
using System;
using System.Runtime.InteropServices;
using UnityEngine;
using UnityEngine.Rendering;

public sealed partial class IslandGenerator
{
    private static bool IsFinite(float value) =>
        IslandMeshInterop.IsFinite(value);

    private static Vector2[] CopyVector2Array(MotuNative.Vector2Array source) =>
        IslandMeshInterop.CopyVector2Array(source);

    private static Mesh CopyTerrainMesh(
        MotuNative.ExportMesh source,
        int lod,
        float worldSize) =>
        IslandMeshInterop.CopyTerrainMesh(source, lod, worldSize);

    public static void BatchValidateNativeInterop()
    {
        IslandRuntime.ValidateOwnershipContract();
        if (Marshal.SizeOf<MotuNative.Options>() != sizeof(float) * 19
            || Marshal.SizeOf<MotuNative.ForestOptions>() != 28
            || Marshal.SizeOf<MotuNative.ReedOptions>() != sizeof(float) * 8
            || Marshal.SizeOf<MotuNative.FernOptions>() != sizeof(float) * 8
            || Marshal.SizeOf<MotuNative.MaterialBakeOptions>() != 12
            || Marshal.SizeOf<MotuNative.ForestTrunkColliderExport>() != sizeof(float) * 9)
        {
            throw new InvalidOperationException(
                "Managed native option layouts do not match their ABI contracts.");
        }
        MotuNative.ExportCloudWeatherMap nativeCloudWeather = default;
        try
        {
            const int cloudResolution = 64;
            if (MotuNative.CreateCloudWeatherMap(
                    2018,
                    cloudResolution,
                    out nativeCloudWeather) == 0
                || nativeCloudWeather.handle == IntPtr.Zero
                || nativeCloudWeather.rgba == IntPtr.Zero
                || nativeCloudWeather.width != cloudResolution
                || nativeCloudWeather.height != cloudResolution)
            {
                throw new InvalidOperationException(
                    "Native cloud weather-map export is invalid.");
            }
            var cloudBytes = new byte[cloudResolution * cloudResolution * 4];
            Marshal.Copy(
                nativeCloudWeather.rgba,
                cloudBytes,
                0,
                cloudBytes.Length);
            var minimum = byte.MaxValue;
            var maximum = byte.MinValue;
            foreach (var value in cloudBytes)
            {
                minimum = Math.Min(minimum, value);
                maximum = Math.Max(maximum, value);
            }
            if (maximum - minimum < 32)
            {
                throw new InvalidOperationException(
                    "Native cloud weather-map channels contain insufficient variation.");
            }
            var validationTexture = CreateCloudWeatherTexture(
                cloudBytes,
                cloudResolution,
                cloudResolution,
                "Cloud Weather Upload Validation");
            UnityEngine.Object.DestroyImmediate(validationTexture);
        }
        finally
        {
            MotuNative.ReleaseCloudWeatherMap(ref nativeCloudWeather);
        }
        var validationSkyDome = IslandPreparationPipeline.PrepareSkyDome(
            ValidationWorldSize);
        if (validationSkyDome.vertices.Length == 0
            || validationSkyDome.vertices.Length != validationSkyDome.normals.Length
            || validationSkyDome.vertices.Length != validationSkyDome.uv.Length
            || validationSkyDome.triangles.Length == 0
            || validationSkyDome.material.Length != 0
            || validationSkyDome.environment.Length != 0)
        {
            throw new InvalidOperationException(
                "Native sky-dome attributes do not match the standalone mesh contract.");
        }
        for (var index = 0; index < validationSkyDome.vertices.Length; index++)
        {
            var vertex = validationSkyDome.vertices[index];
            var normal = validationSkyDome.normals[index];
            var belowSeaLevel = vertex.y < -0.001f;
            var horizontalRadius = new Vector2(vertex.x, vertex.z).magnitude;
            var inwardReference = belowSeaLevel
                ? new Vector3(-vertex.x, 0f, -vertex.z).normalized
                : -vertex.normalized;
            if (!IsFinite(vertex.x)
                || !IsFinite(vertex.y)
                || !IsFinite(vertex.z)
                || vertex.y < -ValidationWorldSize * SkyDomeSkirtDepthRatio - 0.1f
                || (belowSeaLevel
                    ? Mathf.Abs(horizontalRadius - ValidationWorldSize) > 0.1f
                    : Mathf.Abs(vertex.magnitude - ValidationWorldSize) > 0.1f)
                || Vector3.Dot(inwardReference, normal) < 0.999f)
            {
                throw new InvalidOperationException(
                    "Native sky dome is not an inward-facing island-radius "
                    + "hemisphere with a closed horizon skirt.");
            }
        }
        var uploadedSkyDome = IslandMeshInterop.CreateGeneratedMesh(
            validationSkyDome);
        try
        {
            var bounds = uploadedSkyDome.bounds;
            var expectedHeight = ValidationWorldSize
                * (1f + SkyDomeSkirtDepthRatio);
            if (Mathf.Abs(bounds.size.x - ValidationWorldSize * 2f) > 0.1f
                || Mathf.Abs(bounds.size.y - expectedHeight) > 0.1f
                || Mathf.Abs(bounds.size.z - ValidationWorldSize * 2f) > 0.1f)
            {
                throw new InvalidOperationException(
                    "Unity did not preserve the island-scale sky-dome bounds.");
            }
        }
        finally
        {
            DestroyImmediate(uploadedSkyDome);
        }
        var options = new MotuNative.Options
        {
            maxZ = 0.2f,
            waterRatio = 0.6f,
            slopeMultiplier = 1.3f,
            coastalSlopeMultiplier = 1f,
            continentalNoiseFrequency = 2.2f,
            detailNoiseFrequency = 12f,
            hydraulicErosionStrength = 1f,
            hydraulicDepositionStrength = 1.5f,
            hydraulicDepositionSlopeDegrees = 12f,
            riverSourceCatchmentHectares = 0.05f,
            riverSourceSteepMultiplier = 4f,
            riverSourceElevationBoost = 9f,
            riverSourceWidthMetres = 2f,
            riverMaximumWidthMetres = 14f,
            riverSourceDepthMetres = 0.35f,
            riverMaximumDepthMetres = 2f,
            continentalNoiseStrength = 0.78f,
            detailNoiseStrength = 0.22f,
            landMassOffset = 0f,
        };
        var forestOptions = new MotuNative.ForestOptions
        {
            patchSizeMetres = 200f,
            noiseThreshold = 0.62f,
            noiseOctaves = 4,
            snowlineMetres = 100f,
            prototypeCount = 8,
            minimumScale = 1f,
            maximumScale = 2f,
        };
        var handle = MotuNative.CreateMotuWithForest(
            2018,
            ref options,
            ref forestOptions);
        if (handle == IntPtr.Zero)
        {
            throw new InvalidOperationException("Native validation could not generate an island.");
        }

        try
        {
            const int validationMapDimension = 32;
            const int validationSeaMaskDimension = 128;
            var validationMaps = IslandPreparationPipeline.PrepareSurfaceMaps(
                handle,
                validationMapDimension);
            var validationSeaMask = IslandPreparationPipeline.PrepareSeaMask(
                handle,
                validationSeaMaskDimension);
            var validationTrunkColliderTiles =
                IslandPreparationPipeline.PrepareForestTrunkColliders(
                    handle,
                    ValidationWorldSize);
            var validationFernTiles = IslandPreparationPipeline.PrepareFernMeshGrid(
                handle,
                ValidationWorldSize);
            var validationTrunkColliderCount = 0;
            foreach (var tile in validationTrunkColliderTiles)
            {
                validationTrunkColliderCount += tile.Length;
            }
            if (validationTrunkColliderCount == 0)
            {
                throw new InvalidOperationException(
                    "Native forest validation did not export any trunk colliders.");
            }
            if (validationFernTiles.Length != FernTileStreamer.TileCount
                || !Array.Exists(validationFernTiles, tile => tile != null))
            {
                throw new InvalidOperationException(
                    "Native fern validation did not export a populated owner grid.");
            }
            if (validationSeaMask.rgba.Length
                != validationSeaMaskDimension * validationSeaMaskDimension * 4)
            {
                throw new InvalidOperationException(
                    "Native sea mask byte count does not match its RGBA dimensions.");
            }
            var hasOffshoreLandDistance = false;
            var hasSubmergedRiver = false;
            for (var index = 0; index < validationSeaMask.rgba.Length; index += 4)
            {
                if (validationSeaMask.rgba[index + 1] != 0)
                {
                    hasOffshoreLandDistance = true;
                }
                if (validationSeaMask.rgba[index + 2] != 0)
                {
                    hasSubmergedRiver = true;
                }
                if (validationSeaMask.rgba[index + 3] != byte.MaxValue)
                {
                    throw new InvalidOperationException(
                        "Native sea mask reserved alpha channel is not opaque.");
                }
            }
            if (!hasOffshoreLandDistance)
            {
                throw new InvalidOperationException(
                    "Native sea mask contains no offshore land-distance coverage.");
            }
            if (!hasSubmergedRiver)
            {
                throw new InvalidOperationException(
                    "Native sea mask contains no submerged river-carve coverage.");
            }
            var hasTerrainNormal = false;
            for (var index = 0; index < validationMaps.normalRgb.Length; index += 3)
            {
                if (validationMaps.normalRgb[index] != 127
                    || validationMaps.normalRgb[index + 1] != 127
                    || validationMaps.normalRgb[index + 2] != 255)
                {
                    hasTerrainNormal = true;
                    break;
                }
            }
            if (!hasTerrainNormal)
            {
                throw new InvalidOperationException(
                    "Native LOD 0 surface maps contain only a flat normal.");
            }

            MotuNative.CreateMesh(
                handle,
                IntPtr.Zero,
                0,
                0,
                out var environmentMeshExport);
            try
            {
                if (environmentMeshExport.environment.length
                    != environmentMeshExport.vertices.length)
                {
                    throw new InvalidOperationException(
                        "Native LOD 0 terrain has invalid environment attributes.");
                }
                var nativeEnvironment = CopyVector2Array(
                    environmentMeshExport.environment);
                var nativeMaximumForestFloor = 0f;
                var nativeMaximumStones = 0f;
                var nativeForestFloorVertices = 0;
                var nativeStoneVertices = 0;
                foreach (var environment in nativeEnvironment)
                {
                    nativeMaximumForestFloor = Mathf.Max(
                        nativeMaximumForestFloor,
                        environment.x);
                    if (environment.x > 0.01f) nativeForestFloorVertices++;
                    nativeMaximumStones = Mathf.Max(
                        nativeMaximumStones,
                        environment.y);
                    if (environment.y > 0.01f) nativeStoneVertices++;
                }
                if (nativeMaximumForestFloor < 0.99f
                    || nativeForestFloorVertices == 0
                    || nativeMaximumStones < 0.99f
                    || nativeStoneVertices == 0)
                {
                    throw new InvalidOperationException(
                        "Native LOD 0 terrain is missing environment switches.");
                }
                var uploadedEnvironmentMesh = CopyTerrainMesh(
                    environmentMeshExport,
                    0,
                    ValidationWorldSize);
                try
                {
                    var uploadedEnvironment = uploadedEnvironmentMesh.uv2;
                    var uploadedMaximumForestFloor = 0f;
                    var uploadedMaximumStones = 0f;
                    foreach (var environment in uploadedEnvironment)
                    {
                        uploadedMaximumForestFloor = Mathf.Max(
                            uploadedMaximumForestFloor,
                            environment.x);
                        uploadedMaximumStones = Mathf.Max(
                            uploadedMaximumStones,
                            environment.y);
                    }
                    if (uploadedEnvironment.Length
                            != uploadedEnvironmentMesh.vertexCount
                        || uploadedMaximumForestFloor < 0.99f
                        || uploadedMaximumStones < 0.99f)
                    {
                        throw new InvalidOperationException(
                            "Unity did not retain the native environment values in UV1.");
                    }
                    Debug.Log(
                        $"Forest-floor validation: {nativeForestFloorVertices}/"
                            + $"{nativeEnvironment.Length} LOD 0 vertices are marked; "
                            + $"native/uploaded maximum {nativeMaximumForestFloor:F2}/"
                            + $"{uploadedMaximumForestFloor:F2}. Stones: "
                            + $"{nativeStoneVertices}/{nativeEnvironment.Length}; "
                            + $"native/uploaded maximum {nativeMaximumStones:F2}/"
                            + $"{uploadedMaximumStones:F2}.");
                }
                finally
                {
                    DestroyImmediate(uploadedEnvironmentMesh);
                }
            }
            finally
            {
                MotuNative.ReleaseMesh(ref environmentMeshExport);
            }

            var terrainShader = Shader.Find("Motu/Terrain Unified");
            if (terrainShader == null
                || !terrainShader.isSupported
                || UnityEditor.ShaderUtil.ShaderHasError(terrainShader))
            {
                throw new InvalidOperationException(
                    "The unified terrain shader is missing or unsupported.");
            }
            var terrainMaterial = new Material(terrainShader);
            try
            {
                var requiredTerrainProperties = new[]
                {
                    "_TerrainAlbedoArray", "_TerrainNormalArray", "_TerrainMaskArray",
                    "_TerrainLayerWorldSizesA", "_TerrainLayerWorldSizesB",
                    "_TerrainHeightInfluencesA", "_TerrainHeightInfluencesB",
                    "_TerrainNormalStrengthsA", "_TerrainNormalStrengthsB",
                    "_TerrainParallaxDepthsA", "_TerrainParallaxDepthsB",
                    "_TerrainParallaxNeutralHeightsA", "_TerrainParallaxNeutralHeightsB",
                    "_TerrainOcclusionStrengthsA", "_TerrainOcclusionStrengthsB",
                    "_TerrainHeightBlendDepth", "_TopTextureFadeOutSlope",
                    "_SteepStoneBlendWidth",
                    "_WorldNormal", "_WorldNormalWeight",
                    "_Occlusion", "_RockColor", "_ForestFloorColor",
                    "_StonesColor", "_GroundDirtColor", "_SandColor", "_CliffNoise3D",
                    "_GrassPatchNoise", "_ForestFloorEdgeNoiseStrength",
                    "_ForestFloorEdgeBlendWidth", "_StonesEdgeNoiseStrength",
                    "_StonesEdgeBlendWidth", "_BeachEdgeNoiseStrength",
                    "_BeachEdgeBlendWidth", "_RiverEdgeNoiseStrength",
                    "_RiverEdgeBlendWidth", "_RockBoundaryNoiseStrength",
                    "_SandRockSlopeThreshold", "_RockPatchNoiseDetailScale",
                    "_CliffNormalStrength", "_GrassNormalDetailScale",
                    "_SandNormalDetailScale", "_SnowNormalDetailScale",
                    "_DirtNormalStrength", "_GrassNormalStrength",
                    "_SandNormalStrength", "_SnowNormalStrength",
                    "_GrassColorA", "_GrassColorB", "_GrassColorNoiseWorldSize",
                    "_GrassPatchNoiseWorldSize", "_GrassWindDirection",
                    "_GrassWindStrength", "_GrassWindSpeed", "_GrassWindWorldSize",
                    "_GrassWindNormalStrength", "_SandPatchNoiseWorldSize",
                    "_GrassPlayerPosition", "_GroundDirtCoreRadius",
                    "_GroundDirtFadeWidth", "_SnowMacroNoiseMetres",
                    "_WetBankBlendExponent", "_WetDarkening", "_WetSmoothness",
                    "_WetSpecularStrength", "_CoastalWetnessNoiseStrength",
                    "_CoastalWetnessBlendWidth",
                };
                if (Array.Exists(
                    requiredTerrainProperties,
                    property => !terrainMaterial.HasProperty(property)))
                {
                    throw new InvalidOperationException(
                        "The unified terrain shader is missing its texture-array or shared coverage properties.");
                }
                var normalStrengthsA = terrainMaterial.GetVector("_TerrainNormalStrengthsA");
                var normalStrengthsB = terrainMaterial.GetVector("_TerrainNormalStrengthsB");
                var parallaxDepthsA = terrainMaterial.GetVector("_TerrainParallaxDepthsA");
                var parallaxDepthsB = terrainMaterial.GetVector("_TerrainParallaxDepthsB");
                var occlusionStrengthsA = terrainMaterial.GetVector("_TerrainOcclusionStrengthsA");
                var occlusionStrengthsB = terrainMaterial.GetVector("_TerrainOcclusionStrengthsB");
                if (normalStrengthsA.x <= 0f
                    || normalStrengthsB.x <= 0f
                    || parallaxDepthsA.x <= 0f
                    || parallaxDepthsB.x <= 0f
                    || occlusionStrengthsA.x <= 0f
                    || occlusionStrengthsB.x <= 0f)
                {
                    throw new InvalidOperationException(
                        "Dirt and beach must retain authored normal, parallax, and occlusion contributions.");
                }
                var validationColours = new IslandMaterialColours(
                    new Color(0.09f, 0.055f, 0.026f, 1f),
                    new Color(0.3f, 0.32f, 0.29f, 1f),
                    new Color(0.62f, 0.57f, 0.34f, 1f));
                var nativeValidationColours = validationColours.ToNative();
                if (!Mathf.Approximately(
                        nativeValidationColours.dirtRed,
                        validationColours.dirt.linear.r)
                    || !Mathf.Approximately(
                        nativeValidationColours.stoneRed,
                        validationColours.stone.linear.r)
                    || !Mathf.Approximately(
                        nativeValidationColours.sandRed,
                        validationColours.sand.linear.r))
                {
                    throw new InvalidOperationException(
                        "Runtime material palette colours must be decoded to linear RGB exactly once before baking.");
                }
                var validationTextures = IslandPreparationPipeline.PrepareMaterialTextures(
                    validationColours,
                    64);
                var beachPixel = validationTextures.beach.albedoRgb;
                var expectedBeach = (Color32)validationColours.sand;
                if (Mathf.Abs(beachPixel[0] - expectedBeach.r) > 1
                    || Mathf.Abs(beachPixel[1] - expectedBeach.g) > 1
                    || Mathf.Abs(beachPixel[2] - expectedBeach.b) > 1)
                {
                    throw new InvalidOperationException(
                        "The baked beach albedo no longer round-trips to its Unity display colour.");
                }
                using (var textureArrays = new TerrainMaterialTextureArrays(validationTextures))
                {
                    textureArrays.BindTerrain(terrainMaterial);
                    var albedoArray = terrainMaterial.GetTexture("_TerrainAlbedoArray")
                        as Texture2DArray;
                    var normalArray = terrainMaterial.GetTexture("_TerrainNormalArray")
                        as Texture2DArray;
                    var maskArray = terrainMaterial.GetTexture("_TerrainMaskArray")
                        as Texture2DArray;
                    if (albedoArray == null
                        || normalArray == null
                        || maskArray == null
                        || albedoArray.depth != TerrainMaterialTextureArrays.LayerCount
                        || normalArray.depth != TerrainMaterialTextureArrays.LayerCount
                        || maskArray.depth != TerrainMaterialTextureArrays.LayerCount
                        || albedoArray.width != 64
                        || normalArray.width != 64
                        || maskArray.width != 64)
                    {
                        throw new InvalidOperationException(
                            "The runtime terrain texture arrays have an invalid layer order or extent.");
                    }
                    var expectedNeutralHeightsA = new Vector4(
                        validationTextures.dirt.NormalizedBaseHeight,
                        validationTextures.forestFloor.NormalizedBaseHeight,
                        validationTextures.rock.NormalizedBaseHeight,
                        validationTextures.riverBed.NormalizedBaseHeight);
                    var expectedNeutralHeightsB = new Vector4(
                        validationTextures.beach.NormalizedBaseHeight,
                        validationTextures.fallenStones.NormalizedBaseHeight,
                        0f,
                        0f);
                    if (Vector4.Distance(
                            terrainMaterial.GetVector("_TerrainParallaxNeutralHeightsA"),
                            expectedNeutralHeightsA) > 1e-5f
                        || Vector4.Distance(
                            terrainMaterial.GetVector("_TerrainParallaxNeutralHeightsB"),
                            expectedNeutralHeightsB) > 1e-5f)
                    {
                        throw new InvalidOperationException(
                            "The runtime terrain parallax neutral heights do not match the baked recipes.");
                    }
                    textureArrays.Unbind(terrainMaterial, null);
                }
                var cliffNoise = CreateCliffNoiseTexture();
                try
                {
                    terrainMaterial.SetTexture("_CliffNoise3D", cliffNoise);
                    if (cliffNoise.width != CliffNoiseDimension
                        || cliffNoise.height != CliffNoiseDimension
                        || cliffNoise.depth != CliffNoiseDimension)
                    {
                        throw new InvalidOperationException(
                            "The cliff noise texture has invalid dimensions.");
                    }
                }
                finally
                {
                    DestroyImmediate(cliffNoise);
                }
                TerrainTileStreamer.ValidateTerrainRenderBatching(terrainMaterial);
            }
            finally
            {
                DestroyImmediate(terrainMaterial);
            }

            var rockShader = Shader.Find("Motu/Rock Decoration");
            if (rockShader == null
                || !rockShader.isSupported
                || UnityEditor.ShaderUtil.ShaderHasError(rockShader))
            {
                throw new InvalidOperationException(
                    "The rock decoration shader is missing or unsupported.");
            }
            var validationRockMaterial = new Material(rockShader);
            try
            {
                if (!validationRockMaterial.HasProperty("_RockColor")
                    || !validationRockMaterial.HasProperty("_RockTint")
                    || !validationRockMaterial.HasProperty("_CliffNoise3D")
                    || !validationRockMaterial.HasProperty("_CliffNoisePeriod")
                    || !validationRockMaterial.HasProperty("_CliffNoiseDetailScale")
                    || !validationRockMaterial.HasProperty("_CliffNormalStrength"))
                {
                    throw new InvalidOperationException(
                        "The rock decoration shader is missing its shared geology properties.");
                }
            }
            finally
            {
                DestroyImmediate(validationRockMaterial);
            }

            ValidateTreeSurfaceShader("Motu/Tree Wood", "wood");
            ValidateTreeSurfaceShader("Motu/Tree Foliage", "foliage");
            ValidateDistantFoliageShader();

            var riverWaterShader = Shader.Find("Motu/River Water");
            if (riverWaterShader == null
                || !riverWaterShader.isSupported
                || UnityEditor.ShaderUtil.ShaderHasError(riverWaterShader))
            {
                throw new InvalidOperationException(
                    "The animated river-water shader is missing or unsupported.");
            }
            var riverWaterMaterial = new Material(riverWaterShader);
            try
            {
                if (!riverWaterMaterial.HasProperty("_NoiseTex")
                    || !riverWaterMaterial.HasProperty("_CoarseNoiseWorldSize")
                    || !riverWaterMaterial.HasProperty("_FineNoiseWorldSize")
                    || !riverWaterMaterial.HasProperty("_CoarseFlowSpeed")
                    || !riverWaterMaterial.HasProperty("_FineFlowSpeed")
                    || !riverWaterMaterial.HasProperty("_WorldSize")
                    || !riverWaterMaterial.HasProperty("_ShallowOpacity")
                    || !riverWaterMaterial.HasProperty("_OpacityDepth")
                    || !riverWaterMaterial.HasProperty("_EstuaryStrength")
                    || !riverWaterMaterial.HasProperty("_EstuaryBlendHeight")
                    || !riverWaterMaterial.HasProperty("_SeaLevel")
                    || !riverWaterMaterial.HasProperty("_ReflectionColor")
                    || !riverWaterMaterial.HasProperty("_WaterSkyExposure")
                    || !riverWaterMaterial.HasProperty("_RefractionStrength")
                    || !riverWaterMaterial.HasProperty("_RefractionDepth")
                    || !riverWaterMaterial.HasProperty("_PlanarReflectionWeight")
                    || !riverWaterMaterial.HasProperty("_PlanarReflectionDistortion")
                    || !riverWaterMaterial.HasProperty("_ShoreWaveStrength")
                    || !riverWaterMaterial.HasProperty("_WhitewaterStrength")
                    || !riverWaterMaterial.HasProperty("_WhitewaterSlopeStart")
                    || !riverWaterMaterial.HasProperty("_WhitewaterSlopeFull")
                    || riverWaterMaterial.GetFloat("_RefractionStrength") <= 0f
                    || riverWaterMaterial.GetFloat("_RefractionDepth") <= 0f
                    || riverWaterMaterial.HasProperty("_SeaMask"))
                {
                    throw new InvalidOperationException(
                        "The river-water shader has invalid river or sea properties.");
                }
                var riverNoise = CreateRiverNoiseTexture();
                try
                {
                    riverWaterMaterial.SetTexture("_NoiseTex", riverNoise);
                    if (riverNoise.width != RiverNoiseDimension
                        || riverNoise.height != RiverNoiseDimension)
                    {
                        throw new InvalidOperationException(
                            "The river noise texture has invalid dimensions.");
                    }
                }
                finally
                {
                    DestroyImmediate(riverNoise);
                }
            }
            finally
            {
                DestroyImmediate(riverWaterMaterial);
            }

            var seaWaterShader = Shader.Find("Motu/Sea Water");
            if (seaWaterShader == null
                || !seaWaterShader.isSupported
                || UnityEditor.ShaderUtil.ShaderHasError(seaWaterShader))
            {
                throw new InvalidOperationException(
                    "The animated sea-water shader is missing or unsupported.");
            }
            var seaWaterMaterial = new Material(seaWaterShader);
            try
            {
                if (!seaWaterMaterial.HasProperty("_NoiseTex")
                    || !seaWaterMaterial.HasProperty("_ShallowOpacity")
                    || !seaWaterMaterial.HasProperty("_OpacityDepth")
                    || !seaWaterMaterial.HasProperty("_ReflectionColor")
                    || !seaWaterMaterial.HasProperty("_WaterSkyExposure")
                    || !seaWaterMaterial.HasProperty("_RefractionStrength")
                    || !seaWaterMaterial.HasProperty("_RefractionDepth")
                    || !seaWaterMaterial.HasProperty("_PlanarReflectionWeight")
                    || !seaWaterMaterial.HasProperty("_PlanarReflectionDistortion")
                    || !seaWaterMaterial.HasProperty("_GeometricWaves")
                    || !seaWaterMaterial.HasProperty("_WaveAttenuationTex")
                    || !seaWaterMaterial.HasProperty("_WaveAttenuationWorldRect")
                    || !seaWaterMaterial.HasProperty("_WaveFadeStart")
                    || !seaWaterMaterial.HasProperty("_WaveFadeEnd")
                    || !seaWaterMaterial.HasProperty("_OceanWave0")
                    || !seaWaterMaterial.HasProperty("_OceanWaveSpeeds")
                    || seaWaterMaterial.HasProperty("_SeaMask")
                    || seaWaterMaterial.HasProperty("_WorldSize")
                    || seaWaterMaterial.HasProperty("_ShoreWaveStrength")
                    || seaWaterMaterial.HasProperty("_CoarseFlowSpeed")
                    || seaWaterMaterial.HasProperty("_EstuaryStrength")
                    || seaWaterMaterial.HasProperty("_WhitewaterStrength"))
                {
                    throw new InvalidOperationException(
                        "The sea-water shader has invalid sea or river properties.");
                }
            }
            finally
            {
                DestroyImmediate(seaWaterMaterial);
            }

            var coastalWaterShader = Shader.Find("Motu/Coastal Water Overlay");
            if (coastalWaterShader == null
                || !coastalWaterShader.isSupported
                || UnityEditor.ShaderUtil.ShaderHasError(coastalWaterShader))
            {
                throw new InvalidOperationException(
                    "The island coastal-water overlay shader is missing or unsupported.");
            }
            var coastalWaterMaterial = new Material(coastalWaterShader);
            try
            {
                if (!coastalWaterMaterial.HasProperty("_NoiseTex")
                    || !coastalWaterMaterial.HasProperty("_SeaMask")
                    || !coastalWaterMaterial.HasProperty("_WorldSize")
                    || !coastalWaterMaterial.HasProperty("_CoastalOpacity")
                    || !coastalWaterMaterial.HasProperty("_FoamOpacity")
                    || !coastalWaterMaterial.HasProperty("_EdgeFadeMetres")
                    || !coastalWaterMaterial.HasProperty("_ShoreWaveStrength")
                    || !coastalWaterMaterial.HasProperty("_ShoreWaveSpacing")
                    || !coastalWaterMaterial.HasProperty("_ShoreWaveSpeed")
                    || !coastalWaterMaterial.HasProperty("_ShoreWaveDepth")
                    || !coastalWaterMaterial.HasProperty("_ShoreWaveIncomingStrength")
                    || !coastalWaterMaterial.HasProperty("_ShoreWaveEchoStrength")
                    || !coastalWaterMaterial.HasProperty("_ShoreWaveNoiseWorldSize")
                    || coastalWaterMaterial.HasProperty("_ShallowOpacity")
                    || coastalWaterMaterial.HasProperty("_OpacityDepth")
                    || coastalWaterMaterial.HasProperty("_RefractionStrength")
                    || coastalWaterMaterial.HasProperty("_PlanarReflectionWeight"))
                {
                    throw new InvalidOperationException(
                        "The coastal-water overlay does not isolate island shore effects.");
                }
            }
            finally
            {
                DestroyImmediate(coastalWaterMaterial);
            }

            var waterCameraObject = new GameObject("Water depth validation camera");
            try
            {
                var waterCamera = waterCameraObject.AddComponent<Camera>();
                waterCamera.enabled = false;
                waterCamera.depthTextureMode = DepthTextureMode.DepthNormals;
                EnsureCameraDepthTexture(waterCamera);
                if ((waterCamera.depthTextureMode & DepthTextureMode.Depth) == 0
                    || (waterCamera.depthTextureMode & DepthTextureMode.DepthNormals) == 0)
                {
                    throw new InvalidOperationException(
                        "Island cameras do not retain the depth texture required by sea waves.");
                }
            }
            finally
            {
                DestroyImmediate(waterCameraObject);
            }

            var grassShader = Shader.Find("Motu/Terrain Grass");
            if (grassShader == null
                || !grassShader.isSupported
                || UnityEditor.ShaderUtil.ShaderHasError(grassShader))
            {
                throw new InvalidOperationException(
                    "The terrain grass shader is missing or unsupported.");
            }
            var grassMaterial = new Material(grassShader);
            try
            {
                var requiredGrassProperties = new[]
                {
                    "_TerrainMaskArray", "_TerrainLayerWorldSizesA",
                    "_TerrainLayerWorldSizesB", "_TerrainHeightInfluencesA",
                    "_TerrainHeightInfluencesB", "_TerrainHeightBlendDepth",
                    "_TopTextureFadeOutSlope", "_SteepStoneBlendWidth",
                    "_CliffNoise3D", "_ForestFloorEdgeNoiseStrength",
                    "_ForestFloorEdgeBlendWidth", "_StonesEdgeNoiseStrength",
                    "_StonesEdgeBlendWidth", "_BeachEdgeNoiseStrength",
                    "_BeachEdgeBlendWidth", "_RiverEdgeNoiseStrength",
                    "_RiverEdgeBlendWidth", "_RockPatchNoiseDetailScale",
                    "_GrassPatchNoise", "_GrassPatchNoiseWorldSize",
                    "_GrassColorA", "_GrassColorB", "_GrassColorNoiseWorldSize",
                    "_GrassPlayerPosition", "_GrassRadius", "_GrassHeight",
                    "_GrassBrightness", "_GrassWindDirection", "_GrassWindStrength",
                    "_GrassWindSpeed", "_GrassWindWorldSize",
                    "_GrassWindNormalStrength", "_GrassLightDirection",
                    "_GrassLightColor", "_GrassAmbientColor",
                    "_SandPatchNoiseWorldSize", "_RockBoundaryNoiseStrength",
                    "_SnowMacroNoiseMetres",
                };
                if (Array.Exists(
                    requiredGrassProperties,
                    property => !grassMaterial.HasProperty(property)))
                {
                    throw new InvalidOperationException(
                        "The terrain grass shader is missing its required properties.");
                }
                var grassPatchNoise = CreateGrassPatchNoiseTexture();
                try
                {
                    grassMaterial.SetTexture("_GrassPatchNoise", grassPatchNoise);
                    if (grassPatchNoise.width != GrassPatchNoiseDimension
                        || grassPatchNoise.height != GrassPatchNoiseDimension)
                    {
                        throw new InvalidOperationException(
                            "The grass patch noise texture has invalid dimensions.");
                    }
                }
                finally
                {
                    DestroyImmediate(grassPatchNoise);
                }
            }
            finally
            {
                DestroyImmediate(grassMaterial);
            }

            const float lod0ParentResolution = 64f;
            var area = new MotuNative.ExportArea(
                24f / lod0ParentResolution,
                24f / lod0ParentResolution,
                25f / lod0ParentResolution,
                25f / lod0ParentResolution);
            MotuNative.CreateMeshGrid(handle, ref area, 0, 8, 0, out var grid);
            try
            {
                if (grid.handle == IntPtr.Zero || grid.length != 64)
                {
                    throw new InvalidOperationException("Native render-grid layout is invalid.");
                }
                var exportSize = Marshal.SizeOf<MotuNative.ExportMesh>();
                for (var index = 0; index < grid.length; index++)
                {
                    var nativeMesh = Marshal.PtrToStructure<MotuNative.ExportMesh>(
                        IntPtr.Add(grid.data, index * exportSize));
                    if (nativeMesh.vertices.length == 0
                        || nativeMesh.triangles.length == 0
                        || nativeMesh.uv.length != nativeMesh.vertices.length
                        || nativeMesh.material.length != nativeMesh.vertices.length
                        || nativeMesh.environment.length != nativeMesh.vertices.length)
                    {
                        throw new InvalidOperationException(
                            "A render tile has invalid geometry or vertex attributes.");
                    }
                    var renderMesh = CopyTerrainMesh(nativeMesh, 0, ValidationWorldSize);
                    if (renderMesh.uv2.Length != renderMesh.vertexCount)
                    {
                        DestroyImmediate(renderMesh);
                        throw new InvalidOperationException(
                            "A render tile did not upload forest-floor UV1 data.");
                    }
                    DestroyImmediate(renderMesh);
                }
            }
            finally
            {
                MotuNative.ReleaseMeshGrid(ref grid);
            }

            var riverArea = new MotuNative.ExportArea(0f, 0f, 1f, 1f);
            const int riverResolution = TerrainTileStreamer.Lod1Resolution;
            MotuNative.CreateRiverMeshGrid(
                handle,
                ref riverArea,
                riverResolution,
                out var riverGrid);
            try
            {
                if (riverGrid.handle == IntPtr.Zero
                    || riverGrid.data == IntPtr.Zero
                    || riverGrid.length != riverResolution * riverResolution)
                {
                    throw new InvalidOperationException("Native river-grid layout is invalid.");
                }
                var exportSize = Marshal.SizeOf<MotuNative.ExportMesh>();
                var foundRiverGeometry = false;
                var minimumRiverV = float.PositiveInfinity;
                var maximumRiverV = float.NegativeInfinity;
                for (var index = 0; index < riverGrid.length; index++)
                {
                    var nativeMesh = Marshal.PtrToStructure<MotuNative.ExportMesh>(
                        IntPtr.Add(riverGrid.data, index * exportSize));
                    if (nativeMesh.triangles.length == 0)
                    {
                        continue;
                    }
                    foundRiverGeometry = true;
                    if (nativeMesh.uv.length != nativeMesh.vertices.length)
                    {
                        throw new InvalidOperationException(
                            "A sliced river tile has invalid UV coordinates.");
                    }
                    foreach (var riverUv in CopyVector2Array(nativeMesh.uv))
                    {
                        if (!IsFinite(riverUv.x) || !IsFinite(riverUv.y))
                        {
                            throw new InvalidOperationException(
                                "A sliced river tile has invalid flow coordinates.");
                        }
                        minimumRiverV = Mathf.Min(minimumRiverV, riverUv.y);
                        maximumRiverV = Mathf.Max(maximumRiverV, riverUv.y);
                    }
                }
                if (!foundRiverGeometry || maximumRiverV - minimumRiverV < 0.01f)
                {
                    throw new InvalidOperationException(
                        "Native river grid is empty or lacks downstream UV progression.");
                }
            }
            finally
            {
                MotuNative.ReleaseMeshGrid(ref riverGrid);
            }

            MotuNative.CreateRiverRockMeshGrid(
                handle,
                ref riverArea,
                riverResolution,
                out var riverRockGrid);
            try
            {
                if (riverRockGrid.handle == IntPtr.Zero
                    || riverRockGrid.data == IntPtr.Zero
                    || riverRockGrid.length != riverResolution * riverResolution)
                {
                    throw new InvalidOperationException(
                        "Native river-rock grid layout is invalid.");
                }
                var exportSize = Marshal.SizeOf<MotuNative.ExportMesh>();
                var foundRockGeometry = false;
                for (var index = 0; index < riverRockGrid.length; index++)
                {
                    var nativeMesh = Marshal.PtrToStructure<MotuNative.ExportMesh>(
                        IntPtr.Add(riverRockGrid.data, index * exportSize));
                    if (nativeMesh.triangles.length == 0)
                    {
                        continue;
                    }
                    foundRockGeometry = true;
                    if (nativeMesh.normals.length != nativeMesh.vertices.length)
                    {
                        throw new InvalidOperationException(
                            "A sliced river-rock tile has invalid normals.");
                    }
                }
                if (!foundRockGeometry)
                {
                    throw new InvalidOperationException(
                        "Native combined rock grid contains no geometry.");
                }
            }
            finally
            {
                MotuNative.ReleaseMeshGrid(ref riverRockGrid);
            }

            MotuNative.CreateWaterfallFeet(handle, out var waterfallFeet);
            try
            {
                if (waterfallFeet.handle == IntPtr.Zero
                    || waterfallFeet.length <= 0
                    || waterfallFeet.data == IntPtr.Zero)
                {
                    throw new InvalidOperationException(
                        "Native generation has no authoritative waterfall-foot records.");
                }
                var footSize = Marshal.SizeOf<MotuNative.WaterfallFootExport>();
                if (footSize != sizeof(float) * 8)
                {
                    throw new InvalidOperationException(
                        "Native waterfall-foot record layout is invalid.");
                }
                for (var index = 0; index < waterfallFeet.length; index++)
                {
                    var foot = Marshal.PtrToStructure<MotuNative.WaterfallFootExport>(
                        IntPtr.Add(waterfallFeet.data, index * footSize));
                    var directionLengthSquared = foot.direction.x * foot.direction.x
                        + foot.direction.y * foot.direction.y
                        + foot.direction.z * foot.direction.z;
                    if (!IsFinite(foot.position.x)
                        || !IsFinite(foot.position.y)
                        || !IsFinite(foot.position.z)
                        || foot.position.x < 0f
                        || foot.position.x > 1f
                        || foot.position.y < 0f
                        || foot.position.y > 1f
                        || !IsFinite(directionLengthSquared)
                        || Mathf.Abs(directionLengthSquared - 1f) > 0.001f
                        || !IsFinite(foot.halfWidth)
                        || foot.halfWidth <= 0f
                        || !IsFinite(foot.drop)
                        || foot.drop <= 0f)
                    {
                        throw new InvalidOperationException(
                            "A native waterfall-foot record is invalid.");
                    }
                }
            }
            finally
            {
                MotuNative.ReleaseWaterfallFeet(ref waterfallFeet);
            }
            if (waterfallFeet.handle != IntPtr.Zero
                || waterfallFeet.data != IntPtr.Zero
                || waterfallFeet.length != 0)
            {
                throw new InvalidOperationException(
                    "Native waterfall-foot release did not clear ownership.");
            }

            if (Marshal.SizeOf<MotuNative.ExportDecoration>()
                != Marshal.SizeOf<MotuNative.Vector3Array>() * 2)
            {
                throw new InvalidOperationException(
                    "Native decoration export layout is invalid.");
            }
            MotuNative.GetDecoration(handle, out var nativeDecoration);
            IslandPreparationPipeline.ValidateBorrowedArray(
                nativeDecoration.trees,
                "tree");
            IslandPreparationPipeline.ValidateBorrowedArray(
                nativeDecoration.bushes,
                "bush");

            var indexFeet = new[]
            {
                new IslandPreparedWaterfallFoot(
                    new Vector3(-999.9f, -2f, -999.9f),
                    Vector3.up,
                    1f,
                    2f),
                new IslandPreparedWaterfallFoot(Vector3.zero, Vector3.forward, 2f, 4f),
                new IslandPreparedWaterfallFoot(
                    new Vector3(999.9f, 2f, 999.9f),
                    Vector3.right,
                    3f,
                    8f),
            };
            var footIndex = new WaterfallFootIndex(indexFeet, ValidationWorldSize);
            var seen = new bool[indexFeet.Length];
            for (var y = 0; y < WaterfallFootIndex.Resolution; y++)
            {
                for (var x = 0; x < WaterfallFootIndex.Resolution; x++)
                {
                    footIndex.GetCellRange(x, y, out var start, out var end);
                    for (var order = start; order < end; order++)
                    {
                        var candidateIndex = footIndex.CandidateIndexAt(order);
                        if (candidateIndex < 0
                            || candidateIndex >= seen.Length
                            || seen[candidateIndex])
                        {
                            throw new InvalidOperationException(
                                "The waterfall-foot packed index contains an invalid entry.");
                        }
                        seen[candidateIndex] = true;
                    }
                }
            }
            if (Array.Exists(seen, value => !value)
                || footIndex.CellAt(indexFeet[0].position) != Vector2Int.zero
                || footIndex.CellAt(indexFeet[2].position)
                    != new Vector2Int(
                        WaterfallFootIndex.Resolution - 1,
                        WaterfallFootIndex.Resolution - 1))
            {
                throw new InvalidOperationException(
                    "The waterfall-foot packed index does not cover the world bounds.");
            }

            var mistRoot = new GameObject("Waterfall fog pool validation");
            try
            {
                mistRoot.transform.SetPositionAndRotation(
                    new Vector3(137f, 11f, -83f),
                    Quaternion.Euler(0f, 31f, 0f));
                var pool = mistRoot.AddComponent<WaterfallMistPool>();
                pool.Initialize(indexFeet, ValidationWorldSize, true);
                if (pool.PoolCount != 32
                    || pool.CreatedVolumeCount != 32
                    || pool.CreatedSpraySystemCount != 32)
                {
                    throw new InvalidOperationException(
                        "The waterfall-effects pool is not fixed at 32 mist/spray slots.");
                }
                pool.SetPlayerPosition(indexFeet[0].position, Vector2Int.zero);
                if (pool.ActiveVolumeCount != 1
                    || pool.ActiveSpraySystemCount != 1)
                {
                    throw new InvalidOperationException(
                        "A nearby waterfall foot did not activate its mist and spray.");
                }
                var activeSpray = Array.Find(
                    mistRoot.GetComponentsInChildren<ParticleSystem>(true),
                    spray => spray.isPlaying);
                var sprayVelocity = activeSpray?.velocityOverLifetime;
                if (activeSpray == null
                    || activeSpray.main.simulationSpace
                        != ParticleSystemSimulationSpace.World
                    || activeSpray.main.startSpeed.constantMin <= 0f
                    || activeSpray.main.startSpeed.constantMax > 2.91f
                    || activeSpray.main.startSize.constantMin < 0.1f
                    || activeSpray.main.gravityModifier.constantMin < 1f
                    || activeSpray.main.maxParticles < 1000
                    || activeSpray.emission.rateOverTime.constant < 50f
                    || activeSpray.transform.forward.y <= 0f
                    || !sprayVelocity.HasValue
                    || sprayVelocity.Value.x.mode != sprayVelocity.Value.y.mode
                    || sprayVelocity.Value.x.mode != sprayVelocity.Value.z.mode)
                {
                    throw new InvalidOperationException(
                        "Waterfall spray is not configured as a dense outward ballistic arc.");
                }
                var activeMistRenderer = Array.Find(
                    mistRoot.GetComponentsInChildren<MeshRenderer>(true),
                    renderer => renderer.enabled);
                var expectedImpactPosition = mistRoot.transform.TransformPoint(
                    new Vector3(
                        indexFeet[0].position.x,
                        SeaHeight,
                        indexFeet[0].position.z));
                var activeEffect = activeMistRenderer != null
                    ? activeMistRenderer.transform.parent
                    : null;
                if (activeMistRenderer == null
                    || activeEffect == null
                    || (activeEffect.position - expectedImpactPosition).sqrMagnitude
                        > 1.0e-4f
                    || activeMistRenderer.transform.localPosition.z <= 0f
                    || activeMistRenderer.transform.position.y
                        <= expectedImpactPosition.y)
                {
                    throw new InvalidOperationException(
                        "Translated waterfall effects were not placed at the local sea-level foot or directed downstream.");
                }
                pool.SetPlayerPosition(
                    indexFeet[0].position,
                    new Vector2Int(
                        WaterfallFootIndex.Resolution - 1,
                        WaterfallFootIndex.Resolution - 1));
                if (pool.ActiveVolumeCount != 0
                    || pool.ActiveSpraySystemCount != 0)
                {
                    throw new InvalidOperationException(
                        "Waterfall effects remained active outside the LOD 0 neighborhood.");
                }
                pool.ClearPlayerFocus();
                if (pool.ActiveVolumeCount != 0
                    || pool.ActiveSpraySystemCount != 0)
                {
                    throw new InvalidOperationException(
                        "Clearing waterfall focus did not clear waterfall effects.");
                }
                pool.DisposePool();
            }
            finally
            {
                DestroyImmediate(mistRoot);
            }

            const int validationSamplesPerTile = TerrainTileStreamer.ColliderSamplesPerTile;
            var heightMapPointer = MotuNative.CreateTerrainColliderHeightMap(
                handle,
                validationSamplesPerTile);
            try
            {
                if (heightMapPointer == IntPtr.Zero)
                {
                    throw new InvalidOperationException(
                        "Native terrain-collider height-map export is null.");
                }
                var nativeHeightMap = Marshal.PtrToStructure<
                    MotuNative.ExportHeightMapWithSeaLevel>(heightMapPointer);
                var expectedDimension = TerrainTileStreamer.Lod1Resolution
                    * (validationSamplesPerTile - 1)
                    + 1;
                if (nativeHeightMap.width != expectedDimension
                    || nativeHeightMap.height != expectedDimension
                    || nativeHeightMap.data == IntPtr.Zero)
                {
                    throw new InvalidOperationException(
                        "Native terrain-collider height-map dimensions are invalid.");
                }
                var heights = new float[checked(expectedDimension * expectedDimension)];
                Marshal.Copy(nativeHeightMap.data, heights, 0, heights.Length);
                var preparedHeightMap = new IslandPreparedColliderHeightMap(
                    expectedDimension,
                    validationSamplesPerTile,
                    heights,
                    ValidationWorldSize);
                var leftTile = preparedHeightMap.CopyTileHeights(Vector2Int.zero);
                var rightTile = preparedHeightMap.CopyTileHeights(new Vector2Int(1, 0));
                for (var row = 0; row < validationSamplesPerTile; row++)
                {
                    if (leftTile[row, validationSamplesPerTile - 1] != rightTile[row, 0])
                    {
                        throw new InvalidOperationException(
                            "Adjacent terrain-collider height maps do not share an identical edge.");
                    }
                }

                var validationTerrainData = new TerrainData
                {
                    heightmapResolution = validationSamplesPerTile,
                    size = new Vector3(
                        ValidationWorldSize / TerrainTileStreamer.Lod1Resolution,
                        preparedHeightMap.verticalSize,
                        ValidationWorldSize / TerrainTileStreamer.Lod1Resolution),
                };
                var validationTerrainObject = new GameObject(
                    "Terrain collider validation");
                try
                {
                    validationTerrainData.SetHeights(0, 0, leftTile);
                    validationTerrainObject.transform.position = new Vector3(
                        -ValidationWorldSize * 0.5f,
                        preparedHeightMap.verticalOrigin,
                        -ValidationWorldSize * 0.5f);
                    var hiddenTerrain = validationTerrainObject.AddComponent<Terrain>();
                    hiddenTerrain.terrainData = validationTerrainData;
                    hiddenTerrain.drawHeightmap = false;
                    hiddenTerrain.enabled = false;
                    var terrainCollider = validationTerrainObject.AddComponent<TerrainCollider>();
                    terrainCollider.terrainData = validationTerrainData;
                    Physics.SyncTransforms();
                    var tileSize = ValidationWorldSize / TerrainTileStreamer.Lod1Resolution;
                    var ray = new Ray(
                        validationTerrainObject.transform.position
                            + new Vector3(tileSize * 0.5f, ValidationWorldSize, tileSize * 0.5f),
                        Vector3.down);
                    if (!terrainCollider.Raycast(ray, out _, ValidationWorldSize * 2f))
                    {
                        throw new InvalidOperationException(
                            "The hidden Unity TerrainCollider did not hit its prepared heightfield.");
                    }
                }
                finally
                {
                    DestroyImmediate(validationTerrainObject);
                    DestroyImmediate(validationTerrainData);
                }

                var streamingValidationObject = new GameObject(
                    "Terrain collider neighbourhood validation");
                try
                {
                    streamingValidationObject
                        .AddComponent<TerrainTileStreamer>()
                        .ValidateColliderStreaming(preparedHeightMap, ValidationWorldSize);
                }
                finally
                {
                    DestroyImmediate(streamingValidationObject);
                }
            }
            finally
            {
                MotuNative.ReleaseTerrainColliderHeightMap(heightMapPointer);
            }
        }
        finally
        {
            MotuNative.ReleaseMotu(handle);
        }
        Debug.Log(
            "Motu CPU generation, terrain collider, and material validation passed.");
    }


}
#endif
