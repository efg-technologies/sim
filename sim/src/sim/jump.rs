//! JUMP-1
//! The jump by label in Sim's state (pure), the owner's flash.nvim on `s`: the query typed after it, its
//! matches in the lines on the screen, the label of each, and the keys that end it. Nothing here edits: a
//! label moves the cursor, and every other key ends the jump or is more of the query.
use super::ptr::put;
use super::{Buf, Sim, digits, text_of};
use crate::find::fold;
use crate::grep::Query;
use crate::layout::{LineUi, number};
use sim_core::core::app::Key;
use sim_core::core::buffer::PREV;
use sim_core::core::view::{show, width};
use std::ops::Range;

/// flash.nvim's labels, its default: the home row first; then the same in capitals (`label.uppercase`).
const LABELS: &str = "asdfghjklqwertyuiopzxcvbnm";

/// A match: its line, its chars in the line, its label (none where every label is taken, or where the char
/// after it cannot carry one) and its distance from the cursor.
pub(super) type Hit = (usize, Range<usize>, Option<char>, usize);

/// What line `l` lights while a jump is typed: its matches in place of a search's, and their labels.
pub(super) fn lights(hits: Option<&[Hit]>, l: usize, mut u: LineUi) -> LineUi {
    let Some(hits) = hits else { return u };
    let on = || hits.iter().filter(|h| h.0 == l);
    (u.hits, u.now) = (on().map(|h| h.1.clone()).collect(), None);
    u.labels = on().filter_map(|h| Some((h.1.end, h.2?))).collect();
    u
}

impl Sim {
    /// The matches of `query` in the lines the screen shows of `b`, in the order of the screen, each with its
    /// label: the nearest the cursor have the first ones (flash's `label.distance`). No letter that follows a
    /// match, in either case, is a label, so that a label is never taken for more of the query.
    pub(super) fn jump_hits(&self, b: &Buf, query: &str) -> Vec<Hit> {
        let (Some(q), Some(f)) = (Query::new(query), b.frame.as_ref()) else { return vec![] };
        let (buf, digits) = (&b.app.buf, digits(&b.app.buf));
        let rows = &f.rows[..f.rows.len().saturating_sub(1)]; // (the last row is the status line)
        let (mut hits, mut next) = (vec![], String::new());
        let (cl, cc, w) = (buf.line_of(buf.cursor), buf.col(), usize::from(self.size.0));
        for l in rows.iter().filter_map(|r| number(r, digits)?.checked_sub(1)) {
            let Some(t) = text_of(buf, l) else { continue };
            let cs: Vec<char> = t.chars().collect();
            for r in q.ranges(&t) {
                let (i, c) = (r.end, cs.get(r.end));
                next.extend(c.map(|&c| fold(c, false))); // (as the query folds it: GREP-2)
                // a label stands on a char drawn as itself, on a tab's first cell, or past the line's end;
                // never on a form (SAFE-8), which it would cover
                let ok =
                    c.is_none_or(|&c| c == '\t' || show(&cs, i).is_none() && width(&cs, i, 0) > 0);
                let d = (l * w + r.start).abs_diff(cl * w + cc);
                hits.push((l, r, ok.then_some('\0'), d)); // ('\0': it may have one)
            }
        }
        let upper = LABELS.chars().map(|c| c.to_ascii_uppercase());
        for c in LABELS.chars().chain(upper).filter(|&c| !next.contains(fold(c, false))) {
            let waiting = hits.iter_mut().filter(|h| h.2 == Some('\0'));
            let Some(h) = waiting.min_by_key(|h| h.3) else { break };
            h.2 = Some(c);
        }
        hits.iter_mut().filter(|h| h.2 == Some('\0')).for_each(|h| h.2 = None);
        hits
    }

    /// A key while a jump is typed: a label goes to its match and Enter to the nearest one (the place before it
    /// is `''`'s, VI-17); Backspace takes the last char off the query; a char is more of it, and one that
    /// leaves no match ends the jump (flash's); Esc and any other key end it, and nothing moved.
    pub(super) fn jump_key(&mut self, k: Key) {
        let (Some(mut query), Some(b)) = (self.jump.take(), self.bufs.get(self.cur)) else {
            return;
        };
        let hits = || self.jump_hits(b, &query); // (found where a key needs them: a key costs what they cost)
        let to = match k {
            Key::Enter => hits().into_iter().min_by_key(|h| h.3),
            Key::Char(c) if !query.is_empty() => hits().into_iter().find(|h| h.2 == Some(c)),
            _ => None,
        };
        if let Some((l, r, ..)) = to {
            let at = b.app.buf.start(l) + r.start;
            let Some(b) = self.bufs.get_mut(self.cur) else { return };
            let buf = &mut b.app.buf;
            if at != buf.cursor {
                buf.vmark[PREV] = Some((buf.line_of(buf.cursor), buf.col()));
            }
            put(&mut b.app, at); // (in Visual mode the selection goes with the cursor)
            return b.frame = None;
        }
        match k {
            Key::Backspace => drop(query.pop()),
            Key::Char(c) => query.push(c),
            _ => return,
        }
        if query.is_empty() || !self.jump_hits(b, &query).is_empty() {
            self.jump = Some(query);
        }
    }
}
