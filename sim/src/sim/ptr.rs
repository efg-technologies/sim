//! PTR-1, PTR-2, UI-4
//! The mouse (pure): a click in the panel acts on its row, a click in the text puts the cursor on
//! the char under the pointer (through the composed frame, `layout::hit`), a drag selects as Visual
//! mode does, and the wheel scrolls what is under the pointer. Nothing here edits: the mouse
//! changes the cursor, the focus, the scroll and the selection, and nothing else (PTR-2).
use super::{Id, Sim, SimEffect, text_of};
use crate::layout;
use crate::tabs::{self, Hit};
use crossterm::event::{MouseButton as B, MouseEvent, MouseEventKind as K};
use si::core::app::{App, Event, Key};
use si::core::vi::Mode;
use si::core::view;

type Out<'a> = &'a mut Vec<SimEffect>;

/// How many lines or rows a notch of the wheel scrolls.
const NOTCH: usize = 3;

/// Puts the cursor on char `at`, as a key that moved it would leave it: a Normal-mode cursor rests
/// on a char (the App's own `clamp`), the wanted column starts again, and the mark of the line it
/// enters is seen (DISK-6).
fn put(app: &mut App, at: usize) {
    let b = &mut app.buf;
    (b.cursor, b.want) = (at.min(b.text.len_chars()), None);
    app.clamp();
    app.buf.see();
}

impl Sim {
    /// PTR-1: where the pointer is decides what it does. The panel has its columns, then a blank
    /// one, then the text; the status line and the blank column take clicks and the wheel and do
    /// nothing with them. Only the left button acts; the others, and a pointer that only moves,
    /// are ignored.
    pub(super) fn mouse(&mut self, m: MouseEvent, out: Out) {
        let g = self.geo();
        let (x, y) = (m.column, m.row.saturating_sub(g.y0));
        let body = m.row >= g.y0 && y < g.rows;
        let (panel, text) =
            (body && x >= g.px && x - g.px < g.pw, body && x >= g.tx() && x - g.tx() < g.ew);
        // UI-4: a press on the tab bar (the header row, over the editor) switches the file.
        let on_bar = g.head > 0 && m.row + 1 == g.y0 && x >= g.ex && x - g.ex < g.ew + g.sign;
        match m.kind {
            K::Down(B::Left) if on_bar => self.click_tab(x - g.ex, out),
            K::Down(B::Left) if panel => self.click_panel(usize::from(y), out),
            K::Down(B::Left) if text => self.press(x - g.tx(), y, out),
            K::Drag(B::Left) => self.drag(x.saturating_sub(g.tx()), y.min(g.rows - 1), out),
            K::Up(B::Left) => self.drag = None,
            K::ScrollUp | K::ScrollDown if panel => self.roll(m.kind == K::ScrollUp),
            K::ScrollUp | K::ScrollDown if text => self.wheel(m.kind == K::ScrollUp, out),
            _ => {}
        }
    }

    /// UI-4: a click on the tab bar at column `x` of it: the file of that tab is shown (the arrows
    /// go one file back or on), as Tab and Shift-Tab would, and nothing is edited (PTR-2). What the
    /// file being left had pending (a count, an operator, a selection) ends as a click ends it.
    fn click_tab(&mut self, x: u16, out: Out) {
        let g = self.geo();
        let hit = tabs::hit(&self.tabs(), self.icons, self.cur, g.ew + g.sign, x);
        let to = match hit {
            Some(Hit::Tab(i)) => i,
            Some(Hit::Prev) => self.cur.saturating_sub(1),
            Some(Hit::Next) => (self.cur + 1).min(self.bufs.len().saturating_sub(1)),
            None => return,
        };
        let Some(b) = self.bufs.get(self.cur) else { return };
        let id = b.id;
        if b.app.vi.mode == Mode::Cmd {
            return; // a half-typed command line is not left by a click
        }
        self.drag = None;
        self.msg = None;
        self.settle(id, true, out);
        self.cur = to.min(self.bufs.len().saturating_sub(1));
        self.panel.focus = false;
        self.panel.search.iter_mut().for_each(|s| s.typing = false);
    }

    /// A click on a row of the panel: the panel's cursor goes there and the panel has the focus;
    /// a file is opened and a directory toggled, as Enter would (TREE-4). The keys of the panel's
    /// own cancel a reveal that is waiting, and so does a click; a search stops being typed.
    fn click_panel(&mut self, y: usize, out: Out) {
        let i = self.panel.top + y;
        self.drag = None;
        if i >= self.view().visible() {
            return;
        }
        self.msg = None;
        self.panel.reveal = None;
        self.panel.search.iter_mut().for_each(|s| s.typing = false);
        (self.panel.cur, self.panel.focus) = (i, true);
        match self.row(i) {
            Some(r) if r.is_dir && r.open => self.back(),
            _ => self.enter(out),
        }
    }

    /// The char of the current file under the point (`x` counted from the text's first column) of
    /// the frame the user sees.
    fn char_at(&self, x: u16, y: u16) -> Option<usize> {
        let b = self.bufs.get(self.cur)?;
        let (buf, x) = (&b.app.buf, usize::from(x));
        let digits = buf.lines().to_string().len().max(3);
        let (l, i) = layout::hit(b.frame.as_ref()?, digits, &|l| text_of(buf, l), (x, y.into()))?;
        Some(buf.start(l) + i.min(buf.line_len(l)))
    }

    /// What a click ends before it moves the cursor, as the Esc key would, and no more: a Visual
    /// selection, and a count, operator or prefix still waiting for the rest of its command (which
    /// the pointer must not complete).
    fn settle(&mut self, id: Id, click: bool, out: Out) {
        for _ in 0..2 {
            // (the Esc of a Visual mode with a prefix waiting only forgets the prefix)
            let Some(b) = self.bufs.iter().find(|b| b.id == id) else { return };
            let vi = &b.app.vi;
            if vi.mode == Mode::Normal && !vi.idle() || click && vi.mode.visual() {
                self.to(id, Event::Key(Key::Esc), out);
            }
        }
    }

    /// A press in the text: the text has the focus, and the cursor goes to the char under it. A
    /// drag may go on from here. On the command line nothing happens to the text.
    fn press(&mut self, x: u16, y: u16, out: Out) {
        let Some(b) = self.bufs.get(self.cur) else { return };
        let id = b.id;
        (self.msg, self.drag, self.panel.focus, self.z) = (None, None, false, false); // (a `z` waiting for its `h` is forgotten too)
        self.panel.search.iter_mut().for_each(|s| s.typing = false);
        if b.app.vi.mode == Mode::Cmd {
            return;
        }
        let at = self.char_at(x, y);
        self.settle(id, true, out);
        if let (Some(at), Some(b)) = (at, self.bufs.get_mut(self.cur)) {
            put(&mut b.app, at);
            (b.frame, self.drag) = (None, Some((id, false)));
        }
    }

    /// The pointer moved with the left button down. The first move off the char that was pressed
    /// starts Visual mode there (the key `v`: charwise), and from then on the pointer is the
    /// cursor. A pointer outside the text is at its edge. Insert mode and a half-typed command
    /// select nothing, and a selection ended by a key is not taken up again.
    fn drag(&mut self, x: u16, y: u16, out: Out) {
        let Some((id, began)) = self.drag else { return };
        let (at, Some(b)) =
            (self.char_at(x, y), self.bufs.get_mut(self.cur).filter(|b| b.id == id))
        else {
            return;
        };
        let old = b.app.buf.cursor;
        let app = &mut b.app;
        if began && !app.vi.mode.visual() {
            return self.drag = None;
        }
        let Some(at) = at.filter(|_| began || app.vi.idle()) else { return };
        put(app, at);
        b.frame = None;
        if !began && app.buf.cursor != old {
            app.buf.cursor = old;
            self.to(id, Event::Key(Key::Char('v')), out); // the anchor is where the press put the cursor
            if let Some(b) = self.bufs.get_mut(self.cur) {
                put(&mut b.app, at);
                b.frame = None;
            }
            self.drag = Some((id, true));
        }
    }

    /// The wheel over the text scrolls it three lines a notch. The cursor keeps the context
    /// `render` gives it (VIS-12): while `render` pushes the view back to keep it, the cursor
    /// moves towards the view instead, by the lines it was pushed back, keeping its column. A scroll that moves
    /// nothing (the end of the text) leaves the cursor too.
    fn wheel(&mut self, up: bool, out: Out) {
        let Some(b) = self.bufs.get(self.cur) else { return };
        let (id, cmd) = (b.id, b.app.vi.mode == Mode::Cmd);
        if cmd {
            return;
        }
        self.settle(id, false, out);
        let Some(b) = self.bufs.get_mut(self.cur) else { return };
        let app = &mut b.app;
        let (last, cursor, want, top) =
            (app.buf.lines() - 1, app.buf.cursor, app.buf.want, app.top);
        let to = if up { top.saturating_sub(NOTCH) } else { (top + NOTCH).min(last) };
        let mut shown = None;
        for _ in 0..app.size.1 {
            (app.top, app.skip) = (to, 0); // `render` keeps the top it settled on
            shown = Some(view::render(app));
            let l = app.buf.line_of(app.buf.cursor);
            // How far `render` pushed the view back, in lines, is how far the cursor must go (a
            // line takes a row at least, so this is a step towards the view and not past it).
            let (pushed, end) = if up {
                (app.top.saturating_sub(to), l == 0)
            } else {
                (to.saturating_sub(app.top), l == last)
            };
            if pushed == 0 || end {
                break;
            }
            let col = app.buf.want.unwrap_or(app.buf.col());
            let next = if up { l.saturating_sub(pushed) } else { (l + pushed).min(last) };
            let at = app.buf.start(next) + col.min(app.buf.line_len(next));
            (app.buf.cursor, app.buf.want) = (at, Some(col));
            app.clamp();
        }
        let (moved, marks) = (app.top != top, app.buf.marks.len());
        if !moved {
            (app.buf.cursor, app.buf.want) = (cursor, want);
        }
        app.buf.see();
        // The last frame of the loop is the one to show, unless the cursor went back or a mark went.
        b.frame = shown.filter(|_| moved && app.buf.marks.len() == marks);
    }

    /// The wheel over the panel scrolls its rows three a notch, and its cursor stays on the screen.
    fn roll(&mut self, up: bool) {
        let (rows, n) = (usize::from(self.geo().rows), self.view().visible());
        let p = &mut self.panel;
        p.top = if up {
            p.top.saturating_sub(NOTCH)
        } else {
            (p.top + NOTCH).min(n.saturating_sub(rows))
        };
        let hi = (p.top + rows - 1).min(n.saturating_sub(1));
        p.cur = p.cur.min(hi).max(p.top.min(hi));
    }
}
