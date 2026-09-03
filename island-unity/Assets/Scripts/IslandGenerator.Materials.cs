using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using UnityEngine;
using UnityEngine.Rendering;

public sealed partial class IslandGenerator
{
    private void BuildRuntimeMaterials(IslandPreparedMaterialTextures materialTextures)
    {
        var skyColor = new Color(0.49f, 0.68f, 0.82f);
        skyDomeMaterial = CreateMaterial(
            "Motu/Sky Dome",
            skyColor,
            null,
            Generation.WorldSizeMetres);
        if (skyDomeMaterial.shader.name != "Motu/Sky Dome")
        {
            throw new InvalidOperationException("Could not find shader 'Motu/Sky Dome'.");
        }
        skyDomeMaterial.SetColor("_ZenithColor", skyColor);
        skyDomeMaterial.SetColor("_HorizonColor", Rendering.DistanceHazeColour);
        skyDomeMaterial.SetFloat(
            SunDiscCosRadiusId,
            Mathf.Cos(SunDiscAngularRadiusDegrees * Mathf.Deg2Rad));
        skyDomeMaterial.SetColor(SunHaloColourId, SunsetSunHaloColour);
        skyDomeMaterial.SetColor(MoonColourId, MoonDiscColour);
        skyDomeMaterial.SetColor(MoonDarkColourId, MoonDarkColour);
        skyDomeMaterial.SetFloat(
            MoonDiscCosRadiusId,
            Mathf.Cos(MoonDiscAngularRadiusDegrees * Mathf.Deg2Rad));
        skyDomeMaterial.SetFloat(SkyExposureId, currentSkyExposure);
        ApplyStarSettings();
        EnsureCloudWeatherTexture();
        terrainMaterial = CreateMaterial(
            "Motu/Terrain Unified",
            Color.white,
            Rendering.TerrainMaterial,
            Generation.WorldSizeMetres);
        terrainMaterialTextures = new TerrainMaterialTextureArrays(materialTextures);
        terrainMaterialTextures.BindTerrain(terrainMaterial);
        cliffNoiseTexture = Rendering.CliffDetailNoise;
        ownsCliffNoiseTexture = cliffNoiseTexture == null;
        if (ownsCliffNoiseTexture) cliffNoiseTexture = CreateCliffNoiseTexture();
        terrainMaterial.SetTexture("_CliffNoise3D", cliffNoiseTexture);
        var dirtColor = materialTextures.colours.dirt;
        var rockColor = materialTextures.colours.stone;
        var sandColor = materialTextures.colours.sand;
        terrainMaterial.SetColor("_RockColor", rockColor);
        terrainMaterial.SetColor("_GroundDirtColor", dirtColor);
        terrainMaterial.SetColor("_SandColor", sandColor);
        terrainMaterial.SetColor("_ForestFloorColor", Color.white);
        terrainMaterial.SetColor("_StonesColor", Color.white);
        grassMaterial = CreateMaterial(
            "Motu/Terrain Grass",
            Color.white,
            Rendering.GrassMaterial,
            Generation.WorldSizeMetres);
        terrainMaterialTextures.BindGrass(grassMaterial);
        grassMaterial.SetTexture("_CliffNoise3D", cliffNoiseTexture);
        CopyTerrainBlendSettingsToGrass();
        ApplySnowlineSettings();
        treeWoodMaterial = CreateMaterial(
            "Motu/Tree Wood",
            new Color(0.24f, 0.105f, 0.045f, 1f),
            Rendering.TreeWoodMaterial,
            Generation.WorldSizeMetres);
        treeWoodMaterial.SetTexture("_CliffNoise3D", cliffNoiseTexture);
        treeWoodMaterial.enableInstancing = true;
        treeLod1WoodMaterial = new Material(treeWoodMaterial)
        {
            name = "Island tree LOD1 wood material (no parallax)",
        };
        treeLod1WoodMaterial.EnableKeyword("MOTU_TREE_BARK_NO_PARALLAX");
        treeLod1WoodMaterial.enableInstancing = true;
        treeLod0FoliageMaterial = CreateMaterial(
            "Motu/Tree Foliage",
            new Color(0.08f, 0.28f, 0.055f, 1f),
            Rendering.TreeFoliageMaterial,
            Generation.WorldSizeMetres);
        treeLod0FoliageMaterial.SetTexture("_CliffNoise3D", cliffNoiseTexture);
        treeLod0FoliageMaterial.SetFloat("_CullMode", (float)CullMode.Off);
        treeLod0FoliageMaterial.enableInstancing = true;
        var distantFoliageShader = Shader.Find("Motu/Tree Foliage Distant")
            ?? throw new InvalidOperationException(
                "Could not find shader 'Motu/Tree Foliage Distant'.");
        treeFoliageMaterial = new Material(distantFoliageShader)
        {
            name = "Island distant tree foliage material (base canopy only)",
        };
        treeFoliageMaterial.CopyPropertiesFromMaterial(treeLod0FoliageMaterial);
        treeFoliageMaterial.SetFloat("_CullMode", (float)CullMode.Back);
        treeFoliageMaterial.renderQueue = (int)RenderQueue.Geometry;
        treeFoliageMaterial.enableInstancing = true;
        reedMaterial = CreateMaterial(
            "Motu/Riverbank Reeds",
            Reeds.BaseColour,
            null,
            Generation.WorldSizeMetres);
        if (reedMaterial.shader.name != "Motu/Riverbank Reeds")
        {
            throw new InvalidOperationException(
                "Could not find shader 'Motu/Riverbank Reeds'.");
        }
        reedMaterial.SetColor("_BaseColor", Reeds.BaseColour);
        reedMaterial.SetColor("_TipColor", Reeds.TipColour);
        reedMaterial.SetFloat("_ReedWindMultiplier", Reeds.WindStrength);
        reedMaterial.enableInstancing = true;
        fernMaterial = CreateMaterial(
            "Motu/Forest Ferns",
            Ferns.BaseColour,
            null,
            Generation.WorldSizeMetres);
        if (fernMaterial.shader.name != "Motu/Forest Ferns")
        {
            throw new InvalidOperationException(
                "Could not find shader 'Motu/Forest Ferns'.");
        }
        fernMaterial.SetColor("_BaseColor", Ferns.BaseColour);
        fernMaterial.SetColor("_TipColor", Ferns.TipColour);
        fernMaterial.SetFloat("_FernWindMultiplier", Ferns.WindStrength);
        fernMaterial.enableInstancing = true;
        rockMaterial = CreateMaterial(
            "Motu/Rock Decoration",
            rockColor,
            Rendering.RockMaterial,
            Generation.WorldSizeMetres);
        rockMaterial.name = "Island rock decoration material";
        rockMaterial.enableInstancing = true;
        rockMaterial.SetColor("_RockColor", rockColor);
        rockMaterial.SetTexture("_CliffNoise3D", cliffNoiseTexture);
        rockMaterial.SetFloat(
            "_CliffNoisePeriod",
            terrainMaterial.GetFloat("_CliffNoisePeriod"));
        rockMaterial.SetFloat(
            "_CliffNoiseDetailScale",
            terrainMaterial.GetFloat("_CliffNoiseDetailScale"));
        rockMaterial.SetFloat(
            "_CliffNormalStrength",
            terrainMaterial.GetFloat("_CliffNormalStrength"));
        terrainMaterial.SetFloat(
            "_RockPatchNoiseDetailScale",
            RockPatchNoiseDetailScale);
        grassMaterial.SetFloat(
            "_RockPatchNoiseDetailScale",
            RockPatchNoiseDetailScale);
        terrainMaterial.SetFloat(
            "_SandPatchNoiseWorldSize",
            Rendering.SandPatchSizeMetres);
        grassMaterial.SetFloat(
            "_SandPatchNoiseWorldSize",
            Rendering.SandPatchSizeMetres);
        grassPatchNoiseTexture = Rendering.GrassPatchNoise;
        ownsGrassPatchNoiseTexture = grassPatchNoiseTexture == null;
        if (ownsGrassPatchNoiseTexture) grassPatchNoiseTexture = CreateGrassPatchNoiseTexture();
        terrainMaterial.SetTexture("_GrassPatchNoise", grassPatchNoiseTexture);
        terrainMaterial.SetFloat(
            "_GrassPatchNoiseWorldSize",
            Rendering.GrassPatchSizeMetres);
        grassMaterial.SetTexture("_GrassPatchNoise", grassPatchNoiseTexture);
        grassMaterial.SetFloat(
            "_GrassPatchNoiseWorldSize",
            Rendering.GrassPatchSizeMetres);
        treeWoodMaterial.SetTexture("_GrassPatchNoise", grassPatchNoiseTexture);
        treeLod1WoodMaterial.SetTexture("_GrassPatchNoise", grassPatchNoiseTexture);
        treeFoliageMaterial.SetTexture("_GrassPatchNoise", grassPatchNoiseTexture);
        treeLod0FoliageMaterial.SetTexture("_GrassPatchNoise", grassPatchNoiseTexture);
        reedMaterial.SetTexture("_GrassPatchNoise", grassPatchNoiseTexture);
        fernMaterial.SetTexture("_GrassPatchNoise", grassPatchNoiseTexture);
        ApplyGrassColourSettings();
        grassMaterial.SetFloat("_GrassBrightness", Rendering.GrassBrightness);
        ApplyGrassWindSettings();
        terrainLod1Material = new Material(terrainMaterial)
        {
            name = "Island terrain LOD1 material (no parallax)",
        };
        terrainLod1Material.EnableKeyword("MOTU_TERRAIN_LOD1");
        terrainLod2Material = new Material(terrainMaterial)
        {
            name = "Island terrain LOD2 material (procedural only)",
        };
        terrainLod2Material.EnableKeyword("MOTU_TERRAIN_LOD2");
        var sun = Rendering.Sunlight != null ? Rendering.Sunlight : RenderSettings.sun;
        grassMaterial.SetVector(
            "_GrassLightDirection",
            sun != null ? -sun.transform.forward : Vector3.down);
        grassMaterial.SetColor(
            "_GrassLightColor",
            sun != null ? sun.color * sun.intensity : Color.white);
        grassMaterial.SetColor("_GrassAmbientColor", RenderSettings.ambientLight);
        riverNoiseTexture = Rendering.RiverNoise;
        ownsRiverNoiseTexture = riverNoiseTexture == null;
        if (ownsRiverNoiseTexture) riverNoiseTexture = CreateRiverNoiseTexture();
        seaNoiseTexture = Rendering.RiverNoise;
        ownsSeaNoiseTexture = seaNoiseTexture == null;
        if (ownsSeaNoiseTexture) seaNoiseTexture = CreateRiverNoiseTexture();
        var waterColor = new Color(0.03f, 0.28f, 0.55f, 1f);
        const float shallowWaterOpacity = 0.25f;
        const float fullOpacityDepth = 5f;
        const float shoreWaveStrength = 0.35f;
        const float riverShoreWaveSpeed = -0.07f;
        const float seaShoreWaveSpeed = 0.35f;
        const float riverShoreWaveSpacing = 0.11f;
        const float riverShoreWaveDepth = 0.5f;
        const float riverShoreWaveNoiseWorldSize = 1f;
        const float seaShoreWaveSpacing = 0.55f;
        const float seaShoreWaveDepth = 2.5f;
        const float seaShoreWaveNoiseWorldSize = 5f;
        riverMaterial = CreateMaterial(
            "Motu/River Water",
            waterColor,
            Rendering.RiverMaterial,
            Generation.WorldSizeMetres);
        riverMaterial.renderQueue = (int)RenderQueue.Transparent + 10;
        riverMaterial.SetTexture("_NoiseTex", riverNoiseTexture);
        riverMaterial.SetFloat("_WhitewaterStrength", 0.9f);
        riverMaterial.SetFloat("_ShallowOpacity", shallowWaterOpacity);
        riverMaterial.SetFloat("_OpacityDepth", fullOpacityDepth);
        riverMaterial.SetFloat("_EstuaryStrength", 1f);
        riverMaterial.SetFloat("_EstuaryBlendHeight", Rendering.EstuaryBlendHeightMetres);
        riverMaterial.SetFloat("_SeaLevel", SeaHeight);
        riverMaterial.SetColor("_ReflectionColor", skyColor);
        riverMaterial.SetColor(
            "_ReflectionHorizonColor",
            Color.Lerp(skyColor, Color.white, 0.35f));
        riverMaterial.SetFloat("_ReflectionStrength", 0.45f);
        riverMaterial.SetFloat("_PlanarReflectionWeight", 1f);
        riverMaterial.SetFloat("_PlanarReflectionDistortion", 0.006f);
        riverMaterial.SetFloat("_SunGlintStrength", 0.55f);
        ConfigureShoreWaves(
            riverMaterial,
            riverNoiseTexture,
            shoreWaveStrength,
            riverShoreWaveSpacing,
            riverShoreWaveSpeed,
            riverShoreWaveDepth,
            riverShoreWaveNoiseWorldSize);
        var seaShader = Shader.Find("Motu/Sea Water")
            ?? throw new InvalidOperationException("Could not find shader 'Motu/Sea Water'.");
        seaMaterial = new Material(seaShader)
        {
            name = "Motu/Sea Water (Global Deep Ocean)",
        };
        if (Rendering.SeaMaterial != null)
        {
            seaMaterial.CopyPropertiesFromMaterial(Rendering.SeaMaterial);
        }
        else
        {
            seaMaterial.SetColor("_Color", waterColor);
        }
        seaMaterial.renderQueue = (int)RenderQueue.Transparent;
        seaMaterial.SetTexture("_NoiseTex", seaNoiseTexture);
        seaMaterial.SetFloat("_ShallowOpacity", shallowWaterOpacity);
        seaMaterial.SetFloat("_OpacityDepth", fullOpacityDepth);
        seaMaterial.SetColor("_ReflectionColor", skyColor);
        seaMaterial.SetColor(
            "_ReflectionHorizonColor",
            Color.Lerp(skyColor, Color.white, 0.35f));
        seaMaterial.SetFloat("_ReflectionStrength", 0.65f);
        seaMaterial.SetFloat("_PlanarReflectionWeight", 1f);
        seaMaterial.SetFloat("_PlanarReflectionDistortion", 0.008f);
        seaMaterial.SetFloat("_SunGlintStrength", 0.8f);

        var coastalShader = Shader.Find("Motu/Coastal Water Overlay")
            ?? throw new InvalidOperationException(
                "Could not find shader 'Motu/Coastal Water Overlay'.");
        coastalWaterMaterial = new Material(coastalShader)
        {
            name = "Motu/Coastal Water Overlay (Island Instance)",
        };
        coastalWaterMaterial.SetColor(
            "_Color",
            seaMaterial.HasProperty("_Color")
                ? seaMaterial.GetColor("_Color")
                : waterColor);
        coastalWaterMaterial.renderQueue = (int)RenderQueue.Transparent + 5;
        coastalWaterMaterial.SetColor("_FoamColor", new Color(0.92f, 0.97f, 1f, 1f));
        coastalWaterMaterial.SetFloat("_WorldSize", Generation.WorldSizeMetres);
        coastalWaterMaterial.SetFloat("_CoastalOpacity", 0.16f);
        coastalWaterMaterial.SetFloat("_FoamOpacity", 0.72f);
        coastalWaterMaterial.SetFloat("_EdgeFadeMetres", 24f);
        ConfigureShoreWaves(
            coastalWaterMaterial,
            riverNoiseTexture,
            shoreWaveStrength,
            seaShoreWaveSpacing,
            seaShoreWaveSpeed,
            seaShoreWaveDepth,
            seaShoreWaveNoiseWorldSize);
        meshEdgeMaterial = CreateMaterial(
            "Motu/Mesh Edge Overlay",
            Color.black,
            null,
            Generation.WorldSizeMetres);
        meshEdgeMaterial.renderQueue = (int)RenderQueue.Overlay + 100;
        meshEdgeMaterial.SetColor("_Color", Color.black);
        meshEdgeMaterial.SetFloat("_ZTest", (float)CompareFunction.LessEqual);
        if (controlsWorldEnvironment)
        {
            UpdateSolarLighting(0f);
            ApplyCloudSettings(0f);
        }
        UpdateMaterialTransforms(true);
    }

    private void CopyTerrainBlendSettingsToGrass()
    {
        CopyMaterialVector("_TerrainLayerWorldSizesA");
        CopyMaterialVector("_TerrainLayerWorldSizesB");
        CopyMaterialVector("_TerrainHeightInfluencesA");
        CopyMaterialVector("_TerrainHeightInfluencesB");
        CopyMaterialFloat("_TerrainHeightBlendDepth");
        CopyMaterialFloat("_TopTextureFadeOutSlope");
        CopyMaterialFloat("_SteepStoneBlendWidth");
        CopyMaterialFloat("_ForestFloorEdgeNoiseStrength");
        CopyMaterialFloat("_ForestFloorEdgeBlendWidth");
        CopyMaterialFloat("_StonesEdgeNoiseStrength");
        CopyMaterialFloat("_StonesEdgeBlendWidth");
        CopyMaterialFloat("_BeachEdgeNoiseStrength");
        CopyMaterialFloat("_BeachEdgeBlendWidth");
        CopyMaterialFloat("_RiverEdgeNoiseStrength");
        CopyMaterialFloat("_RiverEdgeBlendWidth");
        CopyMaterialFloat("_CliffNormalCutoff");
        CopyMaterialFloat("_CliffBoundaryNoiseStrength");
        CopyMaterialFloat("_RockBoundaryNoiseStrength");
        CopyMaterialFloat("_SandRockSlopeThreshold");
        CopyMaterialFloat("_CliffNoisePeriod");
        CopyMaterialFloat("_RockPatchNoiseDetailScale");
        CopyMaterialFloat("_SandPatchNoiseWorldSize");
        CopyMaterialFloat("_GrassPatchNoiseWorldSize");
        CopyMaterialFloat("_SnowEdgeNoiseMetres");
        CopyMaterialFloat("_SnowMacroNoiseMetres");
    }

    private void ApplySnowlineSettings()
    {
        var snowline = Forest.SnowlineMetres;
        if (terrainMaterial != null && terrainMaterial.HasProperty("_SnowLine"))
        {
            terrainMaterial.SetFloat("_SnowLine", snowline);
        }
        terrainLod1Material?.SetFloat("_SnowLine", snowline);
        terrainLod2Material?.SetFloat("_SnowLine", snowline);
        if (grassMaterial != null && grassMaterial.HasProperty("_SnowLine"))
        {
            grassMaterial.SetFloat("_SnowLine", snowline);
        }
    }

    private void CopyMaterialVector(string propertyName)
    {
        if (terrainMaterial.HasProperty(propertyName)
            && grassMaterial.HasProperty(propertyName))
        {
            grassMaterial.SetVector(
                propertyName,
                terrainMaterial.GetVector(propertyName));
        }
    }

    private void CopyMaterialFloat(string propertyName)
    {
        if (terrainMaterial.HasProperty(propertyName)
            && grassMaterial.HasProperty(propertyName))
        {
            grassMaterial.SetFloat(
                propertyName,
                terrainMaterial.GetFloat(propertyName));
        }
    }

    private static void ConfigureShoreWaves(
        Material material,
        Texture noise,
        float strength,
        float spacing,
        float speed,
        float depth,
        float noiseWorldSize)
    {
        material.SetTexture("_NoiseTex", noise);
        material.SetFloat("_ShoreWaveStrength", strength);
        material.SetFloat("_ShoreWaveSpacing", spacing);
        material.SetFloat("_ShoreWaveSpeed", speed);
        material.SetFloat("_ShoreWaveDepth", depth);
        material.SetFloat("_ShoreWaveNoiseWorldSize", noiseWorldSize);
    }

    private void ApplyGrassColourSettings()
    {
        appliedGrassColourA = Rendering.GrassColourA;
        appliedGrassColourB = Rendering.GrassColourB;
        appliedGrassColourNoiseWorldSize = Rendering.GrassColourNoiseWorldSizeMetres;
        terrainMaterial?.SetColor("_GrassColorA", appliedGrassColourA.Value);
        terrainMaterial?.SetColor("_GrassColorB", appliedGrassColourB.Value);
        terrainMaterial?.SetFloat(
            "_GrassColorNoiseWorldSize",
            appliedGrassColourNoiseWorldSize);
        terrainLod1Material?.SetColor("_GrassColorA", appliedGrassColourA.Value);
        terrainLod1Material?.SetColor("_GrassColorB", appliedGrassColourB.Value);
        terrainLod1Material?.SetFloat(
            "_GrassColorNoiseWorldSize",
            appliedGrassColourNoiseWorldSize);
        terrainLod2Material?.SetColor("_GrassColorA", appliedGrassColourA.Value);
        terrainLod2Material?.SetColor("_GrassColorB", appliedGrassColourB.Value);
        terrainLod2Material?.SetFloat(
            "_GrassColorNoiseWorldSize",
            appliedGrassColourNoiseWorldSize);
        grassMaterial?.SetColor("_GrassColorA", appliedGrassColourA.Value);
        grassMaterial?.SetColor("_GrassColorB", appliedGrassColourB.Value);
        grassMaterial?.SetFloat(
            "_GrassColorNoiseWorldSize",
            appliedGrassColourNoiseWorldSize);
    }

    private void ApplyGrassWindSettings()
    {
        appliedGrassWindDirection = Rendering.GrassWindDirection;
        appliedGrassWindStrength = Rendering.GrassWindStrengthMetres;
        appliedGrassWindSpeed = Rendering.GrassWindSpeedMetresPerSecond;
        appliedGrassWindGustSize = Rendering.GrassWindGustSizeMetres;
        appliedGrassWindNormalStrength = Rendering.GrassWindNormalStrength;
        var direction = new Vector4(
            appliedGrassWindDirection.x,
            0f,
            appliedGrassWindDirection.y,
            0f);
        ApplyGrassWindSettingsToMaterial(terrainMaterial, direction);
        ApplyGrassWindSettingsToMaterial(terrainLod1Material, direction);
        ApplyGrassWindSettingsToMaterial(terrainLod2Material, direction);
        ApplyGrassWindSettingsToMaterial(grassMaterial, direction);
        ApplyGrassWindSettingsToMaterial(treeWoodMaterial, direction);
        ApplyGrassWindSettingsToMaterial(treeLod1WoodMaterial, direction);
        ApplyGrassWindSettingsToMaterial(treeFoliageMaterial, direction);
        ApplyGrassWindSettingsToMaterial(treeLod0FoliageMaterial, direction);
        ApplyGrassWindSettingsToMaterial(reedMaterial, direction);
        ApplyGrassWindSettingsToMaterial(fernMaterial, direction);
    }

    private void ApplyGrassWindSettingsToMaterial(
        Material material,
        Vector4 direction)
    {
        material?.SetVector("_GrassWindDirection", direction);
        material?.SetFloat("_GrassWindStrength", appliedGrassWindStrength);
        material?.SetFloat("_GrassWindSpeed", appliedGrassWindSpeed);
        material?.SetFloat("_GrassWindWorldSize", appliedGrassWindGustSize);
        material?.SetFloat(
            "_GrassWindNormalStrength",
            appliedGrassWindNormalStrength);
    }

    private void CreateSurfaceTextures(IslandPreparedSurfaceMaps surfaceMaps)
    {
        terrainOcclusionTexture = CreateSurfaceTexture(
            "Motu Shared Terrain Occlusion",
            surfaceMaps.dimension,
            TextureFormat.R8,
            surfaceMaps.occlusion);
        islandRuntime?.OwnTexture(terrainOcclusionTexture);
        terrainNormalTexture = CreateSurfaceTexture(
            "Motu Shared Terrain World Normal",
            surfaceMaps.dimension,
            TextureFormat.RGB24,
            surfaceMaps.normalRgb);
        islandRuntime?.OwnTexture(terrainNormalTexture);
        if (!terrainMaterial.HasProperty("_WorldNormal")
            || !terrainMaterial.HasProperty("_Occlusion"))
        {
            throw new InvalidOperationException(
                "The unified terrain shader does not expose its shared surface textures.");
        }
        terrainMaterial.SetTexture("_WorldNormal", terrainNormalTexture);
        terrainMaterial.SetTexture("_Occlusion", terrainOcclusionTexture);
        terrainLod1Material.SetTexture("_WorldNormal", terrainNormalTexture);
        terrainLod1Material.SetTexture("_Occlusion", terrainOcclusionTexture);
        terrainLod2Material.SetTexture("_WorldNormal", terrainNormalTexture);
        terrainLod2Material.SetTexture("_Occlusion", terrainOcclusionTexture);
    }

    private void CreateSeaMaskTexture(IslandPreparedSeaMask seaMask)
    {
        if (!SystemInfo.SupportsTextureFormat(TextureFormat.RGBA32))
        {
            throw new InvalidOperationException(
                "This graphics device does not support the required RGBA32 sea mask texture.");
        }
        if (coastalWaterMaterial == null)
        {
            throw new InvalidOperationException(
                "The coastal-water material was not created before its sea mask.");
        }
        seaMaskTexture = CreateSurfaceTexture(
            "Motu Coastal Wave Mask",
            seaMask.dimension,
            TextureFormat.RGBA32,
            seaMask.rgba);
        islandRuntime.OwnTexture(seaMaskTexture);
        coastalWaterMaterial.SetTexture("_SeaMask", seaMaskTexture);
    }

    private void CreateCoastalWaterOverlay(float worldSize)
    {
        if (runtimeRoot == null || coastalWaterMaterial == null)
        {
            throw new InvalidOperationException(
                "Coastal water requires an installed island root and material.");
        }
        coastalWaterObject = GameObject.CreatePrimitive(PrimitiveType.Plane);
        coastalWaterObject.name = "Island Coastal Water Overlay";
        coastalWaterObject.transform.SetParent(runtimeRoot.transform, false);
        coastalWaterObject.transform.localPosition = Vector3.up
            * (SeaHeight + CoastalWaterVerticalOffset);
        coastalWaterObject.transform.localRotation = Quaternion.identity;
        coastalWaterObject.transform.localScale = Vector3.one
            * (Mathf.Max(worldSize, 1f) / UnityPlaneSizeMetres);
        var waterLayer = LayerMask.NameToLayer("Water");
        if (waterLayer >= 0)
        {
            coastalWaterObject.layer = waterLayer;
        }
        DestroyUnityObject(coastalWaterObject.GetComponent<Collider>());
        var renderer = coastalWaterObject.GetComponent<MeshRenderer>();
        renderer.sharedMaterial = coastalWaterMaterial;
        renderer.shadowCastingMode = ShadowCastingMode.Off;
        renderer.receiveShadows = true;
        renderer.lightProbeUsage = LightProbeUsage.Off;
        renderer.reflectionProbeUsage = ReflectionProbeUsage.Off;
        renderer.allowOcclusionWhenDynamic = false;
        coastalWaterObject.SetActive(Rendering.ShowSea);
    }

    private void TransferMaterialOwnershipToRuntime()
    {
        if (islandRuntime == null)
        {
            throw new InvalidOperationException(
                "Per-island materials require an installing island runtime.");
        }
        islandRuntime.OwnTerrainTextureArrays(
            terrainMaterialTextures,
            terrainMaterial,
            grassMaterial);
        islandRuntime.OwnMaterial(terrainMaterial);
        islandRuntime.OwnMaterial(terrainLod1Material);
        islandRuntime.OwnMaterial(terrainLod2Material);
        islandRuntime.OwnMaterial(grassMaterial);
        islandRuntime.OwnMaterial(rockMaterial);
        islandRuntime.OwnMaterial(treeWoodMaterial);
        islandRuntime.OwnMaterial(treeLod1WoodMaterial);
        islandRuntime.OwnMaterial(treeFoliageMaterial);
        islandRuntime.OwnMaterial(treeLod0FoliageMaterial);
        islandRuntime.OwnMaterial(reedMaterial);
        islandRuntime.OwnMaterial(fernMaterial);
        islandRuntime.OwnMaterial(riverMaterial);
        islandRuntime.OwnMaterial(coastalWaterMaterial);
        islandRuntime.OwnMaterial(meshEdgeMaterial);
        if (ownsCliffNoiseTexture) islandRuntime.OwnTexture(cliffNoiseTexture);
        if (ownsRiverNoiseTexture) islandRuntime.OwnTexture(riverNoiseTexture);
        if (ownsGrassPatchNoiseTexture)
        {
            islandRuntime.OwnTexture(grassPatchNoiseTexture);
        }
    }

    private static Texture2D CreateSurfaceTexture(
        string textureName,
        int dimension,
        TextureFormat format,
        byte[] pixels)
    {
        var texture = new Texture2D(dimension, dimension, format, true, true)
        {
            name = textureName,
            filterMode = FilterMode.Trilinear,
            wrapMode = TextureWrapMode.Clamp,
            anisoLevel = 4,
        };
        try
        {
            // Rust supplies only mip 0. LoadRawTextureData expects storage for the
            // entire mip chain when the texture was created with mipmaps enabled.
            // Upload the base mip explicitly and let Apply generate the rest.
            texture.SetPixelData(pixels, 0);
            texture.Apply(true, true);
            return texture;
        }
        catch
        {
            DestroyUnityObject(texture);
            throw;
        }
    }

    private void DestroyRuntimeMaterials()
    {
        terrainMaterialTextures?.Unbind(terrainMaterial, grassMaterial);
        terrainMaterialTextures?.Dispose();
        terrainMaterialTextures = null;
        if (!environmentResourcesInstalled)
        {
            DestroyUnityObject(skyDomeMaterial);
            DestroyUnityObject(seaMaterial);
            DestroyUnityObject(cloudWeatherTexture);
            if (ownsSeaNoiseTexture) DestroyUnityObject(seaNoiseTexture);
        }
        DestroyUnityObject(terrainMaterial);
        DestroyUnityObject(terrainLod1Material);
        DestroyUnityObject(terrainLod2Material);
        DestroyUnityObject(grassMaterial);
        DestroyUnityObject(rockMaterial);
        DestroyUnityObject(treeWoodMaterial);
        DestroyUnityObject(treeLod1WoodMaterial);
        DestroyUnityObject(treeFoliageMaterial);
        DestroyUnityObject(treeLod0FoliageMaterial);
        DestroyUnityObject(reedMaterial);
        DestroyUnityObject(fernMaterial);
        DestroyUnityObject(riverMaterial);
        DestroyUnityObject(coastalWaterMaterial);
        DestroyUnityObject(meshEdgeMaterial);
        if (ownsCliffNoiseTexture) DestroyUnityObject(cliffNoiseTexture);
        if (ownsRiverNoiseTexture) DestroyUnityObject(riverNoiseTexture);
        if (ownsGrassPatchNoiseTexture) DestroyUnityObject(grassPatchNoiseTexture);
        terrainMaterial = null;
        skyDomeMaterial = null;
        terrainLod1Material = null;
        terrainLod2Material = null;
        grassMaterial = null;
        rockMaterial = null;
        treeWoodMaterial = null;
        treeLod1WoodMaterial = null;
        treeFoliageMaterial = null;
        treeLod0FoliageMaterial = null;
        reedMaterial = null;
        fernMaterial = null;
        riverMaterial = null;
        coastalWaterMaterial = null;
        seaMaterial = null;
        meshEdgeMaterial = null;
        cliffNoiseTexture = null;
        riverNoiseTexture = null;
        seaNoiseTexture = null;
        grassPatchNoiseTexture = null;
        cloudWeatherTexture = null;
        environmentResourcesInstalled = false;
        ownsCliffNoiseTexture = false;
        ownsRiverNoiseTexture = false;
        ownsSeaNoiseTexture = false;
        ownsGrassPatchNoiseTexture = false;
        appliedCloudSeed = int.MinValue;
        appliedCloudResolution = 0;
        if (worldEnvironment == null)
        {
            Shader.SetGlobalFloat(CloudEnabledId, 0f);
        }
    }

    private void ResetAppliedLiveSettings()
    {
        appliedShowRivers = null;
        appliedShowSea = null;
        appliedShowGrass = null;
        appliedShowRocks = null;
        appliedShowForests = null;
        appliedShowReeds = null;
        appliedReedBaseColour = null;
        appliedReedTipColour = null;
        appliedReedWindStrength = float.NaN;
        appliedShowFerns = null;
        appliedFernBaseColour = null;
        appliedFernTipColour = null;
        appliedFernWindStrength = float.NaN;
        appliedShowMeshEdges = null;
        appliedShowTreeMeshEdges = null;
        appliedWaterfallDebug = null;
        appliedGrassColourA = null;
        appliedGrassColourB = null;
        appliedGrassColourNoiseWorldSize = float.NaN;
        appliedGrassBrightness = float.NaN;
        appliedGrassWindDirection = new Vector2(float.NaN, float.NaN);
        appliedGrassWindStrength = float.NaN;
        appliedGrassWindSpeed = float.NaN;
        appliedGrassWindGustSize = float.NaN;
        appliedGrassWindNormalStrength = float.NaN;
        hasAppliedWorldToLocal = false;
    }

    private static void DestroyUnityObject(UnityEngine.Object value)
    {
        if (value != null)
        {
            Destroy(value);
        }
    }

    private static Material CreateMaterial(
        string shaderName,
        Color color,
        Material template,
        float worldSize)
    {
        Material material;
        if (template != null)
        {
            material = new Material(template);
        }
        else
        {
            var shader = Shader.Find(shaderName) ?? Shader.Find("Standard");
            if (shader == null)
            {
                throw new InvalidOperationException($"Could not find shader '{shaderName}'.");
            }
            material = new Material(shader);
            material.color = color;
            if (material.HasProperty("_BaseColor"))
            {
                material.SetColor("_BaseColor", color);
            }
        }
        material.name = $"{shaderName} (Island Instance)";
        if (material.HasProperty("_WorldSize"))
        {
            material.SetFloat("_WorldSize", worldSize);
        }

        return material;
    }
}
