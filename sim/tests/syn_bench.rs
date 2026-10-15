//! SYN-3: what loading the syntaxes and highlighting cost, in a release build. This is its own
//! test binary so that the syntax set is not yet loaded when the first test runs.
//!
//!   cargo test --release -p sim --test syn_bench -- --ignored bench --nocapture
#![allow(clippy::unwrap_used)] // helpers; the tests themselves may unwrap
use sim::syn::{Syn, syntaxes};
use std::time::{Duration, Instant};

/// Rust-looking source: `n` lines of declarations, strings, comments and control flow.
fn source(n: usize) -> Vec<String> {
    let block = [
        "/// Doc comment for the next function.",
        "pub fn compute(input: &str, limit: usize) -> Result<Vec<u32>, String> {",
        "    let mut out = Vec::new(); // results",
        "    for (i, word) in input.split_whitespace().enumerate() {",
        "        if i >= limit { break; }",
        "        match word.parse::<u32>() {",
        "            Ok(n) if n % 2 == 0 => out.push(n),",
        "            Ok(_) => continue,",
        "            Err(e) => return Err(format!(\"bad number {word:?}: {e}\")),",
        "        }",
        "    }",
        "    /* a block comment",
        "       over two lines */",
        "    Ok(out)",
        "}",
        "",
    ];
    (0..n).map(|i| block[i % block.len()].to_string()).collect()
}

#[test]
#[ignore = "benchmark: run with --release"]
fn syn_3_bench_load_first_highlight_and_edit() {
    let text = source(100_000);
    let line = |i: usize| text.get(i).cloned();
    let t = Instant::now();
    let _ = syntaxes();
    let load = t.elapsed();
    let t = Instant::now();
    let mut syn = Syn::new("main.rs", "").unwrap();
    let find = t.elapsed();
    let t = Instant::now();
    let first = syn.spans(&line, 0, 50);
    let first_window = t.elapsed();
    assert_eq!(first.len(), 50);
    // A second language: its contexts are decoded the first time they are used.
    let t = Instant::now();
    let mut py = Syn::new("tool.py", "").unwrap();
    py.spans(&|i| ["def f(x):", "    return 'a' # c"].get(i).map(|s| s.to_string()), 0, 2);
    let second_language = t.elapsed();
    // Parsing speed on a long run, which also caches the states of the first 70,000 lines.
    let rss = || {
        let status = std::fs::read_to_string("/proc/self/status").unwrap_or_default();
        let kb = status.lines().find_map(|l| l.strip_prefix("VmRSS:"));
        kb.and_then(|v| v.trim().trim_end_matches("kB").trim().parse::<u64>().ok()).unwrap_or(0)
    };
    let mut cached = Syn::new("main.rs", "").unwrap();
    let before = rss();
    let t = Instant::now();
    cached.spans(&line, 70_000, 1);
    let per_line = t.elapsed() / 70_000;
    let held = rss().saturating_sub(before);
    drop(cached);
    syn.spans(&line, 50, 70_000);
    // A jump far below the cached states parses everything between (SYN-3: in the background).
    let t = Instant::now();
    syn.spans(&line, 80_000, 50);
    let jump = t.elapsed();
    // Typing at line 60,000 (the cursor line, then the 50 lines of a screen after it).
    let (mut one, mut screen) = (Duration::ZERO, Duration::ZERO);
    for e in 60_000..60_000 + sim::syn::EVERY {
        syn.edit(e);
        let t = Instant::now();
        syn.spans(&line, e, 1);
        one = one.max(t.elapsed());
        syn.edit(e);
        let t = Instant::now();
        syn.spans(&line, e, 50);
        screen = screen.max(t.elapsed());
    }
    println!("load the syntax set:                     {load:.2?}");
    println!("find the syntax of main.rs:              {find:.2?}");
    println!("first 50 lines of a Rust file (cold):    {first_window:.2?}");
    println!("first use of a second language:          {second_language:.2?}");
    println!("per line, in a long run of Rust:         {per_line:.2?}");
    println!("memory of the states of 70,000 lines:    {held} kB resident");
    println!("jump 10,000 lines below the states:      {jump:.2?}");
    println!("after an edit: the edited line, worst:   {one:.2?}");
    println!("after an edit: that line and 49, worst:  {screen:.2?}");
    assert!(load < Duration::from_millis(100), "{load:?}");
    assert!(per_line < Duration::from_micros(200), "{per_line:?}");
    assert!(one < Duration::from_millis(1), "{one:?}");
}
