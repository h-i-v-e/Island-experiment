Shader "Hidden/Motu/Tests/Ocean Refraction"
{
    Properties
    {
        _ProbeSafe ("Depth safe", Float) = 1
        _ProbeRipple ("Offset", Vector) = (1, 1, 0, 0)
        _RefractionStrength ("Strength", Float) = 0.1
        _RefractionDepth ("Depth", Float) = 0.6
    }
    SubShader
    {
        Tags { "Queue"="Transparent" }
        GrabPass { "_MotuWaterBackground" }
        Pass
        {
            CGPROGRAM
            #pragma vertex Vertex
            #pragma fragment Fragment
            #pragma target 3.5
            #include "Assets/Shaders/WaterCommon.cginc"
            #include "Assets/Shaders/OceanWaves.cginc"
            #include "Assets/Shaders/OceanOptics.cginc"
            float _ProbeSafe;
            float4 _ProbeRipple;
            struct Output
            {
                float4 position : SV_POSITION;
                float4 grab : TEXCOORD0;
                float4 screen : TEXCOORD1;
                float3 world : TEXCOORD2;
                float eye : TEXCOORD3;
            };
            Output Vertex(appdata_base input)
            {
                Output output;
                output.position = UnityObjectToClipPos(input.vertex);
                output.grab = ComputeGrabScreenPos(output.position);
                output.screen = ComputeScreenPos(output.position);
                output.world = mul(unity_ObjectToWorld, input.vertex).xyz;
                output.eye = -UnityObjectToViewPos(input.vertex).z;
                return output;
            }
            float4 Fragment(Output input) : SV_Target
            {
                float3 view = normalize(_WorldSpaceCameraPos - input.world);
                if (_ProbeSafe < .5)
                    return float4(tex2D(_MotuWaterBackground, input.grab.xy / input.grab.w
                        + _ProbeRipple.xy * _RefractionStrength * lerp(.25, 1, saturate(view.y))).rgb, 1);
                float depth = LinearEyeDepth(SAMPLE_DEPTH_TEXTURE_PROJ(_CameraDepthTexture, UNITY_PROJ_COORD(input.screen))) - input.eye;
                float path;
                return float4(MotuOceanRefract(input.grab, input.screen, input.eye, depth,
                    float3(0, 1, 0), view, _ProbeRipple.xy, path), 1);
            }
            ENDCG
        }
    }
}
