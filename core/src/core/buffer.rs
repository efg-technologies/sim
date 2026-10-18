//! SAFE-3, SAFE-4, DISK-6, VI-17
//! The text (ropey) and undo. Every text change goes through `Buffer::replace`, which also keeps
//! the marks of changes from outside where they belong.
use super::mark::follow;
use ropey::Rope;
use std::mem::replace as swap;
use std::ops::Range;
use std::sync::Arc;

/// A list of more than `BIG_MARKS` marks is kept by the last `KEEP_MARKS` undo steps only. Every edit
/// that moves marks copies the list, and the copies of 100,000 marks (3 MB each) stay in the cache
/// while there are few: 4 steps copy in 70 µs, 16 in 160 µs (measured). 1,024 marks are 32 KB a
/// step, so a short list is kept by all of them.
const BIG_MARKS: usize = 1024;
const KEEP_MARKS: usize = 4;

/// A place in the text: line and column, 0-based.
pub type Place = (usize, usize);

/// The marks of VI-17, in `Buffer::vmark`: `a` to `z`, then the place before the last jump (`'`), then
/// the one it replaced while the jump's command runs (`checkpcmark`, mark.rs).
pub const PREV: usize = 26;
pub const SAVED: usize = 27;
pub type Vmarks = [Option<Place>; 28];

/// Hash of a file that does not exist.
pub const ABSENT: u64 = 0;

/// FNV-1a 64.
pub fn hash(b: &[u8]) -> u64 {
    b.iter().fold(0xcbf2_9ce4_8422_2325, |h, &x| (h ^ u64::from(x)).wrapping_mul(0x0100_0000_01b3))
}

/// BOM, CRLF and whether the file ends with a newline, as `encode` writes them.
pub type Format = (bool, bool, Option<bool>);

/// A line of a change from the disk that nobody has looked at yet (DISK-6): `+` added, `~` changed
/// (or, where the disk's text is not in the buffer, differs: a conflict), `-` something was deleted
/// just before it. `words` are the char columns in the line to paint, `None` for the whole line.
/// Marks are copied for every undo step, so they hold no allocation of their own: a copy costs a
/// count, not a clone of the words (PERF-2).
#[rustfmt::skip]
#[derive(Clone, Debug, PartialEq)]
pub struct Mark { pub line: usize, pub kind: char, pub words: Option<Arc<[Range<usize>]>> }

#[rustfmt::skip]
struct Snap { text: Rope, cursor: usize, version: u64, nl: Option<bool>, marks: Arc<Vec<Mark>>, vmark: Vmarks }

#[derive(Default)]
pub struct Buffer {
    pub text: Rope,
    /// The text as last read from or written to disk.
    pub base: Rope,
    /// Char index into `text`.
    pub cursor: usize,
    /// Column wanted by `j` / `k`.
    pub want: Option<usize>,
    /// Why the buffer cannot be edited or written (SAFE-4).
    pub readonly: Option<String>,
    /// Changes taken in from the disk, one per line, in line order. Part of undo, like the text.
    pub marks: Arc<Vec<Mark>>,
    /// The marks of VI-17 (`m` `'` `` ` ``): they move with the edits, and undo brings back the ones it
    /// took away.
    pub vmark: Vmarks,
    bom: bool,
    crlf: bool,
    /// Whether the file ends with a newline; `None` (empty or new file): write one iff there is text.
    /// `eol` is the file's own, which an emptied buffer gets back with text again.
    nl: Option<bool>,
    eol: Option<bool>,
    version: u64,
    /// The version that is the file's text. `u64::MAX`: none is (the file is gone, DISK-1).
    pub(super) saved: u64,
    next: u64,
    undo: Vec<Snap>,
    redo: Vec<Snap>,
    pending: Option<Snap>,
}

impl Buffer {
    pub fn decode(bytes: &[u8]) -> Buffer {
        let bom = bytes.starts_with(b"\xEF\xBB\xBF");
        let body = &bytes[if bom { 3 } else { 0 }..];
        let nul = bytes[..bytes.len().min(8192)].contains(&0);
        let readonly = nul.then_some("read-only: binary file (NUL in the first 8 KB)");
        let readonly =
            readonly.or(std::str::from_utf8(body).is_err().then_some("read-only: not valid UTF-8"));
        let s = String::from_utf8_lossy(body);
        let crlf = body.contains(&b'\n')
            && body.split(|&b| b == b'\n').rev().skip(1).all(|l| l.ends_with(b"\r"));
        let s = if crlf { s.replace("\r\n", "\n").into() } else { s };
        let nl = (!s.is_empty()).then(|| s.ends_with('\n'));
        let text = Rope::from_str(s.strip_suffix('\n').unwrap_or(&s));
        let readonly = readonly.map(String::from);
        Buffer { base: text.clone(), text, readonly, bom, crlf, nl, eol: nl, ..Default::default() }
    }

    /// The exact inverse of `decode` for a buffer that was not edited.
    pub fn encode(&self) -> Vec<u8> {
        let fin = if self.nl.unwrap_or(self.text.len_chars() > 0) { "\n" } else { "" };
        let body: String = self.text.chunks().chain([fin]).collect();
        let body = if self.crlf { body.replace('\n', "\r\n") } else { body };
        [if self.bom { &b"\xEF\xBB\xBF"[..] } else { b"" }, body.as_bytes()].concat()
    }

    pub fn dirty(&self) -> bool {
        self.version != self.saved
    }

    pub fn version(&self) -> u64 {
        self.version
    }

    pub fn format(&self) -> Format {
        (self.bom, self.crlf, self.nl)
    }

    /// The file now holds what was written for `version`, in the format it was written in: a merge
    /// meanwhile may have taken in the format of an older file. A buffer that was emptied since has no
    /// final newline of its own (SAFE-3).
    pub fn mark_saved(&mut self, version: u64, text: Rope, (bom, crlf, nl): Format) {
        let emptied = self.nl.is_none() && self.text.len_chars() == 0;
        (self.saved, self.base, self.bom, self.crlf) = (version, text, bom, crlf);
        (self.eol, self.nl) = (nl.or(self.eol), if emptied { None } else { nl });
    }

    pub fn lines(&self) -> usize {
        self.text.len_lines()
    }

    pub fn line_of(&self, idx: usize) -> usize {
        self.text.char_to_line(idx)
    }

    pub fn start(&self, line: usize) -> usize {
        self.text.line_to_char(line)
    }

    /// Chars in the line, without its line break.
    pub fn line_len(&self, line: usize) -> usize {
        let l = self.text.line(line);
        l.len_chars() - usize::from(l.len_chars() > 0 && l.char(l.len_chars() - 1) == '\n')
    }

    /// Index of the line break after the line (or the end of the text).
    pub fn end(&self, line: usize) -> usize {
        self.start(line) + self.line_len(line)
    }

    /// First non-blank of a line (its end, if there is none).
    pub fn nonblank(&self, line: usize) -> usize {
        self.start(line)
            + self.text.line(line).chars().take_while(|c| matches!(c, ' ' | '\t')).count()
    }

    /// The char at `i`, or NUL past the end.
    pub fn ch(&self, i: usize) -> char {
        self.text.get_char(i).unwrap_or('\0')
    }

    pub fn col(&self) -> usize {
        self.cursor - self.start(self.line_of(self.cursor))
    }

    /// Starts a new version: the pending undo unit is recorded, and redo is gone.
    fn record(&mut self) {
        if let Some(p) = self.pending.take() {
            self.undo.push(p);
        }
        // A long list of marks is copied by every edit that moves them, so only the last few undo
        // steps keep theirs: the memory stays bounded, and the copy reuses what was let go (PERF-2).
        let old = self.undo.len().checked_sub(KEEP_MARKS + 1).and_then(|i| self.undo.get_mut(i));
        if let Some(s) = old.filter(|s| s.marks.len() > BIG_MARKS) {
            s.marks = Arc::default();
        }
        self.redo.clear();
        self.next += 1;
        self.version = self.next;
    }

    /// The only way to change the text by hand. A no-op on a read-only buffer.
    pub fn replace(&mut self, range: Range<usize>, with: &str) {
        let n = self.text.len_chars();
        let range = range.start.min(n)..range.end.min(n);
        if self.readonly.is_some() || (range.is_empty() && with.is_empty()) {
            return;
        }
        self.record();
        self.adjust(&range, with);
        self.shift(&range, with);
        self.text.remove(range.clone());
        self.text.insert(range.start, with);
        // A file with all its text deleted is an empty file; with text again it ends as it did (SAFE-3).
        self.nl = if self.text.len_chars() == 0 { None } else { self.nl.or(self.eol) };
    }

    /// Moves marks to where an edit of `r` into `with` leaves them. A mark on a line the edit
    /// changes is dropped (the cursor is there anyway).
    fn shift(&mut self, r: &Range<usize>, with: &str) {
        let (l1, l2) = (self.line_of(r.start), self.line_of(r.end));
        // The first (last) line stays as it was if the edit starts at its end (ends at its start)
        // and adds nothing to it.
        let first = r.start == self.end(l1)
            && (with.starts_with('\n') || (with.is_empty() && r.end == self.end(l2)));
        let last = r.end == self.start(l2)
            && (with.ends_with('\n') || (with.is_empty() && r.start == self.start(l1)));
        let add = with.matches('\n').count();
        // Joining two empty lines reads both ways (the first stays, the second stays) but one line
        // is left: the first it is.
        let last = last && !(first && add == 0 && l1 != l2);
        // The old lines that changed, the new lines they became, and the change in line count.
        let (old, d) = (
            l1 + usize::from(first)..l2 + 1 - usize::from(last),
            add as isize - (l2 - l1) as isize,
        );
        let go = |l: usize| (l as isize + d) as usize;
        let (i, j) = (
            self.marks.partition_point(|m| m.line < old.start),
            self.marks.partition_point(|m| m.line < old.end),
        );
        if i < j || d != 0 {
            // The list is copied only if an undo step holds it: a command of many edits (`:s`) then
            // changes the one it made, and an edit near the end of it moves nothing (PERF-2).
            let marks = Arc::make_mut(&mut self.marks);
            marks.drain(i..j.max(i));
            marks[i..].iter_mut().filter(|_| d != 0).for_each(|m| m.line = go(m.line));
        }
    }

    /// DISK-6: the cursor is on this line now, so nobody can miss its mark any more.
    pub fn see(&mut self) {
        let l = self.line_of(self.cursor);
        if let Ok(i) = self.marks.binary_search_by_key(&l, |m| m.line) {
            Arc::make_mut(&mut self.marks).remove(i);
        }
    }

    /// Puts the text of a merge with the disk (DISK-2) in place of the text, as one undo step. `disk`
    /// is what the file holds: its text is the new base, and so is its format. The buffer is clean
    /// iff `clean`. The caller sets the new marks.
    pub fn merged(&mut self, text: &str, disk: Buffer, clean: bool) {
        self.checkpoint();
        self.record();
        self.text = Rope::from_str(text);
        // Clean iff the text is the file's. If the file is not what it was, no older state is.
        let same = disk.text == self.base && disk.format() == self.format();
        (self.bom, self.crlf, self.nl, self.eol) = (disk.bom, disk.crlf, disk.nl, disk.nl);
        self.base = disk.text;
        self.saved = match (clean, same) {
            (true, _) => self.version,
            (_, true) => self.saved,
            _ => u64::MAX,
        };
    }

    /// Starts an undo unit: it is recorded when the next `replace` happens.
    pub fn checkpoint(&mut self) {
        self.pending = Some(Snap {
            vmark: self.vmark,
            text: self.text.clone(),
            cursor: self.cursor,
            version: self.version,
            nl: self.nl,
            marks: self.marks.clone(),
        });
    }

    /// Undoes (or redoes) one unit; false if there is none.
    pub fn travel(&mut self, undo: bool) -> bool {
        let (from, to) =
            if undo { (&mut self.undo, &mut self.redo) } else { (&mut self.redo, &mut self.undo) };
        let Some(s) = from.pop() else { return false };
        let before = self.vmark;
        follow(&self.text, &s.text, &mut self.vmark);
        for (m, old) in self.vmark.iter_mut().zip(s.vmark).take(PREV) {
            *m = old.or(*m);
        }
        to.push(Snap {
            vmark: before,
            text: swap(&mut self.text, s.text),
            cursor: swap(&mut self.cursor, s.cursor),
            version: swap(&mut self.version, s.version),
            nl: swap(&mut self.nl, s.nl),
            marks: swap(&mut self.marks, s.marks),
        });
        (self.pending, self.want) = (None, None);
        true
    }
}
