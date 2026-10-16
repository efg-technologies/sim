//! PERF-2: on a 100,000-line file, handling one key and building the Frame take at most 1 ms
//! at p99, for motions, insertion, deletion and undo. The time of a key is `App::update` plus
//! `Term::draw` into a buffer: everything between the key and the write to the terminal.
//!
//!   cargo test --release --test perf_2 -- --ignored bench --nocapture
// Helpers may unwrap; expected values are written as `vec![a..b]` on purpose: a list holding one range.
#![allow(clippy::unwrap_used, clippy::single_range_in_vec_init)]
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

// ---- marks and hunks: the keys typed after a big change came in ------------------------------------

const LINES: usize = 100_000;

fn statement(i: usize, name: &str) -> String {
    format!("    let {name}_{i} = compute({i}, \"some text on the line\");\n")
}

/// The median of the slowest key of `keys`, over 20 runs, with the cursor in the middle.
fn worst_key(r: &mut Rig, keys: &str) -> Duration {
    r.time("50000G");
    let mut t: Vec<Duration> =
        (0..20).map(|_| r.time(keys).iter().map(|k| k.0).max().unwrap()).collect();
    t.sort();
    t[10]
}

/// Keys that move marks and hunks to other lines (and make an undo step that holds them), and the
/// ones that do not.
const MOVING: [&str; 6] = ["o<Esc>", "dd", "J", "yyp", "5dd", "A!<Esc>"];
const STAYING: [&str; 6] = ["j", "x", "u", "<C-r>", "<C-d>", "zz"];

/// Types every key, prints the worst key of each and returns the slowest of the `MOVING` ones and
/// of the others.
fn report(what: &str, r: &mut Rig) -> (Duration, Duration) {
    let mut worst = (Duration::ZERO, Duration::ZERO);
    for (moving, keys) in MOVING.iter().map(|k| (true, k)).chain(STAYING.iter().map(|k| (false, k)))
    {
        let t = worst_key(r, keys);
        eprintln!("PERF-2 {what:24} {keys:8} {t:>9.1?}");
        if moving { worst.0 = worst.0.max(t) } else { worst.1 = worst.1.max(t) }
    }
    worst
}

/// A formatter rewrites every line of a 100,000-line file: every line is marked. An edit that
/// moves the marks copies them for its undo step, and a copy of a mark is a count, not a clone of
/// its words, whatever the file looks like (a word diff is made for the first 256 KiB only).
#[test]
#[ignore]
fn perf_2_bench_keys_after_a_rewrite_of_every_line_have_100k_marks_to_carry() {
    release_only();
    let old: String = (0..LINES).map(|i| statement(i, "value")).collect();
    let mut r = Rig::on(&old, (80, 24));
    let new: String = (0..LINES).map(|i| statement(i, "renamed")).collect();
    let t = Instant::now();
    r.a.update(Event::Disk {
        bytes: Some(new.into_bytes()),
        hash: None,
        deadline: t + Duration::from_secs(1),
        seq: 0,
    });
    eprintln!("PERF-2 the rewrite took {:?}: {} marks", t.elapsed(), r.a.buf.marks.len());
    assert_eq!(r.a.buf.marks.len(), LINES);
    let (moving, staying) = report("100,000 marks", &mut r);
    assert!(staying < P99, "a key took {staying:?}");
    assert!(moving < Duration::from_micros(500), "a key that moves the marks took {moving:?}");
}

/// Conflicts pile up (nothing is dropped, PROP-2) when a tool keeps rewriting what mine changed.
/// The frame looks at the hunks of the screen only.
#[test]
#[ignore]
fn perf_2_bench_keys_with_many_hunks_to_carry() {
    release_only();
    for (hunks, size) in [(3_000usize, (80u16, 24u16)), (20_000, (80, 24)), (3_000, (240, 60))] {
        let text: String = (0..LINES).map(|i| statement(i, "value")).collect();
        let mut r = Rig::on(&text, size);
        r.a.buf.hunks = (0..hunks)
            .map(|k| {
                let start = k * (LINES / hunks);
                hunk(start..start + 1, &format!("theirs {k}\n"), Some((vec![0..1], vec![0..1])))
            })
            .collect::<Vec<_>>()
            .into();
        let (moving, staying) = report(&format!("{hunks} hunks {}x{}", size.0, size.1), &mut r);
        assert!(moving.max(staying) < P99, "a key took {:?}", moving.max(staying));
    }
}

/// A long list of marks is copied by every edit that moves it, so only the last 4 undo steps
/// keep theirs: the memory of a session stays bounded and the copies reuse what was let go. A short
/// list is kept by every step.
#[test]
fn perf_2_a_long_list_of_marks_is_kept_by_the_last_4_undo_steps_only() {
    for (marks, kept) in [(2000usize, 4usize), (1000, 20)] {
        let text: String = (0..marks).map(|i| format!("L{i}\n")).collect();
        let mut b = si::core::buffer::Buffer::decode(text.as_bytes());
        b.marks = (0..marks).map(|l| mark(l, '~', vec![])).collect::<Vec<_>>().into();
        for _ in 0..20 {
            b.checkpoint();
            b.replace(b.start(0)..b.start(1), "");
        }
        // The first edit took one mark, the second another: step k (1..=20) starts from marks - k + 1.
        for k in (1..=20).rev() {
            assert!(b.travel(true));
            let want = if 20 - k < kept { marks - k + 1 } else { 0 };
            assert_eq!(b.marks.len(), want, "after undoing step {k}");
        }
        assert_eq!(b.text.to_string(), text.trim_end());
    }
}

/// One command that edits every line, on a file whose every line is marked (an AI rewrote it): the
/// marks are moved once per edit, so each edit must not copy the whole list (that is quadratic:
/// 100,000 lines took 3.8 s, 30,000 took 0.35 s in a release build).
#[test]
fn perf_2_a_substitute_over_marked_lines_does_not_copy_the_marks_for_each_line() {
    let n = 30_000;
    let [old, new] =
        ["alpha", "beta"].map(|w| (0..n).map(|i| format!("line {i} {w}\n")).collect::<String>());
    let mut a = app_with(Some("f"), old.as_bytes(), (80, 24));
    a.update(Event::Disk { bytes: Some(new.into_bytes()), hash: None, deadline: far(), seq: 5 });
    assert_eq!(a.buf.marks.len(), n, "every line is marked");
    let t = Instant::now();
    feed(&mut a, ":%s/beta/gamma/\n");
    let took = t.elapsed();
    assert_eq!(a.msg.as_deref(), Some("30000 substitutions on 30000 lines"));
    assert!(a.buf.marks.is_empty(), "the lines the command changed lost their marks");
    assert!(took < Duration::from_secs(2), "{took:?} for {n} lines");
    // Undo brings the marks back, but the one on the line the cursor goes to (PROP-9).
    feed(&mut a, "u");
    assert_eq!(a.buf.marks.len(), n - 1);
    assert!(text(&a).starts_with("line 0 beta\n"));
}
