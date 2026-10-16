//! PROP-1
//! notify: debounces changes to one file and hands over the disk content.
use super::file::{opt, stamp};
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, RecvTimeoutError::Disconnected};
use std::time::Duration;

// The wait after the first event of a burst. Well inside the 100 ms of PROP-1.
const DEBOUNCE: Duration = Duration::from_millis(30);

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
    watcher.watch(dir, RecursiveMode::NonRecursive)?;
    let path: PathBuf = path.into();
    // Ends when the watcher is dropped (and the sender with it).
    std::thread::Builder::new().spawn(move || {
        let (mut seen, mut due) = (stamp(&path).ok(), first);
        loop {
            if due {
                let n = super::file::seq();
                seen = stamp(&path).ok();
                opt(fs::read(&path)).into_iter().for_each(|b| on_change(n, b));
            }
            due = match rx.recv_timeout(Duration::from_secs(1)) {
                Ok(()) => {
                    std::thread::sleep(DEBOUNCE);
                    while rx.try_recv().is_ok() {}
                    true
                }
                Err(Disconnected) => return,
                // No event in a second: the file is as the last read saw it, or the watch is dead (PROP-1).
                Err(_) => stamp(&path).ok() != seen,
            };
        }
    })?;
    Ok(watcher)
}
