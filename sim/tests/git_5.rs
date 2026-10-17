//! GIT-5: the frame says what the guard did to the repository. What it turned off in the config (pure,
//! then a real repository's status), then the screens: the words in the tree's top border (depth) or
//! its header (flat), the lines of the boxes where git was not run, inverse and bold with no color,
//! nothing on the frame in calm, and the status line's message once.
#![allow(clippy::unwrap_used)] // helpers; the tests themselves may unwrap
mod common;
use common::*;
use sim::color::{Syntax, Tones};
use sim::io;
use sim::layout::Skin;
use sim::paint::{Painter, Screen};
use sim::sim::SimEvent;
use sim_core::core::app::Event;
use sim_core::core::color::{Rgb, palette};

const BG: Rgb = (0x1e, 0x1e, 0x1e);

/// `git config --list --show-scope --show-origin -z` as git prints it: `scope NUL origin NUL key NL
/// value NUL` for each entry.
fn listing(entries: &[(&str, &str, &str)]) -> Vec<u8> {
    let mut out = vec![];
    for (scope, key, value) in entries {
        out.extend(format!("{scope}\0file:.git/config\0{key}\n{value}\0").bytes());
    }
    out
}

#[test]
fn git_5_what_the_guard_turns_off_in_the_repository_s_own_config_is_counted_once_each() {
    let l = listing(&[
        ("local", "core.fsmonitor", "/tmp/x"),
        ("local", "core.hookspath", "/tmp/hooks"),
        ("local", "diff.external", "difft"),
        ("local", "diff.x.textconv", "cat"),
        ("local", "diff.y.command", "cat"),
        ("local", "filter.wit.clean", "cat"),
        ("local", "filter.wit.smudge", "cat"),
        ("local", "filter.wit.required", "true"),
        ("worktree", "filter.other.process", "x"),
        ("local", "core.fsmonitor", "/tmp/y"),
    ]);
    assert_eq!(
        io::turned_off(&l),
        [
            "core.fsmonitor",
            "core.hookspath",
            "diff.external",
            "diff.x.textconv",
            "diff.y.command",
            "filter.wit",
            "filter.other"
        ],
        "a filter is one setting however many of its keys there are, and a key set twice is one"
    );
}

#[test]
fn git_5_the_user_s_own_scopes_and_the_keys_that_are_not_turned_off_are_not_counted() {
    let l = listing(&[
        ("global", "core.fsmonitor", "x"),
        ("system", "filter.a.clean", "x"),
        ("command", "core.hookspath", "x"),
        ("local", "core.editor", "vim"),
        ("local", "user.name", "me"),
        ("local", "core.autocrlf", "true"),
        ("local", "diff.tool", "meld"),
        ("local", "filter.broken", "x"),
    ]);
    assert_eq!(io::turned_off(&l), Vec::<String>::new());
    assert_eq!(io::turned_off(b""), Vec::<String>::new());
    assert_eq!(io::turned_off(b"local\0"), Vec::<String>::new(), "a listing cut short");
}

#[test]
fn git_5_git_lfs_s_own_filter_is_not_turned_off_and_so_is_not_counted() {
    let lfs = [
        ("local", "filter.lfs.required", "true"),
        ("local", "filter.lfs.clean", "git-lfs clean -- %f"),
        ("local", "filter.lfs.smudge", "git-lfs smudge -- %f"),
        ("local", "filter.lfs.process", "git-lfs filter-process"),
    ];
    assert_eq!(io::turned_off(&listing(&lfs)), Vec::<String>::new(), "GIT-2 lets it run");
    let mut other = lfs.to_vec();
    other.push(("local", "filter.lfs.smudge", "touch /tmp/x; cat"));
    assert_eq!(io::turned_off(&listing(&other)), ["filter.lfs"], "an lfs that was changed is off");
}

#[test]
fn git_5_a_real_repository_reports_the_settings_it_has_and_runs_none_of_them() {
    let d = temp_dir("g5-real");
    repo(&d.0, &["a.rs", "b.rs"]);
    let witness = d.0.join("witness");
    let sh = format!("touch '{}'; cat", witness.display());
    git(&d.0, &["config", "filter.wit.clean", &sh]);
    git(&d.0, &["config", "core.fsmonitor", &format!("touch '{}-m'", witness.display())]);
    let (_, _, off) = io::status(&d.0).unwrap();
    assert_eq!(off, ["core.fsmonitor", "filter.wit"], "in git's order");
    assert!(!witness.exists() && !d.0.join("witness-m").exists(), "and none of it ran");
    let clean = temp_dir("g5-clean");
    repo(&clean.0, &["a.rs"]);
    assert_eq!(io::status(&clean.0).unwrap().2, Vec::<String>::new());
}

fn draw(s: &Screen) -> String {
    let mut p = Painter::new(Vec::new());
    p.draw(s.clone()).unwrap();
    String::from_utf8(p.out).unwrap()
}

/// The SGR sequence in force at the first `what` of `out`.
fn seq(out: &str, what: &str) -> String {
    assert!(out.contains(what), "{what:?} is not drawn in {out:?}");
    let i = out.find(what).unwrap();
    let chunk = out[..i].rsplit("\x1b[").next().unwrap();
    format!("\x1b[{}", &chunk[..=chunk.find('m').unwrap()])
}

/// The alert's block as flat and depth write it.
fn block() -> String {
    let (back, text) = Tones::new(&palette(BG)).alert_block();
    format!("\x1b[0;38;2;{};{};{};48;2;{};{};{}m", text.0, text.1, text.2, back.0, back.1, back.2)
}

const REFUSED: &str = "[core]\n\tsshCommand = cat\n";
const OFF: &str = "[core]\n\tfsmonitor = /bin/false\n[filter \"wit\"]\n\tclean = cat\n[diff]\n\texternal = /bin/false\n";

/// A repository whose config has `config` added, drawn in `skin`.
fn shown(name: &str, skin: Skin, mono: bool, config: &str, size: (u16, u16)) -> Rig {
    let mut r = rig_env(name, &["a.rs", "b.rs"], &[], size, |e| {
        git(&e.cwd, &["init", "-q"]);
        git(&e.cwd, &["add", "-A"]);
        git(&e.cwd, &["commit", "-q", "-m", "init"]);
        let path = e.cwd.join(".git/config");
        let old = std::fs::read_to_string(&path).unwrap();
        std::fs::write(&path, old + config).unwrap();
        e.skin = skin;
        e.chrome = true;
        e.syn = if mono { Syntax::mono() } else { Syntax::new(true) };
    });
    r.send(SimEvent::Term(Event::Bg(BG)));
    r
}

/// The foreground of column `x` of row `y`: the first paint that has one.
fn ink(r: &Rig, y: usize, x: u16) -> Option<Rgb> {
    r.shot.paints[y].iter().filter(|(c, _)| c.contains(&x)).find_map(|p| p.1.fg)
}

fn col(text: &str, needle: &str) -> u16 {
    text[..text.find(needle).unwrap()].chars().count() as u16
}

#[test]
fn git_5_where_git_is_not_run_the_trees_border_says_so_and_the_lines_of_both_boxes_are_the_alert_s()
{
    let t = Tones::new(&palette(BG));
    let r = shown("g5-depth-refused", Skin::Depth, false, REFUSED, (100, 14));
    let top = &r.shot.frame.rows[0].text;
    assert!(top.starts_with("╭─files"), "{top:?}");
    assert!(top.contains(" git not run "), "{top:?}");
    assert_eq!(seq(&draw(&r.shot), "git not run"), block(), "the alert's block");
    // the lines of both boxes: the corner and the sides of the tree's, and the editor's
    let second = top.chars().enumerate().filter(|(_, c)| *c == '╭').nth(1).unwrap().0 as u16;
    for (y, x) in [(0, 0), (3, 0), (3, second - 1), (3, second), (0, second)] {
        assert_eq!(ink(&r, y, x), Some(t.alert_fg()), "({x}, {y}) of {top:?}");
    }
    // a repository that is as it was has the lines it had
    let clean = shown("g5-depth-clean", Skin::Depth, false, "", (100, 14));
    assert!(!clean.shot.frame.rows[0].text.contains("git"), "{:?}", clean.shot.frame.rows[0].text);
    assert_ne!(ink(&clean, 3, 0), Some(t.alert_fg()));
}

#[test]
fn git_5_where_settings_are_turned_off_it_is_the_words_alone_and_the_lines_are_as_they_were() {
    let t = Tones::new(&palette(BG));
    let r = shown("g5-depth-off", Skin::Depth, false, OFF, (100, 14));
    let top = &r.shot.frame.rows[0].text;
    assert!(top.contains(" 3 git settings off "), "{top:?}");
    assert_eq!(seq(&draw(&r.shot), "3 git settings off"), block());
    for (y, x) in [(0, 0), (3, 0), (3, r.sim.geo().pw + 1)] {
        assert_ne!(ink(&r, y, x), Some(t.alert_fg()), "({x}, {y}): git works, nothing was lost");
    }
    // one setting is said in the singular
    let one =
        shown("g5-depth-one", Skin::Depth, false, "[core]\n\tfsmonitor = /bin/false\n", (100, 14));
    assert!(one.shot.frame.rows[0].text.contains(" 1 git setting off "));
}

#[test]
fn git_5_in_flat_the_words_are_at_the_right_end_of_the_trees_header() {
    for (config, words) in [(REFUSED, "git not run"), (OFF, "3 git settings off")] {
        let r = shown("g5-flat", Skin::Flat, false, config, (100, 14));
        let head = &r.shot.frame.rows[0].text;
        let pw = usize::from(r.sim.geo().pw);
        let at = usize::from(col(head, words));
        assert!(head.contains(words) && at + words.len() < pw, "{head:?}");
        assert_eq!(seq(&draw(&r.shot), words), block());
    }
}

#[test]
fn git_5_without_color_the_words_are_inverse_and_bold() {
    for skin in [Skin::Flat, Skin::Depth] {
        for (config, words) in [(REFUSED, "git not run"), (OFF, "3 git settings off")] {
            let r = shown("g5-mono", skin, true, config, (100, 14));
            let out = draw(&r.shot);
            let sgr = seq(&out, words);
            assert!(sgr.contains(";7") && sgr.contains(";1"), "{skin:?} {words}: {sgr:?}");
            assert!(!out.contains("38;") && !out.contains("48;"), "no color at all");
        }
    }
}

#[test]
fn git_5_a_panel_too_narrow_for_the_words_gets_the_short_ones_and_calm_has_no_frame() {
    let r = shown("g5-narrow", Skin::Depth, false, REFUSED, (50, 12));
    let top = &r.shot.frame.rows[0].text;
    assert!(top.contains(" no git ") && !top.contains("git not run"), "{top:?}");
    let r = shown("g5-narrow-off", Skin::Depth, false, OFF, (50, 12));
    assert!(r.shot.frame.rows[0].text.contains(" 3 off "), "{:?}", r.shot.frame.rows[0].text);
    // a panel narrower still has nothing to say it in, and the status line says it
    let r = shown("g5-tiny", Skin::Depth, false, REFUSED, (30, 12));
    assert!(!r.shot.frame.rows[0].text.contains("git"), "{:?}", r.shot.frame.rows[0].text);
    let r = shown("g5-calm", Skin::Calm, false, REFUSED, (100, 14));
    let all: String = r.shot.frame.rows.iter().map(|x| x.text.clone()).collect();
    assert!(!all.contains("git not run") && !all.contains("no git"), "{all:?}");
    assert!(r.status().starts_with("git skipped: core.sshcommand"), "{}", r.status());
}

#[test]
fn git_5_the_status_line_says_which_setting_once_and_is_safe_to_draw() {
    let mut r = shown("g5-msg", Skin::Depth, false, OFF, (100, 14));
    assert_eq!(r.status(), "git: turned off the repository's core.fsmonitor and 2 others");
    // told once: a status that comes again with the same settings says nothing new
    r.keys("<Esc>");
    let again = io::status(&r.sim.root).unwrap();
    r.send(SimEvent::Status(Ok(again)));
    assert!(!r.status().contains("turned off"), "{}", r.status());
    // names come from a file: a control char in one is not sent to the terminal
    let mut r = shown("g5-hostile", Skin::Depth, false, "", (100, 14));
    r.send(SimEvent::Status(Ok((vec![], String::new(), vec!["filter.a\u{1b}[2Jb".into()]))));
    let all: String = r.shot.frame.rows.iter().map(|x| x.text.clone()).collect();
    assert!(!all.contains('\u{1b}') && all.contains("turned off"), "{all:?}");
    assert!(
        draw(&r.shot).contains("filter.a\u{fffd}[2Jb"),
        "a replacement char, not the control char"
    );
}
