#ifndef MOTU_PLANET_FOREST_BARK_INCLUDED
#define MOTU_PLANET_FOREST_BARK_INCLUDED
TEXTURE2D_ARRAY(_ForestBarkAlbedoHeight);
SAMPLER(sampler_ForestBarkAlbedoHeight);
TEXTURE2D_ARRAY(_ForestBarkNormalOcclusion);
SAMPLER(sampler_ForestBarkNormalOcclusion);
float4 _ForestBarkTileSizes;
float4 _ForestPalmBarkTileSize;

float3 ForestBarkAxis(float2 encoded)
{
    float2 unfolded = encoded * 2 - 1;
    float3 axis = float3(unfolded,1-abs(unfolded.x)-abs(unfolded.y));
    if (axis.z < 0) axis.xy = (1-abs(axis.yx))*sign(axis.xy);
    return normalize(axis.xzy);
}

float2 ForestBarkParallax(float2 uv, float2 tile, float2 along, float viewDepth,
    float species, float strength)
{
    if (strength < 0.0001) return uv;
    float2 step = along / tile * (strength / max(viewDepth,0.2)) * 0.125;
    float2 current = uv;
    float layer = 0;
    float depth = 1-SAMPLE_TEXTURE2D_ARRAY(_ForestBarkAlbedoHeight,
        sampler_ForestBarkAlbedoHeight,current,species).a;
    [unroll] for (int i=0;i<8;++i)
    {
        if (layer >= depth) break;
        current -= step;
        layer += 0.125;
        depth = 1-SAMPLE_TEXTURE2D_ARRAY(_ForestBarkAlbedoHeight,
            sampler_ForestBarkAlbedoHeight,current,species).a;
    }
    float2 previous = current+step;
    float previousDepth = 1-SAMPLE_TEXTURE2D_ARRAY(_ForestBarkAlbedoHeight,
        sampler_ForestBarkAlbedoHeight,previous,species).a;
    float after = depth-layer;
    float before = previousDepth-max(layer-0.125,0);
    float denominator = after-before;
    return lerp(current,previous,abs(denominator)>0.0001 ? saturate(after/denominator) : 0);
}

struct ForestBarkSample
{
    half3 colour;
    half3 normal;
    half occlusion;
};

// The original branch-aligned, two-plane bark projection, using shared packed
// arrays for plated broadleaf, scaly pine and ringed palm bark.
ForestBarkSample SampleForestBark(float3 position, float3 world, half3 normal,
    float2 encodedAxis, float speciesTag, bool twig)
{
    float species = speciesTag > 1.5 ? 2 : speciesTag < 0.5 ? 1 : 0;
    float2 tile = species > 1.5 ? _ForestPalmBarkTileSize.xy
        : species > 0.5 ? _ForestBarkTileSizes.zw : _ForestBarkTileSizes.xy;
    float3 axis = twig ? float3(0,1,0) : ForestBarkAxis(encodedAxis);
    float3 tangent = normalize(lerp(cross(float3(0,1,0),axis),cross(float3(1,0,0),axis),
        smoothstep(0.80,0.98,abs(axis.y))));
    float3 bitangent = normalize(cross(axis,tangent));
    float3 wa = normalize(TransformObjectToWorldDir(axis));
    float3 wt = normalize(TransformObjectToWorldDir(tangent));
    float3 wb = normalize(TransformObjectToWorldDir(bitangent));
    float2 facing = float2(dot(normal,wt),dot(normal,wb));
    float2 side = float2(facing.x<0 ? -1 : 1,facing.y<0 ? -1 : 1);
    float along = dot(position,axis)/tile.y;
    float2 first = float2(dot(position,bitangent)*side.x/tile.x,along);
    float2 second = float2(-dot(position,tangent)*side.y/tile.x,along);
    float3 view = GetWorldSpaceNormalizeViewDir(world);
    float close = 1-smoothstep(8,20,distance(world,_MotuForestViewer));
    float parallax = twig ? 0 : close*0.04;
    first = ForestBarkParallax(first,tile,float2(dot(view,wb)*side.x,dot(view,wa)),
        abs(dot(view,wt)),species,parallax);
    second = ForestBarkParallax(second,tile,float2(-dot(view,wt)*side.y,dot(view,wa)),
        abs(dot(view,wb)),species,parallax);
    float2 weight = pow(abs(facing),4);
    weight = weight.x+weight.y > 0.0001 ? weight/(weight.x+weight.y) : float2(0.5,0.5);
    half4 colourA = SAMPLE_TEXTURE2D_ARRAY(_ForestBarkAlbedoHeight,sampler_ForestBarkAlbedoHeight,first,species);
    half4 colourB = SAMPLE_TEXTURE2D_ARRAY(_ForestBarkAlbedoHeight,sampler_ForestBarkAlbedoHeight,second,species);
    half4 reliefA = SAMPLE_TEXTURE2D_ARRAY(_ForestBarkNormalOcclusion,sampler_ForestBarkNormalOcclusion,first,species);
    half4 reliefB = SAMPLE_TEXTURE2D_ARRAY(_ForestBarkNormalOcclusion,sampler_ForestBarkNormalOcclusion,second,species);
    half3 na = normalize((reliefA.rgb*2-1)*half3(0.7,0.7,1));
    half3 nb = normalize((reliefB.rgb*2-1)*half3(0.7,0.7,1));
    half3 mapped = (wb*side.x*na.x+wa*na.y+wt*side.x*na.z)*weight.x
        +(-wt*side.y*nb.x+wa*nb.y+wb*side.y*nb.z)*weight.y
        +wa*dot(normal,wa);
    ForestBarkSample sample;
    sample.colour = colourA.rgb*weight.x+colourB.rgb*weight.y;
#ifdef UNITY_COLORSPACE_GAMMA
    sample.colour = SRGBToLinear(sample.colour);
#endif
    sample.normal = twig || dot(mapped,mapped)<0.0001 ? normal : normalize(mapped);
    sample.occlusion = lerp(1,reliefA.a*weight.x+reliefB.a*weight.y,0.7);
    return sample;
}
#endif
