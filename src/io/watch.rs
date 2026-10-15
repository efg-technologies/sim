//! PROP-1
//! notify: debounces changes to one file and hands over the disk content.
use super::file::opt;
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::{Duration, Instant};

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
        let read = || {
            let n = super::file::seq();
            opt(fs::read(&path)).into_iter().for_each(|b| on_change(n, b))
        };
        if first {
            read();
        }
        while rx.recv().is_ok() {
            let end = Instant::now() + DEBOUNCE;
            while let Some(left) = end.checked_duration_since(Instant::now())
                && rx.recv_timeout(left).is_ok()
            {}
            read();
        }
    })?;
    Ok(watcher)
}
