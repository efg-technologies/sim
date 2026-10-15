//! SAFE-1, SAFE-2, SAFE-3
//! Atomic save, comparison by hash, keeping attributes.
use crate::core::buffer::{ABSENT, hash};
use std::fs::{self, DirBuilder, File, Metadata, OpenOptions, Permissions};
use std::io::{self, Write};
use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt, fchown};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering::Relaxed};

pub enum Saved {
    /// The hash of what was written, and a warning if something after the rename failed.
    Done(u64, Option<String>),
    /// Nothing was written: the disk holds this.
    Conflict(Vec<u8>),
}

static SEQ: AtomicUsize = AtomicUsize::new(0);

#[cfg(test)]
thread_local!(static FAULT: std::cell::Cell<(usize, usize)> = const { std::cell::Cell::new((usize::MAX, 0)) });

/// Runs one step of a save; under test the step number `FAULT.0` fails instead.
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

/// Follows symlinks, so a link is saved through to its target and stays a link.
pub fn resolve(p: &str) -> PathBuf {
    let abs = std::path::absolute(p).unwrap_or_else(|_| p.into());
    let beside = || {
        fs::canonicalize(abs.parent()?).ok().map(|d| d.join(abs.file_name().unwrap_or_default()))
    };
    fs::canonicalize(&abs).ok().or_else(beside).unwrap_or(abs)
}

/// Writes `bytes` to `path`. With `expect`, nothing is written if the disk no longer hashes to
/// it (`ABSENT` for no file). `state` is where in-place saves keep their recovery copy.
pub fn save(path: &str, bytes: &[u8], expect: Option<u64>, state: &Path) -> Result<Saved, String> {
    let t = resolve(path);
    let fail = |e: io::Error| format!("cannot save {path}: {}", why(&e));
    let meta = match fs::metadata(&t) {
        Ok(m) => Some(m),
        Err(e) if e.kind() == io::ErrorKind::NotFound => None,
        Err(e) => return Err(fail(e)),
    };
    if let Some(want) = expect {
        let disk = meta.as_ref().map(|_| fs::read(&t)).transpose().map_err(fail)?;
        if disk.as_deref().map_or(ABSENT, hash) != want {
            return Ok(Saved::Conflict(disk.unwrap_or_default()));
        }
    }
    let mut warn = None;
    if !(meta.as_ref().is_none_or(|m| m.nlink() < 2)
        && atomic(&t, bytes, meta.as_ref(), &mut warn).map_err(fail)?)
    {
        in_place(&t, bytes, state)?;
    }
    Ok(Saved::Done(hash(bytes), warn))
}

/// SAFE-1: temp file next to the target, fsync, rename, fsync of the directory.
/// `Ok(false)` means the in-place path (SAFE-2) has to be used instead. Once the rename has
/// happened the save is done: a failing sync of the directory only sets `warn`.
fn atomic(
    t: &Path,
    bytes: &[u8],
    meta: Option<&Metadata>,
    warn: &mut Option<String>,
) -> io::Result<bool> {
    let name = t.file_name().unwrap_or_default().to_string_lossy();
    let tmp = t.with_file_name(format!(".{name}.si-{}", uniq()));
    let mode = if meta.is_some() { 0o600 } else { 0o666 };
    let mut f = match step(|| OpenOptions::new().write(true).create_new(true).mode(mode).open(&tmp))
    {
        Ok(f) => f,
        Err(_) if meta.is_some() => return Ok(false),
        Err(e) => return Err(e),
    };
    let r = (|| -> io::Result<bool> {
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
                return Ok(false);
            }
        }
        step(|| f.write_all(bytes))?;
        step(|| f.sync_all())?;
        step(|| fs::rename(&tmp, t))?;
        Ok(true)
    })();
    if !matches!(r, Ok(true)) {
        let _ = fs::remove_file(&tmp);
    } else if let Err(e) = step(|| File::open(t.parent().unwrap_or(t))?.sync_all()) {
        *warn = Some(format!("could not sync the directory: {}", why(&e)));
    }
    r
}

/// SAFE-2: write over the file itself, after a synced recovery copy of the new text.
fn in_place(t: &Path, bytes: &[u8], state: &Path) -> Result<(), String> {
    let name = t.file_name().unwrap_or_default().to_string_lossy();
    let copy = state.join(format!("{name}.{}.recovery", uniq()));
    let gone = || drop(fs::remove_file(&copy));
    let made = step(|| {
        DirBuilder::new().recursive(true).mode(0o700).create(state)?;
        let mut c = OpenOptions::new().write(true).create_new(true).mode(0o600).open(&copy)?;
        c.write_all(bytes)?;
        step(|| c.sync_all())
    });
    if let Err(e) = made {
        gone();
        return Err(format!(
            "cannot save {}: no recovery copy possible ({})",
            t.display(),
            why(&e)
        ));
    }
    let mut f = step(|| OpenOptions::new().write(true).truncate(true).open(t)).map_err(|e| {
        gone();
        format!("cannot save {}: {}", t.display(), why(&e))
    })?;
    let written = step(|| f.write_all(bytes)).and_then(|()| step(|| f.sync_all()));
    written.map_err(|e| {
        format!(
            "write to {} failed ({}); the unsaved text is in {}",
            t.display(),
            why(&e),
            copy.display()
        )
    })?;
    gone();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const OLD: &[u8] = b"old\n";
    const NEW: &[u8] = b"new text\n";

    /// A fresh directory holding `f` (OLD) and a `state` directory.
    fn setup() -> (PathBuf, PathBuf) {
        let d = std::env::temp_dir().join(format!("si-test-{}", uniq()));
        fs::create_dir_all(d.join("state")).unwrap();
        fs::write(d.join("f"), OLD).unwrap();
        (d.join("f"), d.join("state"))
    }

    /// Saves NEW over `f` with the step `k` failing.
    fn put(f: &Path, state: &Path, k: usize) -> Result<Saved, String> {
        FAULT.set((k, 0));
        save(f.to_str().unwrap(), NEW, None, state)
    }

    fn temps(f: &Path) -> usize {
        let names = fs::read_dir(f.parent().unwrap()).unwrap().map(|e| e.unwrap().file_name());
        names.filter(|n| n.to_string_lossy().starts_with(".f.si-")).count()
    }

    #[test]
    fn safe_1_every_step_leaves_old_or_new() {
        let mut warned = vec![];
        for exists in [true, false] {
            for k in 0..=7 {
                let (f, state) = setup();
                if !exists {
                    fs::remove_file(&f).unwrap();
                }
                let r = put(&f, &state, k);
                let got = fs::read(&f).ok();
                let new = got.as_deref() == Some(NEW);
                assert!(new || got == exists.then(|| OLD.to_vec()), "{exists} {k}: {got:?}");
                assert!(
                    temps(&f) == 0 && (r.is_err() || new) && (k < 6 || r.is_ok()),
                    "{exists} {k}"
                );
                if matches!(r, Ok(Saved::Done(_, Some(_)))) {
                    warned.push((exists, k));
                    assert!(new, "a failing directory sync is only a warning");
                }
                fs::remove_dir_all(f.parent().unwrap()).unwrap();
            }
        }
        assert_eq!(warned, [(true, 5), (false, 4)], "the last step is the directory sync");
    }

    #[test]
    fn safe_2_hard_linked_file_is_written_in_place() {
        for k in 0..=6 {
            let (f, state) = setup();
            let other = f.with_file_name("other");
            fs::hard_link(&f, &other).unwrap();
            let r = put(&f, &state, k);
            let got = fs::read(&f).unwrap();
            let copies: Vec<_> = fs::read_dir(&state).unwrap().map(|e| e.unwrap().path()).collect();
            assert_eq!(temps(&f), 0);
            match r {
                Ok(_) => assert!(
                    got == NEW && fs::read(&other).unwrap() == NEW && copies.is_empty(),
                    "{k}"
                ),
                Err(e) if got == OLD => assert!(copies.is_empty(), "{k}: {e}"),
                Err(e) => {
                    let c = copies.first().unwrap_or_else(|| panic!("{k}: no recovery copy"));
                    assert!(
                        fs::read(c).unwrap() == NEW && e.contains(c.to_str().unwrap()),
                        "{k}: {e}"
                    );
                }
            }
            fs::remove_dir_all(f.parent().unwrap()).unwrap();
        }
    }
}
