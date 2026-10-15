//! PROP-1: the real pieces (App, notify watcher, save thread, an outside process writing the file)
//! wired the way src/main.rs wires them, with random timing. At the end, when everything has
//! settled: a clean buffer is the file; the words the outside process wrote last are in the file,
//! in the buffer or in a hunk. `SI_FUZZ_CASES` is the number of rounds (20 by default).
//! Slow and timing-dependent, so it is ignored: cargo test --release --test shell -- --ignored --nocapture
#![allow(clippy::unwrap_used, clippy::type_complexity, clippy::needless_range_loop)]
mod common;
use common::*;
use si::core::app::{App, Effect, Event, Key};
use si::core::buffer::Buffer;
use si::io::file::{Saved, save};
use si::io::watch::watch;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Sender};
use std::thread;
use std::time::{Duration, Instant};

fn words(s: &str) -> HashSet<String> {
    s.split_whitespace().map(String::from).collect()
}

fn read(path: &str) -> Option<Option<Vec<u8>>> {
    match fs::read(path) {
        Ok(b) => Some(Some(b)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Some(None),
        Err(_) => None,
    }
}

fn watching(tx: &Sender<Event>, path: &str, first: bool) -> Option<notify::RecommendedWatcher> {
    let t = tx.clone();
    let on = move |bytes: Option<Vec<u8>>| {
        drop(t.send(Event::Disk {
            hash: bytes.as_deref().map(si::core::buffer::hash),
            bytes,
            deadline: Instant::now() + Duration::from_millis(50),
        }))
    };
    watch(&si::io::file::resolve(path), on, first).ok()
}

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: u64) -> u64 {
        (self.next() >> 11) % n
    }
}

/// The outside process: for `ms` milliseconds, now and then rewrites, appends to, deletes or
/// re-creates the file. Returns the words of its last content.
fn outsider(
    dir: PathBuf,
    path: PathBuf,
    seed: u64,
    ms: u64,
) -> thread::JoinHandle<HashSet<String>> {
    thread::spawn(move || {
        let mut rng = Rng(seed | 1);
        let mut last: HashSet<String> = HashSet::new();
        let mut lines: Vec<String> =
            fs::read_to_string(&path).unwrap().lines().map(String::from).collect();
        let start = Instant::now();
        let mut n = 0;
        while start.elapsed() < Duration::from_millis(ms) {
            thread::sleep(Duration::from_millis(rng.below(45)));
            n += 1;
            let tok = format!("e{seed}x{n}");
            match rng.below(6) {
                0 => lines.insert(rng.below(lines.len() as u64 + 1) as usize, tok),
                1 if !lines.is_empty() => {
                    let p = rng.below(lines.len() as u64) as usize;
                    lines.remove(p);
                }
                2 if !lines.is_empty() => {
                    let p = rng.below(lines.len() as u64) as usize;
                    lines[p] = format!("{} {tok}", lines[p]);
                }
                3 => {
                    // Gone for a moment.
                    let _ = fs::remove_file(&path);
                    last.clear();
                    thread::sleep(Duration::from_millis(rng.below(40)));
                }
                _ => lines.push(tok),
            }
            let body: String = lines.iter().map(|l| format!("{l}\n")).collect();
            if rng.below(2) == 0 {
                let tmp = dir.join(format!(".ext-{n}"));
                fs::write(&tmp, &body).unwrap();
                fs::rename(&tmp, &path).unwrap();
            } else {
                fs::write(&path, &body).unwrap();
            }
            last = words(&body);
        }
        last
    })
}

fn round(seed: u64) -> Result<(), String> {
    let dir = temp_dir(&format!("shell-{seed}"));
    let state = dir.0.join("state");
    let path: PathBuf = dir.0.join("f");
    let pstr = path.to_str().unwrap().to_string();
    fs::write(&path, "b0\nb1\nb2\n").unwrap();
    let (tx, rx) = mpsc::channel::<Event>();
    let mut watcher = watching(&tx, &pstr, false);
    let first = fs::read(&path).unwrap();
    let mut app = App::new(Some(pstr.clone()), Ok(Some(first)), (80, 12));
    let ext = outsider(dir.0.clone(), path.clone(), seed, 1200);
    let mut rng = Rng(seed.wrapping_mul(0x9e37_79b9_7f4a_7c15) | 1);
    let mut n = 0;
    let mut log: Vec<String> = vec![];
    let run = |app: &mut App,
               ev: Event,
               watcher: &mut Option<notify::RecommendedWatcher>,
               log: &mut Vec<String>| {
        let mut queue = vec![ev];
        while let Some(ev) = queue.pop() {
            log.push(format!("{ev:?}").chars().take(100).collect());
            for fx in app.update(ev) {
                match fx {
                    Effect::Write { path, bytes, expect_hash } => {
                        let (tx, state) = (tx.clone(), state.clone());
                        thread::spawn(move || {
                            let _ = tx.send(match save(&path, &bytes, expect_hash, Some(&state)) {
                                Ok(Saved::Done(h, w)) => Event::Saved(Ok((h, w))),
                                Ok(Saved::Conflict(d)) => Event::SaveConflict(
                                    d,
                                    Instant::now() + Duration::from_millis(50),
                                ),
                                Err(e) => Event::Saved(Err(e)),
                            });
                        });
                    }
                    Effect::Watch(p) => {
                        drop(watcher.take());
                        *watcher = watching(&tx, &p, true); // reads once, on its own thread
                    }
                    _ => {}
                }
            }
        }
    };
    let start = Instant::now();
    let mut user_removed: HashSet<String> = HashSet::new();
    while start.elapsed() < Duration::from_millis(1600) {
        // Events first, then maybe a user action.
        while let Ok(ev) = rx.try_recv() {
            run(&mut app, ev, &mut watcher, &mut log);
        }
        match rng.below(8) {
            0 | 1 => {
                n += 1;
                let before = words(&text(&app));
                for k in keys(&format!("A m{seed}x{n}\x1b")) {
                    run(&mut app, Event::Key(k), &mut watcher, &mut log);
                }
                let _ = before;
            }
            2 => {
                for k in keys(":w\n") {
                    run(&mut app, Event::Key(k), &mut watcher, &mut log);
                }
            }
            3 => {
                n += 1;
                for k in keys(&format!("o m{seed}x{n}\x1b")) {
                    run(&mut app, Event::Key(k), &mut watcher, &mut log);
                }
            }
            4 => {
                let before = words(&text(&app));
                run(&mut app, Event::Key(Key::Char('j')), &mut watcher, &mut log);
                let _ = before;
            }
            _ => {}
        }
        thread::sleep(Duration::from_millis(rng.below(25)));
    }
    let ext_last = ext.join().unwrap();
    // Settle: the watcher reads once more after the last change.
    let settle = Instant::now();
    while settle.elapsed() < Duration::from_millis(700) {
        while let Ok(ev) = rx.try_recv() {
            run(&mut app, ev, &mut watcher, &mut log);
        }
        thread::sleep(Duration::from_millis(10));
    }
    let _ = &mut user_removed;
    let disk = read(&pstr).unwrap();
    let ctx = || {
        format!(
            "\nlast events:\n{}",
            log.iter().rev().take(30).rev().cloned().collect::<Vec<_>>().join("\n")
        )
    };
    match &disk {
        None => {
            if !app.gone && !app.buf.dirty() {
                return Err(format!(
                    "the file is gone, the buffer says it is clean and not gone{}",
                    ctx()
                ));
            }
        }
        Some(d) => {
            if app.gone {
                return Err(format!("the buffer says the file is gone, but it is there{}", ctx()));
            }
            if !app.buf.dirty() && app.buf.encode() != *d {
                return Err(format!(
                    "CLEAN but differs from the file: buffer {:?} file {:?}{}",
                    text(&app),
                    Buffer::decode(d).text.to_string(),
                    ctx()
                ));
            }
            let mut have = words(&Buffer::decode(d).text.to_string());
            have.extend(words(&text(&app)));
            for h in app.buf.hunks.iter() {
                have.extend(words(&h.theirs));
            }
            for w in &ext_last {
                if !have.contains(w) {
                    return Err(format!(
                        "THEIRS lost: {w:?}; file {:?}, buffer {:?}{}",
                        String::from_utf8_lossy(d),
                        text(&app),
                        ctx()
                    ));
                }
            }
        }
    }
    Ok(())
}

#[test]
#[ignore]
fn prop_1_shell_real_threads_real_watcher_random_timing() {
    let rounds = u64::from(cases(20));
    let mut bad = vec![];
    for seed in 1..=rounds {
        if let Err(e) = round(seed * 7919) {
            eprintln!("seed {}: {e}", seed * 7919);
            bad.push(seed * 7919);
        }
    }
    assert!(bad.is_empty(), "failing seeds: {bad:?}");
    let _ = Path::new("");
}
