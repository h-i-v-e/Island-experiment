#import planet_bevy::environment_material::{material_cloud_light,environment_sky_sampler}
#import planet_bevy::weather::{weather_enabled,weather_debug_mode,weather_debug_colour}
// Unity Island ground shading with radial gravity, plus streamed LOD normal/occlusion atlases.
// Water keeps StandardMaterial shading.
#import bevy_pbr::{
    forward_io::{VertexOutput, FragmentOutput},
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::{alpha_discard, apply_pbr_lighting, main_pass_post_lighting_processing},
    pbr_types::{STANDARD_MATERIAL_FLAGS_UNLIT_BIT, STANDARD_MATERIAL_FLAGS_DOUBLE_SIDED_BIT},
    mesh_bindings::mesh,
    mesh_view_bindings::view,
    mesh_functions::get_world_from_local,
    decal::clustered::apply_decals,
}
#import bevy_render::maths::mat2x4_f32_to_mat3x3_unpack

#ifdef VISIBILITY_RANGE_DITHER
#import bevy_pbr::pbr_functions::visibility_range_dither;
#endif

struct SurfaceSettings {
    enabled: u32,
    debug: u32,
    rivers: u32,
    terrain: u32,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(100) var<uniform> settings: SurfaceSettings;
@group(#{MATERIAL_BIND_GROUP}) @binding(101) var surface_map: texture_2d<f32>;

@group(#{MATERIAL_BIND_GROUP}) @binding(103) var river_map: texture_2d<f32>;

#import planet_bevy::ground::{ground, shade_ground, volume_noise, perturb, ROCK}
#import planet_bevy::river::river_water

@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
#ifdef VISIBILITY_RANGE_DITHER
    visibility_range_dither(in.position, in.visibility_range_dither);
#endif
    var pbr_input = pbr_input_from_standard_material(in, is_front);
    if settings.terrain==4u {
        return FragmentOutput(main_pass_post_lighting_processing(pbr_input,river_water(in,pbr_input,is_front)));
    }
#ifdef VERTEX_UVS_A
    let atlas_uv = in.uv;
#else
    let atlas_uv = vec2<f32>(0.0);
#endif
    // Sample outside the mode branch so implicit derivatives remain uniform.
    let surface = textureSample(surface_map, environment_sky_sampler, atlas_uv);
    let river = textureSample(river_map, environment_sky_sampler, atlas_uv).r;
    let normal_to_world = mat2x4_f32_to_mat3x3_unpack(
        mesh[in.instance_index].local_from_world_transpose_a,
        mesh[in.instance_index].local_from_world_transpose_b,
    );
    let transform = get_world_from_local(in.instance_index);
    let world_linear = mat3x3<f32>(transform[0].xyz, transform[1].xyz, transform[2].xyz);
    let local_position = transpose(normal_to_world) * (in.world_position.xyz - transform[3].xyz);
    let metres = local_position * ground.radius;
    let footprint = max(length(dpdx(metres)), length(dpdy(metres)));
    var normal = pbr_input.N;
    var occlusion = 1.0;
    if settings.enabled != 0u {
        // Store the difference from the target mesh's interpolated normal,
        // rather than another absolute normal. Filtering an unchanged steep
        // bank then adds precisely zero instead of blurring across its crest.
        let object_correction = (surface.rgb * 255.0 - 128.0) / 63.5;
        // PBR has already normalized its world normal. Recover the inverse of
        // that scale before adding the unnormalized correction, so rotation
        // and nonuniform entity scale retain the correction's true magnitude.
        let base_scale = length(transpose(world_linear) * pbr_input.N);
        var world_correction = normal_to_world * object_correction * base_scale;
        let double_sided = (pbr_input.material.flags & STANDARD_MATERIAL_FLAGS_DOUBLE_SIDED_BIT) != 0u;
        if !is_front && double_sided {
            world_correction = -world_correction;
        }
        let corrected = pbr_input.N + world_correction;
        if dot(corrected, corrected) > 1e-8 {
            normal = normalize(corrected);
        }
        pbr_input.N = normal;
        // Keep geometric world_normal for shadow bias, as StandardMaterial's
        // tangent normal maps do. The baked normal supplies lighting detail.
        occlusion = clamp(surface.a, 0.0, 1.0);
        let shared_ao = mix(1.0, occlusion, select(1.0, 0.25, settings.terrain == 1u));
        pbr_input.diffuse_occlusion *= vec3<f32>(shared_ao);
        pbr_input.specular_occlusion *= shared_ao;
    }
    if settings.terrain == 1u {
#ifdef VERTEX_COLORS
        let material_channels = in.color;
#else
        let material_channels = vec4<f32>(0.5, 1.0, 0.0, 0.0);
#endif
        let local_normal = normalize(transpose(world_linear) * normal);
        let local_view = normalize(transpose(normal_to_world) * pbr_input.V);
        let ground_surface = shade_ground(metres, local_normal, local_view, material_channels,
            footprint, length(in.world_position.xyz - view.world_position) * ground.radius);
        pbr_input.material.base_color = vec4<f32>(ground_surface.colour, 1.0);
        pbr_input.N = normalize(normal_to_world * ground_surface.normal);
        // Coarse rivers are a water-colour overlay on simplified terrain. Keep
        // the bed's pebble normals/AO out of that water, as for the LOD0 mesh.
        let water_coverage = select(0.0, river, settings.rivers != 0u);
        let ground_ao = mix(ground_surface.occlusion, 1.0, water_coverage);
        pbr_input.diffuse_occlusion *= vec3<f32>(ground_ao);
        pbr_input.specular_occlusion *= ground_ao;
        pbr_input.material.perceptual_roughness = mix(0.92, 0.35, ground_surface.wet);
    } else if settings.terrain == 3u {
        let local_normal = normalize(transpose(world_linear) * normal);
        let q = metres / 160.0;
        let broad = volume_noise(q) * 2.0 - 1.0;
        let detail = volume_noise(q * 16.0 + vec3<f32>(0.37, 0.61, 0.83)) * 2.0 - 1.0;
        pbr_input.N = normalize(normal_to_world * perturb(local_normal, broad * 0.45 + detail * 0.55, 0.12));
#ifdef VERTEX_COLORS
        let tint = in.color.rgb;
#else
        let tint = vec3<f32>(1.0);
#endif
        pbr_input.material.base_color = vec4<f32>(ROCK * tint * (1.0 + dot(broad, vec3<f32>(0.45, 0.35, 0.20)) * 0.10), 1.0);
        if length(in.world_position.xyz - view.world_position) * ground.radius > 80.0 { discard; }
    } else if settings.terrain == 2u {
        pbr_input.material.base_color = ground.colour;
    }
    if settings.rivers != 0u {
        // Same linear colour as the LOD0 river material. This lies directly on
        // freely simplified terrain; no detached coarse river mesh is drawn.
        let water_colour = vec3<f32>(0.003501, 0.125967, 0.380056);
        pbr_input.material.base_color = vec4<f32>(mix(pbr_input.material.base_color.rgb,
            water_colour, river), pbr_input.material.base_color.a);
        pbr_input.material.perceptual_roughness = mix(pbr_input.material.perceptual_roughness, 0.35, river);
        pbr_input.N = normalize(mix(pbr_input.N, normal, river));
    }
    pbr_input.material.base_color = alpha_discard(pbr_input.material, pbr_input.material.base_color);
    apply_decals(&pbr_input);
    var out: FragmentOutput;
    if (pbr_input.material.flags & STANDARD_MATERIAL_FLAGS_UNLIT_BIT) == 0u {
        out.color = apply_pbr_lighting(pbr_input);
        out.color = vec4<f32>(out.color.rgb * material_cloud_light(in.world_position.xyz), out.color.a);
    } else {
        out.color = pbr_input.material.base_color;
    }
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);
    if settings.enabled != 0u && settings.debug == 1u {
        out.color = vec4<f32>(normal * 0.5 + 0.5, 1.0);
    } else if settings.enabled != 0u && settings.debug == 2u {
        out.color = vec4<f32>(vec3<f32>(occlusion), 1.0);
    }
    if weather_enabled() && weather_debug_mode() > 0u { out.color = vec4<f32>(weather_debug_colour(in.world_position.xyz), 1.0); }
    return out;
}
