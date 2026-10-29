use crate::{
    DVec3,
    ocean_surface::{cube_direction, cube_pixel},
};
use std::collections::BTreeSet;

#[derive(Clone, Debug)]
pub(super) struct Cell {
    pub direction: DVec3,
    pub area: f64,
    pub neighbours: [usize; 4],
}

#[derive(Clone, Copy, Debug)]
pub(super) struct Edge {
    pub a: usize,
    pub b: usize,
    pub width: f64,
    pub tangent: DVec3,
}

#[derive(Clone, Debug)]
pub(super) struct Grid {
    pub size: u32,
    pub cells: Vec<Cell>,
    pub edges: Vec<Edge>,
    /// A geodesic optical footprint, independent of the cube's row/column axes.
    pub optical_ring: Vec<[usize; 8]>,
}

pub(super) fn tangent(vector: DVec3, normal: DVec3) -> DVec3 {
    vector - normal * vector.dot(normal)
}

pub(super) fn basis(normal: DVec3) -> (DVec3, DVec3) {
    let east = DVec3::Y.cross(normal).try_normalize().unwrap_or(DVec3::X);
    (east, normal.cross(east))
}

pub(super) fn parallel_transport(vector: DVec3, from: DVec3, to: DVec3) -> DVec3 {
    vector - (from + to) * (vector.dot(to) / (1.0 + from.dot(to)).max(1.0e-8))
}

fn triangle_area(a: DVec3, b: DVec3, c: DVec3) -> f64 {
    2.0 * a
        .dot(b.cross(c))
        .abs()
        .atan2(1.0 + a.dot(b) + b.dot(c) + c.dot(a))
}

impl Grid {
    pub fn new(size: u32) -> Self {
        let width = size as usize;
        let count = 6 * width * width;
        let mut cells = Vec::with_capacity(count);
        let uv = |pixel: f64| pixel.mul_add(2.0 / f64::from(size), -1.0);
        for index in 0..count {
            let face = index / (width * width);
            let x = (index % width) as f64;
            let y = ((index / width) % width) as f64;
            let direction = cube_direction(face, uv(x + 0.5), uv(y + 0.5));
            let corners = [(x, y), (x + 1.0, y), (x + 1.0, y + 1.0), (x, y + 1.0)]
                .map(|(u, v)| cube_direction(face, uv(u), uv(v)));
            let area = triangle_area(corners[0], corners[1], corners[2])
                + triangle_area(corners[0], corners[2], corners[3]);
            let neighbours = [
                (x - 0.5, y + 0.5),
                (x + 1.5, y + 0.5),
                (x + 0.5, y - 0.5),
                (x + 0.5, y + 1.5),
            ]
            .map(|(u, v)| Self::index_for(cube_direction(face, uv(u), uv(v)), size));
            cells.push(Cell {
                direction,
                area,
                neighbours,
            });
        }
        let pairs: BTreeSet<_> = cells
            .iter()
            .enumerate()
            .flat_map(|(a, c)| c.neighbours.map(move |b| (a.min(b), a.max(b))))
            .filter(|(a, b)| a != b)
            .collect();
        let edges = pairs
            .into_iter()
            .map(|(a, b)| {
                let midpoint = (cells[a].direction + cells[b].direction).normalize();
                Edge {
                    a,
                    b,
                    width: cells[a].area.min(cells[b].area).sqrt(),
                    tangent: tangent(cells[b].direction - cells[a].direction, midpoint).normalize(),
                }
            })
            .collect();
        let optical_ring = cells
            .iter()
            .map(|cell| {
                let (east, north) = basis(cell.direction);
                let (sin, cos) = (cell.area.sqrt() * 0.9).sin_cos();
                let diagonal = std::f64::consts::FRAC_1_SQRT_2;
                [
                    (1.0, 0.0),
                    (diagonal, diagonal),
                    (0.0, 1.0),
                    (-diagonal, diagonal),
                    (-1.0, 0.0),
                    (-diagonal, -diagonal),
                    (0.0, -1.0),
                    (diagonal, -diagonal),
                ]
                .map(|(x, y)| {
                    Self::index_for(cell.direction * cos + (east * x + north * y) * sin, size)
                })
            })
            .collect();
        Self {
            size,
            cells,
            edges,
            optical_ring,
        }
    }
    pub fn index_for(direction: DVec3, size: u32) -> usize {
        let (face, x, y) = cube_pixel(direction, size);
        (face * size as usize + y as usize) * size as usize + x as usize
    }
    pub fn index(&self, direction: DVec3) -> usize {
        Self::index_for(direction, self.size)
    }
}
