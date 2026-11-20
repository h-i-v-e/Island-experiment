use super::{ISLAND_WORLD_METRES, Island, Mesh, Vec2};

impl Island {
    /// Candidate cart-track crossings of a coastal strip's left and right edges,
    /// in native local metres. The town grader preserves the seam collar.
    /// Neighbouring tracks meet on dry ground here. A bridge requires both
    /// banks and their approaches in one section.
    #[must_use]
    pub fn coastal_track_portals(&self) -> [Option<Vec2>; 2] {
        // Coastal generation retains the complete pinned seam in every LOD.
        let mesh = &self.coarser_lods[1];
        [0.0, 1.0].map(|edge_x| track_portal(mesh, edge_x))
    }
}

fn track_portal(mesh: &Mesh, edge_x: f32) -> Option<Vec2> {
    let edge = mesh
        .vertices
        .iter()
        .zip(&mesh.normals)
        .filter(|(p, _)| p.x.to_bits() == edge_x.to_bits());
    // Choose only the inland-connected dry stretch of the seam. A low
    // offshore islet can otherwise look easier to reach than the headland.
    let last_water = edge
        .clone()
        .filter(|(p, _)| p.z <= 0.0)
        .map(|(p, _)| p.y * ISLAND_WORLD_METRES)
        .fold(0.0, f32::max);

    edge.filter_map(|(p, n)| {
        let y = p.y * ISLAND_WORLD_METRES;
        let height = p.z * ISLAND_WORLD_METRES;
        if y < last_water + 8.0 || y > ISLAND_WORLD_METRES - 8.0 || height < 2.0 || n.z <= 0.0 {
            return None;
        }
        // The wheels cross the seam along X. Both edge representations carry
        // exactly the same periodic-field normal, unlike one-sided triangles.
        let grade = n.x.abs() / n.z;
        let crossfall = n.y.abs() / n.z;
        if grade > 0.60 || crossfall > 0.46 {
            return None;
        }
        let cost = (height - 12.0).abs() + 120.0 * grade + 100.0 * crossfall;
        Some((cost, y))
    })
    .min_by(|a, b| a.0.total_cmp(&b.0).then_with(|| a.1.total_cmp(&b.1)))
    .map(|(_, y)| Vec2::new(edge_x * ISLAND_WORLD_METRES, y))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terrain::coastal_strip::CoastalStripField;
    use crate::{CoastalStripOptions, IslandOptions, Terrain, Vec3};

    #[test]
    fn low_offshore_spots_cannot_replace_an_impassable_headland() {
        let mut mesh = Mesh {
            vertices: vec![
                Vec3::new(0.0, 0.35, 0.006),
                Vec3::new(0.0, 0.50, -0.001),
                Vec3::new(0.0, 0.80, 0.040),
            ],
            normals: vec![Vec3::Z, Vec3::Z, Vec3::new(0.0, 0.8, 0.6)],
            ..Mesh::default()
        };
        assert_eq!(track_portal(&mesh, 0.0), None);
        mesh.normals[2] = Vec3::Z;
        assert_eq!(
            track_portal(&mesh, 0.0),
            Some(Vec2::new(0.0, 0.80 * ISLAND_WORLD_METRES))
        );
    }

    #[test]
    fn adjacent_and_wrapped_sections_choose_identical_cart_track_crossings() {
        let count = 8;
        let sections: Vec<_> = (0..count)
            .map(|section_index| {
                // Use the same pinned edge and canonical normal calculation as
                // final coastal generation, without generating forests and rivers.
                let field = CoastalStripField {
                    seed: 42,
                    terrain: IslandOptions {
                        terrain_size: 32,
                        water_ratio: 0.8,
                        ..IslandOptions::default()
                    },
                    options: CoastalStripOptions {
                        section_index,
                        section_count: count,
                        ..CoastalStripOptions::default()
                    },
                };
                let (base, _) = field.initial();
                let mut mesh = field.tessellate(&base).mesh;
                field.apply_boundary_normals(&mut mesh);
                Terrain::new(mesh)
            })
            .collect();
        for (left, right) in sections.iter().zip(sections.iter().cycle().skip(1)) {
            let exit = track_portal(left.mesh(), 1.0).expect("dry right crossing");
            let entry = track_portal(right.mesh(), 0.0).expect("dry left crossing");
            assert_eq!(exit, entry + Vec2::X * ISLAND_WORLD_METRES);
            let height = |terrain: &Terrain, p: Vec2| {
                terrain
                    .sample_accurate_surface(p / ISLAND_WORLD_METRES)
                    .expect("crossing on the coastal terrain")
                    .position
                    .z
                    * ISLAND_WORLD_METRES
            };
            let left_height = height(left, exit);
            assert!(left_height >= 2.0);
            assert!((left_height - height(right, entry)).abs() < 0.01);
        }
    }
}
