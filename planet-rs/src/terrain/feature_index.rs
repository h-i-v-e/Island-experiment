//! Conservative spherical-cap lookup in fixed Cartesian cells. No per-query
//! allocations, latitude seams, or dependence on the evolving mesh density.
use crate::DVec3;
const SIDE: usize = 32;
const CELLS: usize = SIDE * SIDE * SIDE;

pub(super) struct FeatureIndex {
    cells: Vec<Vec<usize>>,
}
impl FeatureIndex {
    pub fn new(caps: impl IntoIterator<Item = (DVec3, f64)>) -> Self {
        let mut caps = caps.into_iter().enumerate().peekable();
        if caps.peek().is_none() {
            return Self { cells: Vec::new() };
        }
        let mut cells = (0..CELLS).map(|_| Vec::new()).collect::<Vec<_>>();
        for (id, (centre, angle)) in caps {
            let radius = if angle >= std::f64::consts::PI {
                2.0
            } else {
                2.0 * (angle * 0.5).sin()
            };
            for cell in cell_range(centre, radius) {
                cells[cell].push(id);
            }
        }
        Self { cells }
    }
    pub fn at(&self, point: DVec3) -> &[usize] {
        if self.cells.is_empty() {
            return &[];
        }
        &self.cells[index(key(point))]
    }
    pub fn any_near(
        &self,
        centre: DVec3,
        radius: f64,
        mut predicate: impl FnMut(usize) -> bool,
    ) -> bool {
        !self.cells.is_empty()
            && cell_range(centre, radius)
                .any(|cell| self.cells[cell].iter().any(|&id| predicate(id)))
    }
}
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // Clamped fixed-grid coordinates.
fn key(p: DVec3) -> [usize; 3] {
    p.to_array()
        .map(|v| ((v + 1.0) * 16.0).floor().clamp(0.0, 31.0) as usize)
}
const fn index([x, y, z]: [usize; 3]) -> usize {
    (x * SIDE + y) * SIDE + z
}
fn cell_range(centre: DVec3, radius: f64) -> impl Iterator<Item = usize> {
    let lo = key(centre - DVec3::splat(radius));
    let hi = key(centre + DVec3::splat(radius));
    (lo[0]..=hi[0]).flat_map(move |x| {
        (lo[1]..=hi[1]).flat_map(move |y| (lo[2]..=hi[2]).map(move |z| index([x, y, z])))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn lookup_preserves_caps_across_cell_edges_poles_and_large_faces() {
        let caps = [
            (DVec3::Y, 0.1),
            (DVec3::new(0.5, 0.5, 0.5).normalize(), 0.3),
            (-DVec3::X, 0.02),
        ];
        let grid = FeatureIndex::new(caps);
        for (i, (p, r)) in caps.into_iter().enumerate() {
            let a = p.any_orthonormal_vector();
            let b = p.cross(a);
            for j in 0..32 {
                let angle = f64::from(j) * std::f64::consts::TAU / 32.0;
                let q = p * r.cos() + (a * angle.cos() + b * angle.sin()) * r.sin();
                assert!(grid.at(q).contains(&i));
                assert!(grid.any_near(q, 0.01, |id| id == i));
            }
        }
        assert!(grid.any_near(DVec3::ZERO, 2.0, |id| id == 2));
        assert!(!grid.at(-DVec3::Z).contains(&0));
    }
}
