Shader "Hidden/Motu/Tests/Hull Surface Normal"
{
    Properties { _ProbePositions ("World sample positions", 2D) = "black" {}
        _ProbeOutput ("Output: normals or geometry", Float) = 0 }
    HLSLINCLUDE
    #include "Packages/com.motu.runtime/Runtime/Shaders/MotuUrp.hlsl"
    #include "Packages/com.motu.runtime/Runtime/Shaders/OceanSurfaceGeometry.cginc"
    sampler2D _ProbePositions;
    float _ProbeOutput;
    float4x4 _ProbeViewProjection;
    float3 _ProbeEye;

    float4 Probe(v2f_img input) : SV_Target
    {
        float3 basePosition = tex2Dlod(_ProbePositions, float4(input.uv, 0, 0)).xyz;
        float4 deck;
        float clamped;
        float3 surface = MotuOceanSurfacePosition(basePosition, 0, deck, clamped);
        if (_ProbeOutput > .5) return float4(surface, clamped);
        float3 waveNormal;
        float foam, crest, allowance, variance;
        MotuEvaluateOceanWaveNormal(basePosition.xz, 0, variance, waveNormal, foam, crest, allowance);
        float4 hullNormal = MotuOceanHullSurfaceNormal(basePosition, 0, surface, clamped);
        return float4(MotuOceanBlendHullNormal(waveNormal, hullNormal), hullNormal.w);
    }

    struct Input { float4 position : POSITION; };
    struct Output { float4 position : SV_POSITION; float3 world : TEXCOORD0; float2 samplePosition : TEXCOORD1; float4 normal : TEXCOORD2; };
    Output Vertex(Input input)
    {
        Output output;
        float4 deck;
        float clamped;
        output.world = MotuOceanSurfacePosition(input.position.xyz, 0, deck, clamped);
        output.normal = MotuOceanHullSurfaceNormal(input.position.xyz, 0, output.world, clamped);
        output.samplePosition = input.position.xz;
        output.position = mul(_ProbeViewProjection, float4(output.world, 1));
        return output;
    }
    half4 Shade(Output input) : SV_Target
    {
        MotuClipHullInterior(input.world);
        float3 waveNormal;
        float foam, crest, allowance, variance;
        MotuEvaluateOceanWaveNormal(input.samplePosition, 0, variance, waveNormal, foam, crest, allowance);
        float3 normal = MotuOceanBlendHullNormal(waveNormal, input.normal);
        float3 light = normalize(float3(-1, 1, .2));
        float3 view = normalize(_ProbeEye - input.world);
        float diffuse = .3 + saturate(dot(normal, light)) * .7;
        float highlight = pow(saturate(dot(normal, normalize(light + view))), 32);
        return half4(half3(.07,.24,.34) * diffuse + highlight * .8, 1);
    }
    ENDHLSL
    SubShader
    {
        Pass
        {
            ZTest Always Cull Off ZWrite Off
            HLSLPROGRAM
            #pragma target 3.5
            #pragma vertex vert_img
            #pragma fragment Probe
            ENDHLSL
        }
        Pass
        {
            ZTest LEqual Cull Off ZWrite On
            HLSLPROGRAM
            #pragma target 3.5
            #pragma vertex Vertex
            #pragma fragment Shade
            ENDHLSL
        }
    }
}
