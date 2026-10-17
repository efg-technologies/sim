//! GUI-15
//! The few places this crate makes files or directories of its own (the demo project and journal
//! of the offscreen renders, the PNGs and tables of the shots), done so that a path another local
//! user can guess is never trusted: a directory is made with a fresh random name by `mkdir` (which
//! fails when the name exists, whatever it is, a symlink included) at mode 0700 and then checked;
//! a file is written to a name that did not exist and renamed over its target, so a symlink left at
//! the target is replaced and never followed. No dependency: `std` only.
use std::collections::hash_map::RandomState;
use std::fs::{self, DirBuilder, File, OpenOptions};
use std::hash::{BuildHasher, Hasher};
use std::io::{self, Write};
use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// 64 unpredictable bits: `RandomState` is seeded from the operating system's randomness; the time
/// and a counter only keep two calls apart.
fn random() -> u64 {
    static N: AtomicU64 = AtomicU64::new(0);
    let mut h = RandomState::new().build_hasher();
    h.write_u64(N.fetch_add(1, Ordering::Relaxed));
    h.write_u128(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos()),
    );
    h.finish()
}

/// The uid files made by this process get, found by making one in `dir` (which must be ours to write).
fn my_uid(dir: &Path) -> io::Result<u32> {
    let probe = dir.join(format!(".probe-{:016x}", random()));
    let f = OpenOptions::new().write(true).create_new(true).mode(0o600).open(&probe)?;
    let uid = f.metadata()?.uid();
    drop(f);
    fs::remove_file(&probe)?;
    Ok(uid)
}

/// Whether `dir` is a real directory (not a link to one), owned by this user and with exactly the
/// mode 0700.
pub fn check_private(dir: &Path) -> io::Result<()> {
    let md = fs::symlink_metadata(dir)?;
    let bad = |why: &str| {
        Err(io::Error::new(io::ErrorKind::PermissionDenied, format!("{}: {why}", dir.display())))
    };
    if !md.file_type().is_dir() {
        return bad("not a directory (a link is not one)");
    }
    if md.permissions().mode() & 0o777 != 0o700 {
        return bad("the mode is not 0700");
    }
    if md.uid() != my_uid(dir)? {
        return bad("not owned by this user");
    }
    Ok(())
}

/// A directory made here, with a name nobody can predict, at mode 0700, and removed with everything in
/// it when this is dropped.
pub struct PrivateDir {
    path: PathBuf,
}

impl PrivateDir {
    /// In the system's temporary directory (`$TMPDIR`, else `/tmp`).
    pub fn new(prefix: &str) -> io::Result<PrivateDir> {
        PrivateDir::new_in(&std::env::temp_dir(), prefix)
    }

    pub fn new_in(parent: &Path, prefix: &str) -> io::Result<PrivateDir> {
        let mut last = None;
        for _ in 0..64 {
            let path = parent.join(format!("{prefix}{:016x}", random()));
            // `mkdir` fails with AlreadyExists for any existing name, a dangling symlink included
            match DirBuilder::new().mode(0o700).create(&path) {
                Ok(()) => {
                    let dir = PrivateDir { path };
                    check_private(&dir.path)?;
                    return Ok(dir);
                }
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => last = Some(e),
                Err(e) => return Err(e),
            }
        }
        Err(last.unwrap_or_else(|| io::Error::other("no free name")))
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for PrivateDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

/// An output directory the user named: made at mode 0700 if it is not there, and refused if it is
/// there and is not a directory of this user's that nobody else can write to.
pub fn ensure_out_dir(dir: &Path) -> io::Result<()> {
    match DirBuilder::new().recursive(true).mode(0o700).create(dir) {
        Ok(()) => {}
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
        Err(e) => return Err(e),
    }
    let md = fs::metadata(dir)?; // (a link to a directory of the user's own is the user's choice)
    let bad = |why: &str| {
        Err(io::Error::new(io::ErrorKind::PermissionDenied, format!("{}: {why}", dir.display())))
    };
    if !md.is_dir() {
        return bad("not a directory");
    }
    if md.permissions().mode() & 0o022 != 0 {
        return bad("other users can write to it (chmod go-w, or name a directory of your own)");
    }
    if md.uid() != my_uid(dir)? {
        return bad("not owned by this user");
    }
    Ok(())
}

/// Writes a file by making a new one next to it (a name that did not exist, mode 0600) and renaming it
/// over `path`: whatever is at `path` (a symlink to somebody's file included) is replaced, not written
/// through.
pub fn write_replace(
    path: &Path,
    write: impl FnOnce(&mut File) -> io::Result<()>,
) -> io::Result<()> {
    let (dir, name) =
        (path.parent().unwrap_or(Path::new(".")), path.file_name().unwrap_or_default());
    for _ in 0..64 {
        let tmp = dir.join(format!(".{}.{:016x}.tmp", name.to_string_lossy(), random()));
        let mut f = match OpenOptions::new().write(true).create_new(true).mode(0o600).open(&tmp) {
            Ok(f) => f,
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        };
        let done = write(&mut f).and_then(|()| f.flush());
        drop(f);
        return match done.and_then(|()| fs::rename(&tmp, path)) {
            Ok(()) => Ok(()),
            Err(e) => {
                let _ = fs::remove_file(&tmp);
                Err(e)
            }
        };
    }
    Err(io::Error::other("no free temporary name"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;

    #[test]
    fn gui_15_a_private_dir_is_0700_ours_and_named_by_chance() {
        let (a, b) =
            (PrivateDir::new("sim-gui-test-").unwrap(), PrivateDir::new("sim-gui-test-").unwrap());
        assert_ne!(a.path(), b.path());
        for d in [&a, &b] {
            let md = fs::symlink_metadata(d.path()).unwrap();
            assert!(md.file_type().is_dir());
            assert_eq!(md.permissions().mode() & 0o777, 0o700);
            check_private(d.path()).unwrap();
        }
        // not made of anything a neighbour could know: no pid, no counter
        let name = a.path().file_name().unwrap().to_string_lossy().into_owned();
        assert!(!name.contains(&std::process::id().to_string()), "{name}");
        assert_eq!(name.len(), "sim-gui-test-".len() + 16);
    }

    #[test]
    fn gui_15_it_is_gone_when_dropped() {
        let d = PrivateDir::new("sim-gui-test-").unwrap();
        let p = d.path().to_path_buf();
        fs::write(p.join("x"), "x").unwrap();
        drop(d);
        assert!(!p.exists());
    }

    #[test]
    fn gui_15_a_name_that_exists_is_never_taken_over_and_a_link_is_not_a_private_dir() {
        let parent = PrivateDir::new("sim-gui-test-").unwrap();
        let victim = PrivateDir::new("sim-gui-victim-").unwrap();
        let link = parent.path().join("l");
        symlink(victim.path(), &link).unwrap();
        // mkdir on a name that is a symlink fails, so a planted link cannot be the directory
        let e = DirBuilder::new().mode(0o700).create(&link).unwrap_err();
        assert_eq!(e.kind(), io::ErrorKind::AlreadyExists);
        assert!(check_private(&link).is_err(), "a link to a good directory is still a link");
        // a directory with a wider mode is refused
        let open = parent.path().join("open");
        DirBuilder::new().mode(0o755).create(&open).unwrap();
        fs::set_permissions(&open, fs::Permissions::from_mode(0o755)).unwrap();
        assert!(check_private(&open).is_err());
        assert_eq!(fs::read_dir(victim.path()).unwrap().count(), 0);
    }

    #[test]
    fn gui_15_a_written_file_replaces_a_link_at_its_target_and_does_not_write_through_it() {
        let d = PrivateDir::new("sim-gui-test-").unwrap();
        let victim = d.path().join("victim");
        fs::write(&victim, "precious").unwrap();
        let target = d.path().join("out.png");
        symlink(&victim, &target).unwrap();
        write_replace(&target, |f| f.write_all(b"new")).unwrap();
        assert_eq!(fs::read_to_string(&victim).unwrap(), "precious");
        assert_eq!(fs::read_to_string(&target).unwrap(), "new");
        assert!(!fs::symlink_metadata(&target).unwrap().file_type().is_symlink());
        // mode 0600, and no temporary file is left
        assert_eq!(fs::metadata(&target).unwrap().permissions().mode() & 0o777, 0o600);
        assert_eq!(fs::read_dir(d.path()).unwrap().count(), 2);
    }

    #[test]
    fn gui_15_a_failed_write_leaves_nothing_and_the_old_file_stays() {
        let d = PrivateDir::new("sim-gui-test-").unwrap();
        let target = d.path().join("t");
        fs::write(&target, "old").unwrap();
        assert!(write_replace(&target, |_| Err(io::Error::other("no"))).is_err());
        assert_eq!(fs::read_to_string(&target).unwrap(), "old");
        assert_eq!(fs::read_dir(d.path()).unwrap().count(), 1);
    }

    #[test]
    fn gui_15_an_output_dir_is_made_at_0700_and_one_others_can_write_to_is_refused() {
        let d = PrivateDir::new("sim-gui-test-").unwrap();
        let out = d.path().join("a/b/out");
        ensure_out_dir(&out).unwrap();
        assert_eq!(fs::metadata(&out).unwrap().permissions().mode() & 0o777, 0o700);
        ensure_out_dir(&out).unwrap(); // again: fine
        let wide = d.path().join("wide");
        DirBuilder::new().mode(0o777).create(&wide).unwrap();
        fs::set_permissions(&wide, fs::Permissions::from_mode(0o777)).unwrap();
        assert!(ensure_out_dir(&wide).is_err());
    }
}
