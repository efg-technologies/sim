//! TREE-1, TREE-2, BUF-2, FIND-1, MARK-1, MARK-2, SYN-1, SYN-5, PTR-1, LOOK-1, LOOK-2, LOOK-3, LOOK-4, LOOK-5, LOOK-6, LOOK-7, UI-8, UI-2, UI-3, UI-5, COLOR-1, UI-7, UI-11, UI-13
//! The panel and the editor composed into one screen (pure): the panel on the left with its
//! marks, a column of space, the editor with its colors, and one status line across the whole
//! width.
use crate::color::{Chg, Tones};
use crate::diff::Sign;
use crate::grep::PRow;
use crate::icons;
use crate::marks::Git;
use crate::paint::{Look, Looks, Paint, Paints};
use crate::syn::Spans;
use crate::tabs::Tab;
use crate::tree;
use si::core::app::App;
use si::core::color::Rgb;
use si::core::vi::Mode;
use si::core::view::{Frame, Row, Style, shorten, width};
use std::mem::take;
use std::ops::Range;

/// What the editor area shows while no file is open (TREE-1).
const HELP: [&str; 5] =
    ["Ctrl-n  tree", "Enter   open", "Tab     next file", "q       close", ":qa     quit all"];

/// Whether a char must not reach the terminal as it is (SAFE-8): a file name can hold anything.
fn hostile(c: char) -> bool {
    c.is_control()
        || matches!(c, '\u{61c}' | '\u{200b}' | '\u{200e}'..='\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}' | '\u{feff}')
}

/// `s` cut to `max` columns, hostile chars replaced by U+FFFD, and the columns it takes: by si's
/// own `width`, so an emoji presentation sequence takes what the editor gives it (VIS-7).
pub fn fit(s: &str, max: usize) -> (String, usize) {
    let cs: Vec<char> = s.chars().map(|c| if hostile(c) { '\u{fffd}' } else { c }).collect();
    let (mut end, mut cols) = (0, 0);
    while end < cs.len() && cols + width(&cs, end, cols) <= max {
        (end, cols) = (end + 1, cols + width(&cs, end, cols));
    }
    (cs[..end].iter().collect(), cols)
}

/// `s` cut from the left to `max` columns, so that its end stays: `…` and the last chars (a query
/// as it is typed). Hostile chars are replaced.
pub fn tail(s: &str, max: usize) -> String {
    let all = fit(s, usize::MAX).0;
    let cs: Vec<char> = all.chars().collect();
    let cols = |from: usize| fit(&cs[from..].iter().collect::<String>(), usize::MAX).1;
    if cols(0) <= max {
        return all;
    } else if max == 0 {
        return String::new();
    }
    // The first char kept is the smallest one whose tail fits next to the `…`.
    let (mut lo, mut hi) = (0, cs.len());
    while hi - lo > 1 {
        let mid = (lo + hi) / 2;
        *(if cols(mid) < max { &mut hi } else { &mut lo }) = mid;
    }
    format!("…{}", cs[hi..].iter().collect::<String>())
}

/// A path made to fit `max` columns, the way si's status line does it (`shorten`): `…/` and as
/// many of its last components as fit, the file name at least; a file name wider than that is cut
/// from the left (`tail`).
pub fn short(path: &str, max: usize) -> String {
    tail(&shorten(&fit(path, usize::MAX).0, "", max), max)
}

/// A line of a path and the words that say the state: the state is never cut, the path is made to
/// fit what is left (`short`).
pub fn stated(path: &str, state: &str, max: usize) -> String {
    let (state, used) = fit(state, max);
    short(path, max - used) + &state
}

/// What the panel shows about a row besides its name: MARK-1 (`change` the change color on the
/// name of a file, `below` the `~` of a directory), MARK-2 (`likely`, the `•`), and FIND-1
/// (`hit`: none when no search runs; bold for a match or a directory leading to one, dimmed else).
/// For the interface (UI-3): the file's git status, the strongest one below a directory (`dot`),
/// whether git ignores it, and whether it is the open file's row.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Mark {
    pub change: bool,
    pub below: bool,
    pub likely: bool,
    pub hit: Option<bool>,
    pub git: Option<Git>,
    pub dot: Option<Git>,
    pub ignored: bool,
    pub open: bool,
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

/// LOOK-1: how the screen looks. Calm is si's four styles; flat adds green and red and a tint to
/// the rows of changes; depth adds boxes, an overview ruler, a raised card in REVIEW and gradients.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Skin {
    #[default]
    Calm,
    Flat,
    Depth,
}

const SKINS: [Skin; 3] = [Skin::Calm, Skin::Flat, Skin::Depth];

impl Skin {
    /// What `zl` goes to.
    pub fn next(self) -> Skin {
        SKINS[(self as usize + 1) % 3]
    }

    pub fn name(self) -> &'static str {
        ["calm", "flat", "depth"][self as usize]
    }

    /// The word of the state file (the line break after it is allowed).
    pub fn parse(s: &str) -> Option<Skin> {
        SKINS.into_iter().find(|k| k.name() == s.trim_end_matches('\n'))
    }
}

/// LOOK-3: where the parts of a screen are, for its size, its look and a panel that is open or not:
/// the columns of the panel (`pw`, none when it is closed or too narrow) and of the editor (`ew`),
/// the column each starts at (after the left border of its box in depth), the first body row and
/// how many there are. Depth needs 40 columns and 10 rows for its boxes. With the interface
/// (UI-2, UI-4) the first row inside the boxes is a header, the panel's root and the editor's tabs
/// (`head`, 0 or 1): the body starts under it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Geo {
    pub boxed: bool,
    pub head: u16,
    /// UI-5: the columns the editor keeps to the left of its text for the signs (0 or 1; the second
    /// cell of the sign column is the App's own change symbol, the first of its row).
    pub sign: u16,
    /// UI-11: the column at the right end of the editor that holds the scrollbar where there is no
    /// box border to hold it (flat: 0 or 1).
    pub rule: u16,
    pub pw: u16,
    pub ew: u16,
    pub px: u16,
    pub ex: u16,
    pub y0: u16,
    pub rows: u16,
}

impl Geo {
    pub fn new(size: (u16, u16), skin: Skin, panel: bool) -> Geo {
        Geo::with_head(size, skin, panel, false)
    }

    /// `head`: the interface is drawn (a screen under 10 rows has no room for the header).
    pub fn with_head((w, h): (u16, u16), skin: Skin, panel: bool, head: bool) -> Geo {
        let boxed = skin == Skin::Depth && w >= 40 && h >= 10;
        let (b, pw) = (u16::from(boxed), if panel { (w / 3).min(32) } else { 0 });
        let (pw, head) = (if pw < 4 { 0 } else { pw }, u16::from(head && h >= 10));
        let ex = if pw > 0 { pw + 1 + 2 * b } else { b };
        // (the editor keeps at least 24 columns for its text)
        let sign = u16::from(head > 0 && w.saturating_sub(ex + b) >= 25);
        let rule = u16::from(head > 0 && !boxed && w.saturating_sub(ex + sign) >= 26);
        let ew = w.saturating_sub(ex + b + sign + rule);
        let y0 = b + head;
        Geo {
            boxed,
            head,
            sign,
            rule,
            pw,
            ew,
            px: b,
            ex,
            y0,
            rows: h.saturating_sub(1 + 2 * b + head),
        }
    }

    /// The column the editor's text starts at: after its sign column.
    pub fn tx(&self) -> u16 {
        self.ex + self.sign
    }
}

/// A row of the screen being put together (the status line's too): the row, the looks and the paints of its columns, and
/// how many columns it takes.
#[derive(Default)]
pub struct Line {
    pub row: Row,
    pub looks: Looks,
    pub paints: Paints,
    pub cols: u16,
}

impl Line {
    /// Writes `s` after what is there, with the paint `p` over it.
    pub fn put(&mut self, s: &str, p: Paint) {
        let (text, n) = fit(s, usize::MAX);
        let r = self.cols..self.cols + n as u16;
        self.row.text += &text;
        if p != Paint::default() {
            self.paints.push((r.clone(), p));
        }
        self.cols = r.end;
    }

    /// Puts another line after this one: its columns move by the columns this one takes.
    pub fn add(&mut self, o: Line) {
        let go = |r: Range<u16>| r.start + self.cols..r.end + self.cols;
        self.row.text += &o.row.text;
        self.row.spans.extend(o.row.spans.into_iter().map(|(r, s)| (go(r), s)));
        self.row.underline.extend(o.row.underline.into_iter().map(go));
        self.looks.extend(o.looks.into_iter().map(|(r, l)| (go(r), l)));
        self.paints.extend(o.paints.into_iter().map(|(r, p)| (go(r), p)));
        self.cols += o.cols;
    }

    /// Fills up to `to` columns with blanks.
    pub fn pad(&mut self, to: u16) {
        self.row.text += &" ".repeat(usize::from(to.saturating_sub(self.cols)));
        self.cols = self.cols.max(to);
    }
}

/// How the panel's text is faded (LOOK-7): by the distance of a row from `Near(row, top)`, the row
/// of the open file (or of the cursor while the panel has the focus) and the first row shown, or by
/// the distance from the cursor row `Find(row)` while a search has the focus.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fade {
    None,
    Near(usize, usize),
    Find(usize),
}

/// A fade in percent that grows from `from` rows away over `span` rows up to `top` (LOOK-6, LOOK-7).
fn ramp(d: usize, from: f64, span: f64, top: f64) -> u8 {
    (((d as f64 - from) / span * top).clamp(0.0, top) * 100.0).round() as u8
}

/// LOOK-7: the fade of a row `d` rows from the cursor row of a search, in percent.
fn find_fog(d: usize) -> u8 {
    ((0.12 + d as f64 / 16.0 * 0.6).min(0.7) * 100.0).round() as u8
}

/// LOOK-4: what a row of the overview ruler shows, the plain border first: the lines on screen, a
/// deletion point (red), text that was gained or changed (green). The later wins.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum Tick {
    #[default]
    Plain,
    Shown,
    /// UI-11: a match of a search (the accent), a line that was changed (amber).
    Hit,
    Loss,
    Change,
    Gain,
}

/// LOOK-4: the rows of the overview ruler: `rows` of them stand for the lines of a file of `n`
/// lines, line `l` on row `l * rows / n`; a row is `Gain` or `Loss` where one of `changes` is (a
/// range of lines and which it is; an empty range stands for its place), `Shown` where the lines
/// on screen (the first and the last) are.
pub fn ruler(
    rows: usize,
    n: usize,
    changes: impl Iterator<Item = (Range<usize>, Tick)>,
    shown: Option<(usize, usize)>,
) -> Vec<Tick> {
    let mut out = vec![Tick::Plain; rows];
    if rows == 0 || n == 0 {
        return out;
    }
    let row = |l: usize| (l.min(n - 1) * rows / n).min(rows - 1);
    if let Some((a, z)) = shown {
        out[row(a)..=row(z.max(a))].fill(Tick::Shown);
    }
    for (c, k) in changes {
        out[row(c.start)..=row(c.end.max(c.start + 1) - 1)]
            .iter_mut()
            .for_each(|t| *t = (*t).max(k));
    }
    out
}

/// UI-5: the git sign of each row of an editor frame: the row a line starts in has that line's
/// sign (`of(l)`, 0-based), the rows that go on in have none.
pub fn sign_rows(
    f: &Frame,
    digits: usize,
    of: &dyn Fn(usize) -> Option<Sign>,
) -> Vec<Option<Sign>> {
    let line = |r: &Row| number(r, digits).and_then(|n| n.checked_sub(1));
    f.rows.iter().map(|r| line(r).and_then(of)).collect()
}

/// UI-11: what a row of the scrollbar shows: the strongest mark in its range of lines (`Plain` for
/// none), whether it is in the thumb (the lines on screen), and where the cursor is in it, in quarters.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ScrollCell {
    pub mark: Tick,
    pub thumb: bool,
    pub cursor: Option<usize>,
}

/// UI-11: the scrollbar's `rows` rows for a file of `n` lines: LOOK-4's rows (`ruler`), the thumb
/// from the first to the last line `shown`, and the row of the line the `cursor` is on, with which
/// quarter of the row it is (satellite's `⎺⎻⎼⎽`).
pub fn scrollbar(
    rows: usize,
    n: usize,
    marks: impl Iterator<Item = (Range<usize>, Tick)>,
    shown: Option<(usize, usize)>,
    cursor: Option<usize>,
) -> Vec<ScrollCell> {
    let mut out: Vec<ScrollCell> = ruler(rows, n, marks, None)
        .into_iter()
        .map(|mark| ScrollCell { mark, ..ScrollCell::default() })
        .collect();
    if rows == 0 || n == 0 {
        return out;
    }
    let row = |l: usize| (l.min(n - 1) * rows / n).min(rows - 1);
    if let Some((a, z)) = shown {
        out[row(a)..=row(z.max(a))].iter_mut().for_each(|c| c.thumb = true);
    }
    if let Some(l) = cursor.map(|l| l.min(n - 1)) {
        out[row(l)].cursor = Some((l * rows % n) * 4 / n);
    }
    out
}

/// UI-11: a cell of the scrollbar, `plain` where it shows nothing: a mark is `┃` in its color (a
/// change's, the accent's for a match), the cursor `⎺⎻⎼⎽` in the secondary gray or the mark's
/// color, the thumb a surface behind them.
fn scroll_cell(c: ScrollCell, t: &Tones, plain: Line) -> Line {
    let ink = match c.mark {
        Tick::Gain => Some(t.change_fg(Chg::Add)),
        Tick::Change => Some(t.change_fg(Chg::Mod)),
        Tick::Loss => Some(t.change_fg(Chg::Del)),
        Tick::Hit => Some(t.accent_fg()),
        _ => None,
    };
    let glyph = match (c.cursor, ink) {
        (Some(q), _) => ["⎺", "⎻", "⎼", "⎽"][q.min(3)],
        (None, Some(_)) => "┃",
        (None, None) if c.thumb => " ",
        _ => return plain,
    };
    let mut l = Line::default();
    let fg = ink.or(c.cursor.map(|_| t.focus));
    l.put(glyph, Paint { fg, bg: c.thumb.then_some(t.thumb), ..Paint::default() });
    l
}

/// The line number a row of an editor frame shows, if it starts a line.
fn number(r: &Row, digits: usize) -> Option<usize> {
    r.text.get(1..1 + digits)?.trim().parse().ok()
}

/// What a row of an editor frame is (LOOK-2): no change, text that a change from disk gained or
/// changed (`Gain`), a deletion point (`Loss`, the `-` of the gutter), or, in REVIEW, a row that
/// has a change (`Hunk`: a proposal, which has both).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Plain,
    Gain,
    Loss,
    Hunk,
}

/// LOOK-2: the kind of each row of an editor frame, read from its gutter: a symbol in the first
/// column is a change, and so are the rows after it whose gutter is blank (the wrapped rows of the
/// line, or the rows a hunk goes on in).
pub fn changed(f: &Frame, digits: usize, review: bool) -> Vec<Kind> {
    let mut last = Kind::Plain;
    let body = f.rows.len().saturating_sub(1);
    f.rows[..body]
        .iter()
        .map(|r| {
            match (r.text.chars().next(), number(r, digits)) {
                (Some(' '), None) => {} // the rest of a row that has a change
                (Some(' ') | None, _) => last = Kind::Plain, // a line of no change, or past the end
                (_, _) if review => last = Kind::Hunk,
                (Some('-'), _) => last = Kind::Loss,
                _ => last = Kind::Gain,
            }
            last
        })
        .collect()
}

/// LOOK-5: the rows of the card in REVIEW: from the first changed row at or after the cursor to
/// the row before the first row numbered past the hunk's lines (`end`, one past its last), or to
/// the end of the frame.
pub fn card(f: &Frame, ch: &[Kind], digits: usize, end: usize) -> Option<Range<usize>> {
    let (cur, body) = (usize::from(f.cursor.1), f.rows.len().saturating_sub(1));
    let start = (cur..body).find(|&i| ch.get(i).is_some_and(|k| *k != Kind::Plain))?;
    let past = |i: &usize| number(&f.rows[*i], digits).is_some_and(|n| n > end);
    Some(start..(start + 1..body).find(past).unwrap_or(body))
}

/// PTR-1: the row of the frame that a row of the screen shows, with the half rows around the card
/// inserted: the half rows are the card's first and last row.
pub fn unshift(y: usize, c: &Range<usize>) -> usize {
    y - usize::from(y > c.start) - usize::from(y > c.end)
}

/// LOOK-2: a blank between two cells on the same background is on it too (`words` are the paints
/// of the changed words of the row, from `from` on).
fn join(text: &str, words: &mut Paints, from: u16) {
    let on = |w: &Paints, c: u16| w.iter().find(|(r, _)| r.contains(&c)).and_then(|p| p.1.bg);
    let (cs, mut col, mut gaps) = (text.chars().collect::<Vec<_>>(), 0u16, vec![]);
    for i in 0..cs.len() {
        let w = width(&cs, i, usize::from(col)) as u16;
        if cs[i] == ' ' && col > from {
            let bg = on(words, col - 1);
            if bg.is_some() && bg == on(words, col + 1) && on(words, col).is_none() {
                gaps.push((col..col + 1, Paint { bg, ..Paint::default() }));
            }
        }
        col += w;
    }
    words.extend(gaps);
}

/// The editor as `body` takes it: si's frame (its last row is its status line, which the screen
/// shows apart), the looks of its rows, the width of its line numbers, what each row is
/// (`changed`, empty when the look draws no tint), the rows of the card (`card`) and the row of the
/// frame the cursor is on (LOOK-6).
pub struct Ed<'a> {
    pub frame: &'a Frame,
    pub looks: &'a [Looks],
    pub extra: &'a [Extra],
    pub digits: usize,
    pub ch: Vec<Kind>,
    pub card: Option<Range<usize>>,
    pub crow: usize,
    /// UI-5: the line the cursor is on (0-based) when the editor draws relative numbers and the
    /// cursor's line, and whether the mode is Insert (the whole line has a surface then).
    pub cur_line: Option<usize>,
    pub insert: bool,
    /// UI-5: the git sign of each row (the row a line starts in carries its sign).
    pub signs: Vec<Option<Sign>>,
}

impl<'a> Ed<'a> {
    /// A frame with no changes and no card, the cursor where the frame has it (a preview).
    pub fn new(frame: &'a Frame, looks: &'a [Looks]) -> Ed<'a> {
        Ed {
            frame,
            looks,
            extra: &[],
            digits: 3,
            ch: vec![],
            card: None,
            crow: usize::from(frame.cursor.1),
            cur_line: None,
            insert: false,
            signs: vec![],
        }
    }
}

/// What `body` needs to know about the look: the geometry, the tones (none when no tint, fog or
/// color of a box can be drawn: calm, or no palette or truecolor), the fade of the panel, whether
/// the Visual selection is a gray background (SYN-5), the rows of a content search that are the
/// panel's (GREP-3), and for the borders of depth the App that has the file shown, the lines of it
/// on screen and whether the panel has the focus.
pub struct Deck<'a> {
    pub geo: Geo,
    pub tones: Option<&'a Tones>,
    pub fade: Fade,
    pub gray: bool,
    pub live: Option<Vec<PRow>>,
    pub app: Option<&'a App>,
    pub win: Option<(usize, usize)>,
    pub focus: bool,
    pub chrome: Option<Chrome<'a>>,
    /// UI-11: the scrollbar's rows, none where the interface does not draw it.
    pub scroll: Vec<ScrollCell>,
}

/// UI-2, UI-4: what the interface adds to a screen: whether the icons are drawn, the root as the
/// header shows it, and the tabs (the name of each open file, whether it is modified, and which
/// is shown).
pub struct Chrome<'a> {
    pub icons: bool,
    pub root: &'a str,
    pub tabs: Vec<Tab>,
    pub cur: usize,
}

/// The column the name of a row of the panel starts at, relative to the panel's: the margin and
/// the guides of the interface (UI-3), or calm's indent.
pub fn name_col(depth: usize, chrome: bool) -> u16 {
    (2 * depth + if chrome { 3 } else { 2 }) as u16
}

/// UI-3: the change a git status is drawn as (a rename is a modification, told by its letter).
fn git_chg(g: Git) -> Chg {
    match g {
        Git::Modified | Git::Renamed => Chg::Mod,
        Git::Added | Git::Untracked => Chg::Add,
        Git::Deleted | Git::Conflict => Chg::Del,
    }
}

/// What comes (the change style: text from disk, a proposal's words) is on green, what goes on red:
/// the paints of the two, with their text grays.
fn ink_of(t: &Tones, red: bool) -> Paint {
    let (bg, fg) = if red { (t.del, t.del_text) } else { (t.add, t.add_text) };
    Paint { bg: Some(bg), fg: Some(fg), ..Paint::default() }
}

/// A row of the panel, `pw` columns: the name with its marks (on green if the file changed), the
/// selected row underlined; or, in a content search, a row of its results (GREP-3).
fn panel_line(pw: u16, at: Option<&(tree::Row, Mark)>, sel: bool, t: Option<&Tones>) -> Line {
    let mut line = Line::default();
    if let Some((r, m)) = at {
        let (text, used) = fit(&label(r, m), usize::from(pw));
        line.put(&text, Paint::default());
        let start = (2 * r.depth + 2).min(used);
        let end = (start + fit(&r.name, usize::MAX).1 + usize::from(r.is_dir)).min(used);
        if m.change {
            line.row.spans.push((start as u16..end as u16, Style::Change));
            line.paints.extend(t.map(|t| (start as u16..end as u16, ink_of(t, false))));
        }
        match m.hit {
            Some(false) => line.row.spans.push((0..used as u16, Style::Secondary)),
            Some(true) => line.looks.push((0..used as u16, Look::Bold)),
            None => {}
        }
        line.row.underline.extend(sel.then_some(0..used as u16));
    }
    line.pad(pw);
    line
}

/// UI-2, UI-3: a row of the panel in flat and depth: a margin (`•` for a likely file, MARK-2), the
/// guides of the levels above, the icon (or calm's `▸ `), the name in the color of its git status
/// (italic when untracked, bold when in conflict, the secondary gray for a dotfile or an ignored
/// entry), and at the edge the git letter, or a dot for a directory with something below it.
fn chrome_line(pw: u16, at: Option<&(tree::Row, Mark)>, sel: bool, t: &Tones, icons: bool) -> Line {
    let mut line = Line::default();
    let Some((r, m)) = at else {
        line.pad(pw);
        return line;
    };
    let ink = |c: Rgb| Paint { fg: Some(c), ..Paint::default() };
    let room = usize::from(pw.saturating_sub(2)); // a blank and the letter are the edge's
    let put = |line: &mut Line, s: &str, p: Paint| {
        let (s, _) = fit(s, room.saturating_sub(usize::from(line.cols)));
        line.put(&s, p);
    };
    let dim = (r.name.starts_with('.') || m.ignored) && m.git.is_none() && !m.change;
    put(&mut line, if m.likely { "•" } else { " " }, ink(t.focus));
    for _ in 0..r.depth {
        put(&mut line, "│ ", ink(t.line));
    }
    if icons {
        let glyph = icons::of(&r.name, r.is_dir, r.open);
        put(&mut line, &format!("{glyph} "), ink(t.focus)); // (monochrome)
    } else {
        put(
            &mut line,
            if !r.is_dir {
                "  "
            } else if r.open {
                "▾ "
            } else {
                "▸ "
            },
            Paint::default(),
        );
    }
    let start = line.cols;
    let text = if r.is_dir && !icons { format!("{}/", r.name) } else { r.name.clone() };
    put(&mut line, &text, Paint::default());
    let (name, used) = (start..line.cols, line.cols);
    if m.change {
        line.row.spans.push((name.clone(), Style::Change));
        line.paints.push((name.clone(), ink_of(t, false)));
    } else if let Some(g) = m.git {
        let bold = g == Git::Conflict;
        line.paints.push((name.clone(), Paint { bold, ..ink(t.change_fg(git_chg(g))) }));
        line.looks.extend((g == Git::Untracked).then_some((name.clone(), Look::Em)));
    } else if dim {
        line.row.spans.push((name.clone(), Style::Secondary));
    }
    match m.hit {
        Some(false) => line.row.spans.push((0..used, Style::Secondary)),
        Some(true) => line.looks.push((0..used, Look::Bold)),
        None => {}
    }
    line.row.underline.extend(sel.then_some(0..used));
    line.pad(pw.saturating_sub(2));
    line.put(" ", Paint::default());
    match (m.git, m.dot) {
        (Some(g), _) => {
            let (c, bold) = (t.change_fg(git_chg(g)), g == Git::Conflict);
            line.put(&g.letter().to_string(), Paint { bold, ..ink(c) });
        }
        (_, Some(g)) => line.put("●", ink(t.change_fg(git_chg(g)))),
        _ => {}
    }
    line.pad(pw);
    line
}

/// UI-2: the first row of the panel: the root, home-relative, in bold, with the open folder.
fn header_line(pw: u16, root: &str, icons: bool, t: &Tones) -> Line {
    let mut line = Line::default();
    let bar = Paint { bg: Some(t.bar), ..Paint::default() };
    line.put(" ", bar);
    if icons {
        line.put(&format!("{} ", icons::OPEN), Paint { fg: Some(t.focus), ..bar });
    }
    let room = usize::from(pw).saturating_sub(usize::from(line.cols) + 1);
    line.put(&tail(root, room), Paint { bold: true, ..bar });
    line.pad(pw);
    line.paints.push((0..pw, bar));
    line
}

/// UI-4: the first row of the editor: the tab bar (`tabs.rs`).
fn tabs_line(ew: u16, c: &Chrome, t: &Tones) -> Line {
    crate::tabs::layout(&c.tabs, c.icons, c.cur, ew, Some(t)).0
}

fn live_line(pw: u16, p: Option<&PRow>) -> Line {
    let mut l = Line::default();
    if let Some(p) = p {
        l.put(&p.text, Paint::default());
        l.row.spans.extend((!p.dim.is_empty()).then(|| (p.dim.clone(), Style::Secondary)));
        l.row.underline.extend(p.sel.then_some(0..l.cols));
        l.looks.extend(p.bold.iter().map(|r| (r.clone(), Look::Bold)));
    }
    l.pad(pw);
    l
}

/// The tint and the fade of a row of the panel (LOOK-2, LOOK-7): a changed file's row is tinted
/// across; the others fade by the distance from the open file's row (or the cursor), or from the
/// cursor row while a search has the focus (but not its bold rows).
fn panel_paint(i: usize, m: &Mark, fade: Fade, t: &Tones) -> Paint {
    // A row with a git status never fades: it is news (UI-3).
    let status = m.git.is_some() || m.dot.is_some();
    let fade = match fade {
        Fade::Near(a, top) if !m.change && !status => ramp((top + i).abs_diff(a), 1.0, 14.0, 0.6),
        Fade::Find(c) if m.hit == Some(false) => find_fog(i.abs_diff(c)),
        _ => 0,
    };
    let bg = if m.change { Some(t.gain) } else { m.open.then_some(t.card) };
    Paint { bg, fade, ..Paint::default() }
}

/// UI-8: the markdown of a row of the editor, in flat and depth: the surfaces (a fenced block
/// across the row, inline code), blue for a link and a level-5 heading, a quote's `>` drawn as a
/// bar in its own cell, and the language a fence names, dim, at the right of its row.
fn extra_row(line: &mut Line, m: &Extra, t: &Tones, gutter: u16, ew: u16) {
    let on = |bg| Paint { bg: Some(bg), ..Paint::default() };
    let link = Paint { fg: Some(t.accent_fg()), ..Paint::default() };
    let mut paints: Paints = line
        .looks
        .iter()
        .filter_map(|(r, l)| match l {
            Look::Code | Look::Tick => Some((r.clone(), on(t.code))),
            Look::Link => Some((r.clone(), link)),
            _ => None,
        })
        .collect();
    // UI-7: one style for every keyword: bold primary on the card's surface.
    let badge = Paint { bg: Some(t.card), fg: Some(t.primary), bold: true, ..Paint::default() };
    paints.extend(m.badge.iter().map(|r| (r.clone(), badge)));
    // UI-13: the yank, lit in the accent's block, over everything else of the cell.
    let (back, text) = t.accent_block();
    let flash = Paint { bg: Some(back), fg: Some(text), ..Paint::default() };
    paints.splice(0..0, m.flash.iter().map(|r| (r.clone(), flash)));
    if !m.glyph.is_empty() {
        let mut cs: Vec<char> = line.row.text.chars().collect();
        for g in &m.glyph {
            if g.idx >= cs.len() {
                cs.resize(g.idx, ' ');
                cs.push(g.ch);
            } else {
                cs[g.idx] = g.ch;
            }
            let ink = match g.tone {
                Tone::Plain => None,
                Tone::Dim | Tone::Guide => Some(t.line),
                Tone::Scope => Some(t.focus),
            };
            let mark = Paint { fg: ink, ..Paint::default() };
            paints.extend(ink.map(|_| (g.col..g.col + 1, mark)));
        }
        line.row.text = cs.into_iter().collect();
        line.cols = fit(&line.row.text, usize::MAX).1 as u16;
    }
    let lang = m.lang.as_ref().map(|l| fit(l, usize::from(ew)));
    if let Some((lang, w)) = lang.filter(|(_, w)| line.cols + 2 + *w as u16 <= ew) {
        line.pad(ew - w as u16 - 1);
        line.looks.push((line.cols..line.cols + w as u16, Look::Lang));
        line.put(&lang, Paint::default());
    }
    if m.band {
        line.pad(ew);
        paints.push((gutter..ew, on(t.band)));
    }
    line.paints.extend(paints);
}

/// The rows of the editor, `ew` columns each where something is drawn behind the blanks and as
/// they are else: si's rows with the looks; in flat and depth (`t`) the words of the change style
/// on green or red, the tint of the rows that have a change, the joined words and the bar of the
/// gutter; in depth the card (its tint and its half rows) and the fog of the rows that are no change
/// (LOOK-2, LOOK-5, LOOK-6).
fn editor(g: &Geo, ed: Option<&Ed>, gray: bool, t: Option<&Tones>) -> Vec<Line> {
    let (rows, ew, to) = (usize::from(g.rows), g.ew, if g.boxed || g.rule > 0 { g.ew } else { 0 });
    // The sign column's first cell, ahead of the App's row (UI-5); `bg` is the cursor's line.
    let signed = |l: Line, bg: Option<Rgb>, sign: Option<Sign>| {
        if g.sign == 0 {
            return l;
        }
        let mut s = Line::default();
        let (glyph, accent) = match sign {
            Some(Sign::Add) => ("▎", Some(Chg::Add)),
            Some(Sign::Change) => ("▎", Some(Chg::Mod)),
            Some(Sign::Del) => ("▁", Some(Chg::Del)),
            Some(Sign::DelTop) => ("▔", Some(Chg::Del)),
            None => (" ", None),
        };
        s.put(
            glyph,
            Paint { bg, fg: accent.zip(t).map(|(a, t)| t.change_fg(a)), ..Paint::default() },
        );
        s.add(l);
        s
    };
    let Some(ed) = ed else {
        let help = |i: usize| {
            let mut l = Line::default();
            if let Some(h) = HELP.get(i) {
                l.put(&fit(h, usize::from(ew)).0, Paint::default());
                l.row.spans.push((0..l.cols, Style::Secondary));
            }
            l.pad(to);
            signed(l, None, None)
        };
        return (0..rows).map(help).collect();
    };
    let gutter = ed.digits as u16 + 2;
    let half = |glyph: &str, bg: Option<Rgb>| {
        let mut l = Line::default();
        l.pad(gutter);
        let p = Paint { fg: t.map(|t| t.card), bg, ..Paint::default() };
        l.put(&glyph.repeat(usize::from(ew.saturating_sub(gutter))), p);
        l.pad(ew);
        signed(l, None, None)
    };
    // UI-5: the line each row belongs to (1-based; none for a row past the end, or the rest of a
    // line that began above the screen).
    let mut owner = Vec::with_capacity(ed.frame.rows.len());
    let mut last = None;
    for r in &ed.frame.rows {
        last = if r.text.is_empty() { None } else { number(r, ed.digits).or(last) };
        owner.push(last);
    }
    // UI-6: the colorcolumn, the author's `colorcolumn = 100`: a pale band at the hundredth column of
    // the text on every row, where the pane is wide enough for it.
    let cc = t.filter(|_| ed.cur_line.is_some() && usize::from(ew) >= usize::from(gutter) + 100);
    let band = |l: &mut Line| {
        if let Some(t) = cc {
            l.pad(gutter + 100);
            l.paints
                .push((gutter + 99..gutter + 100, Paint { bg: Some(t.cc), ..Paint::default() }));
        }
    };
    let mut out = Vec::with_capacity(rows);
    for (i, e) in ed.frame.rows[..ed.frame.rows.len().saturating_sub(1)].iter().enumerate() {
        let cols = if e.text.is_ascii() { e.text.len() } else { fit(&e.text, usize::MAX).1 };
        let mut line = Line { row: e.clone(), cols: cols as u16, ..Line::default() };
        // The Visual selection is a gray background instead of an underline (SYN-5), ahead of the syntax looks.
        if gray {
            line.looks =
                take(&mut line.row.underline).into_iter().map(|r| (r, Look::Sel)).collect();
        }
        line.looks.extend(ed.looks[i].iter().cloned());
        let kind = ed.ch.get(i).copied().unwrap_or(Kind::Plain);
        if let (Some(t), true) = (t, kind != Kind::Plain) {
            // Words that come are on green, the words that go (REVIEW) on red, struck and dim; the
            // gutter's symbol by its own char.
            let sym = e.text.chars().next();
            let mut words = Paints::new();
            for (r, style) in &e.spans {
                let (red, text) = match style {
                    Style::Deleted => (true, false),
                    Style::Change => (r.start == 0 && sym == Some('-'), true),
                    _ => continue,
                };
                let p = ink_of(t, red);
                words.push((r.clone(), Paint { fg: p.fg.filter(|_| text), ..p }));
            }
            join(&e.text, &mut words, gutter);
            if e.text.starts_with(' ') {
                let bg = Some(if kind == Kind::Loss { t.del } else { t.add }); // the bar goes on over the wrapped rows
                words.push((0..1, Paint { bg, ..Paint::default() }));
            }
            line.paints.extend(words);
            line.pad(ew);
            let raised = ed.card.as_ref().is_some_and(|c| c.contains(&i));
            let bg = match kind {
                Kind::Gain => t.gain,
                Kind::Loss => t.loss,
                _ if raised => t.card,
                _ => t.hunk,
            };
            line.paints.push((gutter..ew, Paint { bg: Some(bg), ..Paint::default() }));
        } else if let (Some(_), true) = (t, g.boxed) {
            let fade = match &ed.card {
                Some(c) => {
                    ramp(if i < c.start { c.start - i } else { i + 1 - c.end }, 2.0, 11.0, 0.6)
                }
                None => ramp(i.abs_diff(ed.crow), 3.0, 24.0, 0.45),
            };
            if fade > 0 {
                line.paints.push((0..ew, Paint { fade, ..Paint::default() }));
            }
        }
        if let (Some(t), Some(m)) = (t, ed.extra.get(i)) {
            extra_row(&mut line, m, t, gutter, ew);
        }
        // UI-5: the numbers as Neovim's `number` and `relativenumber` draw them: the cursor's line
        // is absolute, left-aligned, amber and bold; the others are the distance from it. In Insert
        // mode the whole line has a surface, else only its number is lit.
        let mut sign_bg = None;
        if let (Some(t), Some(cl)) = (t, ed.cur_line) {
            let digits = ed.digits;
            if let Some(n) = number(e, digits) {
                let shown = if n == cl + 1 {
                    // the cursor's line: the primary gray and weight (a color is for what changed)
                    let lit = Paint { fg: Some(t.primary), bold: true, ..Paint::default() };
                    line.paints.insert(0, (1..1 + digits as u16, lit));
                    format!("{n:<digits$}")
                } else {
                    format!("{:>digits$}", n.abs_diff(cl + 1))
                };
                line.row.text.replace_range(1..1 + digits, &shown);
            }
            if ed.insert && owner[i] == Some(cl + 1) {
                line.pad(ew);
                line.paints.push((0..ew, Paint { bg: Some(t.cursor), ..Paint::default() }));
                sign_bg = Some(t.cursor);
            }
        }
        band(&mut line);
        line.pad(to);
        if ed.card.as_ref().is_some_and(|c| c.start == i) {
            out.push(half("▄", None));
        }
        out.push(signed(line, sign_bg, ed.signs.get(i).copied().flatten()));
        if ed.card.as_ref().is_some_and(|c| c.end == i + 1) {
            out.push(half("▀", t.map(|t| t.shadow)));
        }
    }
    out.resize_with(rows, || {
        let mut l = Line::default();
        band(&mut l);
        signed(l, None, None)
    });
    out
}

/// A border row (LOOK-3): `ends` are its corners, `left` follows the first one and `right` comes
/// before the last, with `─` between all of them, in `line`'s paint. `w` columns; what `left` has
/// more than the room is cut.
fn border(
    w: u16,
    ends: [char; 2],
    line: Paint,
    left: &[(String, Paint)],
    right: &[(String, Paint)],
) -> Line {
    let mut l = Line::default();
    l.put(&format!("{}─", ends[0]), line);
    let used: u16 = right.iter().map(|s| fit(&s.0, usize::MAX).1 as u16).sum();
    let room = |l: &Line| usize::from(w.saturating_sub(l.cols + used + 2));
    for (s, p) in left {
        l.put(&fit(s, room(&l)).0, *p);
    }
    l.put(&"─".repeat(room(&l)), line);
    right.iter().for_each(|(s, p)| l.put(s, *p));
    l.put(&format!("─{}", ends[1]), line);
    l
}

/// The top and the bottom border of the panel's box and the editor's (LOOK-3): numbers, titles,
/// badges (changes on disk on green, proposals on the neutral surface), the hunk REVIEW is on, and
/// its keys.
fn borders(d: &Deck) -> [Line; 2] {
    let (g, t) = (&d.geo, d.tones);
    let paint = |fg: Option<Rgb>, bold| Paint { fg, bold, ..Paint::default() };
    let own = |panel: bool| panel == d.focus; // whether that box has the focus
    let line = |panel| paint(t.map(|t| if own(panel) { t.focus } else { t.line }), false);
    let title = |panel| paint(t.filter(|_| !own(panel)).map(|t| t.dim), true);
    let num = |panel| paint(t.map(|t| if own(panel) { t.focus } else { t.dim }), false);
    let s = |text: &str, p| (text.to_string(), p);
    let (pw, ew) = (g.pw + 2, g.ew + g.sign + 2);
    let (mut top, mut bottom) = (Line::default(), Line::default());
    let files = [s("¹", num(true)), s("files", title(true))];
    if g.pw > 0 {
        top.add(border(pw, ['╭', '╮'], line(true), &files, &[]));
        bottom.add(border(pw, ['╰', '╯'], line(true), &[], &[]));
    }
    let (mut left, mut right, mut keys) = (vec![], vec![], vec![]);
    if let Some(a) = d.app {
        let b = &a.buf;
        // A change on disk is a run of marked lines next to each other (si's status line counts so).
        let runs =
            (0..b.marks.len()).filter(|&i| i == 0 || b.marks[i - 1].line + 1 < b.marks[i].line);
        let badge =
            |n: usize, one: &str, many: &str| format!(" {n} {} ", if n == 1 { one } else { many });
        let surface =
            t.map(|t| Paint { bg: Some(t.card), fg: Some(t.primary), ..Paint::default() });
        let notes = [
            (runs.count(), ["change on disk", "changes on disk"], t.map(|t| ink_of(t, false))),
            (b.hunks.len(), ["proposal", "proposals"], surface),
        ];
        let notes: Vec<(String, Paint)> = notes
            .into_iter()
            .filter(|n| n.0 > 0)
            .map(|(n, w, p)| (badge(n, w[0], w[1]), p.unwrap_or(paint(None, true))))
            .collect();
        if a.vi.mode == Mode::Review {
            right.push(s(&format!(" {}/{} ", a.current() + 1, b.hunks.len()), paint(None, true)));
            keys.push(s(" REVIEW ", paint(None, true)));
            let (key, word) = (paint(None, true), paint(t.map(|t| t.focus), false));
            for (k, w) in
                [("y", "accept"), ("n", "reject"), ("Y", "all"), ("N", "none"), ("Esc", "back")]
            {
                let gap = if keys.len() == 1 { " " } else { "  " };
                keys.extend([s(gap, Paint::default()), s(k, key), s(&format!(" {w}"), word)]);
            }
            keys.push(s(" ", Paint::default()));
        }
        // The title keeps what the number, the badges and ` i/n ` leave (10 columns at least).
        let taken: usize = notes.iter().chain(&right).map(|n| fit(&n.0, usize::MAX).1 + 1).sum();
        let path = a.path.as_deref().unwrap_or("[no name]");
        left = vec![
            s("²", num(false)),
            (short(path, usize::from(ew).saturating_sub(5 + taken).max(10)), title(false)),
        ];
        for n in notes {
            left.extend([s("─", line(false)), n]);
        }
    }
    top.add(border(ew, ['╭', '╮'], line(false), &left, &right));
    bottom.add(border(ew, ['╰', '╯'], line(false), &keys, &[]));
    [top, bottom]
}

/// LOOK-4: the rows of the ruler of the file `d.app` has shown.
fn ticks(d: &Deck) -> Vec<Tick> {
    let rows = usize::from(d.geo.rows);
    let Some(b) = d.app.map(|a| &a.buf) else { return vec![Tick::Plain; rows] };
    let loss = |gone: bool| if gone { Tick::Loss } else { Tick::Gain };
    let marks = b.marks.iter().map(|m| (m.line..m.line + 1, loss(m.kind == '-')));
    let hunks = b.hunks.iter().map(|h| (h.mine.clone(), loss(h.theirs.is_empty())));
    ruler(rows, b.lines(), marks.chain(hunks), d.win)
}

/// The body rows of the screen (all but the status line), their looks and their paints: the panel
/// (the rows `panel`, `sel` the one that is underlined), a column of space and the editor; in depth
/// the boxes around them, with a title, the keys and the overview ruler in their borders.
pub fn body(
    d: &Deck,
    panel: &[(tree::Row, Mark)],
    sel: Option<usize>,
    ed: Option<&Ed>,
) -> (Vec<Row>, Vec<Looks>, Vec<Paints>) {
    let (g, t) = (&d.geo, d.tones);
    let cell = |s: &str, fg: Option<Rgb>| {
        let mut l = Line::default();
        l.put(s, Paint { fg, ..Paint::default() });
        l
    };
    let edge = |panel: bool| cell("│", t.map(|t| if panel == d.focus { t.focus } else { t.line }));
    let (mut lines, ticks) = (vec![], if g.boxed { ticks(d) } else { vec![] });
    let [top, bottom] = if g.boxed { borders(d).map(Some) } else { [None, None] };
    lines.extend(top);
    // A row of the screen: the panel's part, the editor's, and what ends it (the ruler's cell).
    let compose = |p: Option<Line>, e: Line, end: Line| {
        let mut line = Line::default();
        if g.pw > 0 {
            if g.boxed {
                line.add(edge(true));
            }
            line.add(p.unwrap_or_default());
            line.add(if g.boxed { edge(true) } else { cell(" ", None) });
        }
        if g.boxed {
            line.add(edge(false));
        }
        line.add(e);
        if g.boxed || g.rule > 0 {
            line.add(end);
        }
        line
    };
    if let (Some(c), Some(t), true) = (&d.chrome, t, g.head > 0) {
        let p = (g.pw > 0).then(|| header_line(g.pw, c.root, c.icons, t));
        let end = if g.boxed {
            edge(false)
        } else {
            let mut l = Line::default();
            l.put(" ", Paint { bg: Some(t.bar), ..Paint::default() });
            l
        };
        lines.push(compose(p, tabs_line(g.ew + g.sign, c, t), end));
    }
    for (i, e) in editor(g, ed, d.gray, t).into_iter().enumerate() {
        let p = (g.pw > 0).then(|| {
            let found = d.live.as_ref().map(|rows| rows.get(i));
            let mut p = match (found, &d.chrome, t) {
                (Some(row), ..) => live_line(g.pw, row),
                (_, Some(c), Some(t)) => {
                    chrome_line(g.pw, panel.get(i), sel == Some(i), t, c.icons)
                }
                _ => panel_line(g.pw, panel.get(i), sel == Some(i), t),
            };
            // (the rows of a search fade by their distance from the cursor, but the ones with a match)
            let paint = match (found, d.fade, t) {
                (Some(Some(r)), Fade::Find(c), Some(_)) if r.bold.is_empty() => {
                    Some(Paint { fade: find_fog(i.abs_diff(c)), ..Paint::default() })
                }
                (Some(_), ..) => None,
                _ => t.zip(panel.get(i)).map(|(t, (_, m))| panel_paint(i, m, d.fade, t)),
            };
            p.paints.extend(paint.filter(|p| *p != Paint::default()).map(|p| (0..g.pw, p)));
            p
        });
        let plain = if g.boxed { edge(false) } else { cell(" ", None) };
        let end = match (d.scroll.get(i), t) {
            (Some(c), Some(t)) => scroll_cell(*c, t, plain),
            _ if g.boxed => match ticks[i] {
                Tick::Gain => cell("┃", t.map(|t| t.add)),
                Tick::Loss => cell("┃", t.map(|t| t.del)),
                Tick::Shown if t.is_some() => cell("┃", t.map(|t| t.line)),
                _ => edge(false),
            },
            _ => Line::default(),
        };
        lines.push(compose(p, e, end));
    }
    lines.extend(bottom);
    let mut out: (Vec<Row>, Vec<Looks>, Vec<Paints>) = Default::default();
    for l in lines {
        out.0.push(l.row);
        out.1.push(l.looks);
        out.2.push(l.paints);
    }
    out
}

/// How `cs[i]` of the buffer is written in a row of the editor when it starts at column `col`
/// (SAFE-8, VIS-7, VIS-8; view.rs): the chars it takes in the row's text and the columns, which
/// are si's own `width`. A tab and a char shown as `^X` or `<U+XXXX>` take as many chars as
/// columns; a U+FE0F after one of those, or after another U+FE0F, is not written (VIS-7).
fn shown(cs: &[char], i: usize, col: usize) -> (usize, usize) {
    let (w, token) = (width(cs, i, col), |c: char| c != '\t' && hostile(c));
    let elided = cs[i] == '\u{fe0f}' && i > 0 && (token(cs[i - 1]) || cs[i - 1] == '\u{fe0f}');
    (
        if elided {
            0
        } else if cs[i] == '\t' || token(cs[i]) {
            w
        } else {
            1
        },
        w,
    )
}

/// How a glyph put over a char of a row is drawn: as it is (a quote's bar has the look of its
/// char), dim (the whitespace marks, UI-6), an indent guide, or the guide of the scope around the
/// cursor, which is brighter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tone {
    Plain,
    Dim,
    Guide,
    Scope,
}

/// A char drawn in the place of a cell of a row: the index of the char it stands for in the row's
/// text (past the end, the text is padded with blanks up to it), the column of the cell, and its tone.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Glyph {
    pub idx: usize,
    pub ch: char,
    pub col: u16,
    pub tone: Tone,
}

/// UI-8, UI-6: what a row of an editor frame has besides the looks of its chars: a line of a fenced
/// block (a surface across the row), the chars drawn as others (a quote's `>` as `▌`, the indent
/// guides), and the language a fence names, for the right of its row.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Extra {
    pub band: bool,
    pub glyph: Vec<Glyph>,
    pub lang: Option<String>,
    /// UI-7: the columns of a TODO badge in this row.
    pub badge: Vec<std::ops::Range<u16>>,
    /// UI-13: the columns of the text a yank has just taken, in this row.
    pub flash: Vec<std::ops::Range<u16>>,
}

/// UI-6: what the interface wants drawn on a line of the buffer: the columns of its indent guides
/// and which of them is the scope's around the cursor.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LineUi {
    pub guides: Vec<u16>,
    pub scope: Option<u16>,
    /// UI-13: the chars of the line a yank has just taken.
    pub flash: Option<std::ops::Range<usize>>,
}

/// SYN-1: what `colors` finds in an editor frame: the looks of its rows, the first and the last
/// line on screen, and the extras of each row (markdown, guides).
#[derive(Debug, Default)]
pub struct Colored {
    pub looks: Vec<Looks>,
    pub win: Option<(usize, usize)>,
    pub extra: Vec<Extra>,
}

/// UI-7: the keywords of a TODO comment, the longest of those that start alike first.
const TODO: [&str; 10] =
    ["FIXME", "WARNING", "FIX", "BUG", "HACK", "WARN", "PERF", "TEST", "TODO", "NOTE"];

/// UI-7: the chars of a badge in a line (the keyword, the `(name)` it may have and the colon) if one
/// of its comments has one: a keyword in capitals as a word, followed by `:` or by `(name):`. Only
/// the first is a badge. `spans` are the highlighter's, and the badge is in one of its comments.
pub fn todo_badge(line: &str, spans: &Spans) -> Option<std::ops::Range<usize>> {
    let cs: Vec<char> = line.chars().collect();
    let word = |c: char| c.is_alphanumeric() || c == '_';
    for (range, _) in spans.iter().filter(|s| s.1 == Look::Com) {
        for i in range.clone().filter(|&i| i < cs.len() && (i == 0 || !word(cs[i - 1]))) {
            for k in TODO {
                let end = i + k.len();
                if end > cs.len() || cs[i..end].iter().collect::<String>() != k {
                    continue;
                }
                let colon = match cs.get(end) {
                    Some(':') => end,
                    Some('(') => match cs[end..].iter().position(|&c| c == ')') {
                        Some(p) if cs.get(end + p + 1) == Some(&':') => end + p + 1,
                        _ => continue,
                    },
                    _ => continue,
                };
                if colon < range.end {
                    return Some(i..colon + 1);
                }
            }
        }
    }
    None
}

/// SYN-1: the looks of the rows of an editor frame. `digits` is the width of its line numbers,
/// `text(l)` the buffer's line `l` and `spans(l)` the syntax spans of it, if they are known.
/// The line of a row is read from its gutter, and a wrapped line goes on in the rows below it.
/// Also the first and the last line on screen, and the markdown of each row (UI-8).
pub fn colors<'a>(
    f: &Frame,
    digits: usize,
    text: &dyn Fn(usize) -> Option<String>,
    spans: &dyn Fn(usize) -> Option<&'a Spans>,
    ui: Option<&dyn Fn(usize) -> LineUi>,
) -> Colored {
    let (gutter, mut win) = (digits + 2, None);
    let (mut out, mut md) =
        (vec![Looks::new(); f.rows.len()], vec![Extra::default(); f.rows.len()]);
    let (mut cs, mut sp, mut ci, mut band) = (Vec::<char>::new(), None, 0, false);
    let (mut lui, mut trail, mut badge) = (LineUi::default(), 0, 0..0);
    for (i, row) in f.rows.iter().enumerate() {
        let (mut col, mut used, len) = (0, 0, row.text.chars().count().saturating_sub(gutter));
        let starts = number(row, digits).and_then(|n| n.checked_sub(1));
        if let Some(l) = starts {
            (cs, sp, ci) = (text(l).unwrap_or_default().chars().collect(), spans(l), 0);
            win = Some((win.map_or(l, |w: (usize, usize)| w.0), l));
            // a fenced block, and a first-level heading (a faint band of its own)
            band = sp.is_some_and(|s| {
                s.first().is_some_and(|s| s.1 == Look::Band) || s.iter().any(|s| s.1 == Look::H1)
            });
            let lang = sp.and_then(|s| s.iter().find(|s| s.1 == Look::Lang));
            md[i].lang = lang.map(|s| cs[s.0.clone()].iter().collect());
            lui = ui.map_or_else(LineUi::default, |f| f(l));
            trail = cs.iter().rposition(|c| !matches!(c, ' ' | '\t')).map_or(0, |i| i + 1);
            badge = sp
                .filter(|_| ui.is_some())
                .and_then(|s| todo_badge(&cs.iter().collect::<String>(), s))
                .unwrap_or(0..0);
        }
        // (a row with no text and no number is past the end of the file, not the rest of a line)
        md[i].band = band && (starts.is_some() || len > 0);
        while used < len && ci < cs.len() {
            let (n, w) = shown(&cs, ci, col);
            let look = sp.and_then(|s| s.get(s.partition_point(|s| s.0.end <= ci)));
            let look = look.filter(|s| s.0.contains(&ci) && w > 0).map(|s| s.1);
            let cols = (gutter + col) as u16..(gutter + col + w) as u16;
            if look == Some(Look::Quote) && n == 1 {
                md[i]
                    .glyph
                    .push(Glyph { idx: gutter + used, ch: '▌', col: 0, tone: Tone::Plain });
            }
            // UI-13: the cells of the text a yank has just taken, merged where they touch.
            if lui.flash.as_ref().is_some_and(|f| f.contains(&ci)) {
                let cols = (gutter + col) as u16..(gutter + col + w) as u16;
                match md[i].flash.last_mut() {
                    Some(b) if b.end == cols.start => b.end = cols.end,
                    _ => md[i].flash.push(cols),
                }
            }
            // UI-7: the cells of a TODO badge, merged where they touch.
            if badge.contains(&ci) {
                let cols = (gutter + col) as u16..(gutter + col + w) as u16;
                match md[i].badge.last_mut() {
                    Some(b) if b.end == cols.start => b.end = cols.end,
                    _ => md[i].badge.push(cols),
                }
            }
            // UI-6: the whitespace marks of `listchars`: a tab as `»-` to its width, a trailing blank
            // as `-`, a no-break space as `%`, all dim; an indent guide, drawn after, wins a blank.
            if ui.is_some() {
                let at = (gutter + used, (gutter + col) as u16);
                let mark = |k: usize, ch: char| Glyph {
                    idx: at.0 + k,
                    ch,
                    col: at.1 + k as u16,
                    tone: Tone::Dim,
                };
                match cs[ci] {
                    '\t' => {
                        md[i].glyph.extend((0..n).map(|k| mark(k, if k == 0 { '»' } else { '-' })))
                    }
                    '\u{a0}' => md[i].glyph.push(mark(0, '%')),
                    ' ' if ci >= trail => md[i].glyph.push(mark(0, '-')),
                    _ => {}
                }
            }
            // UI-6: an indent guide where a blank is at a guide's column.
            if starts.is_some() && cs[ci] == ' ' && lui.guides.contains(&(col as u16)) {
                let tone = if lui.scope == Some(col as u16) { Tone::Scope } else { Tone::Guide };
                md[i].glyph.push(Glyph {
                    idx: gutter + used,
                    ch: '│',
                    col: (gutter + col) as u16,
                    tone,
                });
            }
            match (look, out[i].last_mut()) {
                (Some(k), Some((p, pk))) if *pk == k && p.end == cols.start => p.end = cols.end,
                (Some(k), _) => out[i].push((cols, k)),
                _ => {}
            }
            (used, col, ci) = (used + n, col + w, ci + 1);
        }
        // UI-6: `↲` after the last char of a line, where the row has a cell left for it.
        let room = gutter + col < usize::from(f.width);
        if let (Some(_), true, true) = (ui, room, ci == cs.len() && (starts.is_some() || len > 0)) {
            md[i].glyph.push(Glyph {
                idx: gutter + used,
                ch: '↲',
                col: (gutter + col) as u16,
                tone: Tone::Dim,
            });
        }
        // UI-6: a blank line has its guides past the end of its own blanks.
        if starts.is_some() && ci == cs.len() {
            for &c in lui.guides.iter().filter(|&&c| usize::from(c) >= col) {
                let tone = if lui.scope == Some(c) { Tone::Scope } else { Tone::Guide };
                let at = gutter + usize::from(c) - col + used;
                md[i].glyph.push(Glyph { idx: at, ch: '│', col: gutter as u16 + c, tone });
            }
        }
    }
    Colored { looks: out, win, extra: md }
}

/// PTR-1: the char under the point `(x, row)` of an editor frame, as (line, index in the line), by
/// the same walk as `colors`: the line of a row is read from its gutter, a wrapped line goes on in
/// the rows below, and each char takes what `shown` says. A point in the gutter is the start of
/// its row's text; one past the end of the text is the row's last char, or, on the last row of a
/// line, the end of the line (a Normal-mode cursor is then put on its last char by the App).
pub fn hit(
    f: &Frame,
    digits: usize,
    text: &dyn Fn(usize) -> Option<String>,
    (x, row): (usize, usize),
) -> Option<(usize, usize)> {
    let (gutter, mut line, mut cs, mut ci) = (digits + 2, None, Vec::<char>::new(), 0);
    for (i, r) in f.rows.iter().enumerate().take(row + 1) {
        if let Some(l) = number(r, digits).and_then(|n| n.checked_sub(1)) {
            (line, cs, ci) = (Some(l), text(l).unwrap_or_default().chars().collect(), 0);
        }
        let (mut col, mut used, first) = (0, 0, ci);
        let len = r.text.chars().count().saturating_sub(gutter);
        while used < len && ci < cs.len() {
            let (n, w) = shown(&cs, ci, col);
            if i == row && gutter + col + w > x {
                return line.map(|l| (l, ci));
            }
            (used, col, ci) = (used + n, col + w, ci + 1);
        }
        if i == row {
            let end = if ci < cs.len() { ci.saturating_sub(1).max(first) } else { ci };
            return line.map(|l| (l, end));
        }
    }
    None
}
