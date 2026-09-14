Shader "Hidden/Motu/Fallen Log Wind Probe"
{
    SubShader
    {
        Pass
        {
            Cull Off ZWrite Off ZTest Always
            CGPROGRAM
            #pragma vertex vert_img
            #pragma fragment Fragment
            #pragma target 3.0
            #include "UnityCG.cginc"
            #include "Assets/Shaders/TreeWindCommon.cginc"
            float4 _ProbeTreeData;
            float4 Fragment(v2f_img input) : SV_Target
            {
                float3 offset = MotuTreeWindOffset(float3(100, 50, 200), float3(0, 50, 0), _ProbeTreeData);
                return float4(offset, MotuHasTreeRoot(_ProbeTreeData));
            }
            ENDCG
        }
    }
}
