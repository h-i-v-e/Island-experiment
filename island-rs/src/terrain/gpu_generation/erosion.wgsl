struct VertexState {
    position: vec4<f32>,
    normal: vec4<f32>,
}

struct MaterialState {
    loose_depth: f32,
    bedrock_rate: f32,
    _padding: vec2<f32>,
}

struct Accumulator {
    x: atomic<i32>,
    y: atomic<i32>,
    z: atomic<i32>,
    loose: atomic<i32>,
}

@group(0) @binding(0) var<storage, read_write> vertices: array<VertexState>;
@group(0) @binding(1) var<storage, read_write> materials: array<MaterialState>;
@group(0) @binding(2) var<storage, read> topology: array<u32>;
@group(0) @binding(3) var<storage, read_write> accumulators: array<Accumulator>;
@group(0) @binding(4) var<storage, read> params: array<u32>;

const MODE_CLEAR: u32 = 0u;
const MODE_NORMALS: u32 = 1u;
const MODE_PARTICLES: u32 = 2u;
const MODE_APPLY: u32 = 3u;

fn param_u(index: u32) -> u32 {
    return params[index];
}

fn param_f(index: u32) -> f32 {
    return bitcast<f32>(params[index]);
}

fn unit_hash(left: u32, right: u32) -> f32 {
    var value = left * 0x9e3779b9u + right * 0x7f4a7c15u;
    value = value ^ (value >> 16u);
    value = value * 0x85ebca6bu;
    value = value ^ (value >> 13u);
    value = value * 0xc2b2ae35u;
    value = value ^ (value >> 16u);
    return f32(value >> 8u) / 16777215.0;
}

fn smooth_unit(value: f32) -> f32 {
    return value * value * (3.0 - 2.0 * value);
}

fn deposition_weight(slope: f32) -> f32 {
    let full_slope = param_f(17u);
    let maximum_slope = param_f(18u);
    let width = max(maximum_slope - full_slope, 0.0000001);
    let normalized = clamp((slope - full_slope) / width, 0.0, 1.0);
    return 1.0 - smooth_unit(normalized);
}

fn erosion_direction(normal: vec3<f32>) -> vec3<f32> {
    let vertical = clamp(normal.z, 0.0, 1.0);
    let beyond_forty_five = clamp(1.0 - 2.0 * vertical * vertical, 0.0, 1.0);
    return normalize(mix(normal, vec3<f32>(0.0, 0.0, 1.0), smooth_unit(beyond_forty_five)));
}

fn slope_erosion_weight(normal_z: f32) -> f32 {
    let vertical = clamp(normal_z, 0.0, 1.0);
    let horizontal = sqrt(max(0.0, 1.0 - vertical * vertical));
    return 2.0 * vertical * horizontal;
}

fn quantize(value: f32) -> i32 {
    let scale = param_f(13u);
    return i32(round(clamp(value * scale, -2147483520.0, 2147483520.0)));
}

fn add_delta(index: u32, position: vec3<f32>, loose: f32) {
    atomicAdd(&accumulators[index].x, quantize(position.x));
    atomicAdd(&accumulators[index].y, quantize(position.y));
    atomicAdd(&accumulators[index].z, quantize(position.z));
    atomicAdd(&accumulators[index].loose, quantize(loose));
}

fn clear_vertex(index: u32) {
    atomicStore(&accumulators[index].x, 0);
    atomicStore(&accumulators[index].y, 0);
    atomicStore(&accumulators[index].z, 0);
    atomicStore(&accumulators[index].loose, 0);
}

fn calculate_normal(index: u32) {
    let face_offsets_base = param_u(8u);
    let faces_base = param_u(9u);
    let triangles_base = param_u(7u);
    let start = topology[face_offsets_base + index];
    let end = topology[face_offsets_base + index + 1u];
    var normal = vec3<f32>(0.0, 0.0, 0.0);
    for (var cursor = start; cursor < end; cursor = cursor + 1u) {
        let face = topology[faces_base + cursor];
        let triangle = triangles_base + face * 3u;
        let a = vertices[topology[triangle]].position.xyz;
        let b = vertices[topology[triangle + 1u]].position.xyz;
        let c = vertices[topology[triangle + 2u]].position.xyz;
        normal = normal + cross(b - a, c - a);
    }
    let length_squared = dot(normal, normal);
    if length_squared > 0.000000000001 {
        normal = normal * inverseSqrt(length_squared);
    } else {
        normal = vec3<f32>(0.0, 0.0, 1.0);
    }
    let offsets_base = param_u(5u);
    let neighbours_base = param_u(6u);
    let position = vertices[index].position.xyz;
    let neighbour_start = topology[offsets_base + index];
    let neighbour_end = topology[offsets_base + index + 1u];
    var minimum_edge = param_f(14u) / max(param_f(20u), 0.000001);
    for (var cursor = neighbour_start; cursor < neighbour_end; cursor = cursor + 1u) {
        minimum_edge = min(
            minimum_edge,
            distance(position, vertices[topology[neighbours_base + cursor]].position.xyz),
        );
    }
    let move_limit = min(param_f(14u), minimum_edge * param_f(20u));
    vertices[index].normal = vec4<f32>(normal, move_limit);
}

fn downstream(current: u32, source: u32, step: u32) -> u32 {
    let offsets_base = param_u(5u);
    let neighbours_base = param_u(6u);
    let route_jitter = param_f(19u);
    let current_height = vertices[current].position.z;
    let start = topology[offsets_base + current];
    let end = topology[offsets_base + current + 1u];
    var best = 0xffffffffu;
    var best_score = -1.0;
    for (var cursor = start; cursor < end; cursor = cursor + 1u) {
        let candidate = topology[neighbours_base + cursor];
        let drop = current_height - vertices[candidate].position.z;
        if drop > 0.0 {
            let edge_bias = unit_hash(current, candidate);
            let particle_bias = unit_hash(source ^ (step * 0x9e3779b9u), candidate);
            let score = drop * (1.0 + route_jitter * ((edge_bias - 0.5) + 0.2 * (particle_bias - 0.5)));
            if score > best_score {
                best = candidate;
                best_score = score;
            }
        }
    }
    return best;
}

fn local_edge_cap(index: u32, global_shift: f32) -> f32 {
    let offsets_base = param_u(5u);
    let neighbours_base = param_u(6u);
    let position = vertices[index].position.xyz;
    let start = topology[offsets_base + index];
    let end = topology[offsets_base + index + 1u];
    var minimum_edge = global_shift / 0.08;
    for (var cursor = start; cursor < end; cursor = cursor + 1u) {
        let candidate = topology[neighbours_base + cursor];
        minimum_edge = min(minimum_edge, distance(position, vertices[candidate].position.xyz));
    }
    return min(global_shift, minimum_edge * 0.08);
}

fn deposit_terminal(current: u32, sediment: ptr<function, f32>) {
    let global_shift = param_f(14u);
    let deposited = min(min((*sediment) * param_f(16u) * 0.35, global_shift), *sediment);
    *sediment = *sediment - deposited;
    add_delta(current, vec3<f32>(0.0, 0.0, deposited), deposited);
}

fn trace_particle(invocation: u32) {
    let land_sources = param_u(2u);
    let batch_count = param_u(3u);
    let rank = param_u(4u) + invocation * batch_count;
    if rank >= land_sources {
        return;
    }
    let source = topology[param_u(10u) + rank];
    let global_shift = param_f(14u);
    var current = source;
    var speed = 0.0;
    var sediment = 0.0;
    for (var step = 0u; step < param_u(12u); step = step + 1u) {
        let next = downstream(current, source, step);
        if next == 0xffffffffu || vertices[next].position.z < 0.0 {
            deposit_terminal(current, &sediment);
            break;
        }

        let fall = vertices[current].position.xyz - vertices[next].position.xyz;
        let distance_3d = max(length(fall), 0.0000001);
        let horizontal_distance = max(length(fall.xy), 0.0000001);
        let slope = fall.z / horizontal_distance;
        let sin_slope = fall.z / distance_3d;
        let acceleration = sin_slope * sin_slope * sin_slope * distance_3d;
        speed = speed * 0.75 + acceleration * 0.25;

        let normal = vertices[current].normal.xyz;
        let direction = erosion_direction(normal);
        let deposit_weight = deposition_weight(slope);
        let difference = sediment - speed;
        var position_delta = vec3<f32>(0.0, 0.0, 0.0);
        var loose_delta = 0.0;
        if difference > 0.0 {
            let rate = min(param_f(16u) * 0.35 * deposit_weight, 1.0);
            let deposited = min(min(difference * rate, global_shift), sediment);
            sediment = sediment - deposited;
            position_delta.z = deposited;
            loose_delta = deposited;
        } else {
            let erosion_weight = 1.0 - deposit_weight;
            let requested = min(
                (-difference) * param_f(15u) * erosion_weight * slope_erosion_weight(normal.z),
                local_edge_cap(current, global_shift),
            );
            let available = select(0.0, max(vertices[current].position.z, 0.0) / direction.z, direction.z > 0.0000001);
            let bounded = min(requested, available);
            let loose_removed = min(materials[current].loose_depth, bounded);
            let bedrock_removed = max(bounded - loose_removed, 0.0) * clamp(materials[current].bedrock_rate, 0.0, 1.0);
            let retreat = loose_removed + bedrock_removed;
            sediment = sediment + retreat;
            position_delta = -direction * retreat;
            loose_delta = -loose_removed;
        }
        add_delta(current, position_delta, loose_delta);
        current = next;
    }
}

fn apply_vertex(index: u32) {
    let scale_inverse = 1.0 / param_f(13u);
    var delta = vec3<f32>(
        f32(atomicLoad(&accumulators[index].x)),
        f32(atomicLoad(&accumulators[index].y)),
        f32(atomicLoad(&accumulators[index].z)),
    ) * scale_inverse;
    let position = vertices[index].position.xyz;
    let limit = vertices[index].normal.w;
    let delta_length = length(delta);
    if delta_length > limit && delta_length > 0.0000001 {
        delta = delta * (limit / delta_length);
    }
    vertices[index].position = vec4<f32>(position + delta, 0.0);
    let loose_delta = f32(atomicLoad(&accumulators[index].loose)) * scale_inverse;
    materials[index].loose_depth = max(0.0, materials[index].loose_depth + loose_delta);
}

@compute @workgroup_size(64)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let index = global_id.x;
    let mode = param_u(0u);
    if mode == MODE_CLEAR {
        if index < param_u(1u) {
            clear_vertex(index);
        }
    } else if mode == MODE_NORMALS {
        if index < param_u(1u) {
            calculate_normal(index);
        }
    } else if mode == MODE_PARTICLES {
        trace_particle(index);
    } else if mode == MODE_APPLY && index < param_u(1u) {
        apply_vertex(index);
    }
}
