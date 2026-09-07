Shader "Hidden/Motu/Vegetation Wind Probe"
{
    SubShader
    {
        Pass
        {
            ZTest Always Cull Off ZWrite Off
            CGPROGRAM
            #pragma vertex vert_img
            #pragma fragment Frag
            #pragma target 3.0
            #include "UnityCG.cginc"
            #include "../Shaders/TreeWindCommon.cginc"
            #include "../Shaders/CloudCommon.cginc"
            float4 _ProbePosition;
            float _ProbeMode;

            float4 Frag(v2f_img input) : SV_Target
            {
                float2 position = _ProbePosition.xy;
                if (_ProbeMode > 2.5)
                {
                    float3 pinned = MotuTreeWindOffsetAtHeight(
                        float3(position.x, 0, position.y), float3(0, 0, 0),
                        float4(.5, .5, 0, .5), 0);
                    return float4(pinned, 0);
                }
                if (_ProbeMode > 1.5)
                    return MotuCloudBroadDensity(position).xxxx;
                if (_ProbeMode > .5)
                    return float4(MotuCloudWeatherUv(position), 0, 0);
                float3 wind = MotuWindSample(position);
                float2 displacement = wind.xz * (MotuWindDisplacementStrength() * wind.y);
                float3 tree = MotuTreeWindOffsetAtHeight(
                    float3(position.x, 32, position.y), float3(0, 32, 0),
                    float4(.5, .5, 0, .5), 32);
                return float4(displacement, tree.xz);
            }
            ENDCG
        }
    }
}
