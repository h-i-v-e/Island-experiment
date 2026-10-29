//! Small finite global weather texture shared by native shader probes.
pub fn image(device: &wgpu::Device, queue: &wgpu::Queue) -> wgpu::TextureView {
    image_with_previous(device, queue, 1.0)
}
pub fn image_with_previous(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    cloud_scale: f32,
) -> wgpu::TextureView {
    fields(device, queue, cloud_scale, 1.0)
}
#[allow(dead_code)] // Only the opacity probe needs this variant.
pub fn image_with_density(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    density_scale: f32,
) -> wgpu::TextureView {
    fields(device, queue, 1.0, density_scale)
}
fn fields(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    cloud_scale: f32,
    density_scale: f32,
) -> wgpu::TextureView {
    fields_with_clouds(
        device,
        queue,
        cloud_scale,
        [0.8, 0.55],
        [0.85 * density_scale, 0.55 * density_scale],
    )
}
#[allow(dead_code)]
pub fn image_with_clouds(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    coverage: [f32; 2],
    density: [f32; 2],
) -> wgpu::TextureView {
    fields_with_clouds(device, queue, 1.0, coverage, density)
}
fn fields_with_clouds(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    cloud_scale: f32,
    coverage: [f32; 2],
    density: [f32; 2],
) -> wgpu::TextureView {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("two_layer_weather_fixture"),
        size: wgpu::Extent3d {
            width: 4,
            height: 4,
            depth_or_array_layers: 72,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba16Float,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    let layers: [[f32; 4]; 6] = [
        [6.0, 0.0, 1.0, coverage[0]],
        [-4.0, 0.0, 3.0, coverage[1]],
        [288.0, 1013.0, 0.8, 0.1],
        [0.0; 4],
        [0.0; 4],
        [density[0], density[1], 0.0, 0.0],
    ];
    let mut data = Vec::new();
    let mut previous = layers;
    previous[0][3] *= cloud_scale;
    previous[1][3] *= cloud_scale;
    for layer in layers.into_iter().chain(previous) {
        for _ in 0..6 * 4 * 4 {
            for value in layer {
                data.extend(half(value).to_le_bytes());
            }
        }
    }
    queue.write_texture(
        texture.as_image_copy(),
        &data,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(4 * 8),
            rows_per_image: Some(4),
        },
        texture.size(),
    );
    texture.create_view(&wgpu::TextureViewDescriptor {
        dimension: Some(wgpu::TextureViewDimension::CubeArray),
        ..Default::default()
    })
}
fn half(value: f32) -> u16 {
    let bits = value.to_bits();
    let sign = (bits >> 16) as u16 & 0x8000;
    let exponent = ((bits >> 23) & 0xff) as i32 - 127 + 15;
    if exponent <= 0 {
        return sign;
    }
    let rounded = ((bits & 0x7fffff) + 0x1000) >> 13;
    sign | ((exponent as u16) << 10) + rounded as u16
}
