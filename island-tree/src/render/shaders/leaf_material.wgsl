#import bevy_pbr::{
    forward_io::{VertexOutput, FragmentOutput},
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::{alpha_discard, apply_pbr_lighting, main_pass_post_lighting_processing},
}

const LEAF_ATLAS_INSET: f32 = 1.0 / 256.0;

fn leaf_hash(value: f32) -> f32 {
    return fract(sin(value * 91.713 + 17.319) * 43758.5453);
}

fn leaf_value_noise(point: vec2<f32>, seed: f32) -> f32 {
    let cell = floor(point);
    let fraction = fract(point);
    let blend = fraction * fraction * (3.0 - 2.0 * fraction);
    let cell_seed = dot(cell, vec2<f32>(127.1, 311.7)) + seed * 74.7;
    let bottom = mix(
        leaf_hash(cell_seed),
        leaf_hash(cell_seed + 127.1),
        blend.x,
    );
    let top = mix(
        leaf_hash(cell_seed + 311.7),
        leaf_hash(cell_seed + 438.8),
        blend.x,
    );
    return mix(bottom, top, blend.y);
}

fn leaf_local_uv(atlas_uv: vec2<f32>) -> vec2<f32> {
    let tiled = fract(atlas_uv * 2.0);
    return clamp(
        (tiled - vec2<f32>(LEAF_ATLAS_INSET)) / (1.0 - LEAF_ATLAS_INSET * 2.0),
        vec2<f32>(0.0),
        vec2<f32>(1.0),
    );
}

// Seven finite, individually jittered curves form the secondary venation.
// This deliberately avoids a periodic phase field: repeated bands were visible
// even when their normal amplitude was very small.
fn secondary_vein_mask(uv: vec2<f32>) -> f32 {
    let lateral = abs(uv.x - 0.5) * 2.0;
    let root_fade = smoothstep(0.025, 0.080, lateral);
    let end_fade = smoothstep(0.055, 0.14, uv.y) * (1.0 - smoothstep(0.90, 0.985, uv.y));
    var mask = 0.0;

    for (var index = 0u; index < 7u; index += 1u) {
        let branch = f32(index);
        let origin = 0.13 + branch * 0.115 + (leaf_hash(branch + 3.1) - 0.5) * 0.034;
        let slope = 0.085 + leaf_hash(branch + 19.7) * 0.055;
        let bow = (leaf_hash(branch + 31.3) - 0.5) * 0.050;
        let curve = origin + lateral * slope + lateral * (1.0 - lateral) * bow;
        let half_width = 0.0050 + leaf_hash(branch + 47.9) * 0.0025;
        let line = 1.0 - smoothstep(half_width, half_width * 3.0, abs(uv.y - curve));
        let reach = 0.72 + leaf_hash(branch + 61.1) * 0.20;
        let reach_fade = 1.0 - smoothstep(reach, min(reach + 0.10, 0.995), lateral);
        mask = max(mask, line * reach_fade);
    }

    return mask * root_fade * end_fade;
}

fn cuticle_height(uv: vec2<f32>, tile: f32) -> f32 {
    // Smooth value noise gives the cuticle a weak, non-directional break-up.
    // Unlike the former crossed sine waves it has no rows for the eye to lock
    // onto, and the tile seed keeps the four archetypes from sharing a pattern.
    let seed = tile * 11.3 + 5.7;
    let coarse = leaf_value_noise(uv * vec2<f32>(47.0, 61.0), seed) - 0.5;
    let fine = leaf_value_noise(
        vec2<f32>(uv.y, -uv.x) * vec2<f32>(89.0, 73.0) + vec2<f32>(17.3, 9.1),
        seed + 19.0,
    ) - 0.5;
    return coarse * 0.72 + fine * 0.28;
}

fn mesophyll_density(uv: vec2<f32>, tile: f32) -> f32 {
    // A leaf's internal palisade and spongy tissue is not optically uniform.
    // Two low-frequency, non-periodic fields vary the path length seen by
    // transmitted light without painting visible pigment noise on the surface.
    let seed = tile * 13.9 + 31.7;
    let broad = leaf_value_noise(uv * vec2<f32>(7.0, 11.0), seed) - 0.5;
    let fine = leaf_value_noise(
        vec2<f32>(-uv.y, uv.x) * vec2<f32>(17.0, 13.0) + vec2<f32>(11.9, 7.3),
        seed + 23.0,
    ) - 0.5;
    return broad * 0.72 + fine * 0.28;
}

fn leaf_thickness_profile(uv: vec2<f32>, atlas_tile: f32, veins: f32) -> f32 {
    let lateral = abs(uv.x - 0.5) * 2.0;
    let margin = smoothstep(0.66, 0.98, lateral);
    let base_and_tip = 1.0 - smoothstep(0.055, 0.17, uv.y)
        + smoothstep(0.84, 0.985, uv.y);
    let midrib = 1.0 - smoothstep(0.012, 0.060, abs(uv.x - 0.5));
    var age_scale = 1.0;
    if atlas_tile >= 2.0 {
        age_scale -= 0.07;
    }
    if atlas_tile >= 3.0 {
        age_scale -= 0.06;
    }
    return clamp(
        (0.74 + midrib * 0.30 + veins * 0.055 - margin * 0.20 - base_and_tip * 0.10)
            * age_scale,
        0.42,
        1.12,
    );
}

fn perturb_leaf_normal(
    world_position: vec3<f32>,
    world_normal: vec3<f32>,
    height: f32,
) -> vec3<f32> {
    let position_dx = dpdx(world_position);
    let position_dy = dpdy(world_position);
    let height_dx = dpdx(height);
    let height_dy = dpdy(height);
    let basis_u = cross(position_dy, world_normal);
    let basis_v = cross(world_normal, position_dx);
    let determinant = dot(position_dx, basis_u);
    let surface_gradient = sign(determinant) * (height_dx * basis_u + height_dy * basis_v);
    return normalize(max(abs(determinant), 1.0e-9) * world_normal - surface_gradient);
}

@fragment
fn fragment(
    in: VertexOutput,
    @builtin(front_facing) is_front: bool,
) -> FragmentOutput {
    var pbr_input = pbr_input_from_standard_material(in, is_front);
    let uv = leaf_local_uv(in.uv);
    let atlas_tile = dot(floor(in.uv * 2.0), vec2<f32>(1.0, 2.0));
    let uv_footprint = max(length(dpdx(uv)), length(dpdy(uv)));
    let vein_fade = 1.0 - smoothstep(0.014, 0.060, uv_footprint);
    let cuticle_fade = 1.0 - smoothstep(0.0025, 0.014, uv_footprint);
    let mesophyll_fade = 1.0 - smoothstep(0.010, 0.040, uv_footprint);
    let veins = secondary_vein_mask(uv) * vein_fade;
    let cuticle = cuticle_height(uv, atlas_tile) * cuticle_fade;
    let mesophyll = mesophyll_density(uv, atlas_tile) * mesophyll_fade;
    let height = veins * 0.000055 + cuticle * 0.000008;
    let geometric_normal = normalize(select(-in.world_normal, in.world_normal, is_front));

    // Keep the face-oriented geometric normal for shadows and transmission
    // offsets; only the lighting normal receives sub-millimetre relief.
    pbr_input.world_normal = geometric_normal;
    pbr_input.N = perturb_leaf_normal(in.world_position.xyz, geometric_normal, height);
    // The weak upper-surface clearcoat represents the waxy cuticle rather than
    // a varnished shell. A smaller, independently perturbed normal broadens and
    // breaks up its highlight without imprinting the deeper veins twice.
    pbr_input.clearcoat_N = perturb_leaf_normal(
        in.world_position.xyz,
        geometric_normal,
        cuticle * 0.000003,
    );
    pbr_input.material.clearcoat *= clamp(1.0 - cuticle * 0.10, 0.94, 1.06);
    pbr_input.material.clearcoat_perceptual_roughness = clamp(
        pbr_input.material.clearcoat_perceptual_roughness + cuticle * 0.035,
        0.46,
        0.68,
    );
    let tissue_density = clamp(1.0 + mesophyll * 0.26, 0.87, 1.13);
    pbr_input.material.thickness *=
        leaf_thickness_profile(uv, atlas_tile, veins) * tissue_density;
    pbr_input.material.diffuse_transmission *= clamp(
        1.0 - mesophyll * 0.22 - veins * 0.035,
        0.89,
        1.11,
    );
    pbr_input.material.base_color = vec4<f32>(
        pbr_input.material.base_color.rgb * vec3<f32>(1.0 + veins * 0.004),
        pbr_input.material.base_color.a,
    );
    pbr_input.material.perceptual_roughness = clamp(
        pbr_input.material.perceptual_roughness - veins * 0.006 + cuticle * 0.005,
        0.38,
        0.96,
    );

    if !is_front {
        // Pohutukawa undersides retain a dense grey-white felt. Blend toward a
        // restrained olive-grey rather than multiplying the front pigment;
        // this keeps the botanical face distinction without white card flashes.
        pbr_input.material.base_color = vec4<f32>(
            mix(
                pbr_input.material.base_color.rgb * vec3<f32>(0.82, 0.88, 0.80),
                vec3<f32>(0.36, 0.41, 0.33),
                0.20,
            ),
            pbr_input.material.base_color.a,
        );
        pbr_input.material.perceptual_roughness = min(
            pbr_input.material.perceptual_roughness + 0.15,
            1.0,
        );
        pbr_input.material.reflectance *= vec3<f32>(0.62);
        pbr_input.material.clearcoat = 0.0;
        pbr_input.material.diffuse_transmission *= 0.95;
        pbr_input.material.attenuation_color = vec4<f32>(
            mix(
                pbr_input.material.attenuation_color.rgb,
                vec3<f32>(0.34, 0.45, 0.29),
                0.24,
            ),
            pbr_input.material.attenuation_color.a,
        );
    }

    pbr_input.material.base_color = alpha_discard(
        pbr_input.material,
        pbr_input.material.base_color,
    );
    var out: FragmentOutput;
    out.color = apply_pbr_lighting(pbr_input);
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);
    return out;
}
