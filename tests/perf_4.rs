//! PERF-4: on a 100,000-line file, a search that matches nothing finishes within 50 ms, for a
//! literal and for a regex, with or without a literal in it that keeps lines away from regex-lite
//! (about 20 ns a byte). Lines lacking the literal are skipped, and a big buffer is searched in 8
//! chunks on 8 threads; the last two rows have no literal that helps, so they need the threads.
//!
//!   cargo test --release --test perf_4 -- --ignored bench --nocapture
mod common;
use common::*;
use si::core::app::{App, Event, Key};
use std::time::{Duration, Instant};

const BUDGET: Duration = Duration::from_millis(50);

/// Types `/pattern` and then Enter; returns how long Enter (the search and the Frame) took.
fn search(a: &mut App, pattern: &str) -> Duration {
    feed(a, &format!("/{pattern}"));
    let t = Instant::now();
    drop(a.update(Event::Key(Key::Enter)));
    t.elapsed()
}

fn app_on(text: &str) -> App {
    let mut a = app_with(Some("big.rs"), text.as_bytes(), (80, 24));
    a.buf.cursor = a.buf.start(50_000);
    a
}

#[test]
#[ignore]
fn perf_4_bench_a_search_that_matches_nothing_takes_under_50ms() {
    release_only();
    let text = code_text(100_000, 70);
    // Every line holds `foo` and `bar`, so the literal keeps nothing out.
    let hostile = text.replace('\n', " foobar\n");
    let rows = [
        (&text, "zzqx"),
        (&text, "ZZQX"),
        (&text, "foo\\d+bar"),
        (&text, "\\d+zzqx"),
        (&hostile, "foo\\d+bar"),
        (&text, "[a-z]+\\d+[A-Z]{9}"),
    ];
    let mut failed = vec![];
    for (text, pattern) in rows {
        let mut a = app_on(text);
        let cursor = a.buf.cursor;
        search(&mut a, pattern); // warm up
        let mut t: Vec<Duration> = (0..5).map(|_| search(&mut a, pattern)).collect();
        t.sort();
        assert_eq!(a.msg.as_deref(), Some(&*format!("not found: {pattern}")));
        assert_eq!(a.buf.cursor, cursor, "a failed search leaves the cursor alone");
        eprintln!(
            "PERF-4 /{pattern:18} {:>5} KB  best {:>7.1?}  median {:>7.1?}  worst {:>7.1?}",
            text.len() / 1000,
            t[0],
            t[2],
            t[4]
        );
        if t[4] > BUDGET {
            failed.push(format!("/{pattern}: {:?}", t[4]));
        }
    }
    assert!(failed.is_empty(), "over 50 ms (worst of 5): {failed:?}");
}

/// VI-14: a key of the search line searches again from the start, so on the worst file it may take
/// what PERF-4 allows a search. Types each pattern one key at a time, Backspace and Esc included.
#[test]
#[ignore]
fn vi_14_bench_a_key_of_the_search_line_takes_under_50ms() {
    release_only();
    let text = code_text(100_000, 70);
    let hostile = text.replace('\n', " foobar\n");
    let rows = [
        (&text, "zzqx"),
        (&text, "foo\\d+bar"),
        (&hostile, "foo\\d+bar"),
        (&text, "[a-z]+\\d+[A-Z]{9}"),
    ];
    let mut failed = vec![];
    for (text, pattern) in rows {
        let mut a = app_on(text);
        let cursor = a.buf.cursor;
        let mut worst = Duration::ZERO;
        let mut key = |a: &mut App, k: Key| {
            let t = Instant::now();
            drop(a.update(Event::Key(k)));
            worst = worst.max(t.elapsed());
        };
        key(&mut a, Key::Char('/'));
        pattern.chars().for_each(|c| key(&mut a, Key::Char(c)));
        (0..pattern.len()).for_each(|_| key(&mut a, Key::Backspace));
        key(&mut a, Key::Esc);
        assert_eq!(a.buf.cursor, cursor, "Esc gives the start back");
        eprintln!("VI-14 /{pattern:18} worst key {worst:>7.1?}");
        if worst > BUDGET {
            failed.push(format!("/{pattern}: {worst:?}"));
        }
    }
    assert!(failed.is_empty(), "over 50 ms: {failed:?}");
}
