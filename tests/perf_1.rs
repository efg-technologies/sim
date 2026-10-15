//! PERF-1: `si +q` opens and closes a 10,000-line file within 5 ms (median). si needs a
//! terminal, so scripts/pty_bench.py spawns the release binary 200 times with a pty as stdin,
//! stdout and stderr. Its time includes starting the process from Python; the same harness
//! spawning `/bin/true` is the baseline, and both are reported. The assertion is on the raw
//! median, so it holds with the harness's own cost included. (`sh scripts/bench.sh` uses
//! hyperfine instead, when it is installed.)
//!
//!   cargo test --release --test perf_1 -- --ignored bench --nocapture
mod common;
use common::*;

#[test]
#[ignore]
fn perf_1_bench_startup_under_5ms() {
    release_only();
    let dir = temp_dir("perf1");
    let file = dir.0.join("ten_thousand_lines.rs");
    std::fs::write(&file, code_text(10_000, 70)).unwrap();
    let args = ["startup", env!("CARGO_BIN_EXE_si"), file.to_str().unwrap()];
    let Some(m) = pty_bench(&args) else { return };
    let (si, base) = (m["median_ms"], m["baseline_ms"]);
    eprintln!(
        "PERF-1 median {si:.2} ms; baseline {base:.2} ms; minus baseline {:.2} ms",
        si - base
    );
    assert!(si <= 5.0, "si +q took {si:.2} ms at the median (/bin/true: {base:.2} ms), over 5 ms");
}
