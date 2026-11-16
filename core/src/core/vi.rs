//! VI-1, VI-2, VI-3, VI-10, VI-12, VI-13, VI-15, VI-17, VI-18, VI-19, VI-20
//! Key sequence to Command: counts, operators and prefixes in Normal and Visual mode, and the
//! editing of the command line.
use super::app::Key;
use std::mem::take;

#[rustfmt::skip]
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Mode {
    #[default]
    Normal, Insert,
    /// The command line of `:`, `/` and `?`.
    Cmd,
    Visual, VLine,
}

impl Mode {
    pub fn visual(self) -> bool {
        matches!(self, Mode::Visual | Mode::VLine)
    }
}

/// A command as keys: `k` is the final key (a motion, a command, or `i` / `a` of a text
/// object), `arg` the char after `f t r g z Z i a` (and the surrounding `gsd` and `gsr` take). `k == '\0'`
/// with an operator means "the selection"; `k == op` means "whole lines". `n` is 0 when no count was
/// typed. `reg` is the register named with `"` (VI-15). `to` is the surrounding `gsa` puts, or the one
/// `gsr` puts in its place (VI-20).
#[rustfmt::skip]
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Cmd { pub n: usize, pub op: Option<char>, pub k: char, pub arg: Option<char>, pub reg: Option<char>, pub to: Option<char> }

// Keys that have no char of their own are given private ones.
pub const HOME: char = '\u{e000}';
pub const PGDN: char = '\u{e001}';
pub const PGUP: char = '\u{e002}';
pub const HALF_DN: char = '\x04';
pub const HALF_UP: char = '\x15';
/// `gc` (VI-18), the operator of two keys, and `gv` (VI-19). (`gu`, `gU` and `g~` are `u`, `U` and `~`.)
pub const GC: char = '\u{e003}';
pub const GV: char = '\u{e004}';
/// The surroundings (VI-20): `gs` the prefix, `gsa` the operator, `gsd` and `gsr` the commands.
pub const GS: char = '\u{e005}';
pub const GSA: char = '\u{e006}';
pub const GSD: char = '\u{e007}';
pub const GSR: char = '\u{e008}';
// `VERTICAL` and `MOTIONS` spell the private chars above (and Ctrl-d, Ctrl-u) out.
/// The motions that keep the wanted column.
pub const VERTICAL: &str = "jk\x04\x15\u{e001}\u{e002}";
/// The keys that only move the cursor (and, after an operator, make a range).
pub const MOTIONS: &str = "hjklwbeWBE0^$GgfFtT;,%nN*#'`{}\u{e000}\u{e001}\u{e002}\x04\x15";

/// The char a key stands for in Normal and Visual mode.
#[rustfmt::skip]
pub fn ch(k: Key) -> char {
    match k {
        Key::Char(c) => c,
        Key::Ctrl('d') => HALF_DN, Key::Ctrl('u') => HALF_UP, Key::Ctrl('r') => '\x12', Key::Ctrl('c') => '\x03',
        Key::Esc => '\x1b', Key::Left => 'h', Key::Right => 'l', Key::Up => 'k', Key::Down => 'j',
        Key::Home => HOME, Key::End => '$', Key::PageDown => PGDN, Key::PageUp => PGUP, Key::Delete => 'x',
        _ => '\0',
    }
}

#[derive(Default)]
pub struct Parser {
    pub mode: Mode,
    /// The command line being typed, with its `:`, `/` or `?`, and the cursor in it (chars).
    pub line: String,
    pub pos: usize,
    /// The first char of the line on the screen, once it is longer than the screen (VI-12).
    pub off: usize,
    n: usize,
    /// The count typed before the pending operator.
    n0: usize,
    op: Option<char>,
    /// The register named with `"`.
    reg: Option<char>,
    /// A prefix key waiting for its argument.
    pre: Option<char>,
    /// A command that waits for the char of the surrounding it puts (`gsa`, `gsr`: VI-20).
    wait: Option<Cmd>,
}

impl Parser {
    /// Normal mode with nothing half typed.
    pub fn idle(&self) -> bool {
        self.mode == Mode::Normal
            && self.n + self.n0 == 0
            && self.op.or(self.pre).or(self.reg).is_none()
            && self.wait.is_none()
    }

    /// A prefix key (the `]` of `]c`) waits for the key that completes it.
    pub fn waiting(&self) -> bool {
        self.pre.is_some()
    }

    /// The keys of a command that is typed and not complete: its operator, then its prefix (`d`, `g`, `di`).
    pub fn pending(&self) -> String {
        let keys = |c: char| match c {
            GC => "gc".into(),
            GS => "gs".into(),
            GSA => "gsa".into(),
            'u' | 'U' | '~' => format!("g{c}"), // (an operator, so typed after `g`)
            c => c.to_string(),
        };
        self.op.into_iter().chain(self.pre).map(keys).collect()
    }

    /// The next key is text, not a command: typed in Insert mode or on the command line, or the char
    /// after `f F t T r` (VI-10).
    pub fn text(&self) -> bool {
        matches!(self.mode, Mode::Insert | Mode::Cmd)
            || matches!(self.pre, Some('f' | 'F' | 't' | 'T' | 'r' | GSD | GSR))
            || self.wait.is_some()
    }

    pub fn open(&mut self, init: &str) {
        (self.mode, self.line, self.pos, self.off) =
            (Mode::Cmd, init.into(), init.chars().count(), 0);
    }

    fn at(&self, pos: usize) -> usize {
        self.line.char_indices().nth(pos).map_or(self.line.len(), |x| x.0)
    }

    pub fn insert_str(&mut self, s: &str) {
        self.line.insert_str(self.at(self.pos), s);
        self.pos += s.chars().count();
    }

    /// Forgets a half-typed command (a count, an operator, a register, a prefix).
    pub fn reset(&mut self) -> Option<Cmd> {
        (self.n, self.n0, self.op, self.pre, self.reg, self.wait) = (0, 0, None, None, None, None);
        None
    }

    fn emit(&mut self, op: Option<char>, k: char, arg: Option<char>) -> Option<Cmd> {
        let n =
            if self.n + self.n0 == 0 { 0 } else { self.n0.max(1).saturating_mul(self.n.max(1)) };
        let reg = self.reg;
        self.reset();
        let c = Cmd { n, op, k, arg, reg, to: None };
        // (`gsa{motion}` and `gsr{char}` wait for the char of the surrounding to put)
        if op == Some(GSA) || k == GSR {
            self.wait = Some(c);
            return None;
        }
        Some(c)
    }

    /// Feeds a key in Normal or Visual mode.
    pub fn key(&mut self, k: Key) -> Option<Cmd> {
        let (c, vis) = (ch(k), self.mode.visual());
        if let Some(mut w) = self.wait.take() {
            let Key::Char(c) = k else { return None }; // (any other key gives it up)
            w.to = Some(c);
            return Some(w);
        }
        if let Some(p) = self.pre.take() {
            #[rustfmt::skip]
            return match (k, p) {
                (Key::Char(r), '"') => {
                    self.reg = Some(r);
                    None
                }
                (Key::Char('g'), 'g') | (Key::Char('z'), 'z') | (Key::Char('Z' | 'Q'), 'Z')
                | (Key::Char('c'), '[' | ']') | (Key::Char(_), 'f' | 'F' | 't' | 'T' | 'r' | 'i' | 'a' | 'm' | '\'' | '`' | GSD | GSR) => self.emit(self.op, p, Some(c)),
                (Key::Char(o @ ('c' | 'u' | 'U' | '~')), 'g') => self.operator(if o == 'c' { GC } else { o }, vis),
                (Key::Char('v'), 'g') if self.op.is_none() && !vis => self.emit(None, GV, None),
                (Key::Char('s'), 'g') if self.op.is_none() => {
                    self.pre = Some(GS);
                    None
                }
                (Key::Char('a'), GS) => self.operator(GSA, vis),
                (Key::Char(k @ ('d' | 'r')), GS) if !vis => {
                    self.pre = Some(if k == 'd' { GSD } else { GSR });
                    None
                }
                _ => self.reset(),
            };
        }
        if "\x1b\x03".contains(c) {
            let busy = self.n + self.n0 > 0 || self.op.or(self.reg).is_some();
            return if busy { self.reset() } else { self.emit(None, c, None) };
        }
        if c.is_ascii_digit() && (c != '0' || self.n > 0) {
            self.n = self.n.saturating_mul(10).saturating_add(c as usize - '0' as usize);
            return None;
        }
        if "dcy<>!".contains(c) || self.op == Some(c) || (vis && "uU~".contains(c)) {
            // (`gcc` is `gc` doubled, `guu` is `gu`; in Visual mode `u` and `U` change the case)
            return self.operator(if c == 'c' && self.op == Some(GC) { GC } else { c }, vis);
        }
        if self.op.is_some() && !(MOTIONS.contains(c) || "ia".contains(c)) {
            return self.reset();
        }
        match c {
            'f' | 'F' | 't' | 'T' | 'g' | 'r' | 'z' | 'Z' | '[' | ']' | '"' | 'm' | '\'' | '`' => {
                self.pre = Some(c)
            }
            'i' | 'a' if vis || self.op.is_some() => self.pre = Some(c),
            'D' | 'C' | 'Y' if !vis => return self.emit(Some(c.to_ascii_lowercase()), '$', None),
            'x' if !vis => return self.emit(Some('d'), 'l', None),
            'x' => return self.emit(Some('d'), '\0', None),
            _ => return self.emit(self.op, c, None),
        }
        None
    }

    /// An operator's key: it waits for a motion, takes whole lines when it is doubled, and takes the
    /// selection in Visual mode.
    fn operator(&mut self, o: char, vis: bool) -> Option<Cmd> {
        match self.op {
            _ if vis => self.emit(Some(o), '\0', None),
            None => {
                (self.op, self.n0, self.n) = (Some(o), self.n, 0);
                None
            }
            Some(p) if p == o => self.emit(Some(o), o, None),
            Some(_) => self.reset(),
        }
    }

    /// Feeds a key on the command line; returns the line, with its prefix, when it is submitted.
    pub fn cmdline(&mut self, k: Key) -> Option<String> {
        let n = self.line.chars().count();
        match k {
            Key::Esc | Key::Ctrl('c') => self.mode = Mode::Normal,
            Key::Enter => {
                self.mode = Mode::Normal;
                return Some(take(&mut self.line));
            }
            Key::Backspace if n == 1 => self.mode = Mode::Normal,
            Key::Backspace if self.pos > 1 => {
                self.pos -= 1;
                self.line.remove(self.at(self.pos));
            }
            Key::Delete if self.pos < n => drop(self.line.remove(self.at(self.pos))),
            Key::Left => self.pos = (self.pos - 1).max(1),
            Key::Right => self.pos = (self.pos + 1).min(n),
            Key::Home => self.pos = 1,
            Key::End => self.pos = n,
            Key::Char(c) => self.insert_str(c.encode_utf8(&mut [0; 4])),
            Key::Tab => self.insert_str(" "),
            _ => {}
        }
        None
    }
}
