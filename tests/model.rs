//! PROP-1, PROP-2, PROP-8, SAFE-6, SAFE-7: a model of the shell (one event channel, a save thread, a watcher, an
//! outside process) driving `App::update` with random, interleaved events. Words are the unit of
//! account: `b*` come from the first file, `e*` are written by the outside process, `m*` are typed
//! by the user. `check_merge` holds what every merge must keep, `check_end` what must be true when
//! everything has settled: nobody's words are lost, a clean buffer is the file, a deleted file is
//! told. `SI_FUZZ_CASES` sets the number of cases of every property.
#![allow(
    clippy::cmp_owned,
    clippy::type_complexity,
    clippy::manual_contains,
    clippy::unwrap_used,
    clippy::single_range_in_vec_init
)]
mod common;
use common::*;
use proptest::prelude::*;
use si::core::app::{App, Effect, Event, Key};
use si::core::buffer::{ABSENT, Buffer, Hunk, hash};
use si::core::vi::Mode;
use std::collections::{HashSet, VecDeque};

type Words = HashSet<String>;

fn words_of(s: &str) -> Words {
    s.split_whitespace().map(String::from).collect()
}

fn lines_of(s: &str) -> Words {
    s.split('\n').filter(|l| !l.is_empty()).map(String::from).collect()
}

#[derive(Clone, Debug)]
pub enum U {
    Append,
    OpenBelow,
    OpenAbove,
    InsertStart,
    Type,
    Esc,
    Move(u8),
    DelLine,
    Join,
    YankPut(bool),
    Undo,
    Redo,
    Save,
    ForceSave,
    Visual(bool),
    Colon,
    Subst,
    EditBang,
    Paste,
    /// REVIEW (PROP-8): `]c`, `[c`, `y`, `n`, `Y`, `N`.
    Review(u8),
    /// SAFE-7: `:q`, `ZZ`, `:x`, `:wq` and Ctrl-s, none of which may lose a hunk.
    Leave(u8),
}

#[derive(Clone, Debug)]
pub enum E {
    Insert(usize),
    Delete(usize),
    Append(usize),
    Rewrite(usize),
    Revert(usize),
    Format(u8),
    Remove,
    Touch,
    /// A word is added to the line the user's cursor is on: a conflict, when they edit it too.
    Hit,
}

#[derive(Clone, Debug)]
pub enum Op {
    U(U),
    E(E),
    Deliver,
    Exec,
    Send,
    Read,
    WSend,
    Tick,
}

pub fn op(
    allow_del: bool,
    allow_undo: bool,
    allow_edit_bang: bool,
    review: bool,
) -> impl Strategy<Value = Op> {
    let mut us = vec![
        U::Append,
        U::OpenBelow,
        U::OpenAbove,
        U::InsertStart,
        U::Type,
        U::Esc,
        U::Join,
        U::YankPut(true),
        U::YankPut(false),
        U::Save,
        U::Visual(true),
        U::Visual(false),
        U::Colon,
        U::Subst,
        U::Paste,
    ];
    us.extend((0..6).map(U::Move));
    if allow_del {
        us.push(U::DelLine);
    }
    if allow_undo {
        us.extend([U::Undo, U::Redo]);
    }
    if allow_edit_bang {
        us.push(U::EditBang);
    }
    if review {
        // The hands of a person going through proposals: mostly `]c` and decisions.
        us.extend((0..10).flat_map(|k| [U::Review(k), U::Review(k)]));
        us.extend((0..5).map(U::Leave));
        us.push(U::ForceSave);
    }
    let es = prop_oneof![
        (0..12usize).prop_map(E::Insert),
        (0..12usize).prop_map(E::Delete),
        (0..12usize).prop_map(E::Append),
        (1..4usize).prop_map(E::Rewrite),
        (0..3u8).prop_map(E::Format),
        Just(E::Remove),
        Just(E::Touch),
        Just(E::Hit),
        Just(E::Hit),
    ];
    // A person going through proposals waits for the screen to catch up between keys.
    let deliver = if review { 30 } else { 8 };
    prop_oneof![
        10 => prop::sample::select(us).prop_map(Op::U),
        4 => es.prop_map(Op::E),
        deliver => Just(Op::Deliver),
        2 => Just(Op::Exec),
        2 => Just(Op::Send),
        3 => Just(Op::Read),
        3 => Just(Op::WSend),
        2 => Just(Op::Tick),
    ]
}

struct Pre {
    mode: Mode,
    waiting: bool,
    hunks: Vec<Hunk>,
    text: String,
    base: String,
    cur_line: String,
    version: u64,
    all_words: Words,
}

pub struct Sim {
    /// The shell serialises a save's rename+answer and the watcher's read+answer, so the order of
    /// events in the channel is the order in which things happened to the file.
    pub atomic: bool,
    pub app: App,
    pub disk: Option<Vec<u8>>,
    last_disk: Vec<u8>,
    history: Vec<Vec<u8>>,
    flag: bool,
    /// The save in flight is a `:w!`.
    forced: bool,
    held: Option<Option<Vec<u8>>>,
    chan: VecDeque<Event>,
    job: Option<(Vec<u8>, Option<u64>)>,
    done: Option<Event>,
    n: usize,
    /// The shell's tick clock, and the version of the buffer the journal last got.
    tick_ms: u64,
    snapped: Option<u64>,
    ever: Words,
    ext_last: Words,
    user_removed: Words,
    pub log: Vec<String>,
}

fn cur_line(a: &App) -> String {
    let b = &a.buf;
    b.text.line(b.line_of(b.cursor)).to_string().trim_end_matches('\n').to_string()
}

fn hunk_words(a: &App) -> Words {
    a.buf.hunks.iter().flat_map(|h| h.theirs.split_whitespace().map(String::from)).collect()
}

fn all_words(a: &App) -> Words {
    let mut w = words_of(&text(a));
    w.extend(hunk_words(a));
    w
}

impl Sim {
    pub fn new(first: usize, name: &str) -> Sim {
        let t: String = (0..first).map(|i| format!("b{i}\n")).collect();
        let app = app_with(Some(name), t.as_bytes(), (80, 12));
        let mut s = Sim {
            atomic: false,
            app,
            disk: Some(t.clone().into_bytes()),
            last_disk: t.clone().into_bytes(),
            history: vec![t.clone().into_bytes()],
            flag: false,
            forced: false,
            held: None,
            chan: VecDeque::new(),
            job: None,
            done: None,
            n: 0,
            tick_ms: 0,
            snapped: None,
            ever: lines_of(&t),
            ext_last: Words::new(),
            user_removed: Words::new(),
            log: vec![],
        };
        s.log.push(format!("start {t:?}"));
        s
    }

    fn tok(&mut self, p: char) -> String {
        self.n += 1;
        format!("{p}{}", self.n)
    }

    fn set_disk(&mut self, bytes: Option<Vec<u8>>) {
        if let Some(b) = &bytes {
            self.ever.extend(lines_of(&Buffer::decode(b).text.to_string()));
            self.last_disk = b.clone();
            self.history.push(b.clone());
        }
        self.disk = bytes;
        self.flag = true;
    }

    fn effects(&mut self, fx: Vec<Effect>) {
        for e in fx {
            match e {
                Effect::Write { bytes, expect_hash, .. } => {
                    assert!(self.job.is_none() && self.done.is_none(), "two saves in flight");
                    self.forced = expect_hash.is_none();
                    self.job = Some((bytes, expect_hash));
                }
                // The shell watches anew, and the new watcher thread reads the file once, as it
                // reads after any change: after everything done so far, before what comes next.
                Effect::Watch(_) => self.flag = true,
                Effect::Open(path) => {
                    let r = Ok(self.disk.clone());
                    self.chan.push_back(Event::Opened(path, r));
                }
                _ => {}
            }
        }
    }

    pub fn user(&mut self, u: &U) {
        let t = self.tok('m');
        let s = match u {
            U::Append => format!("A {t}\x1b"),
            U::OpenBelow => format!("o{t}\x1b"),
            U::OpenAbove => format!("O{t}\x1b"),
            U::InsertStart => "A ".into(),
            U::Type => format!("{t} "),
            U::Esc => "\x1b".into(),
            U::Move(0) => "j".into(),
            U::Move(1) => "k".into(),
            U::Move(2) => "gg".into(),
            U::Move(3) => "G".into(),
            U::Move(4) => "jj".into(),
            U::Move(_) => "kk".into(),
            U::DelLine => "dd".into(),
            U::Join => "J".into(),
            U::YankPut(true) => "yyp".into(),
            U::YankPut(false) => "yyP".into(),
            U::Undo => "u".into(),
            U::Redo => "<C-r>".into(),
            U::Save => ":w\n".into(),
            U::ForceSave => ":w!\n".into(),
            U::Visual(true) => "V".into(),
            U::Visual(false) => "v".into(),
            U::Colon => ":".into(),
            U::Subst => "s/b/B/g\n".into(),
            U::EditBang => ":e!\n".into(),
            // (An Esc first, so that Insert mode and a half-typed command do not eat the keys.)
            U::Review(k) => [
                "<Esc>]c", "<Esc>[c", "y", "n", "Y", "N", "<Esc>]cy", "<Esc>]cn", "<Esc>]cY",
                "<Esc>]cN",
            ][usize::from(*k)]
            .into(),
            U::Leave(k) => {
                ["<Esc>:q\n", "<Esc>ZZ", "<Esc>:x\n", "<Esc>:wq\n", "<C-s>"][usize::from(*k)].into()
            }
            U::Paste => {
                self.chan.push_back(Event::Paste(format!("{t}\n{t}x")));
                self.log.push(format!("user {u:?}"));
                return;
            }
        };
        self.log.push(format!("user {u:?} {s:?}"));
        for k in keys(&s) {
            self.chan.push_back(Event::Key(k));
        }
    }

    fn ext_content(&self) -> (Vec<String>, bool, bool, bool) {
        let cur = self.disk.clone().unwrap_or_else(|| self.last_disk.clone());
        let bom = cur.starts_with(b"\xEF\xBB\xBF");
        let body = String::from_utf8_lossy(&cur[if bom { 3 } else { 0 }..]).to_string();
        let crlf = body.contains("\r\n");
        let body = body.replace("\r\n", "\n");
        let nl = body.ends_with('\n');
        let t = body.strip_suffix('\n').unwrap_or(&body);
        let ls = if body.is_empty() { vec![] } else { t.split('\n').map(String::from).collect() };
        (ls, bom, crlf, nl)
    }

    fn encode(ls: &[String], bom: bool, crlf: bool, nl: bool) -> Vec<u8> {
        let mut s = ls.join("\n");
        if nl && !ls.is_empty() {
            s.push('\n');
        }
        if crlf {
            s = s.replace('\n', "\r\n");
        }
        let mut out = if bom { b"\xEF\xBB\xBF".to_vec() } else { vec![] };
        out.extend(s.into_bytes());
        out
    }

    pub fn ext(&mut self, e: &E) {
        let (mut ls, mut bom, mut crlf, mut nl) = self.ext_content();
        self.log.push(format!("ext {e:?} on {ls:?} bom={bom} crlf={crlf} nl={nl}"));
        let tok = self.tok('e');
        let new = match e {
            E::Insert(p) => {
                let p = p % (ls.len() + 1);
                ls.insert(p, tok);
                true
            }
            E::Delete(p) if !ls.is_empty() => {
                let p = p % ls.len();
                ls.remove(p);
                true
            }
            E::Append(p) if !ls.is_empty() => {
                let p = p % ls.len();
                ls[p] = format!("{} {tok}", ls[p]);
                true
            }
            E::Hit if !ls.is_empty() => {
                let b = &self.app.buf;
                let p = b.line_of(b.cursor).min(ls.len() - 1);
                ls[p] = format!("{} {tok}", ls[p]);
                true
            }
            E::Rewrite(k) => {
                ls = (0..*k).map(|i| format!("{tok}x{i}")).collect();
                true
            }
            E::Revert(i) => {
                let h = self.history[i % self.history.len()].clone();
                self.ext_last = words_of(&Buffer::decode(&h).text.to_string());
                self.set_disk(Some(h));
                return;
            }
            E::Format(0) => {
                bom = !bom;
                true
            }
            E::Format(1) => {
                crlf = !crlf;
                true
            }
            E::Format(_) => {
                nl = !nl;
                true
            }
            E::Remove => {
                self.ext_last.clear();
                self.set_disk(None);
                return;
            }
            E::Touch => true,
            _ => false,
        };
        if new {
            let bytes = Self::encode(&ls, bom, crlf, nl);
            self.ext_last = words_of(&Buffer::decode(&bytes).text.to_string());
            self.set_disk(Some(bytes));
        }
    }

    /// The save thread: compare-and-swap and write, in one step.
    pub fn exec(&mut self) {
        if self.done.is_some() {
            return;
        }
        let Some((bytes, expect)) = self.job.take() else { return };
        let have = self.disk.as_deref().map_or(ABSENT, hash);
        self.log.push(format!("save thread: expect {expect:?} have {have:#x} writes {bytes:?}"));
        if expect.is_none() {
            // `:w!` is an explicit overwrite: what was on the disk is the user's to drop, and so is
            // what the file held earlier, which a read that is still on its way may bring in (an
            // outside deletion of a line, merged after the save has put the line back).
            let past = self.history.iter().chain(&self.disk);
            let gone = past.flat_map(|d| words_of(&Buffer::decode(d).text.to_string()));
            self.user_removed.extend(gone.collect::<Vec<_>>());
        }
        self.done = Some(match expect {
            Some(w) if w != have => Event::SaveConflict(self.disk.clone(), far()),
            _ => {
                let h = hash(&bytes);
                self.set_disk(Some(bytes));
                Event::Saved(Ok((h, None)))
            }
        });
    }

    pub fn send(&mut self) {
        if let Some(ev) = self.done.take() {
            self.log.push(format!("save thread sends {ev:?}"));
            self.chan.push_back(ev);
        }
    }

    pub fn read(&mut self) {
        if self.flag && self.held.is_none() {
            self.held = Some(self.disk.clone());
            self.flag = false;
            self.log.push(format!("watcher reads {:?}", self.disk));
        }
    }

    pub fn wsend(&mut self) {
        if let Some(bytes) = self.held.take() {
            self.log.push("watcher sends".into());
            self.chan.push_back(Event::Disk { bytes, hash: None, deadline: far() });
        }
    }

    pub fn structure(&mut self) -> Result<(), String> {
        let a = &mut self.app;
        let b = &a.buf;
        let mut last = None;
        for m in b.marks.iter() {
            if !(last.is_none_or(|l| l < m.line) && m.line < b.lines()) {
                return Err(format!("marks unsorted or out of range: {:?}", b.marks));
            }
            last = Some(m.line);
            if matches!(m.kind, '+' | '~') {
                let len = b.line_len(m.line);
                if words(m).iter().flatten().any(|r| r.start > r.end || r.end > len) {
                    return Err(format!("mark words outside the line: {m:?} len {len}"));
                }
            }
        }
        let mut prev = 0;
        for h in b.hunks.iter() {
            if !(h.mine.start <= h.mine.end && h.mine.end <= b.lines() && prev <= h.mine.start) {
                return Err(format!(
                    "hunk out of order or range: {:?} in {} lines",
                    b.hunks.iter().map(|h| h.mine.clone()).collect::<Vec<_>>(),
                    b.lines()
                ));
            }
            prev = h.mine.start;
        }
        if b.cursor > b.text.len_chars() {
            return Err("cursor past the end".into());
        }
        drop(screen(a));
        Ok(())
    }

    fn pre(&self) -> Pre {
        Pre {
            mode: self.app.vi.mode,
            waiting: self.app.vi.waiting(),
            hunks: self.app.buf.hunks.to_vec(),
            text: text(&self.app),
            base: self.app.buf.base.to_string(),
            cur_line: cur_line(&self.app),
            version: self.app.buf.version(),
            all_words: all_words(&self.app),
        }
    }

    /// Delivers the next event of the channel to the app, checking what it must do.
    pub fn deliver(&mut self) -> Result<(), String> {
        let Some(ev) = self.chan.pop_front() else { return Ok(()) };
        let d = match &ev {
            Event::Disk { bytes: Some(b), .. } | Event::SaveConflict(Some(b), _) => Some(b.clone()),
            _ => None,
        };
        let decision = match &ev {
            Event::Key(Key::Char(c @ ('y' | 'n' | 'Y' | 'N'))) => Some(c.to_string()),
            _ => None,
        };
        let saved = matches!(ev, Event::Saved(Ok(_)));
        let user = matches!(ev, Event::Key(_) | Event::Paste(_) | Event::Opened(..));
        self.log.push(format!("deliver {ev:?}"));
        let pre = self.pre();
        let tick = matches!(ev, Event::Tick(_));
        let fx = self.app.update(ev);
        let snaps: Vec<_> = fx
            .iter()
            .filter_map(|e| match e {
                Effect::Snapshot { path, base, text } => {
                    Some((path.clone(), base.to_string(), text.to_string()))
                }
                _ => None,
            })
            .collect();
        let unsnap = fx.iter().any(|e| matches!(e, Effect::Unsnapshot));
        // SAFE-7: nothing goes over a conflict with the disk, and nothing quits with a hunk open.
        for e in &fx {
            match e {
                Effect::Write { path, expect_hash: Some(_), .. }
                    if path == "f" && pre.hunks.iter().any(|h| h.disk) =>
                {
                    return Err(format!(
                        "a plain save with a conflict with the disk open: {:?}",
                        pre.hunks
                    ));
                }
                Effect::Quit(0) if !self.app.buf.hunks.is_empty() => {
                    return Err(format!("quit with hunks open: {:?}", self.app.buf.hunks));
                }
                _ => {}
            }
        }
        // PROP-8: a decision is exactly what it says.
        if let (Some(k), Mode::Review, false) = (&decision, pre.mode, pre.waiting) {
            let hunks = self.app.buf.hunks.to_vec();
            check_decision(k, &pre.text, &pre.hunks, &text(&self.app), &hunks)?;
        }
        // SAFE-7: a forced save that is done settles the conflicts with the disk.
        if saved && self.forced && self.app.buf.hunks.iter().any(|h| h.disk) {
            return Err(format!(
                "`:w!` is saved and conflicts with the disk are still open: {:?}",
                self.app.buf.hunks
            ));
        }
        if saved {
            self.forced = false;
        }
        self.effects(fx);
        for (path, base, text_) in &snaps {
            // SAFE-6: a snapshot holds exactly the buffer's path, base and text.
            if *path != self.app.path
                || *base != self.app.buf.base.to_string()
                || *text_ != text(&self.app)
            {
                return Err(format!(
                    "snapshot differs from the buffer: {path:?} base {base:?} text {text_:?} vs {:?} {:?} {:?}",
                    self.app.path,
                    self.app.buf.base.to_string(),
                    text(&self.app)
                ));
            }
            self.snapped = Some(self.app.buf.version());
        }
        if unsnap {
            self.snapped = None;
        }
        if tick
            && self.app.buf.dirty()
            && self.snapped != Some(self.app.buf.version())
            && snaps.is_empty()
        {
            // A tick is 1000 ms after the last one by construction: a dirty buffer that changed since
            // the journal last got it is snapshotted now.
            return Err(format!(
                "dirty buffer not snapshotted on a tick (version {}, journal has {:?})",
                self.app.buf.version(),
                self.snapped
            ));
        }
        if user {
            let after = all_words(&self.app);
            self.user_removed.extend(pre.all_words.difference(&after).cloned());
        }
        if let (Some(d), true) = (d, self.app.buf.version() != pre.version) {
            self.check_merge(&pre, &d)?;
        }
        self.structure()
    }

    fn check_merge(&self, pre: &Pre, d: &[u8]) -> Result<(), String> {
        let a = &self.app;
        let post = text(a);
        let dec = Buffer::decode(d);
        let dtext = dec.text.to_string();
        if (post == dtext) == a.buf.dirty() {
            return Err(format!(
                "clean iff the text is the file's: dirty={} text={post:?} file={dtext:?}",
                a.buf.dirty()
            ));
        }
        if a.buf.base.to_string() != dtext {
            return Err(format!(
                "base is not the disk text: {:?} vs {dtext:?}",
                a.buf.base.to_string()
            ));
        }
        let pw = words_of(&post);
        let hw = hunk_words(a);
        let base_w = words_of(&pre.base);
        for w in words_of(&dtext).difference(&base_w) {
            if !pw.contains(w) && !hw.contains(w) && !self.user_removed.contains(w) {
                return Err(format!("THEIRS lost: {w:?} (post {post:?}, hunks {:?})", a.buf.hunks));
            }
        }
        for w in words_of(&pre.text).difference(&base_w) {
            if !pw.contains(w) {
                return Err(format!("MINE lost: {w:?} (pre {:?}, post {post:?})", pre.text));
            }
        }
        // The cursor keeps pointing at the same text.
        let pre_lines: Vec<&str> = pre.text.split('\n').collect();
        let post_lines: Vec<&str> = post.split('\n').collect();
        let count = |v: &[&str], x: &str| v.iter().filter(|l| **l == x).count();
        if !pre.cur_line.is_empty()
            && count(&pre_lines, &pre.cur_line) == 1
            && count(&post_lines, &pre.cur_line) == 1
        {
            let now = cur_line(a);
            if now != pre.cur_line {
                return Err(format!(
                    "cursor moved off its text: was on {:?}, now on {now:?}; text {post:?}",
                    pre.cur_line
                ));
            }
        }
        // Marks: on lines that came from a disk, and every new line from disk is marked.
        for m in a.buf.marks.iter() {
            if matches!(m.kind, '+' | '~') {
                let l = post_lines[m.line];
                if !l.is_empty() && !self.ever.contains(l) {
                    return Err(format!(
                        "mark {m:?} on a line that never was on a disk: {l:?}; text {post:?}"
                    ));
                }
            }
        }
        let base_l = lines_of(&pre.base);
        let pre_l = lines_of(&pre.text);
        let in_hunk = |i: usize| a.buf.hunks.iter().any(|h| h.mine.contains(&i));
        for (i, l) in post_lines.iter().enumerate() {
            if !l.is_empty()
                && lines_of(&dtext).contains(*l)
                && !base_l.contains(*l)
                && !pre_l.contains(*l)
                && !in_hunk(i)
                && a.buf.marks.binary_search_by_key(&i, |m| m.line).is_err()
            {
                return Err(format!(
                    "new line {l:?} at {i} from the disk is not marked: marks {:?}; text {post:?}",
                    a.buf.marks
                ));
            }
        }
        Ok(())
    }

    pub fn step(&mut self, op: &Op) -> Result<(), String> {
        match op {
            Op::U(u) => self.user(u),
            Op::E(e) => self.ext(e),
            Op::Deliver => return self.deliver(),
            Op::Exec => {
                self.exec();
                if self.atomic {
                    self.send();
                }
            }
            Op::Send if !self.atomic => self.send(),
            Op::Read => {
                self.read();
                if self.atomic {
                    self.wsend();
                }
            }
            Op::WSend if !self.atomic => self.wsend(),
            Op::Send | Op::WSend => {}
            Op::Tick => {
                self.tick_ms += 1000;
                self.chan.push_back(Event::Tick(self.tick_ms));
            }
        }
        Ok(())
    }

    /// Lets every thread finish and the app see every event, then checks the end state.
    pub fn quiesce(&mut self) -> Result<(), String> {
        for _ in 0..200 {
            let busy = self.job.is_some()
                || self.done.is_some()
                || self.flag
                || self.held.is_some()
                || !self.chan.is_empty();
            if !busy {
                break;
            }
            self.exec();
            self.send();
            self.read();
            self.wsend();
            while !self.chan.is_empty() {
                self.deliver()?;
            }
        }
        Ok(())
    }

    pub fn check_end(&self) -> Result<(), String> {
        let a = &self.app;
        match &self.disk {
            None if !a.gone => {} // `:e!` of a file that is not there: a new file
            None => {
                if !a.gone || !a.buf.dirty() {
                    return Err(format!(
                        "the file is gone but the buffer says gone={} dirty={}",
                        a.gone,
                        a.buf.dirty()
                    ));
                }
            }
            Some(d) => {
                let dtext = Buffer::decode(d).text.to_string();
                if a.gone {
                    return Err("buffer says the file is gone, but it is there".into());
                }
                if !a.buf.dirty() && a.buf.encode() != *d {
                    return Err(format!(
                        "CLEAN but differs from the disk: buffer {:?} (would write {:?}) disk {d:?}",
                        text(a),
                        a.buf.encode()
                    ));
                }
                let mut have = words_of(&dtext);
                have.extend(all_words(a));
                for w in &self.ext_last {
                    if !have.contains(w) && !self.user_removed.contains(w) {
                        return Err(format!(
                            "THEIRS lost for good: {w:?} (disk {dtext:?}, buffer {:?})",
                            text(a)
                        ));
                    }
                }
            }
        }
        Ok(())
    }
}

fn run(first: usize, ops: &[Op], atomic: bool) -> Result<(), (String, Vec<String>)> {
    let mut s = Sim::new(first, "f");
    s.atomic = atomic;
    let r = (|| {
        for o in ops {
            s.step(o)?;
        }
        s.quiesce()?;
        s.check_end()
    })();
    r.map_err(|e| (e, s.log.clone()))
}

macro_rules! model {
    ($name:ident, $atomic:expr, $del:expr, $undo:expr, $reload:expr, $review:expr) => {
        proptest! {
            #![proptest_config(ProptestConfig { cases: cases(500), max_shrink_iters: 4000, failure_persistence: None, ..ProptestConfig::default() })]
            #[test]
            fn $name(first in 1usize..5, ops in prop::collection::vec(op($del, $undo, $reload, $review), 1..(if $review { 120 } else { 40 }))) {
                if let Err((e, log)) = run(first, &ops, $atomic) {
                    return Err(TestCaseError::fail(format!("{e}\n{}", log.join("\n"))));
                }
            }
        }
    };
}

// Appends only on the user side, no undo: no word of anyone is lost, ever.
model!(prop_2_fuzz_model_atomic_appends_only, true, false, false, false, false);
model!(prop_2_fuzz_model_atomic_with_deletes, true, true, false, false, false);
model!(prop_2_fuzz_model_atomic_with_undo, true, true, true, false, false);
model!(prop_2_fuzz_model_atomic_with_reload, true, true, true, true, false);
// The same, with the watcher and the save thread free to interleave.
model!(prop_2_fuzz_model_racy_appends_only, false, false, false, false, false);
model!(prop_2_fuzz_model_racy_with_deletes, false, true, false, false, false);
model!(prop_2_fuzz_model_racy_with_undo, false, true, true, false, false);
model!(prop_2_fuzz_model_racy_with_reload, false, true, true, true, false);
// The hands of a person going through proposals (PROP-8), and the refusals of SAFE-7: nothing is
// saved over a conflict or quit away with a hunk open, and every decision does what it says.
model!(prop_8_fuzz_model_atomic_with_review, true, true, true, true, true);
model!(prop_8_fuzz_model_racy_with_review, false, true, true, true, true);
model!(prop_8_fuzz_model_atomic_review_without_undo, true, true, false, false, true);
model!(prop_8_fuzz_model_racy_review_without_undo, false, true, false, false, true);

// ---------------------------------------------------------------------------------------------
// Recovery (SAFE-6): the snapshot is `mine`, the file is `theirs`, the snapshot's base is `base`
// ---------------------------------------------------------------------------------------------

fn edit_lines(base: &[String], ops: &[(u8, usize)], prefix: char, n: &mut usize) -> Vec<String> {
    let mut v = base.to_vec();
    for &(k, pos) in ops {
        *n += 1;
        let tok = format!("{prefix}{n}");
        match k % 3 {
            0 => v.insert(pos % (v.len() + 1), tok),
            1 if !v.is_empty() => {
                v.remove(pos % v.len());
            }
            _ if !v.is_empty() => {
                let p = pos % v.len();
                v[p] = format!("{} {tok}", v[p]);
            }
            _ => {}
        }
    }
    v
}

fn join(v: &[String]) -> String {
    v.join("\n")
}

proptest! {
    #![proptest_config(ProptestConfig { cases: cases(2000), max_shrink_iters: 4000, failure_persistence: None, ..ProptestConfig::default() })]

    /// A snapshot recovered over a file that is the same, changed, emptied or missing: every word
    /// the snapshot added is in the buffer, every word the file added is in the buffer or a hunk, and
    /// when the file is not there the buffer is the snapshot.
    #[test]
    fn safe_6_fuzz_recover_loses_nothing(
        nb in 0usize..5,
        mine_ops in prop::collection::vec((0u8..3, 0usize..8), 0..4),
        disk_ops in prop::collection::vec((0u8..3, 0usize..8), 0..4),
        disk_kind in 0u8..4,
        named in any::<bool>(),
    ) {
        let mut n = 0;
        let base: Vec<String> = (0..nb).map(|i| format!("b{i}")).collect();
        let mine = edit_lines(&base, &mine_ops, 'm', &mut n);
        let theirs = edit_lines(&base, &disk_ops, 'e', &mut n);
        let disk_text = join(&theirs);
        let file: Option<Vec<u8>> = match disk_kind {
            0 => Some(join(&base).into_bytes()),
            1 => Some(disk_text.clone().into_bytes()),
            2 => Some(Vec::new()),
            _ => None,
        };
        let mut a = App::new(named.then(|| "f".to_string()), Ok(file.clone()), (80, 12));
        let (b, t) = (join(&base), join(&mine));
        a.update(Event::Recover(b.clone().into_bytes(), t.clone().into_bytes(), 1000, "snap".into(), far()));
        let post = text(&a);
        let pw = words_of(&post);
        let hw = hunk_words(&a);
        // Mine: every word the snapshot added to its base is in the buffer.
        for w in words_of(&t).difference(&words_of(&b)) {
            prop_assert!(pw.contains(w), "MINE lost: {w:?}: snapshot {t:?} base {b:?} file {file:?} -> buffer {post:?}");
        }
        match &file {
            None if !b.is_empty() => {
                prop_assert_eq!(&post, &t, "the file is not there: the snapshot is the only copy");
                prop_assert!(a.buf.dirty());
            }
            Some(f) => {
                let ft = Buffer::decode(f).text.to_string();
                for w in words_of(&ft).difference(&words_of(&b)) {
                    prop_assert!(pw.contains(w) || hw.contains(w), "THEIRS lost: {w:?}: snapshot {t:?} base {b:?} file {ft:?} -> buffer {post:?}");
                }
            }
            None => {}
        }
        // Whatever the buffer is, a clean one equals the file.
        if !a.buf.dirty() && file.is_some() {
            prop_assert_eq!(a.buf.encode(), file.clone().unwrap(), "clean but not the file");
        }
        // Undo takes the recovery back.
        let before = text(&a);
        feed(&mut a, "u");
        feed(&mut a, "<C-r>");
        prop_assert_eq!(text(&a), before);
    }
}
