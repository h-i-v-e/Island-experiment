use super::index::Volumes;
use super::{
    ExtensionKind, ExtensionRecipe,
    clip::{self, Vertex, Volume},
};
use crate::{
    BuildingLod, BuildingRecipe, Facade, MaterialSlot, OpeningKind, OpeningRecipe, RoofKind,
    collision::BoxCollider,
};

#[derive(Clone, Copy)]
pub(super) struct Placement {
    pub centre: [f32; 3],
    pub x: [f32; 3],
    pub y: [f32; 3],
    pub normal_depth: f32,
    pub lean: bool,
    pub roof_reach: f32,
}

impl Placement {
    pub fn identity() -> Self {
        Self {
            centre: [0.; 3],
            x: [1., 0., 0.],
            y: [0., 1., 0.],
            normal_depth: 0.,
            lean: false,
            roof_reach: 0.,
        }
    }
    pub fn vector(self, p: [f32; 3]) -> [f32; 3] {
        [
            self.x[0] * p[0] + self.y[0] * p[1],
            self.x[1] * p[0] + self.y[1] * p[1],
            p[2],
        ]
    }
    pub fn point(self, p: [f32; 3]) -> [f32; 3] {
        let v = self.vector(p);
        std::array::from_fn(|i| v[i] + self.centre[i])
    }
    pub fn local_vector(self, p: [f32; 3]) -> [f32; 3] {
        [
            p[0] * self.x[0] + p[1] * self.x[1],
            p[0] * self.y[0] + p[1] * self.y[1],
            p[2],
        ]
    }
    pub fn local(self, p: [f32; 3]) -> [f32; 3] {
        self.local_vector(std::array::from_fn(|i| p[i] - self.centre[i]))
    }
    pub fn transform_box(self, bounds: BoxCollider) -> BoxCollider {
        let a = self.point(bounds.min);
        let b = self.point(bounds.max);
        BoxCollider {
            min: std::array::from_fn(|i| a[i].min(b[i])),
            max: std::array::from_fn(|i| a[i].max(b[i])),
        }
    }
    pub fn bounds(self, width: f32, depth: f32, thickness: f32) -> BoxCollider {
        let run = depth + 2. * thickness + 0.04;
        let (width, depth) = if self.lean {
            (run, width)
        } else {
            (width, run)
        };
        self.transform_box(BoxCollider {
            min: [-width * 0.5, -depth * 0.5, 0.],
            max: [width * 0.5, depth * 0.5, 0.],
        })
    }
    fn volume(self, mut volume: Volume) -> Volume {
        for plane in &mut volume.planes {
            plane.normal = self.vector(plane.normal);
            plane.distance += plane
                .normal
                .iter()
                .zip(self.centre)
                .map(|(a, b)| a * b)
                .sum::<f32>();
        }
        volume.bounds = self.transform_box(volume.bounds);
        volume
    }
}

pub(super) fn placement(parent: &BuildingRecipe, extension: &ExtensionRecipe) -> Placement {
    let normal = extension.facade.normal();
    let tangent = extension.facade.tangent();
    let overlap = parent.wall_thickness * 2. + 0.04;
    let centre = extension.facade.point(
        parent,
        extension.offset,
        0.,
        -(extension.depth - overlap) * 0.5,
    );
    let lean = extension.kind == ExtensionKind::LeanTo;
    Placement {
        centre,
        x: if lean { normal.map(|v| -v) } else { tangent },
        y: if lean {
            tangent.map(|v| -v)
        } else {
            normal.map(|v| -v)
        },
        normal_depth: extension.depth + overlap,
        lean,
        roof_reach: if normal[0].abs() > 0.5 {
            parent.width * 0.5
        } else {
            parent.depth * 0.5
        },
    }
}

pub(super) struct Node {
    pub recipe: BuildingRecipe,
    pub placement: Placement,
    pub outer: [Volumes; 3],
    pub roof: [Volumes; 3],
    pub foundation: Volume,
    pub wall_volume: Volume,
    pub rooms: Vec<Volume>,
    pub interior_span: Volume,
    pub floors: Vec<Volume>,
    pub timber_rooms: Vec<Volume>,
    pub below_header: Vec<Volume>,
    pub room_bounds: Vec<BoxCollider>,
}

impl Node {
    pub fn new(recipe: BuildingRecipe, placement: Placement, bottom: f32, header: f32) -> Self {
        let outer = std::array::from_fn(|_| Volumes::default());
        let roof = std::array::from_fn(|_| Volumes::default());
        let mut foundation = crate::geometry::foundation::bounds(&recipe);
        foundation.min[2] = bottom;
        let foundation = placement.volume(Volume::bounds(foundation));
        let wall_volume = placement.volume(Volume::bounds(BoxCollider {
            min: [-recipe.width * 0.5, -recipe.depth * 0.5, bottom],
            max: [recipe.width * 0.5, recipe.depth * 0.5, 1000.],
        }));
        let mut room_bounds = Vec::new();
        for storey in 0..recipe.storeys {
            let local = crate::storeys::recipe_for_storey(&recipe, storey);
            let floor = recipe.floor_height(storey);
            let ceiling = crate::geometry::ceiling_height(&local);
            room_bounds.push(placement.transform_box(BoxCollider {
                min: [
                    -recipe.width * 0.5 + recipe.wall_thickness,
                    -recipe.depth * 0.5 + recipe.wall_thickness,
                    floor,
                ],
                max: [
                    recipe.width * 0.5 - recipe.wall_thickness,
                    recipe.depth * 0.5 - recipe.wall_thickness,
                    floor + ceiling,
                ],
            }));
        }
        let rooms = room_bounds.iter().copied().map(Volume::bounds).collect();
        // Attachment framing must stop at the interior wall even where a
        // stairwell exposes the space between a ceiling and the floor above.
        let mut interior_span = room_bounds[0];
        interior_span.min[2] -= recipe.floor_thickness;
        interior_span.max[2] = placement.centre[2]
            + recipe.floor_height(recipe.storeys - 1)
            + crate::geometry::top_storey_ceiling_top(&recipe);
        // Floorboards occupy the complete footprint, including the wall thickness.
        // Room bounds alone would leave two coincident floors at the connection.
        let floors = (0..recipe.storeys)
            .map(|storey| {
                let top = recipe.floor_height(storey);
                placement.volume(Volume::bounds(BoxCollider {
                    min: [
                        -recipe.width * 0.5,
                        -recipe.depth * 0.5,
                        top - recipe.floor_thickness,
                    ],
                    max: [recipe.width * 0.5, recipe.depth * 0.5, top],
                }))
            })
            .collect();
        let timber_rooms = room_bounds
            .iter()
            .copied()
            .map(|mut b| {
                b.min[2] -= recipe.floor_thickness;
                Volume::bounds(b)
            })
            .collect();
        let below_header = room_bounds
            .iter()
            .copied()
            .map(|mut b| {
                b.min[2] -= recipe.floor_thickness;
                b.max[2] = b.max[2].min(header);
                Volume::bounds(b)
            })
            .collect();
        Self {
            recipe,
            placement,
            outer,
            roof,
            foundation,
            wall_volume,
            rooms,
            interior_span: Volume::bounds(interior_span),
            floors,
            timber_rooms,
            below_header,
            room_bounds,
        }
    }
    pub fn set_surfaces(&mut self, lods: &[BuildingLod; 3], bottom: f32) {
        let bounds = self.placement.transform_box(BoxCollider {
            min: [-self.recipe.width * 0.5, -self.recipe.depth * 0.5, bottom],
            max: [self.recipe.width * 0.5, self.recipe.depth * 0.5, 1000.],
        });
        for (level, lod) in lods.iter().enumerate() {
            let soffits = overhang_soffits(lod, &self.recipe, bounds);
            for points in roof_faces(lod, &self.recipe) {
                if let Some(volume) = clip::roof_volume(points, bottom, Some(bounds)) {
                    self.outer[level].items.push(volume);
                }
                if let Some(upper) = clip::roof_volume(points, bottom, None) {
                    self.roof[level].items.extend(
                        soffits
                            .query(upper.bounds)
                            .filter_map(|soffit| upper.intersect(soffit)),
                    );
                }
            }
            self.roof[level].build();
            self.outer[level].build();
        }
    }
    pub fn inner_wall(&self, point: [f32; 3], normal: [f32; 3]) -> bool {
        let p = self.placement.local(point);
        let n = self.placement.local_vector(normal);
        (0..2).any(|i| {
            let half = [self.recipe.width, self.recipe.depth][i] * 0.5 - self.recipe.wall_thickness;
            (p[i].abs() - half).abs() < 0.0001 && p[i] * n[i] < -0.1
        })
    }
}

// Only the roof skin beyond the walls needs an underside limit. Inside, the
// occupied wall envelope extends all the way down to the foundations.
fn overhang_soffits(lod: &BuildingLod, recipe: &BuildingRecipe, walls: BoxCollider) -> Volumes {
    let mut soffits = Volumes::default();
    soffits.items.extend(
        roof_faces(lod, recipe)
            .filter_map(|points| clip::soffit_volume(points, walls.max[2]))
            .filter(|soffit| {
                (0..2).any(|i| {
                    soffit.bounds.min[i] < walls.min[i] || soffit.bounds.max[i] > walls.max[i]
                })
            }),
    );
    soffits.build();
    soffits
}

fn roof_faces<'a>(
    lod: &'a BuildingLod,
    recipe: &'a BuildingRecipe,
) -> impl Iterator<Item = [[f32; 3]; 3]> + 'a {
    lod.parts
        .iter()
        .filter(|part| {
            matches!(
                part.material,
                MaterialSlot::Roof
                    | MaterialSlot::ThatchEnd
                    | MaterialSlot::Atlas
                    | MaterialSlot::Interior
            )
        })
        .flat_map(move |part| {
            let mesh = &part.mesh;
            mesh.indices.chunks_exact(3).filter_map(move |tri| {
                let vertices = [tri[0], tri[1], tri[2]].map(|index| {
                    let index = index as usize;
                    Vertex {
                        point: mesh.positions[index],
                        normal: mesh.normals[index],
                        uv: mesh.uvs[index],
                    }
                });
                super::junction::roof_surface(part.material, &vertices, recipe)
                    .then(|| vertices.map(|v| v.point))
            })
        })
}

pub(super) fn child(
    parent: &BuildingRecipe,
    extension: &ExtensionRecipe,
) -> (BuildingRecipe, Placement) {
    let placement = placement(parent, extension);
    let mut child = parent.clone();
    child.extensions.clear();
    child.partitions.clear();
    child.openings.clear();
    child.opening_generation = None;
    child.chimney = None;
    child.storeys = 1;
    child.staircase = crate::StaircaseRecipe::default();
    child.foundation.stairs = false;
    child.width = if placement.lean {
        placement.normal_depth
    } else {
        extension.width
    };
    child.depth = if placement.lean {
        extension.width
    } else {
        placement.normal_depth
    };
    child.wall_height = extension.wall_height;
    child.roof.rise = extension.roof_rise;
    child.roof.kind = if placement.lean {
        RoofKind::Shed
    } else {
        RoofKind::Gable
    };
    if let Some(panes) = extension.window_panes {
        child.opening_defaults.window_panes = panes;
    }
    if extension.windows {
        add_windows(
            &mut child,
            extension,
            if placement.lean {
                Facade::East
            } else {
                Facade::North
            },
        );
    }
    (child, placement)
}

fn add_windows(child: &mut BuildingRecipe, extension: &ExtensionRecipe, back: Facade) {
    let size = child.opening_defaults.window;
    if size.height + 0.4 > child.wall_height {
        return;
    }
    let sill = size
        .sill
        .min(child.wall_height - size.height - 0.15)
        .max(0.25);
    for facade in Facade::ALL.into_iter().filter(|f| *f != back) {
        let usable = facade.length(child) - child.wall_thickness * 2. - 0.3;
        let count = (1_u8..=8)
            .take_while(|&n| size.width + f32::from(n - 1) * (size.width + 1.) <= usable)
            .last()
            .unwrap_or(0);
        for index in 0..count {
            child.openings.push(OpeningRecipe {
                id: format!("{}-window-{}-{index}", extension.id, facade.index()),
                storey: 0,
                facade,
                kind: OpeningKind::Window,
                offset: -usable * 0.5 + (f32::from(index) + 0.5) * usable / f32::from(count),
                width: size.width,
                height: size.height,
                sill,
                use_default_dimensions: false,
                window_panes: None,
                hinge: crate::HingeSide::Left,
                swing_degrees: 90.,
                door_style: crate::DoorStyle::Plank,
            });
        }
    }
}
