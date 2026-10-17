//! FILT-1, FILT-2: `sh -c` runs asynchronously, collects everything, and can be cancelled whole.
// Helpers outside `#[test]` fns are not covered by clippy.toml, so allow it for the file.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use sim_core::io::filter::{Output, cancel, spawn};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

/// How long to wait for something that must happen. A pass never waits this long.
const PATIENCE: Duration = Duration::from_secs(30);

/// Starts `cmd`; `done`'s Output arrives on the channel.
fn start(cmd: &str, input: Vec<u8>) -> (u32, Receiver<Output>) {
    let (tx, rx) = mpsc::channel();
    let pgid = spawn(cmd, input, move |o| drop(tx.send(o))).unwrap();
    (pgid, rx)
}

fn run(cmd: &str, input: Vec<u8>) -> Output {
    let (_, rx) = start(cmd, input);
    rx.recv_timeout(PATIENCE).expect("done was not called (deadlock?)")
}

/// `kill -0` on a pid, or on a process group when `target` is negative.
fn exists(target: i64) -> bool {
    let status =
        Command::new("kill").args(["-0", "--", &target.to_string()]).stderr(Stdio::null()).status();
    status.unwrap().success()
}

/// Polls `f` until it holds, for at most `within`.
fn within(within: Duration, f: impl Fn() -> bool) -> bool {
    let start = Instant::now();
    while !f() {
        if start.elapsed() > within {
            return false;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    true
}

/// A unique directory under target/tmp, removed on drop.
struct Tmp(PathBuf);

impl Tmp {
    fn new() -> Tmp {
        static N: AtomicUsize = AtomicUsize::new(0);
        let n = N.fetch_add(1, Ordering::SeqCst);
        let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
            .join(format!("filter-{}-{n}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        Tmp(dir)
    }
}

impl Drop for Tmp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn big(n: usize) -> Vec<u8> {
    (0..n).map(|i| (i % 251) as u8).collect()
}

#[test]
fn filt_1_tr_round_trip() {
    let out = run("tr a-z A-Z", b"hello, world\n".to_vec());
    assert_eq!(out.stdout, b"HELLO, WORLD\n");
    assert_eq!(out.stderr, b"");
    assert_eq!(out.status, Some(0));
}

#[test]
fn filt_1_ten_megabytes_through_cat_do_not_deadlock() {
    let input = big(10 * 1024 * 1024);
    let out = run("cat", input.clone());
    assert_eq!(out.status, Some(0));
    assert!(out.stdout == input, "{} bytes in, {} bytes out", input.len(), out.stdout.len());
}

#[test]
fn filt_1_large_stdout_and_stderr_are_read_at_the_same_time() {
    // Written one after the other: reading stdout to the end first would block the child on stderr.
    let out = run("head -c 2000000 /dev/zero >&2; head -c 3000000 /dev/zero", vec![]);
    assert_eq!((out.stderr.len(), out.stdout.len()), (2_000_000, 3_000_000));
}

#[test]
fn filt_1_a_command_that_ignores_its_stdin_is_not_a_problem() {
    let out = run("echo done", big(5 * 1024 * 1024));
    assert_eq!((out.status, out.stdout), (Some(0), b"done\n".to_vec()));
}

#[test]
fn filt_1_stdin_is_closed_after_the_input() {
    // `cat` and `wc -c` only finish at EOF. Empty input must work the same.
    assert_eq!(run("wc -c", vec![]).stdout.trim_ascii(), b"0");
    assert_eq!(run("wc -c", b"abc".to_vec()).stdout.trim_ascii(), b"3");
}

#[test]
fn filt_1_returns_the_process_group_id_and_the_group_has_its_own() {
    let (pgid, rx) = start("echo $$", vec![]);
    let out = rx.recv_timeout(PATIENCE).unwrap();
    // The shell is the leader of a new group, so the group id is its pid.
    assert_eq!(out.stdout.trim_ascii(), pgid.to_string().as_bytes());
    assert_ne!(pgid, std::process::id());
}

#[test]
fn filt_1_done_is_called_exactly_once() {
    let calls = std::sync::Arc::new(AtomicUsize::new(0));
    let (tx, rx) = mpsc::channel();
    let c = calls.clone();
    spawn("echo hi", vec![], move |_| {
        c.fetch_add(1, Ordering::SeqCst);
        let _ = tx.send(());
    })
    .unwrap();
    rx.recv_timeout(PATIENCE).unwrap();
    std::thread::sleep(Duration::from_millis(200));
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[test]
fn filt_1_the_child_is_reaped() {
    if !cfg!(target_os = "linux") {
        return;
    }
    let (pgid, rx) = start("true", vec![]);
    rx.recv_timeout(PATIENCE).unwrap();
    // A zombie keeps its /proc entry until it is waited for; `done` comes after the wait.
    assert!(!Path::new(&format!("/proc/{pgid}")).exists(), "pid {pgid} is a zombie");
}

#[test]
fn filt_1_a_command_that_cannot_be_spawned_is_an_error() {
    // A NUL byte cannot be passed to exec.
    assert!(spawn("echo \0", vec![], |_| {}).is_err());
}

#[test]
fn filt_1_cancel_stops_the_whole_process_group_including_background_children() {
    let dir = Tmp::new();
    let pidfile = dir.0.join("bg.pid");
    let cmd = format!("sleep 30 & echo $! > '{}'; sleep 30; wait", pidfile.display());
    let (pgid, rx) = start(&cmd, vec![]);
    // The background child exists once its pid is written.
    let read_pid = || fs::read_to_string(&pidfile).ok().and_then(|s| s.trim().parse::<i64>().ok());
    assert!(within(Duration::from_secs(5), || read_pid().is_some()), "the shell did not start");
    let bg = read_pid().unwrap();
    assert!(exists(bg) && exists(-i64::from(pgid)), "premise: the group is running");

    cancel(pgid);

    let out = rx.recv_timeout(Duration::from_secs(1)).expect("done not called after cancel");
    assert_eq!(out.status, None, "killed by a signal has no exit code");
    assert!(
        within(Duration::from_secs(1), || !exists(-i64::from(pgid))),
        "the group is still there"
    );
    assert!(!exists(bg), "the background child survived");
}

#[test]
fn filt_1_a_child_left_behind_with_the_pipes_open_does_not_hold_the_filter() {
    // The shell is done at once; `sleep` keeps stdout and stderr open for 20 s, as a daemon that a
    // filter started would. The filter ends with what the shell printed.
    let started = Instant::now();
    let (pgid, rx) = start("sleep 20 & echo hi; echo oops >&2", vec![]);
    let out = rx.recv_timeout(Duration::from_secs(10));
    cancel(pgid); // the sleeper is in the group
    let out = out.expect("done was not called although the shell exited");
    assert!(started.elapsed() < Duration::from_secs(10));
    assert_eq!(
        (out.status, out.stdout, out.stderr),
        (Some(0), b"hi\n".to_vec(), b"oops\n".to_vec())
    );
}

#[test]
fn filt_1_cancel_of_a_finished_or_unknown_group_does_nothing() {
    cancel(u32::MAX / 2); // no such group; must not panic or hurt anything
    let out = run("echo still fine", vec![]);
    assert_eq!(out.stdout, b"still fine\n");
}

#[test]
fn filt_2_nonzero_exit_code_and_stderr_are_captured() {
    let out = run("echo oops >&2; echo partial; exit 3", vec![]);
    assert_eq!(out.status, Some(3));
    assert_eq!(out.stderr, b"oops\n");
    assert_eq!(out.stdout, b"partial\n", "the caller decides to discard the output");
}

#[test]
fn filt_2_a_signal_death_has_no_status() {
    assert_eq!(run("kill -KILL $$", vec![]).status, None);
}
