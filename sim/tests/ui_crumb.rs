//! UI-18: the breadcrumb `dir › file › section`: the section the cursor is in (a Markdown heading, or
//! the definition it is inside), how the parts are cut from the left, and where it is drawn.
#![allow(clippy::unwrap_used)] // helpers; the tests themselves may unwrap
mod common;
use common::*;
use ropey::Rope;
use sim::color::Syntax;
use sim::crumb::{cut, parts, section};
use sim::layout::Skin;
use sim::sim::SimEvent;
use sim_core::core::app::Event;
use sim_core::core::color::Rgb;

/// The section at line `cur` (0-based) of `text`, a file called `name`.
fn at(name: &str, text: &str, cur: usize) -> Option<String> {
    section(name, cur, &Rope::from_str(text), false, false)
}

#[test]
fn ui_18_a_markdown_section_is_the_nearest_heading_at_or_above_the_cursor() {
    let md = "# Title\n\ntext\n\n## Sub *a* ##\n\nmore\n\n### Deep\n";
    assert_eq!(at("a.md", md, 0).as_deref(), Some("Title"), "the heading's own line");
    assert_eq!(at("a.md", md, 2).as_deref(), Some("Title"));
    assert_eq!(at("a.md", md, 6).as_deref(), Some("Sub *a*"), "no #s, none at the end either");
    assert_eq!(at("a.md", md, 8).as_deref(), Some("Deep"));
    assert_eq!(at("a.md", "text only\n", 0), None, "no heading above: no section");
    assert_eq!(at("a.md", "#nospace\n", 0), None, "a heading has a blank after its #s");
    assert_eq!(at("a.md", "####### seven\n", 0), None, "and at most six");
}

#[test]
fn ui_18_a_hash_in_a_fenced_block_is_not_a_heading() {
    let md = "## Real\n\n```sh\n# a comment\necho hi\n```\n\nafter\n";
    let rope = Rope::from_str(md);
    let on = |cur: usize, fenced: bool, above: bool| section("a.md", cur, &rope, fenced, above);
    assert_eq!(on(4, true, true).as_deref(), Some("Real"), "inside the block");
    assert_eq!(on(7, false, true).as_deref(), Some("Real"), "after the block: its # was not one");
    assert_eq!(on(3, true, true).as_deref(), Some("Real"), "on the comment");
    // the cursor on a fence: the opening one has nothing fenced above it, the closing one has
    assert_eq!(on(2, true, false).as_deref(), Some("Real"), "on the opening fence");
    assert_eq!(on(5, true, true).as_deref(), Some("Real"), "on the closing fence");
}

#[test]
fn ui_18_a_section_of_code_is_the_definition_the_cursor_is_inside() {
    let rs = "use x;\n\nimpl<T> Foo<T> {\n    pub fn bar(&self) {\n        let x = 1;\n        y();\n    }\n\n    async fn baz() {}\n}\n\nfn free() {\n    if a {\n        b();\n    }\n}\n";
    assert_eq!(at("a.rs", rs, 4).as_deref(), Some("bar"), "in a method");
    assert_eq!(at("a.rs", rs, 3).as_deref(), Some("bar"), "on its own line");
    assert_eq!(at("a.rs", rs, 6).as_deref(), Some("bar"), "on the method's closing brace");
    assert_eq!(at("a.rs", rs, 7).as_deref(), Some("impl Foo<T>"), "between two methods");
    assert_eq!(at("a.rs", rs, 9).as_deref(), Some("impl Foo<T>"), "on the impl's closing brace");
    assert_eq!(at("a.rs", rs, 8).as_deref(), Some("baz"));
    assert_eq!(at("a.rs", rs, 13).as_deref(), Some("free"), "through the `if`'s own lines");
    assert_eq!(at("a.rs", rs, 0), None, "outside every definition");
    assert_eq!(at("a.rs", rs, 10), None, "after the impl");
    let py = "import os\n\nclass A:\n    def m(self):\n        return 1\n\n    @property\n    async def n(self):\n        pass\n";
    assert_eq!(at("a.py", py, 4).as_deref(), Some("m"));
    assert_eq!(at("a.py", py, 8).as_deref(), Some("n"));
    assert_eq!(at("a.py", py, 5).as_deref(), Some("A"), "a blank line inside the class");
    let js = "export default async function foo(a) {\n  const f = () => {\n    return 1;\n  };\n}\nclass K {\n  m() {\n    x;\n  }\n}\n";
    assert_eq!(at("a.js", js, 2).as_deref(), Some("foo"));
    assert_eq!(at("a.ts", js, 7).as_deref(), Some("m"), "a method");
    assert_eq!(at("a.ts", js, 8).as_deref(), Some("m"), "its closing brace");
    assert_eq!(at("a.ts", js, 9).as_deref(), Some("K"), "the class's");
    let go = "package p\n\nfunc (s *S) Run() {\n\tdo()\n}\n\ntype T struct {\n\tx int\n}\n";
    assert_eq!(at("a.go", go, 3).as_deref(), Some("Run"));
    assert_eq!(at("a.go", go, 7).as_deref(), Some("T"));
    let rb = "module M\n  class C\n    def self.run\n      x\n    end\n  end\nend\n";
    assert_eq!(at("a.rb", rb, 3).as_deref(), Some("self.run"));
    assert_eq!(at("a.rb", rb, 4).as_deref(), Some("self.run"), "the `end` of the method");
    assert_eq!(at("a.rb", rb, 5).as_deref(), Some("C"));
    let sh = "#!/bin/sh\nfoo() {\n  echo a\n}\nfunction bar {\n  echo b\n}\n";
    assert_eq!(at("run.sh", sh, 2).as_deref(), Some("foo"));
    assert_eq!(at("run.sh", sh, 5).as_deref(), Some("bar"));
    let lua = "local M = {}\nfunction M.go(a)\n  return a\nend\nlocal function h()\n  x()\nend\n";
    assert_eq!(at("a.lua", lua, 2).as_deref(), Some("M.go"));
    assert_eq!(at("a.lua", lua, 5).as_deref(), Some("h"));
}

#[test]
fn ui_18_a_file_of_another_kind_has_no_section_and_a_long_line_is_not_read_whole() {
    assert_eq!(at("a.txt", "fn a() {\n    x\n}\n", 1), None);
    assert_eq!(at("Makefile", "all:\n\tcc\n", 1), None);
    let long = format!("fn a() {{\n    {}\n}}\n", "x".repeat(100_000));
    assert_eq!(at("a.rs", &long, 1).as_deref(), Some("a"));
    // the scan stops 400 lines up: a definition further away is not found
    let far: String = std::iter::once("fn a() {\n".to_string())
        .chain((0..500).map(|_| "    x;\n".to_string()))
        .collect();
    assert_eq!(at("a.rs", &far, 450), None);
    assert_eq!(at("a.rs", &far, 300).as_deref(), Some("a"));
}

fn s(v: &[&str]) -> Vec<String> {
    v.iter().map(ToString::to_string).collect()
}

#[test]
fn ui_18_the_parts_are_the_path_components_and_the_section() {
    assert_eq!(
        parts("src/sim/guide.rs", Some("leader_key")),
        s(&["src", "sim", "guide.rs", "leader_key"])
    );
    assert_eq!(parts("README.md", None), s(&["README.md"]));
    assert_eq!(parts("./a//b.rs", None), s(&["a", "b.rs"]), "no empty part, no `.`");
    assert_eq!(parts("HEAD:src/a.rs", None), s(&["HEAD:src", "a.rs"]));
}

#[test]
fn ui_18_what_does_not_fit_is_cut_from_the_left_whole_components_first_the_section_last() {
    let p = s(&["src", "sim", "md.rs", "Read"]);
    let width = |v: &[String]| -> usize { v.join(" › ").chars().count() };
    assert_eq!(cut(&p, 99), p, "it all fits");
    assert_eq!(cut(&p, 24), p, "24 columns is just enough");
    assert_eq!(cut(&p, 23), s(&["…", "sim", "md.rs", "Read"]));
    assert_eq!(cut(&p, 18), s(&["…", "md.rs", "Read"]));
    assert_eq!(cut(&p, 10), s(&["…", "Read"]), "the file goes before the section");
    for max in 6..30 {
        assert!(width(&cut(&p, max)) <= max, "{max}: {:?}", cut(&p, max));
    }
    // the section alone is cut from its left too, with the ellipsis, as the tail of a query is
    let long = s(&["a", "very_long_function_name"]);
    assert_eq!(cut(&long, 8), vec![sim::layout::tail("very_long_function_name", 8)]);
    assert!(cut(&[], 9).is_empty());
    assert!(cut(&p, 0).is_empty());
}

// ---- where it is drawn ----

const BG: Rgb = (0x1e, 0x1e, 0x1e);

fn shown(name: &str, skin: Skin, size: (u16, u16), files: &[&str], open: &str, keys: &str) -> Rig {
    let mut r = rig_env(name, files, &[open], size, |e| {
        e.skin = skin;
        e.chrome = true;
        e.syn = Syntax::new(true);
    });
    r.send(SimEvent::Term(Event::Bg(BG)));
    r.hold_spans = true;
    r.keys("l");
    r.deliver_spans();
    r.keys(keys);
    r
}

const CODE: &str = "pub struct S;\n\nimpl S {\n    pub fn go(&self) {\n        run();\n    }\n}\n";

#[test]
fn ui_18_depth_has_the_breadcrumb_in_the_title_of_the_editor_s_box_and_it_follows_the_cursor() {
    let file = format!("src/lib.rs={CODE}");
    let mut r = shown("crumb-depth", Skin::Depth, (100, 20), &[&file], "src/lib.rs", "4j");
    let top = |r: &Rig| r.screen.rows[0].text.clone();
    assert!(top(&r).contains("src › lib.rs › go"), "{}", top(&r));
    r.keys("kk");
    assert!(top(&r).contains("src › lib.rs › impl S"), "{}", top(&r));
    r.keys("gg");
    assert!(top(&r).contains("src › lib.rs") && !top(&r).contains("› impl"), "{}", top(&r));
    assert!(!top(&r).chars().any(|c| "¹²".contains(c)), "no number (UI-16)");
}

#[test]
fn ui_18_the_badges_stay_after_the_breadcrumb() {
    let file = format!("src/lib.rs={CODE}");
    let mut r = shown("crumb-badge", Skin::Depth, (100, 20), &[&file], "src/lib.rs", "4j");
    let id = r.sim.bufs[0].id;
    r.send(SimEvent::App(id, disk(format!("{CODE}// more\n").as_bytes())));
    let top = r.screen.rows[0].text.clone();
    let (c, b) = (top.find("› go"), top.find("on disk"));
    assert!(c.is_some() && b.is_some() && c < b, "{top}");
}

#[test]
fn ui_18_flat_has_it_at_the_right_of_the_tab_bar_and_calm_none() {
    let file = format!("src/lib.rs={CODE}");
    let r = shown("crumb-flat", Skin::Flat, (100, 20), &[&file], "src/lib.rs", "4j");
    let tabs = r.screen.rows[0].text.clone();
    assert!(tabs.contains("lib.rs") && tabs.trim_end().ends_with("src › lib.rs › go"), "{tabs}");
    let r = shown("crumb-calm", Skin::Calm, (100, 20), &[&file], "src/lib.rs", "4j");
    assert!(!r.screen.rows.iter().any(|x| x.text.contains(" › ")), "{}", dump(&r.screen));
}

#[test]
fn ui_18_a_long_path_is_cut_from_the_left_with_an_ellipsis() {
    let file = "a/very/deep/tree/of/directories/with/long/names/lib.rs=fn f() {\n    x\n}\n";
    let r = shown(
        "crumb-long",
        Skin::Depth,
        (70, 20),
        &[file],
        "a/very/deep/tree/of/directories/with/long/names/lib.rs",
        "j",
    );
    let top = r.screen.rows[0].text.clone();
    assert!(top.contains("… › ") && top.contains("lib.rs › f"), "{top}");
    assert!(!top.contains("a › very"), "{top}");
}

#[test]
fn ui_18_a_markdown_file_names_its_heading_and_a_fence_does_not_make_one() {
    let md = "# Guide\n\n## Install\n\n```sh\n# comment\nbrew install x\n```\n";
    let file = format!("docs/guide.md={md}");
    let r = shown("crumb-md", Skin::Depth, (100, 20), &[&file], "docs/guide.md", "6j");
    let top = r.screen.rows[0].text.clone();
    assert!(top.contains("docs › guide.md › Install"), "{top}");
}

// ---- SAFE-8: what the breadcrumb is made of can hold anything ----

/// Whether a char must not reach the terminal as it is (SAFE-8): a control (C0, C1, DEL), a
/// bidirectional one, a zero-width one.
fn hostile(c: char) -> bool {
    c.is_control()
        || matches!(c, '\u{61c}' | '\u{200b}' | '\u{200e}'..='\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}' | '\u{feff}')
}

const EVIL: &str = "d\u{1b}[2J\u{9b}\u{202e}x\n\u{7f}";
const HEAD: &str = "T\u{1b}]0;x\u{7}\u{200b}";

fn clean(rows: &[String]) {
    for (y, r) in rows.iter().enumerate() {
        let bad: Vec<char> = r.chars().filter(|&c| hostile(c)).collect();
        assert!(bad.is_empty(), "row {y} holds {bad:?}: {r:?}");
    }
}

#[test]
fn ui_18_the_parts_of_a_breadcrumb_hold_no_char_that_must_not_reach_the_terminal() {
    let p = parts(&format!("{EVIL}/sub/a.md"), Some(HEAD));
    assert_eq!(p.len(), 4, "{p:?}");
    assert!(p.iter().all(|x| !x.chars().any(hostile)), "{p:?}");
    assert!(p[0].contains('\u{fffd}') && p[3].contains('\u{fffd}'), "{p:?}");
    assert!(p[0].starts_with("d\u{fffd}[2J"), "the rest of the name is as it is: {p:?}");
    // what is measured is what is drawn
    let width = |v: &[String]| -> usize { v.join(" › ").chars().count() };
    let cut = cut(&p, 20);
    assert!(width(&cut) <= 20 && !cut.iter().any(|x| x.chars().any(hostile)), "{cut:?}");
}

/// A frame of the editor in `skin` that is given a breadcrumb whose parts are not cleaned: the
/// drawing of the title (depth) and of the tab bar (flat) must not trust them.
fn raw_deck(skin: Skin) -> Vec<String> {
    use sim::color::Tones;
    use sim::layout::{Chrome, Deck, Fade, Geo, body};
    use sim::tabs::Tab;
    use sim_core::core::color::palette;
    let t = Tones::new(&palette(BG));
    let geo = Geo::with_head((80, 12), skin, false, true);
    let tab = Tab { name: "a.md".into(), ..Tab::default() };
    let chrome = Chrome { icons: false, root: "~", tabs: vec![tab], cur: 0 };
    let parts: Vec<String> = [EVIL, "ok", "a.md", HEAD].iter().map(ToString::to_string).collect();
    // (the box's title is drawn for the file an App has)
    let app =
        sim_core::core::app::App::new(Some("a.md".into()), Ok(Some(b"x\n".to_vec())), (70, 10));
    let deck = Deck {
        geo,
        tones: Some(&t),
        fade: Fade::None,
        gray: false,
        app: Some(&app),
        win: None,
        focus: false,
        chrome: Some(chrome),
        scroll: vec![],
        crumb: (parts, 2),
    };
    body(&deck, &[], None, None).0.into_iter().map(|r| r.text).collect()
}

#[test]
fn ui_18_the_drawing_of_the_breadcrumb_cannot_leak_what_it_is_given_in_flat_or_in_depth() {
    for skin in [Skin::Flat, Skin::Depth] {
        let rows = raw_deck(skin);
        clean(&rows);
        let title = rows.iter().find(|r| r.contains('\u{fffd}')).unwrap_or_else(|| &rows[0]);
        assert!(title.contains('\u{fffd}'), "{skin:?}: {rows:?}");
    }
}

#[test]
fn ui_18_a_directory_and_a_heading_with_control_chars_never_reach_a_row() {
    for skin in [Skin::Flat, Skin::Depth] {
        let name = format!("{}/a.md", EVIL.replace('\n', "_"));
        let file = format!("{name}=# {HEAD}\n\ntext\n");
        let r = shown("crumb-evil", skin, (100, 20), &[&file], &name, "j");
        clean(&r.screen.rows.iter().map(|x| x.text.clone()).collect::<Vec<_>>());
        let top = r.screen.rows.iter().find(|x| x.text.contains('\u{fffd}'));
        assert!(top.is_some(), "{skin:?}: {}", dump(&r.screen));
    }
}
