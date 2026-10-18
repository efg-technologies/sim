//! TUTOR-1, TUTOR-2
//! `:tutor` in sim's state: the scratch buffer the lessons are in (never a file, never written, no
//! snapshot of it), the marks of its exercises that the screen draws over `[ ]`, and what the learner
//! has been seen to do that no text shows.
use super::{Id, Opened, Pick, Sim, SimEffect, text_of};
use crate::layout::{self, Skin};
use crate::tutor::{self, Tutor};
use sim_core::core::app::Event;
use sim_core::core::view;
use sim_core::io::file;

/// What every way of writing the lesson says (TUTOR-1).
const SCRATCH: &str = "the tutor is a scratch copy: nothing was written (:q! leaves)";

/// The core's status line for a buffer that has no file says `[no name]`: the lesson's says what it is.
pub(super) fn named(lesson: bool, status: String) -> String {
    if lesson { status.replacen("[no name]", "[tutor]", 1) } else { status }
}

/// The lesson that is open: its buffer, its exercises, which of them are done, what was seen of
/// the events (`tutor::bit`) and the panel and the look it began with.
pub(super) struct Lesson {
    id: Id,
    tutor: Tutor,
    ok: Vec<bool>,
    seen: u8,
    began: (bool, Skin),
}

impl Sim {
    /// TUTOR-1: the buffer the lesson is in.
    pub(super) fn lesson_id(&self) -> Option<Id> {
        self.lesson.as_ref().map(|l| l.id)
    }

    /// TUTOR-1: whether `id` is the tutor's scratch buffer: it is the lesson's and has no file (`:e name`
    /// from it makes the buffer that file's, and the lesson is over).
    pub(super) fn scratch(&self, id: Id) -> bool {
        let is = self.lesson.as_ref().is_some_and(|l| l.id == id);
        is && self.bufs.iter().any(|b| b.id == id && b.app.path.is_none())
    }

    /// The name of a buffer that has no file: `[tutor]` for the lesson's.
    pub(super) fn unnamed(&self, id: Id) -> &'static str {
        if self.scratch(id) { "[tutor]" } else { "[no name]" }
    }

    /// TUTOR-1: `:tutor` shows the lesson, a scratch buffer of the language of the locale; if it is
    /// open already, it is shown as it is.
    pub(super) fn open_tutor(&mut self, out: &mut Vec<SimEffect>) {
        let open = self.lesson.as_ref().and_then(|l| self.bufs.iter().position(|b| b.id == l.id));
        if let Some(i) = open {
            return (self.cur, self.panel.focus) = (i, false);
        }
        let tutor = tutor::load(&self.lang, self.plain);
        let (id, file) = (self.next, Ok(Some(tutor.text.clone().into_bytes())));
        self.next += 1;
        let key = "\0tutor".to_string();
        self.opened(Opened { id, path: String::new(), key, file, found: None, place: None }, out);
        let ok = vec![false; tutor.goals.len()];
        self.lesson = Some(Lesson { id, tutor, ok, seen: 0, began: (self.panel.open, self.skin) });
    }

    /// TUTOR-1: a write of the scratch buffer is not made: it is answered as a write that failed, so
    /// that the App does not wait for it and says why.
    pub(super) fn refuse(&mut self, id: Id, out: &mut Vec<SimEffect>) {
        self.to(id, Event::Saved(Err(SCRATCH.into()), file::seq()), out);
    }

    /// TUTOR-1: `:w`, `:wq`, `:x` and `ZZ` of a buffer that has no file are the core's, which says `no file
    /// name` and asks for nothing: what it says is said as `:w name` says it, without a line of the core's.
    pub(super) fn said(&mut self, id: Id) {
        let Some(b) = self.bufs.iter_mut().find(|b| b.id == id) else { return };
        if b.app.msg.as_deref() == Some("no file name") {
            b.app.msg = Some(SCRATCH.into());
            b.frame = Some(view::render(&mut b.app));
        }
    }

    /// TUTOR-2: before a frame, the lesson in view (what is done while another file is shown is not its): what has been seen of the events, the marks of
    /// the exercises (`✗` or `✓` drawn over the space of `[ ]`, the text not touched), and a word on the
    /// status line when an exercise has just been done.
    pub(super) fn grade(&mut self) {
        let here = self.bufs.get(self.cur).map(|b| b.id);
        let Some(lesson) = self.lesson.as_mut().filter(|l| here == Some(l.id)) else { return };
        let Some(b) = self.bufs.iter_mut().find(|b| b.id == lesson.id) else {
            return self.lesson = None;
        };
        if b.app.path.is_some() {
            return self.lesson = None;
        }
        let at = match self.panel.live.as_ref().map(|l| l.kind) {
            Some(Pick::Files) => "finder",
            Some(Pick::Grep) => "grep",
            _ => "",
        };
        let moved =
            [(self.panel.open != lesson.began.0, "tree"), (self.skin != lesson.began.1, "look")];
        lesson.seen |=
            tutor::bit(at) | moved.iter().filter(|m| m.0).fold(0, |a, m| a | tutor::bit(m.1));
        let buf = &b.app.buf;
        let lines: Vec<String> =
            (0..buf.lines()).map(|l| text_of(buf, l).unwrap_or_default()).collect();
        let marks = lesson.tutor.grade(&lines, lesson.seen);
        let news = marks.iter().find(|m| m.ok && !lesson.ok[m.goal]);
        let count = marks.iter().filter(|m| m.ok).count();
        if let Some(m) = news {
            self.msg = Some(format!(
                "✓ {} done · {count} of {}",
                lesson.tutor.goals[m.goal].id,
                marks.len()
            ));
        }
        lesson.ok = vec![false; lesson.ok.len()];
        marks.iter().for_each(|m| lesson.ok[m.goal] = m.ok);
        let digits = super::digits(buf);
        let Some(f) = b.frame.as_mut() else { return };
        for row in f.rows.iter_mut() {
            let line = layout::number(row, digits).and_then(|n| n.checked_sub(1));
            let Some(m) = marks.iter().find(|m| Some(m.line) == line) else { continue };
            // (the row is `{gutter}[ ]`: the space between the brackets is where the mark goes)
            let at = digits + 3;
            if row.text.get(at - 1..at + 2).is_some_and(|s| s.starts_with('[') && s.ends_with(']'))
            {
                row.text.replace_range(at..=at, if m.ok { "✓" } else { "✗" });
            }
        }
    }

    /// TUTOR-2: `<Space>bk` was pressed (the last of the events).
    pub(super) fn saw_kept(&mut self) {
        if let Some(l) = self.lesson.as_mut() {
            l.seen |= tutor::bit("kept");
        }
    }
}
