//! KEY-3: a pause on a prefix opens which-key, a small window near the bottom with a rounded border,
//! the prefix as its title and the next keys one line each; typing on is what it is without it.
#![allow(clippy::unwrap_used)] // helpers; the tests themselves may unwrap
mod common;
use common::*;
use sim::color::Syntax;
use sim::layout::Skin;
use sim::sim::{PAUSE_MS, SimEffect, SimEvent};
use sim_core::core::app::Event;

fn rig_wk(name: &str, skin: Skin, size: (u16, u16)) -> Rig {
    let text: String = (1..=30).map(|i| format!("line {i} of the file\n")).collect();
    let mut r =
        rig_env(name, &[&format!("a.txt={text}"), "b.txt=other\n"], &["a.txt"], size, |e| {
            e.skin = skin;
            e.chrome = true;
            e.syn = Syntax::new(true);
        });
    r.send(SimEvent::Term(Event::Bg((0x1e, 0x1e, 0x1e))));
    r.keys(":e b.txt\n<Tab>");
    r
}

/// The window: where its top border is (row, column) and its rows, border and all. It is the box whose
/// top border has a title with blanks around it (the interface's boxes have none).
fn window(r: &Rig) -> Option<((usize, usize), Vec<String>)> {
    let rows: Vec<Vec<char>> = r.screen.rows.iter().map(|x| x.text.chars().collect()).collect();
    let top = rows.iter().position(|t| t.iter().collect::<String>().contains("╭─ "))?;
    let left = rows[top].iter().position(|&c| c == '╭')?;
    let right = left + rows[top][left..].iter().position(|&c| c == '╮')?;
    let bottom = (top..rows.len()).find(|&y| rows[y].get(left) == Some(&'╰'))?;
    let cut = |y: usize| rows[y][left..=right].iter().collect::<String>();
    Some(((top, left), (top..=bottom).map(cut).collect()))
}

/// A row of the window as `key description`: the border and the blanks between the columns gone.
fn norm(row: &str) -> String {
    row.trim_matches(|c| c == '│' || c == ' ').split_whitespace().collect::<Vec<_>>().join(" ")
}

#[test]
fn key_3_the_delay_is_which_key_s_default_for_a_mapping_that_is_not_a_plugin_s() {
    assert_eq!(PAUSE_MS, 200);
}

#[test]
fn key_3_a_pause_on_the_leader_opens_the_window_near_the_bottom_with_the_prefix_as_its_title() {
    let mut r = rig_wk("k3-space", Skin::Depth, (100, 24));
    r.keys(" ");
    assert!(window(&r).is_none(), "nothing before the pause");
    assert!(
        r.log.iter().any(|e| matches!(e, SimEffect::Pause(_))),
        "the shell is asked for a timer"
    );
    r.pause();
    let ((top, left), rows) = window(&r).expect("the window");
    assert!(rows[0].starts_with("╭─ <Space> ─") && rows[0].ends_with('╮'), "{rows:?}");
    assert!(
        rows.last().unwrap().starts_with("╰─") && rows.last().unwrap().ends_with('╯'),
        "{rows:?}"
    );
    assert!(rows.iter().any(|t| norm(t).starts_with("b buffers")), "{rows:?}");
    // near the bottom: the last row of the window is the one above the status line
    assert_eq!(top + rows.len(), r.screen.rows.len() - 1, "{top} {}", rows.len());
    // in the middle of the screen
    assert!(left > 20 && left < 60, "{left}");
    // typing on: the next key is the editor's, and the window is of the new prefix
    r.keys("b");
    assert!(window(&r).is_none(), "a key shuts it");
    r.pause();
    let (_, rows) = window(&r).unwrap();
    assert!(rows[0].starts_with("╭─ <Space>b ─"), "{rows:?}");
    assert!(rows.iter().any(|t| norm(t).starts_with("p pick a tab by letter")), "{rows:?}");
    assert!(rows.iter().any(|t| norm(t).starts_with("c pick a tab to close")), "{rows:?}");
    // Esc closes it, by ending the prefix
    r.keys("<Esc>");
    assert!(window(&r).is_none() && r.sim.prefix().is_empty());
}

#[test]
fn key_3_the_prefixes_of_the_core_and_of_sim_have_windows_too() {
    let mut r = rig_wk("k3-prefixes", Skin::Depth, (100, 24));
    for (keys, title, row) in [
        ("z", "z", "│ z  center cursor line"),
        ("z", "z", "│ h  syntax colors on/off"),
        ("g", "g", "│ g  first line"),
        ("[", "[", "│ c  previous change from outside"),
        ("]", "]", "│ c  next change from outside"),
        ("Z", "Z", "│ Q  quit without saving"),
        ("d", "d", "│ d  delete lines"),
        ("d", "d", "│ w  next word"),
        ("d", "d", "│ i  inner"),
        ("di", "di", "│ w/W  inner word"),
        ("y", "y", "│ y  yank lines"),
        ("c", "c", "│ c  change lines"),
        (">", ">", "│ >  indent line"),
    ] {
        r.keys(keys);
        r.pause();
        let (_, rows) =
            window(&r).unwrap_or_else(|| panic!("{keys}: no window\n{}", dump(&r.screen)));
        assert!(rows[0].starts_with(&format!("╭─ {title} ─")), "{keys}: {rows:?}");
        assert!(
            rows.iter().any(|t| norm(t).starts_with(&norm(row))),
            "{keys}: {row:?} not in {rows:?}"
        );
        r.keys("<Esc>");
    }
    // Visual mode: an operator is not pending, but `i` is, and its window lists the objects
    r.keys("vi");
    r.pause();
    let (_, rows) = window(&r).unwrap();
    assert!(rows.iter().any(|t| norm(t).starts_with("w/W inner word")), "{rows:?}");
    r.keys("<Esc><Esc>");
    // the tree: `g`, and the leader
    r.keys("<C-n>g");
    r.pause();
    let (_, rows) = window(&r).unwrap();
    assert!(rows.iter().any(|t| norm(t).starts_with("g top")), "{rows:?}");
    r.keys("<Esc> ");
    r.pause();
    let (_, rows) = window(&r).unwrap();
    assert!(rows.iter().any(|t| norm(t).starts_with("b buffers")), "{rows:?}");
}

#[test]
fn key_3_what_is_typed_after_a_prefix_or_before_one_opens_no_window() {
    let mut r = rig_wk("k3-none", Skin::Depth, (100, 24));
    for keys in ["f", "t", "r", "3", "df", "i", ":", "/", "v"] {
        r.keys(keys);
        r.pause();
        assert!(window(&r).is_none(), "{keys}: {}", dump(&r.screen));
        r.keys("<Esc><Esc>");
    }
}

#[test]
fn key_3_the_window_is_never_given_a_key_and_a_stale_pause_opens_nothing() {
    let mut r = rig_wk("k3-keys", Skin::Depth, (100, 24));
    let before = r.text();
    r.keys("d");
    r.pause();
    assert!(window(&r).is_some());
    r.keys("w"); // the editor's: the word goes
    assert!(window(&r).is_none() && r.sim.prefix().is_empty());
    assert_ne!(r.text(), before);
    // a pause asked for before a later key is dropped
    r.keys("d");
    let old = r
        .log
        .iter()
        .rev()
        .find_map(|e| if let SimEffect::Pause(n) = e { Some(*n) } else { None })
        .unwrap();
    r.keys("i");
    r.send(SimEvent::Pause(old));
    assert!(window(&r).is_none(), "stale");
    r.pause();
    assert!(window(&r).unwrap().1[0].starts_with("╭─ di ─"));
    // a pause after the prefix was completed opens nothing
    r.keys("w");
    r.pause();
    assert!(window(&r).is_none());
    // a key that completes a command asks for no pause at all
    let n = r.log.iter().filter(|e| matches!(e, SimEffect::Pause(_))).count();
    r.keys("x");
    assert_eq!(r.log.iter().filter(|e| matches!(e, SimEffect::Pause(_))).count(), n);
}

#[test]
fn key_3_a_prefix_with_more_keys_than_rows_says_how_many_more() {
    let mut r = rig_wk("k3-more", Skin::Depth, (100, 16));
    r.keys("d");
    r.pause();
    let ((top, _), rows) = window(&r).unwrap();
    assert!(rows.iter().any(|t| t.contains("more (:help)")), "{rows:?}");
    assert!(top >= 1, "it fits above the status line and the tabs: {top}");
    // a screen too small has none, and typing is unaffected
    let mut r = rig_wk("k3-small", Skin::Depth, (100, 8));
    r.keys("d");
    r.pause();
    assert!(window(&r).is_none());
}

#[test]
fn key_3_calm_draws_it_without_color_and_the_looks_with_the_interface_s_surface() {
    let mut r = rig_wk("k3-calm", Skin::Calm, (100, 24));
    r.keys(" ");
    r.pause();
    let ((top, _), rows) = window(&r).expect("calm has it too");
    assert!(rows[0].starts_with("╭─ <Space> ─"), "{rows:?}");
    for y in top..top + rows.len() {
        assert!(
            r.shot.paints[y].iter().all(|(_, p)| p.fg.is_none() && p.bg.is_none()),
            "row {y}: {:?}",
            r.shot.paints[y]
        );
    }
    let mut d = rig_wk("k3-depth", Skin::Depth, (100, 24));
    d.keys(" ");
    d.pause();
    let ((top, _), _) = window(&d).unwrap();
    assert!(d.shot.paints[top + 1].iter().any(|(_, p)| p.bg.is_some()), "a surface");
}
