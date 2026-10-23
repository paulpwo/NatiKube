//! Pixel <-> cell math, independent of GPUI types.

use alacritty_terminal::index::Side;

/// Size of the cell grid and of one cell, in logical pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GridMetrics {
    pub cell_width: f32,
    pub cell_height: f32,
    pub cols: usize,
    pub rows: usize,
}

impl GridMetrics {
    /// Derives the grid that fits a viewport. Never smaller than 2x1 so the
    /// emulator always has a valid grid.
    pub fn fit(width: f32, height: f32, cell_width: f32, cell_height: f32) -> Self {
        let cols = if cell_width > 0.0 {
            (width / cell_width).floor() as usize
        } else {
            0
        };
        let rows = if cell_height > 0.0 {
            (height / cell_height).floor() as usize
        } else {
            0
        };
        Self {
            cell_width,
            cell_height,
            cols: cols.max(2),
            rows: rows.max(1),
        }
    }

    /// Maps a position relative to the grid origin to `(row, col, side)`,
    /// clamped to the grid. `side` is the half of the cell that was hit.
    pub fn cell_at(&self, x: f32, y: f32) -> (usize, usize, Side) {
        let col_f = (x / self.cell_width).max(0.0);
        let row_f = (y / self.cell_height).max(0.0);
        let col = (col_f.floor() as usize).min(self.cols - 1);
        let row = (row_f.floor() as usize).min(self.rows - 1);
        let within = x - col as f32 * self.cell_width;
        let side = if within > self.cell_width / 2.0 && x >= 0.0 {
            Side::Right
        } else {
            Side::Left
        };
        (row, col, side)
    }
}

/// Converts pixel scroll deltas (trackpads) into whole lines, carrying the
/// remainder so slow scrolling still moves.
#[derive(Debug, Default, Clone)]
pub struct ScrollAccumulator {
    remainder: f32,
}

impl ScrollAccumulator {
    /// Adds a delta in pixels (positive = content moves down / scroll up in
    /// history) and returns the whole lines to scroll.
    pub fn push(&mut self, delta_px: f32, line_height: f32) -> i32 {
        if line_height <= 0.0 {
            return 0;
        }
        self.remainder += delta_px / line_height;
        let lines = self.remainder.trunc();
        self.remainder -= lines;
        lines as i32
    }

    pub fn reset(&mut self) {
        self.remainder = 0.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fit_floors_and_clamps() {
        let m = GridMetrics::fit(805.0, 400.0, 8.0, 16.0);
        assert_eq!((m.cols, m.rows), (100, 25));
        let tiny = GridMetrics::fit(1.0, 1.0, 8.0, 16.0);
        assert_eq!((tiny.cols, tiny.rows), (2, 1));
    }

    #[test]
    fn cell_at_maps_sides_and_clamps() {
        let m = GridMetrics::fit(80.0, 32.0, 8.0, 16.0);
        assert_eq!(m.cell_at(1.0, 1.0), (0, 0, Side::Left));
        assert_eq!(m.cell_at(7.0, 17.0), (1, 0, Side::Right));
        assert_eq!(m.cell_at(-5.0, -5.0), (0, 0, Side::Left));
        let (row, col, _) = m.cell_at(1000.0, 1000.0);
        assert_eq!((row, col), (1, 9));
    }

    #[test]
    fn scroll_accumulator_carries_remainder() {
        let mut acc = ScrollAccumulator::default();
        assert_eq!(acc.push(6.0, 16.0), 0);
        assert_eq!(acc.push(6.0, 16.0), 0);
        assert_eq!(acc.push(6.0, 16.0), 1);
        assert_eq!(acc.push(-40.0, 16.0), -2);
    }
}
