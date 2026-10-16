//! BUF-1, TREE-5, FIND-3, MARK-1, MARK-2, SYN-3
//! The impure helpers: reading files and directories, the threads that save and keep snapshots
//! and highlight, git, the list of files, and the history file in sim's state directory.
use crate::layout;
use crate::rank::History;
use crate::sim::{Found, Hl, Id, Opened, Req, SimEvent};
use crate::syn::{Spans, Syn};
use crate::tree::{self, Entry};
use ropey::Rope;
use si::core::app::{Effect, Event};
use si::io::file::{self, Saved, why};
use si::io::journal::{self, NO_STATE, Snapshot};
use std::collections::{HashMap, HashSet};
use std::ffi::OsString;
use std::fs::{self, DirBuilder, File, OpenOptions};
use std::io::{self, Write};
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// How long a diff may take before it gives up (PROP-7).
pub const MERGE: Duration = Duration::from_millis(50);

pub fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64)
}

/// The journal key of a file (SAFE-6): its canonical path.
fn key(path: &Path) -> String {
    file::resolve(&path.to_string_lossy()).to_string_lossy().into_owned()
}

/// The shell's answer to `SimEffect::Open` (BUF-1): `at` is where the file is, `shown` the name
/// its App will show. A directory is shown in the panel; a file is read, and the snapshot that a
/// dead session left for it (SAFE-6) is looked up.
pub fn open(state: Option<&Path>, id: Id, shown: &str, at: &Path) -> SimEvent {
    if at.is_dir() {
        return SimEvent::Dir(id, shown.into());
    }
    let key = key(at);
    let found = state.and_then(|state| {
        let s = journal::find(state, &key, std::process::id())?;
        let (file, age) =
            (journal::file(state, &s.path, s.pid), now_ms().saturating_sub(s.time_ms));
        Some(Found(s, file.display().to_string(), age, Instant::now() + MERGE))
    });
    let place = state.and_then(|state| journal::place(state, &key));
    // A listing shows a name that is not UTF-8 with U+FFFD, and every path in sim is text: that
    // name is no file, and opening it as a new one would create it under another name (SAFE-8).
    let file = if shown.contains('\u{fffd}') && !at.exists() {
        Err("the name is not valid UTF-8, so the file cannot be opened".into())
    } else {
        file::read(&at.to_string_lossy())
    };
    SimEvent::Opened(Opened { id, path: shown.into(), key, file, found, place })
}

/// A job for the journal thread.
pub enum Job {
    /// The history of opened files, to save (MARK-2).
    History(History),
    /// A snapshot that a dead session left for this file, to delete when ours replaces it.
    Hold(Id, String, u32),
    /// What this file's App asked of the journal: a `Snapshot`, an `Unsnapshot` (SAFE-6), a
    /// `Remember` of where its cursor was left, or the `Open` of a reload, whose place is looked up
    /// (CLI-6).
    App(Id, Effect),
}

/// The journal thread (SAFE-6) runs snapshots and removals of every file in order, so no key
/// waits for a disk, and saves the history (MARK-2) in `hist` as well. The receiver reports
/// `Disconnected` once the thread has finished what was sent. The first snapshot and the first
/// history that cannot be written are told to the user through `tell`.
pub fn journal_thread(
    state: Option<PathBuf>,
    hist: Option<PathBuf>,
    tell: Sender<SimEvent>,
) -> (Sender<Job>, Receiver<()>) {
    let (jobs, rx) = mpsc::channel();
    let (done, finished) = mpsc::channel();
    thread::spawn(move || {
        let (pid, _done) = (std::process::id(), done);
        let rm = |h: (String, u32)| state.iter().for_each(|s| journal::remove(s, &h.0, h.1));
        // The snapshot of each file that is ours to delete: a recovered one, then our own.
        let (mut held, mut told) = (HashMap::<Id, (String, u32)>::new(), [false; 2]);
        let mut say = |n: usize, what: String| {
            if !std::mem::replace(&mut told[n], true) {
                drop(tell.send(SimEvent::Notice(what)));
            }
        };
        for job in rx {
            match job {
                Job::History(h) => {
                    hist.iter().filter_map(|d| save_history(d, &h).err()).for_each(|e| say(1, e))
                }
                Job::Hold(id, key, pid) => drop(held.insert(id, (key, pid))),
                Job::App(_, Effect::Remember { path, line, col }) => {
                    // Read again, so that another session's places are kept (CLI-6).
                    for d in state.iter() {
                        let mut all = journal::places(d);
                        journal::touch(&mut all, key(Path::new(&path)), line, col);
                        drop(journal::save_places(d, &all));
                    }
                }
                Job::App(id, Effect::Open(path)) => {
                    let at =
                        state.as_deref().and_then(|d| journal::place(d, &key(Path::new(&path))));
                    if let Some((l, c)) = at {
                        drop(tell.send(SimEvent::App(id, Event::Place(path, l, c))));
                    }
                }
                Job::App(id, Effect::Snapshot { path, base, text }) => {
                    let [base, text] = [base, text].map(|r| r.to_string().into_bytes());
                    let path = path.map_or(String::new(), |p| key(Path::new(&p)));
                    let snap = Snapshot { path, pid, time_ms: now_ms(), base, text };
                    let mine = (snap.path.clone(), pid);
                    let wrote = state.as_deref().ok_or_else(|| io::Error::other(NO_STATE));
                    match wrote.and_then(|d| journal::write(d, &snap)) {
                        Ok(()) => {
                            held.insert(id, mine.clone()).filter(|old| *old != mine).map(rm);
                        }
                        Err(e) => say(0, format!("cannot write snapshots: {}", why(&e))),
                    }
                }
                Job::App(id, _) => drop(held.remove(&id).map(rm)),
            }
        }
    });
    (jobs, finished)
}

/// A save: file, path, bytes, and the hash the disk must still have (SAFE-5).
pub type Save = (Id, String, Vec<u8>, Option<u64>);

/// The save thread: one for all files, in order. Each answer is tagged with the file's id.
pub fn save_thread(state: Option<PathBuf>, answers: Sender<SimEvent>) -> Sender<Save> {
    let (jobs, rx) = mpsc::channel::<Save>();
    thread::spawn(move || {
        for (id, path, bytes, expect) in rx {
            // The answer is numbered when the save is done (PROP-1), as in si's main.
            let ev = match file::save(&path, &bytes, expect, state.as_deref()) {
                Ok(Saved::Done(h, warn)) => Event::Saved(Ok((h, warn)), file::seq()),
                Ok(Saved::Conflict(disk)) => {
                    Event::SaveConflict(disk, Instant::now() + MERGE, file::seq())
                }
                Err(e) => Event::Saved(Err(e), file::seq()),
            };
            if answers.send(SimEvent::App(id, ev)).is_err() {
                return;
            }
        }
    });
    jobs
}

/// The entries of a directory, or why it cannot be read (the panel says which directory, by its
/// short name, so that a long path never cuts the reason off the status line). A symlink to a
/// directory counts as one, unless it leads back to the directory itself or to one above it (a
/// loop); a link that cannot be followed is a file.
pub fn read_dir(dir: &Path) -> Result<Vec<Entry>, String> {
    let list = fs::read_dir(dir).map_err(|e| why(&e))?;
    let here = fs::canonicalize(dir).ok();
    let is_dir = |e: &fs::DirEntry| match e.file_type() {
        Ok(t) if t.is_symlink() => {
            let target = fs::canonicalize(e.path()).ok().filter(|t| t.is_dir());
            target.is_some_and(|t| here.as_ref().is_some_and(|h| !h.starts_with(t)))
        }
        Ok(t) => t.is_dir(),
        Err(_) => false,
    };
    let entries = list
        .filter_map(Result::ok)
        .map(|e| Entry { name: e.file_name().to_string_lossy().into_owned(), is_dir: is_dir(&e) });
    let mut entries: Vec<Entry> = entries.collect();
    tree::sort(&mut entries);
    Ok(entries)
}

/// MARK-1: what is said (once) when git is not run because the repository's config could make it
/// run commands.
pub const GIT_SKIPPED: &str = "git skipped: this repository's config could run commands";

/// MARK-1: whether a key (`section.[subsection.]variable`, section and variable in lower case) can
/// hold a command: the section is `filter`, `include` or `includeif`, or the variable is one of
/// the names below, or ends with `command` (`sshcommand`, `alternaterefscommand`), `cmd`,
/// `program` or `hook`. `core.fsmonitor` is one by its variable. It is a class, not a list of
/// what git runs today, so that a setting git adds later is in it too; `status`, `ls-files` and
/// `rev-parse` use few of them. A key that has no section is not one git writes.
fn holds_command(key: &[u8]) -> bool {
    let (section, var) = (key.split(|&b| b == b'.').next(), key.rsplit(|&b| b == b'.').next());
    let (section, var) = (section.unwrap_or_default(), var.unwrap_or_default());
    let names = b"driver textconv helper process clean smudge askpass editor pager gitproxy \
                  hookspath fsmonitor uploadpack receivepack vcs";
    section.len() == key.len()
        || matches!(section, b"filter" | b"include" | b"includeif" | b"protocol")
        || names.split(|&b| b == b' ').any(|name| name == var)
        || [&b"command"[..], b"cmd", b"program", b"hook"].iter().any(|end| var.ends_with(end))
}

/// MARK-1: whether the config that git lists for a repository (`git config --list --show-scope
/// -z`: `scope NUL key [NL value] NUL` for each entry) has an entry of the repository's own whose
/// key can hold a command (`holds_command`). An included file has the scope of the file that
/// includes it. The user's own scopes (system, global, the command line) are trusted; any other
/// scope, and a listing that does not parse, counts as the repository's.
pub fn runs_commands(listing: &[u8]) -> bool {
    let mut fields = listing.split(|&b| b == 0);
    while let Some(scope) = fields.next() {
        let Some(entry) = fields.next() else { return !scope.is_empty() };
        let key = entry.split(|&b| b == b'\n').next().unwrap_or_default();
        if holds_command(key) && !matches!(scope, b"system" | b"global" | b"command") {
            return true;
        }
    }
    false
}

/// Runs git in `dir`, in English so that "not a git repository" can be told from other failures.
fn run(dir: &Path, args: &[&str]) -> Result<Vec<u8>, String> {
    let out = Command::new("git")
        .args(["-c", "core.fsmonitor=false", "--no-optional-locks"])
        .args(args)
        .current_dir(dir)
        .env("LC_ALL", "C")
        // Git never fetches for sim: in a partial clone with objects missing a `git status`
        // would, from the promisor remote, and run what the repository says serves a fetch (its
        // `uploadpack`, an `ext::` url) or ask for a password on the terminal sim draws on.
        .env("GIT_NO_LAZY_FETCH", "1")
        .env("GIT_ALLOW_PROTOCOL", "sim-none")
        .env("GIT_TERMINAL_PROMPT", "0")
        .output();
    let shown = layout::short(&dir.to_string_lossy(), 30); // (the reason is what matters: it is last)
    let out = out.map_err(|e| format!("cannot run git in {shown}: {}", why(&e)))?;
    if out.status.success() {
        return Ok(out.stdout);
    }
    let err = String::from_utf8_lossy(&out.stderr);
    let err = err.lines().next().unwrap_or_default();
    // (a bare repository, or inside `.git`, has no marks to give: it is no repository to this)
    Err(if err.contains("not a git repository") || err.contains("must be run in a work tree") {
        format!("not a git repository: {}", dir.display())
    } else {
        format!("git {}: {err}", args[0])
    })
}

/// Runs git in `dir`, unless the repository's config could make it run a command (MARK-1). The
/// config is read by git itself, so that sim cannot read it differently from git. Listing config
/// runs nothing: `builtin/config.c` only parses it and prints (the one command it can start is
/// `config --edit`'s editor); hooks, filters and the monitor belong to commands that use the index
/// or the work tree. In a directory that is no repository the list is the user's own and git then
/// says so itself. An unreadable or invalid config makes `config` fail, which counts as unsafe.
fn git(dir: &Path, args: &[&str]) -> Result<Vec<u8>, String> {
    let list = run(dir, &["config", "--list", "--show-scope", "-z"]);
    let list =
        list.map_err(|e| if e.starts_with("git config:") { GIT_SKIPPED.into() } else { e })?;
    if runs_commands(&list) {
        return Err(GIT_SKIPPED.into());
    }
    run(dir, args)
}

/// FIND-3: the files of the repository, NUL-separated and relative to `dir`: tracked ones and
/// untracked ones that are not ignored.
pub fn git_files(dir: &Path) -> Result<Vec<u8>, String> {
    git(dir, &["ls-files", "--cached", "--others", "--exclude-standard", "-z"])
}

/// MARK-1: `git status --porcelain=v1 -z`, with every untracked file listed (not just its
/// directory). Its paths are relative to the top of the repository, not to `dir`. Submodules are
/// ignored: git would run a child git in each, which reads the submodule's own config (and runs
/// its filters), and the guard looked at the repository's alone.
pub fn git_status(dir: &Path) -> Result<Vec<u8>, String> {
    git(
        dir,
        &["status", "--porcelain=v1", "-z", "--untracked-files=all", "--ignore-submodules=all"],
    )
}

/// Where `dir` lies below the top of its repository: empty at the top, else `sub/dir/`.
pub fn git_prefix(dir: &Path) -> Result<String, String> {
    let out = git(dir, &["rev-parse", "--show-prefix"])?;
    Ok(String::from_utf8_lossy(&out).trim_end_matches('\n').to_string())
}

/// FIND-3: past this many files a walk stops (git's list has no limit): a walk of `/` or of a
/// home directory must not eat the memory.
const WALK_MAX: usize = 200_000;

/// The files below `root` without git, relative to it, at most `max`. Links are files, never
/// followed.
pub fn walk(root: &Path, max: usize) -> Vec<String> {
    let (mut out, mut todo) = (vec![], vec![PathBuf::new()]);
    while let Some(rel) = todo.pop() {
        for e in fs::read_dir(root.join(&rel)).into_iter().flatten().flatten() {
            if out.len() >= max {
                return out; // (a directory of millions of files is one directory)
            }
            let path = rel.join(e.file_name());
            match e.file_type() {
                Ok(t) if t.is_dir() => todo.extend((e.file_name() != ".git").then_some(path)),
                _ => out.push(path.to_string_lossy().into_owned()),
            }
        }
    }
    out
}

/// FIND-3: the files under `root`, sorted, relative to it: the repository's (tracked and
/// untracked, not ignored) if `root` is in one, else a walk.
pub fn files(root: &Path) -> Vec<String> {
    let listed = |out: Vec<u8>| -> Vec<String> {
        let names = out.split(|&b| b == 0).filter(|p| !p.is_empty());
        names.map(|p| String::from_utf8_lossy(p).into_owned()).collect()
    };
    let mut list = git_files(root).map_or_else(|_| walk(root, WALK_MAX), listed);
    list.sort_unstable();
    list.dedup();
    list
}

/// TREE-5: the reads of directories, each in a thread of its own, so two reads of one directory
/// can end in the other order, and the older one would put a listing that is gone over the new
/// one. A read takes a ticket when it starts, and its answer is sent only if no read of that
/// directory that started later has sent its own.
#[derive(Clone, Default)]
pub struct Reads(Arc<Mutex<(u64, HashMap<PathBuf, u64>)>>);

impl Reads {
    pub fn start(&self) -> u64 {
        let mut state = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        state.0 += 1;
        state.0
    }

    /// Runs `send` (under the lock, so that two answers are sent in the order of their tickets)
    /// unless an answer for `dir` of a later ticket has been sent already.
    pub fn answer(&self, dir: &Path, ticket: u64, send: impl FnOnce()) {
        let mut state = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        let newest = state.1.entry(dir.to_path_buf()).or_default();
        if *newest < ticket {
            *newest = ticket;
            send();
        }
    }
}

/// A background job that never runs twice at once: a request while it runs is one more run
/// after it, however many requests came (FIND-3, MARK-1). Its answer goes to `tx`.
pub fn worker(tx: Sender<SimEvent>, job: impl Fn() -> SimEvent + Send + 'static) -> Sender<()> {
    let (ask, rx) = mpsc::channel();
    thread::spawn(move || {
        while rx.recv().is_ok() {
            rx.try_iter().for_each(drop);
            if tx.send(job()).is_err() {
                return;
            }
        }
    });
    ask
}

/// MARK-1: git's status and where `root` lies below the top of the repository.
pub fn status(root: &Path) -> Result<(Vec<u8>, String), String> {
    Ok((git_status(root)?, git_prefix(root)?))
}

/// MARK-2: `$XDG_STATE_HOME/sim`, else `$HOME/.local/state/sim`: next to si's directory, by si's
/// own rule (`journal::state_dir`), so with no fallback to a temporary directory: the history
/// must not sit where others can read it.
pub fn state_dir(xdg_state_home: Option<OsString>, home: Option<OsString>) -> Option<PathBuf> {
    journal::state_dir(xdg_state_home, home).map(|d| d.with_file_name("sim"))
}

/// The history saved in `dir`; empty if there is none, or it cannot be read.
pub fn load_history(dir: &Path) -> History {
    fs::read_to_string(dir.join("history"))
        .map_or_else(|_| History::default(), |text| History::parse(&text))
}

/// Saves the history in `dir` (made with 0700 if needed): a temporary file (0600) is written and
/// synced, then renamed over the old one, so the file is always whole.
pub fn save_history(dir: &Path, history: &History) -> Result<(), String> {
    let fail = |what: &str, e: std::io::Error| {
        format!("history: cannot {what} {}: {}", layout::short(&dir.to_string_lossy(), 30), why(&e))
    };
    if fs::symlink_metadata(dir).is_ok_and(|m| m.file_type().is_symlink()) {
        return Err(fail("use", io::Error::other("it is a link")));
    }
    DirBuilder::new().recursive(true).mode(0o700).create(dir).map_err(|e| fail("create", e))?;
    fs::set_permissions(dir, fs::Permissions::from_mode(0o700)).map_err(|e| fail("protect", e))?;
    let tmp = dir.join(format!("history.{}.tmp", std::process::id()));
    let _ = fs::remove_file(&tmp);
    let write = || -> std::io::Result<()> {
        let mut f = OpenOptions::new().write(true).create_new(true).mode(0o600).open(&tmp)?;
        f.write_all(history.serialize().as_bytes())?;
        f.sync_all()?;
        fs::rename(&tmp, dir.join("history"))
    };
    write().map_err(|e| {
        let _ = fs::remove_file(&tmp);
        fail("write in", e)
    })?;
    let _ = File::open(dir).and_then(|d| d.sync_all()); // the rename has happened; this only makes it durable
    Ok(())
}

/// The first byte at which two texts differ, or an earlier one: ropes cut into chunks in different
/// ways count as different where their chunks first do.
fn first_diff(a: &Rope, b: &Rope) -> usize {
    let mut at = 0;
    for (p, q) in a.chunks().zip(b.chunks()) {
        if p != q {
            return at + p.bytes().zip(q.bytes()).take_while(|(x, y)| x == y).count();
        }
        at += p.len();
    }
    at
}

/// SYN-3: highlights windows of files off the main thread. It keeps a `Syn` (the parse states)
/// for each file, and the text they were made from: where a new text first differs from that
/// one is where the states stop being valid.
#[derive(Default)]
pub struct Highlighter(HashMap<Id, (Option<Syn>, Rope)>);

impl Highlighter {
    /// How many files it keeps parse states and text for.
    pub fn files(&self) -> usize {
        self.0.len()
    }

    /// The file is closed: its states and the text they were made from are not needed.
    pub fn forget(&mut self, id: Id) {
        self.0.remove(&id);
    }

    pub fn run(&mut self, req: Req) -> SimEvent {
        let Req { id, name, ver, text, from, n } = req;
        let line =
            |i: usize| text.get_line(i).map(|l| l.to_string().trim_end_matches('\n').to_string());
        let first = line(0).unwrap_or_default();
        let (syn, seen) =
            self.0.entry(id).or_insert_with(|| (Syn::new(&name, &first), Rope::new()));
        let lines = text.len_lines();
        let spans = match syn {
            Some(syn) => {
                if *seen != text {
                    syn.edit(text.byte_to_line(first_diff(seen, &text).min(text.len_bytes())));
                    *seen = text.clone();
                }
                syn.spans(&line, from, n)
            }
            None => vec![Spans::new(); n.min(lines.saturating_sub(from))],
        };
        SimEvent::Spans(Hl { id, ver, lines, from, spans })
    }
}

/// What the highlighter's thread is asked: a window to highlight, or to forget a closed file.
pub enum HlJob {
    Run(Req),
    Forget(Id),
}

/// How long a request may take, besides 100 us for each line it parses from the top of what is
/// cached: far longer than any parse (a first window 50,000 lines down takes 2.6 s), but not for
/// ever, which syntect can take (see `highlight_thread_within`).
pub const HL_BUDGET: Duration = Duration::from_secs(5);

pub fn highlight_thread(tx: Sender<SimEvent>) -> Sender<HlJob> {
    highlight_thread_within(tx, HL_BUDGET)
}

/// The thread of the highlighter. Of the requests waiting for it, only the newest for each file
/// is answered: the others are about text that is gone. syntect can loop for ever on some text
/// (5.3 with bat's JavaScript: a regex literal that starts a file or a statement and ends its
/// line, then a `/*`), so every request runs in a thread of its own that the highlighter waits for
/// `budget` (and 100 us a line); a request that does not finish is given up: the file is plain
/// from then on, the user is told once, and the highlighter goes on with a new one for the other
/// files. The thread that loops stays, busy, until sim ends: nothing in Rust stops a thread.
pub fn highlight_thread_within(tx: Sender<SimEvent>, budget: Duration) -> Sender<HlJob> {
    let (jobs, rx) = mpsc::channel::<HlJob>();
    thread::spawn(move || {
        let (mut hl, mut given_up) = (Highlighter::default(), HashSet::<Id>::new());
        while let Ok(first) = rx.recv() {
            let (mut latest, mut closed) = (HashMap::new(), vec![]);
            for job in std::iter::once(first).chain(rx.try_iter()) {
                match job {
                    HlJob::Run(r) => drop(latest.insert(r.id, r)),
                    HlJob::Forget(id) => closed.push(id),
                }
            }
            closed.iter().for_each(|id| drop(latest.remove(id)));
            for (_, r) in latest {
                let (id, ver, name, from, n) = (r.id, r.ver, r.name.clone(), r.from, r.n);
                let lines = r.text.len_lines();
                let wait = budget + Duration::from_micros(100) * (from + n).min(1 << 20) as u32;
                let plain = |lines: usize| {
                    let spans = vec![Spans::new(); n.min(lines.saturating_sub(from))];
                    SimEvent::Spans(Hl { id, ver, lines, from, spans })
                };
                let answer = if given_up.contains(&id) {
                    plain(lines)
                } else {
                    let (done, finished) = mpsc::channel();
                    let mut worker = std::mem::take(&mut hl);
                    thread::spawn(move || {
                        let ev = worker.run(r);
                        drop(done.send((worker, ev)));
                    });
                    match finished.recv_timeout(wait) {
                        Ok((back, ev)) => {
                            hl = back;
                            ev
                        }
                        Err(_) => {
                            given_up.insert(id);
                            let why = format!(
                                "syntax colors stopped for {name}: the highlighter did not finish"
                            );
                            drop(tx.send(SimEvent::Notice(why)));
                            plain(lines)
                        }
                    }
                };
                if tx.send(answer).is_err() {
                    return;
                }
            }
            closed.into_iter().for_each(|id| hl.forget(id));
        }
    });
    jobs
}
