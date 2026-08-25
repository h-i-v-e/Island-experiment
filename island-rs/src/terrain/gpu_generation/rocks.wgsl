struct RockState {
    position_radius: vec4<f32>,
    velocity_inverse_mass: vec4<f32>,
    previous_position: vec4<f32>,
    metadata: vec4<u32>,
}

struct TerrainVertex {
    position: vec4<f32>,
    normal: vec4<f32>,
}

struct RockParams {
    counts: vec4<u32>,
    offsets: vec4<u32>,
    physics: vec4<f32>,
    contact: vec4<f32>,
}

struct SurfaceSample {
    height: f32,
    normal: vec3<f32>,
    found: bool,
}

@group(0) @binding(0) var<storage, read> source_states: array<RockState>;
@group(0) @binding(1) var<storage, read_write> target_states: array<RockState>;
@group(0) @binding(2) var<storage, read> terrain_vertices: array<TerrainVertex>;
@group(0) @binding(3) var<storage, read> terrain_topology: array<u32>;
@group(0) @binding(4) var<storage, read_write> grid_counts: array<atomic<u32>>;
@group(0) @binding(5) var<storage, read_write> grid_indices: array<u32>;
@group(0) @binding(6) var<uniform> params: RockParams;

fn grid_cell(point: vec2<f32>) -> u32 {
    let dimension = params.counts.w;
    let maximum = i32(dimension) - 1;
    let x = u32(clamp(i32(point.x * f32(dimension)), 0, maximum));
    let y = u32(clamp(i32(point.y * f32(dimension)), 0, maximum));
    return y * dimension + x;
}

fn barycentric(point: vec2<f32>, a: vec2<f32>, b: vec2<f32>, c: vec2<f32>) -> vec3<f32> {
    let ab = b - a;
    let ac = c - a;
    let ap = point - a;
    let denominator = ab.x * ac.y - ac.x * ab.y;
    if abs(denominator) <= 1.0e-12 {
        return vec3<f32>(-1.0);
    }
    let b_weight = (ap.x * ac.y - ac.x * ap.y) / denominator;
    let c_weight = (ab.x * ap.y - ap.x * ab.y) / denominator;
    return vec3<f32>(1.0 - b_weight - c_weight, b_weight, c_weight);
}

fn sample_terrain(point: vec2<f32>) -> SurfaceSample {
    let dimension = params.counts.z;
    let maximum = i32(dimension) - 1;
    let x = u32(clamp(i32(point.x * f32(dimension)), 0, maximum));
    let y = u32(clamp(i32(point.y * f32(dimension)), 0, maximum));
    let bin = y * dimension + x;
    let offsets_base = params.offsets.z;
    let faces_base = params.offsets.w;
    let triangles_base = params.offsets.y;
    let begin = terrain_topology[offsets_base + bin];
    let end = terrain_topology[offsets_base + bin + 1u];
    var cursor = begin;
    loop {
        if cursor >= end {
            break;
        }
        let face = terrain_topology[faces_base + cursor];
        let triangle = triangles_base + face * 3u;
        let ia = terrain_topology[triangle];
        let ib = terrain_topology[triangle + 1u];
        let ic = terrain_topology[triangle + 2u];
        let a = terrain_vertices[ia];
        let b = terrain_vertices[ib];
        let c = terrain_vertices[ic];
        let weights = barycentric(
            point,
            a.position.xy,
            b.position.xy,
            c.position.xy,
        );
        if all(weights >= vec3<f32>(-1.0e-5)) {
            let height = dot(weights, vec3<f32>(a.position.z, b.position.z, c.position.z));
            let interpolated_normal =
                a.normal.xyz * weights.x + b.normal.xyz * weights.y + c.normal.xyz * weights.z;
            let normal_length = length(interpolated_normal);
            let normal = select(vec3<f32>(0.0, 0.0, 1.0), interpolated_normal / normal_length, normal_length > 1.0e-8);
            return SurfaceSample(height, normal, true);
        }
        cursor += 1u;
    }
    return SurfaceSample(-1.0e6, vec3<f32>(0.0, 0.0, 1.0), false);
}

@compute @workgroup_size(64)
fn integrate(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let index = invocation.x;
    if index >= params.counts.x {
        return;
    }
    var state = source_states[index];
    let time_step = params.physics.x;
    var velocity = state.velocity_inverse_mass.xyz;
    velocity.z -= params.physics.y * time_step;
    velocity *= params.physics.z;
    let previous = state.position_radius.xyz;
    var position = previous + velocity * time_step;
    let radius = state.position_radius.w;
    let minimum = 0.01 + radius;
    let maximum = 0.99 - radius;
    if position.x < minimum {
        position.x = minimum;
        velocity.x = abs(velocity.x) * params.contact.x;
    } else if position.x > maximum {
        position.x = maximum;
        velocity.x = -abs(velocity.x) * params.contact.x;
    }
    if position.y < minimum {
        position.y = minimum;
        velocity.y = abs(velocity.y) * params.contact.x;
    } else if position.y > maximum {
        position.y = maximum;
        velocity.y = -abs(velocity.y) * params.contact.x;
    }
    state.position_radius = vec4<f32>(position, radius);
    state.velocity_inverse_mass = vec4<f32>(velocity, state.velocity_inverse_mass.w);
    state.previous_position = vec4<f32>(previous, 0.0);
    state.metadata.y = 0u;
    state.metadata.z = 0u;
    target_states[index] = state;
}

@compute @workgroup_size(64)
fn clear_grid(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let cell = invocation.x;
    let cell_count = params.counts.w * params.counts.w;
    if cell < cell_count {
        atomicStore(&grid_counts[cell], 0u);
    }
}

@compute @workgroup_size(64)
fn scatter_grid(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let index = invocation.x;
    if index >= params.counts.x {
        return;
    }
    let cell = grid_cell(source_states[index].position_radius.xy);
    let slot = atomicAdd(&grid_counts[cell], 1u);
    let capacity = params.offsets.x;
    if slot < capacity {
        grid_indices[cell * capacity + slot] = index;
    }
}

@compute @workgroup_size(64)
fn sort_grid(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let cell = invocation.x;
    let cell_count = params.counts.w * params.counts.w;
    if cell >= cell_count {
        return;
    }
    let capacity = params.offsets.x;
    let count = min(atomicLoad(&grid_counts[cell]), capacity);
    let base = cell * capacity;
    var index = 1u;
    loop {
        if index >= count {
            break;
        }
        let key = grid_indices[base + index];
        var cursor = index;
        loop {
            if cursor == 0u || grid_indices[base + cursor - 1u] <= key {
                break;
            }
            grid_indices[base + cursor] = grid_indices[base + cursor - 1u];
            cursor -= 1u;
        }
        grid_indices[base + cursor] = key;
        index += 1u;
    }
}

@compute @workgroup_size(64)
fn solve_contacts(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let index = invocation.x;
    if index >= params.counts.x {
        return;
    }
    let source = source_states[index];
    let original_position = source.position_radius.xyz;
    let radius = source.position_radius.w;
    let inverse_mass = source.velocity_inverse_mass.w;
    var correction = vec3<f32>(0.0);
    var contact_count = 0u;
    var neighbour_supported = false;
    let dimension = params.counts.w;
    let cell_x = clamp(i32(original_position.x * f32(dimension)), 0, i32(dimension) - 1);
    let cell_y = clamp(i32(original_position.y * f32(dimension)), 0, i32(dimension) - 1);
    let capacity = params.offsets.x;
    var offset_y = -1;
    loop {
        if offset_y > 1 {
            break;
        }
        let y = cell_y + offset_y;
        if y >= 0 && y < i32(dimension) {
            var offset_x = -1;
            loop {
                if offset_x > 1 {
                    break;
                }
                let x = cell_x + offset_x;
                if x >= 0 && x < i32(dimension) {
                    let cell = u32(y) * dimension + u32(x);
                    let count = min(atomicLoad(&grid_counts[cell]), capacity);
                    var slot = 0u;
                    loop {
                        if slot >= count {
                            break;
                        }
                        let other_index = grid_indices[cell * capacity + slot];
                        if other_index != index {
                            let other = source_states[other_index];
                            let separation = original_position - other.position_radius.xyz;
                            let minimum_distance = radius + other.position_radius.w;
                            let distance_squared = dot(separation, separation);
                            if distance_squared < minimum_distance * minimum_distance {
                                let distance = sqrt(max(distance_squared, 0.0));
                                var normal = vec3<f32>(1.0, 0.0, 0.0);
                                if distance > 1.0e-8 {
                                    normal = separation / distance;
                                } else if source.metadata.x > other.metadata.x {
                                    normal = vec3<f32>(-1.0, 0.0, 0.0);
                                }
                                let inverse_mass_sum = inverse_mass + other.velocity_inverse_mass.w;
                                if inverse_mass_sum > 1.0e-12 {
                                    let overlap = minimum_distance - distance;
                                    correction += normal * (overlap * inverse_mass / inverse_mass_sum);
                                    contact_count += 1u;
                                    if normal.z > 0.35 && other.metadata.y != 0u {
                                        neighbour_supported = true;
                                    }
                                }
                            }
                        }
                        slot += 1u;
                    }
                }
                offset_x += 1;
            }
        }
        offset_y += 1;
    }

    if contact_count != 0u {
        correction *= params.contact.y / f32(contact_count);
    }
    var corrected_position = original_position + correction;
    let minimum = 0.01 + radius;
    let maximum = 0.99 - radius;
    corrected_position.x = clamp(corrected_position.x, minimum, maximum);
    corrected_position.y = clamp(corrected_position.y, minimum, maximum);
    var surface = sample_terrain(corrected_position.xy);
    let terrain_contact_height = surface.height + radius / max(surface.normal.z, 0.2);
    var terrain_supported = false;
    if surface.found && corrected_position.z <= terrain_contact_height {
        corrected_position.z = terrain_contact_height;
        terrain_supported = true;
    }

    var velocity = source.velocity_inverse_mass.xyz + correction / params.physics.x;
    let supported = terrain_supported || neighbour_supported;
    if terrain_supported {
        let normal_speed = dot(velocity, surface.normal);
        if normal_speed < 0.0 {
            velocity -= surface.normal * ((1.0 + params.contact.x) * normal_speed);
        }
        let remaining_normal_speed = max(dot(velocity, surface.normal), 0.0);
        let tangent = velocity - surface.normal * dot(velocity, surface.normal);
        velocity = tangent * params.physics.w + surface.normal * remaining_normal_speed;
    } else if supported {
        velocity *= params.physics.w;
    }
    let stable = supported && (neighbour_supported || surface.normal.z >= params.contact.z);
    var output = source;
    output.position_radius = vec4<f32>(corrected_position, radius);
    output.velocity_inverse_mass = vec4<f32>(velocity, inverse_mass);
    output.metadata.y = select(0u, 1u, supported);
    output.metadata.z = select(0u, 1u, stable);
    target_states[index] = output;
}
