//! UI-5: the git signs of the sign column and the counts, as gitsigns reads a hunk: the parser of
//! `git diff -U0` (pure), the diff asked for the file on open, save and change, and the signs on
//! the screen, on a real repository.
#![allow(clippy::unwrap_used)] // helpers; the tests themselves may unwrap
mod common;
use common::*;
use sim::color::{Accent, Syntax, Tones};
use sim::diff::{Diff, Sign};
use sim::layout::Skin;
use sim::sim::SimEvent;
use sim_core::core::app::Event;
use sim_core::core::color::{Rgb, palette};

const BG: Rgb = (0x1e, 0x1e, 0x1e);

fn signs(d: &Diff) -> Vec<(usize, Sign)> {
    d.signs.iter().map(|(l, s)| (*l, *s)).collect()
}

#[test]
fn ui_5_a_hunk_is_read_as_gitsigns_reads_it() {
    // lines added: `b = 0`, the lines c..c + d (1-based)
    let d = Diff::parse(b"@@ -3,0 +4,2 @@\n+x\n+y\n", 10);
    assert_eq!(signs(&d), [(3, Sign::Add), (4, Sign::Add)]);
    assert_eq!((d.added, d.changed, d.deleted), (2, 0, 0));
    // lines deleted: `d = 0`, a sign on line c, which they were below
    let d = Diff::parse(b"@@ -5,2 +4,0 @@\n-a\n-b\n", 10);
    assert_eq!(signs(&d), [(3, Sign::Del)]);
    assert_eq!((d.added, d.changed, d.deleted), (0, 0, 2));
    // deleted at the top: above the first line
    let d = Diff::parse(b"@@ -1,2 +0,0 @@\n", 10);
    assert_eq!(signs(&d), [(0, Sign::DelTop)]);
    // changed, and added after: min(b, d) changed, the rest added
    let d = Diff::parse(b"@@ -2,3 +2,5 @@\n", 10);
    assert_eq!(
        signs(&d),
        [(1, Sign::Change), (2, Sign::Change), (3, Sign::Change), (4, Sign::Add), (5, Sign::Add)]
    );
    assert_eq!((d.added, d.changed, d.deleted), (2, 3, 0));
    // changed, with lines deleted below: the change shows, the deletion is counted
    let d = Diff::parse(b"@@ -2,5 +2,3 @@\n", 10);
    assert_eq!(signs(&d), [(1, Sign::Change), (2, Sign::Change), (3, Sign::Change)]);
    assert_eq!((d.added, d.changed, d.deleted), (0, 3, 2));
    // one line, the counts left out; several hunks; the text of the hunks is not read
    let d = Diff::parse(
        b"diff --git a/f b/f\n@@ -7 +7 @@ fn x() {\n-a\n+b\n@@ -20,0 +21 @@\n+@@ -1,0 +1 @@\n",
        30,
    );
    assert_eq!(signs(&d), [(6, Sign::Change), (20, Sign::Add)]);
    assert_eq!((d.added, d.changed, d.deleted), (1, 1, 0));
    // nothing, junk, and hunks past the end of the file
    assert!(Diff::parse(b"", 5).signs.is_empty());
    assert!(Diff::parse(b"@@ nonsense\n@@ -a +b @@\n", 5).signs.is_empty());
    assert!(Diff::parse(b"@@ -90,0 +91,2 @@\n", 5).signs.is_empty());
}

#[test]
fn ui_5_a_line_below_an_edit_is_looked_up_where_it_was_until_the_next_save() {
    let d = Diff::parse(b"@@ -8 +8 @@\n", 10); // line 8 (index 7) changed, in a file of 10 lines
    assert_eq!(d.sign(7, 0, 10), Some(Sign::Change));
    // Two lines were inserted at the cursor (line 2): what was line index 7 is 9 now.
    assert_eq!(d.sign(9, 1, 12), Some(Sign::Change));
    assert_eq!(d.sign(7, 1, 12), None);
    // Above the edit nothing moved.
    assert_eq!(d.sign(0, 1, 12), None);
    assert_eq!(Diff::all_added(4).sign(2, 0, 4), Some(Sign::Add));
}

/// A repository with `a.rs` (ten lines) committed, then changed: line 2 changed, a line added
/// after line 5 and line 8 deleted.
fn signed(name: &str, skin: Skin) -> Rig {
    let text: String = (1..=10).map(|i| format!("l{i}\n")).collect();
    let mut r = rig_env(name, &[&format!("a.rs={text}"), "b.rs"], &["a.rs"], (80, 24), |e| {
        git(&e.cwd, &["init", "-q"]);
        git(&e.cwd, &["add", "-A"]);
        git(&e.cwd, &["commit", "-q", "-m", "init"]);
        let now = "l1\nL2\nl3\nl4\nl5\nnew\nl6\nl7\nl9\nl10\n";
        std::fs::write(e.cwd.join("a.rs"), now).unwrap();
        make(&e.cwd, &["c.rs"]);
        e.skin = skin;
        e.syn = Syntax::new(true);
        e.chrome = true;
    });
    r.send(SimEvent::Term(Event::Bg(BG)));
    r
}

fn sign_cell(r: &Rig, what: &str) -> (char, Option<Rgb>) {
    let g = r.sim.geo();
    let y = (usize::from(g.y0)..r.shot.frame.rows.len())
        .find(|&y| r.shot.frame.rows[y].text.contains(what))
        .unwrap();
    let ch = r.shot.frame.rows[y].text.chars().nth(usize::from(g.ex)).unwrap();
    let fg = r.shot.paints[y].iter().find(|(c, _)| c.contains(&g.ex)).and_then(|p| p.1.fg);
    (ch, fg)
}

#[test]
fn ui_5_the_signs_of_the_file_against_head_are_in_the_first_cell_of_the_sign_column() {
    for skin in [Skin::Flat, Skin::Depth] {
        let r = signed("ui5-signs", skin);
        assert!(r.jobs.contains(&"diff"), "git was asked for the diff of the file that opened");
        let t = Tones::new(&palette(BG));
        let (green, amber, red) =
            (Some(t.fg(Accent::Green)), Some(t.fg(Accent::Amber)), Some(t.fg(Accent::Red)));
        assert_eq!(sign_cell(&r, "L2"), ('┆', amber), "{skin:?}: changed");
        assert_eq!(sign_cell(&r, "new"), ('▎', green), "{skin:?}: added");
        assert_eq!(sign_cell(&r, "l7"), ('▁', red), "{skin:?}: lines were deleted below it");
        assert_eq!(sign_cell(&r, "l3").0, ' ', "{skin:?}: no change, no sign");
        // Only the row a line starts in has its sign; the second cell is the App's own symbol.
        let g = r.sim.geo();
        let y = (usize::from(g.y0)..).find(|&y| r.shot.frame.rows[y].text.contains("L2")).unwrap();
        assert_eq!(
            r.shot.frame.rows[y].text.chars().nth(usize::from(g.ex) + 1),
            Some(' '),
            "{skin:?}"
        );
    }
}

#[test]
fn ui_5_a_file_git_does_not_know_has_every_line_added_and_one_outside_a_repository_none() {
    let mut r = signed("ui5-untracked", Skin::Flat);
    r.keys(":e c.rs\n");
    let green = Some(Tones::new(&palette(BG)).fg(Accent::Green));
    assert_eq!(sign_cell(&r, "c.rs"), ('▎', green), "(the text of the file is its name: line 1)");
    // A directory that is no repository: nothing is shown, nothing is said.
    let mut plain = rig_env("ui5-norepo", &["a.rs=x\ny\n"], &["a.rs"], (80, 24), |e| {
        e.skin = Skin::Flat;
        e.syn = Syntax::new(true);
        e.chrome = true;
    });
    plain.send(SimEvent::Term(Event::Bg(BG)));
    assert_eq!(sign_cell(&plain, "x"), (' ', None));
    assert!(!plain.status().contains("git"), "{}", plain.status());
}

#[test]
fn ui_5_the_diff_is_asked_again_after_a_save_and_after_a_change_on_disk() {
    let mut r = signed("ui5-again", Skin::Flat);
    let asked = |r: &Rig| r.jobs.iter().filter(|j| **j == "diff").count();
    let before = asked(&r);
    r.keys("ggx:w\n");
    assert!(asked(&r) > before, "a save asks again");
    let id = r.sim.bufs[0].id;
    let before = asked(&r);
    r.send(SimEvent::App(id, disk(b"l1\nl2\nl3\nl4\nl5\nl6\nl7\nl8\nl9\nl10\nl11\n")));
    assert!(asked(&r) > before, "a change on disk asks again");
}

#[test]
fn ui_5_the_diff_is_run_by_the_guarded_git_with_the_flags_of_the_spec() {
    let r = signed("ui5-guard", Skin::Flat);
    let out = sim::io::git_diff(&r.cwd, "a.rs").unwrap();
    let text = String::from_utf8_lossy(&out);
    let hunks: Vec<&str> = text.lines().filter(|l| l.starts_with("@@")).collect();
    assert_eq!(hunks, ["@@ -2 +2 @@ l1", "@@ -5,0 +6 @@ l5", "@@ -8 +8,0 @@ l7"]);
    // A repository whose config names a command is not run (MARK-1's guard): nothing is asked.
    git(&r.cwd, &["config", "filter.x.clean", "touch witness"]);
    let skipped = sim::io::git_diff(&r.cwd, "a.rs").unwrap_err();
    assert!(skipped.starts_with(sim::io::GIT_SKIPPED), "{skipped}");
    assert!(!r.cwd.join("witness").exists());
    // And a name that looks like an option is a name: git is given `--` before it.
    let odd = sim::io::git_diff(&r.cwd, "--output=x");
    assert!(odd.is_err() || odd.unwrap().is_empty());
    assert!(!r.cwd.join("x").exists());
}
