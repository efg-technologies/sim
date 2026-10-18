//! VI-1, VI-2, VI-3, VI-10, VI-12, VI-13, VI-15, VI-17
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
/// object), `arg` the char after `f t r g z Z i a`. `k == '\0'` with an operator means "the
/// selection"; `k == op` means "whole lines". `n` is 0 when no count was typed. `reg` is the
/// register named with `"` (VI-15).
#[rustfmt::skip]
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Cmd { pub n: usize, pub op: Option<char>, pub k: char, pub arg: Option<char>, pub reg: Option<char> }

// Keys that have no char of their own are given private ones.
pub const HOME: char = '\u{e000}';
pub const PGDN: char = '\u{e001}';
pub const PGUP: char = '\u{e002}';
pub const HALF_DN: char = '\x04';
pub const HALF_UP: char = '\x15';
// `VERTICAL` and `MOTIONS` spell the private chars above (and Ctrl-d, Ctrl-u) out.
/// The motions that keep the wanted column.
pub const VERTICAL: &str = "jk\x04\x15\u{e001}\u{e002}";
/// The keys that only move the cursor (and, after an operator, make a range).
pub const MOTIONS: &str = "hjklwbeWBE0^$GgfFtT;,%nN*#'`\u{e000}\u{e001}\u{e002}\x04\x15";

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
}

impl Parser {
    /// Normal mode with nothing half typed.
    pub fn idle(&self) -> bool {
        self.mode == Mode::Normal
            && self.n + self.n0 == 0
            && self.op.or(self.pre).or(self.reg).is_none()
    }

    /// A prefix key (the `]` of `]c`) waits for the key that completes it.
    pub fn waiting(&self) -> bool {
        self.pre.is_some()
    }

    /// The keys of a command that is typed and not complete: its operator, then its prefix (`d`, `g`, `di`).
    pub fn pending(&self) -> String {
        self.op.into_iter().chain(self.pre).collect()
    }

    /// The next key is text, not a command: typed in Insert mode or on the command line, or the char
    /// after `f F t T r` (VI-10).
    pub fn text(&self) -> bool {
        matches!(self.mode, Mode::Insert | Mode::Cmd)
            || matches!(self.pre, Some('f' | 'F' | 't' | 'T' | 'r'))
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
        (self.n, self.n0, self.op, self.pre, self.reg) = (0, 0, None, None, None);
        None
    }

    fn emit(&mut self, op: Option<char>, k: char, arg: Option<char>) -> Option<Cmd> {
        let n =
            if self.n + self.n0 == 0 { 0 } else { self.n0.max(1).saturating_mul(self.n.max(1)) };
        let reg = self.reg;
        self.reset();
        Some(Cmd { n, op, k, arg, reg })
    }

    /// Feeds a key in Normal or Visual mode.
    pub fn key(&mut self, k: Key) -> Option<Cmd> {
        let (c, vis) = (ch(k), self.mode.visual());
        if let Some(p) = self.pre.take() {
            #[rustfmt::skip]
            return match (k, p) {
                (Key::Char(r), '"') => {
                    self.reg = Some(r);
                    None
                }
                (Key::Char('g'), 'g') | (Key::Char('z'), 'z') | (Key::Char('Z' | 'Q'), 'Z')
                | (Key::Char('c'), '[' | ']') | (Key::Char(_), 'f' | 'F' | 't' | 'T' | 'r' | 'i' | 'a' | 'm' | '\'' | '`') => self.emit(self.op, p, Some(c)),
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
        if "dcy<>!".contains(c) {
            return match self.op {
                _ if vis => self.emit(Some(c), '\0', None),
                None => {
                    (self.op, self.n0, self.n) = (Some(c), self.n, 0);
                    None
                }
                Some(o) if o == c => self.emit(Some(c), c, None),
                Some(_) => self.reset(),
            };
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
