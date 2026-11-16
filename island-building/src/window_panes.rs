//! Shared rectangular glazing layout for near joinery, middle proxies and the atlas.
use crate::{BuildingRecipe, OpeningRecipe};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct WindowPanes {
    pub columns: u8,
    pub rows: u8,
}

impl Default for WindowPanes {
    fn default() -> Self {
        Self {
            columns: 2,
            rows: 2,
        }
    }
}

impl WindowPanes {
    pub const MAX_PER_AXIS: u8 = 32;

    pub(crate) fn is_valid(self) -> bool {
        (1..=Self::MAX_PER_AXIS).contains(&self.columns)
            && (1..=Self::MAX_PER_AXIS).contains(&self.rows)
    }
}

pub(crate) struct PaneGrid {
    pub left: f32,
    pub right: f32,
    pub bottom: f32,
    pub top: f32,
    panes: WindowPanes,
    half_width: f32,
    half_height: f32,
}

impl PaneGrid {
    pub fn new(recipe: &BuildingRecipe, opening: &OpeningRecipe) -> Self {
        let panes = opening
            .window_panes
            .unwrap_or(recipe.opening_defaults.window_panes);
        let width = opening.width - 0.11;
        let height = opening.height - 0.11;
        Self {
            left: opening.offset - width * 0.5,
            right: opening.offset + width * 0.5,
            bottom: opening.sill + 0.055,
            top: opening.sill + opening.height - 0.055,
            panes,
            half_width: 0.018_f32.min(width / f32::from(panes.columns) * 0.12),
            half_height: 0.014_f32.min(height / f32::from(panes.rows) * 0.12),
        }
    }

    fn column(&self, index: u8) -> f32 {
        self.left + (self.right - self.left) * f32::from(index) / f32::from(self.panes.columns)
    }

    fn row(&self, index: u8) -> f32 {
        self.bottom + (self.top - self.bottom) * f32::from(index) / f32::from(self.panes.rows)
    }

    /// Horizontal bars stop at vertical bars, avoiding intersecting timber faces.
    pub fn bars(&self) -> impl Iterator<Item = ([f32; 2], [f32; 2])> + '_ {
        let vertical = (1..self.panes.columns).map(move |column| {
            let centre = self.column(column);
            (
                [centre - self.half_width, centre + self.half_width],
                [self.bottom, self.top],
            )
        });
        let horizontal = (1..self.panes.rows).flat_map(move |row| {
            let centre = self.row(row);
            (0..self.panes.columns).map(move |column| {
                let left = self.column(column) + if column == 0 { 0. } else { self.half_width };
                let right = self.column(column + 1)
                    - if column + 1 == self.panes.columns {
                        0.
                    } else {
                        self.half_width
                    };
                (
                    [left, right],
                    [centre - self.half_height, centre + self.half_height],
                )
            })
        });
        vertical.chain(horizontal)
    }

    pub fn solid_at(&self, [x, z]: [f32; 2]) -> bool {
        if x <= self.left || x >= self.right || z <= self.bottom || z >= self.top {
            return true;
        }
        for (value, start, end, count, half) in [
            (
                x,
                self.left,
                self.right,
                self.panes.columns,
                self.half_width,
            ),
            (z, self.bottom, self.top, self.panes.rows, self.half_height),
        ] {
            let spacing = (end - start) / f32::from(count);
            let nearest = ((value - start) / spacing).round();
            if nearest >= 1.
                && nearest < f32::from(count)
                && (value - start - nearest * spacing).abs() <= half
            {
                return true;
            }
        }
        false
    }
}
