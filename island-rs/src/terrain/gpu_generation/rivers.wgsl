struct Params {
    grid: vec4<u32>,
    routing: vec4<u32>,
    channel: vec4<f32>,
}

struct TerrainVertex {
    position: vec4<f32>,
}

struct RiverVertex {
    position_surface: vec4<f32>,
    attributes: vec4<f32>,
}

@group(0) @binding(0) var<storage, read> heights: array<f32>;
@group(0) @binding(1) var<storage, read> spill_source: array<f32>;
@group(0) @binding(2) var<storage, read_write> spill_target: array<f32>;
@group(0) @binding(3) var<storage, read_write> flow: array<atomic<u32>>;
@group(0) @binding(4) var<storage, read_write> downstream: array<u32>;
@group(0) @binding(5) var<storage, read_write> river_field: array<vec4<f32>>;
@group(0) @binding(6) var<storage, read> vertices: array<TerrainVertex>;
@group(0) @binding(7) var<storage, read_write> carved: array<RiverVertex>;
@group(0) @binding(8) var<uniform> params: Params;

const LARGE_HEIGHT: f32 = 1.0e6;
const SPILL_EPSILON: f32 = 1.0e-7;
const CHANNEL_SEARCH_RADIUS: i32 = 5;

fn grid_index(point: vec2<i32>) -> u32 {
    return u32(point.y) * params.grid.x + u32(point.x);
}

fn in_grid(point: vec2<i32>) -> bool {
    let dimension = i32(params.grid.x);
    return all(point >= vec2<i32>(0)) && all(point < vec2<i32>(dimension));
}

fn cell_point(index: u32) -> vec2<f32> {
    let dimension = params.grid.x;
    let point = vec2<u32>(index % dimension, index / dimension);
    return (vec2<f32>(point) + vec2<f32>(0.5)) / f32(dimension);
}

fn hash(value: u32) -> u32 {
    var mixed = value ^ params.routing.z;
    mixed = mixed ^ (mixed >> 16u);
    mixed = mixed * 0x7feb352du;
    mixed = mixed ^ (mixed >> 15u);
    mixed = mixed * 0x846ca68bu;
    return mixed ^ (mixed >> 16u);
}

@compute @workgroup_size(64)
fn initialize_spill(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let index = invocation.x;
    if index >= params.grid.y {
        return;
    }
    let dimension = params.grid.x;
    let x = index % dimension;
    let y = index / dimension;
    let outlet = heights[index] <= 0.0 || x == 0u || y == 0u || x + 1u == dimension || y + 1u == dimension;
    spill_target[index] = select(LARGE_HEIGHT, heights[index], outlet);
    atomicStore(&flow[index], 0u);
    downstream[index] = index;
    river_field[index] = vec4<f32>(0.0);
}

@compute @workgroup_size(64)
fn relax_spill(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let index = invocation.x;
    if index >= params.grid.y {
        return;
    }
    if heights[index] <= 0.0 {
        spill_target[index] = heights[index];
        return;
    }
    let dimension = i32(params.grid.x);
    let centre = vec2<i32>(i32(index % params.grid.x), i32(index / params.grid.x));
    var lowest = spill_source[index];
    for (var dy = -1; dy <= 1; dy += 1) {
        for (var dx = -1; dx <= 1; dx += 1) {
            if dx == 0 && dy == 0 {
                continue;
            }
            let point = centre + vec2<i32>(dx, dy);
            if all(point >= vec2<i32>(0)) && all(point < vec2<i32>(dimension)) {
                lowest = min(lowest, spill_source[grid_index(point)]);
            }
        }
    }
    spill_target[index] = max(heights[index], lowest + SPILL_EPSILON);
}

@compute @workgroup_size(64)
fn map_drainage(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let index = invocation.x;
    if index >= params.grid.y || heights[index] <= 0.0 {
        return;
    }
    let centre = vec2<i32>(i32(index % params.grid.x), i32(index / params.grid.x));
    let current_spill = spill_source[index];
    var best = index;
    var best_spill = current_spill;
    var best_height = heights[index];
    var best_hash = hash(index);
    for (var dy = -1; dy <= 1; dy += 1) {
        for (var dx = -1; dx <= 1; dx += 1) {
            if dx == 0 && dy == 0 {
                continue;
            }
            let point = centre + vec2<i32>(dx, dy);
            if !in_grid(point) {
                continue;
            }
            let candidate = grid_index(point);
            let candidate_spill = spill_source[candidate];
            let candidate_height = heights[candidate];
            let candidate_hash = hash(candidate);
            let lower_spill = candidate_spill < best_spill;
            let tied_spill = abs(candidate_spill - best_spill) <= SPILL_EPSILON * 0.25;
            let better_tie = candidate_height < best_height ||
                (candidate_height == best_height && candidate_hash < best_hash);
            if lower_spill || (tied_spill && better_tie) {
                best = candidate;
                best_spill = candidate_spill;
                best_height = candidate_height;
                best_hash = candidate_hash;
            }
        }
    }
    downstream[index] = best;
}

@compute @workgroup_size(64)
fn accumulate_rain(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let origin = invocation.x;
    if origin >= params.grid.y || heights[origin] <= 0.0 {
        return;
    }
    var cell = origin;
    for (var step = 0u; step < params.grid.w; step += 1u) {
        atomicAdd(&flow[cell], 1u);
        let next = downstream[cell];
        if next == cell || next >= params.grid.y || heights[next] <= 0.0 {
            break;
        }
        cell = next;
    }
}

@compute @workgroup_size(64)
fn derive_field(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let index = invocation.x;
    if index >= params.grid.y {
        return;
    }
    let accumulated = atomicLoad(&flow[index]);
    if heights[index] <= 0.0 || accumulated < params.routing.x {
        river_field[index] = vec4<f32>(0.0);
        return;
    }
    let minimum_flow = f32(params.routing.x);
    let maximum_flow = f32(params.routing.y);
    let logarithmic = log2(max(f32(accumulated), minimum_flow) / minimum_flow);
    let range = max(log2(maximum_flow / minimum_flow), 1.0);
    let strength = clamp(logarithmic / range, 0.0, 1.0);
    // A sub-cell channel cannot select a continuous strip of the irregular
    // output mesh. Preserve requested widths once they exceed the sampling
    // footprint, but make the narrowest represented stream one and a half
    // hydrology cells wide.
    let minimum_width = 1.5 / f32(params.grid.x);
    let width = max(
        mix(params.channel.x, params.channel.y, smoothstep(0.0, 1.0, strength)),
        minimum_width,
    );
    let depth = mix(params.channel.z, params.channel.w, strength);
    let surface = heights[index] - depth * 0.18;
    river_field[index] = vec4<f32>(strength, width * 0.5, depth, surface);
}

@compute @workgroup_size(64)
fn carve_vertices(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let index = invocation.x;
    if index >= params.grid.z {
        return;
    }
    let input = vertices[index].position;
    let dimension = f32(params.grid.x);
    let centre = vec2<i32>(clamp(vec2<i32>(input.xy * dimension), vec2<i32>(0), vec2<i32>(i32(params.grid.x) - 1)));
    var nearest_distance = LARGE_HEIGHT;
    var nearest_field = vec4<f32>(0.0);
    for (var dy = -CHANNEL_SEARCH_RADIUS; dy <= CHANNEL_SEARCH_RADIUS; dy += 1) {
        for (var dx = -CHANNEL_SEARCH_RADIUS; dx <= CHANNEL_SEARCH_RADIUS; dx += 1) {
            let point = centre + vec2<i32>(dx, dy);
            if !in_grid(point) {
                continue;
            }
            let candidate = river_field[grid_index(point)];
            if candidate.y <= 0.0 {
                continue;
            }
            let distance = length(input.xy - cell_point(grid_index(point)));
            let normalized_distance = distance / candidate.y;
            if normalized_distance < nearest_distance {
                nearest_distance = normalized_distance;
                nearest_field = candidate;
            }
        }
    }
    if nearest_field.y <= 0.0 {
        carved[index].position_surface = vec4<f32>(input.xyz, 0.0);
        carved[index].attributes = vec4<f32>(0.0);
        return;
    }
    let bed_blend = 1.0 - smoothstep(0.65, 2.5, nearest_distance);
    let coverage = 1.0 - smoothstep(0.85, 1.25, nearest_distance);
    let carved_height = input.z - nearest_field.z * bed_blend;
    carved[index].position_surface = vec4<f32>(input.xy, carved_height, nearest_field.w);
    carved[index].attributes = vec4<f32>(coverage, nearest_field.x, nearest_distance, nearest_field.z);
}
