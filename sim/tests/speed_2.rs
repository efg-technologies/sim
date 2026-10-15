//! SPEED-2: in a repository of 100,000 files, sim shows its first frame within 20 ms; the list of
//! files arrives later. The real binary is started under a pseudo-terminal (`script`) and the
//! time to the first frame is read off the bytes it writes, from the start of the process.
//!
//!   cargo test --release -p sim --test speed_2 -- --ignored bench --nocapture
#![allow(clippy::unwrap_used)] // helpers; the tests themselves may unwrap
mod common;
use common::*;
use sim::io;
use sim::paint::Painter;
use sim::sim::{Env, Sim, SimEffect, start};
use std::io::Read;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const FILES: usize = 100_000;

/// A repository of 100,000 files: 1,000 directories of 100.
fn big_repo() -> TempDir {
    let d = temp_dir("speed-2-repo");
    for i in 0..FILES {
        let dir = d.0.join(format!("pkg{:03}/mod{:02}", i / 1000, (i / 100) % 10));
        if i % 100 == 0 {
            std::fs::create_dir_all(&dir).unwrap();
        }
        std::fs::write(dir.join(format!("file{i}.rs")), "fn f() {}\n").unwrap();
    }
    git(&d.0, &["init", "-q"]);
    git(&d.0, &["add", "-A"]);
    git(&d.0, &["commit", "-q", "-m", "init"]);
    d
}

/// Runs `cmd` under a pty of 80x24 and returns the time until `marker` was written.
fn first_output(dir: &std::path::Path, cmd: &str, marker: &str, env: &[(&str, &str)]) -> Duration {
    let t = Instant::now();
    let mut child = Command::new("script")
        .args(["-qfec", &format!("stty rows 24 cols 80; {cmd}"), "/dev/null"])
        .current_dir(dir)
        .envs(env.iter().copied())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut out = child.stdout.take().unwrap();
    let (mut seen, mut buf) = (Vec::new(), [0u8; 4096]);
    let at = loop {
        let n = out.read(&mut buf).unwrap();
        assert!(n > 0, "the program ended before the marker: {}", String::from_utf8_lossy(&seen));
        seen.extend(&buf[..n]);
        if String::from_utf8_lossy(&seen).contains(marker) {
            break t.elapsed();
        }
    };
    drop(child.stdin.take()); // end of input: sim gets a hangup and ends
    let _ = child.kill();
    let _ = child.wait();
    at
}

#[test]
#[ignore = "benchmark: run with --release"]
fn speed_2_bench_first_frame_in_a_repository_of_100000_files() {
    release_only();
    let d = big_repo();
    // In process: what happens before the first frame is drawn (the arguments, the state,
    // the first Sim, its frame written into memory).
    let t = Instant::now();
    let cwd = d.0.clone();
    let s = start(&[], &cwd, |_| false, |p| p.is_dir()).unwrap();
    let (_sim, fx) = Sim::new(s, Env { cwd, size: (80, 24), now: 1, ..Env::default() });
    let mut p = Painter::new(Vec::new());
    for e in fx {
        if let SimEffect::Render(f) = e {
            p.draw(f).unwrap();
        }
    }
    let in_process = t.elapsed();
    // How long the list and the status take, in the background.
    let t = Instant::now();
    let list = io::files(&d.0);
    let list_time = t.elapsed();
    let t = Instant::now();
    io::status(&d.0).unwrap();
    let status_time = t.elapsed();
    assert_eq!(list.len(), FILES);
    // The real binary under a pty, including the wrapper's own time (script, sh and stty), which
    // a baseline run with a `printf` in place of sim shows.
    let bin = env!("CARGO_BIN_EXE_sim");
    let state = temp_dir("speed-2-state");
    let env = [
        ("HOME", state.0.to_str().unwrap()),
        ("XDG_STATE_HOME", state.0.to_str().unwrap()),
        ("COLORTERM", ""),
    ];
    let median = |f: &dyn Fn() -> Duration| {
        let mut v: Vec<Duration> = (0..15).map(|_| f()).collect();
        v.sort();
        v[v.len() / 2]
    };
    let base =
        median(&|| first_output(&d.0, "printf 'Ctrl-n  tree'; sleep 1", "Ctrl-n  tree", &env));
    let sim = median(&|| first_output(&d.0, &format!("exec {bin}"), "Ctrl-n  tree", &env));
    println!("SPEED-2: {FILES} files");
    println!("  before the first frame, in process:      {in_process:.2?}");
    println!(
        "  the real binary under a pty, to frame 1: {sim:.2?} (script, sh and stty alone: {base:.2?})"
    );
    println!("  the list of files (in the background):   {list_time:.2?}");
    println!("  git status (in the background):          {status_time:.2?}");
    assert!(in_process < Duration::from_millis(20), "{in_process:?}");
    assert!(sim < Duration::from_millis(20), "{sim:?}: the wrapper takes {base:?} of it");
}
