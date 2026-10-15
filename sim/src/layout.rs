//! TREE-1, TREE-2, BUF-2, FIND-1, MARK-1, MARK-2, SYN-1, SYN-5
//! The panel and the editor composed into one screen (pure): the panel on the left with its
//! marks, a column of space, the editor with its colors, and one status line across the whole
//! width.
use crate::paint::{Look, Looks};
use crate::syn::Spans;
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

/// What the panel shows about a row besides its name: MARK-1 (`change` the change color on the
/// name of a file, `below` the `~` of a directory), MARK-2 (`likely`, the `•`), and FIND-1
/// (`hit`: none when no search runs; bold for a match or a directory leading to one, dimmed else).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Mark {
    pub change: bool,
    pub below: bool,
    pub likely: bool,
    pub hit: Option<bool>,
}

/// How a row of the tree is written: indent 2 per depth, `▸ name/` or `▾ name/` (then ` ~` if
/// something below changed), `• name` for a likely file, `  name` for any other.
pub fn label(r: &tree::Row, m: &Mark) -> String {
    let indent = "  ".repeat(r.depth);
    if r.is_dir {
        let (mark, below) = (if r.open { "▾ " } else { "▸ " }, if m.below { " ~" } else { "" });
        format!("{indent}{mark}{}/{below}", r.name)
    } else {
        format!("{indent}{}{}", if m.likely { "• " } else { "  " }, r.name)
    }
}

/// The body rows of the screen (all but the status line), `w` columns wide, and their looks:
/// the panel's `pw` columns showing `panel` (the row `sel` underlined), a column of space, then
/// the `editor`'s rows and looks, which are as wide as what is left; without an editor, the
/// help. With `gray` the Visual selection of the editor is a gray background instead of an
/// underline (SYN-5).
pub fn body(
    w: u16,
    h: u16,
    pw: u16,
    panel: &[(tree::Row, Mark)],
    sel: Option<usize>,
    ed: Option<(&Frame, &[Looks])>,
    gray: bool,
) -> (Vec<Row>, Vec<Looks>) {
    let off = if pw > 0 { pw + 1 } else { 0 };
    let (mut rows, mut all) = (Vec::with_capacity(usize::from(h)), vec![]);
    for i in 0..usize::from(h) - 1 {
        let (mut row, mut looks) = (Row::default(), Looks::new());
        if pw > 0 {
            let at = panel.get(i);
            let (text, used) =
                at.map_or((String::new(), 0), |(r, m)| fit(&label(r, m), usize::from(pw)));
            row.text = format!("{text}{} ", " ".repeat(usize::from(pw) - used));
            if let Some((r, m)) = at {
                let start = (2 * r.depth + 2).min(used);
                let end = (start + fit(&r.name, usize::MAX).1 + usize::from(r.is_dir)).min(used);
                if m.change {
                    row.spans.push((start as u16..end as u16, Style::Change));
                }
                match m.hit {
                    Some(false) => row.spans.push((0..used as u16, Style::Secondary)),
                    Some(true) => looks.push((0..used as u16, Look::Bold)),
                    None => {}
                }
            }
            if sel == Some(i) {
                row.underline.push(0..used as u16);
            }
        }
        if let Some((e, l)) = ed.map(|(f, l)| (&f.rows[i], &l[i])) {
            row.text += &e.text;
            row.spans.extend(e.spans.iter().map(|(r, s)| (r.start + off..r.end + off, *s)));
            let picked = e.underline.iter().map(|r| r.start + off..r.end + off);
            if gray {
                looks.extend(picked.map(|r| (r, Look::Sel)));
            } else {
                row.underline.extend(picked);
            }
            looks.extend(l.iter().map(|(r, k)| (r.start + off..r.end + off, *k)));
        } else if let Some(help) = HELP.get(i) {
            let (text, used) = fit(help, usize::from(w - off));
            row.text += &text;
            row.spans.push((off..off + used as u16, Style::Secondary));
        }
        rows.push(row);
        all.push(looks);
    }
    (rows, all)
}

/// How a char of the buffer is written in a row of the editor (SAFE-8, VIS-8; view.rs): the
/// chars it takes in the row's text and the columns, when it starts at column `col`.
fn shown(c: char, col: usize) -> (usize, usize) {
    match c {
        '\t' => (4 - col % 4, 4 - col % 4),
        '\0'..='\x08' | '\n'..='\x1f' | '\x7f' => (2, 2), // ^X
        '\u{80}'..='\u{9f}' | '\u{200b}' | '\u{200e}'..='\u{200f}' | '\u{202a}'..='\u{202e}' => {
            (8, 8)
        } // <U+XXXX>
        '\u{2066}'..='\u{2069}' | '\u{feff}' => (8, 8),
        _ => (1, c.width().unwrap_or(0)),
    }
}

/// SYN-1: the looks of the rows of an editor frame. `digits` is the width of its line numbers,
/// `text(l)` the buffer's line `l` and `spans(l)` the syntax spans of it, if they are known.
/// The line of a row is read from its gutter, and a wrapped line goes on in the rows below it.
/// Also the first and the last line on screen.
pub fn colors<'a>(
    f: &Frame,
    digits: usize,
    text: &dyn Fn(usize) -> Option<String>,
    spans: &dyn Fn(usize) -> Option<&'a Spans>,
) -> (Vec<Looks>, Option<(usize, usize)>) {
    let (gutter, mut out, mut win) = (digits + 2, vec![Looks::new(); f.rows.len()], None);
    let (mut cs, mut sp, mut ci) = (Vec::<char>::new(), None, 0);
    for (i, row) in f.rows.iter().enumerate() {
        let no: String = row.text.chars().skip(1).take(digits).collect();
        if let Some(l) = no.trim().parse::<usize>().ok().and_then(|n| n.checked_sub(1)) {
            (cs, sp, ci) = (text(l).unwrap_or_default().chars().collect(), spans(l), 0);
            win = Some((win.map_or(l, |w: (usize, usize)| w.0), l));
        }
        let (mut col, mut used, len) = (0, 0, row.text.chars().count().saturating_sub(gutter));
        while used < len && ci < cs.len() {
            let (n, w) = shown(cs[ci], col);
            let look = sp.and_then(|s| s.iter().find(|s| s.0.contains(&ci))).filter(|_| w > 0);
            let look = look.map(|s| s.1);
            let cols = (gutter + col) as u16..(gutter + col + w) as u16;
            match (look, out[i].last_mut()) {
                (Some(k), Some((p, pk))) if *pk == k && p.end == cols.start => p.end = cols.end,
                (Some(k), _) => out[i].push((cols, k)),
                _ => {}
            }
            (used, col, ci) = (used + n, col + w, ci + 1);
        }
    }
    (out, win)
}
