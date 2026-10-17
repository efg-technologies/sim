//! PERF-1: `si +q` opens and closes a 10,000-line file within 5 ms (median). si needs a
//! terminal, so scripts/pty_bench.py spawns the release binary 200 times with a pty as stdin,
//! stdout and stderr. Its time includes starting the process from Python; the same harness
//! spawning `/bin/true` is the baseline, and both are reported. The assertion is on the raw
//! median, so it holds with the harness's own cost included. `+q` quits before the first frame, so
//! it does not wait for the terminal's color (VIS-4) and does not ask for it; the time to the first
//! frame and the time to quit after it are printed too, with the pty playing a terminal that
//! answers everything and one that answers nothing (the 3 ms wait is in the first frame, and the
//! 100 ms a quit allows the terminal in the quit). (`sh scripts/bench.sh` uses hyperfine instead, when it is installed.)
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
    let args = ["startup", concat!(env!("CARGO_BIN_EXE_sim"), " --plain"), file.to_str().unwrap()];
    let Some(m) = pty_bench(&args) else { return };
    let (si, base) = (m["median_ms"], m["baseline_ms"]);
    eprintln!(
        "PERF-1 median {si:.2} ms; baseline {base:.2} ms; minus baseline {:.2} ms",
        si - base
    );
    eprintln!(
        "PERF-1 first frame (what `+q` quits before): {:.2} ms; with a terminal that answers nothing {:.2} ms",
        m["first_frame_ms"], m["first_frame_unanswered_ms"]
    );
    eprintln!(
        "PERF-1 `:q` to the exit: {:.2} ms; without OSC 11 {:.2} ms; with a terminal that answers nothing {:.2} ms",
        m["quit_ms"], m["quit_no_osc_ms"], m["quit_unanswered_ms"]
    );
    assert!(si <= 5.0, "si +q took {si:.2} ms at the median (/bin/true: {base:.2} ms), over 5 ms");
}
