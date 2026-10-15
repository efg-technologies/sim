//! SAFE-6
//! Snapshots and recovery: one atomically written file per (path, pid) in the state directory.
use crate::core::buffer::hash;
use std::fs::{self, DirBuilder, File, OpenOptions};
use std::io::{self, Write};
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Snapshot {
    pub path: String,
    pub pid: u32,
    pub time_ms: u64,
    pub base: Vec<u8>,
    pub text: Vec<u8>,
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

/// Writes atomically into `state` (created 0700): temp file (0600, create_new) + fsync + rename.
/// One file per (path, pid): the name holds the FNV-1a-64 hash of `path` and the pid, so two
/// sessions on one file never overwrite each other.
pub fn write(state: &Path, s: &Snapshot) -> io::Result<()> {
    DirBuilder::new().recursive(true).mode(0o700).create(state)?;
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

/// The newest snapshot of `path` whose process is no longer running (and is not `own_pid`).
pub fn find(state: &Path, path: &str, own_pid: u32) -> Option<Snapshot> {
    let prefix = format!("{}-", stem(path));
    fs::read_dir(state)
        .ok()?
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().starts_with(&prefix))
        .filter_map(|e| decode(&fs::read(e.path()).ok()?))
        .filter(|s| s.path == path && s.pid != own_pid && !running(s.pid))
        .max_by_key(|s| s.time_ms)
}

/// Deletes the snapshot of (`path`, `pid`), if any.
pub fn remove(state: &Path, path: &str, pid: u32) {
    let _ = fs::remove_file(file(state, path, pid));
}
