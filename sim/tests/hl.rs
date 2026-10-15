//! SYN-1, SYN-2, SYN-3, SYN-5: colored text through sim's state (the highlighter's answers come
//! back as events), the palette from the terminal's replies, and the painter's bytes.
#![allow(clippy::unwrap_used, clippy::panic)] // helpers; the tests themselves may unwrap
mod common;
use common::*;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use proptest::prelude::*;
use si::core::view::{Frame, Row, Shape, Style};
use sim::color::{self, Palette, lc};
use sim::io::Highlighter;
use sim::paint::{Look, Painter, Screen};
use sim::sim::{Env, Req, SimEvent};

const RUST: &str = "fn main() {\n    let s = \"a\";\n    f(1); // c\n}\n";

fn colored(r: &Rig, look: Look) -> usize {
    r.shot.looks.iter().flatten().filter(|(_, l)| *l == look).count()
}

fn any_color(r: &Rig) -> bool {
    r.shot.looks.iter().flatten().any(|(_, l)| *l != Look::Bold)
}

/// Sim with a palette that has answered: this background and these ANSI colors (red, green,
/// yellow, magenta, cyan).
fn palette(bg: &str) -> Palette {
    let mut p = Palette::new(true);
    p.reply(&format!("11;rgb:{bg}"));
    for (i, c) in
        [(1, "cc/00/00"), (2, "00/aa/00"), (3, "aa/aa/00"), (5, "aa/00/aa"), (6, "00/aa/aa")]
    {
        p.reply(&format!("4;{i};rgb:{c}"));
    }
    p
}

#[test]
fn syn_1_the_text_of_a_file_is_colored_from_its_spans() {
    let r = rig_sized("hl-rust", &[&format!("main.rs={RUST}")], &["main.rs"], (40, 8));
    assert_eq!(colored(&r, Look::Kw), 2, "fn and let");
    assert_eq!((colored(&r, Look::Str), colored(&r, Look::Com), colored(&r, Look::Num)), (1, 1, 1));
    golden("syn_1_sim_rust", &dump_screen(&r.shot));
}

#[test]
fn syn_1_tabs_wide_chars_and_control_chars_do_not_shift_the_colors() {
    let text = "fn\tf() { let s = \"日本\\t語\"; // 文\n\tlet c = 1; \u{1}\u{85}x // y\n";
    let r = rig_sized("hl-wide", &[&format!("main.rs={text}")], &["main.rs"], (50, 6));
    golden("syn_1_sim_wide", &dump_screen(&r.shot));
}

#[test]
fn syn_1_a_wrapped_line_is_colored_on_every_row() {
    let text = "let long = \"aaaa bbbb cccc dddd eeee ffff gggg\"; // hhhh iiii jjjj\n";
    let r = rig_sized("hl-wrap", &[&format!("main.rs={text}")], &["main.rs"], (24, 8));
    golden("syn_1_sim_wrap", &dump_screen(&r.shot));
    assert!(r.shot.looks[1].iter().any(|(_, l)| *l == Look::Str), "the string goes on in row 2");
    assert!(r.shot.looks[3].iter().any(|(_, l)| *l == Look::Com), "and the comment after it");
}

#[test]
fn syn_1_next_to_the_panel_the_colors_are_at_the_columns_of_the_text() {
    let mut r = rig_sized("hl-panel", &[&format!("main.rs={RUST}")], &["main.rs"], (60, 6));
    let before: Vec<_> = r
        .shot
        .looks
        .iter()
        .map(|l| l.iter().map(|(r, k)| (r.start, r.end, *k)).collect::<Vec<_>>())
        .collect();
    r.keys("<C-n>");
    let (pw, _) = r.sim.widths();
    let off = pw + 1;
    for (row, want) in r.shot.looks.iter().zip(&before).take(4) {
        let got: Vec<_> = row
            .iter()
            .filter(|(s, _)| s.start >= off)
            .map(|(s, k)| (s.start - off, s.end - off, *k))
            .collect();
        assert_eq!(&got, want);
    }
}

#[test]
fn syn_1_the_painter_writes_sgr_31_to_36_without_a_palette_and_never_blue() {
    let mut p = Painter::new(Vec::new());
    let frame = Frame {
        width: 30,
        height: 2,
        rows: vec![Row { text: "fn s 1 T f c b".into(), ..Row::default() }, Row::default()],
        cursor: (0, 0),
        shape: Shape::Block,
    };
    let looks = vec![
        vec![
            (0..2, Look::Kw),
            (3..4, Look::Str),
            (5..6, Look::Num),
            (7..8, Look::Ty),
            (9..10, Look::Fun),
            (11..12, Look::Com),
        ],
        vec![],
    ];
    p.draw(Screen { frame, looks, pal: Palette::default() }).unwrap();
    let out = String::from_utf8(p.out).unwrap();
    for sgr in [
        "\x1b[0;1;31mfn",
        "\x1b[0;32ms",
        "\x1b[0;35m1",
        "\x1b[0;33mT",
        "\x1b[0;36mf",
        "\x1b[0;2;3mc",
    ] {
        assert!(out.contains(sgr), "{sgr:?} in {out:?}");
    }
    assert!(!out.contains(";34m") && !out.contains(";44m"), "blue is never used");
}

#[test]
fn syn_1_the_change_background_wins_over_a_color_and_a_color_never_makes_a_background() {
    let draw = |style| {
        let mut p = Painter::new(Vec::new());
        let frame = Frame {
            width: 10,
            height: 2,
            rows: vec![
                Row { text: "fn x".into(), spans: vec![(0..4, style)], ..Row::default() },
                Row::default(),
            ],
            cursor: (0, 0),
            shape: Shape::Block,
        };
        let looks = vec![vec![(0..2, Look::Kw), (0..4, Look::Sel)], vec![]];
        p.draw(Screen { frame, looks, pal: palette("1e/1e/1e") }).unwrap();
        String::from_utf8(p.out).unwrap()
    };
    let on_change = draw(Style::Change);
    assert!(on_change.contains("\x1b[0;7;1mfn"), "reverse video and bold, no color: {on_change:?}");
    assert!(
        !on_change.contains("38;2") && !on_change.contains("48;2") && !on_change.contains(";31")
    );
    let plain = draw(Style::Primary);
    assert!(plain.contains("48;2;"), "the selection's gray is a background (SYN-5): {plain:?}");
}

#[test]
fn syn_1_a_palette_moves_each_color_until_it_reads_at_lc_75_on_every_background() {
    let backgrounds = [
        "00/00/00", "1e/1e/1e", "28/2c/34", "00/2b/36", "ff/ff/ff", "fa/fa/fa", "fd/f6/e3",
        "ef/f1/f5",
    ];
    for bg in backgrounds {
        let pal = palette(bg);
        let rgb = |s: &str| {
            let v: Vec<u8> = s.split('/').map(|h| u8::from_str_radix(h, 16).unwrap()).collect();
            [v[0], v[1], v[2]]
        };
        for look in [Look::Kw, Look::Str, Look::Ty, Look::Num, Look::Fun] {
            let c = pal.hue(look).unwrap();
            let contrast = lc(c, rgb(bg)).abs();
            assert!(contrast >= 75.0, "{look:?} on {bg}: Lc {contrast:.1}");
        }
    }
}

#[test]
fn syn_1_the_colors_keep_their_hue_and_blue_is_not_asked_for() {
    let pal = palette("1e/1e/2e");
    for (look, ansi) in [
        (Look::Kw, [0xcc, 0, 0]),
        (Look::Str, [0, 0xaa, 0]),
        (Look::Num, [0xaa, 0, 0xaa]),
        (Look::Fun, [0, 0xaa, 0xaa]),
    ] {
        let (a, b) = (color::to_lch(ansi), color::to_lch(pal.hue(look).unwrap()));
        let d = (a[2] - b[2]).abs().min(360.0 - (a[2] - b[2]).abs());
        assert!(d < 12.0, "{look:?}: hue {:.0} became {:.0}", a[2], b[2]);
    }
    assert!(!color::QUERY.contains("4;4;"), "ANSI blue is never asked for");
    let mut only_blue = Palette::new(true);
    only_blue.reply("11;rgb:00/00/00");
    only_blue.reply("4;4;rgb:00/00/ff");
    assert!(
        [Look::Kw, Look::Str, Look::Ty, Look::Num, Look::Fun]
            .iter()
            .all(|l| only_blue.hue(*l).is_none())
    );
}

#[test]
fn syn_1_without_truecolor_or_a_reply_there_is_no_palette() {
    let mut p = Palette::new(false);
    p.reply("11;rgb:1e/1e/1e");
    p.reply("4;1;rgb:cc/00/00");
    assert!(
        p.hue(Look::Kw).is_none() && p.sel().is_none(),
        "no COLORTERM: the terminal's own colors"
    );
    let mut p = Palette::new(true);
    p.reply("4;1;rgb:cc/00/00");
    assert!(p.hue(Look::Kw).is_none(), "no background: no way to tell the contrast");
    for junk in [
        "",
        "11",
        "11;",
        "11;rgb:zz/00/00",
        "11;rgb:1/2",
        "4;99;rgb:00/00/00",
        "52;c;AAAA",
        "11;rgb:1/2/3/4",
    ] {
        p.reply(junk); // nothing panics
    }
    p.reply("11;rgb:ffff/ffff/ffff");
    p.reply("4;1;rgb:cc00/0000/0000");
    assert!(p.hue(Look::Kw).is_some(), "four hex digits a channel, as terminals send");
}

fn key(code: KeyCode, mods: KeyModifiers) -> KeyEvent {
    KeyEvent::new(code, mods)
}

#[test]
fn syn_1_an_osc_reply_is_put_together_from_keys_and_never_reaches_the_app() {
    let (eaten, alt) = (Some(None), KeyModifiers::ALT);
    let mut buf = None;
    let plain = |c: char| key(KeyCode::Char(c), KeyModifiers::NONE);
    // ESC ] 11 ; rgb:1E1E/1e1e/1E1E ESC \  (crossterm: Alt+], the payload, Alt+\)
    assert_eq!(color::feed(&mut buf, &key(KeyCode::Char(']'), alt)), eaten);
    for c in "11;rgb:1E1E/1e1e/1E1E".chars() {
        let mods = if c.is_uppercase() { KeyModifiers::SHIFT } else { KeyModifiers::NONE };
        assert_eq!(color::feed(&mut buf, &key(KeyCode::Char(c), mods)), eaten, "{c}");
    }
    let done = color::feed(&mut buf, &key(KeyCode::Char('\\'), alt));
    assert_eq!(done, Some(Some("11;rgb:1E1E/1e1e/1E1E".into())));
    // BEL ends a reply too (Ctrl-g).
    color::feed(&mut buf, &key(KeyCode::Char(']'), alt));
    color::feed(&mut buf, &plain('1'));
    let bel = color::feed(&mut buf, &key(KeyCode::Char('g'), KeyModifiers::CONTROL));
    assert_eq!(bel, Some(Some("1".into())));
    // Ordinary keys are ordinary keys; a reply that turns out to be keys gives the key back.
    assert_eq!(color::feed(&mut buf, &plain('x')), None);
    assert_eq!(color::feed(&mut buf, &key(KeyCode::Char('x'), alt)), None);
    color::feed(&mut buf, &key(KeyCode::Char(']'), alt));
    assert_eq!(color::feed(&mut buf, &key(KeyCode::Enter, KeyModifiers::NONE)), None);
    assert_eq!(color::feed(&mut buf, &plain('y')), None, "and then nothing is being collected");
    // A runaway reply is cut off, not collected for ever.
    color::feed(&mut buf, &key(KeyCode::Char(']'), alt));
    let n = (0..300).filter(|_| color::feed(&mut buf, &plain('a')) == eaten).count();
    assert!(n < 300);
}

#[test]
fn syn_1_a_reply_from_the_terminal_changes_the_palette_of_the_next_screen() {
    let mut r =
        rig_env("hl-osc", &[&format!("main.rs={RUST}")], &["main.rs"], (40, 8), |e: &mut Env| {
            e.pal = Palette::new(true);
        });
    assert!(r.shot.pal.hue(Look::Kw).is_none());
    r.send(SimEvent::Osc("11;rgb:1e1e/1e1e/1e1e".into()));
    r.send(SimEvent::Osc("4;1;rgb:cc/00/00".into()));
    assert!(r.shot.pal.hue(Look::Kw).is_some());
    let mut p = Painter::new(Vec::new());
    p.draw(r.shot.clone()).unwrap();
    assert!(String::from_utf8(p.out).unwrap().contains("38;2;"), "drawn with the adjusted red");
    r.send(SimEvent::Osc("52;c;junk".into()));
    r.keys("lll"); // an unknown reply and keys go on as usual
    assert_eq!(r.status(), "main.rs  1:4  NORMAL");
}

// ---- SYN-2 ----

#[test]
fn syn_2_it_is_on_by_default_and_zh_turns_it_off_and_on() {
    let mut r = rig_sized("hl-zh", &[&format!("main.rs={RUST}")], &["main.rs"], (40, 8));
    assert!(any_color(&r));
    r.keys("zh");
    assert!(!any_color(&r));
    assert_eq!(r.status(), "syntax colors off");
    r.keys("jzh");
    assert!(any_color(&r));
    assert_eq!(r.status(), "syntax colors on");
}

#[test]
fn syn_2_only_zh_toggles_and_only_in_normal_mode() {
    let mut r = rig_sized("hl-zh2", &[&format!("main.rs={RUST}")], &["main.rs"], (40, 8));
    r.keys("zzzl");
    assert!(any_color(&r), "zz is the App's (center the line)");
    r.keys("3zh");
    assert!(any_color(&r), "a count in between: not ours");
    r.keys("izh<Esc>");
    assert!(any_color(&r), "typed text");
    assert!(r.text().contains("zh"));
    r.keys("z");
    r.keys("<Esc>h");
    assert!(any_color(&r));
    r.keys("zxh");
    assert!(any_color(&r));
    r.keys("zh");
    assert!(!any_color(&r));
    // It is for the session: another file is not colored either.
    r.keys(":e other.rs\n");
    assert!(!any_color(&r));
}

// ---- SYN-3 ----

#[test]
fn syn_3_lines_show_plain_until_their_spans_arrive_and_the_request_is_made_once() {
    // No file argument: the panel is open; Enter opens the file with the spans held back.
    let mut r = rig_sized("hl-late", &[&format!("main.rs={RUST}")], &[], (40, 8));
    r.hold_spans = true;
    r.keys("<CR>");
    assert!(!any_color(&r), "plain");
    assert_eq!(r.held.len(), 1);
    r.keys("jjj");
    assert_eq!(r.held.len(), 1, "keys that stay in the window ask for nothing more");
    r.deliver_spans();
    assert!(any_color(&r), "colored once the spans have come");
    r.keys("jjj");
    assert!(r.held.is_empty());
}

#[test]
fn syn_3_an_edit_keeps_the_colors_of_the_other_lines_while_the_new_spans_are_on_their_way() {
    let mut r = rig_sized("hl-edit", &[&format!("main.rs={RUST}")], &["main.rs"], (40, 8));
    let before = r.shot.looks.clone();
    r.hold_spans = true;
    r.keys("jjA x<Esc>"); // an edit on line 3: the number of lines is the same
    assert_eq!(r.shot.looks[0], before[0], "line 1 kept its colors");
    assert_eq!(r.shot.looks[1], before[1]);
    assert!(!r.held.is_empty(), "the new spans were asked for");
    r.deliver_spans();
    assert!(any_color(&r));
    // A new line moves the lines below it, and their colors with them, until the answer comes.
    let before = r.shot.looks.clone();
    r.hold_spans = true;
    r.keys("ggo<Esc>");
    assert_eq!(r.shot.looks[0], before[0], "above the new line");
    assert_eq!(r.shot.looks[2..4], before[1..3], "below it, one row on");
    r.deliver_spans();
    assert!(any_color(&r));
}

#[test]
fn syn_3_a_window_is_asked_a_screenful_above_and_below_and_again_when_the_view_leaves_it() {
    let text: String = (0..300).map(|i| format!("let v{i} = {i};\n")).collect();
    let mut r = rig_sized("hl-win", &[&format!("main.rs={text}")], &["main.rs"], (40, 10));
    let asked = |r: &Rig| r.jobs.iter().filter(|j| **j == "highlight").count();
    assert_eq!(asked(&r), 1);
    r.keys("jjjjjjjj");
    assert_eq!(asked(&r), 1, "the window held the view");
    r.keys("200G");
    assert_eq!(asked(&r), 2, "a jump out of it");
    r.keys("kkk");
    assert_eq!(asked(&r), 2);
    assert!(any_color(&r));
}

#[test]
fn syn_3_a_file_over_10_mb_is_not_highlighted() {
    let line = "let x = 1; // 123456789012345678901234567890\n";
    let big: String = line.repeat(10_000_000 / line.len() + 2);
    assert!(big.len() > 10_000_000);
    let r = rig_sized("hl-big", &[&format!("big.rs={big}")], &["big.rs"], (60, 8));
    assert_eq!(r.jobs.iter().filter(|j| **j == "highlight").count(), 0);
    assert!(!any_color(&r));
    let small =
        rig_sized("hl-small", &[&format!("small.rs={}", line.repeat(100))], &["small.rs"], (60, 8));
    assert!(any_color(&small));
}

#[test]
fn syn_3_a_file_with_no_known_language_is_answered_with_empty_spans_once() {
    let r = rig_sized("hl-txt", &["notes.txt=plain text\nmore\n"], &["notes.txt"], (40, 8));
    assert_eq!(r.jobs.iter().filter(|j| **j == "highlight").count(), 1);
    assert!(!any_color(&r));
}

fn req(id: usize, name: &str, ver: u64, text: &str, from: usize, n: usize) -> Req {
    Req { id, name: name.into(), ver, text: ropey::Rope::from_str(text), from, n }
}

fn spans_of(ev: SimEvent) -> Vec<Vec<(std::ops::Range<usize>, Look)>> {
    match ev {
        SimEvent::Spans(h) => h.spans,
        e => panic!("{e:?}"),
    }
}

#[test]
fn syn_3_the_highlighter_finds_the_edit_by_comparing_the_texts() {
    let lines: Vec<String> = (0..200).map(|i| format!("let a{i} = 1;")).collect();
    let text = lines.join("\n") + "\n";
    let mut h = Highlighter::default();
    let first = spans_of(h.run(req(1, "a.rs", 1, &text, 150, 20)));
    assert_eq!(first.len(), 20);
    // Opening a comment at line 40 swallows the lines below it.
    let mut edited = lines.clone();
    edited[40] = "/* open".into();
    let edited = edited.join("\n") + "\n";
    let after = spans_of(h.run(req(1, "a.rs", 2, &edited, 150, 20)));
    let fresh = spans_of(Highlighter::default().run(req(1, "a.rs", 2, &edited, 150, 20)));
    assert_eq!(after, fresh, "same as a parse from the top");
    assert!(after.iter().all(|l| l.len() == 1 && l[0].1 == Look::Com));
    // Closing it again.
    let back = spans_of(h.run(req(1, "a.rs", 3, &text, 150, 20)));
    assert_eq!(back, first);
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(48))]

    #[test]
    fn syn_3_after_any_edit_the_highlighter_agrees_with_a_fresh_parse(
        base in proptest::collection::vec("[ a-z\"'/*#{};=]{0,24}", 1..80),
        at in 0usize..80,
        with in "[ a-z\"'/*#{};=\\n]{0,12}",
        cut in 0usize..30,
    ) {
        let text = base.join("\n");
        let mut h = Highlighter::default();
        let n = base.len() + 2;
        h.run(req(1, "a.rs", 1, &text, 0, n));
        // Change a stretch of the text: it may shorten it, lengthen it, or add and remove lines.
        let chars: Vec<char> = text.chars().collect();
        let (a, b) = (at.min(chars.len()), (at + cut).min(chars.len()));
        let edited: String = chars[..a].iter().chain(with.chars().collect::<Vec<_>>().iter()).chain(&chars[b..]).collect();
        let got = spans_of(h.run(req(1, "a.rs", 2, &edited, 0, n + 12)));
        let want = spans_of(Highlighter::default().run(req(1, "a.rs", 2, &edited, 0, n + 12)));
        prop_assert_eq!(got, want);
    }
}

#[test]
fn syn_3_the_highlighter_thread_answers_the_newest_request_of_each_file() {
    let (tx, rx) = std::sync::mpsc::channel();
    let jobs = sim::io::highlight_thread(tx);
    for v in 1..=5 {
        jobs.send(req(7, "a.rs", v, "fn f() {}\n", 0, 1)).unwrap();
    }
    jobs.send(req(8, "b.rs", 1, "let x = 1;\n", 0, 1)).unwrap();
    let mut seen = std::collections::HashMap::new();
    while seen.len() < 2 {
        match rx.recv_timeout(std::time::Duration::from_secs(5)).unwrap() {
            SimEvent::Spans(h) => {
                seen.insert(h.id, h.ver);
            }
            e => panic!("{e:?}"),
        }
    }
    assert_eq!(seen[&8], 1);
    assert!(seen[&7] >= 1);
    // The last request of file 7 is always answered.
    let mut last = seen[&7];
    while last < 5 {
        match rx.recv_timeout(std::time::Duration::from_secs(5)).unwrap() {
            SimEvent::Spans(h) if h.id == 7 => last = h.ver,
            e => panic!("{e:?}"),
        }
    }
}

// ---- SYN-5 ----

#[test]
fn syn_5_the_selection_is_a_gray_background_with_truecolor_and_an_underline_without() {
    let content = format!("main.rs={RUST}");
    let gray = |r: &Rig| r.shot.looks.iter().flatten().filter(|(_, l)| *l == Look::Sel).count();
    let underlined = |r: &Rig| r.screen.rows.iter().filter(|row| !row.underline.is_empty()).count();
    // No palette (no COLORTERM, or no reply yet): si's underline.
    let mut r = rig_sized("sel-none", &[&content], &["main.rs"], (40, 8));
    r.keys("vlll");
    assert_eq!((gray(&r), underlined(&r)), (0, 1));
    // A palette: the gray; and the underline is gone.
    let mut r = rig_env("sel-gray", &[&content], &["main.rs"], (40, 8), |e: &mut Env| {
        e.pal = palette("1e/1e/1e")
    });
    r.keys("vlll");
    assert_eq!((gray(&r), underlined(&r)), (1, 0));
    golden("syn_5_selection", &dump_screen(&r.shot));
    // Not truecolor: the underline, whatever the terminal answered.
    let mut r = rig_env("sel-16", &[&content], &["main.rs"], (40, 8), |e: &mut Env| {
        e.pal = Palette::new(false);
        e.pal.reply("11;rgb:1e/1e/1e");
    });
    r.keys("vlll");
    assert_eq!((gray(&r), underlined(&r)), (0, 1));
}

#[test]
fn syn_5_the_gray_is_neutral_reads_at_lc_15_and_its_text_at_lc_75_on_every_background() {
    let backgrounds = [
        "00/00/00", "1e/1e/1e", "28/2c/34", "00/2b/36", "ff/ff/ff", "fa/fa/fa", "fd/f6/e3",
        "ef/f1/f5",
    ];
    for bg in backgrounds {
        let pal = palette(bg);
        let (shade, text) = pal.sel().unwrap();
        let rgb: Vec<u8> = bg.split('/').map(|h| u8::from_str_radix(h, 16).unwrap()).collect();
        let bg = [rgb[0], rgb[1], rgb[2]];
        assert_eq!(shade[0], shade[1], "neutral");
        assert_eq!(shade[1], shade[2], "neutral");
        assert!(lc(shade, bg).abs() >= 15.0, "{bg:?}: the gray reads at {:.1}", lc(shade, bg));
        assert!(lc(text, shade).abs() >= 75.0, "{bg:?}: its text reads at {:.1}", lc(text, shade));
        assert!(lc(shade, bg).abs() < 30.0, "{bg:?}: a hint, not a block: {:.1}", lc(shade, bg));
    }
}
