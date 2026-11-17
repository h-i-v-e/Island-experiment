use crate::{
    BuildingRecipe, MaterialSlot,
    collision::BoxCollider,
    geometry::{Builder, slabs},
};

pub(super) fn foundation(builder: &mut Builder, recipe: &BuildingRecipe, boxes: &[BoxCollider]) {
    slabs::joined_boxes(builder, MaterialSlot::Foundation, boxes);
    if recipe.church.is_some() {
        return;
    }
    // Preserve the original narrow stone cap at floor height, while the wooden
    // floor owns its interior top surface. Otherwise both faces lie at Z=0.
    let mesh = builder.mesh(MaterialSlot::Foundation);
    mesh.indices = mesh
        .indices
        .chunks_exact(3)
        .filter(|tri| mesh.normals[tri[0] as usize][2] < 0.5)
        .flatten()
        .copied()
        .collect();
    let floors = boxes
        .iter()
        .map(|b| BoxCollider {
            min: [
                b.min[0] + recipe.foundation.outset,
                b.min[1] + recipe.foundation.outset,
                0.,
            ],
            max: [
                b.max[0] - recipe.foundation.outset,
                b.max[1] - recipe.foundation.outset,
                0.,
            ],
        })
        .collect::<Vec<_>>();
    for (index, &bounds) in boxes.iter().enumerate() {
        let mut regions = vec![bounds];
        for &cover in floors.iter().chain(&boxes[..index]) {
            regions = regions
                .into_iter()
                .flat_map(|r| subtract(r, cover))
                .collect();
        }
        for region in regions {
            let [x0, y0, _] = region.min;
            let [x1, y1, _] = region.max;
            builder.face(
                MaterialSlot::Foundation,
                [[x0, y0, 0.], [x1, y0, 0.], [x1, y1, 0.], [x0, y1, 0.]],
            );
        }
    }
}

fn subtract(region: BoxCollider, cover: BoxCollider) -> impl Iterator<Item = BoxCollider> {
    let x0 = region.min[0].max(cover.min[0]);
    let y0 = region.min[1].max(cover.min[1]);
    let x1 = region.max[0].min(cover.max[0]);
    let y1 = region.max[1].min(cover.max[1]);
    let pieces = if x0 < x1 && y0 < y1 {
        [
            (region.min, [x0, region.max[1], 0.]),
            ([x1, region.min[1], 0.], region.max),
            ([x0, region.min[1], 0.], [x1, y0, 0.]),
            ([x0, y1, 0.], [x1, region.max[1], 0.]),
        ]
        .map(|(min, max)| Some(BoxCollider { min, max }))
    } else {
        [Some(region), None, None, None]
    };
    pieces
        .into_iter()
        .flatten()
        .filter(|b| b.min[0] < b.max[0] && b.min[1] < b.max[1])
}
