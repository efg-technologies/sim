//! SAFE-1, SAFE-2, SAFE-3, SAFE-5 on io::file: what a save leaves behind. (What it leaves behind when
//! any one of its steps fails is tested inside io/file.rs, where the fault hook exists: `cfg(test)`.)
#![allow(clippy::unwrap_used, clippy::expect_used)] // helpers; the tests themselves may unwrap
mod common;
use common::{TempDir, temp_dir};
use si::core::buffer::{ABSENT, hash};
use si::io::file::{Saved, foreign_link, read, save};
use std::fs::{self, DirBuilder, File, OpenOptions, Permissions};
use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt, symlink};
use std::path::{Path, PathBuf};

const OLD: &[u8] = b"old\n";
const NEW: &[u8] = b"new text\n";

/// A directory holding `f` (OLD) and an empty state directory.
fn setup(name: &str) -> (TempDir, PathBuf, PathBuf) {
    let d = temp_dir(name);
    DirBuilder::new().mode(0o700).create(d.0.join("state")).unwrap();
    fs::write(d.0.join("f"), OLD).unwrap();
    let (f, state) = (d.0.join("f"), d.0.join("state"));
    (d, f, state)
}

fn put(f: &Path, state: &Path) -> Result<Saved, String> {
    save(f.to_str().unwrap(), NEW, None, Some(state))
}

/// The temporary files a save leaves beside `f`.
fn temps(f: &Path) -> usize {
    let names = fs::read_dir(f.parent().unwrap()).unwrap().map(|e| e.unwrap().file_name());
    names.filter(|n| n.to_string_lossy().starts_with(".f.si-")).count()
}

fn mode(p: &Path) -> u32 {
    fs::metadata(p).unwrap().mode() & 0o7777
}

#[test]
fn safe_1_conflict_writes_nothing() {
    let (_d, f, state) = setup("conflict");
    let path = f.to_str().unwrap();
    assert!(
        matches!(save(path, NEW, Some(hash(b"x")), Some(&state)), Ok(Saved::Conflict(d)) if d.as_deref() == Some(OLD))
    );
    assert_eq!(fs::read(&f).unwrap(), OLD);
    assert_eq!(temps(&f), 0, "the temporary file is gone when the compare before the rename fails");
    assert!(
        matches!(save(path, NEW, Some(hash(OLD)), Some(&state)), Ok(Saved::Done(h, None)) if h == hash(NEW))
    );
    let new = f.with_file_name("new");
    assert!(save(new.to_str().unwrap(), NEW, Some(ABSENT), Some(&state)).is_ok());
    assert!(matches!(
        save(new.to_str().unwrap(), OLD, Some(ABSENT), Some(&state)),
        Ok(Saved::Conflict(_))
    ));
    assert_eq!(fs::read(&new).unwrap(), NEW);
}

#[test]
fn safe_1_symlink_is_written_through_and_stays() {
    let (_d, f, state) = setup("symlink");
    let link = f.with_file_name("link");
    symlink(&f, &link).unwrap();
    save(link.to_str().unwrap(), NEW, None, Some(&state)).unwrap();
    assert!(fs::symlink_metadata(&link).unwrap().is_symlink());
    assert_eq!(fs::read(&f).unwrap(), NEW);
}

/// The uid that owns the files we create.
fn me(d: &TempDir) -> u32 {
    let p = d.0.join("who");
    File::create(&p).unwrap();
    fs::metadata(&p).unwrap().uid()
}

#[test]
fn safe_1_a_symlink_is_written_through_only_if_the_user_owns_it() {
    let (d, f, state) = setup("symlink-owner");
    let me = me(&d);
    let (link, dangling, chain) =
        (f.with_file_name("link"), f.with_file_name("dangling"), f.with_file_name("chain"));
    symlink(&f, &link).unwrap();
    symlink("missing", &dangling).unwrap();
    symlink("link", &chain).unwrap();
    for l in [&link, &dangling, &chain] {
        let name = l.to_str().unwrap();
        assert_eq!(foreign_link(name, me), None, "mine: {name}");
        assert_eq!(foreign_link(name, me + 1).as_deref(), Some(name), "someone else's: {name}");
    }
    // Not links at all: a file, a directory, a name that is not there.
    for p in [f.clone(), d.0.clone(), d.0.join("nothing")] {
        assert_eq!(foreign_link(p.to_str().unwrap(), me + 1), None, "{p:?}");
    }
    // A relative name is looked at from the current directory, like the save does.
    assert_eq!(foreign_link("Cargo.toml", me + 1), None);
    // The user's own links are written through, dangling or not, and the message stays out of it.
    for l in [&link, &dangling, &chain] {
        let r = save(l.to_str().unwrap(), NEW, None, Some(&state));
        assert!(matches!(r, Ok(Saved::Done(..))), "{l:?}: {:?}", r.err());
    }
}

/// A link someone else owns is not written through. The system has some (`/etc/os-release` on Arch,
/// `/etc`, `/tmp` and `/var` on macOS); a test run by root has none to refuse, and skips.
#[test]
fn safe_1_a_symlink_owned_by_someone_else_is_refused() {
    let (d, f, state) = setup("symlink-foreign");
    let me = me(&d);
    let theirs = |p: &&str| fs::symlink_metadata(p).is_ok_and(|m| m.is_symlink() && m.uid() != me);
    let all =
        ["/etc/os-release", "/etc/mtab", "/etc/localtime", "/etc/resolv.conf", "/bin", "/lib"];
    let all = all.into_iter().chain(["/tmp", "/etc", "/var", "/usr/bin/sh"]);
    let Some(link) = all.into_iter().find(theirs).filter(|_| me != 0) else {
        eprintln!("skipped: no symlink owned by someone else here, or run by root");
        return;
    };
    let r = save(link, NEW, None, Some(&state));
    assert_eq!(
        r.err().as_deref(),
        Some(&*format!("not saved: {link} is a symlink owned by someone else"))
    );
    let r = save(link, NEW, Some(ABSENT), Some(&state));
    assert!(r.is_err_and(|e| e.starts_with("not saved: ")), "the compare-and-swap form too");
    assert_eq!(fs::read(&f).unwrap(), OLD, "nothing else was touched");
}

/// As root a link can be given to somebody else: then the whole path is testable.
#[test]
fn safe_1_a_link_given_away_is_refused_whatever_its_place_in_the_chain() {
    let (d, f, state) = setup("symlink-given");
    if me(&d) != 0 {
        eprintln!("skipped: only root can give a symlink away (lchown)");
        return;
    }
    let (first, second, dangling) =
        (f.with_file_name("first"), f.with_file_name("second"), f.with_file_name("dangling"));
    symlink("second", &first).unwrap();
    symlink(&f, &second).unwrap();
    symlink("nowhere", &dangling).unwrap();
    std::os::unix::fs::lchown(&second, Some(12345), None).unwrap();
    std::os::unix::fs::lchown(&dangling, Some(12345), None).unwrap();
    let r = save(first.to_str().unwrap(), NEW, None, Some(&state));
    assert_eq!(
        r.err(),
        Some(format!("not saved: {} is a symlink owned by someone else", second.display()))
    );
    let r = save(dangling.to_str().unwrap(), NEW, None, Some(&state));
    assert_eq!(
        r.err(),
        Some(format!("not saved: {} is a symlink owned by someone else", dangling.display()))
    );
    assert_eq!(fs::read(&f).unwrap(), OLD);
    assert!(!f.with_file_name("nowhere").exists(), "the missing target was not created");
    assert_eq!(temps(&f), 0);
}

#[test]
fn safe_1_new_file_mode_follows_the_umask() {
    let (_d, f, state) = setup("umask");
    let (reference, new) = (f.with_file_name("reference"), f.with_file_name("new"));
    OpenOptions::new().write(true).create_new(true).mode(0o666).open(&reference).unwrap();
    save(new.to_str().unwrap(), NEW, None, Some(&state)).unwrap();
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

/// The target of a dangling link does not exist yet: the save creates it and the link stays.
#[test]
fn safe_1_a_dangling_symlink_is_followed_to_its_missing_target() {
    let (_d, f, state) = setup("dangling");
    let (link, target) = (f.with_file_name("link"), f.with_file_name("target"));
    symlink("target", &link).unwrap();
    let r = save(link.to_str().unwrap(), NEW, Some(ABSENT), Some(&state));
    assert!(matches!(r, Ok(Saved::Done(..))), "{:?}", r.err());
    assert!(fs::symlink_metadata(&link).unwrap().is_symlink(), "the link stays");
    assert_eq!(fs::read(&target).unwrap(), NEW, "the target is created");
    // Through a chain of links, and into a directory the link names.
    let (a, b) = (f.with_file_name("a"), f.with_file_name("b"));
    symlink("b", &a).unwrap();
    symlink("target2", &b).unwrap();
    save(a.to_str().unwrap(), OLD, Some(ABSENT), Some(&state)).unwrap();
    assert_eq!(fs::read(f.with_file_name("target2")).unwrap(), OLD);
}

#[test]
fn safe_1_a_file_without_write_permission_is_not_saved_over_unless_forced() {
    let (_d, f, state) = setup("readonly-file");
    fs::set_permissions(&f, Permissions::from_mode(0o444)).unwrap();
    let path = f.to_str().unwrap();
    let r = save(path, NEW, Some(hash(OLD)), Some(&state));
    assert_eq!(r.err().as_deref(), Some("read-only file (:w! to write anyway)"));
    assert_eq!((fs::read(&f).unwrap(), temps(&f)), (OLD.to_vec(), 0));
    // `:w!` (no expected hash) writes it and keeps the mode.
    assert!(matches!(save(path, NEW, None, Some(&state)), Ok(Saved::Done(..))));
    assert_eq!((fs::read(&f).unwrap(), mode(&f)), (NEW.to_vec(), 0o444));
}

#[test]
fn safe_2_without_a_private_state_directory_an_in_place_save_refuses() {
    for state in [None, Some("open"), Some("link")] {
        let (_d, f, st) = setup("no-state");
        fs::hard_link(&f, f.with_file_name("other")).unwrap();
        let open = f.with_file_name("open");
        DirBuilder::new().mode(0o755).create(&open).unwrap();
        symlink(&st, f.with_file_name("link")).unwrap();
        let state = state.map(|n| f.with_file_name(n));
        let r = save(f.to_str().unwrap(), NEW, None, state.as_deref());
        let e = r.err().unwrap_or_default();
        assert!(e.contains("no recovery copy possible"), "{state:?}: {e}");
        assert_eq!(fs::read(&f).unwrap(), OLD, "{state:?}: the old text is intact");
    }
}

#[test]
fn safe_4_a_directory_says_so() {
    let d = temp_dir("read-dir");
    let dir = d.0.to_str().unwrap();
    assert_eq!(read(dir).err(), Some(format!("{dir} is a directory")));
    assert_eq!(read(&format!("{dir}/missing")).unwrap(), None);
    fs::write(d.0.join("f"), b"x").unwrap();
    assert_eq!(read(&format!("{dir}/f")).unwrap(), Some(b"x".to_vec()));
    let denied = d.0.join("denied");
    fs::write(&denied, b"x").unwrap();
    fs::set_permissions(&denied, Permissions::from_mode(0o000)).unwrap();
    if File::open(&denied).is_err() {
        assert_eq!(
            read(denied.to_str().unwrap()).err().as_deref(),
            Some("cannot read: Permission denied")
        );
    }
}

/// SAFE-5: another process replaces the file after the save has made its temporary file (a big
/// one, so that the window is wide): the save must find that and write nothing.
#[test]
fn safe_5_a_file_replaced_while_the_temp_file_is_written_is_a_conflict() {
    let (_d, f, state) = setup("toctou");
    let dir = f.parent().unwrap().to_path_buf();
    let theirs = b"THEIRS: written while the save was running\n".to_vec();
    let big = vec![b'x'; 64 << 20];
    let (f2, t2) = (f.clone(), theirs.clone());
    let other = std::thread::spawn(move || {
        let start = std::time::Instant::now();
        while start.elapsed() < std::time::Duration::from_secs(20) {
            let temp = fs::read_dir(&dir).unwrap().flatten();
            if temp.into_iter().any(|e| e.file_name().to_string_lossy().starts_with(".f.si-")) {
                fs::write(dir.join("other.tmp"), &t2).unwrap();
                fs::rename(dir.join("other.tmp"), &f2).unwrap();
                return true;
            }
        }
        false
    });
    let r = save(f.to_str().unwrap(), &big, Some(hash(OLD)), Some(&state)).unwrap();
    assert!(other.join().unwrap(), "the save never created its temporary file");
    assert!(matches!(r, Saved::Conflict(Some(d)) if d == theirs));
    assert_eq!(fs::read(&f).unwrap(), theirs, "their text is still there");
}

/// SAFE-5: the stamp of the file is checked again right before the rename, and the file is read
/// again only if it moved. A file that is rewritten all the time with the same text moves its
/// stamp over and over: that is no conflict, and the save does not wait for it to stand still.
#[test]
fn safe_5_a_rewrite_of_the_same_text_during_a_save_is_no_conflict() {
    let (_d, f, state) = setup("same-text");
    let text = vec![b'y'; 8 << 20];
    fs::write(&f, &text).unwrap();
    let (f2, t2, dir) = (f.clone(), text.clone(), f.parent().unwrap().to_path_buf());
    let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let s2 = stop.clone();
    let writer = std::thread::spawn(move || {
        let mut n = 0;
        while !s2.load(std::sync::atomic::Ordering::Relaxed) {
            fs::write(dir.join("again.tmp"), &t2).unwrap();
            fs::rename(dir.join("again.tmp"), &f2).unwrap(); // whole at every moment
            n += 1;
        }
        n
    });
    for _ in 0..5 {
        let r = save(f.to_str().unwrap(), NEW, Some(hash(&text)), Some(&state)).unwrap();
        assert!(matches!(r, Saved::Done(..)), "same text, so no conflict");
        fs::write(&f, &text).unwrap();
    }
    stop.store(true, std::sync::atomic::Ordering::Relaxed);
    assert!(writer.join().unwrap() > 0);
}
