//! GUI-16
//! Box-drawing and block chars drawn as shapes, not glyphs. sim writes its guides, rules, signs and
//! bars with these chars (a terminal draws them itself, to the cell's edge, so they join); a font's
//! versions never quite meet at the cell edge and are not the same weight in every font. Each is a
//! few rectangles in a cell, in fractions of its width and height; the thickness of a line is the
//! renderer's (a light line is a pixel or two, a heavy one twice that).

/// One piece of a char, in the unit square of its cell (x to the right, y down).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Piece {
    /// A rectangle `x0 y0 x1 y1`.
    Rect(f32, f32, f32, f32),
    /// The same with rounded ends (a sign, a quote's bar): the radius is half of the narrow side.
    Bar(f32, f32, f32, f32),
    /// A horizontal line across `x0..x1` at height `y`; heavy or light.
    H(f32, f32, f32, bool),
    /// A vertical line at `x` across `y0..y1`.
    V(f32, f32, f32, bool),
}

use Piece::*;

/// The pieces of a char, if it is drawn as shapes.
pub fn pieces(c: char) -> Option<&'static [Piece]> {
    Some(match c {
        // lines and their joints (light)
        '\u{2500}' => &[H(0.5, 0.0, 1.0, false)],
        '\u{2502}' => &[V(0.5, 0.0, 1.0, false)],
        '\u{251c}' => &[V(0.5, 0.0, 1.0, false), H(0.5, 0.5, 1.0, false)],
        '\u{2524}' => &[V(0.5, 0.0, 1.0, false), H(0.5, 0.0, 0.5, false)],
        '\u{252c}' => &[H(0.5, 0.0, 1.0, false), V(0.5, 0.5, 1.0, false)],
        '\u{2534}' => &[H(0.5, 0.0, 1.0, false), V(0.5, 0.0, 0.5, false)],
        '\u{253c}' => &[H(0.5, 0.0, 1.0, false), V(0.5, 0.0, 1.0, false)],
        // rounded corners are drawn as the two lines they join: the boxes' own are surfaces now
        '\u{256d}' => &[H(0.5, 0.5, 1.0, false), V(0.5, 0.5, 1.0, false)],
        '\u{256e}' => &[H(0.5, 0.0, 0.5, false), V(0.5, 0.5, 1.0, false)],
        '\u{2570}' => &[H(0.5, 0.5, 1.0, false), V(0.5, 0.0, 0.5, false)],
        '\u{256f}' => &[H(0.5, 0.0, 0.5, false), V(0.5, 0.0, 0.5, false)],
        // a changed line's sign (COLOR-3): three dashes, a shape of its own where the added line's is a bar
        '\u{2506}' => {
            &[V(0.5, 0.04, 0.28, false), V(0.5, 0.38, 0.62, false), V(0.5, 0.72, 0.96, false)]
        }
        // heavy vertical: the overview ruler's marks
        '\u{2503}' => &[V(0.5, 0.0, 1.0, true)],
        // blocks
        '\u{2580}' => &[Rect(0.0, 0.0, 1.0, 0.5)],
        '\u{2584}' => &[Rect(0.0, 0.5, 1.0, 1.0)],
        '\u{2588}' => &[Rect(0.0, 0.0, 1.0, 1.0)],
        '\u{258c}' => &[Bar(0.30, 0.0, 0.42, 1.0)], // a quote's bar
        '\u{258e}' => &[Bar(0.22, 0.06, 0.40, 0.94)], // a sign, the selected row's marker, a tab's indicator
        '\u{2581}' => &[Bar(0.12, 0.84, 0.88, 0.96)], // a deletion below a line
        '\u{2594}' => &[Bar(0.12, 0.04, 0.88, 0.16)], // a deletion above the first line
        _ => return None,
    })
}

/// A box-drawing char of the borders of a box: these are hidden on the edge of a box, which is a
/// surface now.
pub fn is_border(c: char) -> bool {
    matches!(
        c,
        '\u{2500}'
            | '\u{2502}'
            | '\u{251c}'
            | '\u{2524}'
            | '\u{252c}'
            | '\u{2534}'
            | '\u{253c}'
            | '\u{256d}'..='\u{2570}'
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gui_16_the_chars_sim_writes_its_lines_and_signs_with_are_shapes() {
        for c in "│─├┤┼╭╮╰╯┃┆▀▄█▌▎▁▔".chars() {
            assert!(pieces(c).is_some(), "{c}");
        }
        for c in "ab ·»⍽⠋".chars() {
            assert!(pieces(c).is_none(), "{c}");
        }
    }

    #[test]
    fn gui_16_every_piece_lies_in_its_cell() {
        for c in "│─├┤┬┴┼╭╮╰╯┃┆▀▄█▌▎▁▔".chars() {
            for p in pieces(c).unwrap_or(&[]) {
                let ok = |v: f32| (0.0..=1.0).contains(&v);
                let inside = match *p {
                    Rect(a, b, c, d) | Bar(a, b, c, d) => ok(a) && ok(b) && ok(c) && ok(d),
                    H(y, a, b, _) | V(y, a, b, _) => ok(y) && ok(a) && ok(b),
                };
                assert!(inside, "{c}: {p:?}");
            }
        }
    }

    #[test]
    fn gui_16_the_borders_of_a_box_are_the_joints_and_corners() {
        assert!(is_border('╭') && is_border('│') && is_border('─') && is_border('├'));
        assert!(!is_border('┃') && !is_border('▎') && !is_border('┆') && !is_border('a'));
    }
}
