//! SPEED-1: with the panel open, a key on a 100,000-line file takes at most 1 ms at p99. The time
//! of a key is `Sim::update` plus `Painter::draw` into memory: everything before the write to the
//! terminal (the panel, the composed frame and si's own PERF-2 work all included).
//!
//!   cargo test --release -p sim --test speed_1 -- --ignored bench --nocapture
#![allow(clippy::unwrap_used)] // helpers; the tests themselves may unwrap
mod common;
use common::*;
use sim::color::Syntax;
use sim::layout::Skin;
use sim::paint::Painter;
use sim::sim::{SimEffect, SimEvent};
use sim_core::core::app::{Event, Key};
use std::time::{Duration, Instant};

const P99: Duration = Duration::from_millis(1);

type Times = Vec<(Duration, Key)>;

struct Bench {
    r: Rig,
    /// The highlighter of the run, which keeps its parse states between requests (the shell's does).
    hl: sim::io::Highlighter,
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
        files.push(format!("big.md={}", md_text(20_000)));
        let names: Vec<&str> = files.iter().map(String::as_str).collect();
        // A history as full as it gets (MARK-2: 1,000 files), for the marks of the panel.
        let mut r = rig_env("speed-1", &names, &["big.rs"], size, |e| {
            // The look, with a palette and truecolor, so that the tints, the fog and the card are drawn too (LOOK-3 to LOOK-7).
            (e.skin, e.syn, e.chrome, e.icons, e.md) = (skin, Syntax::new(true), true, true, true);
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
        let hl = sim::io::Highlighter::default();
        Bench { r, hl, term: Painter::new(Vec::new()), size, spent: Default::default() }
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

    /// MD-8: the keys of `keys`, each followed by the answer of the highlighter to what it asked for
    /// (the shell's thread plays that off the clock, and its answer is an event on the main thread:
    /// timed as a key is).
    fn walked(&mut self, keys: &str) -> Times {
        let mut out = vec![];
        for k in common::keys(keys) {
            let mut todo = vec![SimEvent::Term(Event::Key(k))];
            while let Some(ev) = todo.pop() {
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
                out.push((d, k));
                todo.extend(fx.into_iter().filter_map(|e| match e {
                    SimEffect::Highlight(req) => Some(self.hl.run(req)),
                    _ => None,
                }));
            }
        }
        out
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
                        // the pause that opens which-key (KEY-3): the timer is the shell's, the event is timed
                        SimEffect::Pause(n) => todo.push((SimEvent::Pause(n), label)),
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

/// MD-8: Markdown with what the read view works on: prose with markers, tables of Japanese and
/// Latin cells, lists and rules, `lines` lines of it.
fn md_text(lines: usize) -> String {
    let block = |i: usize| {
        format!(
            "## Section {i}\n\nSome **bold** text, *emphasis*, `inline code` and a [link](https://example.com/{i}) in prose with 日本語 mixed in, long enough to wrap in a pane of eighty columns because prose does.\n\n| name | 説明 | n | note |\n|:--|:-:|--:|---|\n| alpha | **強い** | 1 | `a` |\n| beta | 普通 | 22 | _b_ |\n| gamma | ある | 333 | c |\n| 日本語 | x | 4444 | d |\n\n- item **one**\n- item `two`\n- [x] done\n\n---\n\n"
        )
    };
    (0..).map(block).take(lines / 20 + 1).collect()
}

/// MD-8: the read view over a Markdown file of 20,000 lines, tables on most screens: the motions of
/// `motions` (without the jumps to the ends), with the highlighter's answers played after each key.
fn markdown(b: &mut Bench) -> Times {
    b.r.keys(":e big.md\n");
    b.r.deliver_spans();
    b.r.keys("10000G");
    b.r.deliver_spans();
    assert!(b.r.sim.md && b.r.status().contains("big.md"), "{}", b.r.status());
    const SET: [&str; 12] = [
        "j",
        "k",
        "w",
        "b",
        "e",
        "<C-d>",
        "<C-u>",
        "<Down>",
        "<Up>",
        "<PageDown>",
        "<PageUp>",
        "$",
    ];
    let mut rng = Rng(5);
    let walk: String = (0..3000).map(|_| SET[rng.below(SET.len())]).collect();
    let t = b.walked(&walk);
    // the last screen is the read view's: tables, and the markers left out of every line but the cursor's
    let rows = &b.r.shot.frame.rows;
    assert!(rows.iter().any(|r| r.text.contains('|') || r.text.contains('│')));
    assert!(rows.iter().filter(|r| r.text.contains("**")).count() <= 2, "the read view is on");
    t
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

/// KEY-3: which-key's window open over the big file: a pause after Space, after an operator (the
/// longest list: motions and objects), `z`, `g` and `]`, the next key typed on with the window open
/// (the window of the longer prefix opens at once) and Esc. The pause is played as an event, timed.
fn whichkey(b: &mut Bench) -> Times {
    let mut out = vec![];
    for _ in 0..60 {
        out.extend(b.picked("<Space>b<Esc>d<Esc>di<Esc>yj<Esc>zz<Esc>gj<Esc>]<Esc>[<Esc>"));
        out.extend(b.picked("<Space>bp<Esc><Space>bc<Esc>zzzz"));
    }
    out
}

/// KEY-4: the help in the picker over the big file: `:help` and F1, typed into (the filter by
/// fuzzy match over every key of the table), moved in, cleared and closed.
fn help(b: &mut Bench) -> Times {
    let mut out = vec![];
    for _ in 0..40 {
        out.extend(b.picked(":help\n"));
        out.extend(b.picked("tab<C-n><C-n><C-p><BS><BS><BS>leader<C-n><C-u>dd<C-n><Esc>"));
        out.push((b.one(SimEvent::Help), Key::Char('?'))); // F1 is an event of its own
        out.extend(b.picked("<C-n><C-n><C-n>search<BS><C-p><Esc>"));
    }
    out
}

/// DIFF-1: the changes view over 300 changed files, in a repository made here (the bench's files
/// committed, then a line added to each small one): moved in by lines and pages, folded and unfolded,
/// to the top and the bottom. The numbers are asked of git and answered off the clock, as the status.
fn changes(b: &mut Bench) -> Times {
    let root = b.r.sim.root.clone();
    git(&root, &["init", "-q"]);
    git(&root, &["add", "-A"]);
    git(&root, &["commit", "-q", "-m", "bench"]);
    for i in 0..300 {
        let at = root.join(format!("dir{}/file{i}.rs", i % 12));
        std::fs::write(at, format!("dir{}/file{i}.rs\nmore\n", i % 12)).unwrap();
    }
    b.r.send(SimEvent::Status(sim::io::status(&root)));
    b.r.keys(" ge");
    assert!(b.r.sim.panel.focus && b.r.status().contains("CHANGES"), "{}", b.r.status());
    let mut out = vec![];
    for _ in 0..40 {
        out.extend(b.time("jjjjjjjjjjkkkkkkkkkk<C-d><C-u><PageDown><PageUp>Gggjjhljjkk"));
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
            let kinds: [(&str, Kind); 10] = [
                ("motions", motions),
                ("insert", inserts),
                ("delete", deletes),
                ("panel", panel),
                ("mouse", mouse),
                ("picker", picker),
                ("whichkey", whichkey),
                ("help", help),
                ("changes", changes),
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

/// MD-8: a key in a Markdown file whose screens hold tables, with the read view on, takes at most
/// 1 ms at p99, in calm and in depth, at both sizes.
///
///   cargo test --release -p sim-editor --test speed_1 -- --ignored md_8 --nocapture
#[test]
#[ignore]
fn md_8_bench_a_key_in_a_markdown_file_with_tables_takes_under_1ms_at_p99() {
    release_only();
    let mut failed = vec![];
    for skin in [Skin::Calm, Skin::Depth] {
        for size in [(80, 24), (240, 60)] {
            markdown(&mut Bench::new(size, false, skin)); // warm up
            let mut b = Bench::new(size, true, skin);
            let mut t = markdown(&mut b);
            t.sort();
            let (p50, p99, max) = (percentile(&t, 50), percentile(&t, 99), t[t.len() - 1]);
            eprintln!(
                "MD-8 {skin:?} {}x{} keys {:5}  p50 {:>8.1?}  p99 {:>8.1?}  max {:>8.1?} ({:?})  mean: update {:>7.1?}, painter {:>7.1?}",
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
            if p99.0 > P99 {
                failed.push(format!("{skin:?} {}x{}: p99 {:?}", size.0, size.1, p99.0));
            }
        }
    }
    assert!(failed.is_empty(), "p99 over 1 ms: {failed:?}");
}
