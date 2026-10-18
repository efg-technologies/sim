//! MD-11: Markdown callouts (GitHub's alerts): which quotes are one, how the read view draws the
//! kind's word in place of the marker, and how a kind is told in each look.
#![allow(clippy::unwrap_used)] // helpers; the tests themselves may unwrap
mod common;
use common::*;
use sim::color::Syntax;
use sim::layout::Skin;
use sim::md::{Callout, Read, Why, callouts, may_hide};
use sim::paint::{Painter, Resolver};
use sim::sim::SimEvent;
use sim::syn::{Spans, Syn};
use sim_core::core::app::Event;
use sim_core::core::color::{Rgb, palette};

const BG: Rgb = (0x1e, 0x1e, 0x1e);

fn spans_of(lines: &[String]) -> Vec<Spans> {
    let mut syn = Syn::new("a.md", "").unwrap();
    syn.spans(&|i| lines.get(i).cloned(), 0, lines.len())
}

/// The callouts of `text`: the line of the marker and the kind.
fn found(text: &str) -> Vec<(usize, Callout)> {
    let lines: Vec<String> = text.lines().map(str::to_string).collect();
    let sp = spans_of(&lines);
    callouts(0..lines.len(), &|l| lines.get(l).cloned(), &|l| sp.get(l))
        .iter()
        .map(|b| (b.lines.start, b.kind))
        .collect()
}

/// What GitHub's own renderer did with each of these (its `/markdown` API, mode `gfm`, 2026-10-18):
/// the same text, and the kind it made an alert of, if any.
const GITHUB: [(&str, &str, Option<Callout>); 24] = [
    ("upper", "> [!NOTE]\n> body\n", Some(Callout::Note)),
    ("lower", "> [!note]\n> body\n", Some(Callout::Note)),
    ("mixed", "> [!nOtE]\n> body\n", Some(Callout::Note)),
    ("text on the marker's line", "> [!NOTE] body here\n> more\n", None),
    ("no body", "> [!NOTE]\n", None),
    ("a blank quote line is no body", "> [!NOTE]\n>\n", None),
    ("blank quote lines, then a body", "> [!NOTE]\n>\n>\n> body\n", Some(Callout::Note)),
    ("trailing blanks", "> [!TIP]   \n> body\n", Some(Callout::Tip)),
    ("a blank inside", "> [! NOTE]\n> body\n", None),
    ("unknown kind", "> [!FOO]\n> body\n", None),
    ("a title after it (Obsidian's)", "> [!NOTE]- title\n> body\n", None),
    ("no blank after the bar", ">[!IMPORTANT]\n>body\n", Some(Callout::Important)),
    ("a tab after the bar", ">\t[!NOTE]\n>\tbody\n", Some(Callout::Note)),
    ("blanks before the marker", ">    [!NOTE]\n> x\n", Some(Callout::Note)),
    ("a quote in a quote", "> > [!NOTE]\n> > body\n", None),
    ("not the first line", "> intro\n> [!NOTE]\n> body\n", None),
    ("in strong", "> **[!NOTE]**\n> x\n", None),
    ("in code", "> `[!NOTE]`\n> x\n", None),
    ("a colon after it", "> [!NOTE]:\n> x\n", None),
    ("empty", "> [!]\n> x\n", None),
    ("two bangs", "> [!!NOTE]\n> x\n", None),
    ("a lazy line", "> [!WARNING]\n> first\nlazy\n", Some(Callout::Warning)),
    ("after a heading", "# h\n> [!CAUTION]\n> body\n", Some(Callout::Caution)),
    ("in a fence", "```\n> [!NOTE]\n> x\n```\n", None),
];

#[test]
fn md_11_a_quote_is_a_callout_where_githubs_renderer_makes_an_alert_of_it() {
    for (what, text, kind) in GITHUB {
        let at = text.lines().position(|l| l.contains("[!")).unwrap();
        assert_eq!(found(text), kind.map(|k| (at, k)).into_iter().collect::<Vec<_>>(), "{what}");
    }
    // two in one file, and a body that is a list or a fence is a body
    let two = "> [!NOTE]\n> a\n\n> [!TIP]\n> - b\n> - c\n";
    assert_eq!(found(two), [(0, Callout::Note), (3, Callout::Tip)]);
}

#[test]
fn md_11_where_sim_does_not_follow_github_a_quote_stays_a_quote() {
    // GitHub makes an alert of a quote indented by one to three blanks, and sim leaves it plain: in a
    // list item GitHub leaves it plain too, and a line cannot tell the two apart
    assert_eq!(found("  > [!CAUTION]\n  > body\n"), []);
    assert_eq!(found("- item\n\n  > [!NOTE]\n  > in a list\n"), []);
    assert_eq!(found("1. a\n   > [!NOTE]\n   > b\n"), []);
}

/// The lines of `text` as the read view draws them with the callouts it finds; `raw` are the lines
/// drawn as they are.
fn drawn(text: &str, raw: std::ops::Range<usize>, icons: bool) -> Vec<String> {
    let lines: Vec<String> = text.lines().map(str::to_string).collect();
    let sp = spans_of(&lines);
    let at = |l: usize| lines.get(l).cloned();
    let mut read = Read::new(0..lines.len(), raw, 80, &at, &|l| sp.get(l));
    read.calls = callouts(0..lines.len(), &at, &|l| sp.get(l));
    read.icons = icons;
    (0..lines.len())
        .map(|l| {
            let cs: Vec<char> = lines[l].chars().collect();
            let Some(plan) = read.plan(l, &cs, &sp[l]) else { return lines[l].clone() };
            let mut out = String::new();
            for i in 0..=cs.len() {
                let op = plan.op(i);
                if let Some((c, n)) = op.fill {
                    out.extend(std::iter::repeat_n(c, n.into()));
                }
                out.extend(cs.get(i).filter(|_| !op.hide));
            }
            out
        })
        .collect()
}

const ALL: &str = "> [!NOTE]\n> a\n\n> [!tip]\n> b\n\n> [!Important]\n> c\n\n> [!WARNING]  \n> d\n\n> [!CAUTION]\n> e\n";

#[test]
fn md_11_the_read_view_draws_the_kinds_word_in_place_of_its_marker_and_keeps_the_rest() {
    let d = drawn(ALL, 99..99, false);
    assert_eq!(d[0], "> Note");
    assert_eq!(d[3], "> Tip", "{d:?}");
    assert_eq!(d[6], "> Important");
    assert_eq!(d[9], "> Warning  ", "the blanks after it are the file's");
    assert_eq!(d[12], "> Caution");
    for body in [1, 4, 7, 10, 13] {
        assert_eq!(d[body], ALL.lines().nth(body).unwrap(), "a body line is the file's own");
    }
    // with icons (zi): GitHub's own octicon in the Nerd Font, a blank, the word
    let d = drawn(ALL, 99..99, true);
    for (l, icon) in
        [(0, '\u{f449}'), (3, '\u{f400}'), (6, '\u{f50a}'), (9, '\u{f421}'), (12, '\u{f46e}')]
    {
        assert!(d[l].starts_with(&format!("> {icon} ")), "{:?}", d[l]);
    }
    assert_eq!(d[0], "> \u{f449} Note");
    // the marker's line is raw where the cursor or a selection is
    assert_eq!(drawn(ALL, 0..1, true)[0], "> [!NOTE]");
    assert_eq!(drawn(ALL, 3..4, false)[3], "> [!tip]");
    // a quote that is no callout is drawn as it was
    let plain = "> [!FOO]\n> body\n\n> [!NOTE] text\n> more\n";
    assert_eq!(drawn(plain, 99..99, true), plain.lines().collect::<Vec<_>>());
}

#[test]
fn md_11_a_plan_leaves_out_only_the_chars_of_a_marker_that_is_exactly_a_kinds() {
    // MD-2 holds: what is left out passes through `may_hide`, and a callout's reason takes the
    // bracket, the bang and the letters of a marker and nothing else
    for c in ['[', '!', ']', 'N', 'o', 'z'] {
        assert!(may_hide(c, Why::Callout), "{c:?}");
    }
    for c in [' ', '>', '*', '`', '-', '\u{202e}', '\u{200b}', '日', '\x1b', '\u{feff}', '(', '|']
    {
        assert!(!may_hide(c, Why::Callout), "{c:?}");
    }
    let lines: Vec<String> = ALL.lines().map(str::to_string).collect();
    let sp = spans_of(&lines);
    let at = |l: usize| lines.get(l).cloned();
    let mut read = Read::new(0..lines.len(), 99..99, 80, &at, &|l| sp.get(l));
    read.calls = callouts(0..lines.len(), &at, &|l| sp.get(l));
    for b in &read.calls.clone() {
        let cs: Vec<char> = lines[b.lines.start].chars().collect();
        let plan = read.plan(b.lines.start, &cs, &sp[b.lines.start]).unwrap();
        for (i, c) in cs.iter().enumerate() {
            assert_eq!(plan.op(i).hide, b.marker.contains(&i), "{c:?} at {i} of {:?}", cs);
        }
        // the marker is the kind's: seven chars for Note, and the word never longer than what it replaces
        assert!(b.kind.word().len() + 2 <= b.marker.len(), "{:?}", b.kind);
    }
    // hostile chars beside a marker: a marker with one is no marker, and nothing is hidden
    for hostile in ["[!NO\u{202e}TE]", "[!NOTE\u{200b}]", "[!\u{feff}NOTE]", "[!NOTE\x1b]"] {
        let text = format!("> {hostile}\n> body\n");
        assert_eq!(found(&text), [], "{hostile:?}");
        assert_eq!(drawn(&text, 99..99, false)[0], format!("> {hostile}"));
    }
}

// ---- the screens ----

const DOC: &str = "# Notes\n\n> [!NOTE]\n> a note\n\n> [!TIP]\n> a tip\n\n> [!IMPORTANT]\n> an important thing\n\n> [!WARNING]\n> a warning\n\n> [!CAUTION]\n> a caution\n\n> [!FOO]\n> a plain quote\n\n> [!NOTE]\n> second note\n";

fn rig(name: &str, skin: Skin, syn: Syntax, icons: bool) -> Rig {
    let file = format!("README.md={DOC}");
    let mut r = rig_env(name, &[&file], &["README.md"], (80, 44), |e| {
        (e.skin, e.syn, e.md, e.icons, e.chrome) = (skin, syn, true, icons, skin != Skin::Calm);
    });
    if !syn.mono {
        r.send(SimEvent::Term(Event::Bg(BG)));
    }
    r.hold_spans = true;
    r.keys("l");
    r.deliver_spans();
    r
}

/// The row that has `what`, its text after the gutter, and its resolved cells.
fn row_of(r: &Rig, what: &str) -> (usize, Vec<sim::paint::Resolved>) {
    let rows: Vec<&str> = r.shot.frame.rows.iter().map(|x| x.text.as_str()).collect();
    let y = rows.iter().position(|x| x.contains(what));
    let y = y.unwrap_or(usize::MAX);
    assert!(y != usize::MAX, "{what:?} not on the screen:\n{}", rows.join("\n"));
    (y, Resolver::new().row(&r.shot, y))
}

/// The column of the first char of `what` in the row `y`.
fn col_of(r: &Rig, y: usize, what: &str) -> usize {
    let t = &r.shot.frame.rows[y].text;
    width(&t[..t.find(what).unwrap()])
}

#[test]
fn md_11_in_flat_and_depth_a_callout_has_no_hue_a_bold_word_and_a_bar_a_gray_brighter_than_a_quote_s()
 {
    let p = palette(BG);
    for skin in [Skin::Flat, Skin::Depth] {
        let r = rig("md11-hue", skin, Syntax::new(true), false);
        for word in ["Note", "Tip", "Important", "Warning", "Caution"] {
            let (y, cells) = row_of(&r, &format!("▌ {word}"));
            let x = col_of(&r, y, word);
            for k in 0..word.len() {
                assert_eq!(
                    cells[x + k].fg,
                    Some(p.primary),
                    "{skin:?}: {word}: {k}: the text's gray"
                );
                assert!(cells[x + k].attrs.bold(), "{skin:?}: {word} is bold");
            }
            // every kind is drawn alike: the kind is told by its word (and its icon), and by no hue
            assert_eq!(cells[x - 2].fg, Some(p.primary), "{skin:?}: {word}: the bar");
            assert!(cells[..78].iter().all(|c| c.bg.is_none()), "{skin:?}: {word}: a background");
        }
        // a body line: the bar of the block it is in, and the text is the text's own
        let (y, cells) = row_of(&r, "▌ an important thing");
        let x = col_of(&r, y, "an important");
        assert_eq!(cells[x - 2].fg, Some(p.primary), "{skin:?}: the body's bar");
        assert_eq!(cells[x].fg, Some(p.primary), "{skin:?}: the body is not tinted");
        assert!(cells[..78].iter().all(|c| c.bg.is_none()), "{skin:?}: the body's row");
        // a quote that is none keeps the secondary gray, so the two are told apart by rank
        for what in ["▌ a plain quote", "▌ [!FOO]"] {
            let (y, cells) = row_of(&r, what);
            let bar = cells[col_of(&r, y, what.trim_start_matches("▌ ")) - 2].fg;
            assert_eq!(bar, Some(p.secondary), "{skin:?}: {what}: a plain quote");
            assert!(cells.iter().all(|c| !c.attrs.bold()), "{skin:?}: {what} is not a word");
        }
    }
}

#[test]
fn md_11_the_cursor_s_line_is_the_files_own_with_the_marker_in_bold() {
    let p = palette(BG);
    for skin in [Skin::Flat, Skin::Depth, Skin::Calm] {
        let mut r = rig("md11-raw", skin, Syntax::new(true), false);
        r.keys("jj"); // the line `> [!NOTE]`
        let (y, cells) = row_of(&r, "[!NOTE]");
        let x = col_of(&r, y, "[!NOTE]");
        for k in 0..7 {
            assert!(cells[x + k].attrs.bold(), "{skin:?}: raw marker: {k}");
            assert_eq!(cells[x + k].fg, Some(p.primary), "{skin:?}: no hue: {k}");
        }
        assert!(!r.shot.frame.rows[y].text.contains("Note"), "{skin:?}: the marker, not the word");
    }
}

#[test]
fn md_11_calm_has_the_word_in_bold_and_no_hue_and_no_background() {
    let r = rig("md11-calm", Skin::Calm, Syntax::new(true), false);
    let p = palette(BG);
    for word in ["Note", "Tip", "Important", "Warning", "Caution"] {
        let (y, cells) = row_of(&r, &format!("> {word}"));
        let x = col_of(&r, y, word);
        for k in 0..word.len() {
            assert!(cells[x + k].attrs.bold(), "{word}: {k}");
            assert_eq!(cells[x + k].fg, Some(p.primary), "{word}: the text's own gray");
        }
        assert!(cells.iter().all(|c| c.bg.is_none()), "{word}: P3, no background");
        // the bar is the quote's own: the secondary gray, whatever the kind
        assert_eq!(cells[x - 2].fg, Some(p.secondary), "{word}: the bar");
    }
}

#[test]
fn md_11_without_color_the_word_is_bold_and_nothing_names_a_color() {
    for skin in [Skin::Calm, Skin::Flat] {
        let r = rig("md11-mono", skin, Syntax::mono(), false);
        let mut p = Painter::new(Vec::<u8>::new());
        p.draw(r.shot.clone()).unwrap();
        let out = String::from_utf8(p.out).unwrap();
        for word in ["Note", "Tip", "Important", "Warning", "Caution"] {
            assert!(out.contains(word), "{skin:?}: {word}");
            let before =
                &out[..out.find(&format!("{word}\x1b")).unwrap_or_else(|| out.find(word).unwrap())];
            let sgr = before.rsplit("\x1b[").next().unwrap();
            assert!(
                sgr.starts_with("0") && sgr.contains(";1"),
                "{skin:?}: {word} is bold: {sgr:?}"
            );
        }
        let colors = out
            .split("\x1b[")
            .skip(1)
            .filter_map(|s| s.split_once('m'))
            .flat_map(|(p, _)| {
                p.split(';').filter_map(|n| n.parse::<u16>().ok()).collect::<Vec<_>>()
            })
            .filter(|n| matches!(n, 30..=48 | 90..=107))
            .count();
        assert_eq!(colors, 0, "{skin:?}: COLOR-2");
    }
}

#[test]
fn md_11_the_icon_is_there_with_zi_and_not_without() {
    for skin in [Skin::Flat, Skin::Calm] {
        let mut r = rig("md11-icon", skin, Syntax::new(true), true);
        let rows =
            |r: &Rig| -> Vec<String> { r.shot.frame.rows.iter().map(|x| x.text.clone()).collect() };
        for (word, icon) in [("Note", '\u{f449}'), ("Tip", '\u{f400}'), ("Warning", '\u{f421}')] {
            let want = format!("{icon} {word}");
            assert!(rows(&r).iter().any(|x| x.contains(&want)), "{skin:?}: {want:?}");
        }
        r.keys("zi");
        let off = rows(&r);
        assert!(
            !off.iter().any(|x| x.chars().any(|c| ('\u{e000}'..='\u{f8ff}').contains(&c))),
            "{skin:?}: zi off: no private-use char"
        );
        assert!(
            off.iter().any(|x| x.contains("> Note") || x.contains("▌ Note")),
            "{skin:?}: the word stays"
        );
    }
}
