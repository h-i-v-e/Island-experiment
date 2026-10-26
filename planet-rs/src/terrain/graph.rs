use crate::{DVec3, PlanetMesh};

#[derive(Debug)]
pub(super) struct SurfaceGraph {
    pub neighbours: Vec<Vec<usize>>,
    /// Unit-sphere solid angle allocated to each site; sum is 4*pi.
    pub areas: Vec<f64>,
}

impl SurfaceGraph {
    pub fn new(mesh: &PlanetMesh) -> Self {
        let mut neighbours = vec![Vec::new(); mesh.directions.len()];
        let mut areas = vec![0.0; mesh.directions.len()];
        for face in &mesh.triangles {
            let [a, b, c] = face.map(|i| i as usize);
            let [pa, pb, pc] = [a, b, c].map(|i| mesh.directions[i]);
            let area = 2.0
                * pa.dot(pb.cross(pc))
                    .abs()
                    .atan2(1.0 + pa.dot(pb) + pb.dot(pc) + pc.dot(pa));
            for (vertex, left, right) in [(a, b, c), (b, c, a), (c, a, b)] {
                neighbours[vertex].extend([left, right]);
                areas[vertex] += area / 3.0;
            }
        }
        for neighbours in &mut neighbours {
            neighbours.sort_unstable();
            neighbours.dedup();
        }
        Self { neighbours, areas }
    }

    pub fn steepest(&self, directions: &[DVec3], heights: &[f64], source: usize) -> Option<usize> {
        self.neighbours[source]
            .iter()
            .copied()
            .filter(|&target| heights[target] < heights[source])
            .max_by(|&a, &b| {
                let slope =
                    |i| (heights[source] - heights[i]) / arc(directions[source], directions[i]);
                slope(a).total_cmp(&slope(b)).then_with(|| b.cmp(&a))
            })
    }
}

pub(super) fn arc(a: DVec3, b: DVec3) -> f64 {
    a.cross(b).length().atan2(a.dot(b))
}
