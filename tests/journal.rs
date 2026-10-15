//! SAFE-6: snapshots survive a crash, never collide between sessions, and never leak a live one.
// Helpers outside `#[test]` fns are not covered by clippy.toml, so allow it for the file.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use si::io::journal::{Snapshot, find, remove, state_dir, write};
use std::ffi::OsString;
use std::fs;
use std::os::unix::fs::{MetadataExt, PermissionsExt, symlink};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};

/// A pid that cannot exist (Linux caps pids at 2^22).
const DEAD: u32 = u32::MAX / 2;

/// A unique directory under target/tmp, removed on drop. Never the real `$XDG_STATE_HOME`.
struct Tmp(PathBuf);

impl Tmp {
    fn new() -> Tmp {
        static N: AtomicUsize = AtomicUsize::new(0);
        let n = N.fetch_add(1, Ordering::SeqCst);
        let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
            .join(format!("journal-{}-{n}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        Tmp(dir)
    }

    /// The state directory; it does not exist yet, so `write` must create it.
    fn state(&self) -> PathBuf {
        self.0.join("state").join("si")
    }
}

impl Drop for Tmp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn snap(path: &str, pid: u32, time_ms: u64) -> Snapshot {
    Snapshot {
        path: path.to_string(),
        pid,
        time_ms,
        base: b"base\n".to_vec(),
        text: format!("text of {pid} at {time_ms}\n").into_bytes(),
    }
}

fn files(state: &Path) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = fs::read_dir(state).unwrap().map(|e| e.unwrap().path()).collect();
    v.sort();
    v
}

/// Independent FNV-1a 64, to pin the name format.
fn fnv1a(s: &str) -> u64 {
    s.bytes().fold(0xcbf2_9ce4_8422_2325, |h, b| (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3))
}

fn is_alive(pid: u32) -> bool {
    let status = Command::new("kill").args(["-0", &pid.to_string()]).stderr(Stdio::null()).status();
    status.unwrap().success()
}

#[test]
fn safe_6_round_trip_is_binary_safe() {
    let t = Tmp::new();
    let s = Snapshot {
        path: "/work/日本語 \n名前.txt".to_string(),
        pid: DEAD,
        time_ms: 1_700_000_123_456,
        base: (0..=255u8).collect(),
        text: b"si-snapshot 1 \n\n\0\xff\xfe\r\n no newline at the end".to_vec(),
    };
    write(&t.state(), &s).unwrap();
    assert_eq!(find(&t.state(), &s.path, std::process::id()), Some(s));
}

#[test]
fn safe_6_empty_base_and_text_round_trip() {
    let t = Tmp::new();
    let s = Snapshot { path: "x".into(), pid: DEAD, time_ms: 0, base: vec![], text: vec![] };
    write(&t.state(), &s).unwrap();
    assert_eq!(find(&t.state(), "x", 1), Some(s));
}

#[test]
fn safe_6_permissions_are_0700_dir_and_0600_file() {
    let t = Tmp::new();
    write(&t.state(), &snap("/a", DEAD, 1)).unwrap();
    let mode = |p: &Path| fs::metadata(p).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode(&t.state()), 0o700, "state directory");
    let all = files(&t.state());
    assert_eq!(all.len(), 1);
    assert_eq!(mode(&all[0]), 0o600, "snapshot file");
}

#[test]
fn safe_6_name_holds_fnv1a_hash_of_path_and_the_pid() {
    let t = Tmp::new();
    // Known FNV-1a 64 test vector: "a" -> af63dc4c8601ec8c.
    assert_eq!(fnv1a("a"), 0xaf63_dc4c_8601_ec8c);
    write(&t.state(), &snap("a", 4242, 1)).unwrap();
    let name = files(&t.state())[0].file_name().unwrap().to_string_lossy().into_owned();
    assert!(name.contains("af63dc4c8601ec8c"), "name {name} lacks the hash of the path");
    assert!(name.contains("4242"), "name {name} lacks the pid");
}

#[test]
fn safe_6_two_pids_on_one_path_keep_two_files() {
    let t = Tmp::new();
    write(&t.state(), &snap("/a", DEAD, 1)).unwrap();
    write(&t.state(), &snap("/a", DEAD + 1, 2)).unwrap();
    assert_eq!(files(&t.state()).len(), 2);
    // Each one is still intact: remove the newer, the older is found.
    remove(&t.state(), "/a", DEAD + 1);
    assert_eq!(find(&t.state(), "/a", 1), Some(snap("/a", DEAD, 1)));
}

#[test]
fn safe_6_same_pid_on_two_paths_keep_two_files() {
    let t = Tmp::new();
    write(&t.state(), &snap("/a", DEAD, 1)).unwrap();
    write(&t.state(), &snap("/b", DEAD, 2)).unwrap();
    assert_eq!(files(&t.state()).len(), 2);
    assert_eq!(find(&t.state(), "/a", 1), Some(snap("/a", DEAD, 1)));
    assert_eq!(find(&t.state(), "/b", 1), Some(snap("/b", DEAD, 2)));
}

#[test]
fn safe_6_writing_again_replaces_the_same_file() {
    let t = Tmp::new();
    write(&t.state(), &snap("/a", DEAD, 1)).unwrap();
    write(&t.state(), &snap("/a", DEAD, 2)).unwrap();
    assert_eq!(files(&t.state()).len(), 1);
    assert_eq!(find(&t.state(), "/a", 1), Some(snap("/a", DEAD, 2)));
}

#[test]
fn safe_6_find_skips_own_pid() {
    let t = Tmp::new();
    write(&t.state(), &snap("/a", DEAD, 1)).unwrap();
    assert_eq!(find(&t.state(), "/a", DEAD), None);
    assert!(find(&t.state(), "/a", DEAD + 1).is_some());
}

#[test]
fn safe_6_find_skips_a_live_process_and_leaves_its_file() {
    let t = Tmp::new();
    let me = std::process::id();
    assert!(is_alive(me) && !is_alive(DEAD), "test premise: the pids' states");
    write(&t.state(), &snap("/a", me, 900)).unwrap();
    assert_eq!(find(&t.state(), "/a", DEAD + 1), None, "a live process's snapshot is not offered");
    assert_eq!(files(&t.state()).len(), 1, "and it is left alone");
    // A newer live snapshot does not hide an older dead one.
    write(&t.state(), &snap("/a", DEAD, 100)).unwrap();
    assert_eq!(find(&t.state(), "/a", DEAD + 1), Some(snap("/a", DEAD, 100)));
}

#[test]
fn safe_6_find_follows_a_real_process_dying() {
    let t = Tmp::new();
    let mut child = Command::new("sleep").arg("30").spawn().unwrap();
    write(&t.state(), &snap("/a", child.id(), 1)).unwrap();
    assert_eq!(find(&t.state(), "/a", DEAD), None, "child is running");
    child.kill().unwrap();
    child.wait().unwrap();
    assert!(find(&t.state(), "/a", DEAD).is_some(), "child is gone");
}

#[test]
fn safe_6_newest_wins() {
    let t = Tmp::new();
    for (pid, time) in [(DEAD, 100), (DEAD + 1, 300), (DEAD + 2, 200)] {
        write(&t.state(), &snap("/a", pid, time)).unwrap();
    }
    assert_eq!(find(&t.state(), "/a", 1), Some(snap("/a", DEAD + 1, 300)));
}

#[test]
fn safe_6_find_ignores_other_paths_and_a_missing_directory() {
    let t = Tmp::new();
    assert_eq!(find(&t.state(), "/a", 1), None, "no state directory yet");
    write(&t.state(), &snap("/a", DEAD, 1)).unwrap();
    assert_eq!(find(&t.state(), "/b", 1), None);
}

#[test]
fn safe_6_garbage_is_ignored_and_left_alone() {
    let t = Tmp::new();
    let state = t.state();
    write(&state, &snap("/a", DEAD, 1)).unwrap();
    let file = files(&state)[0].clone();
    let good = fs::read(&file).unwrap();
    let mut flipped = good.clone();
    flipped[0] ^= 0xff;
    let mut longer = good.clone();
    longer.push(b'x');
    let huge = b"si-snapshot 1 1 1 99999999999999999999 18446744073709551615 1\nabc".to_vec();
    let lies = b"si-snapshot 1 1 1 18446744073709551615 18446744073709551615 2\nab".to_vec();
    let bad = [
        vec![],
        b"\n".to_vec(),
        b"garbage without a newline".to_vec(),
        b"\xff\xfe\x00 not utf-8\nrest".to_vec(),
        good[..good.len() - 1].to_vec(),
        good[..good.len() / 2].to_vec(),
        longer,
        flipped,
        huge,
        lies,
    ];
    for (i, content) in bad.iter().enumerate() {
        fs::write(&file, content).unwrap();
        assert_eq!(find(&state, "/a", 1), None, "garbage #{i} must be ignored");
        assert_eq!(fs::read(&file).unwrap(), *content, "garbage #{i} must be left alone");
    }
    // Junk of every other kind in the directory does not hide a valid snapshot.
    fs::write(&file, &good).unwrap();
    fs::write(state.join("junk.snap"), b"junk").unwrap();
    fs::write(state.join("README"), b"hello").unwrap();
    fs::create_dir(state.join("dir.snap")).unwrap();
    assert_eq!(find(&state, "/a", 1), Some(snap("/a", DEAD, 1)));
}

#[test]
fn safe_6_remove_deletes_only_its_own() {
    let t = Tmp::new();
    write(&t.state(), &snap("/a", DEAD, 1)).unwrap();
    write(&t.state(), &snap("/a", DEAD + 1, 2)).unwrap();
    write(&t.state(), &snap("/b", DEAD, 3)).unwrap();
    remove(&t.state(), "/a", DEAD);
    assert_eq!(files(&t.state()).len(), 2);
    assert_eq!(find(&t.state(), "/a", 1), Some(snap("/a", DEAD + 1, 2)));
    assert_eq!(find(&t.state(), "/b", 1), Some(snap("/b", DEAD, 3)));
    remove(&t.state(), "/a", DEAD); // already gone: nothing happens
    remove(&t.0.join("missing"), "/a", DEAD); // no such directory: nothing happens
    assert_eq!(files(&t.state()).len(), 2);
}

#[test]
fn safe_6_no_temp_files_are_left() {
    let t = Tmp::new();
    for time in 0..20 {
        write(&t.state(), &snap("/a", DEAD, time)).unwrap();
    }
    write(&t.state(), &snap("/b", DEAD, 1)).unwrap();
    let names: Vec<String> = files(&t.state())
        .iter()
        .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names.len(), 2, "{names:?}");
    assert!(names.iter().all(|n| n.ends_with(".snap") && !n.starts_with('.')), "{names:?}");
}

#[test]
fn safe_6_write_to_an_unusable_directory_is_an_error_not_a_panic() {
    let t = Tmp::new();
    fs::create_dir_all(&t.0).unwrap();
    let blocker = t.0.join("file");
    fs::write(&blocker, b"x").unwrap();
    assert!(write(&blocker, &snap("/a", DEAD, 1)).is_err());
    assert!(write(&blocker.join("sub"), &snap("/a", DEAD, 1)).is_err());
    assert_eq!(find(&blocker, "/a", 1), None);
}

/// A directory with the given mode inside `t`, holding a snapshot written before it was opened up.
fn state_with_mode(t: &Tmp, mode: u32) -> PathBuf {
    let state = t.state();
    write(&state, &snap("/a", DEAD, 1)).unwrap();
    fs::set_permissions(&state, fs::Permissions::from_mode(mode)).unwrap();
    state
}

#[test]
fn safe_6_a_state_dir_that_group_or_others_can_use_is_not_used() {
    for mode in [0o750, 0o705, 0o770, 0o707, 0o755, 0o777] {
        let t = Tmp::new();
        let state = state_with_mode(&t, mode);
        let e = write(&state, &snap("/b", DEAD, 2)).unwrap_err();
        assert!(e.to_string().contains("not private"), "{mode:o}: {e}");
        assert_eq!(find(&state, "/a", 1), None, "{mode:o}: its snapshots are not read either");
        assert_eq!(files(&state).len(), 1, "{mode:o}: nothing was written");
    }
    let t = Tmp::new();
    let state = state_with_mode(&t, 0o700);
    assert!(write(&state, &snap("/b", DEAD, 2)).is_ok());
    assert!(find(&state, "/a", 1).is_some());
    // The probe file that tells our uid is gone again.
    assert!(
        files(&state).iter().all(|f| !f.file_name().unwrap().to_string_lossy().starts_with('.'))
    );
}

#[test]
fn safe_6_a_symlink_is_not_a_state_dir() {
    let t = Tmp::new();
    let real = t.0.join("real");
    fs::create_dir_all(&real).unwrap();
    fs::set_permissions(&real, fs::Permissions::from_mode(0o700)).unwrap();
    symlink(&real, t.0.join("link")).unwrap();
    let link = t.0.join("link");
    assert!(write(&link, &snap("/a", DEAD, 1)).is_err());
    write(&real, &snap("/a", DEAD, 1)).unwrap();
    assert_eq!(find(&link, "/a", 1), None);
    assert!(find(&real, "/a", 1).is_some());
}

#[test]
fn safe_6_snapshots_that_are_not_ours_are_skipped() {
    let t = Tmp::new();
    let state = t.state();
    write(&state, &snap("/a", DEAD, 1)).unwrap();
    let file = files(&state).pop().unwrap();
    assert!(find(&state, "/a", 1).is_some());
    // A link to a snapshot somewhere else is not a snapshot file of ours.
    let outside = t.0.join("outside.snap");
    fs::rename(&file, &outside).unwrap();
    symlink(&outside, &file).unwrap();
    assert_eq!(find(&state, "/a", 1), None);
    // Owned by someone else (only a root can arrange that).
    fs::remove_file(&file).unwrap();
    write(&state, &snap("/a", DEAD, 1)).unwrap();
    let root = fs::metadata(&state).unwrap().uid() == 0;
    if root && std::os::unix::fs::chown(&file, Some(65534), None).is_ok() {
        assert_eq!(find(&state, "/a", 1), None, "a snapshot of another user is ignored");
    }
}

#[test]
fn safe_6_without_xdg_state_home_and_home_there_is_no_state_dir() {
    let os = |s: &str| Some(OsString::from(s));
    assert_eq!(state_dir(None, None), None, "never a shared fallback such as /tmp");
    assert_eq!(state_dir(os(""), os("")), None);
    assert_eq!(state_dir(os("relative"), None), None, "a relative path is not a state directory");
    assert_eq!(state_dir(os("/x"), None), Some(PathBuf::from("/x/si")));
    assert_eq!(state_dir(os("/x"), os("/h")), Some(PathBuf::from("/x/si")));
    assert_eq!(state_dir(None, os("/h")), Some(PathBuf::from("/h/.local/state/si")));
    assert_eq!(state_dir(os(""), os("/h")), Some(PathBuf::from("/h/.local/state/si")));
}
