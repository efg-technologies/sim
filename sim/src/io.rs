//! BUF-1, TREE-5, FIND-3, MARK-1, MARK-2
//! The impure helpers: reading files and directories, the threads that save and keep snapshots,
//! git, and the history file in sim's state directory.
use crate::rank::History;
use crate::sim::{Found, Id, Opened, SimEvent};
use crate::tree::Entry;
use si::core::app::{Effect, Event, Read};
use si::io::file::{self, Saved, why};
use si::io::journal::{self, Snapshot};
use std::collections::HashMap;
use std::ffi::OsString;
use std::fs::{self, DirBuilder, File, OpenOptions};
use std::io::{self, Write};
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use std::{env, thread};

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

/// Where snapshots go: si's directory, so that si and sim recover each other's files (SAFE-6).
pub fn snapshot_dir() -> PathBuf {
    let var = |k| env::var_os(k).filter(|v| !v.is_empty()).map(PathBuf::from);
    var("XDG_STATE_HOME")
        .or_else(|| var("HOME").map(|h| h.join(".local/state")))
        .unwrap_or_else(env::temp_dir)
        .join("si")
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
pub fn open(state: &Path, id: Id, shown: &str, at: &Path) -> SimEvent {
    if at.is_dir() {
        return SimEvent::Dir(shown.into());
    }
    let key = key(at);
    let found = journal::find(state, &key, std::process::id()).map(|s| {
        let (file, age) =
            (journal::file(state, &s.path, s.pid), now_ms().saturating_sub(s.time_ms));
        Found(s, file.display().to_string(), age, Instant::now() + MERGE)
    });
    SimEvent::Opened(Opened {
        id,
        path: shown.into(),
        key,
        file: read(&at.to_string_lossy()),
        found,
    })
}

/// A job for the journal thread.
pub enum Job {
    /// A snapshot that a dead session left for this file, to delete when ours replaces it.
    Hold(Id, String, u32),
    /// A `Snapshot` or an `Unsnapshot` of this file's App.
    Snap(Id, Effect),
}

/// The journal thread (SAFE-6) runs snapshots and removals of every file in order, so no key
/// waits for a disk. The receiver reports `Disconnected` once the thread has finished what was
/// sent. The first snapshot that cannot be written is told to the user through `tell`.
pub fn journal_thread(state: PathBuf, tell: Sender<SimEvent>) -> (Sender<Job>, Receiver<()>) {
    let (jobs, rx) = mpsc::channel();
    let (done, finished) = mpsc::channel();
    thread::spawn(move || {
        let (pid, _done) = (std::process::id(), done);
        let rm = |h: (String, u32)| journal::remove(&state, &h.0, h.1);
        // The snapshot of each file that is ours to delete: a recovered one, then our own.
        let (mut held, mut told) = (HashMap::<Id, (String, u32)>::new(), false);
        for job in rx {
            match job {
                Job::Hold(id, key, pid) => drop(held.insert(id, (key, pid))),
                Job::Snap(id, Effect::Snapshot { path, base, text }) => {
                    let [base, text] = [base, text].map(|r| r.to_string().into_bytes());
                    let path = path.map_or(String::new(), |p| key(Path::new(&p)));
                    let snap = Snapshot { path, pid, time_ms: now_ms(), base, text };
                    let mine = (snap.path.clone(), pid);
                    let wrote = journal::write(&state, &snap);
                    if wrote.is_ok() {
                        held.insert(id, mine.clone()).filter(|old| *old != mine).map(rm);
                    }
                    if let Err(e) = wrote
                        && !std::mem::replace(&mut told, true)
                    {
                        drop(tell.send(SimEvent::Notice(format!(
                            "cannot write snapshots: {}",
                            why(&e)
                        ))));
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
pub fn save_thread(state: PathBuf, answers: Sender<SimEvent>) -> Sender<Save> {
    let (jobs, rx) = mpsc::channel::<Save>();
    thread::spawn(move || {
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

/// MARK-2: `$XDG_STATE_HOME/sim`, else `$HOME/.local/state/sim` (the values of the two
/// variables are passed in; a relative one is ignored, as the XDG spec says). There is no
/// fallback to a temporary directory: the history must not sit where others can read it.
pub fn state_dir(xdg_state_home: Option<OsString>, home: Option<OsString>) -> Option<PathBuf> {
    let abs = |v: Option<OsString>| v.map(PathBuf::from).filter(|p| p.is_absolute());
    let base = abs(xdg_state_home).or_else(|| abs(home).map(|h| h.join(".local/state")))?;
    Some(base.join("sim"))
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
