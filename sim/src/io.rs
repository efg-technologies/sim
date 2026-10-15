//! BUF-1, TREE-5, FIND-3, MARK-1, MARK-2, SYN-3
//! The impure helpers: reading files and directories, the threads that save and keep snapshots
//! and highlight, git, the list of files, and the history file in sim's state directory.
use crate::rank::History;
use crate::sim::{Found, Hl, Id, Opened, Req, SimEvent};
use crate::syn::{Spans, Syn};
use crate::tree::Entry;
use ropey::Rope;
use si::core::app::{Effect, Event, Read};
use si::io::file::{self, Saved, why};
use si::io::journal::{self, Snapshot};
use std::collections::HashMap;
use std::ffi::OsString;
use std::fs::{self, DirBuilder, File, OpenOptions};
use std::io::{self, Write};
use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// How long a diff may take before it gives up (PROP-7).
pub const MERGE: Duration = Duration::from_millis(50);

/// `Ok(None)`: no such file yet.
pub fn read(path: &str) -> Read {
    match fs::metadata(path) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(why(&e)),
        Ok(m) if !m.is_file() => Err("not a regular file".into()),
        Ok(_) => fs::read(path).map(Some).map_err(|e| why(&e)),
    }
}

/// Where recovery copies go when snapshots are off: a path nothing can be made under.
pub const DEAD_STATE: &str = "/dev/null/sim-no-state";

/// `$XDG_STATE_HOME`, else `$HOME/.local/state`; a relative path is ignored, as the XDG spec says.
fn state_base(xdg: Option<OsString>, home: Option<OsString>) -> Option<PathBuf> {
    let abs = |v: Option<OsString>| v.map(PathBuf::from).filter(|p| p.is_absolute());
    abs(xdg).or_else(|| abs(home).map(|h| h.join(".local/state")))
}

/// SAFE-6: `dir` must be private to us: a real directory (not a symlink) that we own and that
/// group and others cannot enter. Returns our uid, which libc is not needed for: it is the owner
/// of a file we create there (the probe). This is si's `file::private`, which this tree's si
/// does not have yet.
pub fn private(dir: &Path) -> io::Result<u32> {
    let m = fs::symlink_metadata(dir)?;
    let probe = dir.join(format!(".owner-{}-{}", std::process::id(), now_ms()));
    let mine = OpenOptions::new().write(true).create_new(true).mode(0o600).open(&probe);
    let mine = mine.and_then(|f| f.metadata());
    drop(fs::remove_file(&probe));
    let uid = mine?.uid();
    let ok = m.is_dir() && m.uid() == uid && m.mode() & 0o077 == 0;
    ok.then_some(uid).ok_or_else(|| {
        io::Error::other(format!("{} is not private (yours, mode 700)", dir.display()))
    })
}

/// Where snapshots go: si's directory, so that si and sim recover each other's files (SAFE-6).
/// With neither variable set (or no absolute path in them) there is none, never a shared place
/// such as /tmp; and it must be private (see `private`). `Err` says why snapshots are off.
pub fn snapshot_dir(xdg: Option<OsString>, home: Option<OsString>) -> Result<PathBuf, String> {
    let dir = state_base(xdg, home)
        .ok_or("no state directory: neither $XDG_STATE_HOME nor $HOME is set")?
        .join("si");
    let make = DirBuilder::new().recursive(true).mode(0o700).create(&dir);
    make.map_err(|e| format!("cannot make {}: {}", dir.display(), why(&e)))?;
    private(&dir).map(|_| dir).map_err(|e| why(&e))
}

/// SAFE-6: whether a snapshot file of `key` in `state` belongs to someone other than `uid`.
/// This tree's `journal::find` does not look at owners, so sim does not call it then.
pub fn foreign(state: &Path, key: &str, uid: u32) -> bool {
    let name = journal::file(state, key, 0);
    let stem = name.file_name().and_then(|n| n.to_str()).and_then(|n| n.split('-').next());
    let named = |e: &fs::DirEntry| e.file_name().to_string_lossy().starts_with(stem.unwrap_or("-"));
    let mut entries = fs::read_dir(state).into_iter().flatten().flatten();
    entries.any(|e| named(&e) && !e.metadata().is_ok_and(|m| m.uid() == uid))
}

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
        return SimEvent::Dir(shown.into());
    }
    let key = key(at);
    let found = state.and_then(|state| {
        let uid = fs::metadata(state).ok()?.uid();
        let s = journal::find(state, &key, std::process::id())
            .filter(|_| !foreign(state, &key, uid))?;
        let (file, age) =
            (journal::file(state, &s.path, s.pid), now_ms().saturating_sub(s.time_ms));
        Some(Found(s, file.display().to_string(), age, Instant::now() + MERGE))
    });
    let file = read(&at.to_string_lossy());
    SimEvent::Opened(Opened { id, path: shown.into(), key, file, found })
}

/// A job for the journal thread.
pub enum Job {
    /// The history of opened files, to save (MARK-2).
    History(History),
    /// A snapshot that a dead session left for this file, to delete when ours replaces it.
    Hold(Id, String, u32),
    /// A `Snapshot` or an `Unsnapshot` of this file's App.
    Snap(Id, Effect),
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
                Job::Snap(id, Effect::Snapshot { path, base, text }) => {
                    let Some(state) = &state else { continue }; // snapshots are off
                    let [base, text] = [base, text].map(|r| r.to_string().into_bytes());
                    let path = path.map_or(String::new(), |p| key(Path::new(&p)));
                    let snap = Snapshot { path, pid, time_ms: now_ms(), base, text };
                    let mine = (snap.path.clone(), pid);
                    match journal::write(state, &snap) {
                        Ok(()) => {
                            held.insert(id, mine.clone()).filter(|old| *old != mine).map(rm);
                        }
                        Err(e) => say(0, format!("cannot write snapshots: {}", why(&e))),
                    }
                }
                Job::Snap(id, _) => drop(held.remove(&id).map(rm)),
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
        let state = state.unwrap_or_else(|| DEAD_STATE.into()); // no recovery copies then (SAFE-2)
        for (id, path, bytes, expect) in rx {
            let ev = match file::save(&path, &bytes, expect, &state) {
                Ok(Saved::Done(h, warn)) => Event::Saved(Ok((h, warn))),
                Ok(Saved::Conflict(disk)) => Event::SaveConflict(disk, Instant::now() + MERGE),
                Err(e) => Event::Saved(Err(e)),
            };
            if answers.send(SimEvent::App(id, ev)).is_err() {
                return;
            }
        }
    });
    jobs
}

/// The entries of a directory. A symlink to a directory counts as one, unless it leads back to
/// the directory itself or to one above it (a loop); a link that cannot be followed is a file.
pub fn read_dir(dir: &Path) -> Result<Vec<Entry>, String> {
    let list =
        fs::read_dir(dir).map_err(|e| format!("cannot read {}: {}", dir.display(), why(&e)))?;
    let here = fs::canonicalize(dir).ok();
    let is_dir = |e: &fs::DirEntry| match e.file_type() {
        Ok(t) if t.is_symlink() => {
            let target = fs::canonicalize(e.path()).ok().filter(|t| t.is_dir());
            target.is_some_and(|t| here.as_ref().is_some_and(|h| !h.starts_with(t)))
        }
        Ok(t) => t.is_dir(),
        Err(_) => false,
    };
    Ok(list
        .filter_map(Result::ok)
        .map(|e| Entry { name: e.file_name().to_string_lossy().into_owned(), is_dir: is_dir(&e) })
        .collect())
}

/// Runs git in `dir`. In English, so that "not a git repository" can be told from other failures.
fn git(dir: &Path, args: &[&str]) -> Result<Vec<u8>, String> {
    let out = Command::new("git")
        .args(["-c", "core.fsmonitor=false", "--no-optional-locks"])
        .args(args)
        .current_dir(dir)
        .env("LC_ALL", "C")
        .output();
    let out = out.map_err(|e| format!("cannot run git in {}: {}", dir.display(), why(&e)))?;
    if out.status.success() {
        return Ok(out.stdout);
    }
    let err = String::from_utf8_lossy(&out.stderr);
    let err = err.lines().next().unwrap_or_default();
    Err(if err.contains("not a git repository") {
        format!("not a git repository: {}", dir.display())
    } else {
        format!("git {}: {err}", args[0])
    })
}

/// FIND-3: the files of the repository, NUL-separated and relative to `dir`: tracked ones and
/// untracked ones that are not ignored.
pub fn git_files(dir: &Path) -> Result<Vec<u8>, String> {
    git(dir, &["ls-files", "--cached", "--others", "--exclude-standard", "-z"])
}

/// MARK-1: `git status --porcelain=v1 -z`, with every untracked file listed (not just its
/// directory). Its paths are relative to the top of the repository, not to `dir`.
pub fn git_status(dir: &Path) -> Result<Vec<u8>, String> {
    git(dir, &["status", "--porcelain=v1", "-z", "--untracked-files=all"])
}

/// Where `dir` lies below the top of its repository: empty at the top, else `sub/dir/`.
pub fn git_prefix(dir: &Path) -> Result<String, String> {
    let out = git(dir, &["rev-parse", "--show-prefix"])?;
    Ok(String::from_utf8_lossy(&out).trim_end_matches('\n').to_string())
}

/// FIND-3: past this many files a walk stops (git's list has no limit): a walk of `/` or of a
/// home directory must not eat the memory.
const WALK_MAX: usize = 200_000;

/// The files below `root` without git, relative to it. Links are files, never followed.
fn walk(root: &Path) -> Vec<String> {
    let (mut out, mut todo) = (vec![], vec![PathBuf::new()]);
    while let Some(rel) = todo.pop().filter(|_| out.len() < WALK_MAX) {
        for e in fs::read_dir(root.join(&rel)).into_iter().flatten().flatten() {
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
    let mut list = git_files(root).map_or_else(|_| walk(root), listed);
    list.sort_unstable();
    list.dedup();
    list
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

/// MARK-2: `$XDG_STATE_HOME/sim`, else `$HOME/.local/state/sim` (see `state_base`). There is no
/// fallback to a temporary directory: the history must not sit where others can read it.
pub fn state_dir(xdg_state_home: Option<OsString>, home: Option<OsString>) -> Option<PathBuf> {
    Some(state_base(xdg_state_home, home)?.join("sim"))
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
        format!("history: cannot {what} {}: {}", dir.display(), why(&e))
    };
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

/// The thread of the highlighter. Of the requests waiting for it, only the newest for each file
/// is answered: the others are about text that is gone.
pub fn highlight_thread(tx: Sender<SimEvent>) -> Sender<Req> {
    let (jobs, rx) = mpsc::channel::<Req>();
    thread::spawn(move || {
        let mut hl = Highlighter::default();
        while let Ok(first) = rx.recv() {
            let mut latest = HashMap::from([(first.id, first)]);
            rx.try_iter().for_each(|r| drop(latest.insert(r.id, r)));
            for (_, r) in latest {
                if tx.send(hl.run(r)).is_err() {
                    return;
                }
            }
        }
    });
    jobs
}
