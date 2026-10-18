//! VI-17
//! Marks: `m{a-z}` sets one, `'` and `` ` `` go to one, `''` goes back to where the last jump was
//! made from; they move with the edits and are listed by `:marks`.
use super::app::App;
use super::buffer::{Buffer, PREV, Place, SAVED, Vmarks};
use super::reg::one_line;
use ropey::{Rope, RopeSlice, iter::Chunks};
use std::ops::{Range, RangeInclusive};

impl Buffer {
    /// A command that takes whole lines away says so: the marks on `lines` are gone.
    pub(super) fn forget(&mut self, lines: RangeInclusive<usize>) {
        for m in &mut self.vmark {
            if m.is_some_and(|p| lines.contains(&p.0)) {
                *m = None;
            }
        }
    }

    /// `replace` for an edit that puts lines after `line` or joins the next to it (`o`, `p`, `J`): the marks
    /// on `line` stay on it, an empty one too (the same edit of an empty line reads as lines put above it).
    pub(super) fn replace_after(&mut self, line: usize, r: Range<usize>, with: &str) {
        let own = self.vmark.map(|m| m.filter(|p| p.0 == line));
        self.replace(r, with);
        self.vmark.iter_mut().zip(own).for_each(|(m, o)| *m = o.or(*m));
    }

    /// Moves the marks to where an edit of `r` into `with` leaves them (called before the text is
    /// changed). Lines inserted above push a mark down; a mark on a line that goes is gone; a mark on the
    /// last line of a join goes with its text to the first. Chars typed inside a line move no mark.
    pub(super) fn adjust(&mut self, r: &Range<usize>, with: &str) {
        let (l1, l2, k) = (self.line_of(r.start), self.line_of(r.end), with.matches('\n').count());
        let (a, e) = (r.start - self.start(l1), r.end - self.start(l2));
        let (tail, same) = (with.rsplit('\n').next().map_or(0, |t| t.chars().count()), l1 == l2);
        let (gone, dels) = (with.is_empty() && !same, l2 - l1);
        // Whole lines deleted: l1 to l2 - 1 (they end where l2 begins), or l1 + 1 to l2 (the last line
        // goes with the break before it; an empty one is no such line, it is the join of J).
        let (head, back) = (
            gone && a == 0 && e == 0,
            gone && a == self.line_len(l1) && e > 0 && e == self.line_len(l2),
        );
        let above = r.is_empty() && a == 0 && with.ends_with('\n');
        // (the lines at the ends of a delete of whole lines were forgotten by the command: that range reads
        // the same for an empty line, which `head` and `back` cannot tell; the ones between go here)
        let dead = |l: usize| l1 < l && l < l2;
        for m in &mut self.vmark {
            let Some((l, c)) = *m else { continue };
            *m = match l {
                _ if dead(l) => None,
                l if l < l1 || (l == l1 && !above) => Some((l, c)),
                l if l == l1 => Some((l + k, c)),
                l if l == l2 && !head && !back && !same => {
                    Some((l1 + k, if k == 0 { a + tail } else { tail } + c.saturating_sub(e)))
                }
                l => Some((l + k - dels, c)),
            };
        }
    }
}

/// Line `i` of a text without its line break (the last line of a text has none).
fn bare(t: &Rope, i: usize) -> RopeSlice<'_> {
    let l = t.line(i);
    l.slice(..l.len_chars() - usize::from(l.len_chars() > 0 && l.char(l.len_chars() - 1) == '\n'))
}

/// How many bytes two ropes have in common at their fronts (or, with `back`, at their ends). A rope that
/// is a copy of another with an edit shares its chunks, which are then the same memory and not compared.
fn shared(a: &Rope, b: &Rope, back: bool) -> usize {
    fn at(r: &Rope, back: bool) -> Chunks<'_> {
        if back { r.chunks_at_byte(r.len_bytes()).0 } else { r.chunks() }
    }
    let (mut x, mut y, mut n) = (at(a, back), at(b, back), 0);
    let (mut cx, mut cy): (&[u8], &[u8]) = (&[], &[]);
    loop {
        for (c, it) in [(&mut cx, &mut x), (&mut cy, &mut y)] {
            if c.is_empty() {
                *c = if back { it.prev() } else { it.next() }.map_or(&[], str::as_bytes);
            }
        }
        let k = cx.len().min(cy.len());
        if k == 0 {
            return n;
        }
        let (p, q) =
            if back { (&cx[cx.len() - k..], &cy[cy.len() - k..]) } else { (&cx[..k], &cy[..k]) };
        if p.as_ptr() != q.as_ptr() {
            let apart = if back {
                p.iter().rev().zip(q.iter().rev()).position(|(i, j)| i != j)
            } else {
                p.iter().zip(q).position(|(i, j)| i != j)
            };
            if let Some(i) = apart {
                return n + i;
            }
        }
        n += k;
        (cx, cy) =
            if back { (&cx[..cx.len() - k], &cy[..cy.len() - k]) } else { (&cx[k..], &cy[k..]) };
    }
}

/// Undo or redo is about to put the text `to` in place of the text `from`: the marks follow their lines,
/// as they do in vim while it replays the change. When the number of lines differs, the lines the change
/// took away or put in are the changed ones and a mark on them is gone, and the marks below them are as
/// many lines further or nearer; with as many lines, every mark stays. The two texts share what the
/// change did not touch, so the ends they have in common are found by chunks (`shared`), not line by line.
pub(super) fn follow(from: &Rope, to: &Rope, marks: &mut Vmarks) {
    let (n, m) = (from.len_lines(), to.len_lines());
    if n == m || marks.iter().all(Option::is_none) {
        return;
    }
    let same = |i: usize, j: usize| bare(from, i) == bare(to, j);
    let (front, ends) = (shared(from, to, false), from.len_bytes().min(to.len_bytes()));
    let back = shared(from, to, true).min(ends - front);
    // (the lines wholly inside the bytes in common are the same; the lines that end the texts, or the one
    // that holds the first byte apart, are told by the line)
    let (top, tail) =
        (from.byte_to_line(front), n - from.byte_to_line(from.len_bytes() - back) - 1);
    let top = top + (top..n.min(m)).take_while(|&i| same(i, i)).count();
    let lim = n.min(m) - top;
    let tail =
        tail.min(lim) + (tail.min(lim)..lim).take_while(|&i| same(n - 1 - i, m - 1 - i)).count();
    let (old, new) = (n - top - tail, m - top - tail);
    for mk in marks {
        *mk = mk.and_then(|(l, c)| match l {
            l if l < top => Some((l, c)),
            l if l >= top + old => Some((l + new - old, c)),
            _ => None,
        });
    }
}

impl App {
    /// Where the cursor is, as a place.
    pub(super) fn here(&self) -> Place {
        (self.buf.line_of(self.buf.cursor), self.buf.col())
    }

    /// `m{a-z}`.
    pub(super) fn mark_set(&mut self, name: Option<char>) {
        match name {
            Some(c @ 'a'..='z') => self.buf.vmark[c as usize - 'a' as usize] = Some(self.here()),
            _ => self.say("marks: a-z only"),
        }
    }

    /// The place mark `c` names (`'` and `` ` `` the one before the last jump), or why there is none.
    pub(super) fn mark_at(&mut self, c: char) -> Option<Place> {
        let i = match c {
            'a'..='z' => c as usize - 'a' as usize,
            '\'' | '`' => PREV,
            _ => return self.fail("marks: a-z only"),
        };
        self.buf.vmark[i].or_else(|| self.fail(format!("mark not set: {c}")))
    }

    /// A motion that jumps (`G` `gg` `n` `N` `*` `#` `%` `/` a mark) is about to move the cursor: here is
    /// the place `''` goes back to.
    pub(super) fn jumped(&mut self, k: char) {
        if "GgnN*#%'`/".contains(k) {
            self.jump_from(self.here());
        }
    }

    /// The command jumps from `from`; the place before it is kept, to be put back if the jump goes nowhere.
    pub(super) fn jump_from(&mut self, from: Place) {
        let v = &mut self.buf.vmark;
        (v[SAVED], v[PREV]) = (v[PREV], Some(from));
    }

    /// A command that jumped has ended (vim's `checkpcmark`): if the cursor is where the jump began, or
    /// the place is gone (a delete took its line), the place before the jump is the one again.
    pub(super) fn jump_end(&mut self) {
        let here = self.here();
        let v = &mut self.buf.vmark;
        if let Some(old) = v[SAVED].take()
            && v[PREV].is_none_or(|p| p == here)
        {
            v[PREV] = Some(old);
        }
    }

    /// `:marks`: every mark that is set, as the line, the column and the text of its line, on one line of
    /// `width` columns (`one_line`).
    pub(super) fn list_marks(&self, width: usize) -> String {
        let b = &self.buf;
        let held = b.vmark.iter().enumerate().take(PREV + 1).filter_map(|(i, m)| m.map(|p| (i, p)));
        let rows: Vec<(String, String)> = held
            .map(|(i, (l, c))| {
                let name = if i == PREV { '\'' } else { char::from(b'a' + i as u8) };
                let line = b.text.line(l.min(b.lines() - 1));
                let text = line.slice(..line.len_chars().min(width + 80)).to_string();
                (format!("'{name} {}:{} ", l + 1, c + 1), text.trim().into())
            })
            .collect();
        one_line(&rows, width, "no marks")
    }
}
