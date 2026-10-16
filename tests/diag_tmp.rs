//! TEMPORARY diagnostics (removed before merge): what a macOS runner's timers, stamps and FSEvents do.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use notify::{RecursiveMode, Watcher};
use std::fs;
use std::sync::mpsc;
use std::time::{Duration, Instant, SystemTime};

fn ms(d: Duration) -> String {
    format!("{:.0}", d.as_secs_f64() * 1000.0)
}

#[test]
#[ignore]
fn diag_raw() {
    let sleeps: Vec<String> = (0..20)
        .map(|_| {
            let t = Instant::now();
            std::thread::sleep(Duration::from_millis(25));
            ms(t.elapsed())
        })
        .collect();
    eprintln!("DIAG sleep(25ms) took: {}", sleeps.join(" "));
    let (tx, rx) = mpsc::channel::<()>();
    let waits: Vec<String> = (0..20)
        .map(|_| {
            let t = Instant::now();
            let _ = rx.recv_timeout(Duration::from_millis(25));
            ms(t.elapsed())
        })
        .collect();
    drop(tx);
    eprintln!("DIAG recv_timeout(25ms) took: {}", waits.join(" "));

    let dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join(format!("diag-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let file = dir.join("a.txt");
    fs::write(&file, "v0").unwrap();
    // How far the file's mtime is from the clock at the write, and how soon a stat sees it change.
    let mut lag = vec![];
    let mut seen = vec![];
    for i in 0..10 {
        std::thread::sleep(Duration::from_millis(60));
        let before = fs::metadata(&file).unwrap().modified().unwrap();
        let t = Instant::now();
        let now = SystemTime::now();
        fs::write(&file, format!("w{i:03}")).unwrap();
        let mut polls = 0;
        loop {
            polls += 1;
            if fs::metadata(&file).unwrap().modified().unwrap() != before {
                break;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        seen.push(format!("{}ms/{polls}", ms(t.elapsed())));
        let m = fs::metadata(&file).unwrap().modified().unwrap();
        lag.push(format!(
            "{}",
            m.duration_since(now).map_or(-1.0, |d| d.as_secs_f64() * 1000.0) as i64
        ));
    }
    eprintln!("DIAG mtime visible after (ms/polls): {}", seen.join(" "));
    eprintln!("DIAG mtime minus clock at write (ms): {}", lag.join(" "));

    // The raw FSEvents/inotify callback, no debounce, no thread of ours.
    let (tx, rx) = mpsc::channel();
    let mut w = notify::recommended_watcher(move |r: notify::Result<notify::Event>| {
        let _ = tx.send((Instant::now(), r.map(|e| format!("{:?}", e.kind))));
    })
    .unwrap();
    w.watch(&dir, RecursiveMode::NonRecursive).unwrap();
    std::thread::sleep(Duration::from_millis(500));
    while rx.try_recv().is_ok() {}
    let mut raw = vec![];
    for i in 0..10 {
        std::thread::sleep(Duration::from_millis(200));
        while rx.try_recv().is_ok() {}
        let t = Instant::now();
        fs::write(&file, format!("x{i:03}")).unwrap();
        match rx.recv_timeout(Duration::from_secs(2)) {
            Ok((at, k)) => {
                raw.push(format!("{}({})", ms(at.duration_since(t)), k.unwrap_or_default()))
            }
            Err(_) => raw.push("none".into()),
        }
    }
    eprintln!("DIAG raw notify callback after write: {}", raw.join(" "));
    let _ = fs::remove_dir_all(&dir);
}
