//! SAFE-6
//! Snapshots and recovery: one atomically written file per (path, pid) in the state directory.
use super::file::private;
use crate::core::buffer::hash;
use std::ffi::OsString;
use std::fs::{self, DirBuilder, File, OpenOptions};
use std::io::{self, Write};
use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

#[rustfmt::skip]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Snapshot { pub path: String, pub pid: u32, pub time_ms: u64, pub base: Vec<u8>, pub text: Vec<u8> }

/// Why there are no snapshots and no recovery copies when `state_dir` finds no place for them.
pub const NO_STATE: &str = "no state directory: neither $XDG_STATE_HOME nor $HOME is set";

/// `$XDG_STATE_HOME/si`, else `$HOME/.local/state/si`. With neither set (or not an absolute path)
/// there is none, never a shared place such as /tmp: snapshots and recovery copies are off (SAFE-6).
pub fn state_dir(xdg: Option<OsString>, home: Option<OsString>) -> Option<PathBuf> {
    let var = |v: Option<OsString>| v.map(PathBuf::from).filter(|p| p.is_absolute());
    var(xdg).or_else(|| var(home).map(|h| h.join(".local/state"))).map(|d| d.join("si"))
}

// FNV-1a 64 of the path, in hex: stable across runs and Rust versions (unlike `DefaultHasher`).
fn stem(path: &str) -> String {
    format!("{:016x}", hash(path.as_bytes()))
}

/// Where the snapshot of (`path`, `pid`) lives in `state`.
pub fn file(state: &Path, path: &str, pid: u32) -> PathBuf {
    state.join(format!("{}-{pid}.snap", stem(path)))
}

// Format: a header line `si-snapshot 1 <pid> <time_ms> <path len> <base len> <text len>`, then the three byte strings.
fn encode(s: &Snapshot) -> Vec<u8> {
    let Snapshot { path, pid, time_ms, base, text } = s;
    let head =
        format!("si-snapshot 1 {pid} {time_ms} {} {} {}\n", path.len(), base.len(), text.len());
    [head.as_bytes(), path.as_bytes(), base, text].concat()
}

// None for anything that is not exactly one well-formed snapshot.
fn decode(data: &[u8]) -> Option<Snapshot> {
    let (head, rest) = data.split_at(data.iter().position(|&b| b == b'\n')? + 1);
    let mut f = std::str::from_utf8(head).ok()?.split_ascii_whitespace();
    (f.next()? == "si-snapshot" && f.next()? == "1").then_some(())?;
    let (pid, time_ms) = (f.next()?.parse().ok()?, f.next()?.parse().ok()?);
    let lens = [f.next()?, f.next()?, f.next()?].map(|n| n.parse::<usize>().ok());
    let [Some(p), Some(b), Some(t)] = lens else { return None };
    (f.next().is_none() && p.checked_add(b)?.checked_add(t)? == rest.len()).then_some(())?;
    let (path, rest) = rest.split_at(p);
    let (base, text) = rest.split_at(b);
    let path = String::from_utf8(path.into()).ok()?;
    Some(Snapshot { path, pid, time_ms, base: base.into(), text: text.into() })
}

// A process counts as running when it cannot be shown to be gone.
fn running(pid: u32) -> bool {
    if cfg!(target_os = "linux") {
        return Path::new(&format!("/proc/{pid}")).exists();
    }
    let kill = Command::new("kill").args(["-0", &pid.to_string()]).stderr(Stdio::null()).status();
    kill.map_or(true, |s| s.success())
}

/// Writes atomically into `state` (created 0700, and refused unless private): temp file (0600,
/// create_new) + fsync + rename.
/// One file per (path, pid): the name holds the FNV-1a-64 hash of `path` and the pid, so two
/// sessions on one file never overwrite each other.
pub fn write(state: &Path, s: &Snapshot) -> io::Result<()> {
    DirBuilder::new().recursive(true).mode(0o700).create(state)?;
    private(state)?;
    let dest = file(state, &s.path, s.pid);
    let tmp = dest.with_extension("tmp");
    let _ = fs::remove_file(&tmp); // left behind by a crashed session whose pid came round again
    let put = || -> io::Result<()> {
        let mut f = OpenOptions::new().write(true).create_new(true).mode(0o600).open(&tmp)?;
        f.write_all(&encode(s))?;
        f.sync_all()?;
        // The directory is synced to make the rename durable; if that fails the snapshot is still there.
        fs::rename(&tmp, &dest).inspect(|_| drop(File::open(state).and_then(|d| d.sync_all())))
    };
    put().inspect_err(|_| drop(fs::remove_file(&tmp)))
}

/// The newest snapshot of `path` whose process is no longer running (and is not `own_pid`). A
/// state directory that is not private, and files in it that are not ours, are not looked at.
pub fn find(state: &Path, path: &str, own_pid: u32) -> Option<Snapshot> {
    let (prefix, uid) = (format!("{}-", stem(path)), private(state).ok()?);
    fs::read_dir(state)
        .ok()?
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().starts_with(&prefix))
        .filter(|e| e.metadata().is_ok_and(|m| m.is_file() && m.uid() == uid))
        .filter_map(|e| decode(&fs::read(e.path()).ok()?))
        .filter(|s| s.path == path && s.pid != own_pid && !running(s.pid))
        .max_by_key(|s| s.time_ms)
}

/// Deletes the snapshot of (`path`, `pid`), if any.
pub fn remove(state: &Path, path: &str, pid: u32) {
    let _ = fs::remove_file(file(state, path, pid));
}
