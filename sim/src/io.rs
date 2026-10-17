//! BUF-1, TREE-5, FIND-3, MARK-1, MARK-2, SYN-3, LOOK-1, GREP-4, GREP-5, GREP-7, GREP-8, UI-2, UI-3, UI-5, PLAIN-1, STATE-1, COLOR-2, ICON-1, MD-5, GIT-1, GIT-2, GIT-3, GIT-4, DIFF-2, DIFF-3
//! The impure helpers: reading files and directories, the threads that save and keep snapshots
//! and highlight, git, the list of files, the history file in sim's state directory, and the
//! search of the contents of files (the built-in reader and ripgrep) with the preview's read.
use crate::changes::{self, Counts};
use crate::grep::{Batch, Group, HEAD, Line, MAX_LINES, Query, Request, is_binary, parse_rg};
use crate::layout::{self, Skin};
use crate::rank::History;
use crate::sim::{DiffReq, Found, Hl, Id, Opened, Pv, Req, SimEvent};
use crate::syn::{Spans, Syn};
use crate::tree::{self, Entry};
use ropey::Rope;
use sim_core::core::app::{Effect, Event};
use sim_core::io::file::{self, Saved, why};
use sim_core::io::filter;
use sim_core::io::journal::{self, NO_STATE, Snapshot};
use std::collections::{HashMap, HashSet};
use std::ffi::OsString;
use std::fs::{self, DirBuilder, File, OpenOptions};
use std::io::{self, BufRead, BufReader, Read, Write};
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering::SeqCst};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex, OnceLock, PoisonError};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use std::{env, thread};

/// How long a diff may take before it gives up (DISK-5).
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
    // (the empty name is plain mode's unnamed buffer: the journal's key for it is the empty path)
    let unnamed = shown.is_empty();
    if at.is_dir() && !unnamed {
        return SimEvent::Dir(id, shown.into());
    }
    let key = if unnamed { String::new() } else { key(at) };
    let found = state.and_then(|state| {
        let s = journal::find(state, &key, std::process::id())?;
        let (file, age) =
            (journal::file(state, &s.path, s.pid), now_ms().saturating_sub(s.time_ms));
        Some(Found(s, file.display().to_string(), age, Instant::now() + MERGE))
    });
    let place = state.filter(|_| !unnamed).and_then(|state| journal::place(state, &key));
    // A listing shows a name that is not UTF-8 with U+FFFD, and every path in sim is text: that
    // name is no file, and opening it as a new one would create it under another name (SAFE-8).
    let file = if unnamed {
        Ok(None)
    } else if shown.contains('\u{fffd}') && !at.exists() {
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
    /// The look that was chosen, to save (LOOK-1).
    Look(Skin),
    /// Whether the icons are on, to save (UI-2).
    Icons(bool),
    /// Whether the read view of Markdown is on, to save (MD-5).
    Md(bool),
    /// How many times Tab and Shift-Tab were used, to save (KEY-6).
    TabUses(u8),
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
                Job::Look(k) => {
                    hist.iter().filter_map(|d| save_look(d, k).err()).for_each(|e| say(1, e))
                }
                Job::Icons(on) => {
                    hist.iter().filter_map(|d| save_icons(d, on).err()).for_each(|e| say(1, e))
                }
                Job::Md(on) => {
                    hist.iter().filter_map(|d| save_md(d, on).err()).for_each(|e| say(1, e))
                }
                Job::TabUses(n) => {
                    hist.iter().filter_map(|d| save_tab_uses(d, n).err()).for_each(|e| say(1, e))
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
                Job::App(id, Effect::Keep { path, bytes, ext }) => {
                    // SAFE-7: a `:w!` or an `:e!` waits for this copy, so the answer is always sent.
                    let at = state.as_deref().ok_or_else(|| io::Error::other(NO_STATE));
                    let at = at.and_then(|d| journal::keep(d, &path, &bytes, ext));
                    let at = at.map(|p| p.display().to_string()).map_err(|e| why(&e));
                    drop(tell.send(SimEvent::App(id, Event::Kept(at))));
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
            // The answer is numbered when the save is done (DISK-1), as in the core's main.
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

/// MARK-1: what is said (once) when git is not run, and then why: the key that could run a command
/// (`runs_commands`), or that the config cannot be read.
pub const GIT_SKIPPED: &str = "git skipped: ";
/// PLAIN-1: what a program that is asked for in plain mode answers.
pub const PLAIN_RUNS_NOTHING: &str = "plain mode runs no program";

/// VI-11: the command that runs when Normal mode is entered: `SIM_ON_NORMAL`, else `SI_ON_NORMAL`,
/// the name it had before sim was one program; an empty one counts as unset.
pub fn hook(sim: Option<String>, si: Option<String>) -> Option<String> {
    sim.filter(|h| !h.is_empty()).or(si.filter(|h| !h.is_empty()))
}

/// PLAIN-1: whether this is a privileged context, where plain mode is the default: the effective
/// uid is 0, or the parent process is `sudo` (what runs the editor of `sudoedit`; `parent` is its
/// name in `/proc/{ppid}/comm`).
pub fn privileged(euid: u32, parent: Option<&str>) -> bool {
    euid == 0 || parent.is_some_and(|p| matches!(p.trim(), "sudo" | "sudoedit"))
}

/// The same for this process. The uid and the parent come from the system (`geteuid`, `getppid`),
/// never from a file made to see who owns it (a name in a shared directory is somebody else's to
/// plant); the parent's name is read from /proc, so on macOS only the uid tells.
pub fn plain_by_default() -> bool {
    let ppid = rustix::process::getppid().map(|p| p.as_raw_nonzero().get());
    let parent = ppid.and_then(|p| fs::read_to_string(format!("/proc/{p}/comm")).ok());
    privileged(file::euid(), parent.as_deref())
}

/// COLOR-2: whether to draw with no color at all: `NO_COLOR` is set and not empty (no-color.org), or
/// the terminal cannot do 8 colors: TERM is unset, `dumb` or a `vt…`, or terminfo's `colors` says
/// fewer.
pub fn mono(no_color: Option<OsString>, term: Option<OsString>, colors: Option<i32>) -> bool {
    let named =
        term.as_ref().and_then(|t| t.to_str()).is_none_or(|t| t == "dumb" || t.starts_with("vt"));
    no_color.is_some_and(|v| !v.is_empty()) || named || colors.is_some_and(|n| n < 8)
}

/// The `colors` of a compiled terminfo file (ncurses' format, numbers of 16 or 32 bits), if it has
/// one: the 14th number, not -1 (absent) or -2 (cancelled).
pub fn colors_in(db: &[u8]) -> Option<i32> {
    let word =
        |at: usize| db.get(at..at + 2).map(|b| usize::from(u16::from_le_bytes([b[0], b[1]])));
    let wide = match word(0)? {
        0o432 => 2,
        0o1036 => 4,
        _ => return None,
    };
    let (names, bools, count) = (word(2)?, word(4)?, word(6)?);
    let at = 12 + names + bools + (names + bools) % 2 + 13 * wide;
    let n = db.get(at..at + wide).filter(|_| count > 13)?;
    let n = if wide == 2 {
        i32::from(i16::from_le_bytes([n[0], n[1]]))
    } else {
        i32::from_le_bytes([n[0], n[1], n[2], n[3]])
    };
    (n >= 0).then_some(n)
}

/// `colors` of terminfo's entry for `term`, looked up as ncurses does: `dir/t/term`, or (macOS)
/// `dir/74/term`. A TERM is a name, not a path.
pub fn terminfo_colors(term: &str, dirs: &[PathBuf]) -> Option<i32> {
    let first = term.chars().next().filter(|_| !term.contains('/'))?;
    let subs = [first.to_string(), format!("{:x}", first as u32)];
    dirs.iter()
        .flat_map(|d| subs.iter().map(move |s| d.join(s).join(term)))
        .find_map(|f| colors_in(&fs::read(f).ok()?))
}

/// The same for this process (terminfo is looked for in `$TERMINFO`, `~/.terminfo`,
/// `$TERMINFO_DIRS`, then the system's directories).
pub fn no_color_here() -> bool {
    let get = std::env::var_os;
    let term = get("TERM");
    let mut dirs: Vec<PathBuf> = get("TERMINFO").map(PathBuf::from).into_iter().collect();
    dirs.extend(get("HOME").map(|h| PathBuf::from(h).join(".terminfo")));
    dirs.extend(get("TERMINFO_DIRS").iter().flat_map(|l| std::env::split_paths(l)));
    dirs.extend(["/etc/terminfo", "/lib/terminfo", "/usr/share/terminfo"].map(PathBuf::from));
    let colors = term.as_ref().and_then(|t| t.to_str()).and_then(|t| terminfo_colors(t, &dirs));
    mono(get("NO_COLOR"), term, colors)
}
const UNREADABLE: &str = "this repository's config cannot be read";

/// MARK-1: whether a key (`section.[subsection.]variable`, section and variable in lower case) can
/// hold a command: the section is `filter`, `include` or `includeif`, or the variable is one of
/// the names below, or ends with `command` (`sshcommand`, `alternaterefscommand`), `cmd`,
/// `program` or `hook`. `core.fsmonitor` is one by its variable. It is a class, not a list of
/// what git runs today, so that a setting git adds later is in it too; `status`, `ls-files` and
/// `rev-parse` use few of them. `core.hooksPath` is not one: hooks run in commands that write,
/// and with `--no-optional-locks` none of sim's four writes the index. A key that has no section
/// is not one git writes.
fn holds_command(key: &[u8]) -> bool {
    let (section, var) = (key.split(|&b| b == b'.').next(), key.rsplit(|&b| b == b'.').next());
    let (section, var) = (section.unwrap_or_default(), var.unwrap_or_default());
    let names = b"driver textconv helper process clean smudge askpass editor pager gitproxy \
                  fsmonitor uploadpack receivepack vcs";
    section.len() == key.len()
        || matches!(section, b"filter" | b"include" | b"includeif" | b"protocol")
        || names.split(|&b| b == b' ').any(|name| name == var)
        || [&b"command"[..], b"cmd", b"program", b"hook"].iter().any(|end| var.ends_with(end))
}

/// GIT-2: the name of the filter a key is one of the four of (`filter.<name>.clean`, `smudge`,
/// `process`, `required`), if the command line can turn it off: a name that holds an `=` cannot be
/// given to `-c` (git cuts a key there), and one that is not UTF-8 cannot be spelled.
fn filter_name(key: &[u8]) -> Option<&str> {
    let rest = key.strip_prefix(b"filter.")?;
    let at = rest.iter().rposition(|&b| b == b'.')?;
    let (name, var) = (&rest[..at], &rest[at + 1..]);
    let four = [&b"clean"[..], b"smudge", b"process", b"required"];
    let name = std::str::from_utf8(name).ok()?;
    (!name.is_empty() && !name.contains('=') && four.contains(&var)).then_some(name)
}

/// GIT-2: whether the command line turns this key off (GIT-1): the monitor, the hooks, an external
/// diff and a diff's text conversion, and the four keys of a filter that `-c` can name.
fn neutralized(key: &[u8]) -> bool {
    let (section, var) = (key.split(|&b| b == b'.').next(), key.rsplit(|&b| b == b'.').next());
    let (section, var) = (section.unwrap_or_default(), var.unwrap_or_default());
    let named = section.len() + var.len() + 1 < key.len(); // (a subsection between them)
    filter_name(key).is_some()
        || matches!(key, b"core.fsmonitor" | b"core.hookspath" | b"diff.external")
        || (section == b"diff" && named && matches!(var, b"textconv" | b"command"))
}

/// MARK-1: why not to run git, if the config that git lists for a repository (`git config --list
/// --show-scope --show-origin -z`: `scope NUL origin NUL key [NL value] NUL` for each entry) has
/// an entry of the repository's own whose key can hold a command (`holds_command`): the first
/// such key and its file, cut to 30 columns each (the end of a key is its variable, the end of a
/// path its file). An included file has the scope of the file that includes it. The user's own
/// scopes (system, global, the command line) are trusted; any other scope, and a listing that
/// does not parse, counts as the repository's.
pub fn runs_commands(listing: &[u8]) -> Option<String> {
    let mut fields = listing.split(|&b| b == 0);
    while let Some(scope) = fields.next() {
        let (Some(origin), Some(entry)) = (fields.next(), fields.next()) else {
            return (!scope.is_empty()).then(|| UNREADABLE.into());
        };
        let key = entry.split(|&b| b == b'\n').next().unwrap_or_default();
        let trusted = matches!(scope, b"system" | b"global" | b"command");
        if holds_command(key) && !neutralized(key) && !trusted {
            let (key, origin) = (String::from_utf8_lossy(key), String::from_utf8_lossy(origin));
            let file = layout::short(origin.strip_prefix("file:").unwrap_or(&origin), 30);
            return Some(format!("{} in {file} could run a command", layout::tail(&key, 30)));
        }
    }
    None
}

/// GIT-1: where `core.hooksPath` points: `no-hooks` in the state directory, set by the shell at the
/// start (`use_no_hooks`); `/dev/null`, which can hold no hook either, where there is none.
static NO_HOOKS: OnceLock<PathBuf> = OnceLock::new();

/// GIT-1: the empty private directory for `core.hooksPath` in `state`: made again at every start
/// (so that nothing is in it), mode 700, inside a state directory that is ours (STATE-1).
pub fn no_hooks_dir(state: &Path) -> io::Result<PathBuf> {
    DirBuilder::new().recursive(true).mode(0o700).create(state)?;
    file::private(state)?;
    let dir = state.join("no-hooks");
    let _ = fs::remove_dir_all(&dir);
    DirBuilder::new().mode(0o700).create(&dir)?;
    Ok(dir)
}

/// GIT-1: the shell makes the directory once, before any git command is built.
pub fn use_no_hooks(state: &Path) {
    drop(no_hooks_dir(state).map(|d| NO_HOOKS.set(d)));
}

/// GIT-1: the start of every git command line of sim's: no shell; the monitor and the hooks off;
/// no optional lock (a status that does not write the index); `diff.submodule=short`, since a
/// repository's own `diff.submodule = diff` makes git start a child `git diff` in a submodule, under
/// the submodule's own config, which the guard never read (flags like `--no-ext-diff` do not reach
/// that child, and config does); the names after `--` are paths, never patterns or magic
/// (`GIT_LITERAL_PATHSPECS`: a file named `*` or `:(top)` would match the whole repository, the
/// submodules too); in English, so that "not a git repository" can be told from other failures; and
/// nothing fetched.
fn command(dir: &Path) -> Command {
    let hooks = NO_HOOKS.get().map_or(Path::new("/dev/null"), PathBuf::as_path);
    let mut c = Command::new("git");
    c.args(["-c", "core.fsmonitor=false", "-c", "diff.submodule=short", "-c"])
        .arg(format!("core.hooksPath={}", hooks.display()))
        .arg("--no-optional-locks")
        .current_dir(dir)
        .env("LC_ALL", "C")
        .env("GIT_LITERAL_PATHSPECS", "1")
        // Git never fetches for sim: in a partial clone with objects missing a `git status`
        // would, from the promisor remote, and run what the repository says serves a fetch (its
        // `uploadpack`, an `ext::` url) or ask for a password on the terminal sim draws on.
        .env("GIT_NO_LAZY_FETCH", "1")
        .env("GIT_ALLOW_PROTOCOL", "sim-none")
        .env("GIT_TERMINAL_PROMPT", "0");
    c
}

/// GIT-1: the flags a subcommand gets: no external diff and no text conversion where it diffs (a
/// repository can name both); no child git in a submodule (GIT-4): none of a diff's looks at one
/// (`all`: its patch would start a child git, and a file is never a submodule), and a status sees
/// a submodule's commit and not its work tree (`dirty`).
fn flags(cmd: &str) -> &'static [&'static str] {
    match cmd {
        "diff" | "show" | "log" => &["--no-ext-diff", "--no-textconv", "--ignore-submodules=all"],
        "status" => &["--ignore-submodules=dirty"],
        _ => &[],
    }
}

/// GIT-2: the `-c` arguments that turn off every filter that the repository's own scopes name (the
/// four keys of each: an empty `clean`, `smudge` and `process`, and `required=false`), in git's
/// listing; except git-lfs's own, if every entry of `filter.lfs` is exactly what `git lfs install`
/// writes (git then runs `git-lfs`, the user's program). The user's scopes are trusted: they are
/// no business of the command line's.
pub fn neutralizing(listing: &[u8]) -> Vec<String> {
    const LFS: [(&str, &str); 4] = [
        ("required", "true"),
        ("clean", "git-lfs clean -- %f"),
        ("smudge", "git-lfs smudge -- %f"),
        ("process", "git-lfs filter-process"),
    ];
    let mut names: Vec<(&str, bool)> = vec![]; // (name, every entry so far is git-lfs's own)
    let mut lfs_unsafe = false;
    let mut fields = listing.split(|&b| b == 0);
    while let (Some(scope), Some(_), Some(entry)) = (fields.next(), fields.next(), fields.next()) {
        let nl = entry.iter().position(|&b| b == b'\n').unwrap_or(entry.len());
        let (key, value) = (&entry[..nl], entry.get(nl + 1..).unwrap_or_default());
        let own_scope = !matches!(scope, b"system" | b"global" | b"command");
        lfs_unsafe |= own_scope && !lfs_harmless(key);
        let Some(name) = filter_name(key).filter(|_| own_scope) else {
            continue;
        };
        let var = key.rsplit(|&b| b == b'.').next().unwrap_or_default();
        let own =
            name == "lfs" && LFS.iter().any(|(k, v)| k.as_bytes() == var && v.as_bytes() == value);
        match names.iter_mut().find(|n| n.0 == name) {
            Some(n) => n.1 &= own,
            None => names.push((name, own)),
        }
    }
    // a setting of git-lfs's that the repository made which would let git-lfs write or run outside
    // it (its storage, an extension, a transfer program) takes the exception away, and the lfs
    // filter is turned off even where only the user's own config defines it
    if lfs_unsafe {
        match names.iter_mut().find(|n| n.0 == "lfs") {
            Some(n) => n.1 = false,
            None => names.push(("lfs", false)),
        }
    }
    let off = |name: &str| {
        ["clean=", "smudge=", "process=", "required=false"].map(|v| format!("filter.{name}.{v}"))
    };
    names
        .iter()
        .filter(|n| !n.1)
        .flat_map(|n| off(n.0))
        .flat_map(|k| ["-c".to_string(), k])
        .collect()
}

/// GIT-2: whether a key can stay in the repository's own config without taking away the exception for
/// git-lfs's filter: every key that is not one of git-lfs's (`lfs.*`), and of those the ones that cannot
/// make git-lfs write or run anything outside the repository: what git-lfs lets a `.lfsconfig` set
/// (`git-lfs-config(5)`, and its own warning "These unsafe '.lfsconfig' keys were ignored", which
/// names `lfs.storage`, `lfs.extension.*`, `lfs.customtransfer.*` and the like), and what `git lfs`
/// writes into `.git/config` itself: `lfs.repositoryformatversion`, and for a url `access` and
/// `locksverify`.
fn lfs_harmless(key: &[u8]) -> bool {
    const KEYS: [&str; 9] = [
        "allowincompletepush",
        "fetchexclude",
        "fetchinclude",
        "gitprotocol",
        "locksverify",
        "pushurl",
        "repositoryformatversion",
        "skipdownloaderrors",
        "url",
    ];
    let Some(rest) = key.strip_prefix(b"lfs.") else { return true };
    let var = rest.rsplit(|&b| b == b'.').next().unwrap_or_default();
    KEYS.iter().any(|k| k.as_bytes() == rest)
        || (rest.len() > var.len() && matches!(var, b"access" | b"locksverify"))
}

/// What git said when it failed, as sim says it: "not a git repository" is told from the rest.
fn failed(dir: &Path, cmd: &str, err: &[u8]) -> String {
    let err = String::from_utf8_lossy(err);
    let err = err.lines().next().unwrap_or_default();
    // (a bare repository, or inside `.git`, has no marks to give: it is no repository to this)
    if err.contains("not a git repository") || err.contains("must be run in a work tree") {
        format!("not a git repository: {}", dir.display())
    } else {
        format!("git {cmd}: {err}")
    }
}

/// Runs a git command and gives what it printed, or why it failed.
fn output(mut c: Command, dir: &Path, cmd: &str) -> Result<Vec<u8>, String> {
    let shown = layout::short(&dir.to_string_lossy(), 30); // (the reason is what matters: it is last)
    let out = c.output().map_err(|e| format!("cannot run git in {shown}: {}", why(&e)))?;
    if out.status.success() { Ok(out.stdout) } else { Err(failed(dir, cmd, &out.stderr)) }
}

/// GIT-1: the one place that builds the command line of a git command, from `args` (a subcommand
/// first): `command`, the neutralized filters of the repository (GIT-2) and the flags of the
/// subcommand. It is not built where the repository's config could make git run a command that
/// none of these stops (GIT-3; MARK-1's message names the key). The config is read by git itself, so
/// that sim cannot read it differently from git. Listing config runs nothing: `builtin/config.c`
/// only parses it and prints (the one command it can start is `config --edit`'s editor); hooks,
/// filters and the monitor belong to commands that use the index or the work tree. In a directory
/// that is no repository the list is the user's own and git then says so itself. An unreadable or
/// invalid config makes `config` fail, which counts as unsafe.
pub fn git_command(dir: &Path, args: &[&str]) -> Result<Command, String> {
    if !filter::programs_allowed() {
        return Err(PLAIN_RUNS_NOTHING.into());
    }
    let mut list = command(dir);
    list.args(["config", "--list", "--show-scope", "--show-origin", "-z"]);
    let list = output(list, dir, "config").map_err(|e| {
        if e.starts_with("git config:") { format!("{GIT_SKIPPED}{UNREADABLE}") } else { e }
    })?;
    if let Some(why) = runs_commands(&list) {
        return Err(format!("{GIT_SKIPPED}{why}"));
    }
    let (cmd, rest) = args.split_first().ok_or("no git command")?;
    let mut c = command(dir);
    c.args(neutralizing(&list)).arg(cmd).args(flags(cmd)).args(rest);
    Ok(c)
}

/// Runs git in `dir` as `git_command` builds it.
fn git(dir: &Path, args: &[&str]) -> Result<Vec<u8>, String> {
    output(git_command(dir, args)?, dir, args[0])
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

/// UI-5: what a file differs from HEAD by, as `git diff -U0` says it, for the signs of the sign
/// column and the counts of the status line. It runs through the same guard and the same hardened
/// environment as every git of sim's (`git`, `run`): no external diff and no text conversion
/// (those are commands a repository can name), no color, and nothing fetched.
pub fn git_diff(dir: &Path, name: &str) -> Result<Vec<u8>, String> {
    git(dir, &git_diff_args(name))
}

/// UI-5: the arguments of that diff; `git_command` adds the flags of every diff of sim's (GIT-1: no
/// external diff, no text conversion, and no submodule looked at: a file is never one, and no child
/// git runs, nor reads a submodule's own config).
pub fn git_diff_args(name: &str) -> [&str; 6] {
    ["diff", "--no-color", "-U0", "HEAD", "--", name]
}

/// UI-5: the diff of the file a request names (its path is relative to `cwd`, as its App has it).
pub fn diff(cwd: &Path, req: &DiffReq) -> Result<Vec<u8>, String> {
    let at = cwd.join(&req.path);
    let name = at.file_name().map(|n| n.to_string_lossy().into_owned());
    let (Some(dir), Some(name)) = (at.parent(), name) else { return Err("no file".into()) };
    git_diff(dir, &name)
}

/// UI-5: the thread that asks git for the diffs of the files, off the main thread (P4). Of the
/// requests waiting for it only the newest for each file is run, since the others are about a text
/// that is gone; the answers are `SimEvent::Diff`.
pub fn diff_thread(tx: Sender<SimEvent>, cwd: PathBuf) -> Sender<DiffReq> {
    let (jobs, rx) = mpsc::channel::<DiffReq>();
    thread::spawn(move || {
        while let Ok(first) = rx.recv() {
            let mut latest = HashMap::<Id, DiffReq>::new();
            for r in std::iter::once(first).chain(rx.try_iter()) {
                latest.insert(r.id, r);
            }
            for (_, r) in latest {
                if tx.send(SimEvent::Diff(r.id, diff(&cwd, &r), r.lines)).is_err() {
                    return;
                }
            }
        }
    });
    jobs
}

/// DIFF-2: the thread that asks git for the numbers of the changes view, off the main thread (P4): of
/// the requests waiting only the newest is run (the newest has the newest list of untracked files).
pub fn changes_thread(tx: Sender<SimEvent>, root: PathBuf) -> Sender<Vec<PathBuf>> {
    let (jobs, rx) = mpsc::channel::<Vec<PathBuf>>();
    thread::spawn(move || {
        while let Ok(first) = rx.recv() {
            let newest = rx.try_iter().last().unwrap_or(first);
            if tx.send(SimEvent::Changes(changes(&root, &newest))).is_err() {
                return;
            }
        }
    });
    jobs
}

/// DIFF-3: HEAD's version of `path`, asked of git in a thread of its own (a blob can be big).
pub fn show_thread(tx: Sender<SimEvent>, root: PathBuf, path: String) {
    thread::spawn(move || drop(tx.send(SimEvent::Shown(path.clone(), show_head(&root, &path)))));
}

/// UI-3: the entries git ignores, as `!! path` lines of `git status --porcelain=v1 -z`: with
/// `--untracked-files=normal`, so that an ignored directory is one entry and its files are not
/// listed (with `-uall` git lists them all). Only those lines are kept.
pub fn git_ignored(dir: &Path) -> Result<Vec<u8>, String> {
    let args =
        ["status", "--porcelain=v1", "-z", "--ignored=traditional", "--untracked-files=normal"];
    let out = git(dir, &args)?;
    let mut kept = vec![];
    for field in out.split(|&b| b == 0).filter(|f| f.starts_with(b"!! ")) {
        kept.extend_from_slice(field);
        kept.push(0);
    }
    Ok(kept)
}

/// DIFF-2: the git tree that every tree is compared with when there is no HEAD yet (no commit).
const EMPTY_TREE: &str = "4b825dc642cb6eb9a060e54bf8d69288fbee4904";

/// DIFF-2: at most this many untracked files have their lines counted, none over `COUNT_MAX` bytes.
const COUNTED: usize = 1_000;
const COUNT_MAX: u64 = 1 << 20;

/// DIFF-3: a blob over this many bytes is not shown.
const SHOW_MAX: usize = 20 << 20;

/// DIFF-2: the numbers of the changes view: git's `--numstat` of the work tree and the index
/// against HEAD (against the empty tree before the first commit), without submodules (GIT-4: a
/// letter and no counts), and the lines of the `untracked` files (relative to `root`).
pub fn changes(root: &Path, untracked: &[PathBuf]) -> Result<Counts, String> {
    let args = |base: &'static str| ["diff", "--numstat", "-z", base];
    let numstat = git(root, &args("HEAD"))
        .or_else(|e| if e.contains("'HEAD'") { git(root, &args(EMPTY_TREE)) } else { Err(e) })?;
    let count = |p: &PathBuf| {
        let at = root.join(p);
        let small = fs::metadata(&at).is_ok_and(|m| m.is_file() && m.len() <= COUNT_MAX);
        (p.clone(), small.then(|| fs::read(&at).ok().and_then(|b| changes::lines(&b))).flatten())
    };
    Ok(Counts { numstat, lines: untracked.iter().take(COUNTED).map(count).collect() })
}

/// DIFF-3: HEAD's version of a file (`path` relative to `root`): `git show HEAD:./path` through the
/// guard, so that no text conversion runs and the repository's filters are off.
pub fn show_head(root: &Path, path: &str) -> Result<Vec<u8>, String> {
    let blob = git(root, &["show", &format!("HEAD:./{path}")])?;
    if blob.len() > SHOW_MAX { Err(format!("HEAD:{path} is over 20 MiB")) } else { Ok(blob) }
}

/// MARK-1: git's status and where `root` lies below the top of the repository, the branch (UI-10, a
/// `## branch` entry first) and (UI-3) what it ignores, appended as `!! path` entries (a failure of that alone leaves the status as it is).
pub fn status(root: &Path) -> Result<(Vec<u8>, String), String> {
    let mut status = git_status(root)?;
    // UI-10: the branch, as the `## branch` entry `git status --branch` would have put first; a
    // detached HEAD is `HEAD`. (`git_status` itself is as it was.)
    let branch = git(root, &["symbolic-ref", "--short", "-q", "HEAD"]);
    let branch = branch.map_or("HEAD".into(), |b| String::from_utf8_lossy(&b).trim().to_string());
    status.splice(0..0, format!("## {branch}\0").into_bytes());
    if !status.is_empty() && !status.ends_with(&[0]) {
        status.push(0);
    }
    status.extend(git_ignored(root).unwrap_or_default());
    Ok((status, git_prefix(root)?))
}

/// The history saved in `dir`; empty if there is none, or it cannot be read.
pub fn load_history(dir: &Path) -> History {
    fs::read_to_string(dir.join("history"))
        .map_or_else(|_| History::default(), |text| History::parse(&text))
}

/// LOOK-1: the look saved in `dir` (`save_look`); depth when there is none, or it says anything else.
pub fn load_look(dir: &Path) -> Skin {
    fs::read_to_string(dir.join("look")).ok().and_then(|t| Skin::parse(&t)).unwrap_or(Skin::Depth)
}

pub fn save_look(dir: &Path, skin: Skin) -> Result<(), String> {
    save_state(dir, "look", &format!("{}\n", skin.name()))
}

/// UI-2, ICON-1: the choice of the icons as saved in `dir` (`save_icons`), `on` or `off`; none if
/// there is no choice yet (or the file says anything else).
pub fn load_icons(dir: &Path) -> Option<bool> {
    match fs::read_to_string(dir.join("icons")).ok()?.trim_end_matches('\n') {
        "on" => Some(true),
        "off" => Some(false),
        _ => None,
    }
}

/// ICON-1: what the icons start as when nothing was chosen.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum Icons {
    On,
    Off,
    /// Off, for want of a font: once, a hint says how to turn them on.
    Hint,
}

/// ICON-1: a console, a dumb terminal (`linux`, `dumb`, `vt…`) and a locale that is not UTF-8 never
/// get icons. Otherwise they start on where the terminal brings Nerd Font symbols with it, which
/// its environment shows: Ghostty (`TERM_PROGRAM=ghostty`, `GHOSTTY_RESOURCES_DIR`), kitty
/// (`KITTY_PID`) and WezTerm (`TERM_PROGRAM=WezTerm`, `WEZTERM_PANE`); tmux, which hides
/// `TERM_PROGRAM`, passes the others on. Anywhere else they start off, with the hint.
pub fn icons_by(var: impl Fn(&str) -> Option<String>) -> Icons {
    let term = var("TERM").unwrap_or_default();
    let locale =
        ["LC_ALL", "LC_CTYPE", "LANG"].iter().find_map(|k| var(k).filter(|v| !v.is_empty()));
    let utf8 = locale
        .is_some_and(|l| l.to_lowercase().contains("utf-8") || l.to_lowercase().contains("utf8"));
    let program = var("TERM_PROGRAM").unwrap_or_default();
    let brings = matches!(program.as_str(), "ghostty" | "WezTerm")
        || ["GHOSTTY_RESOURCES_DIR", "KITTY_PID", "WEZTERM_PANE"].iter().any(|k| var(k).is_some());
    match () {
        () if matches!(term.as_str(), "linux" | "dumb") || term.starts_with("vt") || !utf8 => {
            Icons::Off
        }
        () if brings => Icons::On,
        () => Icons::Hint,
    }
}

/// ICON-1: whether the icons start on, and whether to say how to turn them on. What `zi` chose
/// last (`dir/icons`) wins; else `icons_by`. The hint is due once: its marker is written here, and
/// without a state directory there is nowhere to remember that it was given, so it is not. In plain
/// mode `zi` is refused, so there is no hint, and the marker is neither read nor written.
pub fn start_icons(
    dir: Option<&Path>,
    plain: bool,
    var: impl Fn(&str) -> Option<String>,
) -> (bool, bool) {
    if let Some(on) = dir.and_then(load_icons) {
        return (on, false);
    }
    let hint =
        |d: &Path| !d.join("icons-hint").exists() && save_state(d, "icons-hint", "shown\n").is_ok();
    match icons_by(var) {
        Icons::On => (true, false),
        Icons::Off => (false, false),
        Icons::Hint => (false, !plain && dir.is_some_and(hint)),
    }
}

pub fn save_icons(dir: &Path, on: bool) -> Result<(), String> {
    save_state(dir, "icons", if on { "on\n" } else { "off\n" })
}

/// MD-5: whether the read view of Markdown is on, as saved in `dir` (`md`, `on` or `off`); on when
/// there is no choice yet, or the file says anything else.
pub fn load_md(dir: &Path) -> bool {
    !fs::read_to_string(dir.join("md")).is_ok_and(|t| t.trim_end_matches('\n') == "off")
}

pub fn save_md(dir: &Path, on: bool) -> Result<(), String> {
    save_state(dir, "md", if on { "on\n" } else { "off\n" })
}

/// KEY-6: how many times Tab and Shift-Tab were used, as saved in `dir` (`tab-uses`): a number, at most 3.
pub fn load_tab_uses(dir: &Path) -> u8 {
    let text = fs::read_to_string(dir.join("tab-uses")).unwrap_or_default();
    text.trim().parse::<u8>().map_or(0, |n| n.min(3))
}

pub fn save_tab_uses(dir: &Path, n: u8) -> Result<(), String> {
    save_state(dir, "tab-uses", &format!("{n}\n"))
}

pub fn save_history(dir: &Path, history: &History) -> Result<(), String> {
    save_state(dir, "history", &history.serialize())
}

/// Saves the file `name` in `dir` (made with 0700 if needed): a temporary file (0600) is written
/// and synced, then renamed over the old one, so the file is always whole.
fn save_state(dir: &Path, name: &str, text: &str) -> Result<(), String> {
    let fail = |what: &str, e: std::io::Error| {
        format!("{name}: cannot {what} {}: {}", layout::short(&dir.to_string_lossy(), 30), why(&e))
    };
    if fs::symlink_metadata(dir).is_ok_and(|m| m.file_type().is_symlink()) {
        return Err(fail("use", io::Error::other("it is a link")));
    }
    DirBuilder::new().recursive(true).mode(0o700).create(dir).map_err(|e| fail("create", e))?;
    // SAFE-6, STATE-1: one state directory, one rule: a directory that is not private is not used
    // (and not made private: what is in it may have been readable).
    file::private(dir).map_err(|e| format!("{name}: {e}"))?;
    let tmp = dir.join(format!("{name}.{}.tmp", std::process::id()));
    let _ = fs::remove_file(&tmp);
    let write = || -> std::io::Result<()> {
        let mut f = OpenOptions::new().write(true).create_new(true).mode(0o600).open(&tmp)?;
        f.write_all(text.as_bytes())?;
        f.sync_all()?;
        fs::rename(&tmp, dir.join(name))
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

// ---- GREP-4, GREP-5, GREP-7, GREP-8: the contents of files ----

/// GREP-4: a file bigger than this is not previewed.
const PREVIEW_MAX: u64 = 2 * 1024 * 1024;
/// GREP-2: the built-in reader skips a file bigger than this.
const READ_MAX: u64 = 16 * 1024 * 1024;
/// GREP-5: a batch goes every so long, or after so many lines; rg is given the files in pieces
/// of at most this many bytes of paths (a command line has a limit).
const BATCH: Duration = Duration::from_millis(30);
const BATCH_LINES: usize = 200;
const PIECE: usize = 128 * 1024;

/// GREP-8: `rg` in the directories of `path` that are absolute. An empty or relative entry would
/// find a `rg` in the working directory, which can be the repository's own.
pub fn which_rg(path: Option<OsString>) -> Option<PathBuf> {
    let (path, runs) =
        (path?, |m: fs::Metadata| m.is_file() && m.permissions().mode() & 0o111 != 0);
    let dirs = env::split_paths(&path).filter(|d| d.is_absolute());
    dirs.map(|d| d.join("rg")).find(|f| fs::metadata(f).is_ok_and(runs))
}

/// GREP-7: which files may be read: regular files, not links, whose directory really lies below
/// the root (a link in the list, or a directory that has become one, leads out of the tree).
struct Inside {
    root: PathBuf,
    dirs: HashMap<String, bool>,
}

impl Inside {
    fn new(root: &Path) -> Inside {
        Inside {
            root: fs::canonicalize(root).unwrap_or_else(|_| root.into()),
            dirs: HashMap::new(),
        }
    }

    fn ok(&mut self, rel: &str) -> bool {
        let dir = rel.rsplit_once('/').map_or("", |d| d.0);
        let inside = *self.dirs.entry(dir.into()).or_insert_with(|| {
            fs::canonicalize(self.root.join(dir)).is_ok_and(|d| d.starts_with(&self.root))
        });
        inside && fs::symlink_metadata(self.root.join(rel)).is_ok_and(|m| m.is_file())
    }
}

/// GREP-4: the answer to `SimEffect::Preview`: the file as the editor would read it, but no more
/// than `PREVIEW_MAX` bytes of it. A file that is not there is empty, as `:e` makes it.
pub fn preview(root: &Path, rel: &str) -> SimEvent {
    let read = || -> io::Result<Result<Option<Vec<u8>>, String>> {
        let mut f = File::open(root.join(rel))?;
        if f.metadata()?.len() > PREVIEW_MAX {
            return Ok(Err("too big to preview".into()));
        }
        let mut bytes = vec![];
        f.read_to_end(&mut bytes)?;
        Ok(Ok(Some(bytes)))
    };
    let file = match fs::symlink_metadata(root.join(rel)) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        _ if !Inside::new(root).ok(rel) => Err("not a file below the root".into()),
        _ => read().unwrap_or_else(|e| Err(why(&e))),
    };
    SimEvent::Preview(Pv { path: rel.into(), file })
}

/// GREP-5: what a search sends, in batches: groups are whole files, the first is sent as soon as
/// it is there, then one every `BATCH` or `BATCH_LINES`. It counts the lines against the cap.
struct Out<'a> {
    generation: u64,
    send: &'a mut dyn FnMut(Batch),
    pending: Vec<Group>,
    lines: usize,
    total: usize,
    last: Option<Instant>,
    capped: bool,
}

impl Out<'_> {
    /// Adds a line of file `path`; false at the cap: this one was the 1,001st.
    fn push(&mut self, path: &str, line: Line) -> bool {
        if self.total >= MAX_LINES {
            self.capped = true;
            return false;
        }
        if self.pending.last().is_none_or(|g| g.path != path) {
            self.pending.push(Group { path: path.into(), lines: vec![] });
        }
        if let Some(g) = self.pending.last_mut() {
            g.lines.push(line);
        }
        (self.total, self.lines) = (self.total + 1, self.lines + 1);
        true
    }

    /// A file is whole: send what is pending if it is time.
    fn end_file(&mut self) {
        let due = self.last.is_none_or(|t| t.elapsed() >= BATCH) || self.lines >= BATCH_LINES;
        if due && !self.pending.is_empty() {
            self.flush(false);
        }
    }

    fn flush(&mut self, done: bool) {
        (self.last, self.lines) = (Some(Instant::now()), 0);
        let groups = std::mem::take(&mut self.pending);
        (self.send)(Batch { generation: self.generation, groups, done, capped: self.capped });
    }
}

/// GREP-8: rg run in `root` on these files (relative to it) with nothing from the environment
/// (`RIPGREP_CONFIG_PATH` among it) and no config: the repository must not be able to make it run
/// a program (`--pre`). The query follows `-e` and the files `--`, so that neither is an option.
pub fn rg_command(rg: &Path, root: &Path, query: &str, files: &[&str]) -> Command {
    let mut c = Command::new(rg);
    c.env_clear()
        .current_dir(root)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    c.args(["--null", "--line-number", "--no-heading", "--with-filename", "--fixed-strings"]);
    c.args(["--smart-case", "--no-config", "--no-ignore", "--no-messages", "--color=never"]);
    c.args(["--max-columns=65536", "--max-columns-preview", "-e", query, "--"]).args(files);
    c
}

/// GREP-5: reads rg's output into `out` until it ends, the cap is reached or the search is old.
/// The notice rg prints for a binary file has no NUL and is not a line (GREP-2).
fn read_rg(
    r: &mut impl BufRead,
    q: &Query,
    live: &(dyn Fn() -> bool + Sync),
    out: &mut Out,
    stop: &AtomicBool,
) {
    let (mut buf, mut cur) = (vec![], String::new());
    while live() && r.read_until(b'\n', &mut buf).is_ok_and(|n| n > 0) {
        let line = buf.strip_suffix(b"\n").unwrap_or(&buf);
        if let Some((path, no, text)) = parse_rg(line) {
            let path = String::from_utf8_lossy(path);
            if *path != cur {
                out.end_file();
                cur = path.into_owned();
            }
            if !out.push(&cur, q.line(no, &String::from_utf8_lossy(text))) {
                return stop.store(true, SeqCst);
            }
        }
        buf.clear();
    }
    out.end_file();
}

/// GREP-5: one piece of the list searched by rg. A thread kills the process when the search is
/// old or over. `None` if rg could not be started, or failed (exit code 2 or more, or a signal)
/// before it said anything: the built-in reader goes on with the piece.
fn run_rg(
    rg: &Path,
    q: &Query,
    req: &Request,
    files: &[&str],
    live: &(dyn Fn() -> bool + Sync),
    out: &mut Out,
) -> Option<()> {
    if !filter::programs_allowed() {
        return None; // plain mode: the built-in reader does it
    }
    let mut child = rg_command(rg, &req.root, &req.query, files).spawn().ok()?;
    let mut stdout = BufReader::new(child.stdout.take()?);
    let (stop, over, before) = (AtomicBool::new(false), AtomicBool::new(false), out.total);
    let status = thread::scope(|s| {
        let reaper = s.spawn(|| {
            while !stop.load(SeqCst) && live() && !over.load(SeqCst) {
                thread::park_timeout(Duration::from_millis(5));
            }
            if !over.load(SeqCst) || stop.load(SeqCst) {
                drop(child.kill());
            }
            child.wait().ok()
        });
        read_rg(&mut stdout, q, live, out, &stop);
        over.store(true, SeqCst);
        reaper.thread().unpark();
        reaper.join().ok().flatten()
    });
    let fine = status.is_some_and(|s| s.code().is_some_and(|c| c <= 1));
    (fine || out.total > before || stop.load(SeqCst) || !live()).then_some(())
}

/// GREP-2: the built-in reader on one file: not binary (a NUL in its first 8 KiB), not too big.
fn scan_file(q: &Query, root: &Path, rel: &str, buf: &mut Vec<u8>, out: &mut Out) {
    let Ok(mut f) = File::open(root.join(rel)) else { return };
    buf.clear();
    let big = f.metadata().is_ok_and(|m| m.len() > READ_MAX);
    if big || (&mut f).take(HEAD as u64).read_to_end(buf).is_err() || is_binary(buf) {
        return;
    }
    if f.read_to_end(buf).is_ok() {
        for l in q.lines(&String::from_utf8_lossy(buf), MAX_LINES) {
            if !out.push(rel, l) {
                break;
            }
        }
        out.end_file();
    }
}

/// GREP-5: searches the files of the request, `first` before the rest of `files`, and sends the
/// matching lines in batches, ending with one that says `done` (and `capped`). `live` says
/// whether this is still the newest search: if not, it stops and says nothing more. Each piece of
/// the list is given to rg if there is one, else read here.
pub fn grep(
    req: &Request,
    rg: Option<&Path>,
    live: &(dyn Fn() -> bool + Sync),
    send: &mut dyn FnMut(Batch),
) {
    let Some(q) = Query::new(&req.query) else { return };
    let mut out = Out {
        generation: req.generation,
        send,
        pending: vec![],
        lines: 0,
        total: 0,
        last: None,
        capped: false,
    };
    let (mut inside, mut buf) = (Inside::new(&req.root), vec![]);
    let first: HashSet<&str> = req.first.iter().map(String::as_str).collect();
    let rest = req.files.iter().filter(|f| !first.contains(f.as_str()));
    let mut order = req.first.iter().chain(rest).map(String::as_str).peekable();
    while order.peek().is_some() && !out.capped {
        let (mut piece, mut size) = (vec![], 0);
        while size < PIECE
            && let Some(f) = order.next()
        {
            if inside.ok(f) {
                piece.push(f);
                size += f.len() + 1;
            }
        }
        if !live() {
            return;
        } else if piece.is_empty() {
            continue; // (rg given no files would search the working directory)
        }
        if rg.is_none_or(|rg| run_rg(rg, &q, req, &piece, live, &mut out).is_none()) {
            for f in piece {
                if !live() || out.capped {
                    break;
                }
                scan_file(&q, &req.root, f, &mut buf, &mut out);
            }
        }
        if !live() {
            return;
        }
    }
    out.flush(true);
}

/// GREP-5: starts searches. Each request has a thread of its own, which looks at the newest
/// generation as it goes and stops when its own is not it; a thread stuck on a file cannot hold
/// up the next search. Its answers are `SimEvent::Grep`.
pub struct Greps {
    tx: Sender<SimEvent>,
    latest: Arc<AtomicU64>,
    rg: Option<PathBuf>,
}

impl Greps {
    pub fn new(tx: Sender<SimEvent>, rg: Option<PathBuf>) -> Greps {
        Greps { tx, latest: Arc::new(AtomicU64::new(0)), rg }
    }

    pub fn ask(&self, req: Request) {
        self.latest.store(req.generation, SeqCst);
        let (tx, latest, rg) = (self.tx.clone(), self.latest.clone(), self.rg.clone());
        thread::spawn(move || {
            let live = || latest.load(SeqCst) == req.generation;
            grep(&req, rg.as_deref(), &live, &mut |b| drop(tx.send(SimEvent::Grep(b))));
        });
    }
}
