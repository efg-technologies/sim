//! SPEED-1: with the panel open, a key on a 100,000-line file takes at most 1 ms at p99. The time
//! of a key is `Sim::update` plus `Painter::draw` into memory: everything before the write to the
//! terminal (the panel, the composed frame and si's own PERF-2 work all included).
//!
//!   cargo test --release -p sim --test speed_1 -- --ignored bench --nocapture
#![allow(clippy::unwrap_used)] // helpers; the tests themselves may unwrap
mod common;
use common::*;
use si::core::app::{Event, Key};
use sim::color::Syntax;
use sim::layout::Skin;
use sim::paint::Painter;
use sim::sim::{SimEffect, SimEvent};
use std::time::{Duration, Instant};

const P99: Duration = Duration::from_millis(1);

type Times = Vec<(Duration, Key)>;

struct Bench {
    r: Rig,
    term: Painter<Vec<u8>>,
    size: (u16, u16),
    /// What the events took in `Sim::update` and in the painter, all together (for the print).
    spent: (Duration, Duration),
}

impl Bench {
    /// 100,000 lines in `big.rs`, a tree of some 300 files beside it, the panel open, the cursor
    /// in the middle of the file and the focus in the text.
    fn new(size: (u16, u16), colored: bool, skin: Skin) -> Bench {
        let code = code_text(100_000, 70);
        let mut files: Vec<String> =
            (0..300).map(|i| format!("dir{}/file{i}.rs", i % 12)).collect();
        files.push(format!("big.rs={code}"));
        let names: Vec<&str> = files.iter().map(String::as_str).collect();
        // A history as full as it gets (MARK-2: 1,000 files), for the marks of the panel.
        let mut r = rig_env("speed-1", &names, &["big.rs"], size, |e| {
            // The look, with a palette and truecolor, so that the tints, the fog and the card are drawn too (LOOK-3 to LOOK-7).
            (e.skin, e.syn, e.chrome, e.icons) = (skin, Syntax::new(true), true, true);
            for i in 0..1000 {
                e.history.record(
                    &e.cwd.join(format!("dir{}/file{i}.rs", i % 12)).to_string_lossy(),
                    NOW - 60 * i as u64,
                );
            }
        });
        r.hold_spans = true; // parsing 50,000 lines takes seconds: once, and only if the run is to be colored
        r.send(SimEvent::Term(Event::Bg((0x1e, 0x1e, 0x1e))));
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
        Bench { r, term: Painter::new(Vec::new()), size, spent: Default::default() }
    }

    /// The time of one event: the state's update and the painter's bytes into memory.
    fn one(&mut self, ev: SimEvent) -> Duration {
        let t = Instant::now();
        let fx = self.r.sim.update(ev);
        let mid = t.elapsed();
        for e in fx {
            if let SimEffect::Render(f) = e {
                self.term.draw(f).unwrap();
            }
        }
        let d = t.elapsed();
        self.spent = (self.spent.0 + mid, self.spent.1 + d - mid);
        self.term.out.clear();
        d
    }

    fn time(&mut self, keys: &str) -> Times {
        let keys = common::keys(keys);
        keys.into_iter().map(|k| (self.one(SimEvent::Term(Event::Key(k))), k)).collect()
    }

    /// The keys of the picker (PICK-2) with the shell played after each: what it asks for is done
    /// off the clock (the search, the read of a preview, the highlighter) and what comes back is
    /// an event of its own that is timed as a key is, since it is on the main thread too.
    fn picked(&mut self, keys: &str) -> Times {
        let mut out = vec![];
        for k in common::keys(keys) {
            let mut todo = vec![(SimEvent::Term(Event::Key(k)), k)];
            while let Some((ev, label)) = todo.pop() {
                let t = Instant::now();
                let fx = self.r.sim.update(ev);
                let mid = t.elapsed();
                for e in &fx {
                    if let SimEffect::Render(f) = e {
                        self.term.draw(f.clone()).unwrap();
                    }
                }
                let d = t.elapsed();
                self.spent = (self.spent.0 + mid, self.spent.1 + d - mid);
                self.term.out.clear();
                out.push((d, label));
                for e in fx {
                    match e {
                        SimEffect::Preview(path) => {
                            todo.push((sim::io::preview(&self.r.sim.root, &path), label));
                        }
                        SimEffect::Highlight(req) => {
                            todo.push((sim::io::Highlighter::default().run(req), label));
                        }
                        SimEffect::Grep(req) => {
                            let mut batches = vec![];
                            sim::io::grep(&req, None, &|| true, &mut |b| batches.push(b));
                            todo.extend(batches.into_iter().map(|b| (SimEvent::Grep(b), label)));
                        }
                        _ => {}
                    }
                }
            }
        }
        out
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

/// UI-14: the motions of `motions` with the matches of a search lit in every line on screen.
fn lit(b: &mut Bench) -> Times {
    b.r.keys("/let\n");
    motions(b)
}

/// PICK-1 to PICK-5: the picker open over the big file: Ctrl-p's finder typed into, moved in and
/// cleared, and Ctrl-f's search of contents (over the 300 files and the big one), with the previews
/// that come, their colors and the batches of results.
fn picker(b: &mut Bench) -> Times {
    let mut out = vec![];
    for _ in 0..40 {
        out.extend(b.picked("<C-p>file1<C-n><C-n><C-p><C-n>2<BS><BS><C-u>dir3/fi<C-n><Esc>"));
        out.extend(b.picked("<C-f>fn<C-n><C-n><BS>f<C-p><C-u>let<C-n><Esc>"));
    }
    out
}

/// The mouse (PTR-1) on the big file with the panel open: clicks in the text, drags (a press, some
/// moves, a release), the wheel in the text (three lines a notch, the cursor keeping its context)
/// and over the panel. Each event is timed, as a key is.
fn mouse(b: &mut Bench) -> Times {
    use crossterm::event::{KeyModifiers, MouseButton as B, MouseEvent, MouseEventKind as K};
    let ev = |kind, column, row| {
        SimEvent::Mouse(MouseEvent { kind, column, row, modifiers: KeyModifiers::NONE })
    };
    let (w, h) = b.size;
    let mut rng = Rng(11);
    let mut out = vec![];
    let at = |rng: &mut Rng| {
        ((27 + rng.below(usize::from(w) - 27)) as u16, rng.below(usize::from(h) - 1) as u16)
    };
    for round in 0..600 {
        let (x, y) = at(&mut rng);
        // The key it is reported under: `c` a click, `d` a drag, `w` the wheel over the text, `r` over the panel.
        let (mut events, label) = match round % 4 {
            0 => (vec![ev(K::Down(B::Left), x, y), ev(K::Up(B::Left), x, y)], 'c'),
            1 => {
                let (x2, y2) = at(&mut rng);
                let moves = [(x, y), (x2, y), (x2, y2), (x, y2)];
                let drags = moves.iter().map(|&(a, b)| ev(K::Drag(B::Left), a, b));
                let press = [ev(K::Down(B::Left), x, y)];
                (press.into_iter().chain(drags).chain([ev(K::Up(B::Left), x, y2)]).collect(), 'd')
            }
            2 => {
                let down = (round / 4) % 2 == 0;
                let notch = if down { K::ScrollDown } else { K::ScrollUp };
                ((0..6).map(|_| ev(notch, x, y)).collect(), 'w')
            }
            _ => (
                (0..4)
                    .map(|_| ev(K::ScrollDown, 3, y))
                    .chain((0..4).map(|_| ev(K::ScrollUp, 3, y)))
                    .collect(),
                'r',
            ),
        };
        events.push(SimEvent::Term(Event::Key(Key::Esc))); // leaves the selection a drag made
        out.extend(events.into_iter().map(|e| (b.one(e), Key::Char(label))));
    }
    out
}

#[test]
#[ignore]
fn speed_1_bench_a_key_with_the_panel_open_takes_under_1ms_at_p99() {
    release_only();
    let mut failed = vec![];
    // Each look: calm is si's; depth has the boxes, the tints, the card and the fog (LOOK-3 to LOOK-7).
    for skin in [Skin::Calm, Skin::Depth] {
        for size in [(80, 24), (240, 60)] {
            type Kind = fn(&mut Bench) -> Times;
            let kinds: [(&str, Kind); 7] = [
                ("motions", motions),
                ("insert", inserts),
                ("delete", deletes),
                ("panel", panel),
                ("mouse", mouse),
                ("picker", picker),
                ("lit", lit),
            ];
            for (name, run) in kinds {
                // `SPEED_1_ONLY=lit` runs the kinds named, for a look at one of them
                if std::env::var("SPEED_1_ONLY").is_ok_and(|o| !o.split(',').any(|k| k == name)) {
                    continue;
                }
                run(&mut Bench::new(size, false, skin)); // warm up: page in the text and the allocator
                let mut b = Bench::new(size, true, skin);
                let mut t = run(&mut b);
                t.sort();
                let (p50, p99, max) = (percentile(&t, 50), percentile(&t, 99), t[t.len() - 1]);
                eprintln!(
                    "SPEED-1 {skin:?} {}x{} {name:8} keys {:5}  p50 {:>8.1?}  p99 {:>8.1?}  max {:>8.1?} ({:?})  mean: update {:>7.1?}, painter {:>7.1?}",
                    size.0,
                    size.1,
                    t.len(),
                    p50.0,
                    p99.0,
                    max.0,
                    max.1,
                    b.spent.0 / t.len() as u32,
                    b.spent.1 / t.len() as u32
                );
                if name == "mouse" {
                    for label in ['c', 'd', 'w', 'r'] {
                        let of: Times =
                            t.iter().copied().filter(|k| k.1 == Key::Char(label)).collect();
                        eprintln!(
                            "          {label} {:5}  p99 {:>8.1?}  max {:>8.1?}",
                            of.len(),
                            percentile(&of, 99).0,
                            of[of.len() - 1].0
                        );
                    }
                }
                assert!(t.len() >= 1000);
                // A wheel notch and the first move of a drag render twice (the cursor goes where the
                // first render showed it must), so an event of the mouse has twice a key's time (PTR-1).
                let most = if name == "mouse" { 2 * P99 } else { P99 };
                if p99.0 > most {
                    failed.push(format!("{skin:?} {name} {}x{}: p99 {:?}", size.0, size.1, p99.0));
                }
            }
        }
    }
    assert!(
        failed.is_empty(),
        "p99 over its time (1 ms for a key, 2 ms for the mouse): {failed:?}"
    );
}
