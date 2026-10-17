//! SAFE-1, SAFE-2, SAFE-3, SAFE-5 on io::file: what a save leaves behind. (What it leaves behind when
//! any one of its steps fails is tested inside io/file.rs, where the fault hook exists: `cfg(test)`.)
#![allow(clippy::unwrap_used, clippy::expect_used)] // helpers; the tests themselves may unwrap
mod common;
use common::{TempDir, temp_dir, within};
use sim_core::core::buffer::{ABSENT, hash};
use sim_core::io::file::{Saved, foreign_link, open_same, read, save, symlinks};
use std::fs::{self, DirBuilder, File, OpenOptions, Permissions};
use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt, symlink};
use std::path::{Path, PathBuf};
use std::process::Command;

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
    names.filter(|n| n.to_string_lossy().starts_with(".f.sim-")).count()
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
fn safe_1_a_symlink_is_written_through_only_if_the_user_or_root_owns_it() {
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

/// Links of root's are the system's own and are trusted, whoever looks at them: `/bin` on Arch,
/// `/tmp`, `/var` and `/etc` on macOS. (A test run by root has none that are not its own: it skips.)
#[test]
fn safe_1_a_symlink_of_root_is_trusted_whoever_looks() {
    let (d, ..) = setup("symlink-root");
    let me = me(&d);
    let links: Vec<_> = [true, false].into_iter().filter_map(|dir| root_link(me, dir)).collect();
    if links.is_empty() {
        return eprintln!("skipped: no symlink owned by root here, or run by root");
    }
    for link in links {
        assert_eq!(symlinks(link).first().map(|l| l.1), Some(0), "{link}");
        assert_eq!((foreign_link(link, me), foreign_link(link, me + 1)), (None, None), "{link}");
    }
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
    // A directory given away, with a link of the user's own in front of it.
    let (real, dir_link, front) = (d.0.join("real"), d.0.join("dir_link"), d.0.join("front"));
    fs::create_dir(&real).unwrap();
    symlink(&real, &dir_link).unwrap();
    symlink(&dir_link, &front).unwrap();
    std::os::unix::fs::lchown(&dir_link, Some(12345), None).unwrap();
    for through in [&dir_link, &front] {
        let r = save(through.join("new").to_str().unwrap(), NEW, None, Some(&state));
        let want = format!("not saved: {} is a symlink owned by someone else", dir_link.display());
        assert_eq!(r.err(), Some(want), "{through:?}");
        assert!(!real.join("new").exists());
    }
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

/// SAFE-2: a file that is a mount point (a bind-mounted config in a container) cannot be replaced by
/// a rename (EBUSY), so it is written in place. The test runs itself in a user and mount namespace,
/// where `mount --bind` needs no privilege; where that is not allowed it has nothing to check.
#[cfg(target_os = "linux")]
#[test]
fn safe_2_a_mount_point_is_written_in_place() {
    if let Ok(dir) = std::env::var("SIM_TEST_BIND") {
        let (target, state) = (format!("{dir}/target"), Path::new(&dir).join("state"));
        let r = save(&target, b"new\n", Some(hash(b"other\n")), Some(&state));
        assert!(matches!(r, Ok(Saved::Done(..))), "{:?}", r.map(|_| ()));
        assert_eq!(fs::read(&target).unwrap(), b"new\n");
        assert_eq!(fs::read(format!("{dir}/src")).unwrap(), b"new\n", "what is mounted is written");
        return;
    }
    let d = temp_dir("bind");
    fs::write(d.0.join("src"), b"other\n").unwrap();
    fs::write(d.0.join("target"), b"hello\n").unwrap();
    DirBuilder::new().mode(0o700).create(d.0.join("state")).unwrap();
    let ns = |script: &str| {
        let mut c = Command::new("unshare");
        c.args(["--user", "--map-root-user", "--mount", "sh", "-c", script]).current_dir(&d.0);
        c.env("SIM_TEST_BIND", &d.0)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
        c.status().is_ok_and(|s| s.success())
    };
    if !ns("mount --bind src target") {
        eprintln!("no user namespaces here: nothing to check");
        return;
    }
    let me = std::env::current_exe().unwrap();
    let run = format!(
        "mount --bind src target && exec '{}' --exact safe_2_a_mount_point_is_written_in_place",
        me.display()
    );
    assert!(
        ns(&run),
        "the save of a mount point failed (run it with --nocapture in the namespace)"
    );
}

/// SAFE-5: the compare reads the file, so a file that is not a regular one (a FIFO that took the
/// place of the file waits for a writer for ever, a device may never end) is refused before it is
/// read, and the save does not hang.
#[test]
fn safe_5_a_file_that_is_not_regular_is_refused_before_it_is_read() {
    let (_d, f, state) = setup("fifo");
    fs::remove_file(&f).unwrap();
    assert!(Command::new("mkfifo").arg(&f).status().unwrap().success());
    for expect in [Some(hash(OLD)), Some(ABSENT)] {
        let (path, state) = (f.to_str().unwrap().to_string(), state.clone());
        let r = within(10, move || save(&path, NEW, expect, Some(&state)).map(|_| ()));
        let err = r.expect("the save hangs on a FIFO").unwrap_err();
        assert!(err.contains("not a regular file"), "{err}");
    }
    // `:w!` compares nothing and replaces the name, as it does for any file.
    assert!(save(f.to_str().unwrap(), NEW, None, Some(&state)).is_ok());
    assert_eq!(fs::read(&f).unwrap(), NEW);
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
            if temp.into_iter().any(|e| e.file_name().to_string_lossy().starts_with(".f.sim-")) {
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

/// A symlink of the system that root owns, to a directory or to a file. `None` when run by root
/// (then the links the test makes are root's too) or when this system has none.
fn root_link(me: u32, dir: bool) -> Option<&'static str> {
    let all = ["/bin", "/lib", "/sbin", "/lib64", "/usr/sbin", "/var/run", "/tmp", "/etc", "/var"];
    let files =
        ["/etc/os-release", "/etc/mtab", "/etc/localtime", "/etc/resolv.conf", "/usr/bin/sh"];
    let root = |p: &&str| fs::symlink_metadata(p).is_ok_and(|m| m.is_symlink() && m.uid() == 0);
    let named = |p: &&str| fs::metadata(p).is_ok_and(|m| m.is_dir() == dir);
    all.into_iter().chain(files).filter(root).find(named).filter(|_| me != 0)
}

/// The directory of `d` spelled through a link of root's, so that a save there goes through one:
/// `/proc/self` on Linux, `/var` on macOS (where `$TMPDIR` is under it: `/private/var/...` is the
/// directory itself, `/var/...` the same one through root's link). `None` when run by root.
fn via_root(d: &TempDir, me: u32) -> Option<String> {
    let dir = d.0.display().to_string();
    let short = dir.strip_prefix("/private").map(str::to_string);
    let through = |p: &String| symlinks(&format!("{p}/f")).iter().any(|l| l.1 == 0);
    let spellings = [format!("/proc/self/root{dir}")].into_iter().chain(short).chain([dir]);
    spellings.into_iter().find(through).filter(|_| me != 0)
}

/// SAFE-1: a path that goes through links of root's is saved through, in front, and behind links of
/// the user's own: the system's links, such as macOS's `/var` that `$TMPDIR` lies under, are not a
/// reason to refuse the prompt a program opens in `$EDITOR`.
#[test]
fn safe_1_a_path_through_links_of_root_is_saved_through() {
    let (d, f, state) = setup("root-links");
    let me = me(&d);
    let Some(via) = via_root(&d, me) else {
        return eprintln!("skipped: no way through a link of root's here, or run by root");
    };
    let p = format!("{via}/f");
    assert_eq!(foreign_link(&p, me), None);
    let r = save(&p, NEW, Some(hash(OLD)), Some(&state));
    assert!(matches!(r, Ok(Saved::Done(..))), "{p}: {:?}", r.err());
    assert_eq!(fs::read(&f).unwrap(), NEW);
    // Behind a link of the user's own that leads there, and a file that is not there yet.
    let (mine, mine2) = (d.0.join("mine"), d.0.join("mine2"));
    symlink(&via, &mine).unwrap();
    symlink("mine", &mine2).unwrap();
    let new = format!("{}/new", mine2.display());
    assert!(matches!(save(&new, NEW, Some(ABSENT), Some(&state)), Ok(Saved::Done(..))));
    assert_eq!(fs::read(d.0.join("new")).unwrap(), NEW);
}

/// SAFE-1: a link is followed if the user or root owns it, and refused if anybody else does. The
/// links that can be made without root are the user's own, so the uid we are looked at from stands
/// in for the owner; the full save is `safe_1_a_link_given_away_...`, which runs as root.
#[test]
fn safe_1_only_links_of_the_user_and_root_are_followed() {
    let (d, ..) = setup("trusted");
    let me = me(&d);
    let Some(via) = via_root(&d, me) else {
        return eprintln!("skipped: no way through a link of root's here, or run by root");
    };
    let mine = d.0.join("mine");
    symlink(&via, &mine).unwrap();
    let p = format!("{}/f", mine.display());
    let owners: Vec<_> = symlinks(&p).iter().map(|l| l.1).collect();
    assert!(owners.contains(&0) && owners.contains(&me), "{owners:?}");
    assert_eq!(foreign_link(&p, me), None, "the user's and root's");
    let name = mine.display().to_string();
    assert_eq!(foreign_link(&p, me + 1), Some(name.clone()), "root's are trusted, the user's not");
    assert_eq!(foreign_link(&p, 0), Some(name), "and a link is not trusted for being first");
}

/// `real/sub/target` (OLD) reached through links of the user's: `dl` is a link to the directory
/// `real/sub`, `inner` a link next to the target, `chain` a link whose target passes through `dl`
/// and ends at `inner`, and `real/x` a link that only `dl/../x` reaches.
fn links(d: &TempDir) -> (PathBuf, impl Fn(&str) -> String + '_) {
    let dir = d.0.clone();
    fs::create_dir_all(dir.join("real/sub")).unwrap();
    fs::write(dir.join("real/sub/target"), OLD).unwrap();
    symlink("real/sub", dir.join("dl")).unwrap();
    symlink("target", dir.join("real/sub/inner")).unwrap();
    symlink("dl/inner", dir.join("chain")).unwrap();
    symlink("sub/target", dir.join("real/x")).unwrap();
    (dir.clone(), move |p: &str| dir.join(p).display().to_string())
}

fn names(p: &str) -> Vec<String> {
    symlinks(p).into_iter().map(|l| l.0).collect()
}

/// SAFE-1: every symlink met on the way is looked at: a link to a directory in the middle of the
/// path, links reached by following other links, and `..`, which is the parent of where a link
/// leads. They come in the order they are followed, the path's own link under the name it was
/// given and the others by their absolute path.
#[test]
fn safe_1_every_symlink_on_the_way_is_looked_at_in_the_order_it_is_followed() {
    let (d, f, _) = setup("walk");
    let me = me(&d);
    let (dir, at) = links(&d);
    let p = |s: &str| at(s);
    assert_eq!(names(&p("dl/target")), [p("dl")]);
    assert_eq!(names(&p("chain")), [p("chain"), p("dl"), p("real/sub/inner")]);
    assert_eq!(names(&p("dl/../x")), [p("dl"), p("real/x")], "the parent of real/sub is real");
    let own = p("real/./sub/../sub/inner");
    assert_eq!(names(&own), std::slice::from_ref(&own), "the path's own link keeps its given name");
    // Not links at all: a file, a directory, a name that is not there, `..`.
    for q in [f.clone(), dir.clone(), dir.join("nothing/at/all"), dir.join("real/..")] {
        assert_eq!(names(q.to_str().unwrap()), Vec::<String>::new(), "{q:?}");
    }
    assert_eq!(
        names("Cargo.toml"),
        Vec::<String>::new(),
        "a relative name, from the current directory"
    );
    // The owner goes with the name, and the uid we are looked at from decides which is foreign.
    assert!(symlinks(&p("chain")).iter().all(|l| l.1 == me));
    assert_eq!(foreign_link(&p("dl/target"), me), None);
    assert_eq!(
        foreign_link(&p("dl/target"), me + 1),
        Some(p("dl")),
        "the directory link, not the file"
    );
    assert_eq!(
        foreign_link(&p("chain"), me + 1),
        Some(p("chain")),
        "the first one met, by the given name"
    );
    // At most 40 links are followed, like the kernel: a loop ends the list.
    symlink("loop_b", dir.join("loop_a")).unwrap();
    symlink("loop_a", dir.join("loop_b")).unwrap();
    assert_eq!(names(&p("loop_a")).len(), 40);
}

/// SAFE-1: the user's own links, of any kind and in any place, are written through.
#[test]
fn safe_1_a_path_through_the_users_own_links_is_saved_through() {
    let (d, _, state) = setup("own-links");
    let (dir, at) = links(&d);
    for through in ["dl/target", "chain", "dl/inner", "real/sub/inner"] {
        fs::write(dir.join("real/sub/target"), OLD).unwrap();
        let r = save(&at(through), NEW, Some(hash(OLD)), Some(&state));
        assert!(matches!(r, Ok(Saved::Done(..))), "{through}: {:?}", r.err());
        assert_eq!(fs::read(dir.join("real/sub/target")).unwrap(), NEW, "{through}");
        assert!(
            fs::symlink_metadata(at(through)).unwrap().is_symlink() == (through != "dl/target")
        );
    }
    // A file that is not there yet, behind a directory link, and a `..` after one.
    save(&at("dl/new"), NEW, Some(ABSENT), Some(&state)).unwrap();
    assert_eq!(fs::read(dir.join("real/sub/new")).unwrap(), NEW);
    save(&at("dl/../via_parent"), NEW, Some(ABSENT), Some(&state)).unwrap();
    assert_eq!(fs::read(dir.join("real/via_parent")).unwrap(), NEW, "the parent of real/sub");
    assert!(!dir.join("via_parent").exists());
}

/// SAFE-1: a loop of links is no path: the save fails and nothing is written or made.
#[test]
fn safe_1_a_loop_of_links_is_refused_and_nothing_is_written() {
    let (d, f, state) = setup("loop");
    let (a, b) = (d.0.join("loop_a"), d.0.join("loop_b"));
    symlink("loop_b", &a).unwrap();
    symlink("loop_a", &b).unwrap();
    let before = fs::read_dir(&d.0).unwrap().count();
    for expect in [None, Some(ABSENT)] {
        let e = save(a.to_str().unwrap(), NEW, expect, Some(&state)).err().unwrap();
        assert!(e.contains("symbolic links") && !e.contains("someone else"), "{e}");
    }
    let e = save(a.join("below").to_str().unwrap(), NEW, None, Some(&state)).err().unwrap();
    assert!(e.contains("symbolic links"), "a loop in the middle: {e}");
    assert_eq!(
        (fs::read_dir(&d.0).unwrap().count(), fs::read(&f).unwrap()),
        (before, OLD.to_vec())
    );
}

/// The identity of a file: (device, inode).
fn id(p: &Path) -> Option<(u64, u64)> {
    let m = fs::metadata(p).unwrap();
    Some((m.dev(), m.ino()))
}

/// SAFE-2: the file an in-place save opens must be the one that was validated, and nothing of it
/// is truncated before that is known.
#[test]
fn safe_2_the_file_opened_for_an_in_place_write_must_be_the_one_validated() {
    let (d, f, _) = setup("same-file");
    let (victim, other) = (d.0.join("victim"), d.0.join("other"));
    let seen = id(&f);
    // The file itself is opened, and truncated only now.
    assert!(open_same(&f, seen).unwrap().is_some());
    assert_eq!(fs::read(&f).unwrap(), b"", "the validated file is truncated");
    // A link swapped in for it, to a file the user owns: refused, and that file is untouched.
    fs::remove_file(&f).unwrap();
    fs::write(&victim, OLD).unwrap();
    symlink(&victim, &f).unwrap();
    assert!(open_same(&f, seen).unwrap().is_none());
    assert_eq!(fs::read(&victim).unwrap(), OLD, "nothing was written through the link");
    // Another file renamed over it: refused, and not truncated by the open.
    fs::remove_file(&f).unwrap();
    fs::write(&other, OLD).unwrap();
    fs::rename(&other, &f).unwrap();
    assert!(open_same(&f, seen).unwrap().is_none());
    assert_eq!(fs::read(&f).unwrap(), OLD, "the other file kept its text");
    // Validated as not there, and a file is: refused.
    assert!(open_same(&f, None).unwrap().is_none());
    assert_eq!(fs::read(&f).unwrap(), OLD);
    // Gone altogether is an error of its own, not a swap.
    fs::remove_file(&f).unwrap();
    assert!(open_same(&f, seen).is_err());
}

/// SAFE-2: the file of an in-place save is swapped for a link, to a file the user owns, after the
/// save has looked at it and while its recovery copy is written (a big one, so that the window is
/// wide). The save is refused and nothing is written through the link. If the swap comes late
/// (after the open) the text goes to the file that was validated; the link's target is never
/// written either way, and the save is refused at least once in a few tries.
#[test]
fn safe_2_a_file_swapped_for_a_link_during_an_in_place_save_is_not_written_through() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering::Relaxed};
    let (d, f, state) = setup("swapped");
    let (victim, other) = (d.0.join("victim"), d.0.join("other"));
    fs::write(&victim, b"victim\n").unwrap();
    let big = vec![b'x'; 64 << 20];
    let mut refused = false;
    for _ in 0..5 {
        drop(fs::remove_file(&f));
        drop(fs::remove_file(&other));
        fs::write(&f, OLD).unwrap();
        fs::hard_link(&f, &other).unwrap(); // two links: the save goes in place
        let done = Arc::new(AtomicBool::new(false));
        let (f2, state2, done2) = (f.clone(), state.clone(), done.clone());
        let victim2 = victim.clone();
        let swapper = std::thread::spawn(move || {
            let copy = |s: &Path| fs::read_dir(s).unwrap().flatten().next().is_some();
            while !done2.load(Relaxed) && !copy(&state2) {}
            fs::remove_file(&f2).unwrap();
            symlink(&victim2, &f2).unwrap();
        });
        let r = save(f.to_str().unwrap(), &big, None, Some(&state));
        done.store(true, Relaxed);
        swapper.join().unwrap();
        assert_eq!(
            fs::read(&victim).unwrap(),
            b"victim\n",
            "written through the link: {:?}",
            r.as_ref().err()
        );
        if let Err(e) = r {
            assert!(e.starts_with("not saved: ") && e.ends_with(" changed while saving"), "{e}");
            assert_eq!(
                fs::read_dir(&state).unwrap().count(),
                0,
                "nothing was written: no copy to keep"
            );
            assert_eq!(
                fs::read(&other).unwrap(),
                OLD,
                "the file the link replaced was not touched"
            );
            refused = true;
            break;
        }
    }
    assert!(refused, "the swap never came before the open: the window was too narrow");
}
