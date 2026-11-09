Shader "Motu/Planet Forest"
{
    Properties
    {
        [PerRendererData] _MotuForestAmbientWood ("Ambient only wood", Float) = 0
        [NoScaleOffset] _ForestNoise ("Planet forest noise", 3D) = "gray" {}
        [NoScaleOffset] _PalmFrond ("Shared palm frond cutout", 2D) = "white" {}
        [HideInInspector] _PalmLodDistances ("Palm LOD fade distances", Vector) = (24,30,228,240)
        [HideInInspector] _PalmLodEnabled ("Palm LOD fade enabled", Float) = 0
        [NoScaleOffset] _TwigCard ("Shared branch fan mask", 2D) = "white" {}
        [NoScaleOffset] _ForestBarkAlbedoHeight ("Species bark colour and height", 2DArray) = "white" {}
        [NoScaleOffset] _ForestBarkNormalOcclusion ("Species bark normal and occlusion", 2DArray) = "white" {}
        _ForestBarkTileSizes ("Broadleaf XY and pine ZW tile metres", Vector) = (1.2,1.6,0.85,1.5)
        _ForestPalmBarkTileSize ("Palm XY tile metres", Vector) = (0.8,1.2,0,0)
        [HideInInspector] _MotuForestViewer ("Forest viewer", Vector) = (0,0,0,0)
        [HideInInspector] _ForestKind ("Wood, leaves, shells, distant, twig cards", Float) = 0
        [Enum(UnityEngine.Rendering.CullMode)] _Cull ("Culling", Float) = 2
    }
    SubShader
    {
        Tags { "RenderPipeline"="UniversalPipeline" "RenderType"="TransparentCutout" "Queue"="AlphaTest" "MotuReflection"="Foliage" }
        Pass
        {
            Name "Forward"
            Tags { "LightMode"="UniversalForwardOnly" }
            Cull [_Cull]
            HLSLPROGRAM
            #pragma target 4.5
            #pragma vertex ForestVertex
            #pragma fragment ForestFragment
            #pragma multi_compile_instancing
            #pragma multi_compile_fog
            #pragma multi_compile _ _MAIN_LIGHT_SHADOWS _MAIN_LIGHT_SHADOWS_CASCADE _MAIN_LIGHT_SHADOWS_SCREEN
            #pragma multi_compile_fragment _ _SHADOWS_SOFT
            #pragma multi_compile _ _ADDITIONAL_LIGHTS
            #pragma multi_compile_fragment _ _ADDITIONAL_LIGHT_SHADOWS
            #include "PlanetForestCommon.hlsl"
            ENDHLSL
        }
        Pass
        {
            Name "ShadowCaster"
            Tags { "LightMode"="ShadowCaster" }
            Cull Off
            ZWrite On
            ColorMask 0
            HLSLPROGRAM
            #pragma target 4.5
            #pragma vertex ForestVertex
            #pragma fragment ForestFragment
            #pragma multi_compile_instancing
            #pragma multi_compile_vertex _ _CASTING_PUNCTUAL_LIGHT_SHADOW
            #define FOREST_SHADOW 1
            #include "PlanetForestCommon.hlsl"
            ENDHLSL
        }
        Pass
        {
            Name "DepthOnly"
            Tags { "LightMode"="DepthOnly" }
            Cull [_Cull]
            ColorMask R
            ZWrite On
            HLSLPROGRAM
            #pragma target 4.5
            #pragma vertex ForestVertex
            #pragma fragment ForestFragment
            #pragma multi_compile_instancing
            #define FOREST_DEPTH 1
            #include "PlanetForestCommon.hlsl"
            ENDHLSL
        }
        Pass
        {
            Name "Reflection"
            Tags { "LightMode"="MotuReflection" }
            Cull [_Cull]
            HLSLPROGRAM
            #pragma target 4.5
            #pragma vertex ForestVertex
            #pragma fragment ForestFragment
            #pragma multi_compile_instancing
            #pragma multi_compile_fog
            #pragma multi_compile _ _MAIN_LIGHT_SHADOWS _MAIN_LIGHT_SHADOWS_CASCADE
            #include "PlanetForestCommon.hlsl"
            ENDHLSL
        }
    }
    FallBack Off
}
