#define_import_path island_building::grub

struct GrubUniform {
    strengths: vec4<f32>, // grime, moss, height falloff (m), patch size (m)
    dirt: vec4<f32>,
    moss: vec4<f32>,
    offset: vec4<f32>,
    noise: vec4<f32>, // fine patch size (m), fine contribution, reserved, reserved
}

fn grub_hash(cell: vec3<i32>) -> f32 {
    let p = bitcast<vec3<u32>>(cell);
    var h = p.x * 1597334677u ^ p.y * 3812015801u ^ p.z * 2798796415u;
    h = (h ^ (h >> 16u)) * 2146121005u;
    h = (h ^ (h >> 15u)) * 2221713035u;
    return f32((h ^ (h >> 16u)) & 16777215u) / 16777215.0;
}

fn grub_noise(p: vec3<f32>) -> f32 {
    let cell = vec3<i32>(floor(p));
    let f = fract(p);
    let t = f * f * (3.0 - 2.0 * f);
    let a = mix(grub_hash(cell), grub_hash(cell + vec3(1, 0, 0)), t.x);
    let b = mix(grub_hash(cell + vec3(0, 1, 0)), grub_hash(cell + vec3(1, 1, 0)), t.x);
    let c = mix(grub_hash(cell + vec3(0, 0, 1)), grub_hash(cell + vec3(1, 0, 1)), t.x);
    let d = mix(grub_hash(cell + vec3(0, 1, 1)), grub_hash(cell + vec3(1, 1, 1)), t.x);
    return mix(mix(a, b, t.y), mix(c, d, t.y), t.z);
}

fn grub_layers(position: vec3<f32>, settings: GrubUniform) -> f32 {
    let broad = grub_noise(position / settings.strengths.w + settings.offset.xyz);
    let fine = grub_noise(position / settings.noise.x + settings.offset.xyz + vec3(17.3, 4.2, 11.1));
    // Contrast each scale before combining, so fine breakup survives broad patches.
    return mix(smoothstep(0.25, 0.75, broad), smoothstep(0.25, 0.75, fine), settings.noise.y);
}

fn grub_strength(noise: f32, floor_height: f32, falloff: f32) -> f32 {
    let influence = 1.0 / (1.0 + exp2(floor_height / falloff));
    // Neutral noise is 50% at the building floor. Do not halve the noise again:
    // at the midpoint its complete 0..1 contrast range should remain visible.
    return clamp(2.0 * influence * noise, 0.0, 1.0);
}

// Position uses building-local Y-up metres; height remains relative to its floor.
// Upness must come from the final normal-mapped normal dotted with building up.
fn grub_weights(position: vec3<f32>, height: f32, upness: f32, settings: GrubUniform) -> vec2<f32> {
    let blotches = grub_layers(position, settings);
    let dirt = settings.strengths.x * grub_strength(blotches, height, settings.strengths.z);
    let moss_patch = smoothstep(0.4, 0.75, grub_layers(position + vec3(21.4, 8.7, 14.2), settings));
    let moss = settings.strengths.y * grub_strength(moss_patch, height, settings.strengths.z)
        * smoothstep(0.35, 0.85, upness);
    return clamp(vec2(dirt, moss), vec2(0.0), vec2(1.0));
}
