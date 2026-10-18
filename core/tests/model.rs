//! DISK-1, DISK-2, FILT-1, FILT-2, DISK-6, SAFE-6, SAFE-7: a model of the shell (one event channel, a save thread, a watcher, an
//! outside process) driving `App::update` with random, interleaved events. Words are the unit of
//! account: `b*` come from the first file, `e*` are written by the outside process, `m*` are typed
//! by the user. `check_merge` holds what every merge must keep, `check_end` what must be true when
//! everything has settled: nobody's words are lost, a clean buffer is the file, a deleted file is
//! told. The journal thread makes the copies of SAFE-7 (`Effect::Keep`) and answers them. A filter
//! (FILT-2) is one more thing the outside does: it starts when the user asks, ends at a random moment
//! with a result of one of a few kinds (it upper-cases the words, changes nothing, prints nothing,
//! fails, is cancelled), and the result must either leave the text exactly as it is or put every word
//! it brought in the text, as one undo step. Every read of the disk is numbered
//! as it starts, from one counter, like the shell does (DISK-1): the watcher's read when it starts
//! (it may be sent long after), a load when `:e` asks, a save's answer when the save is done. A
//! `Disk` that arrives older than something App took in must change nothing, and one that arrives while a save is in flight waits for its answer, which numbers it (the model checks that no read numbered before the answer is merged before it). `SIM_FUZZ_CASES` sets the
//! number of cases of every property.
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
use sim_core::core::app::{App, Effect, Event};
use sim_core::core::buffer::{ABSENT, Buffer, Mark, hash};
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
    /// DISK-6: `]c` and `[c`, the hands of a person going through the marks of a conflict.
    Walk(u8),
    /// SAFE-7: `:q`, `ZZ`, `:x`, `:wq` and Ctrl-s, none of which may write over a conflict.
    Leave(u8),
    /// FILT-1: `:%!f` and friends, and Ctrl-c in Normal mode (which cancels a running filter).
    Filter(u8),
    Cancel,
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
    /// The running filter (if any) ends with a result of this kind.
    FilterEnd(u8),
}

pub fn op(
    allow_del: bool,
    allow_undo: bool,
    allow_edit_bang: bool,
    conflicts: bool,
    filter: bool,
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
    if conflicts {
        // The hands of a person who meets a conflict: walk the marks, try to leave, force a save
        // (and `:e!`, which is allowed above).
        us.extend((0..4).flat_map(|k| [U::Walk(k), U::Walk(k)]));
        us.extend((0..5).map(U::Leave));
        us.extend([U::ForceSave, U::ForceSave, U::EditBang]);
    }
    if filter {
        us.extend((0..5).map(U::Filter));
        us.push(U::Cancel);
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
    // A person meeting a conflict waits for the screen to catch up between keys.
    let deliver = if conflicts { 30 } else { 8 };
    let base = prop_oneof![
        10 => prop::sample::select(us).prop_map(Op::U),
        4 => es.prop_map(Op::E),
        deliver => Just(Op::Deliver),
        2 => Just(Op::Exec),
        2 => Just(Op::Send),
        3 => Just(Op::Read),
        3 => Just(Op::WSend),
        2 => Just(Op::Tick),
    ];
    if filter {
        prop_oneof![10 => base, 1 => (0..6u8).prop_map(Op::FilterEnd)].boxed()
    } else {
        base.boxed()
    }
}

struct Pre {
    diverged: bool,
    text: String,
    base: String,
    cur_line: String,
    version: u64,
    all_words: Words,
    marks: Vec<Mark>,
    msg: Option<String>,
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
    /// The read the watcher holds: its number (taken as it started) and what it saw.
    held: Option<(u64, Option<Vec<u8>>)>,
    /// The shell's one counter of reads of the disk (DISK-1).
    seq: u64,
    /// A save is in flight (App has asked for it and has not got its answer), and the last read of the
    /// disk that came in meanwhile: App keeps it until the answer numbers it (DISK-1).
    saving: bool,
    late: Option<(u64, Option<Vec<u8>>, bool)>,
    /// What the last save wrote: the base of the buffer once its answer is in.
    last_save: Vec<u8>,
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
    /// The words of every copy the journal thread made for SAFE-7, and how many it made.
    kept: Words,
    copies: usize,
    /// The buffer has been diverged at some point (its marks may be on lines of mine).
    clashed: bool,
    /// The input of the filter that runs, the input of the one that last ended, and whether Ctrl-c
    /// has been sent to it (FILT-1).
    filter: Option<String>,
    last_input: Words,
    killed: bool,
    pub log: Vec<String>,
}

fn cur_line(a: &App) -> String {
    let b = &a.buf;
    b.text.line(b.line_of(b.cursor)).to_string().trim_end_matches('\n').to_string()
}

fn all_words(a: &App) -> Words {
    words_of(&text(a))
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
            seq: 0,
            saving: false,
            late: None,
            last_save: vec![],
            chan: VecDeque::new(),
            job: None,
            done: None,
            n: 0,
            tick_ms: 0,
            snapped: None,
            ever: lines_of(&t),
            ext_last: Words::new(),
            user_removed: Words::new(),
            kept: Words::new(),
            copies: 0,
            clashed: false,
            filter: None,
            last_input: Words::new(),
            killed: false,
            log: vec![],
        };
        s.log.push(format!("start {t:?}"));
        s
    }

    /// The next number of a read of the disk, taken as the read starts.
    fn number(&mut self) -> u64 {
        self.seq += 1;
        self.seq
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
                    self.saving = true;
                    self.forced = expect_hash.is_none();
                    self.job = Some((bytes, expect_hash));
                }
                // The shell watches anew, and the new watcher thread reads the file once, as it
                // reads after any change: after everything done so far, before what comes next.
                Effect::Watch(_) => self.flag = true,
                Effect::Open(path) => {
                    let (r, seq) = (Ok(self.disk.clone()), self.number());
                    self.chan.push_back(Event::Opened(path, r, seq));
                }
                Effect::Spawn { input, .. } => {
                    assert!(self.filter.is_none(), "a second filter was started");
                    self.filter = Some(String::from_utf8(input).unwrap());
                    self.killed = false;
                }
                Effect::Kill => self.killed = true,
                // The journal thread copies the bytes (SAFE-7) and answers in its own time: the
                // answer joins the channel.
                Effect::Keep { bytes, .. } => {
                    self.kept.extend(words_of(&Buffer::decode(&bytes).text.to_string()));
                    self.copies += 1;
                    let at = format!("/state/f.{}", self.copies);
                    self.log.push(format!("journal keeps {at}"));
                    self.chan.push_back(Event::Kept(Ok(at)));
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
            U::Walk(k) => ["<Esc>]c", "<Esc>[c", "<Esc>]c]c", "<Esc>[c[c"][usize::from(*k)].into(),
            U::Leave(k) => {
                ["<Esc>:q\n", "<Esc>ZZ", "<Esc>:x\n", "<Esc>:wq\n", "<C-s>"][usize::from(*k)].into()
            }
            U::Filter(k) => {
                ["<Esc>:%!f\n", "<Esc>:.!f\n", "<Esc>:1,2!f\n", "<Esc>Vj!f\n", "<Esc>:$!f\n"]
                    [usize::from(*k)]
                .into()
            }
            U::Cancel => "<Esc><C-c>".into(),
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
        // The compare and the rename are one step here, so the answer's number is that step's.
        let seq = self.number();
        self.done = Some(match expect {
            Some(w) if w != have => Event::SaveConflict(self.disk.clone(), far(), seq),
            _ => {
                let h = hash(&bytes);
                self.last_save.clone_from(&bytes);
                self.set_disk(Some(bytes));
                Event::Saved(Ok((h, None)), seq)
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
            self.held = Some((self.number(), self.disk.clone()));
            self.flag = false;
            self.log.push(format!("watcher reads {:?}", self.disk));
        }
    }

    pub fn wsend(&mut self) {
        if let Some((seq, bytes)) = self.held.take() {
            self.log.push("watcher sends".into());
            self.chan.push_back(Event::Disk { bytes, hash: None, deadline: far(), seq });
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
        if b.cursor > b.text.len_chars() {
            return Err("cursor past the end".into());
        }
        drop(screen(a));
        Ok(())
    }

    fn pre(&self) -> Pre {
        Pre {
            diverged: self.app.diverged(),
            msg: self.app.msg.clone(),
            text: text(&self.app),
            base: self.app.buf.base.to_string(),
            cur_line: cur_line(&self.app),
            version: self.app.buf.version(),
            all_words: all_words(&self.app),
            marks: self.app.buf.marks.to_vec(),
        }
    }

    /// The running filter ends with a result of kind `k`: the words upper-cased, nothing changed,
    /// nothing printed, a failure, a signal, or the words upper-cased with a new one and no final
    /// line break. The answer joins the other events in the channel (FILT-2).
    fn filter_end(&mut self, k: u8) {
        let Some(input) = self.filter.take() else { return };
        self.last_input = words_of(&input);
        let tok = self.tok('f');
        let up = input.to_uppercase();
        let (status, out, err) = match k {
            0 => (Some(0), up, ""),
            1 => (Some(0), input, ""),
            2 => (Some(0), String::new(), ""),
            3 => (Some(1), up, "oops\nmore"),
            4 => (None, String::new(), ""),
            _ => (Some(0), format!("{}{tok}", up.trim_end()), ""),
        };
        self.log.push(format!("filter ends {status:?} {out:?}"));
        let (stdout, stderr) = (out.into_bytes(), err.as_bytes().to_vec());
        self.chan.push_back(Event::FilterDone(status, stdout, stderr, far()));
    }

    /// FILT-2: a result that fails or is cancelled changes nothing; any other leaves the text as it
    /// is (nothing to apply, or it overlaps edits made meanwhile and says so) or puts every word it
    /// brought in the text, in one undo step, with every word mine had added still there.
    fn check_filter(&mut self, pre: &Pre, status: Option<i32>, out: &str) -> Result<(), String> {
        let a = &self.app;
        let cancelled = std::mem::take(&mut self.killed);
        let same = text(a) == pre.text && a.buf.marks.to_vec() == pre.marks;
        if status != Some(0) || cancelled {
            return if same && a.buf.version() == pre.version {
                Ok(())
            } else {
                Err(format!(
                    "a filter that failed or was cancelled changed the buffer: {:?}",
                    text(a)
                ))
            };
        }
        if same {
            return Ok(()); // nothing to apply, or discarded because the lines changed meanwhile
        }
        if a.buf.version() == pre.version {
            return Err("the text changed but the version did not".into());
        }
        let have = all_words(a);
        // A word the filter only passed on is not a change: the buffer may have lost it to the disk
        // meanwhile. A word it brought is in the text.
        for w in words_of(out).difference(&self.last_input) {
            if !have.contains(w) {
                return Err(format!("the filter's word {w:?} is not in the text: {:?}", text(a)));
            }
        }
        for w in words_of(&pre.text).difference(&self.last_input) {
            if !have.contains(w) && !words_of(&pre.base).contains(w) {
                return Err(format!("mine lost to a filter's result: {w:?} {:?}", text(a)));
            }
        }
        Ok(())
    }

    /// Delivers the next event of the channel to the app, checking what it must do.
    pub fn deliver(&mut self) -> Result<(), String> {
        let Some(ev) = self.chan.pop_front() else { return Ok(()) };
        let d = match &ev {
            Event::Disk { bytes: Some(b), .. } | Event::SaveConflict(Some(b), ..) => {
                Some(b.clone())
            }
            _ => None,
        };
        let loaded = matches!(ev, Event::Opened(..));
        let fdone = match &ev {
            Event::FilterDone(status, out, ..) => {
                Some((*status, String::from_utf8_lossy(out).to_string()))
            }
            _ => None,
        };
        let saved = matches!(ev, Event::Saved(Ok(_), _));
        // The answer of a save, with its number: what App merges when it comes in may be the read that
        // came in before it (DISK-1).
        let answer = match &ev {
            Event::Saved(_, n) | Event::SaveConflict(_, _, n) => Some(*n),
            _ => None,
        };
        let came_in = matches!(ev, Event::Disk { .. }) && self.saving;
        if let Event::Disk { bytes, seq, .. } = &ev
            && self.saving
        {
            self.late = Some((*seq, bytes.clone(), false));
        }
        // (A filter's result is something the user asked for: what it takes out is theirs to drop.)
        let user = matches!(
            ev,
            Event::Key(_) | Event::Paste(_) | Event::Opened(..) | Event::FilterDone(..)
        );
        self.log.push(format!("deliver {ev:?}"));
        let mut pre = self.pre();
        let tick = matches!(ev, Event::Tick(_));
        let fx = self.app.update(ev);
        if let Some((status, out)) = fdone {
            self.check_filter(&pre, status, &out)?;
        }
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
        // SAFE-7: nothing is written over a conflict with the disk but by a forced save, which
        // asked for the copy first, and a plain save is refused while one is open.
        for e in &fx {
            if let Effect::Write { path, expect_hash: Some(_), .. } = e
                && path == "f"
                && pre.diverged
            {
                return Err("a plain save with a conflict with the disk open".into());
            }
        }
        // SAFE-7: a forced save that is done settles the conflict, and a reload that discarded mine
        // while it conflicted has a copy of every word of it.
        // (a read that is newer than the answer was taken in after it: that conflict is a new one)
        let newer = answer.zip(self.late.as_ref()).is_some_and(|(n, (seq, ..))| *seq > n);
        if saved && self.forced && self.app.diverged() && !newer {
            return Err("`:w!` is saved and the conflict with the disk is still open".into());
        }
        if loaded && pre.diverged && !self.app.diverged() && text(&self.app) != pre.text {
            let theirs = words_of(&pre.base);
            for w in words_of(&pre.text).difference(&theirs) {
                if !self.kept.contains(w) && !self.user_removed.contains(w) && pre.msg.is_none() {
                    return Err(format!("a reload threw away {w:?} of mine without a copy"));
                }
            }
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
        self.clashed |= self.app.diverged();
        let mut d = d;
        if came_in
            && self.app.buf.version() != pre.version
            && let Some(l) = self.late.as_mut()
        {
            l.2 = true;
        }
        if let Some(n) = answer {
            self.saving = false;
            if let Some((seq, _, true)) = self.late
                && seq < n
            {
                return Err(format!(
                    "a read numbered {seq} was merged while the save was in flight, and its answer is numbered {n}"
                ));
            }
            // A read that came in while the save was in flight and is older than its answer must change
            // nothing; a newer one is taken in then, and is the file that the merge is checked against.
            if let Some((seq, Some(late), _)) = self.late.take()
                && self.app.buf.version() != pre.version
                && d.as_ref().is_none_or(|d| {
                    Buffer::decode(d).text.to_string() != self.app.buf.base.to_string()
                })
                && Buffer::decode(&late).text.to_string() == self.app.buf.base.to_string()
            {
                if seq < n {
                    return Err(format!(
                        "a read numbered {seq} was taken in after the answer numbered {n}"
                    ));
                }
                d = Some(late);
                // (the answer of a save that was done puts its text as the base before the read is merged)
                if saved {
                    pre.base = Buffer::decode(&self.last_save).text.to_string();
                }
            }
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
        let base_w = words_of(&pre.base);
        // Theirs: in the buffer, or the buffer is diverged and the disk's version is kept (SAFE-7).
        for w in words_of(&dtext).difference(&base_w) {
            if !pw.contains(w) && !a.diverged() && !self.user_removed.contains(w) {
                return Err(format!("THEIRS lost: {w:?} (post {post:?})"));
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
        // Marks: on lines that came from a disk (or, once the buffer has been diverged, on a line of
        // mine that conflicted with it), and every new line from disk is marked.
        for m in a.buf.marks.iter().filter(|_| !self.clashed && !a.diverged()) {
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
        for (i, l) in post_lines.iter().enumerate() {
            if !l.is_empty()
                && lines_of(&dtext).contains(*l)
                && !base_l.contains(*l)
                && !pre_l.contains(*l)
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
            Op::FilterEnd(k) => self.filter_end(*k),
        }
        Ok(())
    }

    /// Lets every thread finish and the app see every event, then checks the end state.
    pub fn quiesce(&mut self) -> Result<(), String> {
        for _ in 0..200 {
            self.filter_end(0);
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
                have.extend(self.kept.iter().cloned());
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
    ($name:ident, $atomic:expr, $del:expr, $undo:expr, $reload:expr, $conflicts:expr, $filter:expr) => {
        proptest! {
            #![proptest_config(ProptestConfig { cases: cases(500), max_shrink_iters: 4000, failure_persistence: None, ..ProptestConfig::default() })]
            #[test]
            fn $name(first in 1usize..5, ops in prop::collection::vec(op($del, $undo, $reload, $conflicts, $filter), 1..(if $conflicts || $filter { 120 } else { 40 }))) {
                if let Err((e, log)) = run(first, &ops, $atomic) {
                    return Err(TestCaseError::fail(format!("{e}\n{}", log.join("\n"))));
                }
            }
        }
    };
}

// Appends only on the user side, no undo: no word of anyone is lost, ever.
model!(disk_2_fuzz_model_atomic_appends_only, true, false, false, false, false, false);
model!(disk_2_fuzz_model_atomic_with_deletes, true, true, false, false, false, false);
model!(disk_2_fuzz_model_atomic_with_undo, true, true, true, false, false, false);
model!(disk_2_fuzz_model_atomic_with_reload, true, true, true, true, false, false);
// The same, with the watcher and the save thread free to interleave.
model!(disk_2_fuzz_model_racy_appends_only, false, false, false, false, false, false);
model!(disk_2_fuzz_model_racy_with_deletes, false, true, false, false, false, false);
model!(disk_2_fuzz_model_racy_with_undo, false, true, true, false, false, false);
model!(disk_2_fuzz_model_racy_with_reload, false, true, true, true, false, false);
// The hands of a person who meets a conflict (SAFE-7): nothing is written over it but by a forced
// save that has the disk's version copied first, `:e!` copies mine first, and the words of both sides
// are in the buffer, on the disk or in a copy at every moment.
model!(safe_7_fuzz_model_atomic_with_conflicts, true, true, true, true, true, false);
model!(safe_7_fuzz_model_racy_with_conflicts, false, true, true, true, true, false);
model!(safe_7_fuzz_model_atomic_conflicts_without_undo, true, true, false, false, true, false);
model!(safe_7_fuzz_model_racy_conflicts_without_undo, false, true, false, false, true, false);
// Filters (FILT-1, FILT-2): they start on request and end at any moment between the keys, the saves
// and the disk's changes, with every kind of result; Ctrl-c cancels them.
model!(filt_2_fuzz_model_atomic_with_filters, true, true, true, true, true, true);
model!(filt_2_fuzz_model_racy_with_filters, false, true, true, true, true, true);
model!(filt_2_fuzz_model_atomic_filters_without_conflicts, true, true, true, false, false, true);
model!(filt_2_fuzz_model_racy_filters_appends_only, false, false, false, false, false, true);

proptest! {
    #![proptest_config(ProptestConfig { cases: cases(500), max_shrink_iters: 4000, failure_persistence: None, ..ProptestConfig::default() })]

    /// A read of the watcher that is held up (DISK-1): it is taken first, the outside process writes
    /// again, and everything else happens before it is sent last, and the user saves once more (ZZ): reloads,
    /// saves, conflicts, filters, newer reads. App must drop it as the older news it is. (The random
    /// models above hit this only about once in 100,000 cases, the replay below is such a case.)
    #[test]
    fn disk_1_fuzz_model_a_read_held_across_reloads_and_saves(
        first in 1usize..5,
        mid in prop::collection::vec(
            op(true, true, true, true, true)
                .prop_map(|o| if matches!(o, Op::Read | Op::WSend) { Op::Deliver } else { o }),
            1..100,
        ),
    ) {
        let (head, tail) = ([Op::E(E::Append(0)), Op::Read], [Op::Exec, Op::WSend, Op::Send, Op::U(U::Leave(1))]);
        let ops: Vec<Op> = head.into_iter().chain(mid).chain(tail).collect();
        if let Err((e, log)) = run(first, &ops, false) {
            return Err(TestCaseError::fail(format!("{e}\n{}", log.join("\n"))));
        }
    }
}

proptest! {
    #![proptest_config(ProptestConfig { cases: cases(500), max_shrink_iters: 4000, failure_persistence: None, ..ProptestConfig::default() })]

    /// Reads of the disk that come in while a save is in flight (DISK-1): Ctrl-s is delivered, and then
    /// the outside process, the watcher, the keys and the answers of other things go on, and the save
    /// itself runs (and answers) last. App cannot know the number of the answer until it is in, so what
    /// came in meanwhile must wait for it: a read older than the answer changes nothing, a newer one is
    /// merged after it (the model checks both, `deliver`).
    #[test]
    fn disk_1_fuzz_model_reads_that_come_in_while_a_save_is_in_flight(
        first in 1usize..5,
        atomic in any::<bool>(),
        mid in prop::collection::vec(
            op(true, true, false, true, false)
                .prop_map(|o| if matches!(o, Op::Exec | Op::Send) { Op::Deliver } else { o }),
            1..60,
        ),
    ) {
        let (head, tail) = (
            [Op::U(U::Leave(4)), Op::Deliver],
            [Op::Exec, Op::Send, Op::Deliver, Op::Deliver, Op::Deliver, Op::U(U::Leave(1))],
        );
        let ops: Vec<Op> = head.into_iter().chain(mid).chain(tail).collect();
        if let Err((e, log)) = run(first, &ops, atomic) {
            return Err(TestCaseError::fail(format!("{e}\n{}", log.join("\n"))));
        }
    }
}

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
    /// the snapshot added is in the buffer, every word the file added is in the buffer or the buffer
    /// is diverged (the file keeps it), and when the file is not there the buffer is the snapshot.
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
                    prop_assert!(pw.contains(w) || a.diverged(), "THEIRS lost: {w:?}: snapshot {t:?} base {b:?} file {ft:?} -> buffer {post:?}");
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

/// A race found by a long run of the racy model (about 1 case in 100,000), as old as M3. The
/// watcher read the file before the outside process wrote it twice more, and sends that old read
/// after `:e!` has loaded the newer file and `:wq` has saved it. Without numbers App could not tell
/// that read from a change, took it in, and the next save lost the outside words. Now the shell
/// numbers every read of the disk as it starts, and App drops a read older than what it took in
/// already (DISK-1); the model numbers its reads the same way.
#[test]
fn disk_2_model_replay_a_stale_watcher_read_after_a_reload_and_a_save() {
    use Op::{Deliver, Exec, Read, Send, WSend};
    let (ext, user) = (|e| Op::E(E::Append(e)), |u| Op::U(u));
    #[rustfmt::skip]
    let ops = [
        ext(0), Read, user(U::Visual(true)), user(U::Walk(0)), user(U::EditBang), Deliver, ext(0), Deliver,
        ext(0), Deliver, Deliver, Deliver, Deliver, Deliver, Deliver, user(U::Leave(3)), Deliver, Deliver,
        Deliver, Deliver, Deliver, Deliver, Exec, WSend, Send, user(U::Leave(1)),
    ];
    if let Err((e, log)) = run(1, &ops, false) {
        panic!("{e}\n{}", log.join("\n"));
    }
}

/// Found by the atomic model with conflicts (SAFE-7) in a check on another machine, and as old as the
/// numbers of DISK-1. The watcher read the file ("e4", a line the outside process added) before the
/// outside process removed it again; Ctrl-s was typed before that read was sent, so App has asked for the
/// save when the read comes in, and the save (which found no file, as it expected, and made it) answers
/// after it. The read came before the answer in the channel, and App took it for news, merged "e4" over
/// the line "b0" of the user's text that the save was putting on the disk, and ZZ then wrote "e4" over
/// "b0", which the outside process had touched in the meantime: nowhere was "b0" left. Now a read that comes
/// in while a save is in flight waits for its answer, which numbers it (DISK-1).
#[test]
fn disk_1_model_replay_a_read_that_comes_in_while_a_save_is_in_flight_and_is_older_than_its_answer()
{
    use Op::{Deliver, Exec, Read};
    let (ext, user) = (|e| Op::E(e), |u| Op::U(u));
    #[rustfmt::skip]
    let ops = [
        ext(E::Delete(0)), ext(E::Remove), Read, user(U::Leave(4)), ext(E::Insert(0)), Read, ext(E::Remove),
        Deliver, Deliver, Exec, ext(E::Touch), user(U::Leave(1)),
    ];
    if let Err((e, log)) = run(2, &ops, true) {
        panic!("{e}\n{}", log.join("\n"));
    }
}
