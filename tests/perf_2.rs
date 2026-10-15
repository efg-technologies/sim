//! PERF-2: on a 100,000-line file, handling one key and building the Frame take at most 1 ms
//! at p99, for motions, insertion, deletion and undo. The time of a key is `App::update` plus
//! `Term::draw` into a buffer: everything between the key and the write to the terminal.
//!
//!   cargo test --release --test perf_2 -- --ignored bench --nocapture
#![allow(clippy::unwrap_used)] // helpers; the tests themselves may unwrap
mod common;
use common::*;
use si::core::app::{App, Effect, Event, Key};
use si::io::term::Term;
use std::time::{Duration, Instant};

const P99: Duration = Duration::from_millis(1);

type Times = Vec<(Duration, Key)>;
type Kind = fn(&mut Rig) -> Times;

/// An App on 100,000 lines with the cursor in the middle, and a Term drawing into memory.
struct Rig {
    a: App,
    term: Term<Vec<u8>>,
}

impl Rig {
    fn new(size: (u16, u16)) -> Rig {
        let mut r = Rig::on(&code_text(100_000, 70), size);
        r.a.buf.cursor = r.a.buf.start(50_000);
        r
    }

    fn on(text: &str, size: (u16, u16)) -> Rig {
        Rig {
            a: app_with(Some("big.rs"), text.as_bytes(), size),
            term: Term::new(Vec::new(), false),
        }
    }

    /// How long each key takes.
    fn time(&mut self, keys: &str) -> Times {
        common::keys(keys)
            .into_iter()
            .map(|k| {
                let t = Instant::now();
                for e in self.a.update(Event::Key(k)) {
                    if let Effect::Render(f) = e {
                        self.term.draw(f).unwrap();
                    }
                }
                let d = t.elapsed();
                self.term.out.clear();
                (d, k)
            })
            .collect()
    }
}

/// A pseudo-random walk over the motions, with `G` and `gg` now and then.
fn motions(r: &mut Rig) -> Times {
    const SET: [&str; 18] = [
        "j",
        "k",
        "w",
        "b",
        "e",
        "%",
        "<C-d>",
        "<C-u>",
        "<Down>",
        "<Up>",
        "<Left>",
        "<Right>",
        "<PageDown>",
        "<PageUp>",
        "<Home>",
        "<End>",
        "0",
        "$",
    ];
    let mut rng = Rng(7);
    let walk: String = (0..4000)
        .map(|i| if i % 100 == 99 { ["G", "gg"][i / 100 % 2] } else { SET[rng.below(18)] })
        .collect();
    r.time(&walk)
}

/// 100 insert sessions, `i` to Esc, of about 25 keys each (an Enter and a Japanese char too).
fn inserts(r: &mut Rig) -> Times {
    let mut out = vec![];
    for s in 0..100 {
        out.extend(r.time(&format!("ilet x{s} = foo(あ, 1);\nok<Esc>")));
        r.time("17j");
    }
    out
}

fn deletes(r: &mut Rig) -> Times {
    let mut out = vec![];
    for _ in 0..100 {
        out.extend(r.time("xxxxxxxdddddddddd"));
        r.time("9j");
    }
    out
}

fn undos(r: &mut Rig) -> Times {
    for _ in 0..3000 {
        r.time("xj");
    }
    let mut out = r.time(&"u".repeat(2000));
    out.extend(r.time(&"<C-r>".repeat(1000)));
    assert!(r.a.msg.is_none(), "every undo and redo found something to do: {:?}", r.a.msg);
    out
}

#[test]
#[ignore]
fn perf_2_bench_a_key_on_100k_lines_takes_under_1ms_at_p99() {
    release_only();
    let mut failed = vec![];
    for size in [(80, 24), (240, 60)] {
        let kinds: [(&str, Kind); 4] =
            [("motions", motions), ("insert", inserts), ("delete", deletes), ("undo", undos)];
        for (name, run) in kinds {
            run(&mut Rig::new(size)); // warm up: page in the text and the allocator
            let mut r = Rig::new(size);
            let mut t = run(&mut r);
            t.sort();
            let (p50, p99, max) = (percentile(&t, 50), percentile(&t, 99), t[t.len() - 1]);
            eprintln!(
                "PERF-2 {}x{} {name:8} keys {:5}  p50 {:>8.1?}  p99 {:>8.1?}  max {:>8.1?} ({:?})",
                size.0,
                size.1,
                t.len(),
                p50.0,
                p99.0,
                max.0,
                max.1
            );
            assert!(t.len() >= 1000);
            assert!(r.a.buf.dirty() != (name == "motions"), "{name}: the text changed or not");
            if p99.0 > P99 {
                failed.push(format!("{name} {}x{}: p99 {:?}", size.0, size.1, p99.0));
            }
        }
    }
    assert!(failed.is_empty(), "p99 over 1 ms: {failed:?}");
}

/// `%` on a bracket with no partner scans the whole file: the slowest key there is. The bench
/// text has no `{` or `}`, so one is added at the far end; `%` must leave the cursor alone.
/// Reported, not asserted: the scan is O(file), whatever the iteration costs.
#[test]
#[ignore]
fn perf_2_bench_percent_on_an_unmatched_bracket_reports_the_worst_key() {
    release_only();
    let code = code_text(100_000, 70);
    for (what, text, at) in
        [("{ forward", format!("{{\n{code}"), 0), ("} backward", format!("{code}}}"), code.len())]
    {
        let mut r = Rig::on(&text, (80, 24));
        r.a.buf.cursor = text[..at].chars().count();
        let cursor = r.a.buf.cursor;
        let mut t: Vec<Duration> = (0..7).map(|_| r.time("%")[0].0).collect();
        t.sort();
        assert_eq!(r.a.buf.cursor, cursor, "{what}: no partner, no move");
        eprintln!(
            "PERF-2 % unmatched {what:10} {} KB  best {:>8.1?}  median {:>8.1?}",
            text.len() / 1000,
            t[0],
            t[3]
        );
    }
}
