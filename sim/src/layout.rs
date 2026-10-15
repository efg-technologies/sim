//! TREE-1, TREE-2, BUF-2
//! The panel and the editor composed into one Frame (pure): the panel on the left, a column of
//! space, the editor, and one status line across the whole width.
use crate::tree;
use si::core::view::{Frame, Row, Style};
use unicode_width::UnicodeWidthChar;

/// What the editor area shows while no file is open (TREE-1).
const HELP: [&str; 5] =
    ["Ctrl-n  tree", "Enter   open", "Tab     next file", "q       close", ":qa     quit all"];

/// Whether a char must not reach the terminal as it is (SAFE-8): a file name can hold anything.
fn hostile(c: char) -> bool {
    c.is_control()
        || matches!(c, '\u{200b}' | '\u{200e}'..='\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}' | '\u{feff}')
}

/// `s` cut to `max` columns, hostile chars replaced by U+FFFD, and the columns it takes.
pub fn fit(s: &str, max: usize) -> (String, usize) {
    let (mut out, mut cols) = (String::new(), 0);
    for c in s.chars().map(|c| if hostile(c) { '\u{fffd}' } else { c }) {
        let w = c.width().unwrap_or(0);
        if cols + w > max {
            break;
        }
        out.push(c);
        cols += w;
    }
    (out, cols)
}

/// How a row of the tree is written: indent 2 per depth, `▸ name/` or `▾ name/`, `  name`.
pub fn label(r: &tree::Row) -> String {
    let (indent, mark) = ("  ".repeat(r.depth), if r.open { "▾ " } else { "▸ " });
    if r.is_dir { format!("{indent}{mark}{}/", r.name) } else { format!("{indent}  {}", r.name) }
}

/// The body rows of the screen (all but the status line), `w` columns wide: the panel's `pw`
/// columns showing `panel` (the row `sel` underlined), a column of space, then the `editor`'s rows,
/// which are as wide as what is left; without an editor, the help.
pub fn body(
    w: u16,
    h: u16,
    pw: u16,
    panel: &[tree::Row],
    sel: Option<usize>,
    ed: Option<&Frame>,
) -> Vec<Row> {
    let off = if pw > 0 { pw + 1 } else { 0 };
    let mut rows = Vec::with_capacity(usize::from(h));
    for i in 0..usize::from(h) - 1 {
        let mut row = Row::default();
        if pw > 0 {
            let (text, used) =
                panel.get(i).map_or((String::new(), 0), |r| fit(&label(r), usize::from(pw)));
            row.text = format!("{text}{} ", " ".repeat(usize::from(pw) - used));
            if sel == Some(i) {
                row.underline.push(0..used as u16);
            }
        }
        if let Some(e) = ed.map(|f| &f.rows[i]) {
            row.text += &e.text;
            row.spans.extend(e.spans.iter().map(|(r, s)| (r.start + off..r.end + off, *s)));
            row.underline.extend(e.underline.iter().map(|r| r.start + off..r.end + off));
        } else if let Some(help) = HELP.get(i) {
            let (text, used) = fit(help, usize::from(w - off));
            row.text += &text;
            row.spans.push((off..off + used as u16, Style::Secondary));
        }
        rows.push(row);
    }
    rows
}
