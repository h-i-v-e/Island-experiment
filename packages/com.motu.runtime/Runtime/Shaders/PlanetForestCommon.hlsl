#ifndef MOTU_PLANET_FOREST_INCLUDED
#define MOTU_PLANET_FOREST_INCLUDED
#include "MotuUrp.hlsl"
#include "WeatherWindCommon.cginc"
sampler3D _ForestNoise;
sampler2D _TwigCard;
sampler2D _PalmFrond;
float4 _PalmLodDistances;
float _PalmLodEnabled;
float4 _TwigCard_ST;
float _ForestKind;
float3 _MotuForestViewer;
#include "PlanetForestBark.hlsl"

// Preserve procedural detail when the archipelago recentres its floating origin.
float3 ForestStable(float3 p) { return MotuCloudWorldToLocal(p); }
float3 ForestNoise(float3 p) { return tex3Dlod(_ForestNoise, float4(p, 0)).rgb; }
float3 ForestRoot() { return TransformObjectToWorld(float3(0,0,0)); }
float3 ForestUp() { return normalize(mul((float3x3)GetObjectToWorldMatrix(), float3(0,1,0))); }
half4 ForestTwigSample(float2 uv) { return tex2D(_TwigCard, uv * _TwigCard_ST.xy + _TwigCard_ST.zw); }
half3 ForestFoliageColour(half3 broad, half3 detail, float4 data)
{
    // Palm leaflets carry their silhouette in geometry, with restrained warm green variation.
    if (data.b > 1.5)
        return lerp(half3(0.028,0.092,0.012),half3(0.055,0.155,0.023),data.r) * lerp(0.97,1.03,detail.g);
    half light = saturate(0.25+broad.g*0.55+detail.r*0.20);
    half3 colour = data.b > 0.5
        ? lerp(half3(0.008,0.038,0.006),half3(0.064,0.196,0.022),light)
        : lerp(half3(0.005,0.026,0.014),half3(0.035,0.115,0.055),light);
    return colour * lerp(0.88,1.12,data.r);
}
float3 ForestWind(float3 worldPoint, float3 root)
{
    float3 up = ForestUp();
    float height = max(dot(worldPoint - root, up), 0);
    float3 wind = float3(MotuWindDirection().x, 0, MotuWindDirection().y);
    wind = normalize(wind - up * dot(wind, up) + 0.00001);
    float strength = min(height * height / 144.0, 1.0);
    float phase = dot(ForestStable(root).xzy, float3(0.031,0.027,0.043)) + _Time.y * 1.3;
    float speed = _MotuWeatherWind.z > 0 ? clamp(_MotuWeatherWind.z / 10.0, 0, 3) : 1;
    return wind * (sin(phase)*0.12 + sin(phase*2.37)*0.035) * strength * speed;
}
float3 PineHash(int2 cell)
{
    uint2 key = asuint(cell);
    uint h = key.x * 0x9e3779b9u ^ key.y * 0x85ebca6bu;
    h = (h ^ (h >> 16u)) * 0x7feb352du;
    h = (h ^ (h >> 15u)) * 0x846ca68bu;
    h ^= h >> 16u;
    return float3(h & 0xffffu, (h >> 16u) & 0xffffu, (h * 0x9e3779b9u) & 0xffffu) / 65535.0;
}
float PineStrands(float2 uv, float layer)
{
    float2 flow = float2(sin(dot(uv,float2(3.1,1.7))+sin(uv.y*2.3)), sin(dot(uv,float2(-1.3,2.7))+cos(uv.x*4.1)));
    float2 position = uv * float2(70,20) + flow * 0.65 + float2(0, layer*0.45);
    float cluster = 0.5 + 0.25*(sin(uv.x*2.7+sin(uv.y*3.3))+cos(uv.y*1.7+uv.x*0.8));
    int2 cell = (int2)floor(position);
    float edge = max(length(fwidth(position))*0.45,0.012);
    int2 direction = int2(frac(position.x) >= 0.5 ? 1 : -1, frac(position.y) >= 0.5 ? 1 : -1);
    float mask = 0;
    [unroll] for (int y=0;y<=1;++y) [unroll] for (int x=0;x<=1;++x)
    {
        int2 neighbour = cell + int2(x,y)*direction;
        float3 random = PineHash(neighbour);
        float height = 0.30 + random.z * lerp(0.30,0.70,cluster);
        float2 centre = (float2)neighbour + 0.1 + random.xy*0.8 + (random.xy-0.5)*0.20*layer;
        float radius = 0.33*pow(max(1.0-layer/(height+0.08),0),0.55)+0.025;
        float strand = (1-smoothstep(radius,radius+edge,length(position-centre))) * (1-smoothstep(height,height+0.04,layer));
        mask = max(mask,strand);
    }
    return mask;
}
void PalmClip(float3 root, float2 uv, float4 data, float2 pixel)
{
    bool foliage = _ForestKind > 0.5 && (_ForestKind < 2.5 || data.g > 0.5);
    if (foliage && data.a > 0.5) clip(tex2D(_PalmFrond,uv).a - 0.5);
    if (_PalmLodEnabled < 0.5) return;
    float distanceToRoot = distance(root,_MotuForestViewer);
    float fineToMiddle = smoothstep(_PalmLodDistances.x,_PalmLodDistances.y,distanceToRoot);
    float far = smoothstep(_PalmLodDistances.z,_PalmLodDistances.w,distanceToRoot);
    float fine = (1-fineToMiddle)*(1-far);
    // Partition the same pixel in each stage. Different mesh depths must not
    // choose unrelated masks, which would punch holes through overlapping trunks.
    uint2 cell = (uint2)floor(pixel);
    uint hash = cell.x*0x9e3779b9u ^ cell.y*0x85ebca6bu;
    hash = (hash ^ (hash >> 16u))*0x7feb352du;
    hash = (hash ^ (hash >> 15u))*0x846ca68bu;
    hash ^= hash >> 16u;
    float threshold = (hash & 0x00ffffffu)/16777216.0;
    if (data.a < 0.5) { if (threshold >= fine) clip(-1); }
    else if (data.a < 1.5) { if (threshold < fine || threshold >= 1-far) clip(-1); }
    else if (threshold < 1-far) clip(-1);
}
void ForestClip(float3 worldPoint, float3 root, float3 up, float2 uv, float4 data, float2 pixel)
{
    if (data.b > 1.5) { PalmClip(root,uv,data,pixel); return; }
    if (_ForestKind > 3.5)
    {
        // One cutout is shared by both tree species and every rendering pass.
        clip(ForestTwigSample(uv).a - 0.5);
        float fade = 1-smoothstep(25.0,30.0,distance(root,_MotuForestViewer));
        float3 cell = floor(ForestStable(worldPoint).xzy*128.0);
        float threshold = frac(sin(dot(cell,float3(12.9898,78.233,37.719)))*43758.5453);
        clip(fade-threshold);
        return;
    }
    if (_ForestKind < 0.5 || _ForestKind > 2.5) return;
    if (data.b > 0.5)
    {
        float3 column = (ForestStable(worldPoint) - up * dot(worldPoint-root,up)).xzy;
        float3 broad = ForestNoise(column/32.0 + float3(0.13,0.47,0.71));
        float3 detail = ForestNoise(column/13.0 + 0.37);
        clip(smoothstep(0.30,0.38,broad.r*0.65+detail.b*0.35)-0.5);
        if (_ForestKind > 1.5)
        {
            clip(28.0-distance(worldPoint,_MotuForestViewer));
            float3 p = ForestStable(worldPoint).xzy/1.25;
            float3 leaves = ForestNoise(p+float3(0.41,0.17,0.73));
            float3 fine = ForestNoise(p*1.85+float3(0.13,0.79,0.37));
            clip(dot(leaves.rg,float2(0.50,0.25))+fine.b*0.25-lerp(0.28,0.78,pow(data.r,1.25)));
        }
    }
    else if (_ForestKind > 1.5)
    {
        clip(PineStrands(uv,data.r)-0.44);
        float fade = 1-smoothstep(14.0,28.0,distance(worldPoint,_MotuForestViewer));
        float3 cell = floor(ForestStable(worldPoint).xzy*128.0);
        float threshold = frac(sin(dot(cell,float3(12.9898,78.233,37.719)))*43758.5453);
        clip(fade-threshold);
    }
}
struct ForestInput
{
    float3 position : POSITION;
    float3 normal : NORMAL;
    float2 uv : TEXCOORD0;
    float4 data : COLOR;
    UNITY_VERTEX_INPUT_INSTANCE_ID
};
struct ForestOutput
{
    float4 position : SV_POSITION;
    float3 world : TEXCOORD0;
    half3 normal : TEXCOORD1;
    float2 uv : TEXCOORD2;
    float4 data : TEXCOORD3;
    float fog : TEXCOORD4;
    float3 barkPosition : TEXCOORD5;
    UNITY_VERTEX_INPUT_INSTANCE_ID
    UNITY_VERTEX_OUTPUT_STEREO
};
ForestOutput ForestVertex(ForestInput input)
{
    ForestOutput output = (ForestOutput)0;
    UNITY_SETUP_INSTANCE_ID(input);
    UNITY_TRANSFER_INSTANCE_ID(input,output);
    UNITY_INITIALIZE_VERTEX_OUTPUT_STEREO(output);
    float3 root = ForestRoot();
    output.world = TransformObjectToWorld(input.position);
    output.world += ForestWind(output.world,root);
    output.normal = TransformObjectToWorldNormal(input.normal);
#ifdef FOREST_SHADOW
    output.position = MotuShadowPosition(output.world,output.normal);
#else
    output.position = TransformWorldToHClip(output.world);
#endif
    output.fog = ComputeFogFactor(output.position.z);
    output.uv = input.uv;
    output.data = input.data;
    // Bark dimensions stay in metres when a shared prototype is scaled.
    output.barkPosition = input.position * length(mul((float3x3)GetObjectToWorldMatrix(),float3(0,1,0)));
    return output;
}
half4 ForestFragment(ForestOutput input, bool front : SV_IsFrontFace) : SV_Target
{
    UNITY_SETUP_INSTANCE_ID(input);
    float3 root = ForestRoot();
    float3 up = ForestUp();
    ForestClip(input.world,root,up,input.uv,input.data,input.position.xy);
#if defined(FOREST_SHADOW) || defined(FOREST_DEPTH)
    return 0;
#else
    float3 p = ForestStable(input.world).xzy;
    half3 broad = ForestNoise(p/24.0);
    half3 detail = ForestNoise(p/4.8+0.173);
    half3 normal = normalize(input.normal) * (front ? 1 : -1);
    half3 colour;
    half transmission = 0;
    half occlusion = 1;
    bool twig = _ForestKind > 3.5;
    bool wood = twig || _ForestKind < 0.5 || (_ForestKind > 2.5 && input.data.g < 0.5);
    if (wood)
    {
        ForestBarkSample bark = SampleForestBark(input.barkPosition,input.world,normal,input.uv,input.data.b,twig);
        colour = bark.colour*lerp(0.96,1.04,broad.r);
        normal = bark.normal;
        occlusion = bark.occlusion;
        if (twig)
        {
            // White twigs and green leaves share one alpha cutout. Use chroma
            // only as a material mask, so each species keeps its crown palette.
            half3 fan = ForestTwigSample(input.uv).rgb;
            half leaf = smoothstep(0.03,0.10,fan.g-max(fan.r,fan.b));
            colour = lerp(colour,ForestFoliageColour(broad,detail,float4(0.5,1,input.data.b,1)),leaf);
            occlusion = lerp(occlusion,1,leaf);
            transmission = input.data.b > 0.5 && input.data.b < 1.5 ? 0.22*leaf : 0;
        }
    }
    else if (_ForestKind > 1.5 && _ForestKind < 2.5 && input.data.b < 0.5)
    {
        colour = lerp(half3(0.008,0.035,0.017),half3(0.025,0.075,0.029),input.data.r)*lerp(0.88,1.08,detail.g);
        normal = normalize(normal+(detail-0.5)*0.2);
    }
    else
    {
        colour = ForestFoliageColour(broad,detail,input.data);
        if (input.data.b < 0.5)
        {
            float close = 1-smoothstep(12.0,30.0,distance(input.world,_MotuForestViewer));
            if (_ForestKind < 1.5 && close > 0.001)
            {
                float3 relative = input.world-root;
                float3x3 transform = (float3x3)GetObjectToWorldMatrix();
                float3 local = float3(dot(relative,normalize(mul(transform,float3(1,0,0)))),
                    dot(relative,normalize(mul(transform,float3(0,0,1)))),dot(relative,up));
                half3 needles = ForestNoise(local*float3(12,12,3)+0.37);
                colour *= lerp(1,lerp(0.65,1.35,needles.r),close);
                normal = normalize(normal+(needles-0.5)*0.30*close);
            }
        }
        if (input.data.b < 1.5) normal = normalize(normal+(detail-0.5)*0.24);
        transmission = input.data.b > 0.5 && input.data.b < 1.5 ? 0.22 : 0;
    }
    MotuCloudLighting cloud = MotuCloudSurfaceLighting(input.world);
    // A pine crown is an opaque volume. Its exposed inner face uses its own
    // downward-facing mesh normal, without needle normal noise turning it
    // towards the sky or thin-leaf transmission lighting it from behind.
    if (!wood && (input.data.b < 0.5 || input.data.b > 1.5) && !front)
        normal = -normalize(input.normal);
    InputData lighting = (InputData)0;
    lighting.positionWS = input.world;
    lighting.normalWS = normal;
    lighting.viewDirectionWS = GetWorldSpaceNormalizeViewDir(input.world);
    lighting.shadowCoord = TransformWorldToShadowCoord(input.world);
    lighting.bakedGI = SampleSH(normal) * cloud.ambientTransmittance;
    lighting.normalizedScreenSpaceUV = GetNormalizedScreenSpaceUV(input.position);
    lighting.shadowMask = half4(1,1,1,1);
    SurfaceData surface = (SurfaceData)0;
    // Planet constants are linear albedos. URP expects working-space inputs;
    // preserve their appearance in the original Island Gamma project too.
#ifdef UNITY_COLORSPACE_GAMMA
    colour = LinearToSRGB(colour);
#endif
    surface.albedo = colour;
    surface.specular = 0.04;
    surface.smoothness = 0.06; // Planet roughness 0.94.
    surface.normalTS = half3(0,0,1);
    surface.occlusion = occlusion;
    surface.alpha = 1;
    half4 result = UniversalFragmentPBR(lighting,surface);
    result.rgb *= cloud.directTransmittance;
    Light sun = GetMainLight(lighting.shadowCoord,input.world,lighting.shadowMask);
    result.rgb += colour * sun.color * sun.distanceAttenuation * sun.shadowAttenuation
        * saturate(dot(-normal,sun.direction)) * transmission * cloud.directTransmittance;
    result.rgb = MixFog(result.rgb,input.fog);
    return result;
#endif
}
#endif
