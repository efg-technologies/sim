//! VIS-4, VIS-5 on the terminal: the OSC 11 query for the background and its answer (crossterm reads
//! the answer as keys), the wait of at most 3 ms, the palette App computes from the color, and the
//! truecolor SGR that Term sends for the four styles. The math itself is tests/color.rs.
#![allow(clippy::unwrap_used)]
mod common;
use common::*;
use crossterm::event::{Event as Ev, KeyCode, KeyEvent, KeyModifiers};
use proptest::prelude::*;
use si::core::app::{App, Event, Key};
use si::core::color::{Rgb, palette};
use si::core::view::Frame;
use si::io::term::{self, Reply, Term, wait_bg};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

const NONE: KeyModifiers = KeyModifiers::NONE;
const ALT: KeyModifiers = KeyModifiers::ALT;

/// The backgrounds VIS-4 names.
const BGS: [Rgb; 8] = [
    (0x00, 0x00, 0x00),
    (0x1e, 0x1e, 0x1e),
    (0x28, 0x2c, 0x34),
    (0x00, 0x2b, 0x36),
    (0xff, 0xff, 0xff),
    (0xfa, 0xfa, 0xfa),
    (0xfd, 0xf6, 0xe3),
    (0xef, 0xf1, 0xf5),
];

fn key(c: char, m: KeyModifiers) -> Ev {
    Ev::Key(KeyEvent::new(KeyCode::Char(c), m))
}

/// A typed char, as crossterm reads it (uppercase comes with Shift).
fn typed(c: char) -> Ev {
    key(c, if c.is_uppercase() { KeyModifiers::SHIFT } else { NONE })
}

/// What crossterm makes of `ESC ] body` and the end of the answer, `ESC \` (Alt+`\`) or BEL (Ctrl-g).
fn answer(body: &str, bel: bool) -> Vec<Ev> {
    let end = if bel { key('g', KeyModifiers::CONTROL) } else { key('\\', ALT) };
    [key(']', ALT)].into_iter().chain(body.chars().map(typed)).chain([end]).collect()
}

fn chars(s: &str) -> Vec<Ev> {
    s.chars().map(typed).collect()
}

fn feed_all(r: &mut Reply, evs: Vec<Ev>) -> Vec<Event> {
    evs.into_iter().flat_map(|e| r.feed(e)).collect()
}

/// What App would get, as text: a char as itself, Esc as `^`, the color as `<r,g,b>`.
fn show(evs: &[Event]) -> String {
    let one = |e: &Event| match e {
        Event::Key(Key::Char(c)) => c.to_string(),
        Event::Key(Key::Esc) => "^".into(),
        Event::Key(Key::Enter) => "\n".into(),
        Event::Key(k) => format!("{k:?}"),
        Event::Bg((r, g, b)) => format!("<{r},{g},{b}>"),
        e => format!("{e:?}"),
    };
    evs.iter().map(one).collect()
}

#[test]
fn vis_4_the_query_for_the_background_is_not_part_of_enter() {
    // It goes out after `enter`'s own bytes, and only if the keys given at startup have not quit.
    let enter = String::from_utf8(term::ENTER.to_vec()).unwrap();
    assert!(!enter.contains("\x1b]11"), "{enter:?}");
    assert!(enter.starts_with("\x1b[?1049h"), "the screen is switched first");
    assert_eq!(term::ASK, b"\x1b]11;?\x1b\\");
}
#[test]
fn vis_4_an_answer_ending_with_st_or_bel_becomes_the_color_and_no_key() {
    for (body, rgb) in [
        ("11;rgb:1e1e/1e1e/1e1e", (0x1e, 0x1e, 0x1e)),
        ("11;rgb:ffff/ffff/ffff", (255, 255, 255)),
        ("11;rgb:FFFF/0000/AAAA", (255, 0, 170)),
        ("11;rgb:fa/fa/fa", (0xfa, 0xfa, 0xfa)),
        ("11;#fdf6e3", (0xfd, 0xf6, 0xe3)),
    ] {
        for bel in [false, true] {
            let got = feed_all(&mut Reply::default(), answer(body, bel));
            assert_eq!(show(&got), format!("<{},{},{}>", rgb.0, rgb.1, rgb.2), "{body} {bel}");
        }
    }
}

#[test]
fn vis_4_an_answer_split_across_reads_is_put_together() {
    let mut r = Reply::default();
    let evs = answer("11;rgb:2828/2c2c/3434", false);
    // However the bytes were cut into reads, nothing comes out until the answer ends.
    for cut in 1..evs.len() {
        let mut r2 = Reply::default();
        let (a, b) = evs.split_at(cut);
        let first = feed_all(&mut r2, a.to_vec());
        assert!(first.is_empty(), "{cut}: {}", show(&first));
        assert_eq!(show(&feed_all(&mut r2, b.to_vec())), "<40,44,52>", "{cut}");
    }
    assert_eq!(show(&feed_all(&mut r, evs)), "<40,44,52>");
    // And it can be followed by another one (the terminal asked again): the state is clean.
    assert_eq!(show(&feed_all(&mut r, answer("11;rgb:0/0/0", true))), "<0,0,0>");
}

#[test]
fn vis_4_an_answer_mixed_with_typed_keys_never_reaches_the_app() {
    let mut r = Reply::default();
    let mut evs = chars("ab");
    evs.extend(answer("11;rgb:1e1e/1e1e/1e1e", false));
    evs.extend(chars("cd"));
    assert_eq!(show(&feed_all(&mut r, evs)), "ab<30,30,30>cd");
    // The Alt+key rule of VI-9 is for keys: Esc, then the key. It does not apply to an answer,
    // and an answer does not stop it from applying afterwards.
    let evs =
        [key('x', ALT)].into_iter().chain(answer("11;rgb:0/0/0", false)).chain([key('y', ALT)]);
    assert_eq!(show(&feed_all(&mut r, evs.collect())), "^x<0,0,0>^y");
}

#[test]
fn vis_4_an_answer_that_is_garbage_is_swallowed_and_keys_that_are_not_one_go_on() {
    let mut r = Reply::default();
    // An answer to the query whose color cannot be read: nothing for App, and no keys.
    for body in ["11;zzz", "11;", "11;rgb:1/2", "11;rgb:gg/00/00", "11;rgb:1/2/3/4", "11;#12345"] {
        assert_eq!(show(&feed_all(&mut r, answer(body, false))), "", "{body}");
    }
    // Not an answer to our query: whatever was typed goes on, in order (Esc, then the keys).
    for (body, bel, want) in [
        ("x", false, "^]x^\\"),
        ("12;a", false, "^]12;a^\\"),
        ("1", true, "^]1Ctrl('g')"),
        ("", false, "^]^\\"),
        ("11", false, "^]11^\\"),
    ] {
        assert_eq!(show(&feed_all(&mut r, answer(body, bel))), want, "{body}");
    }
    // An answer cut short by a key that is not a char: the keys go on, then that key.
    let evs = [key(']', ALT)]
        .into_iter()
        .chain(chars("11;rgb:1"))
        .chain([Ev::Key(KeyEvent::new(KeyCode::Enter, NONE))]);
    assert_eq!(show(&feed_all(&mut r, evs.collect())), "^]11;rgb:1\n");
    let evs = [key(']', ALT), typed('1'), Ev::Resize(10, 5)];
    assert_eq!(show(&feed_all(&mut r, evs.to_vec())), "^]1Resize(10, 5)");
    // A terminator by itself is the key it is.
    assert_eq!(show(&feed_all(&mut r, vec![key('g', KeyModifiers::CONTROL)])), "Ctrl('g')");
    assert_eq!(show(&feed_all(&mut r, vec![key('\\', ALT)])), "^\\");
    // A text that never ends is not kept for ever: past 40 chars it goes on as keys, in order.
    let body = format!("11;{}", "a".repeat(60));
    let evs: Vec<Ev> = [key(']', ALT)].into_iter().chain(chars(&body)).collect();
    let got = show(&feed_all(&mut r, evs));
    assert_eq!(got, format!("^]{body}"));
    // The state is clean after all of that.
    assert_eq!(show(&feed_all(&mut r, answer("11;rgb:0/0/0", false))), "<0,0,0>");
}

proptest! {
    #![proptest_config(ProptestConfig { cases: cases(500), failure_persistence: None, ..ProptestConfig::default() })]

    /// Typed keys, Alt+keys and complete answers in any order and cut into reads anywhere: every
    /// typed key reaches App in order, each answer becomes one color, and nothing else comes out.
    #[test]
    fn vis_4_fuzz_typed_keys_survive_answers(
        items in prop::collection::vec(prop_oneof![
            4 => prop::sample::select(vec!['a', 'x', 'Z', ' ', '1', '1', ';', 'r', 'g', '/', ']', '\\']).prop_map(|c| (0u8, c, false)),
            1 => prop::sample::select(vec!['x', 'q', '1']).prop_map(|c| (1u8, c, false)),
            2 => (prop::sample::select(vec!['0', 'f', 'A', '8']), any::<bool>()).prop_map(|(h, bel)| (2u8, h, bel)),
        ], 0..30),
    ) {
        let (mut evs, mut want) = (vec![], String::new());
        for (kind, c, bel) in items {
            match kind {
                0 => { evs.push(typed(c)); want.push(c); }
                1 => { evs.push(key(c, ALT)); want.push('^'); want.push(c); }
                _ => {
                    let h = c.to_string().repeat(2);
                    evs.extend(answer(&format!("11;rgb:{h}/{h}/{h}"), bel));
                    let v = u8::from_str_radix(&h, 16).unwrap();
                    want += &format!("<{v},{v},{v}>");
                }
            }
        }
        let got = feed_all(&mut Reply::default(), evs);
        prop_assert_eq!(show(&got), want);
    }

    /// Whatever arrives, Reply neither panics nor keeps what it holds for ever: a key that is not
    /// a char empties it, and it can read an answer afterwards.
    #[test]
    fn vis_4_fuzz_reply_survives_anything(
        evs in prop::collection::vec(prop_oneof![
            3 => prop::sample::select(vec![']', '1', ';', 'r', 'g', 'b', ':', '/', '\\', 'x', '#']).prop_map(typed),
            2 => prop::sample::select(vec![']', '\\', 'g', 'x']).prop_map(|c| key(c, ALT)),
            1 => prop::sample::select(vec!['g', '\\']).prop_map(|c| key(c, KeyModifiers::CONTROL)),
            1 => Just(Ev::Key(KeyEvent::new(KeyCode::Enter, NONE))),
            1 => Just(Ev::Resize(3, 3)),
        ], 0..60),
    ) {
        let mut r = Reply::default();
        for e in evs {
            drop(r.feed(e));
        }
        drop(r.feed(Ev::Key(KeyEvent::new(KeyCode::Esc, NONE))));
        prop_assert_eq!(show(&feed_all(&mut r, answer("11;rgb:0/0/0", false))), "<0,0,0>");
    }
}

/// A fake reader of the terminal: it sends the color after `ms` milliseconds. Returns the receiver,
/// how long `wait_bg` took with `wait` as its allowance, and what it gave back.
fn waited(ms: u64, wait: Duration) -> (mpsc::Receiver<Event>, Duration, (Option<Rgb>, Vec<Event>)) {
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        thread::sleep(Duration::from_millis(ms));
        drop(tx.send(Event::Bg((1, 2, 3))));
    });
    let start = Instant::now();
    let got = wait_bg(&rx, start + wait);
    (rx, start.elapsed(), got)
}

#[test]
fn vis_4_the_first_frame_waits_for_the_color_at_most_3_ms() {
    let (rx, took, (bg, got)) = waited(400, Duration::from_millis(3));
    assert!(bg.is_none() && got.is_empty(), "{bg:?} {got:?}");
    assert!(took >= Duration::from_millis(3), "gave up after {took:?}, before the 3 ms");
    // Generous: a loaded machine may be late, but the wait does not follow the reader's 400 ms.
    assert!(took < Duration::from_millis(200), "waited {took:?} for a 3 ms allowance");
    // The late color is not lost: it is in the queue, for App to redraw with.
    assert!(matches!(rx.recv_timeout(Duration::from_secs(5)), Ok(Event::Bg((1, 2, 3)))));
}

#[test]
fn vis_4_a_color_that_comes_in_time_ends_the_wait_at_once() {
    // An allowance of 5 s, and a reader that answers at once: the wait ends with the color.
    let (_rx, took, (bg, got)) = waited(0, Duration::from_secs(5));
    assert!(bg == Some((1, 2, 3)) && got.is_empty(), "{bg:?} {got:?}");
    assert!(took < Duration::from_secs(2), "{took:?}");
}

#[test]
fn vis_4_what_comes_before_the_color_is_kept_in_order_and_what_comes_after_stays_queued() {
    let (tx, rx) = mpsc::channel();
    tx.send(Event::Notice("one".into())).unwrap();
    tx.send(Event::Key(Key::Char('x'))).unwrap();
    tx.send(Event::Bg((9, 9, 9))).unwrap();
    tx.send(Event::Notice("after".into())).unwrap();
    let (bg, got) = wait_bg(&rx, Instant::now() + Duration::from_secs(5));
    assert_eq!((bg, show(&got)), (Some((9, 9, 9)), "Notice(\"one\")x".to_string()));
    assert!(matches!(rx.try_recv(), Ok(Event::Notice(m)) if m == "after"));
    // An allowance that has run out takes nothing, even when events are queued.
    tx.send(Event::Notice("late".into())).unwrap();
    assert!(matches!(wait_bg(&rx, Instant::now()), (None, v) if v.is_empty()));
    assert!(matches!(rx.try_recv(), Ok(Event::Notice(m)) if m == "late"));
}

#[test]
fn vis_4_the_app_computes_the_palette_of_the_color_and_the_frame_carries_it() {
    let mut a = app("one\ntwo\n");
    assert_eq!(screen(&mut a).pal, None, "no answer, no palette: reverse and faint stand in");
    for bg in BGS {
        let fx = a.update(Event::Bg(bg));
        assert_eq!(last_frame(&fx).pal, Some(palette(bg)), "{bg:?}");
        assert_eq!(screen(&mut a).pal, Some(palette(bg)), "{bg:?}: it stays for the next frames");
    }
}

#[test]
fn vis_5_the_palette_the_app_uses_is_exactly_the_one_computed_from_the_background() {
    for bg in BGS {
        let mut a = app("one\n");
        a.update(Event::Bg(bg));
        let p = screen(&mut a).pal.unwrap();
        assert_eq!(p, palette(bg));
        assert_eq!(p.bg, bg);
        // The change background is the one VIS-5 checked, and it is the only chromatic color.
        let gray = |(r, g, b): Rgb| r == g && g == b;
        assert!(gray(p.primary) && gray(p.secondary) && gray(p.change_text) && !gray(p.change));
    }
}

/// The bytes of a draw, with the escapes visible and a line break before each row.
fn visible(out: &str) -> String {
    let mut s = out.replace('\x1b', "\\e");
    for row in 1..=30 {
        s = s.replace(&format!("\\e[{row};1H"), &format!("\n\\e[{row};1H"));
    }
    s.replace("\\e[?2026l", "\n\\e[?2026l")
}

fn draw(t: &mut Term<Vec<u8>>, f: &Frame) -> String {
    t.out.clear();
    t.draw(f.clone()).unwrap();
    String::from_utf8(t.out.clone()).unwrap()
}

/// A change from the disk on a small screen: the three styles are on it.
fn changed_word(bg: Option<Rgb>) -> App {
    let mut a = app_with(Some("f.rs"), b"one\ntwo words\nthree\n", (24, 6));
    let bytes = Some(b"one\nTWO words\nthree\n".to_vec());
    a.update(Event::Disk { bytes, hash: None, deadline: far(), seq: 0 });
    feed(&mut a, "G");
    bg.into_iter().for_each(|rgb| drop(a.update(Event::Bg(rgb))));
    a
}

#[test]
fn vis_4_term_sends_the_styles_as_truecolor_golden() {
    let bg = (0x1e, 0x1e, 0x1e);
    let p = palette(bg);
    let mut a = changed_word(Some(bg));
    let out = draw(&mut Term::new(Vec::new(), false), &screen(&mut a));
    golden("vis_4_colored_sgr", &visible(&out));
    let fg = |(r, g, b): Rgb| format!("38;2;{r};{g};{b}");
    let bgc = |(r, g, b): Rgb| format!("48;2;{r};{g};{b}");
    // Primary: its fg. Secondary: its fg. Change: its bg and the text color on it.
    for want in [
        format!("\x1b[0;{}m", fg(p.primary)),
        format!("\x1b[0;{}m", fg(p.secondary)),
        format!("\x1b[0;{};{}m", bgc(p.change), fg(p.change_text)),
    ] {
        assert!(out.contains(&want), "{want:?} missing from {out:?}");
    }
    assert!(!out.contains(";7m") && !out.contains("[0;2"), "no fallback with a palette: {out:?}");
}

#[test]
fn vis_4_without_an_answer_reverse_and_faint_stand_in() {
    let mut a = changed_word(None);
    let out = draw(&mut Term::new(Vec::new(), false), &screen(&mut a));
    // Secondary is faint (SGR 2), Change is reverse (7).
    for want in ["\x1b[0;2m", "\x1b[0;7m", "\x1b[0m"] {
        assert!(out.contains(want), "{want:?} missing from {out:?}");
    }
    assert!(!out.contains("38;2") && !out.contains("48;2"), "{out:?}");
}

#[test]
fn vis_4_a_late_answer_redraws_everything_with_colors() {
    let mut a = app_with(Some("f.rs"), b"one\ntwo\nthree\n", (24, 6));
    let mut t = Term::new(Vec::new(), false);
    let first = draw(&mut t, &screen(&mut a));
    assert!(!first.contains("38;2"));
    assert!(draw(&mut t, &screen(&mut a)).len() < 20, "nothing changed: only the cursor moves");
    let fx = a.update(Event::Bg((0xff, 0xff, 0xff)));
    let late = draw(&mut t, last_frame(&fx));
    // Every row again, in colors, in one synchronized update, though no text changed.
    assert!(late.starts_with("\x1b[?2026h\x1b[2J") && late.ends_with("\x1b[?2026l"), "{late:?}");
    for row in 1..=6 {
        assert!(late.contains(&format!("\x1b[{row};1H")), "row {row} not redrawn: {late:?}");
    }
    assert!(late.contains("\x1b[0;38;2;75;75;75m"), "primary on white is 75: {late:?}");
    // And then it is quiet again; a second answer (another background) redraws once more.
    assert!(draw(&mut t, &screen(&mut a)).len() < 20);
    let fx = a.update(Event::Bg((0, 0, 0)));
    assert!(draw(&mut t, last_frame(&fx)).starts_with("\x1b[?2026h\x1b[2J"));
}

#[test]
fn vis_4_the_visual_underline_is_the_same_with_colors() {
    let mut a = app_with(Some("f.rs"), b"one two\n", (24, 4));
    feed(&mut a, "vl");
    a.update(Event::Bg((0x1e, 0x1e, 0x1e)));
    let out = draw(&mut Term::new(Vec::new(), false), &screen(&mut a));
    let p = palette((0x1e, 0x1e, 0x1e));
    let want = format!("\x1b[0;38;2;{};{};{};4m", p.primary.0, p.primary.1, p.primary.2);
    assert!(out.contains(&want), "{want:?} missing from {out:?}");
    // Without a palette the underline is appended to the fallback parameters, as before.
    let mut a = app_with(Some("f.rs"), b"one two\n", (24, 4));
    feed(&mut a, "vl");
    let out = draw(&mut Term::new(Vec::new(), false), &screen(&mut a));
    assert!(out.contains("\x1b[0;4m"), "{out:?}");
}

/// The real binary in a pty (scripts/pty_bench.py firstframe), the terminal's answer to the query
/// already there when it starts, as it is when the terminal is quick: the first frame is in color,
/// with nothing of the faint stand-in before it.
#[test]
fn vis_4_an_answer_in_time_colors_the_first_frame() {
    if !cfg!(target_os = "linux") {
        return eprintln!("skipped: the pty is only trusted to hold the answer ready on Linux");
    }
    let dir = temp_dir("vis4");
    let file = dir.0.join("f.rs");
    std::fs::write(&file, code_text(200, 60)).unwrap();
    let args = ["firstframe", env!("CARGO_BIN_EXE_si"), file.to_str().unwrap()];
    let Some(m) = pty_bench(&args) else { return };
    assert_eq!((m["colored"], m["faint"]), (1.0, 0.0), "the first frame was not in color");
}

/// The real binary in a pty (scripts/pty_bench.py asks): `si +q` quits before it asks the terminal
/// anything, since the answer would come after it had gone, to be typed into the shell. A start
/// that stays does ask.
#[test]
fn vis_4_a_quit_at_startup_asks_for_no_color() {
    let dir = temp_dir("vis4-asks");
    let file = dir.0.join("f.rs");
    std::fs::write(&file, code_text(20, 40)).unwrap();
    let args = ["asks", env!("CARGO_BIN_EXE_si"), file.to_str().unwrap()];
    let Some(m) = pty_bench(&args) else { return };
    assert_eq!((m["queried_quit"], m["queried_open"]), (0.0, 1.0), "+q asks, or a start does not");
}

/// The real binary in a pty (scripts/pty_bench.py leak), on a terminal that answers 30 ms late, in
/// order, and `:q` at the first frame: si does not leave before the terminal is done answering, so
/// nothing of it is left for the shell. A terminal that never answers costs a quit 100 ms at most.
#[test]
fn vis_4_a_quit_before_the_color_came_leaves_nothing_for_the_shell() {
    let dir = temp_dir("vis4-leak");
    let file = dir.0.join("f.rs");
    std::fs::write(&file, code_text(20, 40)).unwrap();
    let (si, file) = (env!("CARGO_BIN_EXE_si"), file.to_str().unwrap());
    let Some(m) = pty_bench(&["leak", si, file, "30"]) else { return };
    assert_eq!(m["leaked"], 0.0, "the shell would have typed the terminal's answer");
    assert!(
        m["quit_ms"] >= m["owed_ms"] - 5.0,
        "it quit in {:.1} ms, before the answer ({:.1} ms from the `:q`) could be in",
        m["quit_ms"],
        m["owed_ms"]
    );
    for (kind, most) in [("answering", 80.0), ("no_osc", 80.0), ("silent", 400.0)] {
        let m = pty_bench(&["quit", si, file, kind]).unwrap();
        assert!(m["quit_ms"] < most, "a quit with a {kind} terminal took {:.1} ms", m["quit_ms"]);
        let waits = kind == "silent";
        assert_eq!(m["quit_ms"] >= 90.0, waits, "{kind}: {:.1} ms", m["quit_ms"]);
    }
}
