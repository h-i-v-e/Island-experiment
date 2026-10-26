#define_import_path planet_bevy::radial

struct RadialMapping {
    uv: vec2<f32>,
    du: vec3<f32>,
    dv: vec3<f32>,
    stretch: f32,
}

// Lambert azimuthal equal-area maps for the north and south hemispheres.
// Both share the same equator circle: crossing it mirrors the pattern instead
// of jumping between unrelated texture edges. The poles map to disc centres.
// Coordinates are sea-sphere metres, independent of terrain elevation.
fn radial_mapping(p: vec3<f32>, radius: f32) -> RadialMapping {
    let distance = max(length(p), 0.0001);
    let up = p / distance;
    let denominator = 1.0 + abs(up.y);
    let scale = sqrt(2.0 / denominator);
    let uv = up.xz * (radius * scale);
    let fold = sign(up.y) / (2.0 * denominator);
    let u_axis = vec3<f32>(1.0, -up.x * fold, 0.0);
    let v_axis = vec3<f32>(0.0, -up.z * fold, 1.0);
    let stretch = radius * scale / distance;
    // The UV Jacobian supplies surface gradients for normal maps. It includes
    // the hemisphere reflection, so mirrored colour and normal details agree.
    return RadialMapping(uv,
        (u_axis - up * dot(u_axis, up)) * stretch,
        (v_axis - up * dot(v_axis, up)) * stretch,
        stretch);
}
