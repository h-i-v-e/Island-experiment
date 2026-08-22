use super::{
    GeologyField, Mesh, NewVertexStencil, Terrain, TessellationResult, Vec2, Vec3,
    sample_mesh_triangle,
};

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SurfaceMaterial {
    pub(super) deposited_depth: Vec<f32>,
    pub(super) bedrock_hardness: Vec<f32>,
}

impl SurfaceMaterial {
    pub(crate) fn empty(vertex_count: usize) -> Self {
        Self {
            deposited_depth: vec![0.0; vertex_count],
            bedrock_hardness: vec![0.0; vertex_count],
        }
    }

    pub(crate) fn initialize_geology(&mut self, mesh: &Mesh, geology: GeologyField) {
        debug_assert_eq!(self.bedrock_hardness.len(), mesh.vertices.len());
        self.bedrock_hardness
            .iter_mut()
            .zip(&mesh.vertices)
            .for_each(|(hardness, vertex)| *hardness = geology.hardness(vertex.truncate()));
    }

    pub(crate) fn depths(&self) -> &[f32] {
        &self.deposited_depth
    }

    pub(crate) fn depths_mut(&mut self) -> &mut [f32] {
        &mut self.deposited_depth
    }

    pub(crate) fn hardnesses(&self) -> &[f32] {
        &self.bedrock_hardness
    }

    pub(super) fn into_tessellated(
        mut self,
        source: &Mesh,
        tessellation: TessellationResult,
    ) -> (Mesh, Self) {
        let old_volume = self.volume(source);
        self.extend_after_tessellation(old_volume, &tessellation.mesh, &tessellation.new_vertices);
        let mut mesh = tessellation.mesh;
        mesh.optimize_surface_triangulation();
        self.rescale_to_volume(&mesh, old_volume);
        (mesh, self)
    }

    pub(crate) fn volume(&self, mesh: &Mesh) -> f64 {
        debug_assert_eq!(self.deposited_depth.len(), mesh.vertices.len());
        mesh.triangles
            .chunks_exact(3)
            .map(|triangle| {
                let [a, b, c] = [
                    mesh.vertices[triangle[0] as usize].truncate(),
                    mesh.vertices[triangle[1] as usize].truncate(),
                    mesh.vertices[triangle[2] as usize].truncate(),
                ];
                let third_area = f64::from((b - a).perp_dot(c - a).abs() / 6.0);
                let depth = triangle
                    .iter()
                    .map(|&vertex| f64::from(self.deposited_depth[vertex as usize].max(0.0)))
                    .sum::<f64>();
                third_area * depth
            })
            .sum()
    }

    pub(crate) fn extend_after_tessellation(
        &mut self,
        old_volume: f64,
        mesh: &Mesh,
        stencils: &[NewVertexStencil],
    ) {
        let old_vertex_count = self.deposited_depth.len();
        self.deposited_depth.reserve(stencils.len());
        self.bedrock_hardness.reserve(stencils.len());
        for stencil in stencils {
            debug_assert_eq!(stencil.vertex as usize, self.deposited_depth.len());
            let count = usize::from(stencil.count);
            debug_assert!((3..=4).contains(&count));
            debug_assert!(
                stencil.surrounding[..count]
                    .iter()
                    .all(|&vertex| (vertex as usize) < old_vertex_count)
            );
            let depth = stencil.surrounding[..count]
                .iter()
                .map(|&vertex| self.deposited_depth[vertex as usize])
                .sum::<f32>()
                / count as f32;
            let hardness = stencil.surrounding[..count]
                .iter()
                .map(|&vertex| self.bedrock_hardness[vertex as usize])
                .sum::<f32>()
                / count as f32;
            self.deposited_depth.push(depth.max(0.0));
            self.bedrock_hardness.push(hardness.clamp(0.0, 1.0));
        }
        debug_assert_eq!(self.deposited_depth.len(), mesh.vertices.len());
        debug_assert_eq!(self.bedrock_hardness.len(), mesh.vertices.len());
        self.rescale_to_volume(mesh, old_volume);
    }

    pub(crate) fn rescale_to_volume(&mut self, mesh: &Mesh, target_volume: f64) {
        debug_assert_eq!(self.deposited_depth.len(), mesh.vertices.len());
        if target_volume <= f64::EPSILON {
            self.deposited_depth.fill(0.0);
            return;
        }
        let provisional = self.volume(mesh);
        if provisional <= f64::EPSILON {
            debug_assert!(false, "positive loose volume vanished during mesh mutation");
            return;
        }
        let scale = (target_volume / provisional) as f32;
        self.deposited_depth
            .iter_mut()
            .for_each(|depth| *depth = (*depth * scale).max(0.0));
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct TerrainMaterialField {
    pub(super) values: Vec<Vec3>,
}

impl TerrainMaterialField {
    pub(super) fn from_surface(
        material: &SurfaceMaterial,
        river_bed: &[bool],
        forced_rock: &[bool],
    ) -> Self {
        debug_assert_eq!(material.hardnesses().len(), material.depths().len());
        debug_assert_eq!(river_bed.len(), material.depths().len());
        debug_assert_eq!(forced_rock.len(), material.depths().len());
        let values = material
            .hardnesses()
            .iter()
            .zip(material.depths())
            .zip(river_bed)
            .zip(forced_rock)
            .map(|(((&hardness, &depth), &is_river_bed), &is_forced_rock)| {
                let cover = (depth / 0.002).clamp(0.0, 1.0);
                let cover = cover * cover * (3.0 - 2.0 * cover);
                if is_river_bed || is_forced_rock {
                    Vec3::X
                } else {
                    Vec3::new(hardness.clamp(0.0, 1.0), cover, 0.0)
                }
            })
            .collect();
        Self { values }
    }

    pub(super) fn sample(&self, terrain: &Terrain, point: Vec2) -> Vec3 {
        sample_mesh_triangle(&terrain.mesh, &terrain.triangle_index, point).map_or_else(
            || {
                let nearest = terrain.triangle_index.nearest_vertex(&terrain.mesh, point);
                self.values[nearest]
            },
            |(triangle, weights)| {
                self.values[triangle[0]] * weights[0]
                    + self.values[triangle[1]] * weights[1]
                    + self.values[triangle[2]] * weights[2]
            },
        )
    }
}

pub(crate) fn projected_vertex_control_areas(mesh: &Mesh) -> Vec<f32> {
    let mut areas = vec![0.0; mesh.vertices.len()];
    for triangle in mesh.triangles.chunks_exact(3) {
        let [a, b, c] = [
            mesh.vertices[triangle[0] as usize].truncate(),
            mesh.vertices[triangle[1] as usize].truncate(),
            mesh.vertices[triangle[2] as usize].truncate(),
        ];
        let share = (b - a).perp_dot(c - a).abs() / 6.0;
        for &vertex in triangle {
            areas[vertex as usize] += share;
        }
    }
    areas
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn river_bed_vertices_are_exported_as_forced_rock() {
        let material = SurfaceMaterial {
            deposited_depth: vec![0.0, 0.002, 0.001],
            bedrock_hardness: vec![0.25, 0.5, 0.75],
        };

        let field = TerrainMaterialField::from_surface(
            &material,
            &[true, false, false],
            &[false, true, false],
        );

        assert_eq!(field.values[0], Vec3::X);
        assert_eq!(field.values[1], Vec3::X);
        assert_eq!(field.values[2], Vec3::new(0.75, 0.5, 0.0));
    }
}
