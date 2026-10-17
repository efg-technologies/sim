//! UI-4, COLOR-1, COLOR-3, KEY-5
//! The tab bar (pure), as the author's bufferline draws it: one tab for each open file, its icon
//! and name (with the directory above it when two open files have the same name), a dot for a
//! file that is modified and a green one for a file that changed on disk, the counts of a language
//! server's errors and warnings, the shown tab raised with a bar at its left, thin separators,
//! `‹` and `›` where tabs are hidden; and where a click on it lands.
use crate::color::{Chg, Tones};
use crate::icons;
use crate::layout::{Line, fit};
use crate::paint::{Cue, Paint, Role};
use std::ops::Range;

/// What a tab says about its file.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Tab {
    pub name: String,
    pub dirty: bool,
    /// The file changed on disk and the change is not looked at yet.
    pub disk: bool,
    /// The language server's errors and warnings for the file (the server's own code sets them).
    pub errors: usize,
    pub warns: usize,
    /// KEY-5: the letter that shows this file while the tabs are being picked from.
    pub letter: Option<char>,
}

/// KEY-5: the alphabet of bufferline's `pick.alphabet`.
pub const ALPHABET: &str = "abcdefghijklmopqrstuvwxyzABCDEFGHIJKLMOPQRSTUVWXYZ1234567890";

/// KEY-5: the letter of each tab, as bufferline's `pick.get` gives them in order: the first letter of
/// the file's name when it is in the alphabet and not taken, else the first letter of the alphabet that
/// is not (a blank for a tab beyond the alphabet).
pub fn letters(names: &[String]) -> Vec<char> {
    let mut taken: Vec<char> = vec![];
    let mut pick = |name: &String| {
        let first = name.chars().next().filter(|c| ALPHABET.contains(*c) && !taken.contains(c));
        let l = first.or_else(|| ALPHABET.chars().find(|c| !taken.contains(c))).unwrap_or(' ');
        taken.push(l);
        l
    };
    names.iter().map(&mut pick).collect()
}

/// What a click on the bar lands on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hit {
    Tab(usize),
    Prev,
    Next,
}

/// The names the tabs show for these paths: the file's name, and for the files whose names are the
/// same as another's as many components of their paths, from the end, as it takes to tell them apart.
pub fn names(paths: &[&str]) -> Vec<String> {
    // DIFF-3: HEAD's version of a file is named `HEAD:path`, and its tab says so: `HEAD:name`
    let heads: Vec<&str> =
        paths.iter().map(|p| p.strip_prefix("HEAD:").map_or("", |_| "HEAD:")).collect();
    let parts: Vec<Vec<&str>> = (paths.iter().zip(&heads))
        .map(|(p, h)| p[h.len()..].split('/').filter(|c| !c.is_empty()).collect())
        .collect();
    let mut keep = vec![1usize; paths.len()];
    let show = |i: usize, k: usize| {
        let p = &parts[i];
        let from = p.len().saturating_sub(k);
        if p.is_empty() {
            paths[i].to_string()
        } else {
            format!("{}{}", heads[i], p[from..].join("/"))
        }
    };
    for _ in 0..16 {
        let now: Vec<String> = (0..paths.len()).map(|i| show(i, keep[i])).collect();
        let mut more = false;
        for i in 0..paths.len() {
            let clash = (0..paths.len()).any(|j| j != i && now[j] == now[i]);
            if clash && keep[i] < parts[i].len() {
                keep[i] += 1;
                more = true;
            }
        }
        if !more {
            break;
        }
    }
    (0..paths.len()).map(|i| show(i, keep[i])).collect()
}

/// The tabs shown when they do not all fit in `avail` columns (`widths` are theirs, a separator
/// after each but the last counted in): the shown one, then its neighbors to the right and to the
/// left while they fit, with a column for each of `‹` and `›` where tabs are hidden.
pub fn window(widths: &[usize], cur: usize, avail: usize) -> Range<usize> {
    let n = widths.len();
    if n == 0 {
        return 0..0;
    }
    let cur = cur.min(n - 1);
    let mut room = avail;
    for _ in 0..3 {
        let (mut lo, mut hi, mut used) = (cur, cur + 1, widths[cur]);
        while hi < n && used + widths[hi] <= room {
            used += widths[hi];
            hi += 1;
        }
        while lo > 0 && used + widths[lo - 1] <= room {
            used += widths[lo - 1];
            lo -= 1;
        }
        let arrows = usize::from(lo > 0) + usize::from(hi < n);
        if used + arrows <= avail {
            return lo..hi;
        }
        room = avail.saturating_sub(arrows);
    }
    cur..cur + 1
}

/// The glyphs of the diagnostics counts: the author's shapes for severity (`✘ ▲`).
const ERROR: &str = "✘";
const WARN: &str = "▲";

/// One tab as a line, `raised` or not. Its width is what `window` is given (plus a separator).
fn tab(tb: &Tab, icons_on: bool, raised: bool, max: usize, t: Option<&Tones>) -> Line {
    let mut line = Line::default();
    let surface = t.map(|t| if raised { t.card } else { t.bar });
    let role = if raised { Role::Chip } else { Role::Bar }; // (GUI-2: the shown tab is raised from the bar)
    let base = Paint { bg: surface, bold: raised, role, ..Paint::default() };
    // The indicator: a bar at the left of the shown tab, in the accent (`indicator = { icon = '▎' }`).
    if raised {
        line.put("▎", Paint { fg: t.map(|t| t.accent_fg()), ..base });
    } else {
        line.put(" ", base);
    }
    line.put(" ", base);
    if let Some(l) = tb.letter {
        // KEY-5: the letter is where the icon is, on the accent's block (inverse without color)
        let (back, text) = t.map(|t| t.accent_block()).unzip();
        let p =
            Paint { bg: back, fg: text, bold: true, cue: Cue::Solid, role: Role::Patch, ..base };
        line.put(&format!(" {l} "), p);
        line.put(" ", base);
    } else if icons_on {
        let glyph = icons::of(&tb.name, false, false);
        line.put(&format!("{glyph} "), Paint { fg: t.map(|t| t.focus), ..base }); // (monochrome)
    }
    let extra = 4 + 2 * usize::from(icons_on || tb.letter.is_some());
    line.put(&fit(&tb.name, max.saturating_sub(extra).max(1)).0, base);
    // The counts are told by their shapes, in the text's own gray: a color is for what changed.
    for (n, shape) in [(tb.errors, ERROR), (tb.warns, WARN)] {
        if n > 0 {
            line.put(&format!(" {shape} {n}"), base);
        }
    }
    if tb.dirty {
        line.put(" ●", Paint { bold: true, ..base }); // unsaved: weight, not a color
    }
    if tb.disk {
        line.put(" ↯", Paint { fg: t.map(|t| t.change_fg(Chg::Add)), ..base }); // changed on disk: a change
    }
    line.put(" ", base);
    line
}

/// The bar of `width` columns, and where a click on it lands: the tabs of the window, thin
/// separators between them, `‹` and `›` at the ends where tabs are hidden; the rest is the bar's
/// surface. `t` is none when only the hits are wanted.
pub fn layout(
    tabs: &[Tab],
    icons_on: bool,
    cur: usize,
    width: u16,
    t: Option<&Tones>,
) -> (Line, Vec<(Range<u16>, Hit)>) {
    let w = usize::from(width);
    let lines: Vec<Line> = tabs
        .iter()
        .enumerate()
        .map(|(i, tb)| tab(tb, icons_on, i == cur, w.saturating_sub(4), t))
        .collect();
    let widths: Vec<usize> = lines
        .iter()
        .enumerate()
        .map(|(i, l)| usize::from(l.cols) + usize::from(i + 1 < lines.len()))
        .collect();
    let shown = window(&widths, cur, w);
    let bar = Paint { bg: t.map(|t| t.bar), role: Role::Bar, ..Paint::default() };
    let arrow = Paint { fg: t.map(|t| t.focus), ..bar };
    let (mut out, mut hits) = (Line::default(), vec![]);
    if shown.start > 0 {
        hits.push((out.cols..out.cols + 1, Hit::Prev));
        out.put("‹", arrow);
    }
    let last = shown.end;
    for (i, l) in lines.into_iter().enumerate().skip(shown.start).take(shown.len()) {
        let at = out.cols;
        let end = at + l.cols;
        out.add(l);
        hits.push((at..end, Hit::Tab(i)));
        if i + 1 < last {
            out.put("▏", Paint { fg: t.map(|t| t.line), ..bar });
        }
    }
    if shown.end < tabs.len() {
        out.pad(width.saturating_sub(1));
        hits.push((out.cols..out.cols + 1, Hit::Next));
        out.put("›", arrow);
    }
    // A tab that does not fit even alone is cut at the edge.
    if out.cols > width {
        let (text, _) = fit(&out.row.text, w);
        out.row.text = text;
        out.cols = width;
    }
    out.pad(width);
    out.paints.push((0..width, bar));
    (out, hits)
}

/// The tab a click at column `x` of the bar lands on, if any.
pub fn hit(tabs: &[Tab], icons_on: bool, cur: usize, width: u16, x: u16) -> Option<Hit> {
    layout(tabs, icons_on, cur, width, None)
        .1
        .into_iter()
        .find(|(r, _)| r.contains(&x))
        .map(|h| h.1)
}
