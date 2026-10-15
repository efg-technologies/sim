//! PERF-5: with a 10 MB file open, RSS is at most 50 MB (the size of the binary is checked by
//! scripts/check.sh). scripts/pty_bench.py opens the file in the release binary inside a pty,
//! waits for the first frame and reads /proc/<pid>/status. It reports both VmRSS and the peak
//! (VmHWM); the assertion is on the peak, which is the stricter of the two.
//!
//!   cargo test --release --test perf_5 -- --ignored bench --nocapture
mod common;
use common::*;

#[test]
#[ignore]
fn perf_5_bench_rss_with_a_10mb_file_under_50mb() {
    release_only();
    if !cfg!(target_os = "linux") {
        eprintln!("PERF-5 skipped: VmRSS is read from /proc, which only Linux has");
        return;
    }
    let dir = temp_dir("perf5");
    let file = dir.0.join("ten_mb.rs");
    let text = code_text(100_000, 100);
    assert!(text.len() >= 10 << 20 && text.lines().count() >= 100_000, "{} bytes", text.len());
    std::fs::write(&file, text).unwrap();
    let Some(m) = pty_bench(&["rss", env!("CARGO_BIN_EXE_si"), file.to_str().unwrap()]) else {
        return;
    };
    let (rss, peak) = (m["vmrss_kb"] / 1024.0, m["vmhwm_kb"] / 1024.0);
    eprintln!("PERF-5 VmRSS {rss:.1} MB; peak {peak:.1} MB; file {:.1} MB", m["file_bytes"] / 1e6);
    assert!(
        peak <= 50.0,
        "peak RSS {peak:.1} MB (VmRSS {rss:.1} MB) with a 10 MB file, over 50 MB"
    );
}
