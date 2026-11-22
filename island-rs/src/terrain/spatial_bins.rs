use crate::{ISLAND_WORLD_METRES, Vec2};

const GRID: usize = 96;

/// Bins for bounds in island-local XY metres.
#[derive(Debug)]
pub(super) struct SpatialBins<T> {
    items: Vec<T>,
    cells: Vec<Vec<usize>>,
}

impl<T> SpatialBins<T> {
    pub(super) fn new() -> Self {
        Self {
            items: Vec::new(),
            cells: vec![Vec::new(); GRID * GRID],
        }
    }

    fn cells(min: Vec2, max: Vec2) -> impl Iterator<Item = usize> {
        let bin = |v: f32| ((v / ISLAND_WORLD_METRES * GRID as f32) as usize).min(GRID - 1);
        let (x0, x1) = (bin(min.x), bin(max.x));
        (bin(min.y)..=bin(max.y)).flat_map(move |y| (x0..=x1).map(move |x| y * GRID + x))
    }

    pub(super) fn insert(&mut self, min: Vec2, max: Vec2, item: T) {
        let index = self.items.len();
        self.items.push(item);
        for cell in Self::cells(min, max) {
            self.cells[cell].push(index);
        }
    }

    pub(super) fn nearby(&self, min: Vec2, max: Vec2) -> impl Iterator<Item = &T> {
        Self::cells(min, max).flat_map(|cell| self.cells[cell].iter().map(|&id| &self.items[id]))
    }
}
