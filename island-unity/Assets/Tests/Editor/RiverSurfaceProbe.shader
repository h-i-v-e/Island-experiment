Shader "Hidden/Motu/Tests/River Surface"
{
    Properties { _MainTex ("Input", 2D) = "black" {} }
    SubShader
    {
        Pass
        {
            ZTest Always ZWrite Off Cull Off
            CGPROGRAM
            #pragma vertex vert_img
            #pragma fragment Frag
            #pragma target 3.5
            #include "Assets/Shaders/WaterCommon.cginc"
            #include "Assets/Shaders/WaterOptics.cginc"
            #include "Assets/Shaders/RiverSurface.cginc"
            float _ProbeMode, _ProbeTime, _ProbeSpan, _ProbeRapids, _ProbeMirror;
            float4 _ProbeOffset;
            float4x4 _ProbeRotation;
            float4 Frag(v2f_img input) : SV_Target
            {
                float2 metres = input.uv * _ProbeSpan + _ProbeOffset.xy;
                if (_ProbeMirror > .5) metres.x = _ProbeSpan - metres.x;
                if (_ProbeMode > 1.5) return float4(MotuRiverWaterfallNoise(metres, _ProbeTime), 0, 1);
                if (_ProbeMode > .5) return MotuRiverFoam(metres, _ProbeTime, _ProbeRapids);
                float3 position = mul((float3x3)_ProbeRotation, float3(input.uv.x, 0, input.uv.y) * _ProbeSpan);
                float3 baseNormal = mul((float3x3)_ProbeRotation, float3(0, 1, 0));
                float roughness;
                float3 normal = MotuRiverDetailNormal(position, baseNormal, metres, _ProbeTime, _ProbeRapids, roughness);
                return float4(normal, roughness);
            }
            ENDCG
        }
    }
}
