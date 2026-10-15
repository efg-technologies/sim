//! SAFE-1, SAFE-2, SAFE-3, SAFE-4, SAFE-5, SAFE-6
//! Reading a file, atomic save, comparison by hash right before the rename, keeping attributes.
use super::journal::NO_STATE;
use crate::core::app::Read;
use crate::core::buffer::{ABSENT, hash};
#[cfg(test)]
use std::cell::Cell;
use std::fs::{self, DirBuilder, File, Metadata, OpenOptions, Permissions};
use std::io::{self, Write};
use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt, fchown};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering::Relaxed};

pub enum Saved {
    /// The hash of what was written, and a warning if something after the rename failed.
    Done(u64, Option<String>),
    /// Nothing was written: the disk holds this (`None`: no file).
    Conflict(Option<Vec<u8>>),
}

static SEQ: AtomicUsize = AtomicUsize::new(0);

/// What a save is to write: the target, the bytes, and the hash the disk must still have.
type Job<'a> = (&'a Path, &'a [u8], Option<u64>);

#[cfg(test)]
thread_local!(static FAULT: Cell<(usize, usize)> = const { Cell::new((usize::MAX, 0)) });

/// Runs one step of a save; under test, the step number `FAULT.0` fails instead (SAFE-1).
#[cfg(test)]
fn step<T>(run: impl FnOnce() -> io::Result<T>) -> io::Result<T> {
    let (at, n) = FAULT.get();
    FAULT.set((at, n + 1));
    if at == n { Err(io::Error::other("injected fault")) } else { run() }
}

#[cfg(not(test))]
fn step<T>(run: impl FnOnce() -> io::Result<T>) -> io::Result<T> {
    run()
}

/// A name that is unique in this process and among processes.
fn uniq() -> String {
    format!("{}-{}", std::process::id(), SEQ.fetch_add(1, Relaxed))
}

/// An I/O error without its "(os error N)" tail.
pub fn why(e: &io::Error) -> String {
    let s = e.to_string();
    s.split(" (os error").next().unwrap_or(&s).to_string()
}

/// Follows symlinks, so a link is saved through to its target and stays a link. A dangling link
/// is followed to its missing target, which the save creates.
pub fn resolve(p: &str) -> PathBuf {
    let mut abs = std::path::absolute(p).unwrap_or_else(|_| p.into());
    for _ in 0..40 {
        let Ok(to) = fs::read_link(&abs) else { break };
        abs = abs.parent().unwrap_or(&abs).join(to);
    }
    let beside = || {
        fs::canonicalize(abs.parent()?).ok().map(|d| d.join(abs.file_name().unwrap_or_default()))
    };
    fs::canonicalize(&abs).ok().or_else(beside).unwrap_or(abs)
}

/// `Ok(None)` for a file that does not exist.
pub(super) fn opt<T>(r: io::Result<T>) -> io::Result<Option<T>> {
    match r {
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        r => r.map(Some),
    }
}

/// Reads a file: `Ok(None)` if there is no such file yet (SAFE-4 tells why it cannot be read).
pub fn read(path: &str) -> Read {
    let bad = |e: io::Error| format!("cannot read: {}", why(&e));
    match opt(fs::metadata(path)).map_err(bad)? {
        Some(m) if m.is_dir() => Err(format!("{path} is a directory")),
        Some(m) if !m.is_file() => Err(format!("{path} is not a regular file")),
        Some(_) => fs::read(path).map(Some).map_err(bad),
        None => Ok(None),
    }
}

/// SAFE-6: `dir` must be private to us, a real directory (not a symlink) that group and others
/// cannot enter. Returns our uid. libc is not needed for it: it is the owner of a file we create.
pub fn private(dir: &Path) -> io::Result<u32> {
    let m = fs::symlink_metadata(dir)?;
    let probe = dir.join(format!(".owner-{}", uniq()));
    let mine = OpenOptions::new().write(true).create_new(true).mode(0o600).open(&probe);
    let mine = mine.and_then(|f| f.metadata());
    drop(fs::remove_file(&probe));
    let uid = mine?.uid();
    if m.is_dir() && m.uid() == uid && m.mode() & 0o077 == 0 {
        return Ok(uid);
    }
    Err(io::Error::other(format!("{} is not private (yours, mode 700)", dir.display())))
}

/// What a file is without its content: device, inode, size, mtime and ctime.
type Stamp = (u64, u64, u64, i64, i64, i64, i64);

/// The stamp of the file (`None`: there is none).
fn stamp(t: &Path) -> io::Result<Option<Stamp>> {
    let of = |m: Metadata| {
        (m.dev(), m.ino(), m.size(), m.mtime(), m.mtime_nsec(), m.ctime(), m.ctime_nsec())
    };
    Ok(opt(fs::metadata(t))?.map(of))
}

/// Whether the disk no longer hashes to `expect` (`ABSENT`: no file), and what it holds then. It is
/// asked right before the file is replaced (SAFE-5). Reading and hashing a big file takes time, so
/// the file's stamp is taken before and checked again after, and the file is read again (once) only
/// if the stamp moved. The rename follows the check at once.
fn changed(t: &Path, expect: Option<u64>) -> io::Result<Option<Option<Vec<u8>>>> {
    let Some(want) = expect else { return Ok(None) };
    for tries in 1.. {
        let before = stamp(t)?;
        let disk = opt(fs::read(t))?;
        let differs = disk.as_deref().map_or(ABSENT, hash) != want;
        if differs || tries == 2 || stamp(t)? == before {
            return Ok(differs.then_some(disk));
        }
    }
    Ok(None)
}

/// Writes `bytes` to `path`. With `expect`, nothing is written if the disk no longer hashes to
/// it (`ABSENT` for no file), and a file without write permission is not written over. Without it
/// (`:w!`) both are overwritten. The compare is the last step before the rename (SAFE-5). `state`
/// is where in-place saves keep their recovery copy (`None`: there is no state directory).
#[rustfmt::skip]
pub fn save(path: &str, bytes: &[u8], expect: Option<u64>, state: Option<&Path>) -> Result<Saved, String> {
    let t = resolve(path);
    let fail = |e: io::Error| format!("cannot save {path}: {}", why(&e));
    let meta = opt(fs::metadata(&t)).map_err(fail)?;
    if expect.is_some() && meta.as_ref().is_some_and(|m| m.permissions().readonly()) {
        return Err("read-only file (:w! to write anyway)".into());
    }
    if meta.as_ref().is_none_or(|m| m.nlink() < 2)
        && let Some(done) = atomic((&t, bytes, expect), meta.as_ref()).map_err(fail)?
    {
        return Ok(done);
    }
    in_place((&t, bytes, expect), state)
}

/// SAFE-1: temp file next to the target, fsync, compare with the disk, rename, fsync of the
/// directory. `Ok(None)` means the in-place path (SAFE-2) has to be used instead. Once the rename
/// has happened the save is done: a failing sync of the directory is only a warning.
fn atomic((t, bytes, expect): Job, meta: Option<&Metadata>) -> io::Result<Option<Saved>> {
    let name = t.file_name().unwrap_or_default().to_string_lossy();
    let tmp = t.with_file_name(format!(".{name}.si-{}", uniq()));
    let mode = if meta.is_some() { 0o600 } else { 0o666 };
    let mut f = match step(|| OpenOptions::new().write(true).create_new(true).mode(mode).open(&tmp))
    {
        Ok(f) => f,
        Err(_) if meta.is_some() => return Ok(None),
        Err(e) => return Err(e),
    };
    let r = (|| -> io::Result<Option<Saved>> {
        if let Some(m) = meta {
            let keep = step(|| {
                if f.metadata()?.uid() != m.uid() {
                    return Err(io::Error::other("file belongs to someone else"));
                }
                if f.metadata()?.gid() != m.gid() {
                    fchown(&f, None, Some(m.gid()))?;
                }
                f.set_permissions(Permissions::from_mode(m.mode() & 0o7777))
            });
            if keep.is_err() {
                return Ok(None);
            }
        }
        step(|| f.write_all(bytes))?;
        step(|| f.sync_all())?;
        if let Some(disk) = step(|| changed(t, expect))? {
            return Ok(Some(Saved::Conflict(disk)));
        }
        step(|| fs::rename(&tmp, t))?;
        let sync = step(|| File::open(t.parent().unwrap_or(t))?.sync_all());
        let warn = sync.err().map(|e| format!("could not sync the directory: {}", why(&e)));
        Ok(Some(Saved::Done(hash(bytes), warn)))
    })();
    if !matches!(r, Ok(Some(Saved::Done(..)))) {
        let _ = fs::remove_file(&tmp);
    }
    r
}

/// SAFE-2: write over the file itself, after a synced recovery copy of the new text.
fn in_place((t, bytes, expect): Job, state: Option<&Path>) -> Result<Saved, String> {
    let (name, shown) = (t.file_name().unwrap_or_default().to_string_lossy(), t.display());
    let Some(state) = state else {
        return Err(format!("cannot save {shown}: no recovery copy possible ({NO_STATE})"));
    };
    let copy = state.join(format!("{name}.{}.recovery", uniq()));
    let gone = || drop(fs::remove_file(&copy));
    let made = step(|| {
        DirBuilder::new().recursive(true).mode(0o700).create(state)?;
        private(state)?;
        let mut c = OpenOptions::new().write(true).create_new(true).mode(0o600).open(&copy)?;
        c.write_all(bytes)?;
        step(|| c.sync_all())
    });
    let fail = |e: io::Error| format!("cannot save {shown}: {}", why(&e));
    if let Err(e) = made {
        gone();
        return Err(format!("cannot save {shown}: no recovery copy possible ({})", why(&e)));
    }
    match step(|| changed(t, expect)) {
        Ok(None) => {}
        Ok(Some(disk)) => return Ok(Saved::Conflict(disk)).inspect(|_| gone()),
        Err(e) => return Err(fail(e)).inspect_err(|_| gone()),
    }
    let mut f = step(|| OpenOptions::new().write(true).truncate(true).open(t)).map_err(|e| {
        gone();
        fail(e)
    })?;
    let written = step(|| f.write_all(bytes)).and_then(|()| step(|| f.sync_all()));
    let lost = |e: io::Error| {
        format!("write to {shown} failed ({}); the unsaved text is in {}", why(&e), copy.display())
    };
    written.map_err(lost)?;
    gone();
    Ok(Saved::Done(hash(bytes), None))
}

#[cfg(test)]
mod tests {
    use super::*;

    const OLD: &[u8] = b"old\n";
    const NEW: &[u8] = b"new text\n";

    /// What a save left: its result, the temp files beside `f`, `f` and `other`, and the recovery
    /// copies as (path, content).
    type Out =
        (Result<Saved, String>, usize, Option<Vec<u8>>, Option<Vec<u8>>, Vec<(String, Vec<u8>)>);

    /// Saves NEW over a fresh `f` (holding OLD if `exists`; `other` is a second hard link to it if
    /// `linked`), with a private `state` directory, while step `fault` fails.
    fn put_failing(exists: bool, linked: bool, expect: Option<u64>, fault: usize) -> Out {
        let d = std::env::temp_dir().join(format!("si-fault-{}", uniq()));
        let (f, state) = (d.join("f"), d.join("state"));
        DirBuilder::new().recursive(true).mode(0o700).create(&state).unwrap();
        if exists {
            fs::write(&f, OLD).unwrap();
        }
        if linked {
            fs::hard_link(&f, d.join("other")).unwrap();
        }
        FAULT.set((fault, 0));
        let r = save(f.to_str().unwrap(), NEW, expect, Some(&state));
        FAULT.set((usize::MAX, 0));
        let names = fs::read_dir(&d).unwrap().map(|e| e.unwrap().file_name());
        let temps = names.filter(|n| n.to_string_lossy().starts_with(".f.si-")).count();
        let copy = |e: fs::DirEntry| (e.path().display().to_string(), fs::read(e.path()).unwrap());
        let copies = fs::read_dir(&state).unwrap().map(|e| copy(e.unwrap())).collect();
        let out = (r, temps, fs::read(&f).ok(), fs::read(d.join("other")).ok(), copies);
        fs::remove_dir_all(&d).unwrap();
        out
    }

    #[test]
    fn safe_1_every_step_leaves_old_or_new() {
        let mut warned = vec![];
        for exists in [true, false] {
            // Steps: open the temp file, [keep the mode], write, sync, compare, rename, sync the directory.
            let (last, first) = if exists { (6, 2) } else { (5, 0) };
            for expect in [None, Some(if exists { hash(OLD) } else { ABSENT })] {
                for k in 0..=last + 1 {
                    let (r, temps, got, ..) = put_failing(exists, false, expect, k);
                    let new = got.as_deref() == Some(NEW);
                    assert!(new || got == exists.then(|| OLD.to_vec()), "{exists} {k}: {got:?}");
                    assert!(temps == 0 && (r.is_err() || new), "{exists} {k}");
                    // A temp file that cannot be made or set up falls back to the in-place save.
                    assert_eq!(r.is_err(), (first..last).contains(&k), "{exists} {k}");
                    if matches!(r, Ok(Saved::Done(_, Some(_)))) {
                        warned.push((exists, k));
                        assert!(new, "a failing directory sync is only a warning");
                    }
                }
            }
        }
        assert_eq!(warned, [(true, 6), (true, 6), (false, 5), (false, 5)], "the last step");
    }

    /// SAFE-5: the compare comes after the temp file is written and synced, and right before the
    /// rename. The disk is not what the save expects, so it ends in a conflict at the compare: a step
    /// before that fails (an error), and a step after it never runs (a conflict all the same).
    #[test]
    fn safe_5_the_compare_is_the_last_step_before_the_rename() {
        let want = [true, true, false, false, false, true, true, true];
        for (k, conflict) in want.into_iter().enumerate() {
            let (r, temps, ..) = put_failing(true, false, Some(hash(b"other\n")), k);
            assert_eq!((matches!(r, Ok(Saved::Conflict(_))), temps), (conflict, 0), "step {k}");
        }
    }

    #[test]
    fn safe_2_hard_linked_file_is_written_in_place() {
        for expect in [None, Some(hash(OLD))] {
            for k in 0..=7 {
                let (r, temps, got, other, copies) = put_failing(true, true, expect, k);
                assert_eq!(temps, 0);
                match r {
                    Ok(_) => assert!(
                        got.as_deref() == Some(NEW) && other == got && copies.is_empty(),
                        "{k}"
                    ),
                    Err(e) if got.as_deref() == Some(OLD) => assert!(copies.is_empty(), "{k}: {e}"),
                    Err(e) => {
                        let (path, text) = copies.first().unwrap();
                        assert!(text == NEW && e.contains(path), "{k}: {e}");
                    }
                }
            }
        }
    }
}
