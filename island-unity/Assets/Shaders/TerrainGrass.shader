Shader "Motu/Terrain Grass"
{
    Properties
    {
        [HideInInspector] _GrassEnabled ("Grass Enabled", Float) = 0
        _GrassPlayerPosition ("Player Position", Vector) = (0, 0, 0, 0)
        _GrassRadius ("Grass Outer Radius (metres)", Float) = 20
        _GrassFadeWidth ("Grass Edge Fade (metres)", Range(0.1, 20)) = 10
        _GrassHeight ("Grass Height (metres)", Range(0.02, 0.5)) = 0.14
        _GrassDensity ("Grass Tufts Per Metre", Range(2, 32)) = 24
        _GrassBladeWidth ("Grass Blade Width", Range(0.04, 0.5)) = 0.48
        _GrassRootColor ("Grass Root", Color) = (0.14, 0.34, 0.11, 1)
        _GrassTipColor ("Grass Tip", Color) = (0.26, 0.62, 0.21, 1)
        [NoScaleOffset] _GrassPatchNoise ("Grass Patch Noise", 2D) = "white" {}
        _GrassPatchNoiseWorldSize ("Grass Patch Repeat (metres)", Float) = 32
        _GrassBrightness ("Grass Brightness", Range(0.25, 3)) = 1.35
        [HideInInspector] _GrassLightDirection ("Light Direction", Vector) = (0, 1, 0, 0)
        [HideInInspector] _GrassLightColor ("Light Color", Color) = (1, 1, 1, 1)
        [HideInInspector] _GrassAmbientColor ("Ambient Color", Color) = (0.42, 0.46, 0.52, 1)
        [NoScaleOffset] _CliffNoise3D ("Terrain 3D Noise", 3D) = "gray" {}
        _SnowLine ("Snow Line (metres)", Float) = 100
        _SnowEdgeNoiseMetres ("Snow Edge Noise (metres)", Range(0, 10)) = 2.5
        _SnowMacroNoiseMetres ("Snow Macro Noise (metres)", Range(0, 40)) = 18
        _BeachMaximumElevation ("Sand Maximum Elevation (metres)", Float) = 3
        _SandPatchNoiseWorldSize ("Sand Patch Repeat (metres)", Float) = 32
        _RiverEdgeNoiseStrength ("River Edge Noise Strength", Range(0, 0.45)) = 0.20
        _RiverEdgeBlendWidth ("River Edge Blend Width", Range(0.01, 0.5)) = 0.20
        _CliffNormalCutoff ("Cliff Up-Normal Cutoff", Range(0, 1)) = 0.55
        _CliffBoundaryNoiseStrength ("Cliff Boundary Noise Strength", Range(0, 0.5)) = 0.30
        _RockBoundaryNoiseStrength ("Sand Rock Edge Noise Strength", Range(0, 0.4)) = 0.18
        _SandRockSlopeThreshold ("Sand Rock Slope Threshold", Range(0, 0.5)) = 0.10
        _CliffNoisePeriod ("Cliff Noise Period (metres)", Float) = 160
        _RockPatchNoiseDetailScale ("Rock Mask Detail Frequency", Range(1, 32)) = 8
    }

    SubShader
    {
        Tags { "RenderType"="TransparentCutout" "Queue"="AlphaTest+10" }
        LOD 350
        Cull Off
        ZWrite On

        Pass
        {
            Tags { "LightMode"="ForwardBase" }

            CGPROGRAM
            #define GRASS_SHELL_LAYER 0.0625
            #pragma vertex GrassVertex
            #pragma fragment GrassFragment
            #pragma target 3.0
            #pragma multi_compile_fwdbase
            #pragma multi_compile_fog
            #include "TerrainGrassCommon.cginc"
            ENDCG
        }
        Pass
        {
            Tags { "LightMode"="ForwardBase" }

            CGPROGRAM
            #define GRASS_SHELL_LAYER 0.125
            #pragma vertex GrassVertex
            #pragma fragment GrassFragment
            #pragma target 3.0
            #pragma multi_compile_fwdbase
            #pragma multi_compile_fog
            #include "TerrainGrassCommon.cginc"
            ENDCG
        }
        Pass
        {
            Tags { "LightMode"="ForwardBase" }

            CGPROGRAM
            #define GRASS_SHELL_LAYER 0.1875
            #pragma vertex GrassVertex
            #pragma fragment GrassFragment
            #pragma target 3.0
            #pragma multi_compile_fwdbase
            #pragma multi_compile_fog
            #include "TerrainGrassCommon.cginc"
            ENDCG
        }
        Pass
        {
            Tags { "LightMode"="ForwardBase" }

            CGPROGRAM
            #define GRASS_SHELL_LAYER 0.25
            #pragma vertex GrassVertex
            #pragma fragment GrassFragment
            #pragma target 3.0
            #pragma multi_compile_fwdbase
            #pragma multi_compile_fog
            #include "TerrainGrassCommon.cginc"
            ENDCG
        }
        Pass
        {
            Tags { "LightMode"="ForwardBase" }

            CGPROGRAM
            #define GRASS_SHELL_LAYER 0.3125
            #pragma vertex GrassVertex
            #pragma fragment GrassFragment
            #pragma target 3.0
            #pragma multi_compile_fwdbase
            #pragma multi_compile_fog
            #include "TerrainGrassCommon.cginc"
            ENDCG
        }
        Pass
        {
            Tags { "LightMode"="ForwardBase" }

            CGPROGRAM
            #define GRASS_SHELL_LAYER 0.375
            #pragma vertex GrassVertex
            #pragma fragment GrassFragment
            #pragma target 3.0
            #pragma multi_compile_fwdbase
            #pragma multi_compile_fog
            #include "TerrainGrassCommon.cginc"
            ENDCG
        }
        Pass
        {
            Tags { "LightMode"="ForwardBase" }

            CGPROGRAM
            #define GRASS_SHELL_LAYER 0.4375
            #pragma vertex GrassVertex
            #pragma fragment GrassFragment
            #pragma target 3.0
            #pragma multi_compile_fwdbase
            #pragma multi_compile_fog
            #include "TerrainGrassCommon.cginc"
            ENDCG
        }
        Pass
        {
            Tags { "LightMode"="ForwardBase" }

            CGPROGRAM
            #define GRASS_SHELL_LAYER 0.5
            #pragma vertex GrassVertex
            #pragma fragment GrassFragment
            #pragma target 3.0
            #pragma multi_compile_fwdbase
            #pragma multi_compile_fog
            #include "TerrainGrassCommon.cginc"
            ENDCG
        }
        Pass
        {
            Tags { "LightMode"="ForwardBase" }

            CGPROGRAM
            #define GRASS_SHELL_LAYER 0.5625
            #pragma vertex GrassVertex
            #pragma fragment GrassFragment
            #pragma target 3.0
            #pragma multi_compile_fwdbase
            #pragma multi_compile_fog
            #include "TerrainGrassCommon.cginc"
            ENDCG
        }
        Pass
        {
            Tags { "LightMode"="ForwardBase" }

            CGPROGRAM
            #define GRASS_SHELL_LAYER 0.625
            #pragma vertex GrassVertex
            #pragma fragment GrassFragment
            #pragma target 3.0
            #pragma multi_compile_fwdbase
            #pragma multi_compile_fog
            #include "TerrainGrassCommon.cginc"
            ENDCG
        }
        Pass
        {
            Tags { "LightMode"="ForwardBase" }

            CGPROGRAM
            #define GRASS_SHELL_LAYER 0.6875
            #pragma vertex GrassVertex
            #pragma fragment GrassFragment
            #pragma target 3.0
            #pragma multi_compile_fwdbase
            #pragma multi_compile_fog
            #include "TerrainGrassCommon.cginc"
            ENDCG
        }
        Pass
        {
            Tags { "LightMode"="ForwardBase" }

            CGPROGRAM
            #define GRASS_SHELL_LAYER 0.75
            #pragma vertex GrassVertex
            #pragma fragment GrassFragment
            #pragma target 3.0
            #pragma multi_compile_fwdbase
            #pragma multi_compile_fog
            #include "TerrainGrassCommon.cginc"
            ENDCG
        }
        Pass
        {
            Tags { "LightMode"="ForwardBase" }

            CGPROGRAM
            #define GRASS_SHELL_LAYER 0.8125
            #pragma vertex GrassVertex
            #pragma fragment GrassFragment
            #pragma target 3.0
            #pragma multi_compile_fwdbase
            #pragma multi_compile_fog
            #include "TerrainGrassCommon.cginc"
            ENDCG
        }
        Pass
        {
            Tags { "LightMode"="ForwardBase" }

            CGPROGRAM
            #define GRASS_SHELL_LAYER 0.875
            #pragma vertex GrassVertex
            #pragma fragment GrassFragment
            #pragma target 3.0
            #pragma multi_compile_fwdbase
            #pragma multi_compile_fog
            #include "TerrainGrassCommon.cginc"
            ENDCG
        }
        Pass
        {
            Tags { "LightMode"="ForwardBase" }

            CGPROGRAM
            #define GRASS_SHELL_LAYER 0.9375
            #pragma vertex GrassVertex
            #pragma fragment GrassFragment
            #pragma target 3.0
            #pragma multi_compile_fwdbase
            #pragma multi_compile_fog
            #include "TerrainGrassCommon.cginc"
            ENDCG
        }
        Pass
        {
            Tags { "LightMode"="ForwardBase" }

            CGPROGRAM
            #define GRASS_SHELL_LAYER 1.0
            #pragma vertex GrassVertex
            #pragma fragment GrassFragment
            #pragma target 3.0
            #pragma multi_compile_fwdbase
            #pragma multi_compile_fog
            #include "TerrainGrassCommon.cginc"
            ENDCG
        }
    }

    FallBack Off
}
