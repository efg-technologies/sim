//! TREE-1, TREE-2, TREE-4, TREE-5, BUF-1, BUF-2, BUF-3, FIND-3, MARK-1, MARK-2, SYN-1, SYN-2, SYN-3, SYN-5, LOOK-1, LOOK-8, UI-2, UI-3, UI-9, UI-5, UI-10, UI-4, UI-6, COLOR-1, UI-11, UI-13, UI-14, PLAIN-1, COLOR-2, ICON-1, SAFE-7, UI-17, GIT-3, DIFF-1, DIFF-3, DIFF-4
//! sim's state (pure): the open files (each one a core App), the panel, and what each event does.
//! `Sim::update` takes an event and returns what the shell is to do; it reads no clock, no file
//! and no terminal, so the tests drive it without threads. The panel's keys, search and focus
//! are in `panel.rs`, the search of file contents in `live.rs`.
mod guide;
mod live;
mod panel;
mod ptr;

use crate::bar::{self, Bar, Block as BarBlock};
use crate::changes::{Changes, Counts};
use crate::color::{Syntax, Tones};
use crate::crumb;
use crate::diff::{Diff, Sign};
use crate::grep;
use crate::icons;
use crate::indent::{self, Indent, Lead};
use crate::io::GIT_SKIPPED;
use crate::keys;
use crate::layout::{
    self, Chrome, Colored, Deck, Ed, Fade, Geo, Kind, Line, LineUi, Mark, ScrollCell, Skin, Tick,
};
use crate::marks::{Git, Marks};
use crate::paint::{Layer, LayerKind, Look, Paint, Role, Screen};
use crate::pick;
use crate::rank::History;
use crate::search::Matcher;
use crate::syn::Spans;
use crate::tabs::{self, Tab};
use crate::tree::{Entry, Row as TreeRow, Tree};
use crossterm::event::MouseEvent;
use live::{Live, Pick};
use panel::Search;
use ropey::Rope;
use sim_core::core::app::{App, Effect, Event, Key, Read};
use sim_core::core::buffer::Buffer;
use sim_core::core::cli;
use sim_core::core::color::{Palette, Rgb, palette};
use sim_core::core::vi::Mode;
use sim_core::core::view::{self, Frame, Row, Shape};
use sim_core::io::journal::Snapshot;
use std::collections::HashSet;
use std::mem::{replace, take};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;
use unicode_width::UnicodeWidthStr;

/// UI-13: how long a yank is lit, as the author's `on_yank({ timeout = 150 })`.
pub const FLASH_MS: u64 = 150;
/// PICK-5: how long a frame of the spinner lasts.
pub const SPIN_MS: u64 = 80;
/// KEY-3: how long a pause on a prefix lasts before which-key opens: which-key's own default for a
/// mapping that is not a plugin's (`delay` in its `config.lua`).
pub const PAUSE_MS: u64 = 200;
/// COLOR-2: the page the interface is laid out on when nothing asks the terminal for its own.
const MONO_BG: (u8, u8, u8) = (0x1e, 0x1e, 0x1e);

/// The text a yank took, lit (UI-13).
struct Flash {
    id: Id,
    chars: std::ops::Range<usize>,
    ver: u64,
    n: u64,
}

/// A file open in sim. Ids are never reused, so a late answer for a closed file is dropped.
pub type Id = usize;

/// SYN-3: files over this many bytes are not highlighted.
pub const MAX_HL: usize = 10_000_000;

#[derive(Debug)]
pub enum SimEvent {
    /// An event of one open file: a change on disk, the result of a save, a reload.
    App(Id, Event),
    /// What the terminal sent that the core understands: a key, a paste, a new size, its background.
    Term(Event),
    /// The mouse (PTR-1): a click, a drag, a release, the wheel.
    Mouse(MouseEvent),
    /// Shift-Tab, which the core's `Key` does not have.
    BackTab,
    Tick(u64),
    Signal(i32),
    /// A thread has something to tell the user.
    Notice(String),
    /// A directory has been read (TREE-5).
    Listing(PathBuf, Result<Vec<Entry>, String>),
    /// The answer to `SimEffect::Open`: the file, read.
    Opened(Opened),
    /// The answer to `SimEffect::Open` when the path is a directory: the id asked for, and the path.
    Dir(Id, String),
    /// FIND-3: the files under the root, sorted, relative to it.
    Files(Vec<String>),
    /// MARK-1: git's status, and where the root lies below the top of the repository.
    Status(Result<(Vec<u8>, String), String>),
    /// SYN-1: the payload of a reply of the terminal to OSC 4.
    Osc(String),
    /// SYN-3: the spans of a window of a file.
    Spans(Hl),
    /// GREP-5: a batch of what a content search found.
    Grep(grep::Batch),
    /// GREP-4: the answer to `SimEffect::Preview`.
    Preview(Pv),
    /// UI-5: git's diff of a file against HEAD (or why there is none), and how many lines the file
    /// had when it was asked for.
    Diff(Id, Result<Vec<u8>, String>, usize),
    /// UI-13: the flash of a yank, the one with this number, is over.
    Unflash(u64),
    /// PICK-5: the spinner of the search of this generation is to show its next frame.
    Spin(u64),
    /// KEY-3: the pause asked for after the key that made this number is over.
    Pause(u64),
    /// KEY-4: F1, which the core's `Key` does not have.
    Help,
    /// DIFF-2: the numbers of the changes view, or why there are none.
    Changes(Result<Counts, String>),
    /// DIFF-3: HEAD's version of a file (named as it was asked for), or why there is none.
    Shown(String, Result<Vec<u8>, String>),
    /// GUI-6: the background and the ANSI colors (0 to 7, those the window has) that a terminal
    /// would have answered with, told as they are: a window has none to ask.
    Colors(Rgb, [Option<Rgb>; 8]),
}

/// UI-5: a file to ask git's diff of: its id, its name as its App has it (relative to the working
/// directory), and how many lines it has now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffReq {
    pub id: Id,
    pub path: String,
    pub lines: usize,
}

/// GREP-4: a file read for the preview: the name asked for (relative to the root) and what the
/// read gave.
#[derive(Debug)]
pub struct Pv {
    pub path: String,
    pub file: Read,
}

#[derive(Debug)]
pub enum SimEffect {
    Render(Screen),
    /// What a file's App asks for: a write, a watch, a snapshot, the clipboard, a reload.
    App(Id, Effect),
    /// Read this directory and answer with `SimEvent::Listing`.
    Read(PathBuf),
    /// Open this file (a name relative to the working directory) as file `Id`: watch it, read it,
    /// look for the snapshot a dead session left, and answer with `SimEvent::Opened`; or with
    /// `SimEvent::Dir` if it is a directory.
    Open(Id, String),
    /// The file is no longer open: stop watching it.
    Close(Id),
    /// A recovered snapshot (journal key, pid of the dead session) is ours to delete once the
    /// file's own snapshot is on disk, or when the file is saved or closed (SAFE-6).
    Hold(Id, String, u32),
    /// List the files under the root and answer with `SimEvent::Files` (FIND-3). The shell runs
    /// it so that it never runs twice at once: a request while it runs is one more run after it.
    Files,
    /// Ask git for the status of the root and answer with `SimEvent::Status` (MARK-1); likewise.
    Status,
    /// Save the history of opened files (MARK-2).
    History(History),
    /// Highlight a window of a file and answer with `SimEvent::Spans` (SYN-3).
    Highlight(Req),
    /// Search the contents of files and answer with `SimEvent::Grep`, in batches; a search that
    /// is not the newest is to stop (GREP-5).
    Grep(grep::Request),
    /// Read this file (relative to the root) for the preview and answer with `SimEvent::Preview` (GREP-4).
    Preview(String),
    /// Ask for the diff of a file against HEAD and answer with `SimEvent::Diff` (UI-5). Requests
    /// for one file that wait are one: the newest.
    Diff(DiffReq),
    /// UI-13: answer with `SimEvent::Unflash(n)` after `FLASH_MS` milliseconds.
    Flash(u64),
    /// PICK-5: answer with `SimEvent::Spin(n)` after `SPIN_MS` milliseconds.
    Spin(u64),
    /// LOOK-1: the look that was chosen, to remember.
    Look(Skin),
    /// UI-2: whether the icons are on, to remember.
    Icons(bool),
    /// MD-5: whether the read view of Markdown is on, to remember.
    Md(bool),
    /// KEY-3: answer with `SimEvent::Pause(n)` after `PAUSE_MS` milliseconds.
    Pause(u64),
    /// KEY-6: how many times Tab and Shift-Tab were used, to remember.
    TabUses(u8),
    /// DIFF-2: ask git for the numbers of the changes view (the untracked files, relative to the root,
    /// are the ones whose lines are counted) and answer with `SimEvent::Changes`.
    Changes(Vec<PathBuf>),
    /// DIFF-3: ask git for HEAD's version of this file (relative to the root) and answer with
    /// `SimEvent::Shown`.
    Show(String),
    Quit(i32),
}

/// A snapshot that a dead session left (SAFE-6), the file that holds it, its age in ms and the
/// instant its merge gives up at: what `Event::Recover` takes.
#[derive(Debug)]
pub struct Found(pub Snapshot, pub String, pub u64, pub Instant);

#[derive(Debug)]
pub struct Opened {
    pub id: Id,
    pub path: String,
    /// The journal key (SAFE-6): two names of one file have the same.
    pub key: String,
    pub file: Read,
    pub found: Option<Found>,
    /// Where the file was closed last time (CLI-6): line and column.
    pub place: Option<(usize, usize)>,
}

/// SYN-3: lines `from..from + n` of the file `id` named `name`, as of version `ver` of `text`.
#[derive(Debug)]
pub struct Req {
    pub id: Id,
    pub name: String,
    pub ver: u64,
    pub text: Rope,
    pub from: usize,
    pub n: usize,
}

/// SYN-3: the spans of lines `from..` of a file at version `ver`, which had `lines` lines.
#[derive(Debug)]
pub struct Hl {
    pub id: Id,
    pub ver: u64,
    pub lines: usize,
    pub from: usize,
    pub spans: Vec<Spans>,
}

/// UI-18: what a section was found for: the text's version, the cursor's line, and whether the
/// highlighter calls that line and the one above it fenced.
type CrumbKey = (u64, usize, bool, bool);

pub struct Buf {
    pub id: Id,
    pub key: String,
    pub app: App,
    /// The App's last frame, valid for the size it has now.
    frame: Option<Frame>,
    /// The latest spans, and what was last asked for (version, first and last line on screen).
    hl: Option<Hl>,
    asked: Option<(u64, usize, usize)>,
    /// Where the command line scrolls to on the full-width status line (VI-12): the App's own
    /// `vi.off` is that of its narrower frame, which every one of its updates renders.
    wide: usize,
    /// The row of the frame the text's cursor is on, kept while the command line has the cursor (LOOK-6).
    crow: usize,
    /// UI-5: the file's changes against HEAD, as the last answer of git said.
    diff: Option<Diff>,
    /// UI-10: the language server's state for this file (` rust-analyzer · sandboxed`), which the
    /// code of the server's own sets and the status line shows as it is; sim never writes it.
    pub lsp: Option<String>,
    /// UI-10: whether what `lsp` says is an alert (a server running with no sandbox), which the
    /// slot draws in the alert's color; set by the server's code.
    pub lsp_alert: bool,
    /// UI-4: the language server's errors and warnings for the file, which the tab shows (`✘ 2 ▲ 1`);
    /// set by the server's own code, never by sim.
    pub diag: (usize, usize),
    /// UI-6: how the file is indented, found from its first lines when it is opened and when it
    /// is saved or changes on disk.
    indent: Indent,
    /// UI-11: the lines the last search matches, for the scrollbar, found for a pattern and a
    /// version of the text.
    hits: Option<Hits>,
    /// UI-14: whether the matches of the last search are lit (a search, `n` `N` `*` `#` light them;
    /// `:noh` and Esc in Normal mode put them out), and the matcher of the pattern.
    lit: bool,
    finder: Option<Finder>,
    /// UI-17: where the matches are (the first 100, as chars of the text) for a pattern and a
    /// version of the text, to say which one the cursor is at.
    count: Option<Count>,
    /// UI-18: the section the cursor's line is in, for a version of the text, a line and whether
    /// the highlighter calls it and the line above it fenced ones.
    crumb: Option<(CrumbKey, Option<String>)>,
}

/// UI-14: the pattern (and whether it was asked to be case-sensitive) a matcher was made for; none
/// if the pattern does not compile.
struct Finder {
    key: (String, bool),
    m: Option<Matcher>,
}

/// UI-17: the chars the first matches start at, and what they were found for (as `Hits`).
struct Count {
    key: (String, bool, u64),
    at: Vec<usize>,
}

/// The lines a search matches, and what they were found for: the pattern, whether it was asked to be
/// case-sensitive, and the version of the text.
struct Hits {
    key: (String, bool, u64),
    lines: Vec<usize>,
}

#[derive(Default)]
pub struct Panel {
    pub open: bool,
    pub focus: bool,
    /// The cursor row and the first row on screen.
    pub cur: usize,
    pub top: usize,
    /// A `g` waits for a second one.
    g: bool,
    /// A path (below the root) to show: its directories are expanded, step by step, as their
    /// listings arrive, and the cursor goes there.
    reveal: Option<PathBuf>,
    /// FIND-1: the search that runs, if any.
    search: Option<Search>,
    /// GREP-1: the content search that runs, if any.
    live: Option<Live>,
}

#[derive(Default)]
pub struct Sim {
    pub bufs: Vec<Buf>,
    pub cur: usize,
    pub tree: Tree,
    pub panel: Panel,
    pub root: PathBuf,
    cwd: PathBuf,
    size: (u16, u16),
    next: Id,
    /// The keys of `+cmd` and `file:line`, typed into the first file once it is open.
    keys: String,
    msg: Option<String>,
    /// FIND-3: the files under the root, sorted (shared with the thread of a content search).
    files: Arc<Vec<String>>,
    /// GREP-5: the generation of the last content search asked for.
    live_gen: u64,
    /// MARK-1: git's status (and the prefix), the paths seen changing on disk, and the marks.
    status: (Vec<u8>, String),
    disk: HashSet<PathBuf>,
    marks: Marks,
    no_repo: bool,
    /// MARK-1: git was not run because the repository's config could run commands (told once).
    skipped: bool,
    /// MARK-2: the history, and the time in seconds (at the start, then the shell's ticks).
    history: History,
    now: u64,
    t0: u64,
    /// MARK-3: the rows of the narrowed tree, when it is narrowed.
    focus: Option<Tree>,
    /// DIFF-1: the changes view is on (its narrowed tree is `focus`), with the numbers git gave.
    changes: Option<Changes>,
    /// VIS-4: the core's palette, made of the terminal's background (it paints the four styles).
    /// SYN-1, SYN-2, SYN-5: the syntax colors, whether they are on, and a `z` just typed.
    pal: Option<Palette>,
    syn: Syntax,
    hl_off: bool,
    z: bool,
    /// LOOK-1: the look, and the colors it adds to the palette (none without a palette or truecolor, LOOK-8).
    pub skin: Skin,
    tones: Option<Tones>,
    /// UI-10: whether the root is trusted (the language servers' `:trust`), which the status line
    /// shows in the security slot; none while nobody has said. Sim never writes it.
    pub trust: Option<bool>,
    /// UI-13: the text a yank has just taken, lit for `FLASH_MS`: the file, the chars, the version
    /// of the text they are of, and which flash it is.
    flash: Option<Flash>,
    flash_gen: u64,
    /// UI-1, UI-2: whether the interface is drawn in flat and depth, `$HOME`, and the icons.
    chrome: bool,
    home: Option<PathBuf>,
    pub icons: bool,
    /// MD-1, MD-5: whether the read view of Markdown is on (never in plain mode).
    pub md: bool,
    /// `SIM_ON_NORMAL` (VI-11), which every file's App runs.
    hook: Option<String>,
    /// PLAIN-1: plain mode: calm, no panel at the start, and no program (git, rg, filters, the hook).
    pub plain: bool,
    /// PTR-1: the press in the text that a drag goes on from: the file, and whether it has begun.
    drag: Option<(Id, bool)>,
    /// KEY-2: what was typed after a Space that makes no sequence yet: `Some("")` after the Space,
    /// `Some("b")` after `<Space>b`.
    leader: Option<String>,
    /// KEY-5: the letters are on the tabs (`Some(false)` to show a file, `Some(true)` to close one).
    tabpick: Option<bool>,
    /// KEY-3: the number of the last pause asked for, and the prefix whose window is open.
    pause: u64,
    which: Option<Vec<String>>,
    /// KEY-6: how many times Tab and Shift-Tab were used (at most 3), and whether the hint was told in this run.
    pub tab_uses: u8,
    hinted: bool,
}

/// What `sim` starts with besides what it was asked to open: the shell's side of the world.
#[derive(Default)]
pub struct Env {
    pub cwd: PathBuf,
    pub size: (u16, u16),
    /// Seconds since the epoch.
    pub now: u64,
    pub history: History,
    pub syn: Syntax,
    pub hook: Option<String>,
    /// PLAIN-1: plain mode by default (root, sudoedit): the same as `--plain`.
    pub plain: bool,
    /// LOOK-1: the look to start with (calm if none is given).
    pub skin: Skin,
    /// UI-2: `$HOME`, for the root the header shows as `~/...`.
    pub home: Option<PathBuf>,
    /// UI-2: whether the tree has icons (the saved choice; the tests start without).
    pub icons: bool,
    /// KEY-6: how many times Tab and Shift-Tab were used (at most 3; the saved count).
    pub tab_uses: u8,
    /// MD-5: whether the read view of Markdown is on (the saved choice; the tests start without).
    pub md: bool,
    /// ICON-1: the first status line is the hint that `zi` turns the icons on.
    pub hint: bool,
    /// UI-1: whether flat and depth draw the interface (the binary does; the tests of the looks
    /// before it start without, and say so when they test it).
    pub chrome: bool,
}

/// What `sim` was asked to open (TREE-1).
pub struct Start {
    pub root: PathBuf,
    pub file: Option<String>,
    pub keys: String,
    /// PLAIN-1: `--plain` was given.
    pub plain: bool,
}

/// TREE-1: `sim`, `sim {dir}`, `sim {file}`, with the core's rules for the file; PLAIN-1: and
/// `--plain` among the options (before a `--`). `None` is a usage error.
pub fn start(
    args: &[String],
    cwd: &Path,
    exists: impl Fn(&str) -> bool,
    is_dir: impl Fn(&Path) -> bool,
) -> Option<Start> {
    let opts = args.iter().position(|a| a == "--").unwrap_or(args.len());
    let plain = args[..opts].iter().any(|a| a == "--plain");
    let rest: Vec<String> = args
        .iter()
        .enumerate()
        .filter(|(i, a)| *i >= opts || *a != "--plain")
        .map(|x| x.1.clone())
        .collect();
    let cli = cli::parse(&rest, exists)?;
    let dir = cli.file.as_deref().map(|f| cwd.join(f)).filter(|d| is_dir(d));
    Some(match dir {
        Some(root) => Start { root, file: None, keys: String::new(), plain },
        None => Start { root: cwd.into(), file: cli.file, keys: cli.keys, plain },
    })
}

/// The history's key for a path: the same file under `./a` and `a` is one key.
fn key_of(path: &Path) -> String {
    path.components().collect::<PathBuf>().to_string_lossy().into_owned()
}

/// The width of the line numbers of a buffer (the core's gutter).
fn digits(buf: &Buffer) -> usize {
    buf.lines().to_string().len().max(3)
}

/// What the core says of a file that is not valid UTF-8 (`read-only: ` and this).
const NOT_UTF8: &str = "not valid UTF-8";

/// MD-1: whether a file is Markdown: by its extension, as the core does for prose (`view::render`).
fn is_markdown(path: Option<&str>) -> bool {
    let ext = path.and_then(|p| Path::new(p).extension()).and_then(|e| e.to_str());
    ext.is_some_and(|e| e.eq_ignore_ascii_case("md") || e.eq_ignore_ascii_case("markdown"))
}

/// Line `l` of the buffer without its line break; none past the end.
/// The first 200 chars of line `l`, for what reads a line only to know what kind it is (a shebang, a
/// heading, a definition): a line of a minified file is not copied whole for it.
fn head_of(buf: &Buffer, l: usize) -> Option<String> {
    (l < buf.lines()).then(|| buf.text.line(l).chars().take(200).collect())
}

fn text_of(buf: &Buffer, l: usize) -> Option<String> {
    (l < buf.lines()).then(|| buf.text.line(l).chars().take(buf.line_len(l)).collect())
}

/// What follows the line address at the start of `s` (the core's: `.` `$` `'<` `'>` or digits).
fn after_address(s: &str) -> Option<&str> {
    match s.chars().next()? {
        '.' | '$' => Some(&s[1..]),
        '\'' => s[1..].strip_prefix(['<', '>']),
        '0'..='9' => Some(s.trim_start_matches(|c: char| c.is_ascii_digit())),
        _ => None,
    }
}

/// What follows the range at the start of `s`, as the core reads one: `%`, or an address and perhaps a
/// `,` and another.
fn after_range(s: &str) -> &str {
    let Some(rest) = s.strip_prefix('%').map_or_else(|| after_address(s), Some) else { return s };
    rest.strip_prefix(',').and_then(after_address).unwrap_or(rest)
}

/// KEY-4: `:help`, `:h` and `:help words`: the words typed after it (none for the first two).
fn help_words(line: &str) -> Option<String> {
    let s = line.strip_prefix(':')?.trim();
    let (name, words) = s.split_once(char::is_whitespace).unwrap_or((s, ""));
    matches!(name, "help" | "h").then(|| words.trim().to_string())
}

/// The commands for all the files at once, which the core's App does not know (BUF-3), and `:e name`,
/// which opens another file instead of replacing this one (BUF-1): name, `!`, argument. The core takes
/// a range before `:e` and ignores it, and would replace this file by the other: `:%e name` is
/// `:e name` here too.
fn all(line: &str) -> Option<(&str, bool, &str)> {
    fn parse(s: &str) -> Option<(&str, bool, &str)> {
        let (name, rest) =
            s.split_at(s.find(|c: char| !c.is_ascii_alphabetic()).unwrap_or(s.len()));
        let (bang, arg) = rest.strip_prefix('!').map_or((false, rest), |a| (true, a));
        let ok = matches!(
            (name, arg.trim().is_empty()),
            ("qa" | "qall" | "wa" | "wall", true) | ("e" | "edit", false)
        );
        ok.then_some((name, bang, arg.trim()))
    }
    let s = line.strip_prefix(':')?.trim();
    parse(s).or_else(|| parse(after_range(s).trim_start()).filter(|c| c.0.starts_with('e')))
}

impl Sim {
    pub fn new(s: Start, env: Env) -> (Sim, Vec<SimEffect>) {
        let plain = s.plain || env.plain;
        let open = s.file.is_none() && !plain;
        let panel = Panel { open, focus: open, ..Panel::default() };
        let (root, keys, size) = (s.root, s.keys, (env.size.0.max(1), env.size.1.max(2)));
        let (history, syn, cwd) = (env.history, env.syn, env.cwd);
        let mut sim = Sim { panel, cwd, size, keys, root, history, syn, plain, ..Sim::default() };
        // Plain mode is calm, with no hook, and (PLAIN-1) asks for no git and no highlighting colors.
        (sim.skin, sim.hook) = if plain { (Skin::Calm, None) } else { (env.skin, env.hook) };
        (sim.chrome, sim.home, sim.icons) = (env.chrome, env.home, env.icons);
        (sim.tab_uses, sim.md) = (env.tab_uses, env.md && !plain);
        if sim.syn.mono {
            // COLOR-2: nothing asks the terminal, so the interface is laid out on a dark page, whose
            // colors the Painter does not write.
            sim.pal = Some(palette(MONO_BG));
            sim.retone();
        }
        (sim.now, sim.t0) = (env.now, env.now);
        sim.msg = (env.hint && !plain).then(|| "zi: icons (needs a Nerd Font)".into());
        let mut out = vec![SimEffect::Read(sim.root.clone())];
        out.push(SimEffect::Files);
        sim.git(&mut out);
        match s.file {
            Some(f) => {
                sim.note(&sim.cwd.join(&f), &mut out);
                sim.request(f, &mut out);
            }
            // Plain mode without a file is the core's unnamed buffer (the empty name; its snapshot's key is
            // the empty path, SAFE-6).
            None if plain => sim.request(String::new(), &mut out),
            None => {}
        }
        let screen = sim.frame(&mut out);
        out.push(SimEffect::Render(screen));
        (sim, out)
    }

    /// Where the parts of the screen are (LOOK-3). The panel is a third of the screen, at most 32
    /// columns (0 when it is closed or the screen is too narrow for it), and a column of space
    /// follows; in depth the borders of the boxes are around it and the editor.
    pub fn geo(&self) -> Geo {
        Geo::with_head(self.size, self.skin, self.panel.open, self.ui())
    }

    /// UI-1: whether the interface is drawn: flat or depth with tones (a palette and truecolor).
    pub fn ui(&self) -> bool {
        self.chrome && self.skin != Skin::Calm && self.tones.is_some()
    }

    /// The columns of the panel and of the editor.
    pub fn widths(&self) -> (u16, u16) {
        let g = self.geo();
        (g.pw, g.ew)
    }

    pub fn update(&mut self, ev: SimEvent) -> Vec<SimEffect> {
        let mut out = vec![];
        self.fit();
        match ev {
            SimEvent::Term(Event::Key(k)) => {
                self.key(k, &mut out);
                self.arm(&mut out);
            }
            SimEvent::Pause(n) => self.paused(n),
            SimEvent::Help => self.help(String::new(), &mut out),
            SimEvent::Changes(r) => match r {
                Ok(counts) if self.changes.is_some() => {
                    self.changes = Some(Changes::new(&counts, &self.status.1));
                    if self.marks.differing().next().is_none() {
                        self.msg = Some("no changes from HEAD".into());
                    }
                }
                Ok(_) => {}
                // (the status says why there is no view: git skipped, or no repository)
                Err(_) if self.changes.is_none() => {}
                Err(e) if e.starts_with(GIT_SKIPPED) => {}
                Err(e) => self.msg = Some(e),
            },
            SimEvent::Shown(path, r) => self.shown(path, r, &mut out),
            SimEvent::Term(Event::Paste(s)) if !self.panel.focus && self.panel.live.is_none() => {
                self.to(self.id(), Event::Paste(s), &mut out)
            }
            SimEvent::Term(Event::Resize(w, h)) => self.size = (w.max(1), h.max(2)),
            SimEvent::Term(Event::Bg(rgb)) if !self.syn.mono => self.background(rgb, &mut out),
            SimEvent::Term(_) => {}
            SimEvent::Mouse(m) if self.panel.live.is_none() => self.mouse(m, &mut out),
            SimEvent::Mouse(_) => {}
            SimEvent::BackTab => self.step(self.bufs.len().saturating_sub(1), &mut out),
            SimEvent::App(id, e) => self.to(id, e, &mut out),
            SimEvent::Notice(m) => self.msg = Some(m),
            SimEvent::Tick(n) => {
                self.now = self.t0 + n / 1000;
                let (fx, drawn) = self.snapshots(None, || Event::Tick(n));
                if !drawn {
                    return fx;
                }
                out = fx;
            }
            SimEvent::Signal(n) => {
                let mut out = self.snapshots(None, || Event::Signal(n)).0;
                out.push(SimEffect::Quit(128 + n));
                return out;
            }
            SimEvent::Listing(dir, r) => self.listing(&dir, r, &mut out),
            SimEvent::Opened(o) => self.opened(o, &mut out),
            SimEvent::Dir(id, path) => {
                out.push(SimEffect::Close(id)); // no file is open: its watcher is not needed
                // `open` noted it before it was known to be a directory: it is no file that was opened (MARK-2).
                self.history.forget(&key_of(&self.cwd.join(&path)));
                out.push(SimEffect::History(self.history.clone()));
                self.reveal_dir(&path, &mut out)
            }
            SimEvent::Files(files) => {
                let same = *self.files == files;
                self.files = Arc::new(files);
                self.search_run(&mut out);
                if !same {
                    self.live_run(&mut out);
                }
            }
            SimEvent::Status(r) => match r {
                Ok(status) => {
                    self.status = status;
                    self.remark();
                    self.diff(self.id(), &mut out); // (a commit moves HEAD)
                    self.ask_changes(&mut out); // DIFF-1: and the view's numbers with it
                }
                Err(e) => {
                    let (norepo, skip) =
                        (e.starts_with("not a git repository"), e.starts_with(GIT_SKIPPED));
                    self.no_repo |= norepo || skip;
                    // Told once (MARK-1), not again at every opening of the panel
                    let told = skip && replace(&mut self.skipped, true);
                    if self.changes.take().is_some() {
                        // DIFF-1: there is no view to make: it is left, and says why
                        self.focus = None;
                        self.msg = Some(e);
                    } else if (skip && !told) || !(norepo || skip) {
                        self.msg = Some(e);
                    }
                }
            },
            SimEvent::Osc(s) => {
                self.syn.reply(&s);
                self.retone();
            }
            SimEvent::Colors(bg, ansi) => {
                ansi.iter()
                    .enumerate()
                    .for_each(|(i, c)| c.iter().for_each(|c| self.syn.set(i, *c)));
                self.background(bg, &mut out);
            }
            SimEvent::Spans(h) => match self.bufs.iter_mut().find(|b| b.id == h.id) {
                Some(b) => b.hl = Some(h),
                None => self.live_spans(h), // (the picker's preview, PICK-4)
            },
            SimEvent::Grep(b) => self.live_batch(b, &mut out),
            SimEvent::Preview(pv) => self.live_preview(pv, &mut out),
            SimEvent::Spin(n) => self.live_spin(n, &mut out),
            SimEvent::Unflash(n) => {
                if self.flash.as_ref().is_some_and(|f| f.n == n) {
                    self.flash = None;
                }
            }
            SimEvent::Diff(id, r, lines) => {
                if let Some(b) = self.bufs.iter_mut().find(|b| b.id == id) {
                    b.diff = r.ok().map(|out| Diff::parse(&out, lines));
                }
            }
        }
        let screen = self.frame(&mut out);
        out.push(SimEffect::Render(screen));
        out
    }

    /// VIS-4: the terminal's background came, however late: the core's palette is made of it, every App
    /// is told (as the core's loop tells its one), and the syntax colors follow it (SYN-1).
    fn background(&mut self, rgb: (u8, u8, u8), out: &mut Vec<SimEffect>) {
        (self.pal, _) = (Some(palette(rgb)), self.syn.bg(rgb));
        self.retone();
        let ids: Vec<Id> = self.bufs.iter().map(|b| b.id).collect();
        ids.into_iter().for_each(|id| self.to(id, Event::Bg(rgb), out));
    }

    /// UI-1: the tones of the looks, made of the palette and of the colors the terminal has told.
    fn retone(&mut self) {
        let colors = self.syn.truecolor || self.syn.mono;
        self.tones = self.pal.filter(|_| colors).map(|p| Tones::with(&p, &self.syn));
    }

    /// Whether the terminal may still be answering what was asked of it (VIS-4): the quit then
    /// waits for it to finish, so that no answer is typed into the shell.
    pub fn asking(&self) -> bool {
        self.pal.is_none() || self.syn.pending()
    }

    /// Tells every App the size it has, so that what it renders fits next to the panel: the rows
    /// of the body and its status line.
    fn fit(&mut self) {
        let g = self.geo();
        for b in &mut self.bufs {
            b.app.size = (g.ew, g.rows + 1);
        }
    }

    /// GUI-5: whether the next key is text (typed in Insert mode, on the command line, in the picker's
    /// query, in the tree's search, the char after `f` or `r`) and not a command: a window turns its
    /// input method on for it, and off where a letter is a command.
    pub fn wants_text(&self) -> bool {
        match (self.panel.live.is_some(), self.panel.open && self.panel.focus) {
            (true, _) => true,
            (_, true) => self.panel.search.as_ref().is_some_and(|s| s.typing),
            _ => self.bufs.get(self.cur).is_some_and(|b| b.app.vi.text()),
        }
    }

    /// The id of the current file (none: 0, which no `to` finds).
    fn id(&self) -> Id {
        self.bufs.get(self.cur).map_or(0, |b| b.id)
    }

    /// Normal mode with nothing pending, in a file: the state in which Tab is ours (BUF-2).
    fn idle(&self) -> bool {
        self.bufs.get(self.cur).is_some_and(|b| b.app.vi.idle())
    }

    /// Shows the file `by` places on, wrapping round (Tab is 1, Shift-Tab one less than all).
    fn step(&mut self, by: usize, out: &mut Vec<SimEffect>) {
        let at = self.cur;
        if self.idle() {
            self.cur = (self.cur + by) % self.bufs.len();
        }
        if self.cur != at {
            self.used_tab(out); // KEY-6
        }
    }

    /// MARK-1: asks for git's status (not where it said there is no repository).
    fn git(&mut self, out: &mut Vec<SimEffect>) {
        out.extend((!self.no_repo && !self.plain).then_some(SimEffect::Status));
    }

    /// MARK-1: the marks from git's status and the paths seen changing on disk.
    fn remark(&mut self) {
        self.marks = Marks::new(&self.status.0, &self.status.1, &self.disk);
        self.refocus();
    }

    /// UI-11: finds the lines the last `/` search matches in the current file, for the scrollbar, when
    /// the pattern is a plain word (a regex is the core's to run, and its marks are not worth a scan on
    /// the way of a key), the file is not long (`HIT_LINES`) and the text is not being typed: once
    /// for a pattern and a version of the text.
    fn refresh_hits(&mut self) {
        let Some(b) = self.bufs.get_mut(self.cur) else { return };
        let (buf, mode) = (&b.app.buf, b.app.vi.mode);
        if matches!(mode, Mode::Insert | Mode::Cmd) {
            return; // the marks of the text as it was stay while it is typed
        }
        let want =
            b.app.search.clone().filter(|(p, ..)| b.lit && plain(p) && buf.lines() <= HIT_LINES);
        let Some((pat, cs, _)) = want else { return b.hits = None };
        let key = (pat.clone(), cs, buf.version());
        if b.hits.as_ref().is_some_and(|h| h.key == key) {
            return;
        }
        // The case rule of the core's search for a pattern with no escape: smartcase (one capital makes it
        // exact), or exact when it was asked to be.
        let fold = !cs && !pat.chars().any(char::is_uppercase);
        let need = if fold { pat.to_ascii_lowercase() } else { pat };
        let has = |l: usize| {
            let line: std::borrow::Cow<str> = buf.text.line(l).into();
            if fold { line.to_ascii_lowercase().contains(&need) } else { line.contains(&need) }
        };
        let lines = (0..buf.lines()).filter(|&l| has(l)).collect();
        b.hits = Some(Hits { key, lines });
    }

    /// UI-14: the matcher of the last search of the current file, made again when the pattern changes
    /// (and dropped when the matches are not lit).
    fn refresh_finder(&mut self) {
        let Some(b) = self.bufs.get_mut(self.cur) else { return };
        let Some((pat, cs, _)) = b.app.search.clone().filter(|_| b.lit) else {
            return b.finder = None;
        };
        if b.finder.as_ref().is_none_or(|f| f.key != (pat.clone(), cs)) {
            b.finder = Some(Finder { m: Matcher::new(&pat, cs), key: (pat, cs) });
        }
    }

    /// UI-17: finds where the first matches of the lit search are in the current file (not in a file
    /// of more than `HIT_LINES` lines, and not while the text is being typed), once for a pattern
    /// and a version of the text.
    fn refresh_count(&mut self) {
        let Some(b) = self.bufs.get_mut(self.cur) else { return };
        let (buf, mode) = (&b.app.buf, b.app.vi.mode);
        if matches!(mode, Mode::Insert | Mode::Cmd) {
            return; // the count of the text as it was stays while it is typed
        }
        let found = b.finder.as_ref().and_then(|f| Some((f.m.as_ref()?, &f.key)));
        let Some((m, (pat, cs))) = found.filter(|_| buf.lines() <= HIT_LINES) else {
            return b.count = None;
        };
        let key = (pat.clone(), *cs, buf.version());
        if b.count.as_ref().is_some_and(|c| c.key == key) {
            return;
        }
        let mut at: Vec<usize> = vec![];
        for l in 0..buf.lines() {
            if at.len() >= 100 {
                break;
            }
            let start = buf.start(l);
            at.extend(
                m.ranges(&text_of(buf, l).unwrap_or_default()).iter().map(|r| start + r.start),
            );
        }
        at.truncate(100);
        b.count = Some(Count { key, at });
    }

    /// UI-17: `[3/12]`, for the match the cursor is at (the last one that starts at or before it): `[0/12]`
    /// before the first, `>99` for more than 99. None when no search is lit or nothing matches.
    fn searchcount(&self) -> Option<String> {
        let b = self.bufs.get(self.cur)?;
        let at = &b.count.as_ref().filter(|_| b.lit)?.at;
        let (i, n) = (at.partition_point(|&p| p <= b.app.buf.cursor), at.len());
        let say = |k: usize| if k > 99 { ">99".to_string() } else { k.to_string() };
        (n > 0).then(|| format!("[{}/{}]", say(i), say(n)))
    }

    /// UI-11: the scrollbar's rows for the current file: the marks of changes on disk, the git signs
    /// against HEAD (every line for a file git does not know) and the matches of a search, the
    /// thumb for the lines on screen and the cursor's place.
    fn scrollbar(&self, win: Option<(usize, usize)>) -> Option<Vec<ScrollCell>> {
        let b = self.bufs.get(self.cur)?;
        let (buf, rows) = (&b.app.buf, usize::from(self.geo().rows));
        let loss = |gone: bool| if gone { Tick::Loss } else { Tick::Gain };
        let mut marks: Vec<(std::ops::Range<usize>, Tick)> =
            buf.marks.iter().map(|m| (m.line..m.line + 1, loss(m.kind == '-'))).collect();
        let untracked =
            self.open_path().is_some_and(|p| self.marks.git(&p) == Some(Git::Untracked));
        match (&b.diff, untracked) {
            (_, true) => marks.push((0..buf.lines(), Tick::Gain)),
            (Some(d), _) if d.all => marks.push((0..buf.lines(), Tick::Gain)),
            (Some(d), _) => marks.extend(d.signs.iter().map(|(&l, s)| {
                let tick = match s {
                    Sign::Add => Tick::Gain,
                    Sign::Change => Tick::Change,
                    Sign::Del | Sign::DelTop => Tick::Loss,
                };
                (l..l + 1, tick)
            })),
            _ => {}
        }
        marks.extend(b.hits.iter().flat_map(|h| h.lines.iter().map(|&l| (l..l + 1, Tick::Hit))));
        Some(layout::scrollbar(
            rows,
            buf.lines(),
            marks.into_iter(),
            win,
            Some(buf.line_of(buf.cursor)),
        ))
    }

    /// UI-4: the tabs: one for each open file, in order.
    pub fn tabs(&self) -> Vec<Tab> {
        let paths: Vec<&str> =
            self.bufs.iter().map(|b| b.app.path.as_deref().unwrap_or("[no name]")).collect();
        let names = tabs::names(&paths);
        let picking = self.tabpick.map(|_| self.tab_letters());
        let tab = |(i, (b, name)): (usize, (&Buf, String))| Tab {
            name,
            letter: picking.as_ref().and_then(|l| l.get(i).copied()),
            dirty: b.app.buf.dirty(),
            disk: !b.app.buf.marks.is_empty(),
            errors: b.diag.0,
            warns: b.diag.1,
        };
        self.bufs.iter().zip(names).enumerate().map(tab).collect()
    }

    /// UI-18: the parts of the breadcrumb of the current file: its path below the root, and the section
    /// the cursor is in. The section is kept for the version of the text and the cursor's line.
    fn crumb(&mut self) -> (Vec<String>, usize) {
        let (root, cwd) = (self.root.clone(), self.cwd.clone());
        let Some(b) = self.bufs.get_mut(self.cur) else { return Default::default() };
        let Some(path) = b.app.path.clone().filter(|p| !p.is_empty()) else {
            return Default::default();
        };
        let (buf, v) = (&b.app.buf, b.app.buf.version());
        let l = buf.line_of(buf.cursor);
        // the highlighter's word on a line: a fenced one starts with a zero-width band
        let band = |l: Option<usize>| {
            let (h, l) = (b.hl.as_ref()?, l?);
            h.spans.get(l.checked_sub(h.from)?)?.first().map(|s| s.1 == Look::Band)
        };
        let (fenced, above) = (band(Some(l)) == Some(true), band(l.checked_sub(1)) == Some(true));
        let key = (v, l, fenced, above);
        let section = match &b.crumb {
            Some((k, s)) if *k == key => s.clone(),
            _ => {
                let s = crumb::section(&path, l, &buf.text, fenced, above);
                b.crumb = Some((key, s.clone()));
                s
            }
        };
        let below = cwd.join(&path);
        let shown = below.strip_prefix(&root).unwrap_or(Path::new(&path)).to_string_lossy();
        let parts = crumb::parts(&shown, section.as_deref());
        let file = parts.len().saturating_sub(1 + usize::from(section.is_some()));
        (parts, file)
    }

    /// UI-5: asks for the diff of a file against HEAD (not where git said there is no repository).
    fn diff(&self, id: Id, out: &mut Vec<SimEffect>) {
        let req = self.bufs.iter().find(|b| b.id == id && !is_head(b)).and_then(|b| {
            let path = b.app.path.clone()?;
            Some(DiffReq { id, path, lines: b.app.buf.lines() })
        });
        out.extend(req.filter(|_| !self.no_repo && !self.plain).map(SimEffect::Diff));
    }

    /// MARK-2: notes that the user opened a file, and has the history saved.
    fn note(&mut self, path: &Path, out: &mut Vec<SimEffect>) {
        self.history.record(&key_of(path), self.now);
        out.extend((!self.plain).then(|| SimEffect::History(self.history.clone())));
    }

    /// Gives an event to one file's App, and runs what it asks for. A change on disk or a save
    /// is a change for MARK-1 (it works without git too), and git is asked again.
    fn to(&mut self, id: Id, ev: Event, out: &mut Vec<SimEffect>) {
        let changed = matches!(ev, Event::Disk { .. } | Event::Saved(Ok(_), _));
        let Some(b) = self.bufs.iter_mut().find(|b| b.id == id) else { return };
        // UI-14: what a key may light or put out is decided from the state before and after it.
        let pre = match &ev {
            Event::Key(k) => Some((*k, b.app.vi.mode, b.app.vi.idle(), b.app.search.clone())),
            _ => None,
        };
        let slash = b.app.vi.line.starts_with(['/', '?']);
        let (fx, path) = (b.app.update(ev), b.app.path.clone());
        if let Some((k, mode, idle, was)) = pre {
            b.lit = lit_after(b.lit, (k, mode, idle, slash), &was, &b.app.search);
        }
        let (yank, ver) = (yanked(&b.app, &fx), b.app.buf.version());
        self.run(id, fx, out);
        if let Some(chars) = yank {
            // UI-13: the text a yank took is lit for a moment, as the author's `on_yank` does.
            self.flash_gen += 1;
            self.flash = Some(Flash { id, chars, ver, n: self.flash_gen });
            out.push(SimEffect::Flash(self.flash_gen));
        }
        if let (true, Some(path)) = (changed, path) {
            if let Ok(rel) = self.cwd.join(path).strip_prefix(&self.root) {
                self.disk.insert(rel.into());
                self.remark();
            }
            self.git(out);
            self.diff(id, out);
            if let Some(b) = self.bufs.iter_mut().find(|b| b.id == id) {
                b.indent = indent_of(&b.app.buf);
            }
        }
    }

    /// Takes the frames and the quits out of an App's effects; the shell gets the rest.
    fn run(&mut self, id: Id, fx: Vec<Effect>, out: &mut Vec<SimEffect>) {
        let (mut quit, mut frame) = (None, None);
        for e in fx {
            match e {
                Effect::Render(f) => frame = Some(f),
                Effect::Quit(code) => quit = Some(code),
                e => out.push(SimEffect::App(id, e)),
            }
        }
        if let Some(b) = self.bufs.iter_mut().find(|b| b.id == id) {
            b.frame = frame.or(b.frame.take());
        }
        match quit {
            Some(0) => self.close(id, out),
            // `:cq`: the whole of sim ends, and the other files keep what is unsaved in them.
            Some(code) => {
                out.extend(self.snapshots(Some(id), || Event::Signal(0)).0);
                out.push(SimEffect::Quit(code));
            }
            None => {}
        }
    }

    /// BUF-3: closes a file: the one after it shows, the one before if it was the last; sim ends
    /// with the last one. The App has asked the journal to forget its snapshot already.
    fn close(&mut self, id: Id, out: &mut Vec<SimEffect>) {
        let Some(i) = self.bufs.iter().position(|b| b.id == id) else { return };
        self.bufs.remove(i);
        out.push(SimEffect::Close(id));
        self.cur = (self.cur - usize::from(i < self.cur)).min(self.bufs.len().saturating_sub(1));
        self.refocus();
        if self.bufs.is_empty() {
            out.push(SimEffect::Quit(0));
        }
    }

    /// Gives every App but `except` an event that makes it ask for a snapshot of its unsaved text
    /// (a tick, a signal: SAFE-6), and keeps only those requests (and, for a signal, the place of
    /// its cursor, CLI-6). The frame an App makes meanwhile is kept for the screen: a tick brings
    /// the seconds of a running filter (FILT-1).
    fn snapshots(&mut self, except: Option<Id>, ev: impl Fn() -> Event) -> (Vec<SimEffect>, bool) {
        let (mut out, mut drawn) = (vec![], false);
        for b in self.bufs.iter_mut().filter(|b| Some(b.id) != except) {
            for e in b.app.update(ev()) {
                match e {
                    Effect::Render(f) => (b.frame, drawn) = (Some(f), true),
                    Effect::Snapshot { .. } | Effect::Remember { .. } => {
                        out.push(SimEffect::App(b.id, e))
                    }
                    _ => {}
                }
            }
        }
        (out, drawn)
    }

    fn key(&mut self, k: Key, out: &mut Vec<SimEffect>) {
        self.msg = None;
        // KEY-5, KEY-2: the letters of the tabs are waiting for a key; a leader's sequence is half typed
        // or starts with this Space: the keys are the guide's until it is done
        if let Some(close) = self.tabpick.take() {
            return self.pick_tab(k, close, out);
        }
        if self.leader.is_some() || (k == Key::Char(' ') && self.leads()) {
            return self.leader_key(k, out);
        }
        match k {
            // PICK-2: while a picker is open every key is its own (Ctrl-n and Ctrl-p move in it)
            _ if self.panel.live.is_some() => return self.live_key(k, out),
            Key::Ctrl('n') => return self.toggle(out),
            Key::Ctrl('p') => return self.live_start(Pick::Files, out),
            Key::Ctrl('f') => return self.live_start(Pick::Grep, out),
            Key::Tab if self.idle() => return self.step(1, out),
            _ if self.panel.open && self.panel.focus => return self.panel_key(k, out),
            _ => {}
        }
        let Some(b) = self.bufs.get(self.cur) else {
            return out.extend((k == Key::Char('q')).then_some(SimEffect::Quit(0)));
        };
        let submit = k == Key::Enter && b.app.vi.mode == Mode::Cmd;
        let (id, line) = (b.id, if submit { b.app.vi.line.clone() } else { String::new() });
        if noh(&line) {
            // UI-14: `:noh` is ours (the core has no such command): the command line closes, as for `:qa`.
            self.to(id, Event::Key(Key::Esc), out);
            if let Some(b) = self.bufs.iter_mut().find(|b| b.id == id) {
                b.lit = false;
            }
            return;
        }
        if let Some(words) = help_words(&line) {
            self.to(id, Event::Key(Key::Esc), out); // the command line is ours: close it
            return self.help(words, out);
        }
        // (plain mode is the core alone: `:e name` replaces the file, there is no `:qa`)
        if let Some(cmd) = all(&line).filter(|_| !self.plain) {
            self.to(id, Event::Key(Key::Esc), out); // the command line is ours: close it
            return self.run_all(cmd, out);
        }
        // SYN-2: `zh` turns the colors off and on. The core's parser takes the `h` after a `z` and
        // does nothing, so the App still gets the key.
        let (zh, zl) = (self.z && k == Key::Char('h'), self.z && k == Key::Char('l'));
        let zi = self.z && k == Key::Char('i');
        self.z = self.idle() && k == Key::Char('z');
        self.to(id, Event::Key(k), out);
        if zh {
            self.hl_off = !self.hl_off;
            self.msg = Some(format!("syntax colors {}", if self.hl_off { "off" } else { "on" }));
        }
        // UI-2: `zi` turns the icons of the tree off and on, and the choice is remembered.
        if zi && !self.plain {
            self.icons = !self.icons;
            self.msg = Some(format!("icons {}", if self.icons { "on" } else { "off" }));
            out.push(SimEffect::Icons(self.icons));
        }
        // LOOK-1: `zl` goes on to the next look, and the choice is remembered; PLAIN-1: plain mode
        // stays calm.
        if zl && self.plain {
            self.msg = Some("plain mode: the look is calm".into());
        } else if zl {
            self.skin = self.skin.next();
            self.msg = Some(format!("look: {}", self.skin.name()));
            out.push(SimEffect::Look(self.skin));
        }
    }

    /// The names of the files `pick` says yes to, in order, with their ids.
    fn named(&self, pick: impl Fn(&Buf) -> bool) -> (Vec<Id>, String) {
        let files = self.bufs.iter().filter(|b| pick(b));
        let (ids, names): (_, Vec<_>) =
            files.map(|b| (b.id, b.app.path.clone().unwrap_or_default())).unzip();
        (ids, names.join(", "))
    }

    /// BUF-3: `:qa` and `:wa` hold SAFE-7 for each file. `:qa` stays while one is modified;
    /// `:wa` leaves out the files that have a conflict with the disk, naming them, and saves the rest.
    fn run_all(&mut self, (name, bang, arg): (&str, bool, &str), out: &mut Vec<SimEffect>) {
        let (modified, names) = self.named(|b| b.app.buf.dirty());
        match name {
            "qa" | "qall" if bang || modified.is_empty() => {
                for b in &self.bufs {
                    let fx = [Some(Effect::Unsnapshot), b.app.place()]; // CLI-6: the place is remembered
                    out.extend(fx.into_iter().flatten().map(|e| SimEffect::App(b.id, e)));
                }
                out.push(SimEffect::Quit(0));
            }
            "qa" | "qall" => {
                self.msg = Some(format!("no write since last change in {names} (:qa! to discard)"))
            }
            "wa" | "wall" if modified.is_empty() => self.msg = Some("no modified files".into()),
            "wa" | "wall" => {
                let (stuck, stuck_names) = self.named(|b| b.app.diverged());
                for id in modified.into_iter().filter(|id| !stuck.contains(id)) {
                    self.to(id, Event::Key(Key::Ctrl('s')), out);
                }
                if !stuck.is_empty() {
                    self.msg = Some(format!(
                        "not saved: conflicts with the disk in {stuck_names} (:w! keeps yours, :e! takes the disk's)"
                    ));
                }
            }
            _ => self.open(self.cwd.join(arg), out),
        }
    }

    /// BUF-1: shows the file if it is open, else asks for it to be read. MARK-2: either way the
    /// user chose it, so it goes into the history.
    fn open(&mut self, path: PathBuf, out: &mut Vec<SimEffect>) {
        self.note(&path, out);
        let same = |b: &Buf| b.app.path.as_ref().is_some_and(|p| self.cwd.join(p) == path);
        if let Some(i) = self.bufs.iter().position(same) {
            (self.cur, self.panel.focus) = (i, false);
        } else {
            self.request(
                path.strip_prefix(&self.cwd).unwrap_or(&path).to_string_lossy().into_owned(),
                out,
            );
        }
    }

    fn request(&mut self, name: String, out: &mut Vec<SimEffect>) {
        out.push(SimEffect::Open(self.next, name));
        self.next += 1;
    }

    fn opened(&mut self, o: Opened, out: &mut Vec<SimEffect>) {
        self.panel.focus = false;
        if let Some(i) = self.bufs.iter().position(|b| b.key == o.key) {
            self.cur = i; // another name of a file that is open: one buffer, one snapshot
            self.keys.clear(); // (a jump to a line was meant for the file just read)
            return out.push(SimEffect::Close(o.id));
        }
        let g = self.geo();
        // (the empty name is plain mode's unnamed buffer)
        let mut app =
            App::new(Some(o.path.clone()).filter(|p| !p.is_empty()), o.file, (g.ew, g.rows + 1));
        app.wide = true; // prose wraps at the pane's width, not at 72 columns (UI-9)
        (app.hook, app.plain) = (self.hook.clone(), self.plain);
        let mut fx = vec![];
        if let Some(Found(s, file, age, by)) = o.found {
            let v = app.buf.version();
            fx = app.update(Event::Recover(s.base, s.text, age, file, by));
            if app.buf.version() != v {
                out.push(SimEffect::Hold(o.id, s.path, s.pid));
            }
        }
        // A `+cmd` or `file:line` says where to go; otherwise it is where the file was closed (CLI-6).
        if let Some((l, c)) = o.place.filter(|_| self.keys.is_empty()) {
            fx.extend(app.update(Event::Place(o.path, l, c)));
        }
        fx.extend(self.pal.map(|p| app.update(Event::Bg(p.bg))).into_iter().flatten());
        fx.extend(app.start(&take(&mut self.keys)));
        self.bufs.push(Buf {
            id: o.id,
            key: o.key,
            app,
            frame: None,
            hl: None,
            asked: None,
            wide: 0,
            crow: 0,
            diff: None,
            lsp: None,
            lsp_alert: false,
            diag: (0, 0),
            indent: Indent::default(),
            hits: None,
            lit: false,
            count: None,
            finder: None,
            crumb: None,
        });
        if let Some(b) = self.bufs.last_mut() {
            b.indent = indent_of(&b.app.buf);
        }
        self.cur = self.bufs.len() - 1;
        self.refocus();
        self.run(o.id, fx, out);
        self.tab_hint(); // KEY-6
        self.diff(o.id, out);
    }

    // ---- the frame ----

    /// SYN-1, SYN-3: the colors of the file on screen. The spans come from the highlighter's
    /// thread: the last ones are used until new ones come (an edit leaves the colors of the other
    /// lines as they were for the moment), and when they do not cover what is on screen they are
    /// asked for again, a screenful above and below.
    fn colors(&self, b: &Buf, f: &Frame) -> (Colored, Option<Req>) {
        let buf = &b.app.buf;
        let text = |l: usize| text_of(buf, l);
        // Lines inserted or removed at the cursor move the lines below it: their old spans are
        // shifted to match, for the moment until the new ones arrive.
        let (hl, cur) = (b.hl.as_ref().filter(|_| !self.hl_off), buf.line_of(buf.cursor));
        let spans = |l: usize| {
            let h = hl?;
            let old = if l > cur {
                l.checked_add_signed(h.lines as isize - buf.lines() as isize)?
            } else {
                l
            };
            h.spans.get(old.checked_sub(h.from)?)
        };
        let lit = self.flash.as_ref().filter(|f| f.id == b.id && f.ver == buf.version());
        let guides = self.ui().then(|| guides_of(b, lit.map(|f| f.chars.clone())));
        let line_ui = guides.as_ref().map(|g| move |l: usize| g.of(buf, l));
        // MD-1: the lines the read view leaves as they are: the cursor's, and the selected ones
        let read = (self.md && is_markdown(b.app.path.as_deref())).then(|| {
            let (cl, last) = (buf.line_of(buf.cursor), |e: usize| buf.line_of(e.saturating_sub(1)));
            b.app
                .selection()
                .map_or(cl..cl + 1, |(s, e)| buf.line_of(s)..last(e).max(buf.line_of(s)) + 1)
        });
        let colored = layout::colors(
            f,
            digits(buf),
            &text,
            &spans,
            line_ui.as_ref().map(|f| f as &dyn Fn(usize) -> LineUi),
            read,
        );
        let win = colored.win;
        let (ver, h) = (buf.version(), usize::from(self.size.1));
        let (Some((a, z)), Some(name)) = (win, b.app.path.clone()) else {
            return (colored, None);
        };
        let z = z.max(a); // a screen too narrow for its gutter shows cut numbers: no window that ends before it starts
        // MD-4: a table that the window has not all of is asked for whole
        let (a, z) = colored.need.as_ref().map_or((a, z), |n| (a.min(n.start), z.max(n.end)));
        let covered = |h: &Hl| h.ver == ver && h.from <= a && z < h.from + h.spans.len();
        let (from, on) = (a.saturating_sub(h), !self.hl_off && buf.text.len_bytes() <= MAX_HL);
        let n = z + h - from + 1;
        let ask = on && !b.hl.as_ref().is_some_and(covered) && b.asked != Some((ver, from, n));
        (colored, ask.then(|| Req { id: b.id, name, ver, text: buf.text.clone(), from, n }))
    }

    /// LOOK-2: what each row of the current file's frame is, when flat or depth tints them.
    fn changes(&self) -> Vec<Kind> {
        let b = self.bufs.get(self.cur).filter(|_| self.skin != Skin::Calm && self.tones.is_some());
        b.and_then(|b| Some(layout::changed(b.frame.as_ref()?, digits(&b.app.buf))))
            .unwrap_or_default()
    }

    /// UI-5: the git sign of each row of the frame of a file: from the last answer of git, and every
    /// line for a file git does not know yet.
    fn signs(&self, b: &Buf, f: &Frame, ui: bool) -> Vec<Option<Sign>> {
        let (buf, rel) = (&b.app.buf, b.app.path.as_ref().map(|p| self.cwd.join(p)));
        let untracked =
            rel.and_then(|p| p.strip_prefix(&self.root).ok().map(|r| self.marks.git(r))).flatten()
                == Some(Git::Untracked);
        let all = Diff::all_added(buf.lines());
        let Some(diff) = b.diff.as_ref().filter(|_| !untracked).or(untracked.then_some(&all))
        else {
            return vec![];
        };
        if !ui {
            return vec![];
        }
        let (edit, now) = (buf.line_of(buf.cursor), buf.lines());
        layout::sign_rows(f, digits(buf), &|l| diff.sign(l, edit, now))
    }

    /// UI-3: the open file's path below the root, if it is below it.
    fn open_path(&self) -> Option<PathBuf> {
        let path = self.cwd.join(self.bufs.get(self.cur)?.app.path.as_ref()?);
        path.strip_prefix(&self.root).ok().map(Path::to_path_buf)
    }

    /// The whole screen: the panel, the current file and the status line (BUF-2: `2/5` is
    /// appended when more than one file is open).
    fn frame(&mut self, out: &mut Vec<SimEffect>) -> Screen {
        self.fit();
        self.scroll();
        self.live_prepare(out);
        self.refresh_finder();
        self.refresh_count();
        let ((w, h), g) = (self.size, self.geo());
        let focus = self.panel.open && self.panel.focus;
        let rows =
            if g.pw > 0 { self.view().rows(self.panel.top, usize::from(g.rows)) } else { vec![] };
        let root_str = self.root.to_string_lossy();
        let likely = if g.pw > 0 { self.history.top(&root_str, 5, self.now) } else { vec![] };
        let (ui, open) = (self.ui(), self.open_path());
        let marked: Vec<(TreeRow, Mark)> =
            rows.into_iter().map(|r| self.mark(r, &likely, open.as_deref(), ui)).collect();
        let at = self.row(self.panel.cur);
        let root = self.root.display().to_string();
        // (a directory's name ends with `/`, which is part of the state words: a path is cut as a list of components)
        let (tree, slash) = at.as_ref().map_or((root.clone(), ""), |r| {
            (r.path.display().to_string(), if r.is_dir { "/" } else { "" })
        });
        // BUF-2: the position `2/5` follows the status, which is cut to what is left of the width;
        // on the command line, which has the whole width, there is none.
        let cmd = !focus && self.bufs.get(self.cur).is_some_and(|b| b.app.vi.mode == Mode::Cmd);
        let found = self.searchcount().filter(|_| !cmd && !focus);
        let pos = if self.bufs.len() > 1 && !cmd {
            format!("  {}/{}", self.cur + 1, self.bufs.len())
        } else {
            String::new()
        };
        let pos = found.map_or(String::new(), |c| format!("  {c}")) + &pos; // UI-17
        let room = usize::from(w).saturating_sub(pos.width());
        let (mut status, mut cursor, mut shape) =
            (layout::short(&root, room), (g.tx(), g.y0), Shape::Block);
        if let Some(b) = self.bufs.get_mut(self.cur) {
            if b.frame.as_ref().is_some_and(|f| (f.width, f.height) != b.app.size) {
                b.frame = None;
            }
            let f = b.frame.get_or_insert_with(|| view::render(&mut b.app));
            if b.app.vi.mode != Mode::Cmd {
                b.crow = usize::from(f.cursor.1);
            }
            (shape, cursor) = (f.shape, (f.cursor.0 + g.tx(), f.cursor.1 + g.y0));
            status = f.rows.last().map_or(String::new(), |r| r.text.clone());
            // The App cuts its status line to its own width; the screen has more (all of it for
            // the command line, what the position leaves for the status), or less (no panel, a position).
            if !cmd {
                b.wide = 0;
            }
            let width = if cmd { w } else { room.max(1) as u16 };
            if width != g.ew && (cmd || status.width() + 2 >= usize::from(g.ew.min(width))) {
                let (top, skip, center, size) = (b.app.top, b.app.skip, b.app.center, b.app.size);
                (b.app.size.0, b.app.vi.off) = (width, b.wide);
                let wide = view::render(&mut b.app);
                b.wide = b.app.vi.off;
                (b.app.top, b.app.skip, b.app.center, b.app.size) = (top, skip, center, size);
                status = wide.rows.last().map_or(String::new(), |r| r.text.clone());
                cursor = if cmd { (wide.cursor.0, h - 1) } else { cursor };
            }
        }
        if focus && self.bufs.get(self.cur).is_none_or(|b| b.app.msg.is_none()) {
            let mode = if self.changes.is_some() { "CHANGES" } else { "TREE" };
            status = layout::stated(&tree, &format!("{slash}  {mode}"), room);
        }
        if let (true, Some(s)) = (focus, &self.panel.search) {
            let n = s.found.hits.len();
            let found = match (s.query.is_empty(), n) {
                (true, _) => String::new(),
                (_, 0) => "  no match".into(),
                _ => format!("  {n} match{}", if n == 1 { "" } else { "es" }),
            };
            let (found, used) = layout::fit(&found, room);
            let head = layout::tail(&format!("/{}", s.query), room - used);
            let cols = layout::fit(&head, usize::MAX).1;
            status = head + &found;
            if s.typing {
                (cursor, shape) = ((cols.min(usize::from(w) - 1) as u16, h - 1), Shape::Bar);
            }
        }
        let si_status = status.clone();
        status = layout::fit(&self.msg.clone().unwrap_or(status), room).0 + &pos;
        let sel = focus.then(|| self.panel.cur - self.panel.top);
        let typing = self.panel.search.as_ref().is_some_and(|s| s.typing);
        if let (Some(i), false) = (sel, typing) {
            let col = at.as_ref().map_or(0, |r| layout::name_col(r.depth, ui));
            let x = g.px + col.min(g.pw.saturating_sub(1));
            (cursor, shape) = ((x, g.y0 + i as u16), Shape::Block);
        }
        let ch = self.changes();
        let looked = self.bufs.get(self.cur).and_then(|b| Some(self.colors(b, b.frame.as_ref()?)));
        let (colored, ask) = looked.unwrap_or_default();
        let win = colored.win;
        if let (Some(req), Some(b)) = (ask, self.bufs.get_mut(self.cur)) {
            b.asked = Some((req.ver, req.from, req.n));
            out.push(SimEffect::Highlight(req));
        }
        let tones = self.tones.filter(|_| self.skin != Skin::Calm);
        let fade = match (g.boxed, focus, sel) {
            (false, ..) => Fade::None,
            (_, true, Some(i)) if self.panel.search.is_some() => Fade::Find(i),
            _ => Fade::None,
        };
        self.refresh_hits();
        let scroll = ui.then(|| self.scrollbar(win)).flatten();
        let home_root = tilde(&self.root, self.home.as_deref());
        let tabs = self.tabs();
        let crumb = if ui { self.crumb() } else { Default::default() };
        let deck = Deck {
            geo: g,
            tones: tones.as_ref(),
            fade,
            gray: self.syn.sel().is_some(),
            app: self.bufs.get(self.cur).map(|b| &b.app),
            win,
            focus,
            chrome: ui.then(|| Chrome { icons: self.icons, root: &home_root, tabs, cur: self.cur }),
            scroll: scroll.unwrap_or_default(),
            crumb,
        };
        let cur = self.bufs.get(self.cur).and_then(|b| Some((b, b.frame.as_ref()?)));
        let editor = match cur {
            Some((b, f)) => Some(Ed {
                digits: digits(&b.app.buf),
                ch,
                crow: b.crow,
                cur_line: ui.then(|| b.app.buf.line_of(b.app.buf.cursor)),
                insert: b.app.vi.mode == Mode::Insert,
                signs: self.signs(b, f, ui),
                extra: &colored.extra,
                ..Ed::new(f, &colored.looks)
            }),
            None => None,
        };
        let (mut rows, mut looks, mut paints) = layout::body(&deck, &marked, sel, editor.as_ref());
        let mut floats = vec![]; // GUI-1: the windows laid over the body
        // PICK-1: the picker is laid over the rows of the body, and has the terminal's cursor.
        if let Some(win) = self.pick_win()
            && let Some((lines, qcols)) = self.live_draw(&win, tones.as_ref())
        {
            let shadow = tones.as_ref().filter(|_| g.boxed);
            pick::lay(&mut rows, &mut looks, &mut paints, w, &win, lines, shadow);
            let at = qcols.min(usize::from(win.prompt.w).saturating_sub(5));
            (cursor, shape) = ((win.prompt.x + 3 + at as u16, win.prompt.y), Shape::Bar);
            floats.push(Layer {
                kind: LayerKind::Float { modal: true },
                rect: pick::Rect { x: win.x, y: win.y, w: win.w, h: win.h },
            });
        }
        // KEY-3: which-key, laid over the text as the picker is, while a prefix has been typed for a pause;
        // with no shadow: beside a window this small it read as a second cursor
        if let Some((title, next)) = self.which_rows()
            && let Some((win, lines)) = keys::which(&title, &next, (w, h), tones.as_ref())
        {
            pick::lay(&mut rows, &mut looks, &mut paints, w, &win, lines, None);
            floats.push(Layer {
                kind: LayerKind::Float { modal: false },
                rect: pick::Rect { x: win.x, y: win.y, w: win.w, h: win.h },
            });
        }
        // UI-10: the bar, unless a message, the command line or a search has the row's text.
        let tree = focus.then(|| (at.as_ref().map(|r| r.path.display().to_string()), slash));
        let drawn =
            self.bar(w, &si_status, tree.as_ref().map(|t| (t.0.as_deref().unwrap_or(&root), t.1)));
        match drawn {
            Some(l) => {
                rows.push(l.row);
                looks.push(l.looks);
                paints.push(l.paints);
            }
            None => {
                rows.push(Row { text: layout::fit(&status, usize::from(w)).0, ..Row::default() });
                looks.push(vec![]);
                // What says that git was not run, for safety, is the alert's (UI-1): hidden, and
                // dangerous if it were not so.
                let alert = tones.as_ref().filter(|_| ui && status.starts_with(GIT_SKIPPED));
                let ink = |t: &Tones| Paint {
                    fg: Some(t.alert_fg()),
                    role: Role::Alert,
                    ..Paint::default()
                };
                paints.push(alert.map(|t| vec![(0..w, ink(t))]).unwrap_or_default());
            }
        }
        let frame = Frame { width: w, height: h, rows, cursor, shape, pal: self.pal };
        let layers = self.layers(ui, sel, floats);
        Screen { frame, looks, paints, syn: self.syn, layers }
    }

    /// GUI-1: what a window draws as surfaces, from the lowest to the highest: the boxes of the panel
    /// and of the text (depth), the text of the open file, its scrollbar and the status line (the
    /// interface), and the windows (`floats`) over them.
    fn layers(&self, ui: bool, sel: Option<usize>, floats: Vec<Layer>) -> Vec<Layer> {
        let (g, (w, h)) = (self.geo(), self.size);
        let rect = |x, y, w, h| pick::Rect { x, y, w, h };
        let mut out = vec![];
        let open = self.panel.open && self.panel.focus;
        if g.boxed {
            let rows = h.saturating_sub(1);
            if g.pw > 0 {
                let kind = LayerKind::Box { focused: open };
                out.push(Layer { kind, rect: rect(0, 0, g.pw + 2, rows) });
            }
            let kind = LayerKind::Box { focused: !open };
            out.push(Layer { kind, rect: rect(g.ex - 1, 0, g.sign + g.ew + 2, rows) });
        }
        if let (true, Some(b)) = (ui, self.bufs.get(self.cur)) {
            let kind = LayerKind::Text { sign: g.sign, gutter: digits(&b.app.buf) as u16 + 2 };
            out.push(Layer { kind, rect: rect(g.ex, g.y0, g.sign + g.ew, g.rows) });
            if g.boxed || g.rule > 0 {
                let x = g.ex + g.sign + g.ew;
                out.push(Layer { kind: LayerKind::Scroll, rect: rect(x, g.y0, 1, g.rows) });
            }
        }
        if let (true, Some(i), true) = (ui, sel, g.pw > 0) {
            out.push(Layer { kind: LayerKind::Cursor, rect: rect(g.px, g.y0 + i as u16, g.pw, 1) });
        }
        if ui {
            out.push(Layer { kind: LayerKind::Status, rect: rect(0, h - 1, w, 1) });
        }
        out.extend(floats);
        out
    }

    /// UI-10: the status line of the interface, or none where the text of the core, a message, the
    /// command line or a search is to be on the row. `si_status` is the core's own line for the current
    /// file (its state words are taken from it, so that none is lost), `tree` the selected path
    /// and its slash while the panel has the focus.
    fn bar(&self, w: u16, si_status: &str, tree: Option<(&str, &str)>) -> Option<Line> {
        let t = self.tones.as_ref().filter(|_| self.ui())?;
        if self.msg.is_some() || self.panel.search.is_some() {
            return None;
        }
        let marks = &self.marks;
        let branch = marks.branch();
        let icons = self.icons;
        let b = self.bufs.get(self.cur);
        if b.is_some_and(|b| b.app.msg.is_some()) {
            return None;
        }
        if let Some((path, slash)) = tree {
            let (n, all) = (self.panel.cur + 1, self.view().visible());
            let (path, pos) = (format!("{path}{slash}"), format!("{n}/{all}"));
            let bar = Bar {
                icons,
                mode: if self.changes.is_some() { "CHANGES" } else { "TREE" },
                block: BarBlock::Accent,
                branch,
                counts: None,
                icon: None,
                path: Some(&path),
                modified: false,
                notes: "",
                slot: None,
                encoding: None,
                eol: None,
                bom: None,
                ftype: None,
                progress: None,
                pos: &pos,
            };
            return bar::render(w, &bar, t);
        }
        let b = b?;
        let a = &b.app;
        let (buf, name) = (&a.buf, a.path.as_deref().unwrap_or("[no name]"));
        let (line, col) = (buf.line_of(buf.cursor), buf.col());
        // The mode's block is neutral in Normal mode and the accent in any other (UI-1).
        let (mode, block) = match a.vi.mode {
            Mode::Insert => ("INSERT".to_string(), BarBlock::Accent),
            Mode::Visual => ("VISUAL".into(), BarBlock::Accent),
            Mode::VLine => ("V-LINE".into(), BarBlock::Accent),
            Mode::Cmd => return None,
            Mode::Normal => ("NORMAL".into(), BarBlock::Gray),
        };
        let mut notes = state_words(si_status, line + 1, col + 1);
        // UI-10: the format is said once, at the right: the middle says only that the file is read-only
        let not_utf8 = buf.readonly.as_deref().is_some_and(|r| r.contains(NOT_UTF8));
        if not_utf8 {
            notes = notes.replace(&format!("read-only: {NOT_UTF8}"), "read-only");
        }
        if let Some(c) = self.searchcount() {
            notes = format!("{notes}  {c}").trim_start().to_string(); // UI-17
        }
        if self.bufs.len() > 1 && self.geo().head == 0 {
            notes = format!("{}/{}  {notes}", self.cur + 1, self.bufs.len()).trim_end().to_string();
        }
        let untracked = self.open_path().is_some_and(|p| marks.git(&p) == Some(Git::Untracked));
        let all = Diff::all_added(buf.lines());
        let diff = if untracked { Some(&all) } else { b.diff.as_ref() };
        let counts = diff.map(|d| (d.added, d.changed, d.deleted));
        let last = buf.lines().saturating_sub(1);
        let progress = match line {
            0 => "Top".to_string(),
            l if l >= last => "Bot".into(),
            l => format!("{}%", l * 100 / buf.lines().max(1)),
        };
        // UI-10: a fact is in one place. The tab bar says the file and that it is modified, the
        // scrollbar where the cursor is; only what is not as usual is said here.
        let g = self.geo();
        let (tabbed, scrolled) = (g.head > 0, g.boxed || g.rule > 0);
        let pos = format!("{}:{}", line + 1, col + 1);
        let file = Path::new(name).file_name().map_or(name.into(), |n| n.to_string_lossy());
        // What is hidden or dangerous is the alert's (magenta): an untrusted root, a server that
        // runs with no sandbox (the server's code says so); the rest of the slot is neutral.
        let slot = match (&b.lsp, self.trust) {
            (Some(l), _) => Some((l.trim().to_string(), b.lsp_alert)),
            (None, Some(true)) => Some(("trusted".to_string(), false)),
            (None, Some(false)) => Some(("untrusted".to_string(), true)),
            _ => None,
        };
        let slot = slot.map(|(s, c)| (if icons { format!("\u{f132} {s}") } else { s }, c));
        let first = head_of(buf, 0).filter(|_| icons::filetype(&file).is_none());
        let bar = Bar {
            icons,
            mode: &mode,
            block,
            branch,
            counts,
            icon: Some(icons::of(&file, false, false)),
            path: (!tabbed).then_some(name),
            modified: buf.dirty(),
            notes: &notes,
            slot: slot.as_ref().map(|(s, c)| (s.as_str(), *c)),
            encoding: not_utf8.then_some("not utf-8"),
            eol: buf.format().1.then_some("CRLF"),
            bom: buf.format().0.then_some("BOM"),
            ftype: first
                .as_deref()
                .and_then(icons::shebang)
                .filter(|_| icons::filetype(&file).is_none()),
            progress: (!scrolled).then_some(progress.as_str()),
            pos: &pos,
        };
        bar::render(w, &bar, t)
    }

    /// A row of the panel with what it shows besides its name (see `layout::Mark`). In the
    /// interface (`ui`) the change color is for the files that changed on disk while sim ran, git's
    /// own changes show by their letter (UI-3), and there is no `~`.
    /// DIFF-2: asks for the numbers of the changes view, when it is on.
    fn ask_changes(&self, out: &mut Vec<SimEffect>) {
        out.extend(self.changes.as_ref().map(|_| SimEffect::Changes(self.marks.untracked())));
    }

    /// DIFF-3: HEAD's version of a deleted file came: a read-only buffer named `HEAD:path`, and the
    /// message line says so.
    fn shown(&mut self, path: String, r: Result<Vec<u8>, String>, out: &mut Vec<SimEffect>) {
        let bytes = match r {
            Ok(bytes) => bytes,
            Err(e) => return self.msg = Some(e),
        };
        let (name, id) = (format!("HEAD:{path}"), self.next);
        self.next += 1;
        let (path, key) = (name.clone(), name.clone());
        self.opened(Opened { id, path, key, file: Ok(Some(bytes)), found: None, place: None }, out);
        if let Some(b) = self.bufs.iter_mut().find(|b| b.key == name) {
            b.app.buf.readonly.get_or_insert_with(|| "read-only".into());
        }
        self.msg = Some(format!("{name} (read-only)"));
    }

    fn mark(
        &self,
        r: TreeRow,
        likely: &[(String, f64)],
        open: Option<&Path>,
        ui: bool,
    ) -> (TreeRow, Mark) {
        let (p, name) = (r.path.as_path(), r.path.to_string_lossy());
        let hit = self.panel.search.as_ref().filter(|s| !s.query.is_empty()).map(|s| {
            r.is_dir && s.found.dirs.contains(name.as_ref())
                || !r.is_dir
                    && (self.files.binary_search_by(|f| f.as_str().cmp(&name)).ok())
                        .is_some_and(|i| s.found.hits.binary_search_by_key(&i, |h| h.index).is_ok())
        });
        let file = !r.is_dir;
        let m = &self.marks;
        // DIFF-1, DIFF-2, DIFF-4: in the changes view a row has its numbers, its letter (a rename's is
        // `R`, though git says `RM` of one that is changed too) and a rename its old name
        let view = self.changes.as_ref();
        let old = view.and_then(|_| m.renamed_from(p));
        let shown = old.map(|o| moved(o, &r));
        let mark = Mark {
            change: file && if ui { m.on_disk(p) } else { m.changed(p) && view.is_none() },
            below: r.is_dir && !ui && view.is_none() && m.changed_below(p),
            likely: file && likely.iter().any(|l| Path::new(&l.0) == p),
            hit,
            git: old.map(|_| Git::Renamed).or(m.git(p)).filter(|_| (ui || view.is_some()) && file),
            stat: view.and_then(|c| c.get(p)),
            changes: view.is_some(),
            dot: m.below(p).filter(|_| ui && r.is_dir),
            ignored: ui && m.ignored(p),
            open: ui && file && open == Some(p),
        };
        let r = match shown {
            Some(name) => TreeRow { name, ..r },
            None => r,
        };
        (r, mark)
    }
}

/// DIFF-3: whether a buffer is HEAD's version of a file: its key is `HEAD:path`, where a file's is an
/// absolute path.
fn is_head(b: &Buf) -> bool {
    b.key.starts_with("HEAD:")
}

/// DIFF-4: how a renamed file's row is written: `old → new`, the old name as a path from the new
/// name's folder (`../a.rs → b.rs`).
fn moved(old: &Path, r: &TreeRow) -> String {
    let dir: Vec<_> = r.path.parent().map(|d| d.components().collect()).unwrap_or_default();
    let from: Vec<_> = old.components().collect();
    let same = dir.iter().zip(&from).take_while(|(a, b)| a == b).count();
    let same = same.min(from.len().saturating_sub(1));
    let rest: PathBuf = from[same..].iter().collect();
    format!("{}{} → {}", "../".repeat(dir.len() - same), rest.display(), r.name)
}

/// UI-13: the chars a yank has taken, in the buffer as it is after it: the App says so by its
/// message (`3 lines yanked`) and by putting the text on the clipboard, and leaves the cursor at the
/// start of what it took (at the start of the cursor's line, for whole lines).
fn yanked(app: &App, fx: &[Effect]) -> Option<std::ops::Range<usize>> {
    let text = fx.iter().find_map(|e| if let Effect::Clipboard(t) = e { Some(t) } else { None })?;
    let msg = app.msg.as_deref().filter(|m| m.ends_with("yanked"))?;
    let b = &app.buf;
    let from = if msg.contains("line") { b.start(b.line_of(b.cursor)) } else { b.cursor };
    Some(from..(from + text.chars().count()).min(b.text.len_chars()))
}

/// UI-11: how many lines a file may have for the scrollbar to mark the matches of a search.
const HIT_LINES: usize = 5_000;

/// Whether a search pattern is a plain word: no regex syntax and no escape, so that its matches are
/// the lines that contain it.
fn plain(pat: &str) -> bool {
    !pat.is_empty()
        && !pat.contains(['\\', '.', '+', '*', '?', '(', ')', '|', '[', ']', '{', '}', '^', '$'])
}

/// UI-14: whether the matches of the search are lit after a key: a search lights them (the pattern
/// changed, or `/` `?` was submitted with none typed, or `n` `N` `*` `#` in Normal mode with
/// nothing pending), Esc in Normal mode with nothing pending puts them out. `key` is the key, the
/// mode before it, whether the App was idle and whether its command line was a search.
fn lit_after(
    lit: bool,
    (k, mode, idle, slash): (Key, Mode, bool, bool),
    was: &Option<(String, bool, bool)>,
    now: &Option<(String, bool, bool)>,
) -> bool {
    let searched = now != was
        || (mode == Mode::Cmd && k == Key::Enter && slash && now.is_some())
        || (idle && matches!(k, Key::Char('n' | 'N' | '*' | '#')) && now.is_some());
    searched || (lit && !(k == Key::Esc && mode == Mode::Normal && idle))
}

/// UI-14: whether a command line is `:noh`, `:nohl`, ... `:nohlsearch`.
fn noh(line: &str) -> bool {
    line.strip_prefix(':')
        .map(str::trim)
        .is_some_and(|n| n.len() >= 3 && "nohlsearch".starts_with(n))
}

/// UI-6: the indentation of a buffer, from its first lines.
fn indent_of(buf: &Buffer) -> Indent {
    let lines: Vec<String> = buf.text.lines().take(indent::LOOK).map(|l| l.to_string()).collect();
    indent::detect(lines.iter().map(String::as_str))
}

/// The leading blanks of line `l` of a buffer, read off the rope with no copy.
fn lead_of(buf: &Buffer, l: usize) -> Lead {
    let line = buf.text.line(l);
    let (mut cols, mut tab, mut n) = (0u16, false, 0);
    for c in line.chars().take_while(|c| *c != '\n') {
        match c {
            ' ' => cols = cols.saturating_add(1),
            '\t' => (cols, tab) = (cols.saturating_add(4 - cols % 4), true),
            _ => return Lead { cols, tab, blank: false },
        }
        n += 1;
    }
    Lead { cols, tab, blank: n == buf.line_len(l) }
}

/// UI-6: what is known of the cursor's scope for the frame being made: the lines around the cursor
/// whose indent is at least its line's, and the column of the innermost guide of the cursor's line.
struct Guides<'a> {
    /// UI-13: the chars of the buffer that a yank has just taken.
    flash: Option<std::ops::Range<usize>>,
    /// UI-14: the matcher of the search being lit, and the line and the char the cursor is at.
    finder: Option<&'a Matcher>,
    at: (usize, usize),
    width: u16,
    /// The width of the pane: no guide is made further right.
    limit: u16,
    scope: Option<(std::ops::RangeInclusive<usize>, u16)>,
}

/// How many lines are looked at, up and down, for a blank line's neighbors and for a scope.
const NEAR: usize = 50;
const SCOPE: usize = 300;

fn guides_of(b: &Buf, flash: Option<std::ops::Range<usize>>) -> Guides<'_> {
    let (buf, width, limit) = (&b.app.buf, b.indent.width.max(1), b.app.size.0);
    let cur = buf.line_of(buf.cursor);
    let finder = b.finder.as_ref().and_then(|f| f.m.as_ref());
    let at = (cur, buf.cursor - buf.start(cur));
    let nonblank = |from: usize, up: bool| -> Option<Lead> {
        let ls: Box<dyn Iterator<Item = usize>> =
            if up { Box::new((0..from).rev()) } else { Box::new(from + 1..buf.lines()) };
        ls.take(NEAR).map(|l| lead_of(buf, l)).find(|d| !d.blank)
    };
    let own = lead_of(buf, cur);
    let lead = if own.blank { nonblank(cur, true).unwrap_or(own) } else { own };
    if lead.cols == 0 || lead.tab {
        return Guides { flash, finder, at, width, limit, scope: None };
    }
    let col = (lead.cols - 1) / width * width;
    let inside = |l: usize| {
        let d = lead_of(buf, l);
        d.blank || (!d.tab && d.cols >= lead.cols)
    };
    let (mut lo, mut hi) = (cur, cur);
    while lo > 0 && cur - lo < SCOPE && inside(lo - 1) {
        lo -= 1;
    }
    while hi + 1 < buf.lines() && hi - cur < SCOPE && inside(hi + 1) {
        hi += 1;
    }
    while lo < cur && lead_of(buf, lo).blank {
        lo += 1;
    }
    while hi > cur && lead_of(buf, hi).blank {
        hi -= 1;
    }
    Guides { flash, finder, at, width, limit, scope: Some((lo..=hi, col)) }
}

impl Guides<'_> {
    /// UI-6: the guides of line `l`: at 0, w, 2w, ... in front of where its text starts; none for
    /// a line indented with a tab (the tab is drawn as the level); for a blank line those of the
    /// fewer of the lines around it, from the first level on (the end-of-line mark is at 0).
    fn of(&self, buf: &Buffer, l: usize) -> LineUi {
        let d = lead_of(buf, l);
        let guides = if d.tab {
            vec![]
        } else if d.blank {
            let up = (0..l).rev().take(NEAR).map(|k| lead_of(buf, k)).find(|d| !d.blank);
            let down = (l + 1..buf.lines()).take(NEAR).map(|k| lead_of(buf, k)).find(|d| !d.blank);
            let cols = match (up, down) {
                (Some(a), Some(b)) if !a.tab && !b.tab => a.cols.min(b.cols),
                _ => 0,
            };
            indent::guides(cols, self.width, 0, self.limit)
        } else {
            indent::guides(d.cols, self.width, 0, self.limit)
        };
        let scope = self
            .scope
            .as_ref()
            .filter(|(run, c)| run.contains(&l) && guides.contains(c))
            .map(|s| s.1);
        let (from, len) = (buf.start(l), buf.line_len(l));
        let flash = self.flash.as_ref().and_then(|r| {
            let (lo, hi) = (r.start.max(from), r.end.min(from + len));
            (lo < hi).then(|| lo - from..hi - from)
        });
        // UI-14: the matches of the search in this line, and the one the cursor is on.
        let hits = self.finder.and_then(|m| Some(m.ranges(&text_of(buf, l)?))).unwrap_or_default();
        let now =
            (self.at.0 == l).then(|| hits.iter().position(|r| r.contains(&self.at.1))).flatten();
        LineUi { guides, scope, flash, hits, now }
    }
}

/// UI-10: the state words of the core's status line, which follow the position and the mode
/// (`  3:7  NORMAL  2 changes on disk · deleted on disk`): without `modified`, which the tab says by its
/// dot, and `CRLF` and `BOM`, which the bar says at its right. None is built again here, so none can
/// be lost (P1).
fn state_words(status: &str, line: usize, col: usize) -> String {
    let at = format!("  {line}:{col}  ");
    let Some(i) = status.find(&at) else { return String::new() };
    let rest = &status[i + at.len()..];
    let notes = rest.split_once("  ").map_or("", |(_, n)| n.trim());
    let said_elsewhere = |n: &str| matches!(n, "modified" | "CRLF" | "BOM");
    notes.split(" · ").filter(|n| !said_elsewhere(n)).collect::<Vec<_>>().join(" · ")
}

/// UI-2: `path` as the header shows it: `~/...` below `$HOME`, `~` for `$HOME`, else as it is.
pub fn tilde(path: &Path, home: Option<&Path>) -> String {
    match home.and_then(|h| path.strip_prefix(h).ok()) {
        Some(rest) if rest.as_os_str().is_empty() => "~".into(),
        Some(rest) => format!("~/{}", rest.display()),
        None => path.display().to_string(),
    }
}
