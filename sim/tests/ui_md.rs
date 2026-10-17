//! UI-8: markdown drawn as render-markdown draws it, with every char of the file still on the
//! screen: the looks the highlighter makes of its scopes (pure), and how a Screen is made of them.
#![allow(clippy::unwrap_used)] // helpers; the tests themselves may unwrap
mod common;
use common::*;
use si::core::app::Event;
use si::core::color::{Rgb, palette};
use sim::color::{Syntax, Tones};
use sim::layout::Skin;
use sim::paint::{Look, Paint, Painter, Screen};
use sim::sim::SimEvent;
use sim::syn::{Spans, Syn};

const BG: Rgb = (0x1e, 0x1e, 0x1e);

fn spans_of(text: &str) -> Vec<Spans> {
    let lines: Vec<String> = text.lines().map(str::to_string).collect();
    let mut syn = Syn::new("a.md", "").unwrap();
    syn.spans(&|i| lines.get(i).cloned(), 0, lines.len())
}

/// The looks of a line as `(text, look)`, in order.
fn pieces(line: &str, spans: &Spans) -> Vec<(String, Look)> {
    let cs: Vec<char> = line.chars().collect();
    spans
        .iter()
        .filter(|(r, _)| r.start < r.end)
        .map(|(r, l)| (cs[r.clone()].iter().collect(), *l))
        .collect()
}

#[test]
fn ui_8_headings_have_their_level_and_their_markers_are_dim_and_still_there() {
    let line = "## Head *one* `code` [link](http://x.y) **bold**";
    let all = spans_of(line);
    let p = pieces(line, &all[0]);
    let has = |s: &str, l: Look| p.iter().any(|(t, k)| t == s && *k == l);
    assert!(has("##", Look::Mark), "{p:?}: the hashes are a marker");
    assert!(
        p.iter().any(|(t, k)| t.contains("Head") && *k == Look::H2),
        "{p:?}: the title has its level's look"
    );
    assert!(has("*", Look::Mark), "{p:?}: emphasis markers are dim");
    assert!(has("one", Look::H2), "{p:?}: emphasis inside a heading keeps its look");
    assert!(has("`", Look::Tick) && has("code", Look::Code), "{p:?}: inline code");
    assert!(has("link", Look::Link) && has("http://x.y", Look::Url), "{p:?}: a link and its url");
    assert!(has("[", Look::Mark) && has("](", Look::Mark) && has(")", Look::Mark), "{p:?}");
    assert!(
        has("**", Look::Mark) && has("bold", Look::H2),
        "{p:?}: strong inside a heading keeps the heading's look"
    );
    // Nothing is hidden: the looks cover the whole line, and the file's chars are all in it.
    let covered: usize = all[0].iter().map(|(r, _)| r.len()).sum();
    assert!(
        covered >= line.chars().count() - 6,
        "the line is covered: {covered} of {}",
        line.chars().count()
    );
}

#[test]
fn ui_8_each_of_the_six_levels_has_its_own_look() {
    let text = "# a\n## b\n### c\n#### d\n##### e\n###### f\n";
    let all = spans_of(text);
    let levels = [Look::H1, Look::H2, Look::H3, Look::H4, Look::H5, Look::H6];
    for (i, l) in levels.into_iter().enumerate() {
        assert!(all[i].iter().any(|(_, k)| *k == l), "line {i}: {:?}", all[i]);
    }
}

#[test]
fn ui_8_bullets_quotes_and_strong_text() {
    let text = "- item\n* star\n1. num\n\n> quote\n\nplain **strong** and *em*\n";
    let all = spans_of(text);
    let line = |i: usize| pieces(text.lines().nth(i).unwrap(), &all[i]);
    assert!(line(0).contains(&("-".into(), Look::Bullet)), "{:?}", line(0));
    assert!(line(1).contains(&("*".into(), Look::Bullet)), "{:?}", line(1));
    assert!(line(2).iter().any(|(t, l)| t.starts_with('1') && *l == Look::Bullet), "{:?}", line(2));
    assert!(line(4).contains(&(">".into(), Look::Quote)), "{:?}", line(4));
    assert!(line(6).contains(&("strong".into(), Look::Bold)), "{:?}", line(6));
    assert!(line(6).contains(&("em".into(), Look::Em)), "{:?}", line(6));
}

#[test]
fn ui_8_a_fenced_block_is_banded_from_its_first_fence_to_its_last_and_names_its_language() {
    let text = "text\n```bash\nls -l\n\n# c\n```\nafter\n```\nplain\n```\n";
    let all = spans_of(text);
    let band = |i: usize| all[i].first() == Some(&(0..0, Look::Band));
    assert_eq!(
        (0..10).map(band).collect::<Vec<_>>(),
        [false, true, true, true, true, true, false, true, true, true]
    );
    let open = pieces("```bash", &all[1]);
    assert!(open.contains(&("```".into(), Look::Mark)), "{open:?}");
    assert!(open.contains(&("bash".into(), Look::Lang)), "{open:?}");
    // The code in it has the highlighting of its language.
    assert!(all[2].iter().any(|(_, l)| *l == Look::Fun), "ls is a command: {:?}", all[2]);
    assert!(all[4].iter().any(|(_, l)| *l == Look::Com), "{:?}", all[4]);
    assert_eq!(all[3], [(0..0, Look::Band)], "a blank line in a block carries the band alone");
}

// ---- the Screen ----

/// `text` open as README.md, with the highlighter's answer in, in `skin`, a palette and truecolor.
fn shown(name: &str, skin: Skin, text: &str) -> Rig {
    let file = format!("README.md={text}");
    let mut r = rig_env(name, &[&file], &["README.md"], (60, 16), |e| {
        e.skin = skin;
        e.syn = Syntax::new(true);
    });
    r.send(SimEvent::Term(Event::Bg(BG)));
    r.hold_spans = true;
    r.keys("l"); // (a key: the highlighter is asked for the window)
    r.deliver_spans();
    r
}

fn row_of(r: &Rig, what: &str) -> usize {
    let y = r.shot.frame.rows.iter().position(|x| x.text.contains(what));
    assert!(y.is_some(), "no row with {what:?}:\n{}", dump_screen(&r.shot));
    y.unwrap()
}

fn paint_at(r: &Rig, y: usize, col: u16) -> Paint {
    r.shot.paints[y].iter().find(|(c, _)| c.contains(&col)).map(|x| x.1).unwrap_or_default()
}

fn look_at(r: &Rig, y: usize, col: u16) -> Option<Look> {
    r.shot.looks[y].iter().find(|(c, _)| c.contains(&col)).map(|x| x.1)
}

const DOC: &str = "# Title\n\nSome `code` and a [link](http://x.y) here.\n\n> quoted\n\n```bash\nls -l\n\necho hi\n```\n\n- item\n";

#[test]
fn ui_8_inline_code_is_on_a_surface_links_and_level_five_are_blue_in_flat_and_depth() {
    for skin in [Skin::Flat, Skin::Depth] {
        let r = shown("md-surface", skin, DOC);
        let t = Tones::new(&palette(BG));
        let y = row_of(&r, "Some `code`");
        let x = r.shot.frame.rows[y].text.find("code").unwrap() as u16;
        assert_eq!(
            paint_at(&r, y, x).bg,
            Some(t.code),
            "{skin:?}: the text of inline code is on the surface"
        );
        assert_eq!(paint_at(&r, y, x - 1).bg, Some(t.code), "{skin:?}: and so is its backtick");
        assert_eq!(paint_at(&r, y, 3).bg, None, "{skin:?}: not the prose");
        let l = r.shot.frame.rows[y].text.find("link").unwrap() as u16;
        assert_eq!(look_at(&r, y, l), Some(Look::Link));
        assert_eq!(paint_at(&r, y, l).fg, Some(t.accent_fg()), "{skin:?}: a link is the accent");
    }
}

#[test]
fn ui_8_calm_has_the_colors_of_the_looks_and_no_surface_no_blue_and_no_glyph() {
    let r = shown("md-calm", Skin::Calm, DOC);
    let y = row_of(&r, "Some `code`");
    let x = r.shot.frame.rows[y].text.find("code").unwrap() as u16;
    assert_eq!(look_at(&r, y, x), Some(Look::Code));
    assert!(r.shot.paints.iter().all(Vec::is_empty), "calm draws no tint and no surface");
    let q = row_of(&r, "quoted");
    assert!(
        r.shot.frame.rows[q].text.contains("> quoted"),
        "the quote's own char: {}",
        r.shot.frame.rows[q].text
    );
    let h = row_of(&r, "# Title");
    assert_eq!(look_at(&r, h, 6), Some(Look::H1));
}

#[test]
fn ui_8_a_fenced_block_has_a_surface_across_the_pane_and_the_language_at_the_right() {
    let r = shown("md-fence", Skin::Flat, DOC);
    let t = Tones::new(&palette(BG));
    let (open, code, blank, close) = (
        row_of(&r, "```bash"),
        row_of(&r, "ls -l"),
        row_of(&r, "ls -l") + 1,
        row_of(&r, "echo hi") + 1,
    );
    let w = r.shot.frame.width;
    for y in [open, code, blank, close] {
        assert_eq!(
            paint_at(&r, y, 8).bg,
            Some(t.band),
            "row {y} (a blank one too) has the surface"
        );
        assert_eq!(paint_at(&r, y, w - 2).bg, Some(t.band), "row {y}: to the right edge");
    }
    assert_eq!(paint_at(&r, open - 1, 8).bg, None, "not above");
    assert_eq!(paint_at(&r, close + 1, 8).bg, None, "not below");
    let row = &r.shot.frame.rows[open].text;
    assert!(row.contains("```bash"), "the fence is as in the file: {row}");
    assert!(
        row.trim_end().ends_with("bash") && row.trim_end().len() > "```bash".len() + 10,
        "the language at the right: {row:?}"
    );
    assert!(!r.shot.frame.rows[code].text.contains("bash"), "only the opening row names it");
    // The label is no char of the buffer: the text is the file's.
    assert_eq!(r.text(), DOC.trim_end_matches('\n'));
}

#[test]
fn ui_8_a_quote_is_drawn_as_a_bar_in_its_own_cell_and_nothing_else_is_hidden() {
    let r = shown("md-quote", Skin::Depth, DOC);
    let t = row_of(&r, "quoted");
    let text = &r.shot.frame.rows[t].text;
    assert!(text.contains("▌ quoted"), "{text}");
    let h = row_of(&r, "# Title");
    assert!(r.shot.frame.rows[h].text.contains("# Title"), "the hash stays");
    let c = row_of(&r, "Some `code`");
    assert!(
        r.shot.frame.rows[c].text.contains("`code`")
            && r.shot.frame.rows[c].text.contains("(http://x.y)"),
        "backticks and url stay"
    );
    let (g, tones) = (r.sim.widths(), Tones::new(&palette(BG)));
    let _ = (g, tones);
}

fn draw(s: Screen) -> String {
    let mut p = Painter::new(Vec::new());
    p.draw(s).unwrap();
    String::from_utf8(p.out).unwrap()
}

#[test]
fn ui_8_the_painter_draws_a_heading_bold_in_its_color_and_dims_the_markers() {
    let r = shown("md-sgr", Skin::Calm, "# Title\n");
    let out = draw(r.shot.clone());
    let y = row_of(&r, "# Title");
    let _ = y;
    let at = |c: &str| out.find(c).map(|i| &out[..i]).unwrap_or_default();
    let before_title = at("Title").rsplit("\x1b[").next().unwrap().to_string();
    let has = |s: &str, p: &str| s.trim_end_matches('m').split(';').any(|x| x == p);
    assert!(has(&before_title, "1"), "bold: {before_title}");
    assert!(before_title.contains("38;2;"), "in a color of its own: {before_title}");
    let before_hash = at("#").rsplit("\x1b[").next().unwrap().to_string();
    assert!(!has(&before_hash, "1"), "the marker is not bold: {before_hash}");
    let p = palette(BG);
    assert!(
        before_hash
            .contains(&format!("38;2;{};{};{}", p.secondary.0, p.secondary.1, p.secondary.2)),
        "the marker is the secondary gray: {before_hash}"
    );
}

#[test]
fn ui_8_a_link_is_underlined_and_the_fog_fades_a_markdown_color_too() {
    let r = shown("md-fade", Skin::Calm, "[l](u)\n");
    let out = draw(r.shot.clone());
    let before = out[..out.find('l').unwrap()].rsplit("\x1b[").next().unwrap().to_string();
    assert!(before.contains(";4"), "underlined: {before}");
    let _ = SimEvent::Notice(String::new());
    // A fade over a heading's color: the paint's own foreground is faded as the syntax colors are.
    let mut s = r.shot.clone();
    let half = Paint { fg: Some((250, 210, 210)), fade: 50, ..Paint::default() };
    s.paints = vec![vec![(0..40, half)]; s.frame.rows.len()];
    let faded = draw(s);
    assert!(
        !faded.contains("38;2;250;210;210"),
        "the paint's color is faded, not as it is: {faded:?}"
    );
}

/// The SGR sequences of `out`, as lists of parameters.
fn sequences(out: &str) -> Vec<Vec<&str>> {
    let mut all = vec![];
    let mut rest = out;
    while let Some(i) = rest.find("\x1b[") {
        rest = &rest[i + 2..];
        let end = rest.find(|c: char| c.is_ascii_alphabetic()).unwrap();
        if rest.as_bytes()[end] == b'm' {
            all.push(rest[..end].split(';').collect());
        }
        rest = &rest[end..];
    }
    all
}

/// The SGR sequence in force at the first `what` of `out`.
fn before<'a>(out: &'a str, what: &str) -> Vec<&'a str> {
    let i = out.find(what).unwrap();
    sequences(&out[..i]).pop().unwrap()
}

#[test]
fn ui_8_the_code_of_a_fenced_block_keeps_its_language_s_colors_on_the_band() {
    let text = "para\n\n```bash\nparu -S chezmoi # c\nbrew install x\n```\n";
    for skin in [Skin::Flat, Skin::Depth] {
        let r = shown("md-fence-colors", skin, text);
        let t = Tones::new(&palette(BG));
        let y = row_of(&r, "paru -S");
        let x = r.shot.frame.rows[y].text.find("paru").unwrap() as u16;
        assert_eq!(look_at(&r, y, x), Some(Look::Fun), "{skin:?}: paru is a command");
        assert_eq!(paint_at(&r, y, x).bg, Some(t.band), "{skin:?}: on the band");
        let out = draw(r.shot.clone());
        let sgr = before(&out, "paru").join(";");
        let hue = r.shot.syn.hue(Look::Fun).unwrap();
        assert!(sgr.contains(&format!("38;2;{};{};{}", hue.0, hue.1, hue.2)), "{skin:?}: {sgr}");
        assert!(
            sgr.contains(&format!("48;2;{};{};{}", t.band.0, t.band.1, t.band.2)),
            "{skin:?}: {sgr}"
        );
        let plain = before(&out, "-S chezmoi").join(";");
        assert_ne!(
            sgr.split(";48").next(),
            plain.split(";48").next(),
            "{skin:?}: a command is not drawn as the plain text"
        );
    }
}

#[test]
fn ui_8_no_sgr_sequence_is_longer_than_tmux_takes() {
    // tmux 3.7c drops a whole sequence of more than 20 parameters: the colors of the fenced block's
    // code were gone in the owner's tmux, while every other terminal drew them.
    let text = "# T\n\n`c` [l](u) **b**\n\n```bash\nparu -S x # c\n```\n\n> q\n\n- b\n";
    for skin in [Skin::Calm, Skin::Flat, Skin::Depth] {
        let mut r = shown("md-tmux", skin, text);
        r.keys("G"); // the cursor far from the block: the fog fades it
        let out = draw(r.shot.clone());
        let longest = sequences(&out).iter().map(Vec::len).max().unwrap();
        assert!(longest <= 19, "{skin:?}: a sequence of {longest} parameters");
    }
}

#[test]
fn ui_8_a_first_level_heading_is_on_a_faint_band_and_the_other_levels_are_bold_with_no_color() {
    for skin in [Skin::Flat, Skin::Depth] {
        let r = shown("md-h1", skin, "# One\n\n## Two\n\ntext\n");
        let t = Tones::new(&palette(BG));
        let (one, two, text) = (row_of(&r, "# One"), row_of(&r, "## Two"), row_of(&r, "text"));
        let w = r.shot.frame.width;
        assert_eq!(paint_at(&r, one, 8).bg, Some(t.band), "{skin:?}: H1's band");
        assert_eq!(paint_at(&r, one, w - 3).bg, Some(t.band), "{skin:?}: across the pane");
        assert_eq!(paint_at(&r, two, 8).bg, None, "{skin:?}: H2 has none");
        assert_eq!(paint_at(&r, text, 8).bg, None);
        // Bold in the text's own color: no hue of a level of its own.
        for y in [one, two] {
            let x = r.shot.frame.rows[y].text.find(['O', 'T']).unwrap() as u16;
            assert!(matches!(look_at(&r, y, x), Some(Look::H1 | Look::H2)), "{skin:?}");
            assert_eq!(paint_at(&r, y, x).fg, None, "{skin:?}: no color of its own");
        }
    }
    let calm = shown("md-h1-calm", Skin::Calm, "# One\n");
    assert!(calm.shot.paints.iter().all(Vec::is_empty), "calm draws no band");
}
