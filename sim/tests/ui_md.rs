//! UI-8: markdown drawn as render-markdown draws it, with every char of the file still on the
//! screen: the looks the highlighter makes of its scopes (pure), and how a Screen is made of them.
#![allow(clippy::unwrap_used)] // helpers; the tests themselves may unwrap
mod common;
use common::*;
use sim::color::{Syntax, Tones};
use sim::layout::Skin;
use sim::paint::{Look, Paint, Painter, Screen};
use sim::sim::SimEvent;
use sim::syn::{Spans, Syn};
use sim_core::core::app::Event;
use sim_core::core::color::{Rgb, palette};

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

/// Whether the first `what` of `out` is drawn bold: the attributes of its sequence (single digits,
/// ahead of the two-digit colors) have a 1.
fn is_bold(out: &str, what: &str) -> bool {
    before(out, what).iter().skip(1).take_while(|p| p.len() == 1).any(|p| *p == "1")
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

const LEVELS: &str =
    "# One\n\n## Two\n\n### Three\n\n#### Four\n\n##### Five\n\n###### Six\n\ntext\n";
const WORDS: [&str; 6] = ["One", "Two", "Three", "Four", "Five", "Six"];

/// The column (of the screen) where `needle` first is in a row's text.
fn col(text: &str, needle: &str) -> u16 {
    text[..text.find(needle).unwrap()].chars().count() as u16
}

/// The first background of a cell's paints (the row's band is behind the text's own paints).
fn bg_at(r: &Rig, y: usize, x: u16) -> Option<Rgb> {
    r.shot.paints[y].iter().find(|(c, p)| c.contains(&x) && p.bg.is_some()).and_then(|p| p.1.bg)
}

/// The first foreground of a cell's paints (the fog's paint, which has none, may come first).
fn fg_at(r: &Rig, y: usize, x: u16) -> Option<Rgb> {
    r.shot.paints[y].iter().find(|(c, p)| c.contains(&x) && p.fg.is_some()).and_then(|p| p.1.fg)
}

/// The row of a heading `hashes #` + `word`, the column of its word and the one of its hashes.
fn heading(r: &Rig, hashes: usize, word: &str) -> (usize, u16, u16) {
    let y = row_of(r, &format!("{} {word}", "#".repeat(hashes)));
    let text = &r.shot.frame.rows[y].text;
    (y, col(text, word), col(text, "#"))
}

#[test]
fn ui_8_every_heading_is_in_the_heading_hue_and_its_hashes_stay_dim() {
    for skin in [Skin::Flat, Skin::Depth] {
        let r = shown("md-hue", skin, LEVELS);
        let t = Tones::new(&palette(BG));
        for (n, word) in WORDS.iter().enumerate() {
            let (y, x, hash) = heading(&r, n + 1, word);
            let level = n + 1;
            assert_eq!(fg_at(&r, y, x), Some(t.head), "{skin:?} level {level}: the hue");
            assert_eq!(fg_at(&r, y, hash), None, "{skin:?} level {level}: a hash is dim");
            assert_eq!(look_at(&r, y, hash), Some(Look::Mark));
        }
        assert_eq!(fg_at(&r, row_of(&r, "text"), 8), None, "{skin:?}: prose has no hue");
    }
}

#[test]
fn ui_8_levels_1_to_3_are_bold_and_4_to_6_are_not_and_calm_is_as_it_was() {
    for skin in [Skin::Flat, Skin::Depth] {
        let r = shown("md-bold", skin, LEVELS);
        let out = draw(r.shot.clone());
        let want = [true, true, true, false, false, false];
        for (word, bold) in WORDS.iter().zip(want) {
            assert_eq!(is_bold(&out, word), bold, "{skin:?} {word}: {:?}", before(&out, word));
        }
        // the levels are told apart by the looks: 1 to 3 as they are, 4 to 6 as `Sub`
        let looks: Vec<_> = WORDS
            .iter()
            .enumerate()
            .map(|(n, w)| {
                let (y, x, _) = heading(&r, n + 1, w);
                look_at(&r, y, x).unwrap()
            })
            .collect();
        let sub = [Look::H1, Look::H2, Look::H3, Look::Sub, Look::Sub, Look::Sub];
        assert_eq!(looks, sub, "{skin:?}");
    }
    // calm: every level bold, no hue, no band, the looks of the highlighter
    let r = shown("md-bold-calm", Skin::Calm, LEVELS);
    let out = draw(r.shot.clone());
    for (n, word) in WORDS.iter().enumerate() {
        assert!(is_bold(&out, word), "calm {word}: {:?}", before(&out, word));
        let (y, x, _) = heading(&r, n + 1, word);
        let level = [Look::H1, Look::H2, Look::H3, Look::H4, Look::H5, Look::H6][n];
        assert_eq!(look_at(&r, y, x), Some(level), "calm keeps the highlighter's looks");
    }
    assert!(r.shot.paints.iter().all(Vec::is_empty), "calm draws no hue and no band");
}

#[test]
fn ui_8_a_first_level_heading_is_on_a_band_of_the_heading_hue_and_a_second_level_one_on_a_fainter_one()
 {
    for skin in [Skin::Flat, Skin::Depth] {
        let r = shown("md-bands", skin, LEVELS);
        let t = Tones::new(&palette(BG));
        let w = r.shot.frame.width;
        let (one, two) = (heading(&r, 1, "One").0, heading(&r, 2, "Two").0);
        for (y, band, name) in [(one, t.head1, "H1"), (two, t.head2, "H2")] {
            assert_eq!(bg_at(&r, y, 8), Some(band), "{skin:?}: {name}'s band");
            assert_eq!(bg_at(&r, y, w - 3), Some(band), "{skin:?}: {name}'s band to the edge");
        }
        assert_ne!(t.head1, t.head2, "the first is the stronger");
        let (page, lch) = (sim::color::to_lch(BG), |c| sim::color::to_lch(c));
        assert!((lch(t.head1)[0] - page[0]).abs() > (lch(t.head2)[0] - page[0]).abs());
        // the bands are of the heading's hue: mixed into the page, never gray
        for band in [t.head1, t.head2] {
            assert!(lch(band)[1] > page[1] + 0.005, "{skin:?}: {band:?} has a hue");
        }
        for (hashes, word) in [(3, "Three"), (4, "Four"), (5, "Five"), (6, "Six")] {
            let y = heading(&r, hashes, word).0;
            assert_eq!(bg_at(&r, y, 8), None, "{skin:?}: level {hashes} has no band");
        }
        assert_eq!(bg_at(&r, row_of(&r, "text"), 8), None);
        if one > 0 {
            assert_eq!(bg_at(&r, one - 1, 8), None, "not above");
        }
    }
}

#[test]
fn ui_8_a_link_or_code_in_a_heading_keeps_its_own_color() {
    for skin in [Skin::Flat, Skin::Depth] {
        let r = shown("md-hlink", skin, "## Head [lnk](http://x.y) and `cod`\n");
        let t = Tones::new(&palette(BG));
        let y = row_of(&r, "## Head");
        let text = &r.shot.frame.rows[y].text;
        let (head, link, code) = (col(text, "Head"), col(text, "lnk"), col(text, "cod"));
        assert_eq!(fg_at(&r, y, head), Some(t.head));
        assert_eq!(fg_at(&r, y, link), Some(t.accent_fg()), "{skin:?}: a link is the accent's");
        assert_eq!(fg_at(&r, y, code), None, "{skin:?}: code is neutral");
        assert_eq!(bg_at(&r, y, code), Some(t.code), "{skin:?}: on its surface, over the band");
    }
}

// ---- tables ----

const TABLE: &str = "| a | b |\n|---|:--:|\n| 1 | 2 |\n| 日本 | x |\n\ntext | not a table\n";

#[test]
fn ui_8_the_delimiter_row_of_a_table_is_a_marker_from_end_to_end() {
    let all = spans_of(TABLE);
    // `|---|:--:|`: the pipes, the dashes and the colons are all dim markers
    let delim: Vec<char> = "|---|:--:|".chars().collect();
    let mut marked = vec![false; delim.len()];
    for (r, l) in &all[1] {
        assert_eq!(*l, Look::Mark, "{:?}", all[1]);
        marked[r.clone()].fill(true);
    }
    assert!(marked.iter().all(|m| *m), "{:?}", all[1]);
    // a cell's text is not
    assert!(
        all[2].iter().all(|(r, _)| r.len() == 1),
        "only the pipes of a row of cells: {:?}",
        all[2]
    );
}

#[test]
fn ui_8_a_table_has_bars_for_its_pipes_and_a_grid_for_its_delimiter_row() {
    for skin in [Skin::Flat, Skin::Depth] {
        let r = shown("md-table", skin, TABLE);
        let (head, delim, cells, wide) = (
            row_of(&r, "│ a │ b │"),
            row_of(&r, "├───┼:──:┤"),
            row_of(&r, "│ 1 │ 2 │"),
            row_of(&r, "│ 日本 │ x │"),
        );
        assert_eq!((delim, cells, wide), (head + 1, head + 2, head + 3), "{skin:?}: in order");
        // the glyphs stand in the cells of the chars they stand for: the file's chars are the file's
        assert_eq!(r.text(), TABLE.trim_end_matches('\n'));
        // in the secondary gray: the markers' look, which the Painter draws dim
        let x = col(&r.shot.frame.rows[delim].text, "├");
        for dx in 0..10 {
            assert_eq!(look_at(&r, delim, x + dx), Some(Look::Mark), "{skin:?}: cell {dx}");
            assert_eq!(fg_at(&r, delim, x + dx), None, "{skin:?}: a gray, no hue of its own");
        }
        // a pipe in prose is a pipe
        assert!(r.shot.frame.rows[row_of(&r, "text |")].text.contains("text | not"));
    }
}

#[test]
fn ui_8_calm_and_code_keep_their_pipes() {
    let r = shown("md-table-calm", Skin::Calm, TABLE);
    assert!(r.shot.frame.rows.iter().any(|x| x.text.contains("|---|:--:|")), "calm draws no glyph");
    // a pipe at the start of a line of code is not a table's: nothing is a marker there
    let file = "x.sh=cat a \\\n  | grep b\n";
    let mut r = rig_env("md-pipe", &[file], &["x.sh"], (60, 10), |e| {
        e.skin = Skin::Flat;
        e.syn = Syntax::new(true);
    });
    r.send(SimEvent::Term(Event::Bg(BG)));
    r.keys("l");
    assert!(r.screen.rows.iter().any(|x| x.text.contains("| grep b")), "{:?}", r.screen.rows);
    assert!(!r.screen.rows.iter().any(|x| x.text.contains('│')));
}

// ---- task lists ----

const TASKS: &str = "- [x] done `c` and [l](u)\n* [X] Done\n- [ ] todo\n1. [x] numbered done\n  - [x] nested\n- [x]\n- [x] \n+ [x]no blank\n\n```\n- [x] in a fence\n```\n";

#[test]
fn ui_8_the_text_of_a_done_task_is_done_and_the_rest_of_the_list_is_as_it_was() {
    let all = spans_of(TASKS);
    let done = |n: usize| -> Vec<String> {
        let line = TASKS.lines().nth(n).unwrap();
        pieces(line, &all[n]).into_iter().filter(|p| p.1 == Look::Done).map(|p| p.0).collect()
    };
    // the gaps between the looks of the text (code, a link) are done; those looks stay what they were
    assert_eq!(done(0), ["done ", " and "]);
    assert!(
        pieces(TASKS.lines().next().unwrap(), &all[0])
            .iter()
            .any(|(t, l)| t == "c" && *l == Look::Code)
    );
    assert_eq!(done(1), ["Done"], "`* [X]`");
    assert!(done(2).is_empty(), "`[ ]` is as it was");
    assert_eq!(done(3), ["numbered done"], "a number is a bullet too");
    assert_eq!(done(4), ["nested"], "and so is a nested one");
    assert!(done(5).is_empty() && done(6).is_empty(), "no text, nothing to do");
    assert!(done(7).is_empty(), "the box needs a blank after it");
    assert!(done(10).is_empty(), "code in a fence is code");
}

#[test]
fn ui_8_a_done_task_is_drawn_in_the_secondary_gray_in_every_look() {
    for skin in [Skin::Calm, Skin::Flat, Skin::Depth] {
        let r = shown("md-done", skin, "- [x] finished work\n- [ ] open work\n");
        let out = draw(r.shot.clone());
        let p = palette(BG);
        let gray = format!("38;2;{};{};{}", p.secondary.0, p.secondary.1, p.secondary.2);
        assert!(
            before(&out, "finished").join(";").contains(&gray),
            "{skin:?}: {:?}",
            before(&out, "finished")
        );
        assert!(
            !before(&out, "open work").join(";").contains(&gray),
            "{skin:?}: {:?}",
            before(&out, "open work")
        );
    }
}

// ---- thematic breaks ----

const BREAKS: &str = "intro\n\n---\n\nmid\n\n***\n\n___\n\n- - -\n\nend\n\n```\n---\n```\n";

#[test]
fn ui_8_a_thematic_break_is_a_rule_except_in_a_fence_and_at_the_top_of_a_file() {
    let all = spans_of(BREAKS);
    let rule = |n: usize| all[n].iter().any(|(_, l)| *l == Look::Rule);
    for n in [2, 6, 8, 10] {
        assert!(rule(n), "line {}: {:?}", n + 1, all[n]);
        let width = BREAKS.lines().nth(n).unwrap().chars().count();
        assert_eq!(
            all[n].iter().filter(|s| s.1 == Look::Rule).map(|s| s.0.len()).sum::<usize>(),
            width
        );
    }
    assert!(!rule(15), "a `---` in a fenced block is code: {:?}", all[15]);
    assert!((0..all.len()).filter(|&n| ![2, 6, 8, 10].contains(&n)).all(|n| !rule(n)));
    // the first line of a file opens YAML front matter; what closes it reads as a setext heading
    let front = spans_of("---\ntitle: x\n---\nbody\n\n---\n");
    assert!(front.iter().take(3).all(|s| s.iter().all(|(_, l)| *l != Look::Rule)), "{front:?}");
    assert!(
        front[5].iter().any(|(_, l)| *l == Look::Rule),
        "a later one is a break: {:?}",
        front[5]
    );
}

#[test]
fn ui_8_a_thematic_break_is_drawn_as_a_line_across_the_pane_in_the_secondary_gray() {
    for skin in [Skin::Flat, Skin::Depth] {
        let r = shown("md-rule", skin, BREAKS);
        let (g, t) = (r.sim.geo(), Tones::new(&palette(BG)));
        let (from, to) = (usize::from(g.tx()) + 5, usize::from(g.tx() + g.ew));
        let body = usize::from(g.y0)..usize::from(g.y0 + g.rows);
        let rules: Vec<usize> = body
            .filter(|&y| {
                let cs: Vec<char> = r.shot.frame.rows[y].text.chars().collect();
                cs.len() >= to && cs[from..to].iter().all(|c| *c == '─')
            })
            .collect();
        assert_eq!(
            rules.len(),
            4,
            "{skin:?}: four breaks across the pane:\n{}",
            dump_screen(&r.shot)
        );
        for &y in &rules {
            for x in [from as u16, to as u16 - 1] {
                assert_eq!(fg_at(&r, y, x), Some(t.focus), "{skin:?}: the secondary gray at {x}");
            }
        }
        assert_eq!(r.text(), BREAKS.trim_end_matches('\n'), "the file's chars are the file's");
    }
}

#[test]
fn ui_8_calm_shows_a_break_as_the_chars_it_has_in_the_secondary_gray() {
    let r = shown("md-rule-calm", Skin::Calm, "intro\n\n---\n");
    let out = draw(r.shot.clone());
    let p = palette(BG);
    let gray = format!("38;2;{};{};{}", p.secondary.0, p.secondary.1, p.secondary.2);
    assert!(r.shot.frame.rows.iter().any(|x| x.text.contains("---")));
    assert!(before(&out, "---").join(";").contains(&gray), "{:?}", before(&out, "---"));
}
