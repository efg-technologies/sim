//! PROP-1: changes on disk (in place, by rename, deletion, re-creation) reach the callback quickly.
// Helpers outside `#[test]` fns are not covered by clippy.toml, so allow it for the file.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use notify::RecommendedWatcher;
use si::io::watch::watch;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

/// How long to wait for something that must happen. Generous: a pass never waits this long.
const PATIENCE: Duration = Duration::from_secs(5);

/// A unique directory under target/tmp, removed on drop.
struct Tmp(PathBuf);

impl Tmp {
    fn new() -> Tmp {
        static N: AtomicUsize = AtomicUsize::new(0);
        let n = N.fetch_add(1, Ordering::SeqCst);
        let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
            .join(format!("watch-{}-{n}", std::process::id()));
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

type Change = Option<Vec<u8>>;

/// A watched file "a.txt" (holding "v0", unless `initial` is false), and the channel its callbacks arrive on.
struct Fixture {
    dir: Tmp,
    file: PathBuf,
    rx: Receiver<Change>,
    _watcher: RecommendedWatcher,
}

fn fixture() -> Fixture {
    fixture_with(true)
}

fn fixture_with(initial: bool) -> Fixture {
    let dir = Tmp::new();
    let file = dir.0.join("a.txt");
    if initial {
        fs::write(&file, "v0").unwrap();
    }
    let (tx, rx) = mpsc::channel();
    let _watcher = watch(&file, move |_, c| drop(tx.send(c)), false).unwrap();
    Fixture { dir, file, rx, _watcher }
}

impl Fixture {
    /// Waits until a callback carries `want`, and says how long that took.
    /// Earlier callbacks with other content (a half-written file) are skipped.
    fn expect(&self, want: Option<&[u8]>) -> Duration {
        let start = Instant::now();
        loop {
            let left = PATIENCE.checked_sub(start.elapsed()).unwrap_or_default();
            match self.rx.recv_timeout(left) {
                Ok(got) if got.as_deref() == want => return start.elapsed(),
                Ok(_) => {}
                Err(e) => panic!("no callback with {want:?} within {PATIENCE:?}: {e:?}"),
            }
        }
    }

    /// Everything that arrives within `quiet` of the last callback.
    fn drain(&self, quiet: Duration) -> Vec<Change> {
        let mut got = vec![];
        while let Ok(c) = self.rx.recv_timeout(quiet) {
            got.push(c);
        }
        got
    }
}

#[test]
fn prop_1_write_in_place() {
    let f = fixture();
    fs::write(&f.file, "v1").unwrap();
    f.expect(Some(b"v1"));
}

#[test]
fn prop_1_save_by_rename() {
    let f = fixture();
    let tmp = f.dir.0.join(".a.txt.swp");
    fs::write(&tmp, "v2").unwrap();
    fs::rename(&tmp, &f.file).unwrap();
    f.expect(Some(b"v2"));
}

#[test]
fn prop_1_delete_then_recreate() {
    let f = fixture();
    fs::remove_file(&f.file).unwrap();
    f.expect(None);
    fs::write(&f.file, "back").unwrap();
    f.expect(Some(b"back"));
}

#[test]
fn prop_1_a_file_that_appears_after_watching_is_seen() {
    // `si new.txt` while another process creates the file.
    let f = fixture_with(false);
    fs::write(&f.file, "hello").unwrap();
    f.expect(Some(b"hello"));
}

#[test]
fn prop_1_unrelated_files_in_the_directory_are_ignored() {
    let f = fixture();
    let other = f.dir.0.join("b.txt");
    fs::write(&other, "x").unwrap();
    fs::write(&other, "y").unwrap();
    fs::rename(&other, f.dir.0.join("c.txt")).unwrap();
    fs::remove_file(f.dir.0.join("c.txt")).unwrap();
    fs::write(f.dir.0.join("a.txt.bak"), "z").unwrap();
    assert_eq!(f.drain(Duration::from_millis(300)), Vec::<Change>::new());
    // The silence was not a dead watcher.
    fs::write(&f.file, "v1").unwrap();
    f.expect(Some(b"v1"));
}

#[test]
fn prop_1_a_burst_of_writes_makes_few_calls_and_the_last_content_arrives() {
    let f = fixture();
    for i in 0..20 {
        fs::write(&f.file, format!("w{i}")).unwrap();
    }
    let calls = f.drain(Duration::from_millis(300));
    assert!(calls.len() <= 3, "a burst of 20 writes made {} calls: {calls:?}", calls.len());
    assert_eq!(calls.last().cloned().flatten().as_deref(), Some(&b"w19"[..]), "{calls:?}");
}

#[test]
fn prop_1_latency_from_write_to_callback_is_under_100ms() {
    let f = fixture();
    // The median of several writes, so one scheduling hiccup doesn't fail the run.
    let mut took: Vec<Duration> = (0..7)
        .map(|i| {
            std::thread::sleep(Duration::from_millis(60)); // let the previous burst settle
            f.drain(Duration::from_millis(1));
            let content = format!("lat{i}");
            let start = Instant::now();
            fs::write(&f.file, &content).unwrap();
            f.expect(Some(content.as_bytes()));
            start.elapsed()
        })
        .collect();
    took.sort();
    let median = took[took.len() / 2];
    eprintln!("PROP-1 latency: median {median:?}, all {took:?}");
    // CI machines are noisy and share cores; the spec's 100 ms is the target, not a CI gate.
    let (limit, who) = match std::env::var_os("CI") {
        Some(_) => (Duration::from_millis(500), "CI (limit relaxed from the 100 ms of PROP-1)"),
        None => (Duration::from_millis(100), "local"),
    };
    assert!(median < limit, "{who}: median latency {median:?} over {limit:?}; all: {took:?}");
}

#[test]
fn prop_1_watching_a_missing_directory_is_an_error_not_a_panic() {
    let dir = Tmp::new();
    let r = watch(&dir.0.join("no-such-dir").join("a.txt"), |_, _| {}, false);
    assert!(r.is_err());
}

#[test]
fn prop_1_dropping_the_watcher_stops_the_calls() {
    let Fixture { dir: _dir, file, rx, _watcher } = fixture();
    drop(_watcher);
    fs::write(&file, "after drop").unwrap();
    // The callback (and the sender inside it) is gone with the watcher's thread.
    assert_eq!(rx.recv_timeout(Duration::from_millis(500)), Err(RecvTimeoutError::Disconnected));
}

#[test]
fn prop_1_a_watcher_asked_to_read_first_reads_the_file_once_on_its_own_thread() {
    let dir = Tmp::new();
    let file = dir.0.join("a.txt");
    fs::write(&file, "v0").unwrap();
    let (tx, rx) = mpsc::channel();
    let main = std::thread::current().id();
    let _watcher =
        watch(&file, move |_, c| drop(tx.send((c, std::thread::current().id()))), true).unwrap();
    let (got, thread) = rx.recv_timeout(PATIENCE).unwrap();
    assert_eq!(got.as_deref(), Some(&b"v0"[..]), "the file as it is, without any change");
    assert_ne!(thread, main, "never on the caller's thread");
    // Nothing else comes until the file changes, and then the change comes after it.
    assert_eq!(rx.recv_timeout(Duration::from_millis(300)), Err(RecvTimeoutError::Timeout));
    fs::write(&file, "v1").unwrap();
    loop {
        let (got, _) = rx.recv_timeout(PATIENCE).unwrap();
        if got.as_deref() == Some(&b"v1"[..]) {
            break;
        }
    }
    // A file that is not there is told too (None).
    let dir2 = Tmp::new();
    let missing = dir2.0.join("none.txt");
    let (tx, rx) = mpsc::channel();
    let _w = watch(&missing, move |_, c| drop(tx.send(c)), true).unwrap();
    assert_eq!(rx.recv_timeout(PATIENCE).unwrap(), None);
}

#[test]
fn prop_1_every_read_is_numbered_as_it_starts_from_one_counter() {
    let dir = Tmp::new();
    let file = dir.0.join("a.txt");
    fs::write(&file, "v0").unwrap();
    let (tx, rx) = mpsc::channel();
    let _w = watch(&file, move |n, c| drop(tx.send((n, c))), true).unwrap();
    let (first, c) = rx.recv_timeout(PATIENCE).unwrap();
    assert_eq!(c.as_deref(), Some(&b"v0"[..]));
    // A number taken after this read ended is larger; one taken before the next one started is
    // smaller: the numbers order the reads of every thread, not only the watcher's own.
    let between = si::io::file::seq();
    assert!(first < between);
    fs::write(&file, "v1").unwrap();
    let (second, c) = rx.recv_timeout(PATIENCE).unwrap();
    assert_eq!(c.as_deref(), Some(&b"v1"[..]));
    assert!(between < second && second < si::io::file::seq(), "{first} {between} {second}");
}
