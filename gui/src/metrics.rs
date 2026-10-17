//! GUI-10, GUI-16
//! Where the cells are in the window: the size of a cell, the margin around the grid, and the
//! device scale. Pure numbers; the fonts decide them (`font.rs`), the renderer and the input
//! adapter use them.

/// Pixels of the window that a grid of cells stands on. All in physical pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Metrics {
    pub cell_w: f32,
    pub cell_h: f32,
    /// The distance from the top of a cell to the baseline of its text.
    pub baseline: f32,
    pub font_px: f32,
    /// The margin around the grid: room for the shadows of the boxes.
    pub pad_x: f32,
    pub pad_y: f32,
    /// Physical pixels per device-independent pixel: every size in dp is multiplied by it.
    pub scale: f32,
}

impl Metrics {
    /// A device-independent length in physical pixels.
    pub fn dp(&self, v: f32) -> f32 {
        (v * self.scale).round().max(1.0)
    }

    /// The same, not rounded (for a shadow's blur and offset).
    pub fn dpf(&self, v: f32) -> f32 {
        v * self.scale
    }

    /// How many columns and rows of cells a window of this size holds (at least 1 each).
    pub fn grid_for(&self, width: u32, height: u32) -> (u16, u16) {
        let cols = ((width as f32 - 2.0 * self.pad_x) / self.cell_w).floor();
        let rows = ((height as f32 - 2.0 * self.pad_y) / self.cell_h).floor();
        (cols.clamp(1.0, f32::from(u16::MAX)) as u16, rows.clamp(1.0, f32::from(u16::MAX)) as u16)
    }

    /// The top-left pixel of a cell.
    pub fn origin(&self, col: f32, row: f32) -> (f32, f32) {
        (self.pad_x + col * self.cell_w, self.pad_y + row * self.cell_h)
    }

    /// The cell under a pixel, clamped to the grid (a terminal reports no position outside it).
    pub fn cell_at(&self, x: f64, y: f64, cols: u16, rows: u16) -> (u16, u16) {
        let c = ((x - f64::from(self.pad_x)) / f64::from(self.cell_w)).floor();
        let r = ((y - f64::from(self.pad_y)) / f64::from(self.cell_h)).floor();
        let clamp = |v: f64, n: u16| v.clamp(0.0, f64::from(n.saturating_sub(1))) as u16;
        (clamp(c, cols), clamp(r, rows))
    }
}

/// A made-up set of metrics for tests and the scene golden: whole numbers, so every position in a
/// dump is exact.
pub const TEST: Metrics = Metrics {
    cell_w: 10.0,
    cell_h: 20.0,
    baseline: 15.0,
    font_px: 14.0,
    pad_x: 16.0,
    pad_y: 16.0,
    scale: 1.0,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gui_16_a_pixel_is_in_the_cell_it_lies_in() {
        let m = TEST;
        assert_eq!(m.cell_at(16.0, 16.0, 80, 24), (0, 0));
        assert_eq!(m.cell_at(16.0 + 3.5 * 10.0, 16.0 + 2.2 * 20.0, 80, 24), (3, 2));
        assert_eq!(m.cell_at(16.0 + 10.0 - 0.01, 16.0, 80, 24), (0, 0));
        assert_eq!(m.cell_at(16.0 + 10.0, 16.0, 80, 24), (1, 0));
    }

    #[test]
    fn gui_16_the_margin_and_the_far_side_clamp_to_the_grid() {
        let m = TEST;
        assert_eq!(m.cell_at(0.0, 0.0, 80, 24), (0, 0));
        assert_eq!(m.cell_at(5000.0, 5000.0, 80, 24), (79, 23));
        assert_eq!(m.cell_at(-10.0, 100.0, 80, 24).0, 0);
    }

    #[test]
    fn gui_16_the_grid_fits_inside_the_margins() {
        let m = TEST;
        assert_eq!(m.grid_for(832, 512), (80, 24));
        assert_eq!(m.grid_for(10, 10), (1, 1));
    }

    #[test]
    fn gui_16_dp_scales_and_never_vanishes() {
        let m = Metrics { scale: 2.0, ..TEST };
        assert_eq!(m.dp(1.0), 2.0);
        assert_eq!(Metrics { scale: 0.1, ..TEST }.dp(1.0), 1.0);
    }
}
