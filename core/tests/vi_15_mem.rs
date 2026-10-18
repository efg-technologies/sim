//! VI-15 (the memory registers 1 to 9 hold). A file of its own: the test reads the process's RSS, and
//! no other test may run beside it.
mod common;
use common::*;

/// The resident set of this process in KiB (Linux).
fn rss_kb() -> Option<usize> {
    let s = std::fs::read_to_string("/proc/self/status").ok()?;
    let v = s.lines().find(|l| l.starts_with("VmRSS:"))?.split_whitespace().nth(1)?;
    v.parse().ok()
}

#[test]
fn vi_15_nine_big_deletes_cost_no_more_memory_than_the_text_they_share() {
    let Some(_) = rss_kb() else {
        eprintln!("VI-15 memory skipped: VmRSS is read from /proc, which only Linux has");
        return;
    };
    let text = code_text(100_000, 100);
    assert!(text.len() >= 9 << 20, "{} bytes", text.len());
    let mut a = app(&text);
    drop(text);
    let before = rss_kb().unwrap();
    for _ in 0..9 {
        feed(&mut a, "ggdGu");
    }
    let grew = rss_kb().unwrap().saturating_sub(before) / 1024;
    // A register that holds its own copy of a 10 MB delete costs 10 MB: nine would add 90 MB (87 measured).
    // A rope shares the text with the buffer and with the undo step, and adds nothing worth the name.
    assert!(grew < 30, "nine deletes of the whole file grew the process by {grew} MB");
    feed(&mut a, "\"9p");
    assert!(a.buf.text.len_bytes() > 18 << 20, "and \"9 still holds the text");
}
