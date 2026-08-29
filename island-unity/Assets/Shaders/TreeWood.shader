Shader "Motu/Tree Wood"
{
    Properties
    {
        _BaseColor ("Dark Bark", Color) = (0.16, 0.085, 0.045, 1)
        _LightColor ("Light Bark", Color) = (0.42, 0.28, 0.14, 1)
        _BarkContrast ("Bark Colour Variation", Range(0, 1)) = 0.42
        [NoScaleOffset] _BarkAlbedoMap ("Bark Recipe Albedo", 2D) = "gray" {}
        [NoScaleOffset] _BarkHeightMap ("Bark Recipe Height", 2D) = "gray" {}
        [NoScaleOffset] _BarkNormalMap ("Bark Recipe Normal", 2D) = "bump" {}
        [NoScaleOffset] _BarkOcclusionMap ("Bark Recipe Occlusion", 2D) = "white" {}
        _BarkTileWidthMetres ("Bark Tile Width (metres)", Float) = 1
        _BarkTileHeightMetres ("Bark Tile Height (metres)", Float) = 1
        _BarkNormalMapStrength ("Bark Recipe Normal Strength", Range(0, 2)) = 0.7
        _BarkParallaxStrengthMetres ("Bark Recipe Parallax (metres)", Range(0, 0.08)) = 0.05
        _BarkOcclusionStrength ("Bark Recipe Occlusion Strength", Range(0, 1)) = 0.7
        _BarkAmbientFloor ("Bark Ambient Floor", Range(0, 0.5)) = 0.16
        [HideInInspector] _WorldSize ("Island World Size", Float) = 2000
        [NoScaleOffset] _CliffNoise3D ("Tree Surface Noise", 3D) = "gray" {}
        _TreeNoisePeriod ("Bark Noise Period (metres)", Float) = 7
        _TreeNoiseDetailScale ("Bark Detail Frequency", Range(1, 16)) = 4
        _TreeNoiseFineScale ("Bark Fine Frequency", Range(2, 48)) = 18
        _TreeNormalStrength ("Bark Normal Strength", Range(0, 0.5)) = 0.14
        _TreeHueVariationDegrees ("Bark Hue Variation", Range(0, 30)) = 8
        [NoScaleOffset] [HideInInspector] _GrassPatchNoise ("Shared Wind Noise", 2D) = "white" {}
        [HideInInspector] _GrassWindDirection ("Wind Direction", Vector) = (1, 0, 0.35, 0)
        [HideInInspector] _GrassWindStrength ("Grass Wind Strength", Float) = 0.07
        [HideInInspector] _GrassWindSpeed ("Wind Speed", Float) = 1.8
        [HideInInspector] _GrassWindWorldSize ("Wind Gust Size", Float) = 12
        _TreeWindStrengthMultiplier ("Tree Wind Strength", Range(0, 10)) = 5
        _TreeWindBasePinHeight ("Pinned Trunk Height (metres)", Range(0, 4)) = 0.6
        _TreeWindFullBendHeight ("Full Bend Height (metres)", Range(1, 24)) = 9
    }

    SubShader
    {
        Tags { "RenderType"="Opaque" "Queue"="Geometry" "MotuReflection"="Wood" }
        LOD 200

        Pass
        {
            Tags { "LightMode"="ForwardBase" }

            CGPROGRAM
            #pragma vertex Vertex
            #pragma fragment Fragment
            #pragma target 3.0
            #pragma multi_compile_fwdbase
            #pragma multi_compile_fog
            #pragma multi_compile_instancing
            #pragma shader_feature_local_fragment _ MOTU_TREE_BARK_NO_PARALLAX

            #include "UnityCG.cginc"
            #include "Lighting.cginc"
            #include "AutoLight.cginc"
            #include "TreeSurfaceNoise.cginc"
            #include "TreeWindCommon.cginc"

            struct VertexInput
            {
                float4 vertex : POSITION;
                float3 normal : NORMAL;
                float2 barkAxis : TEXCOORD0;
                float4 treeData : COLOR;
                UNITY_VERTEX_INPUT_INSTANCE_ID
            };

            struct VertexOutput
            {
                float4 pos : SV_POSITION;
                float3 worldPosition : TEXCOORD0;
                float3 worldNormal : TEXCOORD1;
                SHADOW_COORDS(2)
                UNITY_FOG_COORDS(3)
                float3 islandLocalPosition : TEXCOORD4;
                float2 barkAxis : TEXCOORD5;
                float4 treeData : TEXCOORD6;
                UNITY_VERTEX_INPUT_INSTANCE_ID
                UNITY_VERTEX_OUTPUT_STEREO
            };

            fixed4 _BaseColor;
            fixed4 _LightColor;
            half _BarkContrast;
            sampler2D _BarkAlbedoMap;
            sampler2D _BarkHeightMap;
            sampler2D _BarkNormalMap;
            sampler2D _BarkOcclusionMap;
            float _BarkTileWidthMetres;
            float _BarkTileHeightMetres;
            half _BarkNormalMapStrength;
            half _BarkParallaxStrengthMetres;
            half _BarkOcclusionStrength;
            half _BarkAmbientFloor;
            struct BarkRecipeSample
            {
                fixed3 albedo;
                half3 worldNormal;
                half occlusion;
            };

            float3 DecodeBarkAxis(float2 encoded)
            {
                float2 unfolded = encoded * 2.0 - 1.0;
                float3 axis = float3(
                    unfolded.x,
                    unfolded.y,
                    1.0 - abs(unfolded.x) - abs(unfolded.y));
                if (axis.z < 0.0)
                {
                    axis.xy = (1.0 - abs(axis.yx)) * sign(axis.xy);
                }
                return normalize(axis);
            }

            half3 StrengthenRecipeNormal(half3 normal)
            {
                normal.xy *= _BarkNormalMapStrength;
                normal.z = sqrt(saturate(1.0 - dot(normal.xy, normal.xy)));
                return normalize(normal);
            }

            float2 ParallaxBarkUv(
                float2 uv,
                float2 tileSizeMetres,
                float2 viewDirectionAlongSurface,
                half viewDepth)
            {
                float2 rayOffset = viewDirectionAlongSurface
                    / max(tileSizeMetres, float2(0.01, 0.01))
                    * (_BarkParallaxStrengthMetres / max(viewDepth, 0.2h));
                const half layerStep = 0.125h;
                float2 uvStep = rayOffset * layerStep;
                float2 currentUv = uv;
                half currentLayer = 0.0h;
                half surfaceDepth = 1.0h - tex2D(_BarkHeightMap, currentUv).r;
                [unroll]
                for (int stepIndex = 0; stepIndex < 8; ++stepIndex)
                {
                    if (currentLayer >= surfaceDepth)
                    {
                        break;
                    }
                    currentUv -= uvStep;
                    currentLayer += layerStep;
                    surfaceDepth = 1.0h - tex2D(_BarkHeightMap, currentUv).r;
                }

                float2 previousUv = currentUv + uvStep;
                half previousLayer = max(currentLayer - layerStep, 0.0h);
                half previousSurfaceDepth = 1.0h
                    - tex2D(_BarkHeightMap, previousUv).r;
                half afterDepth = surfaceDepth - currentLayer;
                half beforeDepth = previousSurfaceDepth - previousLayer;
                half denominator = afterDepth - beforeDepth;
                half interpolation = abs(denominator) > 1.0e-4h
                    ? saturate(afterDepth / denominator)
                    : 0.0h;
                return lerp(currentUv, previousUv, interpolation);
            }

            BarkRecipeSample SampleBarkRecipe(
                float3 barkPosition,
                float3 worldPosition,
                half3 geometricWorldNormal,
                float3 barkAxis)
            {
                float3 tangentFromUp = cross(float3(0.0, 1.0, 0.0), barkAxis);
                float3 tangentFromSide = cross(float3(1.0, 0.0, 0.0), barkAxis);
                float verticalBlend = smoothstep(0.80, 0.98, abs(barkAxis.y));
                float3 tangent = normalize(lerp(
                    tangentFromUp,
                    tangentFromSide,
                    verticalBlend));
                float3 bitangent = normalize(cross(barkAxis, tangent));
                half3 worldAxis = normalize(UnityObjectToWorldDir(barkAxis));
                half3 worldTangent = normalize(UnityObjectToWorldDir(tangent));
                half3 worldBitangent = normalize(UnityObjectToWorldDir(bitangent));

                half tangentFacing = dot(geometricWorldNormal, worldTangent);
                half bitangentFacing = dot(geometricWorldNormal, worldBitangent);
                half tangentSign = tangentFacing < 0.0 ? -1.0 : 1.0;
                half bitangentSign = bitangentFacing < 0.0 ? -1.0 : 1.0;
                float along = dot(barkPosition, barkAxis)
                    / max(_BarkTileHeightMetres, 0.01);

                float3 tangentProjectionU = bitangent * tangentSign;
                float3 bitangentProjectionU = -tangent * bitangentSign;
                float2 tangentUv = float2(
                    dot(barkPosition, tangentProjectionU)
                        / max(_BarkTileWidthMetres, 0.01),
                    along);
                float2 bitangentUv = float2(
                    dot(barkPosition, bitangentProjectionU)
                        / max(_BarkTileWidthMetres, 0.01),
                    along);

                half3 viewDirection = normalize(UnityWorldSpaceViewDir(worldPosition));
                float2 tileSizeMetres = float2(
                    _BarkTileWidthMetres,
                    _BarkTileHeightMetres);
                #if !defined(MOTU_TREE_BARK_NO_PARALLAX)
                    tangentUv = ParallaxBarkUv(
                        tangentUv,
                        tileSizeMetres,
                        float2(
                            dot(viewDirection, worldBitangent * tangentSign),
                            dot(viewDirection, worldAxis)),
                        abs(dot(viewDirection, worldTangent * tangentSign)));
                    bitangentUv = ParallaxBarkUv(
                        bitangentUv,
                        tileSizeMetres,
                        float2(
                            dot(viewDirection, -worldTangent * bitangentSign),
                            dot(viewDirection, worldAxis)),
                        abs(dot(viewDirection, worldBitangent * bitangentSign)));
                #endif

                half tangentWeight = pow(abs(tangentFacing), 4.0);
                half bitangentWeight = pow(abs(bitangentFacing), 4.0);
                half weightTotal = max(tangentWeight + bitangentWeight, 1.0e-4);
                tangentWeight /= weightTotal;
                bitangentWeight /= weightTotal;

                fixed3 tangentAlbedo = tex2D(_BarkAlbedoMap, tangentUv).rgb;
                fixed3 bitangentAlbedo = tex2D(_BarkAlbedoMap, bitangentUv).rgb;
                half tangentOcclusion = tex2D(_BarkOcclusionMap, tangentUv).r;
                half bitangentOcclusion = tex2D(_BarkOcclusionMap, bitangentUv).r;
                half3 tangentNormal = StrengthenRecipeNormal(
                    UnpackNormal(tex2D(_BarkNormalMap, tangentUv)));
                half3 bitangentNormal = StrengthenRecipeNormal(
                    UnpackNormal(tex2D(_BarkNormalMap, bitangentUv)));

                half3 tangentWorldNormal = normalize(
                    worldBitangent * tangentSign * tangentNormal.x
                    + worldAxis * tangentNormal.y
                    + worldTangent * tangentSign * tangentNormal.z);
                half3 bitangentWorldNormal = normalize(
                    -worldTangent * bitangentSign * bitangentNormal.x
                    + worldAxis * bitangentNormal.y
                    + worldBitangent * bitangentSign * bitangentNormal.z);

                BarkRecipeSample sample;
                sample.albedo = tangentAlbedo * tangentWeight
                    + bitangentAlbedo * bitangentWeight;
                sample.worldNormal = normalize(
                    tangentWorldNormal * tangentWeight
                    + bitangentWorldNormal * bitangentWeight);
                sample.occlusion = tangentOcclusion * tangentWeight
                    + bitangentOcclusion * bitangentWeight;
                return sample;
            }

            VertexOutput Vertex(VertexInput input)
            {
                VertexOutput output;
                UNITY_SETUP_INSTANCE_ID(input);
                UNITY_INITIALIZE_OUTPUT(VertexOutput, output);
                UNITY_TRANSFER_INSTANCE_ID(input, output);
                UNITY_INITIALIZE_VERTEX_OUTPUT_STEREO(output);
                float3 surfaceWorldPosition = mul(unity_ObjectToWorld, input.vertex).xyz;
                output.islandLocalPosition = mul(
                    _IslandWorldToLocal,
                    float4(surfaceWorldPosition, 1.0)).xyz;
                float3 windOffset = MotuTreeWindOffset(
                    surfaceWorldPosition,
                    output.islandLocalPosition,
                    input.treeData);
                output.worldPosition = surfaceWorldPosition + windOffset;
                output.pos = UnityWorldToClipPos(output.worldPosition);
                output.worldNormal = MotuTreeWindNormal(
                    UnityObjectToWorldNormal(input.normal),
                    windOffset);
                output.barkAxis = input.barkAxis;
                output.treeData = input.treeData;
                TRANSFER_SHADOW(output);
                UNITY_TRANSFER_FOG(output, output.pos);
                return output;
            }

            fixed4 Fragment(VertexOutput input) : SV_Target
            {
                UNITY_SETUP_INSTANCE_ID(input);
                half3 geometricNormal = normalize(input.worldNormal);
                MotuTreeNoiseSample noise = MotuSampleTreeNoise(
                    input.islandLocalPosition);

                // Rust stores branch axes as X/Y/Z; Unity imports the geometry
                // as X/Z/Y. Reconstruct that same island-local direction here.
                float3 rustAxis = DecodeBarkAxis(input.barkAxis);
                float3 barkAxis = normalize(float3(rustAxis.x, rustAxis.z, rustAxis.y));
                float hasTreeRoot = MotuHasTreeRoot(input.treeData);
                float3 treeRoot = MotuDecodeTreeRoot(input.treeData);
                float3 barkPosition = input.islandLocalPosition - treeRoot * hasTreeRoot;
                BarkRecipeSample bark = SampleBarkRecipe(
                    barkPosition,
                    input.worldPosition,
                    geometricNormal,
                    barkAxis);
                half3 normal = bark.worldNormal;
                half broadVariation = 1.0 + noise.broad.r * _BarkContrast * 0.12;
                fixed3 albedo = MotuRotateTreeHue(
                    saturate(bark.albedo * broadVariation),
                    noise.hue * 0.25);
                half occlusion = lerp(1.0, bark.occlusion, _BarkOcclusionStrength);
                half3 ambient = max(
                    ShadeSH9(half4(normal, 1.0)),
                    half3(_BarkAmbientFloor, _BarkAmbientFloor, _BarkAmbientFloor));
                half3 lightDirection = normalize(UnityWorldSpaceLightDir(input.worldPosition));
                half diffuse = saturate(dot(normal, lightDirection));
                UNITY_LIGHT_ATTENUATION(attenuation, input, input.worldPosition);
                fixed4 result = fixed4(
                    albedo * (ambient * occlusion
                        + _LightColor0.rgb * diffuse * attenuation),
                    1.0);
                UNITY_APPLY_FOG(input.fogCoord, result);
                return result;
            }
            ENDCG
        }

        Pass
        {
            Tags { "LightMode"="ShadowCaster" }
            ZWrite On
            ZTest LEqual

            CGPROGRAM
            #pragma vertex ShadowVertex
            #pragma fragment ShadowFragment
            #pragma target 3.0
            #pragma multi_compile_shadowcaster
            #pragma multi_compile_instancing

            #include "UnityCG.cginc"
            #include "Lighting.cginc"
            #include "TreeSurfaceNoise.cginc"
            #include "TreeWindCommon.cginc"

            struct ShadowInput
            {
                float4 vertex : POSITION;
                float3 normal : NORMAL;
                float4 treeData : COLOR;
                UNITY_VERTEX_INPUT_INSTANCE_ID
            };

            struct ShadowOutput
            {
                V2F_SHADOW_CASTER;
                UNITY_VERTEX_OUTPUT_STEREO
            };

            ShadowOutput ShadowVertex(ShadowInput v)
            {
                ShadowOutput output;
                UNITY_SETUP_INSTANCE_ID(v);
                UNITY_INITIALIZE_VERTEX_OUTPUT_STEREO(output);
                float3 worldPosition = mul(unity_ObjectToWorld, v.vertex).xyz;
                float3 islandLocalPosition = mul(
                    _IslandWorldToLocal,
                    float4(worldPosition, 1.0)).xyz;
                float3 windOffset = MotuTreeWindOffset(
                    worldPosition,
                    islandLocalPosition,
                    v.treeData);
                v.vertex.xyz += mul((float3x3)unity_WorldToObject, windOffset);
                TRANSFER_SHADOW_CASTER_NORMALOFFSET(output)
                return output;
            }

            float4 ShadowFragment(ShadowOutput input) : SV_Target
            {
                SHADOW_CASTER_FRAGMENT(input)
            }
            ENDCG
        }
    }

    FallBack "Diffuse"
}
