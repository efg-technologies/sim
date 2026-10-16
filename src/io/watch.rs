//! PROP-1
//! notify: debounces changes to one file and hands over the disk content.
use super::file::opt;
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::{Duration, Instant};

// The wait after the first event of a burst. Well inside the 100 ms of PROP-1.
const DEBOUNCE: Duration = Duration::from_millis(30);

// FSEvents (macOS) reports a change after 40 to 200 ms on a CI runner, which PROP-1's 100 ms cannot
// bear, so there the watcher also looks at the file's stamp this often (inotify is prompt: no looking).
// The worst case is this plus DEBOUNCE, 55 ms plus the read; at 40 wake-ups a second the idle cost is
// one `stat` each.
const POLL: Option<Duration> =
    if cfg!(target_os = "macos") { Some(Duration::from_millis(25)) } else { None };

/// What the file system says of the file: time, size and inode, `None` for no file.
fn stamp(path: &Path) -> Option<(i64, i64, u64, u64)> {
    fs::metadata(path).ok().map(|m| (m.mtime(), m.mtime_nsec(), m.size(), m.ino()))
}

/// Watches the parent directory of `path` (so saves by rename, deletion and re-creation are seen)
/// and calls `on_change(seq, Some(bytes))` with the file's new content, or `on_change(seq, None)`
/// when it no longer exists; `seq` is the number of the read, taken as it started (PROP-1).
/// Changes are debounced: a burst of events yields one call, and the content is read within 100 ms
/// of the first event. With `first`, the file is also read once at the start,
/// on the same thread, so that this read cannot overtake a later one. Keep the returned watcher
/// alive while watching.
pub fn watch(
    path: &Path,
    on_change: impl Fn(u64, Option<Vec<u8>>) + Send + 'static,
    first: bool,
) -> notify::Result<RecommendedWatcher> {
    watch_polling(path, on_change, first, POLL)
}

/// `watch`, and with `poll` the file's stamp (time, size, inode) is also looked at that often: a
/// change the system reports late, or not at all (a write through a link in another directory), is
/// taken in at the next look.
pub fn watch_polling(
    path: &Path,
    on_change: impl Fn(u64, Option<Vec<u8>>) + Send + 'static,
    first: bool,
    poll: Option<Duration>,
) -> notify::Result<RecommendedWatcher> {
    let dir = path.parent().filter(|d| !d.as_os_str().is_empty()).unwrap_or(Path::new("."));
    let name = path.file_name().map(ToOwned::to_owned);
    let (tx, rx) = mpsc::channel();
    // Only the file's own name counts. Reading it does not make events (Access is skipped anyway).
    let mut watcher = notify::recommended_watcher(move |res: notify::Result<Event>| {
        let mine = |e: &Event| {
            e.paths.is_empty() || e.paths.iter().any(|p| p.file_name() == name.as_deref())
        };
        if res.is_ok_and(|e| !matches!(e.kind, EventKind::Access(_)) && mine(&e)) {
            let _ = tx.send(());
        }
    })?;
    let path: PathBuf = path.into();
    // Taken before the watching starts, not by the thread: a change before it runs is still a change.
    let seen = stamp(&path);
    watcher.watch(dir, RecursiveMode::NonRecursive)?;
    // Ends when the watcher is dropped (and the sender with it).
    std::thread::Builder::new().spawn(move || {
        let read = || {
            let n = super::file::seq();
            opt(fs::read(&path)).into_iter().for_each(|b| on_change(n, b))
        };
        let mut seen = seen;
        if first {
            read();
        }
        loop {
            let got = match poll {
                Some(p) => rx.recv_timeout(p),
                None => rx.recv().map_err(Into::into),
            };
            match got {
                Err(RecvTimeoutError::Timeout) if stamp(&path) == seen => continue,
                Err(RecvTimeoutError::Disconnected) => break,
                _ => {}
            }
            let end = Instant::now() + DEBOUNCE;
            while let Some(left) = end.checked_duration_since(Instant::now())
                && rx.recv_timeout(left).is_ok()
            {}
            seen = stamp(&path); // before the read: a write during it is a change again
            read();
        }
    })?;
    Ok(watcher)
}
