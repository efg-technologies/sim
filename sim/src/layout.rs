//! TREE-1, TREE-2, BUF-2, FIND-1, MARK-1, MARK-2, SYN-1, SYN-5, PTR-1, LOOK-1, LOOK-2, LOOK-3, LOOK-4, LOOK-5, LOOK-6, LOOK-7, UI-8, UI-2, UI-3, UI-5, COLOR-1, UI-7, UI-11, UI-13, UI-14, COLOR-2, COLOR-3, UI-15, UI-16, DIFF-2, DIFF-4, MD-1, MD-3, MD-7, GIT-5, MD-10, UI-19
//! The panel and the editor composed into one screen (pure): the panel on the left with its
//! marks, a column of space, the editor with its colors, and one status line across the whole
//! width.
use crate::changes::Stat;
use crate::color::{Chg, Tones};
use crate::diff::Sign;
use crate::icons;
use crate::marks::Git;
use crate::md::{Plan, Read, Remap, deceptive_targets};
use crate::paint::{Cue, Look, Looks, Paint, Paints, Role};
use crate::syn::Spans;
use crate::tabs::Tab;
use crate::tree;
use sim_core::core::app::App;
use sim_core::core::color::Rgb;
use sim_core::core::view::{Frame, Row, Style, shorten, width};
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

/// `s` cut to `max` columns, hostile chars replaced by U+FFFD, and the columns it takes: by the core's
/// own `width`, so an emoji presentation sequence takes what the editor gives it (VIS-7).
pub fn fit(s: &str, max: usize) -> (String, usize) {
    // (printable ASCII, which nearly all of it is: one column a char and nothing to replace)
    if s.bytes().all(|b| (b' '..0x7f).contains(&b)) {
        let n = s.len().min(max);
        return (s[..n].to_string(), n);
    }
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

/// A path made to fit `max` columns, the way the core's status line does it (`shorten`): `…/` and as
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
    /// DIFF-2: in the changes view, the numbers of a file or the sums of a folder.
    pub stat: Option<Stat>,
    pub changes: bool,
}

/// How a row of the tree is written: indent 2 per depth, `▸ name/` or `▾ name/` (then ` ~` if
/// something below changed), `• name` for a likely file, `  name` for any other.
pub fn label(r: &tree::Row, m: &Mark) -> (String, String, String) {
    let indent = "  ".repeat(r.depth);
    // DIFF-2: the changes view has the letter and the numbers after the name, two blanks away
    let letter = m.git.filter(|_| m.changes).map(|g| g.letter().to_string());
    let words: Vec<String> = [letter, m.stat.map(|s| s.show())].into_iter().flatten().collect();
    let words = words.join(" ");
    let words = if words.trim().is_empty() { String::new() } else { format!("  {}", words.trim()) };
    if r.is_dir {
        let (mark, below) = (if r.open { "▾ " } else { "▸ " }, if m.below { " ~" } else { "" });
        (format!("{indent}{mark}"), format!("{}/", r.name), format!("{below}{words}"))
    } else {
        (format!("{indent}{}", if m.likely { "• " } else { "  " }), r.name.clone(), words)
    }
}

/// `s` in `max` columns, a cut ending in `…`: no name is cut without saying so, or it would look like a
/// whole one (a file called `readme.`).
fn dots(s: &str, max: usize) -> String {
    let (all, cut) = (s.chars().count(), fit(s, max).0);
    if max == 0 || cut.chars().count() == all {
        return cut;
    }
    format!("{}…", fit(s, max - 1).0)
}

/// A row's name in `room` columns (`name` is what the row shows: `r.name`, and a `/` after a folder's).
/// A cut has a `…`. A renamed file's row reads `old → new` (DIFF-4): its new name is where the file is
/// now and stays whole, and the old one is what is cut, from the left, so that its own name stays.
fn cut(r: &tree::Row, name: &str, room: usize) -> String {
    // (a row that shows more than its file's name is a rename's: the others cost no allocation)
    let new = r.path.file_name().and_then(|n| n.to_str()).filter(|n| *n != r.name);
    let split = new.and_then(|n| Some((name.strip_suffix(&format!(" → {n}"))?, n)));
    match split {
        Some((old, new)) => {
            let rest = format!(" → {new}");
            let room_old = room.saturating_sub(fit(&rest, usize::MAX).1);
            if room_old == 0 { dots(new, room) } else { format!("{}{rest}", tail(old, room_old)) }
        }
        None => dots(name, room),
    }
}

/// LOOK-1: how the screen looks. Calm is the core's four styles; flat adds green and red and a tint to
/// the rows of changes; depth adds boxes, an overview ruler and gradients.
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

/// How the panel's text is faded (LOOK-7): not at all, or, while a search has the focus, the rows
/// that are no match fade by their distance from the cursor row `Find(row)`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fade {
    None,
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
/// color, the thumb a surface behind them (where there is no color a rail `║`: a shape at the weight of
/// the text, and not the inverse block that the brightest thing on the screen would be).
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
        (None, None) if c.thumb && t.mono => "║", // (COLOR-3: a rail, not an inverse block)
        (None, None) if c.thumb => " ",
        _ => return plain,
    };
    let mut l = Line::default();
    let fg = ink.or(c.cursor.map(|_| t.focus));
    let cue = if glyph == " " { Cue::Solid } else { Cue::None }; // (the thumb alone: a block)
    let role = if c.thumb { Role::Thumb } else { Role::None };
    l.put(glyph, Paint { fg, bg: c.thumb.then_some(t.thumb), cue, role, ..Paint::default() });
    l
}

/// The line number a row of an editor frame shows, if it starts a line.
pub(crate) fn number(r: &Row, digits: usize) -> Option<usize> {
    r.text.get(1..1 + digits)?.trim().parse().ok()
}

/// What a row of an editor frame is (LOOK-2): no change, text that a change from disk gained or
/// changed (`Gain`), or a deletion point (`Loss`, the `-` of the gutter).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Plain,
    Gain,
    Loss,
}

/// LOOK-2: the kind of each row of an editor frame, read from its gutter: a symbol in the first
/// column is a change, and so are the rows after it whose gutter is blank (the wrapped rows of the
/// line).
pub fn changed(f: &Frame, digits: usize) -> Vec<Kind> {
    let mut last = Kind::Plain;
    let body = f.rows.len().saturating_sub(1);
    f.rows[..body]
        .iter()
        .map(|r| {
            match (r.text.chars().next(), number(r, digits)) {
                (Some(' '), None) => {} // the rest of a row that has a change
                (Some(' ') | None, _) => last = Kind::Plain, // a line of no change, or past the end
                (Some('-'), _) => last = Kind::Loss,
                _ => last = Kind::Gain,
            }
            last
        })
        .collect()
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
                gaps.push((col..col + 1, Paint { bg, role: Role::Patch, ..Paint::default() }));
            }
        }
        col += w;
    }
    words.extend(gaps);
}

/// The editor as `body` takes it: the core's frame (its last row is its status line, which the screen
/// shows apart), the looks of its rows, the width of its line numbers, what each row is
/// (`changed`, empty when the look draws no tint) and the row of the frame the cursor is on (LOOK-6).
pub struct Ed<'a> {
    pub frame: &'a Frame,
    pub looks: &'a [Looks],
    pub extra: &'a [Extra],
    pub digits: usize,
    pub ch: Vec<Kind>,
    pub crow: usize,
    /// UI-5: the line the cursor is on (0-based) when the editor draws relative numbers and the
    /// cursor's line, and whether the mode is Insert (the whole line has a surface then).
    pub cur_line: Option<usize>,
    pub insert: bool,
    /// UI-5: the git sign of each row (the row a line starts in carries its sign).
    pub signs: Vec<Option<Sign>>,
}

impl<'a> Ed<'a> {
    /// A frame with no changes, the cursor where the frame has it (a preview).
    pub fn new(frame: &'a Frame, looks: &'a [Looks]) -> Ed<'a> {
        Ed {
            frame,
            looks,
            extra: &[],
            digits: 3,
            ch: vec![],
            crow: usize::from(frame.cursor.1),
            cur_line: None,
            insert: false,
            signs: vec![],
        }
    }
}

/// GIT-5: what the guard did to the repository, for the frame to say: nothing, `n` settings of its own
/// turned off (git runs), or git not run at all.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Guard {
    #[default]
    None,
    Off(usize),
    Refused,
}

/// GIT-5: the words the frame says of the guard, in the first form that fits `room` columns.
fn guard_words(g: Guard, room: usize) -> Option<String> {
    let forms = match g {
        Guard::None => return None,
        Guard::Refused => vec![" git not run ".to_string(), " no git ".into()],
        Guard::Off(n) => {
            let s = if n == 1 { "" } else { "s" };
            vec![
                format!(" {n} git setting{s} off "),
                format!(" git: {n} off "),
                format!(" {n} off "),
            ]
        }
    };
    forms.into_iter().find(|w| w.len() <= room)
}

/// The color of a box's lines (LOOK-3): the focused one's, the other's, or (GIT-5) the alert's for both
/// where git was not run.
fn box_ink(t: &Tones, guard: Guard, focused: bool) -> Rgb {
    match (guard == Guard::Refused, focused) {
        (true, _) => t.alert_fg(),
        (_, true) => t.focus,
        _ => t.line,
    }
}

/// What `body` needs to know about the look: the geometry, the tones (none when no tint, fog or
/// color of a box can be drawn: calm, or no palette or truecolor), the fade of the panel, whether
/// the Visual selection is a gray background (SYN-5), and for the borders of depth the App that has the file shown, the lines of it
/// on screen and whether the panel has the focus.
pub struct Deck<'a> {
    pub geo: Geo,
    pub tones: Option<&'a Tones>,
    pub fade: Fade,
    pub gray: bool,
    pub app: Option<&'a App>,
    pub win: Option<(usize, usize)>,
    pub focus: bool,
    pub chrome: Option<Chrome<'a>>,
    /// UI-11: the scrollbar's rows, none where the interface does not draw it.
    pub scroll: Vec<ScrollCell>,
    /// UI-18: the parts of the breadcrumb (the path's components and the section) and the index of the
    /// file's, none where the interface does not draw it.
    pub crumb: (Vec<String>, usize),
    /// GIT-5: what the guard did to the repository.
    pub guard: Guard,
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

/// What comes (the change style: text from disk) is on green, what goes on red:
/// the paints of the two, with their text grays.
fn ink_of(t: &Tones, red: bool) -> Paint {
    let (bg, fg) = if red { (t.del, t.del_text) } else { (t.add, t.add_text) };
    Paint { bg: Some(bg), fg: Some(fg), cue: Cue::Solid, role: Role::Patch, ..Paint::default() }
}

/// A row of the panel, `pw` columns: the name with its marks (on green if the file changed), the
/// selected row underlined; or, in a content search, a row of its results (GREP-3).
fn panel_line(pw: u16, at: Option<&(tree::Row, Mark)>, sel: bool, t: Option<&Tones>) -> Line {
    let mut line = Line::default();
    if let Some((r, m)) = at {
        // (a name too long for the panel is cut, with a `…`, and what follows it is not)
        let (pre, name, post) = label(r, m);
        let wide = |s: &str| fit(s, usize::MAX).1;
        let room = usize::from(pw).saturating_sub(wide(&pre) + wide(&post));
        let name = cut(r, &name, room);
        let (start, end) = (wide(&pre), wide(&pre) + wide(&name));
        line.put(&format!("{pre}{name}{post}"), Paint::default());
        let used = end + wide(&post);
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
    // DIFF-2: the numbers of the changes view go before the letter, a blank between
    let stat = m.stat.map(|s| s.show()).filter(|s| !s.is_empty());
    let sw = stat.as_deref().map_or(0, |s| fit(s, usize::MAX).1);
    let room = usize::from(pw.saturating_sub(2)).saturating_sub(sw + usize::from(sw > 0));
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
    let text = cut(r, &text, room.saturating_sub(usize::from(line.cols)));
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
    line.pad(pw.saturating_sub(2 + sw as u16));
    for (i, word) in stat.iter().flat_map(|s| s.split(' ')).enumerate() {
        let (add, del) = (t.change_fg(Chg::Add), t.change_fg(Chg::Del));
        let paint = match word.chars().next() {
            Some('+') => ink(add),
            Some('−') => ink(del),
            _ => Paint::default(),
        };
        line.put(if i > 0 { " " } else { "" }, Paint::default());
        line.put(word, paint);
    }
    line.put(" ", Paint::default());
    match (m.git, m.dot) {
        (Some(g), _) => {
            let (c, bold) = (t.change_fg(git_chg(g)), g == Git::Conflict);
            line.put(&g.letter().to_string(), Paint { bold, ..ink(c) });
        }
        (_, Some(g)) => line.put(&g.letter().to_string(), ink(t.change_fg(git_chg(g)))),
        _ => {}
    }
    line.pad(pw);
    line
}

/// UI-2: the first row of the panel: the root, home-relative, in bold, with the open folder.
fn header_line(pw: u16, root: &str, icons: bool, guard: Guard, t: &Tones) -> Line {
    let mut line = Line::default();
    let bar = Paint { bg: Some(t.bar), role: Role::Bar, ..Paint::default() };
    line.put(" ", bar);
    if icons {
        line.put(&format!("{} ", icons::OPEN), Paint { fg: Some(t.focus), ..bar });
    }
    // GIT-5: the words for the guard at the right end, if the root keeps room for six columns
    let left = usize::from(pw).saturating_sub(usize::from(line.cols) + 1);
    let words = guard_words(guard, left.saturating_sub(6));
    let room = left - words.as_ref().map_or(0, |w| w.len() + 1);
    line.put(&tail(root, room), Paint { bold: true, ..bar });
    if let Some(w) = words {
        line.pad(pw - w.len() as u16 - 1);
        line.put(&w, alert_block(t));
    }
    line.pad(pw);
    line.paints.push((0..pw, bar));
    line
}

/// UI-4: the first row of the editor: the tab bar (`tabs.rs`).
fn tabs_line(ew: u16, c: &Chrome, t: &Tones, crumb: Option<&(Vec<String>, usize)>) -> Line {
    let mut line = crate::tabs::layout(&c.tabs, c.icons, c.cur, ew, Some(t)).0;
    // UI-18: flat has no box to hold the breadcrumb: it is at the right of the bar, where the tabs
    // leave room (a blank at each end and one between).
    let used = fit(line.row.text.trim_end(), usize::MAX).1;
    let room = usize::from(ew).saturating_sub(used + 2);
    let segs = crumb.map(|c| crumb_segments(c, room, Some(t), true)).unwrap_or_default();
    let width: usize = segs.iter().map(|s| fit(&s.0, usize::MAX).1).sum();
    if width > 0 && width <= room {
        let from = usize::from(ew) - width;
        let (mut text, cols) = fit(&line.row.text, from);
        text += &" ".repeat(from - cols);
        let mut at = from as u16;
        for (s, p) in segs {
            // (what is drawn is `fit`'s: no segment can bring a char that must not reach the terminal)
            let (s, w) = fit(&s, usize::MAX);
            line.paints.insert(0, (at..at + w as u16, p));
            (text, at) = (text + &s, at + w as u16);
        }
        line.row.text = text;
        line.cols = ew;
    }
    line
}

/// The tint and the fade of a row of the panel (LOOK-2, LOOK-7): a changed file's row is tinted
/// across; the rows that are no match of a search fade by their distance from the cursor row.
fn panel_paint(i: usize, m: &Mark, fade: Fade, t: &Tones) -> Paint {
    let fade = match fade {
        Fade::Find(c) if m.hit == Some(false) => find_fog(i.abs_diff(c)),
        _ => 0,
    };
    let bg = if m.change { Some(t.gain) } else { m.open.then_some(t.card) };
    let role = if m.change {
        Role::Tint
    } else if m.open {
        Role::Chip
    } else {
        Role::None
    };
    Paint { bg, fade, bold: t.mono && m.open, role, ..Paint::default() }
}

/// UI-8: the markdown of a row of the editor, in flat and depth: the surfaces (a fenced block
/// across the row, inline code), blue for a link and a level-5 heading, a quote's `>` drawn as a
/// bar in its own cell, and the language a fence names, dim, at the right of its row.
fn extra_row(line: &mut Line, m: &Extra, t: &Tones, gutter: u16, ew: u16) {
    let on = |bg, role| Paint { bg: Some(bg), role, ..Paint::default() };
    let link = Paint { fg: Some(t.accent_fg()), ..Paint::default() };
    let head = Paint { fg: Some(t.head), ..Paint::default() };
    let mut paints: Paints = line
        .looks
        .iter()
        .filter_map(|(r, l)| match l {
            Look::Code | Look::Tick => Some((r.clone(), on(t.code, Role::Patch))),
            Look::Link => Some((r.clone(), link)),
            // UI-8: a heading is in the heading hue (after the link's: a link in it is the accent's)
            Look::H1 | Look::H2 | Look::H3 | Look::Sub => Some((r.clone(), head)),
            _ => None,
        })
        .collect();
    // UI-7: one style for every keyword: bold primary on the card's surface.
    let badge = Paint {
        bg: Some(t.card),
        fg: Some(t.primary),
        bold: true,
        role: Role::Chip,
        ..Paint::default()
    };
    paints.extend(m.badge.iter().map(|r| (r.clone(), badge)));
    // UI-13, UI-14: the yank, lit in the accent's block, and the matches of a search, the current
    // one on the accent's solid color, over everything else of the cell.
    let (back, text) = t.accent_block();
    let (solid, ink) = t.accent_solid();
    let patch = Paint { role: Role::Patch, ..Paint::default() };
    let flash = Paint { bg: Some(back), fg: Some(text), cue: Cue::Solid, ..patch };
    let now = Paint { bg: Some(solid), fg: Some(ink), cue: Cue::Solid, ..patch };
    let hit = Paint { bg: Some(back), cue: Cue::Under, ..patch };
    // UI-19: the lines another process wrote are the change's solid green, bold, under the yank's accent
    // (brighter than the block of the words that came, which an added line has whole: else it would not show)
    let (green, green_on) = t.add_solid();
    let came = Paint { bg: Some(green), fg: Some(green_on), bold: true, cue: Cue::Solid, ..patch };
    // MD-10 first (the alert), then these, all before every paint the row has: a changed row has its
    // words and its tint already, and the first paint of a cell wins
    let mut front: Paints = m.lie.iter().map(|r| (r.clone(), alert_block(t))).collect();
    front.extend(m.flash.iter().map(|r| (r.clone(), flash)));
    front.extend(m.wrote.iter().map(|r| (r.clone(), came)));
    front.extend(m.now.iter().map(|r| (r.clone(), now)));
    front.extend(m.hit.iter().map(|r| (r.clone(), hit)));
    line.paints.splice(0..0, front);
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
                Tone::Mark => Some(t.focus),
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
    let band = match m.band {
        Band::None => None,
        Band::Fence => Some((t.band, Role::Fence)),
        Band::H1 => Some((t.head1, Role::Heading)),
        Band::H2 => Some((t.head2, Role::Heading)),
    };
    if let Some((band, role)) = band {
        line.pad(ew);
        paints.push((gutter..ew, on(band, role)));
    }
    line.paints.extend(paints);
}

/// MD-1: `line`, a row the core made, as the read view draws it: its text and every range of
/// columns of it moved by `map` (what stood on a char that is not drawn is gone with it), and the
/// delimiter row's rule dim; the rule is `─` where the interface is drawn and `-` in calm.
fn drawn(line: &mut Line, m: &Extra, map: &Remap, tones: Option<&Tones>) -> Extra {
    line.row.text = map.text(&line.row.text, if tones.is_some() { '─' } else { '-' });
    let go = |r: Range<u16>| map.range(r);
    let row = &mut line.row;
    row.spans = take(&mut row.spans).into_iter().filter_map(|(r, s)| Some((go(r)?, s))).collect();
    row.underline = take(&mut row.underline).into_iter().filter_map(go).collect();
    line.looks = take(&mut line.looks).into_iter().filter_map(|(r, l)| Some((go(r)?, l))).collect();
    line.paints =
        take(&mut line.paints).into_iter().filter_map(|(r, p)| Some((go(r)?, p))).collect();
    line.looks.extend(map.rules().into_iter().map(|r| (r, Look::Mark)));
    line.cols = fit(&line.row.text, usize::MAX).1 as u16;
    m.mapped(map)
}

/// The rows of the editor, `ew` columns each where something is drawn behind the blanks and as
/// they are else: the core's rows with the looks; in flat and depth (`t`) the words of the change style
/// on green or red, the tint of the rows that have a change, the joined words and the bar of the
/// gutter; in depth the fog of the rows that are no change (LOOK-2, LOOK-6).
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
            Some(Sign::Change) => ("┆", Some(Chg::Mod)),
            Some(Sign::Del) => ("▁", Some(Chg::Del)),
            Some(Sign::DelTop) => ("▔", Some(Chg::Del)),
            None => (" ", None),
        };
        s.put(
            glyph,
            Paint {
                bg,
                fg: accent.zip(t).map(|(a, t)| t.change_fg(a)),
                role: if bg.is_some() { Role::Tint } else { Role::None },
                ..Paint::default()
            },
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
            l.paints.push((
                gutter + 99..gutter + 100,
                Paint { bg: Some(t.cc), role: Role::Column, ..Paint::default() },
            ));
        }
    };
    let mut out = Vec::with_capacity(rows);
    for (i, e) in ed.frame.rows[..ed.frame.rows.len().saturating_sub(1)].iter().enumerate() {
        let cols = if e.text.is_ascii() { e.text.len() } else { fit(&e.text, usize::MAX).1 };
        let mut line = Line { row: e.clone(), cols: cols as u16, ..Line::default() };
        if let Some(t) = t {
            alerted(&mut line.row, &mut line.paints, t);
        }
        // The Visual selection is a gray background instead of an underline (SYN-5), ahead of the syntax looks.
        if gray {
            line.looks =
                take(&mut line.row.underline).into_iter().map(|r| (r, Look::Sel)).collect();
        }
        // UI-8: with tones the heading's hue says it is one and its weight its level: levels 4 to 6 are
        // `Sub`, which has the hue and not the weight the Painter gives every heading look.
        let sub = |l: Look| {
            if t.is_some() && matches!(l, Look::H4 | Look::H5 | Look::H6) { Look::Sub } else { l }
        };
        line.looks.extend(ed.looks[i].iter().map(|(r, l)| (r.clone(), sub(*l))));
        let kind = ed.ch.get(i).copied().unwrap_or(Kind::Plain);
        // MD-1: a row of the read view is drawn from the core's row, moved; rows of changes are not
        let map = ed.extra.get(i).and_then(|m| Some((m, m.map.as_ref()?)));
        let moved = map.filter(|_| kind == Kind::Plain).map(|(m, map)| drawn(&mut line, m, map, t));
        let extra = moved.as_ref().or(ed.extra.get(i));
        if let (Some(t), true) = (t, kind != Kind::Plain) {
            // Words that come are on green; the gutter's symbol by its own char.
            let sym = e.text.chars().next();
            let mut words = Paints::new();
            for (r, style) in &e.spans {
                if *style != Style::Change {
                    continue;
                }
                words.push((r.clone(), ink_of(t, r.start == 0 && sym == Some('-'))));
            }
            join(&e.text, &mut words, gutter);
            if e.text.starts_with(' ') {
                let bg = Some(if kind == Kind::Loss { t.del } else { t.add }); // the bar goes on over the wrapped rows
                words.push((0..1, Paint { bg, role: Role::Tint, ..Paint::default() }));
            }
            line.paints.extend(words);
            line.pad(ew);
            let bg = if kind == Kind::Loss { t.loss } else { t.gain };
            line.paints
                .push((gutter..ew, Paint { bg: Some(bg), role: Role::Tint, ..Paint::default() }));
        } else if let (Some(_), true) = (t, g.boxed) {
            let fade = ramp(i.abs_diff(ed.crow), 3.0, 24.0, 0.45);
            if fade > 0 {
                // (the numbers of the gutter have a floor of their own: LOOK-6)
                line.paints.push((0..gutter, Paint { fade, soft: true, ..Paint::default() }));
                line.paints.push((0..ew, Paint { fade, ..Paint::default() }));
            }
        }
        if let (Some(t), Some(m)) = (t, extra) {
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
                line.paints.push((
                    0..ew,
                    Paint { bg: Some(t.cursor), role: Role::Tint, ..Paint::default() },
                ));
                sign_bg = Some(t.cursor);
            }
        }
        band(&mut line);
        line.pad(to);
        out.push(signed(line, sign_bg, ed.signs.get(i).copied().flatten()));
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
pub(crate) fn border(
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

/// UI-18: the breadcrumb as the runs of text of a title, in `room` columns: the directories and the
/// separators in the secondary gray, the file (`file`, the index of its part) in bold and the section
/// as text, each with a blank at the ends; all dim while the other box has the focus (as `files` is).
fn crumb_segments(
    (parts, file): &(Vec<String>, usize),
    room: usize,
    t: Option<&Tones>,
    hot: bool,
) -> Vec<(String, Paint)> {
    let shown = crate::crumb::cut(parts, room);
    let ink = |fg: Option<Rgb>, bold| Paint { fg, bold, ..Paint::default() };
    let dim = ink(t.map(|t| t.dim), false);
    let own = |bold| ink(t.filter(|_| !hot).map(|t| t.dim), bold);
    let ell = shown.len() > 1 && shown[0] == "…";
    let first = parts.len() - (shown.len() - usize::from(ell));
    let mut out = vec![];
    for (i, part) in shown.iter().enumerate() {
        out.push((
            if i == 0 { format!(" {part}") } else { part.clone() },
            match (i < usize::from(ell), (first + i).wrapping_sub(usize::from(ell)).cmp(file)) {
                (true, _) | (_, std::cmp::Ordering::Less) => dim,
                (_, std::cmp::Ordering::Equal) => own(true),
                _ => own(false),
            },
        ));
        if i + 1 < shown.len() {
            out.push((crate::crumb::SEP.to_string(), dim));
        }
    }
    if let Some(l) = out.last_mut() {
        l.0.push(' ');
    }
    out
}

/// The top and the bottom border of the panel's box and the editor's (LOOK-3): numbers, titles,
/// badges (changes on disk on green, a conflict with the disk on the neutral surface).
fn borders(d: &Deck) -> [Line; 2] {
    let (g, t) = (&d.geo, d.tones);
    let paint = |fg: Option<Rgb>, bold| Paint { fg, bold, ..Paint::default() };
    let own = |panel: bool| panel == d.focus; // whether that box has the focus
    let line = |panel| paint(t.map(|t| box_ink(t, d.guard, own(panel))), false);
    let title = |panel| paint(t.filter(|_| !own(panel)).map(|t| t.dim), true);
    let words = |room: usize| t.and_then(|t| Some((guard_words(d.guard, room)?, alert_block(t))));
    let s = |text: &str, p| (text.to_string(), p);
    let (pw, ew) = (g.pw + 2, g.ew + g.sign + 2);
    let (mut top, mut bottom) = (Line::default(), Line::default());
    let mut files = vec![s("files", title(true))];
    if let Some((w, p)) = words(usize::from(pw).saturating_sub(4 + 5 + 1)).filter(|_| g.pw > 0) {
        files.extend([s("─", line(true)), (w, p)]); // (GIT-5, where the tree is)
    }
    if g.pw > 0 {
        top.add(border(pw, ['╭', '╮'], line(true), &files, &[]));
        bottom.add(border(pw, ['╰', '╯'], line(true), &[], &[]));
    }
    let mut left = vec![];
    if let Some(a) = d.app {
        let b = &a.buf;
        // A change on disk is a run of marked lines next to each other (the core's status line counts so).
        let runs =
            (0..b.marks.len()).filter(|&i| i == 0 || b.marks[i - 1].line + 1 < b.marks[i].line);
        let badge =
            |n: usize, one: &str, many: &str| format!(" {n} {} ", if n == 1 { one } else { many });
        let surface = t.map(|t| Paint {
            bg: Some(t.card),
            fg: Some(t.primary),
            cue: Cue::Solid,
            role: Role::Chip,
            ..Paint::default()
        });
        let notes = [
            (runs.count(), ["change on disk", "changes on disk"], t.map(|t| ink_of(t, false))),
            (
                usize::from(a.diverged()),
                ["conflict with the disk", "conflicts with the disk"],
                surface,
            ),
        ];
        let mut notes: Vec<(String, Paint)> = notes
            .into_iter()
            .filter(|n| n.0 > 0)
            .map(|(n, w, p)| (badge(n, w[0], w[1]), p.unwrap_or(paint(None, true))))
            .collect();
        // GIT-5: with no tree to say it on, the editor's box does
        notes.extend(words(usize::from(ew).saturating_sub(20)).filter(|_| g.pw == 0));
        // UI-16, UI-18: the title is the breadcrumb, and then the badges, a rule between them.
        let badges: usize = notes.iter().map(|n| fit(&n.0, usize::MAX).1 + 1).sum();
        left = notes.into_iter().flat_map(|n| [s("─", line(false)), n]).skip(1).collect();
        let room = usize::from(ew).saturating_sub(4 + 2 + badges);
        let hot = own(false);
        let mut crumb = crumb_segments(&d.crumb, room, t, hot);
        if !left.is_empty() && !crumb.is_empty() {
            crumb.push(s("─", line(false)));
        }
        left.splice(0..0, crumb);
    }
    top.add(border(ew, ['╭', '╮'], line(false), &left, &[]));
    bottom.add(border(ew, ['╰', '╯'], line(false), &[], &[]));
    [top, bottom]
}

/// LOOK-4: the rows of the ruler of the file `d.app` has shown.
fn ticks(d: &Deck) -> Vec<Tick> {
    let rows = usize::from(d.geo.rows);
    let Some(b) = d.app.map(|a| &a.buf) else { return vec![Tick::Plain; rows] };
    let loss = |gone: bool| if gone { Tick::Loss } else { Tick::Gain };
    let marks = b.marks.iter().map(|m| (m.line..m.line + 1, loss(m.kind == '-')));
    ruler(rows, b.lines(), marks, d.win)
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
    let edge = |panel: bool| cell("│", t.map(|t| box_ink(t, d.guard, panel == d.focus)));
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
        let p = (g.pw > 0).then(|| {
            header_line(g.pw, c.root, c.icons, if g.boxed { Guard::None } else { d.guard }, t)
        });
        let end = if g.boxed {
            edge(false)
        } else {
            let mut l = Line::default();
            l.put(" ", Paint { bg: Some(t.bar), role: Role::Bar, ..Paint::default() });
            l
        };
        lines.push(compose(p, tabs_line(g.ew + g.sign, c, t, (!g.boxed).then_some(&d.crumb)), end));
    }
    for (i, e) in editor(g, ed, d.gray, t).into_iter().enumerate() {
        let p = (g.pw > 0).then(|| {
            let mut p = match (&d.chrome, t) {
                (Some(c), Some(t)) => chrome_line(g.pw, panel.get(i), sel == Some(i), t, c.icons),
                _ => panel_line(g.pw, panel.get(i), sel == Some(i), t),
            };
            let paint = t.zip(panel.get(i)).map(|(t, (_, m))| panel_paint(i, m, d.fade, t));
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
/// are the core's own `width`. A tab and a char shown as `^X` or `<U+XXXX>` take as many chars as
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
    /// UI-8: the secondary gray (a thematic break's rule).
    Mark,
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

/// UI-15: the alert's block: its surface and its own text, sealed (a char shown in another form, and, for
/// the same reason of being what the reader must not miss, the frame's word for the guard, GIT-5, and
/// the target of a link that lies, MD-10).
pub fn alert_block(t: &Tones) -> Paint {
    let (back, text) = t.alert_block();
    Paint {
        bg: Some(back),
        fg: Some(text),
        sealed: true,
        cue: Cue::Solid,
        role: Role::Alert,
        ..Paint::default()
    }
}

/// UI-15: takes the spans of `Style::Hidden` (a char shown in another form, SAFE-8) out of a row and
/// paints their columns as the alert's block (its surface and its own text), sealed: no selection,
/// match, surface, change, band or fog may change it, so the signal holds in every state of a cell.
/// Where it is not called (calm) the Painter draws the style inverse, also sealed.
pub fn alerted(row: &mut Row, paints: &mut Paints, t: &Tones) {
    let block = alert_block(t);
    let hidden: Vec<_> =
        row.spans.iter().filter(|s| s.1 == Style::Hidden).map(|s| (s.0.clone(), block)).collect();
    row.spans.retain(|s| s.1 != Style::Hidden);
    paints.splice(0..0, hidden);
}

/// UI-8: the surface across a row of markdown: a fenced block's (neutral), a first-level heading's
/// (the heading hue, stronger) or a second-level one's (fainter).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Band {
    #[default]
    None,
    Fence,
    H1,
    H2,
}

/// UI-8, UI-6: what a row of an editor frame has besides the looks of its chars: a line of a fenced
/// block (a surface across the row), the chars drawn as others (a quote's `>` as `▌`, the indent
/// guides), and the language a fence names, for the right of its row.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Extra {
    pub band: Band,
    pub glyph: Vec<Glyph>,
    pub lang: Option<String>,
    /// UI-7: the columns of a TODO badge in this row.
    pub badge: Vec<std::ops::Range<u16>>,
    /// UI-13: the columns of the text a yank has just taken, in this row.
    pub flash: Vec<std::ops::Range<u16>>,
    /// UI-19: the columns of the text of a line another process has just written, in this row.
    pub wrote: Vec<std::ops::Range<u16>>,
    /// UI-14: the columns of the matches of a search in this row, and of the current one.
    pub hit: Vec<std::ops::Range<u16>>,
    pub now: Vec<std::ops::Range<u16>>,
    /// MD-10: the columns of the target of a link that lies, in this row.
    pub lie: Vec<std::ops::Range<u16>>,
    /// MD-1: the row of a line of the read view, as it is drawn from the row the core made.
    pub map: Option<Remap>,
}

impl Extra {
    /// MD-1: the extras of the row as the read view draws it: every column and char moved by `m`,
    /// and what stood on a char that is not drawn gone with it.
    fn mapped(&self, m: &Remap) -> Extra {
        let ranges = |v: &[Range<u16>]| -> Vec<Range<u16>> {
            v.iter().filter_map(|r| m.range(r.clone())).collect()
        };
        let glyph = self
            .glyph
            .iter()
            .filter_map(|g| Some(Glyph { idx: m.idx(g.idx)?, col: m.col(g.col)?, ..*g }));
        Extra {
            glyph: glyph.collect(),
            badge: ranges(&self.badge),
            flash: ranges(&self.flash),
            wrote: ranges(&self.wrote),
            hit: ranges(&self.hit),
            now: ranges(&self.now),
            lie: ranges(&self.lie),
            ..self.clone()
        }
    }
}

/// UI-6: what the interface wants drawn on a line of the buffer: the columns of its indent guides
/// and which of them is the scope's around the cursor.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LineUi {
    pub guides: Vec<u16>,
    pub scope: Option<u16>,
    /// UI-13: the chars of the line a yank has just taken.
    pub flash: Option<std::ops::Range<usize>>,
    /// UI-19: whether another process has just written the line.
    pub wrote: bool,
    /// UI-14: the chars of the line that a search matches, and which of them is the current one.
    pub hits: Vec<std::ops::Range<usize>>,
    pub now: Option<usize>,
}

/// SYN-1: what `colors` finds in an editor frame: the looks of its rows, the first and the last
/// line on screen, and the extras of each row (markdown, guides).
#[derive(Debug, Default)]
pub struct Colored {
    pub looks: Vec<Looks>,
    pub win: Option<(usize, usize)>,
    pub extra: Vec<Extra>,
    /// MD-4: the lines whose spans the read view wants, if the window has not all of a table.
    pub need: Option<Range<usize>>,
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
    read: Option<Range<usize>>,
) -> Colored {
    let (gutter, mut win) = (digits + 2, None);
    // MD-1: the read view of the lines on screen, if it is on (`read`: the lines drawn as they are)
    let shown_lines = f.rows.iter().filter_map(|r| number(r, digits)).map(|n| n.saturating_sub(1));
    let (first, last) = shown_lines.fold((usize::MAX, 0), |(a, z), n| (a.min(n), z.max(n)));
    let pane = usize::from(f.width).saturating_sub(gutter);
    let read =
        read.filter(|_| first <= last).map(|r| Read::new(first..last + 1, r, pane, text, spans));
    let mut plan: Option<Plan> = None;
    let (mut out, mut md) =
        (vec![Looks::new(); f.rows.len()], vec![Extra::default(); f.rows.len()]);
    let (mut cs, mut sp, mut ci, mut band) = (Vec::<char>::new(), None, 0, Band::None);
    let (mut lui, mut trail, mut badge, mut hk) = (LineUi::default(), 0, 0..0, 0);
    let mut lie = Vec::<Range<usize>>::new();
    let (mut delim, mut framed, mut pipes, mut rule) = (false, false, (0, 0), false);
    for (i, row) in f.rows.iter().enumerate() {
        let (mut col, mut used, len) = (0, 0, row.text.chars().count().saturating_sub(gutter));
        let starts = number(row, digits).and_then(|n| n.checked_sub(1));
        if let Some(l) = starts {
            (cs, sp, ci) = (text(l).unwrap_or_default().chars().collect(), spans(l), 0);
            win = Some((win.map_or(l, |w: (usize, usize)| w.0), l));
            // a fenced block, and a first-level heading (a faint band of its own)
            let has = |k: Look| sp.is_some_and(|s| s.iter().any(|s| s.1 == k));
            band = match sp {
                Some(s) if s.first().is_some_and(|s| s.1 == Look::Band) => Band::Fence,
                _ if has(Look::H1) => Band::H1,
                _ if has(Look::H2) => Band::H2,
                _ => Band::None,
            };
            let lang = sp.and_then(|s| s.iter().find(|s| s.1 == Look::Lang));
            md[i].lang = lang.map(|s| cs[s.0.clone()].iter().collect());
            // UI-8: a table's row starts with a pipe (a `|` in prose is a pipe); its delimiter row has
            // only `|`, `-`, `:` and blanks, with a `|` and a `-`, and its first and last pipe are
            // drawn as `├` and `┤`.
            framed = cs.iter().find(|&&c| c != ' ') == Some(&'|');
            rule = sp.is_some_and(|s| s.iter().any(|s| s.1 == Look::Rule));
            delim = cs.contains(&'|')
                && cs.contains(&'-')
                && cs.iter().all(|c| matches!(c, '|' | '-' | ':' | ' '));
            pipes = (
                cs.iter().position(|&c| c == '|').unwrap_or(0),
                cs.iter().rposition(|&c| c == '|').unwrap_or(0),
            );
            plan = read.as_ref().zip(sp).and_then(|(r, s)| r.plan(l, &cs, s));
            (lui, hk) = (ui.map_or_else(LineUi::default, |f| f(l)), 0);
            // MD-10: the targets of the links of this line that say another place
            lie = sp.filter(|_| ui.is_some()).map_or(vec![], |s| deceptive_targets(&cs, s));
            trail = cs.iter().rposition(|c| !matches!(c, ' ' | '\t')).map_or(0, |i| i + 1);
            badge = sp
                .filter(|_| ui.is_some())
                .and_then(|s| todo_badge(&cs.iter().collect::<String>(), s))
                .unwrap_or(0..0);
        }
        // (a row with no text and no number is past the end of the file, not the rest of a line)
        md[i].band = if starts.is_some() || len > 0 { band } else { Band::None };
        let (mut map, mut edited) = (Remap::default(), false);
        if plan.is_some() {
            map.keep(0..gutter as u16, 0..gutter);
        }
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
            if lie.iter().any(|r| r.contains(&ci)) {
                match md[i].lie.last_mut() {
                    Some(b) if b.end == cols.start => b.end = cols.end,
                    _ => md[i].lie.push(cols.clone()),
                }
            }
            if lui.wrote {
                match md[i].wrote.last_mut() {
                    Some(b) if b.end == cols.start => b.end = cols.end,
                    _ => md[i].wrote.push(cols.clone()),
                }
            }
            // UI-8: a table's pipe is a bar, and its delimiter row a grid, over the char: the same
            // width, the marker's look (secondary).
            if look == Some(Look::Mark) && n == 1 && framed {
                let bar = match (cs[ci], delim, ci) {
                    ('|', false, _) => Some('│'),
                    ('|', true, k) if k == pipes.0 => Some('├'),
                    ('|', true, k) if k == pipes.1 => Some('┤'),
                    ('|', true, _) => Some('┼'),
                    ('-', true, _) => Some('─'),
                    _ => None,
                };
                let at = (gutter + used, (gutter + col) as u16);
                md[i].glyph.extend(bar.map(|ch| Glyph {
                    idx: at.0,
                    ch,
                    col: at.1,
                    tone: Tone::Plain,
                }));
            }
            // UI-8: a thematic break's chars are `─`, and the row goes on so to the pane's edge.
            if look == Some(Look::Rule) {
                let at = (gutter + used, (gutter + col) as u16);
                md[i].glyph.push(Glyph { idx: at.0, ch: '─', col: at.1, tone: Tone::Mark });
            }
            // UI-14: the cells of a match of a search, the current one apart, merged where they touch.
            while lui.hits.get(hk).is_some_and(|h| h.end <= ci) {
                hk += 1;
            }
            if lui.hits.get(hk).is_some_and(|h| h.contains(&ci)) {
                let cols = (gutter + col) as u16..(gutter + col + w) as u16;
                let to = if lui.now == Some(hk) { &mut md[i].now } else { &mut md[i].hit };
                match to.last_mut() {
                    Some(b) if b.end == cols.start => b.end = cols.end,
                    _ => to.push(cols),
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
            // UI-6: the whitespace marks, dim: a tab as `»` and blanks to its width, a trailing blank
            // as `·`, a no-break space as `⍽`; an indent guide, drawn after, wins a blank.
            if ui.is_some() {
                let at = (gutter + used, (gutter + col) as u16);
                let mark = |ch: char| Glyph { idx: at.0, ch, col: at.1, tone: Tone::Dim };
                match cs[ci] {
                    '\t' => md[i].glyph.push(mark('»')),
                    '\u{a0}' => md[i].glyph.push(mark('⍽')),
                    ' ' if ci >= trail => md[i].glyph.push(mark('·')),
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
                (Some(k), _) => out[i].push((cols.clone(), k)),
                _ => {}
            }
            // MD-1: what the read view does at this char: puts something in before it, leaves it out
            if let Some(op) = plan.as_ref().map(|p| p.op(ci)) {
                if let Some((c, k)) = op.fill {
                    map.fill(c, k);
                    edited = true;
                }
                if op.hide {
                    edited = true;
                } else {
                    map.keep(cols, gutter + used..gutter + used + n);
                }
            }
            (used, col, ci) = (used + n, col + w, ci + 1);
        }
        // MD-1: the end of the line may have something put in (the last cell of a table), and the
        // row has a map if the view changed it and the walk ended where the core's row does
        let ended = ci == cs.len() && (starts.is_some() || len > 0);
        if let Some((c, k)) = plan.as_ref().filter(|_| ended).and_then(|p| p.op(ci).fill) {
            map.fill(c, k);
            edited = true;
        }
        md[i].map = (plan.is_some() && edited && used == len).then_some(map);
        // UI-8: the rule of a thematic break goes on to the pane's edge.
        if rule && ci == cs.len() && (starts.is_some() || len > 0) {
            let from = gutter + col;
            md[i].glyph.extend((from..usize::from(f.width)).map(|c| Glyph {
                idx: gutter + used + (c - from),
                ch: '─',
                col: c as u16,
                tone: Tone::Mark,
            }));
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
    Colored { looks: out, win, extra: md, need: read.and_then(|r| r.need) }
}

/// PTR-1: the char under the point `(x, row)` of an editor frame, as (line, index in the line), by
/// the same walk as `colors`: the line of a row is read from its gutter, a wrapped line goes on in
/// the rows below, and each char takes what `shown` says (`map`: the read view's of the row hit,
/// MD-7). A point in the gutter is the start of
/// its row's text; one past the end of the text is the row's last char, or, on the last row of a
/// line, the end of the line (a Normal-mode cursor is then put on its last char by the App).
pub fn hit(
    f: &Frame,
    digits: usize,
    text: &dyn Fn(usize) -> Option<String>,
    (x, row): (usize, usize),
    map: Option<&Remap>,
) -> Option<(usize, usize)> {
    // MD-7: a point in a row of the read view is taken back to the column of the core's row
    let x = map.map_or(x, |m| usize::from(m.back(u16::try_from(x).unwrap_or(u16::MAX))));
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
