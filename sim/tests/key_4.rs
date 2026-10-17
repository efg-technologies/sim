//! KEY-4: `:help` and F1 open the picker as `Help · keys and commands`: a row for each entry of the
//! key table, filtered as the finder filters, the longer text in the preview; it runs nothing.
#![allow(clippy::unwrap_used)] // helpers; the tests themselves may unwrap
mod common;
use common::*;
use sim::color::Syntax;
use sim::keys::{rows, table};
use sim::layout::Skin;
use sim::sim::SimEvent;
use sim_core::core::app::Event;

fn rig_help(name: &str, skin: Skin, size: (u16, u16)) -> Rig {
    let mut r = rig_env(name, &["a.txt=one\ntwo\n", "b.txt=other\n"], &["a.txt"], size, |e| {
        e.skin = skin;
        e.chrome = true;
        e.syn = Syntax::new(true);
    });
    r.send(SimEvent::Term(Event::Bg((0x1e, 0x1e, 0x1e))));
    r
}

/// The list's rows as `keys description mode`, blanks collapsed, without the selection's bar.
fn list(r: &Rig) -> Vec<String> {
    r.list()
        .iter()
        .map(|t| t.trim_start_matches('▎').split_whitespace().collect::<Vec<_>>().join(" "))
        .collect()
}

#[test]
fn key_4_the_help_is_the_picker_with_a_row_for_each_entry_of_the_table() {
    let mut r = rig_help("k4-open", Skin::Depth, (90, 40));
    r.keys(":help\n");
    assert!(r.sim.picker_open());
    let float = r.float();
    assert!(float[0].contains("Help · keys and commands"), "{}", float[0]);
    assert!(
        float.last().unwrap().contains(&format!("1 / {}", table().len())),
        "{}",
        float.last().unwrap()
    );
    assert!(float.last().unwrap().contains("↵ close"), "{}", float.last().unwrap());
    // the rows are `keys  description  mode`, in the order of the table
    let first = list(&r);
    assert_eq!(first[0], "i inner object operator/visual", "{first:?}");
    assert_eq!(rows().len(), table().len());
    assert!(rows()[2].1.starts_with("h / <Left>"), "{}", rows()[2].1);
    assert!(
        first.iter().any(|t| t.starts_with("h / <Left> left normal/visual/operator")),
        "{first:?}"
    );
    // the preview: the keys, the modes and the longer text
    let pane = r.pane();
    assert!(pane.iter().any(|t| t.contains("Selects inside the delimiters")), "{pane:?}");
}

#[test]
fn key_4_f1_and_help_open_it_in_every_mode_and_f1_again_closes_it() {
    let mut r = rig_help("k4-f1", Skin::Depth, (120, 30));
    r.keys("<F1>");
    assert!(r.sim.picker_open());
    r.keys("<F1>");
    assert!(!r.sim.picker_open(), "F1 while a picker is open closes it");
    r.keys("i<F1>"); // Insert mode
    assert!(r.sim.picker_open());
    r.keys("<Esc><Esc>");
    r.keys("<C-n><F1>"); // the tree
    assert!(r.sim.picker_open());
    r.keys("<Esc><Esc>"); // the picker, then the tree's focus
    r.keys(":h\n");
    assert!(r.sim.picker_open());
    r.keys("<Esc>:help tab\n"); // with words typed
    assert_eq!(r.query(), "tab");
    assert!(!list(&r).is_empty());
}

#[test]
fn key_4_typing_tab_finds_tab_shift_tab_and_space_b_p() {
    let mut r = rig_help("k4-tab", Skin::Depth, (140, 40));
    r.keys("<F1>tab");
    let rows = list(&r);
    let top: Vec<&String> = rows.iter().take(6).collect();
    for want in ["<Tab> next file", "<S-Tab> previous file", "<Space>bp pick a tab by letter"] {
        assert!(top.iter().any(|t| t.starts_with(want)), "{want:?} in the top of {rows:?}");
    }
    assert!(!r.float().last().unwrap().contains(&format!("/ {}", table().len())), "filtered");
    // the best one is first, and the matched chars are bold (the finder's marks)
    assert!(rows[0].starts_with("<Tab>"), "{rows:?}");
    // the query is edited as the picker's others are
    r.keys("<C-u>");
    assert_eq!(r.query(), "");
    assert_eq!(list(&r).len(), r.list().len());
}

#[test]
fn key_4_enter_and_esc_close_it_and_it_runs_nothing() {
    let mut r = rig_help("k4-run", Skin::Depth, (140, 40));
    let (text, panel) = (r.text(), r.sim.panel.open);
    r.keys("<F1>toggle the tree"); // the entry of <C-n>
    assert!(list(&r)[0].starts_with("<C-n> toggle the tree"), "{:?}", list(&r));
    r.keys("<CR>");
    assert!(!r.sim.picker_open());
    assert_eq!(
        (r.sim.panel.open, r.text(), r.app().vi.mode),
        (panel, text.clone(), sim_core::core::vi::Mode::Normal)
    );
    r.keys("<F1>x<Esc>");
    assert!(!r.sim.picker_open() && r.text() == text);
    // `dd` is an entry too: selecting it and pressing Enter deletes nothing
    r.keys("<F1>delete lines<CR>");
    assert_eq!(r.text(), text);
}

#[test]
fn key_4_the_rows_are_cut_by_the_mode_first_and_the_help_has_no_match_to_say() {
    let mut r = rig_help("k4-narrow", Skin::Depth, (70, 30));
    r.keys("<F1>");
    for row in r.list() {
        assert!(row.chars().count() <= 70, "{row}");
    }
    r.keys("zzzzzzz");
    assert!(r.float().iter().any(|t| t.contains("no matches")), "{:?}", r.float());
}

#[test]
fn key_4_no_row_or_preview_of_the_help_shows_an_escape_of_the_table() {
    let leaks = |text: &str| {
        let low = text.to_lowercase();
        ["<lt>", "<gt>", "<bar>"].iter().any(|e| low.contains(e))
    };
    for (_, row) in rows() {
        assert!(!leaks(row), "{row}");
    }
    for e in table() {
        assert!(!leaks(&e.keys()) && !leaks(&e.desc) && !leaks(&e.long), "{}", e.keys());
    }
    // the angle brackets are themselves, and a `/` between two keys has blanks around it
    let all: Vec<&str> = rows().iter().map(|r| r.1.as_str()).collect();
    for (keys, desc) in [
        ("i< / i>", "inner angle brackets"),
        ("a< / a>", "around angle brackets"),
        ("<", "outdent"),
        ("<<", "outdent line"),
    ] {
        let found =
            |t: &&str| t.starts_with(keys) && t[keys.len()..].trim_start().starts_with(desc);
        assert!(all.iter().any(found), "{keys:?} {desc:?} not in {all:?}");
    }
    // keys wider than the column drop whole alternatives and say so (the preview has them all), and
    // a row never has half of a key
    assert!(all.iter().any(|t| t.starts_with("<Down> / <C-n> …  next result")), "{all:?}");
    assert!(all.iter().any(|t| t.starts_with("1 … 9  ") || t.starts_with("1 … 9 ")), "{all:?}");
    for t in &all {
        let keys = t.chars().take(16).collect::<String>();
        let keys = keys.trim_end();
        assert!(!keys.ends_with(" /") && !keys.ends_with(" / <"), "half a key: {t:?}");
    }
    assert!(table().iter().any(|e| e.keys() == "<Down> / <C-n> / <C-j>"));
    // on the screen: the list, the keys at the top of the preview, and nothing else
    let mut r = rig_help("k4-escapes", Skin::Depth, (120, 40));
    r.keys(":help\n");
    for query in ["angle", "outdent", "operator"] {
        r.keys("<C-u>");
        r.keys(query);
        assert!(!leaks(&dump(&r.screen)), "{query}: {}", dump(&r.screen));
        let first = r.pane().into_iter().map(|t| t.trim().to_string()).find(|t| !t.is_empty());
        assert!(!first.is_some_and(|t| leaks(&t)), "{query}: {:?}", r.pane());
    }
    r.keys("<C-u>angle");
    let rows = list(&r);
    assert!(rows.iter().any(|t| t.starts_with("i< / i> inner angle brackets")), "{rows:?}");
    assert!(r.pane().iter().any(|t| t.contains("i< / i>")), "{:?}", r.pane());
}
