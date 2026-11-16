use crate::{BakedAtlas, BuildingLod, MaterialSlot};

pub(super) fn grid(count: usize) -> u32 {
    let mut grid = 1_u32;
    while usize::try_from(grid * grid).unwrap() < count {
        grid += 1;
    }
    grid
}

pub(super) fn remap(lod: &mut BuildingLod, index: usize, count: usize, size: u32) {
    let grid = grid(count);
    let index = u32::try_from(index).unwrap();
    let x = index % grid;
    let y = index / grid;
    #[allow(clippy::cast_precision_loss)]
    let rect = [
        x * size / grid,
        y * size / grid,
        (x + 1) * size / grid,
        (y + 1) * size / grid,
    ]
    .map(|v| v as f32 / size as f32);
    for part in &mut lod.parts {
        if part.material != MaterialSlot::Atlas {
            continue;
        }
        for uv in &mut part.mesh.uvs {
            uv[0] = rect[0] + uv[0] * (rect[2] - rect[0]);
            uv[1] = rect[1] + uv[1] * (rect[3] - rect[1]);
        }
    }
}

pub(super) fn pack(sources: &[BakedAtlas], size: u32) -> BakedAtlas {
    let mut atlas = BakedAtlas {
        size,
        albedo: vec![0; (size * size * 4) as usize],
        normal: vec![0; (size * size * 4) as usize],
        masks: vec![0; (size * size * 4) as usize],
    };
    let grid = grid(sources.len());
    for (index, source) in sources.iter().enumerate() {
        let index = u32::try_from(index).unwrap();
        let x0 = index % grid * size / grid;
        let x1 = (index % grid + 1) * size / grid;
        let y0 = index / grid * size / grid;
        let y1 = (index / grid + 1) * size / grid;
        for y in y0..y1 {
            for x in x0..x1 {
                let sx = (x - x0) * source.size / (x1 - x0);
                let sy = (y - y0) * source.size / (y1 - y0);
                let src = ((sy * source.size + sx) * 4) as usize;
                let dst = ((y * size + x) * 4) as usize;
                for (output, input) in [
                    (&mut atlas.albedo, &source.albedo),
                    (&mut atlas.normal, &source.normal),
                    (&mut atlas.masks, &source.masks),
                ] {
                    output[dst..dst + 4].copy_from_slice(&input[src..src + 4]);
                }
            }
        }
    }
    atlas
}
