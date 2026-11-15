//! DIFF-7, DIFF-8, DIFF-9, DIFF-10: the changes view as a watchtower. While the view has the focus the
//! text pane shows the diff of the file under its cursor (red `-`, green `+`, the numbers of the file),
//! drawn through a core App that is never given a key; its hunks are walked and scrolled, Enter opens the
//! file at the hunk, it follows the disk, the view is remembered, and FOLLOW is a switch. Driven through
//! sim's state with real repositories under a private temporary directory.
#![allow(clippy::unwrap_used, clippy::panic)] // helpers; the tests themselves may unwrap
mod common;
use common::*;
use sim::color::Syntax;
use sim::io;
use sim::layout::Skin;
use sim::patch::{Is, Patch, added};
use sim::sim::{PatchReq, SimEffect, SimEvent, Touched};
use sim_core::core::app::Event;
use std::path::{Path, PathBuf};

/// `src/a.rs` of 30 lines `l1` to `l30`, committed with `old/gone.txt`.
fn lines(from: usize, to: usize) -> String {
    (from..=to).map(|i| format!("l{i}\n")).collect()
}

/// A repository whose `src/a.rs` changed in two places far apart (line 3 is `L3`, line 25 went), with an
/// untracked `docs/new.md` and a deleted `old/gone.txt`; the panel open, in `skin`.
fn tower(name: &str, skin: Skin, size: (u16, u16)) -> Rig {
    let a = format!("src/a.rs={}", lines(1, 30));
    let mut r = rig_env(name, &[&a, "old/gone.txt=a\nb\n", "docs/c.md=c\n"], &[], size, |e| {
        repo(&e.cwd, &[]);
        let text = lines(1, 30).replace("l3\n", "L3\n").replace("l25\n", "");
        std::fs::write(e.cwd.join("src/a.rs"), text).unwrap();
        std::fs::remove_file(e.cwd.join("old/gone.txt")).unwrap();
        std::fs::write(e.cwd.join("docs/new.md"), "x\ny\n").unwrap();
        e.skin = skin;
        e.chrome = true;
        e.syn = Syntax::new(true);
        e.home = Some(e.cwd.clone());
    });
    r.send(SimEvent::Term(Event::Bg((0x1e, 0x1e, 0x1e))));
    r
}

/// Moves the view's cursor, a row at a time, to the row of `path`.
fn walk(r: &mut Rig, path: &str) {
    for _ in 0..30 {
        if r.sim.row(r.sim.panel.cur).is_some_and(|row| row.path == Path::new(path)) {
            return;
        }
        r.keys("j");
    }
    panic!("no row is {path}: {:?}", r.panel());
}

/// The requests of the preview Sim asked for, in order.
fn asked(r: &Rig) -> Vec<PatchReq> {
    r.log
        .iter()
        .filter_map(|e| if let SimEffect::Patch(p) = e { Some(p.clone()) } else { None })
        .collect()
}

#[test]
fn diff_7_a_patch_is_its_hunks_with_the_lines_of_the_file_and_where_each_hunk_goes() {
    let diff = "diff --git a/a b/a\nindex 1..2 100644\n--- a/a\n+++ b/a\n@@ -1,4 +1,4 @@ fn x\n l1\n-l2\n+L2\n l3\n@@ -20,3 +20,2 @@\n l20\n-l21\n l22\n\\ No newline at end of file\n";
    let p = Patch::parse(diff.as_bytes());
    assert!(p.text.starts_with("@@ -1,4 +1,4 @@ fn x\n l1\n-l2\n"), "{}", p.text);
    let is: Vec<Is> = p.lines.iter().map(|l| l.0).collect();
    use Is::*;
    assert_eq!(is, [Head, Same, Went, Came, Same, Head, Same, Went, Same, Same]);
    let now: Vec<Option<usize>> = p.lines.iter().map(|l| l.1).collect();
    assert_eq!(now, [None, Some(1), None, Some(2), Some(3), None, Some(20), None, Some(21), None]);
    // each hunk: its header's line, and the first line of the file it changes (a deletion: where it was)
    assert_eq!(p.hunks, [(0, 2), (5, 21)]);
}

#[test]
fn diff_7_an_untracked_file_is_all_plus_a_binary_one_and_one_with_no_hunk_say_so() {
    let p = Patch::parse(&added(b"x\ny"));
    assert_eq!(p.text, "@@ -0,0 +1,2 @@\n+x\n+y\n");
    assert_eq!(p.hunks, [(0, 1)]);
    let said = |p: Patch| p.text.trim_end().to_string();
    assert_eq!(said(Patch::parse(&added(b"\0\x01"))), "binary file: no diff to show");
    let bin = "diff --git a/b b/b\nindex 1..2\nBinary files a/b and b/b differ\n";
    assert_eq!(said(Patch::parse(bin.as_bytes())), "binary file: no diff to show");
    assert_eq!(said(Patch::parse(b"")), "no changes from HEAD");
    let mode = "diff --git a/m b/m\nold mode 100644\nnew mode 100755\n";
    assert_eq!(said(Patch::parse(mode.as_bytes())), "no change in the text");
    assert_eq!(Patch::note("why").lines, [(Is::Head, None)]);
}

#[test]
fn diff_7_the_preview_s_diff_is_git_s_through_the_guard_and_a_deleted_file_is_all_minus() {
    let r = tower("d7-io", Skin::Calm, (80, 20));
    let ask = |path: &str, new: bool| {
        let req = PatchReq { n: 7, path: path.into(), old: None, new };
        match io::patch(&r.sim.root, &req) {
            SimEvent::Patched(7, p) => p,
            e => panic!("{e:?}"),
        }
    };
    let a = ask("src/a.rs", false);
    assert_eq!(a.hunks.len(), 2, "{}", a.text);
    assert!(a.text.contains("-l3\n+L3\n") && a.text.contains("-l25\n"), "{}", a.text);
    let gone = ask("old/gone.txt", false);
    assert_eq!(gone.text, "@@ -1,2 +0,0 @@\n-a\n-b\n");
    assert_eq!(ask("docs/new.md", true).text, "@@ -0,0 +1,2 @@\n+x\n+y\n");
    // a link that leads out of the root is not followed (GREP-7)
    std::os::unix::fs::symlink("/etc/passwd", r.sim.root.join("docs/out.md")).unwrap();
    assert!(ask("docs/out.md", true).text.contains("not a file below the root"));
}

#[test]
fn diff_7_the_text_pane_shows_the_diff_of_the_file_under_the_cursor() {
    for (skin, name) in [(Skin::Flat, "flat"), (Skin::Depth, "depth"), (Skin::Calm, "calm")] {
        let mut r = tower(&format!("d7-golden-{name}"), skin, (80, 20));
        r.keys(" ge");
        walk(&mut r, "src/a.rs");
        let ed = r.editor().join("\n");
        assert!(ed.contains("@@ -1,6 +1,6 @@") && ed.contains("-l3") && ed.contains("+L3"), "{ed}");
        // the numbers are the file's: none for a line that went
        let row = |what: &str| r.editor().into_iter().find(|l| l.contains(what)).unwrap();
        assert!(row("+L3").contains("  3 +L3"), "{}", row("+L3"));
        assert!(!row("-l3").chars().any(|c| c.is_ascii_digit() && c != '3'), "{}", row("-l3"));
        let tones = sim::color::Tones::new(&sim_core::core::color::palette((0x1e, 0x1e, 0x1e)));
        let dump =
            if skin == Skin::Calm { dump_screen(&r.shot) } else { dump_look(&r.shot, &tones) };
        golden(&format!("diff_7_{name}"), &dump);
        // a line that went is on the red tint and one that came on the green; calm has no tint
        let tint = |what: &str| {
            let y = r.screen.rows.iter().position(|l| l.text.contains(what)).unwrap();
            let text = &r.screen.rows[y].text;
            let x = text[..text.find(what).unwrap()].chars().count() as u16 + 1;
            r.shot.paints[y]
                .iter()
                .find(|(c, p)| c.contains(&x) && p.bg.is_some())
                .and_then(|p| p.1.bg)
        };
        match skin {
            Skin::Calm => assert_eq!((tint("-l3"), tint("+L3")), (None, None)),
            _ => assert_eq!(
                (tint("-l3"), tint("+L3")),
                (Some(tones.loss), Some(tones.gain)),
                "{name}"
            ),
        }
    }
}

#[test]
fn diff_7_with_the_focus_in_the_text_the_pane_is_the_file_s_again() {
    let mut r = tower("d7-focus", Skin::Flat, (80, 20));
    r.keys(":e docs/c.md\n<C-n> ge");
    walk(&mut r, "src/a.rs");
    assert!(r.editor().iter().any(|l| l.contains("+L3")), "{:?}", r.editor());
    r.keys("<Esc>");
    assert!(!r.editor().iter().any(|l| l.contains("+L3")), "{:?}", r.editor());
    assert!(r.editor().iter().any(|l| l.contains("c")), "{:?}", r.editor());
    // and with the panel back it is the preview, with no key to git
    r.keys("<C-n>");
    assert!(r.editor().iter().any(|l| l.contains("+L3")), "{:?}", r.editor());
    // a click in the preview gives the text the focus and moves no cursor of the file
    let at = r.app().buf.cursor;
    let g = r.sim.geo();
    r.click(g.tx() + 6, g.y0 + 2);
    assert!(!r.sim.panel.focus);
    assert_eq!(r.app().buf.cursor, at);
}

#[test]
fn diff_7_moving_the_cursor_asks_for_that_file_s_diff_and_an_older_answer_is_dropped() {
    let mut r = tower("d7-ask", Skin::Calm, (80, 20));
    r.keys(" ge");
    walk(&mut r, "old/gone.txt");
    let first = asked(&r).last().cloned().unwrap();
    assert_eq!(first.path, Path::new("old/gone.txt"));
    walk(&mut r, "src/a.rs");
    let next = asked(&r).last().cloned().unwrap();
    assert_eq!(
        (next.path.as_path(), next.old.as_ref(), next.new),
        (Path::new("src/a.rs"), None, false)
    );
    assert!(r.editor().iter().any(|l| l.contains("+L3")));
    // a late answer of the first request is not drawn
    r.send(SimEvent::Patched(first.n, Patch::parse(b"@@ -1 +1 @@\n-stale\n+STALE\n")));
    assert!(!r.editor().iter().any(|l| l.contains("STALE")), "{:?}", r.editor());
    // the untracked file is asked as one, all of it came
    r.keys("gg");
    walk(&mut r, "docs/new.md");
    assert!(asked(&r).last().unwrap().new);
    assert!(r.editor().iter().any(|l| l.contains("+x")), "{:?}", r.editor());
}

#[test]
fn diff_7_the_preview_is_no_buffer() {
    let mut r = tower("d7-nobuf", Skin::Flat, (80, 20));
    r.keys(" ge");
    let (opens, history) =
        (r.log.iter().filter(|e| matches!(e, SimEffect::Open(..))).count(), r.history.clone());
    for path in ["docs/new.md", "old/gone.txt", "src/a.rs"] {
        walk(&mut r, path);
    }
    r.keys("]c]c[cJK");
    assert!(r.sim.bufs.is_empty(), "no buffer");
    assert_eq!(
        r.log.iter().filter(|e| matches!(e, SimEffect::Open(..))).count(),
        opens,
        "no watcher"
    );
    assert_eq!(r.history, history, "no history");
    assert!(
        !r.log.iter().any(|e| matches!(e, SimEffect::App(..) | SimEffect::Highlight(_))),
        "no snapshot, no write, no highlighter"
    );
    assert!(r.status().contains("CHANGES"));
}

#[test]
fn diff_7_a_hidden_char_in_a_diff_is_its_form() {
    let mut r = tower("d7-hidden", Skin::Depth, (80, 20));
    std::fs::write(r.sim.root.join("src/a.rs"), lines(1, 30).replace("l3\n", "x\u{202e}y\n"))
        .unwrap();
    r.keys(" ge");
    walk(&mut r, "src/a.rs");
    let row = r.screen.rows.iter().find(|l| l.text.contains("+x")).unwrap().text.clone();
    assert!(row.contains("+x<U+202E>y"), "{row}");
    assert!(!r.screen.rows.iter().any(|l| l.text.contains('\u{202e}')));
}

#[test]
fn diff_8_bracket_c_walks_the_hunks_into_the_next_file() {
    let mut r = tower("d8-walk", Skin::Calm, (80, 12));
    r.keys(" ge");
    walk(&mut r, "old/gone.txt");
    assert!(r.editor().iter().any(|l| l.contains("-a")), "{:?}", r.editor());
    // one hunk: `]c` goes on to the next file's first hunk
    r.keys("]c");
    assert_eq!(r.sim.row(r.sim.panel.cur).unwrap().path, Path::new("src/a.rs"));
    assert!(r.editor().first().unwrap().contains("@@ -1,6 +1,6 @@"), "{:?}", r.editor());
    r.keys("]c");
    // (the last page of the preview is a whole pane: the last hunk is shown where that puts it)
    assert!(r.editor().iter().any(|l| l.contains("@@ -22,7 +22,6 @@")), "{:?}", r.editor());
    assert!(!r.editor().iter().any(|l| l.contains("@@ -1,6")), "{:?}", r.editor());
    // past the last hunk of the last file nothing moves
    r.keys("]c");
    assert_eq!(r.sim.row(r.sim.panel.cur).unwrap().path, Path::new("src/a.rs"));
    r.keys("[c");
    assert!(r.editor().first().unwrap().contains("@@ -1,6 +1,6 @@"), "{:?}", r.editor());
    // before the first, the previous file's last hunk
    r.keys("[c");
    assert_eq!(r.sim.row(r.sim.panel.cur).unwrap().path, Path::new("old/gone.txt"));
    assert!(r.editor().iter().any(|l| l.contains("-a")), "{:?}", r.editor());
}

#[test]
fn diff_8_j_and_k_scroll_the_preview_by_half_a_pane_and_the_wheel_by_three_lines() {
    let mut r = tower("d8-scroll", Skin::Calm, (80, 8));
    let text = lines(1, 30).replace("l3\n", "L3\nm1\nm2\nm3\nm4\nm5\nm6\nm7\nm8\n");
    std::fs::write(r.sim.root.join("src/a.rs"), text).unwrap();
    r.keys(" ge");
    walk(&mut r, "src/a.rs");
    let first = r.editor()[0].clone();
    let half = usize::from(r.sim.geo().rows / 2);
    r.keys("J");
    assert_ne!(r.editor()[0], first, "{:?}", r.editor());
    assert!(r.editor()[0].contains(["-l3", "+L3", "+m1"][half - 3]), "{half}: {:?}", r.editor());
    r.keys("K");
    assert_eq!(r.editor()[0], first);
    let g = r.sim.geo();
    r.wheel(false, g.tx() + 3, g.y0 + 1);
    assert!(r.editor()[0].contains("-l3"), "{:?}", r.editor());
    assert!(r.sim.panel.focus, "the wheel gives no focus");
}

#[test]
fn diff_8_enter_opens_the_file_at_the_hunk() {
    let mut r = tower("d8-enter", Skin::Calm, (80, 12));
    r.keys(" ge");
    walk(&mut r, "src/a.rs");
    r.keys("]c<CR>");
    assert_eq!(r.open_files(), ["src/a.rs"]);
    assert!(!r.sim.panel.focus);
    let b = &r.app().buf;
    assert_eq!(b.line_of(b.cursor) + 1, 25, "the first line the hunk changes");
    // a file that is open is shown at the hunk too, and the first hunk is the default
    r.keys("<C-n>gg");
    walk(&mut r, "src/a.rs");
    r.keys("<CR>");
    let b = &r.app().buf;
    assert_eq!(b.line_of(b.cursor) + 1, 3);
    assert_eq!(r.open_files(), ["src/a.rs"]);
}

#[test]
fn diff_8_the_keys_of_the_view_are_in_the_table() {
    let table = sim::keys::table();
    for (keys, desc) in
        [("] c", "hunk"), ("[ c", "hunk"), ("J", "scroll"), ("K", "scroll"), ("f", "FOLLOW")]
    {
        let e = table.iter().find(|e| e.keys() == keys.replace(' ', "") && e.modes.contains('t'));
        let e = e.unwrap_or_else(|| panic!("{keys} in mode t"));
        assert!(e.desc.contains(desc) || e.long.contains(desc), "{keys}: {}", e.desc);
    }
    // which-key lists `c` after `]` in the view
    let mut r = tower("d8-which", Skin::Calm, (80, 20));
    r.keys(" ge]");
    assert_eq!(r.sim.prefix(), ["]"]);
}

#[test]
fn diff_9_an_outside_write_makes_the_preview_again_and_keeps_its_hunk() {
    let mut r = tower("d9-live", Skin::Calm, (80, 12));
    r.keys(" ge");
    walk(&mut r, "src/a.rs");
    r.keys("]c");
    let before = asked(&r).len();
    let text = lines(1, 30).replace("l3\n", "L3\n").replace("l25\n", "").replace("l27\n", "NEW\n");
    std::fs::write(r.sim.root.join("src/a.rs"), text).unwrap();
    let t = Touched { paths: vec![(PathBuf::from("src/a.rs"), true)], ..Touched::default() };
    r.send(SimEvent::Touched(t));
    assert!(asked(&r).len() > before, "asked again with git's answer");
    assert!(r.editor().iter().any(|l| l.contains("+NEW")), "{:?}", r.editor());
    assert!(
        !r.editor().iter().any(|l| l.contains("@@ -1,")),
        "the hunk shown stays: {:?}",
        r.editor()
    );
    assert_eq!(r.sim.row(r.sim.panel.cur).unwrap().path, Path::new("src/a.rs"));
}

#[test]
fn diff_9_the_cursor_stays_on_its_file_when_a_row_comes_above_it() {
    let mut r = tower("d9-stay", Skin::Calm, (80, 20));
    r.keys(" ge");
    walk(&mut r, "src/a.rs");
    std::fs::write(r.sim.root.join("aaa.md"), "new\n").unwrap();
    let t = Touched { paths: vec![(PathBuf::from("aaa.md"), true)], ..Touched::default() };
    r.send(SimEvent::Touched(t));
    assert!(r.panel().iter().any(|l| l.contains("aaa.md")), "{:?}", r.panel());
    assert_eq!(r.sim.row(r.sim.panel.cur).unwrap().path, Path::new("src/a.rs"));
}

#[test]
fn diff_9_the_top_border_says_the_sums() {
    let mut r = tower("d9-sums", Skin::Depth, (100, 20));
    r.keys(" ge");
    assert!(
        r.screen.rows[0].text.contains("╭─changes · 3 files +3 −4"),
        "{}",
        r.screen.rows[0].text
    );
    r.keys(" ge");
    assert!(r.screen.rows[0].text.contains("╭─files"), "{}", r.screen.rows[0].text);
}

#[test]
fn diff_9_which_view_the_panel_showed_is_remembered() {
    let mut r = tower("d9-view", Skin::Calm, (80, 20));
    let views = |r: &Rig| -> Vec<bool> {
        r.log
            .iter()
            .filter_map(|e| if let SimEffect::View(on) = e { Some(*on) } else { None })
            .collect()
    };
    r.keys(" geq ge.");
    assert_eq!(views(&r), [true, false, true, false]);
    // the state directory's `view`
    let dir = temp_dir("d9-view-state");
    assert!(!io::load_view(&dir.0), "the tree when nothing was chosen");
    io::save_view(&dir.0, true).unwrap();
    assert!(io::load_view(&dir.0));
    assert_eq!(std::fs::read_to_string(dir.0.join("view")).unwrap(), "changes\n");
    io::save_view(&dir.0, false).unwrap();
    assert!(!io::load_view(&dir.0));
    std::fs::write(dir.0.join("view"), "junk").unwrap();
    assert!(!io::load_view(&dir.0));
    // started with the choice, the panel is the changes view
    let mut s = rig_env("d9-view-start", &["a.txt=a\n"], &[], (80, 20), |e| {
        repo(&e.cwd, &[]);
        std::fs::write(e.cwd.join("a.txt"), "b\n").unwrap();
        e.changes = true;
    });
    assert!(s.status().contains("CHANGES"), "{}", s.status());
    assert!(s.panel().iter().any(|l| l.contains("a.txt")), "{:?}", s.panel());
    s.keys("q");
    assert!(s.status().contains("TREE"), "{}", s.status());
}

#[test]
fn diff_10_follow_is_off_by_default_and_a_write_moves_nothing() {
    let mut r = tower("d10-off", Skin::Calm, (80, 20));
    r.keys(" ge");
    walk(&mut r, "docs/new.md");
    std::fs::write(r.sim.root.join("src/a.rs"), "written\n").unwrap();
    let t = Touched { paths: vec![(PathBuf::from("src/a.rs"), true)], ..Touched::default() };
    r.send(SimEvent::Touched(t));
    assert_eq!(r.sim.row(r.sim.panel.cur).unwrap().path, Path::new("docs/new.md"));
    assert!(!r.status().contains("FOLLOW"), "{}", r.status());
}

#[test]
fn diff_10_f_turns_follow_on_and_the_cursor_goes_to_the_file_written_from_outside() {
    let mut r = tower("d10-on", Skin::Calm, (80, 20));
    r.keys(" ge");
    walk(&mut r, "docs/new.md");
    r.keys("f");
    assert!(r.status().ends_with("CHANGES  FOLLOW"), "{}", r.status());
    // a file of the view
    std::fs::write(r.sim.root.join("src/a.rs"), "written\n").unwrap();
    let t = Touched { paths: vec![(PathBuf::from("src/a.rs"), true)], ..Touched::default() };
    r.send(SimEvent::Touched(t));
    assert_eq!(r.sim.row(r.sim.panel.cur).unwrap().path, Path::new("src/a.rs"));
    assert!(r.editor().iter().any(|l| l.contains("@@ -1,30 +1 @@")), "{:?}", r.editor());
    // a new file: once git's answer gives it a row
    std::fs::write(r.sim.root.join("docs/b.md"), "b\n").unwrap();
    let t = Touched { paths: vec![(PathBuf::from("docs/b.md"), true)], ..Touched::default() };
    r.send(SimEvent::Touched(t));
    assert_eq!(r.sim.row(r.sim.panel.cur).unwrap().path, Path::new("docs/b.md"));
    // `f` again: off, and nothing moves
    r.keys("f");
    assert!(r.status().ends_with("CHANGES"), "{}", r.status());
    std::fs::write(r.sim.root.join("src/a.rs"), "again\n").unwrap();
    let t = Touched { paths: vec![(PathBuf::from("src/a.rs"), true)], ..Touched::default() };
    r.send(SimEvent::Touched(t));
    assert_eq!(r.sim.row(r.sim.panel.cur).unwrap().path, Path::new("docs/b.md"));
    // it is the view's: the view entered again starts with it off
    r.keys("f ge ge");
    assert!(!r.status().contains("FOLLOW"), "{}", r.status());
}

#[test]
fn diff_10_the_bar_says_follow_while_it_is_on() {
    let mut r = tower("d10-bar", Skin::Flat, (100, 20));
    r.keys(" gef");
    let bar = r.status().to_string();
    assert!(bar.contains("CHANGES") && bar.contains("FOLLOW"), "{bar}");
    r.keys("f");
    assert!(!r.status().contains("FOLLOW"), "{}", r.status());
}
