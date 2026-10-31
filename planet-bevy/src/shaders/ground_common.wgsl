#define_import_path planet_bevy::ground

#import planet_bevy::radial::radial_mapping
#import planet_bevy::weather::weather_snow_line

// Ported from Motu/Terrain Unified, TerrainCoverageCommon and TerrainRockCommon.
// Radial elevation and up replace Unity's Y height and Y-up; all lengths are metres.
struct TerrainSettings {
    radius: f32, snow_line: f32, detail: f32, padding: f32,
    colour: vec4<f32>,
}
@group(#{MATERIAL_BIND_GROUP}) @binding(105) var<uniform> ground: TerrainSettings;
@group(#{MATERIAL_BIND_GROUP}) @binding(106) var ground_albedo: texture_2d_array<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(107) var ground_normal: texture_2d_array<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(108) var ground_mask: texture_2d_array<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(109) var ground_sampler: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(110) var cliff_noise: texture_3d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(111) var cliff_sampler: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(128) var forest_mask: texture_2d<f32>;

struct DetailMapping { origin: vec4<f32>, tangent: vec4<f32>, bitangent: vec4<f32> }
@group(#{MATERIAL_BIND_GROUP}) @binding(112) var<storage, read> detail_mapping: DetailMapping;
@group(#{MATERIAL_BIND_GROUP}) @binding(113) var detail_mask: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(114) var detail_sampler: sampler;
fn detail_environment(p: vec3<f32>) -> vec2<f32> {
    if detail_mapping.origin.w <= 0.0 { return vec2<f32>(0.0); }
    let offset = p - detail_mapping.origin.xyz;
    let uv = vec2<f32>(dot(offset, detail_mapping.tangent.xyz), dot(offset, detail_mapping.bitangent.xyz)) / detail_mapping.origin.w + 0.5;
    if any(uv < vec2<f32>(0.0)) || any(uv > vec2<f32>(1.0)) { return vec2<f32>(0.0); }
    return textureSampleLevel(detail_mask, detail_sampler, uv, 0.0).rg;
}

const LAYER_SIZE = array<f32, 6>(2.0, 2.0, 4.0, 2.0, 3.0, 2.0);
const PARALLAX = array<f32, 6>(0.02, 0.018, 0.05, 0.025, 0.015, 0.025);
const NEUTRAL = array<f32, 6>(0.5, 0.428571, 0.5, 0.0, 0.5, 0.304348);
const AO_STRENGTH = array<f32, 6>(0.55, 0.7, 0.5, 0.65, 0.45, 0.75);
const DIRT = vec3<f32>(0.008540, 0.004401, 0.002012);
const ROCK = vec3<f32>(0.073239, 0.083535, 0.068478);
const SAND = vec3<f32>(0.342392, 0.284647, 0.094630);
const GRASS_A = vec3<f32>(0.027212, 0.178868, 0.017390);
const GRASS_B = vec3<f32>(0.094630, 0.214041, 0.017390);
const SNOW = vec3<f32>(0.638283, 0.673860, 0.620916);

struct GroundNoise { broad: vec3<f32>, large: vec3<f32>, grain: vec3<f32>, detail: vec3<f32> }
struct Coverage {
    noise: GroundNoise, rock: f32, river: f32, beach: f32,
    forest: f32, stones: f32, grass: f32, snow: f32, coastal: f32,
}
struct GroundSurface { colour: vec3<f32>, normal: vec3<f32>, occlusion: f32, wet: f32 }
struct Projection {
    footprint: f32,
}

fn volume_noise(p: vec3<f32>) -> vec3<f32> {
    return textureSample(cliff_noise, cliff_sampler, p).rgb;
}
fn ground_noise(p: vec3<f32>) -> GroundNoise {
    let q = p / 160.0;
    return GroundNoise(volume_noise(q) * 2.0 - 1.0,
        volume_noise(q / 3.0 + vec3<f32>(0.23, 0.71, 0.41)) * 2.0 - 1.0,
        volume_noise(q * 8.0 + vec3<f32>(0.67, 0.31, 0.91)),
        volume_noise(q * 32.0 + vec3<f32>(0.11, 0.83, 0.47)) * 2.0 - 1.0);
}
fn antialiased_mask(distance: f32) -> f32 {
    let width = max(fwidth(distance), 0.0001);
    return smoothstep(-width, width, distance);
}
fn coherent_mask(source: f32, noise: f32, strength: f32, width: f32) -> f32 {
    let distance = clamp(source, 0.0, 1.0) - (0.5 + noise * strength);
    let transition = max(max(width, fwidth(distance)), 0.0001);
    return smoothstep(-transition, transition, distance);
}

fn build_coverage(p: vec3<f32>, up_dot: f32, elevation: f32, material: vec4<f32>) -> Coverage {
    var c: Coverage;
    c.noise = ground_noise(p);
    let slope = 1.0 - clamp(up_dot, 0.0, 1.0);
    let hardness = clamp(material.r, 0.0, 1.0);
    let cover = clamp(material.g, 0.0, 1.0);
    let cliff_variation = clamp(c.noise.broad.r * 0.65 + c.noise.large.g * 0.35, -1.0, 1.0);
    var cliff = antialiased_mask(0.55 - (up_dot - cliff_variation * 0.30));
    let forced_noise = clamp((c.noise.grain.b * 2.0 - 1.0) * 0.35 + c.noise.detail.r * 0.65, -1.0, 1.0);
    let forced = smoothstep(clamp(0.8 + forced_noise * 0.15, 0.0, 1.0), 1.0, hardness);
    let geology = clamp(slope * mix(1.3, 3.0, hardness), 0.0, 1.0);
    let geology_distance = dot(c.noise.grain.rg, vec2<f32>(0.65, 0.35)) - (1.0 - geology);
    let geology_width = max(0.20, fwidth(geology_distance));
    let geology_rock = smoothstep(-geology_width, geology_width, geology_distance) * smoothstep(0.0, 0.20, geology);
    c.river = coherent_mask(material.b, clamp(dot(c.noise.broad, vec3<f32>(0.577, -0.577, 0.577)), -1.0, 1.0), 0.20, 0.20);
    let sand_richness = cover * material.a * (1.0 - smoothstep(2.0, 4.0, elevation)) * (1.0 - c.river);
    // A 3D periodic field wraps the same patch rules around every hemisphere.
    let patches = volume_noise(p / 32.0);
    let sand_patch = dot(patches.rg, vec2<f32>(0.40, 0.60));
    let beach_noise = clamp(c.noise.detail.g * 0.65 + c.noise.detail.b * 0.35, -1.0, 1.0);
    let beach_distance = sand_patch - (1.0 - sand_richness) + beach_noise * 0.18;
    let beach_width = max(0.18, fwidth(beach_distance));
    c.beach = smoothstep(-beach_width, beach_width, beach_distance) * step(0.0001, sand_richness);
    c.beach = max(c.beach, step(elevation, 0.0) * material.a * (1.0 - c.river));
    let boundary = clamp(c.noise.broad.b * 0.55 + c.noise.large.b * 0.45, -1.0, 1.0);
    c.rock = max(max(geology_rock, forced) * (1.0 - cover),
        max(cliff, c.beach * antialiased_mask(slope - (0.10 + boundary * 0.18 * 0.25))));
    let direction=normalize(p);
    let forest_uv=vec2<f32>(atan2(direction.z,direction.x)/6.2831853+0.5,
        clamp(asin(clamp(direction.y,-1.0,1.0))/3.14159265+0.5,0.0009765625,0.9990234375));
    c.forest=textureSample(forest_mask,cliff_sampler,forest_uv).r
        *smoothstep(0.90,0.98,up_dot)*(1.0-c.river)*(1.0-c.beach);
    let stone_noise = clamp(c.noise.detail.r * 0.45 + c.noise.detail.b * 0.55, -1.0, 1.0);
    c.stones = coherent_mask(detail_environment(p).r, stone_noise, 0.22, 0.16) * step(0.0, elevation);
    let grass_distance = dot(patches.rg, vec2<f32>(0.65, 0.35)) - (1.0 - cover);
    let grass_width = max(0.18, fwidth(grass_distance));
    c.grass = smoothstep(-grass_width, grass_width, grass_distance) * smoothstep(0.0, 0.20, cover) * step(0.0, elevation);
    let snow_line = weather_snow_line(p, ground.snow_line);
    c.snow = antialiased_mask(elevation - (snow_line + c.noise.large.r * 18.0 + c.noise.broad.g * 2.5)) * (1.0 - cliff) * step(0.0, elevation);
    c.forest*=1.0-c.snow;
    c.coastal = clamp(c.noise.detail.g * 0.65 + (c.noise.grain.r * 2.0 - 1.0) * 0.35, -1.0, 1.0);
    // Unity's 37–45 degree shoulder converges to exclusive procedural stone.
    let progress = 1.0 - smoothstep(cos(radians(45.0)), cos(radians(37.0)), clamp(up_dot, 0.0, 1.0));
    let steep = smoothstep(0.0, 1.0, clamp(progress + cliff_variation * 0.30 * 4.0 * progress * (1.0 - progress), 0.0, 1.0));
    c.rock = max(c.rock, steep);
    c.river *= 1.0 - steep; c.beach *= 1.0 - steep;
    c.forest *= 1.0 - steep; c.stones *= 1.0 - steep;
    c.grass *= 1.0 - steep; c.snow *= 1.0 - steep;
    return c;
}

fn ground_projection(p: vec3<f32>, footprint: f32) -> Projection {
    // The maximum local stretch bounds both axes, including at the fold. A
    // mirrored seam must not choose a blurry mip from a longitude discontinuity.
    return Projection(footprint * radial_mapping(p, ground.radius).stretch);
}
fn layer_sample(tex: texture_2d_array<f32>, p: vec3<f32>, layer: i32, projection: Projection) -> vec4<f32> {
    let size = LAYER_SIZE[layer];
    let uv = radial_mapping(p, ground.radius).uv / size;
    let lod = max(log2(max(projection.footprint * 512.0 / size, 1.0)), 0.0);
    return textureSampleLevel(tex, ground_sampler, uv, layer, lod);
}
fn layer_gradient(p: vec3<f32>, layer: i32, projection: Projection) -> vec3<f32> {
    let mapping = radial_mapping(p, ground.radius);
    let size = LAYER_SIZE[layer];
    let lod = max(log2(max(projection.footprint * 512.0 / size, 1.0)), 0.0);
    let detail = textureSampleLevel(ground_normal, ground_sampler, mapping.uv / size, layer, lod).xyz * 2.0 - 1.0;
    return (mapping.du * detail.x + mapping.dv * detail.y) / max(detail.z, 0.2);
}
fn top_texture_weight(up: f32, noise: f32) -> f32 {
    let gate = smoothstep(cos(radians(45.0)), cos(radians(37.0)), up);
    let preference = smoothstep(cos(radians(37.0)), 1.0, up);
    return min(gate, smoothstep(-0.22, 0.22, noise + mix(-0.10, 0.25, preference)));
}
fn height_weights(candidates: array<f32, 6>, heights: array<f32, 6>) -> array<f32, 6> {
    var scores: array<f32, 6>; var peak = -1.0;
    for (var i = 0; i < 6; i += 1) {
        let influence = select(1.0, 0.65, i >= 4);
        scores[i] = candidates[i] + (heights[i] * 2.0 - 1.0) * influence * 4.0 * candidates[i] * (1.0 - candidates[i]) * 0.5;
        peak = max(peak, scores[i]);
    }
    var result: array<f32, 6>; var total = 0.0;
    for (var i = 0; i < 6; i += 1) {
        result[i] = candidates[i] * clamp((scores[i] - peak + 0.18) / 0.18, 0.0, 1.0);
        total += result[i];
    }
    if total < 0.0001 { return array<f32, 6>(1.0, 0.0, 0.0, 0.0, 0.0, 0.0); }
    for (var i = 0; i < 6; i += 1) { result[i] /= total; }
    return result;
}
fn blended_depth(p: vec3<f32>, projection: Projection, weights: array<f32, 6>, gates: array<f32, 6>) -> f32 {
    var depth = 0.0;
    for (var i = 0; i < 6; i += 1) {
        if weights[i] > 0.001 {
            var h = NEUTRAL[i];
            if gates[i] > 0.001 { h = mix(h, layer_sample(ground_mask, p, i, projection).r, gates[i]); }
            depth += weights[i] * PARALLAX[i] * (1.0 - h);
        }
    }
    return depth;
}
fn parallax_position(p: vec3<f32>, normal: vec3<f32>, view: vec3<f32>, projection: Projection, weights: array<f32, 6>, gates: array<f32, 6>, fade: f32) -> vec3<f32> {
    var maximum = 0.0; var neutral = 0.0;
    for (var i = 0; i < 6; i += 1) {
        maximum += weights[i] * PARALLAX[i];
        neutral += weights[i] * PARALLAX[i] * (1.0 - NEUTRAL[i]);
    }
    if maximum < 0.0001 || fade < 0.001 { return p; }
    let ray = (view - normal * dot(normal, view)) / max(abs(dot(normal, view)), 0.2);
    let start = p + ray * neutral;
    let step_depth = maximum * 0.125;
    var depth = 0.0;
    var surface = blended_depth(start, projection, weights, gates);
    var previous_depth = 0.0; var previous_surface = surface;
    for (var i = 0; i < 8; i += 1) {
        if depth >= surface { break; }
        previous_depth = depth; previous_surface = surface;
        depth += step_depth;
        surface = blended_depth(start - ray * depth, projection, weights, gates);
    }
    let above = previous_surface - previous_depth;
    let below = depth - surface;
    let intersection = clamp(above / max(above + below, 0.00001), 0.0, 1.0);
    let hit = mix(previous_depth, depth, intersection);
    return p + ray * (neutral - hit) * fade;
}
fn perturb(normal: vec3<f32>, detail: vec3<f32>, strength: f32) -> vec3<f32> {
    return normalize(normal + (detail - normal * dot(detail, normal)) * strength);
}

fn shade_ground(p: vec3<f32>, normal: vec3<f32>, view: vec3<f32>, material: vec4<f32>, footprint: f32, distance: f32) -> GroundSurface {
    let up = normalize(p);
    let elevation = length(p) - ground.radius;
    let up_dot = clamp(dot(normal, up), 0.0, 1.0);
    let c = build_coverage(p, up_dot, elevation, material);
    let n = c.noise;
    let projection = ground_projection(p, footprint);
    let strongest = max(max(max(c.rock, c.river), max(c.forest, c.stones)), c.beach);
    let candidates = array<f32, 6>(clamp(1.0 - strongest, 0.0, 1.0), c.forest, c.rock, c.river, c.beach, c.stones);
    let overlay = 1.0 - smoothstep(0.01, 0.20, max(c.river, c.stones));
    let patch_noise = array<f32, 6>(
        n.large.g * 0.20 + n.broad.b * 0.25 + (n.grain.r * 2.0 - 1.0) * 0.55,
        n.large.g * 0.20 + n.broad.r * 0.25 + n.detail.g * 0.55,
        n.large.r * 0.20 + n.broad.g * 0.25 + (n.grain.g * 2.0 - 1.0) * 0.55,
        n.large.b * 0.20 + n.broad.b * 0.25 + n.detail.b * 0.55,
        n.large.r * 0.20 + n.broad.b * 0.25 + n.detail.r * 0.55,
        n.large.b * 0.20 + n.broad.r * 0.25 + (n.grain.b * 2.0 - 1.0) * 0.55);
    var gates: array<f32, 6>; var heights: array<f32, 6>;
    for (var i = 0; i < 6; i += 1) {
        gates[i] = top_texture_weight(up_dot, clamp(patch_noise[i], -1.0, 1.0));
        if i != 3 && i != 5 { gates[i] *= overlay; }
        heights[i] = 0.5;
        if candidates[i] > 0.001 {
            let h = layer_sample(ground_mask, p, i, projection).r;
            heights[i] = mix(0.5, h, select(gates[i], 1.0, i == 3 || i == 5));
        }
    }
    let weights = height_weights(candidates, heights);
    let near_fade = (1.0 - smoothstep(8.0, 24.0, distance)) * ground.detail;
    let detail_fade = (1.0 - smoothstep(0.03, 0.4, footprint)) * ground.detail;
    let displaced = parallax_position(p, normal, view, projection, weights, gates, near_fade);
    let beach_variation = clamp(n.detail.r * 0.65 + n.large.b * 0.35, -1.0, 1.0);
    let simple_beach = SAND * (1.0 + beach_variation * 0.08);
    let underlying = height_weights(array<f32, 6>(clamp(1.0 - max(c.rock, c.beach), 0.0, 1.0), 0.0, c.rock, 0.0, c.beach, 0.0), array<f32, 6>(0.5, 0.5, 0.5, 0.5, 0.5, 0.5));
    let underlying_colour = DIRT * underlying[0] + ROCK * underlying[2] + simple_beach * underlying[4];
    let q = p / 160.0;
    let fine_dirt = volume_noise(q * 96.0 + vec3<f32>(0.79, 0.17, 0.53)) * 2.0 - 1.0;
    let fine_rock = n.broad * 0.45 + (volume_noise(q * 16.0 + vec3<f32>(0.37, 0.61, 0.83)) * 2.0 - 1.0) * 0.55;
    let fine_sand = volume_noise(q * 768.0 + vec3<f32>(0.79, 0.17, 0.53)) * 2.0 - 1.0;
    let simple_normals = array<vec3<f32>, 6>(perturb(normal, fine_dirt, 0.05 * detail_fade), normal,
        perturb(normal, fine_rock, 0.12 * detail_fade), normal,
        perturb(normal, fine_sand, 0.10 * detail_fade), normal);
    let underlying_normal = normalize(simple_normals[0] * underlying[0] + simple_normals[2] * underlying[2] + simple_normals[4] * underlying[4]);
    let simple_colours = array<vec3<f32>, 6>(DIRT, underlying_colour, ROCK, underlying_colour, simple_beach, underlying_colour);
    var result = GroundSurface(vec3<f32>(0.0), vec3<f32>(0.0), 0.0, 0.0);
    for (var i = 0; i < 6; i += 1) {
        if weights[i] > 0.001 {
            var colour = simple_colours[i];
            var layer_normal = select(simple_normals[i], underlying_normal, i == 1 || i == 3 || i == 5);
            var ao = 1.0;
            if gates[i] > 0.001 {
                colour = mix(colour, layer_sample(ground_albedo, displaced, i, projection).rgb, gates[i]);
                let grad = layer_gradient(displaced, i, projection);
                let recipe_normal = normalize(normal + grad - normal * dot(grad, normal));
                layer_normal = normalize(mix(layer_normal, recipe_normal, gates[i]));
                ao = mix(1.0, layer_sample(ground_mask, displaced, i, projection).g, gates[i] * AO_STRENGTH[i]);
            }
            result.colour += colour * weights[i]; result.normal += layer_normal * weights[i]; result.occlusion += ao * weights[i];
        }
    }
    let grass_coverage = smoothstep(0.35, 0.65, c.grass) * weights[0] * (1.0 - c.snow);
    let grass_noise = volume_noise(p / 4096.0).b;
    result.colour = mix(result.colour, mix(GRASS_A, GRASS_B, smoothstep(0.1, 0.9, grass_noise)), grass_coverage);
    result.normal = normalize(mix(normalize(result.normal), perturb(normal, fine_dirt, 0.22 * detail_fade), grass_coverage));
    result.colour = mix(result.colour, SNOW, c.snow);
    let snow_detail = volume_noise(q * 96.0 + vec3<f32>(0.19, 0.89, 0.43)) * 2.0 - 1.0;
    result.normal = normalize(mix(result.normal, perturb(normal, snow_detail, 0.08 * detail_fade), c.snow));
    let coast_source = 1.0 - smoothstep(-0.05, 0.15, elevation);
    let coastal_distance = coast_source - (0.5 + c.coastal * 0.18);
    let coastal_width = max(0.06, fwidth(coastal_distance));
    let coastal_wet = smoothstep(-coastal_width, coastal_width, coastal_distance);
    let bank_wet = pow(max(clamp(material.b, 0.0, 1.0), c.river), 0.45);
    let wet = max(bank_wet, coastal_wet) * step(0.0, elevation) * (1.0 - max(grass_coverage, c.snow));
    result.wet = wet * (1.0 - weights[3]);
    result.colour *= 1.0 - max(wet, step(elevation, 0.0)) * 0.24;
    return result;
}
