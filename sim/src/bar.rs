//! UI-10, COLOR-1, COLOR-2, UI-17
//! The status line of the interface (pure): one bar across the width, lualine's sections with
//! powerline separators when icons are on: the mode in a block of its color, the branch and the
//! changes against HEAD, the file with its icon and a dot when modified, and on the right the
//! security slot, the encoding, the line ending, the file type, the progress and the position.
//! What does not fit is dropped from the right, least important first.
use crate::color::{Chg, Tones};
use crate::layout::{Line, fit, short};
use crate::paint::{Cue, Paint};
use sim_core::core::color::Rgb;

/// `U+E0B0` and `U+E0B2`, the solid separators of Powerline, and the branch icon.
const LEFT: &str = "\u{e0b0}";
const RIGHT: &str = "\u{e0b2}";
const BRANCH: &str = "\u{e0a0}";

/// The color of the mode's block, and of the position's after it: neutral gray in Normal mode (the
/// word says the mode), the accent in any other (UI-1: blue is "here, where you act").
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Block {
    Gray,
    Accent,
}

/// What the bar shows. `None` and empty are left out.
pub struct Bar<'a> {
    pub icons: bool,
    pub mode: &'a str,
    pub block: Block,
    pub branch: Option<&'a str>,
    /// Lines added, changed and deleted against HEAD.
    pub counts: Option<(usize, usize, usize)>,
    /// The file: its icon (a glyph and its accent), its path and whether it is modified; and the core's
    /// state words after it (`2 changes on disk`).
    pub icon: Option<char>,
    pub path: &'a str,
    pub modified: bool,
    pub notes: &'a str,
    /// The security slot: the language server's state or whether the root is trusted, and whether
    /// what it says is an alert (what is hidden or dangerous: magenta).
    pub slot: Option<(&'a str, bool)>,
    pub encoding: Option<&'a str>,
    pub eol: Option<&'a str>,
    pub ftype: Option<&'a str>,
    pub progress: Option<&'a str>,
    pub pos: &'a str,
}

/// What can be dropped, in the order it is.
#[derive(Clone, Copy)]
struct Keep {
    slot: bool,
    eol: bool,
    enc: bool,
    ftype: bool,
    progress: bool,
    counts: bool,
    branch: bool,
    notes: bool,
}

/// A run of text of one color on the section's background.
struct Seg {
    text: String,
    fg: Option<Rgb>,
    bold: bool,
}

struct Sect {
    bg: Rgb,
    segs: Vec<Seg>,
}

impl Sect {
    fn cols(&self) -> usize {
        self.segs.iter().map(|s| fit(&s.text, usize::MAX).1).sum()
    }
}

fn seg(text: impl Into<String>, fg: Option<Rgb>, bold: bool) -> Seg {
    Seg { text: text.into(), fg, bold }
}

/// The sections of the left and of the right side for what is kept, the path as long as `room`.
fn sections(b: &Bar, t: &Tones, k: Keep, room: usize) -> (Vec<Sect>, Vec<Sect>) {
    let p = t.primary;
    let (back, on) = match b.block {
        Block::Gray => (t.gray, t.gray_on),
        Block::Accent => t.accent_block(),
    };
    // A: the mode.
    let mut left =
        vec![Sect { bg: back, segs: vec![seg(format!(" {} ", b.mode), Some(on), true)] }];
    // B: the branch and the changes.
    let mut parts = vec![];
    if let (true, Some(name)) = (k.branch, b.branch) {
        let icon = if b.icons { format!("{BRANCH} ") } else { String::new() };
        parts.push(seg(format!(" {icon}{name}"), Some(p), false));
    }
    if let (true, Some((a, c, d))) = (k.counts, b.counts) {
        let mut put = |n: usize, mark: &str, chg: Chg| {
            let text = if n > 0 { format!(" {mark}{n}") } else { String::new() };
            parts.push(seg(text, Some(t.change_fg(chg)), false));
        };
        put(a, "+", Chg::Add);
        put(c, "~", Chg::Mod);
        put(d, "-", Chg::Del);
    }
    if !parts.is_empty() {
        parts.push(seg(" ", None, false));
        left.push(Sect { bg: t.card, segs: parts });
    }
    // C: the file.
    let mut file = vec![seg(" ", None, false)];
    if let (true, Some(glyph)) = (b.icons, b.icon) {
        file.push(seg(format!("{glyph} "), Some(t.focus), false)); // (monochrome)
    }
    file.push(seg(short(b.path, room), Some(p), false));
    if b.modified {
        file.push(seg(" ●", Some(p), true)); // unsaved: weight, not a color
    }
    if let (true, false) = (k.notes, b.notes.is_empty()) {
        file.push(seg(format!("  {}", b.notes), Some(p), false));
    }
    file.push(seg(" ", None, false));
    left.push(Sect { bg: t.bar, segs: file });
    // X: the slot, the encoding, the line ending, the file type.
    let mut right = vec![];
    let mut x = vec![];
    if let (true, Some((text, alert))) = (k.slot, b.slot) {
        x.push(seg(format!(" {text}"), Some(if alert { t.alert_fg() } else { p }), alert));
    }
    if let (true, Some(e)) = (k.enc, b.encoding) {
        x.push(seg(format!(" {e}"), Some(p), false));
    }
    if let (true, Some(e)) = (k.eol, b.eol) {
        x.push(seg(format!(" {e}"), Some(p), false));
    }
    if let (true, Some(ft)) = (k.ftype, b.ftype) {
        x.extend(b.icon.filter(|_| b.icons).map(|g| seg(format!(" {g}"), Some(t.focus), false)));
        x.push(seg(format!(" {ft}"), Some(p), false));
    }
    if !x.is_empty() {
        x.push(seg(" ", None, false));
        right.push(Sect { bg: t.bar, segs: x });
    }
    // Y: the progress. Z: the position.
    if let (true, Some(pr)) = (k.progress, b.progress) {
        right.push(Sect { bg: t.card, segs: vec![seg(format!(" {pr} "), Some(p), false)] });
    }
    right.push(Sect { bg: back, segs: vec![seg(format!(" {} ", b.pos), Some(on), true)] });
    (left, right)
}

fn width(left: &[Sect], right: &[Sect], icons: bool) -> usize {
    let seps = if icons { left.len() + right.len() } else { 0 };
    left.iter().chain(right).map(Sect::cols).sum::<usize>() + seps
}

/// The bar for `w` columns, or none when the screen is too narrow for it (under 24 columns: the mode,
/// the file's section and the position are what is left of it).
pub fn render(w: u16, b: &Bar, t: &Tones) -> Option<Line> {
    let w = usize::from(w);
    if w < 24 {
        return None;
    }
    let mut keep = Keep {
        slot: true,
        eol: true,
        enc: true,
        ftype: true,
        progress: true,
        counts: true,
        branch: true,
        notes: true,
    };
    let drops: [fn(&mut Keep); 8] = [
        |k| k.slot = false,
        |k| k.eol = false,
        |k| k.enc = false,
        |k| k.ftype = false,
        |k| k.progress = false,
        |k| k.counts = false,
        |k| k.branch = false,
        |k| k.notes = false,
    ];
    let mut next = 0;
    let (left, right) = loop {
        let (l, r) = sections(b, t, keep, usize::MAX);
        let used = width(&l, &r, b.icons);
        if used <= w || next == drops.len() {
            // What is left of the width, besides the path, is its room.
            let path_cols = fit(b.path, usize::MAX).1;
            let spare = (w + path_cols).saturating_sub(used);
            let room = spare.max(1).min(path_cols.max(1));
            break sections(b, t, keep, room);
        }
        drops[next](&mut keep);
        next += 1;
    };
    // The path was cut to what is left, so the width fits; draw it.
    let mut line = Line::default();
    let bar = Paint { bg: Some(t.bar), ..Paint::default() };
    let draw = |line: &mut Line, s: &Sect, cue: Cue| {
        for g in &s.segs {
            let p = Paint { fg: g.fg, bg: Some(s.bg), bold: g.bold, cue, ..Paint::default() };
            line.put(&g.text, p);
        }
    };
    for (i, s) in left.iter().enumerate() {
        draw(&mut line, s, if i == 0 { Cue::Solid } else { Cue::None }); // (the mode is a block)
        let next = left.get(i + 1).map_or(t.bar, |n| n.bg);
        if b.icons {
            line.put(LEFT, Paint { fg: Some(s.bg), bg: Some(next), ..Paint::default() });
        }
    }
    let used = width(&left, &right, b.icons);
    let gap = w.saturating_sub(used);
    line.put(&" ".repeat(gap), bar);
    let mut before = t.bar; // (the left side ends with the file's section, on the bar)
    for s in &right {
        if b.icons {
            line.put(RIGHT, Paint { fg: Some(s.bg), bg: Some(before), ..Paint::default() });
        }
        draw(&mut line, s, Cue::None);
        before = s.bg;
    }
    line.pad(w as u16);
    Some(line)
}
