//! GUI-13, GUI-10
//! What is drawn over the scene and is not sim's: the cursor (which moves smoothly from cell to cell
//! and takes the shape of the mode) and the text an input method is composing. sim's own cursor is
//! a cell and a shape (`Frame::cursor`, `Frame::shape`); the window shows it as a shape that glides.
use crate::api::{Shape, Tones, cell_width};
use crate::depth::rgb;
use crate::metrics::Metrics;
use crate::scene::{CursorSpec, Glyph, Kind, Plane, Rect, Surface};

/// The rectangle of a cursor of this shape in the cell `cell`.
pub fn shape_rect(shape: Shape, cell: Rect, m: &Metrics) -> Rect {
    let t = m.dpf(2.0).round().max(2.0);
    match shape {
        Shape::Block => cell,
        Shape::Bar => Rect::new(cell.x, cell.y, t, cell.h),
    }
}

/// The cursor drawn at `rect` (where the glide has got to), over the cell it is aiming at. In a block
/// the char under it is drawn again, in the page's color, so it reads on the cursor.
pub fn cursor_layer(spec: &CursorSpec, rect: Rect, block: f32, m: &Metrics) -> Plane {
    let mut l = Plane::default();
    let radius = (rect.w.min(rect.h) / 2.0).min(m.dpf(2.5));
    let mut fill = spec.color;
    fill[3] = 0.92;
    l.surfaces.push(Surface {
        radius,
        sheen: (0.06, 0.0),
        ..Surface::flat(rect, fill, Kind::Shape)
    });
    if let Some(g) = spec.under.as_ref().filter(|_| block > 0.5) {
        l.glyphs.push(Glyph { clip: None, ..g.clone() });
    }
    l
}

/// The composing text of an input method at (`x`, `y`), a pill-shaped chip with the text on it and an
/// underline; the part the method is converting (if it says) is underlined heavier. Returns the layer
/// and the width in pixels it takes (for placing the candidate window).
pub fn preedit_layer(
    text: &str,
    (x, y): (f32, f32),
    tones: &Tones,
    m: &Metrics,
    light: bool,
) -> (Plane, f32) {
    let mut l = Plane::default();
    let cs: Vec<char> = text.chars().collect();
    let mut col = 0usize;
    let mut glyphs = vec![];
    for i in 0..cs.len() {
        let w = cell_width(&cs, i, col);
        if w > 0 {
            glyphs.push((col, w, cs[i]));
        }
        col += w;
    }
    let width = col as f32 * m.cell_w;
    let pad = m.dpf(3.0);
    let chip = Rect::new(x - pad, y + m.dpf(1.0), width + 2.0 * pad, m.cell_h - m.dpf(2.0));
    l.surfaces.push(Surface {
        radius: m.dpf(5.0),
        border: Some((rgb(tones.focus), m.dpf(1.0).max(1.0))),
        shadow: Some(crate::depth::Shadow {
            dy: m.dpf(2.0),
            blur: m.dpf(8.0),
            color: [0.0, 0.0, 0.0, if light { 0.25 } else { 0.55 }],
        }),
        sheen: (0.03, 0.0),
        ..Surface::flat(chip, rgb(tones.card), Kind::Chip)
    });
    let ink = rgb(tones.primary);
    for (c, w, ch) in glyphs {
        let gx = x + c as f32 * m.cell_w;
        l.glyphs.push(Glyph {
            x: gx,
            y,
            ch,
            extra: vec![],
            cols: w as u8,
            color: ink,
            bold: false,
            italic: false,
            clip: None,
        });
    }
    let ul = (y + m.baseline + m.dpf(2.0)).round();
    l.surfaces.push(Surface::flat(Rect::new(x, ul, width, m.dpf(1.0).max(1.0)), ink, Kind::Shape));
    (l, width)
}
