//! GUI-1, GUI-2, GUI-3, GUI-9, GUI-12
//! From sim's screen to what is drawn: a display list of surfaces (rounded rectangles with a shadow,
//! a rim, a fill) and glyphs, in layers. This is where sim's layers become elevations: the boxes of
//! the tree and of the text are raised from the floor; a heading's band, a fence's band, a chip (the
//! shown tab, a badge) and the alert's pill sit on the page; the windows float above all of it, the
//! modal one over a dimmed scrim. What each thing is, sim says: the layers of the Screen (`Plane`)
//! and the role of each paint (`Role`); nothing here infers it from a geometry or from a color.
//! Pure: no GPU and no fonts, so a scene can be dumped as text and kept as a golden.
use crate::api::*;
use crate::blocks::{self, Piece};
use crate::cells::{Cell, Grid, GridRow};
use crate::depth::{Color, Depth, Level, Shadow, rgb, rgba};
use crate::metrics::Metrics;
use std::fmt::Write as _;
use std::ops::Range;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    pub fn new(x: f32, y: f32, w: f32, h: f32) -> Rect {
        Rect { x, y, w, h }
    }

    pub fn right(&self) -> f32 {
        self.x + self.w
    }

    pub fn bottom(&self) -> f32 {
        self.y + self.h
    }

    pub fn inset(&self, dx: f32, dy: f32) -> Rect {
        Rect::new(
            self.x + dx,
            self.y + dy,
            (self.w - 2.0 * dx).max(0.0),
            (self.h - 2.0 * dy).max(0.0),
        )
    }
}

/// What a surface is (for the dump, and for the renderer's choices): sim's roles, each with an
/// elevation and a shape of its own here.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// The box of the tree or of the text.
    Box,
    /// A fenced block's band.
    Band,
    /// A heading's band.
    Heading,
    /// A short label raised from the page: the shown tab, a badge, the open file's row, a block of the status line.
    Chip,
    /// A patch behind words: a change, a match, a yank, inline code.
    Patch,
    /// A flat tint across a row.
    Tint,
    /// The Visual selection.
    Select,
    /// The alert's pill: a char shown in another form, text in the alert's ink.
    Pill,
    /// A line or a bar drawn for a box-drawing or block char; an underline; the scrollbar's groove.
    Shape,
    /// The scrollbar's handle.
    Thumb,
    /// A window over the rest.
    Float,
    /// A bar across a row: the tree's header, the tab bar, the status line.
    Strip,
    Plain,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Surface {
    pub rect: Rect,
    pub radius: f32,
    pub fill: Color,
    pub border: Option<(Color, f32)>,
    pub shadow: Option<Shadow>,
    /// A light edge at the top and a dark one at the bottom, as fractions of the fill's way to white and black.
    pub sheen: (f32, f32),
    pub kind: Kind,
    pub clip: Option<Rect>,
}

impl Surface {
    pub fn flat(rect: Rect, fill: Color, kind: Kind) -> Surface {
        Surface {
            rect,
            radius: 0.0,
            fill,
            border: None,
            shadow: None,
            sheen: (0.0, 0.0),
            kind,
            clip: None,
        }
    }
}

/// A char (and what combines with it) to be drawn at a cell. `x` and `y` are the cell's top-left.
#[derive(Clone, Debug, PartialEq)]
pub struct Glyph {
    pub x: f32,
    pub y: f32,
    pub ch: char,
    pub extra: Vec<char>,
    pub cols: u8,
    pub color: Color,
    pub bold: bool,
    pub italic: bool,
    pub clip: Option<Rect>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Plane {
    pub surfaces: Vec<Surface>,
    pub glyphs: Vec<Glyph>,
}

/// Where sim's cursor is and how it looks; the renderer moves it smoothly from where it was.
#[derive(Clone, Debug, PartialEq)]
pub struct CursorSpec {
    pub rect: Rect,
    pub shape: Shape,
    pub color: Color,
    /// The char under it, drawn inverted inside a block.
    pub under: Option<Glyph>,
    /// The cursor is on the scrolling part of the text.
    pub in_body: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Scene {
    pub size: (u32, u32),
    /// The size of a cell, in pixels.
    pub cell: (f32, f32),
    pub floor: Color,
    pub page: Color,
    /// The page's layer first, then each window, the highest last.
    pub layers: Vec<Plane>,
    /// Over the layers before `.0`: the content behind a modal window, dimmed.
    pub scrim: Option<(usize, Color)>,
    pub cursor: Option<CursorSpec>,
    /// The part of the text that scrolls, and the rows of the screen it is.
    pub body: Option<(Rect, Range<usize>)>,
}

/// The part of the old screen that still shows while the text scrolls: its rows, `dy` pixels
/// from where they were.
pub struct Ghosts<'a> {
    pub grid: &'a Grid,
    pub rows: Range<usize>,
    pub dy: f32,
}

/// What the scene is made from.
pub struct View<'a> {
    pub grid: &'a Grid,
    pub tones: &'a Tones,
    pub layers: &'a [Layer],
    pub cursor: (u16, u16),
    pub shape: Shape,
    pub metrics: Metrics,
    pub size: (u32, u32),
    /// The text scrolls: its rows are this many pixels from where they rest.
    pub scroll_dy: f32,
    pub ghosts: Option<Ghosts<'a>>,
    /// The fog of each cell (percent), instead of the cells' own: an animation's.
    pub fog: Option<&'a [Vec<u8>]>,
}

struct Builder<'a> {
    v: &'a View<'a>,
    d: Depth,
    t: &'a Tones,
    m: Metrics,
    page: Rgb,
    boxes: Vec<(CellRect, bool)>,
    /// The windows, the lowest first, and whether each is modal.
    floats: Vec<(CellRect, bool)>,
    text: Option<CellRect>,
    scroll: Option<CellRect>,
    status: Option<CellRect>,
    cursor_row: Option<CellRect>,
}

/// One run of cells on one background, in one row.
#[derive(Clone, Copy)]
struct Run {
    r: usize,
    c0: u16,
    c1: u16,
    color: Rgb,
    role: Role,
    selected: bool,
}

fn contains(r: &CellRect, col: u16, row: usize) -> bool {
    col >= r.x
        && col < r.x + r.w
        && row >= usize::from(r.y)
        && row < usize::from(r.y) + usize::from(r.h)
}

pub fn build(v: &View) -> Scene {
    let m = v.metrics;
    let d = Depth::new(&v.grid.pal, m);
    let mut b = Builder {
        v,
        d,
        t: v.tones,
        m,
        page: v.grid.pal.bg,
        boxes: vec![],
        floats: vec![],
        text: None,
        scroll: None,
        status: None,
        cursor_row: None,
    };
    for l in v.layers {
        match l.kind {
            LayerKind::Box { focused } => b.boxes.push((l.rect, focused)),
            LayerKind::Float { modal } => b.floats.push((l.rect, modal)),
            LayerKind::Text { .. } => b.text = Some(l.rect),
            LayerKind::Scroll => b.scroll = Some(l.rect),
            LayerKind::Status => b.status = Some(l.rect),
            LayerKind::Cursor => b.cursor_row = Some(l.rect),
        }
    }
    b.run()
}

impl Builder<'_> {
    fn cell_x(&self, c: u16) -> f32 {
        self.m.pad_x + f32::from(c) * self.m.cell_w
    }

    fn cell_y(&self, r: usize) -> f32 {
        self.m.pad_y + r as f32 * self.m.cell_h
    }

    fn px(&self, r: &CellRect) -> Rect {
        Rect::new(
            self.cell_x(r.x),
            self.cell_y(usize::from(r.y)),
            f32::from(r.w) * self.m.cell_w,
            f32::from(r.h) * self.m.cell_h,
        )
    }

    /// The layer a cell is drawn in: 0 for the page, 1 + the window's index for a window (the highest
    /// that has the cell).
    fn owner(&self, col: u16, row: usize) -> usize {
        self.floats.iter().rposition(|(r, _)| contains(r, col, row)).map_or(0, |i| i + 1)
    }

    fn body_cols(&self) -> Option<Range<u16>> {
        self.text.map(|t| t.x..t.x + t.w)
    }

    fn body_rows(&self) -> Option<Range<usize>> {
        self.text.map(|t| usize::from(t.y)..usize::from(t.y) + usize::from(t.h))
    }

    fn run(&self) -> Scene {
        let v = self.v;
        let rows = v.grid.rows.len();
        let mut layers = vec![Plane::default(); 1 + self.floats.len()];
        self.boxes(&mut layers[0]);
        self.status_bar(&mut layers[0]);
        let all = 0..v.grid.cols;
        let scrolls = v.scroll_dy != 0.0 || v.ghosts.is_some();
        let body = self.text.map(|t| self.px(&t));
        for (idx, layer) in layers.iter_mut().enumerate() {
            let split = idx == 0 && scrolls;
            let (Some(cols), Some(brows), true) = (self.body_cols(), self.body_rows(), split)
            else {
                self.part(idx, v.grid, 0..rows, all.clone(), 0.0, None, layer);
                continue;
            };
            let (b0, b1) = (brows.start.min(rows), brows.end.min(rows));
            self.part(idx, v.grid, 0..b0, all.clone(), 0.0, None, layer);
            self.part(idx, v.grid, b1..rows, all.clone(), 0.0, None, layer);
            // the rows beside the text (the tree, the borders) stay; the text moves
            self.part(idx, v.grid, b0..b1, 0..cols.start, 0.0, None, layer);
            self.part(idx, v.grid, b0..b1, cols.end..v.grid.cols, 0.0, None, layer);
            self.part(idx, v.grid, b0..b1, cols.clone(), v.scroll_dy, body, layer);
            if let Some(gh) = &v.ghosts {
                self.part(idx, gh.grid, gh.rows.clone(), cols, gh.dy, body, layer);
            }
        }
        self.scrollbar(&mut layers[0]);
        for (i, (r, _)) in self.floats.iter().enumerate() {
            self.float(r, &mut layers[i + 1]);
        }
        let scrim =
            self.floats.iter().position(|(_, modal)| *modal).map(|i| (i + 1, self.d.scrim()));
        Scene {
            size: v.size,
            cell: (self.m.cell_w, self.m.cell_h),
            floor: rgb(self.d.floor),
            page: rgb(self.page),
            layers,
            scrim,
            cursor: self.cursor(),
            body: body.zip(self.body_rows()),
        }
    }

    /// The boxes of the tree and of the text: raised from the floor, rounded, with a rim, and the one
    /// that has the focus a little brighter at its rim.
    fn boxes(&self, l: &mut Plane) {
        let gap = self.m.dpf(5.0);
        for (r, focused) in &self.boxes {
            // a box with another beside it leaves a gap between them
            let left = self.boxes.iter().any(|(o, _)| o.x + o.w == r.x);
            let right = self.boxes.iter().any(|(o, _)| o.x == r.x + r.w);
            let p = self.px(r);
            let rect = Rect::new(
                p.x + if left { gap } else { 0.0 },
                p.y,
                p.w - if left { gap } else { 0.0 } - if right { gap } else { 0.0 },
                p.h,
            );
            l.surfaces.push(Surface {
                rect,
                radius: self.d.radius(Level::Box),
                fill: rgb(self.page),
                border: Some((self.d.rim(*focused), self.m.dpf(1.0).max(1.0))),
                shadow: Some(self.d.shadow(Level::Box)),
                sheen: (if self.d.light { 0.0 } else { 0.015 }, 0.0),
                kind: Kind::Box,
                clip: None,
            });
        }
    }

    /// The status line: a bar of its own below the boxes, raised a little (the segments of sim's bar
    /// lie on it).
    fn status_bar(&self, l: &mut Plane) {
        let Some(s) = self.status else { return };
        let m = &self.m;
        let p = self.px(&s);
        l.surfaces.push(Surface {
            radius: m.dpf(6.0),
            shadow: Some(self.d.shadow(Level::Chip)),
            border: Some((self.d.rim(false), m.dpf(1.0).max(1.0))),
            sheen: (0.03, 0.0),
            ..Surface::flat(p.inset(0.0, m.dpf(1.0)), rgb(self.t.bar), Kind::Strip)
        });
    }

    /// Whether (`col`, `row`) is on the outer ring of the box or window that owns the cell: the lines
    /// there are the surface's own edge, not chars.
    fn on_ring(&self, col: u16, row: usize, owner: usize) -> bool {
        let ring = |r: &CellRect| {
            contains(r, col, row)
                && (col == r.x
                    || col + 1 == r.x + r.w
                    || row == usize::from(r.y)
                    || row + 1 == usize::from(r.y) + usize::from(r.h))
        };
        match owner {
            0 => self.boxes.iter().any(|(r, _)| ring(r)),
            n => self.floats.get(n - 1).is_some_and(|(r, _)| ring(r)),
        }
    }

    fn style_of(&self, run: &Run, rows: usize) -> Kind {
        match run.role {
            Role::Fence => Kind::Band,
            Role::Heading => Kind::Heading,
            Role::Chip => Kind::Chip,
            Role::Patch => Kind::Patch,
            Role::Tint | Role::Column => Kind::Tint,
            Role::Bar => Kind::Strip,
            _ if run.selected => Kind::Select,
            _ => {
                let _ = rows;
                Kind::Plain
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn part(
        &self,
        idx: usize,
        grid: &Grid,
        rows: Range<usize>,
        cols: Range<u16>,
        dy: f32,
        clip: Option<Rect>,
        out: &mut Plane,
    ) {
        let mut runs: Vec<Run> = vec![];
        for r in rows.clone() {
            let Some(row) = grid.rows.get(r) else { continue };
            self.row_runs(idx, row, r, &cols, &mut runs);
        }
        // merge runs of the same color and columns in rows that follow each other
        runs.sort_by_key(|x| (x.c0, x.c1, x.color, x.r));
        let mut groups: Vec<(Run, usize)> = vec![];
        for run in runs {
            let mergeable = matches!(
                run.role,
                Role::Fence | Role::Heading | Role::Tint | Role::Bar | Role::Column
            );
            match groups.last_mut() {
                Some((g, last))
                    if mergeable
                        && g.c0 == run.c0
                        && g.c1 == run.c1
                        && g.color == run.color
                        && g.role == run.role
                        && *last + 1 == run.r =>
                {
                    *last = run.r;
                }
                _ => groups.push((run, run.r)),
            }
        }
        groups.sort_by_key(|(g, _)| (g.r, g.c0));
        if idx == 0 {
            self.cursor_highlight(&rows, dy, clip, out);
        }
        for (g, last) in groups {
            self.group_surface(&g, last, dy, clip, out);
        }
        for r in rows {
            let Some(row) = grid.rows.get(r) else { continue };
            self.row_glyphs(idx, row, r, &cols, dy, clip, out);
        }
    }

    /// The tree's cursor row, which sim says (and the terminal underlines): a highlight across the row,
    /// in the accent (blue is "where you act", COLOR-1), under the name.
    fn cursor_highlight(&self, rows: &Range<usize>, dy: f32, clip: Option<Rect>, out: &mut Plane) {
        let Some(c) = self.cursor_row else { return };
        if !rows.contains(&usize::from(c.y)) {
            return;
        }
        let m = &self.m;
        let p = self.px(&c);
        let rect =
            Rect::new(p.x + m.dpf(2.0), p.y + m.dpf(1.0) + dy, p.w - m.dpf(4.0), p.h - m.dpf(2.0));
        out.surfaces.push(Surface {
            radius: m.dpf(6.0),
            border: Some((rgba(self.t.accent_fg(), 0.35), m.dpf(1.0).max(1.0))),
            clip,
            ..Surface::flat(rect, rgba(self.t.accent_fg(), 0.16), Kind::Select)
        });
    }

    fn row_runs(
        &self,
        idx: usize,
        row: &GridRow,
        r: usize,
        cols: &Range<u16>,
        runs: &mut Vec<Run>,
    ) {
        let end = cols.end.min(row.cols.len() as u16);
        let in_status = self.status.is_some_and(|s| usize::from(s.y) == r);
        let mut c = cols.start;
        while c < end {
            let cell = &row.cols[usize::from(c)];
            let keep = |cc: u16| {
                let q = &row.cols[usize::from(cc)];
                let drawn = self.owner(cc, r) == idx;
                // the pill is the alert's surface; the scrollbar, the windows' fill and the terminal's
                // shadow beside a window are surfaces of their own, and the bar of the status line is
                // the bar
                let apart = q.hidden
                    || q.role == Role::Alert
                    || self.scroll.is_some_and(|s| contains(&s, cc, r))
                    || matches!(q.role, Role::Fill | Role::Shadow | Role::Thumb)
                    || (in_status && q.role == Role::Bar);
                drawn && !apart && q.bg.is_some()
            };
            if !keep(c) {
                c += 1;
                continue;
            }
            let same = |a: &Resolved| {
                a.bg == cell.bg && a.role == cell.role && a.selected == cell.selected
            };
            let mut e = c + 1;
            while e < end && keep(e) && same(&row.cols[usize::from(e)]) {
                e += 1;
            }
            if let Some(color) = cell.bg {
                runs.push(Run { r, c0: c, c1: e, color, role: cell.role, selected: cell.selected });
            }
            c = e;
        }
    }

    fn group_surface(&self, g: &Run, last: usize, dy: f32, clip: Option<Rect>, out: &mut Plane) {
        let rows = last - g.r + 1;
        let kind = self.style_of(g, rows);
        let m = &self.m;
        let (x0, x1) = (self.cell_x(g.c0), self.cell_x(g.c1));
        let (y0, y1) = (self.cell_y(g.r) + dy, self.cell_y(last + 1) + dy);
        let rect = Rect::new(x0, y0, x1 - x0, y1 - y0);
        let fill = rgb(g.color);
        let surface = match kind {
            Kind::Band | Kind::Heading => Surface {
                radius: m.dpf(6.0),
                shadow: Some(self.d.shadow(Level::Chip)),
                sheen: (0.02, 0.0),
                ..Surface::flat(rect.inset(m.dpf(2.0), m.dpf(2.0)), fill, kind)
            },
            Kind::Chip => Surface {
                radius: m.dpf(6.0),
                shadow: Some(self.d.shadow(Level::Chip)),
                border: Some((self.d.rim(false), m.dpf(1.0).max(1.0))),
                sheen: (0.03, 0.0),
                ..Surface::flat(rect.inset(0.0, m.dpf(2.0)), fill, kind)
            },
            Kind::Patch | Kind::Select => Surface {
                radius: m.dpf(if kind == Kind::Patch { 4.0 } else { 3.0 }),
                ..Surface::flat(
                    rect.inset(0.0, m.dpf(if kind == Kind::Patch { 2.0 } else { 1.0 })),
                    fill,
                    kind,
                )
            },
            // the colorcolumn is a hairline in its cell, not a band
            Kind::Tint if g.role == Role::Column => {
                let w = m.dpf(1.0).max(1.0);
                Surface::flat(
                    Rect::new((rect.x + (rect.w - w) / 2.0).round(), rect.y, w, rect.h),
                    fill,
                    Kind::Shape,
                )
            }
            _ => Surface::flat(rect, fill, kind),
        };
        out.surfaces.push(Surface { clip, ..surface });
    }

    fn fog_of(&self, r: usize, k: usize, own: u8) -> u8 {
        self.v.fog.and_then(|f| f.get(r)).and_then(|row| row.get(k)).copied().unwrap_or(own)
    }

    fn alert(&self, c: &Cell) -> bool {
        c.r.hidden || c.r.role == Role::Alert
    }

    #[allow(clippy::too_many_arguments)]
    fn row_glyphs(
        &self,
        idx: usize,
        row: &GridRow,
        r: usize,
        cols: &Range<u16>,
        dy: f32,
        clip: Option<Rect>,
        out: &mut Plane,
    ) {
        let m = &self.m;
        let (alert_back, alert_text) = self.t.alert_block();
        let mut i = 0;
        while i < row.cells.len() {
            let c = &row.cells[i];
            let k = i;
            i += 1;
            if c.col < cols.start || c.col >= cols.end || self.owner(c.col, r) != idx {
                continue;
            }
            if self.scroll.is_some_and(|s| contains(&s, c.col, r)) {
                continue;
            }
            if self.alert(c) {
                // the pill of a run of alert cells is made when its first cell is met
                let first =
                    k == 0 || !self.alert(&row.cells[k - 1]) || row.cells[k - 1].end() != c.col;
                if first {
                    let mut end = k;
                    while end + 1 < row.cells.len()
                        && self.alert(&row.cells[end + 1])
                        && row.cells[end + 1].col == row.cells[end].end()
                    {
                        end += 1;
                    }
                    self.pill(r, c.col, row.cells[end].end(), alert_back, dy, clip, out);
                }
            }
            let x = self.cell_x(c.col);
            let y = self.cell_y(r) + dy;
            let fog = self.fog_of(r, k, c.r.fade);
            let ink = if self.alert(c) && !c.r.hidden {
                alert_text // text in the alert's ink is on the pill: the pill's own text color
            } else {
                c.r.ink(self.page, fog).unwrap_or(self.v.grid.pal.primary)
            };
            // the box-drawing chars of the edge of a box or a window are the surface itself
            if blocks::is_border(c.ch) && self.on_ring(c.col, r, idx) {
                continue;
            }
            if let Some(ps) = blocks::pieces(c.ch) {
                self.piece_char(ps, x, y, rgb(ink), clip, out);
                continue;
            }
            if c.ch != ' ' && c.ch != '\u{a0}' {
                out.glyphs.push(Glyph {
                    x,
                    y,
                    ch: c.ch,
                    extra: c.extra.clone(),
                    cols: c.width,
                    color: rgb(ink),
                    bold: c.r.attrs.bold(),
                    italic: c.r.attrs.italic(),
                    clip,
                });
            }
            // an underline, but not the one sim puts under the tree's cursor row (a highlight is there)
            let on_cursor_row = self.cursor_row.is_some_and(|q| contains(&q, c.col, r));
            if c.r.attrs.underline() && !on_cursor_row {
                let w = f32::from(c.width) * m.cell_w;
                let th = m.dpf(1.0).max(1.0);
                let uy = (y + m.baseline + m.dpf(2.0)).round();
                out.surfaces.push(Surface {
                    clip,
                    ..Surface::flat(Rect::new(x, uy, w, th), rgb(ink), Kind::Shape)
                });
            }
        }
    }

    /// A char drawn as shapes.
    fn piece_char(
        &self,
        ps: &[Piece],
        x: f32,
        y: f32,
        color: Color,
        clip: Option<Rect>,
        out: &mut Plane,
    ) {
        let m = &self.m;
        let (cw, ch) = (m.cell_w, m.cell_h);
        let light = (ch / 18.0).round().max(1.0);
        for p in ps {
            let (rect, radius) = match *p {
                Piece::Rect(a, b, c2, d) => {
                    (Rect::new(x + a * cw, y + b * ch, (c2 - a) * cw, (d - b) * ch), 0.0)
                }
                Piece::Bar(a, b, c2, d) => {
                    let r = Rect::new(x + a * cw, y + b * ch, (c2 - a) * cw, (d - b) * ch);
                    (r, r.w.min(r.h) / 2.0)
                }
                Piece::H(fy, a, b, heavy) => {
                    let t = if heavy { light * 2.0 } else { light };
                    let yc = (y + fy * ch - t / 2.0).round();
                    (Rect::new(x + a * cw, yc, (b - a) * cw, t), 0.0)
                }
                Piece::V(fx, a, b, heavy) => {
                    let t = if heavy { light * 2.0 } else { light };
                    let xc = (x + fx * cw - t / 2.0).round();
                    (Rect::new(xc, y + a * ch, t, (b - a) * ch), 0.0)
                }
            };
            out.surfaces.push(Surface { radius, clip, ..Surface::flat(rect, color, Kind::Shape) });
        }
    }

    /// The pill of a run of alert cells: the alert's color, fully rounded, a little taller and wider than
    /// the text, with a glow of its own color so that nothing else on the screen is lit like it.
    #[allow(clippy::too_many_arguments)]
    fn pill(
        &self,
        r: usize,
        c0: u16,
        c1: u16,
        color: Rgb,
        dy: f32,
        clip: Option<Rect>,
        out: &mut Plane,
    ) {
        let m = &self.m;
        let (x0, x1) = (self.cell_x(c0) - m.dpf(2.0), self.cell_x(c1) + m.dpf(2.0));
        let h = (m.cell_h * 0.84).round();
        let y = (self.cell_y(r) + dy + (m.cell_h - h) / 2.0).round();
        let glow = Shadow {
            dy: m.dpf(1.0),
            blur: m.dpf(9.0),
            color: rgba(color, if self.d.light { 0.55 } else { 0.70 }),
        };
        out.surfaces.push(Surface {
            rect: Rect::new(x0, y, x1 - x0, h),
            radius: h / 2.0,
            fill: rgb(color),
            border: Some(([1.0, 1.0, 1.0, 0.28], m.dpf(1.0).max(1.0))),
            shadow: Some(glow),
            sheen: (0.10, 0.06),
            kind: Kind::Pill,
            clip,
        });
    }

    /// The scrollbar: the column at the right edge of the text. The thumb is a rounded bar behind, the
    /// marks of sim's `┃` are ticks, the `⎺⎻⎼⎽` is the cursor's place in its row.
    fn scrollbar(&self, out: &mut Plane) {
        let (Some(s), v) = (self.scroll, self.v) else { return };
        let m = &self.m;
        let (col, y0, end) = (s.x, usize::from(s.y), usize::from(s.y) + usize::from(s.h));
        let x = self.cell_x(col);
        let bar_w = m.dpf(6.0).round();
        let bx = (x + (m.cell_w - bar_w) / 2.0).round();
        let cell_at =
            |rr: usize| v.grid.rows.get(rr).and_then(|row| row.cells.iter().find(|c| c.col == col));
        let thumb_at = |rr: usize| {
            v.grid
                .rows
                .get(rr)
                .and_then(|row| row.cols.get(usize::from(col)))
                .is_some_and(|q| q.role == Role::Thumb)
        };
        // the groove: a faint rounded track for the whole height
        let (top, bot) = (self.cell_y(y0) + m.dpf(2.0), self.cell_y(end) - m.dpf(2.0));
        if bot > top {
            out.surfaces.push(Surface {
                radius: bar_w / 2.0,
                ..Surface::flat(
                    Rect::new(bx, top, bar_w, bot - top),
                    rgba(self.t.focus, 0.07),
                    Kind::Shape,
                )
            });
        }
        let mut r = y0;
        while r < end {
            if thumb_at(r) {
                let mut e = r + 1;
                while e < end && thumb_at(e) {
                    e += 1;
                }
                let fill = v.grid.rows[r].cols[usize::from(col)].bg.map_or(rgb(self.t.thumb), rgb);
                out.surfaces.push(Surface {
                    radius: bar_w / 2.0,
                    shadow: Some(self.d.shadow(Level::Chip)),
                    sheen: (0.05, 0.0),
                    ..Surface::flat(
                        Rect::new(
                            bx,
                            self.cell_y(r) + m.dpf(1.0),
                            bar_w,
                            self.cell_y(e) - self.cell_y(r) - m.dpf(2.0),
                        ),
                        fill,
                        Kind::Thumb,
                    )
                });
                r = e;
            } else {
                r += 1;
            }
        }
        for rr in y0..end {
            let Some(c) = cell_at(rr) else { continue };
            let ink = rgb(c.r.ink(self.page, c.r.fade).unwrap_or(self.v.grid.pal.primary));
            let y = self.cell_y(rr);
            match c.ch {
                '\u{2503}' => {
                    let tw = m.dpf(3.0).round().max(2.0);
                    out.surfaces.push(Surface {
                        radius: tw / 2.0,
                        ..Surface::flat(
                            Rect::new(
                                (x + (m.cell_w - tw) / 2.0 + bar_w * 0.35).round(),
                                y + m.dpf(1.0),
                                tw,
                                m.cell_h - m.dpf(2.0),
                            ),
                            ink,
                            Kind::Shape,
                        )
                    });
                }
                '\u{23ba}' | '\u{23bb}' | '\u{23bc}' | '\u{23bd}' => {
                    let q = ['\u{23ba}', '\u{23bb}', '\u{23bc}', '\u{23bd}']
                        .iter()
                        .position(|&x| x == c.ch)
                        .unwrap_or(0);
                    let th = m.dpf(2.0).round().max(2.0);
                    let yy = (y + (q as f32 + 0.5) / 4.0 * m.cell_h - th / 2.0).round();
                    out.surfaces.push(Surface {
                        radius: th / 2.0,
                        ..Surface::flat(
                            Rect::new((x + m.dpf(1.0)).round(), yy, m.cell_w - m.dpf(2.0), th),
                            ink,
                            Kind::Shape,
                        )
                    });
                }
                _ => {}
            }
        }
    }

    /// A window: above everything under it, with the shadow of its level (the highest) and a rim.
    fn float(&self, r: &CellRect, out: &mut Plane) {
        let m = &self.m;
        out.surfaces.insert(
            0,
            Surface {
                rect: self.px(r),
                radius: self.d.radius(Level::Float),
                fill: rgb(self.page),
                border: Some((self.d.rim(true), m.dpf(1.0).max(1.0))),
                shadow: Some(self.d.shadow(Level::Float)),
                sheen: (if self.d.light { 0.0 } else { 0.02 }, 0.0),
                kind: Kind::Float,
                clip: None,
            },
        );
    }

    fn cursor(&self) -> Option<CursorSpec> {
        let v = self.v;
        let (col, row) = v.cursor;
        let m = &self.m;
        let line = v.grid.rows.get(usize::from(row))?;
        let cell = line.cells.iter().find(|c| c.col == col);
        let cols = cell.map_or(1, |c| c.width.max(1));
        let (x, y) = (self.cell_x(col), self.cell_y(usize::from(row)));
        let w = f32::from(cols) * m.cell_w;
        let in_body = self.text.is_some_and(|t| contains(&t, col, usize::from(row)))
            && self.floats.is_empty();
        let under = cell.filter(|c| c.ch != ' ').map(|c| Glyph {
            x,
            y,
            ch: c.ch,
            extra: c.extra.clone(),
            cols: c.width,
            color: rgb(self.page),
            bold: c.r.attrs.bold(),
            italic: c.r.attrs.italic(),
            clip: None,
        });
        Some(CursorSpec {
            rect: Rect::new(x, y, w, m.cell_h),
            shape: v.shape,
            color: rgb(v.grid.pal.primary),
            under,
            in_body,
        })
    }
}

fn color_eq(a: Color, b: Color) -> bool {
    a.iter().zip(&b).all(|(x, y)| (x - y).abs() < 1.0 / 512.0)
}

type Run3 = (f32, f32, String, Color, bool, bool, f32);

impl Scene {
    /// The scene as text, for a golden: surfaces with their numbers, glyphs gathered into runs.
    pub fn dump(&self) -> String {
        let mut s = String::new();
        let hex = |c: Color| {
            let b = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
            if c[3] < 0.999 {
                format!("#{:02x}{:02x}{:02x}/{:.2}", b(c[0]), b(c[1]), b(c[2]), c[3])
            } else {
                format!("#{:02x}{:02x}{:02x}", b(c[0]), b(c[1]), b(c[2]))
            }
        };
        let _ = writeln!(
            s,
            "scene {}x{} floor {} page {}",
            self.size.0,
            self.size.1,
            hex(self.floor),
            hex(self.page)
        );
        for (i, l) in self.layers.iter().enumerate() {
            if let Some((at, c)) = self.scrim.filter(|(at, _)| *at == i) {
                let _ = writeln!(s, "scrim over the layers before {at}: {}", hex(c));
            }
            let _ = writeln!(s, "layer {i}");
            for f in &l.surfaces {
                let _ = write!(
                    s,
                    "  {:?} [{:.0} {:.0} {:.0} {:.0}] r{:.0} {}",
                    f.kind,
                    f.rect.x,
                    f.rect.y,
                    f.rect.w,
                    f.rect.h,
                    f.radius,
                    hex(f.fill)
                );
                if let Some(sh) = f.shadow {
                    let _ =
                        write!(s, " shadow(dy {:.0} blur {:.0} {})", sh.dy, sh.blur, hex(sh.color));
                }
                if let Some((c, w)) = f.border {
                    let _ = write!(s, " rim({} {w:.0})", hex(c));
                }
                if f.clip.is_some() {
                    s.push_str(" clip");
                }
                s.push('\n');
            }
            let mut run: Option<Run3> = None;
            let flush = |s: &mut String, run: &mut Option<Run3>| {
                if let Some((x, y, text, c, b, it, _)) = run.take() {
                    let _ = writeln!(
                        s,
                        "  text [{x:.0} {y:.0}] {text:?} {}{}{}",
                        hex(c),
                        if b { " bold" } else { "" },
                        if it { " italic" } else { "" }
                    );
                }
            };
            for g in &l.glyphs {
                let mut piece = g.ch.to_string();
                piece.extend(&g.extra);
                match &mut run {
                    Some((_, y, text, c, b, it, next))
                        if *y == g.y
                            && (g.x - *next).abs() < 0.01
                            && color_eq(*c, g.color)
                            && *b == g.bold
                            && *it == g.italic =>
                    {
                        text.push_str(&piece);
                        *next = g.x + f32::from(g.cols) * self.cell.0;
                    }
                    _ => {
                        flush(&mut s, &mut run);
                        run = Some((
                            g.x,
                            g.y,
                            piece,
                            g.color,
                            g.bold,
                            g.italic,
                            g.x + f32::from(g.cols) * self.cell.0,
                        ));
                    }
                }
            }
            flush(&mut s, &mut run);
        }
        if let Some(c) = &self.cursor {
            let _ = writeln!(
                s,
                "cursor {:?} [{:.0} {:.0} {:.0} {:.0}] {}",
                c.shape,
                c.rect.x,
                c.rect.y,
                c.rect.w,
                c.rect.h,
                hex(c.color)
            );
        }
        s
    }
}
