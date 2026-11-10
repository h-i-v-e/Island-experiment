Shader "Hidden/Motu/Squall Shadow Cookie"
{
    SubShader
    {
        Pass
        {
            Cull Off ZWrite Off ZTest Always
            CGPROGRAM
            #pragma vertex vert_img
            #pragma fragment Fragment
            #pragma target 3.5
            #include "UnityCG.cginc"
            #include "../WorldSquallLighting.cginc"
            float4 _CookieOrigin, _CookieRight, _CookieUp, _CookieLightDirection;
            float _CookieSeaLevel, _PreviousChannel;
            float4 _PreviousTransform;
            sampler2D _PreviousCookie;

            half4 Fragment(v2f_img input) : SV_Target
            {
                float3 position = _CookieOrigin.xyz + (input.uv.x - .5) * _CookieRight.xyz
                    + (input.uv.y - .5) * _CookieUp.xyz;
                float3 direction = _CookieLightDirection.xyz;
                if (direction.y > .02) position -= direction * ((position.y - _CookieSeaLevel) / direction.y);
                float shadow = MotuSquallCloudTransmittance(position, direction);
                float2 previousUV = (input.uv - .5) * _PreviousTransform.xy + _PreviousTransform.zw;
                half4 authored = tex2D(_PreviousCookie, previousUV);
                half3 colour = _PreviousChannel > 1.5 ? authored.rrr : _PreviousChannel > .5 ? authored.aaa : authored.rgb;
                return half4(colour * shadow, 1);
            }
            ENDCG
        }
    }
}
