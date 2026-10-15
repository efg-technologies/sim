//! SAFE-3, SAFE-4
//! The text (ropey) and undo. Every text change goes through `Buffer::replace`.
use ropey::Rope;
use std::mem::replace as swap;
use std::ops::Range;

/// Hash of a file that does not exist.
pub const ABSENT: u64 = 0;

/// FNV-1a 64.
pub fn hash(b: &[u8]) -> u64 {
    b.iter().fold(0xcbf2_9ce4_8422_2325, |h, &x| (h ^ u64::from(x)).wrapping_mul(0x0100_0000_01b3))
}

struct Snap {
    text: Rope,
    cursor: usize,
    version: u64,
}

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
    bom: bool,
    crlf: bool,
    /// Whether the file ended with a newline; `None` (empty or new file): write one iff there is text.
    nl: Option<bool>,
    version: u64,
    saved: u64,
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
        Buffer { base: text.clone(), text, readonly, bom, crlf, nl, ..Default::default() }
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

    pub fn mark_saved(&mut self, version: u64, text: Rope) {
        self.saved = version;
        self.base = text;
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

    /// The only way to change the text. A no-op on a read-only buffer.
    pub fn replace(&mut self, range: Range<usize>, with: &str) {
        let n = self.text.len_chars();
        let range = range.start.min(n)..range.end.min(n);
        if self.readonly.is_some() || (range.is_empty() && with.is_empty()) {
            return;
        }
        if let Some(p) = self.pending.take() {
            self.undo.push(p);
        }
        self.redo.clear();
        self.text.remove(range.clone());
        self.text.insert(range.start, with);
        self.next += 1;
        self.version = self.next;
    }

    /// Starts an undo unit: it is recorded when the next `replace` happens.
    pub fn checkpoint(&mut self) {
        self.pending =
            Some(Snap { text: self.text.clone(), cursor: self.cursor, version: self.version });
    }

    /// Undoes (or redoes) one unit; false if there is none.
    pub fn travel(&mut self, undo: bool) -> bool {
        let (from, to) =
            if undo { (&mut self.undo, &mut self.redo) } else { (&mut self.redo, &mut self.undo) };
        let Some(s) = from.pop() else { return false };
        to.push(Snap {
            text: swap(&mut self.text, s.text),
            cursor: swap(&mut self.cursor, s.cursor),
            version: swap(&mut self.version, s.version),
        });
        (self.pending, self.want) = (None, None);
        true
    }
}
