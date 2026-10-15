//! SPEED-1: with the panel open, a key on a 100,000-line file takes at most 1 ms at p99. The time
//! of a key is `Sim::update` plus `Painter::draw` into memory: everything before the write to the
//! terminal (the panel, the composed frame and si's own PERF-2 work all included).
//!
//!   cargo test --release -p sim --test speed_1 -- --ignored bench --nocapture
#![allow(clippy::unwrap_used)] // helpers; the tests themselves may unwrap
mod common;
use common::*;
use si::core::app::{Event, Key};
use sim::paint::Painter;
use sim::sim::{SimEffect, SimEvent};
use std::time::{Duration, Instant};

const P99: Duration = Duration::from_millis(1);

type Times = Vec<(Duration, Key)>;

struct Bench {
    r: Rig,
    term: Painter<Vec<u8>>,
}

impl Bench {
    /// 100,000 lines in `big.rs`, a tree of some 300 files beside it, the panel open, the cursor
    /// in the middle of the file and the focus in the text.
    fn new(size: (u16, u16), colored: bool) -> Bench {
        let code = code_text(100_000, 70);
        let mut files: Vec<String> =
            (0..300).map(|i| format!("dir{}/file{i}.rs", i % 12)).collect();
        files.push(format!("big.rs={code}"));
        let names: Vec<&str> = files.iter().map(String::as_str).collect();
        // A history as full as it gets (MARK-2: 1,000 files), for the marks of the panel.
        let mut r = rig_env("speed-1", &names, &["big.rs"], size, |e| {
            for i in 0..1000 {
                e.history.record(
                    &e.cwd.join(format!("dir{}/file{i}.rs", i % 12)).to_string_lossy(),
                    NOW - 60 * i as u64,
                );
            }
        });
        r.hold_spans = true; // parsing 50,000 lines takes seconds: once, and only if the run is to be colored
        r.keys("<C-n>llll<Esc>"); // the panel open, some directories expanded
        r.keys("50000G");
        if colored {
            r.deliver_spans();
            let colored = r.shot.looks.iter().flatten().any(|(_, l)| *l != sim::paint::Look::Bold);
            assert!(colored, "the text on screen has its colors (SYN-1)");
        }
        assert!(r.sim.panel.open && !r.sim.panel.focus);
        let b = &r.app().buf;
        assert!(
            b.lines() >= 100_000 && b.line_of(b.cursor) == 49_999,
            "the big file, in the middle"
        );
        Bench { r, term: Painter::new(Vec::new()) }
    }

    fn time(&mut self, keys: &str) -> Times {
        common::keys(keys)
            .into_iter()
            .map(|k| {
                let t = Instant::now();
                for e in self.r.sim.update(SimEvent::Term(Event::Key(k))) {
                    if let SimEffect::Render(f) = e {
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

fn motions(b: &mut Bench) -> Times {
    const SET: [&str; 14] = [
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
        "<PageDown>",
        "<PageUp>",
        "0",
        "$",
    ];
    let mut rng = Rng(7);
    let walk: String = (0..4000)
        .map(|i| if i % 100 == 99 { ["G", "gg"][i / 100 % 2] } else { SET[rng.below(SET.len())] })
        .collect();
    b.time(&walk)
}

fn inserts(b: &mut Bench) -> Times {
    let mut out = vec![];
    for s in 0..100 {
        out.extend(b.time(&format!("ilet x{s} = foo(あ, 1);\nok<Esc>")));
        b.time("17j");
    }
    out
}

fn deletes(b: &mut Bench) -> Times {
    let mut out = vec![];
    for _ in 0..100 {
        out.extend(b.time("xxxxxxxdddddddddd"));
        b.time("9j");
    }
    out
}

/// The keys of the panel itself while a big file is open behind it: moving, expanding, collapsing.
fn panel(b: &mut Bench) -> Times {
    b.time("<C-n>");
    let mut out = vec![];
    for _ in 0..100 {
        out.extend(b.time("jjjjjjjjjjllkkkkhhhhhhhh<C-d><C-u>G<Up>gg"));
    }
    out
}

#[test]
#[ignore]
fn speed_1_bench_a_key_with_the_panel_open_takes_under_1ms_at_p99() {
    release_only();
    let mut failed = vec![];
    for size in [(80, 24), (240, 60)] {
        type Kind = fn(&mut Bench) -> Times;
        let kinds: [(&str, Kind); 4] =
            [("motions", motions), ("insert", inserts), ("delete", deletes), ("panel", panel)];
        for (name, run) in kinds {
            run(&mut Bench::new(size, false)); // warm up: page in the text and the allocator
            let mut b = Bench::new(size, true);
            let mut t = run(&mut b);
            t.sort();
            let (p50, p99, max) = (percentile(&t, 50), percentile(&t, 99), t[t.len() - 1]);
            eprintln!(
                "SPEED-1 {}x{} {name:8} keys {:5}  p50 {:>8.1?}  p99 {:>8.1?}  max {:>8.1?} ({:?})",
                size.0,
                size.1,
                t.len(),
                p50.0,
                p99.0,
                max.0,
                max.1
            );
            assert!(t.len() >= 1000);
            if p99.0 > P99 {
                failed.push(format!("{name} {}x{}: p99 {:?}", size.0, size.1, p99.0));
            }
        }
    }
    assert!(failed.is_empty(), "p99 over 1 ms: {failed:?}");
}
