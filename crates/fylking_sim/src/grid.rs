//! Uniform spatial grid for neighbour queries, rebuilt every tick.
//!
//! Counting-sort layout: one flat array of soldier indices, sorted by cell,
//! plus an offset table. Rebuild is O(n) with no per-cell allocations, so a few
//! thousand soldiers are cheap even on modest hardware.

use crate::math::Vec2;

#[derive(Debug, Clone)]
pub struct SpatialGrid {
    cell: f32,
    origin: Vec2,
    cols: i32,
    rows: i32,
    /// cell_start[c]..cell_start[c+1] indexes into `items` for cell c.
    cell_start: Vec<u32>,
    items: Vec<u32>,
    scratch: Vec<u32>,
}

impl SpatialGrid {
    /// A grid covering a `width` x `height` area centred on the origin, with
    /// generous margin so routing soldiers near the edge still register.
    pub fn new(width: f32, height: f32, cell: f32) -> Self {
        let margin = 40.0;
        let w = width + 2.0 * margin;
        let h = height + 2.0 * margin;
        let cols = (w / cell).ceil() as i32;
        let rows = (h / cell).ceil() as i32;
        SpatialGrid {
            cell,
            origin: Vec2::new(-w / 2.0, -h / 2.0),
            cols,
            rows,
            cell_start: vec![0; (cols * rows + 1) as usize],
            items: Vec::new(),
            scratch: Vec::new(),
        }
    }

    fn cell_coords(&self, p: Vec2) -> (i32, i32) {
        let cx = ((p.x - self.origin.x) / self.cell).floor() as i32;
        let cy = ((p.y - self.origin.y) / self.cell).floor() as i32;
        (cx.clamp(0, self.cols - 1), cy.clamp(0, self.rows - 1))
    }

    fn cell_index(&self, p: Vec2) -> usize {
        let (cx, cy) = self.cell_coords(p);
        (cy * self.cols + cx) as usize
    }

    /// Rebuild from `(index, position)` pairs.
    pub fn rebuild(&mut self, entries: impl Iterator<Item = (usize, Vec2)>) {
        self.cell_start.iter_mut().for_each(|c| *c = 0);
        self.scratch.clear();
        // scratch holds (cell, index) pairs flattened.
        for (i, p) in entries {
            let c = self.cell_index(p) as u32;
            self.cell_start[c as usize + 1] += 1;
            self.scratch.push(c);
            self.scratch.push(i as u32);
        }
        for c in 1..self.cell_start.len() {
            self.cell_start[c] += self.cell_start[c - 1];
        }
        let n = self.scratch.len() / 2;
        self.items.clear();
        self.items.resize(n, 0);
        // Reuse a cursor per cell; cell_start is restored afterwards.
        let mut cursor: Vec<u32> = self.cell_start[..self.cell_start.len() - 1].to_vec();
        for k in 0..n {
            let c = self.scratch[2 * k] as usize;
            let i = self.scratch[2 * k + 1];
            self.items[cursor[c] as usize] = i;
            cursor[c] += 1;
        }
    }

    /// Call `f` for every indexed item in cells overlapping the circle.
    /// Callers still need to check exact distance.
    pub fn for_each_near(&self, p: Vec2, radius: f32, mut f: impl FnMut(usize)) {
        let (x0, y0) = self.cell_coords(Vec2::new(p.x - radius, p.y - radius));
        let (x1, y1) = self.cell_coords(Vec2::new(p.x + radius, p.y + radius));
        for cy in y0..=y1 {
            let row = cy * self.cols;
            let start = self.cell_start[(row + x0) as usize] as usize;
            let end = self.cell_start[(row + x1 + 1) as usize] as usize;
            // Cells in a row are contiguous, so one slice covers x0..=x1.
            for &i in &self.items[start..end] {
                f(i as usize);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_neighbours_and_skips_far_ones() {
        let mut g = SpatialGrid::new(100.0, 100.0, 2.0);
        let pts = [Vec2::new(0.0, 0.0), Vec2::new(1.0, 0.5), Vec2::new(30.0, 30.0)];
        g.rebuild(pts.iter().copied().enumerate());
        let mut found = Vec::new();
        g.for_each_near(Vec2::new(0.2, 0.2), 1.5, |i| found.push(i));
        assert!(found.contains(&0) && found.contains(&1));
        assert!(!found.contains(&2));
    }
}
