//! GUI-3
//! A Screen as the cells of a grid: each char with the columns it takes and what sim says it
//! finally is (`sim::paint::Resolved`, made by the same code that writes the terminal's bytes). The
//! window decides nothing about a cell's style; it only lays the resolved cells out, a char each.
use crate::api::*;

/// One char of a row (with the zero-width chars that combine with it) and where it is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cell {
    pub col: u16,
    /// 1 or 2 columns (zero-width chars are in `extra`).
    pub width: u8,
    pub ch: char,
    pub extra: Vec<char>,
    pub r: Resolved,
}

impl Cell {
    pub fn text(&self) -> String {
        std::iter::once(self.ch).chain(self.extra.iter().copied()).collect()
    }

    pub fn end(&self) -> u16 {
        self.col + u16::from(self.width)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GridRow {
    pub cells: Vec<Cell>,
    /// The resolved cell of each column the text covers.
    pub cols: Vec<Resolved>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Grid {
    pub cols: u16,
    pub rows: Vec<GridRow>,
    pub pal: Palette,
}

/// The cells of a screen. `fallback` is the palette until sim has been told one.
pub fn grid(s: &Screen, resolver: &mut Resolver, fallback: &Palette) -> Grid {
    let pal = s.frame.pal.unwrap_or(*fallback);
    let mut rows = Vec::with_capacity(s.frame.rows.len());
    for (i, row) in s.frame.rows.iter().enumerate() {
        let cols = resolver.row(s, i);
        let cs: Vec<char> = row.text.chars().collect();
        let mut out = GridRow { cells: Vec::with_capacity(cs.len()), cols: vec![] };
        let mut col = 0usize;
        for (k, &c) in cs.iter().enumerate() {
            // the core's own rule for the columns of a char (VIS-7)
            let w = cell_width(&cs, k, col);
            if w == 0 {
                if let Some(prev) = out.cells.last_mut() {
                    prev.extra.push(c);
                }
                continue;
            }
            if let Some(r) = cols.get(col) {
                out.cells.push(Cell {
                    col: col as u16,
                    width: w as u8,
                    ch: c,
                    extra: vec![],
                    r: *r,
                });
            }
            col += w;
        }
        out.cols = cols;
        rows.push(out);
    }
    Grid { cols: s.frame.width, rows, pal }
}
