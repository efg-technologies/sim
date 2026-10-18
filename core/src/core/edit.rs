//! VI-1, VI-2, VI-3, VI-5, VI-6, VI-8, VI-12, VI-13, VI-15, VI-17, VIS-12, FILT-1, DISK-6
//! Motions, text objects, operators, and what each Normal, Visual and Insert mode key does.
use super::app::{App, Effect, Key, plural};
use super::buffer::{Buffer, PREV, SAVED};
use super::reg::Regs;
use super::vi::{Cmd, HALF_DN, HALF_UP, HOME, MOTIONS, Mode, PGUP, VERTICAL, ch};
use ropey::Rope;
use unicode_width::UnicodeWidthChar;

/// How far a motion reaches: up to its target, through it, or over whole lines.
#[rustfmt::skip]
#[derive(PartialEq)]
enum Kind { Excl, Incl, Line }

const PAIRS: [(char, char); 4] = [('(', ')'), ('[', ']'), ('{', '}'), ('<', '>')];
const BLANKS: &str = " \t";
/// The most text one command may add (bytes): above the 10 MB of PERF-5, and a count typed by
/// mistake must not make the allocation abort the process (VI-2).
const MAX_ADD: usize = 16 << 20;

/// 0 blank (a line break too), 1 word char, 2 other, 3 Hiragana, 4 Katakana, 5 Han, 6 other wide
/// letters (as vim's classes). `big`: every non-blank is a word char.
fn cls(c: char, big: bool) -> u8 {
    match c {
        _ if c.is_whitespace() => 0,
        _ if big => 1,
        '\u{3040}'..='\u{309f}' => 3,
        '\u{30a0}'..='\u{30ff}' => 4,
        '\u{3400}'..='\u{9fff}' | '\u{f900}'..='\u{faff}' | '\u{20000}'..='\u{3ffff}' => 5,
        _ if c.is_alphanumeric() => 1 + 5 * u8::from(c.width() == Some(2)),
        '_' => 1,
        _ => 2,
    }
}

/// Whether a word starts at `j`: the class changes to a non-blank, or it is an empty line.
fn starts(b: &Buffer, j: usize, big: bool) -> bool {
    let (c, prev) = (cls(b.ch(j), big), j.checked_sub(1).map(|p| b.ch(p)));
    (c != 0 && prev.is_none_or(|p| cls(p, big) != c)) || (prev == Some('\n') && b.ch(j) == '\n')
}

fn ends(b: &Buffer, j: usize, big: bool) -> bool {
    cls(b.ch(j), big) != 0 && cls(b.ch(j + 1), big) != cls(b.ch(j), big)
}

/// From `i` (exclusive) to the `other` bracket that balances it, across lines.
fn scan(b: &Buffer, i: usize, same: char, other: char, fwd: bool) -> Option<usize> {
    let it = if fwd { b.text.chars_at(i + 1) } else { b.text.chars_at(i).reversed() };
    let mut depth = 0usize;
    for (k, c) in it.enumerate() {
        if c == other && depth == 0 {
            return Some(if fwd { i + 1 + k } else { i - 1 - k });
        }
        depth += usize::from(c == same);
        depth -= usize::from(c == other);
    }
    None
}

impl App {
    /// A Normal-mode cursor rests on a char, not after the last one.
    pub fn clamp(&mut self) {
        let b = &mut self.buf;
        let l = b.line_of(b.cursor);
        if self.vi.mode != Mode::Insert {
            b.cursor = b.cursor.min(b.start(l) + b.line_len(l).saturating_sub(1));
        }
    }

    fn indent_of(&self, line: usize) -> String {
        self.buf.text.line(line).chars().take_while(|c| BLANKS.contains(*c)).collect()
    }

    /// VI-6: the indent unit, inferred from the first indented line on first use.
    fn unit(&mut self) -> String {
        let indented = |l: &ropey::RopeSlice| {
            l.chars().next().is_some_and(|c| BLANKS.contains(c))
                && l.chars().any(|c| !c.is_whitespace())
        };
        let text = &self.buf.text;
        let infer = || match text.lines().find(indented) {
            Some(l) if l.char(0) == '\t' => "\t".to_string(),
            Some(l) => " ".repeat(l.chars().take_while(|&c| c == ' ').count()),
            None => "    ".into(),
        };
        self.unit.get_or_insert_with(infer).clone()
    }

    /// The selection of Visual mode as a char range (whole lines in V-LINE).
    pub fn selection(&self) -> Option<(usize, usize)> {
        let b = &self.buf;
        let (s, e) = (self.anchor.min(b.cursor), self.anchor.max(b.cursor));
        match self.vi.mode {
            Mode::Visual => Some((s, (e + 1).min(b.text.len_chars()))),
            Mode::VLine => Some((b.start(b.line_of(s)), b.end(b.line_of(e)))),
            _ => None,
        }
    }

    /// Ends Visual mode (if it is on) and remembers the selected lines as `'<` and `'>`.
    pub(super) fn leave_visual(&mut self) {
        let b = &self.buf;
        if self.selection().is_some() {
            self.marks =
                (b.line_of(self.anchor.min(b.cursor)), b.line_of(self.anchor.max(b.cursor)));
            self.vi.mode = Mode::Normal;
        }
    }

    /// Where motion `k` goes from the cursor, and the kind of range it makes after operator `op`
    /// (`'\0'` for none).
    fn motion(&mut self, k: char, arg: Option<char>, n: usize, op: char) -> Option<(usize, Kind)> {
        if matches!(k, 'n' | 'N' | '*' | '#') {
            return self.search_key(k).map(|t| (t, Kind::Excl));
        }
        if matches!(k, '\'' | '`') {
            let (l, c) = self.mark_at(arg?)?;
            let b = &self.buf;
            let l = l.min(b.lines() - 1);
            // `'` is a line motion to its first non-blank; `` ` `` is exclusive, to the place.
            return Some(if k == '\'' {
                (b.nonblank(l), Kind::Line)
            } else {
                (b.start(l) + c.min(b.line_len(l)), Kind::Excl)
            });
        }
        let b = &self.buf;
        let (cur, l, n1, len) = (b.cursor, b.line_of(b.cursor), n.max(1), b.text.len_chars());
        let (last, rows) = (b.lines() - 1, usize::from(self.size.1) - 1);
        Some(match k {
            'h' => (cur - n1.min(b.col()), Kind::Excl),
            'l' => (cur.saturating_add(n1).min(b.end(l)), Kind::Excl),
            k if VERTICAL.contains(k) => {
                let page = match k {
                    'j' | 'k' => 1,
                    HALF_DN | HALF_UP => rows / 2,
                    _ => rows,
                };
                let step = n1.saturating_mul(page);
                let up = matches!(k, 'k' | HALF_UP | PGUP);
                let t = if up { l.saturating_sub(step) } else { l.saturating_add(step).min(last) };
                if t == l && op != '\0' {
                    return None;
                }
                self.center |= !matches!(k, 'j' | 'k');
                (b.start(t) + b.want.unwrap_or(b.col()).min(b.line_len(t)), Kind::Line)
            }
            '0' | HOME => (b.start(l), Kind::Excl),
            '^' => (b.nonblank(l), Kind::Excl),
            '$' => (b.end(l.saturating_add(n1 - 1).min(last)), Kind::Incl),
            'G' | 'g' => {
                let t = (if n > 0 || k == 'g' { n.saturating_sub(1) } else { last }).min(last);
                (b.nonblank(t), Kind::Line)
            }
            'w' | 'W' | 'b' | 'B' | 'e' | 'E' => {
                let (big, low) = (k.is_ascii_uppercase(), k.to_ascii_lowercase());
                // The empty last line has no word ahead, and nothing to reach back for.
                if cur >= len && low != 'b' {
                    return Some((cur, Kind::Excl));
                }
                // `cw` is `ce`, unless it starts on a blank.
                let cw = low == 'w' && op == 'c' && cls(b.ch(cur), big) != 0;
                let word_end = |f: usize| (f..len).find(|&j| ends(b, j, big)).unwrap_or(len - 1);
                let (mut i, mut prev) = (cur, cur);
                for s in 0..n1 {
                    let next = match low {
                        'b' => (0..i).rev().find(|&j| starts(b, j, big)).unwrap_or(0),
                        'e' => word_end(i + 1),
                        _ if cw => word_end(i + usize::from(s > 0)),
                        _ => (i + 1..len).find(|&j| starts(b, j, big)).unwrap_or(len),
                    };
                    (prev, i) = (i, next);
                    // Stuck at an end of the text: more steps change nothing, so a huge count ends.
                    if i == prev && s > 0 {
                        break;
                    }
                }
                // `dw` on the last word of a line stops at the line end, or at the start of the
                // next line when it starts on an empty one.
                let pl = b.line_of(prev);
                if low == 'w' && op != '\0' && !cw && b.line_of(i) > pl {
                    i = b.end(pl) + usize::from(b.line_len(pl) == 0);
                }
                (i, if low == 'e' || cw { Kind::Incl } else { Kind::Excl })
            }
            'f' | 'F' | 't' | 'T' | ';' | ',' => {
                let again = matches!(k, ';' | ',');
                let (fk, c) = if again { self.find? } else { (k, arg?) };
                let fk = if k == ',' { (fk as u8 ^ 32) as char } else { fk };
                let (s, e, fwd) = (b.start(l), b.end(l), fk.is_ascii_lowercase());
                // `;` after `t` does not stay stuck in front of the char.
                let skip = usize::from(again && matches!(fk, 't' | 'T'));
                let hit = |i: &usize| b.ch(*i) == c;
                let i = if fwd {
                    (cur + 1 + skip..e).filter(hit).nth(n1 - 1)?
                } else {
                    (s..cur.saturating_sub(skip)).rev().filter(hit).nth(n1 - 1)?
                };
                if !again {
                    self.find = Some((k, c));
                }
                let to = i + usize::from(fk == 'T') - usize::from(fk == 't');
                (to, if fwd { Kind::Incl } else { Kind::Excl })
            }
            '%' => {
                let i = (cur..b.end(l)).find(|&i| "()[]{}".contains(b.ch(i)))?;
                let (o, c) = *PAIRS.iter().find(|p| p.0 == b.ch(i) || p.1 == b.ch(i))?;
                let (same, other, fwd) = if b.ch(i) == o { (o, c, true) } else { (c, o, false) };
                (scan(b, i, same, other, fwd)?, Kind::Incl)
            }
            _ => return None,
        })
    }

    /// Moves the cursor like a motion key does.
    fn go(&mut self, k: char, arg: Option<char>, n: usize) {
        let want = self.buf.want.unwrap_or(self.buf.col());
        if let Some((t, _)) = self.motion(k, arg, n, '\0') {
            self.jumped(k);
            self.buf.cursor = t;
            self.buf.want = VERTICAL.contains(k).then_some(want);
        }
    }

    /// A text object around the cursor: char range and whether it is whole lines.
    fn object(&self, around: bool, o: char) -> Option<(usize, usize, bool)> {
        let b = &self.buf;
        let (i, l) = (b.cursor, b.line_of(b.cursor));
        let (ls, le, big) = (b.start(l), b.end(l), o == 'W');
        let blank = |j: &usize| BLANKS.contains(b.ch(*j));
        // `a`: the blanks after the object, or before it if there are none.
        let with_blanks = |s: usize, e: usize| match (e..le).take_while(blank).count() {
            0 => (s - (ls..s).rev().take_while(blank).count(), e),
            n => (s, e + n),
        };
        let run = |i: usize| {
            let same = |j: &usize| cls(b.ch(*j), big) == cls(b.ch(i), big);
            let (s, e) = ((ls..=i).rev().take_while(same).last(), (i..le).take_while(same).last());
            (s.unwrap_or(i), e.map_or(i, |j| j + 1))
        };
        let range = match o {
            'w' | 'W' => match (around, run(i), cls(b.ch(i), big)) {
                (false, r, _) => r,
                (_, (s, e), 0) => (s, if e < le { run(e).1 } else { e }),
                (_, (s, e), _) => with_blanks(s, e),
            },
            '"' | '\'' => {
                let quote = |j: &usize| b.ch(*j) == o && (*j == ls || b.ch(*j - 1) != '\\');
                let qs: Vec<usize> = (ls..le).filter(quote).collect();
                let q = qs.chunks(2).find(|p| p.len() == 2 && p[1] >= i)?;
                let (qs, qe) = (q[0], q[1]);
                if around { with_blanks(qs, qe + 1) } else { (qs + 1, qe) }
            }
            _ => {
                let (op, cl) = *PAIRS.iter().find(|p| p.0 == o || p.1 == o)?;
                let s = if b.ch(i) == op { i } else { scan(b, i, cl, op, false)? };
                let e = scan(b, s, op, cl, true)?;
                // `{` ending its line and `}` starting its own: the lines between, as lines.
                let (sl, el) = (b.line_of(s), b.line_of(e));
                if !around && b.ch(s + 1) == '\n' && b.nonblank(el) == e && el > sl + 1 {
                    return Some((b.start(sl + 1), b.end(el - 1), true));
                }
                if around { (s, e + 1) } else { (s + 1, e) }
            }
        };
        Some((range.0, range.1, false))
    }

    /// The range an operator acts on: chars, and whether they are whole lines.
    fn target(&mut self, op: char, c: &Cmd) -> Option<(usize, usize, bool)> {
        if let Some((s, e)) = self.selection() {
            return Some((s, e, self.vi.mode == Mode::VLine));
        }
        let (cur, l) = (self.buf.cursor, self.buf.line_of(self.buf.cursor));
        let lines =
            |b: &Buffer, l1: usize, l2: usize| (b.start(l1), b.end(l2.min(b.lines() - 1)), true);
        if c.k == op {
            return Some(lines(&self.buf, l, l.saturating_add(c.n.max(1) - 1)));
        }
        if matches!(c.k, 'i' | 'a') {
            return self.object(c.k == 'a', c.arg?);
        }
        let (t, kind) = self.motion(c.k, c.arg, c.n, op)?;
        self.jumped(c.k);
        let b = &self.buf;
        let (s, e) = (cur.min(t), cur.max(t));
        Some(match kind {
            Kind::Line => lines(b, b.line_of(s), b.line_of(e)),
            // vim's exclusive rules: ending in column 1 it ends at the end of the line before, and
            // from the first non-blank or earlier it takes whole lines.
            Kind::Excl if b.line_of(s) < b.line_of(e) && e == b.start(b.line_of(e)) => {
                let (ls, le) = (b.line_of(s), b.line_of(e));
                if s <= b.nonblank(ls) { lines(b, ls, le - 1) } else { (s, b.end(le - 1), false) }
            }
            Kind::Excl => (s, e, false),
            Kind::Incl => (s, e + usize::from(!matches!(b.ch(e), '\n' | '\0')), false),
        })
    }

    fn operate(&mut self, op: char, c: &Cmd) {
        let Some((a, e, lw)) = self.target(op, c) else { return self.failed = true };
        let vis = self.vi.mode.visual();
        self.leave_visual();
        let b = &mut self.buf;
        let (l1, l2, col) = (b.line_of(a), b.line_of(e), b.col());
        let mut text = Rope::from(b.text.slice(a..e));
        if lw {
            text.append(Rope::from_str("\n"));
        }
        match op {
            'y' => {
                // Taking nothing leaves the registers and the clipboard as they are (VI-8); the black
                // hole takes what a yank takes and says nothing.
                if text.len_chars() > 0 && c.reg != Some('_') {
                    // The clipboard gives no sign that it got the text, so say what was taken.
                    #[rustfmt::skip]
                    let (n, one, many) = if lw { (text.len_lines() - 1, "line", "lines") } else { (text.len_chars(), "char", "chars") };
                    self.msg = Some(format!("{} yanked", plural(n, one, many)));
                    self.yank(c.reg, text, lw);
                }
                let b = &mut self.buf;
                b.cursor = if lw { b.start(l1) + col.min(b.line_len(l1)) } else { a };
            }
            'd' | 'c' => {
                if text.len_chars() > 0 {
                    // A cut into the clipboard (`"+dd`) is asked for; a plain delete never goes out.
                    if matches!(c.reg, Some('+' | '*')) {
                        self.fx.push(Effect::Clipboard(text.to_string()));
                    }
                    let big =
                        lw || text.len_lines() > 1 || matches!(c.k, '%' | 'n' | 'N' | '*' | '#');
                    self.regs.delete(c.reg, text, lw, big);
                }
                let last = e >= b.text.len_chars();
                // Whole lines go with one line break: the one after them, or before the last.
                let (from, to) = match (op, lw) {
                    ('c', _) => (if lw { b.nonblank(l1) } else { a }, e),
                    (_, true) => (a - usize::from(last && a > 0), e + usize::from(!last)),
                    _ => (a, e),
                };
                if lw {
                    // (`c` keeps the first line, emptied)
                    b.forget(l1 + usize::from(op == 'c')..=l2);
                }
                b.replace(from..to, "");
                b.cursor = if op == 'd' && lw { b.nonblank(l1.min(b.lines() - 1)) } else { from };
                self.ai = (op == 'c' && lw && from > b.start(l1)).then_some(l1);
                self.vi.mode = if op == 'c' { Mode::Insert } else { Mode::Normal };
            }
            '!' => {
                let range = if vis { "'<,'>".into() } else { format!("{},{}", l1 + 1, l2 + 1) };
                self.vi.open(&format!(":{range}!"));
            }
            _ => {
                let unit = self.unit();
                let b = &mut self.buf;
                for l in l1..=l2 {
                    let (w, s) = (if unit == "\t" { 4 } else { unit.len() }, b.start(l));
                    let spaces = (0..w).take_while(|&i| b.ch(s + i) == ' ').count();
                    let cut = if op == '>' { 0 } else { spaces.max(usize::from(b.ch(s) == '\t')) };
                    let add = if op == '>' && b.line_len(l) > 0 { unit.as_str() } else { "" };
                    b.replace(s..s + cut, add);
                }
                b.cursor = b.nonblank(l1);
            }
        }
    }

    /// Normal and Visual mode commands.
    pub(super) fn exec(&mut self, c: Cmd) {
        let (n1, vis) = (c.n.max(1), self.vi.mode.visual());
        if let Some(r) = c.reg.filter(|r| !Regs::valid(*r)) {
            self.leave_visual();
            self.failed = true;
            return self.say(format!("not a register: {r}"));
        }
        let edits = match c.op {
            Some(o) => "dc<>".contains(o),
            None => !vis && "iaIAoOpPJr".contains(c.k),
        };
        // Only the vertical motions keep the wanted column.
        self.buf.want =
            self.buf.want.filter(|_| c.op.is_none() && (VERTICAL.contains(c.k) || c.k == 'm'));
        if edits {
            if let Some(why) = self.buf.readonly.clone() {
                self.vi.mode = Mode::Normal;
                return self.say(why);
            }
            self.buf.checkpoint();
        }
        if let Some(op) = c.op {
            self.operate(op, &c);
            return self.clamp();
        }
        match c.k {
            // In Visual mode only motions, selection keys and `:` work.
            k if vis && !MOTIONS.contains(k) && !"\x1b\x03vVoia:m".contains(k) => {}
            k if MOTIONS.contains(k) => self.go(k, c.arg, c.n),
            '\x1b' | '\x03' if vis => self.leave_visual(),
            '\x03' => match self.filter.as_mut() {
                Some(run) => {
                    run.killed = true;
                    self.fx.push(Effect::Kill);
                    self.say("cancelling the filter");
                }
                None => self.say("q to quit"),
            },
            'v' | 'V' => {
                let m = if c.k == 'v' { Mode::Visual } else { Mode::VLine };
                self.anchor = if vis { self.anchor } else { self.buf.cursor };
                if self.vi.mode == m { self.leave_visual() } else { self.vi.mode = m }
            }
            'o' if vis => std::mem::swap(&mut self.anchor, &mut self.buf.cursor),
            'i' | 'a' if vis => {
                if let Some((s, e, _)) =
                    c.arg.and_then(|o| self.object(c.k == 'a', o)).filter(|r| r.1 > r.0)
                {
                    let (a, cur) = (self.anchor, self.buf.cursor);
                    (self.anchor, self.buf.cursor) =
                        if a == cur { (s, e - 1) } else { (a.min(s), cur.max(e - 1)) };
                }
            }
            'i' | 'a' | 'I' | 'A' | 'o' | 'O' => self.open_insert(c.k),
            'p' | 'P' => self.paste(c.k == 'p', n1, c.reg),
            'm' => self.mark_set(c.arg),
            'J' => self.join(n1),
            'r' => {
                let b = &mut self.buf;
                let room = b.line_len(b.line_of(b.cursor)).saturating_sub(b.col());
                if let Some(r) = c.arg.filter(|r| n1 <= room && self.fits(r.len_utf8() * n1)) {
                    let b = &mut self.buf;
                    b.replace(b.cursor..b.cursor + n1, &r.to_string().repeat(n1));
                    b.cursor += n1 - 1;
                }
            }
            'u' | 'U' | '\x12' => {
                self.rec.clear(); // undo is not a change: `.` keeps repeating the last real one
                self.jump_from(self.here()); // (vim's undo is a jump, its marks following the lines it brings back)
                if !self.buf.travel(c.k == 'u') {
                    self.buf.vmark[PREV] = self.buf.vmark[SAVED].take();
                    self.say(if c.k == 'u' { "nothing to undo" } else { "nothing to redo" });
                }
            }
            '.' => self.dot(c.n),
            ':' | '/' | '?' if !vis || c.k == ':' => {
                self.leave_visual();
                self.vi.open(&if vis { ":'<,'>".to_string() } else { c.k.to_string() });
            }
            'q' => self.ex("q"),
            'Z' => self.ex(if c.arg == Some('Z') { "x" } else { "q!" }),
            'z' => self.center = true,
            '[' | ']' => self.jump(c.k == ']'),
            _ => {}
        }
        self.clamp();
    }

    fn open_insert(&mut self, k: char) {
        let l = self.buf.line_of(self.buf.cursor);
        let ind = self.indent_of(l);
        let b = &mut self.buf;
        match k {
            'a' => b.cursor += usize::from(b.line_len(l) > 0),
            'I' => b.cursor = b.nonblank(l),
            'A' => b.cursor = b.end(l),
            'o' | 'O' => {
                let at = if k == 'o' { b.end(l) } else { b.start(l) };
                if k == 'o' {
                    b.replace_after(l, at..at, &format!("\n{ind}"));
                } else {
                    b.replace(at..at, &format!("{ind}\n"));
                }
                b.cursor = at + ind.chars().count() + usize::from(k == 'o');
                self.ai = (!ind.is_empty()).then_some(l + usize::from(k == 'o'));
            }
            _ => {}
        }
        self.vi.mode = Mode::Insert;
    }

    /// Whether a command may add `bytes` of text; says why not (VI-2).
    fn fits(&mut self, bytes: usize) -> bool {
        if bytes > MAX_ADD {
            self.say("too large: 16 MiB at most");
        }
        bytes <= MAX_ADD
    }

    fn paste(&mut self, after: bool, n: usize, name: Option<char>) {
        if matches!(name, Some('+' | '*')) {
            self.failed = true;
            return self
                .say("the clipboard is write-only: paste with the terminal (bracketed paste)");
        }
        let Some((text, lw)) = self.regs.get(name).cloned() else {
            return self.say("the register is empty");
        };
        if !self.fits(text.len_bytes().saturating_mul(n)) {
            return;
        }
        let b = &mut self.buf;
        let (l, s) = (b.line_of(b.cursor), text.to_string().repeat(n));
        if lw && after {
            let (end, s) = (b.end(l), s.strip_suffix('\n').unwrap_or(&s));
            b.replace_after(l, end..end, &format!("\n{s}"));
            b.cursor = b.nonblank(l + 1);
        } else if lw {
            b.replace(b.start(l)..b.start(l), &s);
            b.cursor = b.nonblank(l);
        } else {
            let at = b.cursor + usize::from(after && b.line_len(l) > 0);
            b.replace(at..at, &s);
            b.cursor =
                if s.contains('\n') { at } else { (at + s.chars().count()).saturating_sub(1) };
        }
    }

    fn join(&mut self, n: usize) {
        let b = &mut self.buf;
        let l = b.line_of(b.cursor);
        for _ in 1..n.max(2).min(b.lines() - l) {
            let (e, nx) = (b.end(l), b.nonblank(l + 1));
            let space = b.line_len(l) > 0
                && nx < b.end(l + 1)
                && !b.ch(e - 1).is_whitespace()
                && b.ch(nx) != ')';
            // (a join keeps the marks of its first line, an empty one too)
            b.replace_after(l, e..nx, if space { " " } else { "" });
            b.cursor = e;
        }
    }

    /// `.`: types the keys of the last change again, with `n` instead of its count.
    fn dot(&mut self, n: usize) {
        if self.replaying {
            return; // a `.` inside a replay would replay itself for ever
        }
        let mut keys = self.last.clone();
        if n > 0 {
            let digits = keys.iter().take_while(|k| matches!(k, Key::Char('0'..='9'))).count();
            keys =
                n.to_string().chars().map(Key::Char).chain(keys.into_iter().skip(digits)).collect();
        }
        // `"1p` repeated is `"2p`, then `"3p`: the walk of VI-15 (only a register named in front of the
        // command: a `"1` typed as text is text).
        let at = keys.iter().take_while(|k| matches!(k, Key::Char('0'..='9'))).count();
        if let [Key::Char('"'), Key::Char(d @ '1'..='8'), ..] = keys[at..] {
            keys[at + 1] = Key::Char(char::from(d as u8 + 1));
        }
        (self.failed, self.replaying) = (false, true);
        for k in keys {
            self.key(k);
            // A change that fails stops the replay: its typed text must not become commands.
            if self.failed {
                break;
            }
        }
        self.replaying = false;
    }

    /// Inserts `s` at the cursor as one undo unit (inside an insert session, as part of its unit);
    /// Insert mode rests after it, Normal mode on its last char.
    pub(super) fn paste_text(&mut self, s: &str) {
        let insert = self.vi.mode == Mode::Insert;
        if !insert {
            self.buf.checkpoint();
        }
        self.type_str(s);
        self.buf.cursor -= usize::from(!insert && !s.is_empty());
        (self.buf.want, self.ai) = (None, None);
        self.clamp();
    }

    fn type_str(&mut self, s: &str) {
        let b = &mut self.buf;
        b.replace(b.cursor..b.cursor, s);
        b.cursor += s.chars().count();
    }

    /// A key typed in Insert mode. An indent that nothing was typed after goes when the session
    /// ends or the line is left (VI-6).
    pub(super) fn insert(&mut self, k: Key) {
        let ai = self.ai.take();
        self.insert_key(k);
        let Some(l) = ai else { return };
        let b = &mut self.buf;
        if self.vi.mode == Mode::Insert && b.line_of(b.cursor) == l {
            self.ai = (!matches!(k, Key::Char(_) | Key::Tab)).then_some(l);
        } else if l < b.lines() && b.nonblank(l) == b.end(l) {
            let (s, e) = (b.start(l), b.end(l));
            b.replace(s..e, "");
            b.cursor = if b.cursor > e { b.cursor - (e - s) } else { b.cursor.min(s) };
        }
    }

    fn insert_key(&mut self, k: Key) {
        if !matches!(k, Key::Up | Key::Down | Key::PageUp | Key::PageDown) {
            self.buf.want = None;
        }
        let b = &self.buf;
        let (l, cur, col) = (b.line_of(b.cursor), b.cursor, b.col());
        let (ls, nb) = (b.start(l), b.nonblank(l));
        let to_indent = if cur > nb { nb } else { ls };
        match k {
            Key::Char(c) => self.type_str(&c.to_string()),
            Key::Tab => {
                let unit = self.unit();
                self.type_str(&unit);
            }
            Key::Enter => {
                let ind = self.indent_of(l);
                let ind = &ind[..ind.len().min(col)];
                self.type_str(&format!("\n{ind}"));
                self.ai = (!ind.is_empty()).then_some(l + 1);
            }
            Key::Backspace | Key::Delete | Key::Ctrl('w') | Key::Ctrl('u') => {
                let b = &mut self.buf;
                let from = match k {
                    Key::Backspace => cur.saturating_sub(1),
                    Key::Ctrl('u') => to_indent,
                    Key::Ctrl('w') => {
                        let j =
                            cur - (ls..cur).rev().take_while(|&j| b.ch(j).is_whitespace()).count();
                        let c = cls(b.ch(j.saturating_sub(1)), false);
                        let from =
                            j - (ls..j).rev().take_while(|&x| cls(b.ch(x), false) == c).count();
                        if from == cur { cur.saturating_sub(1) } else { from }
                    }
                    _ => cur,
                };
                b.replace(from..if k == Key::Delete { cur + 1 } else { cur }, "");
                b.cursor = from;
            }
            Key::Esc | Key::Ctrl('c') => {
                self.vi.mode = Mode::Normal;
                self.buf.cursor -= usize::from(col > 0);
            }
            // The cursor keys are the variants from `Up` to `PageDown`.
            k if (Key::Up..=Key::PageDown).contains(&k) => self.go(ch(k), None, 1),
            _ => {}
        }
    }
}
