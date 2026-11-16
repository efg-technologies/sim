//! MD-1 to MD-7, MD-12: the read view of a Markdown file. The plan of a line (pure), then the
//! screens (Rig): which lines are raw, what is never hidden, how a table is aligned, the key, plain
//! mode and the pointer, and a link that reads as its text where its line has nothing that lies.
#![allow(clippy::unwrap_used)] // helpers; the tests themselves may unwrap
mod common;
use common::*;
use sim::color::{Syntax, Tones};
use sim::io::{load_md, save_md};
use sim::layout::Skin;
use sim::md::{Plan, Read, Why, may_hide};
use sim::paint::{Painter, Screen};
use sim::sim::{SimEffect, SimEvent};
use sim::syn::{Spans, Syn};
use sim_core::core::app::Event;
use sim_core::core::color::{Rgb, palette};
use sim_core::core::view::{Frame, Row, Shape, show};

const BG: Rgb = (0x1e, 0x1e, 0x1e);

fn spans_of(lines: &[String]) -> Vec<Spans> {
    let mut syn = Syn::new("a.md", "").unwrap();
    syn.spans(&|i| lines.get(i).cloned(), 0, lines.len())
}

/// The line `cs` as `plan` draws it: what is put in, and the chars that stay.
fn render(plan: &Plan, cs: &[char]) -> String {
    let mut out = String::new();
    for i in 0..=cs.len() {
        let op = plan.op(i);
        if let Some((c, n)) = op.fill {
            out.extend(std::iter::repeat_n(c, n.into()));
        }
        out.extend(cs.get(i).filter(|_| !op.hide));
    }
    out
}

/// The lines of `text` as the read view draws them, none of them raw (`raw`: the lines that are).
fn drawn(text: &str, raw: std::ops::Range<usize>, pane: usize) -> Vec<String> {
    drawn_with(text, raw, pane, true)
}

/// `drawn`, with `honest` saying whether MD-10 gave the lines no mark (MD-12).
fn drawn_with(text: &str, raw: std::ops::Range<usize>, pane: usize, honest: bool) -> Vec<String> {
    let lines: Vec<String> = text.lines().map(str::to_string).collect();
    let sp = spans_of(&lines);
    let read = Read::new(0..lines.len(), raw, pane, &|l| lines.get(l).cloned(), &|l| sp.get(l));
    (0..lines.len())
        .map(|l| {
            let cs: Vec<char> = lines[l].chars().collect();
            let plan = read.plan(l, &cs, &sp[l], honest);
            plan.map_or_else(|| lines[l].clone(), |p| render(&p, &cs))
        })
        .collect()
}

const PROSE: &str = "# Title\n\nSome **bold** and *em* and `code` and _under_ here.\n\nSecond **line** with `ticks`.\n\n- item with **b**\n";

#[test]
fn md_1_the_markers_of_emphasis_and_code_are_left_out_and_the_rest_of_the_line_is_the_files() {
    let d = drawn(PROSE, 99..99, 80);
    assert_eq!(d[0], "# Title", "a heading keeps its hashes");
    assert_eq!(d[2], "Some bold and em and code and under here.");
    assert_eq!(d[4], "Second line with ticks.");
    assert_eq!(d[6], "- item with b", "a bullet is a bullet");
    // a line that is raw is the file's own
    let d = drawn(PROSE, 2..3, 80);
    assert_eq!(d[2], "Some **bold** and *em* and `code` and _under_ here.");
    assert_eq!(d[4], "Second line with ticks.");
}

#[test]
fn md_1_front_matter_and_a_fenced_block_are_not_read_and_nor_is_a_table_without_spans() {
    let text =
        "---\ntitle: a *b* c\ntags: [a_b, c_d]\n---\n\nbody **x**\n\n```\nlet a = **p;\n```\n";
    let d = drawn(text, 99..99, 80);
    assert_eq!(&d[..4], &["---", "title: a *b* c", "tags: [a_b, c_d]", "---"], "front matter");
    assert_eq!(d[5], "body x");
    assert_eq!(&d[7..], &["```", "let a = **p;", "```"], "a fence");
    let tab = "| a | b |\n|---|---|\n| **x** | y |\n";
    let lines: Vec<String> = tab.lines().map(str::to_string).collect();
    let sp = spans_of(&lines);
    let read =
        Read::new(0..3, 99..99, 80, &|l| lines.get(l).cloned(), &|l| (l != 2).then(|| &sp[l]));
    let cs: Vec<char> = lines[2].chars().collect();
    assert_eq!(
        read.plan(2, &cs, &sp[2], true),
        None,
        "a table with a row that has no spans is raw"
    );
}

/// MD-12: the chars a link may leave out, found from the chars alone: the brackets and the target of
/// each `[text](target)` whose `[` has no `!` or backslash before it, whose text has no bracket and a
/// char that is not a blank, and whose target is ASCII and graphic and none of `(`, `)`, `<`, `>`, `\`.
fn link_chars(cs: &[char]) -> Vec<bool> {
    let mut out = vec![false; cs.len()];
    for o in (0..cs.len()).filter(|&o| cs[o] == '[') {
        let Some(c) = (o + 1..cs.len()).find(|&k| matches!(cs[k], '[' | ']')) else { continue };
        let plain = |x: &char| x.is_ascii_graphic() && !"()<>\\".contains(*x);
        let Some(e) = (c + 2..cs.len()).find(|&k| !plain(&cs[k])) else { continue };
        let text = cs[o + 1..c].iter().any(|x| !x.is_whitespace());
        if o > 0 && matches!(cs[o - 1], '!' | '\\') || cs[c] != ']' || cs.get(c + 1) != Some(&'(') {
            continue;
        }
        if text && e > c + 2 && cs[e] == ')' {
            out[o] = true;
            (c..=e).for_each(|k| out[k] = true);
        }
    }
    out
}

#[test]
fn md_2_no_plan_leaves_out_a_char_that_markup_a_table_or_a_link_does_not_name() {
    // (the second three rows: links, which MD-12 may draw as their text, and the pieces of some)
    #[rustfmt::skip]
    const PIECES: [&str; 39] = [
        "*", "**", "_", "`", "|", "-", ":", " ", "  ", "a", "bc", "日", "\u{202e}", "\u{200b}",
        "\x1b", "\u{85}", "[", "](", ")", "#", "\\", "\u{feff}",
        "[a](b)", "](https://x.y/z)", "![", "<", ">", "\"t\"", "(", "\t", "\u{200d}", "\u{fe0f}",
        "\u{2066}", "é", "[a](日)", "[a](x\u{202e}y)", "[b](\x1b)", "[c](d e)", "[a](b(c))",
    ];
    let hostile = |c: char| {
        c.is_control() || "\u{200b}\u{202e}\u{feff}\u{61c}\u{200d}\u{fe0f}\u{2066}".contains(c)
    };
    let mut linked = 0;
    let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
    for round in 0..300 {
        let mut doc = String::new();
        if round % 2 == 0 {
            doc += "| a | b |\n|:--|--:|\n";
        }
        for _ in 0..6 {
            if rng.below(2) == 0 {
                doc.push('|');
            }
            for _ in 0..rng.below(14) + 1 {
                doc += PIECES[rng.below(PIECES.len())];
            }
            doc.push('\n');
        }
        let lines: Vec<String> = doc.lines().map(str::to_string).collect();
        let sp = spans_of(&lines);
        let read =
            Read::new(0..lines.len(), 99..99, 100, &|l| lines.get(l).cloned(), &|l| sp.get(l));
        // (MD-12: a line that MD-10 marked has its links whole; one it did not may have them go)
        for (l, line, honest) in
            lines.iter().enumerate().flat_map(|(l, x)| [(l, x, false), (l, x, true)])
        {
            let cs: Vec<char> = line.chars().collect();
            let Some(plan) = read.plan(l, &cs, &sp[l], honest) else { continue };
            let link = link_chars(&cs);
            for (i, &c) in cs.iter().enumerate() {
                if !plan.op(i).hide {
                    continue;
                }
                let named =
                    [Why::Markup, Why::Blank, Why::Rule].into_iter().any(|w| may_hide(c, w));
                let why = if honest { "" } else { " (a line with a mark)" };
                assert!(
                    (named || honest && link[i]) && !hostile(c),
                    "left out {c:?} at {i} of {line:?}{why} in\n{doc}"
                );
                linked += usize::from(c == '(');
            }
            // every hostile char of the line is in what is drawn, as many times as it is in the line
            let out = render(&plan, &cs);
            for h in cs.iter().filter(|&&c| hostile(c)) {
                let (n, m) = (cs.iter().filter(|c| *c == h).count(), out.matches(*h).count());
                assert_eq!(n, m, "{h:?} of {line:?} drawn as {out:?} in\n{doc}");
            }
        }
    }
    assert!(linked > 50, "the lines have links that are drawn as their text: {linked}");
}

#[test]
fn md_2_a_line_left_with_nothing_but_blanks_keeps_its_markers() {
    let d = drawn("a\n\n****\n\n`` ``\n\n| ** | x |\n|----|---|\n| b | c |\n", 99..99, 80);
    assert_eq!(d[2], "****", "{d:?}");
    assert!(d[6].contains("**"), "a cell that would be empty keeps its markers: {d:?}");
}

#[test]
fn md_3_the_cells_are_aligned_by_display_width_and_as_the_delimiter_row_says() {
    let text = "| name | n |\n|:--|--:|\n| 日本語 | 1 |\n| **b** | 22 |\n";
    let d = drawn(text, 99..99, 80);
    assert_eq!(d[0], "| name   |  n |");
    assert_eq!(d[1], "|────────|────|");
    assert_eq!(d[2], "| 日本語 |  1 |");
    assert_eq!(d[3], "| b      | 22 |");
    let widths: Vec<usize> = d.iter().map(|r| width(r)).collect();
    assert!(widths.iter().all(|w| *w == widths[0]), "one width for every row: {widths:?}");
    let center = drawn("|a|b|\n|:-:|:-:|\n|xxxx|y|\n", 99..99, 80);
    assert_eq!((&*center[0], &*center[2]), ("|  a   | b |", "| xxxx | y |"), "{center:?}");
}

#[test]
fn md_3_an_escaped_pipe_is_text_and_a_short_row_is_shown_with_the_cells_it_has() {
    let d = drawn("| a | b |\n|---|---|\n| x \\| y |\n| p |\n|  |\n", 99..99, 80);
    assert_eq!(d[0], "| a      | b |");
    assert_eq!(d[1], "|────────|───|");
    assert_eq!(d[2], "| x \\| y |", "the escaped pipe is text, and the row has one cell");
    assert_eq!(d[3], "| p      |");
    assert_eq!(d[4], "|        |", "an empty cell is blank");
}

#[test]
fn md_4_a_table_that_is_too_wide_or_has_a_cell_too_many_is_raw_whole() {
    let wide = format!("| {} | b |\n|---|---|\n| x **y** | z |\n", "w".repeat(30));
    let rows: Vec<&str> = wide.lines().collect();
    assert_eq!(drawn(&wide, 99..99, 80)[2], format!("| x y{} | z |", " ".repeat(27)));
    // 38 columns wide, in a pane of 30: raw, and its markers stay
    assert_eq!(drawn(&wide, 99..99, 30), rows);
    let many = "| a |\n|---|\n| x | y |\n| **p** |\n";
    assert_eq!(drawn(many, 99..99, 80), many.lines().collect::<Vec<_>>(), "a cell would be lost");
    let tab = "| a |\n|---|\n| x\ty |\n| **p** |\n";
    assert_eq!(drawn(tab, 99..99, 80), tab.lines().collect::<Vec<_>>());
    let ragged = "| a |\n  |---|\n| **x** |\n";
    assert_eq!(drawn(ragged, 99..99, 80), ragged.lines().collect::<Vec<_>>());
    let lone = "| a *b* c\n";
    assert_eq!(
        drawn(lone, 99..99, 80),
        ["| a *b* c"],
        "a run of pipe lines that is no table is raw"
    );
}

#[test]
fn md_12_a_link_is_drawn_as_its_text_where_its_line_has_no_mark() {
    let text = "See [the docs](https://docs.example/a?b=c#d) and [e](f) here.\n\n- [a](b)\n\n# [a](b)\n\n\
                [`c`](u) and [**b**](u) and [[a](b) and [a](b)[c](d)\n\n<a href=\"x\">[a](b)</a>\n";
    let d = drawn(text, 99..99, 100);
    assert_eq!(d[0], "See the docs and e here.");
    assert_eq!(d[2], "- a", "{d:?}");
    assert_eq!(d[4], "# a", "{d:?}");
    // (the highlighter calls no marker the emphasis of a link's text, MD-2; a `[` before a link is text)
    assert_eq!(d[6], "c and **b** and [a and ac", "{d:?}");
    assert_eq!(d[8], "<a href=\"x\">a</a>", "{d:?}");
    // a line that MD-10 gave a mark keeps every link whole, and so does the cursor's
    let first = text.lines().next().unwrap();
    assert_eq!(drawn_with(text, 99..99, 100, false)[0], first);
    assert_eq!(drawn_with(text, 99..99, 100, false)[2], "- [a](b)");
    assert_eq!(drawn(text, 0..1, 100)[0], first);
}

#[test]
fn md_12_a_link_whose_shape_or_target_is_not_plain_is_drawn_whole() {
    let whole = [
        "a [t](b \"title\") z",
        "a [t](<b>) z",
        "a [t][r] z",
        "a <https://x.example> z",
        "a ![t](b.png) z",
        "a [![i](s.png)](t) z",
        "a \\[t](b) z",
        "a [](b) z",
        "a [ ](b) z",
        "a [t]( b ) z",
        "a [t](b\tc) z",
        "a [t](b(c)) z",
        "a [t](b\\)c) z",
        "a [t](b\\c) z",
        "a [t](b<c) z",
        "a [t](b>c) z",
        "a [t](日) z",
        "a [t](https://例え.jp) z",
        "a [t](b\u{202e}c) z",
        "a [t](b\u{200b}c) z",
        "a [t](b\x1bc) z",
        "a [t](b\u{e0041}c) z",
        "a [t\\]u](b) z",
        "a [t]u](b) z",
        "a [❤\u{fe0f}](u) z",
        "a [\u{200d}t](u) z",
    ];
    let text: String = whole.iter().map(|l| format!("{l}\n\n")).collect();
    let d = drawn(&text, 99..99, 100);
    for (k, l) in whole.iter().enumerate() {
        assert_eq!(d[2 * k], *l, "{l:?}");
    }
    // a link over lines, a table's row (its widths are all its rows'), a fence and front matter
    for doc in [
        "a [t\nu](b) z\n",
        "| [t](b) | c |\n|---|---|\n| d | e |\n",
        "x\n\n```\n[t](b)\n```\n",
        "---\ntitle: [t](b)\n---\n",
    ] {
        let d = drawn(doc, 99..99, 100);
        let raw: Vec<&str> = doc.lines().collect();
        assert!(d.iter().zip(&raw).all(|(x, r)| !r.contains("](") || x == r), "{doc:?}: {d:?}");
    }
}

#[test]
fn md_12_only_a_bracket_and_a_plain_ascii_char_of_a_target_may_go_and_no_char_shown_in_another_form()
 {
    for c in ['[', ']', '(', ')'] {
        assert!(may_hide(c, Why::Link), "{c:?}");
    }
    for c in ['!', '<', '>', '\\', ' ', 'a', '*', '`', '\u{202e}'] {
        assert!(!may_hide(c, Why::Link), "{c:?}");
    }
    for c in "azAZ09/:.#?=&%~-_*+@!$',;[]{}|^`\"".chars() {
        assert!(may_hide(c, Why::Target), "{c:?}");
    }
    for c in ['(', ')', '<', '>', '\\', ' ', '\t', '\x7f', '\u{a0}', '日', 'é', 'ｇ', 'о'] {
        assert!(!may_hide(c, Why::Target), "{c:?}");
    }
    // SAFE-8, SAFE-10: no char that is shown in another form may go, for a link or for anything else
    let mut n = 0;
    for c in (0..=0x10_ffffu32).filter_map(char::from_u32).filter(|&c| show(&[c], 0).is_some()) {
        let whys = [Why::Markup, Why::Blank, Why::Rule, Why::Callout, Why::Link, Why::Target];
        assert!(!whys.into_iter().any(|w| may_hide(c, w)), "{c:?}");
        n += 1;
    }
    assert!(n > 800_000, "{n} chars are shown in another form");
}

// ---- the screens ----

fn md_rig(name: &str, text: &str, skin: Skin, size: (u16, u16)) -> Rig {
    let file = format!("README.md={text}");
    let mut r = rig_env(name, &[&file], &["README.md"], size, |e| {
        e.skin = skin;
        e.syn = Syntax::new(true);
        e.md = true;
    });
    r.send(SimEvent::Term(Event::Bg(BG)));
    r.hold_spans = true;
    r.keys("l"); // (a key: the highlighter is asked for the window)
    r.deliver_spans();
    r
}

/// The text of the editor's rows after the gutter, without trailing blanks (the boxes of depth are not in it).
fn body(r: &Rig) -> Vec<String> {
    let (g, n) = (r.sim.geo(), r.screen.rows.len() - 1);
    let (from, to) = (usize::from(g.tx()) + 5, usize::from(g.tx() + g.ew));
    let cut = |x: &sim_core::core::view::Row| -> String {
        let (mut col, mut out) = (0, String::new());
        for c in x.text.chars() {
            let w = width(&c.to_string());
            if col >= from && col + w <= to {
                out.push(c);
            }
            col += w;
        }
        out.trim_end().to_string()
    };
    r.screen.rows[..n].iter().map(cut).collect()
}

fn has(rows: &[String], what: &str) -> bool {
    rows.iter().any(|x| x == what)
}

#[test]
fn md_1_the_cursor_line_and_the_selected_lines_are_raw_and_the_rest_is_read() {
    let raw = |l: &str| l.to_string();
    for skin in [Skin::Calm, Skin::Flat, Skin::Depth] {
        let mut r = md_rig("md-raw", PROSE, skin, (60, 14));
        let b = body(&r);
        assert!(has(&b, "# Title"), "{skin:?}: {b:?}");
        assert!(has(&b, "Some bold and em and code and under here."), "{skin:?}: read: {b:?}");
        assert!(has(&b, "Second line with ticks."), "{skin:?}: {b:?}");
        r.keys("jj");
        let b = body(&r);
        assert!(
            has(&b, &raw("Some **bold** and *em* and `code` and _under_ here.")),
            "{skin:?}: {b:?}"
        );
        assert!(has(&b, "Second line with ticks."), "{skin:?}: the other lines are read: {b:?}");
        // Visual mode: the lines that have a selected cell are raw
        r.keys("Vjj");
        let b = body(&r);
        assert!(has(&b, "Some **bold** and *em* and `code` and _under_ here."), "{skin:?}: {b:?}");
        assert!(has(&b, "Second **line** with `ticks`."), "{skin:?}: selected: {b:?}");
        assert!(has(&b, "- item with b"), "{skin:?}: not selected: {b:?}");
        r.keys("<Esc>");
        let b = body(&r);
        assert!(
            has(&b, "Some bold and em and code and under here."),
            "{skin:?}: read again: {b:?}"
        );
        assert!(has(&b, "Second **line** with `ticks`."), "{skin:?}: the cursor's line: {b:?}");
    }
}

#[test]
fn md_1_a_line_the_highlighter_has_not_answered_for_is_raw() {
    let rows: Vec<Row> = [" 1   a **b**", " 2   | x | y |", " 3   c"]
        .iter()
        .map(|t| Row { text: (*t).to_string(), ..Row::default() })
        .collect();
    let frame =
        Frame { width: 40, height: 4, rows, cursor: (0, 0), shape: Shape::Block, pal: None };
    let lines = ["a **b**", "| x | y |", "c"];
    let spans: Vec<Spans> = spans_of(&lines.map(str::to_string));
    let text = |l: usize| lines.get(l).map(ToString::to_string);
    let maps = |known: bool| -> Vec<bool> {
        let get = |l: usize| spans.get(l).filter(|_| known || l == 1);
        let c = sim::layout::colors(&frame, 3, &text, &get, None, Some(2..3));
        c.extra.iter().map(|e| e.map.is_some()).collect()
    };
    assert_eq!(maps(true), [true, false, false], "the markers of the first line are left out");
    assert_eq!(maps(false), [false, false, false], "no spans, no guess");
}

#[test]
fn md_1_the_read_view_changes_no_row_no_number_and_not_the_text() {
    let mut on = md_rig("md-same-on", PROSE, Skin::Depth, (60, 14));
    let mut off = md_rig("md-same-off", PROSE, Skin::Depth, (60, 14));
    off.keys("<Space>um");
    for keys in ["", "jj", "w", "Vj", "<Esc>G", "gg"] {
        on.keys(keys);
        off.keys(keys);
        let gutter = |r: &Rig| -> Vec<String> {
            let n = r.screen.rows.len() - 1; // (the status line says which is on)
            r.screen.rows[..n].iter().map(|x| x.text.chars().take(7).collect()).collect()
        };
        assert_eq!(gutter(&on), gutter(&off), "{keys:?}: the same rows and numbers");
        assert_eq!(on.text(), off.text(), "{keys:?}: the file");
        assert_eq!(on.screen.cursor, off.screen.cursor, "{keys:?}: the cursor, column for column");
    }
    assert_eq!(on.text(), PROSE.trim_end_matches('\n'));
}

fn draw(s: Screen) -> String {
    let mut p = Painter::new(Vec::new());
    p.draw(s).unwrap();
    String::from_utf8(p.out).unwrap()
}

/// The SGR sequence in force at the first `what` of `out`.
fn seq(out: &str, what: &str) -> String {
    assert!(out.contains(what), "{what:?} not drawn in {out:?}");
    let i = out.find(what).unwrap();
    format!("\x1b[{}", out[..i].rsplit("\x1b[").next().unwrap())
}

/// What a hidden char is drawn with: the alert's block in flat and depth, reverse video in calm.
fn alert(skin: Skin) -> String {
    let (t, p) = (Tones::new(&palette(BG)), palette(BG));
    let rgb = |c: Rgb| format!("{};{};{}", c.0, c.1, c.2);
    if skin == Skin::Calm {
        return format!("\x1b[0;7;38;2;{}m", rgb(p.primary));
    }
    let (back, text) = t.alert_block();
    format!("\x1b[0;38;2;{};48;2;{}m", rgb(text), rgb(back))
}

/// MD-2: a SAFE-8 char put into each construct whose markup the read view hides, and beside it
/// (`true`: the markers of the line are hidden; the highlighter does not call the emphasis inside a
/// link's text a marker, and it is not hidden).
const SMUGGLED: [(&str, &str, bool); 16] = [
    ("strong", "a **b\u{202e}c** d", true),
    ("only char in strong", "a **\u{202e}** d", true),
    ("emphasis", "a *b\u{200b}c* d", true),
    ("underscore", "a _b\u{feff}c_ d", true),
    ("code", "a `b\u{202e}c` d", true),
    ("beside a marker", "a *\u{202e}*b* d", true),
    ("between markers", "a **\u{202e}**\u{200b}*b* d", true),
    ("link text", "a [b\u{202e}c](http://x.y) **d**", true),
    ("emphasis in link text", "a [**b\u{202e}c**](http://x.y) d", false),
    ("code in link text", "a [`b\u{202e}c`](u) d", true),
    ("url", "a [b](http://x.y/\u{202e}z) **d**", true),
    ("heading", "## h *b\u{202e}c*", true),
    ("control", "a **b\x1bc** d", true),
    ("a table cell", "| **a\u{202e}b** | c |", true),
    ("code in a cell", "| `a\u{202e}b` | c |", true),
    ("edge of a cell", "|\u{202e}**a**\u{200b}| c |", true),
];

#[test]
fn md_2_a_hidden_char_is_the_alert_s_block_in_every_construct_every_look_and_state() {
    for (what, line, hides) in SMUGGLED {
        let table = line.starts_with('|');
        let text = if table {
            format!("# t\n\nx\n\n{line}\n|---|---|\n| p | q |\n\nend\n")
        } else {
            format!("# t\n\nx\n\n{line}\n\nend\n")
        };
        let token = if line.contains('\x1b') { "^[" } else { "<U+" };
        let hostile =
            line.chars().filter(|c| c.is_control() || "\u{202e}\u{200b}\u{feff}".contains(*c));
        for skin in [Skin::Calm, Skin::Flat, Skin::Depth] {
            let mut r = md_rig("md-smuggle", &text, skin, (70, 16));
            let want = alert(skin);
            // the read view is on: the markers of the construct are gone, and every char is drawn
            let b = body(&r);
            let row =
                b.iter().find(|x| x.contains(token)).unwrap_or_else(|| panic!("{what}: {b:?}"));
            assert_eq!(hides, !row.contains("**") && !row.contains('`'), "{what}: read: {row:?}");
            assert_eq!(row.matches(token).count(), hostile.clone().count(), "{what}: {row:?}");
            let out = draw(r.shot.clone());
            assert_eq!(seq(&out, token), want, "{what} {skin:?}: the alert's block, read view");
            // with the cursor on it (raw), selected, and lit by a search
            r.keys("jjjj");
            let raw = body(&r);
            let marked = |x: &String| x.contains(token) && x.contains(['*', '_', '`', '|']);
            assert!(raw.iter().any(marked), "{what}: raw: {raw:?}");
            let out = draw(r.shot.clone());
            assert_eq!(seq(&out, token), want, "{what} {skin:?}: the alert's block, raw");
            r.keys("V");
            let out = draw(r.shot.clone());
            assert_eq!(seq(&out, token), want, "{what} {skin:?}: the alert's block, selected");
            r.keys("<Esc>gg/c\n");
            let out = draw(r.shot.clone());
            assert_eq!(seq(&out, token), want, "{what} {skin:?}: the alert's block, in a search");
        }
    }
}

const TABLE: &str = "# T\n\n| name | n |\n|:--|--:|\n| 日本語 | 1 |\n| **b** | 22 |\n\nend\n";

#[test]
fn md_3_a_table_on_the_screen_is_aligned_in_every_look_and_the_cursor_row_is_raw() {
    for skin in [Skin::Calm, Skin::Flat, Skin::Depth] {
        let mut r = md_rig("md-table", TABLE, skin, (60, 14));
        let b = body(&r);
        let head = b.iter().position(|x| x.contains("name")).unwrap();
        let rows: Vec<String> = b[head..head + 4].to_vec();
        // the glyphs of a look are drawn over the chars: the pipes of the file are still `|`
        let pipes = |x: &String| x.replace(['│', '├', '┼', '┤'], "|").replace('─', "-");
        let flat: Vec<String> = rows.iter().map(pipes).collect();
        assert_eq!(flat[0], "| name   |  n |", "{skin:?}: {rows:?}");
        assert_eq!(flat[1], "|--------|----|", "{skin:?}");
        assert_eq!(flat[2], "| 日本語 |  1 |", "{skin:?}");
        assert_eq!(flat[3], "| b      | 22 |", "{skin:?}");
        if skin != Skin::Calm {
            assert_eq!(rows[0], "│ name   │  n │", "{skin:?}");
            assert_eq!(rows[1], "├────────┼────┤", "{skin:?}");
        } else {
            assert_eq!(rows[1], "|--------|----|", "calm draws no glyph");
        }
        // every bar is in the same column on every row, by display width
        let cols = |s: &str| -> Vec<usize> {
            let (mut col, mut out) = (0, vec![]);
            for c in s.chars() {
                if "|│├┼┤".contains(c) {
                    out.push(col);
                }
                col += width(&c.to_string());
            }
            out
        };
        assert!(rows.iter().all(|x| cols(x) == cols(&rows[0])), "{skin:?}: {rows:?}");
        // the cursor on a row of the table: that row is the file's own, the others stay aligned
        r.keys("jj");
        let b = body(&r);
        assert!(b.iter().any(|x| pipes(x) == "| name | n |"), "{skin:?}: raw: {b:?}");
        assert!(b.iter().any(|x| pipes(x) == "| 日本語 |  1 |"), "{skin:?}: {b:?}");
    }
}

#[test]
fn md_5_space_u_m_turns_the_read_view_off_and_on_and_says_so() {
    let mut r = md_rig("md-key", PROSE, Skin::Depth, (60, 14));
    assert!(has(&body(&r), "Second line with ticks."));
    r.keys("<Space>um");
    assert_eq!(r.status(), "markdown: raw");
    assert!(has(&body(&r), "Second **line** with `ticks`."), "{:?}", body(&r));
    r.keys("<Space>um");
    assert_eq!(r.status(), "markdown: read view");
    assert!(has(&body(&r), "Second line with ticks."));
    let asked = |e: &SimEffect| if let SimEffect::Md(on) = e { Some(*on) } else { None };
    let sent: Vec<bool> = r.log.iter().filter_map(asked).collect();
    assert_eq!(sent, [false, true], "each choice is asked to be remembered");
    // the leader's group is in which-key, with the key in it
    r.keys("<Space>u").pause();
    let shown = |r: &Rig| r.screen.rows.iter().any(|x| x.text.contains("markdown read view"));
    assert!(shown(&r), "{}", dump(&r.screen));
}

#[test]
fn md_5_the_choice_is_in_the_state_directory_and_the_first_run_is_the_read_view() {
    let dir = temp_dir("md-state");
    std::fs::set_permissions(&dir.0, std::os::unix::fs::PermissionsExt::from_mode(0o700)).unwrap();
    assert!(load_md(&dir.0), "no choice yet: read");
    save_md(&dir.0, false).unwrap();
    assert!(!load_md(&dir.0));
    assert_eq!(std::fs::read_to_string(dir.0.join("md")).unwrap(), "off\n");
    save_md(&dir.0, true).unwrap();
    assert!(load_md(&dir.0));
    std::fs::write(dir.0.join("md"), "garbage\n").unwrap();
    assert!(load_md(&dir.0), "anything else is the first run's choice");
}

#[test]
fn md_6_plain_mode_has_no_read_view_and_touches_no_state() {
    let file = format!("README.md={PROSE}");
    let mut r = rig_env("md-plain", &[&file], &["--plain", "README.md"], (60, 14), |e| e.md = true);
    r.keys("jjjj");
    let b = body(&r);
    assert!(has(&b, "Second **line** with `ticks`."), "{b:?}");
    r.keys("<Space>um");
    assert!(r.status().contains("plain mode: no read view"), "{}", r.status());
    assert!(!r.log.iter().any(|e| matches!(e, SimEffect::Md(_))), "nothing to remember");
    assert!(!r.sim.md);
}

#[test]
fn md_7_a_click_in_a_read_line_puts_the_cursor_on_the_char_of_the_file_under_the_pointer() {
    let mut r = md_rig("md-click", PROSE, Skin::Depth, (60, 14));
    let g = r.sim.geo();
    let y = body(&r).iter().position(|x| x == "Some bold and em and code and under here.").unwrap();
    // the `m` of `em`: the point is taken back through the cells the read view moved
    let x = usize::from(g.tx()) + 5 + "Some bold and e".len();
    r.click(x as u16, y as u16);
    let a = r.app();
    let line = a.buf.line_of(a.buf.cursor);
    let col = a.buf.cursor - a.buf.start(line);
    assert_eq!(line, 2);
    assert_eq!(
        &"Some **bold** and *em* and `code` and _under_ here."[col..col + 1],
        "m",
        "col {col}"
    );
    assert!(has(&body(&r), "Some **bold** and *em* and `code` and _under_ here."), "now raw");
}

#[test]
fn md_4_a_table_that_the_window_has_not_all_of_is_asked_for_whole_and_then_read() {
    // the first screen is lines 1 to 22, the window of its spans a screenful more: a jump inside
    // it shows a table whose last rows have no spans yet
    let mut doc: String = (1..=38).map(|i| format!("line {i} with **bold**\n")).collect();
    doc += "\n| name | n |\n|:--|--:|\n";
    doc += &(1..=40).map(|i| format!("| **row** {i} | {i} |\n")).collect::<String>();
    let mut r = md_rig("md-window", &doc, Skin::Calm, (60, 24));
    r.keys("35G");
    assert!(r.held.len() == 1, "the table is asked for: {:?}", r.held.len());
    assert!(!body(&r).iter().any(|x| x.starts_with("| row 1")), "raw until the spans come");
    r.deliver_spans();
    let b = body(&r);
    assert!(b.iter().any(|x| x.starts_with("| name ") && x.ends_with("|  n |")), "{b:?}");
    assert!(
        b.iter().any(|x| x.starts_with("| row 1  |  1 |") || x.starts_with("| row 10 |")),
        "{b:?}"
    );
    assert!(!b.iter().any(|x| x.contains("**row**")), "the markers are gone: {b:?}");
}

/// MD-12: a line of links that are honest, a line where one lies beside an honest one, and the end.
const LINKED: &str = "# t\n\nSee [the docs](https://docs.example/a) and [b](https://b.example).\n\n\
                      [a.example](https://evil.example/x) or [docs](https://docs.example/a).\n\nend\n";

#[test]
fn md_12_a_link_is_its_text_and_a_line_with_one_that_lies_keeps_every_link_whole() {
    for skin in [Skin::Calm, Skin::Flat, Skin::Depth] {
        let mut r = md_rig("md-dark", LINKED, skin, (90, 14));
        let b = body(&r);
        assert!(has(&b, "See the docs and b."), "{skin:?}: {b:?}");
        let lies = "[a.example](https://evil.example/x) or [docs](https://docs.example/a).";
        assert!(has(&b, lies), "{skin:?}: {b:?}");
        // the target that lies is the alert's (MD-10), the honest one beside it is not, and the text of
        // a link drawn as its text has a link's look: underlined
        let out = draw(r.shot.clone());
        assert_eq!(seq(&out, "https://evil.example/x"), alert(skin), "{skin:?}");
        assert_ne!(seq(&out, "https://docs.example/a"), alert(skin), "{skin:?}");
        assert!(seq(&out, "the docs").contains(";4"), "{skin:?}: {}", seq(&out, "the docs"));
        assert_eq!(
            out.matches("https://docs.example/a").count(),
            1,
            "{skin:?}: only on the lie's line"
        );
        // the cursor's line is raw: the targets are read there
        r.keys("jj");
        let b = body(&r);
        assert!(
            has(&b, "See [the docs](https://docs.example/a) and [b](https://b.example)."),
            "{skin:?}: {b:?}"
        );
        assert!(has(&b, lies), "{skin:?}: {b:?}");
        assert_eq!(r.text(), LINKED.trim_end_matches('\n'), "the file is as it was");
    }
}

#[test]
fn md_12_a_click_on_a_link_s_text_after_a_link_that_went_is_the_file_s_char() {
    const LINE: &str = "See [a](https://a.example) and [the docs](https://docs.example) now.";
    let mut r = md_rig("md-dark-click", &format!("# t\n\n{LINE}\n"), Skin::Depth, (80, 14));
    let g = r.sim.geo();
    let y = body(&r).iter().position(|x| x == "See a and the docs now.").unwrap();
    // the `d` of `docs`: the point is taken back past the link that went before it
    let x = usize::from(g.tx()) + 5 + "See a and the ".len();
    r.click(x as u16, y as u16);
    let a = r.app();
    let line = a.buf.line_of(a.buf.cursor);
    let col = a.buf.cursor - a.buf.start(line);
    assert_eq!(line, 2);
    assert_eq!(&LINE[col..col + 4], "docs", "col {col}");
    assert!(has(&body(&r), LINE), "now raw");
}

#[test]
fn md_12_a_search_shows_the_target_it_goes_to() {
    let text = "# t\n\nSee [the docs](https://docs.example/a) now.\n\nAnd [b](https://docs.example/b) too.\n";
    let mut r = md_rig("md-dark-search", text, Skin::Depth, (80, 12));
    r.keys("/docs.example\n");
    r.deliver_spans();
    let b = body(&r);
    assert!(
        has(&b, "See [the docs](https://docs.example/a) now.") && has(&b, "And b too."),
        "{b:?}"
    );
    r.keys("n");
    r.deliver_spans();
    let b = body(&r);
    assert!(
        has(&b, "See the docs now.") && has(&b, "And [b](https://docs.example/b) too."),
        "{b:?}"
    );
}

#[test]
fn md_12_a_char_shown_in_another_form_in_a_target_keeps_the_link_whole_on_the_screen() {
    // SAFE-8: the C0 controls (the line break is the line's end), DEL, the C1 controls, the bidi
    // controls, U+200B and U+FEFF; and a few more of SAFE-10's
    let c0 = (0..0x20u32).filter(|&c| c != 0x0a);
    let bidi = [0x200e, 0x200f, 0x202a, 0x202b, 0x202c, 0x202d, 0x202e, 0x2066, 0x2067, 0x2068];
    let more = [0x2069, 0x200b, 0xfeff, 0x200d, 0xad, 0xe0041, 0x180e, 0x2060, 0x61c];
    let chars: Vec<char> = c0
        .chain([0x7f])
        .chain(0x80..0xa0)
        .chain(bidi)
        .chain(more)
        .filter_map(char::from_u32)
        .collect();
    let text: String = std::iter::once("# t\n".to_string())
        .chain(chars.iter().map(|c| format!("[docs](https://x.example/a{c}b) z\n")))
        .collect();
    for skin in [Skin::Calm, Skin::Depth] {
        let r = md_rig("md-dark-safe8", &text, skin, (80, chars.len() as u16 + 4));
        let b = body(&r);
        let whole = b.iter().filter(|x| x.starts_with("[docs](https://x.example/a")).count();
        assert_eq!(whole, chars.len(), "{skin:?}: every link is whole: {b:#?}");
        assert!(!b.iter().any(|x| x.starts_with("docs")), "{skin:?}: none is its text: {b:#?}");
        // and each char is drawn in its form, where the tab is blanks
        let forms = b.iter().filter(|x| x.contains("<U+") || x.contains('^')).count();
        assert_eq!(forms, chars.len() - 1, "{skin:?}: {b:#?}");
    }
}
