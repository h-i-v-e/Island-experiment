Shader "Motu/Planar Reflection Simplified"
{
    Properties
    {
        _Color ("Tint", Color) = (1, 1, 1, 1)
        _GroundDirtColor ("Dirt", Color) = (0.09, 0.055, 0.026, 1)
        _RockColor ("Rock", Color) = (0.30, 0.32, 0.29, 1)
        _SandColor ("Sand", Color) = (0.62, 0.57, 0.34, 1)
        _GrassColorA ("Grass A", Color) = (0.18, 0.46, 0.14, 1)
        _GrassColorB ("Grass B", Color) = (0.34, 0.50, 0.14, 1)
        _BaseColor ("Dark Surface", Color) = (0.10, 0.16, 0.06, 1)
        _LightColor ("Light Surface", Color) = (0.30, 0.46, 0.16, 1)
        [PerRendererData] _RockTint ("Rock Tint", Color) = (1, 1, 1, 1)
        _SnowLine ("Snow Line", Float) = 100
    }

    CGINCLUDE
    #include "UnityCG.cginc"
    #include "Lighting.cginc"

    struct ReflectionVertexInput
    {
        float4 vertex : POSITION;
        float3 normal : NORMAL;
        float4 material : COLOR;
        float2 environment : TEXCOORD1;
        UNITY_VERTEX_INPUT_INSTANCE_ID
    };

    struct ReflectionVertexOutput
    {
        float4 position : SV_POSITION;
        half3 worldNormal : TEXCOORD0;
        float3 worldPosition : TEXCOORD1;
        float3 localPosition : TEXCOORD2;
        half4 material : TEXCOORD3;
        half2 environment : TEXCOORD4;
        UNITY_FOG_COORDS(5)
        UNITY_VERTEX_OUTPUT_STEREO
    };

    fixed4 _Color;
    fixed4 _GroundDirtColor;
    fixed4 _RockColor;
    fixed4 _SandColor;
    fixed4 _GrassColorA;
    fixed4 _GrassColorB;
    fixed4 _BaseColor;
    fixed4 _LightColor;
    fixed4 _RockTint;
    float _SnowLine;
    float4x4 _IslandWorldToLocal;

    ReflectionVertexOutput ReflectionVertex(ReflectionVertexInput input)
    {
        ReflectionVertexOutput output;
        UNITY_SETUP_INSTANCE_ID(input);
        UNITY_INITIALIZE_OUTPUT(ReflectionVertexOutput, output);
        UNITY_INITIALIZE_VERTEX_OUTPUT_STEREO(output);
        output.position = UnityObjectToClipPos(input.vertex);
        output.worldPosition = mul(unity_ObjectToWorld, input.vertex).xyz;
        output.localPosition = mul(
            _IslandWorldToLocal,
            float4(output.worldPosition, 1.0)).xyz;
        output.worldNormal = UnityObjectToWorldNormal(input.normal);
        output.material = input.material;
        output.environment = input.environment;
        UNITY_TRANSFER_FOG(output, output.position);
        return output;
    }

    fixed3 ReflectionLighting(
        fixed3 albedo,
        half3 worldNormal,
        float3 worldPosition)
    {
        half3 normal = normalize(worldNormal);
        half3 ambient = max(ShadeSH9(half4(normal, 1.0h)), 0.08h);
        half3 lightDirection = normalize(UnityWorldSpaceLightDir(worldPosition));
        half diffuse = saturate(dot(normal, lightDirection));
        return albedo * (ambient + _LightColor0.rgb * (0.18h + diffuse * 0.72h));
    }

    fixed4 FinishReflection(
        fixed3 albedo,
        ReflectionVertexOutput input)
    {
        fixed4 result = fixed4(
            ReflectionLighting(albedo, input.worldNormal, input.worldPosition),
            1.0h);
        UNITY_APPLY_FOG(input.fogCoord, result);
        return result;
    }

    fixed4 TerrainFragment(ReflectionVertexOutput input) : SV_Target
    {
        half up = saturate(normalize(input.worldNormal).y);
        half slope = 1.0h - up;
        half looseCover = saturate(input.material.g);
        half materialRock = saturate(input.material.r) * (1.0h - looseCover);
        half rockWeight = max(
            materialRock,
            smoothstep(0.18h, 0.58h, slope));
        rockWeight = max(rockWeight, saturate(input.environment.y) * 0.65h);
        fixed3 grass = lerp(_GrassColorA.rgb, _GrassColorB.rgb, 0.45h);
        fixed3 albedo = lerp(
            _GroundDirtColor.rgb,
            grass,
            smoothstep(0.12h, 0.72h, looseCover));
        half sandWeight = (1.0h - smoothstep(0.5h, 4.0h, input.localPosition.y))
            * (1.0h - rockWeight);
        albedo = lerp(albedo, _SandColor.rgb, sandWeight);
        albedo = lerp(albedo, _RockColor.rgb, rockWeight);
        half snowWeight = smoothstep(
            _SnowLine,
            _SnowLine + 8.0,
            input.localPosition.y) * smoothstep(0.12h, 0.55h, up);
        albedo = lerp(albedo, fixed3(0.82h, 0.84h, 0.81h), snowWeight);
        return FinishReflection(albedo * _Color.rgb, input);
    }

    fixed4 GrassFragment(ReflectionVertexOutput input) : SV_Target
    {
        fixed3 albedo = lerp(_GrassColorA.rgb, _GrassColorB.rgb, 0.45h);
        return FinishReflection(albedo, input);
    }

    fixed4 WoodFragment(ReflectionVertexOutput input) : SV_Target
    {
        fixed3 albedo = lerp(_BaseColor.rgb, _LightColor.rgb, 0.38h);
        return FinishReflection(albedo, input);
    }

    fixed4 FoliageFragment(ReflectionVertexOutput input) : SV_Target
    {
        fixed3 albedo = lerp(_BaseColor.rgb, _LightColor.rgb, 0.42h);
        return FinishReflection(albedo, input);
    }

    fixed4 RockFragment(ReflectionVertexOutput input) : SV_Target
    {
        return FinishReflection(_RockColor.rgb * _RockTint.rgb, input);
    }
    ENDCG

    SubShader
    {
        Tags { "MotuReflection"="Terrain" "RenderType"="Opaque" }
        LOD 50
        Pass
        {
            CGPROGRAM
            #pragma vertex ReflectionVertex
            #pragma fragment TerrainFragment
            #pragma target 3.0
            #pragma multi_compile_fog
            #pragma multi_compile_instancing
            ENDCG
        }
    }

    SubShader
    {
        Tags { "MotuReflection"="Grass" "RenderType"="Opaque" }
        LOD 50
        Cull Off
        Pass
        {
            CGPROGRAM
            #pragma vertex ReflectionVertex
            #pragma fragment GrassFragment
            #pragma target 3.0
            #pragma multi_compile_fog
            #pragma multi_compile_instancing
            ENDCG
        }
    }

    SubShader
    {
        Tags { "MotuReflection"="Wood" "RenderType"="Opaque" }
        LOD 50
        Pass
        {
            CGPROGRAM
            #pragma vertex ReflectionVertex
            #pragma fragment WoodFragment
            #pragma target 3.0
            #pragma multi_compile_fog
            #pragma multi_compile_instancing
            ENDCG
        }
    }

    SubShader
    {
        Tags { "MotuReflection"="Foliage" "RenderType"="Opaque" }
        LOD 50
        Cull Off
        Pass
        {
            CGPROGRAM
            #pragma vertex ReflectionVertex
            #pragma fragment FoliageFragment
            #pragma target 3.0
            #pragma multi_compile_fog
            #pragma multi_compile_instancing
            ENDCG
        }
    }

    SubShader
    {
        Tags { "MotuReflection"="Rock" "RenderType"="Opaque" }
        LOD 50
        Pass
        {
            CGPROGRAM
            #pragma vertex ReflectionVertex
            #pragma fragment RockFragment
            #pragma target 3.0
            #pragma multi_compile_fog
            #pragma multi_compile_instancing
            ENDCG
        }
    }

    FallBack Off
}
