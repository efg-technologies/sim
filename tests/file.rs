//! SAFE-1, SAFE-2, SAFE-3 on io::file: what a save leaves behind (the per-step faults are in
//! io/file.rs itself).
#![allow(clippy::unwrap_used)] // helpers; the tests themselves may unwrap
mod common;
use common::{TempDir, temp_dir};
use si::core::buffer::{ABSENT, hash};
use si::io::file::{Saved, save};
use std::fs::{self, File, OpenOptions, Permissions};
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt, symlink};
use std::path::{Path, PathBuf};

const OLD: &[u8] = b"old\n";
const NEW: &[u8] = b"new text\n";

/// A directory holding `f` (OLD) and an empty state directory.
fn setup(name: &str) -> (TempDir, PathBuf, PathBuf) {
    let d = temp_dir(name);
    fs::create_dir(d.0.join("state")).unwrap();
    fs::write(d.0.join("f"), OLD).unwrap();
    let (f, state) = (d.0.join("f"), d.0.join("state"));
    (d, f, state)
}

fn put(f: &Path, state: &Path) -> Result<Saved, String> {
    save(f.to_str().unwrap(), NEW, None, state)
}

fn mode(p: &Path) -> u32 {
    fs::metadata(p).unwrap().mode() & 0o7777
}

#[test]
fn safe_1_conflict_writes_nothing() {
    let (_d, f, state) = setup("conflict");
    let path = f.to_str().unwrap();
    assert!(
        matches!(save(path, NEW, Some(hash(b"x")), &state), Ok(Saved::Conflict(d)) if d == OLD)
    );
    assert_eq!(fs::read(&f).unwrap(), OLD);
    assert!(
        matches!(save(path, NEW, Some(hash(OLD)), &state), Ok(Saved::Done(h, None)) if h == hash(NEW))
    );
    let new = f.with_file_name("new");
    assert!(save(new.to_str().unwrap(), NEW, Some(ABSENT), &state).is_ok());
    assert!(matches!(
        save(new.to_str().unwrap(), OLD, Some(ABSENT), &state),
        Ok(Saved::Conflict(_))
    ));
    assert_eq!(fs::read(&new).unwrap(), NEW);
}

#[test]
fn safe_1_symlink_is_written_through_and_stays() {
    let (_d, f, state) = setup("symlink");
    let link = f.with_file_name("link");
    symlink(&f, &link).unwrap();
    save(link.to_str().unwrap(), NEW, None, &state).unwrap();
    assert!(fs::symlink_metadata(&link).unwrap().is_symlink());
    assert_eq!(fs::read(&f).unwrap(), NEW);
}

#[test]
fn safe_1_new_file_mode_follows_the_umask() {
    let (_d, f, state) = setup("umask");
    let (reference, new) = (f.with_file_name("reference"), f.with_file_name("new"));
    OpenOptions::new().write(true).create_new(true).mode(0o666).open(&reference).unwrap();
    save(new.to_str().unwrap(), NEW, None, &state).unwrap();
    assert_eq!(mode(&new), mode(&reference));
}

#[test]
fn safe_3_mode_is_preserved() {
    let (_d, f, state) = setup("mode");
    fs::set_permissions(&f, Permissions::from_mode(0o640)).unwrap();
    put(&f, &state).unwrap();
    assert_eq!((mode(&f), fs::read(&f).unwrap()), (0o640, NEW.to_vec()));
}

#[test]
fn safe_2_unwritable_directory_is_written_in_place() {
    let (_d, f, state) = setup("readonly-dir");
    let dir = f.parent().unwrap();
    fs::set_permissions(dir, Permissions::from_mode(0o555)).unwrap();
    let is_root = File::create(dir.join("probe")).is_ok(); // root ignores the mode
    let r = put(&f, &state);
    fs::set_permissions(dir, Permissions::from_mode(0o755)).unwrap();
    if !is_root {
        assert!(r.is_ok());
        assert_eq!(fs::read(&f).unwrap(), NEW);
        assert_eq!(fs::read_dir(&state).unwrap().count(), 0, "the recovery copy is gone");
    }
}
