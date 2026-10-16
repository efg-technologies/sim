//! SYN-1, SYN-2, SYN-3, SYN-5: colored text through sim's state (the highlighter's answers come
//! back as events), the colors from the terminal's replies, si's palette under them, and the
//! painter's bytes.
#![allow(clippy::unwrap_used, clippy::panic)] // helpers; the tests themselves may unwrap
mod common;
use common::*;
use crossterm::event::{Event as Ev, KeyCode, KeyEvent, KeyModifiers};
use proptest::prelude::*;
use si::core::app::{Event, Key};
use si::core::color::{apca, palette as si_palette, parse_osc11};
use si::core::view::{Frame, Row, Shape, Style};
use sim::color::{self, Input, Syntax};
use sim::io::{Highlighter, HlJob};
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
fn palette(bg: &str) -> Syntax {
    let mut p = Syntax::new(true);
    p.bg(parse_osc11(&format!("rgb:{bg}")).unwrap());
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
fn syn_1_emoji_sequences_and_every_bidirectional_control_keep_the_colors_in_place() {
    // U+061C and U+202E are written as `<U+XXXX>`, a U+FE0F after such a token or after a control
    // char is not written, and an emoji presentation sequence or a ZWJ sequence takes the columns
    // si's `width` gives it (VIS-7, SAFE-8): the string must start and end where the text says.
    let s = "\u{61c}\u{202e}\u{2764}\u{fe0f}\u{1}\u{fe0f}\u{a9}\u{fe0f}\u{1f468}\u{200d}\u{1f469}\t\u{85}";
    let text = format!("let s = \"{s}\"; let n = 1;\n");
    let r = rig_sized("hl-emoji", &[&format!("main.rs={text}")], &["main.rs"], (70, 4));
    let row: Vec<char> = r.screen.rows[0].text.chars().collect();
    let col = |at: usize| (0..at).fold(0, |col, i| col + si::core::view::width(&row, i, col));
    let quotes: Vec<usize> = (0..row.len()).filter(|&i| row[i] == '"').collect();
    let strings: Vec<_> = r.shot.looks[0].iter().filter(|l| l.1 == Look::Str).collect();
    let want = col(quotes[0])..col(quotes[1]) + 1;
    assert_eq!(strings.len(), 1, "{:?}", r.shot.looks[0]);
    assert_eq!(
        (usize::from(strings[0].0.start), usize::from(strings[0].0.end)),
        (want.start, want.end)
    );
    let one = (0..row.len()).rfind(|&i| row[i] == '1').unwrap();
    let num: Vec<_> = r.shot.looks[0].iter().filter(|l| l.1 == Look::Num).collect();
    assert_eq!(num.len(), 1);
    assert_eq!(usize::from(num[0].0.start), col(one), "the number after it");
}

#[test]
fn syn_3_a_line_of_many_spans_is_laid_out_in_the_time_of_a_key() {
    // 200,000 spans (every other char of a line of 400,000), 23 rows of 75 chars on the screen.
    // Finding the span of a char by scanning the line's spans made every plain char cost all of
    // them: 900 plain chars on screen times 200,000 spans, at every key.
    let (n, digits) = (400_000, 3);
    let text: String = "ab".repeat(n / 2);
    let spans: sim::syn::Spans = (0..n / 2).map(|i| (2 * i..2 * i + 1, Look::Num)).collect();
    let chars: Vec<char> = text.chars().collect();
    let rows = (0..23)
        .map(|i| {
            let gutter = if i == 0 { "   1 " } else { "     " };
            let body: String = chars[i * 75..(i + 1) * 75].iter().collect();
            Row { text: format!("{gutter}{body}"), ..Row::default() }
        })
        .collect();
    let frame =
        Frame { width: 80, height: 24, rows, cursor: (5, 0), shape: Shape::Block, pal: None };
    let (t, line) = (std::time::Instant::now(), text.clone());
    let (looks, win) =
        sim::layout::colors(&frame, digits, &|l| (l == 0).then(|| line.clone()), &|l| {
            (l == 0).then_some(&spans)
        });
    let took = t.elapsed();
    assert_eq!(win, Some((0, 0)));
    assert_eq!(looks[0].len(), 38, "every other char of the first row is a number");
    assert!(took < std::time::Duration::from_millis(150), "{took:?}");
}

#[test]
fn syn_1_review_weaves_mine_and_theirs_into_its_rows_so_it_has_no_syntax_colors() {
    let mut r =
        rig_sized("hl-review", &["main.rs=let a = 1;\nfn main() {}\n"], &["main.rs"], (50, 8));
    assert!(colored(&r, Look::Kw) == 2, "colored before: {:?}", r.shot.looks);
    r.keys("jx"); // mine: `n main() {}`
    let id = r.sim.bufs[0].id;
    r.send(SimEvent::App(id, disk(b"let a = 1;\nfn main() { 1 }\n")));
    assert_eq!(r.app().buf.hunks.len(), 1, "a conflict");
    assert!(colored(&r, Look::Kw) >= 1, "outside REVIEW the text is the buffer's: still colored");
    r.keys("]c");
    assert_eq!(r.app().vi.mode, si::core::vi::Mode::Review);
    assert!(!any_color(&r), "no color in REVIEW: {:?}", r.shot.looks);
    assert!(r.screen.rows.iter().any(|row| row.spans.iter().any(|s| s.1 == Style::Change)));
    r.keys("<Esc>");
    assert!(colored(&r, Look::Kw) >= 1, "and back when it ends");
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
        pal: None,
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
    p.draw(Screen { frame, looks, paints: vec![], syn: Syntax::default() }).unwrap();
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
fn syn_1_the_painter_counts_columns_by_si_s_rule_so_spans_stay_on_their_chars() {
    // `❤` + U+FE0F is an emoji presentation sequence: two columns (VIS-7), where unicode-width
    // says one. The change style after it, and the color after a ZWJ sequence, start where si's
    // own terminal output starts them.
    let text = "a\u{2764}\u{fe0f}b \u{1f468}\u{200d}\u{1f469}c";
    let cs: Vec<char> = text.chars().collect();
    let col = |at: usize| (0..at).fold(0, |col, i| col + si::core::view::width(&cs, i, col)) as u16;
    let (b, c) = (col(cs.iter().position(|&x| x == 'b').unwrap()), col(cs.len() - 1));
    assert_eq!(b, 3, "a is 1, the sequence 2");
    let row = Row { text: text.into(), spans: vec![(b..b + 1, Style::Change)], ..Row::default() };
    let frame = Frame {
        width: 20,
        height: 1,
        rows: vec![row],
        cursor: (0, 0),
        shape: Shape::Block,
        pal: None,
    };
    let looks = vec![vec![(c..c + 1, Look::Kw)]];
    let mut p = Painter::new(Vec::new());
    p.draw(Screen { frame, looks, paints: vec![], syn: Syntax::default() }).unwrap();
    let out = String::from_utf8(p.out).unwrap();
    // A zero-width char (the selector, the joiner) goes with the column it stands on, as in si's Term.
    assert!(out.contains("\x1b[0;7m\u{fe0f}b\x1b[0m"), "the change style covers b only: {out:?}");
    assert!(
        out.contains("\x1b[0;1;31m\u{200d}\u{1f469}c"),
        "the color starts at c's column: {out:?}"
    );
    // The panel counts the same way, so its rows are as wide as it says.
    assert_eq!(sim::layout::fit("a\u{2764}\u{fe0f}b", 10), ("a\u{2764}\u{fe0f}b".into(), 4));
    assert_eq!(
        sim::layout::fit("a\u{2764}\u{fe0f}b", 2),
        ("a".into(), 1),
        "a wide char is not cut in two"
    );
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
            pal: None,
        };
        let looks = vec![vec![(0..2, Look::Kw), (0..4, Look::Sel)], vec![]];
        p.draw(Screen { frame, looks, paints: vec![], syn: palette("1e/1e/1e") }).unwrap();
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
            (v[0], v[1], v[2])
        };
        for look in [Look::Kw, Look::Str, Look::Ty, Look::Num, Look::Fun] {
            let c = pal.hue(look).unwrap();
            let contrast = apca(c, rgb(bg)).abs();
            assert!(contrast >= 75.0, "{look:?} on {bg}: Lc {contrast:.1}");
        }
    }
}

#[test]
fn syn_1_the_colors_keep_their_hue_and_blue_is_not_asked_for() {
    let pal = palette("1e/1e/2e");
    for (look, ansi) in [
        (Look::Kw, (0xcc, 0, 0)),
        (Look::Str, (0, 0xaa, 0)),
        (Look::Num, (0xaa, 0, 0xaa)),
        (Look::Fun, (0, 0xaa, 0xaa)),
    ] {
        let (a, b) = (color::to_lch(ansi), color::to_lch(pal.hue(look).unwrap()));
        let d = (a[2] - b[2]).abs().min(360.0 - (a[2] - b[2]).abs());
        assert!(d < 12.0, "{look:?}: hue {:.0} became {:.0}", a[2], b[2]);
    }
    assert!(!color::QUERY.contains("4;4;"), "ANSI blue is never asked for");
    // Blue is never used: a reply for it changes nothing, and the colors that stand in for the
    // ones that did not come are not blue either.
    let mut only_blue = Syntax::new(true);
    only_blue.bg((0, 0, 0));
    let all = [Look::Kw, Look::Str, Look::Ty, Look::Num, Look::Fun];
    let before = all.map(|l| only_blue.hue(l));
    only_blue.reply("4;4;rgb:00/00/ff");
    assert_eq!(all.map(|l| only_blue.hue(l)), before);
    for l in all {
        let hue = color::to_lch(only_blue.hue(l).unwrap())[2];
        assert!(!(220.0..280.0).contains(&hue), "{l:?}: hue {hue:.0} is blue");
    }
}

/// VIS-4's backgrounds.
const BGS: [(u8, u8, u8); 8] = [
    (0x00, 0x00, 0x00),
    (0x1e, 0x1e, 0x1e),
    (0x28, 0x2c, 0x34),
    (0x00, 0x2b, 0x36),
    (0xff, 0xff, 0xff),
    (0xfa, 0xfa, 0xfa),
    (0xfd, 0xf6, 0xe3),
    (0xef, 0xf1, 0xf5),
];

#[test]
fn syn_1_where_osc_4_is_silent_but_the_background_is_known_each_class_has_a_truecolor_of_its_hue() {
    // (inside tmux the replies often never come): red, green, yellow, magenta, cyan by their hues.
    let classes = [
        (Look::Kw, 25.0),
        (Look::Str, 145.0),
        (Look::Ty, 85.0),
        (Look::Num, 320.0),
        (Look::Fun, 195.0),
    ];
    for bg in BGS {
        let mut syn = Syntax::new(true);
        syn.bg(bg);
        for (look, hue) in classes {
            let c = syn.hue(look);
            assert!(c.is_some(), "{look:?} on {bg:?}: derived, not SGR 3x");
            let (c, lch) = (c.unwrap(), color::to_lch(c.unwrap()));
            let lc = apca(c, bg).abs();
            assert!(lc >= 75.0, "{look:?} on {bg:?}: {c:?} |Lc| {lc:.1}");
            let d = (lch[2] - hue).abs().min(360.0 - (lch[2] - hue).abs());
            assert!(d < 6.0, "{look:?} on {bg:?}: hue {:.0}, not {hue}", lch[2]);
            assert!(lch[1] > 0.03, "{look:?} on {bg:?}: chroma {:.3} is no color", lch[1]);
        }
        assert!(syn.pending(), "the replies may still come: the quit still waits for them");
    }
    // The background is not known: the terminal's own SGR 31-36 stand in.
    let mut none = Syntax::new(true);
    assert!(classes.iter().all(|(l, _)| none.hue(*l).is_none()));
    none.reply("4;1;rgb:cc/00/00");
    assert!(none.hue(Look::Kw).is_none());
}

#[test]
fn syn_1_a_late_osc_4_reply_replaces_the_derived_color_and_the_screen_is_redrawn() {
    let bg = (0x1e, 0x1e, 0x1e);
    let mut syn = Syntax::new(true);
    syn.bg(bg);
    let derived = syn.hue(Look::Kw).unwrap();
    let before = syn;
    syn.reply("4;1;rgb:ff/40/40");
    let real = syn.hue(Look::Kw).unwrap();
    assert_ne!(real, derived, "the terminal's own red");
    assert!(apca(real, bg).abs() >= 75.0);
    assert_ne!(syn, before, "the painter compares it and draws everything again");
    // The other four are still derived, and the quit waits until all five have come.
    assert!(syn.pending());
    for (i, c) in
        [(2, "rgb:40/ff/40"), (3, "rgb:ff/ff/40"), (5, "rgb:ff/40/ff"), (6, "rgb:40/ff/ff")]
    {
        syn.reply(&format!("4;{i};{c}"));
    }
    assert!(!syn.pending());
    // What is painted: truecolor for a keyword with no reply (not `;31`), and the real one later.
    let mut late = Syntax::new(true);
    late.bg(bg);
    let drawn = |syn: Syntax| {
        let rows = vec![Row { text: "fn".into(), ..Row::default() }, Row::default()];
        let frame = Frame {
            width: 4,
            height: 2,
            rows,
            cursor: (0, 0),
            shape: Shape::Block,
            pal: Some(si_palette(bg)),
        };
        let looks = vec![vec![(0..2, Look::Kw)], vec![]];
        let mut p = Painter::new(Vec::<u8>::new());
        p.draw(Screen { frame, looks, paints: vec![], syn }).unwrap();
        String::from_utf8(p.out).unwrap()
    };
    let out = drawn(late);
    let (r, g, b) = derived;
    assert!(out.contains(&format!(";38;2;{r};{g};{b}")) && !out.contains(";31"), "{out:?}");
}

#[test]
fn syn_1_without_truecolor_or_a_reply_there_is_no_palette() {
    let mut p = Syntax::new(false);
    p.bg((0x1e, 0x1e, 0x1e));
    p.reply("4;1;rgb:cc/00/00");
    assert!(
        p.hue(Look::Kw).is_none() && p.sel().is_none(),
        "no COLORTERM: the terminal's own colors"
    );
    let mut p = Syntax::new(true);
    p.reply("4;1;rgb:cc/00/00");
    assert!(p.hue(Look::Kw).is_none(), "no background: no way to tell the contrast");
    for junk in [
        "",
        "4",
        "4;",
        "4;1;rgb:zz/00/00",
        "4;1;rgb:1/2",
        "4;99;rgb:00/00/00",
        "52;c;AAAA",
        "4;1;rgb:1/2/3/4",
        "11;rgb:00/00/00",
    ] {
        p.reply(junk); // nothing panics
    }
    p.bg((255, 255, 255));
    p.reply("4;1;rgb:cc00/0000/0000");
    assert!(p.hue(Look::Kw).is_some(), "four hex digits a channel, as terminals send");
}

fn key(code: KeyCode, mods: KeyModifiers) -> Ev {
    Ev::Key(KeyEvent::new(code, mods))
}

/// What `Input` made of these events, as text: `<r,g,b>` the background, `osc:..` a reply to OSC 4,
/// a key as itself (`^` Esc), `tab` Shift-Tab, `mouse` the mouse.
fn heard(input: &mut Input, evs: Vec<Ev>) -> String {
    let one = |e: SimEvent| match e {
        SimEvent::Term(Event::Bg((r, g, b))) => format!("<{r},{g},{b}>"),
        SimEvent::Term(Event::Key(Key::Esc)) => "^".into(),
        SimEvent::Term(Event::Key(Key::Char(c))) => c.to_string(),
        SimEvent::Term(e) => format!("{e:?}"),
        SimEvent::Osc(s) => format!("osc:{s}"),
        SimEvent::BackTab => "tab".into(),
        SimEvent::Mouse(_) => "mouse".into(),
        e => panic!("{e:?}"),
    };
    evs.into_iter().flat_map(|e| input.feed(e)).map(one).collect()
}

/// `ESC ] body ESC \` as crossterm reads it: Alt+`]`, the body as plain keys, Alt+`\`.
fn osc(body: &str) -> Vec<Ev> {
    let (alt, none) = (KeyModifiers::ALT, KeyModifiers::NONE);
    let keys = body.chars().map(|c| key(KeyCode::Char(c), none));
    [key(KeyCode::Char(']'), alt)]
        .into_iter()
        .chain(keys)
        .chain([key(KeyCode::Char('\\'), alt)])
        .collect()
}

#[test]
fn syn_1_a_reply_to_osc_4_is_put_together_from_keys_and_the_one_to_osc_11_is_si_s() {
    let mut input = Input::new(true);
    // The background is si's `Reply`: it becomes `Event::Bg` and none of its keys goes on.
    assert_eq!(heard(&mut input, osc("11;rgb:1E1E/1e1e/1E1E")), "<30,30,30>");
    // An ANSI color is sim's: the payload as it came, and none of its keys goes on.
    assert_eq!(heard(&mut input, osc("4;1;rgb:cc/00/00")), "osc:4;1;rgb:cc/00/00");
    // BEL ends a reply too (Ctrl-g).
    let evs = vec![
        key(KeyCode::Char(']'), KeyModifiers::ALT),
        key(KeyCode::Char('4'), KeyModifiers::NONE),
        key(KeyCode::Char('g'), KeyModifiers::CONTROL),
    ];
    assert_eq!(heard(&mut input, evs), "osc:4");
    // Replies arriving in pieces, between typed keys.
    let (mut first, rest) = (osc("4;2;rgb:00/aa/00"), osc("11;rgb:00/00/00"));
    let tail = first.split_off(6);
    assert_eq!(heard(&mut input, first), "");
    assert_eq!(heard(&mut input, tail), "osc:4;2;rgb:00/aa/00");
    assert_eq!(heard(&mut input, rest), "<0,0,0>");
}

#[test]
fn syn_1_a_reply_that_comes_late_is_still_a_reply_and_is_never_typed_into_the_text() {
    // Over a slow link the terminal's answer can take seconds. Parsed as keys it would be typed
    // into the editor: `4;1;rgb:cc/00/00` is a count, `r` and `c` commands, and text.
    let mut input = Input::new(true);
    std::thread::sleep(std::time::Duration::from_millis(2100));
    assert_eq!(heard(&mut input, osc("4;1;rgb:cc/00/00")), "osc:4;1;rgb:cc/00/00");
    // Once the five colors that were asked for are in, an Alt+`]` is not held for a sixth.
    for i in [2, 3, 5, 6] {
        assert_eq!(
            heard(&mut input, osc(&format!("4;{i};rgb:cc/00/00"))),
            format!("osc:4;{i};rgb:cc/00/00")
        );
    }
    let (alt, none) = (KeyModifiers::ALT, KeyModifiers::NONE);
    let evs = vec![key(KeyCode::Char(']'), alt), key(KeyCode::Char('4'), none)];
    assert_eq!(heard(&mut input, evs), "^]4");
}

#[test]
fn syn_1_keys_that_are_not_a_reply_go_on_as_keys_in_order() {
    let (alt, none) = (KeyModifiers::ALT, KeyModifiers::NONE);
    let mut input = Input::new(true);
    let plain = |c: char| key(KeyCode::Char(c), none);
    assert_eq!(heard(&mut input, vec![plain('x'), key(KeyCode::Char('x'), alt)]), "x^x");
    // A typed Esc and `]`, then something that is no reply: not lost, whichever reply it is not.
    assert_eq!(heard(&mut input, vec![key(KeyCode::Char(']'), alt), plain('x')]), "^]x");
    assert_eq!(heard(&mut input, vec![key(KeyCode::Char(']'), alt), plain('4'), plain('x')]), "");
    // Shift-Tab and the mouse are sim's, wherever they come.
    let click = Ev::Mouse(crossterm::event::MouseEvent {
        kind: crossterm::event::MouseEventKind::Down(crossterm::event::MouseButton::Left),
        column: 1,
        row: 2,
        modifiers: none,
    });
    assert_eq!(
        heard(&mut input, vec![key(KeyCode::BackTab, KeyModifiers::SHIFT), click]),
        "tabmouse"
    );
    // A reply to OSC 4 that never ends is cut off, not collected for ever; its keys go on.
    let mut runaway = vec![key(KeyCode::Char(']'), alt), plain('4')];
    runaway.extend((0..300).map(|_| plain('a')));
    assert!(heard(&mut input, runaway).len() > 100);
    // With no COLORTERM nothing is asked with OSC 4, so an Alt+`]` is not held for it.
    let mut quiet = Input::new(false);
    assert_eq!(heard(&mut quiet, vec![key(KeyCode::Char(']'), alt), plain('4')]), "^]4");
}

#[test]
fn syn_1_si_s_palette_paints_the_four_styles_and_the_syntax_colors_stay_on_top() {
    let mut r = rig_env(
        "hl-base",
        &[&format!("main.rs={RUST}"), "b.rs=fn b() {}\n"],
        &["main.rs"],
        (40, 8),
        |e: &mut Env| e.syn = Syntax::new(true),
    );
    assert_eq!(r.shot.frame.pal, None, "no answer, no palette: reverse and faint stand in");
    let bg = (0x1e, 0x1e, 0x1e);
    r.send(SimEvent::Term(Event::Bg(bg)));
    let pal = si_palette(bg);
    assert_eq!(r.shot.frame.pal, Some(pal), "the Frame carries si's palette");
    assert_eq!(r.app().pal, Some(pal), "and the App has been told");
    r.send(SimEvent::Osc("4;1;rgb:cc/00/00".into()));
    // Another file, opened later, has it too.
    r.keys(":e b.rs\n");
    assert_eq!(r.app().pal, Some(pal));
    r.keys(":e main.rs\n");
    let mut p = Painter::new(Vec::new());
    p.draw(r.shot.clone()).unwrap();
    let out = String::from_utf8(p.out).unwrap();
    let (g, c) = (pal.primary, pal.secondary);
    assert!(out.contains(&format!("\x1b[0;38;2;{};{};{}m", g.0, g.1, g.2)), "primary: {out:?}");
    assert!(
        out.contains(&format!(";38;2;{};{};{};3m", c.0, c.1, c.2))
            || out.contains(&format!(";3;38;2;{};{};{}m", c.0, c.1, c.2)),
        "comments are the secondary gray, in italic: {out:?}"
    );
    assert!(!out.contains("\x1b[0;2;3m"), "not faint, now that there is a gray");
    assert!(out.contains(";1;38;2;"), "a keyword is bold and in the adjusted red: {out:?}");
}

#[test]
fn syn_1_the_change_background_wins_over_every_syntax_color_under_si_s_palette() {
    let bg = (0x1e, 0x1e, 0x1e);
    let pal = si_palette(bg);
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
            pal: Some(pal),
        };
        let looks = vec![vec![(0..2, Look::Kw), (2..3, Look::Com), (3..4, Look::Sel)], vec![]];
        p.draw(Screen { frame, looks, paints: vec![], syn: palette("1e/1e/1e") }).unwrap();
        String::from_utf8(p.out).unwrap()
    };
    let on_change = draw(Style::Change);
    let want = format!("48;2;{};{};{}", pal.change.0, pal.change.1, pal.change.2);
    assert!(on_change.contains(&want), "si's change background: {on_change:?}");
    let text = format!("38;2;{};{};{}", pal.change_text.0, pal.change_text.1, pal.change_text.2);
    // Every sequence in it sets the text to si's gray on the change background, and only that.
    let colors = on_change.matches("38;2;").count();
    assert_eq!(on_change.matches(&text).count(), colors, "no syntax color on it: {on_change:?}");
    assert_eq!(
        on_change.matches("48;2;").count(),
        colors,
        "and no other background: {on_change:?}"
    );
    assert!(on_change.contains(";1m") && on_change.contains(";3m"), "bold and italic remain");
    let plain = draw(Style::Primary);
    assert!(plain.contains("48;2;") && plain.contains("38;2;"), "colors elsewhere: {plain:?}");
}

#[test]
fn syn_1_a_reply_from_the_terminal_changes_the_colors_of_the_next_screen() {
    let mut r =
        rig_env("hl-osc", &[&format!("main.rs={RUST}")], &["main.rs"], (40, 8), |e: &mut Env| {
            e.syn = Syntax::new(true);
        });
    assert!(r.shot.syn.hue(Look::Kw).is_none());
    r.send(SimEvent::Term(Event::Bg((0x1e, 0x1e, 0x1e))));
    r.send(SimEvent::Osc("4;1;rgb:cc/00/00".into()));
    assert!(r.shot.syn.hue(Look::Kw).is_some());
    let mut p = Painter::new(Vec::new());
    p.draw(r.shot.clone()).unwrap();
    assert!(String::from_utf8(p.out).unwrap().contains("38;2;"), "drawn with the adjusted red");
    r.send(SimEvent::Osc("52;c;junk".into()));
    r.keys("lll"); // an unknown reply and keys go on as usual
    assert_eq!(r.status(), "main.rs  1:4  NORMAL");
}

#[test]
fn syn_1_the_quit_waits_for_the_terminal_only_while_an_answer_is_still_to_come() {
    // VIS-4: sim asks the background (si's `enter`) and, with COLORTERM, five ANSI colors; a quit
    // before they are in waits for the terminal to finish (bounded), so none is typed into the shell.
    let ansi = ["4;1;rgb:cc/00/00", "4;2;rgb:00/aa/00", "4;3;rgb:aa/aa/00", "4;5;rgb:aa/00/aa"];
    let mut r = rig_env("hl-asking", &["a.rs"], &["a.rs"], (40, 8), |e: &mut Env| {
        e.syn = Syntax::new(true);
    });
    assert!(r.sim.asking(), "nothing is in yet");
    r.send(SimEvent::Term(Event::Bg((0x1e, 0x1e, 0x1e))));
    assert!(r.sim.asking(), "the colors of syntax are still to come");
    ansi.iter().for_each(|a| r.send(SimEvent::Osc((*a).into())));
    assert!(r.sim.asking(), "all but cyan");
    r.send(SimEvent::Osc("4;6;rgb:00/aa/aa".into()));
    assert!(!r.sim.asking(), "all in: a quit has nothing to wait for");
    // Without COLORTERM only the background is asked.
    let mut r = rig_sized("hl-asking2", &["a.rs"], &["a.rs"], (40, 8));
    assert!(r.sim.asking());
    r.send(SimEvent::Term(Event::Bg((255, 255, 255))));
    assert!(!r.sim.asking());
}

/// The real binary in a pty, against a terminal that answers every query 30 ms late, in order (si's
/// scripts/pty_bench.py): `:q` at the first frame must not end before the answers are in.
#[test]
fn syn_1_a_quit_before_the_terminal_has_answered_leaves_nothing_for_the_shell() {
    let dir = temp_dir("hl-leak");
    std::fs::write(dir.0.join("a.rs"), "fn main() {}\n").unwrap();
    let (sim, file) = (env!("CARGO_BIN_EXE_sim"), dir.0.join("a.rs"));
    let Some(m) = pty_bench(&["leak", sim, file.to_str().unwrap(), "30"]) else { return };
    assert_eq!(m["leaked"], 0.0, "the shell would have typed the terminal's answer");
    assert!(
        m["quit_ms"] >= m["owed_ms"] - 5.0,
        "it quit in {:.1} ms, before the answer ({:.1} ms from the `:q`) could be in",
        m["quit_ms"],
        m["owed_ms"]
    );
    for (kind, most) in [("answering", 80.0), ("no_osc", 80.0), ("silent", 400.0)] {
        let m = pty_bench(&["quit", sim, file.to_str().unwrap(), kind]).unwrap();
        assert!(m["quit_ms"] < most, "a quit with a {kind} terminal took {:.1} ms", m["quit_ms"]);
        assert_eq!(m["quit_ms"] >= 90.0, kind == "silent", "{kind}: {:.1} ms", m["quit_ms"]);
    }
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

#[test]
fn syn_3_the_highlighter_forgets_a_file_that_is_closed() {
    // Its parse states and a copy of its text (shared with a buffer that is gone) would stay for
    // as long as sim runs, for every file ever opened.
    let mut h = Highlighter::default();
    h.run(req(1, "a.rs", 1, "fn a() {}\n", 0, 5));
    h.run(req(2, "b.rs", 1, "fn b() {}\n", 0, 5));
    assert_eq!(h.files(), 2);
    h.forget(1);
    assert_eq!(h.files(), 1);
    h.forget(1);
    h.forget(7);
    assert_eq!(h.files(), 1, "forgetting what is not there does nothing");
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
fn syn_3_a_parse_that_never_ends_is_given_up_and_the_other_files_are_still_highlighted() {
    // syntect 5.3 loops for ever on some text, in bat's QML (`x = y /* c */`, a statement without
    // a semicolon that ends in a comment; JavaScript has the same fault, and is read with
    // TypeScript's syntax, SYN-4): the one thread that highlights every file would be lost, and a
    // core kept busy, for the rest of the session.
    use std::time::Duration;
    let looped = "x = y /* c */\n";
    let (tx, rx) = std::sync::mpsc::channel();
    let jobs = sim::io::highlight_thread_within(tx, Duration::from_millis(300));
    let next =
        |what: &str| rx.recv_timeout(Duration::from_secs(10)).unwrap_or_else(|_| panic!("{what}"));
    jobs.send(HlJob::Run(req(1, "a.qml", 1, looped, 0, 1))).unwrap();
    let mut told = false;
    let plain = loop {
        match next("the parse that never ends was not given up") {
            SimEvent::Notice(m) => told |= m.contains("a.qml") && m.contains("colors"),
            SimEvent::Spans(h) => break h,
            e => panic!("{e:?}"),
        }
    };
    assert_eq!((plain.id, plain.spans.len()), (1, 1));
    assert!(plain.spans.iter().all(Vec::is_empty), "the lines are plain");
    // Another file is highlighted afterwards.
    jobs.send(HlJob::Run(req(2, "b.rs", 1, "fn f() {}\n", 0, 1))).unwrap();
    let mut other = None;
    while other.is_none() {
        match next("the other file was not answered") {
            SimEvent::Spans(h) if h.id == 2 => other = Some(h),
            SimEvent::Notice(m) => told |= m.contains("a.qml"),
            e => panic!("{e:?}"),
        }
    }
    assert_eq!(other.unwrap().spans[0], [(0..2, Look::Kw), (3..4, Look::Fun)]);
    // The file that was given up is answered at once at its next edit, plain, and not parsed again.
    let t = std::time::Instant::now();
    jobs.send(HlJob::Run(req(1, "a.qml", 2, "x = y /* c */\nz\n", 0, 2))).unwrap();
    match next("the file that was given up was parsed again") {
        SimEvent::Spans(h) => assert!(h.id == 1 && h.ver == 2 && h.spans.iter().all(Vec::is_empty)),
        e => panic!("{e:?}"),
    }
    assert!(t.elapsed() < Duration::from_millis(250), "{:?}", t.elapsed());
    assert!(told, "the user is told that the colors of a.qml stopped");
}

#[test]
fn syn_3_the_highlighter_thread_answers_the_newest_request_of_each_file() {
    let (tx, rx) = std::sync::mpsc::channel();
    let jobs = sim::io::highlight_thread(tx);
    for v in 1..=5 {
        jobs.send(HlJob::Run(req(7, "a.rs", v, "fn f() {}\n", 0, 1))).unwrap();
    }
    jobs.send(HlJob::Run(req(8, "b.rs", 1, "let x = 1;\n", 0, 1))).unwrap();
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
        e.syn = palette("1e/1e/1e")
    });
    r.keys("vlll");
    assert_eq!((gray(&r), underlined(&r)), (1, 0));
    golden("syn_5_selection", &dump_screen(&r.shot));
    // Not truecolor: the underline, whatever the terminal answered.
    let mut r = rig_env("sel-16", &[&content], &["main.rs"], (40, 8), |e: &mut Env| {
        e.syn = Syntax::new(false);
        e.syn.bg((0x1e, 0x1e, 0x1e));
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
        let bg = (rgb[0], rgb[1], rgb[2]);
        assert_eq!(shade.0, shade.1, "neutral");
        assert_eq!(shade.1, shade.2, "neutral");
        assert!(apca(shade, bg).abs() >= 15.0, "{bg:?}: the gray reads at {:.1}", apca(shade, bg));
        assert!(
            apca(text, shade).abs() >= 75.0,
            "{bg:?}: its text reads at {:.1}",
            apca(text, shade)
        );
        assert!(
            apca(shade, bg).abs() < 30.0,
            "{bg:?}: a hint, not a block: {:.1}",
            apca(shade, bg)
        );
    }
}
